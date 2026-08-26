//! The *round-trip* witness (docs/design/program.md §10, §16; spec §3, §7.6): a program is
//! rendered to concrete syntax, the syntax is parsed and raised, and the result is the same
//! program — render, parse, and raise return the same value up to provenance. Run it with
//! `cargo run --example round_trip`.
//!
//! The canonical form is what makes this hold by construction: binary operators and intervals
//! render fully parenthesized, so the tree's grouping is carried in the text with no
//! precedence to re-derive on the way back (§10). The same law, over generated programs and
//! the authority's vendored corpus, is asserted by the test suite (tests/round_trip_laws.rs).

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::Program;
use themelios_program::raise::raise;
use themelios_program::render::render;

fn main() {
    // Raise a two-rule reachability program from concrete syntax, so every node carries the
    // span it was parsed from and the terms hold arithmetic and an interval.
    let text = "reachable(a).\n\
                reachable(Y) :- reachable(X), edge(X, Y).\n\
                step(N + 1) :- step(N), N < 10.\n\
                cell(1 .. 3).\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    let program = raise(&parse(&source, Dialect::Clingo)).program().clone();

    // Render the program to canonical concrete syntax.
    let rendered = render(&program, Dialect::Clingo).expect("the program renders");
    println!("{rendered}");

    // Parse and raise the rendering; it is the same program.
    let reparsed_source = Source::new(SourceId::new(0), rendered).expect("the rendering admits");
    let reparsed: Program = raise(&parse(&reparsed_source, Dialect::Clingo))
        .program()
        .clone();

    assert_eq!(
        program, reparsed,
        "render, parse, and raise return the same program up to provenance",
    );
    println!("the round-trip holds: render, parse, and raise return the same program");
}
