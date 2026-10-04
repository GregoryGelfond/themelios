//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::bridge::Door;
use crate::contract::{
    Backend, Cancel, Capabilities, Capability, ConsequenceRequest, ConsequenceSupport, Fault,
    GroundOptions, Mode, OptimizeRequest, Presupposition, SolveRequest, TruthValue,
};
use crate::extend::Facts;
use crate::outcome::{
    Consequences, Determination, NativeAnswer, NotExhausted, Optimized, Solved, Stopped,
};
use themelios_program::program::{Arguments, Part, PartKey};
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
/// The agent keeps its loop's invariant — its engine level with its knowledge,
/// or a rebuild pending — by tracking, not by reacting (§6.2): a step that
/// fails against the engine is never accepted or replayed, and leaves the next
/// step that touches the engine to rebuild. A grounded part whose replay is
/// refused refuses every later such step at the same part until a replay
/// succeeds, and the knowledge stays intact throughout.
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
    /// The parts grounded through [`ground`](Agent::ground), call by call in
    /// the order grounded — the state a rebuild re-establishes (§6.2).
    grounded: Vec<Box<[Part]>>,
    /// Each external's latest truth value assigned through
    /// [`assign_external`](Agent::assign_external) — re-established by a
    /// rebuild after the grounded parts (§6.2).
    assigned: BTreeMap<Symbol, TruthValue>,
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
            grounded: Vec::new(),
            assigned: BTreeMap::new(),
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
    /// the statement would retract — `Rebuild` (re-grounding) for every statement
    /// today, since the agent guards none with an external — readable from the
    /// handle before any retraction is paid for. The engine is brought level
    /// with the amended knowledge at the next ask, not here, so a single
    /// `assert` touches no backend. Two content-equal assertions get distinct
    /// handles — the set-valued program shows the statement once, and it stays
    /// while either handle is unretracted. Cost: `Θ(program size)`.
    pub fn assert(&mut self, statement: impl Into<Statement>) -> Result<StatementId, Fault> {
        let node = WithProvenance::constructed(statement.into());
        let id = self.ledger.push(node, Self::retraction_class());
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
            return Err(not_live());
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the fact source is taken by value — the design's surface \
                  (docs/design/solve.md §6.2): the caller hands its observations over, though a \
                  `Facts` is only read to enumerate them"
    )]
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
        let class = Self::retraction_class();
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
            return Err(spent());
        }
        for &member in &members {
            self.ledger.retire(member);
        }
        self.knowledge = self.ledger.rebuild();
        Ok(())
    }

    /// Ground the named program parts through the backend (docs/design/solve.md
    /// §6.2) — one of the retained multi-shot mechanisms a caller drives
    /// explicitly, distinct from the monotone `assert`. The engine is first
    /// brought level with what the agent holds, so the parts are grounded over
    /// the knowledge as it stands; grounded, they are retained, and every
    /// rebuild grounds them again. Refuses, retaining nothing, where the backend
    /// refuses. Cost: a rebuild, then the grounding.
    pub fn ground(&mut self, parts: &[Part]) -> Result<(), Fault> {
        require(&self.backend.capabilities(), Capability::MultiShot)?;
        self.bring_level()?;
        self.backend.ground(parts, &GroundOptions::default())?;
        self.grounded.push(parts.into());
        Ok(())
    }

    /// Assign an external atom a truth value at the seam (docs/design/solve.md
    /// §6.2) — the retained multi-shot mechanism a caller toggles an open truth
    /// with. The engine is first brought level with what the agent holds, so
    /// the atom is assigned over the knowledge as it stands; the latest value
    /// assigned each external is retained, and every rebuild assigns it again.
    /// Refuses, retaining nothing, where the backend refuses. Cost: a rebuild,
    /// then the assignment.
    pub fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        require(&self.backend.capabilities(), Capability::MultiShot)?;
        self.bring_level()?;
        self.backend.assign_external(external.clone(), value)?;
        self.assigned.insert(external, value);
        Ok(())
    }

    // ---- ask (§6.2, §6.4) ----

    /// Ask the question `solve` of the knowledge base (docs/design/solve.md
    /// §6.2): bring the engine level with the owned knowledge, then the run
    /// handle — stream the models, inspect or resolve the trichotomy, read
    /// the conclusion (§5.2). The handle borrows the agent for its life: the
    /// borrow checker is the "no mutation while reasoning" lock (§6.1), so an
    /// amendment or a second question while it is held does not compile. The
    /// same question the bare `Program` answers with an owned handle (§6.4).
    /// Refusal: an engine or request `Fault` — an inconsistent or inconclusive
    /// knowledge base is a reading of the handle, never a fault (§5.1). Cost:
    /// the engine brought level — on a multi-shot backend a `reset`, one
    /// lowering, and the replay of what the loop retains; on a single-shot one
    /// the lowering — then the engine's, streamed.
    pub fn solve(&mut self) -> Result<Solved<'_>, Fault> {
        self.bring_level()?;
        self.backend.solve(&SolveRequest::default())
    }

    /// The configured pair of [`solve`](Agent::solve) (docs/design/solve.md
    /// §6.3): the same question under the options — the time budget the
    /// question carries, handed on the request to a backend that enforces it
    /// natively. A budget the backend does not so enforce refuses at the
    /// request locus, as unrealisable (§6.3), before anything is lowered: the
    /// core's own timer over a cancelling backend — the realisation rule's
    /// other arm — is realised with cancellation.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the options are taken by value — the design's surface (docs/design/solve.md \
                  §6.3): the caller hands the configuration over, though only its knobs are read"
    )]
    pub fn solve_with(&mut self, options: SolveOptions) -> Result<Solved<'_>, Fault> {
        if options.time.is_some() {
            realisable(&self.backend.capabilities())?;
        }
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
    /// `optimization` (§4.2), before anything is lowered.
    pub fn optimize(&mut self, request: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        require(&self.backend.capabilities(), Capability::Optimization)?;
        self.bring_level()?;
        self.backend.optimize(request)
    }

    /// Ask under a scenario (docs/design/solve.md §6.3): each of its
    /// assumptions fixed for the span of this one question and discharged
    /// after it — a hypothesis, as distinct from a retraction (§6.2), which
    /// amends the knowledge base. Refuses at the request locus over a backend
    /// that does not declare `assumptions` (§4.2) — read from the declaration
    /// before the question is paid for, so a refused scenario lowers nothing.
    pub fn solve_assuming(&mut self, scenario: &Scenario) -> Result<Solved<'_>, Fault> {
        require(&self.backend.capabilities(), Capability::Assumptions)?;
        self.bring_level()?;
        self.backend
            .solve_assuming(scenario, &SolveRequest::default())
    }

    /// The core's handle that interrupts an in-flight question from another
    /// thread (docs/design/solve.md §6.1, §6.3), over the backend's
    /// cancellation primitive — `Some` exactly when the backend declares
    /// `cancellation`; `None` says the engine cannot be interrupted, readable
    /// before any question is paid for. Reserved: the handle's pull is realised
    /// with cancellation, and until then it holds the primitive and nothing
    /// more — `Some` says the engine can be interrupted, not yet that this
    /// handle does (see [`Interrupt`]). O(1).
    pub fn interrupt(&self) -> Option<Interrupt> {
        self.backend.interrupt().map(|primitive| Interrupt {
            _primitive: primitive,
        })
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
        self.consequences(Mode::Cautious, None)
    }

    /// The brave consequences (`⋃`, "what can hold") of the knowledge base
    /// (docs/design/solve.md §5.2; query.md §2.4): the atoms true in SOME answer
    /// set. Routed and gated as the cautious reading is.
    pub fn brave(&mut self) -> Result<Consequences, Fault> {
        self.consequences(Mode::Brave, None)
    }

    /// The cautious consequences under a scenario (docs/design/solve.md §6.2;
    /// query.md §2.4): the atoms true in every answer set the scenario admits —
    /// the models [`solve_assuming`](Agent::solve_assuming) denotes — so "under
    /// these assumptions, what must hold?" is one question, the epistemic sibling
    /// of a solve under a hypothesis. Routed as [`cautious`](Agent::cautious) is —
    /// the engine's own door over a request carrying the scenario, otherwise the
    /// core's fold over the scenario's enumerated models — and refused as it is,
    /// over the scenario's world view. Refuses at the request locus over a
    /// backend that does not declare `assumptions`, on either route, before the
    /// question is paid for. Cost: one solve for the native door, `Θ(|W|)` for
    /// the derived.
    pub fn cautious_assuming(&mut self, scenario: &Scenario) -> Result<Consequences, Fault> {
        self.consequences(Mode::Cautious, Some(scenario))
    }

    /// The brave consequences under a scenario (docs/design/solve.md §6.2;
    /// query.md §2.4): the atoms true in SOME answer set the scenario admits.
    /// Routed and gated as the scoped cautious reading is.
    pub fn brave_assuming(&mut self, scenario: &Scenario) -> Result<Consequences, Fault> {
        self.consequences(Mode::Brave, Some(scenario))
    }

    /// The consequences in `mode` — the shared body of the cautious and brave
    /// readings, unscoped or over a scenario's models. The native door computes
    /// them in one solve, over a request carrying the scenario, and the core
    /// gates its answer; the derived door folds the enumerated CONSISTENT world
    /// view — `solve`'s, or `solve_assuming`'s under a scenario — as its models
    /// stream through the exhaustion gate, one resident beside the accumulator,
    /// never the collection; so the two doors range over the same models, and
    /// answer and refuse alike (the free differential, query.md §2.4). The consistency
    /// gate is load-bearing: a fold over no models is no consequence set — `⋂`
    /// over none is undefined, and an empty cautious set would say the program
    /// forces nothing, of a program that has no model — so an inconsistent program
    /// refuses (query.md §2.3), while a consistent one's stream yields the model
    /// that witnessed it. Both range over all stable models: the question `solve`
    /// asks ignores any objective, and an optimization's consequences range over
    /// its optimal set instead (solve.md §5.2).
    fn consequences(
        &mut self,
        mode: Mode,
        scenario: Option<&Scenario>,
    ) -> Result<Consequences, Fault> {
        if scenario.is_some() {
            require(&self.backend.capabilities(), Capability::Assumptions)?;
        }
        self.bring_level()?;
        match self.backend.capabilities().native_consequences {
            ConsequenceSupport::Native => {
                let request = ConsequenceRequest {
                    scenario: scenario.cloned().unwrap_or_default(),
                };
                match self.backend.consequences_native(mode, &request)? {
                    NativeAnswer::Closed(symbols) => Ok(Consequences { symbols, mode }),
                    NativeAnswer::NoModel => Err(no_answer_set(scenario)),
                    NativeAnswer::Stopped(truncation) => {
                        Err(NotExhausted::not_closed(truncation).into())
                    }
                }
            }
            ConsequenceSupport::DerivedByEnumeration => {
                let solved = match scenario {
                    None => self.backend.solve(&SolveRequest::default())?,
                    Some(scenario) => self
                        .backend
                        .solve_assuming(scenario, &SolveRequest::default())?,
                };
                match solved.into_determination() {
                    // Folded as the models stream, one resident at a time. The
                    // determination's peek parked its witness in the untouched
                    // stream, so a fold the gate passes holds at least that model.
                    Determination::Consistent(mut models) => Ok(models
                        .fold_members(mode)?
                        .expect("a consistent search's collection holds its witness")),
                    Determination::Inconsistent(_) => Err(no_answer_set(scenario)),
                    // A truncated search refuses in the native door's words
                    // whether or not it saw a model, since a native answer
                    // cannot say; a fault stays its own cause.
                    Determination::Inconclusive(partial) => Err(match partial.stopped() {
                        Stopped::Concluded(truncation) => {
                            NotExhausted::not_closed(truncation).into()
                        }
                        Stopped::Faulted(fault) => fault.clone(),
                    }),
                }
            }
        }
    }

    /// Bring the engine level with what the agent holds before a question, a
    /// grounding, or an assignment is put to it (docs/design/solve.md §6.2):
    /// the knowledge base as it stands, lowered through Door B (§10.2) — the
    /// assertions and retractions since the last rebuild included — and the
    /// state the loop retains, re-established. On a multi-shot backend, where
    /// `lower` accumulates, that is §6.2's rebuild: a `reset`, the lowering,
    /// then the replay — the grounded parts first, in the order they were
    /// grounded, then each assigned external's latest value, since an external
    /// is assigned only once grounded. On a single-shot backend `lower`
    /// replaces the program and nothing is retained, so the lowering is the
    /// whole of it. A refused step is the caller's fault, and nothing is
    /// delegated after it.
    ///
    /// The loop's invariant — the engine level with the knowledge, or a
    /// rebuild pending — is kept by tracking, not by reacting: any step of the
    /// agent's own that fails against the engine leaves a rebuild pending,
    /// whatever the fault, and only a rebuild that succeeds clears it, so the
    /// agent reads neither a fault's locus nor `Presupposition::NeedsRebuild`
    /// to decide. Every call rebuilds, so a rebuild is always pending and the
    /// tracking is trivial; the pending mark takes effect with the retained
    /// engine. A refused replay — an accepted part whose grounding now fails —
    /// refuses this step with the knowledge intact, and every later step that
    /// touches the engine retries the rebuild and refuses at the same part
    /// until a replay succeeds. The retained-engine realisations the
    /// retraction classes disclose — the external toggle at the seam, the
    /// incremental grounding of an addition — are reserved until the retained
    /// engine is implemented. Cost: `Θ(program size + grounded
    /// parts + assigned externals)` at the seam — a reset, one lowering
    /// (§10.1), and the replay — the re-grounding the engine's.
    fn bring_level(&mut self) -> Result<(), Fault> {
        if !self.backend.capabilities().multi_shot {
            return self.backend.lower(Door::Program(&self.knowledge));
        }
        self.backend.reset()?;
        self.backend.lower(Door::Program(&self.knowledge))?;
        for parts in &self.grounded {
            self.backend.ground(parts, &GroundOptions::default())?;
        }
        for (external, value) in &self.assigned {
            self.backend.assign_external(external.clone(), *value)?;
        }
        Ok(())
    }

    /// The retraction class an asserted statement is disclosed under
    /// (docs/design/solve.md §6.2) — fixed from the backend's `externals`
    /// capability and whether the statement is externally guarded: `Toggle` for
    /// a guarded statement over a backend honouring externals, `Rebuild`
    /// otherwise. The agent guards no statement yet — every question lowers the
    /// whole knowledge base, the guarded-external realisation landing with the
    /// retained engine — so every statement discloses `Rebuild`, whatever the
    /// backend declares: no caller pays a rebuild believing it a toggle. O(1).
    fn retraction_class() -> RetractionClass {
        RetractionClass::Rebuild
    }
}

/// The refusal of a retract whose handle names nothing live in the agent's
/// knowledge — retracted already, or another agent's (docs/design/solve.md
/// §6.2).
fn not_live() -> Fault {
    Fault::request(
        "retract of a statement that is not live",
        Presupposition::NotLive,
    )
}

/// The refusal of a forget whose observation is spent — forgotten already, or
/// another agent's (docs/design/solve.md §6.2).
fn spent() -> Fault {
    Fault::request("forget of a spent observation", Presupposition::Spent)
}

/// The gate every capability-gated question passes (docs/design/solve.md §4.1,
/// §4.2): a question beyond the `declaration` refuses at the request locus,
/// naming the `capability` it needed. The declaration is read, not the method
/// trusted — a backend answering a method it does not declare, or a native
/// consequence door handed a scenario it cannot honour, would otherwise answer
/// in its place, a silent degrade — and it is read before the question is paid
/// for, so a refused question lowers nothing. A function of the declaration
/// alone. O(1).
fn require(declaration: &Capabilities, capability: Capability) -> Result<(), Fault> {
    if declaration.declares(capability) {
        Ok(())
    } else {
        Err(Fault::unsupported(capability))
    }
}

/// The gate a budgeted question passes (docs/design/solve.md §6.3): a time
/// budget the `declaration` neither enforces nor lets the core enforce refuses
/// at the request locus, as unrealisable, before anything is lowered. The core's
/// own timer over a cancelling backend is realised with cancellation, so until
/// then only a backend enforcing the budget natively passes. A function of the
/// declaration alone. O(1).
fn realisable(declaration: &Capabilities) -> Result<(), Fault> {
    if declaration.budgets.time {
        Ok(())
    } else {
        Err(Fault::request(
            "a time budget this backend neither enforces nor lets the core enforce",
            Presupposition::UnrealisableBudget,
        ))
    }
}

/// The refusal of consequences over no model — `⋂`/`⋃` over the empty world
/// view is undefined, not `∅` (query.md §2.3). Under a scenario the program may
/// well have answer sets, just none the scenario admits, so the refusal says
/// which.
fn no_answer_set(scenario: Option<&Scenario>) -> Fault {
    Fault::request(
        match scenario {
            None => "no consequences: the program has no answer set",
            Some(_) => "no consequences: the program has no answer set under the scenario",
        },
        Presupposition::NoAnswerSet,
    )
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
            Presupposition::NotAnAtom,
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
/// statement in the ledger that issued it and nowhere else. The one
/// process-global the crate keeps, against the design's rule of no global
/// mutable state (docs/design/solve.md §6.1): a monotone counter that confers
/// no authority — it only tells one agent's handles from another's — kept
/// because a handle from another agent at the same slot and generation has no
/// cheaper witness.
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

/// The core's handle that interrupts an in-flight solve from another thread
/// (docs/design/solve.md §6.2, §6.3), over the backend's [`Cancel`] primitive.
/// The core owns it so it can record a caller's pull, and so attribute a stop
/// to the caller rather than to its own budget timer. Reserved: the pull and
/// its attribution are realised with cancellation, until when the handle holds
/// the primitive and nothing more. `Send`, as the primitive is.
pub struct Interrupt {
    _primitive: Box<dyn Cancel>,
}

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

    /// The smaller rebuild-scaling probe: large enough to time well above the
    /// clock's resolution, small enough that the larger probe stays a fast unit
    /// test.
    const PROBE_ENTRIES: usize = 1_000;
    /// The data-size ratio between the smaller and the larger probe.
    const SIZE_RATIO: usize = 16;
    /// A linear rebuild at SIZE_RATIO may cost at most this factor: fourfold
    /// noise headroom above linear (x16) and fourfold separation below quadratic
    /// (x256), the margin the other tiers' scaling tripwires hold.
    const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
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
        let small = fastest_rebuild_nanos(&ledger_of_size(PROBE_ENTRIES));
        let large = fastest_rebuild_nanos(&ledger_of_size(PROBE_ENTRIES * SIZE_RATIO));
        assert!(
            large < small.saturating_mul(LINEAR_CEILING),
            "rebuild grew worse than linearly — {small}ns at {PROBE_ENTRIES} entries, \
             {large}ns at {} entries — the reinsertion recurrence to avoid",
            PROBE_ENTRIES * SIZE_RATIO,
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

    use crate::contract::{Capabilities, Refused};
    use crate::outcome::{AnswerSet, Conclusion, Model, Run, ShowRule, Truncation};

    /// Whether `fault` refused the request for the presupposition `expected`.
    fn refused_for(fault: &Fault, expected: Presupposition) -> bool {
        matches!(fault.refused(), Refused::Request(presupposition) if presupposition == expected)
    }

    /// A search stopped at its budget, short of the space.
    const AT_THE_BUDGET: Presupposition = Presupposition::Unclosed(Truncation::Budget);

    /// The answer sets of `models`, in order.
    fn atoms_of(models: &[Model]) -> Vec<AnswerSet> {
        models.iter().map(|model| model.atoms().clone()).collect()
    }

    /// The consequences `mode` folds over `sets`, a non-empty world view.
    fn folded(mode: Mode, sets: &[AnswerSet]) -> Consequences {
        Consequences::fold(mode, sets).expect("a non-empty world view")
    }
    use themelios_program::Name;

    // A question resolves over a run, and a run is built here, in the defining
    // crate: the laws below drive the agent's questions through a backend that
    // answers with a scripted search, so the resolution register is exercised
    // end to end — a recording backend outside the crate can only refuse.

    /// A scripted search: a fixed model sequence, then a closed space.
    struct Scripted {
        sets: std::vec::IntoIter<AnswerSet>,
        ended: bool,
    }

    impl Run for Scripted {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            let next = self.sets.next().map(|set| Ok(Model::of(set)));
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
        Solved::running(
            Box::new(Scripted {
                sets: sets.into_iter(),
                ended: false,
            }),
            scenario,
            ShowRule::default(),
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

        fn consequences_native(
            &mut self,
            mode: Mode,
            _request: &ConsequenceRequest,
        ) -> Result<NativeAnswer, Fault> {
            if self.sets.is_empty() {
                return Ok(NativeAnswer::NoModel);
            }
            Ok(NativeAnswer::Closed(match mode {
                Mode::Cautious => intersect(&self.sets),
                Mode::Brave => union(&self.sets),
            }))
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
            atoms_of(&enumerated),
            sets,
            "the backend enumerates the models it holds"
        );
        assert_eq!(
            agent.cautious().expect("a consistent program"),
            folded(Mode::Cautious, &sets),
            "the native cautious door and the derived fold disagree",
        );
        assert_eq!(
            agent.brave().expect("a consistent program"),
            folded(Mode::Brave, &sets),
            "the native brave door and the derived fold disagree",
        );
    }

    #[test]
    fn the_native_door_refuses_a_program_with_no_answer_set() {
        // The native door reports no model and the core refuses ⋂/⋃ over it —
        // in the derived door's words, so the two agree there too (query.md
        // §2.4).
        let mut native = Agent::new(Program::empty(), Dual { sets: Vec::new() });
        let mut derived = agent_answering(Vec::new());
        assert_eq!(
            native.cautious().expect_err("no model"),
            derived.cautious().expect_err("no model"),
        );
        assert_eq!(
            native.brave().expect_err("no model"),
            derived.brave().expect_err("no model"),
        );
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
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            let next = self.sets.next().map(|set| Ok(Model::of(set)));
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
            Ok(Solved::running(
                Box::new(Truncated {
                    sets: self.sets.clone().into_iter(),
                    ended: false,
                }),
                Scenario::default(),
                ShowRule::default(),
            ))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
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
            refused_for(&refusal, AT_THE_BUDGET),
            "the refusal names the budget the search hit: {refusal:?}",
        );
        let mut inconclusive = Agent::new(Program::empty(), Budgeted { sets: Vec::new() });
        let refusal = inconclusive
            .brave()
            .expect_err("an inconclusive search refuses");
        assert!(refused_for(&refusal, AT_THE_BUDGET), "{refusal:?}");
    }

    /// A run that faults on its first pull, before any model.
    struct Faulting;

    impl Run for Faulting {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
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
            Ok(Solved::running(
                Box::new(Faulting),
                Scenario::default(),
                ShowRule::default(),
            ))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
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
    fn solve_streams_the_models_the_backend_yields() {
        let sets = vec![answer_set(&["a"]), answer_set(&["a", "b"])];
        let mut agent = agent_answering(sets.clone());
        let mut solved = agent.solve().expect("the backend answers");
        let models = solved.all_models().expect("a closed search");
        assert_eq!(atoms_of(&models), sets);
    }

    #[test]
    fn solve_with_answers_as_solve_does() {
        let sets = vec![answer_set(&["a"])];
        let mut agent = agent_answering(sets.clone());
        let mut solved = agent
            .solve_with(SolveOptions::default())
            .expect("the backend answers");
        let models = solved.all_models().expect("a closed search");
        assert_eq!(atoms_of(&models), sets);
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

    // ---- The scenario-scoped readings (§6.2) ----

    /// The assumption fixing the constant `name` to hold (`true`) or not.
    fn assume(name: &str, holds: bool) -> Assumption {
        Assumption::new(constant(name), holds).expect("a constant is an atom")
    }

    /// The answer sets of `sets` a scenario admits — the models
    /// `solve_assuming(scenario)` denotes: an assumption fixed to hold keeps the
    /// answer sets holding its atom, one fixed not to hold keeps those without it.
    fn admitted(sets: &[AnswerSet], scenario: &Scenario) -> Vec<AnswerSet> {
        sets.iter()
            .filter(|set| {
                scenario
                    .assumptions()
                    .all(|assumption| set.contains(assumption.atom()) == assumption.holds())
            })
            .cloned()
            .collect()
    }

    /// A backend that honours assumptions — its `solve_assuming` keeps only the
    /// answer sets a scenario admits, so a scoped reading differs from the
    /// unscoped one — and declares the consequence path `support`: under
    /// `Native`, its own door ranges over the scenario a request carries,
    /// computed independently of the core fold. A `truncated` scenario-scoped
    /// search leaves the space open (a budget hit) rather than closing it; the
    /// unscoped `solve` always closes it.
    struct Hypothetical {
        sets: Vec<AnswerSet>,
        support: ConsequenceSupport,
        truncated: bool,
    }

    impl Backend for Hypothetical {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                assumptions: true,
                native_consequences: self.support,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(self.sets.clone(), Scenario::default()))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn solve_assuming(
            &mut self,
            scenario: &Scenario,
            _request: &SolveRequest,
        ) -> Result<Solved<'_>, Fault> {
            let sets = admitted(&self.sets, scenario);
            if self.truncated {
                return Ok(Solved::running(
                    Box::new(Truncated {
                        sets: sets.into_iter(),
                        ended: false,
                    }),
                    scenario.clone(),
                    ShowRule::default(),
                ));
            }
            Ok(solved_over(sets, scenario.clone()))
        }

        fn consequences_native(
            &mut self,
            mode: Mode,
            request: &ConsequenceRequest,
        ) -> Result<NativeAnswer, Fault> {
            if self.truncated {
                return Ok(NativeAnswer::Stopped(Truncation::Budget));
            }
            let sets = admitted(&self.sets, &request.scenario);
            if sets.is_empty() {
                return Ok(NativeAnswer::NoModel);
            }
            Ok(NativeAnswer::Closed(match mode {
                Mode::Cautious => intersect(&sets),
                Mode::Brave => union(&sets),
            }))
        }
    }

    /// An agent over a backend honouring assumptions over `sets`, reading its
    /// consequences through `support`, its search closing the space.
    fn agent_assuming(sets: Vec<AnswerSet>, support: ConsequenceSupport) -> Agent<Hypothetical> {
        Agent::new(
            Program::empty(),
            Hypothetical {
                sets,
                support,
                truncated: false,
            },
        )
    }

    /// The fixture world view: `a` holds in two of its three models, and the
    /// third, `{d}`, is the one a scenario fixing `a` excludes.
    fn three_models() -> Vec<AnswerSet> {
        vec![
            answer_set(&["a", "b"]),
            answer_set(&["a", "c"]),
            answer_set(&["d"]),
        ]
    }

    /// The scenario fixing `a` to hold: it admits the two models holding `a`.
    fn assuming_a() -> Scenario {
        [assume("a", true)].into_iter().collect()
    }

    /// The consequences in `mode` that are exactly the named constants.
    fn consequences_of(mode: Mode, names: &[&str]) -> Consequences {
        Consequences {
            symbols: names.iter().copied().map(constant).collect(),
            mode,
        }
    }

    /// A backend that does NOT declare `assumptions` yet answers
    /// `solve_assuming` anyway — a declaration the method contradicts — so a
    /// scoped question refused over it is refused by the declaration, not by the
    /// method's default.
    struct Undeclared {
        sets: Vec<AnswerSet>,
    }

    impl Backend for Undeclared {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                enumeration: true,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(self.sets.clone(), Scenario::default()))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Ok(())
        }

        fn solve_assuming(
            &mut self,
            scenario: &Scenario,
            _request: &SolveRequest,
        ) -> Result<Solved<'_>, Fault> {
            Ok(solved_over(
                admitted(&self.sets, scenario),
                scenario.clone(),
            ))
        }
    }

    /// A backend that declares no `assumptions`, reads its consequences through
    /// `support`, and whose lowering faults — so a question refused with its
    /// capability fault, not this one, was refused before the knowledge base was
    /// lowered, on either consequence route.
    struct Unlowerable {
        support: ConsequenceSupport,
    }

    impl Backend for Unlowerable {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                native_consequences: self.support,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Err(Fault::engine("an unlowered program was solved"))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Err(Fault::engine("the lowering was paid for"))
        }
    }

    #[test]
    fn cautious_assuming_ranges_over_the_scenario_s_models() {
        // Under `a` the models are {a,b} and {a,c}, whose ⋂ is {a}. Unscoped, the
        // model {d} empties the ⋂ — so a reading that dropped its scenario would
        // answer ∅ and be caught.
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::DerivedByEnumeration);
        assert_eq!(
            agent.cautious_assuming(&assuming_a()),
            Ok(consequences_of(Mode::Cautious, &["a"])),
        );
    }

    #[test]
    fn brave_assuming_ranges_over_the_scenario_s_models() {
        // Under `a` the models are {a,b} and {a,c}, whose ⋃ is {a,b,c}: the
        // excluded model's d is not a brave consequence of the scenario.
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::DerivedByEnumeration);
        assert_eq!(
            agent.brave_assuming(&assuming_a()),
            Ok(consequences_of(Mode::Brave, &["a", "b", "c"])),
        );
    }

    #[test]
    fn the_scoped_native_door_and_the_scoped_derived_fold_agree() {
        // The free differential under a scenario (query.md §2.4): the backend's own
        // door over the request's scenario, and the core's fold over the models
        // `solve_assuming` enumerates, give the same answer. The fixture's unscoped
        // ⋂ is ∅ and its scoped ⋂ is {a}, so a native request that lost its
        // scenario would disagree with the fold.
        let scenario = assuming_a();
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::Native);
        let enumerated = match agent
            .solve_assuming(&scenario)
            .map(Solved::into_determination)
        {
            Ok(Determination::Consistent(mut models)) => {
                models.all_members().expect("an exhausted search")
            }
            _ => panic!("the scenario admits a model"),
        };
        let admitted = atoms_of(&enumerated);
        assert_eq!(
            agent.cautious_assuming(&scenario),
            Ok(folded(Mode::Cautious, &admitted)),
            "the scoped native cautious door and the scoped fold disagree",
        );
        assert_eq!(
            agent.brave_assuming(&scenario),
            Ok(folded(Mode::Brave, &admitted)),
            "the scoped native brave door and the scoped fold disagree",
        );
    }

    #[test]
    fn the_scoped_native_door_refuses_a_backend_without_assumptions() {
        // The native door would answer — it ignores the request's scenario — so
        // only the declaration stops a scoped request from being answered as an
        // unscoped one, the silent degrade §4.2 forbids.
        let mut agent = Agent::new(
            Program::empty(),
            Dual {
                sets: three_models(),
            },
        );
        assert_eq!(
            agent.cautious_assuming(&assuming_a()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
        assert_eq!(
            agent.brave_assuming(&assuming_a()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
    }

    #[test]
    fn the_scoped_derived_door_refuses_a_backend_without_assumptions() {
        // The backend answers `solve_assuming` although it does not declare the
        // capability: the declaration, read first, governs.
        let mut agent = Agent::new(
            Program::empty(),
            Undeclared {
                sets: three_models(),
            },
        );
        assert_eq!(
            agent.cautious_assuming(&assuming_a()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
        assert_eq!(
            agent.brave_assuming(&assuming_a()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
    }

    #[test]
    fn solve_assuming_is_refused_though_the_undeclared_method_would_answer() {
        // The backend's `solve_assuming` answers; its declaration does not name
        // `assumptions`. The declaration governs.
        let mut agent = Agent::new(
            Program::empty(),
            Undeclared {
                sets: three_models(),
            },
        );
        assert_eq!(
            agent.solve_assuming(&assuming_a()).err(),
            Some(Fault::unsupported(Capability::Assumptions))
        );
    }

    #[test]
    fn a_scoped_question_without_assumptions_refuses_before_lowering() {
        // The capability is read before the question is paid for (§4.1): each
        // scoped question refuses with the capability fault, not the fault the
        // lowering would have raised — on the native consequence route as on the
        // derived.
        for support in [
            ConsequenceSupport::Native,
            ConsequenceSupport::DerivedByEnumeration,
        ] {
            let mut agent = Agent::new(Program::empty(), Unlowerable { support });
            assert_eq!(
                agent.solve_assuming(&assuming_a()).err(),
                Some(Fault::unsupported(Capability::Assumptions))
            );
            assert_eq!(
                agent.cautious_assuming(&assuming_a()),
                Err(Fault::unsupported(Capability::Assumptions))
            );
            assert_eq!(
                agent.brave_assuming(&assuming_a()),
                Err(Fault::unsupported(Capability::Assumptions))
            );
        }
    }

    #[test]
    fn a_scoped_reading_under_the_empty_scenario_still_needs_assumptions() {
        // The empty scenario fixes nothing, yet it is still a scenario: the scoped
        // doors require `assumptions` with no exception for it (§6.2). The native
        // door here would answer, so only the declaration refuses.
        let mut agent = Agent::new(
            Program::empty(),
            Dual {
                sets: three_models(),
            },
        );
        assert_eq!(
            agent.cautious_assuming(&Scenario::default()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
        assert_eq!(
            agent.brave_assuming(&Scenario::default()),
            Err(Fault::unsupported(Capability::Assumptions))
        );
    }

    #[test]
    fn solve_assuming_under_the_empty_scenario_still_needs_assumptions() {
        // As for the scoped doors (§6.2): the method here would answer, so only
        // the declaration refuses.
        let mut agent = Agent::new(
            Program::empty(),
            Undeclared {
                sets: three_models(),
            },
        );
        assert_eq!(
            agent.solve_assuming(&Scenario::default()).err(),
            Some(Fault::unsupported(Capability::Assumptions))
        );
    }

    #[test]
    fn a_scoped_reading_with_no_model_under_the_scenario_refuses() {
        // No model holds both `a` and `d`: ⋂/⋃ over the scenario's empty world view
        // is undefined, not ∅. The refusal names the scenario, since the program
        // itself has answer sets — only none the scenario admits.
        let impossible: Scenario = [assume("a", true), assume("d", true)].into_iter().collect();
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::DerivedByEnumeration);
        let refusal = agent
            .cautious_assuming(&impossible)
            .expect_err("no model under the scenario");
        assert!(
            refused_for(&refusal, Presupposition::NoAnswerSet),
            "{refusal:?}"
        );
        assert!(refusal.to_string().contains("scenario"), "{refusal}");
        let brave = agent
            .brave_assuming(&impossible)
            .expect_err("no model under the scenario");
        assert!(
            refused_for(&brave, Presupposition::NoAnswerSet),
            "{brave:?}"
        );
    }

    #[test]
    fn the_scoped_native_door_refuses_a_scenario_with_no_model() {
        // The native door owes the derived door's refusal over a scenario that
        // admits no model, so the two agree there too (query.md §2.4). The stub
        // honours that contract; the agent forwards its verdict.
        let impossible: Scenario = [assume("a", true), assume("d", true)].into_iter().collect();
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::Native);
        assert!(agent.cautious_assuming(&impossible).is_err());
        assert!(agent.brave_assuming(&impossible).is_err());
    }

    #[test]
    fn the_native_door_refuses_a_truncated_search_as_the_derived_door_does() {
        // A native search stopped short has converged on an approximation, not
        // the consequences: the core refuses it, naming the budget, in the words
        // the derived door gives a search that did not close the space.
        let truncated = |support| {
            Agent::new(
                Program::empty(),
                Hypothetical {
                    sets: three_models(),
                    support,
                    truncated: true,
                },
            )
        };
        let native = truncated(ConsequenceSupport::Native)
            .cautious_assuming(&assuming_a())
            .expect_err("a native search stopped short refuses");
        let derived = truncated(ConsequenceSupport::DerivedByEnumeration)
            .cautious_assuming(&assuming_a())
            .expect_err("an unexhausted world view refuses");
        assert!(refused_for(&native, AT_THE_BUDGET), "{native:?}");
        assert_eq!(native, derived);
    }

    #[test]
    fn the_doors_refuse_a_truncated_search_alike_though_it_saw_no_model() {
        // No model admitted, and the search cut at its budget: the derived door
        // reads it inconclusive, the native door reports it stopped, and the two
        // refuse in one wording.
        let unwitnessed = |support| {
            Agent::new(
                Program::empty(),
                Hypothetical {
                    sets: Vec::new(),
                    support,
                    truncated: true,
                },
            )
        };
        let native = unwitnessed(ConsequenceSupport::Native)
            .brave_assuming(&assuming_a())
            .expect_err("a native search stopped short refuses");
        let derived = unwitnessed(ConsequenceSupport::DerivedByEnumeration)
            .brave_assuming(&assuming_a())
            .expect_err("a truncated search refuses");
        assert_eq!(native, derived);
    }

    #[test]
    fn a_scoped_reading_of_a_truncated_search_refuses() {
        // A scenario's search that did not close the space cannot give complete
        // ⋂/⋃: the scoped route passes the same completeness gate as the unscoped
        // one, and the refusal names the budget the search hit.
        let mut agent = Agent::new(
            Program::empty(),
            Hypothetical {
                sets: three_models(),
                support: ConsequenceSupport::DerivedByEnumeration,
                truncated: true,
            },
        );
        let refusal = agent
            .cautious_assuming(&assuming_a())
            .expect_err("an unexhausted scoped world view refuses");
        assert!(refused_for(&refusal, AT_THE_BUDGET), "{refusal:?}");
    }

    #[test]
    fn the_empty_scenario_s_scoped_reading_is_the_unscoped_reading() {
        // The empty scenario fixes nothing, so it admits every model (§6.3).
        let mut agent = agent_assuming(three_models(), ConsequenceSupport::DerivedByEnumeration);
        let unscoped = agent.cautious();
        assert_eq!(agent.cautious_assuming(&Scenario::default()), unscoped);
        let unscoped = agent.brave();
        assert_eq!(agent.brave_assuming(&Scenario::default()), unscoped);
    }
}
