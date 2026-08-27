//! Property laws over generated values (docs/design/program.md §16) — the proptest discipline for
//! this stage's surfaces. The most general unifier is a pure function whose result, when it
//! unifies, makes the two atoms equal once the resolving substitute is applied (§11.1). A
//! **constructor-built** program — one that never came from text, so it can carry a negative
//! `Number` — round-trips through render and the raise (§10), reaching the negative-numeral case
//! a text-only corpus cannot. And `signature_range` picks out of an ordered answer set exactly the
//! signature's block (§11.3). Generators draw the constructor fragment: variable, number
//! (spanning the negatives only construction reaches), constant, function, tuple.

use std::collections::BTreeSet;

use proptest::prelude::*;

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::unify::{mgu, signature_range};

// ---- strategies ----

/// A constructor-fragment term. `round_trippable` excludes `i32::MIN`, whose magnitude no numeral
/// spells (grammar §4.3), where a term must render and reparse; the unifier tolerates it.
fn term(round_trippable: bool) -> impl Strategy<Value = Term> {
    let number = if round_trippable {
        prop_oneof![-6..6i32, Just(i32::MIN + 1), Just(i32::MAX)].boxed()
    } else {
        any::<i32>().boxed()
    };
    let leaf = prop_oneof![
        "[A-C]".prop_map(|s| Term::Variable(Variable::Named(
            VarName::new(s).expect("a valid variable name")
        ))),
        number.prop_map(Term::from),
        "[a-c]".prop_map(|s| Term::Function {
            name: Name::new(s).expect("a valid identifier"),
            arguments: Vec::new(),
        }),
    ];
    leaf.prop_recursive(3, 20, 3, |inner| {
        prop_oneof![
            ("[fg]", prop::collection::vec(inner.clone(), 1..3)).prop_map(|(name, arguments)| {
                Term::Function {
                    name: Name::new(name).expect("a valid identifier"),
                    arguments,
                }
            }),
            prop::collection::vec(inner, 2..4).prop_map(Term::Tuple),
        ]
    })
}

/// A pair of atoms with the same predicate and arity — the shape `mgu` reads, and often
/// unifiable (a shared small variable pool).
fn atom_pair() -> impl Strategy<Value = (Atom, Atom)> {
    (1usize..3)
        .prop_flat_map(|arity| {
            (
                prop::collection::vec(term(false), arity),
                prop::collection::vec(term(false), arity),
            )
        })
        .prop_map(|(left, right)| {
            let predicate = Name::new("p").expect("a valid identifier");
            (
                Atom::new(predicate.clone(), left),
                Atom::new(predicate, right),
            )
        })
}

/// A constructor-built program of facts over round-trippable terms.
fn fact_program() -> impl Strategy<Value = Program> {
    prop::collection::vec(
        ("[pqr]", prop::collection::vec(term(true), 1..3)).prop_map(|(name, arguments)| {
            let atom = Atom::new(Name::new(name).expect("a valid identifier"), arguments);
            WithProvenance::constructed(Statement::Rule(Rule::fact(atom)))
        }),
        1..4,
    )
    .prop_map(Program::of)
}

/// A ground symbol spanning the order's bands — sentinels, numbers, strings, constants, and
/// arity-bearing functions and tuples of either sign.
fn ground_symbol() -> impl Strategy<Value = Symbol> {
    let sign = prop_oneof![Just(Sign::Positive), Just(Sign::Negative)];
    let leaf = prop_oneof![
        Just(Symbol::Infimum),
        Just(Symbol::Supremum),
        (-6..6i32).prop_map(Symbol::Number),
        "[xy]".prop_map(Symbol::String),
        ("[pq]", sign.clone()).prop_map(|(name, sign)| Symbol::Function {
            name: Name::new(name).expect("a valid identifier"),
            arguments: Vec::new(),
            sign,
        }),
    ];
    leaf.prop_recursive(2, 10, 2, move |inner| {
        prop_oneof![
            (
                "[pq]",
                prop::collection::vec(inner.clone(), 1..3),
                sign.clone()
            )
                .prop_map(|(name, arguments, sign)| Symbol::Function {
                    name: Name::new(name).expect("a valid identifier"),
                    arguments,
                    sign,
                }),
            prop::collection::vec(inner, 1..3).prop_map(Symbol::Tuple),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn mgu_is_sound_and_deterministic((left, right) in atom_pair()) {
        // A pure function of its inputs (§14), and — a constructor-fragment atom being always a
        // pattern — never a refusal. When it unifies, the atoms are equal once resolved (§11.1).
        let first = mgu(&left, &right);
        prop_assert_eq!(&first, &mgu(&left, &right), "mgu is a pure function of its inputs");
        prop_assert!(first.is_ok(), "a constructor-fragment atom is a pattern");
        if let Ok(Some(sigma)) = &first {
            let left_image: Vec<Term> =
                left.arguments.iter().map(|t| t.clone().substitute(sigma)).collect();
            let right_image: Vec<Term> =
                right.arguments.iter().map(|t| t.clone().substitute(sigma)).collect();
            prop_assert_eq!(left_image, right_image, "a unifier equates the atoms once resolved");
        }
    }

    #[test]
    fn constructed_programs_round_trip(program in fact_program()) {
        // A constructor-built program round-trips through render and the raise (§10), reaching the
        // negative `Number` a text corpus cannot; the numeral fold (§5.1) closes the loop.
        let rendered =
            render(&program, Dialect::Clingo).expect("a constructor-built program renders");
        let source = Source::new(SourceId::new(0), rendered.clone()).expect("the rendering admits");
        let reparsed = raise(&parse(&source, Dialect::Clingo));
        prop_assert!(
            reparsed.diagnostics().is_empty(),
            "`{}` reparses cleanly: {:?}",
            rendered,
            reparsed.diagnostics(),
        );
        prop_assert_eq!(&program, reparsed.program(), "round-trip up to provenance (`{}`)", rendered);
    }

    #[test]
    fn signature_range_finds_the_signature_scan(
        (pattern, _) in atom_pair(),
        symbols in prop::collection::vec(ground_symbol(), 0..16),
    ) {
        // The O(log n + k) range picks exactly the (sign, name, arity) block an O(n) scan finds.
        let answer: BTreeSet<Symbol> = symbols.into_iter().collect();
        let signature = Signature {
            sign: pattern.sign,
            name: pattern.name.clone(),
            arity: u32::try_from(pattern.arguments.len()).expect("a small test arity"),
        };
        let via_range: BTreeSet<Symbol> =
            answer.range(signature_range(&pattern)).cloned().collect();
        let via_scan: BTreeSet<Symbol> = answer
            .iter()
            .filter(|symbol| symbol.signature().as_ref() == Some(&signature))
            .cloned()
            .collect();
        prop_assert_eq!(via_range, via_scan);
    }
}
