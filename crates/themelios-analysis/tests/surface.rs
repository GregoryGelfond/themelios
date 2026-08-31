//! The consolidated analysis surface (docs/design/analysis.md §3): the crate-root
//! facet re-exports and the `prelude` (which globs the program prelude) name the
//! whole reading vocabulary — the analysis facets and the program types the
//! readings return — in one import. A compile-lock, with a run of the analysis
//! doors over a program built through the prelude alone (this file imports no
//! `themelios_program` path).

use themelios_analysis::prelude::*;

#[test]
fn the_prelude_names_analysis_and_program_vocabulary_in_one_import() {
    // Built entirely from names the analysis prelude brought in through the
    // program-prelude glob — no `themelios_program` import appears in this file.
    let program: Program = Program::of([WithProvenance::constructed(Statement::Rule(Rule::fact(
        Atom::constant(Name::new("p").expect("a valid identifier")),
    )))]);

    // The single analysis door and each facet reading, named bare.
    let analysis: Analysis = Analysis::of(&program);
    let _: &Constructs = analysis.constructs();
    let _: &DependencyGraph = analysis.dependencies();
    let _: &Safety = analysis.safety();
    let _: &Classes = analysis.classes();
    assert!(analysis.safety().is_safe());
}

#[test]
fn the_crate_root_names_the_facet_types() {
    // The analysis crate root names its own vocabulary (C-REEXPORT).
    let _: Option<themelios_analysis::Analysis> = None;
    let _: Option<themelios_analysis::Constructs> = None;
    let _: Option<themelios_analysis::DependencyGraph> = None;
    let _: Option<themelios_analysis::Safety> = None;
    let _: Option<themelios_analysis::Classes> = None;
    let _: Option<themelios_analysis::ProgramClass> = None;
}
