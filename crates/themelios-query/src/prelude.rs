//! The curated names a client of the query tier imports with one `use`:
//! the reading vocabulary — `use themelios_query::prelude::*;`.
//!
//! Flat here are the types a reading hands back: the epistemic [`Answer`], the
//! solve tier's [`Consequences`], the live [`WorldView`] and its engine-free
//! [`Snapshot`]; the bindings join them as they are built. NOT here — and this is
//! the rule that keeps this prelude globbable beside
//! `themelios_program::prelude::*` — is
//! [`Query`](crate::Query): the program prelude is flat with its own `Query`,
//! the ASP-Core-2 `a?` statement, and two globs bringing one spelling for two
//! items would fail with `E0659` at the first bare use. So the query tier's
//! `Query` keeps the literature name and is reached by its module path,
//! `themelios_query::Query`, the estate's standing convention for a surface
//! that shares a spelling. The compile-lock is `tests/cross_prelude.rs`.

pub use crate::{Answer, Consequences, Snapshot, WorldView};
