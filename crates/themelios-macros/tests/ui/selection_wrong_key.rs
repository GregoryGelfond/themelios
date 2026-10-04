// A runtime selection reads `#![crate = path]` (docs/design/macros.md §9): another key is a
// compile error at the key, never a guess at what was meant.
fn main() {
    let _ = themelios_macros::fact!(#![krate = ::themelios_program] p(1));
}
