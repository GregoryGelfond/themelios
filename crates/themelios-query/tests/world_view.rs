//! Laws of the live world view and its engine-free snapshot at the public surface
//! (docs/design/query.md §2.3): a world view is non-empty by construction, its
//! materialisation is gated on a search that closed the space, and a materialised
//! snapshot's cautious and brave consequences fold its complete collection.
mod common;

use common::{answer_set, atom, with_faulting_world_view, with_world_view};
use themelios_solve::contract::Locus;
use themelios_solve::outcome::Conclusion;

#[test]
fn a_world_view_is_non_empty_by_construction() {
    with_world_view(vec![answer_set(["a"])], Conclusion::Exhausted, |mut wv| {
        assert!(
            wv.members().next().is_some(),
            "a world view is built only from a consistent search, so it has a member",
        );
    });
}

#[test]
fn a_fresh_world_view_reports_its_search_not_yet_closed() {
    // is_exhausted reads the conclusion the run has reached; a live handle learns it
    // only once driven to the end, so a fresh world view reports false until drained.
    with_world_view(vec![answer_set(["a"])], Conclusion::Exhausted, |wv| {
        assert!(!wv.is_exhausted());
    });
}

#[test]
fn a_drained_closed_world_view_reports_the_space_closed() {
    with_world_view(vec![answer_set(["a"])], Conclusion::Exhausted, |mut wv| {
        let _drained: Vec<_> = wv.members().collect();
        assert!(
            wv.is_exhausted(),
            "once driven to its end, a closed search reads exhausted",
        );
    });
}

#[test]
fn a_drained_budget_cut_world_view_reports_the_space_open() {
    with_world_view(vec![answer_set(["a"])], Conclusion::Budget, |mut wv| {
        let _drained: Vec<_> = wv.members().collect();
        assert!(
            !wv.is_exhausted(),
            "a budget-cut search reads not-exhausted even when drained",
        );
    });
}

#[test]
fn a_world_view_streams_the_answer_sets_the_search_found() {
    with_world_view(
        vec![answer_set(["a"]), answer_set(["b"])],
        Conclusion::Exhausted,
        |mut wv| {
            let streamed: Vec<_> = wv.members().map(|item| item.expect("no fault")).collect();
            assert_eq!(streamed, vec![answer_set(["a"]), answer_set(["b"])]);
        },
    );
}

#[test]
fn a_partially_streamed_world_view_refuses_to_materialise() {
    // Touching the member stream forfeits completeness, so materialize then refuses:
    // a snapshot cannot be built from only the members that remain after a partial
    // read, and none may pose as complete.
    with_world_view(
        vec![answer_set(["a"]), answer_set(["b"])],
        Conclusion::Exhausted,
        |mut wv| {
            let _first = wv.members().next();
            assert!(
                wv.materialize().is_err(),
                "a partially-consumed world view cannot materialise",
            );
        },
    );
}

#[test]
fn a_faulting_world_view_refuses_to_materialise_carrying_the_fault() {
    // The search witnesses a model, so the world view is consistent; a fault when it
    // is driven further makes materialisation refuse, and the engine fault surfaces
    // as its own cause rather than laundered into a plain request fault.
    with_faulting_world_view(|wv| {
        let fault = wv
            .materialize()
            .expect_err("a search that faults cannot materialise");
        assert_eq!(fault.locus(), Locus::Engine);
    });
}

#[test]
fn a_world_view_reports_the_scenario_it_ranges_over() {
    // The fixtures range over the unscoped program — the empty scenario.
    with_world_view(vec![answer_set(["a"])], Conclusion::Exhausted, |wv| {
        assert_eq!(wv.scenario().assumptions().count(), 0);
    });
}

#[test]
fn a_closed_world_view_materialises_to_its_complete_collection() {
    with_world_view(
        vec![answer_set(["a"]), answer_set(["b"])],
        Conclusion::Exhausted,
        |wv| {
            let snapshot = wv.materialize().expect("a closed search materialises");
            let members: Vec<_> = snapshot.members().cloned().collect();
            assert_eq!(members, vec![answer_set(["a"]), answer_set(["b"])]);
        },
    );
}

#[test]
fn a_budget_cut_world_view_refuses_to_materialise() {
    with_world_view(vec![answer_set(["a"])], Conclusion::Budget, |wv| {
        assert!(
            wv.materialize().is_err(),
            "a search that did not close the space cannot pose as a complete snapshot",
        );
    });
}

#[test]
fn a_materialised_snapshot_folds_its_cautious_consequences() {
    with_world_view(
        vec![answer_set(["a", "b"]), answer_set(["a", "c"])],
        Conclusion::Exhausted,
        |wv| {
            let snapshot = wv.materialize().expect("a closed search materialises");
            let cautious: Vec<_> = snapshot.cautious().symbols().cloned().collect();
            assert_eq!(cautious, vec![atom("a")], "the intersection holds a alone");
        },
    );
}

#[test]
fn a_materialised_snapshot_folds_its_brave_consequences() {
    with_world_view(
        vec![answer_set(["a", "b"]), answer_set(["a", "c"])],
        Conclusion::Exhausted,
        |wv| {
            let snapshot = wv.materialize().expect("a closed search materialises");
            let brave: Vec<_> = snapshot.brave().symbols().cloned().collect();
            assert_eq!(
                brave,
                vec![atom("a"), atom("b"), atom("c")],
                "the union holds every atom",
            );
        },
    );
}

#[test]
fn overlapping_member_reads_do_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/world_view_*.rs");
}
