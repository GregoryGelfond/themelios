//! Shape assertions for the checks (docs/design/program.md §15, §16): complexity
//! shape only, held by the median over five interleaved wall-clock ratios with
//! tolerances wide enough for any machine the checks run on — equality, clone,
//! rendering, and traversal linear in the structure, `mgu` near-linear in both
//! atoms (the Martelli–Montanari shape a monolithic ground representation would
//! make quadratic, §11.1), a match against an answer set logarithmic via
//! `signature_range` (§11.3), and part-wise access logarithmic in the parts
//! (§4.1). What they prove: the claimed class — a quadratic `mgu`, an O(n²)
//! equality/clone/render/traversal, a linear-scan match or part lookup. What they
//! cannot: absolute speed, which is machine-dependent and lives in the out-of-band
//! benches (benches/scaling.rs, spec §10.2).
//!
//! The complement to the depth proof (tests/depth_proof.rs): that proves every walk
//! over a value nested far past any real program *survives* a stated small stack
//! (stack-independent, §13); this proves those same walks are *linear-time* at
//! depth — no walk that survives deep does so by paying a per-level re-walk. The two
//! together are the walk discipline's whole claim.
//!
//! Each ratio is the median over five runs that time the small case and the large
//! case back-to-back, not the ratio of two separately-median'd batches: a load
//! transient during a run inflates both of that run's halves and cancels in its
//! ratio, so no transient landing on the large measurement alone can push the ratio
//! past its ceiling. Every fast operation is timed over a fixed repeat count so the
//! measurement clears timer noise; the count is the same on both sides and cancels
//! in the ratio.

use std::collections::BTreeSet;
use std::time::Instant;

use themelios_base::source::{Source, SourceId};

use themelios_program::program::{Atom, PartKey, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::unify::{mgu, signature_range};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// The data-size ratio between the small and large cases for a linear or near-linear
/// claim.
const SIZE_RATIO: usize = 16;
/// A linear (or near-linear) claim at SIZE_RATIO may cost at most this factor:
/// fourfold noise headroom above linear (x16) and fourfold separation below
/// quadratic (x256).
const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
/// The data-size ratio for a logarithmic claim: wide, so a linear scan and a
/// logarithmic search separate by a factor no machine noise closes.
const LOG_SIZE_RATIO: usize = 64;
/// A logarithmic claim across LOG_SIZE_RATIO more elements may cost at most this
/// factor — logarithmic growth is a small factor; a linear scan (x64) fails.
const LOG_CEILING: u128 = 8;
/// Interleaved runs per measurement; the median of their ratios is taken.
const SAMPLES: usize = 5;
/// Ratios are scaled by this factor so the median arithmetic stays in integers; a
/// ceiling `C` is the scaled bound `C * RATIO_SCALE`.
const RATIO_SCALE: u128 = 1000;

/// One elapsed measurement of `work`, in nanoseconds — floored to 1 so a
/// sub-nanosecond reading can still divide.
fn time_once(mut work: impl FnMut()) -> u128 {
    let start = Instant::now();
    work();
    start.elapsed().as_nanos().max(1)
}

/// The median over SAMPLES interleaved runs of `big`'s cost over `small`'s, scaled
/// by RATIO_SCALE. Each run evaluates `small` then `big` back-to-back; the two
/// closures return that run's cost figure (elapsed nanos).
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

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn variable(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable name"),
    ))
}

/// `f(f(… bottom …))` — `depth` `f`-applications over the bottom term.
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

/// A NON-ground deep term, `f(f(… X …))` — the structure the term-level walks are
/// measured over. Non-ground on purpose: a ground nest canonicalizes to a `Symbolic`
/// leaf, which `subterms` does not descend (§3.6), so it would measure O(1), not the
/// O(nodes) walk.
fn deep_term(depth: usize) -> Term {
    nest(variable("X"), depth)
}

/// The base nesting depth for a linear/near-linear term claim; the large case is
/// SIZE_RATIO deeper.
const DEPTH: usize = 1_000;

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn equality_is_linear_in_term_depth() {
    // Two EQUAL deep terms, so equality cannot short-circuit on a mismatch and must
    // walk the whole spine — the O(nodes) claim proportional to structure (§13, §15).
    // A per-level re-walk would be quadratic in depth.
    const REPEAT: usize = 8;
    let small_a = deep_term(DEPTH);
    let small_b = small_a.clone();
    let big_a = deep_term(DEPTH * SIZE_RATIO);
    let big_b = big_a.clone();
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(small_a == small_b);
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(big_a == big_b);
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "equality's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} depth; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn clone_is_linear_in_term_depth() {
    const REPEAT: usize = 8;
    let small = deep_term(DEPTH);
    let big = deep_term(DEPTH * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(small.clone());
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(big.clone());
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "clone's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} depth; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn traversal_is_linear_in_term_depth() {
    // `subterms` visits every node once, pre-order (§3.6); counting the whole walk is
    // O(nodes). A traversal that re-collected a suffix at each node would be quadratic.
    const REPEAT: usize = 8;
    let small = deep_term(DEPTH);
    let big = deep_term(DEPTH * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(small.subterms().count());
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(big.subterms().count());
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "traversal's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} depth; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

/// A one-rule program `q :- p(f(f(… X …))).` carrying a deep term — the structure
/// `render` writes, O(output) in the term's depth.
fn deep_program(depth: usize) -> Program {
    let rule = Rule::new(
        Atom::constant(name("q")),
        Atom::new(name("p"), [deep_term(depth)]),
    );
    Program::of([WithProvenance::constructed(Statement::Rule(rule))])
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn rendering_is_linear_in_structure() {
    // `render` is a single work-list walk, O(output) (§10). A renderer that re-scanned
    // the text it had written so far would be quadratic in the output.
    const REPEAT: usize = 4;
    let small = deep_program(DEPTH);
    let big = deep_program(DEPTH * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(render(&small, Dialect::Clingo).expect("renders"));
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(render(&big, Dialect::Clingo).expect("renders"));
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "rendering's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} output; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

/// `p(f(f(… a)))` — an atom whose one argument is a deep GROUND term. Inside `mgu` it
/// canonicalizes to a deep ground `Symbol`, the case a monolithic representation would
/// make quadratic (§11.1).
fn ground_atom(depth: usize) -> Atom {
    let ground_bottom = Term::Function {
        name: name("a"),
        arguments: Vec::new(),
    };
    Atom::new(name("p"), [nest(ground_bottom, depth)])
}

/// `p(f(f(… X)))` — the non-ground twin of [`ground_atom`], a variable at the bottom.
fn nested_atom(depth: usize) -> Atom {
    Atom::new(name("p"), [deep_term(depth)])
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn mgu_is_near_linear_in_both_atoms() {
    // A deep ground symbol against its non-ground twin — the adversarial shape that was
    // Θ(depth²) before the ground side was decomposed into the unification graph (§11.1,
    // the Stop-B fix). Near-linear: the ratio tracks the depth, not its square. Both
    // atoms unify, so the full decide-and-produce path (including reading out the
    // triangular substitution) is timed.
    const REPEAT: usize = 8;
    let small_ground = ground_atom(DEPTH);
    let small_nested = nested_atom(DEPTH);
    let big_ground = ground_atom(DEPTH * SIZE_RATIO);
    let big_nested = nested_atom(DEPTH * SIZE_RATIO);
    assert!(
        matches!(mgu(&small_ground, &small_nested), Ok(Some(_))),
        "the fixture atoms unify"
    );
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    let outcome = mgu(&small_ground, &small_nested);
                    std::hint::black_box(&outcome);
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    let outcome = mgu(&big_ground, &big_nested);
                    std::hint::black_box(&outcome);
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    let quadratic = (SIZE_RATIO * SIZE_RATIO) as u128;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "mgu's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} depth; the near-linear shape allows at most x{LINEAR_CEILING}, where a quadratic unifier would be ~x{quadratic}"
    );

    // That this tripwire is real — that it would catch a regression, not merely pass on
    // whatever `mgu` does — is shown by the stand-in a quadratic unifier would be. A
    // composition that resolved the whole partial substitution at each bind, or a ground
    // representation the unifier walked without decomposing (the pre-§11.1 shape), costs
    // Θ(depth²): unifying `p(f(f(… a)))` with `p(f(f(… X)))` at depth d does work
    // proportional to d at each of d levels. Written as such a stand-in it would read:
    //
    //     fn quadratic_unify(ground: &Atom, nested: &Atom) -> bool {
    //         // resolve the growing binding set fully at each level: Σ d = Θ(d²)
    //         let depth = /* the shared nesting */;
    //         let mut resolved = 0usize;
    //         for _level in 0..depth {
    //             for _ in 0..resolved { std::hint::black_box(()); }  // re-walk the prefix
    //             resolved += 1;
    //         }
    //         true
    //     }
    //
    // Its median ratio over x16 depth would be ~x256 (SIZE_RATIO²), far past the x64
    // ceiling above — the tripwire trips. It is stated here rather than shipped: a
    // running quadratic double would only slow the checks to re-prove what the arithmetic
    // already shows (analogous to the depth proof's overflow control, which IS shipped
    // because a stack overflow cannot be argued from arithmetic).
}

/// The fixed number of `p(_)` symbols the pattern `p(X)` matches, whatever the answer
/// set's size — so the match is O(log n + k) with k fixed.
const TARGETS: usize = 8;

/// A ground unary symbol `pred(arg)`.
fn ground_symbol(pred: &str, arg: usize) -> Symbol {
    Symbol::Function {
        name: name(pred),
        arguments: vec![Symbol::Number(
            i32::try_from(arg).expect("the argument fits i32"),
        )],
        sign: Sign::Positive,
    }
}

/// An answer set: the fixed `p(0..TARGETS)` block the pattern matches, plus `fillers`
/// symbols of a predicate the pattern's signature range excludes (`p` < `q` in the
/// ground-term order, §3.1, so the `q` block sorts wholly past `p`'s range). Growing
/// `fillers` leaves the match count fixed at TARGETS.
fn answer_set(fillers: usize) -> BTreeSet<Symbol> {
    let mut set = BTreeSet::new();
    for i in 0..TARGETS {
        set.insert(ground_symbol("p", i));
    }
    for i in 0..fillers {
        set.insert(ground_symbol("q", i));
    }
    set
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn matching_an_answer_set_is_logarithmic() {
    // A pattern's predicate, arity, and sign block in the ground-term order, so its
    // matches are a contiguous range of the answer set: `set.range(signature_range(p))`
    // is O(log n + k), not O(n) (§11.3). Growing the answer set with symbols the range
    // excludes leaves k fixed, so the cost grows only with log n. A full scan would grow
    // with n.
    const REPEAT: usize = 500;
    const BASE: usize = 1_024;
    let pattern = Atom::new(name("p"), [variable("X")]);
    let small = answer_set(BASE);
    let big = answer_set(BASE * LOG_SIZE_RATIO);
    // The fixture is honest: the range finds exactly the fixed target block on both
    // sides, whatever the fillers (a broken order or signature would show here).
    assert_eq!(
        small.range(signature_range(&pattern)).count(),
        TARGETS,
        "the small answer set's range holds exactly the target block"
    );
    assert_eq!(
        big.range(signature_range(&pattern)).count(),
        TARGETS,
        "the large answer set's range holds exactly the target block"
    );
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(small.range(signature_range(&pattern)).count());
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(big.range(signature_range(&pattern)).count());
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LOG_CEILING * RATIO_SCALE,
        "the match's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{LOG_SIZE_RATIO} more symbols; the O(log n + k) shape allows at most x{LOG_CEILING}"
    );
}

/// A program of `parts` parts, each opened by a `#program q<i>.` delimiter and holding
/// one fact — the only public door to a multi-part program (`Program::of` fills only
/// `base`; the raise lifts `#program` into part structure, §4.1, §8).
fn multi_part_program(parts: usize) -> Program {
    let mut text = String::with_capacity(parts * 16);
    for i in 0..parts {
        text.push_str("#program q");
        text.push_str(&i.to_string());
        text.push_str(".\na.\n");
    }
    let source = Source::new(SourceId::new(0), text).expect("the multi-part text admits");
    raise(&parse(&source, Dialect::Clingo)).into_program()
}

/// The key of the `q<i>` part.
fn q_key(i: usize) -> PartKey {
    PartKey {
        name: name(&format!("q{i}")),
        formals: Vec::new(),
    }
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "scaling proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn part_wise_access_is_logarithmic_in_the_parts() {
    // `Program::part` is a `BTreeMap` lookup, O(log parts) (§4.1, §15) — cheap
    // random access into a multi-shot program's parts. A rewrite to a linear scan over
    // the parts would grow with the part count. A fixed set of keys is looked up on both
    // sides, so the work per measurement is (keys × log parts); growing the part count
    // grows only the log factor.
    const REPEAT: usize = 200;
    const BASE: usize = 64;
    const LOOKUPS: usize = 64;
    let small = multi_part_program(BASE);
    let big = multi_part_program(BASE * LOG_SIZE_RATIO);
    // The fixture is honest: base plus one part per `#program`.
    assert_eq!(
        small.parts().count(),
        BASE + 1,
        "the small program has base plus one part per delimiter"
    );
    assert_eq!(
        big.parts().count(),
        BASE * LOG_SIZE_RATIO + 1,
        "the large program has base plus one part per delimiter"
    );
    let keys: Vec<PartKey> = (0..LOOKUPS).map(q_key).collect();
    // Every looked-up key is present in both programs, so both do the found-key walk.
    assert!(
        keys.iter()
            .all(|k| small.part(k).is_some() && big.part(k).is_some()),
        "the looked-up keys are present on both sides"
    );
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    for key in &keys {
                        std::hint::black_box(small.part(key));
                    }
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    for key in &keys {
                        std::hint::black_box(big.part(key));
                    }
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LOG_CEILING * RATIO_SCALE,
        "part access's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{LOG_SIZE_RATIO} more parts; the O(log parts) shape allows at most x{LOG_CEILING}"
    );
}
