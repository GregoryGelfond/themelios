//! Compile-fail witnesses (docs/design/macros.md §9, §11): a macro diagnoses at the
//! rust-analyzer bar, at the offending Rust token's span — the direct test of law 1 and of
//! the refusals' clean boundary. `trybuild` compiles each `tests/ui/*.rs` and holds its
//! stderr against the reviewed `.stderr` beside it. The consolidated suite covers each
//! refusal family the design names (§11):
//!
//! - a **macro-site syntax error** — an argument list missing its separator, flagged by the
//!   compile-time parse and re-emitted at the offending token (`syntax_error`);
//! - a **dialect error** of the token mapping (§6) — a float literal (`dialect_error`) and a
//!   raw identifier (`raw_identifier`), each refused at the token it names;
//! - a **non-`ToSymbol` splice** refused at the constructor door naming the `ToSymbol` bound,
//!   pointing at the spliced expression (`splice_not_to_symbol`, §7);
//! - a **non-single-atom `atom!` head** — a disjunction (`atom_not_single`) and a rule with a
//!   body (`atom_not_a_rule`), each refused at the offending construct (§8);
//! - a **`#script` in `program!`** refused at the macro site before assembly (`program_script`,
//!   §7);
//! - and a **lowering diagnostic** from a real expansion landing on the offending numeral's
//!   span, not the call site (`numeral_overflows`, the I2 span check).
//!
//! A **detached `#`** is deliberately *not* a compile-fail fixture. Its refusal reads a
//! `proc_macro2` byte range for span adjacency (source.rs `adjacent`), exact only under the
//! `from_str` fallback the unit tests use; under real macro expansion the byte range is
//! inexact, so a detached `# kw` in a payload is read benignly as `#kw` and does not refuse
//! (docs/design/macros.md §6, §9) — a `compile_fail` case for it would not fail to compile.
//! The adjacency-independent `#script` refusal is covered by `program_script`; the general
//! benign reading is held by the engine's own unit tests (engine.rs).

#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
