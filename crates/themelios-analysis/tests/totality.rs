//! The totality walk (docs/design/analysis.md §9; spec §2 item 8): the mirror of the program
//! tier's failure walk. The program tier has typed refusing doors; **this crate has none** —
//! reading facts off a value never fails, so there is no refusal type, no `Display`, and no
//! table. Its whole failure semantics is *it does not fail*, and this file discharges that
//! obligation: `Analysis::of` and **every accessor** are total — no panic on any program, a
//! recovered or partial one included (program §8), and no accessor returns a failure — because
//! none can.
//!
//! Totality is over the *value*, not a well-formedness precondition: a `Program` raised from a
//! malformed parse can have partial rules, and the analysis walks it and reports the facts it
//! can read (§8). The one thing this crate must never do — assert a strengthening class it has
//! not proven — is a *soundness* obligation the verdict types make structural and the laws of
//! `analysis_laws.rs`/`classify_laws.rs`/`safe_laws.rs` hold; it is not a failure mode, and so
//! not this file's concern. This file proves the surface is total.

use proptest::prelude::*;

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_analysis::analysis::Analysis;
use themelios_program::program::{Atom, IntoHead, Program, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::symbol::Name;
use themelios_program::term::Term;

/// Read **every** accessor of every facet, so that a single call proves the whole read surface
/// total on the given analysis (§3–§7): the construct scan, the dependency graph and its
/// components, safety and finiteness with its witnesses, and the classes with their verdicts.
/// Nothing here can fail — the point is that it all *returns*.
fn exercise_every_accessor(analysis: &Analysis) {
    // §7 — the construct scan: membership, the first bearer, and the whole listing.
    let constructs = analysis.constructs();
    for (construct, statement) in constructs.all() {
        assert!(
            constructs.uses(construct),
            "a construct the scan listed is one it uses",
        );
        assert!(
            constructs.first(construct).is_some(),
            "a listed construct has a first bearer",
        );
        let _borne = statement; // a WithProvenance<Statement> value the caller may keep
    }

    // §4 — the dependency graph and its strongly-connected components.
    let graph = analysis.dependencies();
    let _acyclic = graph.is_acyclic();
    let positive = graph.positive();
    let _positive_acyclic = positive.is_acyclic();
    let _positive_predicates = positive.predicates().count();
    let _positive_components = positive.components().count();
    for predicate in graph.predicates() {
        let _out_degree = graph.edges_from(predicate).count();
        let _component = graph.component_of(predicate);
    }
    for component in graph.components() {
        let _members = component.members().count();
        let _recursive = component.is_recursive();
        let _positive_cycle = component.has_positive_cycle();
        let _negative_cycle = component.has_negative_cycle();
        let _aggregate_cycle = component.has_aggregate_cycle();
    }

    // §5 — safety and grounding finiteness, and the witness a flagged rule carries.
    let safety = analysis.safety();
    let _safe = safety.is_safe();
    let _finite = safety.finiteness();
    for unsafe_rule in safety.unsafe_rules() {
        let _rule = unsafe_rule.rule();
        let _unbound = unsafe_rule.unbound().count();
    }

    // §6 — the program classes and their verdicts.
    let classes = analysis.classes();
    let _tight = classes.tightness();
    let _hcf = classes.head_cycle_free();
    let _stratification = classes.stratification();
    let _normality = classes.normality();
    let _horn = classes.horn();
    let _disjunction = classes.uses_disjunction();
    let _choice = classes.uses_choice();
    assert!(
        classes.confirmed().count() <= 7,
        "a program confirms at most the seven classes",
    );
}

/// Raise a program from concrete syntax (the source admits — the text is bounded).
fn program_of(text: &str) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    raise(&parse(&source, Dialect::Clingo)).program().clone()
}

#[test]
fn the_crate_is_total_across_the_construct_space() {
    // A battery spanning the construct space, each analyzed and read to exhaustion. Rich, valid
    // structure — not garbage — so every accessor is exercised on real facts, not just skipped
    // on an empty graph.
    let battery = [
        "",                           // the empty program
        "a.\n",                       // a lone fact
        "p :- q.\nq :- p.\n",         // a positive cycle (not tight)
        "p :- not q.\nq :- not p.\n", // a negation cycle (not stratified)
        "a | b.\n",                   // a disjunctive head
        "{ a; b } :- c.\nc.\n",       // a choice head
        "reach(X, Y) :- edge(X, Y).\n\
         reach(X, Z) :- reach(X, Y), edge(Y, Z).\n\
         edge(1, 2).\n", // a recursive, safe program
        ":- #count { X : p(X) } > 1.\np(1).\np(2).\n", // a body aggregate
    ];
    for text in battery {
        let analysis = Analysis::of(&program_of(text));
        exercise_every_accessor(&analysis);
    }
}

#[test]
fn the_accessors_return_real_facts_not_merely_avoid_panicking() {
    // A disjunctive head is seen as such; a choice head likewise; and a rule whose head variable
    // has no binding occurrence is flagged unsafe with the offending variable named — the
    // accessors carry information, they do not merely decline to crash.
    let disjunctive = Analysis::of(&program_of("a | b.\n"));
    assert!(disjunctive.classes().uses_disjunction());

    let choice = Analysis::of(&program_of("{ a; b }.\n"));
    assert!(choice.classes().uses_choice());

    let unsafe_program = Analysis::of(&program_of("p(X) :- q(Y).\n"));
    assert!(
        !unsafe_program.safety().is_safe(),
        "p(X) :- q(Y). is unsafe — X and Y have no binding occurrence",
    );
    let flagged = unsafe_program
        .safety()
        .unsafe_rules()
        .next()
        .expect("the unsafe rule is flagged");
    assert!(
        flagged.unbound().count() > 0,
        "the flag names the unbound variables",
    );
}

#[test]
fn the_crate_is_total_on_a_recovered_program() {
    // A `Program` raised from a malformed parse is partial (program §8); the analysis walks it
    // without panic and reports what it can read — totality is over the value, not a
    // well-formedness precondition (§8).
    let malformed = "reachable(a) :- .\n:- not .\np(1 ..).\n";
    let source = Source::new(SourceId::new(0), malformed.to_owned()).expect("the source admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    assert!(
        !raised.diagnostics().is_empty(),
        "the input is genuinely recovered — the raise carried lowering diagnostics",
    );
    let analysis = Analysis::of(raised.program());
    exercise_every_accessor(&analysis);
}

#[test]
fn the_crate_is_total_on_a_constructed_program_with_no_source_span() {
    // A program assembled through the constructors carries no source span, and the analysis of
    // it is as total as that of a parsed one (§8): a witness names its rule by structural value,
    // not a span. A constructed positive cycle, read to exhaustion.
    let name = |text: &str| Name::new(text).expect("a valid identifier");
    let nullary = |text: &str| Atom::new(name(text), Vec::<Term>::new());
    let p_from_q = nullary("p").into_head().when(nullary("q"));
    let q_from_p = nullary("q").into_head().when(nullary("p"));
    let program = Program::of(
        [p_from_q, q_from_p].map(|rule| WithProvenance::constructed(Statement::Rule(rule))),
    );
    let analysis = Analysis::of(&program);
    exercise_every_accessor(&analysis);
}

proptest! {
    /// `Analysis::of` and every accessor are total on the raise of **arbitrary text** — the
    /// adversarial half: whatever the parser recovers from a random byte string, however
    /// partial, the analysis walks it and reads every facet without a panic (spec §2 item 8).
    #[test]
    fn the_crate_is_total_on_the_raise_of_arbitrary_text(text in any::<String>()) {
        if let Ok(source) = Source::new(SourceId::new(0), text) {
            let raised = raise(&parse(&source, Dialect::Clingo));
            let analysis = Analysis::of(raised.program());
            exercise_every_accessor(&analysis);
        }
    }
}
