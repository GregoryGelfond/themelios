//! Token-stream equivalence (docs/design/syntax.md §11): the
//! non-whitespace sequence and its two projections, the two certificates
//! over that one sequence, the first divergence as a witness, and the
//! canonical spellings a spelling-normalizing formatter reads.

use std::borrow::Cow;
use std::fmt;

use themelios_base::span::Location;

use crate::ast::{line_or_shebang_content, script_body_value};
use crate::parse::Parse;
use crate::tree::{
    Asp, AstNode, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole,
    WalkEvent, keeps_leading, role_of,
};

/// Every non-whitespace token under `node`, in order — the sequence the
/// certificates compare: significant tokens and trivia comments
/// interleaved as they stand. Total; a lazy iterative preorder walk.
pub fn non_whitespace_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|token| token.kind() != SyntaxKind::WHITESPACE)
}

/// One node the walk of `token_roles` is inside: the two facts a token's
/// role is decided by (`role_of`) — whether the node is a statement, and
/// whether its leading prefix is still intact where the walk stands.
#[derive(Clone, Copy)]
struct Frame {
    is_statement: bool,
    leading: bool,
}

/// The non-whitespace tokens under `node` with their roles, in document
/// order, in one pass — the roles `token_stream` and `comment_sequence`
/// filter by, read forward per node instead of scanned backward per
/// token (`role`), which cost a k-line doc block O(k²). A stack of one
/// `Frame` per open node: a token reads its role off the top frame, its
/// parent's; an element that does not keep the prefix (`keeps_leading`)
/// ends it there, a child node before opening its own frame — so the
/// frame a token finds holds exactly what `role` would read over its
/// preceding siblings. Total — the walk enters `node` before any token
/// under it, so a token always finds its parent's frame; O(subtree);
/// lazy, as the certificate reads two streams in lockstep.
fn token_roles(node: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, TokenRole)> {
    let mut frames: Vec<Frame> = Vec::new();
    node.preorder_with_tokens()
        .filter_map(move |event| match event {
            WalkEvent::Enter(element) => {
                let yielded = match &element {
                    NodeOrToken::Token(token) if token.kind() != SyntaxKind::WHITESPACE => {
                        let Frame {
                            is_statement,
                            leading,
                        } = *frames.last().expect("a token stands inside its node");
                        Some((token.clone(), role_of(token.kind(), is_statement, leading)))
                    }
                    NodeOrToken::Token(_) | NodeOrToken::Node(_) => None,
                };
                if !keeps_leading(&element)
                    && let Some(parent) = frames.last_mut()
                {
                    parent.leading = false;
                }
                if let NodeOrToken::Node(child) = &element {
                    frames.push(Frame {
                        is_statement: child.kind().is_statement(),
                        leading: true,
                    });
                }
                yielded
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => {
                frames.pop();
                None
            }
            WalkEvent::Leave(NodeOrToken::Token(_)) => None,
        })
}

/// The significant tokens of the tree under `node`, in order: every
/// token whose role is not `Trivia` — all non-comment, non-whitespace
/// tokens plus `DOC_COMMENT` tokens in docs position. Total; O(subtree),
/// the roles read forward in one pass (`token_roles`) with no per-token
/// scan; lazy.
pub fn token_stream(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    token_roles(node)
        .filter(|(_, role)| *role != TokenRole::Trivia)
        .map(|(token, _)| token)
}

/// The trivia comments under `node`, in order: role `Trivia`, kind a
/// comment. Total; O(subtree), the roles read forward in one pass
/// (`token_roles`) with no per-token scan; lazy.
pub fn comment_sequence(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    token_roles(node)
        .filter(|(token, role)| *role == TokenRole::Trivia && token.kind().is_comment())
        .map(|(token, _)| token)
}

/// A token's content for the sequence (docs/design/syntax.md §11.1): the
/// part of its text the certificates compare, per kind. A `LINE_COMMENT`
/// or `SHEBANG_COMMENT` contributes its text without its trailing
/// horizontal whitespace — spaces, tabs, and a carriage return — which is
/// layout the line rule swallowed on its way to the line end (§8.3); a
/// `DOC_COMMENT` its whole text, wherever it stands, since the doc form's
/// trailing whitespace is content (§8.3); a `SCRIPT_BODY` its value — the
/// raw text with the blanks and tabs before `#end` trimmed, the grammar's
/// own trimming (grammar §4.8); every other token — a `BLOCK_COMMENT`, an
/// `ERROR` token, every significant token — its text as it stands. This
/// is the rule's one home: `Comment::content` and `ScriptBody::value` are
/// the same trims, and a `Side` carries this projection as compared — so
/// a consumer certifying a claim of its own composes on `content` rather
/// than re-deriving which whitespace is layout. Total; borrows the
/// token's text: O(|text|) at worst, for the trim, and O(1) beyond the
/// text read — no tree walk.
pub fn content(token: &SyntaxToken) -> &str {
    match token.kind() {
        SyntaxKind::LINE_COMMENT | SyntaxKind::SHEBANG_COMMENT => {
            line_or_shebang_content(token.text())
        }
        SyntaxKind::SCRIPT_BODY => script_body_value(token.text()),
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

/// A token's content as `certificate` compares it: under `LayoutOnly`,
/// `content(token)` as it stands (§11.1); under `UpToSpelling`,
/// `canonical_spelling(token.kind(), content(token))` — the same content,
/// respelled to canonical where the kind has synonyms (§11.3), and
/// nothing else. Two tokens agree under a certificate exactly when their
/// kinds are equal and their `compared` contents are equal: that is what
/// `equivalent` tests at each index of the sequence and what a `Side`
/// reports, so a consumer composing a claim of its own — over a
/// subsequence, or exempting a kind — reads the projection here rather
/// than restating either branch. Borrowed on the common path — a
/// `Cow::Owned` only for a synonym respelled under `UpToSpelling` — so
/// the certificate's equal path, the whole cost when it grants, allocates
/// nothing. Total; O(|text|) at worst — `content`'s trim and the
/// respelling's comparison — and O(1) beyond the text read; no tree walk.
pub fn compared(token: &SyntaxToken, certificate: Certificate) -> Cow<'_, str> {
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
    use rowan::{GreenNodeBuilder, Language};
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::ast::Program;
    use crate::dialect::Dialect;
    use crate::parse::{Parse, parse};
    use crate::tree::role_shapes::role_corpus;
    use crate::tree::{NodeOrToken, SyntaxElement, SyntaxKind, keeps_leading, role};

    fn program(text: &str, id: u32) -> Parse<Program> {
        let source = Source::new(SourceId::new(id), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
    }

    fn certified(left: &str, right: &str, certificate: Certificate) -> Result<(), Mismatch> {
        equivalent(&program(left, 1), &program(right, 2), certificate)
    }

    /// The first element before `token` among its parent's children that
    /// ends the leading prefix, if any: a significant token, or a child
    /// node — the two ways a `DOC_COMMENT` under a statement comes to be
    /// trivia, which the witnesses below tell apart.
    fn ends_the_prefix(token: &SyntaxToken) -> Option<SyntaxElement> {
        token
            .parent()?
            .children_with_tokens()
            .take_while(|element| element.as_token() != Some(token))
            .find(|element| !keeps_leading(element))
    }

    #[test]
    fn token_stream_agrees_with_the_per_token_reading_on_every_node() {
        // The stream's one-pass reading of roles and `role` per token are
        // one reading: on every node of the docs-position corpus the stream
        // is exactly the non-whitespace tokens `role` does not call trivia,
        // in order. With a witness that the corpus reaches each way a
        // DOC_COMMENT's role is decided — kept as documentation; dropped
        // after a significant token; dropped after a child node with no
        // significant token before it, which only a walk that ends the
        // prefix on entering a child node reads right; and dropped under a
        // node that is no statement.
        let mut documented = 0usize;
        let mut after_a_token = 0usize;
        let mut after_a_node_only = 0usize;
        let mut outside_a_statement = 0usize;
        for root in role_corpus() {
            for node in root.descendants() {
                let expected: Vec<SyntaxToken> = non_whitespace_tokens(&node)
                    .filter(|token| role(token) != TokenRole::Trivia)
                    .collect();
                let read: Vec<SyntaxToken> = token_stream(&node).collect();
                assert_eq!(read, expected, "{}", node.kind());
            }
            for token in
                non_whitespace_tokens(&root).filter(|token| token.kind() == SyntaxKind::DOC_COMMENT)
            {
                let parent = token.parent().expect("a token has a parent");
                if !parent.kind().is_statement() {
                    outside_a_statement += 1;
                    continue;
                }
                match ends_the_prefix(&token) {
                    None => documented += 1,
                    Some(NodeOrToken::Token(_)) => after_a_token += 1,
                    Some(NodeOrToken::Node(_)) => after_a_node_only += 1,
                }
            }
        }
        assert!(documented > 0 && after_a_token > 0);
        assert!(after_a_node_only > 0 && outside_a_statement > 0);
    }

    #[test]
    fn comment_sequence_agrees_with_the_per_token_reading_on_every_node() {
        // The mirror law for the other projection: on every node of the
        // corpus the sequence is exactly the comment-kind tokens `role`
        // calls trivia, in order — with a witness that the role decides the
        // admission both ways for a DOC_COMMENT, and that a plain comment
        // inside a doc block is admitted while the doc lines around it are
        // not.
        let mut doc_kind_admitted = 0usize;
        let mut doc_kind_refused = 0usize;
        let mut plain_inside_a_block = 0usize;
        for root in role_corpus() {
            for node in root.descendants() {
                let expected: Vec<SyntaxToken> = non_whitespace_tokens(&node)
                    .filter(|token| token.kind().is_comment() && role(token) == TokenRole::Trivia)
                    .collect();
                let read: Vec<SyntaxToken> = comment_sequence(&node).collect();
                assert_eq!(read, expected, "{}", node.kind());
            }
            let admitted: Vec<SyntaxToken> = comment_sequence(&root).collect();
            for token in
                non_whitespace_tokens(&root).filter(|token| token.kind() == SyntaxKind::DOC_COMMENT)
            {
                if admitted.contains(&token) {
                    doc_kind_admitted += 1;
                } else {
                    doc_kind_refused += 1;
                }
            }
            plain_inside_a_block += admitted
                .iter()
                .filter(|token| {
                    token.kind() != SyntaxKind::DOC_COMMENT
                        && token
                            .parent()
                            .is_some_and(|parent| parent.kind().is_statement())
                        && ends_the_prefix(token).is_none()
                })
                .count();
        }
        assert!(doc_kind_admitted > 0 && doc_kind_refused > 0);
        assert!(plain_inside_a_block > 0);
    }

    #[test]
    fn a_doc_line_after_an_empty_statement_is_trivia() {
        // A deliberate off-grammar robustness test of the stack walk in
        // `token_roles`, not of any real input: this tree is one the parser
        // never produces — an empty statement, a RULE holding one
        // DOC_COMMENT and no significant token. What it exercises is the
        // `pop` at a node's `Leave`. On every tree the parser emits, a
        // statement's significant token ends its frame's leading prefix
        // before the `Leave`, so a frame a missing `pop` left on the stack
        // would read no differently from the one beneath it, and the
        // corpus laws above cannot tell the two apart. Here the RULE's
        // frame is still (statement, leading) at its `Leave`: the
        // DOC_COMMENT after the rule, a child of PROGRAM — no statement —
        // reads PROGRAM's frame and is trivia; over an un-popped RULE frame
        // it would read as documentation.
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! inside the empty rule",
        );
        builder.finish_node();
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! after the rule",
        );
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());
        let roles: Vec<(String, TokenRole)> = token_roles(&root)
            .map(|(token, role)| (token.text().to_owned(), role))
            .collect();
        assert_eq!(
            roles,
            [
                (
                    "%! inside the empty rule".to_owned(),
                    TokenRole::Documentation
                ),
                ("%! after the rule".to_owned(), TokenRole::Trivia),
            ]
        );
        // The same two roles as the public projections read them.
        let stream: Vec<String> = token_stream(&root).map(|t| t.text().to_owned()).collect();
        assert_eq!(stream, ["%! inside the empty rule"]);
        let comments: Vec<String> = comment_sequence(&root)
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(comments, ["%! after the rule"]);
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
