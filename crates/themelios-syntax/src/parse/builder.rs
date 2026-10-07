//! The parser's green-tree builder — without a node cache, its tokens
//! shared through a direct-mapped one (docs/design/syntax.md §6.8, §14).
//!
//! rowan's own `GreenNodeBuilder` interns finished nodes through a
//! `NodeCache` so that structurally equal subtrees are shared. To place a
//! node in that cache it rehashes on the cache's growth, and its rehash
//! (`green/node_cache.rs`'s `node_hash`) walks the node's *whole subtree*.
//! The cache skips nodes with more than three children, and that "not
//! cached" propagates upward — so a wide tree is cheap — but a deep, narrow
//! tree, where every node has at most three children, is cached to the
//! root, and rehashing the spine costs the sum of its subtree sizes: 1, 2,
//! …, depth — O(depth²). The depth proof (§16) surfaced this as an O(depth²)
//! parse of `(((…)))`-shaped input, breaking §6.8's O(text) and giving a
//! nested-bracket input quadratic parse time.
//!
//! Interning only shares memory; the tree the parser hands out — kinds,
//! tokens, and shape — is identical with or without it, and every relation
//! this tier reads (structural equality, the token stream, positional
//! identity) is structural, not by shared pointer. So the parser builds
//! nodes without the cache and keeps §6.8's cost: `finish_node` constructs
//! the node from its children directly, O(its children), hence O(nodes)
//! over the tree. Tokens, which have no children to rehash, are shared
//! through a cache of fixed slots instead (`GreenBuilder`), at one hash and
//! one comparison each. The surface mirrors rowan's `green/builder.rs` —
//! the parser (§5.5) is the only consumer, and `token`/`start_node`/
//! `checkpoint`/`start_node_at`/`finish_node`/`finish` behave exactly as
//! rowan's do.

use rowan::{GreenNode, GreenToken, NodeOrToken, SyntaxKind};

type GreenElement = NodeOrToken<GreenNode, GreenToken>;

/// A marked position in the child sequence, for a later `start_node_at`
/// to wrap the children placed since — the retroactive wrap a precedence
/// climb needs (§6.2). Opaque, as `rowan::Checkpoint` is; only
/// `checkpoint` and `start_node_at` read it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Checkpoint(usize);

/// The parser's builder: the stack of open nodes and the flat run of
/// children accumulated under the innermost, with no node intern-cache
/// (this module's note). The same operations rowan's `GreenNodeBuilder`
/// offers, without the node interning.
///
/// Tokens are shared instead through a small direct-mapped cache: a slot
/// per hash of kind and text, overwritten on a clash, so a repeated short
/// token — a parenthesis, a comma, a recurring name or numeral — is one
/// allocation the tree shares. Each token costs one hash and one comparison
/// whatever the input, so no text can degrade it (docs/specification.md
/// §12.4); and sharing a token changes nothing a reader sees, by this
/// module's note.
pub(super) struct GreenBuilder {
    parents: Vec<(SyntaxKind, usize)>,
    children: Vec<GreenElement>,
    tokens: Vec<Option<GreenToken>>,
}

/// The token cache's slot count, a power of two between these bounds: a
/// fragment's few slots up to a large file's thousand.
const TOKEN_SLOTS: (usize, usize) = (16, 1024);

/// The text's bytes per slot before the bounds apply: about one slot per
/// token, a token and its trivia running some eight bytes.
const BYTES_PER_SLOT: usize = 8;

/// The longest token the cache holds. A longer one — a comment, a string,
/// a script body — seldom repeats, and is built afresh.
const CACHED_TOKEN_LEN: usize = 16;

impl GreenBuilder {
    /// A fresh builder with nothing open, its token cache sized to a text
    /// of `text_len` bytes.
    pub(super) fn new(text_len: usize) -> GreenBuilder {
        let (fewest, most) = TOKEN_SLOTS;
        let slots = (text_len / BYTES_PER_SLOT)
            .next_power_of_two()
            .clamp(fewest, most);
        GreenBuilder {
            parents: Vec::new(),
            children: Vec::new(),
            tokens: vec![None; slots],
        }
    }

    /// Add a token as a child of the current node — the cached one when a
    /// token of this kind and text holds its slot.
    pub(super) fn token(&mut self, kind: SyntaxKind, text: &str) {
        let token = if text.len() <= CACHED_TOKEN_LEN {
            let slot = slot_of(kind, text, self.tokens.len());
            match self.tokens.get_mut(slot) {
                Some(Some(kept)) if kept.kind() == kind && kept.text() == text => kept.clone(),
                Some(entry) => {
                    let token = GreenToken::new(kind, text);
                    *entry = Some(token.clone());
                    token
                }
                None => GreenToken::new(kind, text),
            }
        } else {
            GreenToken::new(kind, text)
        };
        self.children.push(NodeOrToken::Token(token));
    }

    /// Open a node; the children placed until the matching `finish_node`
    /// become its own.
    pub(super) fn start_node(&mut self, kind: SyntaxKind) {
        let first_child = self.children.len();
        self.parents.push((kind, first_child));
    }

    /// Close the current node, building it directly from its children —
    /// O(its children), so O(nodes) over the whole tree, no subtree walk.
    pub(super) fn finish_node(&mut self) {
        let (kind, first_child) = self.parents.pop().expect("a node is open");
        let node = GreenNode::new(kind, self.children.drain(first_child..));
        self.children.push(NodeOrToken::Node(node));
    }

    /// Mark the current position, to maybe wrap the children placed after
    /// it with `start_node_at`.
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.children.len())
    }

    /// Wrap the children placed since `checkpoint` in a new node and make
    /// it current — rowan's own invariants on a checkpoint still holding.
    pub(super) fn start_node_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        let Checkpoint(checkpoint) = checkpoint;
        assert!(
            checkpoint <= self.children.len(),
            "checkpoint no longer valid, was finish_node called early?"
        );
        if let Some(&(_, first_child)) = self.parents.last() {
            assert!(
                checkpoint >= first_child,
                "checkpoint no longer valid, was an unmatched start_node_at called?"
            );
        }
        self.parents.push((kind, checkpoint));
    }

    /// Finish building: the single remaining child is the root node.
    /// `start_node_at` and `finish_node` must be paired.
    pub(super) fn finish(mut self) -> GreenNode {
        assert_eq!(self.children.len(), 1, "one root node remains");
        match self.children.pop().expect("the root") {
            NodeOrToken::Node(node) => node,
            NodeOrToken::Token(_) => panic!("the root is a node, not a token"),
        }
    }
}

/// A token's slot in a cache of `slots` (a power of two): FNV-1a over its
/// kind and text, masked. The hash decides only how often a token is found
/// again; any slot is correct.
fn slot_of(kind: SyntaxKind, text: &str, slots: usize) -> usize {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in kind.0.to_le_bytes().into_iter().chain(text.bytes()) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash as usize) & slots.wrapping_sub(1)
}

#[cfg(test)]
mod tests {
    use rowan::{GreenTokenData, Language, NodeOrToken};

    use super::{GreenBuilder, slot_of};
    use crate::tree::{Asp, SyntaxKind};

    /// The tokens of a one-node tree built from `(kind, text)` pairs.
    fn built(tokens: &[(SyntaxKind, &str)]) -> rowan::GreenNode {
        let mut builder = GreenBuilder::new(64);
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        for (kind, text) in tokens {
            builder.token(Asp::kind_to_raw(*kind), text);
        }
        builder.finish_node();
        builder.finish()
    }

    fn token_data(root: &rowan::GreenNode) -> Vec<&GreenTokenData> {
        root.children()
            .filter_map(NodeOrToken::into_token)
            .collect()
    }

    #[test]
    fn a_repeated_short_token_is_one_allocation() {
        let root = built(&[(SyntaxKind::L_PAREN, "("), (SyntaxKind::L_PAREN, "(")]);
        let tokens = token_data(&root);
        assert!(std::ptr::eq(tokens[0], tokens[1]));
    }

    #[test]
    fn one_text_under_two_kinds_is_two_tokens() {
        let root = built(&[(SyntaxKind::IDENT, "p"), (SyntaxKind::VARIABLE, "p")]);
        let tokens = token_data(&root);
        assert!(!std::ptr::eq(tokens[0], tokens[1]));
        assert_eq!(
            [tokens[0].kind(), tokens[1].kind()],
            [SyntaxKind::IDENT, SyntaxKind::VARIABLE].map(Asp::kind_to_raw)
        );
    }

    #[test]
    fn a_clash_overwrites_the_slot() {
        // Two short tokens whose hashes share a slot: the second takes it,
        // so the first, repeated after it, is built afresh rather than kept.
        let slots = GreenBuilder::new(64).tokens.len();
        let kind = Asp::kind_to_raw(SyntaxKind::IDENT);
        let names: Vec<String> = (0..).map(|i| format!("n{i}")).take(64).collect();
        let (first, second) = names
            .iter()
            .enumerate()
            .find_map(|(i, a)| {
                names[i + 1..]
                    .iter()
                    .find(|b| slot_of(kind, a, slots) == slot_of(kind, b, slots))
                    .map(|b| (a.as_str(), b.as_str()))
            })
            .expect("among 64 names, two share one of the slots");
        let root = built(&[
            (SyntaxKind::IDENT, first),
            (SyntaxKind::IDENT, second),
            (SyntaxKind::IDENT, first),
        ]);
        let tokens = token_data(&root);
        assert!(!std::ptr::eq(tokens[0], tokens[2]));
        assert_eq!(tokens[0], tokens[2]);
    }

    #[test]
    fn a_long_token_is_built_afresh() {
        // Past the cached length, a token is not looked up: two equal comment
        // tokens are two allocations, equal in content.
        let comment = "% a comment longer than a cached token";
        let root = built(&[
            (SyntaxKind::LINE_COMMENT, comment),
            (SyntaxKind::LINE_COMMENT, comment),
        ]);
        let tokens = token_data(&root);
        assert!(!std::ptr::eq(tokens[0], tokens[1]));
        assert_eq!(tokens[0], tokens[1]);
    }
}
