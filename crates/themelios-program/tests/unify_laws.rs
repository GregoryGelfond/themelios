//! The most general unifier (docs/design/program.md §11.1): the near-linear
//! Martelli–Montanari algorithm over two atoms in one shared variable namespace, its result
//! a **triangular** substitution the resolving `substitute` (§9.2) reads to the fixpoint, its
//! **forced** occurs check sound by construction, and `rename_apart` as the caller's
//! standardize-apart step (§9.2, §11.1).

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::unify::{Fresh, Substitution, mgu, rename_apart};

// ---- builders ----

fn var(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a valid variable name"))
}
fn tvar(text: &str) -> Term {
    Term::Variable(var(text))
}
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}
fn atom(functor: &str, arguments: impl IntoIterator<Item = Term>) -> Atom {
    Atom::new(name(functor), arguments)
}
fn func(functor: &str, arguments: impl IntoIterator<Item = Term>) -> Term {
    Term::Function {
        name: name(functor),
        arguments: arguments.into_iter().collect(),
    }
}
fn num(value: i32) -> Term {
    Term::from(value)
}
/// A ground constant, in its canonical (collapsed) `Symbolic` form — the shape a substitution
/// resolves a variable to and the shape an argument canonicalizes to at a construction door.
fn konst(text: &str) -> Term {
    Term::Symbolic(Symbol::Function {
        name: name(text),
        arguments: Vec::new(),
        sign: Sign::Positive,
    })
}

/// The resolved image of an atom's argument terms under a substitution — `substitute`
/// follows the triangular chains to the fixpoint (§9.2), so this is the atom "after σ".
fn image(atom: &Atom, s: &Substitution) -> Vec<Term> {
    atom.arguments
        .iter()
        .map(|term| term.clone().substitute(s))
        .collect()
}

/// The unifier of two atoms, asserting one exists (`Ok(Some)`).
fn unifier(a: &Atom, b: &Atom) -> Substitution {
    match mgu(a, b) {
        Ok(Some(s)) => s,
        other => panic!("expected a unifier of {a:?} and {b:?}, got {other:?}"),
    }
}

/// A source of fresh names seeded over a program carrying the given atoms as facts, so
/// `rename_apart` — which draws from it (§9.2) — mints variables free of theirs.
fn fresh_over(atoms: impl IntoIterator<Item = Atom>) -> Fresh {
    let program = Program::of(
        atoms
            .into_iter()
            .map(|a| WithProvenance::constructed(Statement::Rule(Rule::fact(a)))),
    );
    Fresh::over(&program)
}

// ---- soundness (over the RESOLVING substitute, §11.1) ----

#[test]
fn a_unifier_makes_both_atoms_equal_once_resolved() {
    // A spread of unifiable pairs across the constructor fragment: a bare variable against a
    // ground symbol; two non-ground functions (the Function/Function descent); two non-ground
    // tuples; and a value chain X ↦ f(Y), Y ↦ g(Z) (the deref graph the occurs check walks).
    let cases = [
        (atom("p", [tvar("X")]), atom("p", [num(1)])),
        (
            atom("m", [func("g", [tvar("X")])]),
            atom("m", [func("g", [tvar("Y")])]),
        ),
        (
            atom("t", [Term::Tuple(vec![tvar("X"), num(2)])]),
            atom("t", [Term::Tuple(vec![num(1), tvar("Y")])]),
        ),
        (
            atom("chain", [tvar("X"), tvar("Y")]),
            atom("chain", [func("f", [tvar("Y")]), func("g", [tvar("Z")])]),
        ),
    ];
    for (a, b) in cases {
        let s = unifier(&a, &b);
        assert_eq!(
            image(&a, &s),
            image(&b, &s),
            "unified atoms must be equal once resolved: {a:?} vs {b:?}",
        );
    }
}

#[test]
fn soundness_holds_through_a_triangular_binding() {
    // edge(X, Y) and edge(Y, c): σ binds Y ↦ c and X ↦ Y (a *variable*, not a term) — a
    // triangular chain. A single, non-resolving pass would leave edge(c, Y) beside
    // edge(c, c); the resolving substitute follows X ↦ Y ↦ c, so both are edge(c, c).
    let a = atom("edge", [tvar("X"), tvar("Y")]);
    let b = atom("edge", [tvar("Y"), konst("c")]);
    let s = unifier(&a, &b);
    assert_eq!(image(&a, &s), image(&b, &s));
    assert_eq!(image(&a, &s), vec![konst("c"), konst("c")]);
}

// ---- most generality (§11.1) ----

#[test]
fn the_unifier_of_two_variables_is_a_renaming_not_a_grounding() {
    // mgu(p(X), p(Y)) equates X and Y but grounds neither — the common image is still a
    // variable, the most general choice; a less general unifier would bind both to a term.
    let s = unifier(&atom("p", [tvar("X")]), &atom("p", [tvar("Y")]));
    let xi = tvar("X").substitute(&s);
    let yi = tvar("Y").substitute(&s);
    assert_eq!(xi, yi, "X and Y are equated");
    assert!(
        matches!(xi, Term::Variable(_)),
        "the common image is still a variable: {xi:?}",
    );
}

#[test]
fn the_common_variable_is_the_ord_least_of_its_class() {
    // Unifying p(Z, Y) with p(Y, X) equates {X, Y, Z} into one class. The read-out names
    // the class by its Ord-least member (§11.1) — X — regardless of the order the
    // variables were first seen (Z, then Y, then X); so every member resolves to X, not
    // to the first-seen Z nor the greatest Z.
    let s = unifier(
        &atom("p", [tvar("Z"), tvar("Y")]),
        &atom("p", [tvar("Y"), tvar("X")]),
    );
    assert_eq!(
        tvar("X").substitute(&s),
        tvar("X"),
        "the least is its own image"
    );
    assert_eq!(tvar("Y").substitute(&s), tvar("X"));
    assert_eq!(tvar("Z").substitute(&s), tvar("X"));
}

#[test]
fn the_unifier_binds_only_what_it_must() {
    // mgu(p(X), p(f(Y))) binds X ↦ f(Y) and leaves Y free — it invents no binding for Y.
    let s = unifier(
        &atom("p", [tvar("X")]),
        &atom("p", [func("f", [tvar("Y")])]),
    );
    assert_eq!(tvar("X").substitute(&s), func("f", [tvar("Y")]));
    assert_eq!(tvar("Y").substitute(&s), tvar("Y"), "Y is untouched");
}

#[test]
fn a_fresh_predicate_avoids_a_functor_buried_in_a_term() {
    // A program mentioning the functor `aux0` only inside a term — p(aux0(X)) — never as a
    // head predicate. The argument is non-ground (aux0(X)), so it stays a `Term::Function`
    // rather than canonicalizing to a ground symbol leaf, and its functor is collected: the
    // fresh source must avoid it, functors and predicates sharing one namespace (§9.2). A
    // fresh predicate colliding with a term's functor would conflate two symbols downstream.
    let mut fresh = fresh_over([atom("p", [func("aux0", [tvar("X")])])]);
    assert_ne!(fresh.predicate("aux"), name("aux0"));
}

#[test]
fn a_fresh_predicate_avoids_a_ground_symbol_functor() {
    // The ground twin of the previous law: p(f0(a)) — f0(a) is fully ground, so a canonical
    // program stores it as a `Symbolic` leaf, not a `Term::Function`. Its functor f0 still
    // shares the predicate namespace (§9.2), so the fresh source must avoid it just the same;
    // a scan stopping at the ground leaf would return f0 and conflate two symbols downstream.
    let mut fresh = fresh_over([atom("p", [func("f0", [konst("a")])])]);
    assert_ne!(fresh.predicate("f"), name("f0"));
}

#[test]
fn every_unifier_factors_through_the_most_general_one() {
    // σ = mgu(p(X, Y), p(Y, Z)) equates X, Y, Z. τ, a more specific unifier grounding them all
    // to c, factors through σ: applying σ then τ is the same as applying τ (§11.1).
    let sigma = unifier(
        &atom("p", [tvar("X"), tvar("Y")]),
        &atom("p", [tvar("Y"), tvar("Z")]),
    );
    let tau = unifier(
        &atom("r", [tvar("X"), tvar("Y"), tvar("Z")]),
        &atom("r", [konst("c"), konst("c"), konst("c")]),
    );
    for v in ["X", "Y", "Z"] {
        let direct = tvar(v).substitute(&tau);
        let through = tvar(v).substitute(&sigma).substitute(&tau);
        assert_eq!(
            direct, through,
            "the unifier factors through the mgu at {v}"
        );
    }
}

// ---- the forced occurs check (§11.1) ----

#[test]
fn the_occurs_check_refuses_a_cyclic_binding_and_terminates() {
    // p(X) and p(f(X)): X cannot bind to a term containing X — a cyclic term is
    // unrepresentable, so it is Ok(None), and mgu returns (never diverges building it).
    assert_eq!(
        mgu(
            &atom("p", [tvar("X")]),
            &atom("p", [func("f", [tvar("X")])])
        ),
        Ok(None),
    );
    // Deeper occurrence, still caught.
    assert_eq!(
        mgu(
            &atom("p", [tvar("X")]),
            &atom("p", [func("f", [func("g", [tvar("X")])])]),
        ),
        Ok(None),
    );
}

#[test]
fn the_occurs_check_catches_a_mutual_cycle() {
    // pair(X, Y) and pair(f(Y), g(X)): X ↦ f(Y), Y ↦ g(X) — a cycle through two bindings, an
    // infinite term with no home, refused (§11.1).
    assert_eq!(
        mgu(
            &atom("pair", [tvar("X"), tvar("Y")]),
            &atom("pair", [func("f", [tvar("Y")]), func("g", [tvar("X")])]),
        ),
        Ok(None),
    );
}

#[test]
fn a_variable_in_a_sibling_is_not_an_occurrence() {
    // p(X) and p(f(Y)): X does not occur under f(Y), so it binds; the occurs check fires only
    // on a true occurrence, not on any shared shape.
    assert!(matches!(
        mgu(
            &atom("p", [tvar("X")]),
            &atom("p", [func("f", [tvar("Y")])])
        ),
        Ok(Some(_)),
    ));
}

// ---- matching as the degenerate case, and sign/name/arity agreement (§11.1) ----

#[test]
fn matching_a_pattern_against_a_ground_symbol_binds_the_variable() {
    let s = unifier(&atom("p", [tvar("X")]), &atom("p", [num(1)]));
    assert_eq!(tvar("X").substitute(&s), num(1));
}

#[test]
fn a_pattern_unifies_with_a_ground_symbol_iff_it_is_an_instance() {
    // f(X) matches the ground f(1) (X ↦ 1) but not g(1) (functor clash); two distinct ground
    // symbols never unify.
    assert!(matches!(
        mgu(
            &atom("p", [func("f", [tvar("X")])]),
            &atom("p", [func("f", [num(1)])]),
        ),
        Ok(Some(_)),
    ));
    assert_eq!(
        mgu(
            &atom("p", [func("f", [tvar("X")])]),
            &atom("p", [func("g", [num(1)])]),
        ),
        Ok(None),
    );
    assert_eq!(mgu(&atom("p", [num(1)]), &atom("p", [num(2)])), Ok(None),);
}

#[test]
fn a_ground_symbol_never_unifies_with_a_differing_function() {
    // A ground p(1) against p(f(X)): the number is not a function application, no unifier.
    assert_eq!(
        mgu(&atom("p", [num(1)]), &atom("p", [func("f", [tvar("X")])])),
        Ok(None),
    );
    // Two non-ground functions with different functors clash.
    assert_eq!(
        mgu(
            &atom("p", [func("f", [tvar("X")])]),
            &atom("p", [func("g", [tvar("Y")])]),
        ),
        Ok(None),
    );
    // Tuples of different arity clash.
    assert_eq!(
        mgu(
            &atom("p", [Term::Tuple(vec![tvar("X")])]),
            &atom("p", [Term::Tuple(vec![tvar("Y"), tvar("Z")])]),
        ),
        Ok(None),
    );
}

#[test]
fn atoms_unify_only_when_sign_name_and_arity_agree() {
    let px = atom("p", [tvar("X")]);
    // Name differs.
    assert_eq!(mgu(&px, &atom("r", [tvar("X")])), Ok(None));
    // Arity differs.
    assert_eq!(mgu(&px, &atom("p", [tvar("X"), tvar("Y")])), Ok(None));
    // Strong sign differs (`-p` against `p`).
    assert_eq!(
        mgu(&atom("p", [konst("a")]), &(-atom("p", [konst("a")]))),
        Ok(None),
    );
    // An identical atom unifies binding nothing — the affirmative empty match.
    let s = unifier(&px, &px);
    assert_eq!(s.iter().count(), 0, "identical atoms unify binding nothing");
}

// ---- one namespace, and rename-apart as the caller's standardize-apart step (§11.1) ----

#[test]
fn a_variable_is_shared_across_both_atoms() {
    // q(X, X) and q(a, b): the two X occurrences are one variable across *both* atoms, so X
    // cannot be both a and b — no unifier.
    assert_eq!(
        mgu(
            &atom("q", [tvar("X"), tvar("X")]),
            &atom("q", [konst("a"), konst("b")]),
        ),
        Ok(None),
    );
    // q(X, X) and q(a, a): consistent, X ↦ a.
    let s = unifier(
        &atom("q", [tvar("X"), tvar("X")]),
        &atom("q", [konst("a"), konst("a")]),
    );
    assert_eq!(tvar("X").substitute(&s), konst("a"));
}

#[test]
fn rename_apart_shares_no_variable_with_its_input() {
    // A nested atom so the rename descends the function arguments, renaming X consistently
    // wherever it occurs; no minted variable is one of the input's.
    let a = atom(
        "p",
        [func("f", [tvar("X")]), func("h", [tvar("Y"), tvar("X")])],
    );
    let mut fresh = fresh_over([a.clone()]);
    let renamed = rename_apart(&a, &mut fresh);
    for term in &renamed.arguments {
        for sub in term.subterms() {
            assert!(
                sub != &tvar("X") && sub != &tvar("Y"),
                "a renamed variable collides with the input: {sub:?}",
            );
        }
    }
}

#[test]
fn rename_apart_renames_consistently_and_preserves_shape() {
    // p(X, Y, X) -> p(V, W, V): the two X positions map to one fresh variable, distinct from
    // Y's; the functor, sign, and arity are unchanged.
    let a = atom("p", [tvar("X"), tvar("Y"), tvar("X")]);
    let mut fresh = fresh_over([a.clone()]);
    let renamed = rename_apart(&a, &mut fresh);
    assert_eq!(renamed.name, a.name);
    assert_eq!(renamed.sign, a.sign);
    assert_eq!(renamed.arguments.len(), 3);
    assert_eq!(
        renamed.arguments[0], renamed.arguments[2],
        "the two X positions rename together",
    );
    assert_ne!(
        renamed.arguments[0], renamed.arguments[1],
        "X and Y rename apart",
    );
    assert!(matches!(renamed.arguments[0], Term::Variable(_)));
}

#[test]
fn rename_apart_gives_each_anonymous_variable_its_own_fresh_name() {
    // p(_, _): each `_` is a distinct variable, so the two rename to two different fresh
    // (named) variables.
    let a = atom(
        "p",
        [
            Term::Variable(Variable::Anonymous),
            Term::Variable(Variable::Anonymous),
        ],
    );
    let mut fresh = fresh_over([a.clone()]);
    let renamed = rename_apart(&a, &mut fresh);
    assert_ne!(
        renamed.arguments[0], renamed.arguments[1],
        "each `_` renames apart",
    );
    assert!(matches!(
        renamed.arguments[0],
        Term::Variable(Variable::Named(_)),
    ));
}

#[test]
fn cross_rule_unification_standardizes_apart_first() {
    // p(X) from one rule and p(f(X)) from another: sharing the name X, they fail the occurs
    // check as one namespace; renaming the second apart lets them unify (§11.1).
    let a = atom("p", [tvar("X")]);
    let b = atom("p", [func("f", [tvar("X")])]);
    assert_eq!(mgu(&a, &b), Ok(None));
    let mut fresh = fresh_over([a.clone(), b.clone()]);
    let b_apart = rename_apart(&b, &mut fresh);
    assert!(
        matches!(mgu(&a, &b_apart), Ok(Some(_))),
        "renamed apart, they unify",
    );
}

// ---- iterative in depth (§13) ----

#[test]
fn unifying_deeply_nested_terms_does_not_overflow() {
    // Two terms nesting f(...) 100,000 deep with a variable at the bottom of each: unifying
    // them descends iteratively (§13) and equates the bottom variables — no stack recursion.
    fn nest(bottom: Term, depth: usize) -> Term {
        let mut term = bottom;
        for _ in 0..depth {
            term = Term::Function {
                name: name("f"),
                arguments: vec![term],
            };
        }
        term
    }
    let a = Atom::new(name("p"), [nest(tvar("X"), 100_000)]);
    let b = Atom::new(name("p"), [nest(tvar("Y"), 100_000)]);
    let s = unifier(&a, &b);
    assert_eq!(
        tvar("X").substitute(&s),
        tvar("Y").substitute(&s),
        "the bottom variables are equated",
    );
}

#[test]
fn the_exponential_doubling_case_is_decided_in_compact_triangular_form() {
    // The case unification is infamous for: p(X1, …, Xn) against p(f(X0, X0), f(X1, X1), …,
    // f(X_{n-1}, X_{n-1})) binds Xi ↦ f(X_{i-1}, X_{i-1}), whose *resolved* form doubles at each
    // level (2^n nodes). Martelli–Montanari decides and produces the **triangular** substitution
    // — n small bindings — in near-linear space and time; only a caller that explicitly
    // `substitute`s a high Xi pays the output cost (§11.1). At n = 100 the resolved tree would be
    // 2^100 nodes, so this test completing at all witnesses that the decision never materialises
    // it.
    const N: usize = 100;
    fn xvar(index: usize) -> Term {
        Term::Variable(Variable::Named(
            VarName::new(format!("X{index}")).expect("a valid variable name"),
        ))
    }
    let left: Vec<Term> = (1..=N).map(xvar).collect();
    let right: Vec<Term> = (0..N)
        .map(|index| func("f", [xvar(index), xvar(index)]))
        .collect();
    let s = unifier(&Atom::new(name("p"), left), &Atom::new(name("p"), right));
    // A compact triangular result: one binding per Xi (i = 1..=N); X0 stays free.
    assert_eq!(
        s.iter().count(),
        N,
        "the substitution is n compact bindings, not a doubled tree",
    );
    // The lowest binding resolves in one cheap step (X0 is free): X1 ↦ f(X0, X0).
    assert_eq!(xvar(1).substitute(&s), func("f", [xvar(0), xvar(0)]));
}

#[test]
fn a_deep_ground_symbol_unifies_with_its_non_ground_twin_in_near_linear_time() {
    // p(f(f(…f(a)…))) — a ground symbol 200,000 deep — against p(f(f(…f(X)…))) of the same depth.
    // The ground side is decomposed into the node graph once, so the two descend structurally,
    // near-linearly; a representation that re-cloned the ground symbol one level per step would be
    // Θ(depth²) here and would not finish. Completing at all is the witness that it does not.
    fn nest(bottom: Term, depth: usize) -> Term {
        let mut term = bottom;
        for _ in 0..depth {
            term = func("f", [term]);
        }
        term
    }
    const DEPTH: usize = 200_000;
    let ground = Atom::new(name("p"), [nest(konst("a"), DEPTH)]);
    let non_ground = Atom::new(name("p"), [nest(tvar("X"), DEPTH)]);
    let s = unifier(&ground, &non_ground);
    // The two nests have equal depth, so the f-spines match level by level and X meets the
    // constant `a` at the bottom: the one binding is X ↦ a, a single triangular binding. (The
    // read-out of a *deep* term for one binding — a top variable against a deep nest — is the
    // depth proof's, tests/depth_proof.rs; what this witnesses is that the descent is near-linear.)
    assert_eq!(s.iter().count(), 1, "one binding: X ↦ a");
}

// ---- canonical hard cases from the unification literature ----

#[test]
fn variables_in_mirror_positions_unify() {
    // f(X, a) and f(b, Y): X ↦ b, Y ↦ a — the textbook first case (Robinson).
    let s = unifier(
        &atom("f", [tvar("X"), konst("a")]),
        &atom("f", [konst("b"), tvar("Y")]),
    );
    assert_eq!(tvar("X").substitute(&s), konst("b"));
    assert_eq!(tvar("Y").substitute(&s), konst("a"));
}

#[test]
fn swapped_arguments_equate_the_variables() {
    // f(X, Y) and f(Y, X): X and Y are equated (both resolve to one variable).
    let s = unifier(
        &atom("f", [tvar("X"), tvar("Y")]),
        &atom("f", [tvar("Y"), tvar("X")]),
    );
    assert_eq!(tvar("X").substitute(&s), tvar("Y").substitute(&s));
}

#[test]
fn a_chain_to_a_ground_symbol_binds_the_whole_chain() {
    // g(X, Y, Z) and g(Y, Z, a): X = Y = Z = a, a chain resolved to the ground symbol; the
    // repeated merges exercise the union's rank.
    let s = unifier(
        &atom("g", [tvar("X"), tvar("Y"), tvar("Z")]),
        &atom("g", [tvar("Y"), tvar("Z"), konst("a")]),
    );
    for v in ["X", "Y", "Z"] {
        assert_eq!(tvar(v).substitute(&s), konst("a"), "{v} resolves to a");
    }
}

#[test]
fn a_conflict_through_shared_structure_does_not_unify() {
    // f(X, g(X)) and f(a, g(b)): X must be both a (first argument) and b (under g) — no unifier.
    assert_eq!(
        mgu(
            &atom("f", [tvar("X"), func("g", [tvar("X")])]),
            &atom("f", [konst("a"), func("g", [konst("b")])]),
        ),
        Ok(None),
    );
}

#[test]
fn the_martelli_montanari_example_unifies_triangularly() {
    // p(f(X), Z) and p(Y, g(Y)): Y ↦ f(X), Z ↦ g(Y) — a triangular binding whose resolution
    // gives Z ↦ g(f(X)).
    let s = unifier(
        &atom("p", [func("f", [tvar("X")]), tvar("Z")]),
        &atom("p", [tvar("Y"), func("g", [tvar("Y")])]),
    );
    assert_eq!(tvar("Y").substitute(&s), func("f", [tvar("X")]));
    assert_eq!(
        tvar("Z").substitute(&s),
        func("g", [func("f", [tvar("X")])]),
    );
}

#[test]
fn an_occurs_cycle_through_two_arguments_does_not_unify() {
    // p(X, f(X)) and p(g(Y), Y): X ↦ g(Y) and Y ↦ f(X) forces Y = f(g(Y)) — a cycle, refused.
    assert_eq!(
        mgu(
            &atom("p", [tvar("X"), func("f", [tvar("X")])]),
            &atom("p", [func("g", [tvar("Y")]), tvar("Y")]),
        ),
        Ok(None),
    );
}

#[test]
fn several_variables_collapsing_onto_one_symbol_all_bind() {
    // h(X, X, Y) and h(a, Z, Z): X = a, then Z = X = a, then Y = Z = a — several classes
    // collapsing onto one ground symbol.
    let s = unifier(
        &atom("h", [tvar("X"), tvar("X"), tvar("Y")]),
        &atom("h", [konst("a"), tvar("Z"), tvar("Z")]),
    );
    for v in ["X", "Y", "Z"] {
        assert_eq!(tvar(v).substitute(&s), konst("a"), "{v} resolves to a");
    }
}

#[test]
fn an_anonymous_variable_unifies_with_anything_and_each_is_distinct() {
    // p(_) matches p(f(X)): an anonymous variable unifies with any term. And in p(_, _) against
    // p(1, 2) each `_` is a distinct variable, matching its own argument.
    assert!(matches!(
        mgu(
            &atom("p", [Term::Variable(Variable::Anonymous)]),
            &atom("p", [func("f", [tvar("X")])]),
        ),
        Ok(Some(_)),
    ));
    assert!(matches!(
        mgu(
            &atom(
                "p",
                [
                    Term::Variable(Variable::Anonymous),
                    Term::Variable(Variable::Anonymous),
                ],
            ),
            &atom("p", [num(1), num(2)]),
        ),
        Ok(Some(_)),
    ));
}

#[test]
fn distinct_constructor_kinds_do_not_unify() {
    // A function against a tuple, a term tuple against a ground function symbol, and tuples of
    // different arity — each a constructor clash, Ok(None), not a refusal.
    assert_eq!(
        mgu(
            &atom("p", [func("f", [tvar("X")])]),
            &atom("p", [Term::Tuple(vec![tvar("X"), tvar("Y")])]),
        ),
        Ok(None),
    );
    assert_eq!(
        mgu(
            &atom("p", [Term::Tuple(vec![tvar("X"), tvar("Y")])]),
            &atom("p", [func("g", [konst("a"), konst("b")])]),
        ),
        Ok(None),
    );
    assert_eq!(
        mgu(
            &atom("p", [Term::Tuple(vec![tvar("X"), tvar("Y")])]),
            &atom("p", [Term::Tuple(vec![num(1), num(2), num(3)])]),
        ),
        Ok(None),
    );
}

#[test]
fn a_term_tuple_unifies_with_its_ground_tuple_symbol() {
    // (W, 2) matches the ground tuple (1, 2): the tuple lift, W ↦ 1.
    let s = unifier(
        &atom("p", [Term::Tuple(vec![tvar("W"), num(2)])]),
        &atom("p", [Term::Tuple(vec![num(1), num(2)])]),
    );
    assert_eq!(tvar("W").substitute(&s), num(1));
}
