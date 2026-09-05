//! Attachment totality, single-valuedness, the inverse law between the
//! two forms, and stability under re-spacing that preserves the four
//! facts (docs/design/syntax.md §9.2, §16), over the corpus and over
//! generated re-spacings.

use std::collections::{BTreeSet, HashMap};

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::{AstToken, Comment};
use themelios_syntax::attach::{Attachment, Slot, attachment, attachments, comments, is_skipped};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole, role};

mod common;

fn corpus_texts() -> Vec<(String, String)> {
    common::corpus()
        .into_iter()
        .map(|(name, text, _)| (name, text))
        .collect()
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
            assert_eq!(comment.syntax(), in_tree, "{name}: in source order");
            let single = attachment(comment.syntax()).expect("a trivia comment attaches");
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

/// Runs of two comments sharing one anchor and slot, at a node anchor
/// and at a token anchor: `% a` and `% b` lead the first rule, `%* c *%`
/// and `% d` trail its comma, `%* e *%` and `% f` trail the rule, `% g`
/// and `% h` lead the second rule's comma. The corpus holds leading runs
/// of its own but no two comments trailing one element — a line comment
/// ends its line, so only a single-line block comment can stand before
/// another comment on an anchor's line.
const RUNS_TEXT: &str = "% a\n% b\np(1, %* c *% % d\n 2). %* e *% % f\nq(1\n % g\n % h\n , 2).\n";

/// A comment on its own line directly before each closer — `)`, the `|`
/// of an absolute value, `]`, `}`, and `.` — with nothing else to refuse
/// it: rule 2 dangles each by fact (d) alone (docs/design/syntax.md
/// §9.2), so at the closer the inverse form's own refusal is all that
/// keeps its leading yield empty. The corpus holds no comment line
/// followed by a closer-initial line.
const CLOSERS_TEXT: &str =
    "p(1\n % a\n).\nq(|X\n % b\n|).\n:~ r. [1@2\n % c\n]\n{ s\n % d\n}.\nt\n % e\n.\n";

/// Whether `anchor` is a closer (docs/design/syntax.md §9.2, fact (d))
/// with a comment the bulk form dangles in the run of trivia before it
/// and no empty line — a whitespace token holding two line breaks —
/// between that comment and the closer: the shape where the inverse
/// form's refusal of a closer alone stands between the comment and a
/// leading yield.
fn refused_closer(anchor: &SyntaxElement, groups: &HashMap<Attachment, Vec<Comment>>) -> bool {
    let SyntaxElement::Token(token) = anchor else {
        return false;
    };
    let closer = match token.kind() {
        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE | SyntaxKind::DOT => true,
        SyntaxKind::PIPE => token
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::ABS_TERM),
        _ => false,
    };
    let Some(parent) = token.parent().filter(|_| closer) else {
        return false;
    };
    let Some(dangling) = groups.get(&Attachment {
        anchor: SyntaxElement::Node(parent),
        slot: Slot::Dangling,
    }) else {
        return false;
    };
    let mut cursor = anchor.prev_sibling_or_token();
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            return false;
        }
        if element.kind() == SyntaxKind::WHITESPACE
            && element.to_string().matches('\n').count() >= 2
        {
            return false;
        }
        if let SyntaxElement::Token(candidate) = &element
            && dangling.iter().any(|comment| comment.syntax() == candidate)
        {
            return true;
        }
        cursor = element.prev_sibling_or_token();
    }
    false
}

/// The bulk form's partition: the comments it attaches at each
/// `(anchor, slot)`, in source order.
fn partition(root: &SyntaxNode) -> HashMap<Attachment, Vec<Comment>> {
    let mut groups: HashMap<Attachment, Vec<Comment>> = HashMap::new();
    for (comment, attachment) in attachments(root) {
        groups.entry(attachment).or_default().push(comment);
    }
    groups
}

#[test]
fn the_inverse_form_equals_the_bulk_partition() {
    // The bulk form is the authority — total and single-valued by the test
    // above, whose reading of the inverse form is one direction only: it
    // yields each comment the bulk form attaches. `leading` and `trailing`
    // are a second reading of the rule, so the inverse form is held to the
    // bulk form's partition element for element: at every anchor — every
    // element that is not skipped, the root included — and every slot, the
    // comments the bulk form attaches there, in source order, none extra,
    // none repeated, none where the bulk form attaches nothing. With two
    // witnesses: a leading run and a trailing run of two or more are
    // reached, where order and repetition are not vacuous; and a comment
    // on its own line before each closer is reached — the `(closer,
    // Leading)` case rule 2 refuses by fact (d), where the inverse form's
    // refusal alone keeps the yield empty.
    let mut leading_runs = 0usize;
    let mut trailing_runs = 0usize;
    let mut closers_refused: BTreeSet<SyntaxKind> = BTreeSet::new();
    let fixtures = [
        ("scar", SCAR_TEXT),
        ("runs", RUNS_TEXT),
        ("closers", CLOSERS_TEXT),
    ]
    .map(|(name, text)| (name.to_owned(), text.to_owned()));
    for (name, text) in corpus_texts().into_iter().chain(fixtures) {
        let root = root(&text);
        let groups = partition(&root);
        let mut reached = 0usize;
        for anchor in root
            .descendants_with_tokens()
            .filter(|element| !is_skipped(element))
        {
            for slot in [Slot::Leading, Slot::Trailing, Slot::Dangling] {
                let read: Vec<Comment> = comments(&anchor, slot).collect();
                let group = groups.get(&Attachment {
                    anchor: anchor.clone(),
                    slot,
                });
                let expected = group.map_or(&[][..], Vec::as_slice);
                assert_eq!(
                    read,
                    expected,
                    "{name}: {slot:?} of {} at {:?}",
                    anchor.kind(),
                    anchor.text_range()
                );
                if group.is_some() {
                    reached += 1;
                }
                if expected.len() >= 2 {
                    match slot {
                        Slot::Leading => leading_runs += 1,
                        Slot::Trailing => trailing_runs += 1,
                        Slot::Dangling => {}
                    }
                }
                if slot == Slot::Leading && refused_closer(&anchor, &groups) {
                    closers_refused.insert(anchor.kind());
                }
            }
        }
        assert_eq!(
            reached,
            groups.len(),
            "{name}: every anchor the bulk form attaches to is an unskipped element"
        );
    }
    assert!(
        leading_runs > 0 && trailing_runs > 0,
        "a leading and a trailing run of two or more are reached ({leading_runs}, {trailing_runs})"
    );
    assert_eq!(
        closers_refused,
        BTreeSet::from([
            SyntaxKind::R_PAREN,
            SyntaxKind::PIPE,
            SyntaxKind::R_BRACKET,
            SyntaxKind::R_BRACE,
            SyntaxKind::DOT,
        ]),
        "a comment on its own line before each closer is reached"
    );
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
