//! Scaling shapes for the analysis (docs/design/analysis.md §10), measured out
//! of band as criterion benchmarks: `Analysis::of` linear in `program + edges`
//! and the component decomposition linear in the graph. Each shape lands with
//! the surface it measures, so this harness fills in as those surfaces do; the
//! test suite asserts the shapes, these benchmarks measure the absolute numbers
//! (docs/specification.md §10.2). Run with `cargo bench`.

fn main() {}
