//! The typed AST (docs/design/syntax.md §8): cheap wrappers over the red
//! cursors, one per node kind, an enum per grammar class, accessors
//! mirroring the productions' slots. This file holds the roots and the
//! wrapper macro; the wrappers, enums, and traits land in `nodes`, the
//! token wrappers in `tokens`.

use crate::tree::{Asp, AstNode, SyntaxKind, SyntaxNode};

/// Declares one wrapper over one node kind: a view (`!Send`) that casts
/// on the kind, derives its equality and hash positionally through the
/// cursor, and offers `syntax()` as the escape to the tree.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident => $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

// Re-exported so the wrapper and token-wrapper files (docs/design/syntax.md
// §8.2, §8.3) invoke the macro by path; those files are its consumers and
// remove this allowance.
#[allow(unused_imports)]
pub(crate) use ast_node;

ast_node! {
    /// Grammar §5.11's `program`: the program entry's root.
    Program => PROGRAM
}

ast_node! {
    /// The statement entry's root: leading trivia, the statement when the
    /// input held one, trailing trivia, and an `ERROR` node when input
    /// remained (docs/design/syntax.md §6.1).
    StatementFragment => STATEMENT_FRAGMENT
}

ast_node! {
    /// The term and term-value entries' root, of the same shape as the
    /// statement fragment's (docs/design/syntax.md §6.1).
    TermFragment => TERM_FRAGMENT
}
