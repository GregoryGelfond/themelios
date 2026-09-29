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
//! [`NotAQuery`], that refusal. The collection those readings range over is a
//! [`WorldView`] — the live handle over a consistent program's answer sets — or
//! its engine-free, materialised [`Snapshot`], built here from the solve tier's
//! outcomes (§2.3). The matching a query rests on is the program tier's own
//! (§3.1); this tier owns only the policy over a collection of answer sets (§3.2).
#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use themelios_program::AnswerSet;
use themelios_program::program::{Arguments, Atom};
use themelios_program::symbol::{Sign, Symbol};
use themelios_program::term::{EvalError, Term, Variable};
use themelios_program::unify::{NotAPattern, Substitution, mgu, signature_range};
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::contract::{Backend, Fault, Mode};
use themelios_solve::outcome::{Determination, Models};

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
    /// Neither `Yes` nor `No`: not true in every member, and not false in every
    /// member (§2.2) — settled by some and open in the rest, or open throughout.
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
    /// query true everywhere. O(parts). The nesting is read recursively, so a
    /// pathologically deep composition can exhaust the stack; the iterative treatment
    /// deep structures get elsewhere in the stack is reserved for `Query`.
    pub fn all(parts: impl IntoIterator<Item = Query>) -> Query {
        Query {
            shape: Shape::Conjunction(parts.into_iter().collect()),
        }
    }

    /// The disjunction (∨) of `parts` (docs/design/query.md §2.1, §2.2):
    /// evaluated within each member of a world view as the strongest of its
    /// parts over `false < unknown < true`. Total; the empty disjunction is the
    /// query false everywhere. O(parts). Deeply nested composition carries the same
    /// stack caveat as [`all`](Query::all).
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

/// A program's *world view* (docs/design/query.md §2.3): the live reading over a
/// consistent program's answer sets, driving the engine that produced them. It is
/// built only from a resolved `Determination::Consistent(Models)` (solve.md §5.2)
/// through [`of`](WorldView::of), so a world view that exists is **non-empty by
/// construction** — an inconsistent program is `Determination::Inconsistent`, never
/// an empty world view, and there is no empty value here to mistake for one.
///
/// The reads borrow the engine, so a member stream is fallible (a mid-stream engine
/// fault surfaces at the item) and touching it forfeits completeness;
/// [`materialize`](WorldView::materialize) drains the view once into an engine-free
/// [`Snapshot`] whose reads are infallible and repeatable. Holding a
/// [`members`](WorldView::members) stream borrows the handle mutably, so a second
/// overlapping read cannot even be written — the borrow checker is the
/// serialisation, not a run-time re-entrancy check (§2.3).
pub struct WorldView<'a> {
    models: Models<'a>,
}

impl<'a> WorldView<'a> {
    /// Build the world view over `models` — the construction door, on the query
    /// side, from a resolved `Consistent(Models)` (docs/design/query.md §2.3): the
    /// solve tier does not depend on the query tier, so the `Models` → `WorldView`
    /// transition lives here. O(1).
    pub fn of(models: Models<'a>) -> WorldView<'a> {
        WorldView { models }
    }

    /// Stream the answer sets (docs/design/query.md §2.3): each item a `Result`, so
    /// a mid-stream engine fault surfaces at the item, not as a clean end. Touching
    /// the stream forfeits the complete collection
    /// [`materialize`](WorldView::materialize) would drain. Borrows the handle for
    /// the stream's life. Cost: O(1) resident.
    pub fn members(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_ {
        self.models.members()
    }

    /// Whether the search has so far closed the space (docs/design/query.md §2.3):
    /// it reads the conclusion the run has reached. A lazily-enumerating run reports
    /// that conclusion only once its stream has been driven to the end, so a fresh
    /// world view over one reports `false` until it is drained — by streaming its
    /// members, or by [`materialize`](WorldView::materialize) — and then reflects
    /// the true conclusion; an eagerly-deciding run may report it sooner. The
    /// completeness gate `materialize` enforces does not rest on this reading; it
    /// drains and gates directly, so a truncated search cannot pass as complete
    /// whatever this returns. Total; O(1).
    pub fn is_exhausted(&self) -> bool {
        self.models.is_exhausted()
    }

    /// The scenario the answer sets range over (docs/design/query.md §2.3): the
    /// assumptions in force, or the empty scenario for the unscoped program.
    /// Borrowed; reading does not spend the handle. O(1).
    pub fn scenario(&self) -> &Scenario {
        self.models.scenario()
    }

    /// Drain the world view into an engine-free [`Snapshot`] (docs/design/query.md
    /// §2.3): read every member and hold them as owned data whose reads are then
    /// infallible and repeatable. **Refuses** a world view whose search did not
    /// close the space, or whose members were already streamed — a `Snapshot` is
    /// complete and non-empty by construction, so a partial or non-exhausted view
    /// must not pass as one. The gate is the solve tier's own completeness refusal,
    /// carried as its [`Fault`], so a truncated search cannot be laundered into a
    /// complete snapshot. Cost: O(members) to drain, plus a clone of the scenario.
    pub fn materialize(mut self) -> Result<Snapshot, Fault> {
        let members = self.models.all_members()?;
        let scenario = self.models.scenario().clone();
        Ok(Snapshot::of(members, scenario))
    }
}

/// The engine-free form of a world view (docs/design/query.md §2.3): the owned,
/// materialised answer sets and the scenario they range over, every reading
/// **infallible** — no engine remains to fault, no stream to exhaust. Built only by
/// [`WorldView::materialize`], from a world view whose search closed the space, so
/// a snapshot is **complete and non-empty by construction**: its cautious and brave
/// consequences are the true ⋂ and ⋃ over the whole collection, never a partial
/// fold posing as complete.
///
/// Owned plain data; the readings borrow it and never spend it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Snapshot {
    members: Vec<AnswerSet>,
    scenario: Scenario,
}

/// The three-valued truth of a query within one member (docs/design/query.md
/// §2.2): the lattice `False < Unknown < True`, so a conjunction is the weakest
/// (`min`) of its parts and a disjunction the strongest (`max`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Truth {
    False,
    Unknown,
    True,
}

/// The contrary of a ground literal symbol — its strong negation, the same
/// function symbol with the sign flipped (docs/design/query.md §2.2). A query
/// literal is always a function symbol (built by [`Query::of`]), so the contrary
/// is always defined; any other symbol is returned unchanged (unreachable through
/// a query). `O(arity)`.
fn contrary(symbol: &Symbol) -> Symbol {
    match symbol {
        Symbol::Function {
            name,
            arguments,
            sign,
        } => Symbol::function(
            name.clone(),
            arguments.iter().cloned(),
            match sign {
                Sign::Positive => Sign::Negative,
                Sign::Negative => Sign::Positive,
            },
        ),
        other => other.clone(),
    }
}

/// The contrary of a pattern — the same atom with its strong sign flipped (docs/
/// design/query.md §2.5). A binding is a `no` where the *contrary* pattern is
/// cautiously matched, so `-p(X)` is what finds the refuted instances of `p(X)`.
/// The arguments carry through unchanged, only the sign flips. `O(pattern)`.
fn contrary_pattern(pattern: &Atom) -> Atom {
    Atom {
        sign: match pattern.sign {
            Sign::Positive => Sign::Negative,
            Sign::Negative => Sign::Positive,
        },
        name: pattern.name.clone(),
        arguments: pattern.arguments.clone(),
    }
}

impl Query {
    /// The three-valued truth of this query within one `member` (docs/design/
    /// query.md §2.2): a literal is `True` if present, `False` if its contrary is
    /// present, `Unknown` otherwise; a conjunction is the weakest of its parts, a
    /// disjunction the strongest. Evaluated WITHIN the member — never over ⋂/⋃ — so
    /// a compound that holds through different literals in different members reads
    /// correctly. The empty conjunction is `True`, the empty disjunction `False`.
    /// Cost: `O(query size × member lookup)`.
    fn truth_in(&self, member: &AnswerSet) -> Truth {
        match &self.shape {
            Shape::Literal(symbol) => {
                if member.contains(symbol) {
                    Truth::True
                } else if member.contains(&contrary(symbol)) {
                    Truth::False
                } else {
                    Truth::Unknown
                }
            }
            Shape::Conjunction(parts) => parts
                .iter()
                .map(|part| part.truth_in(member))
                .min()
                .unwrap_or(Truth::True),
            Shape::Disjunction(parts) => parts
                .iter()
                .map(|part| part.truth_in(member))
                .max()
                .unwrap_or(Truth::False),
        }
    }
}

/// The three-valued partition of an open pattern's ground instances over a world
/// view (docs/design/query.md §2.5): each instance is `yes` (present in every
/// member), `no` (its contrary present in every member — refuted, more than merely
/// absent), or `unknown` (mentioned by some member but settled by none). The same
/// trichotomy as [`Answer`], carried to a pattern's bindings rather than to a single
/// ground query.
///
/// The cells are pairwise disjoint — a *partition* — over a conforming backend,
/// whose answer sets are consistent (no member holds an atom and its contrary): that
/// consistency is what keeps a `yes` instance out of `no` and out of the brave
/// domain of the contrary. The reading assumes that backend contract (solve.md §5);
/// a member that violated it would not be a partition.
///
/// Owned plain data; the readings borrow it and never spend it. Non-exhaustive:
/// room for a later facet (a binding's value under an optimization objective, say)
/// as a new field, not a migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bindings {
    yes: BTreeSet<Symbol>,
    no: BTreeSet<Symbol>,
    unknown: BTreeSet<Symbol>,
}

impl Bindings {
    /// The cautiously entailed instances — the ground instances of the pattern
    /// present in EVERY member of the world view (docs/design/query.md §2.5). Exact
    /// and finite: read off the cautious consequences. Borrowed; O(n) over the set.
    pub fn yes(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.yes.iter()
    }

    /// The refuted instances — those whose *contrary* is cautiously entailed (its
    /// strong negation present in every member), reported with the pattern's own
    /// sign, the contrary of the matched contrary (docs/design/query.md §2.5): a
    /// negative pattern's `no` instances are negative. This is more than mere
    /// absence: a `no` instance is settled false by the world view, never merely
    /// unmentioned. Exact and finite. Borrowed; O(n) over the set.
    pub fn no(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.no.iter()
    }

    /// The unsettled instances — **the brave domain of the pattern, less the settled**
    /// (docs/design/query.md §2.5): the ground instances *of the pattern's sign* that
    /// some member mentions, minus those already `yes`. **This is not an exhaustive
    /// listing of every instance the program leaves open.** This tier holds answer
    /// sets, not the program that produced them, so it cannot enumerate the full
    /// Herbrand base; it lists the brave domain (what some answer set mentions with the
    /// pattern's own sign — an instance mentioned only by its *contrary* is listed
    /// under the contrary pattern, not here), and a caller must read it as that, not as
    /// proof the program leaves nothing else open. Borrowed; O(n) over the set.
    pub fn unknown(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.unknown.iter()
    }
}

/// Why an atom is not a binding pattern (docs/design/query.md §2.5): the program
/// tier's own [`NotAPattern`] (a non-denoting or pooled argument, §3.1) *plus* this
/// tier's own partition policy — an anonymous position. An anonymous `_` is a
/// well-formed pattern to the unifier (it denotes, matching anything), but it names
/// no binding: two instances that differ only in a `_` position — which the caller
/// declared "don't care" — can settle differently, so a reading keyed on the named
/// variables' binding is ill-posed. It is refused HERE as a policy (a binding
/// pattern's positions are named), at no cost to expressiveness — a fresh named
/// variable yields the same instances — never laundered into the program tier's
/// `NonDenoting` (§3.1 keeps matching apart from policy). Non-exhaustive: a later
/// reason is a new variant.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NotABindingPattern {
    /// The atom is not a pattern at all (§3.1): a non-denoting or pooled argument —
    /// the program tier's refusal, carried as this refusal's source.
    NotAPattern(NotAPattern),
    /// An argument bears an anonymous variable `_` (§2.5): well formed to the
    /// unifier, but naming no binding to key the partition on. Refused here rather
    /// than matched.
    AnonymousPosition,
}

impl std::fmt::Display for NotABindingPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotABindingPattern::NotAPattern(_) => {
                f.write_str("not a binding pattern: the atom is not a pattern")
            }
            NotABindingPattern::AnonymousPosition => {
                f.write_str("not a binding pattern: an anonymous position names no binding")
            }
        }
    }
}

impl std::error::Error for NotABindingPattern {
    /// The program tier's refusal beneath a `NotAPattern`; nothing beneath
    /// `AnonymousPosition`, which is this tier's own.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NotABindingPattern::NotAPattern(inner) => Some(inner),
            NotABindingPattern::AnonymousPosition => None,
        }
    }
}

impl From<NotAPattern> for NotABindingPattern {
    /// A program-tier non-pattern is a binding-pattern refusal, its `NotAPattern`
    /// arm — so the matcher's refusal propagates through `?`.
    fn from(refusal: NotAPattern) -> NotABindingPattern {
        NotABindingPattern::NotAPattern(refusal)
    }
}

impl Snapshot {
    /// The engine-free snapshot of `members`, ranging over `scenario` — the value
    /// [`WorldView::materialize`] drains into (docs/design/query.md §2.3).
    /// Crate-private: a snapshot is reached only by materialising a live world view,
    /// so the "complete and non-empty by construction" invariant it inherits is not
    /// forgeable from arbitrary data. O(1) — the members move in.
    pub(crate) fn of(members: Vec<AnswerSet>, scenario: Scenario) -> Snapshot {
        Snapshot { members, scenario }
    }

    /// The cautious consequences — the intersection of the answer sets, what holds
    /// in every member (docs/design/query.md §2.4). Infallible over owned data, and
    /// complete because the collection is. Cost: O(members × set size).
    pub fn cautious(&self) -> Consequences {
        Consequences::fold(Mode::Cautious, &self.members)
    }

    /// The brave consequences — the union of the answer sets, what holds in some
    /// member (docs/design/query.md §2.4). Infallible over owned data, and complete
    /// because the collection is. Cost: O(members × set size).
    pub fn brave(&self) -> Consequences {
        Consequences::fold(Mode::Brave, &self.members)
    }

    /// The answer sets, each borrowed (docs/design/query.md §2.3): reading does not
    /// spend the snapshot, and every member is present — the collection is complete
    /// by construction. Cost: O(1) to open; O(n) over the whole stream.
    pub fn members(&self) -> impl Iterator<Item = &AnswerSet> + '_ {
        self.members.iter()
    }

    /// Whether the search that produced this snapshot closed the space
    /// (docs/design/query.md §2.3): always `true`, a fact of the type — a snapshot
    /// is materialised only from an exhausted world view. Total; O(1).
    pub fn is_exhausted(&self) -> bool {
        true
    }

    /// The scenario the answer sets range over (docs/design/query.md §2.3): the
    /// assumptions in force, or the empty scenario for the unscoped program.
    /// Borrowed; reading does not spend the snapshot. O(1).
    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// The three-valued epistemic reading of a ground `query` over this world view
    /// (docs/design/query.md §2.2, the one authoritative definition): `Yes` iff the
    /// query is true in EVERY member, `No` iff false in every member — its contrary
    /// present, never merely absent — and `Unknown` otherwise. Evaluated within each
    /// member (never over ⋂/⋃), so a compound holding through different literals in
    /// different members is read correctly. Infallible and complete over the
    /// materialised members. Cost: `O(members × query size × member lookup)`.
    pub fn answer(&self, query: &Query) -> Answer {
        let mut all_true = true;
        let mut all_false = true;
        for member in &self.members {
            match query.truth_in(member) {
                Truth::True => all_false = false,
                Truth::False => all_true = false,
                Truth::Unknown => {
                    all_true = false;
                    all_false = false;
                }
            }
        }
        // A snapshot is non-empty by construction, so at most one of these holds.
        if all_true {
            Answer::Yes
        } else if all_false {
            Answer::No
        } else {
            Answer::Unknown
        }
    }

    /// The ASP-Core-2 cautious, two-valued reading (docs/design/query.md §2.6):
    /// `true` exactly when the query is [`Answer::Yes`] — cautiously entailed —
    /// projecting the three-valued [`answer`](Snapshot::answer) onto `Yes` versus
    /// (`No` ∪ `Unknown`), never collapsing `Unknown` to the wrong side. Infallible.
    /// Cost: as [`answer`](Snapshot::answer).
    pub fn entails(&self, query: &Query) -> bool {
        self.answer(query) == Answer::Yes
    }

    /// The three-valued [`Bindings`] of an open `pat` over this world view (docs/
    /// design/query.md §2.5): every ground instance of the pattern partitioned into
    /// `yes` (present in every member), `no` (its contrary present in every member),
    /// and `unknown` (the brave domain of the pattern, less the settled — the
    /// instances of `pat`'s own sign some member holds; an instance only whose
    /// *contrary* is bravely present is listed under the contrary pattern, not here).
    /// The trichotomy of [`answer`](Snapshot::answer), carried to a pattern's
    /// instances. Infallible over the materialised members once the pattern is accepted.
    ///
    /// **Refuses**, set-independently, a `pat` that is not a binding pattern (docs/
    /// design/query.md §2.5). The anonymous check runs first: an anonymous position
    /// (`p(X, _)`, at any depth) — well formed to the unifier, but naming no binding —
    /// is [`AnonymousPosition`], taking precedence for an atom that is also a
    /// non-pattern; a non-denoting or pooled argument is the program tier's
    /// [`NotAPattern`], carried. Cost: the cautious and brave folds, then, per cell, an
    /// `O(log n + k)` block scan with each candidate unified at a cost linear in its size.
    ///
    /// [`AnonymousPosition`]: NotABindingPattern::AnonymousPosition
    pub fn bindings(&self, pat: &Atom) -> Result<Bindings, NotABindingPattern> {
        // An anonymous `_` names no binding to key the partition on, so a reading over
        // the named variables' binding is ill-posed; refuse it here, before any
        // matching, as this tier's own refusal (§2.5).
        if has_anonymous_position(pat) {
            return Err(NotABindingPattern::AnonymousPosition);
        }
        // Classify the pattern once, up front and set-independently, so a non-pattern
        // refuses before the folds — the program tier's `mgu` refuses a pool and a
        // non-denoting argument alike (§3.1).
        mgu(pat, pat)?;
        // The cautious (⋂) and brave (⋃) consequences as symbol sets — the derived
        // reading a snapshot folds (§2.4), the ground the partition is read off.
        let cautious: AnswerSet = self.cautious().symbols().cloned().collect();
        let brave: AnswerSet = self.brave().symbols().cloned().collect();
        // `yes`: the instances present in every member.
        let yes: BTreeSet<Symbol> = matched_in(pat, &cautious)?.into_iter().collect();
        // `no`: the instances whose contrary is cautiously entailed, reported with the
        // pattern's sign (the contrary of each matched contrary). Disjoint from the
        // brave domain of `pat` by answer-set consistency (no member holds both `g`
        // and `-g`), so it never re-enters `unknown`.
        let no: BTreeSet<Symbol> = matched_in(&contrary_pattern(pat), &cautious)?
            .iter()
            .map(contrary)
            .collect();
        // `unknown`: the brave domain, less the settled. `yes ⊆ brave` (⋂ ⊆ ⋃), so
        // this subtraction is the whole of the disjointness the partition owes.
        let unknown: BTreeSet<Symbol> = matched_in(pat, &brave)?
            .into_iter()
            .filter(|instance| !yes.contains(instance))
            .collect();
        Ok(Bindings { yes, no, unknown })
    }
}

/// Lift a ground `Symbol` to the signed `Atom` that matches it (docs/design/
/// query.md §3.1). Only a `Function` symbol denotes an atom a pattern can match;
/// a number, string, tuple, `#inf`, or `#sup` has no signature and never
/// matches, so it lifts to `None`. The arguments become `Symbolic` terms — the
/// value each already is — so the mgu reads them without re-evaluation, and the
/// sign is carried through unchanged. `O(arity)`.
///
/// The building block of `matches_in` and `matched_in`; the bindings partition
/// (§2.5) reaches it through the latter.
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

/// Whether any argument position of `pattern` bears an anonymous variable `_`, at
/// any depth (docs/design/query.md §2.5). The scan is deep — a `_` nested in a
/// compound argument (`p(f(_))`) counts — because an anonymous variable anywhere
/// names no binding, so [`Snapshot::bindings`] cannot attribute an instance to it
/// and refuses the pattern. Ground `Symbolic` leaves hold no variable and are not
/// descended. Iterative; O(pattern nodes).
fn has_anonymous_position(pattern: &Atom) -> bool {
    pattern
        .argument_terms()
        .flat_map(Term::subterms)
        .any(|term| matches!(term, Term::Variable(Variable::Anonymous)))
}

/// The matched ground *instances* of `pattern` in `set` — the sibling of
/// [`matches_in`] that returns the matched ground `Symbol`s rather than the
/// substitutions (docs/design/query.md §2.5). [`Snapshot::bindings`] partitions
/// instances, not bindings, so it needs the symbols an answer set holds. The
/// candidate block, the `O(log n + k)` scan, and the set-independent up-front
/// classification (a non-pattern is an `Err`, the same whether or not `set` holds a
/// same-signature member) are exactly [`matches_in`]'s; only the pushed value differs.
pub(crate) fn matched_in(pattern: &Atom, set: &AnswerSet) -> Result<Vec<Symbol>, NotAPattern> {
    mgu(pattern, pattern)?;
    let mut out = Vec::new();
    for candidate in set.range(signature_range(pattern)) {
        if let Some(atom) = lift(candidate)
            && mgu(pattern, &atom)?.is_some()
        {
            out.push(candidate.clone());
        }
    }
    Ok(out)
}

/// The engine-free snapshot a resolved question's world view materialises into
/// (docs/design/query.md §2.3), or the refusal: an inconsistent program has no world
/// view — `inconsistent` is the refusal's message — and an inconclusive search, or
/// one that witnessed models without closing the space, carries why it stopped.
fn materialised(determination: Determination<'_>, inconsistent: &str) -> Result<Snapshot, Fault> {
    match determination {
        Determination::Consistent(models) => WorldView::of(models).materialize(),
        Determination::Inconsistent(_) => Err(Fault::request(inconsistent)),
        Determination::Inconclusive(partial) => Err(partial.into()),
    }
}

/// The query readings an [`Agent`] answers over its own knowledge base (docs/design/
/// query.md §2.2, §2.5, §2.6; solve.md §6.2) — the reading half of the agent's
/// surface, held in the query tier so the solve tier need not depend on it. An
/// extension trait, implemented for every `Agent<B: Backend>` and brought into scope
/// with the reading vocabulary, so `agent.answer(q)?` reads as an inherent method.
///
/// Each reading takes `&mut self` and returns **owned** values — no handle is held
/// across the call — so the readings compose freely, the surface shaped around the
/// question asked rather than the engine's control-flow (solve.md §6.2). Each solves
/// ONCE and materialises the world view; to ask
/// many questions of one search, take a [`snapshot`](AgentReading::snapshot) and read
/// it (its reads are infallible and re-solve nothing). Under a scenario, every reading
/// has its scoped form through [`snapshot_assuming`](AgentReading::snapshot_assuming),
/// whose snapshot ranges over the scenario's models. A reading over a program with
/// no decided world view — inconsistent, or a search that did not close — **refuses
/// with a [`Fault`]** rather than inventing an answer, mirroring the agent's own
/// consequence door (solve.md §6.2): the reading needs a decided program and says so.
pub trait AgentReading {
    /// Solve once and materialise the consistent world view into an engine-free
    /// [`Snapshot`] (docs/design/query.md §2.3): the cache the other readings are
    /// each a shorthand for, exposed so many questions cost one solve. **Refuses** a
    /// program with no answer set, or a search that did not close — there is no world
    /// view to snapshot — carrying why at the [`Fault`]'s locus.
    fn snapshot(&mut self) -> Result<Snapshot, Fault>;

    /// A scenario-scoped [`Snapshot`] (docs/design/query.md §2.2, §2.7): solve once
    /// under `scenario` — the agent's `solve_assuming` — and materialise, so every
    /// reading off it ranges over the scenario's models: the scoped form of every
    /// reading, as `solve_assuming` is of `solve`. **Refuses** over a backend that
    /// does not declare `assumptions` — a scenario needs `solve_assuming` —
    /// with [`Fault::unsupported`] at the request locus, and otherwise as
    /// [`snapshot`](AgentReading::snapshot) does: no answer set under the scenario,
    /// or a search that did not close. Cost: one `solve_assuming`, then `Θ(|W|)` to
    /// materialise.
    fn snapshot_assuming(&mut self, scenario: &Scenario) -> Result<Snapshot, Fault>;

    /// The three-valued [`Answer`] to a ground `query` over the agent's world view
    /// (docs/design/query.md §2.2): solve, materialise, and read. Refuses as
    /// [`snapshot`](AgentReading::snapshot) does.
    fn answer(&mut self, query: &Query) -> Result<Answer, Fault>;

    /// The ASP-Core-2 cautious, two-valued reading of `query` (docs/design/query.md
    /// §2.6): `true` exactly when the query is [`Answer::Yes`]. Refuses as
    /// [`snapshot`](AgentReading::snapshot) does.
    fn entails(&mut self, query: &Query) -> Result<bool, Fault>;

    /// The three-valued [`Bindings`] of an open `pat` over the agent's world view
    /// (docs/design/query.md §2.5). Refuses as [`snapshot`](AgentReading::snapshot)
    /// does, and additionally when `pat` is not a binding pattern (an anonymous
    /// position or a non-pattern) — surfaced as a request [`Fault`], the locus a
    /// non-pattern asked for lives at (§2.5).
    fn bindings(&mut self, pat: &Atom) -> Result<Bindings, Fault>;
}

impl<B: Backend> AgentReading for Agent<B> {
    fn snapshot(&mut self) -> Result<Snapshot, Fault> {
        materialised(
            self.determination()?,
            "no world view: the program has no answer set",
        )
    }

    fn snapshot_assuming(&mut self, scenario: &Scenario) -> Result<Snapshot, Fault> {
        // Under a scenario the program may well have answer sets — just none the
        // scenario admits — so the refusal says which.
        materialised(
            self.solve_assuming(scenario)?.into_determination(),
            "no world view: the program has no answer set under the scenario",
        )
    }

    fn answer(&mut self, query: &Query) -> Result<Answer, Fault> {
        Ok(self.snapshot()?.answer(query))
    }

    fn entails(&mut self, query: &Query) -> Result<bool, Fault> {
        Ok(self.snapshot()?.entails(query))
    }

    fn bindings(&mut self, pat: &Atom) -> Result<Bindings, Fault> {
        self.snapshot()?.bindings(pat).map_err(|refusal| {
            // A `Fault` carries text only, and the refusal's `Display` names the
            // category (per Rust convention) while its `source()` carries the
            // program tier's specific reason — which term does not denote. Fold that
            // reason into the message so the facade caller does not lose it.
            let mut message = refusal.to_string();
            if let Some(source) = std::error::Error::source(&refusal) {
                message.push_str(": ");
                message.push_str(&source.to_string());
            }
            Fault::request(message)
        })
    }
}

#[cfg(test)]
mod snapshot {
    use super::*;
    use themelios_program::symbol::{Name, Sign};

    /// The ground constant `name`.
    fn atom(name: &str) -> Symbol {
        Symbol::function(
            Name::new(name).expect("a valid identifier"),
            [],
            Sign::Positive,
        )
    }

    /// The answer set holding exactly the named constants.
    fn answer_set<'a>(names: impl IntoIterator<Item = &'a str>) -> AnswerSet {
        names.into_iter().map(atom).collect()
    }

    #[test]
    fn a_snapshot_is_exhausted_by_construction() {
        let snapshot = Snapshot::of(vec![answer_set(["a"])], Scenario::default());
        assert!(
            snapshot.is_exhausted(),
            "a snapshot is materialised only from a closed search",
        );
    }

    #[test]
    fn a_snapshot_streams_its_owned_members() {
        let snapshot = Snapshot::of(
            vec![answer_set(["a"]), answer_set(["b"])],
            Scenario::default(),
        );
        let members: Vec<_> = snapshot.members().cloned().collect();
        assert_eq!(members, vec![answer_set(["a"]), answer_set(["b"])]);
    }

    #[test]
    fn a_snapshot_reads_cautious_as_the_intersection() {
        let snapshot = Snapshot::of(
            vec![answer_set(["a", "b"]), answer_set(["a", "c"])],
            Scenario::default(),
        );
        let cautious: Vec<_> = snapshot.cautious().symbols().cloned().collect();
        assert_eq!(cautious, vec![atom("a")], "the intersection holds a alone");
    }

    #[test]
    fn a_snapshot_reads_brave_as_the_union() {
        let snapshot = Snapshot::of(
            vec![answer_set(["a", "b"]), answer_set(["a", "c"])],
            Scenario::default(),
        );
        let brave: Vec<_> = snapshot.brave().symbols().cloned().collect();
        assert_eq!(
            brave,
            vec![atom("a"), atom("b"), atom("c")],
            "the union holds every atom",
        );
    }

    #[test]
    fn a_cautious_reading_carries_the_cautious_mode() {
        let snapshot = Snapshot::of(vec![answer_set(["a"])], Scenario::default());
        assert_eq!(snapshot.cautious().mode(), Mode::Cautious);
    }

    #[test]
    fn a_brave_reading_carries_the_brave_mode() {
        let snapshot = Snapshot::of(vec![answer_set(["a"])], Scenario::default());
        assert_eq!(snapshot.brave().mode(), Mode::Brave);
    }

    #[test]
    fn a_snapshot_carries_the_scenario_it_ranges_over() {
        // The empty (unscoped) scenario round-trips — its assumptions are none.
        let snapshot = Snapshot::of(vec![answer_set(["a"])], Scenario::default());
        assert_eq!(snapshot.scenario().assumptions().count(), 0);
    }
}

#[cfg(test)]
mod epistemic {
    use super::*;
    use proptest::prelude::*;
    use themelios_program::symbol::{Name, Sign};

    /// The ground constant symbol `name`, signed.
    fn symbol(name: &str, sign: Sign) -> Symbol {
        Symbol::function(Name::new(name).expect("a valid identifier"), [], sign)
    }

    /// The answer set holding exactly the given symbols.
    fn answer_set(symbols: impl IntoIterator<Item = Symbol>) -> AnswerSet {
        symbols.into_iter().collect()
    }

    /// A snapshot over the given members, ranging over the unscoped program.
    fn snapshot(members: impl IntoIterator<Item = AnswerSet>) -> Snapshot {
        Snapshot::of(members.into_iter().collect(), Scenario::default())
    }

    /// The positive ground literal query `name`.
    fn lit(name: &str) -> Query {
        Query::of(Atom {
            sign: Sign::Positive,
            name: Name::new(name).expect("a valid identifier"),
            arguments: Arguments::Single(vec![]),
        })
        .expect("a ground literal is a query")
    }

    #[test]
    fn absence_is_not_falsity() {
        // W = { {a}, {b} }: a ∧ b is Unknown — in {a}, b is merely absent (its
        // contrary is not present), so the conjunction is unknown there, not false.
        let world = snapshot([
            answer_set([symbol("a", Sign::Positive)]),
            answer_set([symbol("b", Sign::Positive)]),
        ]);
        assert_eq!(
            world.answer(&Query::all([lit("a"), lit("b")])),
            Answer::Unknown,
        );
    }

    #[test]
    fn a_conjunction_refuted_in_every_member_is_no() {
        // W = { {sunny, warm, -swim}, {swim, -warm} }: warm ∧ swim is No — the first
        // member refutes swim (-swim present), the second refutes warm (-warm present).
        let world = snapshot([
            answer_set([
                symbol("sunny", Sign::Positive),
                symbol("warm", Sign::Positive),
                symbol("swim", Sign::Negative),
            ]),
            answer_set([
                symbol("swim", Sign::Positive),
                symbol("warm", Sign::Negative),
            ]),
        ]);
        assert_eq!(
            world.answer(&Query::all([lit("warm"), lit("swim")])),
            Answer::No,
        );
    }

    #[test]
    fn a_disjunction_true_in_every_member_is_yes() {
        // W = { {a}, {b} }: a ∨ b is Yes — true in the first through a, in the second
        // through b, though no single disjunct is cautiously entailed (the errata).
        let world = snapshot([
            answer_set([symbol("a", Sign::Positive)]),
            answer_set([symbol("b", Sign::Positive)]),
        ]);
        assert_eq!(world.answer(&Query::any([lit("a"), lit("b")])), Answer::Yes,);
    }

    #[test]
    fn a_literal_in_every_member_is_yes() {
        let world = snapshot([answer_set([symbol("a", Sign::Positive)])]);
        assert_eq!(world.answer(&lit("a")), Answer::Yes);
    }

    #[test]
    fn a_literal_whose_contrary_is_present_is_no() {
        let world = snapshot([answer_set([symbol("a", Sign::Negative)])]); // { -a }
        assert_eq!(world.answer(&lit("a")), Answer::No);
    }

    #[test]
    fn a_literal_neither_present_nor_refuted_is_unknown() {
        let world = snapshot([answer_set([symbol("a", Sign::Positive)])]); // { a }
        assert_eq!(world.answer(&lit("b")), Answer::Unknown);
    }

    #[test]
    fn entails_is_yes_against_no_or_unknown() {
        // The ASP-Core-2 projection: `Yes` → true; both `No` and `Unknown` → false,
        // never collapsing Unknown to the wrong side (query.md §2.6).
        let entailed = snapshot([answer_set([symbol("a", Sign::Positive)])]);
        let refuted = snapshot([answer_set([symbol("a", Sign::Negative)])]);
        let open = snapshot([answer_set([symbol("a", Sign::Positive)])]);
        assert!(entailed.entails(&lit("a")), "Yes projects to true");
        assert!(!refuted.entails(&lit("a")), "No projects to false");
        assert!(!open.entails(&lit("b")), "Unknown projects to false");
    }

    proptest! {
        /// A literal and its contrary are never both `Yes` over a consistent world
        /// view: a member holds at most one sign of an atom, so if `a` is in every
        /// member `-a` is in none — the two cautious entailments are mutually
        /// exclusive, whatever the members are.
        #[test]
        fn a_literal_and_its_contrary_are_never_both_yes(
            raw in prop::collection::vec(
                prop::collection::vec((0usize..3usize, any::<bool>()), 0..4),
                1..4usize,
            ),
        ) {
            let names = ["a", "b", "c"];
            // Consistent members: within a member each atom takes at most one sign
            // (last write wins), so no member holds both `a` and `-a`.
            let members = raw.iter().map(|pairs| {
                let signs: std::collections::BTreeMap<usize, bool> =
                    pairs.iter().copied().collect();
                answer_set(signs.into_iter().map(|(atom, positive)| {
                    symbol(names[atom], if positive { Sign::Positive } else { Sign::Negative })
                }))
            });
            let world = snapshot(members);
            let signed = |name: &str, sign| {
                Query::of(Atom {
                    sign,
                    name: Name::new(name).expect("a valid identifier"),
                    arguments: Arguments::Single(vec![]),
                })
                .expect("a ground literal is a query")
            };
            for name in names {
                let both_yes = world.answer(&signed(name, Sign::Positive)) == Answer::Yes
                    && world.answer(&signed(name, Sign::Negative)) == Answer::Yes;
                prop_assert!(!both_yes, "{name} and its contrary are both Yes");
            }
        }
    }
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

#[cfg(test)]
mod bindings {
    use super::*;
    use proptest::prelude::*;
    use themelios_program::symbol::{Name, Sign, VarName};

    /// The 0-ary positive constant `name` — an argument value.
    fn constant(name: &str) -> Symbol {
        Symbol::function(
            Name::new(name).expect("a valid identifier"),
            [],
            Sign::Positive,
        )
    }

    /// The signed unary ground atom `pred(arg)` — a member's element and a matched
    /// instance of a `pred/1` pattern.
    fn atom_symbol(pred: &str, arg: &str, sign: Sign) -> Symbol {
        Symbol::function(
            Name::new(pred).expect("a valid identifier"),
            [constant(arg)],
            sign,
        )
    }

    /// The answer set holding exactly the given symbols.
    fn member(symbols: impl IntoIterator<Item = Symbol>) -> AnswerSet {
        symbols.into_iter().collect()
    }

    /// The snapshot over the given members, ranging over the unscoped program.
    fn snapshot(members: impl IntoIterator<Item = AnswerSet>) -> Snapshot {
        Snapshot::of(members.into_iter().collect(), Scenario::default())
    }

    /// The open pattern `sign pred(X)` with one named variable and the given sign.
    fn signed_var_pattern(pred: &str, variable: &str, sign: Sign) -> Atom {
        Atom {
            sign,
            name: Name::new(pred).expect("a valid identifier"),
            arguments: Arguments::Single(vec![Term::variable(
                VarName::new(variable).expect("a valid variable name"),
            )]),
        }
    }

    /// The open positive pattern `pred(X)` — the pattern `bindings` partitions.
    fn var_pattern(pred: &str, variable: &str) -> Atom {
        signed_var_pattern(pred, variable, Sign::Positive)
    }

    /// The pattern `p(t)` over one argument term, for the refusal cases.
    fn pattern(term: Term) -> Atom {
        Atom {
            sign: Sign::Positive,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![term]),
        }
    }

    /// W = { {p(a), p(b), -p(c)}, {p(a), -p(c)} } — one world exercising all three
    /// cells under the pattern `p(X)`: `p(a)` is cautiously present (yes), `-p(c)` is
    /// cautiously present so `p(c)` is refuted (no), and `p(b)` is in some member only
    /// (unknown).
    fn three_cell_world() -> Snapshot {
        snapshot([
            member([
                atom_symbol("p", "a", Sign::Positive),
                atom_symbol("p", "b", Sign::Positive),
                atom_symbol("p", "c", Sign::Negative),
            ]),
            member([
                atom_symbol("p", "a", Sign::Positive),
                atom_symbol("p", "c", Sign::Negative),
            ]),
        ])
    }

    #[test]
    fn an_anonymous_position_is_refused() {
        // p(X, _): `_` is a well-formed pattern to the mgu but names no binding to
        // partition, so bindings refuses it here, not as a program-tier non-pattern.
        let pattern = Atom {
            sign: Sign::Positive,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![
                Term::variable(VarName::new("X").expect("a valid variable name")),
                Term::anonymous(),
            ]),
        };
        assert!(matches!(
            three_cell_world().bindings(&pattern),
            Err(NotABindingPattern::AnonymousPosition),
        ));
    }

    #[test]
    fn a_nested_anonymous_position_is_refused() {
        // The scan is deep: an anonymous variable inside a compound argument, p(f(_)),
        // is refused too — a `_` anywhere in the pattern names no binding.
        let nested = Term::function(
            Name::new("f").expect("a valid identifier"),
            [Term::anonymous()],
        );
        assert!(matches!(
            three_cell_world().bindings(&pattern(nested)),
            Err(NotABindingPattern::AnonymousPosition),
        ));
    }

    #[test]
    fn a_non_pattern_is_refused_as_a_binding_refusal() {
        // p(1..3): an interval names a set, not a pattern (query.md §3.1). The program
        // tier's refusal is carried, wrapped as this tier's NotAPattern arm.
        let interval = Term::Interval {
            lower: Box::new(Term::Symbolic(Symbol::number(1))),
            upper: Box::new(Term::Symbolic(Symbol::number(3))),
        };
        assert!(matches!(
            three_cell_world().bindings(&pattern(interval)),
            Err(NotABindingPattern::NotAPattern(_)),
        ));
    }

    #[test]
    fn the_anonymous_refusal_names_the_anonymous_position() {
        assert!(
            NotABindingPattern::AnonymousPosition
                .to_string()
                .contains("anonymous"),
            "the refusal's human reading names the anonymous position",
        );
    }

    #[test]
    fn a_non_pattern_binding_refusal_reads_as_a_non_pattern() {
        assert!(
            NotABindingPattern::NotAPattern(NotAPattern::Pooled)
                .to_string()
                .contains("not a pattern"),
            "the non-pattern refusal's human reading names it a non-pattern",
        );
    }

    #[test]
    fn a_non_pattern_refusal_carries_the_program_tier_refusal_as_its_source() {
        use std::error::Error;
        let refusal = NotABindingPattern::NotAPattern(NotAPattern::Pooled);
        assert!(
            refusal.source().is_some(),
            "the program tier's own refusal is this refusal's source",
        );
    }

    #[test]
    fn the_anonymous_refusal_has_no_deeper_source() {
        use std::error::Error;
        assert!(
            NotABindingPattern::AnonymousPosition.source().is_none(),
            "the anonymous refusal is this tier's own, with nothing beneath it",
        );
    }

    #[test]
    fn yes_holds_the_cautiously_entailed_instances() {
        let bindings = three_cell_world()
            .bindings(&var_pattern("p", "X"))
            .expect("a binding pattern");
        let yes: BTreeSet<Symbol> = bindings.yes().cloned().collect();
        assert_eq!(
            yes,
            member([atom_symbol("p", "a", Sign::Positive)]),
            "p(a) is present in every member",
        );
    }

    #[test]
    fn no_holds_the_instances_whose_contrary_is_cautiously_entailed() {
        let bindings = three_cell_world()
            .bindings(&var_pattern("p", "X"))
            .expect("a binding pattern");
        let no: BTreeSet<Symbol> = bindings.no().cloned().collect();
        assert_eq!(
            no,
            member([atom_symbol("p", "c", Sign::Positive)]),
            "-p(c) is cautiously entailed, so p(c) is a No — a positive pattern reports positive",
        );
    }

    #[test]
    fn unknown_holds_the_brave_domain_less_the_settled() {
        let bindings = three_cell_world()
            .bindings(&var_pattern("p", "X"))
            .expect("a binding pattern");
        let unknown: BTreeSet<Symbol> = bindings.unknown().cloned().collect();
        assert_eq!(
            unknown,
            member([atom_symbol("p", "b", Sign::Positive)]),
            "p(b) is in some member but settled by none, so it is unknown",
        );
    }

    #[test]
    fn a_variable_pattern_over_a_growing_world_partitions_by_presence() {
        // W = { {p(a)}, {p(a), p(b)} }: p(a) in every member (yes), p(b) in some
        // (unknown), and no member refutes a p-instance (no is empty) — the classic
        // world that motivates the three-valued reading (query.md §2.5).
        let world = snapshot([
            member([atom_symbol("p", "a", Sign::Positive)]),
            member([
                atom_symbol("p", "a", Sign::Positive),
                atom_symbol("p", "b", Sign::Positive),
            ]),
        ]);
        let bindings = world
            .bindings(&var_pattern("p", "X"))
            .expect("a binding pattern");
        let yes: BTreeSet<Symbol> = bindings.yes().cloned().collect();
        let no: BTreeSet<Symbol> = bindings.no().cloned().collect();
        let unknown: BTreeSet<Symbol> = bindings.unknown().cloned().collect();
        assert_eq!(yes, member([atom_symbol("p", "a", Sign::Positive)]));
        assert_eq!(unknown, member([atom_symbol("p", "b", Sign::Positive)]));
        assert!(no.is_empty(), "no member refutes a p-instance");
    }

    #[test]
    fn a_negative_pattern_partitions_by_its_own_sign() {
        // W = { {-p(a), p(c)}, {-p(a), p(c)} }: over the negative pattern -p(X),
        // -p(a) is cautiously entailed (yes), and p(c) cautiously entailed makes -p(c)
        // a No — the partition is symmetric in the pattern's sign.
        let world = snapshot([
            member([
                atom_symbol("p", "a", Sign::Negative),
                atom_symbol("p", "c", Sign::Positive),
            ]),
            member([
                atom_symbol("p", "a", Sign::Negative),
                atom_symbol("p", "c", Sign::Positive),
            ]),
        ]);
        let negative = Atom {
            sign: Sign::Negative,
            name: Name::new("p").expect("a valid identifier"),
            arguments: Arguments::Single(vec![Term::variable(
                VarName::new("X").expect("a valid variable name"),
            )]),
        };
        let bindings = world.bindings(&negative).expect("a binding pattern");
        let yes: BTreeSet<Symbol> = bindings.yes().cloned().collect();
        let no: BTreeSet<Symbol> = bindings.no().cloned().collect();
        assert_eq!(
            yes,
            member([atom_symbol("p", "a", Sign::Negative)]),
            "-p(a) is cautiously entailed",
        );
        assert_eq!(
            no,
            member([atom_symbol("p", "c", Sign::Negative)]),
            "p(c) is cautiously entailed, so -p(c) is refuted",
        );
    }

    #[test]
    fn matched_in_returns_the_matched_ground_symbols() {
        // p(X) over { p(a), p(b), q(c) }: the matched instances are the two p-symbols,
        // returned as ground symbols (not substitutions); the block scan skips q(c).
        let set = member([
            atom_symbol("p", "a", Sign::Positive),
            atom_symbol("p", "b", Sign::Positive),
            atom_symbol("q", "c", Sign::Positive),
        ]);
        let matched: BTreeSet<Symbol> = matched_in(&var_pattern("p", "X"), &set)
            .expect("a pattern")
            .into_iter()
            .collect();
        assert_eq!(
            matched,
            member([
                atom_symbol("p", "a", Sign::Positive),
                atom_symbol("p", "b", Sign::Positive),
            ]),
        );
    }

    #[test]
    fn matched_in_refuses_a_non_pattern_whichever_members_the_set_holds() {
        // Like matches_in, matched_in classifies the pattern up front, so a non-pattern
        // refuses set-independently: the identical Err over the empty set, a
        // same-signature member, and an other-signature one — never a quiet empty match.
        let interval = || Term::Interval {
            lower: Box::new(Term::Symbolic(Symbol::number(1))),
            upper: Box::new(Term::Symbolic(Symbol::number(3))),
        };
        let refusal = matched_in(&pattern(interval()), &AnswerSet::new());
        assert!(matches!(refusal, Err(NotAPattern::NonDenoting { .. })));
        let same_signature = member([atom_symbol("p", "a", Sign::Positive)]);
        let other_signature = member([atom_symbol("q", "b", Sign::Positive)]);
        for set in [&same_signature, &other_signature] {
            assert_eq!(
                matched_in(&pattern(interval()), set),
                refusal,
                "the refusal is identical whichever members the set holds",
            );
        }
    }

    proptest! {
        /// The three cells never overlap, whatever the (consistent) world: `yes` are
        /// instances in `⋂`, `no`'s contraries are in `⋂`, `unknown` is the brave
        /// domain less `yes`, and answer-set consistency keeps a symbol and its
        /// contrary out of one member — so the partition is a partition.
        #[test]
        fn the_partition_is_always_pairwise_disjoint(
            raw in prop::collection::vec(
                prop::collection::vec((0usize..3usize, any::<bool>()), 0..4),
                1..4usize,
            ),
        ) {
            let args = ["a", "b", "c"];
            // Consistent members: within a member each argument takes at most one sign
            // (last write wins), so no member holds both p(k) and -p(k).
            let members = raw.iter().map(|pairs| {
                let signs: std::collections::BTreeMap<usize, bool> =
                    pairs.iter().copied().collect();
                member(signs.into_iter().map(|(arg, positive)| {
                    atom_symbol(
                        "p",
                        args[arg],
                        if positive { Sign::Positive } else { Sign::Negative },
                    )
                }))
            });
            let world = snapshot(members);
            let bindings = world
                .bindings(&var_pattern("p", "X"))
                .expect("a binding pattern");
            let yes: BTreeSet<Symbol> = bindings.yes().cloned().collect();
            let no: BTreeSet<Symbol> = bindings.no().cloned().collect();
            let unknown: BTreeSet<Symbol> = bindings.unknown().cloned().collect();
            prop_assert!(yes.is_disjoint(&no), "yes ∩ no must be empty: {yes:?} / {no:?}");
            prop_assert!(yes.is_disjoint(&unknown), "yes ∩ unknown must be empty");
            prop_assert!(no.is_disjoint(&unknown), "no ∩ unknown must be empty");
        }

        /// Each cell agrees with the ground query answer — the property disjointness
        /// cannot see (a cell in the wrong bucket): for every instance `g` of the
        /// pattern's sign, `g ∈ yes ⇔ answer(g) = Yes`, `g ∈ no ⇔ answer(g) = No`, and
        /// `g ∈ unknown ⇔ answer(g) = Unknown ∧ g` is bravely present. Both signs.
        #[test]
        fn each_cell_agrees_with_the_ground_query_answer(
            raw in prop::collection::vec(
                prop::collection::vec((0usize..3usize, any::<bool>()), 0..4),
                1..4usize,
            ),
            negative in any::<bool>(),
        ) {
            let args = ["a", "b", "c"];
            let sign = if negative { Sign::Negative } else { Sign::Positive };
            let members: Vec<AnswerSet> = raw
                .iter()
                .map(|pairs| {
                    let signs: std::collections::BTreeMap<usize, bool> =
                        pairs.iter().copied().collect();
                    member(signs.into_iter().map(|(arg, positive)| {
                        atom_symbol(
                            "p",
                            args[arg],
                            if positive { Sign::Positive } else { Sign::Negative },
                        )
                    }))
                })
                .collect();
            let world = snapshot(members.clone());
            let bindings = world
                .bindings(&signed_var_pattern("p", "X", sign))
                .expect("a binding pattern");
            let yes: BTreeSet<Symbol> = bindings.yes().cloned().collect();
            let no: BTreeSet<Symbol> = bindings.no().cloned().collect();
            let unknown: BTreeSet<Symbol> = bindings.unknown().cloned().collect();
            let brave: BTreeSet<Symbol> = members.iter().flatten().cloned().collect();
            for arg in args {
                let instance = atom_symbol("p", arg, sign);
                let query = Query::of(lift(&instance).expect("a function symbol"))
                    .expect("a ground literal is a query");
                let answer = world.answer(&query);
                prop_assert_eq!(
                    yes.contains(&instance),
                    answer == Answer::Yes,
                    "yes holds exactly the Yes instances ({})",
                    arg,
                );
                prop_assert_eq!(
                    no.contains(&instance),
                    answer == Answer::No,
                    "no holds exactly the No instances ({})",
                    arg,
                );
                prop_assert_eq!(
                    unknown.contains(&instance),
                    answer == Answer::Unknown && brave.contains(&instance),
                    "unknown holds exactly the Unknown, bravely-present instances ({})",
                    arg,
                );
            }
        }
    }
}
