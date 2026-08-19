//! Comment attachment, the owned policy (docs/design/syntax.md §9): a
//! pure reading of the tree, a function of exactly four facts, shipped
//! in two forms that agree by law — never a table, since this tree
//! carries every comment in place and nothing can go stale.

use std::collections::VecDeque;
use std::fmt;

use crate::tree::{
    NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole, WalkEvent, role,
};

/// The slot a comment is attached in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot {
    /// On its own line(s) directly above its anchor.
    Leading,
    /// On its anchor's line, after it.
    Trailing,
    /// Inside its parent, attached to nothing nearer.
    Dangling,
}

/// One comment's attachment: the element it belongs to and how. The
/// anchor is a node or a significant token — a comment before `,` leads
/// the comma, which is what keeps it before the comma when a consumer
/// re-emits (kallos's transposition scar, spec §5.1); a comment on the
/// line of a rule's dot trails the rule. A view, not data: the anchor is
/// a cursor, which is the shape a formatter holding the tree wants — it
/// navigates from the anchor directly — and it lives no longer than the
/// tree it reads.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Attachment {
    /// The element the comment belongs to.
    pub anchor: SyntaxElement,
    /// How.
    pub slot: Slot,
}

/// Why a token has no attachment: it is not a comment, or it is a doc
/// line in docs position — structure the statement owns
/// (docs/design/syntax.md §5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotAttachable {
    /// A significant token, or whitespace.
    NotAComment {
        /// Its kind.
        kind: SyntaxKind,
    },
    /// A statement's documentation.
    Documentation,
}

impl fmt::Display for NotAttachable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotAttachable::NotAComment { kind } => write!(f, "the token is {kind}, not a comment"),
            NotAttachable::Documentation => {
                f.write_str("the token is a statement's documentation, not a comment")
            }
        }
    }
}

impl std::error::Error for NotAttachable {}

/// The line break (base §5's newline policy: a `\r` is content of its
/// line).
const LINE_BREAK: char = '\n';

/// Whether `token` is a trivia comment: a comment by kind whose role is
/// `Trivia` (docs/design/syntax.md §5.4).
fn is_trivia_comment(token: &SyntaxToken) -> bool {
    token.kind().is_comment() && role(token) == TokenRole::Trivia
}

/// Whether `element` is skipped when looking for `prev` and `next`:
/// trivia, or an empty node (docs/design/syntax.md §5.4, §9.2).
fn is_skipped(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => role(token) == TokenRole::Trivia,
        NodeOrToken::Node(node) => node.text_range().is_empty(),
    }
}

/// Whether `element` is a closer: a token that ends a construct rather
/// than begins an element — `)`, `]`, `}`, `.`, or the `|` of an
/// absolute value; the `|` of a disjunction is a separator and an
/// anchor like `;` (spec §6.4's dual-role-token carve-out, decided
/// structurally).
fn is_closer(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => match token.kind() {
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE | SyntaxKind::DOT => {
                true
            }
            SyntaxKind::PIPE => token
                .parent()
                .is_some_and(|parent| parent.kind() == SyntaxKind::ABS_TERM),
            _ => false,
        },
        NodeOrToken::Node(_) => false,
    }
}

/// Whether the text of `element` holds a line break — a whitespace run
/// or a multi-line block comment.
fn breaks_line(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => token.text().contains(LINE_BREAK),
        NodeOrToken::Node(node) => node.text().contains_char(LINE_BREAK),
    }
}

/// Whether `element` is an empty line: a `WHITESPACE` token containing
/// two line breaks with only horizontal whitespace between them — a run
/// with at least two line breaks, since a run holds nothing but
/// whitespace (docs/design/syntax.md §9.2).
fn is_empty_line(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => {
            token.kind() == SyntaxKind::WHITESPACE && token.text().matches(LINE_BREAK).count() >= 2
        }
        NodeOrToken::Node(_) => false,
    }
}

/// A comment's attachment. Refuses a token that is not a trivia comment
/// — a doc line in docs position (structure) or any significant token —
/// with the reason. Total otherwise; O(the trivia between `prev` and
/// `next` around the comment), allocation-free.
pub fn attachment(comment: &SyntaxToken) -> Result<Attachment, NotAttachable> {
    if !comment.kind().is_comment() {
        return Err(NotAttachable::NotAComment {
            kind: comment.kind(),
        });
    }
    if role(comment) == TokenRole::Documentation {
        return Err(NotAttachable::Documentation);
    }
    let parent = comment.parent().expect("a token in a tree has a parent");
    // Rule 1: trailing — `prev` exists and no line break stands between
    // its end and the comment's start.
    let mut broken = false;
    let mut cursor = comment.prev_sibling_or_token();
    let mut prev = None;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            prev = Some(element);
            break;
        }
        broken |= breaks_line(&element);
        cursor = element.prev_sibling_or_token();
    }
    if let Some(prev) = prev
        && !broken
    {
        return Ok(Attachment {
            anchor: prev,
            slot: Slot::Trailing,
        });
    }
    // Rule 2: leading — `next` exists, is not a closer, and no empty
    // line stands in the run from the comment to it.
    let mut gap = false;
    let mut cursor = comment.next_sibling_or_token();
    let mut next = None;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            next = Some(element);
            break;
        }
        gap |= is_empty_line(&element);
        cursor = element.next_sibling_or_token();
    }
    if let Some(next) = next
        && !gap
        && !is_closer(&next)
    {
        return Ok(Attachment {
            anchor: next,
            slot: Slot::Leading,
        });
    }
    // Rule 3: dangling in the parent — total, since every comment has one.
    Ok(Attachment {
        anchor: NodeOrToken::Node(parent),
        slot: Slot::Dangling,
    })
}

/// Every trivia comment among `parent`'s children with its attachment,
/// in order, in one pass: the rules read as cumulative facts along the
/// children — a line break since the last significant sibling, an empty
/// line before the next — so each comment resolves in constant time.
fn resolve_children(parent: &SyntaxNode) -> Vec<(SyntaxToken, Attachment)> {
    let elements: Vec<SyntaxElement> = parent.children_with_tokens().collect();
    let count = elements.len();
    // Forward: the nearest significant sibling before each element and
    // whether a line break stands between it and the element.
    let mut prev_of: Vec<Option<usize>> = Vec::with_capacity(count);
    let mut broken_before: Vec<bool> = Vec::with_capacity(count);
    let mut last_significant = None;
    let mut broken = false;
    for element in &elements {
        prev_of.push(last_significant);
        broken_before.push(broken);
        if is_skipped(element) {
            broken |= breaks_line(element);
        } else {
            last_significant = Some(prev_of.len() - 1);
            broken = false;
        }
    }
    // Backward: the nearest significant sibling after each element and
    // whether an empty line stands between the element and it.
    let mut next_of: Vec<Option<usize>> = vec![None; count];
    let mut gap_after: Vec<bool> = vec![false; count];
    let mut next_significant = None;
    let mut gap = false;
    for (index, element) in elements.iter().enumerate().rev() {
        next_of[index] = next_significant;
        gap_after[index] = gap;
        if is_skipped(element) {
            gap |= is_empty_line(element);
        } else {
            next_significant = Some(index);
            gap = false;
        }
    }
    let mut out = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        if !is_trivia_comment(token) {
            continue;
        }
        let attachment = match (prev_of[index], broken_before[index]) {
            (Some(prev), false) => Attachment {
                anchor: elements[prev].clone(),
                slot: Slot::Trailing,
            },
            _ => match next_of[index] {
                Some(next) if !gap_after[index] && !is_closer(&elements[next]) => Attachment {
                    anchor: elements[next].clone(),
                    slot: Slot::Leading,
                },
                _ => Attachment {
                    anchor: NodeOrToken::Node(parent.clone()),
                    slot: Slot::Dangling,
                },
            },
        };
        out.push((token.clone(), attachment));
    }
    out
}

/// The comments attached to `anchor` in `slot`, in source order — the
/// inverse direction, for a consumer walking anchors. Total; O(the
/// trivia adjacent to the anchor) for `Leading` and `Trailing`, O(the
/// anchor's children) for `Dangling`.
pub fn comments(anchor: &SyntaxElement, slot: Slot) -> impl Iterator<Item = SyntaxToken> {
    let found: Vec<SyntaxToken> = match slot {
        Slot::Trailing => trailing(anchor),
        Slot::Leading => leading(anchor),
        Slot::Dangling => match anchor {
            NodeOrToken::Node(node) => resolve_children(node)
                .into_iter()
                .filter(|(_, attachment)| attachment.slot == Slot::Dangling)
                .map(|(comment, _)| comment)
                .collect(),
            NodeOrToken::Token(_) => Vec::new(),
        },
    };
    found.into_iter()
}

/// The comments trailing `anchor`: the trivia comments after it, up to
/// the first line break.
fn trailing(anchor: &SyntaxElement) -> Vec<SyntaxToken> {
    let mut found = Vec::new();
    let mut cursor = anchor.next_sibling_or_token();
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            break;
        }
        if let NodeOrToken::Token(token) = &element
            && is_trivia_comment(token)
        {
            found.push(token.clone());
        }
        if breaks_line(&element) {
            break;
        }
        cursor = element.next_sibling_or_token();
    }
    found
}

/// The comments leading `anchor`: the trivia comments in the run before
/// it — back to the previous significant sibling — that trail nothing
/// (no `prev`, or a line break between `prev` and the comment) and
/// stand after the run's last empty line; none when the anchor is a
/// closer.
fn leading(anchor: &SyntaxElement) -> Vec<SyntaxToken> {
    if is_closer(anchor) {
        return Vec::new();
    }
    let mut run: Vec<SyntaxElement> = Vec::new();
    let mut cursor = anchor.prev_sibling_or_token();
    let mut prev_exists = false;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            prev_exists = true;
            break;
        }
        run.push(element.clone());
        cursor = element.prev_sibling_or_token();
    }
    run.reverse();
    let after_gap = run.iter().rposition(is_empty_line).map_or(0, |gap| gap + 1);
    let mut found = Vec::new();
    // Rule 1 cannot hold where no `prev` exists; where one does, it holds
    // for every comment until the first line break after `prev`.
    let mut not_trailing = !prev_exists;
    for (index, element) in run.iter().enumerate() {
        if let NodeOrToken::Token(token) = element
            && is_trivia_comment(token)
            && not_trailing
            && index >= after_gap
        {
            found.push(token.clone());
        }
        not_trailing |= breaks_line(element);
    }
    found
}

/// Every trivia comment under `node` with its attachment, in source
/// order, computed in one pass — the bulk form. Total; O(subtree).
pub fn attachments(node: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, Attachment)> {
    let mut out = Vec::new();
    // Per open node, its comments' attachments, resolved once, consumed
    // in token order as the walk meets them.
    let mut open: Vec<VecDeque<(SyntaxToken, Attachment)>> = Vec::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(inner)) => {
                open.push(resolve_children(&inner).into_iter().collect());
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => {
                open.pop();
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                if is_trivia_comment(&token)
                    && let Some(resolved) = open.last_mut().and_then(VecDeque::pop_front)
                {
                    out.push(resolved);
                }
            }
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out.into_iter()
}

/// The tree root that `element` belongs to.
fn root_of(element: &SyntaxElement) -> SyntaxNode {
    match element {
        NodeOrToken::Node(node) => node.ancestors().last().expect("a node is its own ancestor"),
        NodeOrToken::Token(token) => token
            .parent_ancestors()
            .last()
            .expect("a token in a tree has a parent"),
    }
}

/// The source text strictly between `a`'s end and `b`'s start, read from
/// the shared tree — so the whitespace facts are a fact of position, not
/// of siblinghood (docs/design/syntax.md §9.3): the text holds whatever
/// stands there, trivia or a significant token between two non-adjacent
/// elements. Empty when the two abut, when `b` starts before `a` ends,
/// or when the offsets fall outside the tree (a pair from two trees) —
/// so the facts are total and never panic on the slice.
fn text_between(a: &SyntaxElement, b: &SyntaxElement) -> String {
    let root = root_of(a);
    let bound = root.text_range().end();
    let start = a.text_range().end().min(bound);
    let end = b.text_range().start().min(bound);
    if start >= end {
        return String::new();
    }
    root.text().slice(start..end).to_string()
}

/// No line break in the text between `a`'s end and `b`'s start. Total;
/// O(the trivia between the two elements).
pub fn same_line(a: &SyntaxElement, b: &SyntaxElement) -> bool {
    !text_between(a, b).contains(LINE_BREAK)
}

/// An empty line in the whitespace directly between `a` and `b`; false
/// when anything but whitespace — a token, a node, a comment — stands
/// between them, so a non-adjacent pair answers false rather than
/// refusing. Total; O(the trivia between the two elements).
pub fn empty_line_between(a: &SyntaxElement, b: &SyntaxElement) -> bool {
    let text = text_between(a, b);
    let whitespace_only = text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
    whitespace_only && text.matches(LINE_BREAK).count() >= 2
}

/// The count of line breaks in the text between `a`'s end and `b`'s
/// start — all of it, so a significant token between a non-adjacent pair
/// counts too, as `same_line` reads it. Total; O(the trivia between the
/// two elements).
pub fn line_breaks_between(a: &SyntaxElement, b: &SyntaxElement) -> u32 {
    text_between(a, b)
        .matches(LINE_BREAK)
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::{AstNode, SyntaxKind};

    fn parsed(text: &str) -> SyntaxNode {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo).syntax()
    }

    /// The trivia comments under `root`, in order.
    fn trivia_comments(root: &SyntaxNode) -> Vec<SyntaxToken> {
        root.descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|t| t.kind().is_comment() && role(t) == TokenRole::Trivia)
            .collect()
    }

    /// `slot anchor-kind` for the comment's attachment.
    fn describe(comment: &SyntaxToken) -> String {
        match attachment(comment) {
            Ok(Attachment { anchor, slot }) => format!("{slot:?} {}", anchor.kind()),
            Err(refusal) => format!("{refusal:?}"),
        }
    }

    #[test]
    fn a_comment_on_the_line_of_a_rules_dot_trails_the_rule() {
        let root = parsed("p. % trailing\nq.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Trailing RULE");
    }

    #[test]
    fn a_comment_on_its_own_line_leads_what_follows_unless_a_blank_line_or_a_closer_stands_between()
    {
        let root = parsed("% leading\np.\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading RULE");
        let root = parsed("% a\n\n% b\np.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Dangling PROGRAM");
        assert_eq!(describe(&comments[1]), "Leading RULE");
        let root = parsed("p(1\n % c\n).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Dangling ARGUMENTS");
    }

    #[test]
    fn a_comment_before_a_comma_leads_the_comma_and_after_it_trails_it() {
        let root = parsed("p(1\n % c\n , 2).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading COMMA");
        let root = parsed("p(1, % c\n 2).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Trailing COMMA");
        let root = parsed("p(1 % c\n , 2).\n");
        assert_eq!(
            describe(&trivia_comments(&root)[0]),
            "Trailing CONSTANT_TERM"
        );
    }

    #[test]
    fn the_pipe_is_a_separator_in_a_disjunction_and_a_closer_in_an_absolute_value() {
        let root = parsed("a\n% c\n| b.\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading PIPE");
        let root = parsed("p(|X\n% c\n|).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Dangling ABS_TERM");
    }

    #[test]
    fn a_multi_line_block_comment_between_prev_and_the_comment_breaks_the_line() {
        let root = parsed("p. %* a\nb *% % c\nq.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Trailing RULE");
        assert_eq!(describe(&comments[1]), "Leading RULE");
    }

    #[test]
    fn documentation_and_significant_tokens_are_refused() {
        let root = parsed("%! doc\np. %! stray\n");
        let tokens: Vec<SyntaxToken> = root
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .collect();
        let doc = tokens
            .iter()
            .find(|t| t.text() == "%! doc")
            .expect("the doc line");
        assert_eq!(attachment(doc), Err(NotAttachable::Documentation));
        let stray = tokens
            .iter()
            .find(|t| t.text() == "%! stray")
            .expect("the stray line");
        assert_eq!(describe(stray), "Trailing RULE");
        let dot = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::DOT)
            .expect("a dot");
        assert_eq!(
            attachment(dot),
            Err(NotAttachable::NotAComment {
                kind: SyntaxKind::DOT
            })
        );
        assert_eq!(
            NotAttachable::NotAComment {
                kind: SyntaxKind::DOT
            }
            .to_string(),
            "the token is DOT, not a comment"
        );
    }

    #[test]
    fn crlf_empty_lines_detach_exactly_as_lf_ones() {
        let root = parsed("% a\r\n\r\n% b\r\np.\r\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Dangling PROGRAM");
        assert_eq!(describe(&comments[1]), "Leading RULE");
    }

    #[test]
    fn the_two_forms_agree_and_the_bulk_form_yields_every_comment_once() {
        let root = parsed("% lead\np(1, % after comma\n 2). % trail\n\n% dangling\n");
        let all: Vec<(SyntaxToken, Attachment)> = attachments(&root).collect();
        assert_eq!(all.len(), 4);
        for (comment, att) in &all {
            assert_eq!(attachment(comment).as_ref(), Ok(att));
            let back: Vec<SyntaxToken> = comments(&att.anchor, att.slot).collect();
            assert!(
                back.contains(comment),
                "the inverse form yields {}",
                comment.text()
            );
        }
        let program = SyntaxElement::Node(root.clone());
        let dangling: Vec<String> = comments(&program, Slot::Dangling)
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(dangling, ["% dangling"]);
    }

    #[test]
    fn the_whitespace_facts() {
        let root = parsed("p(1,\n\n 2). q.\n");
        let tokens: Vec<SyntaxToken> = root
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .collect();
        let comma = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::COMMA)
            .expect("a comma");
        let two = tokens.iter().find(|t| t.text() == "2").expect("the 2");
        let one = tokens.iter().find(|t| t.text() == "1").expect("the 1");
        let a = SyntaxElement::Token(comma.clone());
        let b = SyntaxElement::Token(two.clone());
        assert!(!same_line(&a, &b));
        assert!(same_line(&SyntaxElement::Token(one.clone()), &a));
        assert!(empty_line_between(&a, &b));
        assert_eq!(line_breaks_between(&a, &b), 2);
        let rules: Vec<SyntaxNode> = root.children().collect();
        let first = SyntaxElement::Node(rules[0].clone());
        let second = SyntaxElement::Node(rules[1].clone());
        assert!(same_line(&first, &second));
        assert!(!empty_line_between(&first, &second));
        assert!(
            !empty_line_between(&SyntaxElement::Token(one.clone()), &b),
            "a token stands between"
        );
        let _ = crate::ast::Program::cast(root);
    }
}
