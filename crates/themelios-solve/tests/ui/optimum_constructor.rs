// A proven optimum exists only because a solver proved it (docs/design/solve.md
// §5.3): `Optimum` offers no constructor, so the conventional `new` door a
// consumer would try next is absent — a tripwire against one ever being added.
fn main() {
    let _ = themelios_solve::outcome::Optimum::new();
}
