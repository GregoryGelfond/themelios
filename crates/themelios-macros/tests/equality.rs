//! The construction-equality acceptance (program §16, the *first-solve*
//! witness's construction half): for every macro, the value it builds is
//! **structurally equal — up to and including provenance (`Origin::Constructed`,
//! program §6) — to the value built through the spelled-out program-tier §7.1
//! constructors** it names (docs/design/macros.md §8, §11). The codegen (AST →
//! constructor calls) and a hand-written constructor chain are two spellings of
//! one construction, and this witness holds them exact.
//!
//! The witness **accretes per macro** (TDD): each macro's first test is its
//! equality witness, comparing a value built through the macro against the same
//! value built by hand. Among them the term-shape witnesses carry a value
//! (`assert_eq!`) proof for eight of the nine `codegen_term` arms — `Constant`,
//! `Variable`, `Function`, `External`, `Pool`, `Unary`, `Binary`, and `Abs` —
//! the load-bearing proof that each spells the *right* constructor and not merely
//! a frozen emission (§11, §16); the ninth arm, `Splice`, has its value
//! round-trip in the §11 splice round-trip property law (tests/laws.rs), not
//! here. Beyond the term arms, the finer compositional shapes carry a value
//! witness here too — the comparison chain (its operand order and both step
//! relations), the atom argument-list pool, an `#external` carrying a value, and
//! the nullary-function theory symbol — each told exact against the program §7.1
//! constructor it names (§11, §16), the value proof beside its change-detector
//! golden. The shapes still resting on a golden in `codegen.rs` rather than a
//! value witness here are the boolean and conditional literals, a negated
//! theory-atom body element, and the remaining theory container and symbol-leaf
//! arms (the `Infimum` and `Supremum` bounds among them) — the golden and
//! property instruments the design lists beside this one (§11).
//!
//! The witnesses are gathered by the **floor role** each macro fills in the
//! solve floor (spec §3.2): `mod first_solve` (`atom!`, `fact!`, `rule!`,
//! `constraint!`, `program!` — the core program), `mod optimization`
//! (`minimize!`, `maximize!`), `mod enumeration` (`show!`), and `mod multi_shot`
//! (`external!` — its structural-equality half here, its behavioral multi-shot
//! half seeded for the solve stage that runs it). The
//! `the_floor_mapping_covers_every_macro` test holds the mapping total over the
//! nine, so a macro absent from a role is a visible gap. The codegen-breadth
//! witnesses below — the term, head, body, and theory-atom arms reached through
//! `fact!`/`rule!` — are grammar coverage (§6, §7, §8), distinct from the floor
//! roles.

use themelios_macros::{atom, constraint, external, fact, maximize, minimize, program, rule, show};
use themelios_program::construct;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, Disjunction, DisjunctionElement, External,
    FunctionAggregate, Guard, HeadAggregate, HeadAggregateElement, IntoHead, Literal,
    OptimizeElement, Program, Relation, Rule, SetAggregate, SetElement, Show, Statement,
    TheoryAtom, TheoryElement, TheoryGuard, TheoryOperator, TheoryTerm, weight,
};
use themelios_program::provenance::Origin;
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{Term, Variable};

/// `Name::new(text)`, discharged as the codegen's `.expect()` is (the fixture text is a
/// valid identifier by inspection).
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// `VarName::new(text)`, discharged as [`name`] is.
fn var(text: &str) -> VarName {
    VarName::new(text).expect("a valid variable")
}

/// `Rule::fact(Atom::new(N("p"), [term]))` — the single-argument `p(term)` fact the
/// term-shape witnesses (§6) compare their `fact!(p(term))` against.
fn fact_p(term: Term) -> Rule {
    Rule::fact(Atom::new(name("p"), [term]))
}

// ============================================================================
// The floor mapping (spec §3.2): each construction macro under the solve-floor
// role it realises, so a macro absent from a role is a visible gap. The
// completeness test at the end holds the mapping total over the nine macros.
// ============================================================================

/// The *first-solve* floor (spec §3.2): the core program a satisfiability solve
/// reads — facts, rules, integrity constraints, head atoms, and the whole-program
/// block. Each macro's value equals the program-tier constructor it names (§8).
mod first_solve {
    use super::*;

    #[test]
    fn fact_macro_equals_the_constructor() {
        let by_macro: Rule = fact!(p(1, a));
        let by_hand = Rule::fact(Atom::new(
            name("p"),
            [Term::from(1i32), Term::constant(name("a"))],
        ));
        // Structural equality; provenance is `Constructed` on both (erased from identity).
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn rule_macro_equals_head_when_body() {
        let by_macro = rule!(q(X) :- p(X));
        let by_hand = Atom::new(name("q"), [Term::variable(var("X"))])
            .into_head()
            .when(Atom::new(name("p"), [Term::variable(var("X"))]));
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn constraint_macro_equals_the_constructor() {
        let by_macro = constraint!(:- p(X));
        let by_hand = Rule::constraint(Atom::new(name("p"), [Term::variable(var("X"))]));
        assert_eq!(by_macro, by_hand);
    }

    // The head-atom macro (§8): reaching an `Atom` by assembling a fact and extracting
    // its single head atom, strong negation read positionally.

    #[test]
    fn atom_macro_equals_atom_new() {
        let by_macro: Atom = atom!(p(1, a));
        let by_hand = Atom::new(name("p"), [Term::from(1i32), Term::constant(name("a"))]);
        // Structural equality; provenance is `Constructed` on both (erased from identity).
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn strong_negation_atom_macro_equals_the_negated_atom() {
        // `-p` in head position is the atom's positional strong negation (§8): `Neg` on an `Atom`
        // flips the sign to `Negative`, distinct from the arithmetic `-` of a term.
        let by_macro = atom!(-p(1));
        let by_hand = -Atom::new(name("p"), [Term::from(1i32)]);
        assert_eq!(by_macro, by_hand);
    }

    // The program block macro (§8): a whole-program block assembled through `Program::of`,
    // each statement built by its family constructor and converted to `Statement`.

    // `#[rustfmt::skip]`: the block writes its statements ASP-side — the neck `:-` (two joint
    // tokens) and the statement-terminating `.`s — which rustfmt would read as Rust and reflow
    // (`p(1). q` into a method chain). The skip keeps the fixture's statements intact.
    #[rustfmt::skip]
    #[test]
    fn program_macro_equals_program_of() {
        // Each statement is built by its family constructor and converted to `Statement` — the
        // homogeneous array `Program::of` admits — the program equal to `Program::of` over the
        // same hand-spelled statements, structurally and to provenance (§16).
        let by_macro: Program = program!{ p(1). q(X) :- p(X). };
        let by_hand = Program::of([
            Statement::from(Rule::fact(Atom::new(name("p"), [Term::from(1i32)]))),
            Statement::from(
                Atom::new(name("q"), [Term::variable(var("X"))])
                    .into_head()
                    .when(Atom::new(name("p"), [Term::variable(var("X"))])),
            ),
        ]);
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn empty_program_macro_equals_program_empty() {
        // An empty block is the named empty program, locking the empty-array inference the codegen
        // sidesteps with `Program::empty()` (program §7.1).
        let by_macro: Program = program! {};
        assert_eq!(by_macro, Program::empty());
    }

    // The "including provenance" half of the acceptance (§11): the `assert_eq!`s elsewhere erase
    // provenance (`WithProvenance`'s identity reads content alone), so this witnesses it directly.
    #[rustfmt::skip]
    #[test]
    fn a_built_statement_carries_the_constructed_origin() {
        // The acceptance is structural equality up to *and including* provenance
        // (`Origin::Constructed`, program §6), but `WithProvenance`'s equality reads content alone,
        // so no `assert_eq!` above can observe provenance. Witness the "including provenance" half
        // directly: a statement a macro builds carries the `Constructed` origin, identical to the
        // value built by hand (§5, §11).
        let by_macro: Program = program! { p(1). };
        let statement = by_macro.statements().next().expect("one statement");
        let origins: Vec<&Origin> = statement.provenance().origins().collect();
        assert_eq!(origins, [&Origin::Constructed]);
    }
}

/// The *optimization* floor (spec §3.2): a `#minimize`/`#maximize` statement, each
/// element a weighted term at a priority. Each macro's value equals its program-tier
/// constructor (program §4.7).
mod optimization {
    use super::*;

    #[test]
    fn minimize_macro_equals_the_constructor() {
        let by_macro = minimize!({ 3@1 });
        let by_hand = construct::minimize([OptimizeElement::new(
            weight(Term::from(3i32)).at_priority(Term::from(1i32)),
            [],
            Condition::empty(),
        )]);
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn maximize_macro_equals_the_constructor() {
        let by_macro = maximize!({ 5 });
        let by_hand = construct::maximize([OptimizeElement::new(
            weight(Term::from(5i32)),
            [],
            Condition::empty(),
        )]);
        assert_eq!(by_macro, by_hand);
    }
}

/// The *enumeration* floor (spec §3.2): a `#show` directive, in each of its four
/// forms — a signature, a term, a term under a body, or all. Each macro's value
/// equals the `Show` constructor (program §4.8).
mod enumeration {
    use super::*;

    #[test]
    fn show_signature_macro_equals_the_variant() {
        let by_macro = show!(p / 1);
        let by_hand = Show::Signature(Signature::new(Sign::Positive, name("p"), 1));
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn show_all_macro_equals_the_variant() {
        let by_macro = show!();
        let by_hand = Show::All;
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn show_term_macro_equals_the_variant() {
        let by_macro = show!(a);
        let by_hand = Show::Term(Term::constant(name("a")));
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn show_term_body_macro_equals_the_constructor() {
        let by_macro = show!(a : p(X));
        let by_hand = Show::term_body(
            Term::constant(name("a")),
            Body::new([BodyElement::from(Atom::new(
                name("p"),
                [Term::variable(var("X"))],
            ))]),
        );
        assert_eq!(by_macro, by_hand);
    }
}

/// The *multi-shot* floor (spec §3.2): `external!` builds the `#external`
/// declaration whose atom a solver may assume, and reassign, across successive
/// solve calls. Its **structural-equality** half — the built `External` equals
/// `External::new` — is proved here; its **behavioral** half (that the assumption
/// drives a multi-shot solve) is seeded for the solve stage that runs it, there
/// being no solver in this tier (docs/design/macros.md §11).
mod multi_shot {
    use super::*;

    #[test]
    fn external_macro_equals_the_constructor() {
        let by_macro = external!(p(X));
        let by_hand = External::new(
            Atom::new(name("p"), [Term::variable(var("X"))]),
            Body::empty(),
            None,
        );
        assert_eq!(by_macro, by_hand);
    }

    #[test]
    fn external_macro_over_a_body_equals_the_constructor() {
        let by_macro = external!(a : p(X));
        let by_hand = External::new(
            Atom::constant(name("a")),
            Body::new([BodyElement::from(Atom::new(
                name("p"),
                [Term::variable(var("X"))],
            ))]),
            None,
        );
        assert_eq!(by_macro, by_hand);
    }

    // `external!` builds only the value-less declaration: it appends its own statement
    // terminator, so it cannot spell the post-dot value annotation `#external p. [v]`
    // (grammar §13). The valued form is reached through the program door, which carries its
    // own dots, so the value slot is witnessed here.
    #[rustfmt::skip]
    #[test]
    fn a_valued_external_equals_the_constructor() {
        // The `#external` value slot: the atom a solver may assume carries an optional,
        // not-meaningful value in the post-dot annotation `[a]`, which lands in `External::new`'s
        // third argument as `Some` — its absence `None`, witnessed above (program §7.1; grammar
        // §13). One statement, so `Program::of` carries exactly the one `External`.
        let by_macro: Program = program! { #external p(X). [a] };
        let by_hand = Program::of([Statement::from(External::new(
            Atom::new(name("p"), [Term::variable(var("X"))]),
            Body::empty(),
            Some(Term::constant(name("a"))),
        ))]);
        assert_eq!(by_macro, by_hand);
    }

    /// The behavioral half of the multi-shot floor, seeded for `themelios-solve`
    /// (docs/design/macros.md §11; spec §3.2): a solver must let the `#external`
    /// atom this builds be assumed true, solved, then reassigned false and
    /// re-solved, the answer sets differing only as the assumption dictates. This
    /// tier has no solver to drive the assume / solve / reassign steps, so that
    /// half is ignored until the solve stage supplies one; the structural half —
    /// that `external!` equals `External::new` — is proved by the witnesses above.
    #[test]
    #[ignore = "behavioral multi-shot: driven by themelios-solve (docs/design/macros.md §11); the structural half is proved above"]
    fn external_assumption_is_reassignable_across_solves() {
        // The artifact the behavioral half assumes over — the `#external` atom
        // whose truth a multi-shot solve toggles. Building it here binds the seed
        // to the surface the solve stage will exercise.
        let declaration = external!(p(X));
        // The declaration fixes no value of its own — the assumption the solver
        // toggles does, not a value carried here (program §4.8; grammar §13).
        assert!(declaration.value().is_none());
    }
}

/// The floor mapping is total over the nine construction macros (spec §3.2): each
/// exported macro is invoked here under the solve-floor role its module above
/// realises, so a macro dropped from the surface — or a floor role left without a
/// macro — is a compile error here, not a silent gap. These invocations are the
/// list; the modules above hold each macro's equality witness against the
/// program-tier constructor it names (docs/design/macros.md §8, §11).
#[test]
fn the_floor_mapping_covers_every_macro() {
    // first-solve: the core program a satisfiability solve reads.
    let _ = atom!(p(1));
    let _ = fact!(p(1));
    let _ = rule!(q(X) :- p(X));
    let _ = constraint!(:- p(X));
    let _ = program! {};
    // optimization.
    let _ = minimize!({ 1 });
    let _ = maximize!({ 1 });
    // enumeration.
    let _ = show!();
    // multi-shot.
    let _ = external!(p(X));
}

// ============================================================================
// The codegen-breadth witnesses (§6, §7, §8): reached through the first-solve
// macros `fact!`/`rule!`, these witness the head, body, term, and theory-atom
// codegen arms — grammar coverage, distinct from the floor-role witnesses above.
// ============================================================================

// ---- the head families (§8): the disjunction, choice, and head-aggregate heads the
// statement macros build, each fanning out to its program §7.1 constructor as the raise does ----

#[test]
fn disjunction_head_macro_equals_the_constructor() {
    // `a | b` — a disjunctive head, each element a bare literal under the empty condition (the
    // conditional-literal split is exercised by the choice witness). `Disjunction` is `IntoHead`,
    // so `Rule::fact` coerces it as it does an `Atom`.
    let by_macro = fact!(a | b);
    let by_hand = Rule::fact(Disjunction::new([
        DisjunctionElement::new(Literal::from(Atom::new(name("a"), [])), Condition::empty()),
        DisjunctionElement::new(Literal::from(Atom::new(name("b"), [])), Condition::empty()),
    ]));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn choice_head_macro_equals_the_constructor() {
    // A bounded choice `1 { a : q(X) }`: the left guard `1` (its relation the grammar's default,
    // stated as absence), one element split from its conditional literal into the literal `a`
    // under the condition `q(X)`, and no right guard.
    let by_macro = fact!(1 { a : q(X) });
    let by_hand = Rule::fact(Choice::new(
        Some(Guard {
            relation: None,
            term: Term::from(1i32),
        }),
        [ChoiceElement::new(
            Literal::from(Atom::new(name("a"), [])),
            Condition::new([Literal::from(Atom::new(
                name("q"),
                [Term::variable(var("X"))],
            ))]),
        )],
        None,
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn head_aggregate_macro_equals_the_constructor() {
    // `#count { X : p(X) }`: a head aggregate whose one element *derives* the literal `p(X)` from
    // the term tuple `X` under the empty condition (a head element is `terms : literal :
    // condition`, only its first colon written), no guards.
    let by_macro = fact!(#count { X : p(X) });
    let by_hand = Rule::fact(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [Term::variable(var("X"))],
            Literal::from(Atom::new(name("p"), [Term::variable(var("X"))])),
            Condition::empty(),
        )],
        None,
    ));
    assert_eq!(by_macro, by_hand);
}

// ---- the body aggregates (§8): a body element the `ast::BodyElement::Aggregate` arm fans a
// set to `SetAggregate` and a function to `FunctionAggregate`, under its own default negation ----

#[test]
fn body_function_aggregate_macro_equals_the_constructor() {
    // `1 <= #sum { X : q(X) }` as a body element: a function aggregate with the left guard
    // `1 <=` (its relation `Le` over the bound `1`), one testing element (the term tuple `X`
    // under the condition `q(X)`, no derived literal), no right guard — positive, so it rides
    // in through `From<Aggregate>`.
    let by_macro = rule!(p :- 1 <= #sum { X : q(X) });
    let by_hand = Atom::new(name("p"), [])
        .into_head()
        .when(Body::new([BodyElement::from(Aggregate::Function(
            FunctionAggregate::new(
                Some(Guard {
                    relation: Some(Relation::Le),
                    term: Term::from(1i32),
                }),
                AggregateFunction::Sum,
                [BodyAggregateElement::new(
                    [Term::variable(var("X"))],
                    Condition::new([Literal::from(Atom::new(
                        name("q"),
                        [Term::variable(var("X"))],
                    ))]),
                )],
                None,
            ),
        ))]));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn negated_body_set_aggregate_macro_equals_the_constructor() {
    // `not { a; b }` as a body element: a set (cardinality) aggregate over two bare set elements
    // (a set element keeps a conditional literal whole, unlike a choice element), under
    // whole-aggregate default negation — `not` wraps the `Aggregate` through the `Negatable` door.
    let by_macro = rule!(p :- not { a; b });
    let by_hand = Atom::new(name("p"), [])
        .into_head()
        .when(Body::new([construct::not(Aggregate::Set(
            SetAggregate::new(
                None,
                [
                    SetElement::Literal(Literal::from(Atom::new(name("a"), []))),
                    SetElement::Literal(Literal::from(Atom::new(name("b"), []))),
                ],
                None,
            ),
        ))]));
    assert_eq!(by_macro, by_hand);
}

// ---- the finer statement shapes (§8, §4.6): an atom's argument-list pool and a comparison
// chain, each told exact against the program §7.1 constructor it names, the value proof beside
// its change-detector golden (§11) ----

#[test]
fn a_pooled_argument_list_atom_equals_the_constructor() {
    // `p(a; b)` is one atom whose arguments pool across two alternatives (§8) — `codegen_atom`'s
    // `Atom::pooled` arm, reached in head position through `fact!`, distinct from the pool *term*
    // `p((a; b))` below. Pinning the value proves the alternatives are grouped right, which the
    // golden — asserting only that the emission mentions `Atom::pooled` — does not.
    let by_macro = fact!(p(a; b));
    let by_hand = Rule::fact(
        Atom::pooled(
            name("p"),
            [
                vec![Term::constant(name("a"))],
                vec![Term::constant(name("b"))],
            ],
        )
        .unwrap(),
    );
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_comparison_chain_body_equals_the_constructor() {
    // `1 < X <= 5` is one body literal carrying a guard sequence, not a conjunction (§4.6):
    // `Comparison::new` for the first step, `.chain` for the second. The two steps use *distinct*
    // relations (`<` then `<=`) so the value pins the second step's relation independently of the
    // first: an identical-relation chain (`< < `) would let a mutant that reuses the first step's
    // relation for the second slip through. Pinning the value proves the operand order *and* both
    // step relations — a single-step comparison would leave a swapped operand or a wrong second
    // relation uncaught. A comparison is a positive body literal, riding into the constraint
    // through `IntoBody for Comparison` (program §7.1).
    let by_macro = constraint!(:- 1 < X <= 5);
    let by_hand = Rule::constraint(
        Comparison::new(Term::from(1i32), Relation::Lt, Term::variable(var("X")))
            .chain(Relation::Le, Term::from(5i32)),
    );
    assert_eq!(by_macro, by_hand);
}

// ---- the term-shape witnesses (§6): the value proof that each non-splice `codegen_term`
// arm spells the right constructor, not a frozen emission, beside the codegen goldens — the
// `Splice` arm's value round-trip is the §11 property law (tests/laws.rs) ----

#[test]
fn a_function_term_equals_the_constructor() {
    // A function used as a *term* (`p(f(1))`): the `codegen_term` `Function` arm, spelled
    // through `Term::function` — the value proof beside its change-detector goldens, which
    // assert only that the emission mentions `Term::function`, not that it builds this value.
    assert_eq!(
        fact!(p(f(1))),
        fact_p(Term::function(name("f"), [Term::from(1i32)]))
    );
}

#[test]
fn an_interval_term_equals_the_constructor() {
    assert_eq!(
        fact!(p(1..3)),
        fact_p(Term::from(1i32).to(Term::from(3i32)))
    );
}

#[test]
fn a_pool_term_equals_the_constructor() {
    assert_eq!(
        fact!(p((a; b))),
        fact_p(Term::pool([Term::constant(name("a")), Term::constant(name("b"))]).unwrap())
    );
}

#[test]
fn an_absolute_term_equals_the_constructor() {
    assert_eq!(fact!(p(|X|)), fact_p(Term::variable(var("X")).abs()));
}

#[test]
fn a_binary_arithmetic_term_equals_the_constructor() {
    // `*`-over-`+` in Rust mirrors the ASP parse `1 + (2 * 3)`, so the hand-spelled
    // operator tree matches the parsed one, and neither folds (an operator term never
    // does, program §3.5). One arithmetic witness stands for the whole `binary_operator!`
    // family — `+`/`*` are representative, generated identically (construct.rs).
    assert_eq!(
        fact!(p(1 + 2 * 3)),
        fact_p(Term::from(1i32) + Term::from(2i32) * Term::from(3i32))
    );
}

#[test]
fn a_tuple_term_equals_the_constructor() {
    assert_eq!(
        fact!(p((a, b))),
        fact_p(Term::tuple([
            Term::constant(name("a")),
            Term::constant(name("b"))
        ]))
    );
}

#[test]
fn an_arithmetic_negation_term_equals_the_constructor() {
    // Arithmetic negate in term position, distinct from the strong `-p` of an atom head.
    assert_eq!(fact!(p(-X)), fact_p(-Term::variable(var("X"))));
}

#[test]
fn a_bitwise_complement_term_equals_the_constructor() {
    assert_eq!(fact!(p(~X)), fact_p(Term::variable(var("X")).complement()));
}

// `#[rustfmt::skip]`: the power operator is two joint `*` tokens, which rustfmt would
// reformat to `* *3` — splitting the run the dialect munches into one `STAR_STAR`
// (grammar §4.6). The skip keeps the fixture's `**` intact.
#[rustfmt::skip]
#[test]
fn a_power_term_equals_the_constructor() {
    // `.pow()` is a distinct method, not the `binary_operator!` family, so it carries
    // its own witness (its map-row twin `Interval` uses `.to()`).
    assert_eq!(fact!(p(2 ** 3)), fact_p(Term::from(2i32).pow(Term::from(3i32))));
}

#[test]
fn an_external_call_term_equals_the_constructor() {
    // Direct public-enum construction, bypassing canonicalization (program §3.3).
    assert_eq!(
        fact!(p(@f(1))),
        fact_p(Term::External {
            name: name("f"),
            arguments: vec![Term::from(1i32)],
        })
    );
}

#[test]
fn the_anonymous_variable_term_equals_the_constructor() {
    assert_eq!(fact!(p(_)), fact_p(Term::anonymous()));
}

#[test]
fn a_string_term_equals_the_constructor() {
    assert_eq!(fact!(p("s")), fact_p(Term::from("s")));
}

#[test]
fn the_empty_tuple_term_equals_the_constructor() {
    // The empty tuple `()`, compiling the empty-array element-type inference that the
    // codegen's `[#(#terms),*]` rests on (a codegen-emission concern, locked here).
    assert_eq!(fact!(p(())), fact_p(Term::tuple([])));
}

// ---- the theory-atom witnesses (§7): the non-`Symbolic` theory codegen a splice
// round-trip does not reach — `TheoryAtom::new`, `TheoryElement`, `TheoryGuard`, and the
// `Variable` / `Symbolic` / `Operation` theory-term arms ----

#[test]
fn theory_atom_macro_equals_the_constructor() {
    let by_macro = fact!(&sum { X } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Variable(Variable::Named(var("X")))],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn theory_atom_operation_macro_equals_the_constructor() {
    let by_macro = fact!(&sum { X + 1 } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            // `operators[i]` is the run before `operands[i]`, so a leading empty run
            // precedes `X` and `+` precedes `1` (program §4.9).
            [TheoryTerm::Operation {
                operators: vec![vec![], vec![TheoryOperator::new("+")]],
                operands: vec![
                    TheoryTerm::Variable(Variable::Named(var("X"))),
                    TheoryTerm::Symbolic(Symbol::Number(1)),
                ],
            }],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_string_theory_symbol_equals_the_constructor() {
    // The `Symbol::String` theory-symbol leaf: a string in theory-term position lifts to
    // `TheoryTerm::Symbolic(Symbol::string(…))`, told exact against the hand spelling. This
    // pins the string symbol-leaf value; the nullary-`Function` leaf is pinned by
    // `a_nullary_function_theory_symbol_equals_the_constructor` below, leaving the `Infimum`
    // and `Supremum` bounds to a change-detector golden.
    let by_macro = fact!(&sum { "hi" } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Symbolic(Symbol::string("hi"))],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_nullary_function_theory_symbol_equals_the_constructor() {
    // A bare identifier in theory-term position lifts to a nullary, positive `Symbol::Function`
    // — `codegen_theory_symbol`'s symbolic-constant arm (§7), pinned here by value in both the
    // element (`a`) and the guard (`n`), where the string witness above pins the `Symbol::String`
    // leaf. The value proof beside its change-detector golden.
    let by_macro = fact!(&sum { a } <= n);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Symbolic(Symbol::function(
                name("a"),
                [],
                Sign::Positive,
            ))],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::function(name("n"), [], Sign::Positive)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn conditioned_theory_element_macro_equals_the_constructor() {
    // A theory element's condition is a `Condition` of ordinary literals, lowered
    // through the same door a rule body's condition takes.
    let by_macro = fact!(&sum { X: p(X) } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Variable(Variable::Named(var("X")))],
            Some(Condition::new([Literal::from(Atom::new(
                name("p"),
                [Term::variable(var("X"))],
            ))])),
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}
