// A runtime selection opens the invocation, once (docs/design/macros.md §9): a second selection
// is a compile error at its `#`, so an expansion never names two runtimes.
fn main() {
    let _ = themelios_macros::program! {
        #![crate = ::themelios_program]
        #![crate = ::themelios_program]
        p(1).
    };
}
