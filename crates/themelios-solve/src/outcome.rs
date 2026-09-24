//! The outcome vocabulary (docs/design/solve.md §5): the models and their
//! views — determination, conclusion, the solved outcome, models, consequences,
//! and unsatisfiability.
//!
//! Two closed distinctions open the vocabulary (§5.1). A [`Determination`]
//! answers the logical question — is the program consistent? — as a closed
//! trichotomy whose every variant carries its evidence; a [`Conclusion`]
//! answers the search question — how did the search end? — and is kept apart
//! from it by design, because an engine's own result vocabulary conflates the
//! two. Rust's `Result` forecloses the obvious alternative: an inconclusive
//! search is a value, what the search did establish, never an error and never
//! "no". The answer set itself is the program tier's [`AnswerSet`],
//! re-exported so the program, solve, and query tiers speak one answer-set
//! vocabulary.

use std::marker::PhantomData;

pub use themelios_program::AnswerSet;

// ---- The closed distinctions (§5.1) ----

/// The logical question: is the program consistent? A closed trichotomy,
/// deliberately not `#[non_exhaustive]` — the closed set is the affordance
/// that forbids a fourth reading (docs/design/solve.md §5.1), so a reading
/// that names the three variants needs no fallback arm. The variants are
/// closed; their payloads are the surface that may grow. The `Consistent`
/// payload is a view over the live engine (§5.2), so the trichotomy carries
/// that borrow, `'a`.
pub enum Determination<'a> {
    /// The program has an answer set: read the answer sets, or open the world
    /// view the query tier reads, through the [`Models`] (§5.2).
    Consistent(Models<'a>),
    /// The program has no answer set; for an assumption-scoped solve the
    /// payload carries the blame (§5.4).
    Inconsistent(Unsat),
    /// The search stopped before deciding: what a truncated search did
    /// establish, as a value — never "no".
    Inconclusive(Partial),
}

/// The search question: how did the search end? Closed, and separate from
/// the logical question by design (docs/design/solve.md §5.1): one
/// conclusion, orthogonal to the [`Determination`], with no second flag to
/// disagree with (§5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conclusion {
    /// The search closed the space: every answer set there is was seen, or
    /// the absence of any was proved.
    Exhausted,
    /// The search met the target the request set — the model-count cap §6.3
    /// leaves room for — and stopped there, the space not closed.
    Target,
    /// The search hit the request's budget (§6.3) — reported as what it is,
    /// never as a clean end.
    Budget,
    /// The search was cancelled through the interrupt handle (§6.3).
    Interrupted,
}

/// What a truncated search did establish — the `Inconclusive` payload
/// (docs/design/solve.md §5.1). Non-exhaustive: a payload that may grow. It
/// carries at least the [`Conclusion`] the search reached — the one thing an
/// inconclusive outcome did settle — so whoever holds it can read why the
/// search stopped.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Partial {
    pub(crate) conclusion: Conclusion,
}

impl Partial {
    /// Why the search stopped. Total; O(1).
    pub fn conclusion(&self) -> Conclusion {
        self.conclusion
    }
}

// ---- Answer sets, optima, consequences (§5.2) ----

/// The run handle `solve` returns, borrowing its engine for `'a`. Reserved;
/// its surface is defined with §5.2.
pub struct Solved<'a> {
    _engine: PhantomData<&'a ()>,
}

/// The run handle `optimize` returns, borrowing its engine for `'a`. Reserved;
/// its surface is defined with §5.2.
pub struct Optimized<'a> {
    _engine: PhantomData<&'a ()>,
}

/// Cautious or brave consequences: a set of ground symbols carrying the mode
/// that produced it. Reserved; its surface is defined with §5.2.
pub struct Consequences;

/// The `Consistent` payload (docs/design/solve.md §5.2): the live-engine-access
/// handle, borrowing the engine for `'a` and owning it when `'a` is `'static`.
/// Reserved; its reading surface is defined with the run it is read from
/// (§5.2).
pub struct Models<'a> {
    pub(crate) _engine: PhantomData<&'a mut ()>,
}

/// The `Inconsistent` payload (docs/design/solve.md §5.1): the program has no
/// answer set, and an assumption-scoped solve carries its blame here (§5.4).
/// Reserved; its surface is defined with §5.4.
pub struct Unsat {
    pub(crate) _private: (),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed set of conclusions, each beside its rendering.
    const CONCLUSIONS: [(Conclusion, &str); 4] = [
        (Conclusion::Exhausted, "Exhausted"),
        (Conclusion::Target, "Target"),
        (Conclusion::Budget, "Budget"),
        (Conclusion::Interrupted, "Interrupted"),
    ];

    /// The conclusions of a search that did not close the space — the ones a
    /// partial can carry.
    const TRUNCATING: [Conclusion; 3] = [
        Conclusion::Target,
        Conclusion::Budget,
        Conclusion::Interrupted,
    ];

    #[test]
    fn a_conclusion_s_debug_view_names_its_variant() {
        for (conclusion, rendered) in CONCLUSIONS {
            assert_eq!(format!("{conclusion:?}"), rendered);
        }
    }

    #[test]
    fn distinct_conclusions_are_unequal() {
        for (position, (left, _)) in CONCLUSIONS.iter().enumerate() {
            for (right, _) in CONCLUSIONS.iter().skip(position + 1) {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn a_partial_reports_the_conclusion_it_was_built_with() {
        for conclusion in TRUNCATING {
            let partial = Partial { conclusion };
            assert_eq!(partial.conclusion(), conclusion);
        }
    }

    #[test]
    fn a_cloned_partial_equals_its_original() {
        let partial = Partial {
            conclusion: Conclusion::Budget,
        };
        assert_eq!(partial.clone(), partial);
    }

    #[test]
    fn a_partial_s_debug_view_names_its_conclusion() {
        let partial = Partial {
            conclusion: Conclusion::Interrupted,
        };
        let rendered = format!("{partial:?}");
        assert!(rendered.contains("Interrupted"), "{rendered}");
    }

    /// The three-valued reading of the trichotomy — decided true, decided
    /// false, or not decided — the reading under which an inconclusive
    /// search is never "no" (§5.1).
    fn decided(determination: &Determination<'_>) -> Option<bool> {
        match determination {
            Determination::Consistent(_) => Some(true),
            Determination::Inconsistent(_) => Some(false),
            Determination::Inconclusive(_) => None,
        }
    }

    /// The payloads are built here, in the defining crate, as the engine
    /// will build them.
    fn some_models<'a>() -> Models<'a> {
        Models {
            _engine: PhantomData,
        }
    }

    fn some_unsat() -> Unsat {
        Unsat { _private: () }
    }

    #[test]
    fn a_consistent_determination_is_decided_true() {
        let consistent = Determination::Consistent(some_models());
        assert_eq!(decided(&consistent), Some(true));
    }

    #[test]
    fn an_inconsistent_determination_is_decided_false() {
        let inconsistent = Determination::Inconsistent(some_unsat());
        assert_eq!(decided(&inconsistent), Some(false));
    }

    #[test]
    fn an_inconclusive_determination_is_not_decided() {
        for conclusion in TRUNCATING {
            let inconclusive = Determination::Inconclusive(Partial { conclusion });
            assert_eq!(decided(&inconclusive), None, "{conclusion:?}");
        }
    }

    #[test]
    fn an_inconsistent_determination_pairs_with_every_conclusion() {
        // The logical and the search questions are answered apart (§5.1,
        // §5.3): no answer set, however the search ended.
        for (ended, _) in CONCLUSIONS {
            let (determination, conclusion) = (Determination::Inconsistent(some_unsat()), ended);
            assert_eq!(decided(&determination), Some(false), "{conclusion:?}");
        }
    }
}
