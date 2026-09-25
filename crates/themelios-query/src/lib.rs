//! The themelios query tier — the epistemic reading over the program tier's
//! patterns and the solve tier's outcomes (design of record:
//! `docs/design/query.md`): the three-valued answer, the world view, cautious
//! and brave consequence, and bindings. Engine-free.
//!
//! The keystone is the epistemic question — *is this true, given the program* —
//! whose answer has three values, not two (§1, §2.2). This root holds the
//! vocabulary that question is asked and answered in: [`Answer`], the closed
//! trichotomy; [`Query`], the ground question, which refuses at construction
//! anything it could not answer, so a query that exists denotes; and
//! [`NotAQuery`], that refusal. The matching a query rests on is the program
//! tier's own (§3.1); this tier owns only the policy over a collection of answer
//! sets (§3.2).
#![forbid(unsafe_code)]

use themelios_program::AnswerSet;
use themelios_program::program::{Arguments, Atom};
use themelios_program::symbol::Symbol;
use themelios_program::term::{EvalError, Term};
use themelios_program::unify::{NotAPattern, Substitution, mgu, signature_range};

pub mod prelude;

// The reading-side re-export (docs/design/query.md §2.1): cautious and brave
// consequences are the solve tier's typed sets, each carrying the mode that
// produced it, and the query tier hands them back under the one name.
pub use themelios_solve::outcome::Consequences;

/// The epistemic answer to a ground query (docs/design/query.md §2.2): `Yes`
/// iff the query is true in every member of the world view, `No` iff it is
/// false in every member — its *contrary* present, never merely the query
/// absent — and `Unknown` otherwise, a genuine value never collapsed into `No`.
///
/// Closed — not `#[non_exhaustive]`: the trichotomy is the affordance, and a
/// fourth reading is what this type exists to forbid (§2.1). Owned plain data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    /// True in every member of the world view.
    Yes,
    /// False in every member of the world view: the contrary is present in
    /// each, which is more than the query being absent (§2.2).
    No,
    /// Neither — true in some member and not in all, or settled by none.
    Unknown,
}

impl std::fmt::Display for Answer {
    /// The human reading of the value, one word each.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Answer::Yes => "yes",
            Answer::No => "no",
            Answer::Unknown => "unknown",
        })
    }
}

/// A ground query (docs/design/query.md §2.1, §2.2; Gelfond and Kahl, Def.
/// 2.2.2 as corrected by the authors' errata): a literal, or a conjunction or
/// disjunction of queries — a closed set of *denoting* shapes. Construction is
/// the one door that can refuse: [`of`](Query::of) turns a ground atom into a
/// literal and refuses an atom it could not answer for, so a `Query` that
/// exists denotes and the readings over it never fail on the query's validity.
/// A conjunction ([`all`](Query::all)) and a disjunction ([`any`](Query::any))
/// compose queries and are total.
///
/// Owned plain data; its nesting is the caller's own composition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Query {
    shape: Shape,
}

/// The closed set of a query's shapes (docs/design/query.md §2.1). A literal
/// holds the ground symbol the atom denotes — the value an answer set contains
/// — so reading a query against a member is membership, never re-evaluation.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Shape {
    /// A ground literal, held as the symbol it denotes.
    Literal(Symbol),
    /// A conjunction (∧) of queries, evaluated per member as the weakest part.
    Conjunction(Vec<Query>),
    /// A disjunction (∨) of queries, evaluated per member as the strongest part.
    Disjunction(Vec<Query>),
}

impl Query {
    /// The literal query of a ground atom (docs/design/query.md §2.1): the
    /// atom's arguments evaluate at the program tier's ground door
    /// (`Term::evaluate`, program.md §3.5) to the symbol the atom denotes.
    /// Refuses, with the reason: a variable-bearing argument is `NotGround` — a
    /// query is ground, and a variable-bearing atom is a *pattern*, the
    /// bindings' question — and an argument that does not denote (an interval
    /// or a pool, which name a *set*; an unevaluated `@`-call; an undefined or
    /// out-of-range operation) or an argument-list pool `p(a; b)` is
    /// `NotAPattern`, the program tier's own refusal carried (§3.1). O(nodes).
    pub fn of(atom: Atom) -> Result<Self, NotAQuery> {
        // An argument-list pool names a set of atoms, not one: the atom-level
        // twin of a pooled argument's refusal, and the program tier's own
        // (program.md §11.2).
        let Arguments::Single(terms) = atom.arguments else {
            return Err(NotAQuery::NotAPattern(NotAPattern::Pooled));
        };
        // Each argument denotes the symbol its ground evaluation yields, or
        // refuses with the ground door's own reason: a variable is the one
        // reason that is this tier's (the atom is a pattern, not a query);
        // every other — a set-former, an `@`-call, an undefined or
        // out-of-range operation — is the program tier's non-denoting term.
        let mut arguments = Vec::with_capacity(terms.len());
        for term in terms {
            match term.evaluate() {
                Ok(symbol) => arguments.push(symbol),
                Err(EvalError::NotGround { .. }) => {
                    return Err(NotAQuery::NotGround { term });
                }
                Err(EvalError::External { .. } | EvalError::Undefined | EvalError::Overflow) => {
                    return Err(NotAQuery::NotAPattern(NotAPattern::NonDenoting { term }));
                }
            }
        }
        Ok(Query {
            shape: Shape::Literal(Symbol::function(atom.name, arguments, atom.sign)),
        })
    }

    /// The conjunction (∧) of `parts` (docs/design/query.md §2.1, §2.2):
    /// evaluated within each member of a world view as the weakest of its
    /// parts over `false < unknown < true`. Total; the empty conjunction is the
    /// query true everywhere. O(parts).
    pub fn all(parts: impl IntoIterator<Item = Query>) -> Query {
        Query {
            shape: Shape::Conjunction(parts.into_iter().collect()),
        }
    }

    /// The disjunction (∨) of `parts` (docs/design/query.md §2.1, §2.2):
    /// evaluated within each member of a world view as the strongest of its
    /// parts over `false < unknown < true`. Total; the empty disjunction is the
    /// query false everywhere. O(parts).
    pub fn any(parts: impl IntoIterator<Item = Query>) -> Query {
        Query {
            shape: Shape::Disjunction(parts.into_iter().collect()),
        }
    }
}

/// Why an atom is not a query (docs/design/query.md §2.1, §3.1), carrying the
/// offending term where there is one. Non-exhaustive, so a later reason is a
/// new variant, not a migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NotAQuery {
    /// An argument bears a variable, so the atom is not ground: it is a
    /// *pattern*, whose question is its bindings (§2.5), not a query.
    NotGround {
        /// The offending argument term.
        term: Term,
    },
    /// The atom is not a pattern at all (§3.1): an argument does not denote a
    /// single ground term, or the atom is an argument-list pool — the program
    /// tier's refusal, carried as this refusal's source.
    NotAPattern(NotAPattern),
}

impl std::fmt::Display for NotAQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAQuery::NotGround { term } => {
                write!(f, "not a query: the term {term:?} is not ground")
            }
            NotAQuery::NotAPattern(_) => f.write_str("not a query: the atom is not a pattern"),
        }
    }
}

impl std::error::Error for NotAQuery {
    /// The program tier's refusal beneath a `NotAPattern`; nothing beneath
    /// `NotGround`, which is this tier's own.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NotAQuery::NotGround { .. } => None,
            NotAQuery::NotAPattern(inner) => Some(inner),
        }
    }
}

/// Lift a ground `Symbol` to the signed `Atom` that matches it (docs/design/
/// query.md §3.1). Only a `Function` symbol denotes an atom a pattern can match;
/// a number, string, tuple, `#inf`, or `#sup` has no signature and never
/// matches, so it lifts to `None`. The arguments become `Symbolic` terms — the
/// value each already is — so the mgu reads them without re-evaluation, and the
/// sign is carried through unchanged. `O(arity)`.
///
/// The building block of `matches_in`; the world view (§2.3) and the bindings
/// (§2.5) reach it from there.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn lift(symbol: &Symbol) -> Option<Atom> {
    match symbol {
        Symbol::Function {
            name,
            arguments,
            sign,
        } => Some(Atom {
            // Built directly from parts that are already a canonical ground atom —
            // the name and sign as they stand, each argument wrapped verbatim as a
            // `Symbolic` term — so no constructor canonicalization pass is owed.
            sign: *sign,
            name: name.clone(),
            arguments: Arguments::Single(arguments.iter().cloned().map(Term::Symbolic).collect()),
        }),
        _ => None,
    }
}

/// Every substitution under which `pattern` matches a member of `set` (docs/
/// design/query.md §3.1). The candidates are the contiguous block of symbols
/// sharing `pattern`'s signature — `set.range(signature_range(pattern))`, an
/// `O(log n + k)` scan of the `k` candidates, not the whole `n`-member set, each
/// then lifted and unified at a cost linear in its own size. Reuse, not
/// reinvention: the unifier, the signature range, the triangular substitution,
/// and the forced occurs-check are the program tier's; only enumerating the
/// candidates is this tier's.
///
/// Refusal is *set-independent*. A `pattern` that is not a pattern is an `Err`,
/// and the same `Err`, whether or not `set` holds a same-signature member: it is
/// classified once, up front, by self-unifying `pattern` — the program tier's own
/// `mgu` refuses a pool and a non-denoting argument (a variable-bearing arithmetic
/// term, an undefined or out-of-range ground operation, an interval, a pooled
/// argument, an unevaluated `@`-call, §3.1) alike — never incidentally by a `mgu`
/// reached only when the candidate block is non-empty. That classification runs
/// before `signature_range`, whose value on a pool is the empty range
/// `#sup..=#inf` that `BTreeSet::range` would panic on (`start > end`), so the
/// panic is unreachable. *Cannot decide* is never *no match*.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn matches_in(
    pattern: &Atom,
    set: &AnswerSet,
) -> Result<Vec<Substitution>, NotAPattern> {
    mgu(pattern, pattern)?;
    let mut out = Vec::new();
    for candidate in set.range(signature_range(pattern)) {
        if let Some(atom) = lift(candidate)
            && let Some(substitution) = mgu(pattern, &atom)?
        {
            out.push(substitution);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod matching {
    use super::*;
    use proptest::prelude::*;
    use themelios_program::symbol::{Name, Sign, VarName};

    /// A 0-ary constant symbol.
    fn constant(name: &str) -> Symbol {
        Symbol::function(
            Name::new(name).expect("a valid identifier"),
            [],
            Sign::Positive,
        )
    }

    /// The applied symbol `name(args…)`.
    fn applied(name: &str, args: impl IntoIterator<Item = Symbol>) -> Symbol {
        Symbol::function(
            Name::new(name).expect("a valid identifier"),
            args,
            Sign::Positive,
        )
    }

    /// The pattern `name(terms…)`, a `Single` argument list.
    fn pattern(name: &str, terms: Vec<Term>) -> Atom {
        Atom {
            sign: Sign::Positive,
            name: Name::new(name).expect("a valid identifier"),
            arguments: Arguments::Single(terms),
        }
    }

    /// The ground argument term denoting `symbol`, the value it already is — a
    /// ground query pattern's argument.
    fn ground(symbol: Symbol) -> Term {
        Term::Symbolic(symbol)
    }

    /// The named variable `text` (`X`, `Y`, …) as a pattern argument.
    fn var(text: &str) -> Term {
        Term::variable(VarName::new(text).expect("a valid variable name"))
    }

    /// The ground argument term for the number `value`.
    fn num(value: i32) -> Term {
        Term::Symbolic(Symbol::number(value))
    }

    /// The nested ground symbol `f(f(… f(a) …))`, `depth` applications deep — the
    /// adversarial shape the deep-ground-symbol bound is about.
    fn nested(depth: usize) -> Symbol {
        let mut symbol = constant("a");
        for _ in 0..depth {
            symbol = applied("f", [symbol]);
        }
        symbol
    }

    /// The full-scan reference: lift and `mgu` every member, not just the block.
    fn full_scan(pattern: &Atom, set: &AnswerSet) -> Vec<Substitution> {
        let mut out = Vec::new();
        for candidate in set {
            if let Some(atom) = lift(candidate)
                && let Some(substitution) = mgu(pattern, &atom).expect("a pattern")
            {
                out.push(substitution);
            }
        }
        out
    }

    #[test]
    fn a_non_function_symbol_never_lifts_to_a_pattern_match() {
        assert!(lift(&Symbol::number(3)).is_none());
        assert!(lift(&Symbol::string("x")).is_none());
        assert!(lift(&Symbol::tuple([constant("a"), constant("b")])).is_none());
        assert!(lift(&Symbol::Infimum).is_none());
        assert!(lift(&Symbol::Supremum).is_none());
    }

    #[test]
    fn lift_carries_the_sign_of_a_function_symbol() {
        let negative = Symbol::function(
            Name::new("p").expect("a valid identifier"),
            [constant("a")],
            Sign::Negative,
        );
        let atom = lift(&negative).expect("a function symbol lifts to an atom");
        assert_eq!(
            atom.sign,
            Sign::Negative,
            "lift preserves the sign; the lifted atom matches only a same-signed member",
        );
    }

    #[test]
    fn a_lifted_symbol_matches_its_own_member() {
        // lift inverts a ground query pattern's construction: the atom a symbol
        // lifts to matches that very symbol, and binds nothing.
        let symbol = applied("p", [constant("a")]);
        let atom = lift(&symbol).expect("a function symbol lifts to an atom");
        let set: AnswerSet = [symbol].into_iter().collect();
        let matches = matches_in(&atom, &set).expect("a pattern");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0].iter().next().is_none(),
            "a lifted ground symbol matches itself exactly, binding nothing",
        );
    }

    #[test]
    fn a_pooled_pattern_is_refused_not_matched() {
        let pooled = Atom {
            sign: Sign::Positive,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Pooled(vec![
                vec![Term::Symbolic(constant("a"))],
                vec![Term::Symbolic(constant("b"))],
            ]),
        };
        let set: AnswerSet = [applied("p", [constant("a")])].into_iter().collect();
        assert!(matches!(
            matches_in(&pooled, &set),
            Err(NotAPattern::Pooled)
        ));
    }

    #[test]
    fn a_ground_pattern_finds_its_member_within_its_block() {
        // p(a) over { p(a), p(b), q(c) }: the range is the p/1 block, so q(c) —
        // a different signature — is never scanned, and within the block only
        // the equal member matches.
        let set: AnswerSet = [
            applied("p", [constant("a")]),
            applied("p", [constant("b")]),
            applied("q", [constant("c")]),
        ]
        .into_iter()
        .collect();
        let matches =
            matches_in(&pattern("p", vec![ground(constant("a"))]), &set).expect("a pattern");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0].iter().next().is_none(),
            "a ground pattern binds nothing: its one match is the empty substitution",
        );
    }

    #[test]
    fn a_variable_pattern_matches_every_member_of_its_block() {
        // p(X) over { p(a), p(b), q(c) }: the range is the p/1 block, so both
        // p-members match — a variable pattern is not a single-member match — and
        // q(c), a different signature, is never scanned.
        let set: AnswerSet = [
            applied("p", [constant("a")]),
            applied("p", [constant("b")]),
            applied("q", [constant("c")]),
        ]
        .into_iter()
        .collect();
        let matches = matches_in(&pattern("p", vec![var("X")]), &set).expect("a pattern");
        assert_eq!(matches.len(), 2, "X binds to each of a and b, never to c");
        for substitution in &matches {
            assert_eq!(
                substitution.iter().count(),
                1,
                "a one-variable pattern binds exactly its one variable",
            );
        }
    }

    #[test]
    fn the_inclusive_range_reaches_both_block_edges() {
        // The p/1 block runs from p(#inf) to p(#sup) — the least and greatest p/1
        // symbols. A range narrowed by one at either end would drop an edge member;
        // p(X) must find all three.
        let set: AnswerSet = [
            applied("p", [Symbol::Infimum]),
            applied("p", [constant("a")]),
            applied("p", [Symbol::Supremum]),
        ]
        .into_iter()
        .collect();
        let matches = matches_in(&pattern("p", vec![var("X")]), &set).expect("a pattern");
        assert_eq!(
            matches.len(),
            3,
            "the inclusive signature range spans p(#inf)..=p(#sup)",
        );
    }

    #[test]
    fn a_non_pattern_is_refused_whichever_members_the_set_holds() {
        // p(1..3) is not a pattern: an interval names a set (§3.1). The refusal is
        // the same — NonDenoting, never a quiet empty match — whether the set holds
        // a same-signature member, a different one, or none. The classification is
        // set-independent, not a side effect of the candidate block being non-empty.
        let interval = pattern(
            "p",
            vec![Term::Interval {
                lower: Box::new(num(1)),
                upper: Box::new(num(3)),
            }],
        );
        let with_same_signature: AnswerSet = [applied("p", [constant("a")])].into_iter().collect();
        let with_other_signature: AnswerSet = [applied("q", [constant("a")])].into_iter().collect();
        let empty = AnswerSet::new();
        let refusal = matches_in(&interval, &empty);
        assert!(
            matches!(refusal, Err(NotAPattern::NonDenoting { .. })),
            "an interval pattern is a non-denoting refusal, not a match",
        );
        for set in [&with_same_signature, &with_other_signature] {
            assert_eq!(
                matches_in(&interval, set),
                refusal,
                "the refusal is identical whichever members the set holds",
            );
        }
    }

    #[test]
    fn a_signed_pattern_matches_only_a_same_signed_member() {
        // lift carries the sign and the mgu requires signs to agree, so the negative
        // pattern -p(X) over { p(a), -p(a) } matches only the negative member.
        let negative = Symbol::function(
            Name::new("p").expect("a valid identifier"),
            [constant("a")],
            Sign::Negative,
        );
        let set: AnswerSet = [applied("p", [constant("a")]), negative]
            .into_iter()
            .collect();
        let pattern = Atom {
            sign: Sign::Negative,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![var("X")]),
        };
        let matches = matches_in(&pattern, &set).expect("a pattern");
        assert_eq!(
            matches.len(),
            1,
            "only -p(a) matches -p(X), never the positive p(a)"
        );
    }

    /// The base nesting depth for the near-linear match proof; the large case is
    /// `SIZE_RATIO` deeper.
    const DEPTH: usize = 2_000;
    /// The data-size ratio between the small and large deep-symbol cases.
    const SIZE_RATIO: usize = 16;
    /// A near-linear claim at `SIZE_RATIO` may cost at most this factor: fourfold
    /// noise headroom above linear (x16), fourfold separation below quadratic
    /// (x256).
    const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
    /// Interleaved runs per measurement; the median of their ratios is taken.
    const SAMPLES: usize = 5;
    /// Ratios are scaled by this factor so the median arithmetic stays in integers;
    /// a ceiling `C` is the scaled bound `C * RATIO_SCALE`.
    const RATIO_SCALE: u128 = 1_000;
    /// Matches timed per measurement, to lift a single reading clear of timer noise.
    const REPEAT: usize = 8;

    /// One elapsed measurement of `work`, in nanoseconds, floored to 1 so a
    /// sub-nanosecond reading can still divide.
    fn time_once(mut work: impl FnMut()) -> u128 {
        let start = std::time::Instant::now();
        work();
        start.elapsed().as_nanos().max(1)
    }

    /// The median over `SAMPLES` interleaved runs of `big`'s cost over `small`'s,
    /// scaled by `RATIO_SCALE`. Each run times `small` then `big` back-to-back, so a
    /// load spike lands on both, not on one side of a separately-batched median.
    fn median_ratio(mut small: impl FnMut() -> u128, mut big: impl FnMut() -> u128) -> u128 {
        let mut ratios = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let s = small().max(1);
            let b = big();
            ratios.push(b * RATIO_SCALE / s);
        }
        ratios.sort_unstable();
        ratios[SAMPLES / 2]
    }

    #[cfg_attr(
        not(feature = "scale-proofs"),
        ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
    )]
    #[test]
    fn deep_ground_symbols_match_in_near_linear_time() {
        // Matching one deep ground symbol lifts and unifies a term linear in its
        // depth; a per-level re-walk would be quadratic. The program tier's mgu is
        // near-linear and this crate adds only the O(log n + k) block scan over a
        // one-member set, so SIZE_RATIO more depth must cost near SIZE_RATIO more,
        // well below quadratic (§3.1).
        let match_at = |depth: usize| -> u128 {
            let set: AnswerSet = [applied("p", [nested(depth)])].into_iter().collect();
            let pat = pattern("p", vec![ground(nested(depth))]);
            time_once(|| {
                for _ in 0..REPEAT {
                    let found = matches_in(&pat, &set).expect("a pattern");
                    std::hint::black_box(&found);
                }
            })
        };
        let ratio = median_ratio(|| match_at(DEPTH), || match_at(DEPTH * SIZE_RATIO));
        assert!(
            ratio <= LINEAR_CEILING * RATIO_SCALE,
            "matching a deep ground symbol grew worse than near-linearly: the median cost \
             ratio across x{SIZE_RATIO} more depth was {ratio} (scaled by {RATIO_SCALE}), \
             over the ceiling {}; the mgu quadratic must stay closed",
            LINEAR_CEILING * RATIO_SCALE,
        );
    }

    proptest! {
        /// The signature-range scan is not lossy for a ground pattern: it finds
        /// exactly what a lift-and-mgu over the whole set finds — here, at most the
        /// one equal member.
        #[test]
        fn a_ground_scan_finds_exactly_what_a_full_scan_finds(
            members in prop::collection::vec((0u8..3, 0i32..4), 0..12),
            pat_pred in 0u8..3,
            pat_arg in 0i32..4,
        ) {
            let arg = |a: i32| applied("c", [Symbol::number(a)]);
            let sym = |pred: u8, a: i32| applied(&format!("p{pred}"), [arg(a)]);
            let set: AnswerSet = members.iter().map(|&(pred, a)| sym(pred, a)).collect();
            let pat = pattern(&format!("p{pat_pred}"), vec![ground(arg(pat_arg))]);
            prop_assert_eq!(
                matches_in(&pat, &set).expect("a pattern"),
                full_scan(&pat, &set)
            );
        }

        /// The same for a *variable* pattern, which matches its whole block — so a
        /// range too narrow at either end would drop members the full scan keeps, a
        /// loss the single-member ground case cannot expose.
        #[test]
        fn a_variable_scan_finds_its_whole_block(
            members in prop::collection::vec((0u8..3, 0i32..4), 0..12),
            pat_pred in 0u8..3,
        ) {
            let arg = |a: i32| applied("c", [Symbol::number(a)]);
            let sym = |pred: u8, a: i32| applied(&format!("p{pred}"), [arg(a)]);
            let set: AnswerSet = members.iter().map(|&(pred, a)| sym(pred, a)).collect();
            let pat = pattern(&format!("p{pat_pred}"), vec![var("X")]);
            prop_assert_eq!(
                matches_in(&pat, &set).expect("a pattern"),
                full_scan(&pat, &set)
            );
        }
    }
}
