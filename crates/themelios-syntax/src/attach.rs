//! Comment attachment, the owned policy (docs/design/syntax.md §9): a
//! pure reading of the tree, a function of exactly four facts, shipped
//! in two forms that agree by law — never a table, since this tree
//! carries every comment in place and nothing can go stale. Beside it,
//! the significant-child walk the policy reads `prev` and `next` over
//! (§5.4, §9.2) — `is_skipped`, `significant_children`, and the
//! directional skips — the trivia guard a consumer of the tree would
//! otherwise re-derive.

use std::collections::VecDeque;
use std::fmt;
use std::iter;

use crate::ast::{AstToken, Comment};
use crate::tree::{
    Direction, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole,
    WalkEvent, keeps_leading, role, role_of,
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
/// `Trivia` (docs/design/syntax.md §5.4) — `Comment::cast`'s own test,
/// read here by the walks that yield the token rather than the wrapper.
fn is_trivia_comment(token: &SyntaxToken) -> bool {
    token.kind().is_comment() && role(token) == TokenRole::Trivia
}

/// Whether a significant-child walk skips `element`: a trivia token —
/// whitespace, or a comment that is not a statement's documentation, as
/// `role` reads it — or an empty node, which holds no token to stand at
/// (docs/design/syntax.md §5.4, §9.2). The attachment policy's `prev`
/// and `next` are read over exactly the elements this refuses. Total;
/// O(1) for every kind but `DOC_COMMENT`, whose role is a fact of
/// position — `role`'s O(preceding siblings).
pub fn is_skipped(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => role(token) == TokenRole::Trivia,
        NodeOrToken::Node(node) => node.text_range().is_empty(),
    }
}

/// Each child of `node` with `is_skipped`'s answer for it, read in one
/// forward pass over the children: the two positional facts `role`
/// scans a token's preceding siblings for — whether the parent is a
/// statement, whether every element so far keeps the leading trivia/doc
/// prefix — are carried along the pass and fed to `role_of`, the one
/// definition, so a doc line's role costs O(1) here rather than O(its
/// preceding siblings), and a k-line doc block O(k) rather than O(k²).
/// Total; O(the children).
fn skipped_children(node: &SyntaxNode) -> impl Iterator<Item = (SyntaxElement, bool)> + '_ {
    let is_statement = node.kind().is_statement();
    let mut leading = true;
    node.children_with_tokens().map(move |element| {
        let skipped = match &element {
            NodeOrToken::Token(token) => {
                role_of(token.kind(), is_statement, leading) == TokenRole::Trivia
            }
            NodeOrToken::Node(node) => node.text_range().is_empty(),
        };
        if !keeps_leading(&element) {
            leading = false;
        }
        (element, skipped)
    })
}

/// The children of `node` that are not skipped (`is_skipped`), in order
/// — the significant-child walk, so a consumer reads the guard here
/// instead of re-deriving it. One forward pass over the children
/// (`skipped_children`), each read once with the role facts carried
/// along, never a per-child scan of its siblings: O(the children). Total.
pub fn significant_children(node: &SyntaxNode) -> impl Iterator<Item = SyntaxElement> + '_ {
    skipped_children(node)
        .filter(|(_, skipped)| !skipped)
        .map(|(element, _)| element)
}

/// The nearest sibling of `element` in `direction` that is not skipped —
/// the attachment policy's `prev` (`Direction::Prev`) or `next`
/// (`Direction::Next`) of the element (docs/design/syntax.md §9.2) — or
/// `None` when only skipped siblings, or none, remain. Total; O(the
/// siblings stepped over), each read by `is_skipped` — and a stepped
/// `DOC_COMMENT`'s role is `role`'s O(preceding siblings), so on a
/// statement carrying a k-line doc block and a run of m misplaced `%!`
/// lines after its head, a step across the run is O(k·m). For a whole
/// node's significant children, `significant_children` is one pass,
/// O(the node's children).
pub fn non_trivia_sibling(element: SyntaxElement, direction: Direction) -> Option<SyntaxElement> {
    iter::successors(Some(element), |element| match direction {
        Direction::Next => element.next_sibling_or_token(),
        Direction::Prev => element.prev_sibling_or_token(),
    })
    .skip(1)
    .find(|element| !is_skipped(element))
}

/// `token` itself when it is not trivia; otherwise the nearest non-trivia
/// token in `direction`, in document order — across node boundaries, as
/// `next_token` and `prev_token` step — or `None` when the tree ends
/// first. Trivia is `role`'s: a `DOC_COMMENT` in docs position stops the
/// walk, a stray one is stepped over. Total; O(the trivia stepped over),
/// each read by `role` — and a stepped `DOC_COMMENT` costs O(its
/// preceding siblings), so crossing a run of m misplaced `%!` lines
/// after the head of a statement carrying a k-line doc block is O(k·m).
/// `roles_of` reads a node's roles in one pass, O(the node's children).
pub fn skip_trivia_token(mut token: SyntaxToken, direction: Direction) -> Option<SyntaxToken> {
    while role(&token) == TokenRole::Trivia {
        token = match direction {
            Direction::Next => token.next_token()?,
            Direction::Prev => token.prev_token()?,
        };
    }
    Some(token)
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
/// with the reason. Total otherwise, allocation-free; O(the trivia
/// between `prev` and `next` around the comment), each read by
/// `is_skipped` — a stepped `DOC_COMMENT`'s role is `role`'s O(preceding
/// siblings), so beside a run of m misplaced `%!` lines under a k-line
/// doc block it is O(k·m). For all of a tree's comments, `attachments`
/// is the bulk form, O(subtree).
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

/// Every trivia comment among `parent`'s children, as the typed
/// `Comment`, with its attachment, in order, in one pass: the rules read
/// as cumulative facts along the children — a line break since the last
/// significant sibling, an empty line before the next — so each comment
/// resolves in constant time. The roles are read once, by the forward
/// pass `skipped_children` makes, and that reading is the admission: a
/// comment by kind that the pass skips is a trivia comment by
/// `Comment`'s own test of kind and role, so the wrapper is built from
/// the fact already read — never by a cast, which would read `role` a
/// second time per token. O(the children).
fn resolve_children(parent: &SyntaxNode) -> Vec<(Comment, Attachment)> {
    let (elements, skipped): (Vec<SyntaxElement>, Vec<bool>) = skipped_children(parent).unzip();
    let count = elements.len();
    // Forward: the nearest significant sibling before each element and
    // whether a line break stands between it and the element.
    let mut prev_of: Vec<Option<usize>> = Vec::with_capacity(count);
    let mut broken_before: Vec<bool> = Vec::with_capacity(count);
    let mut last_significant = None;
    let mut broken = false;
    for (index, element) in elements.iter().enumerate() {
        prev_of.push(last_significant);
        broken_before.push(broken);
        if skipped[index] {
            broken |= breaks_line(element);
        } else {
            last_significant = Some(index);
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
        if skipped[index] {
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
        if !(token.kind().is_comment() && skipped[index]) {
            continue;
        }
        let comment = Comment::from_trivia_comment(token.clone());
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
        out.push((comment, attachment));
    }
    out
}

/// The comments attached to `anchor` in `slot`, as the typed `Comment`,
/// in source order — the inverse direction, for a consumer walking
/// anchors. Total; O(the trivia adjacent to the anchor) for `Trailing`,
/// O(the anchor's children) for `Dangling`; for `Leading`, the run
/// before the anchor is read with the same per-`DOC_COMMENT` role read
/// the directional walks make — `role`'s O(preceding siblings) — so on
/// a statement carrying a k-line doc block and a run of m misplaced
/// `%!` lines after its head it is O(k·m). For all of a tree's
/// comments, `attachments` is the bulk form, O(subtree).
pub fn comments(anchor: &SyntaxElement, slot: Slot) -> impl Iterator<Item = Comment> {
    let found: Vec<Comment> = match slot {
        // `trailing` and `leading` yield the token, each established a
        // trivia comment by `is_trivia_comment`: the wrapper is built from
        // that fact, never by a cast that would read `role` again.
        Slot::Trailing => trailing(anchor)
            .into_iter()
            .map(Comment::from_trivia_comment)
            .collect(),
        Slot::Leading => leading(anchor)
            .into_iter()
            .map(Comment::from_trivia_comment)
            .collect(),
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

/// Every trivia comment under `node`, as the typed `Comment`, with its
/// attachment, in source order, computed in one pass — the bulk form.
/// Total; O(subtree).
pub fn attachments(node: &SyntaxNode) -> impl Iterator<Item = (Comment, Attachment)> {
    let mut out = Vec::new();
    // Per open node, its comments' attachments, resolved once, consumed
    // in token order as the walk meets them: the innermost open node is
    // the token's parent and the front of its queue is that node's next
    // comment in order, so an entry is taken exactly when the walk meets
    // the entry's own token — no second reading of what is a comment.
    let mut open: Vec<VecDeque<(Comment, Attachment)>> = Vec::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(inner)) => {
                open.push(resolve_children(&inner).into_iter().collect());
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => {
                open.pop();
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                if let Some(resolved) = open
                    .last_mut()
                    .and_then(|queue| queue.pop_front_if(|(comment, _)| comment.syntax() == &token))
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
/// elements. Empty when the two abut or `b` starts before `a` ends; a
/// safe, unspecified read of `a`'s own tree when `a` and `b` are from
/// different trees (their offsets are incomparable) — so the facts are
/// total and the walk never panics.
fn text_between(a: &SyntaxElement, b: &SyntaxElement) -> String {
    let root = root_of(a);
    let bound = root.text_range().end();
    let start = a.text_range().end().min(bound);
    let end = b.text_range().start().min(bound);
    if start >= end {
        return String::new();
    }
    // Concatenate the tokens spanning `[start, end)` — O(the tokens there), not
    // the O(subtree) of `root.text().slice(..).to_string()`, whose fold visits
    // every token of the root (rowan's `SyntaxText`). The walk steps forward by
    // `next_token`, never scanning the root's child list; `start`/`end` fall on
    // token boundaries — an element ends and begins on one — so the span is a
    // whole run of tokens, none to clip.
    let mut text = String::new();
    let mut next = token_after(a);
    while let Some(token) = next {
        if token.text_range().start() >= end {
            break;
        }
        text.push_str(token.text());
        next = token.next_token();
    }
    text
}

/// The token immediately after `element` in document order, or `None` past the
/// last token of the tree. An empty node holds no token to step from, so its
/// successor is found by a descent from the root to its end offset — a
/// degenerate left operand, off the O(gap) path, so no real fact call pays it.
fn token_after(element: &SyntaxElement) -> Option<SyntaxToken> {
    match element {
        NodeOrToken::Token(token) => token.next_token(),
        NodeOrToken::Node(node) => match node.last_token() {
            Some(last) => last.next_token(),
            None => root_of(element)
                .token_at_offset(node.text_range().end())
                .right_biased(),
        },
    }
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
    use crate::tree::role_shapes::{doc_block_rule, documented_fact, role_corpus};
    use crate::tree::{AstNode, Direction, SyntaxKind};

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

    /// The first token under `root` whose text is `text`.
    fn token_with_text(root: &SyntaxNode, text: &str) -> SyntaxToken {
        root.descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|token| token.text() == text)
            .unwrap_or_else(|| panic!("a token {text:?}"))
    }

    /// The first node under `root` of `kind`.
    fn node_of_kind(root: &SyntaxNode, kind: SyntaxKind) -> SyntaxNode {
        root.descendants()
            .find(|node| node.kind() == kind)
            .unwrap_or_else(|| panic!("a {kind} node"))
    }

    /// The kinds of `elements`, in order.
    fn kinds(elements: impl Iterator<Item = SyntaxElement>) -> Vec<SyntaxKind> {
        elements.map(|element| element.kind()).collect()
    }

    /// The bulk form read per token — the reading the one-pass walk is
    /// held equal to: every token `Comment::cast` admits, by kind and
    /// `role`, with the single form's attachment, which reads `role`
    /// through `is_skipped` at every neighbor it steps over.
    fn attachments_by_role(root: &SyntaxNode) -> Vec<(Comment, Attachment)> {
        root.descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter_map(Comment::cast)
            .map(|comment| {
                let attached = attachment(comment.syntax()).expect("a trivia comment attaches");
                (comment, attached)
            })
            .collect()
    }

    /// The text, slot, and anchor kind of each of `root`'s attachments.
    fn summarized(root: &SyntaxNode) -> Vec<(String, Slot, SyntaxKind)> {
        attachments(root)
            .map(|(comment, attachment)| {
                (
                    comment.text().to_owned(),
                    attachment.slot,
                    attachment.anchor.kind(),
                )
            })
            .collect()
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
    fn attachments_yields_the_typed_comment() {
        // The bulk form yields the `Comment` wrapper, so a consumer reads
        // the content — the trailing blanks the line rule swallowed,
        // trimmed — off the yield itself, with no cast at the call site.
        let root = parsed("% lead  \np.\n");
        let (comment, attachment) = attachments(&root).next().expect("a comment");
        assert_eq!(comment.content(), "% lead");
        assert_eq!(attachment.slot, Slot::Leading);
    }

    #[test]
    fn comments_yields_the_typed_comment() {
        // The inverse form yields the `Comment` wrapper as the bulk form
        // does, in every slot, so a consumer reads the content — the
        // trailing blanks the line rule swallowed, trimmed — off the
        // yield itself, with no cast at the call site.
        let root = parsed("% lead  \np. % trail  \n\n% dangling  \n");
        let rule = SyntaxElement::Node(node_of_kind(&root, SyntaxKind::RULE));
        let lead = comments(&rule, Slot::Leading)
            .next()
            .expect("a leading comment");
        assert_eq!(lead.content(), "% lead");
        let trail = comments(&rule, Slot::Trailing)
            .next()
            .expect("a trailing comment");
        assert_eq!(trail.content(), "% trail");
        let program = SyntaxElement::Node(root.clone());
        let dangling = comments(&program, Slot::Dangling)
            .next()
            .expect("a dangling comment");
        assert_eq!(dangling.content(), "% dangling");
    }

    #[test]
    fn the_two_forms_agree_and_the_bulk_form_yields_every_comment_once() {
        let root = parsed("% lead\np(1, % after comma\n 2). % trail\n\n% dangling\n");
        let all: Vec<(Comment, Attachment)> = attachments(&root).collect();
        assert_eq!(all.len(), 4);
        for (comment, att) in &all {
            assert_eq!(attachment(comment.syntax()).as_ref(), Ok(att));
            let back: Vec<Comment> = comments(&att.anchor, att.slot).collect();
            assert!(
                back.contains(comment),
                "the inverse form yields {}",
                comment.text()
            );
        }
        let program = SyntaxElement::Node(root.clone());
        let dangling: Vec<String> = comments(&program, Slot::Dangling)
            .map(|comment| comment.text().to_owned())
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
    #[test]
    fn an_empty_left_node_reads_the_text_after_it() {
        // A bare-colon aggregate element — a term then `:` with no condition —
        // holds a zero-width CONDITION node (grammar §5.3); a whitespace fact on
        // it reads the text that follows, here the line break before the
        // aggregate's close, rather than folding to empty (docs/design/syntax.md §9.3).
        let root = parsed(":- #count { a :\n} < 1.\n");
        let empty = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::CONDITION && node.text_range().is_empty())
            .expect("a zero-width condition node");
        let close = root
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|token| token.kind() == SyntaxKind::R_BRACE)
            .expect("the aggregate close");
        let left = SyntaxElement::Node(empty);
        let right = SyntaxElement::Token(close);
        assert_eq!(
            line_breaks_between(&left, &right),
            1,
            "the line break after the empty condition is read, not folded to empty"
        );
        assert!(!same_line(&left, &right));
    }

    #[test]
    fn whitespace_and_comments_are_skipped() {
        // A doc line after the last statement is a stray: trivia by role.
        let root = parsed("p. %* block *% % line\n%! stray\n");
        for text in [" ", "%* block *%", "% line", "%! stray"] {
            let token = token_with_text(&root, text);
            assert!(is_skipped(&SyntaxElement::Token(token)), "{text:?}");
        }
    }

    #[test]
    fn a_node_and_a_significant_token_are_not_skipped() {
        let root = parsed("%! doc\np.\n");
        let rule = node_of_kind(&root, SyntaxKind::RULE);
        assert!(!is_skipped(&SyntaxElement::Node(rule)));
        let p = token_with_text(&root, "p");
        assert!(!is_skipped(&SyntaxElement::Token(p)));
        // A doc line in docs position is structure the statement owns
        // (docs/design/syntax.md §5.4).
        let doc = token_with_text(&root, "%! doc");
        assert!(!is_skipped(&SyntaxElement::Token(doc)));
    }

    #[test]
    fn an_empty_node_is_skipped() {
        // The empty body of `h :- .` and the empty condition of `a :` hold
        // no token to stand at (docs/design/syntax.md §5.4, §9.2).
        let root = parsed("h :- .\n");
        let body = node_of_kind(&root, SyntaxKind::BODY);
        assert!(body.text_range().is_empty());
        assert!(is_skipped(&SyntaxElement::Node(body)));
        let root = parsed(":- #count { a : } < 1.\n");
        let condition = node_of_kind(&root, SyntaxKind::CONDITION);
        assert!(condition.text_range().is_empty());
        assert!(is_skipped(&SyntaxElement::Node(condition)));
    }

    #[test]
    fn significant_children_yields_the_unskipped_children_in_order() {
        // The stray doc line after the first dot opens the second rule's
        // docs, and the plain comment inside that run is trivia there.
        let root = parsed("%! doc\np(1, % c\n 2). %! stray\n\n% lone\nq :- not r.\n");
        assert_eq!(
            kinds(significant_children(&root)),
            [SyntaxKind::RULE, SyntaxKind::RULE]
        );
        let rules: Vec<SyntaxNode> = root.children().collect();
        assert_eq!(
            kinds(significant_children(&rules[0])),
            [
                SyntaxKind::DOC_COMMENT,
                SyntaxKind::LITERAL,
                SyntaxKind::DOT
            ]
        );
        assert_eq!(
            kinds(significant_children(&rules[1])),
            [
                SyntaxKind::DOC_COMMENT,
                SyntaxKind::LITERAL,
                SyntaxKind::NECK,
                SyntaxKind::BODY,
                SyntaxKind::DOT
            ]
        );
        let tuple = node_of_kind(&root, SyntaxKind::TUPLE);
        assert_eq!(
            kinds(significant_children(&tuple)),
            [
                SyntaxKind::CONSTANT_TERM,
                SyntaxKind::COMMA,
                SyntaxKind::CONSTANT_TERM
            ]
        );
    }

    #[test]
    fn significant_children_agrees_with_is_skipped_on_every_node() {
        // The one-pass walk and the per-element predicate are one reading:
        // on every node of the docs-position corpus the walk yields exactly
        // the children `is_skipped` refuses, in order — with a witness that
        // the corpus reaches both answers for a DOC_COMMENT child, the one
        // answer that turns on position.
        let mut doc_lines_kept = 0usize;
        let mut doc_lines_skipped = 0usize;
        for root in role_corpus() {
            for node in root.descendants() {
                let expected: Vec<SyntaxElement> = node
                    .children_with_tokens()
                    .filter(|element| !is_skipped(element))
                    .collect();
                let read: Vec<SyntaxElement> = significant_children(&node).collect();
                assert_eq!(read, expected, "{}", node.kind());
                for element in node.children_with_tokens() {
                    if element.kind() != SyntaxKind::DOC_COMMENT {
                        continue;
                    }
                    if is_skipped(&element) {
                        doc_lines_skipped += 1;
                    } else {
                        doc_lines_kept += 1;
                    }
                }
            }
        }
        assert!(doc_lines_kept > 0 && doc_lines_skipped > 0);
    }

    #[test]
    fn attachments_agrees_with_the_per_token_reading() {
        // On every tree of the docs-position corpus the bulk form yields
        // exactly the comments and attachments the per-token reading
        // yields, in order — with a witness that the admission the role
        // decides is exercised both ways: a DOC_COMMENT admitted as a
        // trivia comment, and one refused as documentation.
        let mut doc_kind_admitted = 0usize;
        let mut doc_kind_refused = 0usize;
        for root in role_corpus() {
            let read: Vec<(Comment, Attachment)> = attachments(&root).collect();
            assert_eq!(read, attachments_by_role(&root), "{}", root.text());
            let admitted = read
                .iter()
                .filter(|(comment, _)| comment.syntax().kind() == SyntaxKind::DOC_COMMENT)
                .count();
            let of_doc_kind = root
                .descendants_with_tokens()
                .filter(|element| element.kind() == SyntaxKind::DOC_COMMENT)
                .count();
            doc_kind_admitted += admitted;
            doc_kind_refused += of_doc_kind - admitted;
        }
        assert!(doc_kind_admitted > 0 && doc_kind_refused > 0);
    }

    #[test]
    fn attachments_of_every_docs_position_shape_at_once() {
        // The hand-built rule holds every shape at once; its attachments,
        // frozen: the plain comment inside the block leads the doc line
        // after it; the doc line after the head trails the head; the one
        // leading a body is trivia there and leads the body's first token;
        // the one after the body trails the body node. And a stray doc
        // line after the last statement dangles in the program.
        let expected = [
            ("% plain", Slot::Leading, SyntaxKind::DOC_COMMENT),
            ("%! after the head", Slot::Trailing, SyntaxKind::IDENT),
            ("%! leading a body", Slot::Leading, SyntaxKind::IDENT),
            ("%! after a child node", Slot::Trailing, SyntaxKind::BODY),
        ]
        .map(|(text, slot, kind)| (text.to_owned(), slot, kind));
        assert_eq!(summarized(&doc_block_rule()), expected);
        let expected = [("%! stray".to_owned(), Slot::Dangling, SyntaxKind::PROGRAM)];
        assert_eq!(summarized(&documented_fact()), expected);
    }

    #[test]
    fn significant_children_yields_strictly_ascending_indices() {
        // The single-pass guard, as far as the output shows it: one read of
        // a wide node's children yields each significant child exactly once,
        // where it stands. The cost law itself is held by the walk's shape —
        // one forward pass, `skipped_children`, the roles carried along —
        // which its comment names for the reader, and by the doc-block
        // tripwires in `tests/scaling_shape.rs`.
        let root = parsed(&"% c\np. % t\n\n".repeat(200));
        let indices: Vec<usize> = significant_children(&root)
            .map(|element| element.index())
            .collect();
        assert_eq!(indices.len(), 200);
        assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(significant_children(&root).all(|element| element.kind() == SyntaxKind::RULE));
    }

    #[test]
    fn non_trivia_sibling_steps_over_trivia_in_either_direction() {
        let root = parsed("% lead\np(1).  % trail\n\nq :- r.\t%! stray\n");
        let rules: Vec<SyntaxNode> = root.children().collect();
        let first = SyntaxElement::Node(rules[0].clone());
        let second = SyntaxElement::Node(rules[1].clone());
        assert_eq!(
            non_trivia_sibling(first.clone(), Direction::Next),
            Some(second.clone())
        );
        assert_eq!(non_trivia_sibling(second, Direction::Prev), Some(first));
        let root = parsed("p(1, % c\n 2).\n");
        let comma = SyntaxElement::Token(token_with_text(&root, ","));
        let two = non_trivia_sibling(comma, Direction::Next).expect("the 2");
        assert_eq!(two.kind(), SyntaxKind::CONSTANT_TERM);
        assert_eq!(two.to_string(), "2");
    }

    #[test]
    fn non_trivia_sibling_is_none_when_only_trivia_remains() {
        let root = parsed("% lead\np(1).  % trail\n\nq :- r.\t%! stray\n");
        let rules: Vec<SyntaxNode> = root.children().collect();
        let first = SyntaxElement::Node(rules[0].clone());
        let second = SyntaxElement::Node(rules[1].clone());
        assert_eq!(non_trivia_sibling(first, Direction::Prev), None);
        assert_eq!(non_trivia_sibling(second, Direction::Next), None);
    }

    #[test]
    fn non_trivia_sibling_steps_over_an_empty_node() {
        let root = parsed("h :- .\n");
        let neck = SyntaxElement::Token(token_with_text(&root, ":-"));
        let dot = SyntaxElement::Token(token_with_text(&root, "."));
        assert_eq!(
            non_trivia_sibling(neck.clone(), Direction::Next),
            Some(dot.clone())
        );
        assert_eq!(non_trivia_sibling(dot, Direction::Prev), Some(neck));
        let root = parsed(":- #count { a : } < 1.\n");
        let colon = SyntaxElement::Token(token_with_text(&root, ":"));
        assert_eq!(non_trivia_sibling(colon, Direction::Next), None);
    }

    #[test]
    fn skip_trivia_token_returns_a_significant_token_as_it_stands() {
        let root = parsed("%! doc\np.\n");
        let p = token_with_text(&root, "p");
        assert_eq!(
            skip_trivia_token(p.clone(), Direction::Next),
            Some(p.clone())
        );
        assert_eq!(skip_trivia_token(p.clone(), Direction::Prev), Some(p));
    }

    #[test]
    fn skip_trivia_token_steps_over_trivia_across_node_bounds() {
        let root = parsed("% lead\np(1).  % trail\n\nq :- r.\t%! stray\n");
        // Forward from the run after the first rule's dot into the second
        // rule's first token; backward from the run before the second rule
        // into the first rule's last.
        let gap = token_with_text(&root, "  ");
        let q = token_with_text(&root, "q");
        assert_eq!(skip_trivia_token(gap, Direction::Next), Some(q));
        let blank = token_with_text(&root, "\n\n");
        let dot = token_with_text(&root, ".");
        assert_eq!(skip_trivia_token(blank, Direction::Prev), Some(dot));
    }

    #[test]
    fn skip_trivia_token_is_none_past_either_end_of_the_tree() {
        let root = parsed("% lead\np(1).  % trail\n\nq :- r.\t%! stray\n");
        let lead = token_with_text(&root, "% lead");
        assert_eq!(skip_trivia_token(lead, Direction::Prev), None);
        let tab = token_with_text(&root, "\t");
        assert_eq!(skip_trivia_token(tab, Direction::Next), None);
    }

    #[test]
    fn skip_trivia_token_reads_a_doc_lines_role_not_its_kind() {
        // In docs position a doc line is significant and stops the walk;
        // stray, it is trivia and is stepped over (docs/design/syntax.md §5.4).
        let root = parsed("%! doc\np.\n");
        let doc = token_with_text(&root, "%! doc");
        let break_after_doc = token_with_text(&root, "\n");
        assert_eq!(
            skip_trivia_token(break_after_doc, Direction::Prev),
            Some(doc)
        );
        let root = parsed("p. %! stray\n");
        let space = token_with_text(&root, " ");
        assert_eq!(skip_trivia_token(space, Direction::Next), None);
    }
}
