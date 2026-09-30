//! The agent's reading facade (docs/design/query.md §2.2, §2.5, §2.6; solve.md
//! §6.2): a `Backend`-owning agent reads its own knowledge base through the
//! query-side `AgentReading` trait — `snapshot`/`answer`/`entails`/`bindings`, and
//! `snapshot_assuming` for their scenario-scoped form — each `&mut self` returning
//! owned values, so the readings compose without a lingering borrow. A reading solves
//! once and materialises; over a program with no decided world view it refuses rather
//! than inventing an answer.

mod common;

use common::{answer_set, atom, atoms_of, with_agent, with_hypothetical_agent};
use themelios_program::program::{Arguments, Atom};
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::Term;
use themelios_query::{AgentReading, Answer, Query};
use themelios_solve::agent::{Assumption, Scenario};
use themelios_solve::contract::{Fault, Locus};
use themelios_solve::outcome::{AnswerSet, Conclusion};

/// The positive ground literal query `name`.
fn lit(name: &str) -> Query {
    Query::of(Atom {
        sign: Sign::Positive,
        name: Name::new(name).expect("a valid identifier"),
        arguments: Arguments::Single(vec![]),
    })
    .expect("a ground literal is a query")
}

/// The unary ground atom `pred(arg)` — an applied member element and a matched
/// instance of a `pred/1` pattern.
fn applied(pred: &str, arg: &str) -> Symbol {
    Symbol::function(
        Name::new(pred).expect("a valid identifier"),
        [Symbol::function(
            Name::new(arg).expect("a valid identifier"),
            [],
            Sign::Positive,
        )],
        Sign::Positive,
    )
}

/// The open pattern `p(X)` with one named variable.
fn var_pattern() -> Atom {
    Atom {
        sign: Sign::Positive,
        name: Name::new("p").expect("a valid identifier"),
        arguments: Arguments::Single(vec![Term::variable(
            VarName::new("X").expect("a valid variable name"),
        )]),
    }
}

#[test]
fn snapshot_materialises_the_world_view() {
    // Two answer sets, search closed: the snapshot holds them as owned data.
    with_agent(
        vec![answer_set(["a"]), answer_set(["a", "b"])],
        Conclusion::Exhausted,
        |agent| {
            let snapshot = agent
                .snapshot()
                .expect("a consistent program has a world view");
            let members = atoms_of(snapshot.members());
            assert_eq!(members, vec![answer_set(["a"]), answer_set(["a", "b"])]);
        },
    );
}

#[test]
fn answer_delegates_the_three_valued_reading() {
    // `a` in every member, so the reading is Yes.
    with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        assert_eq!(agent.answer(&lit("a")).expect("a reading"), Answer::Yes);
    });
}

#[test]
fn entails_delegates_the_cautious_projection() {
    with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        assert!(agent.entails(&lit("a")).expect("a reading"));
    });
}

#[test]
fn bindings_delegates_the_partition() {
    // W = { {p(a)}, {p(a), p(b)} }: p(a) is cautiously entailed, p(b) only brave.
    let members = vec![
        [applied("p", "a")].into_iter().collect(),
        [applied("p", "a"), applied("p", "b")].into_iter().collect(),
    ];
    with_agent(members, Conclusion::Exhausted, |agent| {
        let bindings = agent.bindings(&var_pattern()).expect("a binding pattern");
        let yes: Vec<_> = bindings.yes().cloned().collect();
        assert_eq!(yes, vec![applied("p", "a")], "p(a) is cautiously entailed");
    });
}

#[test]
fn a_reading_over_an_inconsistent_program_refuses() {
    // No answer set, search closed: the reading refuses at the request locus rather
    // than inventing a No.
    with_agent(vec![], Conclusion::Exhausted, |agent| {
        let refusal = agent.answer(&lit("a")).expect_err("no world view to read");
        assert_eq!(refusal.locus(), Locus::Request);
    });
}

#[test]
fn a_reading_over_a_truncated_search_refuses() {
    // The search did not close and witnessed nothing: an inconclusive outcome
    // refuses, carrying why it stopped, never a clean answer.
    with_agent(vec![], Conclusion::Budget, |agent| {
        let refusal = agent
            .snapshot()
            .expect_err("an undecided program has no world view");
        assert!(
            refusal.to_string().contains("budget"),
            "the refusal names why the search stopped",
        );
    });
}

#[test]
fn a_reading_over_a_witnessed_but_unclosed_search_refuses() {
    // A model was witnessed (so the outcome is consistent), but the search stopped at
    // its budget without closing the space: materialize's completeness gate refuses
    // rather than pass a partial world view off as complete — a distinct path from
    // the nothing-witnessed inconclusive arm above.
    with_agent(vec![answer_set(["a"])], Conclusion::Budget, |agent| {
        let refusal = agent
            .snapshot()
            .expect_err("a search that did not close has no complete world view");
        assert_eq!(refusal.locus(), Locus::Request);
    });
}

#[test]
fn bindings_refuses_a_non_binding_pattern_at_the_request_locus() {
    // An anonymous position is a query-owned refusal; the facade surfaces it as a
    // request fault (the locus a non-pattern is asked for lives at, query.md §2.5).
    with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        let anonymous = Atom {
            sign: Sign::Positive,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![
                Term::variable(VarName::new("X").expect("a valid variable name")),
                Term::anonymous(),
            ]),
        };
        let refusal = agent
            .bindings(&anonymous)
            .expect_err("an anonymous position is refused");
        assert_eq!(refusal.locus(), Locus::Request);
    });
}

#[test]
fn bindings_carries_the_non_pattern_reason_into_the_fault() {
    // A non-pattern (an interval names a set) refuses at the request locus, and the
    // facade folds the program tier's specific reason — which term does not denote —
    // into the fault message, not only the refusal's category.
    with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        let interval = Atom {
            sign: Sign::Positive,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![Term::Interval {
                lower: Box::new(Term::Symbolic(Symbol::number(1))),
                upper: Box::new(Term::Symbolic(Symbol::number(3))),
            }]),
        };
        let refusal = agent
            .bindings(&interval)
            .expect_err("an interval is not a pattern");
        assert_eq!(refusal.locus(), Locus::Request);
        assert!(
            refusal.to_string().contains("does not denote"),
            "the facade carries the program tier's reason, not only the category",
        );
    });
}

#[test]
fn owned_readings_compose_without_a_lingering_borrow() {
    // Each reading returns owned values, so a second question after the first
    // compiles — no handle is held across the calls.
    with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        let first = agent.answer(&lit("a")).expect("a reading");
        let snapshot = agent.snapshot().expect("a world view");
        assert_eq!(first, Answer::Yes);
        assert!(snapshot.is_exhausted());
    });
}

// ---- The scenario-scoped snapshot (query.md §2.2, §2.7) ----

/// The assumption fixing the constant `name` to hold (`true`) or not.
fn assume(name: &str, holds: bool) -> Assumption {
    Assumption::new(atom(name), holds).expect("a constant is an atom")
}

/// The fixture world view: `a` holds in two of its three models, and the third,
/// `{d}`, is the one a scenario fixing `a` excludes.
fn three_models() -> Vec<AnswerSet> {
    vec![
        answer_set(["a", "b"]),
        answer_set(["a", "c"]),
        answer_set(["d"]),
    ]
}

/// The scenario fixing `a` to hold: it admits the two models holding `a`.
fn assuming_a() -> Scenario {
    [assume("a", true)].into_iter().collect()
}

#[test]
fn snapshot_assuming_materialises_the_scenario_s_models() {
    with_hypothetical_agent(three_models(), Conclusion::Exhausted, |agent| {
        let snapshot = agent
            .snapshot_assuming(&assuming_a())
            .expect("a scoped world view");
        let members = atoms_of(snapshot.members());
        assert_eq!(members, [answer_set(["a", "b"]), answer_set(["a", "c"])]);
    });
}

#[test]
fn a_scoped_snapshot_carries_its_scenario() {
    with_hypothetical_agent(three_models(), Conclusion::Exhausted, |agent| {
        let snapshot = agent
            .snapshot_assuming(&assuming_a())
            .expect("a scoped world view");
        assert_eq!(snapshot.scenario(), &assuming_a());
    });
}

#[test]
fn a_scoped_snapshot_s_readings_range_over_the_scenario() {
    // `a` holds in every model the scenario admits, so the scoped answer is Yes —
    // where the unscoped world view, whose model {d} lacks `a`, answers Unknown.
    with_hypothetical_agent(three_models(), Conclusion::Exhausted, |agent| {
        let scoped = agent
            .snapshot_assuming(&assuming_a())
            .expect("a scoped world view");
        assert_eq!(scoped.answer(&lit("a")), Answer::Yes);
        assert_eq!(agent.answer(&lit("a")), Ok(Answer::Unknown));
    });
}

#[test]
fn snapshot_assuming_refuses_a_backend_without_assumptions() {
    // The backend declares no `assumptions`, and a scenario needs `solve_assuming`:
    // the scoped snapshot is refused as unsupported, at the request locus.
    with_agent(three_models(), Conclusion::Exhausted, |agent| {
        let refusal = agent
            .snapshot_assuming(&assuming_a())
            .expect_err("no assumptions declared");
        assert_eq!(refusal, Fault::unsupported());
    });
}

#[test]
fn a_scoped_snapshot_with_no_model_under_the_scenario_refuses() {
    // No model holds both `a` and `d`: there is no world view under the scenario to
    // snapshot. The refusal names the scenario, since the program itself has answer
    // sets — only none the scenario admits.
    let impossible: Scenario = [assume("a", true), assume("d", true)].into_iter().collect();
    with_hypothetical_agent(three_models(), Conclusion::Exhausted, |agent| {
        let refusal = agent
            .snapshot_assuming(&impossible)
            .expect_err("no model under the scenario");
        assert_eq!(refusal.locus(), Locus::Request);
        assert!(refusal.to_string().contains("scenario"), "{refusal}");
    });
}

#[test]
fn a_scoped_snapshot_of_an_unclosed_search_refuses() {
    // The scenario's search witnessed its models but stopped at its budget without
    // closing the space: the completeness gate refuses rather than pass a partial
    // scoped world view off as complete, and names why the search stopped.
    with_hypothetical_agent(three_models(), Conclusion::Budget, |agent| {
        let refusal = agent
            .snapshot_assuming(&assuming_a())
            .expect_err("a scoped search that did not close");
        assert!(refusal.to_string().contains("budget"), "{refusal}");
    });
}
