//! The cross-prelude no-collision compile-lock (the rule in
//! `themelios_solve::prelude`). A consumer that globs `themelios_solve::prelude::*`
//! beside `themelios_program::prelude::*` and `themelios_syntax::prelude::*` — as
//! a backend author over the program tier does — compiles, and each spelling
//! resolves to one item.
//!
//! `E0659` (an ambiguous glob) is reported where an ambiguous name is *used*, not
//! where it is glob-imported, so this file locks the property one name at a time:
//! it names bare every spelling the solve prelude flattens, and the program
//! spellings nearest them, so an edit flat-globbing a colliding spelling into
//! either prelude fails this build.

use themelios_program::prelude::*;
use themelios_solve::prelude::*;
use themelios_syntax::prelude::*;

#[test]
fn the_solve_prelude_coexists_with_the_program_and_syntax_preludes() {
    // The generic agent and the traits resolve bare, as bounds.
    fn _agent<B: Backend>(_: &Agent<B>) {}
    fn _facts<F: Facts>(_: &F) {}
    fn _run<R: Run>(_: &R) {}
    fn _cancel<C: Cancel>(_: &C) {}

    // The agent's loop.
    let _: Option<Scenario> = None;
    let _: Option<Assumption> = None;
    let _: Option<SolveOptions> = None;
    let _: Option<StatementId> = None;
    let _: Option<Observation> = None;
    let _: Option<RetractionClass> = None;
    let _: Option<Interrupt> = None;

    // The outcome a question hands back.
    let _: Option<Determination<'static>> = None;
    let _: Option<Conclusion> = None;
    let _: Option<Solved<'static>> = None;
    let _: Option<Optimized<'static>> = None;
    let _: Option<Incumbent> = None;
    let _: Option<Models<'static>> = None;
    let _: Option<Model> = None;
    let _: Option<AnswerSet> = None;
    let _: Option<ShowRule> = None;
    let _: Option<Shown<'static>> = None;
    let _: Option<Partial> = None;
    let _: Option<Stopped<'static>> = None;
    let _: Option<Truncation> = None;
    let _: Option<Unsat> = None;
    let _: Option<Refutation> = None;
    let _: Option<NotExhausted> = None;
    let _: Option<Consequences> = None;
    let _: Option<Mode> = None;
    let _: Option<Fault> = None;
    let _: Option<Locus> = None;
    let _: Option<Refused<'static>> = None;
    let _: Option<Presupposition> = None;

    // The contract a backend implements.
    let _: Option<Capabilities> = None;
    let _: Option<Capability> = None;
    let _: Option<ConsequenceSupport> = None;
    let _: Option<SolveRequest> = None;
    let _: Option<OptimizeRequest> = None;
    let _: Option<ConsequenceRequest> = None;
    let _: Option<GroundOptions> = None;
    let _: Option<TruthValue> = None;
    let _: Option<Door<'static>> = None;
    let _: Option<Admitted> = None;
    let _: Option<NotAdmitted> = None;
    let _: Option<GroundProgram> = None;
    let _: Option<GroundRule> = None;
    let _: Option<Grain> = None;
    let _: Option<NativeAnswer> = None;

    // The program spellings nearest them still resolve to the program tier's
    // items: its `Optimize` and `External` statements, its `Part`, `Symbol`,
    // and `Query`.
    let _: Option<Program> = None;
    let _: Option<Optimize> = None;
    let _: Option<External> = None;
    let _: Option<Part> = None;
    let _: Option<Symbol> = None;
    let _: Option<Query> = None;

    // The syntax tier's parse resolves bare beside both, its AST through `ast::`.
    let _: Option<Parse<ast::Program>> = None;
}

#[test]
fn answer_set_is_one_item_under_two_paths() {
    // The solve prelude's `AnswerSet` is the program tier's own, re-exported.
    let _: Option<themelios_program::AnswerSet> = None::<AnswerSet>;
}
