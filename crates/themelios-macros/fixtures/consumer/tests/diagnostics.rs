//! A macro's diagnostic through the facade lands on the consumer's own token
//! (docs/design/macros.md §9): the wrapper forwards the tokens unchanged, so an error reads as it
//! would at a direct call. `trybuild` compiles each `tests/ui/*.rs` and holds its stderr against
//! the reviewed `.stderr` beside it.

#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
