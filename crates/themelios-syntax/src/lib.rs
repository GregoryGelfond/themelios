//! The syntax tier: a total lexer with the fusion oracle beside it; a
//! hand-written, error-resilient parser producing a lossless tree of
//! the one grammar under a declared dialect; comment attachment as
//! owned, exposed policy; a typed AST over the tree; the tier's own
//! typed diagnostics, lowering to the base model; and token-stream
//! equivalence, the certificate a layout-only or spelling-preserving
//! transformation claims.
//!
//! Design of record: `docs/design/syntax.md`; the grammar it is held to:
//! `docs/grammar.md`. Every public operation's failure semantics and
//! computational cost are stated on the operation and consolidated in
//! syntax.md §13. A parse is a pure function of its inputs; this crate
//! does no I/O, holds no global state, and hands out no structure whose
//! depth is proportional to the input's nesting.
//!
//! # A worked example
//!
//! ```
//! use themelios_syntax::ast::{Head, LiteralInner, Statement};
//! use themelios_syntax::attach::{attachments, Slot};
//! use themelios_syntax::base::source::{Source, SourceId};
//! use themelios_syntax::dialect::Dialect;
//! use themelios_syntax::equiv::{equivalent, Certificate};
//! use themelios_syntax::parse::parse;
//!
//! let text = "% a fact\np(1). q(X) :- p(X).\n";
//! let source = Source::new(SourceId::new(0), text.to_owned())?;
//! let parsed = parse(&source, Dialect::Clingo);
//! assert!(!parsed.has_errors());
//! assert_eq!(parsed.syntax().text(), text);
//!
//! // The typed AST over the tree.
//! let Some(Statement::Rule(fact)) = parsed.tree().statements().next() else { unreachable!() };
//! let Some(Head::Literal(head)) = fact.head() else { unreachable!() };
//! assert!(matches!(head.inner(), Some(LiteralInner::Atom(_))));
//!
//! // Attachment as API: the comment leads the fact.
//! let (comment, attachment) = attachments(&parsed.syntax()).next().expect("a comment");
//! assert_eq!(comment.text(), "% a fact");
//! assert_eq!(attachment.slot, Slot::Leading);
//!
//! // The certificate a layout-only change earns.
//! let respaced = Source::new(SourceId::new(1), "% a fact\np(1).\nq(X):-p(X).\n".to_owned())?;
//! let again = parse(&respaced, Dialect::Clingo);
//! assert_eq!(equivalent(&parsed, &again, Certificate::LayoutOnly), Ok(()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

// The base tier, whole, under one name: the vocabulary every door here
// speaks — Source, ByteOffset, Span, Location, Severity, Diagnostic, the
// line index, the views — is reachable through this crate alone
// (docs/design/syntax.md §1).
pub use themelios_base as base;

pub mod dialect;
pub mod tree;
pub mod token;
pub mod lexer;
pub mod parse;
pub mod diagnostic;
pub mod ast;
pub mod attach;
pub mod fusion;
pub mod equiv;
