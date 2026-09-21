// Law 1: a macro-site syntax error reads as the file parser's does, at the rust-analyzer bar
// (docs/design/macros.md §9, §11). `p(1 1)` is an argument list missing its separator — two
// numerals with no comma — so the compile-time parse flags one syntax error, re-emitted as a
// located compile error at the offending token through the span map (§5.3, §6). The engine's
// diagnostics stand in expression position (`let _ = fact!(…)`), block-wrapped, so it is the
// one clean error — no stray "macro expansion ignores token `;`" secondary beside it (§9).
fn main() {
    let _ = themelios_macros::fact!(p(1 1));
}
