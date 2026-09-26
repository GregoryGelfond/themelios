# themelios-query — design of record

2026-09-03. Draft, pre-implementation. This is the normative design for `themelios-query`, the
**query tier** — the engine-free epistemic reading over the solve tier's outcomes and the program
tier's patterns. It is the solve-stage sibling of `solve.md` exactly as `analysis.md` is the
program-stage sibling of `program.md`: `query : solve :: analysis : program`. It stands with
`specification.md` §9.7, `solve.md`, `program.md` (§7.7 patterns and unification, §11 the mgu), and
`analysis.md`.

The keystone: **the query tier answers the *epistemic* questions about a `Program` — is this true,
given the program; what are its bindings — where the answer has three values, not two.** The
mechanism it rests on (unification of patterns against ground symbols) is already built in the
program tier; the query tier is the *epistemic policy* over a collection of answer sets, and that
policy, not the mechanism, is its content.

The register of this document matches its built siblings (`program.md`, `analysis.md`): every
load-bearing surface is stated as a Rust signature with its refusal and its cost model. The
implementation is written at build time; the types, the laws, and the costs are decided here.

**Assumed fluency.** Fluent Rust and ASP as the grammar and specification of record state it; not
assumed are an engine's internals. *Taught, not assumed:* the three-valued epistemic reading (§2.2) and
the epistemic-specifications frame behind `WorldView` (§4), for an application author new to that
literature.

---

## 1. Keystone: the epistemic reading

`themelios-query` is engine-free and multi-client: it reads the solve tier's typed outcomes (§5 of
`solve.md`) and the program tier's pattern/unification surface, and it derives the reading a
knowledge-representation consumer wants. It is a *sibling* of the solve tier, not a layer inside it,
for the same reasons `themelios-analysis` is a sibling of the program tier: it is a distinct consumer
surface (its named client is the elenctic-successor), it is engine-free, and the analysis:program
symmetry places it beside solve rather than within it.

The register is the field's — cautious and brave consequence, entailment, three-valued query,
world views — extending `themelios-analysis`'s *structural* questions to the *semantic* ones. Its
answers are typed models with human and machine views (`solve.md` §1.3), never prose.

### 1.1 Opinionated default, primitives exposed

The tier is **opinionated on its default** epistemic semantics and **exposes the primitives beneath
it**, so that other semantics are consumer *derivations* rather than forks — the same three-layer
discipline `themelios-analysis` uses (its structural verdicts are derivations over exposed primitives,
and the primitives are never gated behind the verdict):

- **The default is the Gelfond–Kahl three-valued reading** (§2.2). It is the tier's first-class
  answer to *is this true, given the program*, and the register a knowledge-representation consumer
  reaches for first.
- **The primitives are first-class too** — the `WorldView` and its members, cautious and brave
  consequences, and the program tier's matching (§3.1) are public surfaces in their own right, not
  merely the inputs to `Answer`. A consumer that wants a *different* epistemic policy — the standard's
  two-valued cautious query (§2.6), a future epistemic-specifications reading (§4), a bespoke one —
  builds it over the primitives. This is checkable, not aspirational: **if a consumer cannot build an
  alternative reading on the exposed surface, the surface is short** (the tier-wide bar, aimed here at
  alternative-semantics consumers).

Default-and-expose, never choose-and-preclude: themelios commits to the reading its lineage prefers
without foreclosing the others.

---

## 2. The surface

### 2.1 The types, at a glance

```rust
// The epistemic answer. Closed — not #[non_exhaustive]: the trichotomy IS the affordance,
// and a fourth reading is what this type exists to forbid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer { Yes, No, Unknown }

// A program's world view (§2.3), in two forms split along the ENGINE axis (not the ownership axis):
// `WorldView<'a>` is the LIVE handle — borrowing the agent's engine (`'a`, from a retained agent,
// solve.md §6.2) or owning an ephemeral one (`'static`, from a single-shot `Program`, solve.md §6.4). It
// exposes the live-run material — the `members` stream, `is_exhausted`, `scenario` — and `materialize`;
// it carries NO epistemic reading of its own, because a reading is a self-contained solve, not a drain
// of this stream (§2.3). `Snapshot` is the engine-free owned value from `WorldView::materialize`, and it
// is where the INFALLIBLE readings (`&self`, pure data) live. Both are non-empty by construction (§2.3).
// The live-handle readings are hosted on the AGENT instead (`AgentReading`, §2.2/§2.7).
pub struct WorldView<'a> { /* live; the run material + materialize; see §2.3 for the invariant */ }
pub struct Snapshot     { /* engine-free, owned — the materialised world view + its readings (§2.3) */ }

// Cautious (⋂) / brave (⋃) consequences — the solve tier's typed sets, re-exported for the
// reading side (solve.md §5.2), each carrying the mode that produced it.
pub use themelios_solve::outcome::Consequences;

// Bindings of an open pattern, partitioned by the trichotomy (§2.5).
#[non_exhaustive]
pub struct Bindings { /* yes / no / brave-unknown partitions */ }

// The central input to `answer`/`entails`: a ground query — a literal, or a conjunction or disjunction
// of literals (Gelfond–Kahl Def. 2.2.2, errata-corrected; §2.2). Construction REFUSES anything that is
// not a ground literal, in two arms (`NotAQuery`, below): a well-formed **pattern** that is not ground
// (its question is its bindings, §2.5) and a **non-denoting** term (an interval, a pool, or arithmetic
// with a variable; program.md §11.2). So a `Query` that EXISTS is ground and denoting: the reads never
// fail on query *validity*. (On the AGENT they still return `Result<_, Fault>` for engine
// faults/exhaustion, §2.2; on a `Snapshot` they are infallible.)
pub struct Query { /* literal | conjunction | disjunction — a closed set of denoting shapes */ }
impl Query {
    pub fn of(atom: Atom) -> Result<Self, NotAQuery>;             // a literal; refuses a pattern or a non-denoting term
    pub fn all(parts: impl IntoIterator<Item = Query>) -> Query;  // conjunction (∧), evaluated per-model §2.2
    pub fn any(parts: impl IntoIterator<Item = Query>) -> Query;  // disjunction (∨), evaluated per-model §2.2
}
/// `Query::of`'s two refusal arms, distinguished so a consumer can classify a query form (elenctic's
/// classifier does, §4): a well-formed pattern that is not ground — ask its bindings (§2.5) — and a term
/// that is not a pattern at all (a non-denoting interval / pool / arithmetic-with-variable, program.md
/// §11.2), which carries the program tier's `NotAPattern` as its `source()`.
#[non_exhaustive] pub enum NotAQuery { NotGround { term: Term }, NotAPattern(NotAPattern) }

// A pattern is a signed `Atom` (program.md §11.2); the query tier reuses it directly — no new type.
pub use themelios_program::Atom;
```

The primitives (`WorldView::members`, the `Agent`/`Snapshot` `cautious` and `brave`, and the program
tier's matching) sit under the derived readings (`answer`, `bindings`, `entails`); §1.1 is why both are
public.

### 2.2 Three-valued `Answer` — the one authoritative definition

The core question is Gelfond–Kahl's: *is this true, given the program.* Its answer is
`Answer::{Yes, No, Unknown}`, with **`Unknown` a genuine value, never collapsed into `No`.** There is
**one** definition of `Answer`, stated here once; the literal case, the conjunction and disjunction
cases, and the matching relationship (§3.1) are all *derived* from it, so that "what `No` means" cannot
drift across the document.

**themelios adopts the Gelfond–Kahl three-valued query answer as its default — the opinionated stance
(§1.1).** The definition below is Gelfond & Kahl's *Definition 2.2.2 (Answer to a Query)* **as
corrected by the authors' published errata**: the uniform per-member reading, which supersedes the
book's original statement of the conjunctive and disjunctive cases over a *single* cautiously-entailed
literal. Choosing the Gelfond–Kahl reading as this tier's default is the opinion — not a correction of
it; the primitives beneath it (`cautious`/`brave`, the `WorldView` and its members, the program tier's
matching) are exposed so a consumer wanting a different policy — the standard's two-valued cautious
query (§2.6), a bespoke epistemic reading (§4) — *derives* it rather than forking.

Let a **world view** `W` be a non-empty set of answer sets (§2.3), and let `q` be a ground query — a
literal, or a conjunction or disjunction of literals. The **contrary** of a ground atom `a` is its
strong negation `-a` (and the contrary of `-a` is `a`). Evaluate `q` **within each member** of `W`
under the three-valued reading — a literal is *true* in a member containing it, *false* in one
containing its contrary, *unknown* otherwise; a conjunction is the weakest (`min`) and a disjunction
the strongest (`max`) of its parts over `false < unknown < true` — and quantify over the members:

- **`Answer::Yes`** iff `q` is **true in every member**.
- **`Answer::No`** iff `q` is **false in every member**.
- **`Answer::Unknown`** otherwise.

The load-bearing subtlety is the boundary between `No` and `Unknown`, and it is where a reader from
SQL or Prolog goes wrong first: **absence is not falsity.** A literal merely *missing* from a member is
not the same as its contrary being *present*. Worked, on the world view `{ {a}, {b} }` and the query
`a ∧ b`:

- In member `{a}`, `a` is true but `b` is *absent* — `-b` is **not present**, so `b` is *unknown*
  there, and `a ∧ b` (the `min`) is *unknown*, not false.
- So `a ∧ b` is not false in every member → the answer is **`Unknown`**, not `No`.

Contrast `{ {sunny, warm, -swim}, {swim, -warm} }` and `warm ∧ swim`: the first member has `-swim` (so
`swim` is false there), the second has `-warm` — `warm ∧ swim` is false in *every* member, so the
answer is a genuine **`No`**. And the errata's own point shows on `{ {a}, {b} }` with the *disjunction*
`a ∨ b`: `a` is true in the first member and `b` in the second, so `a ∨ b` (the `max`) is **true in
every member** and the answer is **`Yes`** — though no single disjunct is cautiously entailed. The
book's pre-errata statement, keyed to one entailed disjunct, would have called this `Unknown`; the
corrected per-member reading calls it `Yes`. The distinction between these cases is the whole point of
the third value; the closed-world assumption is something a program states for itself
(`-p(X) :- not p(X).`), and ASP does not impose it, which is exactly why the answer has three values.

**The insight this preserves.** A query is evaluated *within* each member — the model quantifier scopes
the whole query — **never over ⋂ or ⋃**, because `brave(a) ∧ brave(b)` does not give `brave(a ∧ b)` and
`cautious` cannot express a per-model refutation. That is correct and is the reason `Answer` is not a
projection of the consequence sets.

**Signature and cost.**

```rust
// The reading hangs on the AGENT — through the query-side `AgentReading` facade (§2.7), impl'd for
// `Agent<B>` and re-exported in the prelude so `agent.answer(q)?` reads inherent — where it is
// engine-driving and FALLIBLE (each call solves once, §2.7); and on a materialised `Snapshot`, where it
// is engine-free and INFALLIBLE. It is NOT on the live `WorldView`: a reading is a self-contained solve,
// not a drain of that handle's `members` stream (§2.3).
pub trait AgentReading {   // impl'd for `Agent<B>` (solve.md §6); solves once, then reads
    /// The Gelfond–Kahl three-valued reading of a ground query (drives the engine, then reads; §2.7).
    fn answer(&mut self, q: &Query) -> Result<Answer, Fault>;
    fn entails(&mut self, q: &Query) -> Result<bool, Fault>;       // §2.6
    fn bindings(&mut self, pat: &Atom) -> Result<Bindings, Fault>; // §2.5; the refusal is flattened — note below
    fn snapshot(&mut self) -> Result<Snapshot, Fault>;             // §2.7 — solve once + materialise (whole program)
    /// A scenario-scoped `Snapshot` (§2.7): `solve_assuming` once, then materialise, so every reading off
    /// it ranges over the scenario's models. REQUIRES the backend's `assumptions` capability (a scenario
    /// needs `solve_assuming`) — otherwise `Fault::unsupported()` at `Locus::Request`; else the same
    /// refusals as `snapshot` (no answer set under the scenario, or an unclosed search). Cost: one
    /// `solve_assuming`, then `Θ(|W|)` to materialise.
    fn snapshot_assuming(&mut self, s: &Scenario) -> Result<Snapshot, Fault>;
}
impl Snapshot {            // the engine-free form — the same reading over materialised data, infallible
    pub fn answer(&self, q: &Query) -> Answer;
}
// NAMED DEPARTURE (§1.3): the agent's `bindings` returns `Result<Bindings, Fault>` — the query-owned
// `NotABindingPattern` (§2.3) is folded into the `Fault`'s message text, because `Fault` (solve.md §5.4)
// carries a message, not a source chain. So on the AGENT path the `AnonymousPosition` / `NonDenoting` /
// `Pooled` distinction is legible only as prose, where §1.3 asks for typed data; the typed refusal
// survives intact on `Snapshot::bindings` (§2.3). Carrying it typed on the agent (a `Fault` with a source,
// or a `Result<Bindings, ReadingRefusal>`) is a reserved refinement.
```

- An **atomic or literal** query is two cautious-membership checks (`q` entailed → `Yes`; contrary
  entailed → `No`; else `Unknown`) — *in principle* one solve, no enumeration, through the native
  cautious door (§2.4). **In v1 the agent's reading does not yet take that door:**
  `AgentReading::answer` solves once and **materialises the whole world view** (`Θ(|W|)`), then reads the
  atomic case off it. Routing `answer`'s `Yes`/`No` through `Agent::cautious`/`brave` and materialising
  only for the compound boundary below is a **named seam** — its sound `No ⟺ contrary ∈ ⋂` shortcut lands
  with the native cautious door at the adapter, under a property test — not a v1 cost. A `Snapshot::answer`
  over already-materialised data is the cheap read (no solve).
- A **conjunction or disjunction** is model-scoped on the boundary its shape does *not* project. One
  side is a cheap cautious check: a conjunction is `Yes` iff every conjunct is in `⋂` (all cautiously
  true), a disjunction is `No` iff every disjunct's contrary is in `⋂` (all cautiously false). The
  *other* side — a conjunction's `No`, a disjunction's `Yes` — together with the `Unknown` boundary are
  *not* expressible over `⋂`/`⋃` (they can hold through *different* literals per member, which is the
  errata's whole point), so they cost an enumeration of `W` (linear in `|W|`, which can be exponential),
  or, later, a solver-side satisfiability check that no member escapes the answer. This asymmetry — the
  atomic case cheap *once the native door is taken*, the compound case enumeration-bound — is stated
  because it is exactly the cost surprise a prose contract would hide. Both the native atomic routing and
  the satisfiability check are named seams, not v1 promises; v1's agent reading materialises.

**Laws** (checked as properties, §4): a query and its contrary are never both `Yes`; `answer` is
never `No` on a *non-empty* query nothing refutes (the absence-is-not-falsity law) — the one exception is
the **empty disjunction** `Query::any([])`, the lattice bottom (⊥), which reads `No` by definition,
refuting nothing yet false in every member; a disjunction whose members each mention some disjunct is
`Yes` even when no single disjunct is cautiously entailed (the errata law); on a singleton world view
`answer` agrees with membership-and-contrary in the one model.

### 2.3 `WorldView` — non-empty by construction

A **`WorldView`** is a program's *world view* in the epistemic-specifications sense; for a plain
program (this tier's scope) that is the program's **unique** world view, which is exactly its set of
answer sets.

The invariant is a **type state**, and it is the correctness keystone here: **a `WorldView` value is
non-empty by construction.** It is obtained only from a `Consistent` outcome, so a `WorldView` you
*hold* has at least one member; the empty world view of an *inconsistent* program is **not a
representable `WorldView`** — inconsistency is carried by `Determination::Inconsistent` (`solve.md`
§5.1), never by an empty world view. ("The world view of an inconsistent program is empty" is a
true mathematical statement about the *notion*; it is deliberately not a constructible *value*, because
a value meaning "invalid" inside the space of valid world views would be a sentinel — the pathology
`solve.md` §5.3 forbids.)

```rust
impl<'a> WorldView<'a> {   // the LIVE handle — the run material; `members` is `&mut self`, the rest `&self`
    pub fn of(models: Models<'a>) -> WorldView<'a>;   // the construction door — from a resolved `Consistent(Models)` (solve.md §5.2)
    pub fn members(&mut self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_;  // stream (below)
    pub fn is_exhausted(&self) -> bool;                          // pure — a REPORT (drain-dependent), not the gate; see below
    pub fn scenario(&self) -> &Scenario;                         // pure — what it ranged over
    pub fn materialize(self) -> Result<Snapshot, Fault>;         // drain-then-gate to an engine-free `Snapshot` (eager; opt-in)
}
// The cautious/brave consequence door and the epistemic readings (`answer`/`bindings`/`entails`) are
// deliberately NOT on the live handle — they are on the AGENT (`Agent::cautious`/`brave` inherent,
// solve.md §6.2; `AgentReading::answer`/`bindings`/`entails`/`snapshot`, §2.7) and on the materialised
// `Snapshot` (below). A reading is a self-contained solve, so it composes freely instead of draining
// this handle's one live read, the `members` stream.

/// The engine-free form: `WorldView::materialize` drained the world view into owned data, so every read
/// is INFALLIBLE `&self`. Non-empty by construction and complete, like the live handle it came from.
impl Snapshot {
    pub fn cautious(&self) -> Consequences;
    pub fn brave(&self) -> Consequences;
    pub fn members(&self) -> impl Iterator<Item = &AnswerSet> + '_;
    pub fn is_exhausted(&self) -> bool;                           // a snapshot is complete
    pub fn scenario(&self) -> &Scenario;
    pub fn answer(&self, q: &Query) -> Answer;                    // §2.2
    pub fn bindings(&self, pat: &Atom) -> Result<Bindings, NotABindingPattern>; // §2.5 — a query-owned refusal (below)
    pub fn entails(&self, q: &Query) -> bool;                     // §2.6
}
/// The `bindings` refusal — the program tier's `NotAPattern` (a non-denoting term) *plus* the query tier's
/// own partition policy: an anonymous position (`p(X,_)`) is a well-formed pattern to the mgu (`_` denotes;
/// it matches anything) but is refused HERE, not laundered into `NonDenoting`. The reason is a policy
/// nudge toward NAMED bindings: `_` names no binding, and a fresh named variable yields the same ground
/// instances, so nothing is lost. (The both-sides hazard is real at the *substitution* level — a
/// substitution `{X=a}` with `_` projected away could be cautiously-yes and cautiously-no through
/// different `_`-fillers — but `Bindings` holds ground *instances* (§2.5), each of which lands in exactly
/// one cell, so it is not the built instance partition that breaks.) `query.md` §3.1 keeps matching apart from policy.
#[non_exhaustive] pub enum NotABindingPattern { NotAPattern(NotAPattern), AnonymousPosition }
```

Properties and cost:

- **Live vs snapshot, exhaustion-gated, scenario-scoped.** A `WorldView<'a>` is the live handle —
  borrowing the agent's engine (`'a`) or owning an ephemeral one (`'static`, single-shot, solve.md §6.4);
  a `Snapshot` (from `materialize`) owns its data, engine-free. A universal reading (all members, all
  optimal, a cautious consequence) is answerable only from a search that closed the space, and the gate
  lives at the point the universal value is produced: **`materialize` drains and *then* gates** — it
  returns `Err` if the search did not close (§3.2), or if the members were already streamed (a
  partially-drained live handle cannot yield a complete snapshot) — and the agent's
  `cautious`/`brave`/`answer` solve to
  exhaustion before reading. A `Snapshot` is complete by construction. `is_exhausted` on the live handle
  is a **report, not the gate**: it is drain-dependent — reading the run's terminal conclusion, it is
  `false` on a fresh, not-yet-drained consistent view and becomes `true` only after the stream drains to
  its end — so it answers "is this *known* complete now?", while `materialize` (not a prior `is_exhausted`
  check) is what makes a universal reading refuse honestly.
- **Lazy where possible.** Because a world view can have very high cardinality (exponentially many
  answer sets), the agent's `cautious`/`brave` go through the **native door** (§2.4) — one solve, *no*
  enumeration — where the backend declares native consequences, and fold an enumerated world view
  otherwise (capability-routed, §4.2); and `members` **streams** (each item a `Result`, so a mid-stream
  engine fault surfaces at the item, not as a clean end). Materializing the whole member set is **opt-in,
  never forced**; genuine lazy/incremental *machinery* beyond streaming is the specification's §7.8
  reserved seam.
- **Receiver, fallibility, and exclusion.** The live `WorldView<'a>` drives the engine only through its
  `members` stream (**`&mut self`**, each item a `Result<_, Fault>`) — matching `solve.md`'s `Solved`
  (compile-time serialisation by the borrow checker, no interior mutability). The epistemic readings that
  drive the engine are the **agent's** (`&mut self -> Result<_, Fault>`, §2.7), giving engine faults, the
  exhaustion refusal, and a non-pattern each a typed home (`Fault`, with `Locus` as appropriate): each
  solves once, so a reading is a self-contained call and the `&mut self` borrow is the "no reasoning while
  mutating" lock (solve.md §6.1). Because a reading does not borrow a live `WorldView`, there is no
  live-handle re-entrancy to refuse — holding a `members` stream is a `&mut` borrow that cannot overlap
  another use of the same handle, the borrow checker forbidding it at compile time. The **`Snapshot`**
  (from `materialize`) needs no engine; its reads are infallible `&self`. The fallibility axis lives here
  — engine-driving vs engine-free — kept off the ownership/lifetime axis.
- **Under an optimization objective the world view is the set of *optimal* answer sets** — those tied
  at the proven optimum (`solve.md` §5.2); with no objective it is all stable models (the degenerate
  case). A query therefore ranges over *the answer sets the program denotes*, uniformly, so the
  all-versus-optimal distinction collapses into that denotation — which is what keeps the semantics
  uniform whether or not the program optimizes. The exhaustion gate then requires the optimum *proven*
  and the optimal set exhausted before such a world view is valid.

### 2.4 Cautious and brave consequences

**Cautious** (`⋂`, "what must hold") and **brave** (`⋃`, "what can hold") consequences project a world
view; each is the solve tier's `Consequences` value (`solve.md` §5.2), carrying the mode it was
computed in, so a value that has travelled still says which question it answers. They are **not**
answer sets and carry their own type for that reason.

```rust
impl<B: Backend> Agent<B> {   // the native door lives on the agent — it owns the engine (solve.md §6.2)
    pub fn cautious(&mut self) -> Result<Consequences, Fault>;   // ⋂ over the whole program — native (one solve) or derived fold, capability-routed
    pub fn brave(&mut self)    -> Result<Consequences, Fault>;   // ⋃ over the whole program
    pub fn cautious_assuming(&mut self, s: &Scenario) -> Result<Consequences, Fault>;  // ⋂ over the scenario's models (needs `assumptions`; precondition + cost: solve.md §6.2)
    pub fn brave_assuming(&mut self, s: &Scenario)    -> Result<Consequences, Fault>;  // ⋃ over the scenario's models
}
// a `Snapshot`'s `cautious`/`brave` are the infallible mirror over its materialised members (§2.3);
// `snapshot_assuming(&Scenario)` (§2.2, §2.7) yields a scenario-scoped `Snapshot`, so every reading has a scoped form.
```

**Two doors, and the free differential.** The **native door** — `Agent::cautious`/`brave` when the
backend declares `native_consequences: Native` — has the solver compute `⋂`/`⋃` directly (one solve, no
enumeration); the **derived door** folds an enumerated world view (the agent's `cautious`/`brave` under
`DerivedByEnumeration`, or a `Snapshot`'s infallible `cautious`/`brave` over its materialised members).
The two **must agree**, and their agreement is a standing differential the tier gets for free — the
solver solves through a foreign engine, and an independent check on consequence computation is otherwise
hard to come by. Cost: native is one solve; derived is `Θ(|W|)` in members folded, and is why the native
door exists.

**Under an optimization objective both doors must range over the *optimal* answer sets** (§2.3;
`solve.md` §5.2). The derived door does so by construction — it folds the optimal world view — and the
native door carries the matching obligation: the solver computes `⋂`/`⋃` over the *optimal* set, under
the optimum-proven/exhausted gate. So the required agreement is over the same model set; without that
obligation the two would either disagree or, worse, agree while both range over all stable models and
silently violate the denotation.

### 2.5 Bindings and conjunctions

**Bindings** partition an open pattern by the same trichotomy:

```rust
#[non_exhaustive]
pub struct Bindings { /* … */ }
impl Bindings {
    pub fn yes(&self)     -> impl Iterator<Item = &Symbol>;  // cautiously entailed instances
    pub fn no(&self)      -> impl Iterator<Item = &Symbol>;  // contrary cautiously entailed
    pub fn unknown(&self) -> impl Iterator<Item = &Symbol>;  // the brave domain — see below
}
```

A conjunction inside a pattern is evaluated **within each answer set** exactly as §2.2 defines it —
the model quantifier scopes the whole query, never `⋂`/`⋃`.

**The `unknown` listing is the brave domain of the pattern's own sign, and says so.** `yes` and `no` are
read off the cautious consequences — finite, exact. `unknown` is *everything the program does not
settle*, and this tier holds answer sets rather than the program that produced them, so it cannot
enumerate that domain; a listing therefore shows the **brave** domain **restricted to the pattern's own
sign** — the instances of the pattern present in *some* answer set — and closes with the sentence that
says so, or it reads as exhaustive and teaches the very misreading it exists to prevent. An instance
whose *contrary* alone is bravely present (its `answer` is `Unknown`, but only `-g` is ever mentioned) is
**not** listed here; it appears under the **contrary pattern**'s `unknown`. (Whether "mentions" should
instead span mention-by-contrary is a spec-owner question, §3.2; the surface as built takes the pattern's
own sign.) Cost: on a `Snapshot`, `yes`/`no` are cautious-set reads and `unknown` is bounded by the brave
domain; the **agent**'s `bindings` materialises the world view first (`Θ(|W|)`, §2.2).

### 2.6 The ASP-Core-2 cautious query — a dialect-scoped derivation

The Gelfond–Kahl `Answer` (§2.2) is the tier's default. The **ASP-Core-2 standard** defines its own
query answering (`atom "?"`, grammar §6.1), and its semantics is **cautious** and **two-valued**
(entailed / not-entailed). This is specification **witness 20** (`asp-core-2`), a v1 obligation of
this surface — and it is a *different* question from the three-valued default, so it is exposed as its
**own** operation, not a rename of `Answer`:

```rust
impl<B: Backend> AgentReading for Agent<B> {   // solves once, then reads (fallible); §2.7
    /// The ASP-Core-2 standard's query answer: cautious, two-valued (witness 20, grammar §6.1).
    fn entails(&mut self, q: &Query) -> Result<bool, Fault>;
}
// a `Snapshot`'s `entails` is the infallible mirror over its materialised data (§2.3)
```

The exact relation to `Answer`, stated because it is non-trivial: the standard's *entailed* is
`Answer::Yes` (cautiously entailed); the standard's *not-entailed* spans **both** `Answer::No` **and**
`Answer::Unknown`. So the ASP-Core-2 query is the projection

> `entails(q) == (answer(q) == Answer::Yes)` — i.e. `Yes` vs `(No ∪ Unknown)`,

never a two-way collapse that would send `Unknown` to the wrong side (a mis-lowering the specification
§4 counts as failure). It is a **dialect-scoped** operation: `entails` is the ASP-Core-2 dialect's
reading; where a dialect's query semantics diverges from the clingo-world default beyond this
projection, that is a per-dialect choice, named rather than silently unified (§1.1's
default-and-expose). Cost: on a `Snapshot`, one cautious membership read; on the **agent**, `entails`
materialises the world view first (`Θ(|W|)`, §2.2), the single-solve native-cautious-door path being the
same seam `answer` names.

**The non-ground query is answered by substitution.** The ASP-Core-2 query admits variables
(`q(X)?`, grammar §6.1), and the standard answers a non-ground query by *substitution* — the set of
cautiously entailed instances, not a boolean. That answer is **`bindings(pat)?.yes()`** (§2.5): the
cautiously-entailed instances of the pattern *are* the standard's cautious substitution answer, under
the same `Yes` vs `(No ∪ Unknown)` projection `entails` draws for the ground case (an instance is in
`yes()` iff its `answer` is `Yes`, never the three-valued partition mistaken for the two-valued cautious
answer). So witness 20 is served for both shapes — `entails` for a ground query, `bindings(pat)?.yes()`
for a non-ground one — and the query surface is silent on neither.

### 2.7 The dual face

The query surface carries the two centerpiece faces of `solve.md` §3 through the reading side: a
declarative macro form (a `query!` / `ask!`-style spelling of the goal, through the one grammar,
expanding by the macro law to the same programmatic calls) and the composable programmatic form
(`answer`, `bindings`, `entails`, `cautious`, `brave`, and `snapshot`). A query's goal is authored the
same way a program's atoms are; a run-time patient name, a generated goal, or an LLM's question enter
through the programmatic form.

**The readings hang on the agent, through the `AgentReading` facade.** `answer`, `bindings`, `entails`,
and `snapshot` are a **query-side extension trait `AgentReading`, impl'd for `solve.md`'s `Agent<B>`** and
re-exported in the prelude, so `agent.answer(q)?` reads inherent while `themelios-solve` keeps no
dependency on `themelios-query` (the dependency is one-directional — query depends on solve, the tier
direction, §2.1's `pub use themelios_solve::Consequences`; the reverse would cycle, and `WorldView::of`
keeps the construction side acyclic too, §2.3). `cautious`/`brave` are the agent's own (`solve.md` §6.2), the `Consequences`
type being the solve tier's. Each of these **solves once**, then reads — an owned answer, freely composed
— rather than borrowing and draining a live `WorldView`; that is why the readings are not on the live
handle (§2.2, §2.3). A `Snapshot` (from `materialize`) mirrors them infallibly over materialised data,
for a reading that must outlive its engine or cross a service boundary.

**Under a scenario, the same surface repeats with the `_assuming` suffix** — the epistemic sibling of
`solve_assuming`, so a reading under a hypothesis is as first-class as a solve under one. The
scenario-scoped consequence doors are the agent's `cautious_assuming`/`brave_assuming` (their precondition
and cost stated once, at `solve.md` §6.2), and `snapshot_assuming(&Scenario)` (§2.2) yields a
scenario-scoped `Snapshot` whose infallible readings all range over that scenario's models — so every
reading has a scoped form without doubling the fallible agent surface, and `ConsequenceRequest`'s scenario
field (`solve.md` §4.1) is produced by the surface rather than merely consumed by a backend.

The `WorldView` the stream/materialise path ranges over is obtained from a resolved `Determination`'s
`Consistent` branch (`solve.md` §5.2), and the *reading* is the same whether a `Program` is asked directly
(single-shot) or an `Agent` is asked within its reasoning loop (`solve.md` §6): the epistemic questions
and their answers are identical either way. The forms differ only in whether they hold an engine — the
agent's fallible readings and the live `WorldView<'_>`'s `members` stream drive one, versus an
engine-free `Snapshot`'s infallible reads after `materialize` (`solve.md` §6.4) — not in what they answer.

---

## 3. Mechanism reused, policy owned

### 3.1 The matching mechanism is inherited — and it is *not* the epistemic answer

The matching a query rests on is the program tier's **mgu** (`program.md` §11, Q3): a general
primitive built with epistemics deliberately deferred downstream — *program owns the mechanism,
epistemic policy downstream.* The query tier inherits the whole thing, hardened: the
Martelli–Montanari unifier (near-linear, the deep-ground-symbol quadratic already closed and the
occurs-check forced), the triangular substitution, and `signature_range`'s `O(log n + k)` candidate
enumeration off `Symbol`'s order. Unification of a pattern against a ground symbol is the degenerate,
matchable case; a non-Herbrand pattern is **refused, not guessed** — an interval names a *set* of
atoms, and whether that reads as "all" or "any" depends on a position a bare pattern cannot carry, so
the door will not invent a quantifier the caller never wrote.

The mgu's three-outcome result is a **matching** result, and the correction here is to keep it distinct
from the epistemic `Answer`:

```rust
// program.md §11 — the MATCHING mechanism, not the epistemic reading:
//   Ok(Some(mgu))  a most general unifier exists (the pattern matches this symbol)
//   Ok(None)       no unifier (this symbol does not match)
//   Err(NotAPattern)  a REFUSAL — "I cannot answer that question of this term"
```

These are three levels below the epistemic trichotomy and must not be conflated with it:

- `Ok(None)` ("this symbol does not match the pattern") is **not** `Answer::No` (which requires the
  *contrary* cautiously entailed). Matching is per-symbol; the epistemic answer is per-world-view.
- `Err(NotAPattern)` is a **refusal**, a typed diagnosis with a `source()` chain (§4) —
  `program.md` §11.2's *"I cannot answer that question of this term"* — **not** the epistemic
  `Unknown` **value**. `program.md` §11.4 deliberately keeps the mgu mechanism and the epistemic
  reading apart; the query tier honours that separation: `bindings` *uses* matching to find candidate
  instances and *then* applies the §2.2 policy to classify them.

### 3.2 The epistemic policy, with its corrections banked

What the query tier *owns* is the epistemic reading over a **collection** of answer sets, and that is
where the care goes — the mechanism is done, the policy is small but has teeth. The corrections the
evidenced prior art records (the elenctic class, specification §5.1) are inherited rather than
rediscovered, and all three are now consequences of the one §2.2 definition:

- **"Neither entailed → no" was measured unsound.** `No` holds iff the **contrary** is cautiously
  entailed, not merely because the query itself is not; a query nothing speaks to is `Unknown`. (This
  *is* §2.2; it is restated here because it is the correction the prior art paid for.)
- **`yes` and `no` are exact; `unknown` is not enumerable here** (§2.5) — the listing shows the brave
  domain of the pattern's own sign and says so.
- **The universal readings gate on exhaustion** (§2.3) — a universal claim is answerable only from a
  closed search: `materialize` drains *then* gates and the agent's `cautious`/`brave`/`answer` solve to
  exhaustion before reading, while `is_exhausted` merely *reports* known-completeness (drain-dependent) and
  is not itself the gate.

`themelios-query` computes these facts once, here, for every consumer, and never re-litigates the
model quantifier per tool.

---

## 4. Acceptance, assurance, reserved seams

**elenctic is the acceptance test.** The elenctic-successor (a declarative ASP testing framework, to
be rewritten in Rust on themelios) is the query tier's named arm's-length consumer — and the near-term
priority: it is built **first** among the solve-stage committed clients (`solve.md` §15) and retires the
standing Python project. Its `SolveResult` is, structurally, the solve tier's `Determination`, and its
verdict system already treats
cannot-decide as a value never collapsed into "no". The standard is the tier-wide one: **if
elenctic-on-themelios cannot be built cleanly on this surface, the design is short.** Its query-form
classifier and its cautious/brave/optimal reasoning modes are the concrete checklist — and, per §1.1,
so is *at least one alternative epistemic reading built over the primitives* (the ASP-Core-2 cautious
query, §2.6, is the shipped proof that the primitives suffice).

**Assurance.** The native/derived consequence agreement (§2.4) as a standing differential; property
laws over the trichotomy (§2.2 — a query and its contrary not both `Yes`; `no()` and `yes()` disjoint;
the conjunction-within-a-model law; the `entails == (answer == Yes)` projection law of §2.6); the
mgu's own hardened suite inherited from the program tier; executed examples (the *three-valued-query*
witness, both faces). No panic on any input; the non-Herbrand refusal is a typed diagnosis with a
`source()` chain.

**Reserved seams, and the world-view horizon.** The `WorldView` here is a plain program's *unique*
world view; two named futures generalize it, in order. **Full epistemic specifications** — subjective
literals (the K/M modalities) authored *in* the program, whose semantics is a *set* of world views
under the recent founded-world-view semantics (the original semantics admitted self-supported world
views; the fixes are recent) — a solver in the eclingo class, a not-far-fetched future goal, and
exactly the kind of alternative reading §1.1's primitives-first surface exists to admit. **P-log-class
probabilistic ASP** (specification §1.1) — a probability measure over the collection of possible
worlds, which *is* the world-view notion — builds naturally on that epistemic-specs substrate. Also
reserved: semantic equivalence checking (ordinary and strong equivalence as decision services — the
program tier's named seam, distinct from this epistemic reading); and bindings whose values are
constraints rather than symbols (a goal-directed engine's output, anticipated by `Bindings`'
`#[non_exhaustive]`).

A note on the type name. `WorldView` imports the epistemic-specifications frame for what is, in v1
scope, "the set of answer sets." The choice is §1.4-compliant ("world view" is a literature term) and
deliberate — it is the forward-compatible on-ramp to the epistemic-specs seam above — traded knowingly
against least surprise for an application author new to the literature; the standing choice is to keep
it.

---

## 5. Revisions

1. **Initial design of record** (2026-09-03).
2. **Refinements** (2026-09-03). One authoritative definition of `Answer` (the
   strong-contrary reading, §2.2), from which the atomic, conjunction, and matching cases derive; the
   matching mechanism (§3.1) recast as distinct from the epistemic answer and the `NotAPattern`
   refusal. `WorldView` stated non-empty by construction, with inconsistency carried by
   `Determination`, not an empty value (§2.3). The ASP-Core-2 cautious query added as a dialect-scoped
   derivation, `Yes` vs `(No ∪ Unknown)` (§2.6, witness 20). Opinionated-default / primitives-first
   framing made explicit (§1.1). Raised to the type/interface/cost-model register of the built
   siblings.
3. **Completeness refinements** (2026-09-03). The central `Query` type defined — an atom / literal /
   conjunction whose construction refuses a non-denoting term, which is why `answer`/`entails` are
   infallible (§2.1). `Pattern` resolved to the program tier's signed `Atom` (§2.1, §2.5). The
   ASP-Core-2 *non-ground* query answered by substitution — `bindings(pat)?.yes()` as the standard's
   cautious substitution answer (§2.6, witness 20). The native consequence door's obligation to range
   over the optimal set under an objective stated (§2.4).
4. **Alignment with the reasoning-loop reframe** (2026-09-23). The epistemic reading is unchanged; its
   vocabulary is aligned to `solve.md` §6's agent framing — the `WorldView` it ranges over is obtained
   from a `Program` asked directly or an `Agent` in its reasoning loop, identically (§2.7). elenctic is
   recorded as the **first** of the solve-stage committed clients and the near-term priority (§4).
5. **Construction and refusal refinements** (2026-09-24). `WorldView::of(models)` is stated as the
   construction door — a `WorldView` is built on the query side from a resolved `Consistent(Models)`
   (solve.md §5.2, §6.4), keeping `themelios-solve` free of a query dependency (§2.3). `bindings` refuses
   through a query-owned `NotABindingPattern { NotAPattern, AnonymousPosition }`: an anonymous position is
   a well-formed pattern to the mgu but breaks the yes/no/unknown partition, so it is refused in the query
   tier's own type rather than laundered into the program tier's `NonDenoting` (§2.3, §2.5).
6. **Agent-facade reconciliation** (2026-09-25). The reading surface is aligned to the built agent-facade
   shape (`solve.md` §6.2, revision 6). The epistemic readings are **not** on the live `WorldView`:
   `answer`/`bindings`/`entails`/`snapshot` are a query-side extension trait `AgentReading` impl'd for
   `solve.md`'s `Agent<B>` and re-exported in the prelude, and `cautious`/`brave` are the agent's own;
   each **solves once**, then reads, rather than borrowing and draining a live handle (§2.2–§2.7). The
   live `WorldView<'a>` keeps only the run material — `of`/`members`/`is_exhausted`/`scenario`/
   `materialize` — and a materialised `Snapshot` mirrors every reading infallibly (§2.3). `is_exhausted`
   is recast as a drain-dependent **report**, not the gate; the universal-reading gate is `materialize`
   (drain-then-gate) and the agent's solve-to-exhaustion (§2.3, §3.2). The anonymous-position refusal's
   rationale is corrected — it is a policy nudge toward named bindings, the both-sides hazard being a
   *substitution*-level fact rather than the built ground-instance partition (§2.3). `unknown` is
   clarified as the brave domain **of the pattern's own sign**, an instance whose contrary alone is bravely
   present being listed under the contrary pattern (whether "mentions" should span mention-by-contrary is
   left a spec-owner question, §2.5, §3.2). §2.2's absence-is-not-falsity law is given its
   **empty-disjunction** exception (`Query::any([])` is ⊥ and reads `No`). Several surfaces are aligned to
   the built code: `NotAQuery`'s two arms are stated (`NotGround` — a pattern, ask its bindings — vs
   `NotAPattern`, §2.1); the agent's readings are shown to **materialise the world view** (`Θ(|W|)`) with
   the native atomic-routing shortcut a named seam (§2.2/§2.3/§2.5/§2.6); the agent-side `bindings`
   signature is stated with the `NotABindingPattern`→`Fault` flattening named as a departure (§2.2);
   `cautious`/`brave` are capability-routed (native one solve, or the derived fold, §2.3); and
   `materialize`'s already-streamed refusal is stated (§2.3). **Scenario-scoped readings are added as
   first-class surface**: `cautious_assuming`/`brave_assuming` on the agent and
   `snapshot_assuming(&Scenario)` for the scoped `Snapshot`, mirroring the unscoped surface the way
   `solve_assuming` mirrors `solve` (§2.4, §2.7) — so `ConsequenceRequest`'s scenario field is produced by
   the surface, not merely consumed by a backend.
