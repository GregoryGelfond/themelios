// A runtime selection opens the invocation, once (docs/design/macros.md §9): one written after
// the payload is a compile error at its `#`, not a second reading of the root.
fn main() {
    let _ = themelios_macros::program! { p(1). #![crate = ::themelios_program] };
}
