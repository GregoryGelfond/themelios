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

// A world view: an owned, non-empty-by-construction collection of answer sets (§2.3).
pub struct WorldView { /* private; see §2.3 for the invariant */ }

// Cautious (⋂) / brave (⋃) consequences — the solve tier's typed sets, re-exported for the
// reading side (solve.md §5.2), each carrying the mode that produced it.
pub use themelios_solve::Consequences;

// Bindings of an open pattern, partitioned by the trichotomy (§2.5).
#[non_exhaustive]
pub struct Bindings { /* yes / no / brave-unknown partitions */ }

// The central input to `answer`/`entails`: a ground query — an atom, a literal, or a conjunction.
// Construction REFUSES a non-denoting term (arithmetic-with-variable, interval, pool) — the refusal
// §3.1 describes — so `answer`/`entails` are infallible: a `Query` that EXISTS denotes.
pub struct Query { /* atom | literal | conjunction — a closed set of denoting shapes */ }
impl Query {
    pub fn of(atom: Atom) -> Result<Self, NotAQuery>;             // an atom/literal; refuses non-denoting
    pub fn all(parts: impl IntoIterator<Item = Query>) -> Query;  // conjunction (evaluated per-model, §2.2)
}

// A pattern is a signed `Atom` (program.md §11.2); the query tier reuses it directly — no new type.
pub use themelios_program::Atom;
```

The primitives (`WorldView::members`, `cautious`, `brave`, and the program tier's matching) sit under
the derived readings (`answer`, `bindings`, `entails`); §1.1 is why both are public.

### 2.2 Three-valued `Answer` — the one authoritative definition

The core question is Gelfond–Kahl's: *is this true, given the program.* Its answer is
`Answer::{Yes, No, Unknown}`, with **`Unknown` a genuine value, never collapsed into `No`.** There is
**one** definition of `Answer`, stated here once; the atomic case, the conjunction case, and the
matching relationship (§3.1) are all *derived* from it, so that "what `No` means" cannot drift across
the document.

Let a **world view** `W` be a non-empty set of answer sets (§2.3), and let `q` be a ground query — an
atom, a literal, or a conjunction of them. The **contrary** of a ground atom `a` is its strong
negation `-a` (and the contrary of `-a` is `a`). Then, evaluating `q` **within each member** of `W`
and quantifying over the members:

- **`Answer::Yes`** iff `q` is **cautiously entailed** by `W` — every member satisfies `q` (for a
  conjunction, every member contains every conjunct).
- **`Answer::No`** iff the **contrary of `q` is cautiously entailed** — every member *strongly
  refutes* `q` (for a conjunction, every member contains the contrary of at least one conjunct).
- **`Answer::Unknown`** otherwise.

The load-bearing subtlety is the boundary between `No` and `Unknown`, and it is where a reader from
SQL or Prolog goes wrong first: **absence is not falsity.** A conjunct merely *missing* from a member
is not the same as its contrary being *present*. Worked, on the world view `{ {a}, {b} }` and the
query `a ∧ b`:

- Member `{a}` contains `a`; `b` is *absent*, but `-b` is **not present** — so `{a}` does not contain
  the contrary of any conjunct, and does not strongly refute `a ∧ b`.
- Therefore not every member strongly refutes `a ∧ b` → the answer is **`Unknown`**, not `No`.

Contrast the world view `{ {sunny, warm, -swim}, {swim, -warm} }` and the query `warm ∧ swim`: the
first member contains `-swim` (a conjunct's contrary), the second contains `-warm` — every member
strongly refutes the conjunction, so the answer is a genuine **`No`**. The distinction between these
two cases is the whole point of the third value; the closed-world assumption is something a program
states for itself (`-p(X) :- not p(X).`), and ASP does not impose it, which is exactly why the answer
has three values.

**The insight this preserves.** A conjunction is evaluated *within* each member — the model quantifier
scopes the whole query — **never over ⋂ or ⋃**, because `brave(a) ∧ brave(b)` does not give
`brave(a ∧ b)` and `cautious` cannot express a per-model refutation. That is correct and is the
reason `Answer` is not a projection of the consequence sets.

**Signature and cost.**

```rust
impl WorldView {
    /// The Gelfond–Kahl three-valued reading of a ground query.
    pub fn answer(&self, q: &Query) -> Answer;
}
```

- An **atomic or literal** query is two cautious-membership checks (`q` entailed → `Yes`; contrary
  entailed → `No`; else `Unknown`). Through the native cautious door (§2.4) this is **one solve, no
  enumeration** — cheap.
- A **conjunction** is model-scoped. `Yes` is still a cautious check (all conjuncts in `⋂`), cheap;
  but `No` (every member strongly refutes) and the `Unknown` boundary are *not* expressible over
  `⋂`/`⋃`, so they cost either an enumeration of `W` (linear in `|W|`, which can be exponential) or a
  solver-side satisfiability check that no member is refutation-free. This asymmetry — atomic answers
  cheap, conjunctive answers potentially enumeration-bound — is stated because it is exactly the cost
  surprise a prose contract would hide. The refutation-free satisfiability optimization is a named
  seam, not a v1 promise.

**Laws** (checked as properties, §4): a query and its contrary are never both `Yes`; `answer` is
never `No` on a query nothing refutes (the absence-is-not-falsity law); on a singleton world view
`answer` agrees with membership-and-contrary in the one model.

### 2.3 `WorldView` — non-empty by construction

A **`WorldView`** is a program's *world view* in the epistemic-specifications sense; for a plain
program (this tier's scope) that is the program's **unique** world view, which is exactly its set of
answer sets.

The invariant is a **type state**, and it is the F8 correctness point: **a `WorldView` value is
non-empty by construction.** It is obtained only from a `Consistent` outcome, so a `WorldView` you
*hold* has at least one member; the empty world view of an *inconsistent* program is **not a
representable `WorldView`** — inconsistency is carried by `Determination::Inconsistent` (`solve.md`
§5.1), never by an empty world view. ("The world view of an inconsistent program is empty" is a
true mathematical statement about the *notion*; it is deliberately not a constructible *value*, because
a value meaning "invalid" inside the space of valid world views would be a sentinel — the pathology
`solve.md` §5.3 forbids.)

```rust
impl WorldView {
    // --- primitives (the exposed surface of §1.1) ---
    pub fn cautious(&self) -> Consequences;                       // ⋂ — see §2.4
    pub fn brave(&self) -> Consequences;                          // ⋃ — see §2.4
    pub fn members(&self) -> impl Iterator<Item = Result<AnswerSet, Fault>> + '_;
    pub fn is_exhausted(&self) -> bool;                           // the search closed the space
    pub fn scenario(&self) -> &Scenario;                         // what it ranged over

    // --- derived readings ---
    pub fn answer(&self, q: &Query) -> Answer;                    // §2.2
    pub fn bindings(&self, pat: &Atom) -> Result<Bindings, NotAPattern>; // §2.5 (a pattern is a signed Atom)
    pub fn entails(&self, q: &Query) -> bool;                     // §2.6 (ASP-Core-2 cautious)
}
```

Properties and cost:

- **Owned, exhaustion-gated, scenario-scoped.** A universal reading (all members, all optimal, a
  cautious consequence) is answerable only from a search that closed the space; `is_exhausted` gates
  it, and a truncated search cannot answer a universal (the accessor refuses, §3.2).
- **Lazy where possible.** Because a world view can have very high cardinality (exponentially many
  answer sets), `cautious`/`brave` go through the **native door** (§2.4) — one solve, *no*
  enumeration — and `members` **streams** (each item a `Result`, so a mid-stream engine fault surfaces
  at the item, not as a clean end). Materializing the whole member set is **opt-in, never forced**;
  genuine lazy/incremental *machinery* beyond streaming is the specification's §7.8 reserved seam.
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
impl WorldView {
    pub fn cautious(&self) -> Consequences;   // ⋂
    pub fn brave(&self)    -> Consequences;   // ⋃
}
```

**Two doors, and the free differential.** The **native door** has the solver compute `⋂`/`⋃`
directly (one solve, no enumeration); the **derived door** folds an enumerated world view. The two
**must agree**, and their agreement is a standing differential the tier gets for free — the solver
solves through a foreign engine, and an independent check on consequence computation is otherwise hard
to come by. Cost: native is one solve; derived is `Θ(|W|)` in members folded, and is why the native
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

**The `unknown` listing is the brave domain, and says so.** `yes` and `no` are read off the cautious
consequences — finite, exact. `unknown` is *everything the program does not settle*, and this tier
holds answer sets rather than the program that produced them, so it cannot enumerate that domain; a
listing therefore shows the **brave** domain (the open instances *some* answer set mentions) and
closes with the sentence that says so — or it reads as exhaustive and teaches the very misreading it
exists to prevent. Cost: `yes`/`no` are cautious-set reads; `unknown` is bounded by the brave domain.

### 2.6 The ASP-Core-2 cautious query — a dialect-scoped derivation

The Gelfond–Kahl `Answer` (§2.2) is the tier's default. The **ASP-Core-2 standard** defines its own
query answering (`atom "?"`, grammar §6.1), and its semantics is **cautious** and **two-valued**
(entailed / not-entailed). This is specification **witness 20** (`asp-core-2`), a v1 obligation of
this surface — and it is a *different* question from the three-valued default, so it is exposed as its
**own** operation, not a rename of `Answer`:

```rust
impl WorldView {
    /// The ASP-Core-2 standard's query answer: cautious, two-valued (witness 20, grammar §6.1).
    pub fn entails(&self, q: &Query) -> bool;
}
```

The exact relation to `Answer`, stated because it is non-trivial: the standard's *entailed* is
`Answer::Yes` (cautiously entailed); the standard's *not-entailed* spans **both** `Answer::No` **and**
`Answer::Unknown`. So the ASP-Core-2 query is the projection

> `entails(q) == (answer(q) == Answer::Yes)` — i.e. `Yes` vs `(No ∪ Unknown)`,

never a two-way collapse that would send `Unknown` to the wrong side (a mis-lowering the specification
§4 counts as failure). It is a **dialect-scoped** operation: `entails` is the ASP-Core-2 dialect's
reading; where a dialect's query semantics diverges from the clingo-world default beyond this
projection, that is a per-dialect choice, named rather than silently unified (§1.1's
default-and-expose). Cost: one cautious check — a single solve through the native door.

**The non-ground query is answered by substitution.** The ASP-Core-2 query admits variables
(`q(X)?`, grammar §6.1), and the standard answers a non-ground query by *substitution* — the set of
cautiously entailed instances, not a boolean. That answer is **`bindings(pat).yes()`** (§2.5): the
cautiously-entailed instances of the pattern *are* the standard's cautious substitution answer, under
the same `Yes` vs `(No ∪ Unknown)` projection `entails` draws for the ground case (an instance is in
`yes()` iff its `answer` is `Yes`, never the three-valued partition mistaken for the two-valued cautious
answer). So witness 20 is served for both shapes — `entails` for a ground query, `bindings(pat).yes()`
for a non-ground one — and the query surface is silent on neither.

### 2.7 The dual face

The query surface carries the two centerpiece faces of `solve.md` §3 through the reading side: a
declarative macro form (a `query!` / `ask!`-style spelling of the goal, through the one grammar,
expanding by the macro law to the same programmatic calls) and the composable programmatic form
(`answer`, `bindings`, `cautious`, `brave`, `entails`, `world_view`). A query's goal is authored the
same way a program's atoms are; a run-time patient name, a generated goal, or an LLM's question enter
through the programmatic form.

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

The mgu's three-outcome result is a **matching** result, and the F7 correction is to keep it distinct
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
  domain and says so.
- **The accessors gate on exhaustion** (§2.3) — a universal claim is answerable only from a closed
  search and a stream that has not already drained.

`themelios-query` computes these facts once, here, for every consumer, and never re-litigates the
model quantifier per tool.

---

## 4. Acceptance, assurance, reserved seams

**elenctic is the acceptance test.** The elenctic-successor (a declarative ASP testing framework, to
be rewritten in Rust on themelios) is the query tier's named arm's-length consumer; its `SolveResult`
is, structurally, the solve tier's `Determination`, and its verdict system already treats
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
   ASP-Core-2 *non-ground* query answered by substitution — `bindings(pat).yes()` as the standard's
   cautious substitution answer (§2.6, witness 20). The native consequence door's obligation to range
   over the optimal set under an objective stated (§2.4).
