//! Token-stream equivalence (docs/design/syntax.md §11): the
//! non-whitespace sequence and its two projections, the two certificates
//! over that one sequence, the first divergence as a witness, and the
//! canonical spellings a spelling-normalizing formatter reads.

use std::borrow::Cow;
use std::fmt;

use themelios_base::span::Location;

use crate::parse::Parse;
use crate::tree::{
    Asp, AstNode, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole, role,
};

/// Every non-whitespace token under `node`, in order — the sequence the
/// certificates compare: significant tokens and trivia comments
/// interleaved as they stand. Total; a lazy iterative preorder walk.
pub fn non_whitespace_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|token| token.kind() != SyntaxKind::WHITESPACE)
}

/// The significant tokens of the tree under `node`, in order: every
/// token whose role is not `Trivia` — all non-comment, non-whitespace
/// tokens plus `DOC_COMMENT` tokens in docs position. Total; lazy.
pub fn token_stream(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    non_whitespace_tokens(node).filter(|token| role(token) != TokenRole::Trivia)
}

/// The trivia comments under `node`, in order: role `Trivia`, kind a
/// comment. Total; lazy.
pub fn comment_sequence(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    non_whitespace_tokens(node)
        .filter(|token| token.kind().is_comment() && role(token) == TokenRole::Trivia)
}

/// A token's content for the sequence (docs/design/syntax.md §11.1): a
/// line comment or shebang without its trailing horizontal whitespace,
/// which is layout; a doc comment whole, wherever it stands; a script
/// body by its value — the grammar's own trimming of the blanks before
/// `#end`; every other token its text.
fn content(token: &SyntaxToken) -> &str {
    match token.kind() {
        SyntaxKind::LINE_COMMENT | SyntaxKind::SHEBANG_COMMENT => {
            token.text().trim_end_matches([' ', '\t', '\r'])
        }
        SyntaxKind::SCRIPT_BODY => token.text().trim_end_matches([' ', '\t']),
        _ => token.text(),
    }
}

/// Which claim is being certified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Certificate {
    /// Layout only: the non-whitespace sequences equal by kind and
    /// content. Nothing but whitespace changed — exactly that, since
    /// whitespace is all the sequence leaves out.
    LayoutOnly,
    /// Up to spelling: as `LayoutOnly`, save that a token's content is
    /// compared after canonical respelling — the grammar's synonym pairs
    /// may have been normalized, and nothing else.
    UpToSpelling,
}

/// The first divergence, as a witness: the index in the sequence and
/// both sides — a side is `None` where its sequence ended first. Each
/// side carries the token's kind, its content, and its location in its
/// own tree, so a formatter's `--safe` mode reports where in the input
/// and where in the output the claim broke; the kind says whether the
/// element that diverged is a comment or a significant token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mismatch {
    /// The index in the non-whitespace sequence.
    pub index: usize,
    /// The left side's token, if its sequence had one.
    pub left: Option<Side>,
    /// The right side's token, if its sequence had one.
    pub right: Option<Side>,
}

/// One side of a divergence.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side {
    /// The token's kind.
    pub kind: SyntaxKind,
    /// Its content, as compared.
    pub content: String,
    /// Where it stands, in its own source.
    pub location: Location,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the sequences diverge at index {}: ", self.index)?;
        match (&self.left, &self.right) {
            (Some(left), Some(right)) => write!(
                f,
                "left has {} {:?}, right has {} {:?}",
                left.kind, left.content, right.kind, right.content
            ),
            (Some(left), None) => write!(
                f,
                "left has {} {:?}, right has ended",
                left.kind, left.content
            ),
            (None, Some(right)) => write!(
                f,
                "left has ended, right has {} {:?}",
                right.kind, right.content
            ),
            (None, None) => f.write_str("both have ended"),
        }
    }
}

impl std::error::Error for Mismatch {}

/// A token's content as this certificate compares it: its own content
/// under `LayoutOnly` (§11.1), respelled to canonical under `UpToSpelling`
/// (§11.3). Borrowed on the common path — a `Cow::Owned` only for a
/// synonym respelled under `UpToSpelling` — so the certificate's equal
/// path, the whole cost when it grants, allocates nothing.
fn compared(token: &SyntaxToken, certificate: Certificate) -> Cow<'_, str> {
    match certificate {
        Certificate::LayoutOnly => Cow::Borrowed(content(token)),
        Certificate::UpToSpelling => canonical_spelling(token.kind(), content(token)),
    }
}

/// The certificate: granted, or refused with the first divergence.
/// Compares the two sequences whatever the parses' dialects — a lexical
/// statement about two texts, meaningful across them; both roots are of
/// one family, as the one `T` fixes. Total; O(|left| + |right|); a
/// single zip over two lazy iterative walks, allocating only to name a
/// divergence. Not a refusal but the answer to the certificate's
/// question (docs/design/syntax.md §12.4).
pub fn equivalent<T: AstNode<Language = Asp>>(
    left: &Parse<T>,
    right: &Parse<T>,
    certificate: Certificate,
) -> Result<(), Mismatch> {
    let left_root = left.syntax();
    let right_root = right.syntax();
    let mut lefts = non_whitespace_tokens(&left_root);
    let mut rights = non_whitespace_tokens(&right_root);
    let mut index = 0usize;
    loop {
        match (lefts.next(), rights.next()) {
            (None, None) => return Ok(()),
            (l, r) => {
                let same = match (&l, &r) {
                    (Some(l), Some(r)) => {
                        l.kind() == r.kind() && compared(l, certificate) == compared(r, certificate)
                    }
                    _ => false,
                };
                if !same {
                    let side = |token: SyntaxToken, parse: &Parse<T>| Side {
                        kind: token.kind(),
                        content: compared(&token, certificate).into_owned(),
                        location: parse.location(token.text_range()),
                    };
                    return Err(Mismatch {
                        index,
                        left: l.map(|token| side(token, left)),
                        right: r.map(|token| side(token, right)),
                    });
                }
                index += 1;
            }
        }
    }
}

/// The canonical spelling of a token that has synonyms (grammar §4.5,
/// §4.6): `=` for `EQ`, `!=` for `NEQ`, `#inf`, `#sup` — the spellings
/// the authority renders when it prints its own tree — and `#minimize`,
/// `#maximize`, the roster's own, since the authority prints an optimize
/// statement as a weak constraint (docs/design/syntax.md §11.3); every
/// other token's content is its own canonical form. Total; the identity
/// on non-synonym kinds; O(1).
pub fn canonical_spelling(kind: SyntaxKind, content: &str) -> Cow<'_, str> {
    let canonical = match kind {
        SyntaxKind::EQ => "=",
        SyntaxKind::NEQ => "!=",
        SyntaxKind::KW_INF => "#inf",
        SyntaxKind::KW_SUP => "#sup",
        SyntaxKind::KW_MINIMIZE => "#minimize",
        SyntaxKind::KW_MAXIMIZE => "#maximize",
        _ => return Cow::Borrowed(content),
    };
    if content == canonical {
        Cow::Borrowed(content)
    } else {
        Cow::Owned(canonical.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::ast::Program;
    use crate::dialect::Dialect;
    use crate::parse::{Parse, parse};
    use crate::tree::SyntaxKind;

    fn program(text: &str, id: u32) -> Parse<Program> {
        let source = Source::new(SourceId::new(id), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
    }

    fn certified(left: &str, right: &str, certificate: Certificate) -> Result<(), Mismatch> {
        equivalent(&program(left, 1), &program(right, 2), certificate)
    }

    #[test]
    fn the_sequence_interleaves_significant_tokens_and_trivia_comments() {
        let parse = program("%! d\np. % c\nq :- r.\n", 0);
        let sequence: Vec<String> = non_whitespace_tokens(&parse.syntax())
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(sequence, ["%! d", "p", ".", "% c", "q", ":-", "r", "."]);
        let stream: Vec<String> = token_stream(&parse.syntax())
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(stream, ["%! d", "p", ".", "q", ":-", "r", "."]);
        let comments: Vec<String> = comment_sequence(&parse.syntax())
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(comments, ["% c"]);
    }

    #[test]
    fn layout_only_certifies_exactly_a_change_of_whitespace() {
        assert_eq!(
            certified(
                "p(X):-q(X).",
                "p( X )  :-\n  q(X) .",
                Certificate::LayoutOnly
            ),
            Ok(())
        );
        assert!(certified("p.", "q.", Certificate::LayoutOnly).is_err());
        assert!(
            certified("p. % c\nq.", "p.\nq. % c", Certificate::LayoutOnly).is_err(),
            "a comment moved across a token"
        );
        assert_eq!(
            certified("p. % c   \n", "p. % c\n", Certificate::LayoutOnly),
            Ok(()),
            "a line comment's trailing whitespace is layout"
        );
        assert!(
            certified("%! d  \np.", "%! d\np.", Certificate::LayoutOnly).is_err(),
            "a doc line's trailing whitespace is content"
        );
        assert!(certified("%! one\np.", "%! two\np.", Certificate::LayoutOnly).is_err());
        assert_eq!(
            certified(
                "#script (lua) x = 1   #end.",
                "#script (lua) x = 1 #end.",
                Certificate::LayoutOnly
            ),
            Ok(()),
            "the script body compares by its value"
        );
        assert!(
            certified("p($).", "p(#).", Certificate::LayoutOnly).is_err(),
            "error tokens are significant"
        );
    }

    #[test]
    fn up_to_spelling_admits_exactly_the_synonym_pairs() {
        let left =
            "p :- X == 1, X <> 2, Y = #infimum, Z != #supremum. #minimise { 1 }. #maximise { 2 }.";
        let right = "p :- X = 1, X != 2, Y = #inf, Z != #sup. #minimize { 1 }. #maximize { 2 }.";
        assert!(certified(left, right, Certificate::LayoutOnly).is_err());
        assert_eq!(certified(left, right, Certificate::UpToSpelling), Ok(()));
        assert!(certified("p :- X <= 1.", "p :- X < 1.", Certificate::UpToSpelling).is_err());
    }

    #[test]
    fn the_witness_names_the_first_divergence_on_both_sides() {
        let mismatch =
            certified("p(a, b). q.", "p(a, c). q.", Certificate::LayoutOnly).expect_err("diverges");
        assert_eq!(mismatch.index, 4);
        let left = mismatch.left.expect("a left side");
        let right = mismatch.right.expect("a right side");
        assert_eq!((left.kind, left.content.as_str()), (SyntaxKind::IDENT, "b"));
        assert_eq!(
            (right.kind, right.content.as_str()),
            (SyntaxKind::IDENT, "c")
        );
        assert_eq!(left.location.source, SourceId::new(1));
        assert_eq!(right.location.source, SourceId::new(2));
        let shorter = certified("p. q.", "p.", Certificate::LayoutOnly).expect_err("diverges");
        assert_eq!(shorter.index, 2);
        assert!(shorter.left.is_some() && shorter.right.is_none());
        assert!(shorter.to_string().contains("index 2"));
        let _: &dyn std::error::Error = &shorter;
    }

    #[test]
    fn canonical_spelling_is_the_authoritys_where_it_prints_and_the_rosters_for_the_optimize_pair()
    {
        assert_eq!(canonical_spelling(SyntaxKind::EQ, "=="), "=");
        assert_eq!(canonical_spelling(SyntaxKind::EQ, "="), "=");
        assert_eq!(canonical_spelling(SyntaxKind::NEQ, "<>"), "!=");
        assert_eq!(canonical_spelling(SyntaxKind::KW_INF, "#infimum"), "#inf");
        assert_eq!(canonical_spelling(SyntaxKind::KW_SUP, "#supremum"), "#sup");
        assert_eq!(
            canonical_spelling(SyntaxKind::KW_MINIMIZE, "#minimise"),
            "#minimize"
        );
        assert_eq!(
            canonical_spelling(SyntaxKind::KW_MAXIMIZE, "#maximise"),
            "#maximize"
        );
        assert_eq!(canonical_spelling(SyntaxKind::IDENT, "abc"), "abc");
        assert_eq!(canonical_spelling(SyntaxKind::LE, "<="), "<=");
    }
}
