# themelios-solve — design of record

2026-09-03, revised through 2026-10-04 (§17). The design of record, which the build follows; §17 records
each revision, the reconciliations with built code among them. This is the normative design for the
**solve tier** — `themelios-solve` and the adapter crates that realise it — the fourth tier over the
shared base (§12.1 of the specification). Its sibling `themelios-query` has its own design
(`query.md`); the two are built as one stage, the way `analysis.md` accompanies `program.md`. This
document stands with `specification.md` §9/§11/§12 and the built tiers' designs (`base.md`,
`syntax.md`, `program.md`, `analysis.md`, `grammar.md`); where it evolves the specification's crate
roster or clause it says so in place (§16).

The keystone, stated once so the rest can be read against it: **the solve tier is the *abstract
solver* — the codegen/target contract over the `Program` value, whose operations are the questions a
logician asks of that value and whose answers are typed values.** The concrete engine behind the
contract (clingo and clingcon now, our own engine later) is a configuration of the installation, never
a thing the program author sees.

The register of this document matches its built siblings (`program.md`, `analysis.md`): every
load-bearing surface is stated as a Rust signature with its refusal and its cost model. The
implementation is written at build time; the types, the laws, and the costs are decided here. Where a
shape is deliberately governed by a downstream principle — the propagator trait, held to the DL/CP/LP
litmus (§8) — the design states the **interface and its governing principle** rather than a frozen
signature, and says so in place.

**Assumed fluency.** Fluent Rust (ownership, lifetimes, traits) and ASP as the grammar and
specification of record state it; not assumed are rust-analyzer's, rowan's, or an engine's internals.

---

## 1. Keystone and design method

### 1.1 The abstract solver, in the foundation's own shape

The foundation is LLVM-shaped: `themelios-syntax` is the frontend, `themelios-program`'s `Program` is
the intermediate representation (the logician's abstract object — `program.md` §1), `themelios-analysis`
is the pass layer, and **the solve tier is the codegen/target**. A target, in that shape, is a
**contract** an engine implements — not a second representation. The one genuinely new data object
below the seam is the *ground* program, the machine-IR analog, and it is a named capability over the
contract (§10.4), not the tier's centre.

So `themelios-solve`'s core is a **behavioral contract** (§4), engine-free, that the clingo and
clingcon adapters (§11) and the in-house engine **zetesis** (§12) each implement — and for zetesis,
`themelios-solve` is the first-class *programmatic* API a programmer drives the engine through, not a
CLI or a bespoke per-engine interface. The abstraction is the deliverable; the programmer's ergonomics,
the mission properties, and the in-house engine are all consequences of getting that one object right.
The contract is co-designed so that our own engine slots in behind it with no change above the seam —
and so that reasoning about *our* solver is reasoning about a legible mathematical object rather than
about clingo's operational machinery.

### 1.2 The API is the logician's questions

Because the `Program` **is** the logician's abstract object, the solve and query surfaces are not "a
driver for an engine": they are the **typed answers to the natural questions one asks of that
object**. *Is it consistent? What are its stable models? What must hold, what can hold? Does this
atom hold — yes, no, or unknown? What is the proven optimum? Under these assumptions, is it
consistent, and if not, who is responsible?* Those questions are the surface. This is the same
register `themelios-analysis` already speaks — extended from the *structural* questions (what class
is it, is it safe, does it ground finitely) to the *semantic* ones — so the whole stack reads as one
sentence:

> source → (one grammar) → `Program` → (the API = the logician's questions) → typed answers (its
> denotation).

The design method that follows, and the one this document applies section by section: **enumerate the
logician's questions about a `Program`, let those shape the surface — and where each lands (analysis /
solve / query) — and never shape anything around clingo's control-flow.**

### 1.3 Model–view throughout

Every result the tier produces — outcomes, answer sets, optima, consequences, theory assignments, blame,
faults — is a **typed value**: the *model* of specification §1.5's model–view separation, called a value
here so that *model* keeps its logical sense, a model of the program (§5.1). Views for each consumer
class are derivations over the value: a human-centric `Display`, and a machine-centric
structured/serializable form for LLM agents, editor protocols, and audit. No operation's primary output
is prose, and no consumer parses rendered prose to act. `themelios-base`'s `Diagnostic` already carries
human/editor/machine views; the solve tier holds its faults and outcomes to the same discipline.

### 1.4 Engine-agnostic, and no user-facing solver configuration

The contract is engine-agnostic: an operation names *what* it asks of the program, never *which*
engine or *how* it searches. **Which engines an installation has is a build-time fact** (adapters
behind default features, specification §12.2); **which engine runs a given program is internal** —
the classifier's structure-driven routing (specification §1.1) or a build choice — never a user
knob. Engine *parameters*, where they must surface at all, are typed, per-operation, and optional
(§6.3), never a stringly-typed configuration object. By default a program author sees only the
abstract object and its questions.

---

## 2. Crates and the lean-core/facade boundary

### 2.1 The roster

The solve stage adds these workspace members, evolving specification §12.2:

| crate | unsafe | purpose |
|---|---|---|
| `themelios-solve` | forbid | The backend **contract**, the outcome vocabulary and its typed values, the agent and its driving surface, the fault taxonomy, the extension-surface traits (`@`-functions, propagators, extraction), the bridge seam, and the conformance suite. Engine-free. |
| `themelios-query` | forbid | The epistemic reading — three-valued `Answer`, `WorldView`, cautious/brave, bindings — over the program tier's patterns and the solve tier's outcomes. Engine-free. Its own design (`query.md`). |
| `themelios-potassco-sys` | allow (bindings only) | Vendored, pinned bindgen output over the libclingo and libclingcon C APIs. Regeneration is out-of-band. Feature-gated; never in a default build. |
| `themelios-potassco` | allow (the TCB) | The mechanism-only kernel over the bindings plus the safe adapters implementing the contract against clingo **and clingcon** — both first-class Potassco backends. Named for the engine *family* it adapts. |
| `themelios-macros` | forbid | Extended, at this stage, with the solve-adjacent macros (`scenario!`, `query!`, `#[external]`, `#[derive(Extract)]`, `#[derive(Facts)]`) as syntax-tier and constructor clients. |
| `themelios` | forbid | The facade: curated re-exports and prelude, adapters behind default features (disable them and the stack is FFI-free), and the witness examples executed on every change. |

The specification's separate `-clingo`/`-clingcon` adapter crates collapse to `themelios-potassco`
(§11.1), and the specification's "query in `-solve`" is split into the `-query` sibling (the
`analysis`:`program` symmetry, one tier up). Both changes are recorded in §16 as amendments to
specification §12.2.

### 2.2 Lean core, ergonomic facade — a module boundary, not a crate split

The contract (what an engine implements) and the driving surface (what a user touches) are *tightly
coupled* — a user wants both, a backend author wants the contract — so they are **modules of one
crate**, not two crates. `themelios-solve` is organised so the auditable heart is small:

- `contract` — the `Backend` trait and its capability, refusal, and fault vocabulary. This is the
  "one door" an audit reads; it is deliberately minimal (§4).
- `outcome` — the typed values and their views (§5).
- `agent` — the ergonomic driving surface over the contract: the `Agent` and the reasoning loop (§6).
- `extend` — the extension-surface traits and registration (§7–§9).
- `bridge` — the seam types the adapters implement against (§10).
- `conformance` — the executable suite every adapter passes (§13).

The lean-core property is that `contract` is small and points at the unsafe floor through a narrow,
enumerable interface; the ergonomic-facade property is that `agent` (and the top `themelios` crate)
compose over it. A crate split would be proliferation for a boundary a module already draws; putting
the driving surface only in the top facade would deny it to a client that composes its own crates
(an LSP server, say). The top `themelios` crate remains the *just-works* default — the abstract
solver with sensible defaults, wiring program → solve → adapter → outcomes.

---

## 3. The API experience: the centerpiece faces and how they compose

Five surfaces carry the tier's usability, and four of them are *centerpieces* in their own right —
the **macro API**, the **programmatic API**, the **`@`-function mechanism**, and the **propagator
interface** — with **extraction** the smaller fifth. This section states how they present across the
whole flow and, crucially, **how they interact**, because that is where the developer experience is
won or lost.

### 3.1 The two faces, author → drive → read

Every capability a user drives — authoring, driving, reading — is reachable two ways, both first-class;
the contract's own doors are the backend register (§2.2), which a client composing over `Backend` reaches
directly (§10.2):

- a **declarative macro face**, for the human writer, spelling ASP as the logician writes it; and
- a **composable programmatic face**, for humans *and* for programmatic consumers — a code generator,
  an LLM-driven consumer, a REPL — that build up programs, agents, requests, and queries by composition.

The two straddle the crate-home line by design. The *authoring* half lives one tier down — `rule!` /
`fact!` / `program!` in `themelios-macros`, the value builders in `themelios-program`'s `construct` —
because building the `Program` is a program-tier concern (LLVM's `IRBuilder` lives with the IR, not
the codegen). The *driving* half (the agent, its reasoning loop, options) and the *reading* half (query) are the
solve and query tiers'. The solve tier's obligation is therefore twofold: own its own faces (the
solve-driving and query macros and builders), and ensure the two faces **cohere end to end** — a
program authored through either face drives and reads through either face without a seam — because
"both faces, spanning the tiers" is exactly the examples-and-DX acceptance bar (§13.4).

### 3.2 How the centerpieces compose

The centerpieces are not silos; the design treats their seams as first-class:

- **Macro = sugar over programmatic (the tightest coupling).** By the macro law (specification §8),
  every macro expands to the *same* public constructor and registration calls — no second
  representation. The macro API *is* the programmatic API plus the one grammar's parser. Consequence:
  the programmatic surface's *completeness and regularity determine the macro surface's cleanliness*
  — a construct a macro cannot express as a trivial expansion is a gap in the programmatic surface,
  not the macro. The two produce structurally equal values, checked twin-against-twin (§13.4).

- **The conversion pillar is the shared hub.** Four conversions serve the tier, and getting them right
  pays out across every centerpiece at once; their homes differ, and are named precisely so a reader
  can reach each definition:
  - **`ToSymbol` / `FromSymbol`** (`program.md` §3.4) — a Rust value ⇄ a single ground `Symbol`, with
    the numeric-rounding adapters. Serves `@`-function arguments/results (§7) and per-atom extraction.
  - **`Facts`** (defined at §7.3 here; derived by `#[derive(Facts)]`) — the **bulk** analog of
    `ToSymbol`: a Rust value denoting a *set* of ground atoms. Serves construction of a sub-program's
    facts from Rust data (a code generator, a data-shredding client) and `@`-predicate results.
  - **`Extract`** (`program.md` §7.4 / §3.7; derived by `#[derive(Extract)]`) — the reverse of
    `Facts`: an answer set (or a projection of it) → a user-defined Rust value (§9).

  One machinery underlies all four (the same `Symbol`↔Rust codec), so a seam in it shows everywhere;
  the pillar is the meeting point of `@`-functions (§7), extraction (§9), and construction (program
  tier).

- **`@`-functions and propagators share the extension substrate.** Both register onto an agent; over a
  foreign engine, both cross its FFI seam through a panic-containing trampoline under its adapter's
  interning discipline (§10.5); both are engine-portable because both are the contract's (§7, §8). The
  "quarantined-unsafe floor, 100%-idiomatic safe surface" machinery is *one* thing serving both — and
  the theory atoms a propagator watches are authored through the very macro/programmatic faces of §3.1.

- **The outcome values are the meeting point.** The driving surface *produces* them, query *reads*
  them, extraction *views* them (the machine-view of §1.3), and a propagator *contributes* the theory
  assignment component to them (§5.4). The outcome vocabulary (§5) is the hub the other centerpieces
  plug into.

A short "how the centerpieces compose" map — this interaction graph — opens the doc's usage chapter,
so a reader meets the composition before the parts.

### 3.3 The Rust-exemplar bar

The whole surface is written to be a model of idiomatic Rust — invalid states unrepresentable,
`Result` and typed refusals rather than panics, `#[non_exhaustive]` on every payload that may grow,
ownership as the capability substrate (§6.1), no ambient state, and the deliberate refusal to abuse
`From`/`Into` for the conversion pillar. Where wrapping a stateful C engine tempts an un-idiomatic
shape — a god-object control, CLI-string configuration — the exemplar bar is the discipline that
refuses it, and the comparator witnesses (specification §3.1) hold it honest against clingo's Python
API.

---

## 4. The backend contract (the lean core)

### 4.1 The `Backend` trait and its capability declaration

A backend is an implementation of the `Backend` contract — the sole crossing between the engine-free
core and any engine (§4.3). The trait's shape, stated at the interface level (impl at build time):

```rust
// The REQUIRED engine primitives a backend author implements. The core (§4.2) provides the derived
// READINGS — consequences-by-enumeration and blame — as wrappers over these, so a backend author
// writes engine mechanism, never a derived reading. Each method's obligation is marked.
pub trait Backend {
    /// REQUIRED. What this backend can do — read before a request is paid for (§4.1).
    fn capabilities(&self) -> Capabilities;

    /// REQUIRED. Consistency and enumeration; the handle streams models lazily (§5.2). It enumerates
    /// `solve`'s model set, which ignores any objective (§5.2).
    fn solve(&mut self, req: &SolveRequest) -> Result<Solved<'_>, Fault>;

    /// The engine's cancellation primitive, to cut an in-flight solve short from another thread — `Some`
    /// iff `capabilities().cancellation`; the provided default answers `None`, so a non-cancelling backend
    /// inherits it and a cancelling one overrides. It is the one primitive the request-side time budget
    /// (§6.3) and `Agent::interrupt` — the core's handle over it (§6.2) — are realised over; the
    /// conformance suite (§13.1) checks `cancellation ⇒ interrupt().is_some()` so the bit cannot lie.
    fn interrupt(&self) -> Option<Box<dyn Cancel>> { None }

    /// REQUIRED. The bridge (§10): take a program through either door — a `Program`, or a parse admitted at
    /// Door A, read in source order or as the set `Door::program` lends (§10.2). On a **multi-shot**
    /// backend a repeat `lower` ACCUMULATES into the engine's program (the `assert` path lowers only the
    /// delta, §6.2), so a rebuild is `reset` then `lower` the amended whole. On a **single-shot** backend a
    /// `lower` REPLACES the program (each solve is independent; nothing accumulates), so a rebuild is one
    /// `lower` and `reset` is not called. A refused `lower` changes nothing, save where the engine refuses
    /// past the backend's own check of the door, which leaves the backend needing a rebuild (a backend's own
    /// state, below the trait).
    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault>;

    /// REQUIRED iff `capabilities().ground_program`. The observer, complete or absent by §10.4's law. The
    /// provided default answers `None`, so a backend that does not declare the observer writes nothing.
    fn ground_program(&self) -> Option<&GroundProgram> { None }

    /// REQUIRED iff `capabilities().optimization`. The proven optimum, improving trajectory iff asked (§5.3).
    fn optimize(&mut self, req: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        Err(Fault::unsupported(Capability::Optimization))
    }

    /// REQUIRED iff `capabilities().assumptions`. Solve under a scenario — the models of `solve`'s set
    /// (§5.2) the scenario admits; the core derives blame (`Refutation`, §5.4) over this — there is no
    /// separate backend blame method.
    fn solve_assuming(&mut self, s: &Scenario, req: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Err(Fault::unsupported(Capability::Assumptions))
    }

    // --- REQUIRED iff capabilities().multi_shot (provided defaults that refuse) ---
    /// A grounding that fails is never accepted: it leaves the backend needing a rebuild (a backend's own
    /// state, below the trait), and the agent recovers by its rebuild, replaying what it accepted (§6.2).
    fn ground(&mut self, parts: &[Part], opts: &GroundOptions) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::MultiShot))
    }
    fn assign_external(&mut self, ext: Symbol, v: TruthValue) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::MultiShot))
    }
    /// Clear the engine's accumulated program so the agent can REBUILD it (the rebuild-class retraction
    /// path, §6.2); `lower` then reloads the amended program. Only a multi-shot backend needs it: on a
    /// single-shot backend `lower` REPLACES the program (nothing accumulates), so a rebuild is one `lower`
    /// and `reset` is not called. Distinct from `assign_external` (a toggle) and `ground` (an addition).
    /// It discards the accumulated program and its groundings, leaves a backend that needed a rebuild ready
    /// for one (a backend's own state, below the trait), and keeps every registered `@`-function and
    /// propagator: the agent handed them over and cannot replay them, and a theory left unregistered would
    /// leave its atoms unconstrained — the silent degrade this section forbids — so an adapter whose engine
    /// resets to a fresh control registers them on it again.
    fn reset(&mut self) -> Result<(), Fault> { Err(Fault::unsupported(Capability::MultiShot)) }

    // --- REQUIRED iff the matching capability bit (functions / propagators); extension reg. (§7–§9) ---
    fn register_function(&mut self, f: Box<dyn Function>) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::Functions))
    }
    fn register_propagator(&mut self, p: Box<dyn Propagator>) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::Propagators))
    }

    /// OPTIONAL — override iff `capabilities().native_consequences == Native`. Absent, the core derives
    /// cautious/brave by enumeration over `solve` (unscoped) or `solve_assuming` (scoped) (§4.2); the
    /// request surface says which path runs. What the backend owes: a native door honours the
    /// `ConsequenceRequest`'s scenario, ranging over the models `solve_assuming(scenario)` denotes (§5.2);
    /// an unscoped request carries the empty scenario. It ranges over those models' answer sets, not their
    /// displays (§5.1): an engine whose consequence search tracks only the atoms it displays, or only the
    /// atoms a `#project` directive names, is driven with every atom tracked — given no restricting
    /// directive and no `#project` (§5.1, §5.2) — or does not declare the door (the pinned engine:
    /// `clasp/src/cb_enumerator.cpp`, `CBConsequences::doInit`; `clasp/clasp/shared_context.h`,
    /// `projectMode`). It reports what the engine's search established, a
    /// `NativeAnswer` (§5.2) — the set it computed over a space it closed having seen a model, no model
    /// over a closed space, or where it stopped short — and the core builds the `Consequences` from that
    /// answer or refuses: the refusals' decision and wording are the core's, the report's honesty the
    /// backend's. The agent surface produces a non-empty request through
    /// `Agent::cautious_assuming`/`brave_assuming` (§6.2 — the normative home of the scoped doors'
    /// precondition and cost).
    fn consequences_native(&mut self, mode: Mode, req: &ConsequenceRequest) -> Result<NativeAnswer, Fault> {
        Err(Fault::unsupported(Capability::NativeConsequences))   // provided default
    }
}

/// The engine's cancellation primitive (§6.3): one call cuts the in-flight solve short. `Send + Sync`, so
/// the core's timer thread and the caller's handle pull it from another thread. A pull signals and returns
/// — `O(1)`, never blocking on the solving thread — and the stop is read through the run's `conclusion`.
///
/// A solve is in flight from the moment `solve` is called until its run ends or its handle drops. A pull
/// inside that window is never dropped: the backend takes it at its next check, in grounding or in search
/// (§6.3), and the run concludes `Interrupted`. A pull outside it is a no-op — never a cancellation of the
/// next question. Pulls within one window are one pull: the core may forward a caller's pull twice — when
/// it lands, and again once the run opens (§6.3) — and a caller may pull more than once, so a primitive
/// that counts or toggles is wrong. An adapter over an engine whose own primitive cuts "the active call or
/// the following one" compensates with a slot of its own: it holds a pull for the whole window, forwards
/// it to the engine only while the engine's search is active — armed from before that search can begin
/// until it ends, and disarmed no later than it ends — and begins no search once it holds a pull. So a
/// pull inside the window is never dropped, and one outside it never reaches the engine. That coincidence
/// is a version-scoped claim the spike suite establishes for the pinned engine (§13.2), with the race
/// harness (§13.3) holding the concurrent open and close and the conformance suite the deterministic stale
/// pull (§6.3).
///
/// The primitive carries no authority over a dropped engine: a pull reaches a slot the backend owns and
/// clears on drop, shared with the handle, never a pointer into the engine — so the engine's lifetime is
/// the backend's, and a pull through a handle that outlives its backend is safe: it holds nothing and cuts
/// nothing.
pub trait Cancel: Send + Sync {
    fn cancel(&self);
}

/// A backend's declared capabilities. Closed set of bits/enums; a request beyond them is refused.
#[non_exhaustive]
pub struct Capabilities {
    pub enumeration: bool,
    pub optimization: bool,
    pub native_consequences: ConsequenceSupport,  // Native | DerivedByEnumeration
    pub theories: TheorySupport,                  // which theories this backend evaluates
    pub externals: bool,
    pub functions: bool,      // @-function evaluation
    pub propagators: bool,    // custom propagators
    pub multi_shot: bool,
    pub assumptions: bool,
    pub cancellation: bool,
    pub budgets: BudgetSupport,   // the budgets the backend enforces natively (the realisation rule, §6.3)
    pub ground_program: bool,     // the observer: the complete ground program, exposed (§10.4)
}

/// A declared capability, named — as a refusal names the one a request needed (`Fault::unsupported`,
/// §5.4) and a conformance report the one it checked (§13.1). Non-exhaustive, growing with `Capabilities`.
/// A refusal names one of the six whose method refuses — optimization, native consequences, assumptions,
/// multi-shot, functions, propagators; undeclared cancellation and the observer answer `None` instead, and
/// externals and the time budget name checks, never refusals: a budget nothing realises refuses with
/// `Presupposition::UnrealisableBudget` (§6.3).
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Capability {
    Optimization, NativeConsequences, Assumptions, MultiShot, Externals,
    Cancellation, TimeBudget, Functions, Propagators, GroundProgram,
}
```

A request beyond declared capability receives a **typed refusal** naming the capability it needed
(`Fault::unsupported`, §5.4), never a silent degrade. Cost note: `capabilities()` is `O(1)` and pure — it is
read *before* a request is paid for.

**The `enumeration` bit carries a soundness obligation, not merely a hint.** A backend that declares
`enumeration: false` (it decides consistency but does not enumerate the answer sets) must **never**
conclude `Conclusion::Exhausted` with models unseen — `Target` is the closed set's word for "stopped at
the witness." Every universal reading trusts `Exhausted`: `Solved::all_models` (§5.2), the derived
cautious/brave fold (§4.2), and query.md's `Snapshot` readings all treat an `Exhausted` search as the
whole space. A consistency-only backend that concluded `Exhausted` after one witness would make each of
them silently wrong, so §13.1's conformance suite checks the bit against this obligation; the exhaustion
gate then carries the reading — a universal reading refuses without a closed search (§5.2).

**Required versus provided — why the core stays lean.** The **trait-required** surface (no default) is
`capabilities`, `solve`, and `lower`. Every **capability-gated** method — `optimize` /
`solve_assuming` / `ground` / `assign_external` / `reset` / `register_*`, alongside `interrupt`,
`consequences_native`, and the observer `ground_program` — is a **provided default that refuses**
(`Err(Fault::unsupported(capability))`, naming its capability, or `None` for `interrupt` and
`ground_program`), so a minimal backend implements only the three and a declared-absent capability refuses
with no line written. The type cannot
force a *declared* capability's method to actually do the work
— a provided method needs no override — so that obligation is enforced by §13.1's **positive-capability
check**: a bit set `true` whose method still refuses is the capability lie the conformance suite exists to
catch, and `cancellation ⇒ interrupt().is_some()` likewise, as is `ground_program ⇒ Some` once a grounding
has finished (§10.4). (`interrupt` returns `Option` and defaults to
`None`, so a non-cancelling backend inherits it; `reset` is the tear-down the multi-shot rebuild path needs
given `lower` accumulates, §6.2, and is not called on single-shot backends.) The **core** provides, over that surface and *not* on the trait,
the two derived readings a backend author does not write: cautious/brave **consequences by
enumeration** when a backend lacks `consequences_native` (§4.2), and **blame** (`Refutation`, §5.4)
over `solve_assuming`. No smaller required set exposes consistency, enumeration, optimization, theory,
and multi-shot — each capability-gated method is the sole engine primitive for its witness, and
removing it discards essential structure, not accidental complexity — so this is the minimal contract
by construction, which is what keeps the one-door audit (§4.3) finite.

**A backend's own state.** This paragraph and the next are the law's home; the trait's doc comments, §6.2,
§10.4, and §13.1 cite them. A backend is **ready** — with nothing lowered, or a program lowered and, on a
multi-shot backend, any of its parts grounded, each method then doing what its own documentation states — or
it **needs a rebuild**, the one state that changes what every method does. Two events take it there, and only
two:

- **An engine refusal past `lower`'s check.** The backend checks the whole door before it adds a statement,
  so a refusal its check makes changes nothing; but the engine takes statements one at a time, and a refusal
  past the check leaves those before it in the engine. Such a refusal is the backend's own bug
  (`Fault::adapter_bug`) where its check should have caught it, and a Resource fault where the engine ran out
  of a resource partway — an allocation failure, which no check prevents (`clingo_program_builder_add`,
  `libclingo/clingo.h`).
- **A grounding that fails**, which is never accepted.

While a multi-shot backend needs a rebuild, every method that touches the engine's program or runs a search —
`lower`, `ground`, `assign_external`, `solve`, `solve_assuming`, `optimize`, `consequences_native`, and the
`register_*` doors — refuses with a Request fault naming that state (`Presupposition::NeedsRebuild`, §5.4),
while `capabilities`, `interrupt`, `ground_program` (`None`, §10.4), and `reset` answer; `reset` discards the
accumulated program and its groundings, keeps the registrations, and leaves the backend ready. A single-shot
backend has no `reset` and grounds within each solve (the phases, §6.3): a refusal its check makes leaves
the program lowered
before it, the backend ready; a grounding that fails fails its solve and leaves the lowered program as it was;
and an engine refusal past `lower`'s check leaves every other method that touches the engine refusing, as
above, until a `lower` replaces the program. The agent recovers by its rebuild (§6.2); a caller driving the
backend directly rebuilds what it still wants. Whether a multi-shot backend's search covers what is lowered
but not yet grounded the contract does not yet say: the agent and the conformance suite solve straight after
`lower`, and the first multi-shot adapter settles it with its engine in hand (§11.1).

The law is uniform by decision. A backend whose engine could undo a failed grounding still refuses until its
rebuild, so a client's recovery is one path on every backend — the rebuild the agent already performs — and a
client written against a backend that forgave does not break against one that cannot; the price is a flag such
a backend would not otherwise keep, and a rebuild it could have skipped. The pinned engine cannot forgive:
`ClingoControl::ground` opens its output step before it checks the program and throws with that step open on a
failed check, and past the check it streams the instantiation into the solver as it goes, so an `@`-function
that faults midway has emitted a prefix — state a fresh control alone clears
(`libclingo/src/clingocontrol.cc`, `ClingoControl::update` and `ClingoControl::ground`), a version-scoped
claim the spike suite holds (§13.2). The law does not rest on that claim: refusing until the rebuild is sound
whatever an engine keeps. The conformance suite holds what it can drive (§13.1) — a refusal the check makes
adding nothing, a failed grounding refusing until the rebuild, a registration surviving it — and cannot drive
an engine refusal past the check: a backend that passes has no bug there to drive, and no portable test runs
an engine out of memory.

### 4.2 Refuse-or-derive, disclosed before it is paid for

The core may **refuse-or-derive** deliberately: it can derive cautious consequences by intersection
when an engine lacks them natively. The derived-versus-native distinction is legible **at the request
surface, before the request is paid for** — `Capabilities::native_consequences` says which path a
request will take — because deriving consequences can cost enumeration, a different computational
beast (`Θ(|W|)` models folded) than one solve, and a cost divergence of that size disclosed only in
the receipt is the surprise this design exists to forbid. No outcome records which path ran: that
record (specification §9.1) is deferred until a consumer reads it (§16) — the declaration discloses the
path before the request, and nothing carries it after.

The contract does not assume the engine is foreign: a native backend built from foundation crates
implements the same trait, and the contract's shapes must not force conversions a shared-representation
backend would never need — **zetesis** (§12), which shares the program tier's `Symbol`, is the standing
check on this.

### 4.3 One door per boundary

The `Backend` trait is the sole crossing between the engine-free core and any engine. No tier reaches
around another; the bridge seam (§10) is how a backend admits a program and, where it exposes one,
emits its ground program, and it is part of the contract's surface (`lower`/`ground_program` above),
not a side channel. This is the microkernel "one door per boundary" (specification §12.3) in the tier's
own terms, and it is what makes the audit's job finite.

---

## 5. The outcome vocabulary (typed values)

### 5.1 The closed distinctions, and the model

```rust
/// The logical question: is the program consistent? Closed trichotomy — deliberately NOT
/// #[non_exhaustive], because the closed set is the affordance that forbids a fourth reading.
/// Read from a resolved solve via `Solved::determination` (§5.2); each variant carries its evidence.
/// The `Consistent` payload's world view is a borrowing view over the live engine (§5.2), so the
/// trichotomy carries that lifetime.
pub enum Determination<'a> {
    Consistent(Models<'a>),  // read the answer sets, or open the WorldView (§5.2) — a borrowing view
    Inconsistent(Unsat),     // carries blame (`Refutation`) for an assumption-scoped solve (§5.4)
    Inconclusive(Partial),   // a real value — what a truncated search DID establish; never "no"
}

/// The search question: how did the search end? Separate from the logical question by design.
pub enum Conclusion { Exhausted, Target, Budget, Interrupted }

/// The `Inconclusive` payload: how the search stopped short of deciding. `#[non_exhaustive]`, so it may
/// come to carry more of what the search established; its stopping reason is one closed shape.
#[non_exhaustive]
pub struct Partial { /* how the search stopped */ }
impl Partial {
    pub fn stopped(&self) -> Stopped<'_>;
}
/// How an inconclusive search stopped: at the truncation it concluded at, or at an engine fault, which
/// is no conclusion. Closed: a stopping reason is exactly one of the two.
pub enum Stopped<'a> { Concluded(Truncation), Faulted(&'a Fault) }

/// The conclusions short of the space — its target, its budget, or a cancellation. Closed, and without
/// `Exhausted`: an exhausted search decided, so no inconclusive search concluded there. Each is the
/// `Conclusion` of its name (a total `From<Truncation> for Conclusion`); `Solved::conclusion` reads the
/// four.
pub enum Truncation { Target, Budget, Interrupted }

/// An answer set, as the literature defines it: the ground literals true in a stable model of the
/// program — every atom the model makes true, a strongly negated atom included, so every member is a
/// function symbol (`Model::is_set_of_literals`) — and never what the program's `#show` directives
/// display (below). Re-exported from
/// the program tier (program.md §11.3), so the solve, query, and program tiers speak one answer-set
/// vocabulary.
pub use themelios_program::AnswerSet;   // = BTreeSet<Symbol>

/// One model of the program — the unit every stream yields and every complete collection holds: its
/// answer set; what the program displays of it by its `#show` directives, which the core derives (below);
/// and the theory assignment a backend evaluating theory atoms supplies with it (§5.4) — empty for a
/// backend that evaluates none. The readings read the answer set and only it (below). Cost: the answer set
/// by value; the display stored only where it differs from the answer set; and for a backend evaluating
/// no theory an empty assignment — so the constant-resident stream (§13.3) holds for the unit.
#[non_exhaustive]
pub struct Model { /* atoms: AnswerSet + the displayed terms + the derived display + theory: TheoryAssignments */ }
impl Model {
    pub fn of(atoms: AnswerSet) -> Model;             // the backend's construction door: no displayed term, no assignment
    pub fn with_terms(self, terms: impl IntoIterator<Item = Symbol>) -> Model;
        // the symbols the program's term directives display in this model — the half of the display only
        // an engine evaluates (below); O(|terms| log (|terms| + |M|)), and the union with the answer set,
        // O(|M| + |terms|), where some term is not one of its atoms
    pub fn atoms(&self) -> &AnswerSet;                // the answer set — what every reading reads
    pub fn shown(&self) -> Shown<'_>;                 // the display the core derives, a type of its own — no reading
                                                      // consults it
    pub fn assignment(&self) -> &TheoryAssignments;   // empty unless the backend evaluates a theory
    pub fn is_consistent(&self) -> bool;              // no atom beside its strong negation (query.md §2.3)
    pub fn is_set_of_literals(&self) -> bool;         // every member a literal, a function symbol; O(|M|)
    // Equality compares the display by content. The assignment-bearing construction door lands with
    // `TheoryAssignments`' own constructors, when a theory-evaluating backend is built (§11.1); until then
    // every model's assignment is empty.
}

/// A program's show rule — which of a model's atoms its `#show` directives display (below): every atom
/// where the directives in force include no restricting directive (a signature form, or `#show.`), else
/// the atoms of the signatures those list. A backend builds it from the directives it holds
/// (program.md §4.8's `Show`) and hands it to the core with its run (§5.2); the core applies it, so the
/// rule has one implementation and no backend applies it. O(d log d) over its d directives.
pub struct ShowRule { /* every atom, or the listed signatures */ }
impl ShowRule {
    pub fn of<'p>(directives: impl IntoIterator<Item = &'p Show>) -> ShowRule;
}

/// What a model displays: a view of its own type over the displayed symbols. It is not an `AnswerSet`, so
/// `Model::of`, `Consequences::fold`, and the readings, which take one, do not take it; its symbols reach
/// such a position only through `symbols`, written at the call — a discipline the type names rather than
/// a barrier it raises, since `AnswerSet` is an alias of the same set type (program.md §11.3).
#[derive(Clone, Copy)]
pub struct Shown<'m> { /* &'m BTreeSet<Symbol> */ }
impl<'m> Shown<'m> {
    pub fn symbols(self) -> &'m BTreeSet<Symbol>;     // the displayed atoms and terms
    pub fn contains(self, symbol: &Symbol) -> bool;
}
```

The two names owe their §1.4 reason, stated here: engines' own result vocabularies conflate the
logical question (*is the program consistent?*) with the search question (*did the search finish?*);
these names separate what the engines confuse, and Rust's own `Result` forecloses the obvious
alternative. The names are argued, not inherited: a clearer pair discovered at design time supersedes
them by satisfying §1.4 in its turn. The `Determination` variants are closed; their *payloads* are
`#[non_exhaustive]`.

An `Inconclusive` search's `Partial` says how it stopped, as one closed shape: `Concluded` at the
truncation it reached short of the space — `Target`, `Budget`, or `Interrupted`, which names cancellation
alone — or `Faulted` at an engine fault, which reached no conclusion. A fault is not a way a search
concludes, so the closed `Conclusion` gains no word for it, and the stopping reason is a sum, never an
optional conclusion beside an optional cause. `Concluded` carries a `Truncation`, the conclusions short of
the space, so an exhausted conclusion — which decides — is unrepresentable there, not merely never
produced.

**How a stop is classified.** A run ends in exactly one way, and the contract classifies each way by its
cause, the same before the first model as after models. Each cell gives the determination, then the
conclusion:

| What ends the run | Before any model | After models |
|---|---|---|
| The search closes the space | `Inconsistent`, `Exhausted` | `Consistent`, `Exhausted` |
| The request's time budget passes (§6.3) | `Inconclusive`, `Budget` | `Consistent`, `Budget` |
| The caller's interrupt is pulled (§6.3) | `Inconclusive`, `Interrupted` | `Consistent`, `Interrupted` |
| A target the request set, or a deciding backend's witness (§4.1) | — | `Consistent`, `Target` |
| A limit of the backend's own, or an allocation failure | a Resource fault | that fault, the last item |
| A statement outside the backend's language | a Program fault naming it | that fault, the last item |

- **A fault before any model** is the solve's `Err`, or the run's first item, which reads `Inconclusive`
  with the fault as its cause (`Stopped::Faulted`).
- **A fault after models** leaves the reading `Consistent`, since a model was seen. The run has no
  conclusion, and a complete collection drawn over it refuses (`NotExhausted`, the fault its cause).
- **No reading turns a fault into a closed space.** A fault never becomes an exhausted enumeration or a
  complete `Snapshot` (§5.2, query.md §2.3).
- **A cut is never a fault.** A deadline or a pull that ends a run during its grounding, before any model,
  concludes it `Budget` or `Interrupted` (§6.3).

**`Budget` and `Target` are the request's words, and no other limit borrows them.** `Budget` names the
request's acknowledged time budget alone, whether the backend enforces it natively (`budgets.time`) or the
core's timer does (§6.3). `Target` names a target the request set, or the witness a deciding backend stops
at: its `enumeration: false` declares that stop before the request is paid for (§4.1).

A backend's own ceiling — a grounding size, a work count, a storage bound, the width of a representation —
is configured outside the request. It is environmental, as an allocation failure is. A stop at either is a
Resource fault (`Fault::resource`) carrying the engine's typed cause (`Fault::caused_by`), which a caller
reads by downcasting `Error::source`, never by parsing the message. Calling such a stop `Budget` would
claim the caller set a limit the request never carried. If a configured limit should read as a budget, it
becomes a field of the request, with a declaration that discloses its enforcement as `budgets` does
(§4.1, §6.3). That field is grown when a consumer needs it (§6.3, §14). The request carries no target yet
(§6.3 leaves room for a model-count cap), so an enumerating backend's run never concludes `Target`. An
engine's habit of stopping after one model is a target nobody set, and a backend does not carry it into
`solve`.

`Model` owes its §1.4 reason too: an answer set is the atoms alone, while a stable model of a program with
theory atoms comes with the assignment that satisfied them — the constraint-ASP literature's *constraint
answer set* — and the tier names the pair for what it is, a model of the program: the stream's unit is the
model, and a backend that evaluates no theory yields models whose assignment is empty. The model owns its
answer set, so it outlives its run — the `Send` `Snapshot` is built from models (query.md §2.3) — and a
backend pays one conversion per model into it: a traversal of the engine's answer, and the owned, ordered
set's construction, `O(|M| log |M|)` symbol comparisons, which the scaling benches measure (§13.3). A
native engine fills it straight from its own catalog, with no intermediate tree, its own identifiers
staying its own (§10.5).

**The answer set and the display are different things, and only the answer set is read** — this
paragraph is the law's home, and every other site cites it. A program's `#show` directives say what to
*display* of a model, and the semantics are the language's as the pinned authority implements them: the
**restricting directives** — the signature forms, and `#show.`, which lists none — limit the atoms
displayed to the signatures they list, a strongly negated signature listed in its own right
(`#show -p/1.`), and with no restricting directive every atom is displayed; beside either, a term
directive displays its term wherever its body holds, restricting nothing
(`libgringo/src/input/programbuilder.cc`: `showsig` turns on the signature filter of
`OutputPredicates::add`, `libgringo/gringo/output/output.hh`, while `show`, the term form, adds a
statement and leaves the filter alone — a version-scoped claim the spike suite holds, §13.2). A displayed
term need not be a true atom, or an atom at all: `q. #show p : q.` displays `p`, which is false, and
`q. #show 42 : q.` displays a number. The display is therefore neither a subset of the answer set nor a
reading of it. A reading over it would answer *yes* of a false atom, and *unknown* of an atom the program
hides yet entails (`a. #show.`), or of the contrary of a hidden strongly negated atom (`-p. #show q/0.`).
So every reading reads the answer set — every true atom, displayed or not — and the display, a type of
its own (`Shown`), rides beside it for the client that prints or exports what the program shows. It is a
set: where an engine lists a displayed atom and an equal displayed term both, it holds the symbol once.
The law holds at every door, because both doors carry a program whose directives the backend reads
(§10.2); an adapter that also ingests its engine's own format owes the same answer set from it, or refuses
the input (§10.3).

`AnswerSet` stays an alias of the set type (program.md §11.3) rather than a type of its own that admits
only literals. A type of its own would raise both laws above — an answer set is no display, and holds
only literals — from disciplines to barriers, at the price of a change to a public type every backend
and client constructs, across the tiers that share it; the alias keeps the set's whole interface at no
cost. So the two laws stay disciplines, and this is their accepted residue: a display's symbols reach an
answer-set position only through `Shown::symbols`, written at the call, and literal membership is checked
by `Model::is_set_of_literals` at `materialize` and in the conformance suite. Those checks — every member a
literal, no atom beside its contrary — are necessary, not sufficient: no check of a set's content
establishes that it is an answer set of the program. That is the backend's semantic obligation, held by
the conformance suite's corpus and the differentials (§13.1, §13.2).

**The display's two halves have two homes.** The restricting half is a filter over the answer set, the
`ShowRule`, and the core applies it: a backend hands the core the rule of the directives it holds when it
builds its run (`Solved::running`, §5.2), and the core derives each model's display as the model streams,
from its answer set, the rule, and the model's terms — so the filter has one implementation, a display
cannot disagree with the rule it was derived from, and a backend writes engine mechanism, never a derived
reading (§4.1). Only the term half needs a grounder — a term directive's body is grounded as a rule's is
— so it is the backend's alone: its engine evaluates the terms each model displays, which the backend
supplies with `Model::with_terms`; a backend that cannot evaluate term directives refuses a program
carrying one at `lower`, with a Program fault naming the directive (§5.4). The directives in force are
those lowered and not since `reset`, whatever their part: the pinned authority reads a restricting
directive when it parses it (`libgringo/src/input/programbuilder.cc`, `showsig`), and grounds a term
directive with its part. A program without directives displays its answer set and its models store
nothing more; a model built outside a run displays its answer set and its terms, a display the core's
derivation reuses rather than derives again — kept where the rule restricts nothing, filtered in place where
it does. An adapter that lowers
none of the restricting directives, and no `#project` (§5.2), leaves every atom in its engine's
consequence search (§4.1).

### 5.2 Answer sets, optima, consequences

```rust
/// `solve` returns this borrowed handle. It resolves the trichotomy, streams the models, and —
/// on a consistent search — yields the borrowing live `WorldView<'_>` the query tier reads (an
/// owned-engine `WorldView<'static>` from a single-shot solve; an engine-free `Snapshot` via
/// `materialize`; §6.4, query.md §2.3).
impl<'a> Solved<'a> {
    /// INSPECT the run in place (reborrow): read the trichotomy, then `conclusion` after drain; the
    /// `WorldView` reachable via this reborrow is bounded by it. For a view that outlives to the agent's
    /// borrow, use the consuming resolver below.
    pub fn determination(&mut self) -> Determination<'_>; // §5.1 — the trichotomy, with its payload

    /// RESOLVE the run into a readable outcome (consuming), threading the engine borrow `'a` — so the
    /// `WorldView<'a>` reached through `Consistent` (§5.2) outlives to the agent's borrow. This is the
    /// resolver the agent/bare `determination` conveniences (§6.2/§6.4) build over.
    pub fn into_determination(self) -> Determination<'a>;

    /// Lazy stream; each item a Result, so a mid-stream engine fault surfaces at `?`, not as a clean
    /// end. Iterated by `&mut` so the terminal `conclusion` is readable after drain — the stateful-drain
    /// reason `Solved` reads by `&mut`, as a `WorldView`'s `members` stream does (query.md §2.3), with no
    /// interior mutability. Cost: O(1) resident.
    pub fn models(&mut self) -> impl Iterator<Item = Result<Model, Fault>> + '_;

    /// A COMPLETE collection — available ONLY when the search closed the space; refuses otherwise
    /// (the exhaustion gate, the `WorldView::is_exhausted` analog), a faulted search's refusal carrying
    /// the fault as its cause; a refusal that becomes a fault keeps its kind — a handle whose models were
    /// already streamed `Presupposition::Taken`, a search that stopped short `Presupposition::Unclosed`
    /// with its truncation (§5.4). This is what makes "a truncated search passing as complete"
    /// unconstructible (§5.3), not merely visible via `conclusion`. One model per stable model the
    /// engine enumerates, so no two models are merged because they display alike — `{a}. #show.` has
    /// the two models `∅` and `{a}`, each displaying nothing — and the differentials compare the
    /// collection as a multiset (§13.2).
    pub fn all_models(&mut self) -> Result<Vec<Model>, NotExhausted>;

    /// How the search concluded — `Some` once it ended without a fault; `None` while it is open, and
    /// after a fault, which reached no conclusion (the determination's `Stopped::Faulted` carries it).
    pub fn conclusion(&self) -> Option<Conclusion>;
}

/// A backend constructs the `Solved` that `Backend::solve` (§4.1) returns through `Solved::running`,
/// handing the core its own lazy enumeration as a `Run` — the backend-facing streaming protocol, the seam
/// a native engine (§12) implements. Obligations: **fused** (once `next_model` yields `None` or a fault
/// it stays ended); a **terminal `Conclusion` once the stream ends without a fault**, while after a fault
/// `conclusion()` stays `None` — the search reached no conclusion, and the core records the fault as the
/// cause; a completeness drain stops at the first fault. A run that ends with neither — no fault and no
/// conclusion — breaks the protocol, and the core records an Adapter fault in its place, the bug bit set: the
/// completeness refusal's cause and the run's `Faulted` stopping reason. The
/// **core owns classification** — it resolves `Consistent` iff the run WITNESSED a model — so a backend
/// supplies only enumeration plus a terminal conclusion and **cannot forge
/// `Consistent`** (§5.1). No `Send` bound (a run may hold a raw engine handle whose control is
/// single-threaded), so the `Solved`/`Models`/live-`WorldView` handles built over it are `!Send` and
/// `Snapshot` is the `Send` form (§6.1). Cost: `O(1)`.
///
/// An engine that computes its complete family of answer sets at once hands it over through this same
/// protocol: it streams the family's members and concludes `Exhausted` — an empty family yields none and
/// concludes `Exhausted`, which the core resolves `Inconsistent` — while a search stopped short concludes
/// at its `Truncation`, and a failure is a fault. So no backend reports exhaustion its search did not
/// reach. The family it holds is the engine's; the core's handle still holds one model at a time (below).
pub trait Run {
    fn next_model(&mut self) -> Option<Result<Model, Fault>>;            // stream; None ends it
    fn conclusion(&self) -> Option<Conclusion>;                          // once ended without a fault
}
impl<'a> Solved<'a> {
    pub fn running(run: Box<dyn Run + 'a>, scenario: Scenario, show: ShowRule) -> Solved<'a>;
        // the backend construction door: its enumeration, the scenario it ranged over, and the show rule of
        // the directives it holds — the core derives each streamed model's display from it (§5.1)
}

/// The `Consistent` payload (§5.1): read the models, or open the live `WorldView` the query tier
/// reads. From a RETAINED agent the world view is a BORROWING handle `WorldView<'a>` over the live engine
/// — lazy, its one engine-driving read the fallible `members` stream (query.md §2.3) — so it borrows for
/// its lifetime and does NOT outlive that borrow. The cautious/brave native door and the epistemic
/// readings (`answer`/`bindings`/`entails`) live on the AGENT (§6.2) and on the materialised `Snapshot`,
/// NOT on the live handle (query.md §2.3–§2.6), so a reading is a fresh solve rather than a drain of this
/// stream. An owned, engine-free `Snapshot` (to cross a service boundary, and the home of the infallible
/// readings) is `WorldView::materialize` (query.md §2.3). The single-shot bare form owns its ephemeral
/// engine instead (a live `WorldView<'static>`, §6.4).
pub struct Models<'a> { /* the live-run-access handle, owned or borrowed (below) */ }

/// A PROVEN optimum — no public constructor; it exists only because the solver proved it.
pub struct Optimum { /* levels, in the objectives' own terms */ }

/// A step of the improving trajectory: one model the search found and retained, with its levels in the
/// objectives' own terms — both of one completed result, published only once the backend has retained
/// it. Not proven optimal, and never convertible into an `Optimum`, so a best-found cannot pose as proven
/// (§5.3). No public constructor; its construction door lands with `optimize`'s, beside `Optimum`'s,
/// where the levels' type is fixed for both. Cost: one model conversion (§5.1) at the step it is
/// published; the trajectory streams under the laziness law, one incumbent resident (§13.3).
pub struct Incumbent { /* an owned Model + its levels, one completed result */ }
impl Incumbent {
    pub fn model(&self) -> &Model;   // the retained model, its whole answer set (§5.1)
    // its levels, read as an `Optimum`'s are — the type lands with `optimize`
}

/// The optimization run handle — the same *resolution* register as `Solved`
/// (`determination`/`into_determination`/`conclusion`), specialised with `optimum`/`trajectory` in place
/// of the plain-enumeration accessors, so `optimize` is a sibling of `solve`, not a second vocabulary
/// (§6.4). Its optimal answer sets / world view are read through the resolved `Determination`'s
/// `Consistent` world view, ranging over the OPTIMAL set (§5.2) under the optimum-proven/exhausted gate.
impl<'a> Optimized<'a> {
    pub fn into_determination(self) -> Determination<'a>;   // resolve (consuming) — Consistent ranges over the optimal set
    pub fn determination(&mut self) -> Determination<'_>;   // inspect in place
    pub fn optimum(&self) -> Option<Optimum>;               // the proven optimum, once proved
    pub fn trajectory(&mut self)                            // the improving sequence — Some iff the request asked (§5.3)
        -> Option<impl Iterator<Item = Result<Incumbent, Fault>> + '_>;
    pub fn conclusion(&self) -> Option<Conclusion>;
}

/// Cautious (⋂) or brave (⋃) consequences — a set of ground `Symbol`s carrying the `Mode` that produced
/// it and, under an objective, whether it ranged over the OPTIMAL set or all stable models (query.md
/// §2.4, §5.2). Built by the fold or by the core's gate over a native door's answer (below): a backend
/// reports, and never builds one. Both range over the models' answer sets, never their displays (§5.1).
#[non_exhaustive]
pub struct Consequences { /* Symbol set + Mode + the optimal-vs-all marker */ }
impl Consequences {
    /// The fold, exposed as a primitive: ⋂ (Cautious) or ⋃ (Brave) over the given members — `None` over
    /// none, since no world view is empty (query.md §2.3) and a consequence set over no models certifies
    /// nothing. Nor does it certify completeness: the gated readings are the agent's `cautious`/`brave`
    /// (§6.2) and a `Snapshot`'s (query.md §2.4), each folding a world view whose search closed the space
    /// — the `Snapshot`'s discharging the `Option` on its non-emptiness.
    pub fn fold<'m>(mode: Mode, members: impl IntoIterator<Item = &'m AnswerSet>) -> Option<Consequences>;
}
pub enum Mode { Cautious, Brave }

/// What the engine's own consequence search established (§4.1 `consequences_native`) — the raw material
/// the core builds `Consequences` from, as a `Run` is for a solve: the backend reports, the core gates.
/// Closed: a native search closed the space having seen a model, closed it having seen none, or stopped
/// short of it.
pub enum NativeAnswer {
    Closed(BTreeSet<Symbol>),   // the engine's ⋂ or ⋃ of the answer sets over the space it closed, a model seen
    NoModel,                    // the space closed with no model: the scenario admits none
    Stopped(Truncation),        // short of the space; the engine's partial set is not carried
}
```

`Models` is a two-form value: owned, from the consuming resolver `into_determination`, or borrowed, from
the inspecting `determination(&mut self)`; its lifetime is the access, `'static` when the run owns an
ephemeral engine. The `Models<'a> → WorldView<'a>` transition lives on the query side (query.md §2.7): a
`WorldView` is constructed from a resolved `Consistent(Models)` via
`themelios_query::WorldView::of(models)`, so `themelios-solve` does not depend on `themelios-query`.
`Models` exposes the live-run material a world view drives — the `members` stream, the exhaustion-gated
`all_members` (the gate `WorldView::materialize` goes through, query.md §2.3), `is_exhausted`, and
`scenario` — with no engine, no backend access, and no `world_view()` method of its own. `Solved` and
`Models` are **`!Send`**, because the `Run` trait object they hold carries no `Send` bound (a run may hold
a raw engine handle whose control is single-threaded); the owned, engine-free `Snapshot` (query.md §2.3)
is the `Send` form, which is what `materialize` is for — §6.1's service posture crosses a boundary through
a `Snapshot`, not a live handle.

- Models are **owned, streamable** values — the lazy `Result`-iterator above. Cost: streaming
  enumeration is **constant in resident set on the owned side** — the core's handle holds one model at a
  time and never pulls ahead of its consumer, which the laziness law asserts (§13.3). What the engine
  holds to produce the next model — a lazy search's state, or the whole family an engine computed at once
  — is the engine's, as the huge ground instantiation is, in its compact internals; the owned no-sharing
  tree is the authoring form.
- A **proven optimum** is typed distinct from best-found: `Optimum` has no public constructor, the
  improving trajectory yields `Incumbent`s — best-found levels, a type of their own — and an optimum
  reports its levels in the terms the objectives were written in (a maximized level shows what was
  maximized, not the negation the engine optimizes internally). "All optimal solutions" is available
  only when the search closed the whole space, and says so: a proven optimum establishes the optimal
  levels, not that every model tied at them was enumerated.
- **Each question fixes the model set its answers range over** — the model-set law, whose normative
  home this is; other sections cite it. `optimize` asks for the optimal answer sets, so its world view
  and consequences range over the optimal set, those tied at the proven optimum, under the
  optimum-proven/exhausted gate. `solve` asks for the stable models with any objective **ignored** —
  every answer set, as if the program had none — and with any projection ignored too: a `#project`
  directive asks for projective enumeration, which this tier reserves (§14), so `solve` yields every
  stable model whatever the program projects, and an adapter keeps the directive from an engine whose
  consequence search it would narrow (§4.1). So does `solve_assuming` under its scenario, so
  every reading built over them, the agent's `cautious`/`brave` and query.md's `AgentReading`
  included, ranges over all stable models (with no objective the two sets coincide). A reading that
  wants the optimal set asks `optimize`; an objective the question did not ask about never narrows the
  stable models silently. This departs from an engine whose default is to optimise a program with an
  objective — clingo's among them — so a backend over one owes the objective-off enumeration, which
  the conformance suite checks (§4.1, §13.1). Until optimization is realised every world view ranges
  over all stable models by construction, and the optimal set's world view, the marker saying which set
  a value ranges over, and the native door over the optimal set are one seam that lands with
  `optimize`: the derived door's world view comes from a handle that fixes its model set, so its
  carrier grows inside the core; the native door has no handle, so it will owe a `ConsequenceRequest`
  field naming the set — additive, the request being `#[non_exhaustive]` — and the core stamps the
  marker on the value it builds.
- **Cautious and brave consequences** are typed sets carrying the semantics that produced them, so a
  value that has travelled still says which question it answers. They are not answer sets, and carry
  their own type (`Consequences`) for that reason. **They range over the model set of the question
  that produced them** (the law above), and carry which: the *derived* door folds that question's
  world view, and the **native door (`query.md` §2.4) is obligated to compute over the same set**, so
  both doors target the same model set and their required agreement (`query.md` §2.4) is meaningful
  rather than a silent both-wrong. (Whether the pinned engine computes cautious-over-optimal in one
  solve is measurement, §13.2; the *obligation* is stated here.)
- **The native door's answer is gated by the core**: a `Closed` set becomes the `Consequences` in the
  mode asked; `NoModel` refuses as the derived door refuses an inconsistent program
  (`Presupposition::NoAnswerSet`, §5.4) — `⋂`/`⋃` over the empty world view is undefined, not `∅`; and
  `Stopped` refuses as a truncated search does (`Presupposition::Unclosed`, with its truncation) — a native
  cautious search stopped early has converged on a *super*set of `⋂`, not the consequences. The
  refusals' decision, presupposition, and wording are the core's, so the two doors refuse alike as they
  answer alike (query.md §2.4). Unlike a `Run`, whose models the core pulls and so witnesses, a native
  answer is the backend's report, which the core cannot check: its honesty is the backend's obligation,
  held by the conformance suite (§13.1) and the native-versus-derived differential (§13.2) — a guarantee
  test-borne, not structural (§5.3).

### 5.3 The pathologies are unconstructible

Request types distinguish "enumerate answer sets" (`SolveRequest`) from "optimize, reporting the
improving trajectory" (`OptimizeRequest`), and the improving trajectory is available when — and only
when — the request asked for it. The three named solver pathologies (specification §5.1) are
**unconstructible in the vocabulary**, not merely tested against, each by its own structural device:

- *enumeration reporting an optimization's improving sequence* — distinct `SolveRequest` /
  `OptimizeRequest` and distinct `Solved` / `Optimized` outcomes; the trajectory exists only on
  `Optimized`.
- *a truncated search passing as a complete collection* — a "complete collection" is reachable ONLY
  through `Solved::all_models` (§5.2), which is **exhaustion-gated and refuses without a closed
  search** (the `WorldView::is_exhausted` analog); the streaming `models` never claims
  completeness, so a truncated search cannot be laundered into "all answer sets."
- *contradictory termination flags* — one `Conclusion`, orthogonal to `Determination`, with no second
  flag to disagree with.

`Optimum`'s absent public constructor closes the last gap, and the trajectory's own `Incumbent` keeps it
closed: a best-found is typed apart from a proven optimum, so it cannot pose as one. The
conformance suite (§13.1) still *attempts* each, but the guarantee is structural, not test-borne.

### 5.4 Theory assignments, blame, faults

```rust
/// Theory (constraint) assignments — a DISTINCT typed component of the outcome, never laundered
/// into Herbrand-looking atoms. Carries the wider-than-i32 constraint values (see below).
#[non_exhaustive]
pub struct TheoryAssignments { /* per-variable typed constraint values */ }

/// The `Inconsistent` payload (§5.1). For an assumption-scoped solve it answers blame.
pub struct Unsat { /* … */ }
impl Unsat { pub fn blame(&self) -> Option<Refutation>; }  // Some iff the solve was assumption-scoped

/// Assumption blame — which assumptions are responsible for a scenario's inconsistency. The culprit is
/// a raw set of assumptions (the literature's notion), NOT a named `Scenario` (§6.3).
pub enum Refutation {
    These(Box<[Assumption]>),   // this minimal subset of the scenario's assumptions is responsible
    NotThese,                   // inconsistency is independent of the assumptions
    NoAssumptions,              // the program is inconsistent with none assumed
}

/// A fault is a value with a CLOSED locus taxonomy at the seam, the loci naming where a fault lies. It OWNS
/// its model — a message, the `Locus`, the backend-bug bit, what it refused, and optionally a typed cause
/// (below). What it refused is one of five, a closed sum keyed by locus (`Refused`): a STATEMENT — a
/// Program fault refusing a statement, with its provenance (program.md §6), located iff that statement
/// carries a parsed origin, written in source or transformed from one, while a statement built in Rust
/// carries none; a PART — a Program fault refusing a part of the program a backend does not admit (§6.3),
/// named by its key and never located, since a part keeps no provenance; a PARSE — a Program fault refusing
/// a parse at Door A, its refusal `NotAdmitted` carried whole (§10.2) and located at each of its
/// diagnostics; the REQUEST — a Request fault, naming the presupposition of the request that failed
/// (`Presupposition`); or NOTHING — every Resource, Engine, or Adapter fault, whose locus and bug bit are
/// its typed distinction. So `Locus::Program` holds exactly when a fault refused a statement, a part, or a
/// parse — the program, as written or as built, is where it lies — `Locus::Request` exactly when it refused
/// the request, and a fault is unlocated where it carries no span: it refused the request, nothing, a part,
/// or a statement built in Rust. An unlocated fault is NOT a degenerate diagnostic with a fabricated span
/// at an "unknown source" but a different thing (base's §diagnostic): it renders through its own `Display`.
/// Equality compares the message, the locus, the bit, and what was refused: a refused statement by its
/// content and its origins — two Program faults refusing content-equal statements at different locations
/// are different faults, which lower to different diagnostics, while a difference in annotations alone (a
/// doc comment, a label) is not — hand-written, since the derive would compare content alone (program.md
/// §6.2); a refused part, by its key; a refused parse, by its diagnostics; a refused request, by its
/// presupposition; and never the cause, which is detail, not identity. `Fault` is not `Hash`: a fault is a
/// report, not a key — no consumer keys on one — and its opaque cause carries no hash. A fault's clone is
/// `O(statement)`, `O(part key)` for a refused part, or `O(diagnostics)` for a refused parse, the cause
/// shared.
#[non_exhaustive]
pub struct Fault { /* message + Locus + what it refused + bug bit + optional cause */ }
pub enum Locus { Program, Request, Resource, Engine, Adapter }

/// What a fault refused — closed, one of five, keyed by locus (above). Closed on purpose: a row grows a
/// typed reason inside the sum when a consumer first reads one — the `Statement` or `Part` row, a router's
/// reason why a backend refused the statement or the part (§12, §14) — a breaking change accepted before
/// 1.0, never a reason held beside it.
pub enum Refused<'a> {
    Statement(&'a WithProvenance<Statement>),   // a Program fault refusing a statement
    Part(&'a PartKey),                           // a Program fault refusing a part, unlocated (§6.3)
    Parse(&'a NotAdmitted),                      // a Program fault refusing a parse at Door A (§10.2)
    Request(Presupposition),                     // a Request fault: the presupposition that failed
    Nothing,                                     // a Resource, Engine, or Adapter fault
}

/// Why a request was refused: a presupposition of it that fails, named so a consumer acts on it by matching —
/// retrying under a larger budget, routing to another backend, rebuilding — never by reading the message.
/// Each variant is a refusal the tier makes, at the site cited. Non-exhaustive: a request refused for a
/// reason not yet named gains a variant, never a message to tell it by.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presupposition {
    Unsupported(Capability),   // a capability the backend does not declare, one whose method refuses (§4.1)
    NeedsRebuild,              // the backend refuses until its rebuild (§4.1, a backend's own state)
    NoAnswerSet,               // a reading over no model: the program, or its scenario, admits none (§5.2)
    Unclosed(Truncation),      // a reading that needs a closed space, over a search stopped short (§5.2)
    Taken,                     // a complete collection, from a handle whose models were streamed (§5.2)
    NotLive,                   // a statement handle naming nothing live in this agent's knowledge (§6.2)
    Spent,                     // an observation already forgotten, or another agent's (§6.2)
    NotAnAtom,                 // an observed fact that is not an atom (§6.2, §7.3)
    NotExternal,               // a truth value assigned to an atom that is not external (§6.2)
    UnrealisableBudget,        // a budget the backend neither enforces nor lets the core enforce (§6.3)
}
impl Fault {
    // The construction doors, one per locus; an empty message is replaced, so a fault always has one.
    pub fn program(message: impl Into<String>, statement: &WithProvenance<Statement>) -> Fault;
        // keeps the refused statement whole, O(message + statement), so no span is made up and no
        // statement goes unnamed; a refused parse's door is `From<NotAdmitted>` (§10.2)
    pub fn program_part(message: impl Into<String>, part: &PartKey) -> Fault;
        // a Program fault refusing a part of the program a backend does not admit (§6.3), named by its
        // key, O(message + part key); unlocated, since a part keeps no provenance — never a member
        // statement
    pub fn request(message: impl Into<String>, presupposition: Presupposition) -> Fault;
    pub fn unsupported(capability: Capability) -> Fault;   // a Request fault, `Presupposition::Unsupported`,
                                                           // its message naming the capability
    pub fn resource(message: impl Into<String>) -> Fault;
    pub fn engine(message: impl Into<String>) -> Fault;
    pub fn adapter_bug(message: impl Into<String>) -> Fault; // the one door that sets the backend-bug bit
    pub fn caused_by(self, cause: impl std::error::Error + Send + Sync + 'static) -> Fault;
        // attaches the engine's own typed failure, shared and opaque: `Error::source` returns it, for the
        // caller who downcasts it; no engine type in the signature, and not part of equality
    pub fn is_backend_bug(&self) -> bool;                  // a closed bit
    pub fn locus(&self) -> Locus;
    pub fn refused(&self) -> Refused<'_>;                  // a statement, part, or parse; the request; nothing
    pub fn diagnostics(&self) -> Vec<Diagnostic>;
        // its lowering to base diagnostics: none for an unlocated fault, a refused part among them; one for
        // a refused statement with a parsed origin — the least such origin its primary label, any others
        // secondaries (program.md §6.3); one per diagnostic for a refused parse. O(statement), or
        // O(diagnostics)
}
impl std::fmt::Display for Fault {}                        // Fault is Display + Error — NOT ToDiagnostic
```

- **Theory results — constraint assignments — are a distinct typed component** of the outcome, beside
  the answer set, never laundered into Herbrand-looking atoms. This separation is load-bearing: it is
  what a CP or DL client reads back cleanly (laundering theory values into Herbrand-looking atoms is
  where a client's name-collisions come from), and it is where the `i32`/wider-integer question
  resolves — program literals stay `Symbol` (`i32`), constraint assignments are a wider solve-tier
  typed value. **The component is rich enough for a full CP theory** (§8.2): a global constraint's
  domain values (`&dom`), a sum's result (`&sum`), and an `alldifferent`'s witness assignment are all
  read back here as typed data, not just difference/sum scalars.
- **Assumption blame** — when a scenario is inconsistent, which assumptions are responsible is an
  answerable, typed question (`Refutation` above), scoped by the scenario it ranged over.
- **Faults** are values with the closed locus taxonomy above, with "is this a backend bug" a closed bit.
  Today the bit and the `Adapter` locus coincide — every adapter-locus fault is a bug, such as a model
  holding an atom and its contrary (query.md §2.3); the bit is kept apart because the specification mandates
  it (§9.3) and the case that parts them is real: an engine- or resource-locus failure the adapter should
  have prevented. A fault lowers to `themelios-base` diagnostics through `Fault::diagnostics` — one for a
  refused statement with a parsed origin, one per diagnostic for a refused parse, none for an unlocated
  fault (loci and provenance, solved once, here, for every consumer) — while an unlocated one renders
  through its own `Display`: a fault is not, in general, a diagnostic. A Program fault refers to its source
  whether or not it is located — the statement it refused, the part, or the parse — and a Request fault
  names the presupposition that failed, so a consumer (the agent's `assert`/`retract` register, §6.2; an
  explanation client, §10.4; a router, §12; a service retrying under a larger budget, or rebuilding) acts on
  *what was refused* by matching `Refused`, never by reading prose (specification §4, §9.3); and because a
  location is read from the source's own provenance, a backend cannot invent a source for a statement that
  has none, or for a part, which keeps none. No Program fault goes without its source, no Request fault
  without its presupposition, and a Resource, Engine, or Adapter fault, which refused nothing it can name,
  is told apart by its locus and its bit, its detail in its message and any cause it carries. A Program
  fault may arise at `lower` — a construct outside the backend's language (§10.2) — or later, where the
  engine meets the statement: at grounding, or mid-stream for an engine that grounds as it searches (§5.2's
  stream carries it as a faulted item). So a backend that can refuse after `lower` retains what names a
  refused statement — the statements it took, or a map from its engine's locations to them — at Θ(program
  size) beside its engine's own program, and one that refuses only at `lower` retains nothing for it.
- **What a fault preserves, and what it does not.** The common fault keeps exactly its message, its
  locus, the backend-bug bit, what it refused — a statement, a part, a parse, or the request's failed
  presupposition — and, where the backend attached one, the engine's own typed cause, shared and opaque
  behind `std::error::Error::source` (`Fault::caused_by`). Nothing else
  crosses: no cross-engine taxonomy of causes, and no engine type in the contract's signatures. A backend
  that attaches no cause leaves its message alone, and the design claims no more than that — the richer
  detail stays in the engine's own native API, a downcast away where a cause was attached. The cause is
  detail, not identity: equality ignores it, and it is shared, so `Fault` stays `Clone`.
- **Statistics** are exposed per solve through a `Statistics` trait — engine-scoped, provenance-marked,
  typed data (v1: the clingo adapter provides clingo's own). The minimal v1 shape a consumer reads:

  ```rust
  pub trait Statistics {
      fn measurements(&self) -> impl Iterator<Item = Measurement> + '_;   // named, typed, provenance-marked
  }
  #[non_exhaustive]
  pub struct Measurement { /* engine-scoped name, a typed Value, and the engine that produced it */ }
  ```

  The reserved *normalised* cross-backend schema (§14) is a distinct typed **view that consumes** this
  trait — normalising engine-scoped `Measurement`s into cross-backend ones (a normalised name is not
  engine-scoped, so it is a consumer, not an implementor) — so it lands as an additive drop-in, touching
  neither the trait nor `Measurement`, not a breaking change to what a v1 consumer reads.

Every value in this section is typed data first, with a human `Display` and a machine-consumable view
as derivations (§1.3).

---

## 6. The agent and the reasoning loop

### 6.1 An agent is a program reified as a reasoner

The driving surface is not a session on a solver. **A program made active is an agent** — the same
knowledge seen not as an object of study but as a reasoner one drives. What individuates one agent from
another is its knowledge; the loop that drives them is uniform (the Gelfond–Kahl agent). So an agent is
**instantiated from a `Program` that becomes its knowledge base**, and the primary register's nouns are
exactly two: `Program`, the knowledge at rest, and `Agent`, the knowledge in action. Neither `Solver`
nor `Session` appears in the user-facing register — which engine reasons is the installation's business
(§1.4).

```rust
pub struct Agent<B: Backend> { /* owns the backend + the evolving knowledge base (§6.2) */ }

impl<B: Backend> Agent<B> {
    pub fn new(knowledge: Program, backend: B) -> Self;   // the agent OWNS its knowledge base
    pub fn knowledge(&self) -> &Program;                  // inspect the evolving knowledge base
}

// The facade prelude hangs the just-works reification on the program tier's `Program` through an
// extension trait — the idiomatic way to add a method to a lower tier's type; `DefaultEngine` is the
// facade's default backend (§2.2). The single-shot question surface (§6.4) rides the same trait.
pub trait Reason {
    fn into_agent(self) -> Agent<DefaultEngine>;          // reify with the default engine
    // The single-shot questions (§6.4): each BORROWS the program, lowers it into an ephemeral engine the
    // returned handle owns, and drives it — so `p` stays usable after, and there is no extra clone (the
    // lowering is the solve's own cost, §10.1). Refusal: engine/request `Fault`.
    fn determination(&self) -> Result<Determination<'static>, Fault>;
    fn optimize(&self, req: &OptimizeRequest) -> Result<Optimized<'static>, Fault>;
    fn solve(&self) -> Result<Solved<'static>, Fault>;
}
impl Reason for Program { /* … */ }
```

`into_agent` **consumes** the program, and the C-CONV cost/ownership convention fixes that prefix: an
agent *owns and evolves* its knowledge (§6.2) and outlives any borrow (the service posture), so this is
an owning `owned → owned` conversion, not a free borrowed view — `String::into_bytes`, whose receiver's
content lives on inside the result, is the exact analogue. (`as_agent` would signal a cheap borrowed
view and could back neither an evolving knowledge base nor a `'static`-embeddable agent.)

Ownership is the capability substrate, unchanged from the tier's discipline. An agent is an **owned
value** — the authority to drive the engine; dropping it is revocation, and there is no ambient engine,
nor any global state that confers authority: the one process-wide value, a counter that brands each
agent's ledger so that a handle from another agent is refused, grants nothing. Asking a question borrows
the agent (`&mut self`), so the borrow checker *is* the "no mutation while reasoning" lock, and the
reasoning state machine (initial → grounded → prepared → solved) is expressed in ownership and borrowing
rather than runtime checks — an out-of-order call does not compile. Thread posture is explicit per
backend: the live run handles (`Solved`/`Models`/`WorldView`) are `!Send` — the `Run` trait object they
hold carries no `Send` bound, since a run may hold a raw engine handle whose control is single-threaded
(§5.2) — and the engine-free `Snapshot` is the `Send` form that crosses a service boundary;
cancellation-from-another-thread is a declared capability whose handle (`Interrupt`, the core's over the
backend's `Send + Sync` `Cancel`) is `Send`. Because the agent *owns* its knowledge rather than borrowing
a `Program` off a stack frame, it is embeddable behind a service boundary or an editor host without
ceremony — the LSP/pythia posture (specification §1.2, §9.4). Cost: agent construction is one engine
handle; a question's cost is the engine's, streamed (§5.2).

*The name.* "Agent" is the Gelfond–Kahl term, adopted with its §1.4 warrant and a scope stated so it is
not over-read: themelios provides the agent's **knowledge and reasoning**; observing and acting upon the
world are the embedding application's. This is the same move `query.md` §4 makes for the borrowed term
`WorldView` — a literature name taken deliberately, with its scope named.

### 6.2 The reasoning loop: observe, modify, ask, act

An agent is driven by the loop the architecture names — **observe the world, update the knowledge base,
reason, act, and repeat.** The *content* of each step is logical (extend or amend the knowledge, then
ask a question of it); the *state* the loop carries across steps — the grounding, the engine's warm
search, the open truths in force — is the agent's, retained rather than rebuilt. The tier's footprint in
this loop is **modify + ask + record-observation**: the embedding application *senses* the world and
expresses what it sensed as `Facts`; `observe` is the agent *recording* those observations into the
knowledge base; and *act* has no themelios primitive — the application acts on the typed answers of the
*ask* step. (This is the §6.1 boundary: themelios provides the agent's knowledge and reasoning; the
sensing and the acting are the application's.) Multi-shot *is* this loop; a single question is the loop's
body run once (§6.4).

```rust
impl<B: Backend> Agent<B> {
    // --- modify the knowledge base (the logical register) ---
    pub fn assert(&mut self, stmt: impl Into<Statement>) -> Result<StatementId, Fault>; // add a statement
    pub fn retract(&mut self, stmt: StatementId) -> Result<(), Fault>;                   // remove it (below)
    pub fn observe(&mut self, facts: impl Facts) -> Result<Observation, Fault>;          // bulk-assert (§7.3)
    pub fn forget(&mut self, obs: Observation) -> Result<(), Fault>;                     // retract an observation

    // --- retained full-fidelity multi-shot mechanisms (the loop's realisation floor) ---
    pub fn ground(&mut self, parts: &[Part]) -> Result<(), Fault>;          // instantiate #program parts
    pub fn assign_external(&mut self, ext: Symbol, v: TruthValue) -> Result<(), Fault>; // toggle an open truth

    // --- ask (the shared question vocabulary — the same names the bare `Program` carries, §6.4) ---
    pub fn solve(&mut self) -> Result<Solved<'_>, Fault>;                  // the run handle: stream, inspect, resolve
    pub fn solve_with(&mut self, opts: SolveOptions) -> Result<Solved<'_>, Fault>;
    pub fn determination(&mut self) -> Result<Determination<'_>, Fault>;   // resolve → the trichotomy (§5.1);
                                                                           //   WorldView read from Consistent (query.md §2)
    pub fn optimize(&mut self, req: &OptimizeRequest) -> Result<Optimized<'_>, Fault>;
    pub fn solve_assuming(&mut self, s: &Scenario) -> Result<Solved<'_>, Fault>;
    pub fn interrupt(&self) -> Option<Interrupt>;                         // the core's, over `Cancel` — Some iff it cancels (§6.3)

    // --- consequences (native door or derived fold, capability-routed §4.2), on the agent because it
    //     owns the engine — so the reading is a fresh solve, not a drain of a live world view (§5.2).
    //     The unscoped pair ranges over the whole program; the `_assuming` pair ranges over a scenario's
    //     models — the epistemic sibling of `solve_assuming`. THE NORMATIVE HOME of the scoped doors'
    //     precondition and cost (§4.1 and query.md §2.4 cite this): both routes REQUIRE
    //     `capabilities().assumptions` and otherwise refuse as unsupported (`Capability::Assumptions`) — the
    //     derived route enumerates over `solve_assuming`, and the native route ranges over the same model
    //     set `solve_assuming(scenario)` denotes, so the native-vs-derived agreement (query.md §2.4) stays
    //     checkable. Cost (each): one solve native, `Θ(|W|)` derived. ---
    pub fn cautious(&mut self) -> Result<Consequences, Fault>;                        // ⋂ over the whole program — one solve (query.md §2.4)
    pub fn brave(&mut self)    -> Result<Consequences, Fault>;                        // ⋃ over the whole program
    pub fn cautious_assuming(&mut self, s: &Scenario) -> Result<Consequences, Fault>; // ⋂ over the scenario's models (needs `assumptions`)
    pub fn brave_assuming(&mut self, s: &Scenario)    -> Result<Consequences, Fault>; // ⋃ over the scenario's models (needs `assumptions`)
}
```

The **query-typed readings** — `answer`, `bindings`, `entails`, `snapshot` — are the reading tier's, and
hang on the agent through a query-side extension trait (`AgentReading`, query.md §2.7) re-exported in the
prelude, so `agent.answer(q)?` reads inherent while `themelios-solve` keeps no dependency on
`themelios-query`. They, and the `cautious`/`brave` door above, live on the agent (or on a materialised
`Snapshot`) rather than on the live `WorldView`, so a reading is a self-contained solve that composes
freely, never a drain that consumes the handle it is read from (query.md §2.3). Under a scenario the same
readings ride a scenario-scoped snapshot — `snapshot_assuming(&Scenario)` (query.md §2.7), whose infallible
readings range over the scenario's models — so the `_assuming` surface mirrors the unscoped one, the way
`solve_assuming` mirrors `solve`.

**Assertion is monotone and clean; retraction is the sharp edge, made honest by owning the knowledge base.**
`assert` adds one statement — any `Statement` (`program.md` §4.2): a rule, a fact, a constraint, an
objective (`Optimize`) — and returns a `StatementId` naming it; `ground` instantiates a named `#program`
part, today without its arguments (§14) — the two are the fine- and coarse-grained faces of the same
monotone extension. `observe` is the bulk assertion of ground facts through the `Facts` pillar (§7.3), the
loop's *observe* step, returning an `Observation` a later step can `forget`. Because themelios **owns the
knowledge base as a first-class `Program` value** — which neither Prolog's flat clause database nor an
engine's write-only backend has — `retract` is a *true* operation on that value: it removes the named
statement from the knowledge base, and the agent then realises the removal against the engine by the
cheapest faithful means its declared capabilities allow — **toggling an external** where the retracted
statement was so guarded and the backend declares `externals`, or **resetting** the engine's accumulated
program (`Backend::reset`) and reloading the amended program (`lower`) otherwise — `lower` accumulates, so a
rebuild is a reset then one lowering, never a bare re-lower. The realisation is the agent's to choose; the
register the caller writes stays declarative. This is why themelios can offer retraction where an engine
offers only externals: it holds the program the external mechanism can only approximate.

Retraction's two realisations diverge in cost by the whole program size and the loss of the engine's
warm search, so — following §4.2, which forbids hiding a divergence of that magnitude behind a uniform
signature — retraction is **disclosed before it is paid for**: a statement's
*retraction class* (toggle vs rebuild) is fixed at `assert`, from the backend's `externals` capability
and whether the statement is externally guarded, and is readable from its `StatementId` — the disclosure
of which realisation a retraction takes, an outcome recording it deferred with §4.2's (§16). A stale,
duplicate, or foreign handle is a typed refusal, not a silent no-op — `retract` of an already-retracted
`StatementId` or one another agent issued refuses with `Presupposition::NotLive`, and `forget` of a spent or
foreign `Observation` with `Presupposition::Spent`, each at `Locus::Request` (§5.4).

Two things are deliberately *not* inherited from Prolog's `assert`/`retract`: the `asserta`/`assertz`
ordering variants are absent (clause order is meaningless under answer-set set-semantics), and the
logical-update-view hazards do not arise, because the knowledge base is amended **between** questions, at
the loop's step boundary, never during a running search. The modification methods rest on the
capabilities their realisation uses — `multi_shot` to ground an added statement or part incrementally,
`externals` for the external-toggle retraction path; where a backend declares neither, the agent falls
back to re-grounding the amended `Program` — a `reset` then a `lower` on a multi-shot backend, and on a
**single-shot** backend a plain `lower`, which *replaces* the program there (each solve is independent,
so nothing accumulates and no `reset` is needed) — a rebuild any backend that solves at all supports. A
rebuild on a multi-shot backend re-establishes after its lowering the state the loop carries — the open
truths assigned through `assign_external`, the parts instantiated through `ground` — so a `reset`
discards nothing the agent retains. The retained state upholds the two-representation correspondence
(owned program ↔ engine-internal state) across the cycle, which is also what gives a transform on the
owned side a defined effect under multi-shot. Cost: retained state is
`Θ(program size + assigned externals + grounded parts)`, not `Θ(ground size)` — the ground instantiation
stays in the engine (§10), and beyond the program the loop keeps a truth value per external it assigned
and the parts it grounded; an external-toggle step is `O(1)` at the seam; a rebuild is a `reset`, one
lowering (§10.1), then the replay — the grounded parts first, in the order they were grounded, then each
assigned external's latest value, since an external is assigned only once grounded — whose re-grounding
the engine pays again. A grounding that fails is never accepted: it leaves the backend needing a rebuild
(§4.1, a backend's own state), and the agent recovers by this same rebuild at its next step that touches the
engine — a `reset`, one lowering, and the replay of what it accepted, at the rebuild's cost above — so the
failed step is not replayed and nothing the agent accepted is lost; registrations ride across the `reset`
with the backend (§4.1), so the replay need not restore them. The agent keeps its loop's invariant — the
engine level with its knowledge, or a rebuild pending — by tracking, not by reacting: any step of its own
that fails against the engine leaves a rebuild pending, whatever the fault, and only a rebuild that succeeds
clears it. Until the agent retains its engine between steps, every step rebuilds, so a rebuild is always
pending and the tracking is trivial; the pending mark takes effect with the retained engine. So it reads
neither a fault's locus nor `Presupposition::NeedsRebuild` to decide — that
presupposition's consumer is a client composing over `Backend` (§4.1) — and a refusal that needed no rebuild
costs one, never a wrong answer. Should the replay itself be refused — an
accepted part whose `@`-function now faults, say — the knowledge base stays intact, the backend still needs
its rebuild, and the refusal surfaces from that step; so it does from every later step that touches the
engine, each retrying the rebuild and refusing at the same part, until a replay succeeds — the knowledge
amended so that the part grounds, say. The loop has no un-ground, as an engine's multi-shot has none: a
grounded part stays in every replay.

### 6.3 Per-operation typed options; budgets; cancellation; assumptions

Configuration, where it exists at all, is **surfaced at the operation it affects and nowhere else** —
solve-options where answer sets are asked, grounding-options where knowledge is asserted, and so on.
Each is a typed options value carrying `Default` (the empty case is free) and `#[non_exhaustive]` (a new
knob is not a breaking change), surfaced either as a paired method (`solve()` clean,
`solve_with(SolveOptions)` configured) or a fluent builder on the request. The bare ask stays a
no-options call — the pristine "just the abstract object" path.

**Assumptions and scenarios are typed request-side values**, defined once so the blame surface (§5.4)
and the reasoning loop both use them. An `Assumption` fixes one program atom true or false for one
question; the *raw set* of them is what the literature calls **assumptions**, and it is what
`solve_assuming` scopes
by and what blame (§5.4) reports. A **`Scenario`** is this library's coined term (specification §8) for
a *reusable, named assumption configuration* — the §1.4 reason it owes: the literature's word names the
raw sets, so a named, reusable *bundle* of them is a concept this library introduces and therefore
names.

```rust
/// A program atom fixed true or false for one solve. Refuses a non-atom at construction.
pub struct Assumption { /* Symbol + polarity */ }
impl Assumption {
    pub fn new(atom: Symbol, holds: bool) -> Result<Self, NotAnAtom>;
}

/// A reusable, NAMED assumption configuration — a concept this library introduces (specification §8),
/// so this library names it (§1.4); the literature's "assumptions" names the raw set, not the named,
/// reusable bundle. A set of `Assumption` with an identity you bind and re-apply across solves;
/// `solve_assuming` takes one, and blame (§5.4) reports the responsible raw subset (`Refutation`).
pub struct Scenario { /* a named, owned set of Assumption */ }
impl FromIterator<Assumption> for Scenario { /* … */ }

/// Ergonomic construction from atoms/literals authored the §3.1 way; refuses a non-assumption.
pub trait IntoAssumption { fn into_assumption(self) -> Result<Assumption, NotAnAssumption>; }
// scenario! { p(1), not q(2) }  ==>  Result<Scenario, NotAnAssumption>   (macro law, §3.2)
```

Assumptions and retraction are distinct on purpose: an assumption fixes an atom's truth for the span of
one question and is discharged after it (a hypothesis — *if this held, what would follow?*); a
retraction (§6.2) amends the knowledge base itself and persists (a change of mind). The reasoning loop
uses both.

**Budgets** (time at minimum, with room for model-count caps) are a typed, request-side surface, and
`Conclusion::Budget` reports a hit budget as what it is. How a budget is realised is fixed by the
declaration before the request is paid for, as a retraction's class is (§6.2): where the backend declares
`budgets.time`, it enforces the budget natively — the request carries the budget to it, and its run
concludes `Budget` at the cut; otherwise, where it declares `cancellation`, the core enforces the budget
with its own timer over the backend's `Cancel` (§4.1) — the request is forwarded without the budget, and
the core attributes the stop; and a budgeted request over a backend that declares neither refuses at the
request locus (`Presupposition::UnrealisableBudget`, §5.4), as does a backend handed a budget it does not
enforce — one presupposition for the one event, whoever refuses. The conformance suite's time-budget probe
reads this rule, and drives an enumerating backend's declared budget to its cut — a choice over forty atoms,
whose answer sets no search enumerates within the budget — failing a cut search concluded as closing the
space; a deciding backend stops at its witness, which no such budget cuts. The core's timer is not yet
realised, so until it is, a budget is honoured natively or refused. The readings take no options — the agent's consequence doors and the query tier's readings each
solve over the default request — so a budgeted reading is a composition: `solve_with` under the budget, the
determination it yields, `WorldView::of` over its models, and `materialize` to a `Snapshot` read infallibly
(query.md §2.7). That composition bounds the search by its time budget alone: the world view's models are
materialised before the search's conclusion is read, no model-count cap is realised, and no scoped reading
carries a budget — what an embedder serving untrusted callers must supply beyond it is the threat-model
statement's to say (specification §12.4). Reading forms that carry options are grown when a consumer names
the need.

**The phases of a question, and what the time budget covers.** A single-shot backend answers a question in
two phases, and the contract fixes what each may do:

- **`lower` validates and retains.** It makes its checks at the door, each bounded by the program's size,
  and keeps the program it will solve. It grounds nothing: grounding belongs to `solve`. Its retention is
  transactional, so a refused `lower` leaves the program lowered before it in place (§4.1).
- **`solve` opens the run.** It creates the run's control state first, then grounds the retained program's
  `base` part, then searches, delivering each model as the caller reads it. A grounding that fails fails
  that solve alone: the lowered program stays, ready for another question, and partial ground output never
  becomes it (§4.1).
- **The `base` part alone.** A single-shot solve grounds the `base` part — the statements before any
  `#program` delimiter, and those under `#program base.` — and no other. That is the language's
  single-shot reading, the one the pinned authority's default run realises: with no `main` script and no
  `#include <incmode>.`, it grounds `base` and solves (`libclingo/src/clingocontrol.cc`,
  `ClingoControl::main`, its last branch; a version-scoped claim). themelios's rule holds whatever the
  program carries, departing from the authority's other branches: a `#script` block is carried and never
  run, and an `#include` parsed and never resolved (program.md §4.8, §17). A named part, with formals or
  without, is instantiated only through a multi-shot backend's `ground`, which today takes a part without
  its arguments (§14). So a single-shot backend either leaves a part beyond the base ungrounded — the
  language's reading — or refuses the program at `lower` with a Program fault naming the part
  (`Fault::program_part`, §5.4). The refusal is §12's fragment interim: a backend that admits only `base`
  refuses a named part as it refuses any construct outside its fragment, until the fragment declaration
  (§14) discloses that before `lower`. It never grounds such a part, and never charges the refusal to a
  statement in it. The refusal is unlocated because the program tier keeps no origin for a part, its
  `#program` delimiter being no statement (program.md §4.1, §8). A located part refusal waits on a part
  carrying its delimiter's origin (§14); until then it renders through its `Display`, as any unlocated
  fault does.
- **What may be reused.** A backend may reuse any immutable preparation of a retained program across
  questions — an index, a dependency graph, a compiled form — since each question reads the same program.
  The contract promises no shared mutable search state, and no incremental grounding between questions.

The time budget is a **wall-clock deadline fixed when `solve` is called**. It covers everything the run then
does: grounding, search, and the delivery of each model. It is a deadline rather than a meter of the
backend's own work, for two reasons. A deadline is what an engine's own limit is: clingo's `--time-limit`
is a wall-clock alarm armed before grounding begins (`libpotassco/src/application.cpp`,
`Application::main` and `setAlarm`), so a backend honours `budgets.time` natively, without a clock the core
would stop and start around each read. And a deadline bounds the question's wall-clock time, the bound a
service answering callers needs, which a meter would leave open however long a consumer took.

- **The consumer's time counts.** The deadline is an instant, not a meter, so the time a consumer takes
  between reading one model and asking for the next counts against it. A deadline that passes while the
  backend waits on the consumer ends the run at the next read.
- **A cut during grounding is a cut.** A deadline that passes during grounding, before any model, ends the
  run there: it concludes `Budget` with no model, whether `solve` returns it so or its first read does — as
  the backend grounds eagerly or lazily — and its determination is `Inconclusive`, never a fault (§5.1).
- **Lowering is outside the deadline.** The agent brings the engine level before it calls `solve` — its
  `lower`, and on a multi-shot backend its `reset` and replay (§6.2) — so that work runs outside the
  deadline. A budget bounds a question's run, not the whole `Agent::solve_with` call. A multi-shot
  backend's `ground` is a call of its own, outside any solve, and takes no budget (§4.1).
- **Enforcement is cooperative.** A backend checks its deadline at points its engine provides, so a run
  ends at the first check past the deadline, not at the instant. This is not a hard real-time bound. How
  far a check may lag in each phase is the backend's to state, and its own tests hold it in each grounding
  mode it has; the conformance suite holds only that a cut search concludes `Budget` (§13.1).

**Cancellation.** The core owns the caller's handle, as it would own the timer:

```rust
/// The core's handle over a backend's cancellation primitive (§4.1): `Agent::interrupt` answers `Some`
/// exactly when the backend declares `cancellation`. Owned, and `Send + Sync`, so a caller obtains it
/// before borrowing the agent for a question and pulls it from any thread.
pub struct Interrupt { /* the backend's `Cancel`, and the agent's record of its questions: whether one is
                         in flight, and whether it was pulled */ }
impl Interrupt {
    /// Cut short the agent's question in flight, if any. O(1); it signals and returns, never blocking on
    /// the question's thread.
    pub fn pull(&self);
}
```

- **What a pull cuts.** A pull cuts the question in flight. A question is in flight from the moment it is
  asked, its lowering included, until its run ends or its handle drops — or, for a question answered by
  value, such as a consequence door (§6.2), until it returns. The next question asked ends it in any case,
  so a run handle leaked rather than dropped keeps no question in flight past it. The core forwards the pull
  to the backend's primitive at once. A question pulled before its search begins does not begin it, and
  concludes `Interrupted` with nothing established. A pull that lands while `solve` opens the run is
  forwarded again once the run is open, so a pull the backend could not yet take is not lost. The run stops
  at the backend's next check, in grounding or in search, as for a deadline, and concludes `Interrupted`.
  Nothing the search established is lost: the models read stay read, and the determination is `Consistent`
  if there was one. A question answered by value says the same in its own terms: `determination` reads
  `Inconclusive`, concluded `Interrupted`, when no model was seen, and a question whose answer needs a
  closed space — a consequence door, or a reading of the query tier — refuses at the request locus with
  `Presupposition::Unclosed(Truncation::Interrupted)`, nothing established.
- **A pull with no question in flight cuts nothing.** That covers a pull before the first question, between
  two, after a run has ended or its handle dropped, and after the agent itself has dropped. The core keeps
  no memory of such a pull, so the next question runs as if it had never been pulled. The backend's
  primitive does the same (§4.1).
- **The caller's act takes precedence, over a deadline alone.** For a pulled question the core maps the
  conclusion its backend reported:
  - `Budget` ↦ `Interrupted`, whether the deadline was the backend's native one or the core's timer; a
    question answered by value refuses with `Unclosed(Interrupted)`, not `Unclosed(Budget)`;
  - `Interrupted` ↦ `Interrupted`;
  - `Exhausted` ↦ `Exhausted` and `Target` ↦ `Target`: the search closed the space, or met a target or a
    deciding backend's witness, before the cut took effect, and a cut that arrives too late changes
    nothing;
  - a fault ↦ the same fault, with no conclusion (§5.1).

  A question no one pulled reports as its backend did.
- **Who holds what.** The conformance suite holds a backend's primitive: an active, unfinished search
  that it cuts, and the stale pulls (§13.1). The agent's tests hold the core's part deterministically over
  a test backend: the attribution, the held pull, and a pull after the backend drops. Each adapter's race
  harness holds a pull concurrent with a run's opening and closing (§13.3).

The long tail of engine parameters, when a real consumer needs it, follows the two-tier facade pattern
(typed knobs over a legible open form); it is YAGNI-gated, grown on demand, never a CLI-string passthrough.

### 6.4 Single-shot: the questions asked of a program directly

The simplest use asks a question of a program with no loop around it. The **question vocabulary lives on
`Program` itself**, through the same facade `Reason` trait that provides `into_agent` (§6.1) — the
logician's questions asked of the object, the register made literal. Because there is no retained agent
to borrow against, the bare forms return **owned** results — the owned analogue of §5's borrowed handles:

```rust
p.determination()?  // owned Determination<'static> (§5.1): Consistent(Models<'static>) → WorldView::of (query.md §2.7) / Inconsistent(blame) / Inconclusive
p.optimize(&req)?   // owned Optimized<'static> (§5.2): trajectory, proven Optimum, optimal world view
p.solve()?          // the owned run handle: stream, inspect, resolve
```

These are **exactly the agent's questions, asked once.** The engine-ownership principle, stated per
handle so a builder can implement it: **each bare handle owns the ephemeral agent it drove and drops it
when the handle drops.** So a returned model stream or `WorldView` stays *lazy* — it retains the
engine to pull the next member — and §5.2's constant-resident guarantee and query.md §2.3's opt-in
materialisation hold for the bare form exactly as for the agent. The agent's forms (§6.2) borrow against the agent you retain; the bare forms *own* it.
That is the only difference; the answers are the same. Concretely the reading resolves to a
`Determination`: from a retained agent, `agent.determination() -> Result<Determination<'_>, Fault>`,
whose `Consistent` world view **borrows** the agent (§5.2's consuming resolver threads the borrow); from
a bare `Program`, `p.determination() -> Result<Determination<'static>, Fault>`, **owning** its ephemeral
engine; and `WorldView::materialize` (query.md §2.3) turns a live world view into an owned engine-free
`Snapshot` when one must outlive its agent. `Fault` is reserved for engine/request errors — an
inconsistent (blame-carrying) or inconclusive program is a value of the `Determination`, never a `Fault`
(§5.1). So the identity is denotational — the reading is the same, the ownership differs:

```rust
p.determination()   and   p.into_agent().determination()   read the same determination
```

so the *questions* are the shared spine and the loop is only what multi-shot adds. This is why the tier
keeps no separate one-shot API in step with the multi-shot one: the **run questions** — `solve` /
`determination` / `optimize` — are one vocabulary, hosted bare (`Reason`) or in the loop. The **epistemic
readings** (`answer` / `bindings` / `entails` / `cautious` / `brave`) are the one asymmetry, and it is
named rather than hidden: a reading needs an *owner* for the engine (§5.2), so it lives on the agent
(`AgentReading`, query.md §2.7) and on a materialised `Snapshot`, and a bare `Program` reaches it by
reifying (`into_agent`) or by materialising a `Determination`'s world view — not through `Reason`
directly. The keystone (the API is the logician's questions) holds; the bare/loop *symmetry* is the run
questions'. Cost: identical to the agent's; the ephemeral engine lives for the returned handle's lifetime,
not merely the call's.

---

## 7. `@`-functions — a centerpiece (ground-time extension and the library door)

### 7.1 The mechanism

Named Rust functions (or a context value) register on an agent; `@name(args)` calls into Rust through
a **panic-containing trampoline**; arguments and results cross as typed symbols via the conversion
pillar; multi-valued returns are supported; a failing `@`-function is a typed ground-time fault with a
locus. Before the surface is fixed, an `@`-function's purity and concurrency contract is stated beside its
determinism (§7.2): whether an engine may call it concurrently, and what it may assume of the calling
thread.

```rust
/// A registered ground-time function. Registration is on the agent (§4.1).
pub trait Function {
    // multi-valued; results cross as `Vec<Symbol>` — program.md §3.4's `IntoIterator<Item=Symbol>` shape,
    // realised so `themelios-solve` takes NO dependency beyond the lower tiers (§16, spec §12.5). A
    // stack-inline small-vector is a later measurement away if the ground-time 3% is ever identified.
    fn call(&self, args: &[Symbol]) -> Result<Vec<Symbol>, GroundFault>;
}
// #[external] derives an impl from a plain Rust fn, with COMPILE-TIME-checked signatures
// where the Python comparator has duck typing (the ground-extension witness, spec §3 witness 13).
```

### 7.2 The library door

The centerpiece is not just "call Rust from grounding" — it is the **door to Rust's library
ecosystem**. This is the payoff of the program tier's real/rational strategy (`program.md` §3.4):
compute in Rust's numeric tower (`f64`, `num-bigint`, `num-rational`, and any crate — math, dates,
units, geo, strings) inside an `@`-function, and convert at the ASP boundary via the fallible rounding
adapters, refusing rather than repairing when a value cannot be represented (on the 5.8.2-pinned
target, `Symbol::Number` is `i32`; the boundary refuses out of range). Two hazards are designed in
rather than rediscovered: over an engine that interns process-globally, an `@`-function must intern
*inside* the grounding call under its adapter's interning discipline (§10.5), and the trampoline contains
its panics. One mission discipline is stated: an `@`-function is arbitrary Rust at ground time, so
purity/determinism is a contract the surface makes easy to declare — a clock, an RNG, or the filesystem
breaks deterministic mode and auditability.

### 7.3 `Facts` — the bulk conversion, and `@`-predicates

`Facts` is the bulk analog of `ToSymbol` (§3.2): where `ToSymbol` denotes one ground `Symbol`, `Facts`
denotes a *set* of ground atoms — the codec a data-shredding client or a code generator needs to turn
a Rust value into a sub-program's facts, and the result side of an `@`-predicate.

```rust
/// A Rust value denoting a SET of ground atoms. Derived by #[derive(Facts)] (themelios-macros).
pub trait Facts {
    fn facts(&self) -> impl Iterator<Item = Symbol>;   // the GROUND atoms (each a `Symbol`) it denotes
}
```

Cost: `Θ(atoms produced)`; it shares the `Symbol`↔Rust codec with `ToSymbol`/`FromSymbol`/`Extract`
(§3.2), so a fix to the codec pays out across all four. `Facts` is the construction/`@`-predicate hub;
`Extract` (§9) is its read-time inverse.

---

## 8. Propagators — a centerpiece (the theory-extension platform)

### 8.1 The surface, governed by the litmus (interface + principle, not a frozen signature)

The propagator interface is a **safe Rust trait** for theory propagation — `init`, `propagate`,
`undo`, `check`. Its exact signature is **not fixed by fiat**; it is governed by three principles, in
this order — **ergonomics, coherence with the rest of the API, and the ease of building out our
desired set of extensions** — and validated at build time against the **DL/CP/LP litmus** (§8.2). What
the design *does* fix is the interface shape and its laws:

```rust
// Interface shape (governing principle: the DL/CP/LP litmus, §8.2 — finalized at build). The State-bearing
// trait below is the built (reactive-tier) shape; because the registration seam `register_propagator`
// takes `Box<dyn Propagator>`, an *object-safe* form (no associated `State`) is what a declaration-only
// tier registers through, the full signature being finalized where the theory is built.
pub trait Propagator {
    type State: Send;   // per-thread; the hot methods take &self + &mut Self::State
    fn init(&self, ctx: &mut InitCtx) -> Result<Self::State, TheoryFault>;
    fn propagate(&self, st: &mut Self::State, ctx: &mut PropagateCtx) -> Result<(), TheoryFault>;
    fn undo(&self, st: &mut Self::State, ctx: &UndoCtx);
    fn check(&self, st: &mut Self::State, ctx: &mut CheckCtx) -> Result<(), TheoryFault>;
}
```

The trait brokers per-thread state as an `&mut Self::State` (so the hot methods take `&self`), exposes
typed literals and typed clause-add results (an added clause may assert below the current level, and
the core supports that out-of-order implication), and handles watch management and program↔solver
literal mapping *beneath* the safe surface. Panic containment and callback-scoped lifetimes are the
adapter's obligation (§10.5). The vendored clingo/clingcon source is the reference for the mechanics
beneath (consult it for order atoms, watch generations, the step-literal scoping of clauses added
during solving). The decision-level and clause-addition mechanics above are CDNL's; before the signature
is fixed, its claimed engine portability (§8.3) is reviewed against a non-CDNL engine — zetesis's
reduct-based execution (§12).

### 8.2 The litmus, and the full CP target

The acceptance test is that **difference logic, CP, and linear/real arithmetic must each be *pleasant*
to write** on the trait — a clean port proves the seams are real abstractions, not clingo-shaped
holes. The **CP half is a *full* constraint theory, not a difference-logic subset**: global
constraints — `alldifferent` and its family — and `&sum`/`&dom` must be expressible on the surface,
and their assignments must read back through `TheoryAssignments` (§5.4) as typed data. This is the
concrete bar the propagator surface and the theory-assignment component are held to.

### 8.3 Engine-portable, and the platform it makes

Because the trait is part of the **contract**, not a clingo-specific hook, a theory written once runs
on *any* backend that implements the contract — clingo/clingcon now, a native engine later — on the
portability review §8.1 names: the mechanics stated there are CDNL's, and a non-CDNL backend's
counterpart is what that review establishes. This turns
the propagator surface into a **theory-extension platform**: difference logic, linear/real arithmetic,
and **a full in-house CP theory — the *portable alternative* to the linked clingcon backend (§11)** —
are Rust **satellites** built on it (their own repos; themelios ships the *surface*, not the theories),
best-of-breed and written in Rust on the platform rather than against the Potassco C libraries. The
in-house CP theory and the linked clingcon backend **coexist**: clingcon (§11.1) is a first-class native
backend a deployment links for its mature theory, and the Rust CP satellite is the engine-portable
alternative that runs behind *any* conforming backend. The CP satellite's stated ambition is a
**full clingcon alternative that *exceeds* clingcon-5's constraint set**: because we own the propagator
surface and the theory, we do not inherit the clingo-integration friction that led clingcon-5 to drop
constraint types clingcon-3 carried, so there is no reason to ship the reduced set (§14). The platform
is the reactive tier's payoff and the base a native engine's theory story inherits, so the design
invests the most here.

### 8.4 Encapsulated intra-propagator parallelism

A propagator may parallelise its internal theory work in Rust; the abstraction layer serializes the
*boundary* to the engine — the engine calls `propagate`/`check` serially on a solver thread, and
everything the propagator hands back crosses that one serialized seam. The shared engine state is a
monitor-protected resource; the propagator's internal concurrency cannot touch it except through the
serialized door, so the parallelism is safe by construction (Rust ownership + `Send`/`Sync` + the
monitor boundary — the worst it can do is be slow). This **sidesteps** the reserved multi-threaded
*propagation* seam (specification §9.6, the engine-level problem) entirely: it is encapsulated inside
one propagator on one solver thread. The one discipline it imposes, for the mission side: the parallel
output must be **deterministic** — clauses and conflicts reported identically run-to-run
(parallel-compute, then deterministically order before crossing the seam) — which pairs naturally with
a single solver thread (the engine's own portfolio threading is nondeterministic). As a v1 capability
the specification's §9.6 does not itself mention, it is recorded in §16.

---

## 9. Read-time extraction

`#[derive(Extract)]`-class mapping takes answer sets into user-defined Rust values via the conversion
pillar (the *extraction* witness), with documented failure behaviour on non-matching atoms.

```rust
/// A set of ground symbols → a user-defined Rust value. The read-time inverse of Facts (§7.3). It reads a
/// model's answer set (`model.atoms()`), or what the program displays (`model.shown().symbols()`, §5.1) —
/// a `#show (X, Y) : edge(X, Y).` table, say — the choice the client's, named at the call.
pub trait Extract: Sized {
    fn extract(symbols: &BTreeSet<Symbol>) -> Result<Self, ExtractError>;
}
```

Extraction is the **machine view of an answer set** (§1.3): the answer set exposes its symbols in
canonical order, and any view — a derive-based typed extraction, a JSON rendering, an editor payload —
is a derivation over that. It shares the conversion pillar with `@`-functions (§3.2), so the same
`FromSymbol` that reads an `@`-function's argument reads an answer set's atom. Cost: `Θ(atoms read)`.

---

## 10. The bridge and the seam

### 10.1 The load-bearing surface

The bridge lowers the owned `Program` to the engine and streams answers back. It is where the program
tier's design pays off or fails, and it is itself an algorithms-of-import-class surface: it must be
**fast** (a perf claim) *and* **faithful** (a wrong lowering is a soundness bug at the seam that no
themelios-side rigor catches), so it earns the mgu/finiteness treatment — a differential against the
engine plus worst-case cost tripwires. Cost model: **the lowering — the conversion of a typed value into
a backend's input — is linear in program size** (the scaling bench asserts it, §13.3), and it never
materializes the ground instantiation on the owned side. Past the conversion, preparation and grounding
are the backend's own work, and grounding's cost is the program's — exponential in the program in the
worst case, and unending for a program that does not ground finitely, which a budget bounds (§6.3) — so
the linear bound is the conversion's, never a claim about grounding's time or storage.

### 10.2 The doors

The seam offers two doors, **two *entry values* into one lowering** — the backend's own, into its
engine's input: a Potassco backend's grounder input, its AST builder driven to `ground` (§11.1), or a
native engine's own preparation and analysis (§12). They differ only in what they preserve, mirroring the
two-doors study in `program.md` §7 (the raise, §8):

- **Door A — a parse, admitted (`&Admitted`)**: the program as written — its statements in source order,
  each with its part and provenance, content-equal statements not merged — where the highest fidelity is
  possible. The core admits a parse once, through the raise (`program.md` §8): its lowering half keeps
  the statements in source order (`raise_occurrences`), and they are collected into the program they make
  (`program.md` §6.3). The admission rule is one rule, by severity: `Admitted::of` refuses a parse on any
  error-severity diagnostic, with that side's whole batch — the parse's, which put it outside the
  language, or else the raise's, every one of which is an error (`program.md` §8; the collection adds
  none), a repeated definition among them (`program.md` §6.3) — while a warning on either side, a
  misplaced doc comment say, neither refuses a parse nor rides in the `Admitted`, and stays on the
  caller's `Parse`. A raise diagnostic refuses because what the raise made of its statement is not the
  program as written: it skips a statement it cannot complete, and keeps a best-effort stand-in for a
  value it cannot represent — an out-of-range number, an unexpanded splice, a theory atom's pooled
  argument list — so admitting either would admit another program. The refusal, `NotAdmitted`, is typed
  and located, and reaches the caller before any backend is asked; for a caller that wants `?` it
  converts into a Program fault refusing the parse, which carries it whole (§5.4) — the program text is
  where it lies. So the admission rule has one home and the raise is its one authority: an
  `Admitted` that exists raised cleanly. It certifies that, and nothing of a backend: a backend's
  language, its arithmetic, and its resource limits remain its own checks at `lower`, where a typed
  program's structural limits — its depth and its size — are the backend's to bound, as source bytes and
  syntax-tree depth are the syntax tier's at the parse (`syntax.md` §6.6). Source order is input
  fidelity, not an execution order: a backend that reads it imposes no order on grounding or search.
  Cost: the raise's, `O(tree)` up to the log factor of ordering its sets and counted collections
  (`program.md` §8), and the set's collection from a clone of the occurrences, since the program tier's
  collection consumes them — paid once, at admission, with the statements held twice, in source order
  and as the set.
- **Door B — `themelios_program::Program`**, canonical-order, carrying `Origin` provenance on every
  statement, for an observer to carry through to the ground rules it attributes (§10.4) — a capability the
  C grounder lacks: the primary programmatic entry. Programs
  constructed in Rust, transformed, or loaded through a client enter here, ground or not: a ground
  program is a ground `Program` — no file format, no second rule tree — and it is lowered as any program
  is, since the absence of variables discharges none of a backend's language, arithmetic, or resource
  obligations. The lowering hands the backend every entry of a counted collection (program.md §4.4), so
  a repeat the value keeps reaches the grounder — and, for a theory atom, the theory's rewrite — as
  written.

```rust
pub enum Door<'a> {
    Parsed(&'a Admitted),   // A: a parse, admitted — statements in source order, provenance intact
    Program(&'a Program),   // B: canonical, provenance-carrying, ground or not — the primary entry
}
impl<'a> Door<'a> {
    /// The program this door carries, as a set — Door B's own, or the one Door A's statements collected
    /// to at admission — for a backend that reads a program as a set rather than in source order. O(1).
    pub fn program(&self) -> &'a Program;
}

/// A parse the core admitted at Door A: its statements raised once, in source order, each with its part
/// and provenance (program.md §8, `raise_occurrences`), and the program they collect to (program.md §6.3).
/// Its only constructor is `of`, so an `Admitted` that exists raised cleanly.
pub struct Admitted { /* the raise's occurrences, and the program they collect to */ }
impl Admitted {
    /// Admit a parse, or refuse it whole: with its syntax errors, or, the parse being in the language,
    /// with the raise's whole batch of diagnostics. The raise's cost (above).
    pub fn of(parse: &Parse<ast::Program>) -> Result<Admitted, NotAdmitted>;
    pub fn statements(&self) -> impl Iterator<Item = &StatementOccurrence> + '_;   // in source order
}

/// Why a parse was not admitted — exactly one of two, typed and located, each diagnostic lowering to a
/// base diagnostic.
pub enum NotAdmitted {
    Syntax(Box<[SyntaxError]>),    // the parse is not in the language: its error-severity diagnostics
    Lowering(Box<[LowerError]>),   // in the language, its raise refused: the whole batch (program.md §8)
}
impl NotAdmitted {
    pub fn diagnostics(&self) -> Vec<Diagnostic>;   // either batch lowered, in source order: rendering
}
impl From<NotAdmitted> for Fault {}                 // a Program fault refusing the parse, carrying it whole
```

**Door A's two readings, and who walks through it.** A backend reads Door A's statements in source order
— building its engine's input from them, as from Door B's — or reads the set `Door::program` lends, and
its design records which (§11.1, for the Potassco adapter). Under either reading — admission having
refused every repeated global definition, the one repeat the set's merge would change in meaning
(`program.md` §6.3) — a Door A client may rely on the same answer sets, on every statement's parsed
origins — the set's merge unions a repeated statement's root origins (`program.md` §6.3) — and on each
statement's part. Only the source-order reading
keeps each occurrence apart: a repeated statement lowered as written, an observer able to attribute a
ground rule to its occurrence (§10.4) with its nested provenance intact, and a Program fault naming the
occurrence rather than the merged statement. Door A is an entry of the contract: a client composing over
`Backend` walks through it — a REPL or an explanation client lowering a parse, and the conformance suite
— while the agent's loop runs over the set, its knowledge a `Program` lowered through Door B (§6.2). An
agent door from an `Admitted` — its first lowering through Door A, its rebuilds through Door B — waits
for a consumer that needs the occurrences across the loop (§14).

**Every backend takes both doors**, reading Door A in source order or through `Door::program`, so the door
set is no capability: `Capabilities` declares none, and no program is refused for the door it came through.
What a backend refuses is a construct outside its language — a theory atom of a theory it does not evaluate
(§4.1), a term directive it cannot evaluate (§5.1), a form its engine lacks — at `lower`, or where its
engine meets the statement (§5.4), with a Program fault naming what it refused — the statement, or the part
(§5.4); it never routes the input through another door, and never approximates it (§12). The set is closed,
so a match over the doors is exhaustive without a wildcard, and a new grade is a new variant every backend's
`lower` answers.

The **discipline is absolute: never render to text and re-parse across the seam.** The fragile, slow
path a shell-out imposes (four hand parsers, a pipe deadlock, an output cap, string-matched
optimality — the shell-out's cautionary tale) is exactly what the typed doors erase. The owned,
no-sharing tree stays the authoring/analysis form only; the huge ground instantiation lives in the
engine's compact internals, streamed.

### 10.3 Engine formats stay with their adapters

An engine's own input format belongs to the adapter whose engine reads it (§11), never to the contract:
the Potassco family's aspif, with the atom and literal identifiers it numbers, and the typed sink the
adapter designs over its engines' program backend (§11.1). The doors carry the language's objects
(§10.2); an engine format is one engine's construction interface, so a backend whose engine reads no such
format owes none of it, and an engine's identifiers never cross into the contract (§10.5).

Where an adapter ingests its engine's format — a foreign grounder's output, say — it owes the answer set
the contract defines (§5.1), and the format does not carry one by itself: an aspif stream names an atom
only where it displays one — its one naming statement is the output directive, a string under a condition
(`clasp/libpotassco/potassco/aspif.h`, `AspifOutput::output`) — so `q. #show. #show p : q.` grounds to a
stream whose one name, `p`, is a displayed term, while its true atom `q` goes unnamed. The ingestion
therefore takes every atom named by its symbol — the pinned engine records the correspondence when an
atom is added with its symbol (`clingo_backend_add_atom`, `libclingo/clingo.h`) — or refuses the stream,
and it never reads a displayed name as an atom; it is designed with its adapter. Both engine claims here —
the stream's naming and the backend's correspondence — are version-scoped to the pinned engine and held
by the adapter's spike suite (§13.2). A format that takes
only ground objects is no entry for a non-ground program, and mapping one there is a category error. A
later serialization of the doors' values — a ground program written out for an external engine, say —
encodes this contract rather than determining it, and never stands between a backend and the typed
values.

### 10.4 The ground-program IR / observer capability

The ground program is a first-class value a backend can expose — the machine-IR of §1.1 — each ground rule
naming the statement it was instantiated from, with that statement's part and its provenance whole
(`Backend::ground_program`, §4.1):

```rust
/// A ground program, as this section's law holds it — engine-free data. It holds each statement its rules
/// were instantiated from once and whole, with its part and its provenance: the program's statements under
/// their parts' keys, or a parse's occurrences in source order where the backend attributes per occurrence
/// (`Grain`), each occurrence's own statement — its nested provenance intact — and its part. Each ground rule
/// names its statement within it — the register a Program fault keeps its statement in (§5.4) — so a merged
/// statement's every origin reaches its rules, two content-equal statements under different parts stay
/// apart, and no rule clones its statement. The part is the statement's as declared, `step(t)`; the instance
/// a grounding gave it, `step(3)`, belongs to the fuller observer surface the reserved seams carry (§14). Its
/// construction door lands with the observer that produces it (§11.1), as `Incumbent`'s lands with
/// `optimize` (§5.2); ahead of it one value is constructible, `Default`'s empty ground program — what a
/// backend declaring the observer exposes for the empty program. Cost: the statements and their parts' keys once, Θ(program), which a backend that
/// names a statement after `lower` already retains (§5.4), and one reference per rule.
pub struct GroundProgram { /* its statements with their parts, once each; its rules, each naming one */ }
impl GroundProgram {
    pub fn rules(&self) -> impl Iterator<Item = (&GroundRule, &PartKey, &WithProvenance<Statement>)> + '_;
        // each ground rule with the part and the statement it was instantiated from, in the order produced
    pub fn grain(&self) -> Grain;
}
/// One ground rule. Its ground head and body join with the observer that produces them (§11.1).
#[non_exhaustive]
pub struct GroundRule { /* its ground head and body; its statement, by position among the program's */ }
/// What a ground rule's statement is: a statement of the program lowered, merged as the set merges it, or an
/// occurrence of the parse, its nested provenance intact.
pub enum Grain { Statement, Occurrence }
```

This is a **committed capability, not merely a reserved seam** (§16 records the amendment against
specification §9.6). Two things make the promotion right, and both answer §9.6's "no v1 anchor forces
it / gold-plating grows the TCB":

1. **The anchor exists.** An explanation client (an xclingo-class tool) attributes answer-set atoms
   back through ground rules to source, and its whole method lives below the seam — so §9.6's "no v1
   anchor forces it" no longer holds.
2. **It does not grow the *unsafe* TCB.** `GroundProgram` is *engine-free* Rust data carrying
   `Origin`, not FFI/unsafe, so §9.6's minimality-of-the-TCB rationale does not bite. The honest cost
   it *does* add is an **adapter obligation** — a backend that exposes the value produces it
   faithfully — which the conformance suite checks (§13.1).

It is a capability over the contract, not part of the mandatory lean core: **optional, and declared** —
and this paragraph is the observer's law, which the other sites cite. `Capabilities::ground_program` says
whether a backend exposes it, read before a request is paid for (§4.1), so an explanation client learns
before it lowers whether a backend serves it; `ground_program` is a provided method answering `None`
(§4.1), so a backend that does not declare the observer writes nothing. What a declaring backend exposes
is **complete or absent**. `Some` holds the ground program as the grounder emitted it — not the engine's
own preprocessing of it — from every grounding that finished since the last `reset` or replacing `lower`,
across a multi-shot backend's accumulated lowerings and `ground` steps (§6.2). `None` holds before any
grounding has finished; after a `reset`, or a `lower` that replaces the program on a single-shot backend,
until the next one finishes — so an observation never describes an earlier program; and while the backend
needs a rebuild (§4.1, a backend's own state), whichever event put it there. It is never a prefix, a lazy
engine's fragment, or a failed grounding's partial output. On a single-shot backend a
grounding finishes within the solve, so the observer answers `Some` only once a solve has grounded. The
carrier is the contract's (above); how a backend determines which statement a ground rule came from is its
own design, stated with its observer — the mechanism, what it costs, and its grain (§11.1). The
conformance suite holds the declaration and the attribution's membership (§13.1): declared, `Some` once a
grounding has finished, every ground rule naming a statement of the program lowered — an occurrence of the
parse, at the per-occurrence grain; undeclared, `None`. Membership is not correctness, since a rule could
name the wrong statement of the program: correctness is held by a corpus whose statements ground to rules
told apart by their heads and bodies, each rule's statement checked against the corpus's known
instantiation — a check whose shape is fixed here, run once a ground rule carries its head and body, with
the observer that produces them. What the grounder emitted is engine-specific, so its content is qualified
per engine by a stated relation — the Potassco adapter's,
differenced against the clingo package's own ground-program observer (§13.2). The observer inspects a
ground program; it is not an input to an engine, and it says nothing of a search — whether one ran, or
how it ended, which a run's conclusion says (§5.2). The backend committed to exposing it is the Potassco
adapter (§11.1), the explanation client's anchor (§15 criterion 5). An observer of partial
progress, stating its own completeness, belongs to the fuller observer surface the reserved seams carry
(§14).

### 10.5 The `Symbol` correspondence, and the interning discipline

`themelios_program::Symbol` is the identity the contract shares. It carries the engine's own number width
(`i32`, `program.md` §3.1), so no value is lost or reshaped crossing the seam, and a strongly negated
atom's sign is part of its symbol, while default negation belongs to a literal, never to an atom — so an
atom's identity stays apart from both negations. An engine's own identifiers — an aspif atom, a native
engine's canonical id — are local to the engine that issued them, never a universal atom identity, and
never in the contract (§10.3).

Where an engine keeps immutable catalogs and scoped identities, as a native engine may, carrying a
`Symbol` into it is that engine's own business and owes no lock. Where an engine interns process-globally,
as the Potassco engines do, creating its symbol *handle* from a `Symbol` **is** an interning write —
serialized under its adapter's single interning discipline, not a free correspondence. The FFI cost
concentrates at that process-global interning; the adapter owns a single interning discipline
(specification §5.2) — interning writers under one lock, a reentrant-interning tripwire, and a lint over
every direct interning FFI call — so `@`-functions and located AST construction intern correctly and a
non-returning grounder call is a loud error, not a silent process-wide wedge. The discipline is the
adapter's, stated with it (§11.1), and a backend whose engine interns nothing process-globally owes none
of it. **This compensation is version-scoped to the pinned engine and retired by the spike suite**
(specification §5.2), a framing the adapter design carries.
Semantic divergences from the engine (the characterized arithmetic and safety boundaries, `program.md`
and `analysis.md`) are consciously reconciled or recorded as expected divergences at the bridge, never
erased.

---

## 11. The Potassco adapter

### 11.1 clingo and clingcon

`themelios-potassco` wires **clingo** (5.8.2, the pinned authority) **and clingcon**, each a first-class
native backend behind the contract. The crate is named for the engine *family*, not `themelios-clingo`,
because it binds more than one Potassco C library and keeps room for another: should a further Potassco
library with a stable ABI ever warrant binding, it is a feature of this crate under the same honest name,
not a new crate each. **clingcon ⊇ clingo** — it is clingo extended with an integer constraint theory —
so a theory-free program runs identically on either, while a program with `&sum`/`&dom`/global
constraints reads its per-model constraint assignments back through `TheoryAssignments` (§5.4) from the
clingcon backend. The interchange seam between the two engines (and to any further backend, the in-house
engine of §12 included) is the `Backend` contract itself (§4): the adapter for each engine is one
implementation of it, so no adapter reaches around another. What is particular to the Potassco engines is
the adapter's to handle and to state: their own input format — an aspif ingestion, under §10.3's
obligation to name every atom by its symbol or refuse — and their process-global interning, under the
discipline of §10.5. The adapter is also the backend committed to the ground-program observer (§10.4),
which it declares when its observer is built, and it reads Door A in source order (§10.2). How its
observer attributes a ground rule to a statement — the mechanism over an engine whose own observer
reports no statement, what it costs, and its grain, per statement or per occurrence — is designed with
that observer, and per-occurrence attribution is promised only once that mechanism exists.

Our own **in-house CP theory** on the propagator platform (§8.3) is **not** clingcon's replacement but
its **portable, Rust-native alternative** — written once, it runs behind *any* conforming backend, where
the linked clingcon backend is the mature C engine a deployment links directly. The two coexist by
design: linking clingcon costs one more C library in the trusted computing base, paid only by a
deployment that enables it, and bought back by a battle-tested constraint theory available immediately,
ahead of the satellite.

### 11.2 The clingo and clingcon packages as external oracles

Correctness is proved the way the syntax/program/analysis tiers prove themselves — against **external
oracles** invoked out of band: the clingo and clingcon packages pinned in the out-of-band environment (via
pixi, never linked into the shipped stack), each driven through its Python module — the route that reads a
model whole, its answer set and its display apart (§13.2), where printed output shows the display alone.
**clingo** is the grounding/solving authority over the corpus (§13.2); **clingcon** plays the identical
role for the constraint theory — the differential authority that keeps *both* the linked clingcon backend
and our own Rust CP theory (§8.3) honest on answer sets and constraint assignments. These out-of-band
*packages* are distinct from the *linked* libclingo/libclingcon of §11.1: the linked library is the
shipped backend, the package is the pinned-for-tests oracle it (and the satellite) is differenced
against, so a divergence is caught rather than trusted.

### 11.3 The trusted computing base

The adapter is the TCB under the microkernel criteria (specification §12.3): FFI calls enumerated
against a per-area manifest, each privileged operation carrying stated pre- and postconditions, the
interning discipline (§10.5) implemented once behind the capability story, engine defaults explicitly
configured per request shape so the adapter cannot transmit an ambush upward — projection off among them
(§5.2) — and panic containment on
every callback the engine makes into Rust. With the adapter feature disabled the entire stack is
FFI-free. The threat model of record (specification §12.4) lands before adapter-tier implementation
and is the security audit's object, not this design's.

---

## 12. Native backends and the fragment path

There is no naive in-house reference solver: clingo and clingcon (§11) are the external backends, and the
**in-house engine is zetesis** — a real, mature, pure-Rust answer-set solver (candidate-generation +
Ferraris-reduct checking, deliberately *not* CDNL), a co-designed sibling project — so the properties this
design leans on are stated on its author's word rather than resolvable from this repository — and already
a consumer of the base/syntax/program/analysis tiers. `themelios-solve` is designed to be **zetesis's
first-class programmatic API** — the ergonomic Rust surface a programmer drives it through — and zetesis
is a further backend behind the same `Backend` contract (§4), integrated through an adapter that
implements the contract over its solving session (§14). Because zetesis reaches the same contract from a
*radically different* architecture, it is the **second implementor** that proves the contract is not
clingo-shaped, and — sharing the program tier's `Symbol` — the standing check (§4.2) that the contract's
shapes force no conversion a shared-representation backend would never need.

The contract also opens a **fragment-backend path** a native engine can walk: because `themelios-analysis`
verdicts are sound in the direction that matters (tight ⇒ no unfounded-set check, HCF ⇒ no non-HCF tester,
Horn ⇒ no search, stratified ⇒ facts-only domains), a backend can serve a fragment and grow it up the
lattice over time, what it does not yet cover routed to another backend and the differential run on the
overlap. Today a backend serves its fragment by refusal: a construct outside its language is refused, at
`lower` or where its engine meets the statement, with a Program fault naming what it refused — the
statement, or the part (§5.4) — so nothing outside the fragment is approximated. `Capabilities` declares no
fragment yet. The declaration — with a refusal typed apart from an invalid program and from an exhausted
resource, which a router reads to send the program on, never telling the three apart by message text — is a
reserved seam that lands with the first router (§14), typed within §5.4's sum keyed by locus: an exhausted
resource is a Resource fault already, and a construct outside the fragment will be a Program fault whose
`Statement` or `Part` row carries that reason with the statement or part it names, as a Request fault
carries its presupposition (§5.4). The ambitious native engine (§14) inherits the contract and this path.

---

## 13. Assurance

### 13.1 The conformance suite

Executable, shipped with the contract, and run by every adapter, the suite holds a backend to these
obligations, in the order it runs them — the order of its `Check` enum, one obligation to an item; an
obligation a backend's declared capabilities cannot drive is skipped, and the report says so:

1. **Outcome correctness.** A corpus of small programs with independently known answer sets, each driven
   through both doors, which must agree (§10.2): each determination its known one; every yielded model
   consistent, no atom beside its contrary (query.md §2.3), and its answer set a set of literals
   (`Model::is_set_of_literals`); and, where the backend enumerates, its answer sets the known ones, its
   search closing the space. Among the corpus:
   - a program under an objective, whose `solve` yields its non-optimal models too (§5.2);
   - programs whose `#show` directives hide atoms or display terms — `a. #show.`, `{a}. #show.`,
     `q. #show p : q.`, `q. #show. #show p : q.`, `-p. #show q/0.` — whose models carry their whole answer
     sets, whatever they display (§5.1);
   - a program that projects, `a. {b}. c. #project c/0.`, whose models and consequences range over every
     stable model whole (§5.2);
   - counted repeats, which reach the engine as written: `1 { #true; #true } 1.` has no answer set, and
     `{ #true : p(1;1) } = 2. p(1).` has one (program.md §4.4);
   - a tuple counted once however many of its conditions hold: `x :- #count{ 1 : a; 1 : b } = 1. a. b.`
     has the one answer set `{a, b, x}` (program.md §4.7).
2. **The display.** Each model's display is the one the program's directives select (§5.1):
   `q. #show p : q.` displays `{p, q}` over the answer set `{q}`, and `q. #show. #show p : q.` displays
   `{p}` over the same answer set.
3. **Exhaustion is earned.** A search concluded as closing the space yielded every answer set there is —
   the `enumeration` bit's soundness obligation (§4.1).
4. **Inconsistency is exhausted.** An inconsistent reading rests on a search that closed the space.
5. **Truncation cannot pose as complete.** A stream once touched yields no complete collection — the one
   named pathology a run can attempt; the others are unconstructible in the vocabulary (§5.3).
6. **Cancellation is not exhaustion.** A cancelled search never concludes as closing the space. Over a
   program no search finishes, a search cut after a model it yielded concludes `Interrupted` — never
   `Exhausted`, `Budget`, or a fault — within the suite's bound: it reads at most the cut's cap of further
   items after the pull, the constant its time-budget probe reads to, and fails a stream still yielding past
   them, so the probe ends whatever the backend does. A pull with no solve in flight cancels no later solve,
   whether it lands before a solve, after a run ended, or after a handle dropped. The obligation binds a
   backend that declares cancellation and enumerates. A deciding backend's cut is not driven: over the corpus
   it stops at its first witness, and the suite carries no program it can rely on to keep a deciding backend
   searching, so the report says the check was not driven (§6.3).
7. **The ground-program observer**, where a backend declares it: `Some` once a grounding has finished,
   every ground rule naming a statement of the program lowered, carrying its provenance — an origin at
   least, each among the merged statement's — the membership §10.4 states, whose correctness the corpus of
   rules told apart by their heads and bodies holds once rules carry them; the fact `a.` grounding to a rule
   that names it, since the carrier holds a fact as a rule, so membership alone would hold of an observer
   that exposed nothing; and its content qualified per engine by the relation §10.4 names; a backend that
   declares none passes without it.
8. **Fault loci.** Each fault lands where it belongs: a Program fault names what it refused (§5.4) — a
   statement, located within it where the statement was parsed and unlocated where it was built in Rust,
   while a refused part is obligation 12's; a Request fault names its presupposition — assigning a truth
   value to an atom that is not external refuses with `Presupposition::NotExternal` (§5.4).
9. **A rebuild leaves nothing behind.** A second `lower` on a single-shot backend, or `reset` then `lower`
   on a multi-shot one, carries no statement, model, or cancellation of the run it replaced into the next
   (§4.1, §6.3).
10. **A backend's own state** (§4.1): a refusal `lower`'s check makes adds nothing; a failed grounding — an
    `@`-function that faults while grounding — leaves a multi-shot backend's every method that touches the
    engine refusing with `Presupposition::NeedsRebuild` until the rebuild, while on a single-shot backend it
    fails its solve alone, refused at the solve or its stream's first item: a second solve, with no `lower`
    between, meets the same grounding failure rather than `NeedsRebuild`, the lowered program having stayed,
    and a replacing `lower` then solves (§4.1, §6.3); and a registration survives the rebuild.
11. **Capability honesty.** A declared capability's method answers rightly; an undeclared one whose method
    refuses — optimization, native consequences, assumptions, multi-shot, functions, propagators — refuses
    as unsupported, naming its capability (`Presupposition::Unsupported`), while undeclared cancellation
    and an undeclared observer answer `None`; a budget neither the backend nor the core's timer realises
    refuses with `Presupposition::UnrealisableBudget`, whoever refuses it (§6.3), and an enumerating
    backend's declared time budget cuts an enumeration it cannot finish within, concluding `Budget`, never
    `Exhausted` (§5.3, §6.3); the native consequence door's answer is its known one over the corpus through
    both doors, and `NoModel` over a program with no answer set and under a scenario that admits none
    (§5.2); and the observer's declaration is honest by §10.4's law.
12. **Only the base grounds.** A single-shot solve grounds the `base` part alone (§6.3). Over `q. #program
    step(t). p(t).`, driven through both doors, which must agree (§10.2), a backend either answers as the
    base alone denotes — its every model `{q}`, and where it enumerates, `{q}` the one answer set, its
    search closing the space — or refuses at `lower` with a Program fault naming the part `step(t)` by its
    key, unlocated (§5.4), and the program lowered before it stays: the fact `a.`, lowered first, still
    answers `{a}` (§4.1's transactional `lower`, witnessed here with a part). It never yields a model holding
    `p`, never answers other than its base alone denotes, and never refuses naming a statement. No other
    check lowers the program, so every answer over it is this obligation's. The obligation binds a
    single-shot backend. A multi-shot backend instantiates its parts
    through `ground`, and whether its search covers a part lowered but not yet grounded the contract does
    not yet say (§4.1), so the report says the check was not driven.

Door A's admission is the core's, before any backend is asked (§10.2), so its refusals are the core's own
check, not an adapter's. The suite's skeleton is exercisable **engine-free over a stub backend** before any
adapter — that run is the core's own check (it streams models through the real contract), not an adapter's
authority. The **clingo and clingcon adapters** run it (clingcon adds the constraint-theory cases),
differenced against the out-of-band binaries (§13.2). The **second, architecture-independent implementor**
that proves the contract is not clingo-shaped is **zetesis** as it adopts the contract (§12) — a real engine
on a radically different architecture, stronger corroboration than a naive built-in oracle would give.

### 13.2 Differentials and oracles

The clingo package as the grounding/solving authority over the corpus, driven through its Python module
(§11.2), which reads a model's answer set under its all-atoms selection and its display under its shown
selection, so each is compared against its own (§5.1); a run's models are compared as a multiset, since
two models may display alike (`{a}. #show.` displays nothing, twice) and two with theory assignments may
share an answer set (§5.1, §5.2); the Potassco adapter's ground-program observer, where declared, is
differenced against the package's own observer by the relation stated with it (§10.4). The **clingcon
package** is the external oracle for the constraint
theory — differencing *both* the linked clingcon backend and the in-house CP satellite (§11.2), a theory
atom's by-occurrence elements among its cases (`&sum{x; x} = 4`, program.md §4.9). Beside them stand the
native-versus-derived consequence differential the tier gets for free (query.md §2.4) and the bridge
differential with its worst-case cost tripwires (§10.1). An adapter that
shares the program tier's `Symbol` (zetesis, §12) adds a further cross-implementation differential when it
lands. The **spike suite** (specification §5.2, §10.1) holds the design's version-scoped claims about the
pinned engines' behaviour — the interning compensation (§10.5), the cancellation arming's window (§4.1),
what a failed grounding leaves in the engine (§4.1, a backend's own state), the display rule (§5.1), the
consequence search's premise — that it tracks the displayed atoms, or a `#project` directive's, unless
neither is lowered (§4.1) — and the fidelity of the all-atoms selection,
which omits an atom with no solver
literal (`libgringo/src/output/statements.cc`, `Translator::atoms`): the spike establishes that it drops
no true atom in a single-shot solve and characterizes it across a multi-shot cleanup — and, for an aspif
ingestion, the stream's naming and the backend's atom–symbol correspondence (§10.3); an engine upgrade
re-runs it, re-establishing each claim or retiring the compensation it warrants.
Every instrument documents what it proves *and what it cannot* (specification §10.2).

### 13.3 The mission bar

No panic escapes the public surface on any input; every public operation documents its failure
semantics; every walk over user-reachable structure is work-list based (the depth discipline,
specification §5.2); the trust floor is minimal and legible (unsafe confined to the potassco TCB, zero
above it, FFI-free with the adapter disabled); leak- and race-checking harnesses at the TCB; the
scaling-shape benches, each beside an in-suite scaling tripwire, assert complexity class for the
load-bearing operations (the bridge's conversion linear in program size, §10.1; the query tier's matching
scan; the agent's knowledge-ledger rebuild), and streaming enumeration's constant resident set on the
owned side is asserted by the laziness law — a bounded pull count that goes red if the core's handle
pulls a model ahead of its consumer (§5.2); what an engine holds behind its run is the engine's.

### 13.4 Examples as a deliverable, and the witness roster

The build ships an **executed example set covering at least the specification §3 witness roster** —
first-solve, enumeration, optimization, multi-shot, blame, consequences, three-valued query,
extraction, `@`-functions, propagation, and the rest of the roster (theory-uniformity,
solve-extension, hostile-input, cancellation, budget, comparator-evidence, transformation, round-trip,
comments-as-data, diagnostics-quality, asp-core-2) — held to a pedagogical bar (the examples teach,
honestly, about the pitfalls), so a user learns what themelios provides by reading and running them.
Each scenario ships **both faces** — a macro form and a composable programmatic form — run and diffed
against each other as a free correctness check, which also demonstrates §3.1's coherence and §3.2's
macro=sugar interaction.

Two roster points this tier discharges specifically:

- **The reactive tier has *no prior comparator* witness** (specification §3.1 lists no evidenced
  comparator for `solve-extension` or `theory-uniformity`) — so it gets new examples that establish
  the bar rather than exceed one: a worked `@`-function (`ground-extension`, witness 13) and a worked
  propagator (`solve-extension`, witness 14 — the difference-logic witness).
- **`theory-uniformity` (witness 15) is discharged two ways.** With the clingcon adapter restored as a
  first-class backend (§11.1, §16), theory-uniformity is witnessed by the **linked clingcon backend** —
  a `&sum`/`&dom` program whose per-model constraint assignments read back through `TheoryAssignments`
  (§5.4) — *and* by a worked **CP propagator** on the in-house platform — `alldifferent` + `&sum`, real
  global-constraint CP — whose assignments read back through the same typed component, with the
  agent-driving and outcome-reading code identical to first-solve but for the propagator registration.
  The two agreeing on that typed component *is* the uniformity, and the clingcon binary keeps both honest
  (§11.2). The full best-of-breed CP theory is the satellite (§8.3, §14), of which the propagator witness
  is the in-themelios floor.

The examples **span the tiers** (authoring exercises program+macros, driving exercises solve, reading
exercises query), making the crate-home split visible in the learning surface.

---

## 14. Scope and reserved seams

**The whole stack is the target.** There is no minimal "first increment" and no deadline pressure; the
mission-critical quality bar governs the pace, not a ship date. When the stage is done, the complete
tier ships: the contract, the outcome vocabulary and values, the agent and full multi-shot (the
reasoning loop), all four centerpieces and extraction, the bridge and its ground-program-IR capability,
the potassco **clingo and clingcon** adapters, the facade, and the example set (reactive-tier witnesses
included). The query tier ships with it (`query.md`). The in-house engine **zetesis** is a further
backend behind the contract, integrated as it and `themelios-solve` line up (below); it is not a member
crate of this tier.

The **reserved seams** are only the genuinely-separate:

- the **theory-extension satellites** — a **full in-house CP theory (the clingcon alternative,
  exceeding clingcon-5's set)**, difference logic, and linear/real arithmetic — which are their own
  repositories built *on* the propagator platform, not part of this tier (the tier ships the platform,
  the difference-logic witness, and the CP theory-uniformity witness of §13.4 — not the satellites);
- **multi-threaded propagation** (the engine-level parallel-propagation problem, specification §9.6 —
  distinct from the intra-propagator parallelism of §8.4, which ships);
- **projective enumeration** — a program's models enumerated up to their projection onto the atoms a
  `#project` directive names: until it is designed as a request of its own, `solve` yields every stable
  model whatever the program projects, and an adapter keeps the directive from its engine (§5.2);
- **streaming ground input** — a checked, typed source of semantic statements, or of views over them, for
  a foreign grounder that must stream rather than hand over a ground `Program`. It keeps the atom
  vocabulary, element identity, objectives, observations, and the provenance it has; a complete import
  ends in an explicit, successful finalization, and a stream that fails leaves an incomplete prefix, never
  a complete program; its ownership and batching are settled before its signature is. It is transport,
  not lazy grounding: a lazy engine keeps the source program, or a justified producer plan, and
  establishes the instances each conclusion needs, so the ground fragment buffered so far certifies no
  answer set and no exhausted world view. Until a consumer needs it, ground input is a ground `Program`
  through Door B (§10.2);
- **the fragment declaration** — a backend's language fragment declared in `Capabilities`, with its
  refusals typed three ways within §5.4's sum keyed by locus — a construct outside this backend's fragment,
  an invalid program, an exhausted resource — which a router reads to send the program on, never by message
  text (§12). Until that router, a backend serves its fragment by refusing, at `lower` or where its engine
  meets the statement, naming the statement or the part it refused (§5.4) — a single-shot backend that
  admits only `base` among them (§6.3);
- **a located part refusal** — a part carrying its `#program` delimiter's origin, so a part refused over
  parsed text lowers to a diagnostic at the delimiter (program.md §4.1, §8). Until a consumer needs it, a
  part refusal is unlocated and renders through its `Display` (§6.3);
- **`ground` over part instances** — `ground` taking a part with its ground arguments, `step(3)` for the
  declared `step(t)` (the distinction §10.4 draws), so a part with formals has a route to its instances.
  Today `ground` takes declared parts, and the agent's replay retains them (§6.2); the slice that builds
  multi-shot grounding brings it;
- **an agent door from an `Admitted`** — an agent whose first lowering is through Door A, its rebuilds
  through Door B, for a consumer that needs the occurrences across the reasoning loop (§10.2); until one
  does, a client composing over `Backend` walks through Door A, and the agent's loop runs over the set;
- **request-side limits beyond time** — a model-count cap, and a configured ceiling a consumer wants read as
  a budget (§5.1): each a field of the request, with a `budgets` declaration that discloses its enforcement
  before the request is paid for (§4.1, §6.3), grown when a consumer needs it;
- **`Send` live handles** where a backend allows them — the live handles are `!Send` because a foreign
  engine's control may be single-threaded (§5.2), while a native engine whose solving state may move
  between threads can preserve that transfer for its embedders; the contract will allow it for such a
  backend without requiring it of a foreign one;
- the **native grounder and solver** — a separate engine that implements this contract and can walk the
  fragment-backend path of §12. This is **not hypothetical**: **zetesis** — a clingo-free answer-set
  engine on a candidate-generation + Ferraris-reduct-checking architecture (deliberately *not* CDNL),
  a co-designed sibling project — is a real, mature engine, and `themelios-solve` is designed to be its
  first-class *programmatic* API (§12). Its integration is an **adapter implementing `Backend` over its
  solving session**, near-term rather than hypothetical, gated on two things lining up: a public zetesis
  door from a themelios `Parse`/`Program` value (render-then-parse is forbidden, §10.2), and
  `themelios-solve` **landing on `main`** — the pinnable rev zetesis pins, an event earlier than the
  whole tier being *done* (§15), so the gate is not circular. The contract's **architecture-neutrality is argued, not
  asserted**: the `Determination`/`Conclusion` split (§5.1) separates the logical question from the search
  question, so no engine's operational vocabulary can leak into the surface — and that an engine on a
  *radically different* (non-CDNL) architecture reaches the very same contract bears that neutrality out
  (corroboration, not the proof, which is §5.1). zetesis slots in behind `Backend` as a further backend
  when its adapter is built; being one-shot today, it declares `multi_shot: false` (with `assumptions` and
  `externals` absent) and refuses the reasoning loop's mechanisms (§4.2) until, if ever, it grows them;
- the **normalised, cross-backend statistics schema.** v1 *does* ship statistics — the clingo adapter
  exposes clingo's own, engine-scoped and provenance-marked, behind the **`Statistics` trait whose v1
  shape §5.4 states**, so a clingo-backed user keeps a capability the comparator has (§15 criterion 2).
  What is reserved is the
  *normalised cross-backend schema*: clingo and zetesis measure *different work* by construction — CDNL
  decisions/conflicts/restarts versus region-candidate search, reduct-closure rounds, and gate coverage —
  so a normalised schema drafted with one engine live would be guesswork. Because v1's statistics already
  sit behind the trait, that normalised surface — designed when a second engine (zetesis) is live and both
  measurement models can be read together — is a **later typed view that consumes the trait, an additive
  drop-in** (it touches neither the trait nor `Measurement`), not a breaking change; the future
  **multi-backend benchmarking driver** (themelios-solve as a neutral harness
  over a corpus) builds on the trait;
- and the standing specification seams that touch this tier: the ground-program observer's fuller
  surface beyond what the committed §10.4 capability delivers, formal-methods tooling over the TCB,
  and additional engine backends beyond the Potassco family.

Each is named with its reason; none is a silent gap.

---

## 15. Acceptance criteria

The tier is done when all of the following hold:

1. **The contract is real; v1 ships on the Potassco adapters, with the independent-implementor proof a
   named post-v1 obligation.** *Done* for v1 requires: the potassco **clingo and clingcon** adapters both
   pass the conformance suite; the pathologies are unconstructible; faults land at their loci; capability
   honesty holds. clingo and clingcon share Potassco machinery, so — by the specification's own reading
   (§9.5) — they are **not** the solver-agnostic seam's second *independent* engine; that engine is
   **zetesis** (§12), on a non-CDNL architecture, and its passing the conformance suite is the standing
   proof the contract is not clingo-shaped — zetesis alone, with no Potassco dependency, driving the
   conformance corpus through both doors, and a program built in Rust (the constructor macros' among them)
   through Door B, in each execution configuration it admits — eager, hybrid, and lazy grounding, on its
   author's word (§12). Because zetesis's integration is
   gated on `themelios-solve` landing (§14), that proof is a **post-v1 obligation carried with its residual
   risk** — the risk that the contract is subtly clingo-shaped until a truly independent engine exercises
   it — not a v1 done-condition.
   So this criterion is satisfiable as written, and the independent proof is named, not silently dropped.
2. **The two APIs exceed the evidenced comparators in capability and ergonomics** — a clean
   declarative macro face and a clean composable programmatic face, both first-class, coherent end to
   end, held to the Rust-exemplar bar and the comparator against clingo's Python API (specification
   §3.1).
3. **Full multi-shot fidelity, as the reasoning loop.** The agent drives the Gelfond–Kahl loop —
   observe / assert / retract / ask — over retained program-side state: assumptions, `#program` parts,
   repeated ground/solve, externals, and statement-level assertion and true retraction, no
   lowest-common-denominator. A single-shot question is the loop's body run once (§6.4). A backend that
   lacks the loop's mechanisms declares them absent (§4.1) and refuses rather than degrading — a native
   one-shot engine is a conformant backend.
4. **The extension surfaces are complete and pleasant.** `@`-functions (with the library door and the
   `Facts` bulk conversion) and the propagator platform (validated against the DL/CP/LP litmus, the CP
   half a *full* constraint theory), each idiomatic-Rust and engine-portable; extraction over the
   conversion pillar.
5. **The committed clients build with minimal friction, in a stated order.** The design is validated
   against the definitely-planned arm's-length products — each its own repository on the keryx/morphe
   pattern — built in the order they stress the surface, from the primary register outward: **(1)
   elenctic**, the reading/query half (`query.md` §4), built first and the near-term priority (it
   retires the standing Python project); **(2) the full in-house CP theory** — the *portable alternative*
   to the linked clingcon backend, coexisting with it (§8.3, §11.1) — on the propagator platform, which
   exercises the deepest seam and against which the design is pressure-tested up front (§8, the worked CP
   witness §13.4); **(3) xclingo**, the explanation half, over the ground-program-IR capability (§10.4).
   Alongside these, **zetesis** is a first-consumer of a different kind: as `themelios-solve`'s in-house
   engine adopts the contract, it **audits the solve/query API** as a real independent engine — the
   keryx/morphe/elenctic first-consumer checkpoint, aimed at the `Backend` seam. A **theory-driven service
   consumer** (the pythia-class boundary) rides on these. Each is a first-consumer checkpoint that closes
   before the surface it drives is called done and then continues as a product; elenctic, xclingo, and
   zetesis may drive additive surface revisions afterward, the way keryx/morphe drove the program/syntax
   regularity pass. The standard is absolute: *if a client cannot be built cleanly on the abstractions,
   the design is short.*
6. **The bridge is fast and faithful** — the differential against the engine and the worst-case
   tripwires green (§10.1, §13.3).
7. **The example set ships** (§13.4), and the mission bar holds throughout (§13.3).

---

## 16. Architecture, trust, and dependency reference

**Amendments to the specification, recorded here:**

- **Crate roster (§12.2).** The four adapter crates collapse to `themelios-potassco(-sys)`, which binds
  **clingo and clingcon** (§11.1); the query surface splits into the `themelios-query` sibling
  (`query.md`). There is **no reference-solver crate** — the in-house engine is zetesis, a separate sibling
  project behind the contract, not a member of this tier (§12).
- **The reference solver (specification §1.1, §2 item 4, §9.1, §9.5, §10, §11 build-order item 6, §12.2,
  §12.5) — REMOVED.** The specification's naive pure-Rust reference solver is removed as scope creep. It
  filled several roles across the specification, each now re-homed or carried forward: the
  solver-agnostic seam's **second independent implementor** (§9.5, §10) and the **native-backend seed**
  (§1.1, §12.5) are **zetesis** (§12), which — on a radically different architecture — is the independent
  implementor a private in-house solver could never be as convincingly; the **small-case oracle** (§2 item
  4, §9.1) is the clingo/clingcon binaries (§11.2) plus the engine-free stub the conformance suite runs
  against; the **build-order item** (§11 item 6) and the **crate-roster entry** (§12.2) are struck. The one
  clause this *weakens* rather than re-homes is §9.5's "the seam's second engine is the reference solver":
  the second *independent* engine is now zetesis, a **post-v1 obligation** (§15 criterion 1) gated on the
  tier landing (§14), so v1 ships with the two Potassco adapters and the independent proof carried forward
  with its residual risk. This is a considered supersession on the record, not a silent descope.
- **The clingcon adapter (§9.5, §4, §2 item 5, §12.2) — RESTORED.** An earlier revision permanently
  superseded the specification's clingcon adapter with the in-house CP theory; **that supersession is
  reversed.** clingcon is **restored as a first-class native backend** in `themelios-potassco` (§11.1),
  realigning with the specification: the §9.5 clingcon adapter ships, specification §2 item 5 ("clingo and
  clingcon are one experience") is honoured directly (a theory-free program runs identically on either;
  clingcon ⊇ clingo), and witness 15 (`theory-uniformity`) is discharged by that linked backend *and* by
  the in-house CP propagator (§13.4). The in-house CP theory (§8.3) is **not** clingcon's replacement but
  its **portable, engine-agnostic alternative on the propagator platform** — the two coexist (§11.1). This
  restoration costs one more C library in a deployment that enables it (§11.1) and settles §4's "absent
  without the contingency invoked is a failure" the plainest way: clingcon is present, not absent.
- **The outcome's record of which consequence path ran (§9.1) — DEFERRED.** The specification's "the
  outcome's provenance then records which path ran" is deferred to its first consumer: the capability
  declaration discloses the path before the request is paid for (§4.2) — a disclosure, not a record: no
  outcome carries the path — and the retraction class discloses, on the `StatementId` it rides, which
  realisation a retraction takes (§6.2). An outcome that records either lands when a consumer reads it.
- **The ground-program observer (§9.6, §13).** Promoted from a reserved seam to a **committed
  capability** (§10.4) — optional to a backend and declared, complete where exposed, and committed by
  the Potassco adapter — with the two-part argument that defeats §9.6's rejection (an xclingo anchor now
  forces it; it is engine-free data, not added unsafe TCB — the cost is an adapter obligation).
- **Intra-propagator parallelism (§9.6).** Ships as a v1 capability the specification does not mention
  (§8.4); consistent with §9.6's "single solver thread," and distinct from the reserved engine-level
  multi-threaded-propagation seam.
- **The driving surface (the multi-shot presentation).** Presented as the **agent and the reasoning
  loop** (§6): an `Agent` is a `Program` reified as its knowledge base, driven by the Gelfond–Kahl
  observe/modify/ask/act loop, with a declarative `assert`/`retract`/`observe`/`forget` register beside
  the retained `#program`-part and external mechanisms. It rests on the existing contract — retraction is
  realised via `lower` (rebuild) or `assign_external` (toggle), so no new required `Backend` method — and
  the question vocabulary is shared with a single-shot surface on `Program` (§6.4). The specification's
  full-multi-shot obligation is met in full; only its presentation changes, from a session/control model
  to the agent loop.
- **Statistics.** v1 ships engine-scoped statistics behind a `Statistics` trait (§5.4; the clingo
  adapter exposes clingo's own, provenance-marked); the **normalised cross-backend schema** is the
  reserved seam (§14), a later typed view that consumes the trait — an additive drop-in when a second
  engine is live and the divergent measurement models can be co-analysed.

**Trust architecture.** `themelios-solve` and `themelios-query` are `forbid(unsafe_code)`, FFI-free by
dependency closure. `themelios-potassco-sys` carries the vendored bindgen output; `themelios-potassco` is
the sole `allow(unsafe_code)` TCB, mechanism-only over the bindings plus the safe adapters for clingo and
clingcon (§11.3). The structural trust check asserts forbid-in-pure-crates, allow-only-in-the-named-TCB,
and FFI-free closures for the pure crates. With the potassco feature disabled the stack is FFI-free.

**Dependency policy.** `themelios-solve` and `-query` take nothing beyond the lower tiers and the
proc-macro toolchain (for the solve-adjacent macros, which live in `themelios-macros`); intra-propagator
parallelism, where a propagator wants it, is the *satellite's* dependency, not the tier's. The `-sys`
crate's bindgen is feature-gated and never in a default build. Every dependency carries an argued
necessity where it is declared.

---

## 17. Revisions

1. **Initial design of record** (2026-09-03).
2. **Refinements** (2026-09-03). `Facts`, `Assumption`, and `Scenario` defined as typed surfaces
   (§7.3, §6.3), the conversion-pillar homes corrected (§3.2). Raised to the type/interface/cost-model
   register of the built siblings throughout (§4–§10), the propagator trait stated as interface +
   governing principle (§8.1). The `program.md` §18 mis-citation corrected to §7/§8 (§10.2). The
   specification amendments — the committed ground-program observer, the permanent clingcon
   supersession with witness 15 discharged, and intra-propagator parallelism — consolidated and
   recorded (§16). The CP target stated as a *full* constraint theory (`alldifferent` and the global
   family), the in-house clingcon alternative's ambition to exceed clingcon-5 recorded (§8.2, §8.3,
   §14). Reactive-tier witness coverage corrected to "no prior *comparator*" and the roster enumerated
   (§13.4).
3. **Completeness refinements** (2026-09-03). The `Backend` trait now marks each method required vs.
   core-provided, with the minimality argument (§4.1). The `solve → Determination → WorldView` seam is
   specified: `Determination`'s variants carry payloads (`Models` / `Unsat` / `Partial`),
   `Solved::determination` and the `Models`→owned-`WorldView` move are stated, and `AnswerSet` is
   re-exported from the program tier (§5.1–§5.2). `Solved::all_answer_sets` added as the
   exhaustion-gated completeness accessor, making the truncation pathology structurally unconstructible
   (§5.2–§5.3). The native consequence door's obligation to range over the *optimal* set under an
   objective is stated (§5.2). `Scenario` is given its §1.4 reason as the reusable named bundle, the raw
   set named "assumptions", and blame carries the raw responsible subset (§6.3, §5.4). `Facts::facts()`
   corrected to yield `Symbol` (§7.3). The `themelios-potassco` family-name warrant leads with
   forward-compatibility (§11.1). The clingcon supersession's revised §4 form is restated (§16).
4. **The reasoning-loop reframe** (2026-09-23). The driving surface is recast from a session/control
   model to the **agent and the Gelfond–Kahl reasoning loop** (§6): `Agent<B>`, a `Program` reified as
   its knowledge base via the C-CONV owning conversion `into_agent`; a declarative
   `assert`/`retract`/`observe`/`forget` knowledge register — true retraction realised over the existing
   contract (external-toggle or rebuild), no new required `Backend` method — beside the retained
   full-fidelity `#program`-part and external mechanisms; and the question vocabulary shared with a new
   single-shot surface on `Program`, single-shot being the loop's body run once (§6.4). The reading resolves through a consuming
   `Solved<'a>`/`Optimized<'a>` → `Determination<'a>` resolver (§5.2) that threads the engine borrow:
   `agent.determination()`/`p.determination()` return the trichotomy (`Fault` reserved for engine/request
   errors), the `WorldView` read from `Consistent` is the live, engine-driving handle (borrowing a
   retained agent, or owning an ephemeral engine single-shot) with fallible `&mut` reads, while
   `WorldView::materialize` yields an engine-free `Snapshot` with infallible reads; `Optimized` shares
   `Solved`'s resolution register. The native-engine seam names
   **zetesis** — a co-designed, clingo-free candidate/reduct engine (non-CDNL) built to this contract,
   anchoring the seam, with architecture-neutrality argued on the §5.1 split (zetesis corroborates, not
   proves) — and v1 ships engine-scoped statistics behind a `Statistics` trait (v1 shape stated §5.4), the
   normalised cross-backend schema reserved as a later drop-in view that consumes the trait (§14). The committed
   clients are ordered elenctic → clingcon → xclingo, elenctic first and the near-term priority, with the
   design pressure-tested against clingcon's deep seam up front (§15). Session/driving vocabulary updated
   throughout (§2–§3, §7, §13); the amendments are consolidated in §16.
5. **Refinements** (2026-09-23/24). The `Backend` contract gains the method behind the `cancellation` bit —
   `interrupt(&self) -> Option<Interrupt>`, a provided method defaulting to `None` (a cancelling backend
   overrides; the conformance suite checks `cancellation ⇒ interrupt().is_some()`), over which the
   request-side time budget and `Agent::interrupt` are realised (§4.1, §6.3) — and the multi-shot `reset`
   door, the tear-down the rebuild-class retraction needs because `lower` *accumulates* into a multi-shot
   engine's program (so `assert` lowers only the delta and a rebuild is `reset` then one `lower`); on a
   **single-shot** backend `lower` *replaces* the program, so a rebuild is one `lower` and `reset` is not
   called (§4.1, §6.2). The `Models<'a> → WorldView<'a>` transition lives on the query side
   (`themelios_query::WorldView::of`, query.md §2.7), so `themelios-solve` does not depend on the query
   tier (§5.2, §6.4). `ConsequenceRequest` carries the assumptions it ranges over, so a scenario-scoped
   cautious/brave ranges over that scenario's models (§4.1, query.md §2.4). The propagator *registration*
   seam is object-safe, the `State`-bearing trait being the built shape (§8.1). `Fault` is
   stated as owning its model and lowering to a `themelios-base` `Diagnostic` only through `LocatedFault`
   where it carries a `Location`; an unlocated fault renders through `Display`, not a fabricated span at
   an unknown source (§5.4). The `@`-function `Function` result is `Vec<Symbol>` — `program.md` §3.4's
   `IntoIterator<Item = Symbol>` shape — keeping `themelios-solve` free of any dependency beyond the
   lower tiers (§7.1, §16). Doors A and B are recast as two *entry values* (`&Parse<ast::Program>` and
   `&Program`) into one grounding mechanism, with the engine's ground-by-construction backend named as
   Door C: a non-ground program crosses only through A or B, never the ground-object backend (§10.2). The
   §10.5 symbol correspondence is corrected — a `Symbol` carries the engine's number width, but creating
   the engine's symbol handle is an interning write under the single discipline, not a free
   correspondence. The §3.1 block-macro name is corrected to `program!`.
6. **Clingcon un-supersession, reference-solver removal, and the agent-facade reconciliation**
   (2026-09-25). Three accumulated changes reconciled in one pass. **(a)** The clingcon adapter's
   permanent supersession (revision 2, §16) is **reversed**: clingcon is restored as a first-class native
   backend bound by `themelios-potassco` alongside clingo (§1.1, §2.1, §11.1); the in-house CP theory is
   recast as its *portable alternative* rather than its replacement — the two coexist (§8.3); the clingo
   and clingcon binaries are the out-of-band differential oracles (§11.2, §13.2); and witness 15 is
   discharged by both the linked clingcon backend and the CP propagator (§13.4). Specification §2 item 5
   is honoured directly again, not reinterpreted (§16). **(b)** The naive in-house **reference solver is
   removed**: there is no `themelios-reference` crate, and §12 is repurposed to *native backends and the
   fragment path* — the in-house engine is **zetesis**, a real non-CDNL pure-Rust answer-set engine behind
   the same contract, for which `themelios-solve` is the first-class *programmatic* API; it is the
   architecture-independent second implementor and the §4.2 standing check (§1.1, §12, §13.1, §14).
   Acceptance criterion 1 is reframed to be satisfiable as written: v1 ships on the two Potassco adapters,
   with the architecture-independent proof (zetesis passing conformance) a **named post-v1 obligation**
   carried with its residual risk and gated on the tier landing, not a done-condition (§14, §15); §16
   records the reference-solver removal against every specification clause it supersedes (§1.1, §2 item 4,
   §9.1, §9.5, §10, §11 item 6, §12.2, §12.5). Criterion 5 adds zetesis as an API-auditing first consumer
   (§15). The trust architecture's engine-free crates are `themelios-solve` and `themelios-query` only
   (§16). **(c)** The agent-facade drift is reconciled with `query.md`: the cautious/brave consequence door
   and the epistemic readings live on the **agent** (and on a materialised `Snapshot`), not on the live
   `WorldView`, so a reading is a self-contained solve rather than a drain of a live handle (§5.2, §6.2;
   query.md §2). **(d)** Several surfaces are brought into line with the built code and made honest: the
   capability-gated `Backend` methods are provided defaults that refuse, the "required under the bit"
   obligation enforced by the conformance suite's positive-capability check (§4.1); the `enumeration` bit's
   soundness obligation is stated (§4.1); scenario-scoped readings are first-class surface —
   `cautious_assuming`/`brave_assuming` and `snapshot_assuming`, mirroring the unscoped pair the way
   `solve_assuming` mirrors `solve` (§4.1, §6.2, query.md §2.4/§2.7); the live run handles are `!Send` and `Snapshot` the `Send` form
   (§5.2, §6.1); the epistemic readings' bare/loop asymmetry is named (§6.4); zetesis is marked a private
   sibling project (§12); and a residual reference-crate claim in the opening sentence is struck. The
   scoped consequence doors carry their precondition (they require `assumptions`, deriving over
   `solve_assuming`) and cost, and `snapshot_assuming` its signature/refusal/cost (§4.1, §6.2, query.md
   §2.2); the §4.1 trait sketch draws each gated method's refusing body; and the `!Send` reason is the
   `Run` object's absent `Send` bound, not the borrow (§5.2, §6.1). The backend-facing **`Run` protocol**
   and its **`Solved::running`** construction door — how a backend (or a native engine) supplies the
   enumeration the core classifies over — are now stated in §5.2, and the scoped-door contract is given a
   single normative home (§6.2 for the agent doors, query.md §2.2 for `snapshot_assuming`), the other
   mentions reduced to citations.
7. **The model unit, objective-off solve, and outcome refinements** (2026-09-29). The streamed and
   collected unit is a `Model` — an answer set and the theory assignment a theory-evaluating backend
   supplies with it, empty otherwise — so a theory backend populates the unit rather than reshaping it:
   `Solved::models`/`all_models` and the backend `Run::next_model` (§5.1, §5.2). `solve` enumerates the
   stable models with any objective ignored, and `optimize` owns the optimal set; each question's
   consequences and world view range over its own model set (§5.2). An `Inconclusive` search stopped by
   an engine fault reached no conclusion — `Partial::conclusion` is `None` and `Interrupted` names
   cancellation alone (§5.1) — and a faulted run never concludes `Exhausted` (§5.2). The cautious/brave
   fold is stated as an exposed primitive that answers nothing over no members and certifies no
   completeness (§5.2). The outcome's record of which consequence path, or retraction realisation, ran
   is deferred to its first consumer, the before-the-fact disclosures serving meanwhile (§4.2, §6.2,
   §16). A located fault is a program fault (§5.4). Streaming's constant resident set is asserted by the
   laziness law, and the matching scan and the ledger rebuild by benches beside their tripwires (§13.3).
8. **The stopping reason, objective-off on the contract, and the model-set seam** (2026-09-29). An
   inconclusive search's stopping reason is one closed shape, `Partial::stopped` → `Stopped::Concluded` or
   `Stopped::Faulted`; a `Run` concludes only when its stream ends without a fault, and after a fault
   `conclusion()` stays `None` while the core records the fault (§5.1, §5.2). `solve`, `solve_assuming`,
   and the native consequence door owe the objective-off enumeration, and the conformance suite checks it
   with a program under an objective, and checks every yielded model consistent (§4.1, §13.1). Until
   optimization is realised every world view ranges over all stable models; the optimal marker's producer
   and the native door over the optimal set are one seam that lands with `optimize`, the handle fixing the
   set (§5.2). A rebuild re-establishes the state the loop carries after its lowering (§6.2); the core
   attributes a timed stop, a tie going to the caller's cancellation (§6.3). The fold's `None` rests on
   non-emptiness; located faults are program faults, stated once (§5.2, §5.4). The results are *typed
   values*, so *model* keeps its logical sense; `Model` meets the constraint answer set, states its cost,
   and marks its assignment-bearing door; the deferred path record is worded as a deferral (§1.3, §4.2,
   §5, §5.1, §16).
9. **The native answer, the truncation, and the rebuild's cost** (2026-09-29). The native consequence
   door reports what the engine's search established — a `NativeAnswer`: a set over a closed space, no
   model, or a stop short of the space — and the core gates it and builds the `Consequences`, so a
   truncated native search cannot pose as complete and a backend cannot forget either refusal; when
   `optimize` lands the native door owes a request field naming its set (§4.1, §5.2). An inconclusive
   search concludes at a `Truncation`, so an exhausted conclusion is unrepresentable in its stopping
   reason (§5.1). A rebuild's cost states the replay, the grounded parts before the assigned externals
   (§6.2). The handle `Agent::interrupt` returns is the core's, which is how the core attributes a stop
   (§6.2, §6.3). The results are values at the three sites that still called them models, and a
   reference resolves (§2.2, §3.2, §5.2, §14).
10. **One home for the model-set law, the cancellation primitive, and the native answer's grade**
   (2026-09-29). The model-set law has one normative home, the other sites citing it (§4.1, §5.2). The
   backend's cancellation primitive is its own trait, `Cancel`, and `Interrupt` the core's handle over it
   (§4.1, §6.2, §6.3). The native answer's guarantee is stated at its grade: the refusals' decision and
   wording are the core's, the report's honesty the backend's, held by the conformance suite and the
   differential (§5.2, §13.1). The backend-bug bit and the adapter locus coincide today, and the case that
   parts them is named (§5.4). Form: the model's reason is its own paragraph, and the `Models` note is
   prose (§5.1, §5.2).
11. **The cancellation primitive's other states** (2026-09-29). A pull with no solve in flight, and a pull
   after the backend is dropped, are no-ops: an adapter over an engine whose primitive cuts the following
   call compensates, and shares ownership of what the pull reaches (§4.1). The conformance suite checks the
   stale pull beside the attribution, once cancellation is realised (§6.3).
12. **The cancellation primitive's cost, premise, and referent** (2026-09-29). A pull signals and returns in
   `O(1)`, never blocking on the solving thread; the arming compensation's premise is a version-scoped claim
   the spike suite establishes, the race harness holding the concurrent close; and a pull reaches a slot the
   backend owns and clears on drop, so the engine's lifetime stays the backend's (§4.1).
13. **The arming window, the budget's realisation, and the spike suite** (2026-09-29). The cancellation
   arming is bounded on both sides, its window coinciding with the engine's active call (§4.1). A budget's
   realisation is fixed by the declaration: `budgets.time` enforces natively, `cancellation` lets the core's
   timer enforce it, neither refuses (§4.1, §6.3). The spike suite is named among the assurance instruments,
   with the two claims it holds (§13.2). A model states whether it is consistent (§5.1), and the one
   process-wide value, the ledger brand, confers no authority (§6.1).
14. **The answer set, the display, and the Program fault's statement** (2026-10-01). An answer set is what
   the literature means by one — the ground literals true in a stable model, every member a function
   symbol (`Model::is_set_of_literals`) — and a model carries it whole, whatever the program's `#show`
   directives display (§5.1, the law's one home, which the other sites cite). A term directive displays
   symbols that are no true atom — `q. #show p : q.` displays `p` — so a reading over the display could
   answer *yes* of a false atom: the readings, the fold, and the native consequence door all range over
   answer sets, and a door whose engine tracks only the atoms it displays, or a `#project` directive's, is
   driven with every atom tracked or not declared (§4.1, §5.2). The display is a type of its own, `Shown`,
   so passing it where an answer set is wanted is written at the call — the `AnswerSet` alias kept, its two
   laws disciplines, the tradeoff stated (§5.1). The display's two halves have two homes: the restricting
   half — the signature forms and `#show.` — is the `ShowRule`, which a backend hands the core with its run
   (`Solved::running`) and the core applies as each model streams; only the term half, which needs a
   grounder, is the backend's (`Model::with_terms`), so a backend writes engine mechanism and no derived
   reading, the directives in force being those lowered and not since `reset`, whatever their part (§5.1,
   §5.2). An adapter lowers none of the restricting directives and no `#project`, so its engine's
   consequence search ranges over every atom; projective enumeration is reserved, `solve` yielding every
   stable model whatever the program projects (§5.2, §11.3, §14). The display semantics and the
   consequence search's premise are the authority's, cited, and version-scoped (§4.1, §5.1, §13.2).
   `all_models` keeps one model per stable model, two that display alike included (§5.2); `Extract` reads
   a set of symbols, the answer set or the display named at the call (§9); and the bridge lowers every
   entry of a counted collection (§10.2). A Program
   fault keeps its refused statement with its provenance, so it names the statement whether located or not
   and is located only from a parsed origin — the least as its primary label, the others as secondaries; a
   fault's equality compares its statement's content and origins, and `Fault` is not `Hash`; the
   construction doors are stated (§5.4). The conformance suite gains the display and projection corpus, the
   literal-membership and display checks, and the named, unlocated refusal (§13.1); the oracles are the
   clingo and clingcon packages driven through their Python modules, comparing answer sets and displays
   apart and a run's models as a multiset, and the spike suite gains the display rule, the consequence
   search's premise, and the all-atoms selection's fidelity (§11.2, §13.2).
15. **The semantic boundary** (2026-10-02/03). The doors carry the language's objects, and an engine's own
   format stays with its adapter. `Door` is `Parsed | Program`, and every backend takes both: Door A an
   `Admitted` parse — its statements raised once, in source order, each with its part and provenance, the
   core's `Admitted::of` refusing, by one rule of severity, a parse that does not raise cleanly with a typed
   `NotAdmitted` (`Syntax` or `Lowering`) before any backend is asked, so the admission rule has one home
   and the raise is its one authority; an `Admitted` certifies a clean raise and nothing of a backend's
   language, arithmetic, or resources — and Door B, the primary entry, ground or not, a ground program
   being a ground `Program` lowered as any program is; `Door::program` lends either as a set. A backend
   reads Door A in source order or as the set, its design recording which — the Potassco adapter, source
   order, input fidelity rather than an execution order — and a client may rely on the answer sets, every
   parsed origin, and each statement's part under either, admission having refused every repeated global
   definition; Door A is an entry of the contract, which a client composing over `Backend` walks through,
   the agent's loop running over the set (§3.1, §10.2, §11.1). The aspif door, its sink, its identifiers,
   and the interning discipline leave the contract for the adapter whose engines need them, which owes the
   contract's answer set from its own format — every atom named by its symbol, or the stream refused,
   since an aspif stream names an atom only where it displays one, both claims version-scoped and held by
   the adapter's spike suite (§10.3, §10.5, §11.1, §13.2). So the answer-set law holds at every door. A
   fault refers to what it refused, a closed sum keyed by locus (`Refused`): a statement or a parse — a
   Program fault, the program being where it lies, a refused parse included — the request, a Request fault
   naming the presupposition that failed (`Presupposition`, non-exhaustive, each variant a refusal the tier
   makes, an unsupported request naming its `Capability`, which joins the contract), or nothing, every
   Resource, Engine, or Adapter fault, told apart by its locus and its bit; so a consumer acts on every
   refusal of the source or the request by matching, never by reading the message. The sum is closed on
   purpose: a row grows a typed reason inside it when a consumer first reads one, the `Statement` row a
   router's. One event has one presupposition — a budget nothing realises is `UnrealisableBudget`, whoever
   refuses it — and only the six capabilities whose methods refuse are named by `Unsupported`. A run that
   ends with neither a fault nor a conclusion is an Adapter fault. A fault lowers to zero,
   one, or several diagnostics (`Fault::diagnostics`); a Program fault may arise at `lower` or where the
   engine meets the statement, and a backend that refuses after `lower` retains what names it; a fault may
   carry the engine's own typed cause, opaque behind `Error::source`, and the
   design states what the common fault preserves; the content checks are necessary, not sufficient (§5.1,
   §5.2, §5.4, §6.2, §6.3). The lowering's linear bound is the conversion's, grounding's cost the
   program's, and Door A's admission and a model's conversion are costed exactly — the occurrences' clone,
   the ordered set's construction (§5.1, §10.1, §10.2). A backend's own state has one home: it is ready or
   needs a rebuild, taken there only by an engine refusal past `lower`'s check — the backend's bug where its
   check should have caught it, a Resource fault where the engine ran out partway — or by a failed
   grounding, never accepted; a refusal the check makes changes nothing, a single-shot backend keeping the
   program lowered before it; while it needs one, every method that touches the engine refuses
   (`Presupposition::NeedsRebuild`) until `reset`, which keeps the registrations — on a single-shot backend,
   until a `lower` replaces the program; the law is uniform by decision, its tradeoff and the pinned
   engine's warrant stated. The agent tracks a pending rebuild — any step of its own that fails against the
   engine leaves one pending, and only a successful rebuild clears it, trivially while every step rebuilds —
   and recovers by its rebuild's replay at its next step; after a refused replay every later step retries
   and refuses at the same part until a replay succeeds, the loop having no un-ground (§4.1, §6.2). The
   observer is optional and declared — `Capabilities::ground_program`, the method a provided default, the
   required surface three methods — and its law, at §10.4, holds the program as the grounder emitted it
   across the finished groundings since the last `reset` or replacing `lower`, `None` otherwise and while
   the backend needs a rebuild, a single-shot
   backend's only once a solve has grounded; its carrier is the contract's — each statement held once and
   whole with its part, the program's or a parse's occurrences (`Grain`), each ground rule naming one, its
   construction door landing with the observer — and its attribution mechanism each backend's own design,
   the Potassco adapter's promised per
   occurrence only once designed; conformance checks its declaration and the attribution's membership, its
   correctness held by a corpus of rules told apart once rules carry their heads and bodies, and its
   content qualified per engine (§4.1, §10.4, §11.1, §13.1, §13.2). `Symbol` is the shared identity, and an
   engine's identifiers stay its own (§10.5); an engine that
   computes its complete family streams it through the same `Run`, the laziness law's constant resident
   set being the owned side's (§5.2, §13.3); and the trajectory yields `Incumbent`s — a retained model and
   its levels, one completed result, one conversion each — typed apart from the proven `Optimum` (§5.2,
   §5.3). §12's claim that `Capabilities` declares a fragment is corrected, and the reserved seams gain the
   fragment declaration with its three-way typed refusal, streaming ground input, an agent door from an
   `Admitted`, and `Send` live handles where a backend allows them; the propagator's portability, marked
   at §8.3, and an `@`-function's purity and concurrency contract are owed before their signatures are
   fixed (§7.1, §8.1, §8.3, §12, §14); the fragment declaration's refusals are to be typed within the same
   sum (§12, §14); zetesis's acceptance runs without Potassco, the corpus through both doors and a
   Rust-built program through Door B, in each execution configuration it admits (§15). The conformance
   suite's obligations are an ordered list, the order of its `Check` enum; it drives every case through both
   doors and gains the counted repeats, a tuple counted once, a display that is a displayed term alone
   (`q. #show. #show p : q.`), a rebuild that leaves nothing behind, the observer's declaration and the
   attribution's membership, a Request fault's presupposition, and the backend's own state kept across a
   refused `lower`, a failed grounding, and a `reset` (§13.1). The status line names this the design of
   record the build follows.
16. **Reconciliations with the built boundary** (2026-10-03). The costs of `Model::with_terms` and
    `ShowRule::of` are stated as the code pays them, and a run's derivation reuses the display a model built
    with its terms already holds, so a model's display is derived once (§5.1). The contract's silence on
    whether a multi-shot backend searches what is lowered but not yet grounded is recorded, with the
    presumption the agent and the conformance suite make until the first multi-shot adapter settles it
    (§4.1, §11.1). The readings take no options, so a budgeted reading is stated as a composition — the
    budgeted solve, its world view, and the `Snapshot` it materialises (§6.3, query.md §2.7) — and the empty
    ground program, `Default`, is named the one value constructible ahead of the observer's construction
    door (§10.4). The conformance suite drives a declared time budget to its cut, over a program no search
    finishes within, and fails a cut concluded as closing the space (§6.3, §13.1). The observer's membership
    requires a ground rule's statement to carry an origin (§13.1); that a merged statement's every origin
    reaches its rules at the statement grain is held once the observer's construction door lands (§10.4,
    §11.1). The observer's fact obligation is stated with its reason, and the native door's corpus pass
    named as going through both doors (§13.1); §6.3 states what the budgeted composition bounds and leaves
    the rest to the threat-model statement. The cut binds an enumerating backend: a deciding one stops at
    its witness, which no budget the suite sets cuts, so a deciding backend cut before its witness is not
    yet probed (§6.3, §13.1).
17. **The boundary a native backend integrates against** (2026-10-03). The stops are classified in one
    table, before the first model and after models alike (§5.1). `Budget` and `Target` are the request's
    words: `Budget` is its acknowledged time budget, and `Target` a target it set or a deciding backend's
    witness. A backend's own configured ceiling, or an allocation failure, is a Resource fault carrying the
    engine's typed cause. A configured limit read as a budget would be a request field with its declaration,
    grown on need, and an engine's one-model habit is not carried into `solve` (§5.1). The phases are fixed
    (§6.3). `lower` validates and retains, grounding nothing. `solve` creates the run's control state, then
    grounds and searches. Immutable preparation may be reused across questions, with no mutable search
    state shared. The time budget is a wall-clock deadline fixed when `solve` is called, covering grounding,
    search, and model delivery, the consumer's time between reads included. A cut during grounding reads
    `Budget`, never a fault. The agent's lowering precedes the deadline, and enforcement is cooperative, not
    hard real-time (§6.3). The interrupt handle gains its operation, `Interrupt::pull`. A pull cuts the
    agent's question in flight, its lowering included. A question pulled before its search begins does not
    begin it, and a pull that lands while `solve` opens the run is forwarded again once it is open. A pull
    with no question in flight is forgotten, and the caller's pull takes precedence over a deadline in the
    same run (§6.3). A backend's solve is in flight from `solve` until its run ends or its handle drops; an
    adapter over a primitive that cuts the following call holds a pull for that whole window (§4.1). The
    conformance suite drives obligation 6 for an enumerating backend that declares cancellation — an active,
    unfinished search cut, and the stale pulls, within the cut's cap of further reads — and names a deciding
    backend's skip (§13.1). Obligation 10 states its single-shot arm beside the multi-shot one, witnessing
    that the lowered program stays, and the request-side limits beyond time are named a reserved seam
    (§13.1, §14). The deadline's reason is stated: an engine's own limit is a wall-clock alarm, and a
    deadline bounds the question's wall-clock time (§6.3).
18. **Repeated pulls and a leaked run** (2026-10-04). Pulls within one cancellation window are one pull:
    the core may forward a caller's pull twice, when it lands and again once the run opens, and a caller may
    pull more than once, so a primitive must not count or toggle (§4.1). The next question asked ends one
    whose run handle was leaked rather than dropped, so a leaked run keeps no question in flight past it
    (§6.3). The drop clause revisions 11 and 12 recorded is restated as the pull's own safety, with no
    change of rule (§4.1).
19. **A refused part, and what a single-shot solve grounds** (2026-10-04). A single-shot solve grounds the
    `base` part alone, as the pinned authority's single-shot run does, and a named part is instantiated only
    through a multi-shot backend's `ground` (§6.3). A backend that does not admit a part refuses the program
    at `lower` with a Program fault naming the part by its key — `Refused::Part`, the sum's fifth row, built
    by `Fault::program_part` — and the fault is unlocated, since the program tier keeps no origin for a
    part (§5.4). The rule is the language's single-shot reading, which the authority's default run realises
    and themelios holds whatever a program carries; the refusal is §12's fragment interim (§6.3). Two
    seams are named: a located part refusal, and `ground` over part instances, which a part with formals
    needs (§14). §10.2, §12, and obligation 8 cite §5.4's sum for what a Program fault names, and a router's
    reason grows in its `Statement` or `Part` row. The conformance suite gains obligation 12, which holds a
    single-shot backend to the rule through both doors, which must agree: the base answered as it alone
    denotes, or the part refused, the program lowered before it left in place. No other check lowers the
    probe, so any other answer over it fails the obligation; it says it was not driven for a multi-shot
    one (§13.1).
