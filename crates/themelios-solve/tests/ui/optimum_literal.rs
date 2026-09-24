// A proven optimum exists only because a solver proved it (docs/design/solve.md
// §5.3): `Optimum` has no public constructor, so a best-found value cannot pose
// as proven. The struct literal is the door a consumer would try first; it is
// shut — the type's field is the crate's own — and the refusal is the compile
// error held beside this file.
fn main() {
    let _ = themelios_solve::outcome::Optimum {};
}
