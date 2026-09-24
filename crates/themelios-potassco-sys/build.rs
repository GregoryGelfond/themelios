//! The build script of the potassco binding crate. Under the default features
//! it does nothing, and a default workspace build links nothing
//! (docs/design/solve.md §2.1, §16); the build of the vendored clingo 5.8.2
//! behind the `clingo` feature and the bindings' regeneration behind `bindgen`
//! are not yet wired here.
fn main() {}
