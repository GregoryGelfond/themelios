// A statement family no construction builds (docs/design/macros.md §8): a `#const` in a
// `program!` block is a located compile error at the statement, never a fabricated value. The
// program tier constructs the family; the codegen that would build it is a reserved seam (§12).
fn main() {
    let _ = themelios_macros::program! { #const n = a. };
}
