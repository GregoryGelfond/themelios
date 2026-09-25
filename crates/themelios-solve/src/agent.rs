//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

use std::fmt;
use std::time::Duration;

use crate::bridge::Door;
use crate::contract::{
    Backend, ConsequenceRequest, ConsequenceSupport, Fault, Mode, OptimizeRequest, SolveRequest,
    TruthValue,
};
use crate::extend::Facts;
use crate::outcome::{Consequences, Determination, Optimized, Solved};
use themelios_program::program::{Arguments, PartKey};
use themelios_program::{Atom, Program, Rule, Statement, Symbol, Term, WithProvenance};

/// A program made active — the same knowledge seen not as an object of study
/// but as a reasoner one drives (docs/design/solve.md §6.1). An agent is
/// instantiated from a `Program` that becomes its knowledge base, and it owns
/// both that knowledge and the backend that reasons for it: the owned value is
/// the authority to drive the engine, dropping it is revocation, and there is
/// no ambient engine or global mutable state. Because the knowledge is owned
/// rather than borrowed off a stack frame, an agent is `'static` whenever its
/// backend is, and so embeds behind a service boundary without ceremony.
///
/// Reifying a program with the installation's default engine — the `Reason`
/// extension trait on `Program` and its `into_agent` (§6.1) — belongs to the
/// facade crate, since it names the facade's `DefaultEngine`; this crate stays
/// engine-free, so an agent here is built over an explicit backend.
pub struct Agent<B: Backend> {
    /// The engine that reasons for the agent, driven by the reasoning loop
    /// (§6.2).
    backend: B,
    /// The knowledge base: the program the agent was instantiated from,
    /// evolving as the loop asserts and retracts (§6.2).
    knowledge: Program,
    /// The ledger kept beside the knowledge base (§6.2).
    ledger: KnowledgeLedger,
}

impl<B: Backend> Agent<B> {
    /// Instantiate an agent from a program, which becomes its knowledge base,
    /// over the backend that reasons for it (docs/design/solve.md §6.1). The
    /// agent owns both; the knowledge is indexed for later modification. Cost:
    /// `Θ(program size)` — each statement is recorded in the ledger.
    pub fn new(knowledge: Program, backend: B) -> Self {
        let ledger = KnowledgeLedger::of(&knowledge);
        Agent {
            backend,
            knowledge,
            ledger,
        }
    }

    /// The evolving knowledge base — the program at rest behind the agent
    /// (docs/design/solve.md §6.1). Total; O(1).
    pub fn knowledge(&self) -> &Program {
        &self.knowledge
    }

    /// Add a statement to the knowledge base, returning the handle that names it
    /// and its retraction class (docs/design/solve.md §6.2). Assertion is
    /// monotone and clean: it amends the owned knowledge base and discloses how
    /// the statement would retract — `Toggle` at the seam where the backend
    /// guards externals, `Rebuild` (re-grounding) otherwise — readable from the
    /// handle before any retraction is paid for. The engine is brought level
    /// with the amended knowledge at the next ask, not here, so a single
    /// `assert` touches no backend. Two content-equal assertions get distinct
    /// handles — the set-valued program shows the statement once, and it stays
    /// while either handle is unretracted. Cost: `Θ(program size)`.
    pub fn assert(&mut self, statement: impl Into<Statement>) -> Result<StatementId, Fault> {
        let node = WithProvenance::constructed(statement.into());
        let id = self.ledger.push(node, self.retraction_class());
        self.knowledge = self.ledger.rebuild();
        Ok(id)
    }

    /// Retract a statement named by its handle (docs/design/solve.md §6.2):
    /// remove it from the owned knowledge base. The handle is honoured only by
    /// the ledger that issued it; a stale handle — one already retracted — or
    /// one from another agent is a typed refusal at
    /// [`Locus::Request`](crate::contract::Locus::Request), never a silent
    /// no-op. The engine is brought level at the next ask; the retraction class
    /// the handle disclosed governs how cheaply a warm engine realises the
    /// removal there. Cost: `Θ(program size)`.
    pub fn retract(&mut self, statement: StatementId) -> Result<(), Fault> {
        if !self.ledger.is_live(statement) {
            return Err(Fault::request("retract of a statement that is not live"));
        }
        self.ledger.retire(statement);
        self.knowledge = self.ledger.rebuild();
        Ok(())
    }

    /// Record a bulk set of observed ground facts, returning the observation
    /// that names them (docs/design/solve.md §6.2, §7.3). The observation is
    /// all-or-nothing: every fact the source denotes is converted to a base fact
    /// first, so a symbol that is not an atom — a number, string, tuple, `#inf`,
    /// or `#sup` — refuses with the knowledge base untouched, leaving no orphaned
    /// statement. A later step retracts the whole set with
    /// [`forget`](Agent::forget). Cost: `Θ(facts + program size)` — one rebuild,
    /// not one per fact.
    // The fact source is taken by value — the design's surface (§6.2): the caller
    // hands its observations over, though a `Facts` is only read to enumerate them.
    #[allow(clippy::needless_pass_by_value)]
    pub fn observe(&mut self, facts: impl Facts) -> Result<Observation, Fault> {
        let rules = facts
            .facts()
            .map(|symbol| fact_of(&symbol))
            .collect::<Result<Vec<_>, _>>()?;
        if rules.is_empty() {
            return Ok(Observation {
                members: Box::default(),
            });
        }
        let class = self.retraction_class();
        let members: Box<[StatementId]> = rules
            .into_iter()
            .map(|rule| {
                self.ledger
                    .push(WithProvenance::constructed(rule.into()), class)
            })
            .collect();
        self.knowledge = self.ledger.rebuild();
        Ok(Observation { members })
    }

    /// Retract an observation's facts as a unit (docs/design/solve.md §6.2). The
    /// retraction is all-or-nothing: a spent observation — one already forgotten,
    /// or holding any spent handle — refuses at
    /// [`Locus::Request`](crate::contract::Locus::Request) with the knowledge base
    /// untouched, as a stale statement handle does. Cost: `Θ(facts + program
    /// size)` — one rebuild, not one per fact.
    pub fn forget(&mut self, observation: Observation) -> Result<(), Fault> {
        let Observation { members } = observation;
        if members.is_empty() {
            return Ok(());
        }
        if members.iter().any(|&member| !self.ledger.is_live(member)) {
            return Err(Fault::request("forget of a spent observation"));
        }
        for &member in &members {
            self.ledger.retire(member);
        }
        self.knowledge = self.ledger.rebuild();
        Ok(())
    }

    /// Ground the named program parts through the backend (docs/design/solve.md
    /// §6.2) — one of the retained multi-shot mechanisms a caller drives
    /// explicitly, distinct from the monotone `assert`.
    pub fn ground(&mut self, parts: &[themelios_program::program::Part]) -> Result<(), Fault> {
        self.backend
            .ground(parts, &crate::contract::GroundOptions::default())
    }

    /// Assign an external atom a truth value at the seam (docs/design/solve.md
    /// §6.2) — the retained multi-shot mechanism a caller toggles an open truth
    /// with.
    pub fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        self.backend.assign_external(external, value)
    }

    // ---- ask (§6.2, §6.4) ----

    /// Ask the question `solve` of the knowledge base (docs/design/solve.md
    /// §6.2): bring the engine level with the owned knowledge, then the run
    /// handle — stream the answer sets, inspect or resolve the trichotomy, read
    /// the conclusion (§5.2). The handle borrows the agent for its life: the
    /// borrow checker is the "no mutation while reasoning" lock (§6.1), so an
    /// amendment or a second question while it is held does not compile. The
    /// same question the bare `Program` answers with an owned handle (§6.4).
    /// Refusal: an engine or request `Fault` — an inconsistent or inconclusive
    /// knowledge base is a reading of the handle, never a fault (§5.1). Cost:
    /// one lowering, then the engine's, streamed.
    pub fn solve(&mut self) -> Result<Solved<'_>, Fault> {
        self.bring_level()?;
        self.backend.solve(&SolveRequest::default())
    }

    /// The configured pair of [`solve`](Agent::solve) (docs/design/solve.md
    /// §6.3): the same question under the options — the time budget the
    /// question carries, handed to the backend on the request.
    // The options are taken by value — the design's surface (§6.3): the caller
    // hands the configuration over, though only its knobs are read.
    #[allow(clippy::needless_pass_by_value)]
    pub fn solve_with(&mut self, options: SolveOptions) -> Result<Solved<'_>, Fault> {
        self.bring_level()?;
        self.backend.solve(&SolveRequest { time: options.time })
    }

    /// Ask, and resolve the run into the trichotomy (docs/design/solve.md
    /// §6.2, §5.1): consistent, with the models a world view is read from
    /// (query.md §2); inconsistent; or inconclusive, with what the search did
    /// establish. The determination borrows the agent as the run handle does —
    /// the consuming resolver threads the borrow (§5.2). Refusal: an engine or
    /// request `Fault`.
    pub fn determination(&mut self) -> Result<Determination<'_>, Fault> {
        self.bring_level()?;
        Ok(self
            .backend
            .solve(&SolveRequest::default())?
            .into_determination())
    }

    /// Ask for the proven optimum under the request (docs/design/solve.md
    /// §6.2, §5.3): bring the engine level, then the optimization run handle —
    /// a sibling of [`solve`](Agent::solve), not a second vocabulary. Refuses
    /// at the request locus over a backend that does not declare
    /// `optimization` (§4.2).
    pub fn optimize(&mut self, request: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        self.bring_level()?;
        self.backend.optimize(request)
    }

    /// Ask under a scenario (docs/design/solve.md §6.3): each of its
    /// assumptions fixed for the span of this one question and discharged
    /// after it — a hypothesis, as distinct from a retraction (§6.2), which
    /// amends the knowledge base. Refuses at the request locus over a backend
    /// that does not declare `assumptions` (§4.2).
    pub fn solve_assuming(&mut self, scenario: &Scenario) -> Result<Solved<'_>, Fault> {
        self.bring_level()?;
        self.backend
            .solve_assuming(scenario, &SolveRequest::default())
    }

    /// A handle that interrupts an in-flight question from another thread
    /// (docs/design/solve.md §6.1, §6.3) — `Some` exactly when the backend
    /// declares `cancellation`; `None` says the engine cannot be interrupted,
    /// readable before any question is paid for. O(1).
    pub fn interrupt(&self) -> Option<Interrupt> {
        self.backend.interrupt()
    }

    /// The cautious consequences (`⋂`, "what must hold") of the knowledge base
    /// (docs/design/solve.md §5.2; query.md §2.4): the atoms true in EVERY answer
    /// set. Routed on the backend's declared consequence support — the engine's
    /// own door in one solve where it has one, otherwise the core's fold over the
    /// enumerated world view — and ranges over the unscoped program. Refuses over
    /// a program with no answer set (`⋂` over the empty world view is undefined,
    /// not `∅`) and over a search that did not close. Cost: one solve for the
    /// native door, `Θ(|W|)` for the derived.
    pub fn cautious(&mut self) -> Result<Consequences, Fault> {
        self.consequences(Mode::Cautious)
    }

    /// The brave consequences (`⋃`, "what can hold") of the knowledge base
    /// (docs/design/solve.md §5.2; query.md §2.4): the atoms true in SOME answer
    /// set. Routed and gated as the cautious reading is.
    pub fn brave(&mut self) -> Result<Consequences, Fault> {
        self.consequences(Mode::Brave)
    }

    /// The consequences in `mode` — the shared body of the cautious and brave
    /// readings. The native door computes them in one solve; the derived door
    /// folds the enumerated CONSISTENT world view. The consistency gate is
    /// load-bearing: folding zero members returns an empty set for both modes, and
    /// an empty cautious set would say the program forces nothing — of a program
    /// that has no model, where `⋂` is undefined — so it refuses (query.md §2.3).
    /// Both doors range over the unscoped program (the agent holds no
    /// persistent scenario, §6.3), so the native and derived results agree (the
    /// free differential, query.md §2.4). Both range over ALL stable models today;
    /// under an optimization objective they must instead range over the optimal
    /// set (solve.md §5.2), the obligation that joins when optimization lands.
    fn consequences(&mut self, mode: Mode) -> Result<Consequences, Fault> {
        self.bring_level()?;
        match self.backend.capabilities().native_consequences {
            ConsequenceSupport::Native => self
                .backend
                .consequences_native(mode, &ConsequenceRequest::default()),
            ConsequenceSupport::DerivedByEnumeration => {
                match self
                    .backend
                    .solve(&SolveRequest::default())?
                    .into_determination()
                {
                    Determination::Consistent(mut models) => {
                        let members = models.all_members()?;
                        Ok(Consequences::fold(mode, members.iter()))
                    }
                    Determination::Inconsistent(_) => Err(Fault::request(
                        "no consequences: the program has no answer set",
                    )),
                    Determination::Inconclusive(partial) => Err(partial.into()),
                }
            }
        }
    }

    /// Bring the engine level with the owned knowledge base before a question
    /// is put to it (docs/design/solve.md §6.2): lower the knowledge as it
    /// stands, through Door B (§10.2), so the engine reasons over exactly what
    /// the agent holds — the assertions and retractions since the last question
    /// included. A refused lowering is the question's fault, and nothing is
    /// delegated after it. Every question lowers the whole knowledge base; the
    /// retained-engine realisations the retraction classes disclose — the
    /// external toggle, the rebuild as a `reset` then one lowering, the
    /// incremental grounding of an addition — are reserved until the retained
    /// engine is implemented. Cost: one lowering (§10.1).
    fn bring_level(&mut self) -> Result<(), Fault> {
        self.backend.lower(Door::Program(&self.knowledge))
    }

    /// The retraction class an asserted statement is disclosed under
    /// (docs/design/solve.md §6.2): `Toggle` when the backend declares
    /// `externals` — the agent guards an asserted statement with an external so
    /// its retraction is an `O(1)` toggle, the realisation being the agent's to
    /// choose — and `Rebuild` otherwise, where retraction re-grounds the amended
    /// program. O(1).
    fn retraction_class(&self) -> RetractionClass {
        if self.backend.capabilities().externals {
            RetractionClass::Toggle
        } else {
            RetractionClass::Rebuild
        }
    }
}

/// The base fact a ground atom symbol denotes (docs/design/solve.md §6.2, §7.3),
/// or a refusal when the symbol is not an atom. An atom is a function symbol; a
/// number, string, tuple, `#inf`, or `#sup` is not one a program asserts.
fn fact_of(symbol: &Symbol) -> Result<Rule, Fault> {
    match symbol {
        Symbol::Function {
            name,
            arguments,
            sign,
        } => {
            let terms = arguments.iter().cloned().map(Term::from).collect();
            Ok(Rule::fact(Atom {
                sign: *sign,
                name: name.clone(),
                arguments: Arguments::Single(terms),
            }))
        }
        // The symbol is not embedded in the message: a symbol is unbounded in
        // size, and a fault message is not the place to render one.
        _ => Err(Fault::request(
            "an observed fact must be an atom, not a number, string, tuple, #inf, or #sup",
        )),
    }
}

/// How a statement's retraction is realised (docs/design/solve.md §6.2), fixed
/// at assertion and readable from the statement's handle before any retraction
/// is paid for: a `Toggle` clears an external guard at the seam in `O(1)`, a
/// `Rebuild` re-grounds the amended program in `Θ(program size)`. The divergence
/// is disclosed so no caller pays a rebuild believing it a toggle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RetractionClass {
    /// Retracts by clearing an external guard at the seam — `O(1)`.
    Toggle,
    /// Retracts by re-grounding the amended program — `Θ(program size)`.
    Rebuild,
}

/// A handle naming a statement in an agent's knowledge base together with its
/// retraction class (docs/design/solve.md §6.2). The class is readable before
/// the retraction the handle names is paid for. A handle is honoured only by the
/// ledger that issued it, and only until it is spent: retracting a spent handle,
/// or one issued by another agent, is a typed refusal, not a silent no-op.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StatementId {
    ledger: u64,
    slot: usize,
    generation: u64,
    class: RetractionClass,
}

impl StatementId {
    /// The retraction class fixed at assertion — readable before the retraction
    /// is paid for (docs/design/solve.md §6.2). O(1).
    pub fn retraction_class(&self) -> RetractionClass {
        self.class
    }
}

/// A bulk-observed fact set: the statements one [`observe`](Agent::observe)
/// recorded, named as a unit so a later step can [`forget`](Agent::forget) them
/// together (docs/design/solve.md §6.2).
#[derive(Clone, Debug)]
pub struct Observation {
    members: Box<[StatementId]>,
}

impl Observation {
    /// How many facts the observation recorded. O(1).
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether the observation recorded no facts. O(1).
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

/// The brand a [`KnowledgeLedger`] stamps its handles with, so a handle names a
/// statement in the ledger that issued it and nowhere else. Process-unique.
static NEXT_LEDGER_BRAND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The side structure the agent keeps beside its knowledge base (docs/design/
/// solve.md §6.2): a generational slot map over the statements — the part each
/// belongs to and the statement with its provenance. A retraction frees the slot
/// and bumps its generation, so a spent handle reads spent and the slot is reused
/// by the next assertion — the map is bounded by the high-water mark of
/// concurrently live statements, not the count ever asserted. The owned
/// knowledge base is
/// [`rebuild`](KnowledgeLedger::rebuild)ed from the live slots after each
/// mutation.
pub(crate) struct KnowledgeLedger {
    brand: u64,
    base: PartKey,
    slots: Vec<Slot>,
    free: Vec<usize>,
}

/// One slot of a [`KnowledgeLedger`]: its generation, and the statement it holds
/// while live. The generation tells a handle to the statement once here apart
/// from a handle to a statement that later reused the slot.
struct Slot {
    generation: u64,
    entry: Option<LedgerEntry>,
}

/// One statement held in a [`KnowledgeLedger`]: the part it belongs to and the
/// statement with its provenance. The retraction class travels in the
/// [`StatementId`], not here — the ledger stores, the handle discloses.
struct LedgerEntry {
    part: PartKey,
    node: WithProvenance<Statement>,
}

impl KnowledgeLedger {
    /// Seed a ledger from a program's statements, each under the part it belongs
    /// to (docs/design/solve.md §6.2). Seeded statements carry no handle, so they
    /// are the knowledge base's permanent floor; only asserted statements are
    /// retractable. `Θ(program size)` — every statement is cloned into the map.
    pub(crate) fn of(program: &Program) -> KnowledgeLedger {
        let brand = NEXT_LEDGER_BRAND.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = program.base().key().clone();
        let slots = program
            .parts()
            .flat_map(|part| {
                let key = part.key().clone();
                part.statements().map(move |node| Slot {
                    generation: 0,
                    entry: Some(LedgerEntry {
                        part: key.clone(),
                        node: node.clone(),
                    }),
                })
            })
            .collect();
        KnowledgeLedger {
            brand,
            base,
            slots,
            free: Vec::new(),
        }
    }

    /// Rebuild the owned program from the live slots in one pass (docs/design/
    /// solve.md §6.2). The multi-part rebuild door canonicalizes and merges
    /// content-equal statements within a part. `Θ(slots)` — bounded by the
    /// high-water mark of concurrently live statements, since a freed slot is
    /// reused rather than left a permanent hole.
    pub(crate) fn rebuild(&self) -> Program {
        Program::of_keyed_nodes(
            self.slots
                .iter()
                .filter_map(|slot| slot.entry.as_ref())
                .map(|entry| (entry.part.clone(), entry.node.clone())),
        )
    }

    /// Add a statement to the base part, returning its handle carrying the
    /// retraction `class`. Reuses a freed slot when one is available, so the map
    /// does not grow with the count ever asserted. O(1) amortised.
    pub(crate) fn push(
        &mut self,
        node: WithProvenance<Statement>,
        class: RetractionClass,
    ) -> StatementId {
        let entry = LedgerEntry {
            part: self.base.clone(),
            node,
        };
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot].entry = Some(entry);
            slot
        } else {
            self.slots.push(Slot {
                generation: 0,
                entry: Some(entry),
            });
            self.slots.len() - 1
        };
        StatementId {
            ledger: self.brand,
            slot,
            generation: self.slots[slot].generation,
            class,
        }
    }

    /// Whether the handle still names a live statement in this ledger — false for
    /// a spent handle, a reused slot, or a handle from another ledger. O(1).
    pub(crate) fn is_live(&self, id: StatementId) -> bool {
        id.ledger == self.brand
            && matches!(
                self.slots.get(id.slot),
                Some(Slot {
                    generation,
                    entry: Some(_),
                }) if *generation == id.generation
            )
    }

    /// Retire the live statement the handle names: free its slot and bump the
    /// slot's generation so the spent handle reads spent. The caller has
    /// confirmed the handle is live. O(1).
    pub(crate) fn retire(&mut self, id: StatementId) {
        let slot = &mut self.slots[id.slot];
        // The caller confirms liveness before retiring, and an observation's
        // members are distinct — so a slot is never freed twice, which would
        // double-enter the free list and hand two assertions the same handle.
        debug_assert!(slot.entry.is_some(), "retire of an already-freed slot");
        slot.entry = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.slot);
    }

    /// The number of slots the map holds — live plus freed-and-not-yet-reused.
    /// Bounded by the high-water mark of concurrently live statements.
    #[cfg(test)]
    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

// ---- Assumptions and scenarios (§6.3) ----

/// A single assumption (docs/design/solve.md §6.3): one program atom fixed
/// true or false for the span of one question — a hypothesis, discharged after
/// it, as distinct from a retraction (§6.2), which amends the knowledge base
/// and persists. The atom is a function symbol, a predicate or constant under
/// its strong sign; [`Assumption::new`] refuses every other symbol, so an
/// assumption over a number, a string, a tuple, `#inf`, or `#sup` — none of
/// which a program asserts — cannot be constructed. The raw set of assumptions
/// is what the literature names, what `solve_assuming` scopes by, and what
/// blame reports (§5.4). Owned plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Assumption {
    atom: Symbol,
    holds: bool,
}

impl Assumption {
    /// Fix `atom` to hold (`true`) or not to hold (`false`) for one question.
    /// Refuses a symbol that is not an atom with [`NotAnAtom`], handing the
    /// symbol back rather than dropping it: an atom is a function symbol, the
    /// one variant a program asserts. O(1).
    pub fn new(atom: Symbol, holds: bool) -> Result<Assumption, NotAnAtom> {
        if matches!(atom, Symbol::Function { .. }) {
            Ok(Assumption { atom, holds })
        } else {
            Err(NotAnAtom { symbol: atom })
        }
    }

    /// The atom fixed. O(1).
    pub fn atom(&self) -> &Symbol {
        &self.atom
    }

    /// Whether the atom is fixed to hold (`true`) or not to hold (`false`).
    /// O(1).
    pub fn holds(&self) -> bool {
        self.holds
    }
}

/// A reusable, named assumption configuration (docs/design/solve.md §6.3) — a
/// concept this library introduces, so this library names it (§1.4): the
/// literature's "assumptions" names the raw set, not a bundle bound to an
/// identity and re-applied across solves. Collected from assumptions
/// (`FromIterator`) and read back exactly as given, in order; `solve_assuming`
/// takes one, and blame (§5.4) reports the responsible raw subset. The empty
/// scenario — no atom fixed — is the unscoped program, and is `Default`.
/// Owned plain data.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Scenario {
    assumptions: Vec<Assumption>,
}

impl Scenario {
    /// The assumptions, each as given, in the order collected. Borrowed:
    /// reading does not spend the scenario. O(n) over the whole stream.
    pub fn assumptions(&self) -> impl Iterator<Item = &Assumption> + '_ {
        self.assumptions.iter()
    }
}

impl FromIterator<Assumption> for Scenario {
    /// Bundle the assumptions, each kept as given. O(n).
    fn from_iter<I: IntoIterator<Item = Assumption>>(assumptions: I) -> Scenario {
        Scenario {
            assumptions: assumptions.into_iter().collect(),
        }
    }
}

/// Ergonomic construction of an assumption from a value authored the §3.1 way
/// (docs/design/solve.md §6.3) — the one door the scenario macro (§3.2)
/// expands through, so there is one grammar and one representation. Refuses a
/// value that is not an assumption with [`NotAnAssumption`]; an assumption
/// converts into itself.
pub trait IntoAssumption {
    /// The assumption this value authors, or the refusal.
    fn into_assumption(self) -> Result<Assumption, NotAnAssumption>;
}

impl IntoAssumption for Assumption {
    /// An assumption is already one. Total; O(1).
    fn into_assumption(self) -> Result<Assumption, NotAnAssumption> {
        Ok(self)
    }
}

/// The refusal [`Assumption::new`] issues (docs/design/solve.md §6.3): the
/// symbol is not an atom — a number, a string, a tuple, `#inf`, or `#sup`,
/// none of which a program asserts. Carries the refused symbol, handed back
/// rather than dropped.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnAtom {
    /// The symbol that is not an atom.
    pub(crate) symbol: Symbol,
}

impl NotAnAtom {
    /// The refused symbol. O(1).
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }
}

impl fmt::Display for NotAnAtom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not an atom: the symbol {:?} is not a predicate or constant",
            self.symbol
        )
    }
}

impl std::error::Error for NotAnAtom {}

/// The refusal an [`IntoAssumption`] conversion issues (docs/design/solve.md
/// §6.3): the value authored is not an assumption. Non-exhaustive: what was
/// refused joins the refusal as the fallible conversions are realised; until
/// then it is declared, with its rendering, so the trait's signature is shaped
/// by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnAssumption {}

impl fmt::Display for NotAnAssumption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not an assumption")
    }
}

impl std::error::Error for NotAnAssumption {}

// ---- Per-question options and cancellation (§6.3) ----

/// The options the configured question [`solve_with`](Agent::solve_with)
/// carries (docs/design/solve.md §6.3): surfaced at the operation they affect
/// and nowhere else. The empty options — the pristine question `solve` asks —
/// are `Default`; non-exhaustive, so a new knob is a new field, not a breaking
/// change. Owned plain data.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SolveOptions {
    /// The time budget the question carries (§6.3), handed to the backend on
    /// the request: enforcement is a declared capability, and a hit budget
    /// resolves as [`Conclusion::Budget`](crate::outcome::Conclusion::Budget),
    /// never as a clean end.
    pub time: Option<Duration>,
}

/// A handle that interrupts an in-flight solve from another thread. Reserved;
/// its surface is defined with §6.1 and §6.3.
pub struct Interrupt;

#[cfg(test)]
mod tests {
    use super::*;

    // The conversion refusal is built here, in the defining crate: a
    // non-exhaustive struct is not built by a struct expression elsewhere.

    #[test]
    fn a_conversion_refusal_explains_itself() {
        let refused = NotAnAssumption {};
        assert!(
            format!("{refused}").contains("not an assumption"),
            "{refused}"
        );
    }

    #[test]
    fn a_cloned_conversion_refusal_equals_its_original() {
        let refused = NotAnAssumption {};
        assert_eq!(refused.clone(), refused);
    }
}

#[cfg(test)]
mod ledger_laws {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    /// The rebuild-scaling probe size: large enough that a quadratic rebuild
    /// stands clear of timing noise, small enough to stay a fast unit test.
    const PROBE_ENTRIES: usize = 4_000;
    /// Doubling the entries at most triples the time — a linear rebuild roughly
    /// doubles (~2×), a quadratic one quadruples (~4×), so the tripwire trips
    /// between them.
    const LINEAR_CEILING: u128 = 3;
    /// The rebuild is timed several times and the fastest kept, so a scheduling
    /// hiccup in one run does not read as super-linear growth.
    const SAMPLES: usize = 9;

    /// A ledger seeded with `n` distinct base facts `c0.` … `c{n-1}.`.
    fn ledger_of_size(n: usize) -> KnowledgeLedger {
        let facts = (0..n).map(|i| {
            themelios_program::Rule::fact(themelios_program::Atom::constant(
                themelios_program::Name::new(format!("c{i}")).expect("a valid identifier"),
            ))
        });
        KnowledgeLedger::of(&Program::of(facts))
    }

    /// The fastest of several rebuilds of `ledger`, in nanoseconds.
    fn fastest_rebuild_nanos(ledger: &KnowledgeLedger) -> u128 {
        (0..SAMPLES)
            .map(|_| {
                let start = Instant::now();
                black_box(ledger.rebuild());
                start.elapsed().as_nanos()
            })
            .min()
            .expect("at least one sample")
    }

    #[test]
    fn the_rebuild_scales_linearly_with_the_program_size() {
        let single = fastest_rebuild_nanos(&ledger_of_size(PROBE_ENTRIES));
        let double = fastest_rebuild_nanos(&ledger_of_size(PROBE_ENTRIES * 2));
        assert!(
            double < single.saturating_mul(LINEAR_CEILING),
            "rebuild grew worse than linearly — {single}ns at {PROBE_ENTRIES} entries, \
             {double}ns at {} entries — the reinsertion recurrence to avoid",
            PROBE_ENTRIES * 2,
        );
    }

    /// The number of assert/retract cycles a long-running loop stands in for.
    const CHURN_CYCLES: usize = 10_000;

    #[test]
    fn a_long_churn_reuses_freed_slots_rather_than_growing() {
        let mut ledger = KnowledgeLedger::of(&Program::empty());
        let seeded = ledger.slot_count();
        for i in 0..CHURN_CYCLES {
            let node = WithProvenance::constructed(themelios_program::Statement::from(
                themelios_program::Rule::fact(themelios_program::Atom::constant(
                    themelios_program::Name::new(format!("c{i}")).expect("a valid identifier"),
                )),
            ));
            let id = ledger.push(node, RetractionClass::Rebuild);
            ledger.retire(id);
        }
        // Each cycle frees the slot it took, so the map never grows past the
        // seed plus one transient slot — never the count ever asserted.
        assert!(
            ledger.slot_count() <= seeded + 1,
            "the slot map grew with the count ever asserted: {} slots after {CHURN_CYCLES} cycles",
            ledger.slot_count(),
        );
    }

    #[test]
    fn a_rebuild_preserves_a_statements_provenance() {
        let doc = "an observed fact";
        let documented = WithProvenance::constructed_with_doc(
            themelios_program::Statement::from(themelios_program::Rule::fact(
                themelios_program::Atom::constant(
                    themelios_program::Name::new("a").expect("a valid identifier"),
                ),
            )),
            doc,
        );
        let mut ledger = KnowledgeLedger::of(&Program::empty());
        ledger.push(documented, RetractionClass::Rebuild);
        let rebuilt = ledger.rebuild();
        let statement = rebuilt.statements().next().expect("one statement");
        // Program equality erases provenance, so pin it by inspection: the doc
        // annotation must survive the rebuild, not just the statement's content.
        assert!(
            format!("{:?}", statement.provenance()).contains(doc),
            "the rebuild lost the provenance: {:?}",
            statement.provenance()
        );
    }
}

#[cfg(test)]
mod ask_laws {
    use super::*;
    use std::collections::BTreeSet;

    use crate::bridge::GroundProgram;
    use crate::contract::Capabilities;
    use crate::outcome::{AnswerSet, Conclusion, Run};
    use themelios_program::Name;

    // A question resolves over a run, and a run is built here, in the defining
    // crate: the laws below drive the agent's questions through a backend that
    // answers with a scripted search, so the resolution register is exercised
    // end to end — a recording backend outside the crate can only refuse.

    /// A scripted search: a fixed answer-set sequence, then a closed space.
    struct Scripted {
        sets: std::vec::IntoIter<AnswerSet>,
        ended: bool,
    }

    impl Run for Scripted {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            let next = self.sets.next().map(Ok);
            if next.is_none() {
                self.ended = true;
            }
            next
        }

        fn conclusion(&self) -> Option<Conclusion> {
            self.ended.then_some(Conclusion::Exhausted)
        }
    }

    /// A solved handle over a scripted search of `sets`, ranging over
    /// `scenario`.
    fn solved_over(sets: Vec<AnswerSet>, scenario: Scenario) -> Solved<'static> {
        Solved::over(
            Box::new(Scripted {
                sets: sets.into_iter(),
                ended: false,
            }),
            scenario,
        )
    }

    /// A backend that answers every question with the answer sets it was given
    /// — an engine-free stand-in that lets the agent's questions resolve.
    struct Answering {
        sets: Vec<AnswerSet>,
    }

    impl Backend for Answering {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                assumptions: true,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(self.sets.clone(), Scenario::default()))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            None
        }

        fn solve_assuming(
            &mut self,
            scenario: &Scenario,
            _request: &SolveRequest,
        ) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(self.sets.clone(), scenario.clone()))
        }
    }

    fn constant(name: &str) -> Symbol {
        Symbol::constant(Name::new(name).expect("a valid identifier"))
    }

    /// An answer set of the named constants.
    fn answer_set(names: &[&str]) -> AnswerSet {
        names.iter().copied().map(constant).collect()
    }

    /// An agent over an empty knowledge base and a backend answering `sets`.
    fn agent_answering(sets: Vec<AnswerSet>) -> Agent<Answering> {
        Agent::new(Program::empty(), Answering { sets })
    }

    /// The intersection of the answer sets — computed directly, not through the
    /// core fold, so a native door built on it can be held against the fold.
    fn intersect(sets: &[AnswerSet]) -> BTreeSet<Symbol> {
        let mut sets = sets.iter();
        let mut common = sets.next().cloned().unwrap_or_default();
        for set in sets {
            common = common.intersection(set).cloned().collect();
        }
        common
    }

    /// The union of the answer sets — the direct twin of `intersect`.
    fn union(sets: &[AnswerSet]) -> BTreeSet<Symbol> {
        sets.iter().flat_map(|set| set.iter().cloned()).collect()
    }

    /// A backend that answers with its answer sets AND declares a native
    /// consequence door: its own `⋂`/`⋃` over those sets, computed independently
    /// of the core fold, and refusing an inconsistent program as the derived door
    /// does — so the two doors can be held against each other.
    struct Dual {
        sets: Vec<AnswerSet>,
    }

    impl Backend for Dual {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                native_consequences: ConsequenceSupport::Native,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(self.sets.clone(), Scenario::default()))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            None
        }

        fn consequences_native(
            &mut self,
            mode: Mode,
            _request: &ConsequenceRequest,
        ) -> Result<Consequences, Fault> {
            if self.sets.is_empty() {
                return Err(Fault::request(
                    "no consequences: the program has no answer set",
                ));
            }
            let symbols = match mode {
                Mode::Cautious => intersect(&self.sets),
                Mode::Brave => union(&self.sets),
            };
            Ok(Consequences { symbols, mode })
        }
    }

    #[test]
    fn the_native_door_and_the_derived_fold_agree() {
        // The free differential (query.md §2.4): a backend's own ⋂/⋃ door and the
        // core's fold over the SAME backend's enumerated models give the same
        // answer. A shared atom in only two of the three models, so a truncating
        // fold (stopping short) would be caught for both modes: the full cautious
        // ⋂ is {a}, a two-model fold keeps e; the full brave ⋃ has d, a two-model
        // fold drops it.
        let sets = vec![
            answer_set(&["a", "b", "e"]),
            answer_set(&["a", "c", "e"]),
            answer_set(&["a", "d"]),
        ];
        let mut agent = Agent::new(Program::empty(), Dual { sets: sets.clone() });
        let enumerated = match agent.determination().expect("a consistent program") {
            Determination::Consistent(mut models) => {
                models.all_members().expect("an exhausted search")
            }
            _ => panic!("the backend answers consistent"),
        };
        assert_eq!(
            enumerated, sets,
            "the backend enumerates the models it holds"
        );
        assert_eq!(
            agent.cautious().expect("a consistent program"),
            Consequences::fold(Mode::Cautious, enumerated.iter()),
            "the native cautious door and the derived fold disagree",
        );
        assert_eq!(
            agent.brave().expect("a consistent program"),
            Consequences::fold(Mode::Brave, enumerated.iter()),
            "the native brave door and the derived fold disagree",
        );
    }

    #[test]
    fn the_native_door_refuses_a_program_with_no_answer_set() {
        // Like the derived door, the native door refuses ⋂/⋃ over a program with
        // no answer set, so the two agree there too (query.md §2.4). The stub
        // honours that contract; the agent forwards its verdict — a contract exemplar.
        let mut agent = Agent::new(Program::empty(), Dual { sets: Vec::new() });
        assert!(agent.cautious().is_err());
        assert!(agent.brave().is_err());
    }

    #[test]
    fn cautious_consequences_are_the_atoms_in_every_model() {
        // { {a,b}, {a,c} }: only a holds in both.
        let mut agent = agent_answering(vec![answer_set(&["a", "b"]), answer_set(&["a", "c"])]);
        let cautious = agent.cautious().expect("a consistent program");
        assert!(
            cautious.contains(&constant("a")),
            "a must hold in every model"
        );
        assert!(
            !cautious.contains(&constant("b")),
            "b holds in only one model, so it is not a cautious consequence",
        );
    }

    #[test]
    fn brave_consequences_are_the_atoms_in_some_model() {
        // { {a,b}, {a,c} }: every model's atoms hold in some model.
        let mut agent = agent_answering(vec![answer_set(&["a", "b"]), answer_set(&["a", "c"])]);
        let brave = agent.brave().expect("a consistent program");
        assert!(
            brave.contains(&constant("a"))
                && brave.contains(&constant("b"))
                && brave.contains(&constant("c")),
            "every model's atoms are brave consequences",
        );
    }

    #[test]
    fn the_consequences_of_a_program_with_no_answer_set_refuse() {
        // ⋂/⋃ over the empty world view is undefined, not ∅: folding zero models
        // would return an empty cautious set — a claim that the program forces
        // nothing, of a program with no model at all (query.md §2.3). Refuse.
        let mut agent = agent_answering(Vec::new());
        assert!(
            agent.cautious().is_err(),
            "cautious of an inconsistent program is a refusal, not the empty set",
        );
        assert!(agent.brave().is_err());
    }

    /// A scripted search that TRUNCATES: a fixed sequence, then a space left open
    /// (a budget hit) rather than closed.
    struct Truncated {
        sets: std::vec::IntoIter<AnswerSet>,
        ended: bool,
    }

    impl Run for Truncated {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            let next = self.sets.next().map(Ok);
            if next.is_none() {
                self.ended = true;
            }
            next
        }

        fn conclusion(&self) -> Option<Conclusion> {
            self.ended.then_some(Conclusion::Budget)
        }
    }

    /// A backend whose search truncates on a budget rather than closing the space.
    struct Budgeted {
        sets: Vec<AnswerSet>,
    }

    impl Backend for Budgeted {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(Solved::over(
                Box::new(Truncated {
                    sets: self.sets.clone().into_iter(),
                    ended: false,
                }),
                Scenario::default(),
            ))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            None
        }
    }

    #[test]
    fn the_consequences_of_a_truncated_search_refuse() {
        // A search that did not close the space cannot give complete ⋂/⋃, and does
        // not fold a partial world view: a witnessed but unexhausted run refuses
        // through the completeness gate, a truncated run with no model through the
        // inconclusive gate. Both name the budget the search hit (solve.md §5.3).
        let mut witnessed = Agent::new(
            Program::empty(),
            Budgeted {
                sets: vec![answer_set(&["a"])],
            },
        );
        let refusal = witnessed
            .cautious()
            .expect_err("an unexhausted world view refuses");
        assert!(
            refusal.to_string().contains("budget"),
            "the refusal names the budget the search hit",
        );
        let mut inconclusive = Agent::new(Program::empty(), Budgeted { sets: Vec::new() });
        let refusal = inconclusive
            .brave()
            .expect_err("an inconclusive search refuses");
        assert!(refusal.to_string().contains("budget"));
    }

    /// A run that faults on its first pull, before any model.
    struct Faulting;

    impl Run for Faulting {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            Some(Err(Fault::engine("the engine died mid-search")))
        }

        fn conclusion(&self) -> Option<Conclusion> {
            None
        }
    }

    /// A backend whose search faults before witnessing any model.
    struct Faulty;

    impl Backend for Faulty {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(Solved::over(Box::new(Faulting), Scenario::default()))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            None
        }
    }

    #[test]
    fn a_faulted_search_carries_its_engine_fault_to_the_consequences() {
        // An engine fault before any model reaches the reader as an engine-locus
        // fault, its cause intact — end to end, not laundered into a request fault
        // (docs/design/solve.md §5.1). The witnessed-truncation path names its
        // conclusion; this path names its cause; both stay honest.
        let mut agent = Agent::new(Program::empty(), Faulty);
        let refusal = agent.cautious().expect_err("a faulted search refuses");
        assert_eq!(refusal.locus(), crate::contract::Locus::Engine);
        assert!(refusal.to_string().contains("the engine died"));
    }

    #[test]
    fn determination_resolves_a_consistent_knowledge_base() {
        let mut agent = agent_answering(vec![answer_set(&["a"])]);
        assert!(matches!(
            agent.determination(),
            Ok(Determination::Consistent(_))
        ));
    }

    #[test]
    fn determination_resolves_an_inconsistent_knowledge_base() {
        let mut agent = agent_answering(Vec::new());
        assert!(matches!(
            agent.determination(),
            Ok(Determination::Inconsistent(_))
        ));
    }

    #[test]
    fn a_question_after_an_amendment_resolves() {
        // The loop's shape (§6.2): amend the knowledge base, then ask — the
        // amendment's disclosed class is read off the backend, and the question
        // still resolves over what the engine answers.
        let mut agent = agent_answering(vec![answer_set(&["a"])]);
        agent
            .assert(themelios_program::Rule::fact(Atom::constant(
                Name::new("a").expect("a valid identifier"),
            )))
            .expect("assert succeeds");
        assert!(matches!(
            agent.determination(),
            Ok(Determination::Consistent(_))
        ));
    }

    #[test]
    fn the_answering_backend_exposes_no_ground_program() {
        assert!(Answering { sets: Vec::new() }.ground_program().is_none());
    }

    #[test]
    fn solve_streams_the_answer_sets_the_backend_yields() {
        let sets = vec![answer_set(&["a"]), answer_set(&["a", "b"])];
        let mut agent = agent_answering(sets.clone());
        let mut solved = agent.solve().expect("the backend answers");
        assert_eq!(solved.all_answer_sets().expect("a closed search"), sets);
    }

    #[test]
    fn solve_with_answers_as_solve_does() {
        let sets = vec![answer_set(&["a"])];
        let mut agent = agent_answering(sets.clone());
        let mut solved = agent
            .solve_with(SolveOptions::default())
            .expect("the backend answers");
        assert_eq!(solved.all_answer_sets().expect("a closed search"), sets);
    }

    #[test]
    fn the_models_of_a_scenario_scoped_question_range_over_the_scenario() {
        let scenario: Scenario = [Assumption::new(constant("p"), true).expect("an atom")]
            .into_iter()
            .collect();
        let mut agent = agent_answering(vec![answer_set(&["p"])]);
        let Ok(Determination::Consistent(models)) = agent
            .solve_assuming(&scenario)
            .map(Solved::into_determination)
        else {
            panic!("a consistent, scenario-scoped determination");
        };
        assert_eq!(*models.scenario(), scenario);
    }
}
