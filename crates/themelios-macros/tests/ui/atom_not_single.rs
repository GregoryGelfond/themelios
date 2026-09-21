// atom! reaches head position by assembling a fact and extracting its single head atom
// (docs/design/macros.md §8); a head that is not a single ordinary atom is refused with one
// located compile error at the offending head, not a fabricated atom. `a ; b` is a
// disjunctive head, so the error names "single atom" and "disjunction" — and, the engine's
// own error standing in expression position (`let _ = atom!(…)`), it is the one clean error,
// no stray "macro expansion ignores token `;`" secondary beside it (§9).
fn main() {
    let _ = themelios_macros::atom!(a ; b);
}
