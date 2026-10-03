//! The optimization register at the public surface (docs/design/solve.md
//! §5.2–§5.4): an `Optimized` run shares `Solved`'s resolution register —
//! `determination`/`into_determination`/`conclusion` — specialised with
//! `optimum`/`trajectory`; a proven `Optimum` has no public constructor, so a
//! best-found cannot pose as proven (§5.3); theory assignments are a distinct
//! typed component of the outcome (§5.4); and per-solve statistics are read
//! through a `Statistics` trait a consumer may implement. Nothing yields an
//! `Optimized` yet — every backend declares the optimization bit false — so
//! the register's laws are signature locks the compiler checks, and the
//! constructor's absence is held by the compile-fail witnesses in `tests/ui`.

use std::fmt::Debug;

use themelios_solve::contract::Fault;
use themelios_solve::outcome::{
    Conclusion, Determination, Incumbent, Measurement, Optimized, Optimum, Statistics,
    TheoryAssignments,
};

// --- the resolution register (§5.2) ---

#[test]
fn optimized_shares_solved_s_resolution_register() {
    // The five methods, each at its ruled signature: the inspecting resolver is
    // a reborrow, so the handle answers the accessors after it; a trajectory
    // step is a `Result`, so a mid-search engine fault surfaces at the step;
    // and the consuming resolver threads the engine borrow.
    fn register(mut optimized: Optimized<'_>) -> Determination<'_> {
        let _: Determination<'_> = optimized.determination();
        let _: Option<Optimum> = optimized.optimum();
        let _: Option<Conclusion> = optimized.conclusion();
        if let Some(trajectory) = optimized.trajectory() {
            for step in trajectory {
                let _: Result<Incumbent, Fault> = step;
            }
        }
        optimized.into_determination()
    }
    let _: fn(Optimized<'_>) -> Determination<'_> = register;
}

#[test]
fn a_trajectory_step_is_an_incumbent_never_an_optimum() {
    // A best-found is typed apart from the proven optimum: a trajectory of
    // incumbents is accepted where one is asked for, and an incumbent reads
    // back its retained model, never an optimum (§5.3).
    fn atoms_read(steps: impl Iterator<Item = Result<Incumbent, Fault>>) -> usize {
        steps
            .filter_map(Result::ok)
            .map(|step| step.model().atoms().len())
            .sum()
    }
    fn read(mut optimized: Optimized<'_>) -> Option<usize> {
        optimized.trajectory().map(atoms_read)
    }
    let _: fn(Optimized<'_>) -> Option<usize> = read;
}

// --- the proven optimum is unconstructible (§5.3) ---

#[test]
fn an_optimum_cannot_be_constructed_outside_its_crate() {
    // The last of the named pathologies, closed structurally: each `tests/ui`
    // witness tries a constructor door and holds the compile error that shuts
    // it.
    trybuild::TestCases::new().compile_fail("tests/ui/optimum_*.rs");
}

// --- theory assignments (§5.4) ---

#[test]
fn theory_assignments_are_owned_plain_data() {
    // A distinct typed component of the outcome travels with it.
    fn plain<T: Send + Sync + Clone + Eq + Debug + Default + 'static>() {}
    plain::<TheoryAssignments>();
}

#[test]
fn a_cloned_theory_assignment_equals_its_original() {
    let assignments = TheoryAssignments::default();
    assert_eq!(assignments.clone(), assignments);
}

#[test]
fn a_theory_assignment_s_debug_view_names_the_type() {
    let rendered = format!("{:?}", TheoryAssignments::default());
    assert!(rendered.contains("TheoryAssignments"), "{rendered}");
}

// --- statistics (§5.4) ---

/// A consumer's statistics source with nothing to report: a `Measurement` is
/// the crate's to build, so a source implemented outside it can only be empty.
struct Silent;

impl Statistics for Silent {
    fn measurements(&self) -> impl Iterator<Item = Measurement> + '_ {
        std::iter::empty()
    }
}

#[test]
fn a_consumer_may_implement_statistics() {
    assert_eq!(Silent.measurements().count(), 0);
}

#[test]
fn a_measurement_is_owned_plain_data() {
    fn plain<T: Send + Sync + Clone + Eq + Debug + 'static>() {}
    plain::<Measurement>();
}
