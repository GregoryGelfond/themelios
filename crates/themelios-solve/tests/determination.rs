//! Laws of the outcome vocabulary's closed distinctions (docs/design/solve.md
//! §5.1): the determination is a closed trichotomy, so a reading that names
//! its three variants needs no fallback arm; the conclusion is a closed answer
//! to the search question, kept apart from the logical one, and only an
//! exhausted one closed the space; and an answer set is the program tier's own
//! set of ground symbols — one vocabulary across the tiers.

use std::collections::BTreeSet;
use std::fmt::Debug;

use themelios_program::{Name, Symbol};
use themelios_solve::outcome::{AnswerSet, Conclusion, Determination};

/// The conclusions of a search that did not close the space.
const TRUNCATING: [Conclusion; 3] = [
    Conclusion::Target,
    Conclusion::Budget,
    Conclusion::Interrupted,
];

#[test]
fn the_determination_is_a_closed_trichotomy() {
    // A reading with no `_` arm: the closed set is the affordance that forbids
    // a fourth reading (§5.1) — were a fourth variant ever added, this reading
    // would fail to compile. The payloads are the engine's to build, so the
    // proposition is the reading's compiling, not a value's.
    fn reading(determination: &Determination<'_>) -> &'static str {
        match determination {
            Determination::Consistent(_) => "consistent",
            Determination::Inconsistent(_) => "inconsistent",
            Determination::Inconclusive(_) => "inconclusive",
        }
    }
    let _: fn(&Determination<'_>) -> &'static str = reading;
}

#[test]
fn only_an_exhausted_conclusion_closed_the_space() {
    // A reading with no `_` arm — the conclusion is a closed set — and the
    // exhaustion gate's premise (§5.2, §5.3): a complete collection exists
    // only behind `Exhausted`.
    fn closed_the_space(conclusion: Conclusion) -> bool {
        match conclusion {
            Conclusion::Exhausted => true,
            Conclusion::Target | Conclusion::Budget | Conclusion::Interrupted => false,
        }
    }
    assert!(closed_the_space(Conclusion::Exhausted));
    for conclusion in TRUNCATING {
        assert!(!closed_the_space(conclusion), "{conclusion:?}");
    }
}

#[test]
fn a_conclusion_is_owned_plain_data() {
    // A conclusion travels with the outcome it describes, across threads and
    // service boundaries alike.
    fn plain<T: Send + Sync + Copy + Eq + Debug + 'static>() {}
    plain::<Conclusion>();
}

#[test]
fn an_answer_set_is_a_symbol_set() {
    // The alias is the program tier's `BTreeSet<Symbol>` (program.md §11.3),
    // re-exported unchanged: a set of ground symbols is an answer set.
    let mut answer_set: AnswerSet = BTreeSet::<Symbol>::new();
    assert!(answer_set.is_empty());
    let witness = Symbol::constant(Name::new("sky").expect("an identifier"));
    assert!(answer_set.insert(witness.clone()));
    assert!(answer_set.contains(&witness));
}
