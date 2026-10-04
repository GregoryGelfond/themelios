//! An unrelated crate a consumer names `themelios_program` (docs/design/macros.md §11). An
//! expansion rooted at the default `::themelios_program` would resolve here and fail to compile,
//! so a consumer that holds it shows its expansions name the root they select. A test fixture,
//! never published.

/// This crate's own name, so a consumer can show its `themelios_program` is the decoy and not
/// the program tier.
pub const NAME: &str = "fixture-decoy";
