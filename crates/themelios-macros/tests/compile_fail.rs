//! Compile-fail witnesses (docs/design/macros.md §9, §11): a macro diagnoses at the
//! rust-analyzer bar, at the offending Rust token's span. `trybuild` compiles each
//! `tests/ui/*.rs` and holds its stderr against the reviewed `.stderr` beside it — so a
//! non-`ToSymbol` splice is refused at the constructor door naming the `ToSymbol` bound
//! (`splice_not_to_symbol`), and a lowering diagnostic from a real expansion lands on the
//! offending numeral's span, not the call site (`numeral_overflows`, the I2 span check).

#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
