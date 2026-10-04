//! A consumer that uses the construction macros directly, with the program tier renamed `tp` and
//! selected by path, beside an unrelated crate it names `themelios_program`
//! (docs/design/macros.md §9, §11). Its tests show each expansion resolves the root it selects —
//! the renamed runtime, or the facade's re-export — and never the crate found under the default
//! name. A test fixture, never published.
