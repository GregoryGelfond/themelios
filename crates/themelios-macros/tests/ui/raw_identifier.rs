// A raw identifier is a dialect error of the token mapping (docs/design/macros.md §6, §9;
// grammar §9): `r#not` renders with its `r#` prefix, so admitting it would forge a name
// carrying `#` — it names no reserved word, it is only an error. The mapping refuses it and
// the engine re-emits one located compile error at the raw identifier's span, block-wrapped
// so it stands cleanly in expression position (`let _ = fact!(…)`) with no stray secondary
// (§9). Unlike a detached `#`, this refusal is span-independent — `classify_ident` reads the
// identifier's spelling, not a byte range — so it holds under real macro expansion, not only
// the `from_str` fallback (§6; the detached-`#` case is benign under real expansion, so it is
// deliberately not a compile-fail fixture).
fn main() {
    let _ = themelios_macros::fact!(p(r#not));
}
