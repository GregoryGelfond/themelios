//! The consolidated public surface (docs/design/program.md §2, §14): the
//! crate-root re-exports (Rust API guideline C-REEXPORT) and the `prelude` glob
//! name the working vocabulary without walking the module tree, and the
//! conversion traits land in scope. A compile-lock — dropping a re-export or
//! renaming a path breaks this — over a program built entirely through the
//! newly-named surface.

// The prelude brings the working vocabulary — traits included — into scope by glob.
use themelios_program::prelude::*;

#[test]
fn the_prelude_names_the_working_vocabulary_and_its_traits() {
    // Types reached bare through the prelude glob, with no module path in sight: the
    // bare assembly door over a rule, and the provenance door over a carried node.
    let fact = || Rule::fact(Atom::constant(Name::new("p").expect("a valid identifier")));
    let program: Program = Program::of([fact()]);
    assert_eq!(program.statements().count(), 1);
    let carried: WithProvenance<Statement> = WithProvenance::constructed(Statement::from(fact()));
    assert_eq!(Program::of_nodes([carried]), program);

    // The conversion traits are in scope — the traits-first reason a prelude
    // exists — so `to_symbol`/`from_symbol` resolve as methods with no extra use.
    let number: Symbol = 42_i32.to_symbol();
    assert_eq!(i32::from_symbol(&number), Ok(42));
    assert_eq!("s".to_symbol(), Symbol::String("s".to_owned()));

    // The richer working set — comparisons, aggregates — is present too.
    let _ = Relation::Lt;
    let _ = AggregateFunction::Count;
}

#[test]
fn the_prelude_names_the_doors_their_refusals_and_the_base_seam() {
    // The render doors are flat here — the tier's output door — so a client renders
    // with no module path.
    let program = Program::of([Rule::fact(Atom::constant(
        Name::new("q").expect("a valid identifier"),
    ))]);
    let _: Result<String, Unspellable> = render(&program, Dialect::Clingo);
    let _: Result<String, Unspellable> = render_documented(&program, Dialect::Clingo);

    // The door refusals land in scope without a module path: the pool door, the
    // string-source door, the symbol-conversion door, and the pattern door of `mgu`.
    let _: Option<EmptyPool> = None;
    let _: Option<TooLarge> = None;
    let _: Option<FromSymbolError> = None;
    let _: Option<NotAPattern> = None;

    // The base source seam is flat, and reachable through the `base` module too —
    // the two paths name one type.
    let _: Option<Source> = None;
    let _: Option<SourceId> = None;
    let _: Option<base::source::Source> = None;

    // The raise door's source-level result is nameable here, beside `Raised`.
    let _: Option<RaisedSource> = None;
    let _: Option<Raised> = None;
}

#[test]
fn the_crate_root_names_the_headline_types_and_the_foreign_leaves() {
    // First-guess paths resolve at the crate root (C-REEXPORT), so a client need
    // not walk the module tree or add a base/syntax dependency to name a type this
    // crate hands back.
    let _: Option<themelios_program::Program> = None;
    let _: Option<themelios_program::Atom> = None;
    let _: Option<themelios_program::Symbol> = None;
    let _: Option<themelios_program::Term> = None;
    let _: Option<themelios_program::WithProvenance<themelios_program::Statement>> = None;
    let _: Option<themelios_program::Location> = None; // themelios_base, re-exported here
    let _: themelios_program::Dialect = themelios_program::Dialect::Clingo; // themelios_syntax
}

/// The render doors, the new IR types and door refusals, and the base source seam are named
/// at the crate root too — locked independently of the prelude, since `lib.rs` and
/// `prelude.rs` re-export separately (a drop from either site must fail here).
#[test]
fn the_crate_root_names_the_doors_their_refusals_and_the_base_seam() {
    let _: fn(
        &themelios_program::Program,
        themelios_program::Dialect,
    ) -> Result<String, themelios_program::Unspellable> = themelios_program::render;
    let _: fn(
        &themelios_program::Program,
        themelios_program::Dialect,
    ) -> Result<String, themelios_program::Unspellable> = themelios_program::render_documented;
    let _: Option<themelios_program::BodyElement> = None;
    let _: Option<themelios_program::Comparison> = None;
    let _: Option<themelios_program::Relation> = None;
    let _: Option<themelios_program::FromSymbolError> = None;
    let _: Option<themelios_program::EmptyPool> = None;
    let _: Option<themelios_program::Source> = None;
    let _: Option<themelios_program::SourceId> = None;
    let _: Option<themelios_program::TooLarge> = None;
    let _: Option<themelios_program::base::source::Source> = None; // the `base` module door
}

/// `AnswerSet` is declared at the crate root — not because the program tier itself names it
/// (it does not: `signature_range` returns `RangeInclusive<Symbol>`, and the `range` scan lives
/// in the query tier), but as the declaration point the solve and query tiers re-export
/// (their `pub use themelios_program::AnswerSet`); its audience is the outcome-reader. It is
/// deliberately kept OUT of the prelude — the program author's working vocabulary. Locked here
/// as the `BTreeSet<Symbol>` the pattern surface's `signature_range` scan ranges over.
#[test]
fn the_crate_root_declares_answer_set_as_a_btreeset_of_symbols_for_its_re_exporters() {
    use std::collections::BTreeSet;
    let empty: themelios_program::AnswerSet = BTreeSet::new();
    assert!(empty.is_empty());
    // The alias IS `BTreeSet<Symbol>` — assignable both ways with no conversion.
    let symbols: BTreeSet<themelios_program::Symbol> = empty;
    let _back: themelios_program::AnswerSet = symbols;
}
