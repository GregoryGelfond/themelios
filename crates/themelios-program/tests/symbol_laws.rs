//! Laws of the ground vocabulary (docs/design/program.md §3, §5, §13, §16):
//! the name classifier, the traversal round-trips, the iterative walks against
//! a naive recursive twin, their mutual consistency, and a depth canary that
//! exercises every walk on a value far deeper than the call stack could bear.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use themelios_program::symbol::{FromSymbol, Name, Sign, Symbol, ToSymbol, VarName};

#[test]
fn names_are_exactly_the_grammar_s_identifier_and_variable_classes() {
    // Identifiers: lowercase-led, primed and underscore-led admitted (grammar §4.2).
    assert!(Name::new("edge").is_ok());
    assert!(Name::new("_p").is_ok());
    assert!(Name::new("a'").is_ok());
    // Not identifiers: a variable, the reserved `not`, the lone `_`, empty,
    // trailing text, a non-name character.
    assert_eq!(Name::new("X").unwrap_err().text, "X");
    assert!(Name::new("not").is_err());
    assert!(Name::new("_").is_err());
    assert!(Name::new("").is_err());
    assert!(Name::new("p q").is_err());
    assert!(Name::new("p.").is_err());
    assert!(Name::new("p(1)").is_err());
    // Variables: uppercase-led (grammar §4.2).
    assert!(VarName::new("X").is_ok());
    assert!(VarName::new("_X").is_ok());
    assert_eq!(VarName::new("p").unwrap_err().text, "p");
    assert!(VarName::new("_").is_err());
}

/// The naive recursive reference (§16): obviously correct by inspection, used
/// only on shallow generated values to hold the iterative walks honest. It
/// implements §3.1's stated **total** order — a tuple ordered as an anonymous
/// function, so functions and tuples interleave and no distinct pair ties.
/// Because the twin shares that order with the iterative walk, the mirror
/// differential proves the *iteration* faithful but cannot witness an
/// order-totality defect (a shared `_ => Equal` gap would agree in being wrong);
/// the one-projection law (`Eq` vs `Ord`) and the authority differential (§16)
/// witness totality and the exact order versus clingo.
mod naive {
    use super::{Name, Ordering, Sign, Symbol};

    pub fn eq(a: &Symbol, b: &Symbol) -> bool {
        match (a, b) {
            (Symbol::Infimum, Symbol::Infimum) | (Symbol::Supremum, Symbol::Supremum) => true,
            (Symbol::Number(x), Symbol::Number(y)) => x == y,
            (Symbol::String(x), Symbol::String(y)) => x == y,
            (
                Symbol::Function {
                    name: n1,
                    arguments: a1,
                    sign: s1,
                },
                Symbol::Function {
                    name: n2,
                    arguments: a2,
                    sign: s2,
                },
            ) => {
                s1 == s2
                    && n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|(x, y)| eq(x, y))
            }
            (Symbol::Tuple(x), Symbol::Tuple(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(u, v)| eq(u, v))
            }
            _ => false,
        }
    }

    fn rank(s: &Symbol) -> u8 {
        match s {
            Symbol::Infimum => 0,
            Symbol::Number(_) => 1,
            Symbol::Function { arguments, .. } if arguments.is_empty() => 2,
            Symbol::Tuple(e) if e.is_empty() => 2,
            Symbol::String(_) => 3,
            Symbol::Function { .. } | Symbol::Tuple(_) => 4,
            Symbol::Supremum => 5,
        }
    }

    pub fn cmp(a: &Symbol, b: &Symbol) -> Ordering {
        rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
            (Symbol::Number(x), Symbol::Number(y)) => x.cmp(y),
            (Symbol::String(x), Symbol::String(y)) => x.cmp(y),
            // A function and/or a tuple, a tuple ordered as a positive anonymous
            // function (§3.1): compare the (sign, arity, name) head with a tuple's
            // name anonymous, then the arguments — total, so a function and a
            // same-arity tuple never tie, and the mixed case never falls to `Equal`.
            (
                Symbol::Function { .. } | Symbol::Tuple(_),
                Symbol::Function { .. } | Symbol::Tuple(_),
            ) => head(a)
                .cmp(&head(b))
                .then_with(|| lexicographic(a.arguments(), b.arguments())),
            _ => Ordering::Equal,
        })
    }

    /// The function-like head (§3.1): sign as a rank, then arity, then name. A tuple
    /// is a positive anonymous function — sign positive, name `None` — so it
    /// interleaves among the positive functions of its arity, its `None` name never
    /// tying a function's `Some`.
    fn head(s: &Symbol) -> (u8, usize, Option<&Name>) {
        match s {
            Symbol::Function {
                name,
                arguments,
                sign,
            } => (sign_rank(*sign), arguments.len(), Some(name)),
            Symbol::Tuple(e) => (sign_rank(Sign::Positive), e.len(), None),
            _ => (sign_rank(Sign::Positive), 0, None),
        }
    }

    fn sign_rank(sign: Sign) -> u8 {
        match sign {
            Sign::Positive => 0,
            Sign::Negative => 1,
        }
    }

    fn lexicographic(a: &[Symbol], b: &[Symbol]) -> Ordering {
        for (x, y) in a.iter().zip(b) {
            let c = cmp(x, y);
            if c != Ordering::Equal {
                return c;
            }
        }
        Ordering::Equal
    }
}

/// A structurally and field-order-identical twin that *derives* `Debug` — the
/// reference the hand-written iterative `Debug` is held against (§14, §16). `Name`
/// and `Sign` derive their own `Debug`, so the twin reuses them; only `Symbol`'s
/// `Debug` is hand-written, so only its shape is under test. "Derived-shaped" means
/// byte-equal to this. `PartialEq` is derived and exercised in the law below, so
/// the mirror fields are read outside the derived `Debug` (which dead-code analysis
/// does not count) and the oracle's determinism is itself pinned.
#[derive(Debug, PartialEq)]
enum DebugTwin {
    Infimum,
    Number(i32),
    String(String),
    Function {
        name: Name,
        arguments: Vec<DebugTwin>,
        sign: Sign,
    },
    Tuple(Vec<DebugTwin>),
    Supremum,
}

/// Map a symbol to its derived-Debug twin (naive recursion — a test oracle on
/// shallow values, §16).
fn debug_twin(symbol: &Symbol) -> DebugTwin {
    match symbol {
        Symbol::Infimum => DebugTwin::Infimum,
        Symbol::Number(n) => DebugTwin::Number(*n),
        Symbol::String(s) => DebugTwin::String(s.clone()),
        Symbol::Function {
            name,
            arguments,
            sign,
        } => DebugTwin::Function {
            name: name.clone(),
            arguments: arguments.iter().map(debug_twin).collect(),
            sign: *sign,
        },
        Symbol::Tuple(elements) => DebugTwin::Tuple(elements.iter().map(debug_twin).collect()),
        Symbol::Supremum => DebugTwin::Supremum,
    }
}

/// A generator of shallow symbols (depth-bounded) — the domain the naive twin
/// covers; it draws both nullary and arity-bearing functions and both signs.
fn shallow_symbol() -> impl Strategy<Value = Symbol> {
    // Identifiers that are never the reserved `not` (grammar §4.5).
    let ident = "[a-z][a-z0-9]{0,3}".prop_filter("not the reserved word", |s| s != "not");
    let leaf = prop_oneof![
        Just(Symbol::Infimum),
        Just(Symbol::Supremum),
        any::<i32>().prop_map(Symbol::Number),
        "[a-z][a-z0-9]{0,3}".prop_map(Symbol::String),
        ident.clone().prop_map(|n| Symbol::Function {
            name: Name::new(n).expect("a lowercase identifier"),
            arguments: vec![],
            sign: Sign::Positive,
        }),
    ];
    leaf.prop_recursive(4, 32, 4, move |inner| {
        prop_oneof![
            (
                ident.clone(),
                prop::collection::vec(inner.clone(), 0..4),
                prop_oneof![Just(Sign::Positive), Just(Sign::Negative)],
            )
                .prop_map(|(n, arguments, sign)| Symbol::Function {
                    name: Name::new(n).expect("a lowercase identifier"),
                    arguments,
                    sign,
                }),
            prop::collection::vec(inner, 0..4).prop_map(Symbol::Tuple),
        ]
    })
}

proptest! {
    /// The iterative `Eq` and `Ord` agree with the naive twin on shallow values (§16).
    #[test]
    fn iterative_walks_match_the_naive_twin(a in shallow_symbol(), b in shallow_symbol()) {
        prop_assert_eq!(a == b, naive::eq(&a, &b));
        prop_assert_eq!(a.cmp(&b), naive::cmp(&a, &b));
    }

    /// `Ord`, `Eq`, and `Hash` are one content projection (§5.2): equality
    /// coincides with `Ordering::Equal`, and equal values hash equal.
    #[test]
    fn order_equality_and_hash_are_one_projection(a in shallow_symbol(), b in shallow_symbol()) {
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
    fn the_order_is_total(a in shallow_symbol(), b in shallow_symbol(), c in shallow_symbol()) {
        prop_assert_eq!(a.cmp(&a), Ordering::Equal);
        prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        if a.cmp(&b) != Ordering::Greater && b.cmp(&c) != Ordering::Greater {
            prop_assert_ne!(a.cmp(&c), Ordering::Greater);
        }
    }

    /// The traversal scheme round-trips (§3.6): `fold(From::from)` and
    /// `From<into_parts()>` are the identity.
    #[test]
    fn the_traversal_scheme_round_trips(s in shallow_symbol()) {
        prop_assert_eq!(s.clone().fold(Symbol::from), s.clone());
        prop_assert_eq!(Symbol::from(s.clone().into_parts()), s);
    }

    /// The hand-written iterative `Debug` is byte-identical to a derived one on
    /// shallow values (§14, §16): "derived-shaped" made checkable, so a field
    /// emitted out of declaration order is caught the moment it lands.
    #[test]
    fn the_iterative_debug_matches_a_derived_twin(s in shallow_symbol()) {
        let twin = debug_twin(&s);
        // The oracle is deterministic — a structural read of every mirror field,
        // so the twin's fields are live beyond the derived `Debug` this law reads.
        prop_assert_eq!(&twin, &debug_twin(&s));
        prop_assert_eq!(format!("{s:?}"), format!("{twin:?}"));
    }
}

#[test]
fn arity_counts_arguments_and_two_strings_order_by_content() {
    // A two-argument function has arity 2 — neither 0 nor a constant 1 (§3.2).
    let f = Symbol::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![Symbol::Number(1), Symbol::Number(2)],
        sign: Sign::Positive,
    };
    assert_eq!(f.arity(), 2);
    assert_eq!(Symbol::Number(0).arity(), 0, "an atomic symbol has arity 0");
    // Two strings at the one String rank order by their content, not as equal (§3.1):
    // a dropped String arm in the order would tie every pair of strings.
    assert!(Symbol::String("apple".to_owned()) < Symbol::String("banana".to_owned()));
    assert_ne!(
        Symbol::String("a".to_owned()),
        Symbol::String("b".to_owned())
    );
}

#[test]
fn the_conversion_pillar_is_symmetric_on_strings() {
    // `str` and `String` denote a `Symbol::String` — the inward mirror of
    // `FromSymbol for String` (§3.4), so the pillar is a special-case-free target
    // for the extraction expansions and a codec's string field.
    assert_eq!("edge".to_symbol(), Symbol::String("edge".to_owned()));
    assert_eq!(
        String::from("a b").to_symbol(),
        Symbol::String("a b".to_owned()),
    );
    // The round-trip through the inverse holds.
    assert_eq!(
        String::from_symbol(&"reachable".to_symbol()),
        Ok("reachable".to_owned()),
    );
    // A string denotes a string, never a constant: a `Symbol::Function` still
    // comes only from a `Name`, so nothing is guessed.
    assert!(matches!("p".to_symbol(), Symbol::String(_)));
}

/// A left-nested function `f(f(f(… c …)))` of `depth` levels — the shape a
/// recursive construction produces (§13), built iteratively so the test does not
/// overflow building it either.
fn deep_symbol(depth: usize) -> Symbol {
    let mut symbol = Symbol::Function {
        name: Name::new("c").expect("identifier"),
        arguments: vec![],
        sign: Sign::Positive,
    };
    for _ in 0..depth {
        symbol = Symbol::Function {
            name: Name::new("f").expect("identifier"),
            arguments: vec![symbol],
            sign: Sign::Positive,
        };
    }
    symbol
}

#[test]
fn a_deep_symbol_survives_every_walk_without_overflowing_the_stack() {
    // Far past any real ground term, on the default test stack: an iterative
    // walk handles it; a recursive one would overflow here (§13). The rigorous,
    // stack-controlled proof is the depth proof (§16); this is the
    // canary that catches an accidental recursion the moment it lands.
    let deep = deep_symbol(200_000);
    let same = deep.clone();
    // Compared through a bound `bool` rather than `assert!(deep == same)`, so a
    // (never-taken) failure does not try to `Debug`-render two symbols this deep.
    let clone_is_equal = deep == same;
    assert!(clone_is_equal); // Eq
    assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
    let mut hasher = DefaultHasher::new();
    deep.hash(&mut hasher); // Hash
    let _ = hasher.finish();
    assert_eq!(deep.subsymbols().count(), 200_001); // read-traversal
    let _ = format!("{deep:?}"); // Debug
    let rebuilt = deep.clone().fold(Symbol::from); // fold
    let fold_is_equal = rebuilt == same;
    assert!(fold_is_equal);
    drop(deep); // Drop — the whole tree, iteratively
    drop(same);
    drop(rebuilt);
}
