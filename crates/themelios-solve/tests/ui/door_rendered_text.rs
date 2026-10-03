// The discipline is absolute: never render to text and re-parse across the
// seam (docs/design/solve.md §10.2). The doors are typed — an admitted parse,
// a program — so a rendered program has no door to enter: neither Door A nor
// Door B takes the text, and each refusal is a compile error held beside this
// file.
use themelios_solve::bridge::Door;

fn main() {
    let rendered = String::from("p.");
    let _ = Door::Program(&rendered);
    let _ = Door::Parsed(&rendered);
}
