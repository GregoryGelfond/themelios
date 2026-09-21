// The engine's own error stands cleanly in expression position (docs/design/macros.md
// §9): a dialect error of the token mapping (grammar §9 names no float, so `1.5` is
// refused) is re-emitted as one `compile_error!`, and — like the diagnostics path — it is
// wrapped in a block, so a construction macro in expression position (`let _ = fact!(…)`)
// draws exactly the one real error, never a stray "macro expansion ignores token `;`"
// secondary beside it.
fn main() {
    let _ = themelios_macros::fact!(p(1.5));
}
