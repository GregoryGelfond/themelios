//! The parser's green-tree builder — cache-free (docs/design/syntax.md
//! §6.8, §14).
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
//! without the cache and keeps §6.8's cost: `finish_node` constructs the
//! node from its children directly, O(its children), hence O(nodes) over
//! the tree. The surface mirrors rowan's `green/builder.rs` — the parser
//! (§5.5) is the only consumer, and `token`/`start_node`/`checkpoint`/
//! `start_node_at`/`finish_node`/`finish` behave exactly as rowan's do.

use rowan::{GreenNode, GreenToken, NodeOrToken, SyntaxKind};

type GreenElement = NodeOrToken<GreenNode, GreenToken>;

/// A marked position in the child sequence, for a later `start_node_at`
/// to wrap the children placed since — the retroactive wrap a precedence
/// climb needs (§6.2). Opaque, as `rowan::Checkpoint` is; only
/// `checkpoint` and `start_node_at` read it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Checkpoint(usize);

/// The parser's builder: the stack of open nodes and the flat run of
/// children accumulated under the innermost, with no intern-cache (this
/// module's note). The same operations rowan's `GreenNodeBuilder` offers,
/// without the interning.
#[derive(Default)]
pub(super) struct GreenBuilder {
    parents: Vec<(SyntaxKind, usize)>,
    children: Vec<GreenElement>,
}

impl GreenBuilder {
    /// A fresh builder with nothing open.
    pub(super) fn new() -> GreenBuilder {
        GreenBuilder::default()
    }

    /// Add a token as a child of the current node.
    pub(super) fn token(&mut self, kind: SyntaxKind, text: &str) {
        self.children
            .push(NodeOrToken::Token(GreenToken::new(kind, text)));
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
