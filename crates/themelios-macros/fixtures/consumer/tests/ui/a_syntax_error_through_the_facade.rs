// A syntax error inside a facade invocation (docs/design/macros.md §9): the wrapper forwards the
// consumer's tokens unchanged, so the compile error lands on the consumer's offending token.
fn main() {
    let _ = z::fact!(p(1 2));
}
