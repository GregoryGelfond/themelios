// A runtime selection's value is a Rust path to the program tier's crate (docs/design/macros.md
// §9): anything else is a compile error at the first token that breaks the path.
fn main() {
    let _ = themelios_macros::fact!(#![crate = ::themelios_program, extra] p(1));
}
