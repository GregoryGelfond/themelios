// atom! reaches head position by assembling a fact and extracting its single head atom
// (docs/design/macros.md §8). A head standing over a body is a rule, not an atom: the head `p`
// is a single atom, but the `:- q` makes it a rule, so `atom!` refuses it with one located
// compile error and never drops the body to build the head atom alone — the value the caller
// did not write. The engine's own error stands in expression position (`let _ = atom!(…)`), so
// it is the one clean error, no stray "macro expansion ignores token `;`" secondary beside
// it (§9).
fn main() {
    let _ = themelios_macros::atom!(p :- q);
}
