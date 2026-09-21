// A lowering diagnostic from a *real* macro expansion lands on the offending Rust token's
// span (docs/design/macros.md §5.3, §9): `9999999999` overflows the engine's `i32` width,
// so the splice-free-view raise reports `NumberOutOfRange` at the numeral, re-emitted as a
// compile error there through the span map. This confirms `span_of` is accurate under real
// expansion on the workspace's stable toolchain — the span map answers from the tile's
// captured `proc_macro` span, not a synthetic byte range, so the error blames the numeral,
// not the call site.
fn main() {
    let _ = themelios_macros::fact!(p(9999999999));
}
