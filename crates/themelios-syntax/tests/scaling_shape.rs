//! CI shape assertions (docs/design/syntax.md §16): complexity shape
//! only, held by the median over five interleaved wall-clock ratios with
//! tolerances wide enough for any CI machine — parse linear in text, the
//! certificate linear in both texts, bulk attachment linear in the tree,
//! the oracle constant per pair. What they prove: the claimed class (a
//! quadratic parse, a re-scanning attachment, a certificate that
//! re-walks). What they cannot: absolute speed — that lives in the
//! out-of-band benches.
//!
//! Each ratio is the median over five runs that time the small case and
//! the large case back-to-back, not the ratio of two separately-median'd
//! batches: a load transient during a run inflates both of that run's
//! halves and cancels in its ratio, so no transient landing on the large
//! measurement alone can push the ratio past its ceiling.

use std::time::Instant;

use themelios_base::source::{Source, SourceId};
use themelios_syntax::attach::{attachments, empty_line_between};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{Certificate, equivalent};
use themelios_syntax::fusion::separator;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// One rule with a comment run and a theory atom, so every size
/// exercises the parser's families, the comment run, and the modes.
const UNIT: &str = "% leading\np(X, f(Y)) :- q(X; Y), not r(X), X = 1..3, #sum { W,T : t(T,W) } >= 4, &sum { x, -y : p } <= 3. % trailing\n";

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: usize = 16;
/// A linear claim at SIZE_RATIO may cost at most this factor: fourfold
/// noise headroom above linear (x16) and fourfold separation below
/// quadratic (x256).
const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
/// A constant-per-pair claim over SIZE_RATIO more pairs may cost at most
/// this factor per pair (the same fourfold headroom, on the per-pair
/// figure).
const CONSTANT_CEILING: u128 = 4;
/// Interleaved runs per measurement; the median of their ratios is taken.
const SAMPLES: usize = 5;
/// Ratios are scaled by this factor so the median arithmetic stays in
/// integers; a ceiling `C` is the scaled bound `C * RATIO_SCALE`.
const RATIO_SCALE: u128 = 1000;

fn text_of(units: usize) -> String {
    UNIT.repeat(units)
}

fn admitted(units: usize) -> Source {
    Source::new(SourceId::new(0), text_of(units)).expect("test text admits")
}

/// One elapsed measurement of `work`, in nanoseconds — floored to 1 so a
/// sub-nanosecond reading can still divide.
fn time_once(mut work: impl FnMut()) -> u128 {
    let start = Instant::now();
    work();
    start.elapsed().as_nanos().max(1)
}

/// The median over SAMPLES interleaved runs of `big`'s cost over
/// `small`'s, scaled by RATIO_SCALE. Each run evaluates `small` then
/// `big` back-to-back; the two closures return that run's cost figure —
/// elapsed nanos for the linear claims, nanos-per-pair for the oracle.
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

/// The non-whitespace tokens of `units` copies of UNIT, parsed — the
/// population the oracle's per-pair cost is measured over.
fn oracle_tokens(units: usize) -> Vec<SyntaxToken> {
    let root = parse(&admitted(units), Dialect::Clingo).syntax();
    let tokens: Vec<SyntaxToken> = root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .collect();
    // The fixture must yield adjacent pairs; without this a degenerate
    // UNIT would underflow the pair count and divide by zero below.
    assert!(
        tokens.len() >= 2,
        "the fixture yielded {} non-whitespace tokens; the oracle needs a pair",
        tokens.len()
    );
    tokens
}

/// The oracle over every adjacent pair of `tokens`, once.
fn sweep(tokens: &[SyntaxToken]) {
    for pair in tokens.windows(2) {
        std::hint::black_box(separator(&pair[0], &pair[1], Dialect::Clingo));
    }
}

#[test]
fn parse_is_linear_in_the_text() {
    let small_source = admitted(64);
    let big_source = admitted(64 * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(parse(&small_source, Dialect::Clingo));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(parse(&big_source, Dialect::Clingo));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "parse's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} text; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn the_certificate_is_linear_in_both_texts() {
    let small_source = admitted(64);
    let big_source = admitted(64 * SIZE_RATIO);
    let small_left = parse(&small_source, Dialect::Clingo);
    let small_right = parse(&small_source, Dialect::Clingo);
    let big_left = parse(&big_source, Dialect::Clingo);
    let big_right = parse(&big_source, Dialect::Clingo);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(equivalent(
                    &small_left,
                    &small_right,
                    Certificate::UpToSpelling,
                ))
                .expect("equal");
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(equivalent(&big_left, &big_right, Certificate::UpToSpelling))
                    .expect("equal");
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "the certificate's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} text; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn bulk_attachment_is_linear_in_the_tree() {
    let small_root = parse(&admitted(64), Dialect::Clingo).syntax();
    let big_root = parse(&admitted(64 * SIZE_RATIO), Dialect::Clingo).syntax();
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(attachments(&small_root).count());
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(attachments(&big_root).count());
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "attachment's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} tree; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn the_oracle_is_constant_per_pair() {
    let small_tokens = oracle_tokens(64);
    let big_tokens = oracle_tokens(64 * SIZE_RATIO);
    let small_pairs = (small_tokens.len() - 1) as u128;
    let big_pairs = (big_tokens.len() - 1) as u128;
    let ratio = median_ratio(
        || time_once(|| sweep(&small_tokens)) / small_pairs,
        || time_once(|| sweep(&big_tokens)) / big_pairs,
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < CONSTANT_CEILING * RATIO_SCALE,
        "the oracle's per-pair cost ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} pairs; constant per pair allows at most x{CONSTANT_CEILING}"
    );
}

/// The first two top-level statements of `root`, as elements — the pair the
/// whitespace-fact shape is measured over.
fn first_pair(root: &SyntaxNode) -> (SyntaxElement, SyntaxElement) {
    let mut nodes = root.children();
    let first = nodes.next().expect("the fixture has a first statement");
    let second = nodes.next().expect("the fixture has a second statement");
    (first.into(), second.into())
}

#[test]
fn the_whitespace_facts_are_constant_in_the_tree() {
    // A whitespace fact reads only the trivia between its two elements, so its
    // cost is independent of the tree's size (docs/design/syntax.md §9.3): the
    // fact between the first two statements of a program and of one SIZE_RATIO
    // larger costs the same. A regression to reading the whole tree —
    // materializing `root.text()` and slicing, whose fold visits every token —
    // shows here as a ratio near SIZE_RATIO, well past the constant ceiling.
    const REPEAT: usize = 512;
    let small_root = parse(&admitted(64), Dialect::Clingo).syntax();
    let big_root = parse(&admitted(64 * SIZE_RATIO), Dialect::Clingo).syntax();
    let (small_a, small_b) = first_pair(&small_root);
    let (big_a, big_b) = first_pair(&big_root);
    let ratio = median_ratio(
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(empty_line_between(
                        std::hint::black_box(&small_a),
                        &small_b,
                    ));
                }
            })
        },
        || {
            time_once(|| {
                for _ in 0..REPEAT {
                    std::hint::black_box(empty_line_between(std::hint::black_box(&big_a), &big_b));
                }
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < CONSTANT_CEILING * RATIO_SCALE,
        "the whitespace fact's cost ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} tree; a tree-size-independent fact allows at most x{CONSTANT_CEILING}"
    );
}
