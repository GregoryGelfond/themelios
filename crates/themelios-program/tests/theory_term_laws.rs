//! Laws of the theory-term peer algebra (docs/design/program.md §4.9, §13, §16): the
//! iterative walks against a naive recursive twin over every variant, their mutual
//! consistency and a total order agreeing with equality, the traversal round-trips, the
//! iterative Debug against a derived twin, and a deep theory term surviving every walk.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use themelios_program::program::{TheoryOperator, TheoryTerm};
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::Variable;

/// The naive recursive reference (§16): obviously correct by inspection, on shallow
/// values. `TheoryTerm`'s order has no external authority (§4.9) — a consistent total
/// order agreeing with `Eq`; the twin and the iterative walk share one length-major
/// scheme (operators, then operand count, then the operands).
mod naive {
    use super::{Ordering, TheoryTerm};

    pub fn eq(a: &TheoryTerm, b: &TheoryTerm) -> bool {
        match (a, b) {
            (TheoryTerm::Symbolic(x), TheoryTerm::Symbolic(y)) => x == y,
            (TheoryTerm::Variable(x), TheoryTerm::Variable(y)) => x == y,
            (
                TheoryTerm::Function {
                    name: n1,
                    arguments: a1,
                },
                TheoryTerm::Function {
                    name: n2,
                    arguments: a2,
                },
            ) => n1 == n2 && slice_eq(a1, a2),
            (TheoryTerm::Tuple(a1), TheoryTerm::Tuple(a2))
            | (TheoryTerm::List(a1), TheoryTerm::List(a2))
            | (TheoryTerm::Set(a1), TheoryTerm::Set(a2)) => slice_eq(a1, a2),
            (
                TheoryTerm::Operation {
                    operators: o1,
                    operands: p1,
                },
                TheoryTerm::Operation {
                    operators: o2,
                    operands: p2,
                },
            ) => o1 == o2 && slice_eq(p1, p2),
            _ => false,
        }
    }

    fn slice_eq(a: &[TheoryTerm], b: &[TheoryTerm]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| eq(x, y))
    }

    fn rank(t: &TheoryTerm) -> u8 {
        match t {
            TheoryTerm::Symbolic(_) => 0,
            TheoryTerm::Variable(_) => 1,
            TheoryTerm::Function { .. } => 2,
            TheoryTerm::Tuple(_) => 3,
            TheoryTerm::List(_) => 4,
            TheoryTerm::Set(_) => 5,
            TheoryTerm::Operation { .. } => 6,
        }
    }

    pub fn cmp(a: &TheoryTerm, b: &TheoryTerm) -> Ordering {
        rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
            (TheoryTerm::Symbolic(x), TheoryTerm::Symbolic(y)) => x.cmp(y),
            (TheoryTerm::Variable(x), TheoryTerm::Variable(y)) => x.cmp(y),
            (
                TheoryTerm::Function {
                    name: n1,
                    arguments: a1,
                },
                TheoryTerm::Function {
                    name: n2,
                    arguments: a2,
                },
            ) => (n1, a1.len())
                .cmp(&(n2, a2.len()))
                .then_with(|| slice_cmp(a1, a2)),
            (TheoryTerm::Tuple(a1), TheoryTerm::Tuple(a2))
            | (TheoryTerm::List(a1), TheoryTerm::List(a2))
            | (TheoryTerm::Set(a1), TheoryTerm::Set(a2)) => {
                a1.len().cmp(&a2.len()).then_with(|| slice_cmp(a1, a2))
            }
            (
                TheoryTerm::Operation {
                    operators: o1,
                    operands: p1,
                },
                TheoryTerm::Operation {
                    operators: o2,
                    operands: p2,
                },
            ) => (o1, p1.len())
                .cmp(&(o2, p2.len()))
                .then_with(|| slice_cmp(p1, p2)),
            _ => Ordering::Equal,
        })
    }

    fn slice_cmp(a: &[TheoryTerm], b: &[TheoryTerm]) -> Ordering {
        for (x, y) in a.iter().zip(b) {
            let c = cmp(x, y);
            if c != Ordering::Equal {
                return c;
            }
        }
        Ordering::Equal
    }
}

/// A structurally identical twin that *derives* `Debug` — the reference the hand-written
/// iterative `Debug` is held against (§14, §16). `PartialEq` is derived and exercised, so
/// the mirror fields are read outside the derived `Debug`.
#[derive(Debug, PartialEq)]
enum DebugTwin {
    Symbolic(Symbol),
    Variable(Variable),
    Function {
        name: Name,
        arguments: Vec<DebugTwin>,
    },
    Tuple(Vec<DebugTwin>),
    List(Vec<DebugTwin>),
    Set(Vec<DebugTwin>),
    Operation {
        operators: Vec<Vec<TheoryOperator>>,
        operands: Vec<DebugTwin>,
    },
}

fn debug_twin(term: &TheoryTerm) -> DebugTwin {
    match term {
        TheoryTerm::Symbolic(s) => DebugTwin::Symbolic(s.clone()),
        TheoryTerm::Variable(v) => DebugTwin::Variable(v.clone()),
        TheoryTerm::Function { name, arguments } => DebugTwin::Function {
            name: name.clone(),
            arguments: arguments.iter().map(debug_twin).collect(),
        },
        TheoryTerm::Tuple(items) => DebugTwin::Tuple(items.iter().map(debug_twin).collect()),
        TheoryTerm::List(items) => DebugTwin::List(items.iter().map(debug_twin).collect()),
        TheoryTerm::Set(items) => DebugTwin::Set(items.iter().map(debug_twin).collect()),
        TheoryTerm::Operation {
            operators,
            operands,
        } => DebugTwin::Operation {
            operators: operators.clone(),
            operands: operands.iter().map(debug_twin).collect(),
        },
    }
}

fn simple_symbol() -> impl Strategy<Value = Symbol> {
    let ident = "[a-z][a-z0-9]{0,2}".prop_filter("not the reserved word", |s| s != "not");
    prop_oneof![
        any::<i32>().prop_map(Symbol::Number),
        "[a-z]{1,3}".prop_map(Symbol::String),
        ident.prop_map(|n| Symbol::Function {
            name: Name::new(n).expect("a lowercase identifier"),
            arguments: vec![],
            sign: themelios_program::symbol::Sign::Positive,
        }),
    ]
}

fn any_variable() -> impl Strategy<Value = Variable> {
    prop_oneof![
        "[A-Z][a-z0-9]{0,2}".prop_map(|v| Variable::Named(VarName::new(v).expect("a variable"))),
        Just(Variable::Anonymous),
    ]
}

fn any_operator_run() -> impl Strategy<Value = Vec<TheoryOperator>> {
    prop::collection::vec("[+*<>=!-]{1,2}".prop_map(TheoryOperator::new), 1..3)
}

fn shallow_theory_term() -> impl Strategy<Value = TheoryTerm> {
    let ident = "[a-z][a-z0-9]{0,2}".prop_filter("not the reserved word", |s| s != "not");
    let leaf = prop_oneof![
        simple_symbol().prop_map(TheoryTerm::Symbolic),
        any_variable().prop_map(TheoryTerm::Variable),
    ];
    leaf.prop_recursive(4, 48, 4, move |inner| {
        prop_oneof![
            (ident.clone(), prop::collection::vec(inner.clone(), 0..4)).prop_map(|(n, a)| {
                TheoryTerm::Function {
                    name: Name::new(n).expect("a lowercase identifier"),
                    arguments: a,
                }
            }),
            prop::collection::vec(inner.clone(), 0..4).prop_map(TheoryTerm::Tuple),
            prop::collection::vec(inner.clone(), 0..4).prop_map(TheoryTerm::List),
            prop::collection::vec(inner.clone(), 0..4).prop_map(TheoryTerm::Set),
            // An operation with one operator run per operand.
            prop::collection::vec((any_operator_run(), inner), 1..4).prop_map(|factors| {
                let (operators, operands) = factors.into_iter().unzip();
                TheoryTerm::Operation {
                    operators,
                    operands,
                }
            }),
        ]
    })
}

proptest! {
    /// The iterative `Eq` and `Ord` agree with the naive twin (§16).
    #[test]
    fn iterative_walks_match_the_naive_twin(a in shallow_theory_term(), b in shallow_theory_term()) {
        prop_assert_eq!(a == b, naive::eq(&a, &b));
        prop_assert_eq!(a.cmp(&b), naive::cmp(&a, &b));
    }

    /// `Ord`, `Eq`, and `Hash` are one content projection (§5.2).
    #[test]
    fn order_equality_and_hash_are_one_projection(a in shallow_theory_term(), b in shallow_theory_term()) {
        prop_assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
        if a == b {
            let mut ha = DefaultHasher::new();
            let mut hb = DefaultHasher::new();
            a.hash(&mut ha);
            b.hash(&mut hb);
            prop_assert_eq!(ha.finish(), hb.finish());
        }
    }

    /// The order is total (§16).
    #[test]
    fn the_order_is_total(a in shallow_theory_term(), b in shallow_theory_term(), c in shallow_theory_term()) {
        prop_assert_eq!(a.cmp(&a), Ordering::Equal);
        prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        if a.cmp(&b) != Ordering::Greater && b.cmp(&c) != Ordering::Greater {
            prop_assert_ne!(a.cmp(&c), Ordering::Greater);
        }
    }

    /// The traversal scheme round-trips (§3.6): `fold(From::from)` and `From<into_parts()>`.
    #[test]
    fn the_traversal_scheme_round_trips(t in shallow_theory_term()) {
        prop_assert_eq!(t.clone().fold(TheoryTerm::from), t.clone());
        prop_assert_eq!(TheoryTerm::from(t.clone().into_parts()), t);
    }

    /// The hand-written iterative `Debug` is byte-identical to a derived one (§14, §16).
    #[test]
    fn the_iterative_debug_matches_a_derived_twin(t in shallow_theory_term()) {
        let twin = debug_twin(&t);
        prop_assert_eq!(&twin, &debug_twin(&t));
        prop_assert_eq!(format!("{t:?}"), format!("{twin:?}"));
    }
}

fn deep_theory_term(depth: usize) -> TheoryTerm {
    let mut term = TheoryTerm::Variable(Variable::Anonymous);
    for _ in 0..depth {
        term = TheoryTerm::Function {
            name: Name::new("f").expect("identifier"),
            arguments: vec![term],
        };
    }
    term
}

#[test]
fn a_deep_theory_term_survives_every_walk_without_overflowing_the_stack() {
    // Far past any real theory term, on the default test stack (§13). The rigorous proof
    // is the depth proof (§16); this is the canary.
    let deep = deep_theory_term(200_000);
    let same = deep.clone();
    let clone_is_equal = deep == same;
    assert!(clone_is_equal); // Eq
    assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
    let mut hasher = DefaultHasher::new();
    deep.hash(&mut hasher); // Hash
    let _ = hasher.finish();
    assert_eq!(deep.subterms().count(), 200_001); // read-traversal
    let _ = format!("{deep:?}"); // Debug
    let rebuilt = deep.clone().fold(TheoryTerm::from); // fold
    let fold_is_equal = rebuilt == same;
    assert!(fold_is_equal);
    drop(deep); // Drop — the whole tree, iteratively
    drop(same);
    drop(rebuilt);
}
