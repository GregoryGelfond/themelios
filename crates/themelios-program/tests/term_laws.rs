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
/// both (§16). Pools carry at least one alternative (grammar §5.1).
fn shallow_term() -> impl Strategy<Value = Term> {
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
            prop::collection::vec(inner.clone(), 1..4).prop_map(Term::Pool),
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
    /// nothing. (It intentionally *merges* distinct spellings — a ground constructor
    /// term and its collapsed symbol, a one-alternative pool and its term — so it does
    /// not preserve structural distinctness; only idempotence and equal-in/equal-out
    /// are laws over terms.)
    #[test]
    fn canonicalization_is_idempotent(t in shallow_term()) {
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
fn unary_minus_of_a_numeral_folds_to_the_negative_number() {
    // The one operator that folds (§5.1): `-5` is the integer −5 (the grammar has no negative
    // numeral), so it canonicalizes to `Number(-5)` and round-trips (§10).
    let negate = |t: Term| Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(t),
    };
    assert_eq!(negate(Term::from(5)).canonicalize(), Term::from(-5));
    // Double negation folds through: -(-5) is 5.
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
fn a_one_alternative_pool_becomes_its_term() {
    // (a) is a, but (a; b) and (a,) are not degenerate (grammar §5.1).
    let inside = Term::Symbolic(Symbol::Number(7));
    let pool = Term::Pool(vec![inside.clone()]);
    assert_eq!(pool.canonicalize(), inside);
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
    // rigorous, stack-controlled proof is the depth gate (§16); this is the canary.
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
