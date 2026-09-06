//! Laws of the non-ground term algebra (docs/design/program.md §3.3, §3.6, §3.7,
//! §5.1, §13, §16): the iterative walks against a naive recursive twin over every
//! variant (boxed and sequence children both), their mutual consistency and a
//! total order agreeing with equality, the traversal round-trips, the ground
//! collapse and pool degeneracy of canonicalization, and a two-shape depth canary.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};

/// The naive recursive reference (§16): obviously correct by inspection, used only
/// on shallow generated terms to hold the iterative walks honest. `Term`'s order
/// has no external authority (unlike `Symbol`'s clingo order) — it need only be a
/// consistent total order agreeing with `Eq`, so the twin and the iterative walk
/// share one length-major scheme (a variant's discriminant, then its head scalars,
/// then its children by count-then-elements), and the mirror proves the iteration
/// faithful to the recursion.
mod naive {
    use super::{Ordering, Term};

    pub fn eq(a: &Term, b: &Term) -> bool {
        match (a, b) {
            (Term::Variable(x), Term::Variable(y)) => x == y,
            (Term::Symbolic(x), Term::Symbolic(y)) => x == y,
            (
                Term::Function {
                    name: n1,
                    arguments: a1,
                },
                Term::Function {
                    name: n2,
                    arguments: a2,
                },
            )
            | (
                Term::External {
                    name: n1,
                    arguments: a1,
                },
                Term::External {
                    name: n2,
                    arguments: a2,
                },
            ) => n1 == n2 && slice_eq(a1, a2),
            (Term::Tuple(a1), Term::Tuple(a2)) | (Term::Pool(a1), Term::Pool(a2)) => {
                slice_eq(a1, a2)
            }
            (
                Term::UnaryOperation {
                    operator: o1,
                    argument: g1,
                },
                Term::UnaryOperation {
                    operator: o2,
                    argument: g2,
                },
            ) => o1 == o2 && eq(g1, g2),
            (
                Term::BinaryOperation {
                    operator: o1,
                    left: l1,
                    right: r1,
                },
                Term::BinaryOperation {
                    operator: o2,
                    left: l2,
                    right: r2,
                },
            ) => o1 == o2 && eq(l1, l2) && eq(r1, r2),
            (
                Term::Interval {
                    lower: lo1,
                    upper: up1,
                },
                Term::Interval {
                    lower: lo2,
                    upper: up2,
                },
            ) => eq(lo1, lo2) && eq(up1, up2),
            (Term::Absolute(t1), Term::Absolute(t2)) => eq(t1, t2),
            _ => false,
        }
    }

    fn slice_eq(a: &[Term], b: &[Term]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| eq(x, y))
    }

    fn rank(t: &Term) -> u8 {
        match t {
            Term::Variable(_) => 0,
            Term::Symbolic(_) => 1,
            Term::Function { .. } => 2,
            Term::Tuple(_) => 3,
            Term::Pool(_) => 4,
            Term::UnaryOperation { .. } => 5,
            Term::BinaryOperation { .. } => 6,
            Term::Interval { .. } => 7,
            Term::Absolute(_) => 8,
            Term::External { .. } => 9,
        }
    }

    pub fn cmp(a: &Term, b: &Term) -> Ordering {
        rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
            (Term::Variable(x), Term::Variable(y)) => x.cmp(y),
            (Term::Symbolic(x), Term::Symbolic(y)) => x.cmp(y),
            (
                Term::Function {
                    name: n1,
                    arguments: a1,
                },
                Term::Function {
                    name: n2,
                    arguments: a2,
                },
            )
            | (
                Term::External {
                    name: n1,
                    arguments: a1,
                },
                Term::External {
                    name: n2,
                    arguments: a2,
                },
            ) => n1.cmp(n2).then_with(|| slice_cmp(a1, a2)),
            (Term::Tuple(a1), Term::Tuple(a2)) | (Term::Pool(a1), Term::Pool(a2)) => {
                slice_cmp(a1, a2)
            }
            (
                Term::UnaryOperation {
                    operator: o1,
                    argument: g1,
                },
                Term::UnaryOperation {
                    operator: o2,
                    argument: g2,
                },
            ) => o1.cmp(o2).then_with(|| cmp(g1, g2)),
            (
                Term::BinaryOperation {
                    operator: o1,
                    left: l1,
                    right: r1,
                },
                Term::BinaryOperation {
                    operator: o2,
                    left: l2,
                    right: r2,
                },
            ) => o1
                .cmp(o2)
                .then_with(|| cmp(l1, l2))
                .then_with(|| cmp(r1, r2)),
            (
                Term::Interval {
                    lower: lo1,
                    upper: up1,
                },
                Term::Interval {
                    lower: lo2,
                    upper: up2,
                },
            ) => cmp(lo1, lo2).then_with(|| cmp(up1, up2)),
            (Term::Absolute(t1), Term::Absolute(t2)) => cmp(t1, t2),
            _ => Ordering::Equal,
        })
    }

    fn slice_cmp(a: &[Term], b: &[Term]) -> Ordering {
        // Length-major: count before elements, so counts differ decides and equal
        // counts descend element-wise — total, agreeing with `Eq`, and free of the
        // mid-walk prefix comparison a derived `Vec` order would need.
        a.len().cmp(&b.len()).then_with(|| {
            for (x, y) in a.iter().zip(b) {
                let c = cmp(x, y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            Ordering::Equal
        })
    }
}

/// A structurally and field-order-identical twin that *derives* `Debug` — the
/// reference the hand-written iterative `Debug` is held against (§14, §16). Only
/// `Term`'s `Debug` is hand-written; `Symbol`, `Variable`, and the operators derive
/// theirs and the twin reuses them. `PartialEq` is derived and exercised below, so
/// the mirror fields are read outside the derived `Debug` (which dead-code analysis
/// does not count) and the oracle's determinism is itself pinned.
#[derive(Debug, PartialEq)]
enum DebugTwin {
    Variable(Variable),
    Symbolic(Symbol),
    Function {
        name: Name,
        arguments: Vec<DebugTwin>,
    },
    Tuple(Vec<DebugTwin>),
    Pool(Vec<DebugTwin>),
    UnaryOperation {
        operator: UnaryOp,
        argument: Box<DebugTwin>,
    },
    BinaryOperation {
        operator: BinaryOp,
        left: Box<DebugTwin>,
        right: Box<DebugTwin>,
    },
    Interval {
        lower: Box<DebugTwin>,
        upper: Box<DebugTwin>,
    },
    Absolute(Box<DebugTwin>),
    External {
        name: Name,
        arguments: Vec<DebugTwin>,
    },
}

fn debug_twin(t: &Term) -> DebugTwin {
    match t {
        Term::Variable(v) => DebugTwin::Variable(v.clone()),
        Term::Symbolic(s) => DebugTwin::Symbolic(s.clone()),
        Term::Function { name, arguments } => DebugTwin::Function {
            name: name.clone(),
            arguments: arguments.iter().map(debug_twin).collect(),
        },
        Term::Tuple(items) => DebugTwin::Tuple(items.iter().map(debug_twin).collect()),
        Term::Pool(items) => DebugTwin::Pool(items.iter().map(debug_twin).collect()),
        Term::UnaryOperation { operator, argument } => DebugTwin::UnaryOperation {
            operator: *operator,
            argument: Box::new(debug_twin(argument)),
        },
        Term::BinaryOperation {
            operator,
            left,
            right,
        } => DebugTwin::BinaryOperation {
            operator: *operator,
            left: Box::new(debug_twin(left)),
            right: Box::new(debug_twin(right)),
        },
        Term::Interval { lower, upper } => DebugTwin::Interval {
            lower: Box::new(debug_twin(lower)),
            upper: Box::new(debug_twin(upper)),
        },
        Term::Absolute(inner) => DebugTwin::Absolute(Box::new(debug_twin(inner))),
        Term::External { name, arguments } => DebugTwin::External {
            name: name.clone(),
            arguments: arguments.iter().map(debug_twin).collect(),
        },
    }
}

fn any_unary_op() -> impl Strategy<Value = UnaryOp> {
    prop_oneof![Just(UnaryOp::Negate), Just(UnaryOp::BitwiseNot)]
}

fn any_binary_op() -> impl Strategy<Value = BinaryOp> {
    prop_oneof![
        Just(BinaryOp::Add),
        Just(BinaryOp::Sub),
        Just(BinaryOp::Mul),
        Just(BinaryOp::Div),
        Just(BinaryOp::Mod),
        Just(BinaryOp::Pow),
        Just(BinaryOp::BitAnd),
        Just(BinaryOp::BitOr),
        Just(BinaryOp::BitXor),
    ]
}

/// A generator of simple ground symbols for `Symbolic` leaves — both spellings of a
/// ground term are exercised by the canonicalization tests, not this generator.
fn simple_symbol() -> impl Strategy<Value = Symbol> {
    let ident = "[a-z][a-z0-9]{0,2}".prop_filter("not the reserved word", |s| s != "not");
    prop_oneof![
        Just(Symbol::Infimum),
        Just(Symbol::Supremum),
        any::<i32>().prop_map(Symbol::Number),
        "[a-z]{1,3}".prop_map(Symbol::String),
        ident.prop_map(|n| Symbol::Function {
            name: Name::new(n).expect("a lowercase identifier"),
            arguments: vec![],
            sign: Sign::Positive,
        }),
    ]
}

fn any_variable() -> impl Strategy<Value = Variable> {
    prop_oneof![
        "[A-Z][a-z0-9]{0,2}".prop_map(|v| Variable::Named(VarName::new(v).expect("a variable"))),
        Just(Variable::Anonymous),
    ]
}

/// A generator of shallow terms drawing every variant, boxed and sequence children
/// both (§16), its pools of `pool_alternatives` many.
fn term_with_pools_of(pool_alternatives: std::ops::Range<usize>) -> impl Strategy<Value = Term> {
    let ident = "[a-z][a-z0-9]{0,2}".prop_filter("not the reserved word", |s| s != "not");
    let leaf = prop_oneof![
        any_variable().prop_map(Term::Variable),
        simple_symbol().prop_map(Term::Symbolic),
    ];
    leaf.prop_recursive(4, 48, 4, move |inner| {
        prop_oneof![
            (ident.clone(), prop::collection::vec(inner.clone(), 0..4)).prop_map(|(n, a)| {
                Term::Function {
                    name: Name::new(n).expect("a lowercase identifier"),
                    arguments: a,
                }
            }),
            prop::collection::vec(inner.clone(), 0..4).prop_map(Term::Tuple),
            prop::collection::vec(inner.clone(), pool_alternatives.clone()).prop_map(Term::Pool),
            (any_unary_op(), inner.clone()).prop_map(|(operator, t)| Term::UnaryOperation {
                operator,
                argument: Box::new(t)
            }),
            (any_binary_op(), inner.clone(), inner.clone()).prop_map(|(operator, l, r)| {
                Term::BinaryOperation {
                    operator,
                    left: Box::new(l),
                    right: Box::new(r),
                }
            }),
            (inner.clone(), inner.clone()).prop_map(|(lo, up)| Term::Interval {
                lower: Box::new(lo),
                upper: Box::new(up)
            }),
            inner.clone().prop_map(|t| Term::Absolute(Box::new(t))),
            (ident.clone(), prop::collection::vec(inner, 0..4)).prop_map(|(n, a)| {
                Term::External {
                    name: Name::new(n).expect("a lowercase identifier"),
                    arguments: a,
                }
            }),
        ]
    })
}

/// The well-formed shallow terms: pools carry at least one alternative (grammar §5.1).
fn shallow_term() -> impl Strategy<Value = Term> {
    term_with_pools_of(1..4)
}

/// The shallow terms admitting the malformed empty pool the constructor door refuses (§5.1,
/// §7.2) — the raw `Pool` variant is open to it, so the pass that repairs a raw term must reach
/// its fixed point over it too.
fn shallow_term_or_empty_pool() -> impl Strategy<Value = Term> {
    term_with_pools_of(0..4)
}

proptest! {
    /// The iterative `Eq` and `Ord` agree with the naive twin on shallow terms (§16).
    #[test]
    fn iterative_walks_match_the_naive_twin(a in shallow_term(), b in shallow_term()) {
        prop_assert_eq!(a == b, naive::eq(&a, &b));
        prop_assert_eq!(a.cmp(&b), naive::cmp(&a, &b));
    }

    /// `Ord`, `Eq`, and `Hash` are one content projection (§5.2).
    #[test]
    fn order_equality_and_hash_are_one_projection(a in shallow_term(), b in shallow_term()) {
        prop_assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
        if a == b {
            let mut ha = DefaultHasher::new();
            let mut hb = DefaultHasher::new();
            a.hash(&mut ha);
            b.hash(&mut hb);
            prop_assert_eq!(ha.finish(), hb.finish());
        }
    }

    /// The order is total: reflexive, antisymmetric, and transitive (§16).
    #[test]
    fn the_order_is_total(a in shallow_term(), b in shallow_term(), c in shallow_term()) {
        prop_assert_eq!(a.cmp(&a), Ordering::Equal);
        prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        if a.cmp(&b) != Ordering::Greater && b.cmp(&c) != Ordering::Greater {
            prop_assert_ne!(a.cmp(&c), Ordering::Greater);
        }
    }

    /// The traversal scheme round-trips (§3.6): `fold(From::from)` and
    /// `From<into_parts()>` are the identity, over boxed and sequence children.
    #[test]
    fn the_traversal_scheme_round_trips(t in shallow_term()) {
        prop_assert_eq!(t.clone().fold(Term::from), t.clone());
        prop_assert_eq!(Term::from(t.clone().into_parts()), t);
    }

    /// The hand-written iterative `Debug` is byte-identical to a derived one (§14, §16).
    #[test]
    fn the_iterative_debug_matches_a_derived_twin(t in shallow_term()) {
        let twin = debug_twin(&t);
        prop_assert_eq!(&twin, &debug_twin(&t));
        prop_assert_eq!(format!("{t:?}"), format!("{twin:?}"));
    }

    /// `From<Symbol>` yields a `Symbolic` leaf (§3.3).
    #[test]
    fn from_symbol_yields_a_symbolic_leaf(s in simple_symbol()) {
        prop_assert_eq!(Term::from(s.clone()), Term::Symbolic(s));
    }

    /// Canonicalization is idempotent and deterministic (§5.1): a second pass changes
    /// nothing — over the malformed empty pool too, whose splicing can expose a form
    /// the fold's rules had already passed; the rows a draw cannot be relied on to hit
    /// are seeded in `a_nested_empty_pool_canonicalizes_in_one_pass`. (It intentionally
    /// *merges* distinct spellings — a ground constructor term and its collapsed symbol,
    /// a one-alternative pool and its term — so it does not preserve structural
    /// distinctness; only idempotence and equal-in/equal-out are laws over terms.)
    #[test]
    fn canonicalization_is_idempotent(t in shallow_term_or_empty_pool()) {
        let once = t.clone().canonicalize();
        prop_assert_eq!(once.clone().canonicalize(), once.clone());
        prop_assert_eq!(t.clone().canonicalize(), t.canonicalize());
    }

    /// `subterms` visits in pre-order — the node before its children (§3.6).
    #[test]
    fn subterms_yields_the_node_before_its_children(t in shallow_term()) {
        // Pre-order: the first subterm is the whole term itself.
        prop_assert_eq!(t.subterms().next(), Some(&t));
    }
}

#[test]
fn a_ground_constructor_term_collapses_to_a_symbol() {
    // f(1, 2) built as a Function over Symbolic numbers collapses to Symbolic(f(1, 2)).
    let term = Term::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![
            Term::Symbolic(Symbol::Number(1)),
            Term::Symbolic(Symbol::Number(2)),
        ],
    };
    let expected = Term::Symbolic(Symbol::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![Symbol::Number(1), Symbol::Number(2)],
        sign: Sign::Positive,
    });
    assert_eq!(term.canonicalize(), expected);
}

#[test]
fn nested_ground_constructors_collapse_maximally() {
    // f(g(1)) collapses to Symbolic(f(g(1))) — the collapse is bottom-up and maximal.
    let term = Term::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![Term::Function {
            name: Name::new("g").expect("identifier"),
            arguments: vec![Term::Symbolic(Symbol::Number(1))],
        }],
    };
    let expected = Term::Symbolic(Symbol::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![Symbol::Function {
            name: Name::new("g").expect("identifier"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        }],
        sign: Sign::Positive,
    });
    assert_eq!(term.canonicalize(), expected);
}

#[test]
fn a_ground_operator_term_does_not_fold() {
    // 1 + 2 stays a BinaryOperation even though it is ground — the grounder's to evaluate.
    let sum = Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(Term::Symbolic(Symbol::Number(1))),
        right: Box::new(Term::Symbolic(Symbol::Number(2))),
    };
    assert_eq!(sum.clone().canonicalize(), sum);
}

#[test]
fn unary_minus_of_a_number_folds_to_its_negation() {
    // The one operator that folds (§5.1): `-5` is the integer −5 (the grammar has no negative
    // numeral), so it canonicalizes to `Number(-5)` and round-trips (§10).
    let negate = |t: Term| Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(t),
    };
    assert_eq!(negate(Term::from(5)).canonicalize(), Term::from(-5));
    // Double negation folds through to the positive number — the authority's reading (clingo
    // grounds `- -5` to `5`): -(-5) is 5.
    assert_eq!(negate(Term::from(-5)).canonicalize(), Term::from(5));
    // `-(1 + 2)` is not a numeral and stays a `BinaryOperation` — only the numeral folds.
    let one_plus_two = Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(Term::from(1)),
        right: Box::new(Term::from(2)),
    };
    assert!(matches!(
        negate(one_plus_two).canonicalize(),
        Term::UnaryOperation { .. }
    ));
    // `i32::MIN` has no numeral spelling (its negation leaves range), so it keeps the Negate form.
    assert!(matches!(
        negate(Term::from(i32::MIN)).canonicalize(),
        Term::UnaryOperation { .. }
    ));
}

#[test]
fn the_operator_doors_yield_the_deep_canonical_form() {
    // Each operator door canonicalizes one level, assuming canonical operands (§5.1, §7.1), and
    // yields the very value the deep pass yields over the same raw term. Unary minus is the one
    // door that folds: `-5` is `Number(-5)`, and negating that folds through to `Number(5)` —
    // the authority's reading of `- -5`. A composed `X + 1` is the deep-canonicalized twin of the
    // raw `BinaryOperation`.
    let x = || Term::Variable(Variable::Named(VarName::new("X").expect("variable")));
    let negate = |t: Term| Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(t),
    };
    let minus_five = -Term::from(5);
    assert_eq!(minus_five, Term::from(-5));
    assert_eq!(minus_five, negate(Term::from(5)).canonicalize());
    let minus_minus_five = -minus_five;
    assert_eq!(minus_minus_five, Term::from(5));
    assert_eq!(
        minus_minus_five,
        negate(negate(Term::from(5))).canonicalize()
    );
    let sum = Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(x()),
        right: Box::new(Term::from(1)),
    };
    assert_eq!(x() + 1, sum.canonicalize());
}

// ---- The value constructors (§7.1): total, canonical by the one-level step (§5.1, §7.2) ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn var_name(text: &str) -> VarName {
    VarName::new(text).expect("a valid variable name")
}

#[test]
fn a_function_through_the_constructor_collapses_exactly_when_ground() {
    // `Term::function` canonicalizes one level at the door (§5.1, §7.1): all-ground arguments
    // collapse the application to its symbol, a term-position functor bearing no strong sign
    // (§3.3); one variable argument keeps it a `Term::Function`.
    assert_eq!(
        Term::function(name("f"), [Term::from(1)]),
        Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })
    );
    assert_eq!(
        Term::function(name("f"), [Term::variable(var_name("X"))]),
        Term::Function {
            name: name("f"),
            arguments: vec![Term::Variable(Variable::Named(var_name("X")))],
        }
    );
}

#[test]
fn a_constant_is_the_collapsed_empty_argument_function() {
    // A constant is the empty-argument function (§3.1), and a nullary ground function collapses
    // to its symbol (§5.1), so `Term::constant` is that symbol directly — never the raw
    // `Term::Function` over no arguments, the non-canonical spelling — and is exactly
    // `Term::function` over no arguments.
    let constant = Term::constant(name("c"));
    assert_eq!(
        constant,
        Term::Symbolic(Symbol::Function {
            name: name("c"),
            arguments: Vec::new(),
            sign: Sign::Positive,
        })
    );
    assert_eq!(constant, Term::function(name("c"), []));
}

#[test]
fn the_variable_constructors_build_the_variable_leaves() {
    // A leaf is canonical by construction (§5.1), so `variable` and `anonymous` build it directly.
    assert_eq!(
        Term::variable(var_name("X")),
        Term::Variable(Variable::Named(var_name("X")))
    );
    assert_eq!(Term::anonymous(), Term::Variable(Variable::Anonymous));
}

#[test]
fn a_tuple_through_the_constructor_collapses_exactly_when_ground() {
    // As `function` (§5.1): all-ground elements collapse to the tuple symbol — the empty tuple
    // included, ground over no elements — and one variable element keeps it a `Term::Tuple`.
    assert_eq!(
        Term::tuple([Term::from(1), Term::from(2)]),
        Term::Symbolic(Symbol::Tuple(vec![Symbol::Number(1), Symbol::Number(2)]))
    );
    assert_eq!(Term::tuple([]), Term::Symbolic(Symbol::Tuple(Vec::new())));
    assert_eq!(
        Term::tuple([Term::variable(var_name("X"))]),
        Term::Tuple(vec![Term::Variable(Variable::Named(var_name("X")))])
    );
}

#[test]
fn a_pool_through_the_door_is_the_deep_pass_over_canonical_alternatives() {
    // `Term::pool` canonicalizes one level at the door, assuming canonical alternatives (§5.1,
    // §7.2), as `function` and `tuple` do their children — so over canonical alternatives it is
    // exactly the deep pass over the same raw pool: two or more flat alternatives are kept; one
    // alternative collapses to its term; a flat pool among the alternatives is spliced in place,
    // one level of splicing being the whole flattening; and a deep canonical compound alternative
    // is kept as an alternative, not re-walked (the O(depth) claim tests/scaling_shape.rs holds).
    let x = || Term::variable(var_name("X"));
    let one_two = || Term::Pool(vec![Term::from(1), Term::from(2)]);
    let deep = || (0..64).fold(x(), |inner, _| Term::function(name("f"), [inner]));
    let rows = [
        (
            vec![Term::from(1), x()],
            Term::Pool(vec![Term::from(1), x()]),
        ),
        (vec![x()], x()),
        (
            vec![one_two(), x()],
            Term::Pool(vec![Term::from(1), Term::from(2), x()]),
        ),
        (
            vec![deep(), Term::from(0)],
            Term::Pool(vec![deep(), Term::from(0)]),
        ),
    ];
    for (alternatives, canonical) in rows {
        // The fixture is honest: every alternative is a fixed point of the deep pass.
        for alternative in &alternatives {
            assert_eq!(
                alternative.clone().canonicalize(),
                *alternative,
                "an alternative of {alternatives:?} is canonical"
            );
        }
        assert_eq!(
            Term::pool(alternatives.clone()).expect("a non-empty pool"),
            canonical,
            "the door over {alternatives:?}"
        );
        assert_eq!(
            Term::Pool(alternatives.clone()).canonicalize(),
            canonical,
            "the deep pass over {alternatives:?}"
        );
    }
}

#[test]
fn constructors_composed_bottom_up_yield_the_deep_canonical_form() {
    // Each constructor assumes canonical children (§7.2), and a value built through the
    // constructors has them at every step, so a nest composed bottom-up is the very value the
    // deep pass yields over the same raw term: a ground nest collapses maximally to one symbol;
    // a variable at the bottom keeps every application above it a `Term::Function`; a tuple over
    // a collapsed function and a variable is the same twin.
    let x = || Term::variable(var_name("X"));
    let raw_nest = |bottom: Term| Term::Function {
        name: name("f"),
        arguments: vec![Term::Function {
            name: name("g"),
            arguments: vec![bottom],
        }],
    };
    let built_nest =
        |bottom: Term| Term::function(name("f"), [Term::function(name("g"), [bottom])]);
    assert_eq!(
        built_nest(Term::from(1)),
        raw_nest(Term::from(1)).canonicalize()
    );
    assert!(matches!(built_nest(Term::from(1)), Term::Symbolic(_)));
    assert_eq!(built_nest(x()), raw_nest(x()).canonicalize());
    assert!(matches!(built_nest(x()), Term::Function { .. }));
    let raw_tuple = Term::Tuple(vec![
        Term::Function {
            name: name("f"),
            arguments: vec![Term::from(1)],
        },
        x(),
    ]);
    assert_eq!(
        Term::tuple([Term::function(name("f"), [Term::from(1)]), x()]),
        raw_tuple.canonicalize()
    );
}

#[test]
fn every_value_constructor_yields_a_canonical_value() {
    // Canonical is a fixed point of the deep pass (§5.1): each constructor's value, the folding
    // and the kept branch of each rule both, is unchanged by `canonicalize`.
    let x = || Term::variable(var_name("X"));
    let values = [
        Term::function(name("f"), [Term::from(1)]),
        Term::function(name("f"), [x()]),
        Term::constant(name("c")),
        x(),
        Term::anonymous(),
        Term::tuple([Term::from(1), Term::from(2)]),
        Term::tuple([x()]),
        Term::pool([Term::from(1), x()]).expect("a non-empty pool"),
        Term::pool([x()]).expect("a non-empty pool"),
    ];
    for value in values {
        assert_eq!(
            value.clone().canonicalize(),
            value,
            "{value:?} is a fixed point of the deep pass"
        );
    }
}

#[test]
fn a_value_constructor_keeps_a_raw_child_for_the_deep_pass_to_repair() {
    // The ruled semantic (§7.2): a constructor canonicalizes one level and does not descend, so a
    // raw non-canonical child a caller hand-builds — a ground `g(1)` never collapsed — is kept as
    // given under `Term::function`, whose argument list is then not all-`Symbolic`. The whole-value
    // repair is the deep pass the atom, ingest, and statement doors run on entry (§5.1), which
    // collapses the same value maximally.
    let raw_child = || Term::Function {
        name: name("g"),
        arguments: vec![Term::from(1)],
    };
    let kept = Term::function(name("f"), [raw_child()]);
    assert_eq!(
        kept,
        Term::Function {
            name: name("f"),
            arguments: vec![raw_child()],
        }
    );
    assert_eq!(
        kept.canonicalize(),
        Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Function {
                name: name("g"),
                arguments: vec![Symbol::Number(1)],
                sign: Sign::Positive,
            }],
            sign: Sign::Positive,
        })
    );
}

#[test]
fn a_one_alternative_pool_becomes_its_term() {
    // (a) is a, but (a; b) and (a,) are not degenerate (grammar §5.1).
    let inside = Term::Symbolic(Symbol::Number(7));
    let pool = Term::Pool(vec![inside.clone()]);
    assert_eq!(pool.canonicalize(), inside);
}

#[test]
fn a_nested_pool_is_flattened_through_a_compound_node() {
    // The flatten is one top-down pass that enters every compound node (§5.1): a nested pool
    // below a function is reached through the function — `f(((1; 2); 3))` is `f((1; 2; 3))` —
    // and a compound alternative of a nested pool is entered in turn, its own nested pool
    // flattened — `((f(((1; 2); 3)); 4); 5)` is `(f((1; 2; 3)); 4; 5)`. A pool under a function
    // is no ground symbol (a pool is a set-former), so the function is kept, not collapsed.
    // Pinned directly: the generated laws above reach this re-entry only when a draw happens to
    // nest a pool under a compound node.
    let n = |value: i32| Term::from(value);
    let nested = || Term::Pool(vec![Term::Pool(vec![n(1), n(2)]), n(3)]);
    let flat = || Term::Pool(vec![n(1), n(2), n(3)]);
    let f = |argument: Term| Term::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![argument],
    };
    assert_eq!(f(nested()).canonicalize(), f(flat()));
    let outer = Term::Pool(vec![Term::Pool(vec![f(nested()), n(4)]), n(5)]);
    assert_eq!(
        outer.canonicalize(),
        Term::Pool(vec![f(flat()), n(4), n(5)])
    );
}

#[test]
fn a_nested_empty_pool_canonicalizes_in_one_pass() {
    // The raw `Pool` variant is open to the malformed empty pool the constructor door refuses
    // (§5.1, §7.2). Splicing one out of a pool contributes no alternative, which can leave behind
    // a one-alternative pool, a constructor whose arguments are now all ground, or a negation
    // whose operand is now a number — each a form the fold's rules had already passed, since the
    // flatten runs after the fold. The deep pass is idempotent over every term (§5.1), so each
    // reaches its fixed point in one pass; the generated law holds this over its draws, and these
    // are the rows no draw can be relied on to hit.
    let x = || Term::Variable(Variable::Named(VarName::new("X").expect("variable")));
    let empty = || Term::Pool(vec![]);
    let f = |functor: &str, argument: Term| Term::Function {
        name: Name::new(functor).expect("identifier"),
        arguments: vec![argument],
    };
    let ground = |functor: &str, argument: Symbol| Symbol::Function {
        name: Name::new(functor).expect("identifier"),
        arguments: vec![argument],
        sign: Sign::Positive,
    };
    let rows = [
        // A singleton exposed: the one alternative left is the term.
        (Term::Pool(vec![empty(), x()]), x()),
        // A ground constructor exposed: the alternative left makes `f` all-ground.
        (
            f("f", Term::Pool(vec![empty(), Term::from(1)])),
            Term::Symbolic(ground("f", Symbol::Number(1))),
        ),
        // Exposed two levels up: each node assembled sees its children already repaired.
        (
            f("f", f("g", Term::Pool(vec![empty(), Term::from(1)]))),
            Term::Symbolic(ground("f", ground("g", Symbol::Number(1)))),
        ),
        // A foldable negation exposed: unary minus of the number left is its negation.
        (
            Term::UnaryOperation {
                operator: UnaryOp::Negate,
                argument: Box::new(Term::Pool(vec![empty(), Term::from(5)])),
            },
            Term::from(-5),
        ),
        // Every alternative spliced out: the empty pool is kept, malformed, and stable.
        (Term::Pool(vec![empty(), empty()]), empty()),
    ];
    for (term, fixed_point) in rows {
        let once = term.clone().canonicalize();
        assert_eq!(once, fixed_point, "one pass over {term:?}");
        assert_eq!(
            once.clone().canonicalize(),
            once,
            "a second pass over {term:?}"
        );
    }
}

#[test]
fn a_tuple_is_kept_a_tuple_never_collapsed_to_its_element() {
    // A one-alternative pool collapses to its term, but a one-element tuple does not
    // (grammar §5.1 makes them distinct terms): a ground one-element tuple collapses
    // to a Symbol *tuple*, never to the element's symbol.
    let one = Term::Tuple(vec![Term::Symbolic(Symbol::Number(7))]);
    let expected = Term::Symbolic(Symbol::Tuple(vec![Symbol::Number(7)]));
    assert_eq!(one.canonicalize(), expected);
    // The empty tuple is ground too, so it collapses to the empty-tuple symbol —
    // kept a tuple, not eliminated.
    let empty = Term::Tuple(vec![]);
    assert_eq!(empty.canonicalize(), Term::Symbolic(Symbol::Tuple(vec![])));
    // A non-ground tuple has nothing to collapse and stays a `Term::Tuple`.
    let non_ground = Term::Tuple(vec![Term::Variable(Variable::Anonymous)]);
    assert_eq!(non_ground.clone().canonicalize(), non_ground);
}

#[test]
fn the_point_accessors_read_each_applied_form() {
    let x = || Term::Variable(Variable::Named(VarName::new("X").expect("variable")));
    let n = |t: &str| Name::new(t).expect("identifier");
    // A function reads its name, its arguments, and its arity; a two-argument functor
    // pins arity to 2 — neither 0 nor a constant 1.
    let f = Term::Function {
        name: n("f"),
        arguments: vec![Term::from(1), x()],
    };
    assert_eq!(f.name(), Some(&n("f")));
    assert_eq!(f.arguments(), &[Term::from(1), x()]);
    assert_eq!(f.arity(), 2);
    assert!(
        !f.is_ground(),
        "f(1, X) has a variable, so it is not ground"
    );
    // A tuple has no functor name but reads its elements as its arguments.
    let tuple = Term::Tuple(vec![Term::from(1), Term::from(2)]);
    assert_eq!(tuple.name(), None);
    assert_eq!(tuple.arguments(), &[Term::from(1), Term::from(2)]);
    // A ground function is ground; a bare variable is a non-applied form.
    assert!(
        Term::Function {
            name: n("g"),
            arguments: vec![Term::from(1)],
        }
        .is_ground()
    );
    assert_eq!(x().name(), None);
    assert!(x().arguments().is_empty());
    assert_eq!(x().arity(), 0);
}

#[test]
fn structural_equality_separates_functor_arity_operator_and_call() {
    let x = || Term::Variable(Variable::Named(VarName::new("X").expect("variable")));
    let n = |t: &str| Name::new(t).expect("identifier");
    let f = |functor: &str, arguments: Vec<Term>| Term::Function {
        name: n(functor),
        arguments,
    };
    // A shared functor name is not equality: a name match with a different arity, or a
    // matching arity under a different name, are both unequal (§3.3).
    assert_ne!(
        f("p", vec![Term::from(1)]),
        f("p", vec![Term::from(1), Term::from(2)])
    );
    assert_ne!(f("p", vec![Term::from(1)]), f("q", vec![Term::from(1)]));
    // Two unary operations over one argument differ by operator alone: -X ≠ ~X.
    let unary = |op| Term::UnaryOperation {
        operator: op,
        argument: Box::new(x()),
    };
    assert_ne!(unary(UnaryOp::Negate), unary(UnaryOp::BitwiseNot));
    // Two binary operations over one pair differ by operator alone: X + 1 ≠ X - 1.
    let binary = |op| Term::BinaryOperation {
        operator: op,
        left: Box::new(x()),
        right: Box::new(Term::from(1)),
    };
    assert_ne!(binary(BinaryOp::Add), binary(BinaryOp::Sub));
    // An @-call separates on arity as a function does: @f(1) ≠ @f(1, 2).
    let call = |arguments: Vec<Term>| Term::External {
        name: n("f"),
        arguments,
    };
    assert_ne!(
        call(vec![Term::from(1)]),
        call(vec![Term::from(1), Term::from(2)])
    );
}

/// A left-nested function `f(f(… 0 …))` of `depth` levels — all ground constructor
/// terms, so canonicalization collapses the whole spine to one deep `Symbolic`.
fn deep_via_function(depth: usize) -> Term {
    let mut term = Term::Symbolic(Symbol::Number(0));
    for _ in 0..depth {
        term = Term::Function {
            name: Name::new("f").expect("identifier"),
            arguments: vec![term],
        };
    }
    term
}

/// A nested unary operation `-(-(… _ …))` of `depth` levels — boxed children over a
/// variable, so it is non-ground and canonicalization rebuilds it unchanged.
fn deep_via_unary(depth: usize) -> Term {
    let mut term = Term::Variable(Variable::Anonymous);
    for _ in 0..depth {
        term = Term::UnaryOperation {
            operator: UnaryOp::Negate,
            argument: Box::new(term),
        };
    }
    term
}

#[test]
fn deep_terms_survive_every_walk_without_overflowing_the_stack() {
    // Far past any real term, on the default test stack: sequence children (nested
    // Function) and boxed children (nested UnaryOperation) both, each ~200,000 deep.
    // An iterative walk handles them; a recursive one would overflow here (§13). The
    // rigorous, stack-controlled proof is the depth proof (§16); this is the canary.
    let functiony = deep_via_function(200_000);
    let unary = deep_via_unary(200_000);
    for deep in [&functiony, &unary] {
        let same = deep.clone();
        // Through a bound bool, so a never-taken failure does not Debug-render this deep.
        let clone_is_equal = *deep == same;
        assert!(clone_is_equal); // Eq
        assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
        let mut hasher = DefaultHasher::new();
        deep.hash(&mut hasher); // Hash
        let _ = hasher.finish();
        let _ = format!("{deep:?}"); // Debug
        let _ = deep.subterms().count(); // read-traversal
    }
    // Canonicalize both: the ground spine collapses to one deep Symbolic, the unary
    // spine rebuilds unchanged — both walks (and the deep symbol's own) stay iterative.
    let collapsed = functiony.clone().canonicalize();
    assert!(matches!(collapsed, Term::Symbolic(_)));
    let unchanged = unary.clone().canonicalize();
    let unary_is_unchanged = unchanged == unary;
    assert!(unary_is_unchanged);
    drop(functiony); // Drop — the whole tree, iteratively
    drop(unary);
    drop(collapsed);
}
