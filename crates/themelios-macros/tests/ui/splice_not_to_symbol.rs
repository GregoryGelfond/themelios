// A splice crosses the conversion pillar through `ToSymbol` (docs/design/macros.md §7):
// the spliced Rust value is taken to a ground `Symbol` at the constructor door, so a
// value whose type is not `ToSymbol` is refused *there*, the error naming the `ToSymbol`
// bound and pointing at the spliced expression (the trait bound is the check — no
// macro-side type test). `f64` is not `ToSymbol`.
fn main() {
    let x: f64 = 1.0;
    let _ = themelios_macros::fact!(p($x));
}
