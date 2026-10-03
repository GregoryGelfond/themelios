//! The outcome vocabulary (docs/design/solve.md §5): the typed values and
//! their views — determination, conclusion, the solved and optimized outcomes,
//! models, consequences, unsatisfiability with its assumption blame, theory
//! assignments, and statistics.
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
//! vocabulary; a [`Model`] — the unit every stream yields — carries its whole
//! answer set, the display the core derives from the program's [`ShowRule`] as
//! the model streams — a type of its own, [`Shown`], which no reading consults
//! — and the theory assignment a theory-evaluating backend supplies, empty
//! otherwise.

use std::collections::{BTreeMap, BTreeSet};

use crate::agent::{Assumption, Scenario};
use crate::contract::{Fault, Mode, Presupposition};
pub use themelios_program::AnswerSet;
use themelios_program::program::Show;
use themelios_program::{Name, Sign, Symbol};

// ---- The closed distinctions (§5.1) ----

/// The logical question: is the program consistent? A closed trichotomy,
/// deliberately not `#[non_exhaustive]` — the closed set is the affordance
/// that forbids a fourth reading (docs/design/solve.md §5.1), so a reading
/// that names the three variants needs no fallback arm. The variants are
/// closed; their payloads are the surface that may grow. The `Consistent`
/// payload is a view over the live engine (§5.2), so the trichotomy carries
/// that borrow, `'a`.
pub enum Determination<'a> {
    /// The program has an answer set: read the models, or open the world view
    /// the query tier reads, through the [`Models`] (§5.2).
    Consistent(Models<'a>),
    /// The program has no answer set — under a scenario, none the scenario
    /// admits. The payload is the home of assumption blame (§5.4).
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
    /// The search was cancelled through the interrupt handle (§6.3) before it
    /// closed the space. An engine fault is no conclusion: it stops a search as
    /// [`Stopped::Faulted`].
    Interrupted,
}

impl std::fmt::Display for Conclusion {
    /// A human phrase — the reading a diagnostic shows, not the variant name
    /// (docs/design/solve.md §1.3: every value has a human `Display`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Conclusion::Exhausted => "the search closed the space",
            Conclusion::Target => "the search met its target",
            Conclusion::Budget => "the search hit its budget",
            Conclusion::Interrupted => "the search was interrupted",
        })
    }
}

/// The conclusions short of the space (docs/design/solve.md §5.1) — its
/// target, its budget, or a cancellation. Closed, and without `Exhausted`: an
/// exhausted search decided, so no inconclusive search concluded there. Each
/// is the [`Conclusion`] of its name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Truncation {
    /// The search met the target the request set, the space not closed.
    Target,
    /// The search hit the request's budget.
    Budget,
    /// The search was cancelled through the interrupt handle.
    Interrupted,
}

impl Truncation {
    /// The truncation `conclusion` names — `None` for `Exhausted`, which
    /// closed the space. O(1).
    pub(crate) fn of(conclusion: Conclusion) -> Option<Truncation> {
        match conclusion {
            Conclusion::Exhausted => None,
            Conclusion::Target => Some(Truncation::Target),
            Conclusion::Budget => Some(Truncation::Budget),
            Conclusion::Interrupted => Some(Truncation::Interrupted),
        }
    }
}

impl From<Truncation> for Conclusion {
    /// The conclusion of the same name. Total; O(1).
    fn from(truncation: Truncation) -> Conclusion {
        match truncation {
            Truncation::Target => Conclusion::Target,
            Truncation::Budget => Conclusion::Budget,
            Truncation::Interrupted => Conclusion::Interrupted,
        }
    }
}

impl std::fmt::Display for Truncation {
    /// The phrase of the conclusion of the same name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Conclusion::from(*self).fmt(f)
    }
}

/// The `Inconclusive` payload (docs/design/solve.md §5.1): how the search
/// stopped short of deciding. Non-exhaustive, so it may come to carry more of
/// what the search established; its stopping reason is one closed shape,
/// read through [`stopped`](Partial::stopped).
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Partial {
    reason: Reason,
}

/// The owned form of a [`Stopped`], held by a [`Partial`].
#[derive(Clone, PartialEq, Eq, Debug)]
enum Reason {
    Concluded(Truncation),
    Faulted(Fault),
}

/// How an inconclusive search stopped (docs/design/solve.md §5.1): at the
/// truncation it concluded at, or at an engine fault, which is no conclusion.
/// Closed: a stopping reason is exactly one of the two, and an exhausted
/// conclusion — which decides — is not among them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stopped<'a> {
    /// The search concluded short of the space, at this truncation.
    Concluded(Truncation),
    /// An engine fault stopped the search before it concluded — the fault
    /// retained, never laundered into a truncation.
    Faulted(&'a Fault),
}

impl Partial {
    /// A search that concluded short of the space, with no fault.
    pub(crate) fn truncated(truncation: Truncation) -> Partial {
        Partial {
            reason: Reason::Concluded(truncation),
        }
    }

    /// A search an engine fault stopped before it decided.
    pub(crate) fn faulted(cause: Fault) -> Partial {
        Partial {
            reason: Reason::Faulted(cause),
        }
    }

    /// How the search stopped. Total; O(1).
    pub fn stopped(&self) -> Stopped<'_> {
        match &self.reason {
            Reason::Concluded(truncation) => Stopped::Concluded(*truncation),
            Reason::Faulted(fault) => Stopped::Faulted(fault),
        }
    }
}

// ---- Models, optima, consequences (§5.2) ----

/// One model of the program (docs/design/solve.md §5.1) — the unit every
/// stream yields and every complete collection holds: its answer set, every
/// literal true in it; what the program displays of it by its `#show`
/// directives, which the core derives ([`Model::shown`]); and the theory
/// assignment a backend evaluating theory atoms supplies with it (§5.4) — empty
/// for a backend that evaluates none. The readings read the answer set and only
/// it; the display and the assignment ride beside it, never laundered into
/// atoms. Cost: the answer set by value; the display stored only where it
/// differs from the answer set; and, for a backend evaluating no theory, an
/// empty assignment — so a model of a program without directives carries
/// nothing beyond its answer set. Equality compares the display by content.
/// The assignment-bearing construction door lands with the theory assignments'
/// own constructors, when a theory-evaluating backend is built (§11.1).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Model {
    atoms: AnswerSet,
    /// The symbols the program's term directives display in this model — the
    /// half of the display only an engine evaluates.
    terms: BTreeSet<Symbol>,
    /// The display, where it differs from the answer set.
    display: Option<BTreeSet<Symbol>>,
    theory: TheoryAssignments,
}

impl Model {
    /// The model whose answer set is `atoms`, with no displayed term and no
    /// theory assignment — the backend's construction door
    /// (docs/design/solve.md §5.1). O(1).
    pub fn of(atoms: AnswerSet) -> Model {
        Model {
            atoms,
            terms: BTreeSet::new(),
            display: None,
            theory: TheoryAssignments::default(),
        }
    }

    /// This model, displaying `terms` — the symbols the program's term
    /// directives display in it, the half of the display only an engine
    /// evaluates (§5.1). A model built outside a run displays its answer set
    /// and its terms; a run's derives its display under the program's show
    /// rule as it streams. O(|terms| log |terms|), and the union with the
    /// answer set where any term is given.
    #[must_use]
    pub fn with_terms(self, terms: impl IntoIterator<Item = Symbol>) -> Model {
        Model {
            terms: terms.into_iter().collect(),
            ..self
        }
        .displayed(&ShowRule::default())
    }

    /// The model's answer set — what every reading reads. Total; O(1).
    pub fn atoms(&self) -> &AnswerSet {
        &self.atoms
    }

    /// What the model displays — the core's derivation from the answer set,
    /// the program's show rule, and the displayed terms (§5.1): a type of its
    /// own, which no reading consults. Total; O(1).
    pub fn shown(&self) -> Shown<'_> {
        Shown {
            symbols: self.display.as_ref().unwrap_or(&self.atoms),
        }
    }

    /// The theory assignment that satisfied the model's theory atoms — empty
    /// unless the backend evaluates a theory (docs/design/solve.md §5.4).
    /// Total; O(1).
    pub fn assignment(&self) -> &TheoryAssignments {
        &self.theory
    }

    /// Whether the model is consistent: it holds no atom beside its strong
    /// negation — the literature's consistent set of literals (docs/design/
    /// query.md §2.3). No answer set is otherwise, so a model that is not is
    /// a backend's contract violation: the conformance suite fails it, and a
    /// world view refuses to materialise it. Total; one lookup per strongly
    /// negated atom, each copying its arguments — `O(|M| log |M|)`.
    pub fn is_consistent(&self) -> bool {
        !self.atoms.iter().any(|symbol| match symbol {
            Symbol::Function {
                name,
                arguments,
                sign: Sign::Negative,
            } => self.atoms.contains(&Symbol::function(
                name.clone(),
                arguments.iter().cloned(),
                Sign::Positive,
            )),
            _ => false,
        })
    }

    /// Whether every member of the answer set is a literal — a function
    /// symbol, as every member of an answer set is (§5.1); a number, a string,
    /// a tuple, `#inf`, or `#sup` is not one. Necessary, not sufficient: no
    /// check of a set's content establishes that it is an answer set of the
    /// program. Total; O(|M|).
    pub fn is_set_of_literals(&self) -> bool {
        self.atoms
            .iter()
            .all(|symbol| matches!(symbol, Symbol::Function { .. }))
    }

    /// This model, its display derived under `rule` (§5.1): the atoms the rule
    /// shows together with the terms, stored only where that differs from the
    /// answer set — so where the rule shows every atom and no term is
    /// displayed, nothing is stored. One allocation-free lookup per atom, then
    /// the terms' union: `O(|M| log |M|)`.
    pub(crate) fn displayed(mut self, rule: &ShowRule) -> Model {
        if rule.shows_every_atom() && self.terms.is_empty() {
            self.display = None;
            return self;
        }
        let mut display: BTreeSet<Symbol> = self
            .atoms
            .iter()
            .filter(|atom| rule.shows(atom))
            .cloned()
            .collect();
        display.extend(self.terms.iter().cloned());
        self.display = (display != self.atoms).then_some(display);
        self
    }
}

impl PartialEq for Model {
    /// The answer sets, the displays by content, and the assignments — a
    /// stored display equal to the answer set is no different from none.
    fn eq(&self, other: &Model) -> bool {
        self.atoms == other.atoms
            && self.shown().symbols() == other.shown().symbols()
            && self.theory == other.theory
    }
}

impl Eq for Model {}

/// A program's show rule (docs/design/solve.md §5.1): which of a model's atoms
/// its `#show` directives display — every atom where the directives in force
/// include no restricting directive (a signature form, or `#show.`), else the
/// atoms of the signatures those list. A backend builds it from the directives
/// it holds and hands it to the core with its run; the core applies it, so the
/// rule has one implementation. `Default` is the rule of no directive: every
/// atom. O(directives · log directives) to build.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ShowRule {
    /// `None` where no restricting directive is in force; else each listed
    /// name's signs and arities, keyed by name so a lookup borrows the atom's
    /// name and allocates nothing — the rule runs over every atom of every
    /// model while a restricting directive is in force.
    restricted: Option<BTreeMap<Name, BTreeSet<(Sign, u32)>>>,
}

impl ShowRule {
    /// The rule of `directives`: a term directive restricts nothing; `#show.`
    /// and a signature directive restrict the atoms to the signatures listed,
    /// sign-sensitively, a strongly negated signature listed in its own right.
    pub fn of<'p>(directives: impl IntoIterator<Item = &'p Show>) -> ShowRule {
        let mut restricted: Option<BTreeMap<Name, BTreeSet<(Sign, u32)>>> = None;
        for directive in directives {
            match directive {
                Show::All => {
                    restricted.get_or_insert_with(BTreeMap::new);
                }
                Show::Signature(signature) => {
                    restricted
                        .get_or_insert_with(BTreeMap::new)
                        .entry(signature.name.clone())
                        .or_default()
                        .insert((signature.sign, signature.arity));
                }
                Show::Term(_) | Show::TermBody { .. } => {}
            }
        }
        ShowRule { restricted }
    }

    /// Whether the rule displays `atom`: one lookup by the atom's borrowed
    /// name, then one of its sign and arity — O(log names · name + log forms),
    /// with no allocation.
    pub(crate) fn shows(&self, atom: &Symbol) -> bool {
        let Some(names) = &self.restricted else {
            return true;
        };
        let Symbol::Function {
            name,
            arguments,
            sign,
        } = atom
        else {
            return false;
        };
        // An atom of more arguments than a signature's arity counts is shown by
        // no signature.
        u32::try_from(arguments.len()).is_ok_and(|arity| {
            names
                .get(name)
                .is_some_and(|forms| forms.contains(&(*sign, arity)))
        })
    }

    /// Whether the rule displays every atom — no restricting directive in
    /// force. O(1).
    pub(crate) fn shows_every_atom(&self) -> bool {
        self.restricted.is_none()
    }
}

/// What a model displays (§5.1): a view of its own type over the displayed
/// symbols — not an `AnswerSet`, so a position that takes one does not take it;
/// its symbols reach such a position only through [`Shown::symbols`], written
/// at the call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shown<'m> {
    symbols: &'m BTreeSet<Symbol>,
}

impl<'m> Shown<'m> {
    /// The displayed atoms and terms. O(1).
    pub fn symbols(self) -> &'m BTreeSet<Symbol> {
        self.symbols
    }

    /// Whether `symbol` is displayed. O(log n).
    pub fn contains(self, symbol: &Symbol) -> bool {
        self.symbols.contains(symbol)
    }
}

/// The streaming/terminal-state protocol a backend's `solve` drives (docs/design/
/// solve.md §5.2): the enumeration a backend wraps into the [`Solved`] it returns,
/// built through [`Solved::running`]. A run holds its own enumeration state and
/// whatever it reads of its engine — a raw handle, or a borrow for `'a`. The core
/// reaches the engine only through this protocol, and `Backend::solve` returns the
/// handle as `Solved<'_>`, borrowing the backend, so the borrow checker, not the
/// live handle, serialises engine access (§6.1).
/// A `Solved`/`Models` built over a run is `!Send`: a run may hold a raw engine
/// handle whose control is single-threaded, and the protocol does not require a run
/// to be `Send`, so the live handle stays on one thread by construction — a
/// deliberate consequence, not an accident (a pure-Rust run carries no such handle,
/// yet the handle is `!Send` all the same, so no consumer may rely on it being
/// otherwise).
///
/// The obligations every implementor owes, which the live handle relies on when it
/// re-polls a spent run (after an inspecting resolve, a second stream, a
/// completeness drain):
/// - **Fused**: once `next_model` has returned `None`, it returns `None`
///   forever.
/// - **Terminal conclusion**: once `next_model` has returned `None` with no
///   fault before it, `conclusion` returns `Some`; it is `None` while the
///   search is open.
/// - **After a fault**: a completeness drain ([`Solved::all_models`],
///   [`Models::all_members`]) stops at the first `Some(Err(_))`, keeping it as the
///   cause, while a lazy stream ([`Solved::models`], [`Models::members`]) relays
///   exactly what the run yields — so a conforming run yields `None` after a
///   fault rather than enumerate past it, since a run that never ends is a stream
///   that never ends; and its `conclusion` stays `None`, since a faulted search
///   reached no conclusion — the core records the fault as the cause.
pub trait Run {
    /// The next model, or `None` at the end of the search (fused — see the
    /// trait obligations). Each item a `Result`, so a mid-stream engine fault
    /// surfaces at the item, not as a clean end.
    fn next_model(&mut self) -> Option<Result<Model, Fault>>;
    /// How the search concluded — `None` while it is still open, `Some` once
    /// `next_model` has returned `None` with no fault before it, and `None`
    /// after a fault, which reached no conclusion.
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
    /// The stream has been read — partly, or to its end; completeness is forfeit.
    Touched,
}

/// The live run behind a [`Solved`] or [`Models`] (docs/design/solve.md §5.2):
/// the backend's in-flight enumeration with a one-model `lookahead`, the
/// scenario it ranges over, and the drain state. It holds the run alone — no
/// engine and no backend access — so a reading that needs a fresh solve — a
/// consequence door, an epistemic reading — is the agent's (§6.2), never a
/// drain of this stream. Its lifetime is the run's: the borrow `Backend::solve`
/// takes of the backend — through an agent, the agent's, for the question that
/// opened it, so the borrow checker is the lock against a second question while
/// it lives (§6.1) — or `'static` when the run owns an ephemeral engine (§6.4).
pub(crate) struct LiveRun<'a> {
    current: Box<dyn Run + 'a>,
    scenario: Scenario,
    lookahead: Option<Result<Model, Fault>>,
    drain: DrainState,
    // Whether any model has ever been pulled from the run. The trichotomy is a
    // property of the program, not of how much has been consumed, so a search
    // that has yielded a model stays consistent after its stream is drained.
    witnessed: bool,
    // The engine fault that ended the search, remembered so a handle that
    // survives an inspecting read (the borrowing resolver) keeps reporting the
    // fault rather than forgetting it once its pending item is taken.
    faulted: Option<Fault>,
    // The show rule of the directives the backend holds, under which the core
    // derives each streamed model's display (§5.1).
    show: ShowRule,
}

/// The trichotomy a live run reads off, before each arm is wrapped with its
/// evidence (docs/design/solve.md §5.1).
enum Class {
    Consistent,
    Inconsistent,
    Inconclusive(Truncation),
    /// A resolve-time engine fault before any model — the fault retained.
    Faulted(Fault),
}

impl LiveRun<'_> {
    /// Pull the next item from the run, remembering a witnessed model and a
    /// fault — one the run yields, or its own breach of the terminal obligation
    /// when it ends without concluding. The single point every member flows
    /// through, so `witnessed` and `faulted` are always current and every reading
    /// attributes the breach the one way.
    fn pull(&mut self) -> Option<Result<Model, Fault>> {
        let item = self.current.next_model();
        match &item {
            Some(Ok(_)) => self.witnessed = true,
            Some(Err(fault)) => {
                self.faulted.get_or_insert_with(|| fault.clone());
            }
            None => {
                if self.faulted.is_none() && self.current.conclusion().is_none() {
                    self.faulted = Some(unconcluded());
                }
            }
        }
        item.map(|result| result.map(|model| model.displayed(&self.show)))
    }

    /// The next model, yielding the one-model `lookahead` first so no member
    /// peeked to resolve the trichotomy is lost.
    fn next(&mut self) -> Option<Result<Model, Fault>> {
        if let Some(peeked) = self.lookahead.take() {
            return Some(peeked);
        }
        self.pull()
    }

    /// The streaming pull `models`/`members` yield through: completeness is
    /// forfeit on the FIRST pull (not at iterator creation), then the member is
    /// yielded.
    fn stream_next(&mut self) -> Option<Result<Model, Fault>> {
        if self.drain == DrainState::Fresh {
            self.drain = DrainState::Touched;
        }
        self.next()
    }

    /// How the search concluded — `None` until it ends, and after a fault,
    /// which reached no conclusion whatever the run reports.
    fn conclusion(&self) -> Option<Conclusion> {
        if self.faulted.is_some() {
            return None;
        }
        self.current.conclusion()
    }

    /// Read off the trichotomy (docs/design/solve.md §5.1). Consistent iff a model
    /// has ever been witnessed — the reading is a property of the program, not of
    /// how much has been consumed, so a drained consistent search stays
    /// Consistent. With no witness, peek one member: a model makes it Consistent,
    /// a fault makes it Faulted (retained, never a false "yes"), a clean end reads
    /// the conclusion — Exhausted with no model is Inconsistent, a truncation is
    /// Inconclusive, and an end with no conclusion is the run's own fault.
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
            Some(conclusion) => {
                Truncation::of(conclusion).map_or(Class::Inconsistent, Class::Inconclusive)
            }
            // A run that ends without concluding broke the terminal obligation,
            // and `pull` has recorded that as its fault, read above; the arm
            // states the invariant, so the reading stays total — never "yes",
            // never "no".
            None => Class::Faulted(unconcluded()),
        }
    }

    /// The exhaustion gate, shared by `Solved::all_models` and
    /// `Models::all_members` (docs/design/solve.md §5.3): a complete collection
    /// ONLY from an untouched handle whose search closed the space; refuses
    /// otherwise, keeping a mid-stream fault as the cause.
    fn drain_complete(&mut self) -> Result<Vec<Model>, NotExhausted> {
        // A search that has faulted cannot yield a complete collection — refuse
        // with the remembered fault, the cause preserved on every attempt and named
        // before touched-ness, since the fault is why the collection is gone; and
        // without draining, so a handle that pulled nothing is not marked touched.
        if let Some(fault) = self.faulted.clone() {
            return Err(NotExhausted::faulted(fault));
        }
        if self.drain != DrainState::Fresh {
            return Err(NotExhausted::already_taken());
        }
        let mut all = Vec::new();
        loop {
            match self.next() {
                Some(Ok(model)) => all.push(model),
                Some(Err(fault)) => {
                    // Models were pulled and dropped: the handle is no longer
                    // untouched, so a re-drain refuses rather than returning a
                    // collection missing them.
                    self.drain = DrainState::Touched;
                    return Err(NotExhausted::faulted(fault));
                }
                None => break,
            }
        }
        self.drain = DrainState::Touched;
        // A run that ended without concluding: `pull` recorded the breach, and it
        // is the refusal's cause, as a fault the run yielded would be.
        if let Some(fault) = self.faulted.clone() {
            return Err(NotExhausted::faulted(fault));
        }
        if self.conclusion() == Some(Conclusion::Exhausted) {
            return Ok(all);
        }
        match self.conclusion().and_then(Truncation::of) {
            Some(truncation) => Err(NotExhausted::not_closed(truncation)),
            // A run that ended without concluding broke the run protocol (§5.2):
            // `pull` recorded that breach as its fault, read above, so the arm
            // states the invariant — the breach's adapter fault either way.
            None => Err(NotExhausted::faulted(unconcluded())),
        }
    }
}

/// The fault of a run that ended its stream without concluding: a breach of the
/// run protocol's terminal obligation, so the adapter's (docs/design/solve.md
/// §5.2, §5.4).
fn unconcluded() -> Fault {
    Fault::adapter_bug("the search ended without concluding, against the run protocol")
}

impl<'a> Determination<'a> {
    /// Resolve an owned live run into the trichotomy (consuming), threading the
    /// engine borrow `'a` — the resolver the agent and bare conveniences build
    /// over (docs/design/solve.md §5.2/§6.4).
    pub(crate) fn of_live(mut live: LiveRun<'a>) -> Determination<'a> {
        match live.classify() {
            Class::Consistent => Determination::Consistent(Models::owned(live)),
            // Blame's derivation over `solve_assuming` (§5.4) is not yet
            // realised, so every resolution, scoped or not, reports none.
            Class::Inconsistent => Determination::Inconsistent(Unsat { blame: None }),
            Class::Inconclusive(truncation) => {
                Determination::Inconclusive(Partial::truncated(truncation))
            }
            Class::Faulted(fault) => Determination::Inconclusive(Partial::faulted(fault)),
        }
    }

    /// Resolve a borrowed live run into the trichotomy (reborrow) — inspect in
    /// place, bounded by the borrow (docs/design/solve.md §5.2).
    pub(crate) fn of_live_ref<'b>(live: &'b mut LiveRun<'a>) -> Determination<'b> {
        match live.classify() {
            Class::Consistent => Determination::Consistent(Models::borrowed(live)),
            // No blame yet, as in the consuming resolver.
            Class::Inconsistent => Determination::Inconsistent(Unsat { blame: None }),
            Class::Inconclusive(truncation) => {
                Determination::Inconclusive(Partial::truncated(truncation))
            }
            Class::Faulted(fault) => Determination::Inconclusive(Partial::faulted(fault)),
        }
    }
}

/// The completeness refusal: a complete collection was asked of a handle that
/// cannot yield one (docs/design/solve.md §5.2, §5.3) — its models already
/// streamed, its search stopped short of the space at a named truncation, or
/// its search faulted, the fault kept as the cause — so a truncation and a
/// fault are never laundered into one anonymous refusal.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotExhausted {
    reason: Incompleteness,
}

/// Why a complete collection was refused (docs/design/solve.md §5.2) — one of
/// three, so an unclosed search always carries its truncation.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Incompleteness {
    /// The handle's models were already streamed.
    Taken,
    /// The search ended short of the space, at this truncation.
    Unclosed(Truncation),
    /// The search faulted, reaching no conclusion; the fault is the cause.
    Faulted(Fault),
}

impl NotExhausted {
    /// The handle's stream was already touched, so a complete collection is no
    /// longer available from it.
    pub(crate) fn already_taken() -> NotExhausted {
        NotExhausted {
            reason: Incompleteness::Taken,
        }
    }

    /// The search ran to its end short of the space, at `truncation`.
    pub(crate) fn not_closed(truncation: Truncation) -> NotExhausted {
        NotExhausted {
            reason: Incompleteness::Unclosed(truncation),
        }
    }

    /// The search faulted — mid-stream, or by ending against the run protocol
    /// — and the fault is the cause.
    pub(crate) fn faulted(cause: Fault) -> NotExhausted {
        NotExhausted {
            reason: Incompleteness::Faulted(cause),
        }
    }
}

impl From<NotExhausted> for Fault {
    /// This refusal as a [`Fault`], for a reading that needs a complete world view
    /// and cannot proceed without one — the query tier's `materialize` and cautious
    /// reading among them. A fault the search raised surfaces as its own cause; a
    /// truncation or an already-taken handle surfaces as a request fault naming its
    /// presupposition — the truncation the search stopped at, or the streamed
    /// handle — so the reason stays visible and matchable, never laundered into an
    /// anonymous error (docs/design/solve.md §5.1, §5.3, §5.4).
    fn from(refusal: NotExhausted) -> Fault {
        match refusal.reason {
            Incompleteness::Taken => Fault::request(
                "the models were already taken from this handle, so a complete world view is unavailable",
                Presupposition::Taken,
            ),
            Incompleteness::Unclosed(truncation) => Fault::request(
                format!("the search did not close the space: {truncation}"),
                Presupposition::Unclosed(truncation),
            ),
            Incompleteness::Faulted(cause) => cause,
        }
    }
}

impl From<Partial> for Fault {
    /// This inconclusive reading as a [`Fault`], for a reading that needs a decided
    /// program and cannot proceed without one. The engine fault that stopped the
    /// search surfaces as its own cause; a plain truncation surfaces as a request
    /// fault naming the truncation it stopped at — the reason the search stopped
    /// stays visible, symmetric with a witnessed truncation (docs/design/solve.md
    /// §5.1, §5.4).
    fn from(partial: Partial) -> Fault {
        match partial.reason {
            Reason::Faulted(fault) => fault,
            Reason::Concluded(truncation) => Fault::request(
                format!("the search did not decide the program: {truncation}"),
                Presupposition::Unclosed(truncation),
            ),
        }
    }
}

impl std::fmt::Display for NotExhausted {
    /// Why the collection is unavailable, as the reason says.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.reason {
            Incompleteness::Taken => f.write_str(
                "the models were already taken from this handle; a complete collection is unavailable",
            ),
            Incompleteness::Unclosed(truncation) => {
                write!(f, "{truncation}; a complete collection is unavailable")
            }
            Incompleteness::Faulted(_) => {
                f.write_str("the search faulted; a complete collection is unavailable")
            }
        }
    }
}

impl std::error::Error for NotExhausted {
    /// The fault the search raised, where it faulted.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.reason {
            Incompleteness::Faulted(cause) => Some(cause),
            Incompleteness::Taken | Incompleteness::Unclosed(_) => None,
        }
    }
}

/// The borrowed run handle `solve` returns: stream the models, inspect or
/// resolve the trichotomy, read the conclusion. Read by `&mut` because draining
/// the stream is stateful — the terminal `conclusion` is readable only after the
/// drain reaches the end (docs/design/solve.md §5.2).
pub struct Solved<'a> {
    live: LiveRun<'a>,
}

impl<'a> Solved<'a> {
    /// The lazy model stream; each item a `Result`, so a mid-stream engine
    /// fault surfaces at the item. Touching it forfeits completeness. Cost: O(1)
    /// resident.
    pub fn models(&mut self) -> impl Iterator<Item = Result<Model, Fault>> + '_ {
        std::iter::from_fn(|| self.live.stream_next())
    }

    /// A COMPLETE collection — available ONLY from an untouched handle whose search
    /// closed the space; refuses otherwise (the exhaustion gate, §5.3), a faulted
    /// search's refusal carrying the fault as its cause. This is what makes a
    /// truncated search structurally unable to pass as complete.
    pub fn all_models(&mut self) -> Result<Vec<Model>, NotExhausted> {
        self.live.drain_complete()
    }

    /// How the search concluded — `Some` once it ended without a fault; `None`
    /// while it is open, and after a fault, which reached no conclusion (the
    /// determination's [`Stopped::Faulted`] carries it).
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

impl<'a> Solved<'a> {
    /// The backend-facing construction door (docs/design/solve.md §5.2): wrap an
    /// enumeration `run`, ranging over `scenario`, into the solved handle a
    /// backend's [`solve`](crate::contract::Backend::solve) — or
    /// [`solve_assuming`](crate::contract::Backend::solve_assuming) — returns, with
    /// nothing yet pulled, under `show`, the show rule of the directives the
    /// backend holds: the core derives each streamed model's display from it
    /// (§5.1). The core drives the run and classifies the trichotomy over it
    /// (§5.1), so a backend supplies only its enumeration and the terminal
    /// [`Conclusion`]; it never constructs the [`Determination`], so it cannot pose
    /// an empty or truncated search as consistent. O(1).
    pub fn running(run: Box<dyn Run + 'a>, scenario: Scenario, show: ShowRule) -> Solved<'a> {
        Solved {
            live: LiveRun {
                current: run,
                scenario,
                lookahead: None,
                drain: DrainState::Fresh,
                witnessed: false,
                faulted: None,
                show,
            },
        }
    }
}

/// A PROVEN optimum (docs/design/solve.md §5.2): its levels, in the terms the
/// objectives were written in — a maximized level shows what was maximized,
/// not the negation an engine optimizes internally. It has NO public
/// constructor: a value of this type exists only because a solver proved it,
/// so a best-found cannot pose as proven — the last of the named pathologies,
/// closed structurally rather than by test (§5.3). The levels are reported
/// once optimization is realised; until then the type is declared, with the
/// crate-private field that keeps it unconstructible elsewhere, so the
/// backend contract's `optimize` and the [`Optimized`] register are shaped by
/// it already.
pub struct Optimum {
    pub(crate) _levels: (),
}

/// A step of the improving trajectory (docs/design/solve.md §5.2): one model
/// the search found and retained, with its levels in the objectives' own terms
/// — one completed result, published only once the backend has retained it.
/// Not proven optimal, and never convertible into an [`Optimum`], so a
/// best-found cannot pose as proven (§5.3). No public constructor: its door
/// lands with `optimize`'s, beside `Optimum`'s, where the levels' type is fixed
/// for both. Cost: one model conversion at the step it is published.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Incumbent {
    model: Model,
}

impl Incumbent {
    /// The retained model, its whole answer set (§5.1). Total; O(1).
    pub fn model(&self) -> &Model {
        &self.model
    }
}

/// The run handle `optimize` returns (docs/design/solve.md §5.2): the same
/// resolution register as [`Solved`] — `determination`/`into_determination`/
/// `conclusion` — specialised with `optimum`/`trajectory` in place of the
/// plain-enumeration accessors, so `optimize` is a sibling of `solve`, not a
/// second vocabulary (§6.4). Its optimal answer sets are read through the
/// resolved [`Determination`]'s `Consistent` models, ranging over the OPTIMAL
/// set under the optimum-proven/exhausted gate. It holds the same live run a
/// `Solved` does, borrowing its engine for `'a`.
pub struct Optimized<'a> {
    pub(crate) live: LiveRun<'a>,
}

impl<'a> Optimized<'a> {
    /// RESOLVE the run into the trichotomy (consuming), threading the engine
    /// borrow `'a`; `Consistent` ranges over the optimal set
    /// (docs/design/solve.md §5.2).
    pub fn into_determination(self) -> Determination<'a> {
        Determination::of_live(self.live)
    }

    /// INSPECT the run in place (reborrow): the trichotomy, bounded by the
    /// borrow (docs/design/solve.md §5.2).
    pub fn determination(&mut self) -> Determination<'_> {
        Determination::of_live_ref(&mut self.live)
    }

    /// The proven optimum, once proved — `None` while none is. Reserved until
    /// optimization is realised; answers `None` meanwhile. Total; O(1).
    pub fn optimum(&self) -> Option<Optimum> {
        None
    }

    /// The improving sequence of [`Incumbent`]s — best-found models, typed
    /// apart from the proven optimum — each step a `Result` so a mid-search
    /// engine fault surfaces at the step; `Some` iff the request asked for it
    /// (docs/design/solve.md §5.3). Reserved until optimization is realised;
    /// answers `None` meanwhile.
    pub fn trajectory(&mut self) -> Option<impl Iterator<Item = Result<Incumbent, Fault>> + '_> {
        None::<std::iter::Empty<_>>
    }

    /// How the search concluded — `Some` once it ended without a fault; `None`
    /// while it is open, and after a fault, which reached no conclusion.
    pub fn conclusion(&self) -> Option<Conclusion> {
        self.live.conclusion()
    }
}

/// Cautious (⋂) or brave (⋃) consequences (docs/design/solve.md §5.2): a set
/// of ground symbols carrying the [`Mode`] that produced it, so a value that
/// has travelled still says which question it answers. Not an answer set —
/// its own type, for that reason. Built by the [`fold`](Consequences::fold) or
/// by the core's gate over a [`NativeAnswer`]: a backend reports, and never
/// builds one. The set ranges over the model set of the
/// question that produced it — all stable models for a solve's, whose question
/// ignores any objective; the optimal set for an optimization's — and the
/// optimal-vs-all marker joins these fields with optimization; non-exhaustive
/// leaves it the room.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Consequences {
    pub(crate) symbols: BTreeSet<Symbol>,
    pub(crate) mode: Mode,
}

impl Consequences {
    /// Which question this set answers: the mode that produced it. Total;
    /// O(1).
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The symbols, each once, in the term order. Borrowed: reading does not
    /// spend the set. Cost: O(n) over the whole stream.
    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.symbols.iter()
    }

    /// Whether `symbol` is among the consequences. Total; O(log n).
    pub fn contains(&self, symbol: &Symbol) -> bool {
        self.symbols.contains(symbol)
    }

    /// The consequences as the set they are — borrowed, so a reading that needs
    /// the set itself, a range scan say, reads it without a copy. Total; O(1).
    pub fn as_set(&self) -> &BTreeSet<Symbol> {
        &self.symbols
    }

    /// The cautious (⋂) or brave (⋃) consequences of the given answer sets —
    /// the fold, exposed as a primitive (docs/design/solve.md §5.2). `None` over
    /// none, since no world view is empty (query.md §2.3) and a consequence set
    /// over no models certifies nothing. Nor does it certify completeness: the
    /// gated readings are the agent's `cautious`/`brave` and a `Snapshot`'s, each
    /// folding a world view whose search closed the space. Cautious is the
    /// intersection (a symbol in EVERY answer set), brave the union (a symbol in
    /// SOME). Cost: O(members × set size).
    pub fn fold<'m, I>(mode: Mode, members: I) -> Option<Consequences>
    where
        I: IntoIterator<Item = &'m AnswerSet>,
    {
        let mut members = members.into_iter();
        let mut symbols = members.next()?.clone();
        for member in members {
            match mode {
                Mode::Cautious => symbols.retain(|symbol| member.contains(symbol)),
                Mode::Brave => symbols.extend(member.iter().cloned()),
            }
        }
        Some(Consequences { symbols, mode })
    }
}

/// The lifetime-erased view of a live run that a [`Models`] holds, so `Models<'a>`
/// carries a single lifetime — the access — while the engine's own lifetime lives
/// behind the trait object. [`LiveRun`] is the only implementor; a reborrowed
/// `&mut LiveRun` unsize-coerces to `&mut dyn RunAccess`, which is what lets the
/// inspecting resolver hand back a borrowed `Models` without a second lifetime on
/// the public type.
pub(crate) trait RunAccess {
    fn stream_next(&mut self) -> Option<Result<Model, Fault>>;
    fn is_exhausted(&self) -> bool;
    fn scenario(&self) -> &Scenario;
    fn drain_complete(&mut self) -> Result<Vec<Model>, NotExhausted>;
}

impl RunAccess for LiveRun<'_> {
    fn stream_next(&mut self) -> Option<Result<Model, Fault>> {
        LiveRun::stream_next(self)
    }
    fn is_exhausted(&self) -> bool {
        // After a fault the conclusion reads `None` whatever the run reports, so
        // the report agrees with the gate on the same handle.
        LiveRun::conclusion(self) == Some(Conclusion::Exhausted)
    }
    fn scenario(&self) -> &Scenario {
        &self.scenario
    }
    fn drain_complete(&mut self) -> Result<Vec<Model>, NotExhausted> {
        LiveRun::drain_complete(self)
    }
}

/// The `Consistent` payload (docs/design/solve.md §5.2): the live-run-access
/// handle the query tier reads — no engine and no backend access. It OWNS the live
/// run, from the consuming resolver `into_determination` (`'static` when the run
/// owns a bare program's ephemeral engine, §6.4), or BORROWS it, from the
/// inspecting `determination(&mut self)`. It exposes the raw
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
    pub fn members(&mut self) -> impl Iterator<Item = Result<Model, Fault>> + '_ {
        std::iter::from_fn(|| self.access().stream_next())
    }

    /// A COMPLETE collection of the members — available ONLY from an untouched
    /// handle whose search closed the space; refuses otherwise (the exhaustion
    /// gate, §5.3), so the query tier's `materialize` cannot launder a partial
    /// set as complete (query.md §3.2).
    pub fn all_members(&mut self) -> Result<Vec<Model>, NotExhausted> {
        self.access().drain_complete()
    }

    /// Whether the search closed the space — the `WorldView::is_exhausted` analog
    /// (docs/design/solve.md §5.3). A search that faulted did not, whatever its run
    /// reports, so the report agrees with the exhaustion gate on the same handle.
    pub fn is_exhausted(&self) -> bool {
        self.access_ref().is_exhausted()
    }

    /// The scenario the models range over (query.md §2.3).
    pub fn scenario(&self) -> &Scenario {
        self.access_ref().scenario()
    }
}

/// What the engine's own consequence search established (docs/design/solve.md
/// §5.2) — the raw material the core builds [`Consequences`] from: the backend
/// reports, and the core decides and words the refusal. A `Closed` set becomes
/// the consequences in the mode asked; `NoModel` refuses, `⋂`/`⋃` over the
/// empty world view being undefined, not `∅`; `Stopped` refuses, a native
/// cautious search stopped early having converged on a *super*set of `⋂`, not
/// the consequences. Unlike a [`Run`]'s models, which the core pulls, the
/// report is the backend's word: its honesty is the backend's obligation, which
/// the conformance suite and the native-versus-derived differential hold.
/// Closed: a native search closed the space having seen a model, closed it
/// having seen none, or stopped short of it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NativeAnswer {
    /// The engine's `⋂` or `⋃` over the space it closed, a model seen.
    Closed(BTreeSet<Symbol>),
    /// The space closed with no model: the scenario admits none.
    NoModel,
    /// The search stopped short of the space, at this truncation; the engine's
    /// partial set, not the consequences, is not carried.
    Stopped(Truncation),
}

// ---- Assumption blame (§5.4) ----

/// The `Inconsistent` payload (docs/design/solve.md §5.1): the program has no
/// answer set — under a scenario, none the scenario admits. It is the home of
/// assumption blame (§5.4): which of a scenario's assumptions are responsible;
/// for a plain solve the question is out of scope, which is not the same as
/// answering it "none". Non-exhaustive: a payload that may grow.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unsat {
    pub(crate) blame: Option<Refutation>,
}

impl Unsat {
    /// Which assumptions are responsible (docs/design/solve.md §5.4) — the
    /// reading an assumption-scoped solve owes. Its derivation over
    /// `solve_assuming` is not yet realised, so every resolution, scoped or
    /// not, answers `None` meanwhile. `None` never says that no assumption is
    /// to blame: that reading is [`Refutation::NotThese`]. Total; O(k) in the
    /// assumptions named, which are cloned out.
    pub fn blame(&self) -> Option<Refutation> {
        self.blame.clone()
    }
}

/// Assumption blame (docs/design/solve.md §5.4): which assumptions are
/// responsible for a scenario's inconsistency. The culprit is a raw set of
/// assumptions — the literature's notion — not a named scenario (§6.3).
/// Closed: the three readings are the readings there are, so a consumer
/// matches them with no fallback arm.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Refutation {
    /// This minimal subset of the scenario's assumptions is responsible.
    These(Box<[Assumption]>),
    /// The inconsistency is independent of the assumptions: the program is
    /// inconsistent without them.
    NotThese,
    /// The program is inconsistent with none assumed.
    NoAssumptions,
}

// ---- Theory assignments and statistics (§5.4) ----

/// Theory (constraint) assignments — a DISTINCT typed component of the
/// outcome, beside the answer set, never laundered into Herbrand-looking atoms
/// (docs/design/solve.md §5.4): program literals stay `Symbol` (`i32`), while
/// a constraint assignment is a wider solve-tier typed value, rich enough for a
/// full CP theory (§8.2) — a global constraint's domain values, a sum's result,
/// an `alldifferent`'s witness assignment, each read back as typed data.
/// Non-exhaustive: the per-variable typed constraint values join when the
/// theory door is realised; until then the component is declared, and empty,
/// so the outcome is shaped by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TheoryAssignments {}

/// Per-solve statistics (docs/design/solve.md §5.4): engine-scoped,
/// provenance-marked, typed data, read through this trait — the minimal shape
/// a consumer reads (v1: the clingo adapter provides clingo's own). The
/// reserved normalised cross-backend schema (§14) is a distinct view that
/// CONSUMES this trait, so it lands additively, touching neither the trait nor
/// [`Measurement`]. Not a trait object: the measurement stream is a
/// return-position `impl Iterator`, read where the source's type is known.
pub trait Statistics {
    /// The measurements this source holds, each named, typed, and
    /// provenance-marked. Borrowed: reading does not spend the source.
    fn measurements(&self) -> impl Iterator<Item = Measurement> + '_;
}

/// One statistic (docs/design/solve.md §5.4): an engine-scoped name, a typed
/// value, and the engine that produced it. Non-exhaustive: those fields join
/// when an engine adapter exposes its statistics; until then the measurement
/// is declared, and empty, so what a consumer reads is shaped by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Measurement {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Refused;
    use themelios_program::program::Show;
    use themelios_program::{Name, Signature, Term};

    /// The closed set of conclusions, each beside its rendering.
    const CONCLUSIONS: [(Conclusion, &str); 4] = [
        (Conclusion::Exhausted, "Exhausted"),
        (Conclusion::Target, "Target"),
        (Conclusion::Budget, "Budget"),
        (Conclusion::Interrupted, "Interrupted"),
    ];

    /// The truncations, each beside the conclusion of its name.
    const TRUNCATIONS: [(Truncation, Conclusion); 3] = [
        (Truncation::Target, Conclusion::Target),
        (Truncation::Budget, Conclusion::Budget),
        (Truncation::Interrupted, Conclusion::Interrupted),
    ];

    #[test]
    fn a_truncation_is_the_conclusion_of_its_name() {
        for (truncation, conclusion) in TRUNCATIONS {
            assert_eq!(Conclusion::from(truncation), conclusion);
            assert_eq!(Truncation::of(conclusion), Some(truncation));
        }
    }

    #[test]
    fn an_exhausted_conclusion_is_no_truncation() {
        assert_eq!(Truncation::of(Conclusion::Exhausted), None);
    }

    #[test]
    fn a_truncation_renders_as_the_conclusion_of_its_name() {
        for (truncation, conclusion) in TRUNCATIONS {
            assert_eq!(truncation.to_string(), conclusion.to_string());
        }
    }

    // A completeness refusal becomes an honest fault: its cause verbatim where a
    // fault stopped the drain, otherwise a request fault naming why, so the
    // inconclusive reason stays visible (docs/design/solve.md §5.1).

    #[test]
    fn a_faulted_refusal_converts_to_its_cause() {
        // Locus, message, and bug bit — not laundered into a request error.
        let cause = Fault::engine("a distinctive underlying failure");
        assert_eq!(Fault::from(NotExhausted::faulted(cause.clone())), cause);
    }

    #[test]
    fn an_unclosed_refusal_converts_to_a_request_fault_naming_its_truncation() {
        let not_closed = Fault::from(NotExhausted::not_closed(Truncation::Budget));
        assert!(matches!(
            not_closed.refused(),
            Refused::Request(Presupposition::Unclosed(Truncation::Budget))
        ));
    }

    #[test]
    fn an_unclosed_refusal_s_message_names_its_truncation() {
        let budgeted = Fault::from(NotExhausted::not_closed(Truncation::Budget));
        assert!(budgeted.to_string().contains("budget"), "{budgeted}");
    }

    #[test]
    fn a_refusal_of_a_taken_handle_names_the_taken_presupposition() {
        let taken = Fault::from(NotExhausted::already_taken());
        assert!(matches!(
            taken.refused(),
            Refused::Request(Presupposition::Taken)
        ));
    }

    #[test]
    fn an_unclosed_refusal_has_no_error_source() {
        let refusal = NotExhausted::not_closed(Truncation::Budget);
        assert!(std::error::Error::source(&refusal).is_none());
    }

    /// A malformed run that ends at once and reports its conclusion only the
    /// first time it is asked — a conclusion that does not stay put.
    struct FickleRun {
        asked: std::cell::Cell<u32>,
    }

    impl Run for FickleRun {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            None
        }
        fn conclusion(&self) -> Option<Conclusion> {
            let asked = self.asked.get();
            self.asked.set(asked + 1);
            (asked == 0).then_some(Conclusion::Budget)
        }
    }

    #[test]
    fn a_run_whose_conclusion_vanishes_after_its_end_is_refused_as_a_breach() {
        // The end of the stream read a conclusion, so no breach was recorded
        // there; the drain then reads none — the run protocol broken all the same.
        let mut solved = solved_with(Box::new(FickleRun {
            asked: std::cell::Cell::new(0),
        }));
        let refusal = solved.all_models().unwrap_err();
        let cause = std::error::Error::source(&refusal)
            .and_then(|source| source.downcast_ref::<Fault>())
            .expect("the breach is the refusal's cause");
        assert!(cause.is_backend_bug());
    }

    #[test]
    fn a_faulted_refusal_says_the_search_faulted() {
        let refusal = NotExhausted::faulted(Fault::engine("the engine died"));
        assert!(refusal.to_string().contains("faulted"), "{refusal}");
    }

    #[test]
    fn a_faulted_partial_converts_to_its_cause() {
        let cause = Fault::engine("the engine died mid-search");
        assert_eq!(Fault::from(Partial::faulted(cause.clone())), cause);
    }

    #[test]
    fn a_truncated_partial_converts_to_a_request_fault_naming_its_truncation() {
        let truncated = Fault::from(Partial::truncated(Truncation::Budget));
        assert!(matches!(
            truncated.refused(),
            Refused::Request(Presupposition::Unclosed(Truncation::Budget))
        ));
    }

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
    fn a_truncated_partial_stopped_at_the_truncation_it_was_built_with() {
        for (truncation, _) in TRUNCATIONS {
            let partial = Partial::truncated(truncation);
            assert_eq!(partial.stopped(), Stopped::Concluded(truncation));
        }
    }

    #[test]
    fn a_faulted_partial_stopped_at_its_fault() {
        let cause = Fault::engine("engine died");
        let partial = Partial::faulted(cause.clone());
        assert_eq!(partial.stopped(), Stopped::Faulted(&cause));
    }

    #[test]
    fn a_cloned_partial_equals_its_original() {
        let partial = Partial::truncated(Truncation::Budget);
        assert_eq!(partial.clone(), partial);
    }

    #[test]
    fn a_partial_s_debug_view_names_its_conclusion() {
        let partial = Partial::truncated(Truncation::Interrupted);
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

    /// A stub engine-free run: a fixed model sequence, and a terminal conclusion
    /// known only after the drain reaches its end — a lazy engine does not know
    /// its conclusion until the search reaches its end.
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            if let Some(set) = self.sets.next() {
                Some(Ok(Model::of(set)))
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            Some(Err(Fault::engine("stub engine fault")))
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A malformed run that yields one model, then ends and never reports a
    /// conclusion.
    struct SilentAfterAModelRun {
        yielded: bool,
    }
    impl Run for SilentAfterAModelRun {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            if self.yielded {
                return None;
            }
            self.yielded = true;
            Some(Ok(Model::of(singleton(0))))
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    #[test]
    fn a_run_ending_without_a_conclusion_is_one_adapter_fault_at_every_door() {
        // Its zero-model twin resolves inconclusive at the fault; with a model
        // witnessed, the complete collection refuses with the same fault as its
        // cause — never a request fault that clears the bug bit.
        let Determination::Inconclusive(partial) =
            solved_with(Box::new(SilentRun)).into_determination()
        else {
            panic!("Inconclusive");
        };
        let Stopped::Faulted(unconcluded) = partial.stopped() else {
            panic!("a fault, not a conclusion");
        };
        let mut witnessed = solved_with(Box::new(SilentAfterAModelRun { yielded: false }));
        let refused = Fault::from(
            witnessed
                .all_models()
                .expect_err("an unconcluded search refuses"),
        );
        assert_eq!(&refused, unconcluded);
        assert!(refused.is_backend_bug());
    }

    #[test]
    fn a_run_ending_without_a_conclusion_reads_no_conclusion() {
        let mut solved = solved_with(Box::new(SilentAfterAModelRun { yielded: false }));
        let _drain: Vec<_> = solved.models().collect();
        assert_eq!(solved.conclusion(), None);
    }

    /// A malformed run that ends at once and never reports a conclusion.
    struct SilentRun;
    impl Run for SilentRun {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            if let Some(set) = self.models.next() {
                Some(Ok(Model::of(set)))
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            self.pulls += 1;
            assert!(
                self.pulls <= self.limit,
                "the stream over-pulled — not lazy"
            );
            Some(Ok(Model::of(singleton(0))))
        }
        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A run that faults at its first pull, then ends reporting a clean
    /// conclusion — a search whose only word was an engine fault, over a run that
    /// breaks the after-a-fault obligation.
    struct FaultThenEndRun {
        step: usize,
    }
    impl Run for FaultThenEndRun {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            self.step += 1;
            match self.step {
                1 => Some(Ok(Model::of(singleton(0)))),
                2 => Some(Err(Fault::engine("transient fault"))),
                3 => Some(Ok(Model::of(singleton(1)))),
                _ => None,
            }
        }
        fn conclusion(&self) -> Option<Conclusion> {
            (self.step >= 4).then_some(Conclusion::Exhausted)
        }
    }

    /// An answer set containing exactly the given numbered symbols.
    fn answer_set(symbols: &[i32]) -> AnswerSet {
        symbols
            .iter()
            .map(|&n| themelios_program::Symbol::number(n))
            .collect()
    }

    /// A one-symbol answer set — dummy content the exhaustion gate never inspects.
    fn singleton(symbol: i32) -> AnswerSet {
        answer_set(&[symbol])
    }

    #[test]
    fn a_model_reads_back_the_answer_set_it_was_built_over() {
        assert_eq!(*Model::of(answer_set(&[1, 2])).atoms(), answer_set(&[1, 2]));
    }

    /// The strongly negated constant `-name`.
    fn negated(name: &str) -> Symbol {
        Symbol::function(Name::new(name).expect("an identifier"), [], Sign::Negative)
    }

    #[test]
    fn a_model_holding_an_atom_and_its_contrary_is_inconsistent() {
        let model = Model::of([constant("a"), negated("a")].into_iter().collect());
        assert!(!model.is_consistent());
    }

    #[test]
    fn a_model_holding_a_negated_atom_alone_is_consistent() {
        let model = Model::of([negated("a"), constant("b")].into_iter().collect());
        assert!(model.is_consistent());
    }

    #[test]
    fn a_model_built_over_an_answer_set_carries_no_assignment() {
        let model = Model::of(answer_set(&[1]));
        assert_eq!(*model.assignment(), TheoryAssignments::default());
    }

    /// A live run over `run`, ranging over the empty scenario, of a program
    /// with no `#show` directive.
    fn live_with(run: Box<dyn Run>) -> LiveRun<'static> {
        live_showing(run, ShowRule::default())
    }

    /// A live run over `run`, ranging over the empty scenario, under the show
    /// rule `show`.
    fn live_showing(run: Box<dyn Run>, show: ShowRule) -> LiveRun<'static> {
        Solved::running(run, Scenario::default(), show).live
    }

    // ---- The show rule and the display (§5.1) ----

    /// The positive ground constant `name`.
    fn atom(name: &str) -> Symbol {
        Symbol::function(Name::new(name).expect("an identifier"), [], Sign::Positive)
    }

    /// The rule of the one directive `#show sign name/0.`.
    fn showing_constant(sign: Sign, name: &str) -> ShowRule {
        ShowRule::of([&Show::Signature(Signature {
            sign,
            name: Name::new(name).expect("an identifier"),
            arity: 0,
        })])
    }

    #[test]
    fn show_nothing_shows_no_atom() {
        assert!(!ShowRule::of([&Show::All]).shows(&atom("a")));
    }

    #[test]
    fn a_strongly_negated_signature_shows_its_negation() {
        assert!(showing_constant(Sign::Negative, "p").shows(&negated("p")));
    }

    #[test]
    fn a_strongly_negated_signature_hides_its_positive_atom() {
        assert!(!showing_constant(Sign::Negative, "p").shows(&atom("p")));
    }

    #[test]
    fn a_signature_hides_another_arity() {
        let unary = Symbol::function(
            Name::new("p").expect("an identifier"),
            [Symbol::number(1)],
            Sign::Positive,
        );
        assert!(!showing_constant(Sign::Positive, "p").shows(&unary));
    }

    #[test]
    fn a_restricting_rule_hides_a_number() {
        assert!(!ShowRule::of([&Show::All]).shows(&Symbol::number(1)));
    }

    #[test]
    fn a_term_directive_restricts_nothing() {
        let rule = ShowRule::of([&Show::Term(Term::from(atom("p")))]);
        assert!(rule.shows_every_atom());
    }

    /// A run yielding one model, then reporting its search closed.
    struct OneModel(Option<Model>);

    impl Run for OneModel {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            self.0.take().map(Ok)
        }
        fn conclusion(&self) -> Option<Conclusion> {
            self.0.is_none().then_some(Conclusion::Exhausted)
        }
    }

    /// The model `rule` streams from the model of atoms `{q}` and `terms`.
    fn streamed(rule: ShowRule, terms: &[Symbol]) -> Model {
        let model = Model::of([atom("q")].into_iter().collect()).with_terms(terms.iter().cloned());
        let mut live = live_showing(Box::new(OneModel(Some(model))), rule);
        live.next().expect("a model").expect("no fault")
    }

    #[test]
    fn show_nothing_streams_only_the_displayed_terms() {
        // `q. #show. #show p : q.` — the answer set {q}, the display {p}.
        let model = streamed(ShowRule::of([&Show::All]), &[atom("p")]);
        let expected: BTreeSet<Symbol> = [atom("p")].into_iter().collect();
        assert_eq!(model.shown().symbols(), &expected);
    }

    #[test]
    fn the_rule_of_no_directive_streams_every_atom_and_term() {
        let model = streamed(ShowRule::default(), &[atom("p")]);
        let expected: BTreeSet<Symbol> = [atom("p"), atom("q")].into_iter().collect();
        assert_eq!(model.shown().symbols(), &expected);
    }

    #[test]
    fn a_model_of_a_program_without_directives_stores_no_display() {
        assert!(streamed(ShowRule::default(), &[]).display.is_none());
    }

    #[test]
    fn a_display_equal_to_the_answer_set_is_not_stored() {
        // `q. #show q/0.` — the rule shows every true atom there is.
        let model = streamed(showing_constant(Sign::Positive, "q"), &[]);
        assert!(model.display.is_none());
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

    /// An unscoped inconsistency: no scenario in scope, so no blame.
    fn some_unsat() -> Unsat {
        Unsat { blame: None }
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
        for (truncation, _) in TRUNCATIONS {
            let inconclusive = Determination::Inconclusive(Partial::truncated(truncation));
            assert_eq!(decided(&inconclusive), None, "{truncation:?}");
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
    fn all_models_refuses_a_budget_truncated_search() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Budget);
        assert!(matches!(solved.all_models(), Err(NotExhausted { .. })));
    }

    #[test]
    fn all_models_yields_when_the_space_closed() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        assert_eq!(solved.all_models().unwrap().len(), 2);
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
            assert_eq!(solved.all_models().is_ok(), complete, "{terminal:?}");
        }
    }

    #[test]
    fn a_partially_consumed_stream_cannot_be_read_as_complete() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _first = solved.models().next();
        assert!(matches!(solved.all_models(), Err(NotExhausted { .. })));
    }

    #[test]
    fn a_fault_during_the_drain_becomes_the_refusals_cause() {
        let mut solved = solved_with(Box::new(MidFaultRun::new(vec![singleton(0)])));
        let refusal = solved.all_models().unwrap_err();
        assert!(matches!(refusal.reason, Incompleteness::Faulted(_)));
    }

    #[test]
    fn a_mid_stream_fault_surfaces_at_the_stream_item() {
        // A model, then the fault at the item — not a clean end.
        let mut solved = solved_with(Box::new(MidFaultRun::new(vec![singleton(0)])));
        let ok_ness: Vec<bool> = solved.models().map(|item| item.is_ok()).collect();
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
            .models()
            .take(STREAM_PREFIX)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(prefix.len(), STREAM_PREFIX);
    }

    // ---- The completeness refusal renders and chains (§5.3) ----

    #[test]
    fn a_refusal_names_the_conclusion_the_search_reached() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Budget);
        let refusal = solved.all_models().unwrap_err();
        assert!(format!("{refusal}").contains("budget"), "{refusal}");
    }

    #[test]
    fn an_unconcluded_search_s_refusal_is_caused_by_the_protocol_breach() {
        // A run that ends without concluding breaks the run protocol (§5.2): the
        // refusal's cause is that breach, the adapter's fault.
        let mut solved = solved_with(Box::new(SilentRun));
        let refusal = solved.all_models().unwrap_err();
        let cause = std::error::Error::source(&refusal)
            .and_then(|source| source.downcast_ref::<Fault>())
            .expect("the breach is the refusal's cause");
        assert!(cause.is_backend_bug());
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
        let refusal = solved.all_models().unwrap_err();
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
        for (_, conclusion) in TRUNCATIONS {
            let solved = solved_over(vec![], conclusion);
            assert!(
                matches!(solved.into_determination(), Determination::Inconclusive(_)),
                "{conclusion:?}"
            );
        }
    }

    #[test]
    fn a_run_that_ends_without_a_conclusion_stops_at_an_adapter_fault() {
        // The run broke the terminal obligation; the core says so rather than
        // read the silence as any conclusion.
        let solved = solved_with(Box::new(SilentRun));
        let Determination::Inconclusive(partial) = solved.into_determination() else {
            panic!("Inconclusive");
        };
        let Stopped::Faulted(fault) = partial.stopped() else {
            panic!("a fault, not a conclusion");
        };
        assert_eq!(fault.locus(), crate::contract::Locus::Adapter);
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
    fn a_first_pull_fault_is_the_partial_s_stopping_reason() {
        let solved = solved_with(Box::new(FaultyRun));
        let Determination::Inconclusive(partial) = solved.into_determination() else {
            panic!("Inconclusive");
        };
        assert_eq!(
            partial.stopped(),
            Stopped::Faulted(&Fault::engine("stub engine fault"))
        );
    }

    #[test]
    fn a_faulted_search_reports_no_conclusion() {
        // The run reports `Exhausted` once it ends, against the after-a-fault
        // obligation; the handle reads no conclusion anyway, since a faulted
        // search reached none.
        let mut solved = solved_with(Box::new(FaultThenEndRun { step: 0 }));
        let _drain: Vec<_> = solved.models().collect();
        assert_eq!(solved.conclusion(), None);
    }

    #[test]
    fn faulted_models_report_no_exhaustion() {
        // A model, then a fault, then a run claiming `Exhausted`: the report
        // agrees with the gate, which refuses.
        let solved = solved_with(Box::new(RecoveringRun { step: 0 }));
        let Determination::Consistent(mut models) = solved.into_determination() else {
            panic!("Consistent");
        };
        let _drain: Vec<_> = models.members().collect();
        assert!(!models.is_exhausted());
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
        let all = solved.all_models().unwrap();
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
        assert_eq!(*models.scenario(), Scenario::default());
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
        assert_eq!(*models.scenario(), Scenario::default());
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
        assert_eq!(solved.all_models().unwrap().len(), 2);
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn a_streamed_consistent_search_still_resolves_consistent() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _all: Vec<_> = solved.models().collect();
        assert!(matches!(
            solved.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn a_partially_consumed_consistent_search_still_resolves_consistent() {
        let mut solved = solved_over(vec![singleton(0), singleton(1)], Conclusion::Exhausted);
        let _first = solved.models().next();
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
        assert_eq!(solved.all_models().unwrap().len(), 2);
    }

    #[test]
    fn an_empty_inspected_search_yields_an_empty_complete_collection() {
        let mut solved = solved_over(vec![], Conclusion::Exhausted);
        assert!(matches!(
            solved.determination(),
            Determination::Inconsistent(_)
        ));
        assert!(solved.all_models().unwrap().is_empty());
    }

    #[test]
    fn a_run_that_ends_without_a_conclusion_refuses_a_complete_collection() {
        let mut solved = solved_with(Box::new(SilentRun));
        let refusal = solved.all_models().unwrap_err();
        assert!(matches!(refusal.reason, Incompleteness::Faulted(_)));
    }

    #[test]
    fn a_refusal_after_a_successful_drain_says_the_models_were_taken() {
        let mut solved = solved_over(vec![singleton(0)], Conclusion::Exhausted);
        let _all = solved.all_models().unwrap();
        let refusal = solved.all_models().unwrap_err();
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
        assert!(solved.all_models().is_err());
    }

    #[test]
    fn a_fault_aborted_drain_cannot_later_claim_a_complete_collection() {
        // The first drain pulls a model, hits the fault, and drops what it pulled;
        // a re-drain must refuse, never return a collection missing the dropped
        // model (even over a run that recovers).
        let mut solved = solved_with(Box::new(RecoveringRun { step: 0 }));
        assert!(solved.all_models().is_err());
        assert!(solved.all_models().is_err());
    }

    #[test]
    fn a_faulted_handle_keeps_its_fault_as_the_cause_across_refusals() {
        let mut solved = solved_with(Box::new(FaultThenEndRun { step: 0 }));
        let _ = solved.determination();
        for _ in 0..2 {
            assert!(matches!(
                solved.all_models().unwrap_err().reason,
                Incompleteness::Faulted(_)
            ));
        }
    }

    // ---- Assumption blame (§5.4) ----

    /// The assumptions a blame names: that the constant atom `p` holds.
    fn culprits() -> Box<[Assumption]> {
        Box::from([Assumption::new(constant("p"), true).expect("an atom")])
    }

    /// The readings of blame under which no assumption is named.
    fn blaming_none() -> [Refutation; 2] {
        [Refutation::NotThese, Refutation::NoAssumptions]
    }

    /// An assumption-scoped inconsistency answering blame with `refutation`.
    fn unsat_blaming(refutation: Refutation) -> Unsat {
        Unsat {
            blame: Some(refutation),
        }
    }

    #[test]
    fn an_unscoped_inconsistency_carries_no_blame() {
        assert!(some_unsat().blame().is_none());
    }

    #[test]
    fn a_scoped_inconsistency_names_its_responsible_subset() {
        let unsat = unsat_blaming(Refutation::These(culprits()));
        assert_eq!(unsat.blame(), Some(Refutation::These(culprits())));
    }

    #[test]
    fn a_scoped_inconsistency_may_blame_none_of_its_assumptions() {
        for refutation in blaming_none() {
            let unsat = unsat_blaming(refutation.clone());
            assert_eq!(unsat.blame().as_ref(), Some(&refutation));
        }
    }

    #[test]
    fn reading_the_blame_does_not_spend_it() {
        // The reader clones the blame out; the payload keeps it.
        let unsat = unsat_blaming(Refutation::NotThese);
        let _first = unsat.blame();
        assert_eq!(unsat.blame(), Some(Refutation::NotThese));
    }

    #[test]
    fn a_cloned_unsat_equals_its_original() {
        let unsat = unsat_blaming(Refutation::These(culprits()));
        assert_eq!(unsat.clone(), unsat);
    }

    #[test]
    fn an_unscoped_unsat_is_unequal_to_a_scoped_one() {
        for refutation in blaming_none() {
            assert_ne!(some_unsat(), unsat_blaming(refutation));
        }
    }

    #[test]
    fn an_unsat_s_debug_view_shows_its_blame() {
        let unsat = unsat_blaming(Refutation::NoAssumptions);
        let rendered = format!("{unsat:?}");
        assert!(rendered.contains("NoAssumptions"), "{rendered}");
    }

    // ---- Consequences (§5.2) ----

    /// Both modes, so a law over the mode holds of each.
    const MODES: [Mode; 2] = [Mode::Cautious, Mode::Brave];

    /// A constant symbol by name.
    fn constant(name: &str) -> Symbol {
        Symbol::constant(Name::new(name).expect("an identifier"))
    }

    /// The consequences `mode` produced over `symbols`, built here as the
    /// derived and native doors will build them.
    fn consequences(mode: Mode, symbols: &[Symbol]) -> Consequences {
        Consequences {
            symbols: symbols.iter().cloned().collect(),
            mode,
        }
    }

    fn cautious_consequences(symbols: &[Symbol]) -> Consequences {
        consequences(Mode::Cautious, symbols)
    }

    #[test]
    fn consequences_carry_the_mode_that_produced_them() {
        // A value that has travelled still says which question it answers.
        for mode in MODES {
            let travelled = consequences(mode, &[constant("a")]);
            assert_eq!(travelled.mode(), mode);
        }
    }

    #[test]
    fn consequences_yield_the_symbols_they_were_built_over() {
        let cautious = cautious_consequences(&[constant("a"), constant("b")]);
        let yielded: BTreeSet<Symbol> = cautious.symbols().cloned().collect();
        assert_eq!(yielded, BTreeSet::from([constant("a"), constant("b")]));
    }

    #[test]
    fn consequences_yield_their_symbols_in_the_term_order() {
        let reversed = cautious_consequences(&[constant("b"), constant("a")]);
        let yielded: Vec<Symbol> = reversed.symbols().cloned().collect();
        assert_eq!(yielded, vec![constant("a"), constant("b")]);
    }

    #[test]
    fn consequences_yield_a_repeated_symbol_once() {
        let repeated = cautious_consequences(&[constant("a"), constant("a")]);
        assert_eq!(repeated.symbols().count(), 1);
    }

    #[test]
    fn consequences_contain_each_symbol_they_carry() {
        let cautious = cautious_consequences(&[constant("a"), constant("b")]);
        for member in [constant("a"), constant("b")] {
            assert!(cautious.contains(&member), "{member:?}");
        }
    }

    #[test]
    fn consequences_do_not_contain_a_symbol_they_do_not_carry() {
        let cautious = cautious_consequences(&[constant("a")]);
        assert!(!cautious.contains(&constant("b")));
    }

    #[test]
    fn a_cloned_consequence_set_equals_its_original() {
        let cautious = cautious_consequences(&[constant("a")]);
        assert_eq!(cautious.clone(), cautious);
    }

    #[test]
    fn consequence_sets_differing_only_in_mode_are_unequal() {
        // The mode is part of the value: the same symbols read cautiously and
        // bravely answer different questions (§5.2).
        let symbols = [constant("a")];
        assert_ne!(
            consequences(Mode::Cautious, &symbols),
            consequences(Mode::Brave, &symbols)
        );
    }

    #[test]
    fn a_consequence_set_s_debug_view_names_its_mode() {
        for mode in MODES {
            let rendered = format!("{:?}", consequences(mode, &[]));
            assert!(rendered.contains(&format!("{mode:?}")), "{rendered}");
        }
    }

    // ---- Cautious/brave consequence folding (§4.2, query.md §2.4) ----

    /// The folded symbols as a set, for exact comparison against an expectation.
    fn folded(mode: Mode, members: &[AnswerSet]) -> AnswerSet {
        Consequences::fold(mode, members)
            .expect("a fold over members")
            .symbols()
            .cloned()
            .collect()
    }

    #[test]
    fn cautious_consequences_are_the_intersection() {
        let members = [answer_set(&[1, 2]), answer_set(&[1, 3])];
        assert_eq!(folded(Mode::Cautious, &members), answer_set(&[1]));
    }

    #[test]
    fn brave_consequences_are_the_union() {
        let members = [answer_set(&[1, 2]), answer_set(&[1, 3])];
        assert_eq!(folded(Mode::Brave, &members), answer_set(&[1, 2, 3]));
    }

    #[test]
    fn cautious_folding_intersects_across_three_members() {
        // {1,2,3} ∩ {1,2} ∩ {2,4} = {2}; a fold that stopped after the second
        // member would wrongly keep 1.
        let members = [
            answer_set(&[1, 2, 3]),
            answer_set(&[1, 2]),
            answer_set(&[2, 4]),
        ];
        assert_eq!(folded(Mode::Cautious, &members), answer_set(&[2]));
    }

    #[test]
    fn brave_folding_unions_across_three_members() {
        let members = [
            answer_set(&[1, 2, 3]),
            answer_set(&[1, 2]),
            answer_set(&[2, 4]),
        ];
        assert_eq!(folded(Mode::Brave, &members), answer_set(&[1, 2, 3, 4]));
    }

    #[test]
    fn a_single_model_folds_to_itself_under_either_mode() {
        let members = [answer_set(&[1, 2])];
        assert_eq!(folded(Mode::Cautious, &members), answer_set(&[1, 2]));
        assert_eq!(folded(Mode::Brave, &members), answer_set(&[1, 2]));
    }

    #[test]
    fn folding_no_models_yields_no_consequences() {
        // No world view is empty, and a set over no models certifies nothing —
        // under ⋃ as under ⋂.
        let members: [AnswerSet; 0] = [];
        for mode in MODES {
            assert_eq!(Consequences::fold(mode, &members), None, "{mode:?}");
        }
    }

    #[test]
    fn a_folded_consequence_carries_its_mode() {
        let members = [answer_set(&[1])];
        for mode in MODES {
            let consequences = Consequences::fold(mode, &members).expect("one member");
            assert_eq!(consequences.mode(), mode);
        }
    }

    // ---- The optimization register (§5.2, §5.3) ----

    #[test]
    fn an_incumbent_reads_back_its_retained_model() {
        // Built here, where its door will be: a step of the trajectory holds the
        // model the search retained, its whole answer set.
        let model = Model::of(singleton(1));
        let incumbent = Incumbent {
            model: model.clone(),
        };
        assert_eq!(incumbent.model(), &model);
    }

    /// An optimization handle over `sets`, ending `terminal` — the register's
    /// laws are `Solved`'s, over the same live run.
    fn optimized_over(sets: Vec<AnswerSet>, terminal: Conclusion) -> Optimized<'static> {
        Optimized {
            live: live_with(Box::new(StubRun::new(sets, terminal))),
        }
    }

    #[test]
    fn an_optimized_search_with_a_model_resolves_consistent() {
        let optimized = optimized_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(matches!(
            optimized.into_determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn an_empty_closed_optimized_search_resolves_inconsistent() {
        let optimized = optimized_over(vec![], Conclusion::Exhausted);
        assert!(matches!(
            optimized.into_determination(),
            Determination::Inconsistent(_)
        ));
    }

    #[test]
    fn a_cut_optimized_search_with_no_model_resolves_inconclusive() {
        for (_, conclusion) in TRUNCATIONS {
            let optimized = optimized_over(vec![], conclusion);
            assert!(
                matches!(
                    optimized.into_determination(),
                    Determination::Inconclusive(_)
                ),
                "{conclusion:?}"
            );
        }
    }

    #[test]
    fn the_inspecting_resolver_leaves_the_optimized_handle_live() {
        // A reborrow, not a move: the handle answers a second inspection.
        let mut optimized = optimized_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(matches!(
            optimized.determination(),
            Determination::Consistent(_)
        ));
        assert!(matches!(
            optimized.determination(),
            Determination::Consistent(_)
        ));
    }

    #[test]
    fn an_unresolved_optimized_run_has_no_conclusion() {
        let optimized = optimized_over(vec![], Conclusion::Exhausted);
        assert_eq!(optimized.conclusion(), None);
    }

    #[test]
    fn a_resolved_optimized_run_reports_its_conclusion() {
        let mut optimized = optimized_over(vec![], Conclusion::Budget);
        let _ = optimized.determination();
        assert_eq!(optimized.conclusion(), Some(Conclusion::Budget));
    }

    #[test]
    fn no_optimum_is_reported_while_optimization_is_reserved() {
        let optimized = optimized_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(optimized.optimum().is_none());
    }

    #[test]
    fn no_trajectory_is_reported_while_optimization_is_reserved() {
        let mut optimized = optimized_over(vec![singleton(0)], Conclusion::Exhausted);
        assert!(optimized.trajectory().is_none());
    }

    // ---- Theory assignments and statistics (§5.4) ----

    /// How many measurements the counted statistics source holds.
    const MEASUREMENTS: usize = 3;

    /// A statistics source holding `MEASUREMENTS` measurements, built here, in
    /// the defining crate, as an engine adapter will build them.
    struct Counted;

    impl Statistics for Counted {
        fn measurements(&self) -> impl Iterator<Item = Measurement> + '_ {
            std::iter::repeat_n(Measurement {}, MEASUREMENTS)
        }
    }

    #[test]
    fn statistics_yield_each_measurement_the_source_holds() {
        assert_eq!(Counted.measurements().count(), MEASUREMENTS);
    }

    #[test]
    fn a_cloned_measurement_equals_its_original() {
        let measurement = Measurement {};
        assert_eq!(measurement.clone(), measurement);
    }

    #[test]
    fn a_measurement_s_debug_view_names_the_type() {
        let rendered = format!("{:?}", Measurement {});
        assert!(rendered.contains("Measurement"), "{rendered}");
    }

    #[test]
    fn a_theory_assignment_s_default_is_the_empty_component() {
        assert_eq!(TheoryAssignments::default(), TheoryAssignments {});
    }
}
