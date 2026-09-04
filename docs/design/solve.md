# themelios-solve — design of record

2026-09-03. Draft, pre-implementation. This is the normative design for the **solve tier** —
`themelios-solve` and the adapter and reference crates that realise it — the fourth tier over the
shared base (§12.1 of the specification). Its sibling `themelios-query` has its own design
(`query.md`); the two are built as one stage, the way `analysis.md` accompanies `program.md`. This
document stands with `specification.md` §9/§11/§12 and the built tiers' designs (`base.md`,
`syntax.md`, `program.md`, `analysis.md`, `grammar.md`); where it evolves the specification's crate
roster or clause it says so in place (§16).

The keystone, stated once so the rest can be read against it: **the solve tier is the *abstract
solver* — the codegen/target contract over the `Program` value, whose operations are the questions a
logician asks of that value and whose answers are typed models.** The concrete engine behind the
contract (clingo now, a native engine later) is a configuration of the installation, never a thing
the program author sees.

The register of this document matches its built siblings (`program.md`, `analysis.md`): every
load-bearing surface is stated as a Rust signature with its refusal and its cost model. The
implementation is written at build time; the types, the laws, and the costs are decided here. Where a
shape is deliberately governed by a downstream principle — the propagator trait, held to the DL/CP/LP
litmus (§8) — the design states the **interface and its governing principle** rather than a frozen
signature, and says so in place.

---

## 1. Keystone and design method

### 1.1 The abstract solver, in the foundation's own shape

The foundation is LLVM-shaped: `themelios-syntax` is the frontend, `themelios-program`'s `Program` is
the intermediate representation (the logician's abstract object — `program.md` §1), `themelios-analysis`
is the pass layer, and **the solve tier is the codegen/target**. A target, in that shape, is a
**contract** an engine implements — not a second representation. The one genuinely new data object
below the seam is the *ground* program, the machine-IR analog, and it is a named capability over the
contract (§10.4), not the tier's centre.

So `themelios-solve`'s core is a **behavioral contract** (§4), engine-free, that the clingo adapter
(§11), the reference solver (§12), and a future native engine each implement. The abstraction is the
deliverable; the programmer's ergonomics, the mission properties, and the eventual native engine are
all consequences of getting that one object right. The contract is co-designed so that a native
engine of ours slots in behind it with no change above the seam — and so that reasoning about *our*
solver, once it exists, is reasoning about a legible mathematical object rather than about clingo's
operational machinery.

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
| `themelios-solve` | forbid | The backend **contract**, the outcome vocabulary and MVC models, the session and driving surface, the fault taxonomy, the extension-surface traits (`@`-functions, propagators, extraction), the bridge seam, and the conformance suite. Engine-free. |
| `themelios-query` | forbid | The epistemic reading — three-valued `Answer`, `WorldView`, cautious/brave, bindings — over the program tier's patterns and the solve tier's outcomes. Engine-free. Its own design (`query.md`). |
| `themelios-reference` | forbid, `publish = false` | The naive pure-Rust reference solver: the small-case oracle, the second implementor of the contract, and the native-backend demonstration (§12). |
| `themelios-potassco-sys` | allow (bindings only) | Vendored, pinned bindgen output over libclingo's C API. Regeneration is out-of-band. Feature-gated; never in a default build. |
| `themelios-potassco` | allow (the TCB) | The mechanism-only kernel over the bindings plus the safe adapter implementing the contract against clingo. Named for the engine *family* it adapts; clingo the base. |
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
- `session` — the ergonomic driving surface over the contract (§6).
- `extend` — the extension-surface traits and registration (§7–§9).
- `bridge` — the seam types the adapters implement against (§10).
- `conformance` — the executable suite every adapter passes (§13).

The lean-core property is that `contract` is small and points at the unsafe floor through a narrow,
enumerable interface; the ergonomic-facade property is that `session` (and the top `themelios` crate)
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
  an LLM agent, a REPL — that build up programs, sessions, requests, and queries by composition.

The two straddle the crate-home line by design. The *authoring* half lives one tier down — `rule!` /
`fact!` / `asp!` in `themelios-macros`, the value builders in `themelios-program`'s `construct` —
because building the `Program` is a program-tier concern (LLVM's `IRBuilder` lives with the IR, not
the codegen). The *driving* half (sessions, solve, options) and the *reading* half (query) are the
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

- **`@`-functions and propagators share the extension substrate.** Both register onto a session, both
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

    /// REQUIRED. The bridge (§10): consume a Program, expose the ground program.
    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault>;
    fn ground_program(&self) -> Option<&GroundProgram>;   // the committed observer (§10.4)

    /// REQUIRED iff `capabilities().optimization`. The proven optimum, improving trajectory iff asked (§5.3).
    fn optimize(&mut self, req: &OptimizeRequest) -> Result<Optimized<'_>, Fault>;

    /// REQUIRED iff `capabilities().assumptions`. Solve under a scenario; the core derives blame
    /// (`Refutation`, §5.4) over this — there is no separate backend blame method.
    fn solve_assuming(&mut self, s: &Scenario, req: &SolveRequest) -> Result<Solved<'_>, Fault>;

    // --- REQUIRED iff capabilities().multi_shot ---
    fn ground(&mut self, parts: &[Part], opts: &GroundOptions) -> Result<(), Fault>;
    fn assign_external(&mut self, ext: Symbol, v: TruthValue) -> Result<(), Fault>;

    // --- REQUIRED iff the matching capability bit (functions / propagators); extension reg. (§7–§9) ---
    fn register_function(&mut self, f: Box<dyn Function>) -> Result<(), Fault>;
    fn register_propagator(&mut self, p: Box<dyn Propagator>) -> Result<(), Fault>;

    /// OPTIONAL — override iff `capabilities().native_consequences == Native`. Absent, the core derives
    /// cautious/brave by enumeration over `solve` (§4.2); the request surface says which path runs.
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

**Required versus provided — why the core stays lean.** The required surface is `capabilities`,
`solve`, `lower`, `ground_program`, and the capability-gated `optimize` / `solve_assuming` / `ground`
/ `assign_external` / `register_*`; `consequences_native` is the one *optional* method a
natively-capable engine overrides. The **core** provides, over that surface and *not* on the trait,
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
backend would never need — the reference solver (§12) is the standing check on this.

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
pub enum Determination {
    Consistent(Models),      // read the answer sets, or TAKE the owned WorldView (§5.2)
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
/// on a consistent search — yields the OWNED WorldView the query tier reads.
impl Solved<'_> {
    pub fn determination(&mut self) -> Determination;    // §5.1 — the trichotomy, with its payload

    /// Lazy stream; each item a Result, so a mid-stream engine fault surfaces at `?`, not as a clean
    /// end. Iterated by &mut so the terminal `conclusion` is readable after drain. Cost: O(1) resident.
    pub fn answer_sets(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_;

    /// A COMPLETE collection — available ONLY when the search closed the space; refuses otherwise
    /// (the exhaustion gate, the `WorldView::is_exhausted` analog). This is what makes "a truncated
    /// search passing as complete" unconstructible (§5.3), not merely visible via `conclusion`.
    pub fn all_answer_sets(&mut self) -> Result<Vec<AnswerSet>, NotExhausted>;

    pub fn conclusion(&self) -> Option<Conclusion>;   // readable once the search resolves
}

/// The `Consistent` payload (§5.1): read the answer sets, or TAKE the owned WorldView. The
/// borrowed-`Solved` → owned-`WorldView` transition is a move — the WorldView owns what the native
/// cautious/brave door needs (query.md §2.3–§2.4), so it outlives the borrow.
pub struct Models { /* … */ }
impl Models {
    pub fn world_view(self) -> WorldView;   // owned, non-empty by construction (query.md §2.3)
}

/// A PROVEN optimum — no public constructor; it exists only because the solver proved it.
pub struct Optimum { /* levels, in the objectives' own terms */ }
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

/// A fault is a value with a CLOSED locus taxonomy at the seam.
#[non_exhaustive]
pub struct Fault { /* a base::Diagnostic + the locus below */ }
pub enum Locus { Program, Request, Resource, Engine, Adapter }
impl Fault { pub fn is_backend_bug(&self) -> bool; }  // a closed bit
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
  bit; they are `themelios-base` diagnostics (loci and provenance), solved once, here, for every
  consumer.

Every model in this section is typed data first, with a human `Display` and a machine-consumable view
as derivations (§1.3).

---

## 6. Sessions and multi-shot mechanics

### 6.1 Ownership is the capability substrate

A session is an **owned value** — the authority to drive the engine. Dropping it is revocation; there
is no ambient engine and no global mutable state.

```rust
pub struct Session<B: Backend> { /* owns the backend + the program-side state (§6.2) */ }

impl<B: Backend> Session<B> {
    pub fn solve(&mut self) -> Result<Solved<'_>, Fault>;                    // no-options path
    pub fn solve_with(&mut self, opts: SolveOptions) -> Result<Solved<'_>, Fault>;
    pub fn ground(&mut self, parts: &[Part]) -> Result<(), Fault>;
    pub fn assign_external(&mut self, ext: Symbol, v: TruthValue) -> Result<(), Fault>;
    pub fn solve_assuming(&mut self, s: &Scenario) -> Result<Solved<'_>, Fault>;
    pub fn interrupt(&self) -> Interrupt;   // a cancellation handle, if declared (§6.3)
}
```

`solve` borrows the session (`&mut self`), so the borrow checker *is* the "no mutation while solving"
lock, and the multi-shot state machine (initial → grounded → prepared → solved) is expressed in
ownership and borrowing rather than runtime checks — an out-of-order call does not compile. Thread
posture is explicit per backend, and cancellation-from-another-thread is a declared capability whose
handle (`Interrupt`) is `Send`. A session is embeddable behind a service boundary or an editor host
without ceremony — the LSP/pythia posture (specification §1.2, §9.4). Cost: session construction is
one engine handle; `solve`'s cost is the engine's, streamed (§5.2).

### 6.2 Full-fidelity multi-shot

The multi-shot surface exposes the *complete* capability the engine offers, no lowest-common
denominator: build and ground `#program` parts incrementally, assign and release externals, solve
under assumption scenarios, interrupt, and re-solve. The session **retains the program-side state** it
needs to drive the cycle — it does not lower parts and then forget them (a lowering that discards the
parts it lowered cannot drive the cycle) — so the two-representation correspondence (owned program ↔
engine-internal state) is maintained across ground/solve/assign, which is also what makes a transform
on the owned side have a defined effect under multi-shot. Cost: retained state is `Θ(program size)`,
not `Θ(ground size)` — the ground instantiation stays in the engine.

### 6.3 Per-operation typed options; budgets; cancellation; assumptions

Configuration, where it exists at all, is **surfaced at the operation it affects and nowhere else** —
solve-options at `solve`, grounding-options at `ground`, and so on. Each is a typed options value
carrying `Default` (the empty case is free) and `#[non_exhaustive]` (a new knob is not a breaking
change), surfaced either as a paired method (`solve()` clean, `solve_with(SolveOptions)` configured)
or a fluent builder on the request. The bare `solve()` stays a no-options call — the pristine "just
the abstract object" path.

**Assumptions and scenarios are typed request-side values**, defined once so the blame surface (§5.4)
and multi-shot both use them. An `Assumption` fixes one program atom true or false for a solve; the
*raw set* of them is what the literature calls **assumptions**, and it is what `solve_assuming` scopes
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

**Budgets** (time at minimum, with room for model-count caps) are a typed, request-side surface;
enforcement is a declared capability — an engine without native support gets it through the adapter's
cancellation machinery — and `Conclusion::Budget` reports a hit budget as what it is. The long tail of
engine parameters, when a real consumer needs it, follows the two-tier facade pattern (typed knobs
over a legible open form); it is YAGNI-gated, grown on demand, never a CLI-string passthrough.

---

## 7. `@`-functions — a centerpiece (ground-time extension and the library door)

### 7.1 The mechanism

Named Rust functions (or a context value) register on a session; `@name(args)` calls into Rust through
a **panic-containing trampoline**; arguments and results cross as typed symbols via the conversion
pillar; multi-valued returns are supported; a failing `@`-function is a typed ground-time fault with a
locus.

```rust
/// A registered ground-time function. Registration is on the session (§4.1).
pub trait Function {
    fn call(&self, args: &[Symbol]) -> Result<SmallVec<Symbol>, GroundFault>;  // multi-valued
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
// Interface shape (governing principle: the DL/CP/LP litmus, §8.2 — finalized at build).
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
on *any* backend that implements the contract — clingo now, a native engine later. This turns the
propagator surface into a **theory-extension platform**: difference logic, linear/real arithmetic, and
**a full in-house CP theory — the clingcon alternative** — are Rust **satellites** built on it (their
own repos; themelios ships the *surface*, not the theories), best-of-breed and freed from the Potassco
C libraries (which drop to differential oracles, §11.2). The CP satellite's stated ambition is a
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
§8), the seam offers distinct doors of distinct grades:

- **Door A — typed AST → the grounder's input**, order- and span-preserving, where the highest
  fidelity is possible.
- **Door B — `themelios_program::Program` → the grounder's input**, canonical-order, carrying `Origin`
  provenance through to every ground rule (a capability the C grounder lacks). Programs constructed in
  Rust, transformed, or loaded through a client enter here.
- **Door C — aspif → the solver's ingestion**, for driving a solver from a foreign grounder and for
  the differential harness.

```rust
pub enum Door<'a> {
    Ast(&'a SyntaxTree),        // A: highest fidelity
    Program(&'a Program),       // B: canonical, provenance-carrying
    Aspif(&'a mut dyn AspifSource), // C: foreign grounder / differential
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

`themelios_program::Symbol` is engine-faithful (`i32`, `program.md` §3.1), so lowering a symbol is a
near-direct correspondence, not a re-intern. The FFI cost concentrates at the engine's process-global
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

### 11.1 clingo-only, and why

`themelios-potassco` wires **clingo** (5.8.2, the pinned authority) and nothing else: **libclingcon is
not bound.** The crate is named for the engine *family*, not `themelios-clingo`, for
**forward-compatibility**: should a future Potassco C library (a clingcon-successor with a stable ABI)
ever warrant binding, it is a feature of this crate under the same honest name, not a new crate each.
That is the whole of the family-name's warrant — it binds exactly one Potassco library today (clingo),
and our own CP path is a Rust theory on the propagator platform (§8), a satellite, *not* a linked C
library, so the family name promises room, not a present second binding. clingo-only keeps the shipped
trusted computing base one library smaller, and the `clingcon` capability a deployment might once have
paid for is gone with no loss — a theory-free program needs no CP theory.

### 11.2 clingcon-the-binary as an external oracle

Correctness of our Rust CP theory is proved the way the syntax/program/analysis tiers prove
themselves — against an **external binary oracle** invoked out-of-band (the clingo binary via pixi,
never linked). The **clingcon binary** plays the identical role one theory up: it is the differential
authority that keeps our CP theory's answer sets and constraint assignments honest, vendored for tests
only, never in the shipped stack.

### 11.3 The trusted computing base

The adapter is the TCB under the microkernel criteria (specification §12.3): FFI calls enumerated
against a per-area manifest, each privileged operation carrying stated pre- and postconditions, the
interning discipline (§10.5) implemented once behind the capability story, engine defaults explicitly
configured per request shape so the adapter cannot transmit an ambush upward, and panic containment on
every callback the engine makes into Rust. With the adapter feature disabled the entire stack is
FFI-free. The threat model of record (specification §12.4) lands before adapter-tier implementation
and is the security audit's object, not this design's.

---

## 12. The reference solver

`themelios-reference` is the naive, pure-Rust, `publish = false` solver: the independent oracle for
small cases, the **second implementor** that proves the contract is not clingo-shaped, and the
demonstration that a native backend built from foundation crates is a first-class implementor
(specification §9.1). It is also the **seed of the fragment-backend path**: because `themelios-analysis`
verdicts are sound in the direction that matters (tight ⇒ no unfounded-set check, HCF ⇒ no non-HCF
tester, Horn ⇒ no search, stratified ⇒ facts-only domains), a native backend can *declare its fragment*
through `Capabilities` (§4.1) and grow it up the lattice over time, routing the rest to the clingo
backend, with the differential run on the overlap. The reference solver's simplicity is deliberate (a
fast oracle you cannot trust defeats its purpose); the ambitious native engine is a separate project
(§14) that inherits the contract and this seed.

---

## 13. Assurance

### 13.1 The conformance suite

Executable, shipped with the contract, run by every adapter: outcome correctness on a corpus of small
programs with independently known answer sets; capability honesty (a declared-unsupported request must
refuse); the named pathologies (§5.3) attempted and structurally impossible; fault loci landing where
they belong; and **the ground-program observer produced faithfully** where declared (§10.4). The
reference solver is the second implementor and the independent oracle for small cases; the clingo
differential covers the large corpus.

### 13.2 Differentials and oracles

The reference-versus-clingo differential over the small-program corpus; the clingo binary as the
grounding/solving authority over the large corpus; the **clingcon binary** as the external oracle for
the CP theory (§11.2); the bridge differential with its worst-case cost tripwires (§10.1). Every
instrument documents what it proves *and what it cannot* (specification §10.2).

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
- **`theory-uniformity` (witness 15) is discharged by a worked CP witness *in themelios*.** With the
  clingcon adapter superseded (§11.1, §16), theory-uniformity is demonstrated by a worked CP
  propagator — `alldifferent` + `&sum`, real global-constraint CP — whose constraint assignments read
  back through `TheoryAssignments` (§5.4) as typed data, with the session-driving and outcome-reading
  code identical to first-solve but for the propagator registration. This is the §9.5 contingency's
  "demonstrated through the propagator surface," made permanent; the full best-of-breed CP theory is
  the satellite (§8.3, §14), of which this witness is the in-themelios floor.

The examples **span the tiers** (authoring exercises program+macros, driving exercises solve, reading
exercises query), making the crate-home split visible in the learning surface.

---

## 14. Scope and reserved seams

**The whole stack is the target.** There is no minimal "first increment" and no deadline pressure; the
mission-critical quality bar governs the pace, not a ship date. When the stage is done, the complete
tier ships: the contract, the outcome vocabulary and models, sessions and full multi-shot, all four
centerpieces and extraction, the bridge and its ground-program-IR capability, the potassco (clingo)
adapter, the reference solver, the facade, and the example set (reactive-tier witnesses included). The
query tier ships with it (`query.md`).

The **reserved seams** are only the genuinely-separate:

- the **theory-extension satellites** — a **full in-house CP theory (the clingcon alternative,
  exceeding clingcon-5's set)**, difference logic, and linear/real arithmetic — which are their own
  repositories built *on* the propagator platform, not part of this tier (the tier ships the platform,
  the difference-logic witness, and the CP theory-uniformity witness of §13.4 — not the satellites);
- **multi-threaded propagation** (the engine-level parallel-propagation problem, specification §9.6 —
  distinct from the intra-propagator parallelism of §8.4, which ships);
- the **native grounder and solver** (a separate program that implements this contract and grows the
  fragment-backend seed of §12);
- and the standing specification seams that touch this tier: the ground-program observer's fuller
  surface beyond what the committed §10.4 capability delivers, formal-methods tooling over the TCB,
  and additional engine backends beyond the Potassco family.

Each is named with its reason; none is a silent gap.

---

## 15. Acceptance criteria

The tier is done when all of the following hold:

1. **The contract is real and doubly implemented.** The potassco (clingo) adapter and the reference
   solver both pass the conformance suite; the pathologies are unconstructible; faults land at their
   loci; capability honesty holds.
2. **The two APIs exceed the evidenced comparators in capability and ergonomics** — a clean
   declarative macro face and a clean composable programmatic face, both first-class, coherent end to
   end, held to the Rust-exemplar bar and the comparator against clingo's Python API (specification
   §3.1).
3. **Full multi-shot fidelity** — assumptions, `#program` parts, repeated ground/solve, externals — no
   lowest-common-denominator, program-side state retained.
4. **The extension surfaces are complete and pleasant.** `@`-functions (with the library door and the
   `Facts` bulk conversion) and the propagator platform (validated against the DL/CP/LP litmus, the CP
   half a *full* constraint theory), each idiomatic-Rust and engine-portable; extraction over the
   conversion pillar.
5. **The committed clients build with minimal friction.** The design is validated against the
   definitely-planned in-house satellites: **elenctic** (the reading/query half), **xclingo** (the
   explanation half — the ground-program-IR capability), the **full in-house clingcon alternative**
   (the CP theory on the propagator platform), and **a theory-driven service consumer** (the
   pythia-class boundary — theory-driven solving + the service posture). The standard is absolute: *if
   a client cannot be built cleanly on the abstractions, the design is short.*
6. **The bridge is fast and faithful** — the differential against the engine and the worst-case
   tripwires green (§10.1, §13.3).
7. **The example set ships** (§13.4), and the mission bar holds throughout (§13.3).

---

## 16. Architecture, trust, and dependency reference

**Amendments to the specification, recorded here:**

- **Crate roster (§12.2).** The four adapter crates collapse to `themelios-potassco(-sys)` (§11.1);
  the query surface splits into the `themelios-query` sibling (`query.md`).
- **The clingcon adapter (§9.5, §4, §2 item 5, §12.2).** The specification's clingcon adapter is
  **permanently superseded** — not descoped under §9.5's temporary contingency, but replaced — by the
  in-house CP theory on the propagator platform (§8.3), with the clingcon binary retained as an
  external oracle (§11.2). This is a *stronger* act than §9.5's descope-with-resumption, taken for a
  smaller TCB and a best-of-breed Rust theory; it is recorded here (answering §4's "absent without the
  contingency invoked is a failure" — the absence is a considered supersession on the record, not a
  silent descope). Specification §2 item 5 ("clingo and clingcon are one experience") is
  **reinterpreted**: clingcon is no longer a backend, so the uniformity is now "*any* theory via the
  propagator surface presents its assignments as typed data, uniformly" (§5.4). Witness 15
  (`theory-uniformity`) is discharged by the worked CP witness of §13.4. **Revised §4 form** (a §2
  amendment owes its §4 restatement): clingcon's absence from v1 is *conformant by this recorded
  supersession* — not a §9.5 contingency, and not the silent descope §4 counts as failure — and the
  §2-item-5 uniformity failure now reads against the theory-typed-data form above, not against a
  missing clingcon backend.
- **The ground-program observer (§9.6, §13).** Promoted from a reserved seam to a **committed
  capability** (§10.4), with the two-part argument that defeats §9.6's rejection (an xclingo anchor
  now forces it; it is engine-free data, not added unsafe TCB — the cost is an adapter obligation).
- **Intra-propagator parallelism (§9.6).** Ships as a v1 capability the specification does not mention
  (§8.4); consistent with §9.6's "single solver thread," and distinct from the reserved engine-level
  multi-threaded-propagation seam.

**Trust architecture.** `themelios-solve`, `themelios-query`, and `themelios-reference` are
`forbid(unsafe_code)`, FFI-free by dependency closure. `themelios-potassco-sys` carries the vendored
bindgen output; `themelios-potassco` is the sole `allow(unsafe_code)` TCB, mechanism-only over the
bindings plus the safe adapter (§11.3). The structural trust check asserts forbid-in-pure-crates,
allow-only-in-the-named-TCB, and FFI-free closures for the pure crates. With the potassco feature
disabled the stack is FFI-free.

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
