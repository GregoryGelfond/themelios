//! The certificates' reflexivity through reparse, symmetry, and the
//! corollary — equal non-whitespace sequences, equal significant-token
//! shapes, under equal dialects and one root family, outside the aspif
//! dispatch — and `canonical_spelling` idempotent and closed over the
//! synonym pairs (docs/design/syntax.md §11, §16).

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{Certificate, canonical_spelling, equivalent};
use themelios_syntax::fusion::{Separator, separator};
use themelios_syntax::parse::{Parse, parse};
use themelios_syntax::tree::{
    NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, TokenRole, WalkEvent, role,
};

fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let dialect = fs::read_to_string(path.with_extension("expect"))
                    .ok()
                    .and_then(|sidecar| sidecar.lines().next().map(str::to_owned))
                    .map_or(Dialect::Clingo, |line| {
                        if line == "asp-core-2" {
                            Dialect::AspCore2
                        } else {
                            Dialect::Clingo
                        }
                    });
                found.push((
                    path.strip_prefix(&dir)
                        .expect("under corpus")
                        .display()
                        .to_string(),
                    text,
                    dialect,
                ));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

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
