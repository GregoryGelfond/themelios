//! Shape assertions for the solve tier's boundary (docs/design/solve.md §10.2):
//! complexity shape only, held by the median over five interleaved wall-clock
//! ratios with tolerances wide enough for any machine the checks run on. What
//! they prove: the claimed class — Door A's admission near-linear in the parse,
//! for distinct statements and for repeats that every one collide. What they
//! cannot: absolute speed, which lives in the out-of-band bench
//! (benches/boundary.rs).
//!
//! Each ratio is the median over five runs that time the small case and the
//! large case back-to-back, so a load transient inflates both halves of one run
//! and cancels in its ratio. The parse is built outside the timed window, so a
//! ratio measures the admission alone.

use std::fmt::Write;
use std::time::Instant;

use themelios_base::source::{Source, SourceId};
use themelios_solve::bridge::Admitted;
use themelios_syntax::{Dialect, Parse, ast, parse};

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: usize = 16;
/// A near-linear claim at SIZE_RATIO may cost at most this factor: fourfold
/// noise headroom above linear (x16) and fourfold separation below quadratic
/// (x256).
const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
/// Interleaved runs per measurement; the median of their ratios is taken.
const SAMPLES: usize = 5;
/// Ratios are scaled by this factor so the median arithmetic stays in integers;
/// a ceiling `C` is the scaled bound `C * RATIO_SCALE`.
const RATIO_SCALE: u128 = 1000;
/// The base fact count of the admission shapes; the large case is SIZE_RATIO
/// more.
const FACTS: usize = 512;

/// One elapsed measurement of `work`, in nanoseconds — floored to 1 so a
/// sub-nanosecond reading can still divide.
fn time_once(mut work: impl FnMut()) -> u128 {
    let start = Instant::now();
    work();
    start.elapsed().as_nanos().max(1)
}

/// The median over SAMPLES interleaved runs of `big`'s cost over `small`'s,
/// scaled by RATIO_SCALE. Each run evaluates `small` then `big` back-to-back.
fn median_ratio(mut small: impl FnMut() -> u128, mut big: impl FnMut() -> u128) -> u128 {
    let mut ratios = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let small = small().max(1);
        let big = big();
        ratios.push(big * RATIO_SCALE / small);
    }
    ratios.sort_unstable();
    ratios[SAMPLES / 2]
}

/// The parse of `text` in the clingo dialect.
fn parsed(text: &str) -> Parse<ast::Program> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    parse(&source, Dialect::Clingo)
}

/// `count` distinct facts.
fn distinct_facts(count: usize) -> String {
    let mut text = String::new();
    for i in 0..count {
        write!(text, "p({i}). ").expect("writing to a string does not fail");
    }
    text
}

/// Assert that admitting `big` costs at most the linear ceiling over admitting
/// `small`.
fn assert_admission_near_linear(shape: &str, small: &str, big: &str) {
    let (small, big) = (parsed(small), parsed(big));
    let ratio = median_ratio(
        || time_once(|| drop(std::hint::black_box(Admitted::of(&small)))),
        || time_once(|| drop(std::hint::black_box(Admitted::of(&big)))),
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "admitting {shape}: the median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over \
         x{SIZE_RATIO} facts; the near-linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn admitting_distinct_facts_is_near_linear() {
    assert_admission_near_linear(
        "distinct facts",
        &distinct_facts(FACTS),
        &distinct_facts(FACTS * SIZE_RATIO),
    );
}

#[test]
fn admitting_repeated_facts_is_near_linear() {
    // Every repeat collides in the collection, which merges by move.
    assert_admission_near_linear(
        "repeated facts",
        &"a. ".repeat(FACTS),
        &"a. ".repeat(FACTS * SIZE_RATIO),
    );
}
