//! The certificates' reflexivity through reparse, symmetry, and the
//! corollary — equal non-whitespace sequences, equal significant-token
//! shapes, under equal dialects and one root family, outside the aspif
//! dispatch — `canonical_spelling` idempotent and closed over the
//! synonym pairs, and the per-token projection the certificates compare:
//! `content` per kind, the one rule the typed accessors share, and
//! `compared` under each certificate (docs/design/syntax.md §11, §16).

use std::borrow::Cow;

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::{AstToken, Comment, ScriptBody};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{
    Certificate, canonical_spelling, compared, content, equivalent, non_whitespace_tokens,
};
use themelios_syntax::fusion::{Separator, separator};
use themelios_syntax::parse::{Parse, parse};
use themelios_syntax::tree::{
    NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, TokenRole, WalkEvent, role,
};

mod common;
use common::corpus;

fn admitted(text: &str, id: u32) -> Source {
    Source::new(SourceId::new(id), text.to_owned()).expect("admits")
}

/// The text re-spaced by the oracle: every pair abutted where the oracle
/// allows, one space or one line break where it does not.
fn respaced_by_the_oracle(root: &SyntaxNode, dialect: Dialect) -> String {
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .collect();
    let mut out = String::new();
    for (index, token) in tokens.iter().enumerate() {
        out.push_str(token.text());
        if let Some(next) = tokens.get(index + 1) {
            match separator(token, next, dialect) {
                Separator::Nothing => {}
                Separator::Whitespace => out.push(' '),
                Separator::LineBreak => out.push('\n'),
            }
        }
    }
    out
}

/// The significant-token shape: the preorder over nodes and significant
/// tokens, trivia dropped.
fn shape(root: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => out.push(format!("({}", node.kind())),
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(")".to_owned()),
            WalkEvent::Enter(NodeOrToken::Token(token)) if role(&token) != TokenRole::Trivia => {
                out.push(format!("{}:{}", token.kind(), token.text()));
            }
            _ => {}
        }
    }
    out
}

fn is_aspif(parse: &Parse<themelios_syntax::ast::Program>) -> bool {
    parse
        .diagnostics()
        .iter()
        .any(|d| d.id().name() == "aspif-input")
}

#[test]
fn reflexivity_through_reparse_symmetry_and_the_corollary_over_the_corpus() {
    for (name, text, dialect) in corpus() {
        let left = parse(&admitted(&text, 1), dialect);
        let again = parse(&admitted(&text, 2), dialect);
        // Reflexivity through reparse holds for every input: the same text,
        // parsed twice, has the same non-whitespace sequence.
        assert_eq!(
            equivalent(&left, &again, Certificate::LayoutOnly),
            Ok(()),
            "{name}: reflexive through reparse"
        );
        assert_eq!(
            equivalent(&left, &again, Certificate::UpToSpelling),
            Ok(()),
            "{name}"
        );
        // The respace round-trip, its symmetry, and the corollary are the
        // whole-text lemma lifted to the certificate — a guarantee for
        // members (docs/design/syntax.md §16, §10.1). A non-member's ERROR
        // tokens do not compose under re-spacing: the raw line break that
        // split a string heals into a space, and the sequence changes.
        if left.has_errors() {
            continue;
        }
        let respaced = parse(
            &admitted(&respaced_by_the_oracle(&left.syntax(), dialect), 3),
            dialect,
        );
        assert_eq!(
            equivalent(&left, &respaced, Certificate::LayoutOnly),
            Ok(()),
            "{name}: layout only"
        );
        assert_eq!(
            equivalent(&left, &respaced, Certificate::LayoutOnly).is_ok(),
            equivalent(&respaced, &left, Certificate::LayoutOnly).is_ok(),
            "{name}: symmetric"
        );
        if !is_aspif(&left) {
            assert_eq!(
                shape(&left.syntax()),
                shape(&respaced.syntax()),
                "{name}: the corollary"
            );
        }
    }
}

proptest! {
    #[test]
    fn a_whitespace_change_keeps_the_certificate_and_the_shape(
        text in prop::sample::select(vec![
            "p(X) :- q(X), not r(X), X = 1..3.\n% c\n:- #sum { W,T : t(T,W) } >= 4. %* b *%\n",
            "%! doc\na ; b | c : d.\n&sum { x, -y : p ; {a} } <= 3.\n#script (lua) x = 1 #end.\n",
        ]),
        choices in prop::collection::vec(0u8..3, 1..24)
    ) {
        let left = parse(&admitted(text, 1), Dialect::Clingo);
        let mut respaced = String::new();
        let mut next = choices.iter().copied().cycle();
        for token in left.syntax().descendants_with_tokens().filter_map(SyntaxElement::into_token) {
            if token.kind() == SyntaxKind::WHITESPACE {
                let breaks = token.text().matches('\n').count();
                let filler = match next.next().unwrap_or(0) {
                    0 => " ",
                    1 => "\t",
                    _ => "  ",
                };
                // The line breaks come before the horizontal filler, which is
                // the next line's indentation. A line form — a doc, line, or
                // shebang comment — is terminated by the break before any
                // horizontal space attaches to it: a doc comment's trailing
                // whitespace is content, not layout (docs/design/syntax.md §8.3,
                // §11.1), so filler placed before the break would change the
                // doc's content and the re-spacing would not be layout-only.
                for _ in 0..breaks {
                    respaced.push('\n');
                }
                respaced.push_str(filler);
            } else {
                respaced.push_str(token.text());
            }
        }
        let right = parse(&admitted(&respaced, 2), Dialect::Clingo);
        prop_assert_eq!(equivalent(&left, &right, Certificate::LayoutOnly), Ok(()));
        prop_assert_eq!(shape(&left.syntax()), shape(&right.syntax()));
    }
}

#[test]
fn canonical_spelling_is_idempotent_and_closed_over_the_synonym_pairs() {
    let pairs = [
        (SyntaxKind::EQ, "=", "=="),
        (SyntaxKind::NEQ, "!=", "<>"),
        (SyntaxKind::KW_INF, "#inf", "#infimum"),
        (SyntaxKind::KW_SUP, "#sup", "#supremum"),
        (SyntaxKind::KW_MINIMIZE, "#minimize", "#minimise"),
        (SyntaxKind::KW_MAXIMIZE, "#maximize", "#maximise"),
    ];
    for (kind, canonical, synonym) in pairs {
        assert_eq!(canonical_spelling(kind, canonical), canonical);
        assert_eq!(canonical_spelling(kind, synonym), canonical);
        let once = canonical_spelling(kind, synonym).into_owned();
        assert_eq!(canonical_spelling(kind, &once), once);
    }
    for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_token()) {
        if !pairs.iter().any(|(pair_kind, ..)| *pair_kind == kind) {
            assert_eq!(
                canonical_spelling(kind, "anything"),
                "anything",
                "{kind}: the identity"
            );
        }
    }
}

/// One token of each kind the content rule names (docs/design/syntax.md
/// §11.1), each carrying the trailing whitespace the rule decides on.
const PROJECTION_SNIPPET: &str =
    "#! run  \n%! d \t\np. % c \t\n%* b  *%\n#script (lua) x = 1 \t #end.\n%! stray  \n";

/// The synonym pairs in their non-canonical spellings, so the
/// `UpToSpelling` branch respells at least once.
const SYNONYM_SNIPPET: &str =
    "p :- X == 1, X <> 2, Y = #infimum, Z != #supremum. #minimise { 1 }. #maximise { 2 }.";

/// The corpus with the two snippets beside it.
fn corpus_and_snippets() -> Vec<(String, String, Dialect)> {
    let mut texts = corpus();
    for (name, text) in [
        ("projection snippet", PROJECTION_SNIPPET),
        ("synonym snippet", SYNONYM_SNIPPET),
    ] {
        texts.push((name.to_owned(), text.to_owned(), Dialect::Clingo));
    }
    texts
}

#[test]
fn content_is_the_sequences_per_kind_projection() {
    let parsed = parse(&admitted(PROJECTION_SNIPPET, 0), Dialect::Clingo);
    let projected: Vec<(SyntaxKind, String)> = non_whitespace_tokens(&parsed.syntax())
        .map(|token| (token.kind(), content(&token).to_owned()))
        .collect();
    let expected = [
        // The line forms lose their trailing horizontal whitespace: layout.
        (SyntaxKind::SHEBANG_COMMENT, "#! run"),
        // The doc form keeps it, in docs position and stray alike: content.
        (SyntaxKind::DOC_COMMENT, "%! d \t"),
        (SyntaxKind::IDENT, "p"),
        (SyntaxKind::DOT, "."),
        (SyntaxKind::LINE_COMMENT, "% c"),
        // A block comment, like every other token, is its text.
        (SyntaxKind::BLOCK_COMMENT, "%* b  *%"),
        (SyntaxKind::KW_SCRIPT, "#script"),
        (SyntaxKind::L_PAREN, "("),
        (SyntaxKind::IDENT, "lua"),
        (SyntaxKind::R_PAREN, ")"),
        // The script body loses the blanks and tabs before `#end`: its value.
        (SyntaxKind::SCRIPT_BODY, " x = 1"),
        (SyntaxKind::KW_END, "#end"),
        (SyntaxKind::DOT, "."),
        (SyntaxKind::DOC_COMMENT, "%! stray  "),
    ]
    .map(|(kind, content)| (kind, content.to_owned()));
    assert_eq!(projected, expected);
    let docs: Vec<TokenRole> = non_whitespace_tokens(&parsed.syntax())
        .filter(|token| token.kind() == SyntaxKind::DOC_COMMENT)
        .map(|token| role(&token))
        .collect();
    assert_eq!(docs, [TokenRole::Documentation, TokenRole::Trivia]);
    // A carriage return before the line end is layout too.
    let crlf = parse(&admitted("p. % c \r\n", 1), Dialect::Clingo);
    let comment = non_whitespace_tokens(&crlf.syntax())
        .find(|token| token.kind() == SyntaxKind::LINE_COMMENT)
        .expect("the comment");
    assert_eq!(content(&comment), "% c");
}

#[test]
fn content_agrees_with_the_typed_accessors() {
    // The trims have one home: on a trivia comment the projection is
    // `Comment::content`, on a script body `ScriptBody::value`.
    for (name, text, dialect) in corpus_and_snippets() {
        let parsed = parse(&admitted(&text, 0), dialect);
        for token in non_whitespace_tokens(&parsed.syntax()) {
            if let Some(comment) = Comment::cast(token.clone()) {
                assert_eq!(content(&token), comment.content(), "{name}: a comment");
            }
            if let Some(body) = ScriptBody::cast(token.clone()) {
                assert_eq!(content(&token), body.value(), "{name}: a script body");
            }
        }
    }
}

#[test]
fn compared_is_content_borrowed_under_layout_only() {
    for (name, text, dialect) in corpus_and_snippets() {
        let parsed = parse(&admitted(&text, 0), dialect);
        for token in non_whitespace_tokens(&parsed.syntax()) {
            let projected = compared(&token, Certificate::LayoutOnly);
            assert!(
                matches!(projected, Cow::Borrowed(_)),
                "{name}: never allocates"
            );
            assert_eq!(projected, content(&token), "{name}");
        }
    }
}

#[test]
fn compared_is_canonical_content_under_up_to_spelling() {
    for (name, text, dialect) in corpus_and_snippets() {
        let parsed = parse(&admitted(&text, 0), dialect);
        for token in non_whitespace_tokens(&parsed.syntax()) {
            let projected = compared(&token, Certificate::UpToSpelling);
            let canonical = canonical_spelling(token.kind(), content(&token));
            assert_eq!(projected, canonical, "{name}");
            assert_eq!(
                matches!(projected, Cow::Owned(_)),
                canonical != content(&token),
                "{name}: allocates exactly to respell a synonym"
            );
        }
    }
}
