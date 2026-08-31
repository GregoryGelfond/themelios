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
    // Types reached bare through the prelude glob, with no module path in sight.
    let program: Program = Program::of([WithProvenance::constructed(Statement::Rule(Rule::fact(
        Atom::constant(Name::new("p").expect("a valid identifier")),
    )))]);
    assert_eq!(program.statements().count(), 1);

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
