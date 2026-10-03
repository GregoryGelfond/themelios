//! Shape assertions for the solve tier's boundary (docs/design/solve.md §5.1,
//! §10.2): complexity shape only, held by the median over five interleaved
//! wall-clock ratios with tolerances wide enough for any machine the checks run
//! on. What they prove: the claimed class — Door A's admission near-linear in
//! the parse, for distinct statements and for repeats that every one collide,
//! and the core's display derivation near-linear in a model. What they cannot:
//! absolute speed, which lives in the out-of-band bench (benches/boundary.rs).
//!
//! Each ratio is the median over five runs that time the small case and the
//! large case back-to-back, so a load transient inflates both halves of one run
//! and cancels in its ratio. The parse is built outside the timed window, so an
//! admission ratio measures the admission alone; the display's model clone is
//! linear and inside both halves, so it cancels in the class.

use std::fmt::Write;
use std::time::Instant;

use themelios_base::source::{Source, SourceId};
use themelios_program::program::Show;
use themelios_program::symbol::{Name, Sign, Signature, Symbol};
use themelios_solve::agent::Scenario;
use themelios_solve::bridge::Admitted;
use themelios_solve::contract::Fault;
use themelios_solve::outcome::{Conclusion, Model, Run, ShowRule, Solved};
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
/// The base atom count of the display shape; the large case is SIZE_RATIO
/// more.
const ATOMS: usize = 1_000;

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

/// A run that yields one model, then reports its search closed.
struct OneModel {
    model: Option<Model>,
}

impl Run for OneModel {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.model.take().map(Ok)
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.model.is_none().then_some(Conclusion::Exhausted)
    }
}

/// The unary ground atom `name(i)`.
fn applied(name: &str, i: usize) -> Symbol {
    let i = i32::try_from(i).expect("a small index");
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [Symbol::number(i)],
        Sign::Positive,
    )
}

/// A model of `count` atoms `p(i)` and `count` atoms `q(i)`, with `count`
/// terms `t(i)`.
fn wide_model(count: usize) -> Model {
    let atoms = (0..count)
        .flat_map(|i| [applied("p", i), applied("q", i)])
        .collect();
    Model::of(atoms).with_terms((0..count).map(|i| applied("t", i)))
}

/// The rule of `#show q/1.` — restricting, so the derivation filters every
/// atom.
fn showing_q() -> ShowRule {
    let name = Name::new("q").expect("a valid identifier");
    ShowRule::of([&Show::Signature(Signature {
        sign: Sign::Positive,
        name,
        arity: 1,
    })])
}

#[test]
fn deriving_a_display_is_near_linear() {
    // The core filters the answer set through the rule — one allocation-free
    // lookup per atom — and unions the terms, each an ordered-set walk:
    // O(|M| log |M|). A lookup that allocated or scanned the rule per atom, or a
    // union re-sorting per insertion, breaks the class. The model's clone is
    // linear and inside both halves, so it cancels in the class.
    let rule = showing_q();
    let (small, big) = (wide_model(ATOMS), wide_model(ATOMS * SIZE_RATIO));
    let stream = |model: &Model| {
        let run = OneModel {
            model: Some(model.clone()),
        };
        let mut solved = Solved::running(Box::new(run), Scenario::default(), rule.clone());
        drop(std::hint::black_box(solved.models().next()));
    };
    let ratio = median_ratio(
        || time_once(|| stream(&small)),
        || time_once(|| stream(&big)),
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "deriving a display: the median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over \
         x{SIZE_RATIO} atoms; the near-linear shape allows at most x{LINEAR_CEILING}"
    );
}
