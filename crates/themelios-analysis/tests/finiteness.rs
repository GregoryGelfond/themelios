//! Finiteness verdict soundness (docs/design/analysis.md §5): the growth check proves `Holds` only
//! where grounding is term-depth-bounded, and returns `Unknown` (never a false `Holds`) otherwise.
//! These read the shipped `Safety::finiteness()` on programs raised from concrete syntax — the same
//! `Holds`/`Unknown` a consumer reads — with **no** grounder: the one soundness property the clingo
//! grounder differential structurally cannot witness (clingo makes no static finiteness promise), so it
//! is guarded here by intended-verdict assertions, and cross-checked against clingo's grounding in the
//! feature-gated differential. Runs on every platform in the default test build.

use themelios_analysis::classify::Verdict;
use themelios_analysis::safe::Safety;
use themelios_base::source::{Source, SourceId};
use themelios_program::raise::raise;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

mod common;

/// This tier's grounding-finiteness verdict for a program of concrete syntax. The corpus is ordinary
/// ASP (no theory-atom pool), so the raise is faithful and reports no diagnostic; a row that regresses
/// that assumption trips the assertion rather than reading a truncated program.
fn finiteness(text: &str) -> Verdict {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let parsed = parse(&source, Dialect::Clingo);
    assert!(
        !parsed.has_errors(),
        "finiteness fixture must parse cleanly (no syntax error): {text:?}",
    );
    let lowered = raise(&parsed);
    assert!(
        lowered.diagnostics().iter().next().is_none(),
        "finiteness fixture must raise cleanly (no lossy diagnostic): {text:?}",
    );
    Safety::of(lowered.program()).finiteness().clone()
}

/// This tier's safety reading for a program of concrete syntax — the other half of the composed gate
/// `is_safe() && finiteness() == Holds` a boundary consumer reads (§5). The sibling growth positions the
/// growth check records no former at are each guarded by safety: a program whose only binder for a head
/// variable sits in one is unsafe, so the composed gate refuses it and no false `Holds` escapes.
fn is_safe(text: &str) -> bool {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let parsed = parse(&source, Dialect::Clingo);
    assert!(
        !parsed.has_errors(),
        "safety fixture must parse cleanly (no syntax error): {text:?}",
    );
    let lowered = raise(&parsed);
    assert!(
        lowered.diagnostics().iter().next().is_none(),
        "safety fixture must raise cleanly (no lossy diagnostic): {text:?}",
    );
    Safety::of(lowered.program()).is_safe()
}

fn holds(text: &str) -> bool {
    matches!(finiteness(text), Verdict::Holds)
}

fn unknown(text: &str) -> bool {
    matches!(finiteness(text), Verdict::Unknown { .. })
}

// The triangulation of the term-forming growth positions: three that must be `Unknown` (a term deepens
// the recursion) and one that must be `Holds` (a non-growing recursion). The inverse-descent body atom
// `p(X) :- p(X+1)` is the case the growth check previously missed — its grounding descends p(0)→p(-1)→…
// unbounded — so it must be `Unknown`, exactly as the head-former and body-`=`-former spellings of the
// same act already are.
#[test]
fn a_head_former_over_a_carried_variable_is_unknown() {
    assert!(unknown("p(0).\np(X+1) :- p(X).\n"));
}

#[test]
fn a_body_equality_former_is_unknown() {
    assert!(unknown("p(0).\np(Y) :- p(X), Y = X + 1.\n"));
}

#[test]
fn an_arithmetic_inverse_descent_body_atom_is_unknown() {
    // The fixed defect: `X + 1` in a positive body atom descends the recursion under inversion.
    assert!(unknown("p(0).\np(X) :- p(X + 1).\n"));
}

#[test]
fn a_non_growing_recursion_is_holds() {
    assert!(holds(
        "reach(a, b).\nreach(X, Z) :- reach(X, Y), reach(Y, Z).\n"
    ));
}

// The discriminating cases that pin the `is_former ∧ Arith::Linear` semantics: both conjuncts
// load-bearing, asymmetric with the head on Herbrand, positive-only, and conservative.

#[test]
fn a_herbrand_former_in_a_body_atom_stays_holds() {
    // A Herbrand constructor SHRINKS in the body (matched under `f`), so it is not a growth edge —
    // `is_former` alone would wrongly flag it; the `Arith::Linear` conjunct excludes it.
    assert!(holds("p(a).\np(X) :- p(f(X)).\n"));
}

#[test]
fn a_reachability_carrier_stays_holds() {
    // A plain recursive carrier in a Datalog join stays Holds: the fix records no deepener for a bare
    // carried variable, so ordinary non-growing recursion is untouched. (The `is_former` conjunct — that a
    // bare carried variable is not a deepener — is pinned by `a_non_growing_recursion_is_holds`, whose
    // carried variable is bare in the head.)
    assert!(holds("edge(a, b).\nreach(Y) :- reach(X), edge(X, Y).\n"));
}

#[test]
fn a_contracting_arithmetic_body_former_is_unknown_by_design() {
    // `2*X` is invertible-linear, so it is flagged though it contracts (genuinely bounded): a spurious
    // `Unknown` by design, never a false `Holds` (documented conservatism).
    assert!(unknown("p(0).\np(X) :- p(2 * X).\n"));
}

#[test]
fn a_negated_body_former_does_not_deepen() {
    // The former-deepener is positive-only: a default-negated atom does not record it. `q(X)` binds and
    // carries; `not p(X+1)` neither derives nor deepens `q`. Bounded.
    assert!(holds("q(0).\nq(X) :- q(Y), X = Y, not p(X + 1).\n"));
}

#[test]
fn every_truly_unbounded_template_is_unknown() {
    assert!(!common::SOUNDNESS.is_empty());
    for (label, program) in common::SOUNDNESS {
        assert!(
            unknown(program),
            "[{label}] a truly-unbounded template must be Unknown, never a false Holds: {program:?}",
        );
    }
}

#[test]
fn every_provably_bounded_template_is_holds() {
    assert!(!common::PRECISION.is_empty());
    for (label, program) in common::PRECISION {
        assert!(
            holds(program),
            "[{label}] a provably-bounded template must be Holds (precision is not vacuous): {program:?}",
        );
    }
}

#[test]
fn every_conservatively_refused_template_is_unknown_by_design() {
    assert!(!common::DOCUMENTED_CONSERVATISM.is_empty());
    for (label, program) in common::DOCUMENTED_CONSERVATISM {
        assert!(
            unknown(program),
            "[{label}] a conservatively-refused template is Unknown by design (pinned imprecision): {program:?}",
        );
    }
}

#[test]
fn every_cap_timing_excluded_label_is_a_soundness_row() {
    // A stale or misspelled label would match nothing in the differential's skip, and the row it meant
    // to exclude would then grind to the grounder cross-check's deadline and fail its SOUNDNESS
    // assertion as Inconclusive — caught there, but only after the deadline; caught here for free.
    for excluded in common::CAP_TIMING_EXCLUDED {
        assert!(
            common::SOUNDNESS.iter().any(|(label, _)| label == excluded),
            "CAP_TIMING_EXCLUDED names `{excluded}`, which is not a SOUNDNESS row",
        );
    }
}

// The sibling positions of the body-atom former, each audited against clingo's grounding. Where the
// growth check reads the former (a term pool `unpool` expands, a residual argument-list alternative,
// a residual pooled disjunct's condition, a head-element condition of any kind, an `#external` body)
// the verdict is `Unknown`. Where it does not — a residual pooled condition atom, a term pool wrapping
// the former, a `#min`/`#max` element condition, a body conditional — the position binds nothing under
// this tier's safety, so a program relying on it is **unsafe** and the composed gate refuses it: the
// fail-closed disposition these tests pin, so a safety change that admits one of those positions as a
// binder fails here before it can turn the unread former into a false `Holds`.

#[test]
fn a_term_pool_body_former_is_unknown() {
    // `p((X + 1; X))` is expanded by `unpool` before this reads it, into the grower `p(X) :- p(X + 1)`
    // and the tautology `p(X) :- p(X)`; the grower is the fixed inverse descent, in either alternative
    // order. clingo grounds it unbounded.
    assert!(unknown("p(0).\np(X) :- p((X + 1; X)).\n"));
    assert!(unknown("p(0).\np(X) :- p((X; X + 1)).\n"));
}

#[test]
fn a_former_in_a_residual_argument_list_alternative_is_unknown() {
    // A pooled disjunct literal is the residual `unpool` leaves, its condition left pooled with it. The
    // deepener walks the condition atom's alternatives flattened (`argument_terms`), so the former in
    // either alternative of `t(X + 1; d)` records its self-deepening edge; `s(X)` binds `X` and carries
    // it round the recursion, so the rule is safe and the verdict is `Unknown` — spurious (X ranges over
    // s, a bounded domain), the sound side. The control below shows the alternative's former, and
    // nothing else, is what flags it.
    assert!(unknown(
        "t(0).\nr.\ns(X) :- t(X).\nt(X; a) : s(X), t(X + 1; d) | q :- r.\n"
    ));
    assert!(unknown(
        "t(0).\nr.\ns(X) :- t(X).\nt(X; a) : s(X), t(d; X + 1) | q :- r.\n"
    ));
}

#[test]
fn a_residual_argument_list_alternative_without_a_former_is_holds() {
    assert!(holds(
        "t(0).\nr.\ns(X) :- t(X).\nt(X; a) : s(X), t(X; d) | q :- r.\n"
    ));
}

#[test]
fn a_residual_pooled_disjunct_condition_former_is_unknown() {
    // The residual pooled disjunct `t(X; a)` with an unpooled condition `t(X + 1)`: the condition binds
    // `X` through the former and carries it, so grounding descends t(0)→t(-1)→… unbounded — on the
    // argument: a conditioned head instantiates its condition between rule emissions, so clingo
    // neither completes it nor reaches the differential's rule cap (it runs past the deadline) — the
    // same descent as the disjunction case of
    // `a_head_element_condition_former_is_unknown_in_every_head_kind`. The head-condition lift reads
    // a residual element's condition too.
    assert!(unknown("t(0).\nr.\nt(X; a) : t(X + 1) | q :- r.\n"));
}

#[test]
fn a_residual_pooled_condition_arithmetic_former_is_unsafe() {
    // A residual pooled condition atom binds nothing (`bind_positive_atom` fails closed on a pooled
    // atom, the atom-level twin of a `Term::Pool` being not invertible), so the `X` its former would
    // bind is unbound and the rule is unsafe — even when every alternative would bind it
    // (`t(X + 1; X - 1)`, which clingo accepts and grounds unbounded in both directions). The composed
    // gate refuses it, so the growth the former carries never reaches a trusted `Holds`.
    assert!(!is_safe("t(0).\nr.\nt(X; a) : t(X + 1; d) | q :- r.\n"));
    assert!(!is_safe("t(0).\nr.\nt(X; a) : t(X + 1; X - 1) | q :- r.\n"));
}

#[test]
fn a_residual_term_pool_wrapping_a_body_former_is_unsafe() {
    // `t((X + 1; X - 1))` in the residual condition: a `Term::Pool` position is not invertible, so it
    // binds nothing here — and it is no deepener either (the pool classifies as non-linear), so this
    // program's finiteness reads `Holds`. clingo unpools the term pool per alternative, accepts the
    // program, and grounds it unbounded: the safety guard is the only thing between it and a false
    // `Holds`, so admitting a residual term-pool position as binding must first make the deepener read
    // the pool's alternatives.
    assert!(!is_safe(
        "t(0).\nr.\nt(X; a) : t((X + 1; X - 1)) | q :- r.\n"
    ));
}

#[test]
fn an_aggregate_element_condition_former_is_unsafe() {
    // `X = #min { Y : p(Y + 1) }`: the element condition's former is not a deepener position (the
    // extremum growth read covers only the value term), so this program's finiteness reads `Holds` —
    // and clingo, which binds `X` by aggregate-result assignment, grounds it unbounded (p(0)→p(-1)→…).
    // Under the ASP-Core-2 standard this tier follows an aggregate-result assignment does not bind
    // (the recorded `aggregate-assignment-guard` divergence), so `X` is unbound and the composed gate
    // refuses it: the twin of the `equality-arithmetic-inversion` boundary — admitting aggregate
    // assignment as a binder must first make the element condition a deepener position.
    assert!(!is_safe("p(0).\np(X) :- X = #min { Y : p(Y + 1) }.\n"));
    assert!(!is_safe("p(0).\np(X) :- #min { Y : p(Y + 1) } = X.\n"));
}

#[test]
fn a_body_conditional_former_binding_only_locally_is_unsafe() {
    // A body conditional is growth-inert: it binds only element-locally, so a head variable — global —
    // bound nowhere else is unbound, whether the former sits in the literal (`p(X + 1) : r`) or in the
    // condition (`r : p(X + 1)`); clingo agrees. The position cannot extend a head variable's domain,
    // so no growth reaches a head through it.
    assert!(!is_safe("p(0).\nr.\np(X) :- p(X + 1) : r.\n"));
    assert!(!is_safe("p(0).\nr.\np(X) :- r : p(X + 1).\n"));
}

#[test]
fn a_head_element_condition_former_is_unknown_in_every_head_kind() {
    // The head-condition lift, in each head kind that carries a condition: a choice, a disjunction, and
    // a head aggregate. Each grounds unbounded: the element atom is derivable for every X with p(X + 1),
    // descending from the seed. The choice case clingo confirms through the differential's rule cap (it
    // emits a rule per step); the disjunction and head-aggregate cases instantiate the condition
    // between rule emissions, so the cap never sees them and clingo runs past the differential's
    // deadline without completing — there the claim stands on the argument, not on a cap witness.
    assert!(unknown("p(0).\nr.\n{ p(X) : p(X + 1) } :- r.\n"));
    assert!(unknown("p(0).\nr.\np(X) : p(X + 1) | q :- r.\n"));
    assert!(unknown(
        "p(0).\nr.\n#count { X : p(X) : p(X + 1) } >= 0 :- r.\n"
    ));
}

#[test]
fn an_external_body_arithmetic_former_is_unknown() {
    // The generation graph: `#external p(X) : p(X + 1)` generates p(-1) from p(0), then p(-2), …
    // unbounded (clingo confirms) — the external's pseudo-rule body is read by the same deepener.
    assert!(unknown("p(0).\n#external p(X) : p(X + 1).\n"));
}

#[test]
fn a_body_former_under_a_herbrand_constructor_stays_holds() {
    // Only a top-level linear argument descends: `p(f(X + 1))` is matched under `f`, so the body term
    // is one constructor deeper than the head's bare `X` — Herbrand shrink, whatever the arithmetic
    // beneath it. Bounded (clingo grounds p(f(0)) to p(-1) and stops), and this tier proves it.
    assert!(holds("p(f(0)).\np(X) :- p(f(X + 1)).\n"));
    assert!(holds("p((0, a)).\np(X) :- p((X + 1, a)).\n"));
}
