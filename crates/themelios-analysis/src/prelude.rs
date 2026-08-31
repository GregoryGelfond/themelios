//! The analysis reading vocabulary — the assembled `Analysis` and its facets —
//! plus the whole program vocabulary in one import:
//! `use themelios_analysis::prelude::*;`. It re-exports this crate's facet types
//! and globs the program prelude, so a client that reads programs names both
//! tiers without a second import or a module path.

pub use crate::analysis::Analysis;
pub use crate::classify::{Classes, HornKind, Normality, ProgramClass, Stratification, Verdict};
pub use crate::construct::{Construct, Constructs};
pub use crate::depend::{Component, DependencyGraph};
pub use crate::safe::{Safety, UnsafeStatement};
pub use themelios_program::prelude::*;
