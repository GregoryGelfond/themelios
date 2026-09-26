# themelios-solve — design of record

2026-09-03. Draft, pre-implementation. This is the normative design for the **solve tier** —
`themelios-solve` and the adapter crates that realise it — the fourth tier over the
shared base (§12.1 of the specification). Its sibling `themelios-query` has its own design
(`query.md`); the two are built as one stage, the way `analysis.md` accompanies `program.md`. This
document stands with `specification.md` §9/§11/§12 and the built tiers' designs (`base.md`,
`syntax.md`, `program.md`, `analysis.md`, `grammar.md`); where it evolves the specification's crate
roster or clause it says so in place (§16).

The keystone, stated once so the rest can be read against it: **the solve tier is the *abstract
solver* — the codegen/target contract over the `Program` value, whose operations are the questions a
logician asks of that value and whose answers are typed models.** The concrete engine behind the
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

Every result the tier produces — outcomes, answer sets, optima, consequences, theory assignments,
blame, faults — is a **typed model** (specification §1.5). Views for each consumer class are
derivations over the model: a human-centric `Display`, and a machine-centric structured/serializable
form for LLM agents, editor protocols, and audit. No operation's primary output is prose, and no
consumer parses rendered prose to act. `themelios-base`'s `Diagnostic` already carries
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
| `themelios-solve` | forbid | The backend **contract**, the outcome vocabulary and MVC models, the agent and its driving surface, the fault taxonomy, the extension-surface traits (`@`-functions, propagators, extraction), the bridge seam, and the conformance suite. Engine-free. |
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
- `outcome` — the models and their views (§5).
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

Every capability of the tier is reachable two ways, both first-class:

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

- **`@`-functions and propagators share the extension substrate.** Both register onto an agent, both
  cross the FFI seam through a panic-containing trampoline under the interning discipline (§10.5),
  both are engine-portable because both are the contract's (§7, §8). The "quarantined-unsafe floor,
  100%-idiomatic safe surface" machinery is *one* thing serving both — and the theory atoms a
  propagator watches are authored through the very macro/programmatic faces of §3.1.

- **The outcome models are the meeting point.** The driving surface *produces* them, query *reads*
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

    /// REQUIRED. Consistency and enumeration; the handle streams answer sets lazily (§5.2).
    fn solve(&mut self, req: &SolveRequest) -> Result<Solved<'_>, Fault>;

    /// A handle to interrupt an in-flight solve from another thread — `Some` iff
    /// `capabilities().cancellation` (the handle is `Send`, §6.1/§6.3); the provided default answers
    /// `None`, so a non-cancelling backend inherits it and a cancelling one overrides. It is the one
    /// primitive the request-side time budget (§6.3) and `Agent::interrupt` (§6.2) are realised over; the
    /// conformance suite (§13.1) checks `cancellation ⇒ interrupt().is_some()` so the bit cannot lie.
    fn interrupt(&self) -> Option<Interrupt> { None }

    /// REQUIRED. The bridge (§10): consume a `Program`. On a **multi-shot** backend a repeat `lower`
    /// ACCUMULATES into the engine's program (the `assert` path lowers only the delta, §6.2), so a rebuild
    /// is `reset` then `lower` the amended whole. On a **single-shot** backend a `lower` REPLACES the
    /// program (each solve is independent; nothing accumulates), so a rebuild is one `lower` and `reset`
    /// is not called. Expose the ground program.
    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault>;
    fn ground_program(&self) -> Option<&GroundProgram>;   // the committed observer (§10.4)

    /// REQUIRED iff `capabilities().optimization`. The proven optimum, improving trajectory iff asked (§5.3).
    fn optimize(&mut self, req: &OptimizeRequest) -> Result<Optimized<'_>, Fault> { Err(Fault::unsupported()) }

    /// REQUIRED iff `capabilities().assumptions`. Solve under a scenario; the core derives blame
    /// (`Refutation`, §5.4) over this — there is no separate backend blame method.
    fn solve_assuming(&mut self, s: &Scenario, req: &SolveRequest) -> Result<Solved<'_>, Fault> { Err(Fault::unsupported()) }

    // --- REQUIRED iff capabilities().multi_shot (provided defaults that refuse) ---
    fn ground(&mut self, parts: &[Part], opts: &GroundOptions) -> Result<(), Fault> { Err(Fault::unsupported()) }
    fn assign_external(&mut self, ext: Symbol, v: TruthValue) -> Result<(), Fault> { Err(Fault::unsupported()) }
    /// Clear the engine's accumulated program so the agent can REBUILD it (the rebuild-class retraction
    /// path, §6.2); `lower` then reloads the amended program. Only a multi-shot backend needs it: on a
    /// single-shot backend `lower` REPLACES the program (nothing accumulates), so a rebuild is one `lower`
    /// and `reset` is not called. Distinct from `assign_external` (a toggle) and `ground` (an addition).
    fn reset(&mut self) -> Result<(), Fault> { Err(Fault::unsupported()) }

    // --- REQUIRED iff the matching capability bit (functions / propagators); extension reg. (§7–§9) ---
    fn register_function(&mut self, f: Box<dyn Function>) -> Result<(), Fault> { Err(Fault::unsupported()) }
    fn register_propagator(&mut self, p: Box<dyn Propagator>) -> Result<(), Fault> { Err(Fault::unsupported()) }

    /// OPTIONAL — override iff `capabilities().native_consequences == Native`. Absent, the core derives
    /// cautious/brave by enumeration over `solve` (unscoped) or `solve_assuming` (scoped) (§4.2); the
    /// request surface says which path runs. What the backend owes: a native door honours the
    /// `ConsequenceRequest`'s scenario, ranging over the models `solve_assuming(scenario)` denotes; an
    /// unscoped request carries the empty scenario. The agent surface produces a non-empty request through
    /// `Agent::cautious_assuming`/`brave_assuming` (§6.2 — the normative home of the scoped doors'
    /// precondition and cost).
    fn consequences_native(&mut self, mode: Mode, req: &ConsequenceRequest)
        -> Result<Consequences, Fault> { Err(Fault::unsupported()) }  // provided default
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
    pub budgets: BudgetSupport,
}
```

A request beyond declared capability receives a **typed refusal** (`Fault`, §5.4), never a silent
degrade. Cost note: `capabilities()` is `O(1)` and pure — it is read *before* a request is paid for.

**The `enumeration` bit carries a soundness obligation, not merely a hint.** A backend that declares
`enumeration: false` (it decides consistency but does not enumerate the answer sets) must **never**
conclude `Conclusion::Exhausted` with models unseen — `Target` is the closed set's word for "stopped at
the witness." Every universal reading trusts `Exhausted`: `Solved::all_answer_sets` (§5.2), the derived
cautious/brave fold (§4.2), and query.md's `Snapshot` readings all treat an `Exhausted` search as the
whole space. A consistency-only backend that concluded `Exhausted` after one witness would make each of
them silently wrong, so §13.1's conformance suite checks the bit against this obligation; the exhaustion
gate then carries the reading — a universal reading refuses without a closed search (§5.2).

**Required versus provided — why the core stays lean.** The **trait-required** surface (no default) is
`capabilities`, `solve`, `lower`, and `ground_program`. Every **capability-gated** method — `optimize` /
`solve_assuming` / `ground` / `assign_external` / `reset` / `register_*`, alongside `interrupt` and
`consequences_native` — is a **provided default that refuses** (`Err(Fault::unsupported())`, or `None` for
`interrupt`), so a minimal backend implements only the four and a declared-absent capability refuses with
no line written. The type cannot force a *declared* capability's method to actually do the work
— a provided method needs no override — so that obligation is enforced by §13.1's **positive-capability
check**: a bit set `true` whose method still refuses is the capability lie the conformance suite exists to
catch, and `cancellation ⇒ interrupt().is_some()` likewise. (`interrupt` returns `Option` and defaults to
`None`, so a non-cancelling backend inherits it; `reset` is the tear-down the multi-shot rebuild path needs
given `lower` accumulates, §6.2, and is not called on single-shot backends.) The **core** provides, over that surface and *not* on the trait,
the two derived readings a backend author does not write: cautious/brave **consequences by
enumeration** when a backend lacks `consequences_native` (§4.2), and **blame** (`Refutation`, §5.4)
over `solve_assuming`. No smaller required set exposes consistency, enumeration, optimization, theory,
and multi-shot — each capability-gated method is the sole engine primitive for its witness, and
removing it discards essential structure, not accidental complexity — so this is the minimal contract
by construction, which is what keeps the one-door audit (§4.3) finite.

### 4.2 Refuse-or-derive, disclosed before it is paid for

The core may **refuse-or-derive** deliberately: it can derive cautious consequences by intersection
when an engine lacks them natively. The derived-versus-native distinction is legible **at the request
surface, before the request is paid for** — `Capabilities::native_consequences` says which path a
request will take — because deriving consequences can cost enumeration, a different computational
beast (`Θ(|W|)` models folded) than one solve, and a cost divergence of that size disclosed only in
the receipt is the surprise this design exists to forbid. The outcome's provenance then records which
path ran.

The contract does not assume the engine is foreign: a native backend built from foundation crates
implements the same trait, and the contract's shapes must not force conversions a shared-representation
backend would never need — **zetesis** (§12), which shares the program tier's `Symbol`, is the standing
check on this.

### 4.3 One door per boundary

The `Backend` trait is the sole crossing between the engine-free core and any engine. No tier reaches
around another; the bridge seam (§10) is how a backend consumes a `Program` and emits a ground
program, and it is part of the contract's surface (`lower`/`ground_program` above), not a side
channel. This is the microkernel "one door per boundary" (specification §12.3) in the tier's own
terms, and it is what makes the audit's job finite.

---

## 5. The outcome vocabulary (the MVC models)

### 5.1 The closed distinctions

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

/// An answer set is a set of ground symbols — re-exported from the program tier (program.md §11.3),
/// so the solve, query, and program tiers speak one answer-set vocabulary.
pub use themelios_program::AnswerSet;   // = BTreeSet<Symbol>
```

The two names owe their §1.4 reason, stated here: engines' own result vocabularies conflate the
logical question (*is the program consistent?*) with the search question (*did the search finish?*);
these names separate what the engines confuse, and Rust's own `Result` forecloses the obvious
alternative. The names are argued, not inherited: a clearer pair discovered at design time supersedes
them by satisfying §1.4 in its turn. The `Determination` variants are closed; their *payloads* are
`#[non_exhaustive]`.

### 5.2 Answer sets, optima, consequences

```rust
/// `solve` returns this borrowed handle. It resolves the trichotomy, streams the answer sets, and —
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
    pub fn answer_sets(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_;

    /// A COMPLETE collection — available ONLY when the search closed the space; refuses otherwise
    /// (the exhaustion gate, the `WorldView::is_exhausted` analog). This is what makes "a truncated
    /// search passing as complete" unconstructible (§5.3), not merely visible via `conclusion`.
    pub fn all_answer_sets(&mut self) -> Result<Vec<AnswerSet>, NotExhausted>;

    pub fn conclusion(&self) -> Option<Conclusion>;   // readable once the search resolves
}

/// A backend constructs the `Solved` that `Backend::solve` (§4.1) returns through `Solved::running`,
/// handing the core its own lazy enumeration as a `Run` — the backend-facing streaming protocol, the seam
/// a native engine (§12) implements. Obligations: **fused** (once `next_answer_set` yields `None` or a
/// fault it stays ended); a **terminal `Conclusion` once the stream ends**; a completeness drain stops at
/// the first fault. The **core owns classification** — it resolves `Consistent` iff the run WITNESSED a
/// model — so a backend supplies only enumeration plus a terminal conclusion and **cannot forge
/// `Consistent`** (§5.1). No `Send` bound (a run may hold a raw engine handle whose control is
/// single-threaded), so the `Solved`/`Models`/live-`WorldView` handles built over it are `!Send` and
/// `Snapshot` is the `Send` form (§6.1). Cost: `O(1)`.
pub trait Run {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>>;   // stream; None ends it
    fn conclusion(&self) -> Option<Conclusion>;                          // the terminal state, once ended
}
impl<'a> Solved<'a> {
    pub fn running(run: Box<dyn Run + 'a>, scenario: Scenario) -> Solved<'a>;  // the backend construction door
}

/// The `Consistent` payload (§5.1): read the answer sets, or open the live `WorldView` the query tier
/// reads. From a RETAINED agent the world view is a BORROWING handle `WorldView<'a>` over the live engine
/// — lazy, its one engine-driving read the fallible `members` stream (query.md §2.3) — so it borrows for
/// its lifetime and does NOT outlive that borrow. The cautious/brave native door and the epistemic
/// readings (`answer`/`bindings`/`entails`) live on the AGENT (§6.2) and on the materialised `Snapshot`,
/// NOT on the live handle (query.md §2.3–§2.6), so a reading is a fresh solve rather than a drain of this
/// stream. An owned, engine-free `Snapshot` (to cross a service boundary, and the home of the infallible
/// readings) is `WorldView::materialize` (query.md §2.3). The single-shot bare form owns its ephemeral
/// engine instead (a live `WorldView<'static>`, §6.4).
pub struct Models<'a> { /* the live-run-access handle: a two-form value — Owned by the consuming resolver `into_determination`, Borrowed by the inspecting `determination(&mut self)`; the lifetime is the access, `'static` when the run owns an ephemeral engine */ }
// The `Models<'a> → WorldView<'a>` transition lives on the QUERY side (query.md §2.7): a `WorldView` is
// constructed from a resolved `Consistent(Models)` via `themelios_query::WorldView::of(models)`, so
// `themelios-solve` does not depend on `themelios-query`. `Models` exposes the live-run material a world
// view drives — the `members` stream, the exhaustion-gated `all_members` (the gate `WorldView::materialize`
// goes through, query.md §2.3), `is_exhausted`, and `scenario` — no engine and no backend access, and no
// `world_view()` method of its own. `Solved` and `Models` are **`!Send`** because the `Run` trait object
// they hold carries no `Send` bound (a run may hold a raw engine handle whose control is single-threaded);
// the owned, engine-free `Snapshot` (query.md §2.3) is the `Send` form, which is what `materialize` is for —
// §6.1's service posture crosses a boundary through a `Snapshot`, not a live handle.

/// A PROVEN optimum — no public constructor; it exists only because the solver proved it.
pub struct Optimum { /* levels, in the objectives' own terms */ }

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
        -> Option<impl Iterator<Item = Result<Optimum, Fault>> + '_>;
    pub fn conclusion(&self) -> Option<Conclusion>;
}

/// Cautious (⋂) or brave (⋃) consequences — a set of ground `Symbol`s carrying the `Mode` that produced
/// it and, under an objective, whether it ranged over the OPTIMAL set or all stable models (§2.4, §5.2).
#[non_exhaustive]
pub struct Consequences { /* Symbol set + Mode + the optimal-vs-all marker */ }
pub enum Mode { Cautious, Brave }
```

- Answer sets are **owned, streamable** values — the lazy `Result`-iterator above. Cost: streaming
  enumeration is **constant in resident set** (one answer set materialized at a time — the scaling
  bench asserts it, §13.3); the owned no-sharing tree is the authoring form, the huge ground
  instantiation lives in the engine's compact internals.
- A **proven optimum** is typed distinct from best-found: `Optimum` has no public constructor, and
  reports its levels in the terms the objectives were written in (a maximized level shows what was
  maximized, not the negation the engine optimizes internally). "All optimal solutions" is available
  only when the search closed the whole space, and says so. This fixes the *denotation*: the answer
  sets a program with an objective denotes are exactly those optimal ones (with no objective, all
  stable models — the degenerate case), so consequences and the query tier's world view range over
  the optimal set when the program optimizes.
- **Cautious and brave consequences** are typed sets carrying the semantics that produced them, so a
  value that has travelled still says which question it answers. They are not answer sets, and carry
  their own type (`Consequences`) for that reason. **Under an optimization objective they range over
  the *optimal* answer sets** (§5.2's denotation): the *derived* door honors this by folding the
  optimal world view, and the **native door (`query.md` §2.4) is obligated to compute over the optimal
  set, not all stable models** — the optimum-proven/exhausted gate applies to it — so both doors target
  the same model set and their required agreement (`query.md` §2.4) is meaningful rather than a silent
  both-wrong. (Whether the pinned engine computes cautious-over-optimal in one solve is measurement,
  §13.2; the *obligation* is stated here.)

### 5.3 The pathologies are unconstructible

Request types distinguish "enumerate answer sets" (`SolveRequest`) from "optimize, reporting the
improving trajectory" (`OptimizeRequest`), and the improving trajectory is available when — and only
when — the request asked for it. The three named solver pathologies (specification §5.1) are
**unconstructible in the vocabulary**, not merely tested against, each by its own structural device:

- *enumeration reporting an optimization's improving sequence* — distinct `SolveRequest` /
  `OptimizeRequest` and distinct `Solved` / `Optimized` outcomes; the trajectory exists only on
  `Optimized`.
- *a truncated search passing as a complete collection* — a "complete collection" is reachable ONLY
  through `Solved::all_answer_sets` (§5.2), which is **exhaustion-gated and refuses without a closed
  search** (the `WorldView::is_exhausted` analog); the streaming `answer_sets` never claims
  completeness, so a truncated search cannot be laundered into "all answer sets."
- *contradictory termination flags* — one `Conclusion`, orthogonal to `Determination`, with no second
  flag to disagree with.

`Optimum`'s absent public constructor closes the last gap (a best-found cannot pose as proven). The
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

/// A fault is a value with a CLOSED locus taxonomy at the seam. It OWNS its model — a message, the
/// `Locus`, the backend-bug bit, and a `base::Location` ONLY where it has one (Program/Request faults).
/// An unlocated fault (Engine/Resource/Adapter) is NOT a degenerate diagnostic with a fabricated span
/// at an "unknown source" but a different thing (base's §diagnostic): it renders through its own `Display`.
#[non_exhaustive]
pub struct Fault { /* message + Locus + Option<base::Label> + the backend-bug bit */ }
pub enum Locus { Program, Request, Resource, Engine, Adapter }
impl Fault {
    pub fn is_backend_bug(&self) -> bool;                  // a closed bit
    pub fn locus(&self) -> Locus;
    pub fn located(&self) -> Option<LocatedFault<'_>>;     // Some iff it carries a Location
}
impl std::fmt::Display for Fault {}                        // Fault is Display + Error — NOT ToDiagnostic
/// The only form of a fault that IS a `base::Diagnostic`: one that carries a `Location`.
/// `impl ToDiagnostic for LocatedFault` — a fault without a span does not lower to a diagnostic.
pub struct LocatedFault<'a> { /* a &Fault whose Location is guaranteed present */ }
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
- **Faults** are values with the closed locus taxonomy above, with "is this a backend bug" a closed
  bit; a *located* fault (Program/Request) lowers to a `themelios-base` `Diagnostic` through
  `LocatedFault` (loci and provenance, solved once, here, for every consumer), while an unlocated one
  (Engine/Resource/Adapter) renders through its own `Display` — a fault is not, in general, a diagnostic.
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

Every model in this section is typed data first, with a human `Display` and a machine-consumable view
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
value** — the authority to drive the engine; dropping it is revocation, and there is no ambient engine
or global mutable state. Asking a question borrows the agent (`&mut self`), so the borrow checker *is*
the "no mutation while reasoning" lock, and the reasoning state machine (initial → grounded → prepared →
solved) is expressed in ownership and borrowing rather than runtime checks — an out-of-order call does
not compile. Thread posture is explicit per backend: the live run handles (`Solved`/`Models`/`WorldView`)
are `!Send` — the `Run` trait object they hold carries no `Send` bound, since a run may hold a raw engine
handle whose control is single-threaded (§5.2) — and the engine-free `Snapshot` is the `Send` form that
crosses a service boundary; cancellation-from-another-thread is a declared capability whose handle
(`Interrupt`) is `Send`. Because the agent *owns* its knowledge rather than
borrowing a `Program` off a stack frame, it is embeddable behind a service boundary or an editor host
without ceremony — the LSP/pythia posture (specification §1.2, §9.4). Cost: agent construction is one
engine handle; a question's cost is the engine's, streamed (§5.2).

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
    pub fn interrupt(&self) -> Option<Interrupt>;                         // a cancellation handle — Some iff the backend cancels (§6.3)

    // --- consequences (native door or derived fold, capability-routed §4.2), on the agent because it
    //     owns the engine — so the reading is a fresh solve, not a drain of a live world view (§5.2).
    //     The unscoped pair ranges over the whole program; the `_assuming` pair ranges over a scenario's
    //     models — the epistemic sibling of `solve_assuming`. THE NORMATIVE HOME of the scoped doors'
    //     precondition and cost (§4.1 and query.md §2.4 cite this): both routes REQUIRE
    //     `capabilities().assumptions` and otherwise refuse `Fault::unsupported()` (Locus::Request) — the
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

**Assertion is monotone and clean; retraction is the sharp edge, made honest by owning the knowledge
base.** `assert` adds one statement — any `Statement` (`program.md` §4.2): a rule, a fact, a
constraint, an objective (`Optimize`) — and returns a `StatementId` naming it; `ground` instantiates a
named `#program` part with arguments — the two are the fine- and coarse-grained faces of the same
monotone extension. `observe` is the bulk assertion of ground facts through the `Facts` pillar (§7.3),
the loop's *observe* step, returning an `Observation` a later step can `forget`. Because themelios
**owns the knowledge base as a first-class `Program` value** — which neither Prolog's flat clause
database nor an engine's write-only backend has — `retract` is a *true* operation on that value: it
removes the named statement from the knowledge base, and the agent then realises the removal against the
engine by the cheapest faithful means its declared capabilities allow — **toggling an external** where
the retracted statement was so guarded and the backend declares `externals`, or **resetting** the
engine's accumulated program (`Backend::reset`) and reloading the amended program (`lower`) otherwise —
`lower` accumulates, so a rebuild is a reset then one lowering, never a bare re-lower. The realisation
is the agent's to choose; the register the caller writes
stays declarative. This is why themelios can offer retraction where an engine offers only externals: it
holds the program the external mechanism can only approximate.

Retraction's two realisations diverge in cost by the whole program size and the loss of the engine's
warm search, so — following §4.2, which forbids hiding a divergence of that magnitude behind a uniform
signature — retraction is **disclosed before it is paid for and recorded after**: a statement's
*retraction class* (toggle vs rebuild) is fixed at `assert`, from the backend's `externals` capability
and whether the statement is externally guarded, and is readable from its `StatementId`; the outcome's
provenance then records which realisation ran. A stale or duplicate handle is a typed refusal, not a
silent no-op — `retract` of an already-retracted `StatementId`, or `forget` of a spent `Observation`,
refuses with `Locus::Request`.

Two things are deliberately *not* inherited from Prolog's `assert`/`retract`: the `asserta`/`assertz`
ordering variants are absent (clause order is meaningless under answer-set set-semantics), and the
logical-update-view hazards do not arise, because the knowledge base is amended **between** questions, at
the loop's step boundary, never during a running search. The modification methods rest on the
capabilities their realisation uses — `multi_shot` to ground an added statement or part incrementally,
`externals` for the external-toggle retraction path; where a backend declares neither, the agent falls
back to re-grounding the amended `Program` — a `reset` then a `lower` on a multi-shot backend, and on a
**single-shot** backend a plain `lower`, which *replaces* the program there (each solve is independent, so
nothing accumulates and no `reset` is needed) — a rebuild any backend that solves at all supports. The retained
state upholds the two-representation correspondence (owned program ↔ engine-internal state) across the
cycle, which is also what gives a transform on the owned side a defined effect under multi-shot. Cost:
retained state is `Θ(program size)`, not `Θ(ground size)` — the ground instantiation stays in the
engine (§10); an external-toggle step is `O(1)` at the seam, a rebuild is one lowering (§10.1).

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

**Budgets** (time at minimum, with room for model-count caps) are a typed, request-side surface;
enforcement is a declared capability — an engine without a native time limit gets it through the
`interrupt` primitive (§4.1): a timer thread that calls it on the cut — and `Conclusion::Budget`
reports a hit budget as what it is. The long tail of
engine parameters, when a real consumer needs it, follows the two-tier facade pattern (typed knobs
over a legible open form); it is YAGNI-gated, grown on demand, never a CLI-string passthrough.

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
when the handle drops.** So a returned answer-set stream or `WorldView` stays *lazy* — it retains the
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
locus.

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
rather than rediscovered: an `@`-function must intern *inside* the grounding call under the interning
discipline (§10.5), and the trampoline contains its panics. One mission discipline is stated: an
`@`-function is arbitrary Rust at ground time, so purity/determinism is a contract the surface makes
easy to declare — a clock, an RNG, or the filesystem breaks deterministic mode and auditability.

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
during solving).

### 8.2 The litmus, and the full CP target

The acceptance test is that **difference logic, CP, and linear/real arithmetic must each be *pleasant*
to write** on the trait — a clean port proves the seams are real abstractions, not clingo-shaped
holes. The **CP half is a *full* constraint theory, not a difference-logic subset**: global
constraints — `alldifferent` and its family — and `&sum`/`&dom` must be expressible on the surface,
and their assignments must read back through `TheoryAssignments` (§5.4) as typed data. This is the
concrete bar the propagator surface and the theory-assignment component are held to.

### 8.3 Engine-portable, and the platform it makes

Because the trait is part of the **contract**, not a clingo-specific hook, a theory written once runs
on *any* backend that implements the contract — clingo/clingcon now, a native engine later. This turns
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
/// An answer set (or a projection) → a user-defined Rust value. The read-time inverse of Facts (§7.3).
pub trait Extract: Sized {
    fn extract(model: &AnswerSet) -> Result<Self, ExtractError>;
}
```

Extraction is the **machine-view of the answer-set model** (§1.3): the model exposes its symbols in
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
engine plus worst-case cost tripwires. Cost model: **the lowering is linear in program size** (the
scaling bench asserts it, §13.3); it never materializes the ground instantiation on the owned side.

### 10.2 The doors

Mirroring the engine's own construction paths and the two-doors study in `program.md` §7 (the raise,
§8), the seam offers three grades. **Doors A and B are two *entry values* into one grounding
mechanism** — the engine's non-ground input (its AST builder), driven to `ground` — differing only in
what they preserve; **Door C is the aspif-level ingestion** the engine's ground-by-construction backend
exposes, which takes ground objects only:

- **Door A — the typed AST (`&Parse<ast::Program>`) → the grounder's input**, order- and
  span-preserving, where the highest fidelity is possible.
- **Door B — `themelios_program::Program` → the grounder's input** *through the same mechanism as Door
  A*, canonical-order, carrying `Origin` provenance through to every ground rule (a capability the C
  grounder lacks). Programs constructed in Rust, transformed, or loaded through a client enter here. A
  non-ground `Program` — a variable, an aggregate, a `#program` part — crosses only through A or B, into
  the grounder; it can **not** be expressed through the ground-by-construction backend (that is Door C),
  and mapping it there is a category error.
- **Door C — aspif → the solver's ingestion** (the engine's ground-object backend), for driving a
  solver from a foreign grounder, for the differential harness, and for an agent's ground-fact
  additions where the values are already ground.

```rust
pub enum Door<'a> {
    Ast(&'a Parse<ast::Program>),   // A: highest fidelity (spans preserved)
    Program(&'a Program),           // B: canonical, provenance-carrying — same grounding mechanism as A
    Aspif(&'a mut dyn AspifSource), // C: aspif-level, ground-object ingestion (foreign grounder / differential)
}
```

The **discipline is absolute: never render to text and re-parse across the seam.** The fragile, slow
path a shell-out imposes (four hand parsers, a pipe deadlock, an output cap, string-matched
optimality — the shell-out's cautionary tale) is exactly what the typed doors erase. The owned,
no-sharing tree stays the authoring/analysis form only; the huge ground instantiation lives in the
engine's compact internals, streamed.

### 10.3 The aspif-level sink trait

Between the grounder and the solver sits one typed sink trait in the image of the engines' own program
backend (`rule`, `bd_aggr`, `minimize`, `external`, `project`, `heuristic`, `edge`, `assume`, `show`,
step framing, `next_lit`/`fact_lit`) with a companion theory-backend trait — typed with distinct
`Atom`/`Literal` newtypes and distinct id newtypes for the several id roles, which Rust makes cheap.
Its implementors are the native solver's ingestion, an aspif writer, and the `--text`/reify
projections.

### 10.4 The ground-program IR / observer capability

The ground program is a first-class value the bridge can expose — the machine-IR of §1.1 — carrying
`Origin` provenance on every ground rule (`Backend::ground_program`, §4.1):

```rust
pub struct GroundProgram { /* ground rules, each carrying Origin — engine-free data */ }
```

This is a **committed capability, not merely a reserved seam** (§16 records the amendment against
specification §9.6). Two things make the promotion right, and both answer §9.6's "no v1 anchor forces
it / gold-plating grows the TCB":

1. **The anchor exists.** An explanation client (an xclingo-class tool) attributes answer-set atoms
   back through ground rules to source, and its whole method lives below the seam — so §9.6's "no v1
   anchor forces it" no longer holds.
2. **It does not grow the *unsafe* TCB.** `GroundProgram` is *engine-free* Rust data carrying
   `Origin`, not FFI/unsafe, so §9.6's minimality-of-the-TCB rationale does not bite. The honest cost
   it *does* add is an **adapter obligation** — the adapter must faithfully produce this value
   alongside the aspif lowering — which the conformance suite checks (§13.1).

It is a capability over the contract, not part of the mandatory lean core.

### 10.5 The interning discipline and the `Symbol` correspondence

`themelios_program::Symbol` carries the engine's own number width (`i32`, `program.md` §3.1), so no
value is lost or reshaped crossing the seam; but creating the engine's symbol *handle* from a `Symbol`
**is** an interning write — serialized under the single interning discipline below, not a free
correspondence. The FFI cost concentrates at the engine's process-global
interning; the adapter owns a single interning discipline (specification §5.2) — interning writers
under one lock, a reentrant-interning tripwire, and a lint over every direct interning FFI call — so
`@`-functions and located AST construction intern correctly and a non-returning grounder call is a
loud error, not a silent process-wide wedge. **This compensation is version-scoped to the pinned
engine and retired by the spike suite** (specification §5.2), a framing the adapter design carries.
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
implementation of it, so no adapter reaches around another.

Our own **in-house CP theory** on the propagator platform (§8.3) is **not** clingcon's replacement but
its **portable, Rust-native alternative** — written once, it runs behind *any* conforming backend, where
the linked clingcon backend is the mature C engine a deployment links directly. The two coexist by
design: linking clingcon costs one more C library in the trusted computing base, paid only by a
deployment that enables it, and bought back by a battle-tested constraint theory available immediately,
ahead of the satellite.

### 11.2 The clingo and clingcon binaries as external oracles

Correctness is proved the way the syntax/program/analysis tiers prove themselves — against **external
binary oracles** invoked out-of-band (via pixi, never linked into the shipped stack). The **clingo
binary** is the grounding/solving authority over the corpus (§13.2); the **clingcon binary** plays the
identical role for the constraint theory — the differential authority that keeps *both* the linked
clingcon backend and our own Rust CP theory (§8.3) honest on answer sets and constraint assignments.
These out-of-band *binaries* are distinct from the *linked* libclingo/libclingcon of §11.1: the linked
library is the shipped backend, the binary is the vendored-for-tests oracle it (and the satellite) is
differenced against, so a divergence is caught rather than trusted.

### 11.3 The trusted computing base

The adapter is the TCB under the microkernel criteria (specification §12.3): FFI calls enumerated
against a per-area manifest, each privileged operation carrying stated pre- and postconditions, the
interning discipline (§10.5) implemented once behind the capability story, engine defaults explicitly
configured per request shape so the adapter cannot transmit an ambush upward, and panic containment on
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

The contract also opens a **fragment-backend path** a native engine can walk: because
`themelios-analysis` verdicts are sound in the direction that matters (tight ⇒ no unfounded-set check,
HCF ⇒ no non-HCF tester, Horn ⇒ no search, stratified ⇒ facts-only domains), a backend can *declare its
fragment* through `Capabilities` (§4.1) and grow it up the lattice over time, routing what it does not
yet cover to another backend, with the differential run on the overlap. The `Capabilities` declaration
already expresses this; the ambitious native engine (§14) inherits the contract and this path.

---

## 13. Assurance

### 13.1 The conformance suite

Executable, shipped with the contract, run by every adapter: outcome correctness on a corpus of small
programs with independently known answer sets; capability honesty (a declared-unsupported request must
refuse); the named pathologies (§5.3) attempted and structurally impossible; fault loci landing where
they belong; and **the ground-program observer produced faithfully** where declared (§10.4). The suite's
skeleton is exercisable **engine-free over a stub backend** before any adapter — that run is the core's
own check (it streams answer sets through the real contract), not an adapter's authority. The
**clingo and clingcon adapters** run it (clingcon adds the constraint-theory cases), differenced against
the out-of-band binaries (§13.2). The **second, architecture-independent implementor** that proves the
contract is not clingo-shaped is **zetesis** as it adopts the contract (§12) — a real engine on a
radically different architecture, stronger corroboration than a naive built-in oracle would give.

### 13.2 Differentials and oracles

The clingo binary as the grounding/solving authority over the corpus; the **clingcon binary** as the
external oracle for the constraint theory — differencing *both* the linked clingcon backend and the
in-house CP satellite (§11.2); the native-versus-derived consequence differential the tier gets for free
(query.md §2.4); and the bridge differential with its worst-case cost tripwires (§10.1). An adapter that
shares the program tier's `Symbol` (zetesis, §12) adds a further cross-implementation differential when
it lands. Every instrument documents what it proves *and what it cannot* (specification §10.2).

### 13.3 The mission bar

No panic escapes the public surface on any input; every public operation documents its failure
semantics; every walk over user-reachable structure is work-list based (the depth discipline,
specification §5.2); the trust floor is minimal and legible (unsafe confined to the potassco TCB, zero
above it, FFI-free with the adapter disabled); leak- and race-checking harnesses at the TCB; the
scaling-shape benches assert complexity class for the load-bearing operations (the bridge lowering
linear in program size, §10.1; streaming enumeration constant in resident set, §5.2).

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
tier ships: the contract, the outcome vocabulary and models, the agent and full multi-shot (the
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
   proof the contract is not clingo-shaped. Because zetesis's integration is gated on `themelios-solve`
   landing (§14), that proof is a **post-v1 obligation carried with its residual risk** — the risk that the
   contract is subtly clingo-shaped until a truly independent engine exercises it — not a v1 done-condition.
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
- **The ground-program observer (§9.6, §13).** Promoted from a reserved seam to a **committed
  capability** (§10.4), with the two-part argument that defeats §9.6's rejection (an xclingo anchor
  now forces it; it is engine-free data, not added unsafe TCB — the cost is an adapter obligation).
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
