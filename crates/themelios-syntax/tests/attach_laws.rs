//! Attachment totality, single-valuedness, the inverse law between the
//! two forms, and stability under re-spacing that preserves the four
//! facts (docs/design/syntax.md §9.2, §16), over the corpus and over
//! generated re-spacings.

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::attach::{Slot, attachment, attachments, comments};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole, role};

fn corpus_texts() -> Vec<(String, String)> {
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
                found.push((
                    path.strip_prefix(&dir)
                        .expect("under corpus")
                        .display()
                        .to_string(),
                    text,
                ));
            }
        }
    }
    found.sort();
    found
}

fn root(text: &str) -> SyntaxNode {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, Dialect::Clingo).syntax()
}

fn trivia_comments(root: &SyntaxNode) -> Vec<SyntaxToken> {
    root.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|t| t.kind().is_comment() && role(t) == TokenRole::Trivia)
        .collect()
}

/// `text` with every whitespace run collapsed to one space and the ends
/// trimmed — the part of a rendering re-spacing cannot touch. Read the
/// comment and anchor through it so two records compare equal exactly
/// when re-spacing preserved the attachment: re-spacing changes the
/// rendered spacing of both (a line comment absorbs a trailing space, a
/// node anchor holds its inner whitespace), never the four facts §9.2
/// reads.
fn without_spacing(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A comparable record of one attachment: the comment, the slot, the
/// anchor's kind and text — everything but positions and spacing.
fn record(root: &SyntaxNode) -> Vec<(String, Slot, SyntaxKind, String)> {
    attachments(root)
        .map(|(comment, attachment)| {
            (
                without_spacing(comment.text()),
                attachment.slot,
                attachment.anchor.kind(),
                without_spacing(&attachment.anchor.to_string()),
            )
        })
        .collect()
}

#[test]
fn every_trivia_comment_of_the_corpus_attaches_once_and_the_two_forms_agree() {
    for (name, text) in corpus_texts() {
        let root = root(&text);
        let comments_in_tree = trivia_comments(&root);
        let bulk: Vec<_> = attachments(&root).collect();
        assert_eq!(
            bulk.len(),
            comments_in_tree.len(),
            "{name}: the bulk form yields each trivia comment once"
        );
        for ((comment, bulk_attachment), in_tree) in bulk.iter().zip(&comments_in_tree) {
            assert_eq!(comment, in_tree, "{name}: in source order");
            let single = attachment(comment).expect("a trivia comment attaches");
            assert_eq!(
                &single,
                bulk_attachment,
                "{name}: the two forms agree on {}",
                comment.text()
            );
            assert!(
                comments(&single.anchor, single.slot).any(|c| &c == comment),
                "{name}: the inverse form yields {}",
                comment.text()
            );
        }
    }
}

/// A whitespace token's text re-spaced within its class: no line break
/// stays without one; one line break stays with exactly one; two or
/// more stay with two or more — the facts the policy reads, kept.
fn respace(text: &str, choice: u8) -> String {
    let breaks = text.matches('\n').count();
    let horizontal = match choice % 3 {
        0 => " ",
        1 => "\t",
        _ => "  ",
    };
    match breaks {
        0 => horizontal.to_owned(),
        1 => format!("{horizontal}\n{horizontal}"),
        _ => format!("\n{horizontal}\n\n"),
    }
}

/// The text with every whitespace token re-spaced by the choices.
fn respaced(root: &SyntaxNode, choices: &[u8]) -> String {
    let mut out = String::new();
    let mut next_choice = choices.iter().copied().cycle();
    for token in root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
    {
        if token.kind() == SyntaxKind::WHITESPACE {
            out.push_str(&respace(token.text(), next_choice.next().unwrap_or(0)));
        } else {
            out.push_str(token.text());
        }
    }
    out
}

const SCAR_TEXT: &str = "% lead\np(1, % c1\n  2 % c2\n , 3). % t\n\n% dangling above a gap\n\n%* block\nacross *% q :- r. % end\n";

proptest! {
    #[test]
    fn re_spacing_that_keeps_the_four_facts_keeps_every_attachment(choices in prop::collection::vec(0u8..3, 1..16)) {
        for text in [SCAR_TEXT, "% a\n%* b *%\np. % t\n% l\nq(X, %c\n Y).\n"] {
            let before = root(text);
            let after = root(&respaced(&before, &choices));
            prop_assert_eq!(record(&before), record(&after));
        }
    }
}

#[test]
fn violating_a_fact_changes_exactly_the_attachments_that_read_it() {
    // Joining a leading comment onto the previous line makes it trailing.
    let before = record(&root("p.\n% lead\nq.\n"));
    let after = record(&root("p. % lead\nq.\n"));
    assert_eq!(before[0].1, Slot::Leading);
    assert_eq!(after[0].1, Slot::Trailing);
    // Opening an empty line inside a leading run detaches the comments above it.
    let before = record(&root("% a\n% b\np.\n"));
    let after = record(&root("% a\n\n% b\np.\n"));
    assert_eq!((before[0].1, before[1].1), (Slot::Leading, Slot::Leading));
    assert_eq!((after[0].1, after[1].1), (Slot::Dangling, Slot::Leading));
}
