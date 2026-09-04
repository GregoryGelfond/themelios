//! The common working vocabulary for parsing and reading source under the
//! one grammar: the parse door and its result, the declared dialect, the
//! tree — its cursors, its kind roster, the role of a token — comment
//! attachment, the fusion oracle, the two certificates, and the typed
//! diagnostics; plus the typed AST and the base tier, each as a module.
//! Glob-import it — `use themelios_syntax::prelude::*;`.
//!
//! It is a superset of the crate-root re-exports. The typed AST is reached
//! through `ast`, never as a flat glob of its type names — `ast::Program`,
//! `ast::Statement`, `ast::Rule`, `ast::Atom` — because those names are the
//! program tier's names too (`themelios_program::Program`, …), and a client
//! that globs both tiers' preludes must meet no ambiguity. The base tier is
//! reached the same way — `base::source::Source`. Advanced surfaces are
//! reached by their module path, not here: the general parse doors,
//! `NestingLimit`, `EntryPoint`, and `with_required_stack` (`parse`); the
//! token sources and the lexer (`token`, `lexer`); and, in `tree`, rowan's
//! `Direction` — the program tier has a `Direction` of its own — and the
//! coordinate seam, whose `size_of` would shadow the standard prelude's. So
//! this prelude stays safe to glob.
//!
//! ```
//! use themelios_syntax::prelude::*;
//!
//! let text = "p(1). q(X) :- p(X).\n".to_owned();
//! let source = base::source::Source::new(base::source::SourceId::new(0), text)?;
//! let parsed: Parse<ast::Program> = parse(&source, Dialect::Clingo);
//! assert!(!parsed.has_errors());
//! assert_eq!(parsed.syntax().kind(), SyntaxKind::PROGRAM);
//! assert_eq!(parsed.tree().statements().count(), 2);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! After the glob, `Program` names nothing; `ast::Program` names the root:
//!
//! ```compile_fail,E0412
//! use themelios_syntax::prelude::*;
//!
//! fn takes(_program: Program) {}
//! ```

pub use crate::ast;
pub use crate::attach::{
    Attachment, NotAttachable, Slot, attachment, attachments, comments, empty_line_between,
    line_breaks_between, same_line,
};
pub use crate::base;
pub use crate::diagnostic::{
    Expected, ExpectedSet, GrammarWord, Hint, MisplacedDoc, Related, RelatedLocus, RestrictedForm,
    Restriction, SourceBreach, StringDefect, SyntaxClass, SyntaxError, SyntaxErrorKind,
};
pub use crate::dialect::Dialect;
pub use crate::equiv::{
    Certificate, Mismatch, Side, canonical_spelling, comment_sequence, equivalent,
    non_whitespace_tokens, token_stream,
};
pub use crate::fusion::{LexContext, Separator, lex_mode_of, separator, separator_between};
pub use crate::parse::{Parse, parse};
pub use crate::token::LexMode;
pub use crate::tree::{
    Asp, AstChildren, AstNode, AstPtr, GreenNode, NodeOrToken, Preorder, PreorderWithTokens,
    SyntaxElement, SyntaxElementChildren, SyntaxKind, SyntaxNode, SyntaxNodeChildren,
    SyntaxNodePtr, SyntaxText, SyntaxToken, TextRange, TextSize, TokenAtOffset, TokenRole,
    WalkEvent, role,
};
