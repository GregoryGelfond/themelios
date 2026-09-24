// The discipline is absolute: never render to text and re-parse across the
// seam (docs/design/solve.md §10.2). The doors are typed — a parse, a
// program, a source of ground objects — so a rendered program has no door
// to enter: neither Door B nor Door A takes the text, and each refusal is a
// compile error held beside this file.
use themelios_solve::bridge::Door;

fn main() {
    let rendered = String::from("p.");
    let _ = Door::Program(&rendered);
    let _ = Door::Ast(&rendered);
}
