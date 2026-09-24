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

use crate::agent::Scenario;
use crate::contract::Fault;
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
    /// The search was cut short before closing the space — cancelled through the
    /// interrupt handle (§6.3), or, provisionally, stopped by an engine fault
    /// (see [`Partial::faulted`]).
    Interrupted,
}

impl std::fmt::Display for Conclusion {
    /// A human phrase — the reading a diagnostic shows, not the variant name
    /// (docs/design/solve.md §5.4: a model has a human `Display`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Conclusion::Exhausted => "the search closed the space",
            Conclusion::Target => "the search met its target",
            Conclusion::Budget => "the search hit its budget",
            Conclusion::Interrupted => "the search was interrupted",
        })
    }
}

/// What a truncated search did establish — the `Inconclusive` payload
/// (docs/design/solve.md §5.1). Non-exhaustive: a payload that may grow. It
/// carries at least the [`Conclusion`] the search reached — the one thing an
/// inconclusive outcome did settle — so whoever holds it can read why the
/// search stopped, and the engine fault that stopped it where one did.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Partial {
    pub(crate) conclusion: Conclusion,
    pub(crate) cause: Option<Fault>,
}

impl Partial {
    /// A truncated (budget / target / interrupt) search with no fault.
    pub(crate) fn truncated(conclusion: Conclusion) -> Partial {
        Partial {
            conclusion,
            cause: None,
        }
    }

    /// A search stopped by an engine fault before it decided — the fault is
    /// retained (via [`Partial::cause`]), never laundered into a plain
    /// truncation. The reported [`Conclusion`] is provisionally `Interrupted` —
    /// the closest of the closed set to "stopped without deciding"; no
    /// `Conclusion` yet names an engine fault, a refinement left reserved.
    pub(crate) fn faulted(cause: Fault) -> Partial {
        Partial {
            conclusion: Conclusion::Interrupted,
            cause: Some(cause),
        }
    }

    /// Why the search stopped. Total; O(1).
    pub fn conclusion(&self) -> Conclusion {
        self.conclusion
    }

    /// The engine fault that stopped the search, where one did — `None` for a
    /// plain budget / target / interrupt truncation. Total; O(1).
    pub fn cause(&self) -> Option<&Fault> {
        self.cause.as_ref()
    }
}

// ---- Answer sets, optima, consequences (§5.2) ----

/// The internal streaming/terminal-state protocol a backend's solve drives. A
/// run holds only its own enumeration state and, where an engine is involved, a
/// raw handle into it — never a Rust borrow of the backend — so the live handle,
/// not the borrow checker, serialises engine access (docs/design/solve.md §5.2).
/// Because a run carries a raw engine handle, a `Solved`/`Models` built over one
/// is `!Send` for every backend — a deliberate consequence (the pinned engine's
/// control is single-threaded), not an accident.
///
/// Two obligations every implementor owes, which the live handle relies on when
/// it re-polls a spent run (after an inspecting resolve, a second stream, a
/// completeness drain):
/// - **Fused**: once `next_answer_set` has returned `None`, it returns `None`
///   forever.
/// - **Terminal conclusion**: once `next_answer_set` has returned `None`,
///   `conclusion` returns `Some`; it is `None` only while the search is open.
pub(crate) trait Run {
    /// The next answer set, or `None` at the end of the search (fused — see the
    /// trait obligations). Each item a `Result`, so a mid-stream engine fault
    /// surfaces at the item, not as a clean end.
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>>;
    /// How the search ended — `None` while it is still open, `Some` once
    /// `next_answer_set` has returned `None`.
    fn conclusion(&self) -> Option<Conclusion>;
}

/// How far the one enumeration behind a live handle has been drained. The
/// exhaustion gate reads it (docs/design/solve.md §5.3): a complete collection
/// comes only from a `Fresh` handle drained to its end — touching the stream
/// forfeits it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DrainState {
    /// Untouched: the search may still be drained to a complete collection.
    Fresh,
    /// The stream has been touched; completeness is forfeit.
    MidDrain,
    /// Drained to the end.
    Drained,
}

/// The live run behind a [`Solved`] or [`Models`]: the engine (owned or
/// borrowed), the in-flight enumeration with a one-model `lookahead`, the
/// scenario it ranged over, and the drain state. The live handle — not the
/// borrow checker — is the exclusivity lock (docs/design/solve.md §5.2): reading
/// the engine closes the current enumeration first, so a live query read
/// re-solves and re-establishes a run rather than opening a second solve on an
/// engine with an enumeration already live. `current` is declared before
/// `engine` so it drops first — a run may hold a raw handle into the engine.
pub(crate) struct LiveRun<'a> {
    current: Option<Box<dyn Run + 'a>>,
    scenario: Scenario,
    lookahead: Option<Result<AnswerSet, Fault>>,
    drain: DrainState,
    // Whether any model has ever been pulled from the run. The trichotomy is a
    // property of the program, not of how much has been consumed, so a search
    // that has yielded a model stays consistent after its stream is drained.
    witnessed: bool,
    // The engine fault that ended the search, remembered so a handle that
    // survives an inspecting read (the borrowing resolver) keeps reporting the
    // fault rather than forgetting it once its pending item is taken.
    faulted: Option<Fault>,
    // The engine — owned (a bare program's ephemeral engine, §6.4) or borrowed
    // from a retained agent (§6.2) — is held here with the native consequence
    // door (§4.2); until then the handle reserves its invariant lifetime slot.
    _engine: PhantomData<&'a mut ()>,
}

/// The trichotomy a live run reads off, before each arm is wrapped with its
/// evidence (docs/design/solve.md §5.1).
enum Class {
    Consistent,
    Inconsistent,
    Inconclusive(Conclusion),
    /// A resolve-time engine fault before any model — the fault retained.
    Faulted(Fault),
}

impl LiveRun<'_> {
    /// Pull the next item from the run, remembering a witnessed model. The single
    /// point every member flows through, so `witnessed` is always current.
    fn pull(&mut self) -> Option<Result<AnswerSet, Fault>> {
        let item = self.current.as_mut()?.next_answer_set();
        match &item {
            Some(Ok(_)) => self.witnessed = true,
            Some(Err(fault)) => {
                self.faulted.get_or_insert_with(|| fault.clone());
            }
            None => {}
        }
        item
    }

    /// The next answer set, yielding the one-model `lookahead` first so no member
    /// peeked to resolve the trichotomy is lost.
    fn next(&mut self) -> Option<Result<AnswerSet, Fault>> {
        if let Some(peeked) = self.lookahead.take() {
            return Some(peeked);
        }
        self.pull()
    }

    /// The streaming pull `answer_sets`/`members` yield through: completeness is
    /// forfeit on the FIRST pull (not at iterator creation), then the member is
    /// yielded.
    fn stream_next(&mut self) -> Option<Result<AnswerSet, Fault>> {
        if self.drain == DrainState::Fresh {
            self.drain = DrainState::MidDrain;
        }
        self.next()
    }

    /// How the search ended — `None` until it reaches its end.
    fn conclusion(&self) -> Option<Conclusion> {
        self.current.as_ref().and_then(|run| run.conclusion())
    }

    /// Read off the trichotomy (docs/design/solve.md §5.1). Consistent iff a model
    /// has ever been witnessed — the reading is a property of the program, not of
    /// how much has been consumed, so a drained consistent search stays
    /// Consistent. With no witness, peek one member: a model makes it Consistent,
    /// a fault makes it Faulted (retained, never a false "yes"), a clean end reads
    /// the conclusion — Exhausted with no model is Inconsistent, a truncation is
    /// Inconclusive.
    fn classify(&mut self) -> Class {
        // A witnessed model or a remembered fault decides the reading whatever
        // the handle has since consumed, so repeated inspection is stable. A
        // fault seen before any model takes precedence (Inconclusive), even if a
        // later pull would have yielded one.
        if self.witnessed {
            return Class::Consistent;
        }
        if let Some(fault) = self.faulted.clone() {
            return Class::Faulted(fault);
        }
        if self.lookahead.is_none() {
            self.lookahead = self.pull();
        }
        if self.witnessed {
            return Class::Consistent;
        }
        if let Some(fault) = self.faulted.clone() {
            return Class::Faulted(fault);
        }
        // No model and no fault: the search reached a clean end.
        match self.conclusion() {
            Some(Conclusion::Exhausted) => Class::Inconsistent,
            Some(truncating) => Class::Inconclusive(truncating),
            // A conforming run reports its conclusion once ended (the `Run`
            // fused/terminal obligation); this is the defensive fallback for a
            // run that violates it — never "yes", never "no".
            None => Class::Inconclusive(Conclusion::Interrupted),
        }
    }

    /// The exhaustion gate, shared by `Solved::all_answer_sets` and
    /// `Models::all_members` (docs/design/solve.md §5.3): a complete collection
    /// ONLY from an untouched handle whose search closed the space; refuses
    /// otherwise, keeping a mid-stream fault as the cause.
    fn drain_complete(&mut self) -> Result<Vec<AnswerSet>, NotExhausted> {
        if self.drain != DrainState::Fresh {
            return Err(NotExhausted::already_taken(self.conclusion()));
        }
        // A search that has already faulted cannot yield a complete collection —
        // refuse with the remembered fault (cause preserved on every attempt),
        // without draining, so a handle that pulled nothing is not marked touched.
        if let Some(fault) = self.faulted.clone() {
            return Err(NotExhausted::faulted(self.conclusion(), fault));
        }
        let mut all = Vec::new();
        loop {
            match self.next() {
                Some(Ok(set)) => all.push(set),
                Some(Err(fault)) => {
                    // Models were pulled and dropped: the handle is no longer
                    // untouched, so a re-drain refuses rather than returning a
                    // collection missing them.
                    self.drain = DrainState::MidDrain;
                    return Err(NotExhausted::faulted(self.conclusion(), fault));
                }
                None => break,
            }
        }
        self.drain = DrainState::Drained;
        if self.conclusion() == Some(Conclusion::Exhausted) {
            Ok(all)
        } else {
            Err(NotExhausted::not_closed(self.conclusion()))
        }
    }
}

impl<'a> Determination<'a> {
    /// Resolve an owned live run into the trichotomy (consuming), threading the
    /// engine borrow `'a` — the resolver the agent and bare conveniences build
    /// over (docs/design/solve.md §5.2/§6.4).
    pub(crate) fn of_live(mut live: LiveRun<'a>) -> Determination<'a> {
        match live.classify() {
            Class::Consistent => Determination::Consistent(Models::owned(live)),
            Class::Inconsistent => Determination::Inconsistent(Unsat { _private: () }),
            Class::Inconclusive(conclusion) => {
                Determination::Inconclusive(Partial::truncated(conclusion))
            }
            Class::Faulted(fault) => Determination::Inconclusive(Partial::faulted(fault)),
        }
    }

    /// Resolve a borrowed live run into the trichotomy (reborrow) — inspect in
    /// place, bounded by the borrow (docs/design/solve.md §5.2).
    pub(crate) fn of_live_ref<'b>(live: &'b mut LiveRun<'a>) -> Determination<'b> {
        match live.classify() {
            Class::Consistent => Determination::Consistent(Models::borrowed(live)),
            Class::Inconsistent => Determination::Inconsistent(Unsat { _private: () }),
            Class::Inconclusive(conclusion) => {
                Determination::Inconclusive(Partial::truncated(conclusion))
            }
            Class::Faulted(fault) => Determination::Inconclusive(Partial::faulted(fault)),
        }
    }
}

/// The completeness refusal: a complete collection was asked of a search that did
/// not close the space (docs/design/solve.md §5.3). Carries the [`Conclusion`]
/// the search reached and — for a mid-stream engine fault — the fault itself, so
/// a truncation and a fault are not laundered into one anonymous refusal.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotExhausted {
    pub(crate) conclusion: Option<Conclusion>,
    pub(crate) cause: Option<Fault>,
    /// True when the handle's answer sets were already taken (its stream was
    /// touched), as distinct from a search that ran but did not close the space.
    pub(crate) already_taken: bool,
}

impl NotExhausted {
    /// The handle's stream was already touched, so a complete collection is no
    /// longer available from it.
    pub(crate) fn already_taken(conclusion: Option<Conclusion>) -> NotExhausted {
        NotExhausted {
            conclusion,
            cause: None,
            already_taken: true,
        }
    }

    /// The search ran to its end but did not close the space.
    pub(crate) fn not_closed(conclusion: Option<Conclusion>) -> NotExhausted {
        NotExhausted {
            conclusion,
            cause: None,
            already_taken: false,
        }
    }

    /// A mid-stream engine fault stopped the drain — the fault is the cause.
    pub(crate) fn faulted(conclusion: Option<Conclusion>, cause: Fault) -> NotExhausted {
        NotExhausted {
            conclusion,
            cause: Some(cause),
            already_taken: false,
        }
    }
}

impl std::fmt::Display for NotExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.already_taken {
            return f.write_str(
                "the answer sets were already taken from this handle; a complete collection is unavailable",
            );
        }
        match self.conclusion {
            Some(conclusion) => write!(f, "{conclusion}; a complete collection is unavailable"),
            None => f.write_str(
                "the search did not close the space; a complete collection is unavailable",
            ),
        }
    }
}

impl std::error::Error for NotExhausted {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .map(|fault| fault as &(dyn std::error::Error + 'static))
    }
}

/// The borrowed run handle `solve` returns: stream the answer sets, inspect or
/// resolve the trichotomy, read the conclusion. Read by `&mut` because draining
/// the stream is stateful — the terminal `conclusion` is readable only after the
/// drain reaches the end (docs/design/solve.md §5.2).
pub struct Solved<'a> {
    live: LiveRun<'a>,
}

impl<'a> Solved<'a> {
    /// The lazy answer-set stream; each item a `Result`, so a mid-stream engine
    /// fault surfaces at the item. Touching it forfeits completeness. Cost: O(1)
    /// resident.
    pub fn answer_sets(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_ {
        std::iter::from_fn(|| self.live.stream_next())
    }

    /// A COMPLETE collection — available ONLY from an untouched handle whose search
    /// closed the space; refuses otherwise (the exhaustion gate, §5.3). This is
    /// what makes a truncated search structurally unable to pass as complete.
    pub fn all_answer_sets(&mut self) -> Result<Vec<AnswerSet>, NotExhausted> {
        self.live.drain_complete()
    }

    /// How the search ended — readable once it resolves.
    pub fn conclusion(&self) -> Option<Conclusion> {
        self.live.conclusion()
    }

    /// INSPECT the run in place (reborrow): the trichotomy, bounded by the borrow
    /// (docs/design/solve.md §5.2).
    pub fn determination(&mut self) -> Determination<'_> {
        Determination::of_live_ref(&mut self.live)
    }

    /// RESOLVE the run into the trichotomy (consuming), threading the engine
    /// borrow `'a` so a `Consistent` world view outlives to the agent's borrow
    /// (docs/design/solve.md §5.2).
    pub fn into_determination(self) -> Determination<'a> {
        Determination::of_live(self.live)
    }
}

/// The run handle `optimize` returns, borrowing its engine for `'a`. Reserved;
/// its surface is defined with §5.2.
pub struct Optimized<'a> {
    _engine: PhantomData<&'a ()>,
}

/// Cautious or brave consequences: a set of ground symbols carrying the mode
/// that produced it. Reserved; its surface is defined with §5.2.
pub struct Consequences;

/// The lifetime-erased view of a live run that a [`Models`] holds, so `Models<'a>`
/// carries a single lifetime — the access — while the engine's own lifetime lives
/// behind the trait object. [`LiveRun`] is the only implementor; a reborrowed
/// `&mut LiveRun` unsize-coerces to `&mut dyn RunAccess`, which is what lets the
/// inspecting resolver hand back a borrowed `Models` without a second lifetime on
/// the public type.
pub(crate) trait RunAccess {
    fn stream_next(&mut self) -> Option<Result<AnswerSet, Fault>>;
    fn conclusion(&self) -> Option<Conclusion>;
    fn scenario(&self) -> &Scenario;
    fn drain_complete(&mut self) -> Result<Vec<AnswerSet>, NotExhausted>;
}

impl RunAccess for LiveRun<'_> {
    fn stream_next(&mut self) -> Option<Result<AnswerSet, Fault>> {
        LiveRun::stream_next(self)
    }
    fn conclusion(&self) -> Option<Conclusion> {
        LiveRun::conclusion(self)
    }
    fn scenario(&self) -> &Scenario {
        &self.scenario
    }
    fn drain_complete(&mut self) -> Result<Vec<AnswerSet>, NotExhausted> {
        LiveRun::drain_complete(self)
    }
}

/// The `Consistent` payload (docs/design/solve.md §5.2): the live-engine-access
/// handle the query tier reads. It OWNS the live run — a bare program's ephemeral
/// engine (§6.4) — or BORROWS it from a retained `Solved`. It exposes the raw
/// material a world view drives (the member stream, `is_exhausted`, the
/// `scenario`); the `Models<'a> → WorldView<'a>` transition lives on the query
/// side (query.md §2.7), so this carries no world view of its own. Reached only
/// from a `Consistent` reading — the search has, or had, at least one model,
/// though its members may already have been streamed away.
#[non_exhaustive]
pub struct Models<'a> {
    access: ModelsAccess<'a>,
}

enum ModelsAccess<'a> {
    Owned(Box<dyn RunAccess + 'a>),
    Borrowed(&'a mut dyn RunAccess),
}

impl<'a> Models<'a> {
    /// From a consuming resolve: the models own the live run.
    pub(crate) fn owned(live: LiveRun<'a>) -> Models<'a> {
        Models {
            access: ModelsAccess::Owned(Box::new(live)),
        }
    }

    /// From an inspecting reborrow: the models borrow the live run for `'a` (the
    /// engine's own lifetime `'e` is longer, erased behind the trait object).
    pub(crate) fn borrowed<'e: 'a>(live: &'a mut LiveRun<'e>) -> Models<'a> {
        Models {
            access: ModelsAccess::Borrowed(live),
        }
    }

    fn access(&mut self) -> &mut dyn RunAccess {
        match &mut self.access {
            ModelsAccess::Owned(live) => live.as_mut(),
            ModelsAccess::Borrowed(live) => &mut **live,
        }
    }

    fn access_ref(&self) -> &dyn RunAccess {
        match &self.access {
            ModelsAccess::Owned(live) => live.as_ref(),
            ModelsAccess::Borrowed(live) => &**live,
        }
    }

    /// Stream the members; each item a `Result`, so a mid-stream engine fault
    /// surfaces at the item. Touching the stream forfeits completeness (on the
    /// first pull).
    pub fn members(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_ {
        std::iter::from_fn(|| self.access().stream_next())
    }

    /// A COMPLETE collection of the members — available ONLY from an untouched
    /// handle whose search closed the space; refuses otherwise (the exhaustion
    /// gate, §5.3), so the query tier's `materialize` cannot launder a partial
    /// set as complete (query.md §3.2).
    pub fn all_members(&mut self) -> Result<Vec<AnswerSet>, NotExhausted> {
        self.access().drain_complete()
    }

    /// Whether the search closed the space — the `WorldView::is_exhausted` analog
    /// (docs/design/solve.md §5.3).
    pub fn is_exhausted(&self) -> bool {
        self.access_ref().conclusion() == Some(Conclusion::Exhausted)
    }

    /// The scenario the models range over (query.md §2.3).
    pub fn scenario(&self) -> &Scenario {
        self.access_ref().scenario()
    }
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
            let partial = Partial::truncated(conclusion);
            assert_eq!(partial.conclusion(), conclusion);
        }
    }

    #[test]
    fn a_truncated_partial_carries_no_fault() {
        assert!(Partial::truncated(Conclusion::Budget).cause().is_none());
    }

    #[test]
    fn a_faulted_partial_retains_its_fault() {
        let partial = Partial::faulted(Fault::engine("engine died"));
        assert!(partial.cause().is_some());
    }

    #[test]
    fn a_cloned_partial_equals_its_original() {
        let partial = Partial::truncated(Conclusion::Budget);
        assert_eq!(partial.clone(), partial);
    }

    #[test]
    fn a_partial_s_debug_view_names_its_conclusion() {
        let partial = Partial::truncated(Conclusion::Interrupted);
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

    /// How many members of an unbounded search a laziness proof takes.
    const STREAM_PREFIX: usize = 8;

    /// A stub engine-free run: a fixed answer-set sequence, and a terminal
    /// conclusion known only after the drain reaches its end — a lazy engine does
    /// not know its conclusion until the search reaches its end.
    struct StubRun {
        sets: std::vec::IntoIter<AnswerSet>,
        terminal: Conclusion,
        drained: bool,
    }

    impl StubRun {
        fn new(sets: Vec<AnswerSet>, terminal: Conclusion) -> StubRun {
            StubRun {
                sets: sets.into_iter(),
                terminal,
                drained: false,
            }
        }
    }

    impl Run for StubRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            if let Some(set) = self.sets.next() {
                Some(Ok(set))
            } else {
                self.drained = true;
                None
            }
        }
        fn conclusion(&self) -> Option<Conclusion> {
            self.drained.then_some(self.terminal)
        }
    }

    /// A run that yields an engine fault at its first pull.
    struct FaultyRun;
    impl Run for FaultyRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            Some(Err(Fault::engine("stub engine fault")))
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A malformed run that ends at once and never reports a conclusion.
    struct SilentRun;
    impl Run for SilentRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            None
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A run that yields its `models`, then a single engine fault mid-stream.
    struct MidFaultRun {
        models: std::vec::IntoIter<AnswerSet>,
        faulted: bool,
    }
    impl MidFaultRun {
        fn new(models: Vec<AnswerSet>) -> MidFaultRun {
            MidFaultRun {
                models: models.into_iter(),
                faulted: false,
            }
        }
    }
    impl Run for MidFaultRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            if let Some(model) = self.models.next() {
                Some(Ok(model))
            } else if self.faulted {
                None
            } else {
                self.faulted = true;
                Some(Err(Fault::engine("stub engine fault mid-stream")))
            }
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A run that panics if pulled past `limit` — a deterministic laziness proof:
    /// a collect-then-yield handle over-pulls and the test goes red, where an
    /// unbounded run would merely hang.
    struct BoundedRun {
        pulls: usize,
        limit: usize,
    }
    impl Run for BoundedRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            self.pulls += 1;
            assert!(
                self.pulls <= self.limit,
                "the stream over-pulled — not lazy"
            );
            Some(Ok(singleton(0)))
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A run that faults at its first pull, then ends with a clean conclusion —
    /// a search whose only word was an engine fault.
    struct FaultThenEndRun {
        step: usize,
    }
    impl Run for FaultThenEndRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            self.step += 1;
            if self.step == 1 {
                Some(Err(Fault::engine("first-pull fault")))
            } else {
                None
            }
        }
        fn conclusion(&self) -> Option<Conclusion> {
            (self.step >= 2).then_some(Conclusion::Exhausted)
        }
    }

    /// A run that yields a model, then a fault, then recovers with a second model
    /// and a clean end — the shape a fault-aborted complete-collect must not
    /// silently complete over.
    struct RecoveringRun {
        step: usize,
    }
    impl Run for RecoveringRun {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            self.step += 1;
            match self.step {
                1 => Some(Ok(singleton(0))),
                2 => Some(Err(Fault::engine("transient fault"))),
                3 => Some(Ok(singleton(1))),
                _ => None,
            }
        }
        fn conclusion(&self) -> Option<Conclusion> {
            (self.step >= 4).then_some(Conclusion::Exhausted)
        }
    }

    /// A one-symbol answer set — content the exhaustion gate never inspects.
    fn singleton(symbol: i32) -> AnswerSet {
        std::collections::BTreeSet::from([themelios_program::Symbol::number(symbol)])
    }

    /// A live run over `run`, ranging over the empty scenario, engine reserved.
    fn live_with(run: Box<dyn Run>) -> LiveRun<'static> {
        LiveRun {
            current: Some(run),
            scenario: Scenario,
            lookahead: None,
            drain: DrainState::Fresh,
            witnessed: false,
            faulted: None,
            _engine: PhantomData,
        }
    }

    fn solved_with(run: Box<dyn Run>) -> Solved<'static> {
        Solved {
            live: live_with(run),
        }
    }

    fn solved_over(sets: Vec<AnswerSet>, terminal: Conclusion) -> Solved<'static> {
        solved_with(Box::new(StubRun::new(sets, terminal)))
    }

    /// The payloads are built here, in the defining crate, as the engine will
    /// build them: a consistent handle with a member over a closed search.
    fn some_models() -> Models<'static> {
        Models::owned(live_with(Box::new(StubRun::new(
            vec![singleton(0)],
            Conclusion::Exhausted,
        ))))
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
            let inconclusive = Determination::Inconclusive(Partial::truncated(conclusion));
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

    // ---- The exhaustion gate (§5.3) ----

    #[test]
    fn all_answer_sets_refuses_a_budget_truncated_search() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Budget);
        assert!(matches!(solved.all_answer_sets(), Err(NotExhausted { .. })));
    }

    #[test]
    fn all_answer_sets_yields_when_the_space_closed() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        assert_eq!(solved.all_answer_sets().unwrap().len(), 2);
    }

    #[test]
    fn every_terminal_state_gates_the_complete_collection() {
        // The soundness re-derivation, as a table: `Ok` in exactly the
        // exhausted-and-drainable cross-section, `Err` everywhere else.
        for (terminal, complete) in [
            (Conclusion::Exhausted, true),
            (Conclusion::Target, false),
            (Conclusion::Budget, false),
            (Conclusion::Interrupted, false),
        ] {
            let mut solved = solved_over(vec![singleton(0)], terminal);
            assert_eq!(solved.all_answer_sets().is_ok(), complete, "{terminal:?}");
        }
    }

    #[test]
    fn a_partially_consumed_stream_cannot_be_read_as_complete() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _first = solved.answer_sets().next();
        assert!(matches!(solved.all_answer_sets(), Err(NotExhausted { .. })));
    }

    #[test]
    fn a_fault_during_the_drain_becomes_the_refusals_cause() {
        let mut solved = solved_with(Box::new(MidFaultRun::new(vec![singleton(0)])));
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(refusal.cause.is_some());
    }

    #[test]
    fn a_mid_stream_fault_surfaces_at_the_stream_item() {
        // A model, then the fault at the item — not a clean end.
        let mut solved = solved_with(Box::new(MidFaultRun::new(vec![singleton(0)])));
        let ok_ness: Vec<bool> = solved.answer_sets().map(|item| item.is_ok()).collect();
        assert_eq!(ok_ness, vec![true, false]);
    }

    #[test]
    fn the_stream_pulls_once_per_member_taken() {
        // A bounded run that panics past its limit makes a collect-then-yield
        // regression go red deterministically, rather than hang.
        let mut solved = solved_with(Box::new(BoundedRun {
            pulls: 0,
            limit: STREAM_PREFIX,
        }));
        let prefix: Vec<_> = solved
            .answer_sets()
            .take(STREAM_PREFIX)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(prefix.len(), STREAM_PREFIX);
    }

    // ---- The completeness refusal renders and chains (§5.3) ----

    #[test]
    fn a_refusal_names_the_conclusion_the_search_reached() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Budget);
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(format!("{refusal}").contains("budget"), "{refusal}");
    }

    #[test]
    fn a_refusal_without_a_known_conclusion_still_explains_itself() {
        let mut solved = solved_with(Box::new(SilentRun));
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(refusal.conclusion.is_none());
        assert!(
            format!("{refusal}").contains("did not close the space"),
            "{refusal}"
        );
    }

    #[test]
    fn a_conclusion_renders_a_human_phrase() {
        for (conclusion, phrase) in [
            (Conclusion::Exhausted, "closed the space"),
            (Conclusion::Target, "met its target"),
            (Conclusion::Budget, "hit its budget"),
            (Conclusion::Interrupted, "was interrupted"),
        ] {
            assert!(format!("{conclusion}").contains(phrase), "{conclusion:?}");
        }
    }

    #[test]
    fn a_refusal_from_a_fault_carries_the_fault_as_its_source() {
        use std::error::Error;
        let mut solved = solved_with(Box::new(FaultyRun));
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(refusal.source().is_some());
    }

    // ---- Resolving the trichotomy (§5.1/§5.2) ----

    #[test]
    fn a_search_with_a_model_resolves_consistent() {
        let solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(matches!(
            solved.into_determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn an_empty_closed_search_resolves_inconsistent() {
        let solved = solved_over(vec![], Conclusion::Exhausted);
        assert!(matches!(
            solved.into_determination(),
            Determination::Inconsistent(_)
        ));
    }

    #[test]
    fn a_cut_search_with_no_model_resolves_inconclusive() {
        for conclusion in TRUNCATING {
            let solved = solved_over(vec![], conclusion);
            assert!(
                matches!(solved.into_determination(), Determination::Inconclusive(_)),
                "{conclusion:?}"
            );
        }
    }

    #[test]
    fn a_run_that_ends_without_a_conclusion_resolves_inconclusive() {
        let solved = solved_with(Box::new(SilentRun));
        assert!(matches!(
            solved.into_determination(),
            Determination::Inconclusive(_)
        ));
    }

    #[test]
    fn a_first_pull_fault_does_not_resolve_consistent() {
        // An engine fault is not output: a false "yes" on the logical question
        // (§5.1) is the defect the resolver must not have.
        let solved = solved_with(Box::new(FaultyRun));
        assert!(matches!(
            solved.into_determination(),
            Determination::Inconclusive(_)
        ));
    }

    #[test]
    fn a_first_pull_fault_retains_its_fault_on_the_partial() {
        let solved = solved_with(Box::new(FaultyRun));
        let Determination::Inconclusive(partial) = solved.into_determination() else {
            panic!("Inconclusive");
        };
        assert!(partial.cause().is_some());
    }

    #[test]
    fn the_inspecting_resolver_reads_consistent() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn a_resolved_conclusion_is_readable() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let all = solved.all_answer_sets().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(solved.conclusion(), Some(Conclusion::Exhausted));
    }

    // ---- The consistent-models live handle (§5.2) ----

    #[test]
    fn consistent_models_stream_every_member_including_the_peeked_one() {
        // The member peeked to resolve the trichotomy is not lost — it is yielded
        // first, so a two-member search streams both.
        let solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("a search with models resolves Consistent");
        };
        assert_eq!(models.members().filter_map(Result::ok).count(), 2);
    }

    #[test]
    fn drained_exhausted_models_report_exhaustion() {
        let solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        let _drain: Vec<_> = models.members().collect();
        assert!(models.is_exhausted());
    }

    #[test]
    fn drained_cut_models_are_not_exhausted() {
        let solved = solved_over(vec![singleton(0)], Conclusion::Budget);
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        let _drain: Vec<_> = models.members().collect();
        assert!(!models.is_exhausted());
    }

    #[test]
    fn models_report_their_scenario() {
        let solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let Determination::Consistent(models) = solved.into_determination() else {
            panic!("Consistent");
        };
        assert_eq!(*models.scenario(), Scenario);
    }

    #[test]
    fn a_borrowed_models_reads_the_same_members() {
        // The inspecting reborrow yields a borrowed models over the same run.
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.determination() else {
            panic!("Consistent");
        };
        assert_eq!(models.members().filter_map(Result::ok).count(), 2);
    }

    #[test]
    fn the_inspecting_resolver_reads_an_inconsistent_search() {
        let mut solved = solved_over(vec![], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Inconsistent(_)
        ));
    }

    #[test]
    fn the_inspecting_resolver_reads_an_inconclusive_search() {
        let mut solved = solved_over(vec![], Conclusion::Budget);
        assert!(matches!(
            solved.determination(),
            Determination::Inconclusive(_)
        ));
    }

    #[test]
    fn the_inspecting_resolver_reads_a_faulted_search_as_inconclusive() {
        let mut solved = solved_with(Box::new(FaultyRun));
        assert!(matches!(
            solved.determination(),
            Determination::Inconclusive(_)
        ));
    }

    #[test]
    fn a_borrowed_models_reads_its_scenario() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let Determination::Consistent(models) = solved.determination() else {
            panic!("Consistent");
        };
        assert_eq!(*models.scenario(), Scenario);
    }

    #[test]
    fn an_undrained_models_is_not_yet_exhausted() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let Determination::Consistent(models) = solved.determination() else {
            panic!("Consistent");
        };
        assert!(!models.is_exhausted());
    }

    // ---- The trichotomy is a property of the program, not of consumption ----

    #[test]
    fn a_drained_consistent_search_still_resolves_consistent() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        assert_eq!(solved.all_answer_sets().unwrap().len(), 2);
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn a_streamed_consistent_search_still_resolves_consistent() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _all: Vec<_> = solved.answer_sets().collect();
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn a_partially_consumed_consistent_search_still_resolves_consistent() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _first = solved.answer_sets().next();
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        assert_eq!(models.members().filter_map(Result::ok).count(), 1);
    }

    #[test]
    fn the_inspecting_resolver_is_idempotent_around_a_drain() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
        {
            let Determination::Consistent(mut models) = solved.determination() else {
                panic!("Consistent");
            };
            let _drain: Vec<_> = models.members().collect();
        }
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    // ---- Inspect then gate: the peeked member is not lost on the gate path ----

    #[test]
    fn an_inspected_search_still_yields_its_complete_collection() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
        assert_eq!(solved.all_answer_sets().unwrap().len(), 2);
    }

    #[test]
    fn an_empty_inspected_search_yields_an_empty_complete_collection() {
        let mut solved = solved_over(vec![], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Inconsistent(_)
        ));
        assert!(solved.all_answer_sets().unwrap().is_empty());
    }

    #[test]
    fn a_run_that_ends_without_a_conclusion_refuses_a_complete_collection() {
        let mut solved = solved_with(Box::new(SilentRun));
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(refusal.conclusion.is_none());
    }

    #[test]
    fn a_refusal_after_a_successful_drain_says_the_sets_were_taken() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let _all = solved.all_answer_sets().unwrap();
        let refusal = solved.all_answer_sets().unwrap_err();
        assert!(format!("{refusal}").contains("already taken"), "{refusal}");
    }

    // ---- The consistent-models complete collection is gated too ----

    #[test]
    fn exhausted_models_yield_a_complete_collection() {
        let solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        assert_eq!(models.all_members().unwrap().len(), 2);
    }

    #[test]
    fn a_partially_consumed_models_refuses_a_complete_collection() {
        let solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        let _first = models.members().next();
        assert!(models.all_members().is_err());
    }

    #[test]
    fn borrowed_models_yield_a_complete_collection() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let Determination::Consistent(mut models) = solved.determination() else {
            panic!("Consistent");
        };
        assert_eq!(models.all_members().unwrap().len(), 2);
    }

    // ---- What the handle remembers after an engine fault ----

    #[test]
    fn a_faulted_handle_reads_inconclusive_across_repeated_inspection() {
        // The borrowing resolver leaves the handle live; a second inspection must
        // not forget the fault and flip to a false Inconsistent.
        let mut solved = solved_with(Box::new(FaultThenEndRun { step: 0 }));
        assert!(matches!(
            solved.determination(),
            Determination::Inconclusive(_)
        ));
        assert!(matches!(
            solved.determination(),
            Determination::Inconclusive(_)
        ));
    }

    #[test]
    fn a_faulted_handle_refuses_a_complete_collection_after_inspection() {
        // A faulted search cannot yield a complete collection, even once its fault
        // has been read out by an inspecting resolve.
        let mut solved = solved_with(Box::new(FaultThenEndRun { step: 0 }));
        assert!(matches!(
            solved.determination(),
            Determination::Inconclusive(_)
        ));
        assert!(solved.all_answer_sets().is_err());
    }

    #[test]
    fn a_fault_aborted_drain_cannot_later_claim_a_complete_collection() {
        // The first drain pulls a model, hits the fault, and drops what it pulled;
        // a re-drain must refuse, never return a collection missing the dropped
        // model (even over a run that recovers).
        let mut solved = solved_with(Box::new(RecoveringRun { step: 0 }));
        assert!(solved.all_answer_sets().is_err());
        assert!(solved.all_answer_sets().is_err());
    }

    #[test]
    fn a_faulted_handle_keeps_its_fault_as_the_cause_across_refusals() {
        let mut solved = solved_with(Box::new(FaultThenEndRun { step: 0 }));
        let _ = solved.determination();
        assert!(solved.all_answer_sets().unwrap_err().cause.is_some());
        assert!(solved.all_answer_sets().unwrap_err().cause.is_some());
    }
}
