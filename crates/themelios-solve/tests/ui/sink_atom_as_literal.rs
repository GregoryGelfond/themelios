// The aspif sink's id roles are distinct newtypes (docs/design/solve.md
// §10.3): an atom id is not a literal id, so an `AspifAtom` cannot pass
// where `assume` wants an `AspifLit` — a sign confusion is refused by the
// compile error held beside this file, not found by the engine.
use themelios_solve::bridge::{AspifAtom, AspifSink};

fn assumes_the_atom(sink: &mut dyn AspifSink) {
    let _ = sink.assume(&[AspifAtom(1)]);
}

fn main() {}
