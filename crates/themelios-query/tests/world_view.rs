//! Laws of the live world view and its engine-free snapshot at the public surface
//! (docs/design/query.md §2.3): a world view is non-empty by construction, its
//! materialisation is gated on a search that closed the space, and a materialised
//! snapshot's cautious and brave consequences fold its complete collection.
mod common;

use common::{answer_set, atom, atoms_of, with_faulting_world_view, with_world_view};
use themelios_program::{AnswerSet, Name, Sign, Symbol};
use themelios_solve::contract::{Fault, Locus, Presupposition, Refused};
use themelios_solve::outcome::{Conclusion, Truncation};

/// Whether `fault` refused the request for the presupposition `expected`.
fn refused_for(fault: &Fault, expected: Presupposition) -> bool {
    matches!(fault.refused(), Refused::Request(presupposition) if presupposition == expected)
}

/// The strongly negated atom `-name`.
fn negated(name: &str) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [],
        Sign::Negative,
    )
}

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
fn a_world_view_streams_the_models_the_search_found() {
    with_world_view(
        vec![answer_set(["a"]), answer_set(["b"])],
        Conclusion::Exhausted,
        |mut wv| {
            let streamed: Vec<_> = wv
                .members()
                .map(|item| item.expect("no fault").atoms().clone())
                .collect();
            assert_eq!(streamed, vec![answer_set(["a"]), answer_set(["b"])]);
        },
    );
}

#[test]
fn a_partially_streamed_world_view_refuses_to_materialise_as_taken() {
    // Touching the member stream forfeits completeness, so materialize then refuses:
    // a snapshot cannot be built from only the members that remain after a partial
    // read, and none may pose as complete.
    with_world_view(
        vec![answer_set(["a"]), answer_set(["b"])],
        Conclusion::Exhausted,
        |mut wv| {
            let _first = wv.members().next();
            let refusal = wv
                .materialize()
                .expect_err("a partially-consumed world view cannot materialise");
            assert!(refused_for(&refusal, Presupposition::Taken), "{refusal:?}");
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
            let members = atoms_of(snapshot.members());
            assert_eq!(members, vec![answer_set(["a"]), answer_set(["b"])]);
        },
    );
}

#[test]
fn a_member_holding_an_atom_and_its_contrary_refuses_to_materialise() {
    // No answer set holds `a` and `-a`, so such a member is the backend's contract
    // broken — refused as its bug, never read as a world.
    let inconsistent: AnswerSet = [atom("a"), negated("a")].into_iter().collect();
    with_world_view(vec![inconsistent], Conclusion::Exhausted, |wv| {
        let refusal = wv
            .materialize()
            .expect_err("an inconsistent member is refused");
        assert_eq!(refusal.locus(), Locus::Adapter);
        assert!(refusal.is_backend_bug());
    });
}

#[test]
fn a_member_holding_a_negated_atom_without_its_contrary_materialises() {
    let consistent: AnswerSet = [negated("a"), atom("b")].into_iter().collect();
    with_world_view(vec![consistent], Conclusion::Exhausted, |wv| {
        assert!(wv.materialize().is_ok());
    });
}

#[test]
fn a_budget_cut_world_view_refuses_to_materialise_naming_its_truncation() {
    with_world_view(vec![answer_set(["a"])], Conclusion::Budget, |wv| {
        let refusal = wv
            .materialize()
            .expect_err("a search that did not close the space cannot pose as complete");
        let budget = Presupposition::Unclosed(Truncation::Budget);
        assert!(refused_for(&refusal, budget), "{refusal:?}");
    });
}

#[test]
fn a_member_holding_a_number_refuses_to_materialise() {
    // No answer set holds anything but literals, so such a member is the backend's
    // contract broken — refused as its bug.
    let numbered: AnswerSet = [atom("a"), Symbol::number(1)].into_iter().collect();
    with_world_view(vec![numbered], Conclusion::Exhausted, |wv| {
        let refusal = wv
            .materialize()
            .expect_err("a member holding a number is refused");
        assert_eq!(refusal.locus(), Locus::Adapter);
        assert!(refusal.is_backend_bug());
    });
}

#[test]
fn a_member_holding_no_literal_is_refused_before_its_consistency_is_read() {
    // A member that holds a number and an atom beside its contrary breaks both; the
    // literal check runs first, since consistency reads only function symbols.
    let both: AnswerSet = [atom("a"), negated("a"), Symbol::number(1)]
        .into_iter()
        .collect();
    with_world_view(vec![both], Conclusion::Exhausted, |wv| {
        let refusal = wv.materialize().expect_err("the member is refused");
        assert!(refusal.to_string().contains("literals"), "{refusal}");
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

// ---- The engine-free form crosses a service boundary (solve.md §5.2, §6.1) ----

/// Holds only of plain data: sendable, shareable, cloneable, comparable, and
/// borrowing nothing.
fn plain<T: Send + Sync + Clone + Eq + std::fmt::Debug + 'static>() {}

#[test]
fn a_snapshot_is_plain_data() {
    plain::<themelios_query::Snapshot>();
}

#[test]
fn bindings_are_plain_data() {
    plain::<themelios_query::Bindings>();
}
