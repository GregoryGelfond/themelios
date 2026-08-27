//! The statement raise (docs/design/program.md §8): the part-structured program
//! lowering, per-statement resilience with located diagnostics, the positional
//! corners read from the tree, documentation carried as an annotation, and the
//! parsed provenance that rides every raised node (§6.1). Composed with the term
//! raise (raise_term_laws.rs), this is the one text-to-program door (§8).

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::source::{Source, SourceId};
use themelios_base::view::canonical_order;

use themelios_program::program::{
    Aggregate, BodyElement, Const, Head, Literal, LiteralInner, PartKey, Statement,
};
use themelios_program::provenance::Origin;
use themelios_program::raise::{LowerErrorKind, Raised, raise, raise_statement};
use themelios_program::symbol::{Name, Sign};
use themelios_program::term::Term;

use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{NestingLimit, parse, parse_statement};

// ---- harness ----

/// Raise a whole program under the clingo dialect — the file door (§8).
fn raised(text: &str) -> Raised {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise(&parse(&source, Dialect::Clingo))
}

/// Raise one statement fragment — the single-statement door (§8).
fn raised_statement(text: &str) -> (Option<Statement>, Vec<themelios_program::raise::LowerError>) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let lexer = Lexer::new(&source, Dialect::Clingo);
    raise_statement(&parse_statement(&lexer, NestingLimit::DEFAULT))
}

/// The single rule of a program that holds exactly one.
fn only_rule(raised: &Raised) -> themelios_program::program::Rule {
    let rules: Vec<_> = raised
        .program()
        .statements()
        .filter_map(|statement| match statement.get() {
            Statement::Rule(rule) => Some(rule.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rules.len(), 1, "exactly one rule");
    rules.into_iter().next().expect("one rule")
}

/// The single `#const` of a program that holds exactly one.
fn only_const(raised: &Raised) -> Const {
    raised
        .program()
        .statements()
        .find_map(|statement| match statement.get() {
            Statement::Const(constant) => Some(constant.clone()),
            _ => None,
        })
        .expect("a `#const`")
}

/// Whether a statement is a fact over the named predicate.
fn is_fact_named(statement: &Statement, predicate: &str) -> bool {
    if let Statement::Rule(rule) = statement
        && let Head::Literal(Literal {
            inner: LiteralInner::Atom(atom),
            ..
        }) = rule.head().get()
    {
        return rule.is_fact() && atom.get().name.as_str() == predicate;
    }
    false
}

// ---- Per-statement resilience: a recovered statement is skipped (§8) ----

#[test]
fn a_recovered_statement_is_skipped_while_its_neighbors_still_raise() {
    // `#external .` cannot complete — its atom is absent under recovery; the two
    // well-formed facts around it still raise. The editor-class property.
    let raised = raised("p. #external . q.");
    let facts = raised
        .program()
        .statements()
        .filter(|statement| matches!(statement.get(), Statement::Rule(_)))
        .count();
    assert_eq!(facts, 2, "the well-formed facts around the bad one raise");
    assert!(
        !raised.diagnostics().is_empty(),
        "the incomplete statement is diagnosed, not silently dropped"
    );
}

#[test]
fn raise_never_refuses_and_never_panics_on_a_heavily_recovered_program() {
    // A pile of malformed input still yields a `Raised` — a total raise, a
    // diagnostic a value on it, never a refusal or a panic (§8, §15).
    let raised = raised("p(. :- ){ . #const = . 1 2 3 :- .");
    let mut diagnostics: Vec<_> = raised
        .diagnostics()
        .iter()
        .map(ToDiagnostic::to_diagnostic)
        .collect();
    // Every diagnostic lowers to base's normal form and sorts under the shared
    // batch order — total, no panic.
    diagnostics.sort_by(canonical_order);
    let _ = raised.program().statements().count();
}

// ---- The `-p` corner: strong negation is positional (§8, §4.6) ----

#[test]
fn a_leading_minus_on_a_head_atom_is_strong_negation() {
    // `-p.` is a head atom with `Sign::Negative`; the tree already resolved the
    // `-p` ambiguity (a term-position `-` is arithmetic, raise_term_laws.rs).
    let rule = only_rule(&raised("-p."));
    let Head::Literal(Literal {
        inner: LiteralInner::Atom(atom),
        ..
    }) = rule.head().get()
    else {
        panic!("a head atom, got {:?}", rule.head().get());
    };
    assert_eq!(atom.get().sign, Sign::Negative);
}

#[test]
fn a_leading_minus_on_a_body_atom_is_strong_negation() {
    // `q :- -p.` — the body literal is over a strongly-negated atom.
    let rule = only_rule(&raised("q :- -p."));
    let element = rule.body().get().elements().next().expect("a body element");
    let BodyElement::Literal(Literal {
        inner: LiteralInner::Atom(atom),
        ..
    }) = element.get()
    else {
        panic!("a body atom literal, got {:?}", element.get());
    };
    assert_eq!(atom.get().sign, Sign::Negative);
}

// ---- Set-form position: a Choice in a head, a cardinality Aggregate in a body ----

#[test]
fn a_set_form_is_a_choice_in_head_position() {
    // `{ p; q }.` is a head choice — same surface syntax, head value.
    let rule = only_rule(&raised("{ p; q }."));
    assert!(
        matches!(rule.head().get(), Head::Choice(_)),
        "a head set form is a Choice, got {:?}",
        rule.head().get()
    );
}

#[test]
fn a_set_form_is_a_cardinality_aggregate_in_body_position() {
    // `:- 2 { p; q }.` is a body cardinality aggregate — the same surface syntax,
    // a different value by the position the tree records (§4.4).
    let rule = only_rule(&raised(":- 2 { p; q }."));
    let element = rule.body().get().elements().next().expect("a body element");
    assert!(
        matches!(
            element.get(),
            BodyElement::Aggregate {
                aggregate: Aggregate::Set(_),
                ..
            }
        ),
        "a body set form is a cardinality aggregate, got {:?}",
        element.get()
    );
}

// ---- `#const`: carried unevaluated, a non-constant value diagnosed (§4.8, §8) ----

#[test]
fn a_const_value_is_carried_unevaluated() {
    // `#const x = 1 + 2.` carries the operator term, structurally distinct from
    // `#const x = 3.` — the value is never evaluated at the raise (§4.8).
    let plus = only_const(&raised("#const x = 1 + 2."));
    let three = only_const(&raised("#const x = 3."));
    assert_ne!(plus.value, three.value, "1 + 2 is not folded to 3");
    assert!(
        matches!(plus.value, Term::BinaryOperation { .. }),
        "the value is carried as its operator term, got {:?}",
        plus.value
    );
}

#[test]
fn a_const_value_outside_the_constant_term_subset_is_diagnosed() {
    // `#const x = p(X).` holds a variable — outside the constant-term subset
    // (grammar §5.9); the raise diagnoses it at its span rather than evaluating.
    let raised = raised("#const x = p(X).");
    assert!(
        raised
            .diagnostics()
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::NonConstantValue)),
        "a non-constant `#const` value is a lowering diagnostic, got {:?}",
        raised.diagnostics()
    );
}

#[test]
fn a_const_value_may_be_an_external_call() {
    // `#const x = @f(1).` — an `@`-call is inside the constant-term subset (grammar §5.9, §4.8):
    // admitted, carried unevaluated (resolved with a context later, §3.5), never a diagnostic.
    let clean = raised("#const x = @f(1).");
    assert!(
        clean.diagnostics().is_empty(),
        "an `@`-call constant raises cleanly, got {:?}",
        clean.diagnostics(),
    );
    // Nested, and under a function — still admitted, never a NonConstantValue.
    for text in ["#const x = @g(@h(a)).", "#const x = f(@e(1), 2)."] {
        assert!(
            !raised(text)
                .diagnostics()
                .iter()
                .any(|error| matches!(error.kind(), LowerErrorKind::NonConstantValue)),
            "an `@`-call is a constant term: {text:?}",
        );
    }
}

// ---- Documentation and parsed provenance ride the raised node (§6, §8) ----

#[test]
fn leading_doc_comments_become_a_doc_annotation() {
    let raised = raised("%! reachable pairs\np(1, 2).");
    let statement = raised.program().statements().next().expect("a statement");
    let docs: Vec<&str> = statement.provenance().annotations().doc().collect();
    assert_eq!(docs.len(), 1, "the doc block is one annotation");
    assert!(
        docs[0].contains("reachable pairs"),
        "the documentation rides the rule it documents, got {docs:?}"
    );
}

#[test]
fn a_parsed_origin_rides_every_raised_statement() {
    let raised = raised("p(1, 2).");
    let statement = raised.program().statements().next().expect("a statement");
    assert!(
        statement
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Parsed(_))),
        "a program-level report points back at source (§6)"
    );
}

#[test]
fn content_equal_body_atoms_from_distinct_spans_union_their_provenance() {
    // `q :- p(1), p(1).` — the two identical body atoms are content-equal but
    // parsed from distinct spans; they collapse to one whose provenance unions
    // both parsed origins, nothing lost (§6.3). The nested-child union the raise
    // makes reachable end to end.
    let rule = only_rule(&raised("q :- p(1), p(1)."));
    let body = rule.body().get();
    assert_eq!(
        body.elements().count(),
        1,
        "the content-equal atoms collapse to one"
    );
    let element = body.elements().next().expect("the collapsed element");
    let parsed_origins = element
        .provenance()
        .origins()
        .filter(|origin| matches!(origin, Origin::Parsed(_)))
        .count();
    assert!(
        parsed_origins >= 2,
        "both spans' provenance is unioned onto the one element, got {parsed_origins}"
    );
}

// ---- Parts: `#program` lifts into structure (§4.1, §8) ----

#[test]
fn statements_join_the_part_the_program_directive_opens() {
    // `a.` precedes any `#program`, so it joins `base`; `b.` follows
    // `#program step(t)`, so it joins the `step(t)` part — keyed by the spelled
    // formal (§4.1).
    let raised = raised("a. #program step(t). b.");
    assert!(
        raised
            .program()
            .base()
            .statements()
            .any(|statement| is_fact_named(statement.get(), "a")),
        "the pre-`#program` fact is in base"
    );
    let step = PartKey {
        name: Name::new("step").expect("a valid identifier"),
        formals: vec![Name::new("t").expect("a valid identifier")],
    };
    let part = raised
        .program()
        .part(&step)
        .expect("the `step(t)` part is opened");
    assert!(
        part.statements()
            .any(|statement| is_fact_named(statement.get(), "b")),
        "the post-`#program` fact joins `step(t)`"
    );
}

// ---- Totality of the diagnostics: source order (§8) ----

#[test]
fn diagnostics_are_in_source_order() {
    // Two non-constant `#const` values; their diagnostics ride in span order, as
    // `Raised::diagnostics` promises before base's `canonical_order` sorts.
    let raised = raised("#const a = p(X). #const b = q(Y).");
    let starts: Vec<u32> = raised
        .diagnostics()
        .iter()
        .map(|error| error.location().span.start().get())
        .collect();
    assert!(
        starts.windows(2).all(|window| window[0] <= window[1]),
        "diagnostics are in source order, got {starts:?}"
    );
}

// ---- The single-statement door (§8) ----

#[test]
fn raise_statement_lowers_one_statement_or_none() {
    let (statement, errors) = raised_statement("p(1) :- q(1).");
    assert!(errors.is_empty(), "a well-formed statement raises clean");
    assert!(matches!(statement, Some(Statement::Rule(_))));

    let (none, _) = raised_statement("   ");
    assert!(none.is_none(), "no statement under recovery is None");
}
