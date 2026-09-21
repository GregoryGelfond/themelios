// A `#script` body cannot be recovered from Rust tokens (docs/design/macros.md §7): the
// `program!` block refuses it at the macro site with one located compile error, before the
// source is assembled, so the assembled text never carries a `#script` and the parser never
// requests script-body mode (grammar §4.8). The engine's own error stands in expression
// position (`let _ = program!{…}`), block-wrapped, so it is the one clean error — no stray
// "macro expansion ignores token `;`" secondary beside it (§9).
fn main() {
    let _ = themelios_macros::program! { #script (python) 1 #end };
}
