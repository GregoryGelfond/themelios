// An enumeration reports no improving trajectory (docs/design/solve.md §5.3): the
// trajectory is `Optimized`'s alone, so `Solved` — the enumeration's handle —
// offers none, and asking it for one does not compile.
fn trajectory_of(solved: &mut themelios_solve::outcome::Solved<'_>) {
    let _ = solved.trajectory();
}

fn main() {
    let _ = trajectory_of;
}
