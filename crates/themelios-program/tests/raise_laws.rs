//! The statement raise (docs/design/program.md §8): the part-structured program
//! lowering, per-statement resilience with located diagnostics, the positional
//! corners read from the tree, documentation carried as an annotation, and the
//! parsed provenance that rides every raised node (§6.1). Composed with the term
//! raise (raise_term_laws.rs), this is the one text-to-program door (§8).

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::source::{Source, SourceId};
use themelios_base::view::canonical_order;

use themelios_program::program::{
    Aggregate, Atom, Body, BodyElement, Const, External, HasGuards, Head, Literal, LiteralInner,
    PartKey, Project, Show, Statement,
};
use themelios_program::provenance::Origin;
use themelios_program::raise::{
    LowerErrorKind, Occurrences, Raised, raise, raise_occurrences, raise_statement,
};
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

/// Raise the occurrence stream of a whole program under the clingo dialect (§8).
fn raised_occurrences(text: &str) -> Occurrences {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise_occurrences(&parse(&source, Dialect::Clingo))
}

/// The number of `Parsed` origins on a rule's choice-head boolean elements — the source
/// occurrence count the merge would shed (§8, §6.3).
fn boolean_origin_counts(statement: &Statement) -> Vec<usize> {
    let Statement::Rule(rule) = statement else {
        return Vec::new();
    };
    let Head::Choice(choice) = rule.head().get() else {
        return Vec::new();
    };
    choice
        .elements()
        .filter(|e| {
            matches!(
                e.get().literal().inner,
                LiteralInner::True | LiteralInner::False
            )
        })
        .map(|e| {
            e.provenance()
                .origins()
                .filter(|o| matches!(o, Origin::Parsed(_)))
                .count()
        })
        .collect()
}

/// The part key of each occurrence, name first then spelled formals — the active
/// `#program` part riding each raised statement (§4.1, §8).
fn part_names(occ: &Occurrences) -> Vec<Vec<String>> {
    occ.occurrences()
        .iter()
        .map(|o| {
            let key = o.part();
            std::iter::once(key.name.as_str().to_owned())
                .chain(key.formals.iter().map(|f| f.as_str().to_owned()))
                .collect()
        })
        .collect()
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

// ---- Directive components carry their own parsed provenance (§6.1) ----

/// The single `#external` of a program that holds exactly one.
fn only_external(raised: &Raised) -> External {
    raised
        .program()
        .statements()
        .find_map(|statement| match statement.get() {
            Statement::External(external) => Some(external.clone()),
            _ => None,
        })
        .expect("an `#external`")
}

#[test]
fn a_parsed_directive_s_atom_and_body_carry_their_own_span() {
    // §6.1: provenance rides every structural node down to the atom — a directive's
    // atom and body, not only a rule's head and body.
    let raised = raised("#external p : q.");
    let external = only_external(&raised);
    assert!(
        external
            .atom()
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Parsed(_))),
        "the external's atom carries its parsed span (§6.1)"
    );
    assert!(
        external
            .body()
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Parsed(_))),
        "the external's body carries its parsed span (§6.1)"
    );
}

#[test]
fn a_constructed_directive_s_atom_and_body_carry_a_constructed_origin() {
    // §6.2: a directive built through the construction surface carries a `Constructed`
    // origin on its atom and body — the parsed twin above carries a `Parsed` one.
    let external = External::new(
        Atom::constant(Name::new("p").expect("a valid identifier")),
        Body::empty(),
        None,
    );
    assert!(
        external
            .atom()
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Constructed)),
        "a constructed external's atom carries a Constructed origin (§6.2)"
    );
    assert!(
        external
            .body()
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Constructed)),
        "a constructed external's body carries a Constructed origin (§6.2)"
    );
}

#[test]
fn a_parsed_guard_carries_its_own_span() {
    // §6.1: a guard is a structural node — a relation and a bound — so it carries its own
    // parsed span, as a comparison does.
    let raised = raised(":- #count { X : q(X) } > 3.");
    let rule = only_rule(&raised);
    let guard = rule
        .body()
        .get()
        .elements()
        .find_map(|element| match element.get() {
            BodyElement::Aggregate {
                aggregate: Aggregate::Function(aggregate),
                ..
            } => aggregate.right_guard(),
            _ => None,
        })
        .expect("an aggregate right guard");
    assert!(
        guard
            .provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Parsed(_))),
        "the guard carries its parsed span (§6.1)"
    );
}

#[test]
fn the_show_and_project_body_forms_construct_without_carrier_ceremony() {
    // The declarative constructors hide the provenance carrier, as the struct directives'
    // `new` does (§6.2): a hand-built `#show t : body` / `#project a : body` takes bare
    // content and stamps a `Constructed` origin.
    let Show::TermBody { body, .. } = Show::term_body(1, Body::empty()) else {
        panic!("a term-body show");
    };
    assert!(
        body.provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Constructed)),
        "the show body carries a Constructed origin (§6.2)"
    );
    let Project::Atom { atom, .. } = Project::atom_body(
        Atom::constant(Name::new("p").expect("a valid identifier")),
        Body::empty(),
    ) else {
        panic!("an atom-body project");
    };
    assert!(
        atom.provenance()
            .origins()
            .any(|origin| matches!(origin, Origin::Constructed)),
        "the project atom carries a Constructed origin (§6.2)"
    );
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

#[test]
fn an_absent_weak_constraint_weight_is_diagnosed_not_defaulted() {
    // Grammar §5.7 makes the weight mandatory; a bracket the parser recovered with no
    // weight term (`[@2]` — a priority, no weight) is a recovery hole, diagnosed with an
    // `IncompleteTerm` beside a placeholder, exactly as every other absent required term
    // is (§8) — never silently defaulted to `1`, which would repair a recovered value out
    // of sight (§2, §5.2).
    let raised = raised(":~ a. [@2]");
    assert!(
        raised
            .diagnostics()
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::IncompleteTerm)),
        "an absent weak-constraint weight is diagnosed as incomplete: {:?}",
        raised.diagnostics(),
    );
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
    // A variable, a pool, or an interval is outside the constant-term subset (grammar §5.9,
    // §4.8); the raise diagnoses each at its span rather than evaluating. A pool or interval
    // also draws a syntax error — the parser restricts a constant term — but the raise diagnoses
    // its recovered term all the same, so re-admitting either would surface here.
    for text in [
        "#const x = p(X).",
        "#const x = 1 .. 3.",
        "#const x = (a; b).",
    ] {
        let raised = raised(text);
        assert!(
            raised
                .diagnostics()
                .iter()
                .any(|error| matches!(error.kind(), LowerErrorKind::NonConstantValue)),
            "a non-constant `#const` value is a lowering diagnostic: {text:?} -> {:?}",
            raised.diagnostics(),
        );
    }
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

#[test]
fn an_ordinary_atom_argument_list_pool_is_represented_not_diagnosed() {
    // `p(a; b)` is an atom-level pool the value now carries faithfully (§8): the raise draws no
    // diagnostic, and the unpool pass (§9) expands it into the distinct atoms the grounder does —
    // in head, body, directive, aggregate, choice, and conditional position alike. A pooled *term*
    // (`p((a; b))`, `q(f(a; b))`), a plain list (`p(a, b)`), and a tuple term were never diagnosed.
    for text in [
        "p(a; b).",                     // a head fact
        "q(X) :- p(X; a).",             // a body atom
        "p(X; f(X)) :- p(X).",          // a head atom
        "#external p(a; X) : q.",       // a directive atom
        "#project p(a; b).",            // another directive atom
        "q :- #count { X : p(X; a) }.", // an aggregate element's condition atom
        "{ p(X; a) } :- q.",            // a choice head element atom
        "t :- p(X; a) : r.",            // a conditional literal's atom
        "p((a; b)).",                   // one parenthesized Pool term argument
        "q(f(a; b)).",                  // a pooled function term inside one argument
        "p(a, b).",                     // a plain, comma-separated argument list
        ":~ q. [1@1, p(a; b)]",         // a weak-constraint tuple term (a Pool term, not an atom)
    ] {
        let raised = raised(text);
        assert!(
            !raised
                .diagnostics()
                .iter()
                .any(|error| matches!(error.kind(), LowerErrorKind::PooledArgumentList)),
            "an ordinary atom pool is represented, not diagnosed: {text:?} -> {:?}",
            raised.diagnostics(),
        );
    }
}

#[test]
fn a_theory_atom_argument_list_pool_stays_diagnosed() {
    // A theory-atom argument-list pool (`&sum(a; b)`) is deferred (§7): the raise still reads the
    // first alternative and marks the rest, so the fully-unpooled analysis gate fails closed on it.
    let raised = raised(":- &sum(a; b) { x }.");
    assert!(
        raised
            .diagnostics()
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::PooledArgumentList)),
        "a theory-atom pool stays diagnosed (deferred): {:?}",
        raised.diagnostics(),
    );
}

#[test]
fn an_empty_absolute_is_diagnosed_not_silently_pooled() {
    // `q(||)` — an empty absolute value — is not a term (clingo rejects it; the grammar admits
    // none). A recovered parse can reach the raise; it emits a diagnostic and a placeholder, so no
    // zero-alternative pool escapes to silently delete the statement at `unpool` (§8).
    let raised = raised("p :- q(||).");
    assert!(
        raised
            .diagnostics()
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::IncompleteTerm)),
        "an empty `||` is diagnosed, not silently pooled: {:?}",
        raised.diagnostics(),
    );
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

// ---- The occurrence stream: the lowering half before the set (§8) ----

#[test]
fn occurrences_are_one_per_source_statement_in_source_order() {
    let occ = raised_occurrences("a. b. c.");
    assert_eq!(
        occ.occurrences().len(),
        3,
        "one occurrence per source statement"
    );
    let spans: Vec<_> = occ
        .occurrences()
        .iter()
        .map(|o| o.location().span)
        .collect();
    let mut sorted = spans.clone();
    sorted.sort();
    assert_eq!(spans, sorted, "occurrences are in source order");
}

#[test]
fn occurrences_preserve_per_rule_boolean_element_counts_the_merge_sheds() {
    // 1{#true}1. counts one occurrence; 1{#true;#true}1. counts two (§8). The merged
    // Program holds one content; the occurrence stream keeps both counts.
    let text = "1{#true}1.\n1{#true;#true}1.";
    let occ = raised_occurrences(text);
    let counts: Vec<Vec<usize>> = occ
        .occurrences()
        .iter()
        .map(|o| boolean_origin_counts(o.statement().get()))
        .collect();
    assert_eq!(
        counts,
        vec![vec![1], vec![2]],
        "each source rule keeps its own count"
    );

    // The merged program collapses them to one content-equal statement.
    let merged = raised(text);
    assert_eq!(
        merged.program().statements().count(),
        1,
        "the set merges the two content-equal rules to one"
    );
}

#[test]
fn reversing_the_source_reverses_the_occurrences() {
    let forward = raised_occurrences("1{#true}1.\n1{#true;#true}1.");
    let reverse = raised_occurrences("1{#true;#true}1.\n1{#true}1.");
    let f: Vec<Vec<usize>> = forward
        .occurrences()
        .iter()
        .map(|o| boolean_origin_counts(o.statement().get()))
        .collect();
    let r: Vec<Vec<usize>> = reverse
        .occurrences()
        .iter()
        .map(|o| boolean_origin_counts(o.statement().get()))
        .collect();
    assert_eq!(f, vec![vec![1], vec![2]]);
    assert_eq!(
        r,
        vec![vec![2], vec![1]],
        "reversed source reverses occurrence order"
    );
}

#[test]
fn a_malformed_statement_is_skipped_and_its_neighbors_still_occur() {
    // The middle directive cannot complete — its atom is absent under recovery; it is
    // skipped and diagnosed in the batch, while its rule neighbors still occur.
    let occ = raised_occurrences("a.\n#external .\nb.");
    let names: Vec<_> = occ
        .occurrences()
        .iter()
        .filter_map(|o| match o.statement().get() {
            Statement::Rule(_) => Some(o.location().span),
            _ => None,
        })
        .collect();
    assert_eq!(
        occ.occurrences().len(),
        2,
        "the malformed statement yields no occurrence"
    );
    assert!(
        !occ.diagnostics().is_empty(),
        "the malformed statement is diagnosed in the batch"
    );
    assert!(names.len() == 2, "both well-formed neighbors occur");
}

#[test]
fn per_occurrence_diagnostics_are_a_nonempty_restriction_of_the_batch() {
    // A theory-atom argument-list pool `&t(a; b)` raises its FIRST alternative and marks the
    // rest with a `PooledArgumentList` diagnostic (§8, §17): a best-effort partial that raises
    // to a Some occurrence carrying its OWN diagnostic — so the per-occurrence slice is
    // non-empty and the ⊆ check below is not vacuous.
    let occ = raised_occurrences("&t(a; b).");
    assert!(
        occ.occurrences()
            .iter()
            .any(|o| !o.diagnostics().is_empty()),
        "at least one occurrence carries its own diagnostic (else the ⊆ check is vacuous)"
    );
    for o in occ.occurrences() {
        for d in o.diagnostics() {
            assert!(
                occ.diagnostics().contains(d),
                "per-occurrence diagnostics ride in the batch"
            );
        }
    }
}

#[test]
fn an_occurrence_reports_its_part_and_hands_over_its_owned_forms() {
    // `a.` precedes any `#program`, so it joins `base`; `b.` follows `#program step(t)`, so
    // its `part()` reports `step(t)`, never silently `base` (§4.1). The owned forms —
    // `into_occurrences`, then `into_statement` — carry the same content the borrows lend.
    let step = PartKey {
        name: Name::new("step").expect("a valid identifier"),
        formals: vec![Name::new("t").expect("a valid identifier")],
    };
    let occ = raised_occurrences("a. #program step(t). b.");
    assert_eq!(occ.occurrences().len(), 2, "both facts occur");
    assert_ne!(
        occ.occurrences()[0].part(),
        &step,
        "the pre-`#program` fact joins another part"
    );
    assert_eq!(
        occ.occurrences()[1].part(),
        &step,
        "the post-`#program` fact reports its `step(t)` part"
    );

    let borrowed = occ.occurrences()[1].statement().get().clone();
    let owned = occ.into_occurrences();
    assert_eq!(owned.len(), 2, "the owned occurrences are the same two");
    let second = owned.into_iter().nth(1).expect("the second occurrence");
    assert_eq!(
        second.into_statement().get(),
        &borrowed,
        "the owned statement matches the borrow"
    );
}

// ---- The active part rides each occurrence; a malformed delimiter recovers (§4.1, §8) ----

#[test]
fn each_occurrence_carries_the_active_program_part() {
    // `a.` precedes any `#program`, so it carries `base`; `p(t).` follows `#program step(t)`,
    // so it carries `step(t)`; `q.` follows `#program other`, so it carries `other` — the
    // active part rides each occurrence, never silently `base` (§4.1).
    let occ = raised_occurrences("a.\n#program step(t).\np(t).\n#program other.\nq.");
    assert_eq!(
        part_names(&occ),
        vec![
            vec!["base".to_owned()],
            vec!["step".to_owned(), "t".to_owned()],
            vec!["other".to_owned()],
        ],
    );
}

#[test]
fn a_malformed_program_delimiter_leaves_the_active_part_unchanged() {
    // `#program .` has no name: it cannot open a part, so it is diagnosed and does not advance
    // the active part. The facts on either side both carry the `step(t)` part it failed to
    // replace, and its `IncompleteStatement` rides the batch (§4.1, §8).
    let occ = raised_occurrences("#program step(t).\np(t).\n#program .\nq(t).");
    assert_eq!(
        part_names(&occ),
        vec![
            vec!["step".to_owned(), "t".to_owned()],
            vec!["step".to_owned(), "t".to_owned()],
        ],
        "the malformed delimiter leaves the part at step(t)"
    );
    assert!(
        occ.diagnostics()
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::IncompleteStatement)),
        "the malformed delimiter is diagnosed as an incomplete statement: {:?}",
        occ.diagnostics(),
    );
}

#[test]
fn into_raised_equals_raise_program_and_diagnostics() {
    for text in [
        "a. b. a.",                           // a duplicate that merges
        "1{#true}1.\n1{#true;#true}1.",       // content-equal, unequal nested counts
        "#program step(t).\np(t). q.\n1 { .", // parts + a malformed statement
    ] {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        let parse = parse(&source, Dialect::Clingo);
        let direct = raise(&parse);
        let via = raise_occurrences(&parse).into_raised();
        assert_eq!(
            via.program(),
            direct.program(),
            "same merged program: {text:?}"
        );
        assert_eq!(
            via.diagnostics(),
            direct.diagnostics(),
            "same diagnostics: {text:?}"
        );
    }
}

// ---- The occurrence corners: docs, UTF-8 spans, duplicate elements, separate sources (§8, §16) ----

#[test]
fn an_occurrence_carries_its_leading_doc() {
    // The statement's string argument is read under the parse dialect as setup; the `%!`
    // doc rides the occurrence it documents with exactly one leading ASCII space stripped (§8).
    let occ = raised_occurrences("%! docs here\np(\"s\").");
    let one = &occ.occurrences()[0];
    let docs: Vec<&str> = one.statement().provenance().annotations().doc().collect();
    assert_eq!(
        docs,
        ["docs here"],
        "the %! doc rides the occurrence, one space stripped"
    );
}

#[test]
fn an_occurrences_location_is_correct_after_leading_utf8_and_comments() {
    // A multibyte comment and a multibyte (so, here, unparseable) rule precede `p.`; its
    // occurrence still slices to its own source text, the span a byte offset past the prefix.
    let text = "% über comment\nα :- β.\np.";
    let occ = raised_occurrences(text);
    let last = occ.occurrences().last().expect("an occurrence");
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let sliced = source.slice(last.location().span).expect("in bounds");
    assert_eq!(
        sliced, "p.",
        "the occurrence locates its own source text past the UTF-8 prefix"
    );
}

#[test]
fn duplicate_elements_within_one_rule_carry_one_element_with_the_occurrence_count() {
    // The two content-equal `#true` elements collapse to one element on the occurrence's
    // canonical statement, whose provenance unions both parsed origins — count two (§6.3, §8).
    let occ = raised_occurrences("1{#true;#true}1.");
    assert_eq!(
        boolean_origin_counts(occ.occurrences()[0].statement().get()),
        vec![2]
    );
}

#[test]
fn equal_rules_in_separate_sources_keep_separate_locations() {
    // Content-equal rules raised from distinct sources keep their distinct locations — a merge
    // would be a cross-source collision the occurrence stream never makes (§8).
    let raise_in = |id: u32, text: &str| {
        let source = Source::new(SourceId::new(id), text.to_owned()).expect("admits");
        raise_occurrences(&parse(&source, Dialect::Clingo)).into_occurrences()
    };
    let a = raise_in(1, "1{#true;#true}1.");
    let b = raise_in(2, "1{#true;#true}1.");
    // Read each location before the consuming `into_iter` moves the Vec.
    let a_loc = a[0].location();
    let b_loc = b[0].location();
    assert_ne!(
        a_loc.source, b_loc.source,
        "distinct SourceIds, distinct locations"
    );
    let a_stmt = a
        .into_iter()
        .next()
        .expect("an occurrence")
        .into_statement();
    let b_stmt = b
        .into_iter()
        .next()
        .expect("an occurrence")
        .into_statement();
    assert_eq!(a_stmt.get(), b_stmt.get(), "same content up to provenance");
}
