//! Source-text model, spans, line indexing, and the diagnostics model:
//! the shared vocabulary of *location* and *report* under every tier.
//!
//! Design of record: `docs/design/base.md`. Every public operation's
//! failure semantics and computational cost are stated on the
//! operation and consolidated in base.md §9. This crate does no I/O,
//! holds no global state, and knows nothing about any language.
//!
//! # A worked example
//!
//! ```
//! use themelios_base::diagnostic::{
//!     Diagnostic, DiagnosticId, Label, Severity,
//! };
//! use themelios_base::source::SourceSet;
//! use themelios_base::span::{ByteOffset, Location, Span};
//! use themelios_base::view;
//!
//! let mut catalog = SourceSet::new();
//! let file = catalog.add("demo.lp".into(), "q(X) :- r(X)\n".into())?;
//!
//! const UNEXPECTED: DiagnosticId =
//!     DiagnosticId::new("syntax", "unexpected-token");
//! let diagnostic = Diagnostic::new(
//!     UNEXPECTED,
//!     Severity::Error,
//!     "expected `.` after the rule body".into(),
//!     Label {
//!         location: Location {
//!             source: file,
//!             span: Span::new(ByteOffset::new(8), ByteOffset::new(12))?,
//!         },
//!         message: None,
//!     },
//! )?;
//!
//! // The same value yields every view.
//! let rendered = view::human(&diagnostic, &catalog);
//! assert!(rendered.starts_with("error[syntax::unexpected-token]:"));
//! let payload = view::editor(
//!     &diagnostic,
//!     &catalog,
//!     themelios_base::line::ColumnEncoding::Utf16Units,
//! )?;
//! assert_eq!(payload.code, UNEXPECTED);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod line;
pub mod source;
pub mod span;
pub mod view;
