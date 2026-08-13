# themelios — v1 specification

2026-08-13. Draft for review, pre-implementation. This document is the
normative statement of what themelios v1 is, what it delivers, and what would
count as failing to deliver it. It is written to stand alone: a reader
holding only this repository and public sources can check every claim.

---

## 1. What themelios is

themelios (θεμέλιος, *foundation*) is the foundation of a mission-critical
Answer Set Programming ecosystem in Rust. It is a solver-agnostic,
library-first foundation that makes a family of systems — solver frontends,
formatters, test harnesses, explainers, probabilistic extensions, editor
tooling and language servers, REPLs, deployment services — natural extensions
or elegant compositions of its parts. It speaks the vocabulary of knowledge
representation and ASP as the literature speaks it, shares the concrete
syntax of clingo and clingcon, and is built to be certifiable into
environments where failure is expensive.

The ambition, stated so it can be checked: themelios is to become the gold
standard for usability for any system that has to **interact with, or
extend, an ASP solver** — leveraging Rust's strengths (invalid states
unrepresentable through the type system) and exceeding the expressiveness of
the existing APIs in this space dramatically. The named comparison set is
**clingo's Python API, clingo's C and C++ APIs, and python-clingox**.

### 1.1 The ecosystem thesis

The satellite systems this foundation exists to enable were chosen
deliberately, as the founding members of a composable, mission-critical ASP
ecosystem:

| satellite | what it needs from the foundation |
|---|---|
| A clingcon-class constraint extension, written in Rust | the custom propagator surface (§9.6) |
| clingo-dl-class and clingo-lp-class theory solvers | the same propagator surface |
| xclingo2-class explanation tooling | program transformation with provenance (§7.4, §7.5) |
| plingo-class probabilistic ASP | transformation, weak constraints, optimization, enumeration modes |
| A new ASP solver's frontend | the syntax tier, importable wholesale (§6) |
| A source formatter | the lossless syntax tree and owned comment-attachment policy (§6.3, §6.4) |
| A language server for ASP | error-resilient parsing, structured diagnostics, cheap total reparse (§6.5, §6.6) |
| A REPL for ASP | incremental program construction, multi-shot sessions, rendering (§9.4, §7.6) |
| Declarative ASP testing (elenctic-successor) | comments as data, reasoning-mode vocabulary, search-sufficiency reporting (§9.2) |
| pythia, distributed deployment and interaction | service-grade sessions: ownership, cancellation, stated thread posture (§9.4) |

None of these is a v1 deliverable. Each is a **fitness anchor**: it names a
capability the foundation must expose, and "natural extension" is checkable
only against named systems. Editor extensions generally — beyond the
language server itself — are a named consumer class: any editor tooling
should be assemblable from these libraries in-process.

### 1.2 The ecosystem constitution

Two rules bind every component of the ecosystem, themelios included:

- **Library first.** Every component is an embeddable library; command-line
  tools, servers, and services are thin derivations over it.
- **Embeddable in-process.** No global mutable state, no forced async
  runtime or process model, injectable I/O and observers rather than owned
  terminals, and an explicit thread-safety story per component. An editor
  extension host or a long-running service composes these libraries
  directly; nothing forces it to shell out.

### 1.3 Audience

Four audiences, layered rather than rivalrous:

- **Tool builders** — people building solvers, formatters, explainers,
  analysis and editor tooling on the foundation. Their surface is the
  parser, the syntax trees, the program representation, the transformation
  machinery, and the backend seam.
- **Application authors in Rust** — engineers embedding ASP to solve
  domain problems. Their surface is the program value, the macros, and the
  solve and query tiers.
- **clingo-world practitioners** — arriving from the Python/Lua/text
  ecosystem. The shared concrete syntax and recognizable concept names
  serve them; familiarity is a constraint on every visible surface.
- **Mission-critical operators** — teams who must certify, audit, and
  operate systems containing themelios. Their surface is the failure
  semantics, the trust architecture, the dependency tree, and the paper
  trail. Their needs are a bar that binds every tier, not a trade-off
  against any.

Where tool-builder and application-author needs collide, layering resolves
it: the lower tiers serve the tool builder, the upper tiers the application
author. A collision that survives layering is resolved as a named decision
in this document or its successors, never silently.

### 1.4 Vocabulary

The public vocabulary is the field's: rules, atoms, literals, terms, answer
sets, entailment, cautious and brave consequence, grounding, stable models —
knowledge representation and ASP as the literature speaks them. Mechanism
layers below the backend seam may speak engine vocabulary; it never leaks
above the seam. The checkable consequence: a reader fluent in the ASP
literature finds the public surface self-describing, and **any public name
not drawn from the literature owes a stated reason** where it is
introduced.

### 1.5 Model–view separation

Every result the foundation produces — parses, diagnostics, programs,
answer sets, solve outcomes, faults, reports — is a **typed model**. Views
for each consumer class (humans at terminals, editor protocols, machine
agents such as LLMs) are derivations over the model; controllers (CLIs,
servers, services) are thin shells over the library. No component's primary
output is prose, and no consumer must parse rendered prose to act on a
result.

---

## 2. What v1 delivers

When v1 is done, all of the following are true:

1. **The witness roster runs.** Every scenario in §3 exists as an
   executable, gate-run example.
2. **The syntax tier is a product.** Lossless, error-resilient,
   trivia-preserving parsing of the shared clingo/clingcon syntax,
   importable wholesale: a formatter-class, language-server-class, or
   contract-extraction consumer needs nothing outside the public surface.
   Parse → emit → reparse token-stream equivalence holds and is checkable
   natively.
3. **One grammar.** The macros are compile-time clients of the syntax
   tier. No second parser of the ASP language exists anywhere in the tree,
   and none is needed for any named consumer.
4. **The seam is real.** The core carries no engine types. The backend
   contract is specified, with a conformance suite; the clingo adapter,
   the clingcon adapter, and the test-only reference solver all pass it;
   and the solver pathologies named in §9.2 are unrepresentable in the
   contract's outcome types, not merely tested against.
5. **clingo and clingcon are one experience.** Barring backend
   configuration at session construction, a user's program construction,
   session driving, outcomes, and queries are identical across the two;
   theory results (constraint assignments) are typed model data in the
   same outcome vocabulary.
6. **The extension points exist at all three touch points.** Rust code can
   extend the pipeline at ground time (`@`-functions with typed,
   compile-time-checked signatures), at solve time (custom propagators,
   sufficient to build clingcon-class, clingo-dl-class, and
   clingo-lp-class systems), and at read time (typed extraction from
   answer sets).
7. **The trust floor is minimal and legible.** Unsafe code is confined to
   the engine adapters' mechanism layers; zero unsafe above them; the
   dependency tree is small, pinned, and argued, with every exception
   named and audited. With the engine-adapter features disabled, the
   entire stack is FFI-free.
8. **Failure is designed.** No panic escapes the public surface on any
   input — malformed, hostile, or absurdly deep; resource exhaustion and
   unbounded-depth values have specified behavior; every public operation
   documents its failure semantics.
9. **Diagnostics are a product, at rust-analyzer grade.** Every error,
   warning, and note from every tier is a structured, typed value —
   precise spans, labeled primary and secondary locations, a stable
   machine identity — with message quality held to the rust-analyzer bar,
   because a new solver fronted by this syntax tier ships these
   diagnostics as its own face. Rendering is derived, never primary: the
   same diagnostic value yields the human view, the editor-protocol
   payload, and a machine-consumable form.
10. **The comparators are visibly exceeded.** The witnesses ship beside
    comparator-anchored evidence: the same task in themelios and in
    clingo's Python API side by side, with the themelios rendering
    stricter (invalid states unrepresentable at compile time where the
    comparator discovers them at runtime or never), clearer, and
    diagnostic-superior.

## 3. The witness roster

The floor of v1, stated as executable scenarios. Each is an example the
gate runs, not merely compiles. Together they are the self-contained
definition of "expressive enough": v1 is not done while any witness is
missing, and a witness that is less clear or less safe than its
clingo-Python rendering of the same task is a failure (§4).

1. **First solve.** Construct a small program twice — once through macros,
   once through spelled-out constructors — verify the two are equal,
   ground, solve, and read the answer sets back as owned typed values.
2. **Enumeration.** Enumerate all answer sets of a program with a known
   count; the outcome states that enumeration was exhaustive.
3. **Optimization.** Solve to a proven optimum; the optimum is typed as
   proven, distinct from best-found; the improving trajectory is available
   when — and only when — the request asked for it.
4. **Multi-shot with externals.** Declare externals, ground parts
   incrementally, reassign externals across solves in one session, and
   observe outcomes change accordingly.
5. **Assumption scenarios with blame.** Solve under named assumption sets;
   on inconsistency, obtain which assumptions are responsible, as typed
   data.
6. **Consequences.** Compute cautious and brave consequences; the outcome
   names which semantics produced each set.
7. **Three-valued query.** Ask whether a ground atom holds: receive yes,
   no, or unknown, with unknown a genuine value; bindings for a
   non-ground pattern arrive partitioned by the same trichotomy.
8. **Typed extraction.** Map answer sets into user-defined Rust types via
   the derive-based extraction, including failure behavior on
   non-matching atoms.
9. **Transformation with provenance.** Apply a Program → Program rewrite;
   provenance on rewritten rules reaches back to their origins; a
   diagnostic raised on a transformed rule points at source.
10. **Round trip.** Render a constructed program to concrete syntax, parse
    it back, lower it, and obtain a program equal to the original (up to
    provenance).
11. **Comments as data.** Parse a program bearing comments; retrieve each
    comment and its attachment (trailing, leading, dangling) through the
    public API; emit preserves them byte-for-byte.
12. **Diagnostics quality.** Feed the parser a set of characteristic
    malformed programs; the rendered diagnostics match golden snapshots
    that a reviewer has accepted as rust-analyzer-grade.
13. **Ground-time extension.** Define an `@`-function as a plain Rust fn
    with the registration attribute; ground a program that calls it,
    including a multi-valued case and a failing case that surfaces as a
    typed ground-time fault.
14. **Solve-time extension.** Run the worked difference-logic propagator
    on a small scheduling program; the propagator is written against the
    public trait only.
15. **Theory uniformity.** Solve a constraint program under the clingcon
    backend; read constraint assignments as typed data; the
    session-driving and outcome-reading code differs from witness 1 only
    in backend configuration.
16. **Comparator evidence.** For witnesses 1, 3, 5, 8, and 13: the same
    task written against clingo's Python API, side by side, with the
    differences in safety, clarity, and diagnostics stated. The themelios
    side is the executed example; the comparator side is displayed
    source, not run by the gate.

## 4. What counts as failure

v1 has failed when any of the following holds:

- A witness from §3 cannot be expressed, or is missing from the executed
  examples.
- A witness comes out worse than its clingo-Python equivalent in clarity
  or safety.
- A satellite-class consumer needs a private API, a fork, or a second
  grammar to exist — the composition test fails.
- An engine type or engine-specific behavior leaks above the backend
  seam, or the conformance suite passes an adapter exhibiting a named
  pathology (§9.2).
- Unsafe code appears above the trust floor, or a dependency arrives
  unargued.
- A public name departs the KR/ASP literature without a stated reason.
- A component cannot be embedded in-process — global mutable state, a
  forced runtime, owned I/O — so editor extensions or services must shell
  out instead of composing.
- A diagnostic lacks a precise span or stable identity, or a consumer
  must parse rendered prose to act on any result — the model–view cut has
  failed.
- The diagnostics bar fails concretely: a solver fronting the syntax tier
  would need to wrap or replace its diagnostics before showing them to
  users.

---

## 5. Evidence and hard constraints

### 5.1 Public prior art in this family

Two public implementations by the same author inform this design and are
citable evidence; neither binds as precedent.

**elenctic** (github.com/GregoryGelfond/elenctic) — a declarative testing
framework for ASP in Python, whose contracts live in `%`-comments inside
the program under test, and whose verdict system treats "cannot decide" as
a value never collapsed into "no". What it teaches the foundation:
comments are data (§6.4); the reasoning-mode vocabulary — cautious, brave,
enumeration, optimality, three-valued query, search-sufficiency — must be
first-class (§9.2, §9.7); and fault attribution must be solved once, at
the seam, rather than re-litigated per tool (§9.3). Its changelog records
the solver ambushes that motivate §9.2's unrepresentability requirement:
enumeration under the engine's default optimization reporting the
improving sequence rather than the answer sets; truncated searches passing
as complete collections; contradictory termination flags from a cancelled
solve.

**kallos** (github.com/GregoryGelfond/kallos) — a pure-layout formatter
for clingo programs in Rust over a vendored tree-sitter grammar,
preserving comments by a side-table and re-injection mechanism because its
tree is not lossless. What it teaches: comment attachment is a bug class,
not a detail — its corpus NOTICE and source record a transposition class
(a leading comment emitted after another token's trailing comment),
block-aware blank-line detach rules, and a dual-role-token carve-out, all
of which seed themelios's attachment-policy tests (§6.4); its
three-instrument correctness architecture — a lexical fusion oracle, a
structural re-parse self-check, an external differential authority — is
adopted foundation-wide (§6.7, §10.1); and its documented grammar corners
(pool arguments surfacing as repeated fields, greedy theory-operator
munch, upstream grammar gaps) seed the parser's corner corpus.

Both projects converge, independently, on practices this specification
adopts as standing: zero TODO-markers, with deferred work named as
*reserved seams* beside a refusal; argued comments dense enough to be
audited; totality as the default; differential oracles wherever an
external authority exists.

### 5.2 Hard constraints, designed in from birth

Each of the following is a constraint with its argument carried in place;
where it rests on an engine behavior, that behavior is publicly
reproducible against the named engine.

- **Concurrent interning is unsafe in libclingo despite its header's
  thread-safety claim** (reproducible: concurrent symbol creation from
  enough threads crashes deterministically). The compensation — a single,
  owned interning discipline — lives in exactly one place, in the clingo
  adapter (§9.5), never distributed across consumers.
- **Ground values can be arbitrarily deep** — tens of thousands of
  nesting levels arise in practice from recursive constructions — so any
  recursive walk over user-reachable structure is a latent, uncatchable
  crash. Every walk in themelios — construction, traversal, rendering,
  drop — is work-list based from birth, and the depth gate (§10.1) proves
  it per walk. Designing this in is cheap; retrofitting it means
  converting every walk after the fact.
- **A missing loop terminator in a work-list is a memory bomb that
  compiles clean.** Work-list code trades stack exhaustion (loud) for
  unbounded heap growth (silent) when a pop is missing. Unused-code and
  unused-result warnings are denied workspace-wide, and the full gate
  runs memory-capped where the platform allows (§10.2).
- **Refusal beats repair.** Silent truncation or normalization converts a
  detectable mistake into an undetectable wrong answer; refusal converts
  it into a question the caller can fix. Constructors and conversions
  refuse rather than repair (§7.3).
- **The language tier starves when built last.** Every consumer of parsing
  — macros, formatter-class tools, language servers, contract extraction —
  multiplies the cost of deferring it, and a bespoke stopgap parser
  becomes a second grammar that never dies. The build order (§11) puts
  the syntax tier first, and the one-grammar rule (§2, item 3) forbids
  the stopgap.
- **Documentation lies unless something holds it.** Claims a repository
  makes about its own state are held to an executable-claims standard: a
  claim a test can hold, a test holds (§10.4). Nothing is promised in v1
  documentation that v1 does not gate.

---

## 6. The syntax tier

### 6.1 Grammar of record

themelios carries its own normative grammar document, in this repository,
versioned with the code: the shared clingo/clingcon concrete syntax,
stated once, auditable by a reader. Its reference roster: the clingo and
clingcon implementations themselves (both open source, MIT) as reference
implementations; the tree-sitter-clingo grammar as a secondary
cross-check; the corpus (§10.3) as reachability evidence. Where
references disagree, **clingo's observed behavior is the authority**,
settled by differential test and recorded in the grammar document. Theory
atoms parse grammar-generically at this tier — `&name { … }` with guards —
with admission against `#theory` definitions a concern of tiers above. The
grammar document also specifies the **macro dialect**: the interpolation
forms by which macros splice Rust values into program syntax (§8).

### 6.2 Lexer

Hand-written and total: every byte sequence lexes to a token stream,
unknown input becoming error tokens, nothing dropped. Beside it lives the
**fusion oracle** — the lexical spacing theory answering "may this
adjacency lose its whitespace" — reachable-honest, defaulting to keep,
with the greedy theory-operator munch and rule-neck abutments among its
first pinned cases.

### 6.3 The lossless tree

rowan green/red trees, full fidelity: every byte of the input is in the
tree, trivia included, and `tree.text() == input` holds **unconditionally**
— on valid programs, on garbage, on partial edits. Author whitespace is
data in the tree; a formatter *chooses* to replace it. A typed AST layer
sits over the tree in the rust-analyzer idiom: cheap node wrappers, no
second tree.

### 6.4 Trivia and comment attachment as owned policy

The attachment semantics — trailing beats leading beats dangling,
block-aware blank-line detach, the dual-role-token carve-out class — is
specified, tested, and **exposed as API** by this tier, once, for every
consumer: formatter, language server, and comment-borne contract
extraction all query the same attachment rather than re-deriving it.
kallos's documented scars (§5.1) are the opening test corpus. Doc comments
are first-class syntax, not trivia.

### 6.5 Parser

Hand-written, error-resilient recursive descent. Never fails, never
panics: every input yields a tree and diagnostics. Recovery strategies are
documented per construct family, and error/missing nodes carry enough
structure that consumers degrade gracefully — a language server formats
the well-formed statements around a broken one; a solver frontend reports
precisely.

### 6.6 Diagnostics

The `-base` diagnostics model instantiated to the rust-analyzer bar:
stable namespaced identities, primary and secondary labeled spans,
expected-set reporting at recovery points. The bar is *reviewable*:
rendered diagnostic snapshots live in the test suite as golden cases, so
message quality is a diffable artifact (witness 12).

### 6.7 The ≈ apparatus

Structural token-stream equivalence plus comment-sequence comparison,
native to the tier: any consumer claiming a layout-only or
meaning-preserving transformation gets its certificate and its witness
here.

### 6.8 Incrementality preconditions

Parse is a pure function of source text; total reparse is the supported
cheap path; node identity is positional now, and the durable-identity
attachment point is named for later (syntax-pointer paths at the typed-AST
layer) — designed for, not built. The machinery argument is in §7.8.

---

## 7. The program tier and the pipeline

### 7.1 The Program value

The logician's representation, in KR vocabulary: a `Program` is a set of
`Rule`s and directives, organized in parts for multi-shot use. Rules have
heads — atoms, disjunctions, choices — and bodies — literals under default
and classical negation, aggregates (count, sum, min, max), comparisons,
conditional literals. Terms are the full algebra: symbols, variables,
functions, arithmetic, intervals, pools, tuples. Optimization appears as
weak constraints and minimize/maximize with weights and priorities.
`#show`, `#external`, `#const`, and theory atoms are first-class. Set-like
where the logic says set, ordered where meaning demands order.

### 7.2 Totality and depth discipline

All values are owned and total. No walk over user-reachable structure
recurses — construction, traversal, rendering, and drop are work-list
based from birth (§5.2), and the depth gate proves it per walk.

### 7.3 Smart constructors

The spelled-out construction API: total functions with typed refusals,
never silent truncation or repair (§5.2). Lexical name classes are
enforced at construction. Everything the macros produce goes through
these same doors — one well-formedness authority.

### 7.4 Provenance as model data

Every Program node can carry origin — a span into the tree it was parsed
from, the construction site, the transformation that produced it —
composably. This is the durable-identity attachment point at this tier;
it is what lets a program-level diagnostic point at source precisely; and
it is the ground explanation-class tooling stands on: blame runs from
answer sets back through rules to source *because provenance is data*,
with explanations derived as views.

### 7.5 Transformation

Program → Program as first-class pure functions, with visitor and
rewriter utilities and provenance carried through — the capability
transformation-class satellites need, shipped as machinery; their
specific transforms stay theirs.

### 7.6 Rendering

Program → concrete syntax, canonical and deterministic, round-trippable:
render → parse → lower is identity up to provenance (witness 10). Styled
layout is the formatter satellite's art; the foundation renders correctly
and legibly, and no more.

### 7.7 Patterns and unification

The pattern language over the term algebra and unification of patterns
against ground symbols live here, backing the query surface (§9.7).

### 7.8 Batch-first, incremental-ready

The pipeline is batch: pure, staged derivations (text → tree → program →
outcomes), each a pure function of explicit inputs, each callable
piecemeal on demand. Incremental computation *machinery* (memoization,
dependency tracking) is a named reserved seam, deliberately not built in
v1: ASP's keystroke-driven computations (parse, vocabulary analysis) are
small at ASP file scale, and its expensive computations (grounding,
solving) are demand-driven, not keystroke-driven — so v1 buys
incrementality's *preconditions* and not its machinery. The preconditions
are binding: purity of every derivation, error resilience of every stage,
demand-callable tiers, cheap total reparse, and named identity-attachment
points (§6.8, §7.4). A batch pipeline of pure stages is the shape query
systems have historically been retrofitted onto successfully.

---

## 8. Macros

A tiered vocabulary, designed level by level, each macro admitted on
argument. The floor set: `atom!`, `fact!`, `rule!`, `constraint!`,
`minimize!`, `maximize!`, `show!`, `external!`, `scenario!`,
`#[derive(Extract)]`, `#[external]`. The levels: program-level,
statement-level, element-level, query-level, extraction and registration
attributes.

Three laws govern every macro:

1. **One grammar.** A macro that ingests ASP syntax hands its token
   stream to the real parser at compile time. A macro-site syntax error
   is the same rust-analyzer-grade diagnostic the file parser gives,
   mapped onto the macro's spans.
2. **No second representation.** Macros expand to public
   smart-constructor and registration calls only; everything a macro can
   do is expressible spelled-out.
3. **Specified interpolation.** Splicing Rust values into program syntax
   is part of the grammar document's macro dialect, not an ad-hoc parser
   behavior.

The macro crate exists early; its vocabulary accretes tier by tier, each
macro landing as the natural follow-on of the tier it clients:
construction macros once syntax and constructors exist,
`#[derive(Extract)]` with the extraction machinery, `#[external]` with
the `@`-function surface. The syntax tier is the enabler throughout: any
macro that parses an ASP element does so through the one grammar, safely.

---

## 9. The backend seam and the solve tier

### 9.1 The contract and capability declaration

A backend is an implementation of a specified contract. It declares its
capabilities — enumeration, optimization, native consequences, theory
support and which theories, externals, `@`-function evaluation, custom
propagators, multi-shot, assumptions, cancellation. Requests beyond
declared capability receive a **typed refusal**, never a silent degrade.
The core may refuse-or-derive deliberately (deriving cautious
consequences by intersection when an engine lacks them natively) and says
so in the outcome's provenance.

### 9.2 The outcome vocabulary

- `Determination`: the closed trichotomy — consistent, inconsistent,
  inconclusive — where inconclusive is a real value, never collapsed.
- `Conclusion`: how the search ended — exhausted, target reached, budget,
  interrupted — so every consumer judges search-sufficiency for *its*
  reading.
- Answer sets as owned, streamable values; proven optima typed distinct
  from best-found; cautious and brave consequences as typed sets with
  their semantics named.
- Theory results — constraint assignments — as typed model data in the
  same vocabulary.
- Request types distinguish "enumerate answer sets" from "optimize,
  reporting the improving trajectory," so the three named pathologies —
  enumeration reporting an optimization's improving sequence, truncated
  search passing as a complete collection, contradictory termination
  flags — are **unconstructible in the vocabulary** (evidence for all
  three: §5.1).
- Assumption blame: when a scenario is inconsistent, which assumptions
  are responsible is an answerable, typed question (witness 5).

### 9.3 Fault attribution

A closed locus taxonomy at the seam — program, request, resource, engine,
adapter — with "is this a backend bug" a closed bit. Faults are values
with provenance. Solved once, here, for every consumer.

### 9.4 Sessions

Multi-shot sessions as owned values under the capability discipline:
build and ground parts incrementally, assign externals, solve under
assumption scenarios, interrupt, re-solve. The session handle is the
authority to drive the engine; dropping it is revocation; there is no
ambient engine and no global state. Thread posture is explicit per
backend; cancellation-from-another-thread is a declared capability.
Service-grade by construction: a session is embeddable behind a service
boundary or an editor host without ceremony.

### 9.5 The clingo and clingcon adapters

The clingo adapter is the trusted computing base (TCB) under the
microkernel criteria (§12.3): FFI calls enumerated against a manifest;
each privileged operation carries stated pre- and postconditions; the
interning-concurrency compensation (§5.2) implemented once, behind the
capability story; engine defaults (optimization, enumeration modes)
explicitly configured per request shape so the adapter cannot transmit an
ambush upward.

The clingcon adapter is a thin delta over the same machinery:
registration onto a clingo-backed session plus typed assignment
retrieval. **Contingency:** if libclingcon's surface proves substantially
larger than its registration-shaped appearance, the clingcon adapter
descopes from v1 first and resumes immediately after; the uniformity
clause (§2, item 5) then binds at the design level, demonstrated through
the propagator surface instead.

### 9.6 The extension surfaces

- **Ground time — `@`-functions.** Named Rust functions or a context
  value register on a session; `@name(args)` calls into Rust through a
  panic-containing trampoline; arguments and results cross as typed
  symbols via the conversion traits; multi-valued returns are supported;
  a failing `@`-function is a typed ground-time fault with a locus. The
  `#[external]` attribute macro derives registration from a plain Rust
  fn — compile-time-checked signatures where the Python comparator has
  duck typing (witness 13).
- **Solve time — custom propagators.** A safe Rust trait for theory
  propagation — init, propagate, undo, check — with watch management and
  typed theory-atom access, registered onto a session. Panic containment
  and callback-scoped lifetimes are the adapter's obligation; v1 pins
  propagation to a single solver thread, with multi-threaded propagation
  a named reserved seam. This surface is sufficient to build
  clingcon-class, clingo-dl-class, and clingo-lp-class systems in Rust;
  the worked witness is a small difference-logic propagator (witness 14).
- **Read time — typed extraction.** `#[derive(Extract)]`-class mapping
  from answer sets to Rust values via the conversion traits (witness 8).

The ground-program observer surface is a named reserved seam — no v1
anchor forces it, and gold-plating is how trusted computing bases stop
being minimal.

### 9.7 The query surface

Three-valued `Answer` — yes, no, unknown, with unknown a genuine value —
over solve outcomes; bindings partitioned by the trichotomy; cautious and
brave consequences with their semantics named. Patterns and unification
come from the program tier; the epistemic reading lives here.

### 9.8 The conformance suite

Executable, shipped with the contract, run by every adapter: outcome
correctness on a corpus of small programs with independently known
answer sets; capability honesty (declared-unsupported must refuse); the
named pathologies attempted and structurally impossible; fault loci
landing where they belong. The reference solver is the second
implementor and the independent oracle for small cases; the clingo
differential covers the large corpus.

---

## 10. Assurance

### 10.1 Instruments, by tier

**Syntax:** a committed fuzz crate from the first weeks — arbitrary bytes
never panic, `tree.text() == input` under fuzz, parse terminates; corpus
round-trips (§10.3); differential parse against clingo's canonical AST
(feature-gated harness; clingo the authority); property tests on lexer
totality and the fusion oracle in both directions; golden diagnostic
snapshots; the attachment policy seeded with kallos's scar corpus (§5.1).

**Program:** algebraic property laws (set semantics; render → parse →
lower identity); the subprocess depth gate proving every walk
stack-independent; mutation testing over constructor and transformation
logic.

**Seam and adapters:** the conformance suite on every adapter;
reference-versus-clingo differential over the small-program corpus; leak
checking and race checking harnesses at the TCB; a spike suite pinning
the engines' observed behaviors as a living regression guard against
upstream drift; crash-capture apparatus installed early, not after the
first unexplained crash.

**Cross-cutting:** mutation testing as a standing per-milestone audit; a
coverage floor enforced as the same number on every machine; unused-code
warnings denied workspace-wide with the motivating argument cited beside
the lint (§5.2); executed examples in the gate; documentation examples
that run; pinned toolchain; Linux and macOS CI; memory-capped full-gate
runs where the platform allows.

### 10.2 In-gate versus out-of-band

The gate — run on every change — is: format check, clippy as errors, the
test suite, the trust-boundary checks, executed examples, documentation
build. Out-of-band, on stated cadences: fuzzing continuously with its
corpus committed; mutation and the clingo differential per milestone;
leak and race checks per release and in a dedicated CI job. Every
instrument is documented with what it proves *and what it cannot*.

### 10.3 Corpus sources

Textbook ASP encodings, the formatter-inherited corpus inputs (inputs
only, with provenance), clingo's and clingcon's own test suites (MIT),
and further licensed sources recorded with provenance as they are added.
The corpus serves the parser now and solver conformance later.

### 10.4 Documentation posture

rustdoc plus executed examples are the v1 documentation; claims about the
project's own state are held to the executable-claims standard (§5.2).
Nothing is promised in documentation that v1 does not gate.

---

## 11. Build order

Language-first. Each stage lands with its assurance instruments green —
an instrument-less stage is not done.

1. `themelios-base` — source model, spans, the diagnostics model.
2. `themelios-syntax` — lexer + fusion oracle; then parser + lossless
   tree + attachment policy; then typed AST + the ≈ apparatus. Fuzzing
   starts in week one. The grammar document lands here.
3. `themelios-program` — the Program value, lowering, constructors,
   provenance; then rendering; then transformation, patterns,
   unification.
4. `themelios-macros` — the crate exists from the first stage it can
   client; its vocabulary accretes with its enablers: construction
   macros after stages 2–3, extraction and registration attributes with
   stages 6–7.
5. `themelios-solve` — the contract, outcome vocabulary, fault taxonomy,
   conformance-suite skeleton.
6. `themelios-reference` — the naive solver; first to pass conformance.
7. `themelios-clingo-sys` / `themelios-clingo` — the TCB under the
   manifest discipline; conformance and differential green; then the
   extension surfaces (`@`-functions, propagators); then
   `themelios-clingcon-sys` / `themelios-clingcon` as the thin delta.
8. `themelios` facade — the witness roster (§3) complete, including
   comparator evidence; query tier completed against real outcomes.

---

## 12. Architecture reference

### 12.1 Tiers

Four tiers over a shared base, each a separately usable library: syntax
(§6), program (§7), solve (§9), adapters (§9.5). The pipeline shape and
its incrementality posture are §7.8.

### 12.2 Crates

Eleven workspace members. Satellites live in their own repositories.

| crate | unsafe | purpose |
|---|---|---|
| `themelios-base` | forbid | Source-text model, spans, line indexing, and the diagnostics model. Zero dependencies. |
| `themelios-syntax` | forbid | Lexer, parser, lossless tree, trivia policy, typed AST, token-fidelity emit, ≈ self-check, fusion oracle. FFI-free by dependency closure. Importable wholesale. |
| `themelios-program` | forbid | The Program value, lowering, constructors, provenance, transformation, rendering, patterns and unification. |
| `themelios-macros` | forbid | Procedural macros as syntax-tier clients, expanding to public constructors and registration APIs only. |
| `themelios-solve` | forbid | The backend contract, outcome vocabulary, sessions, fault taxonomy, query surface, conformance suite. Engine-free. |
| `themelios-clingo-sys` | allow (bindings only) | Vendored, pinned bindgen output over clingo's C API; regeneration out-of-band. |
| `themelios-clingo` | allow (the TCB) | Mechanism-only kernel over the bindings plus the safe adapter implementing the contract. |
| `themelios-clingcon-sys` | allow (bindings only) | Vendored, pinned bindings to libclingcon's registration surface. |
| `themelios-clingcon` | allow (thin TCB delta) | Registration onto a clingo-backed session plus typed assignment retrieval. |
| `themelios-reference` | forbid, `publish = false` | The naive pure-Rust reference solver: oracle, second implementor, native-backend demonstration. |
| `themelios` | forbid | The facade: curated re-exports, adapters behind default features — disable them and the stack is FFI-free — and the witness examples, executed by the gate. |

Typed AST placement is deliberate: it lives in `themelios-syntax`
(syntactic accessors over the tree, no semantic opinions), serving
solver-frontend consumers that want structure without the Program tier.

### 12.3 Trust architecture

Three redundant enforcement layers: workspace-level `unsafe_code =
"deny"` with only the four adapter crates opting back in; per-crate
`forbid`/`allow` attributes; and a structural check asserting
forbid-in-pure-crates, allow-only-in-the-named-TCB, FFI-free dependency
closures for `-base`/`-syntax`/`-program`/`-solve`, and no build scripts
outside the sys crates.

The design criteria for the trust architecture and for tier boundaries
generally are the microkernel canon, translated to library terms:

1. **Mechanism below, policy above.** The TCB contains only what must
   touch the unsafe floor; anything expressible above it is above it.
2. **The privileged interface is enumerated and closed.** Every FFI call
   the TCB makes is admitted against a per-area manifest — a
   characterization says which calls are *admissible*, a manifest says
   which were *admitted* — so the audit surface is a finite, named list,
   checked mechanically.
3. **Authority is explicit, never ambient.** Engine access exists only
   through owned handles the adapter issues; Rust ownership is the
   capability substrate; dropping the handle is revocation.
4. **One door per boundary.** The backend contract is the sole crossing
   between core and engine; tier-to-tier seams are narrow, typed, and
   enumerable; no tier reaches around another.
5. **Faults are contained and attributed.** An engine fault becomes a
   typed outcome with a locus; it cannot corrupt core invariants or
   poison shared state.
6. **Verifiability sizes the TCB.** Small enough to audit exhaustively;
   each privileged operation carries stated pre- and postconditions;
   formal-methods tooling over the TCB is a named reserved seam.

### 12.4 Dependency policy

**Import only what is truly necessary** is standing practice: every
dependency carries an argued necessity where it is declared;
hand-writing is the default where hand-writing is reasonable; the burden
of proof sits on the import. Concretely at v1: `-base` has zero
dependencies; `-syntax` carries **rowan** as the one named exception —
pinned, with a written audit note (internal unsafe acknowledged;
exercised at scale daily by rust-analyzer) and hand-rolled green/red
trees recorded as the reserved fallback — plus at most a small hash-map
utility; the lexer and parser are hand-written with no lexer or parser
dependencies; `-macros` carries the proc-macro toolchain only; `-solve`
nothing beyond the lower tiers; the sys crates' bindgen is feature-gated
and never in a default build.

---

## 13. Reserved seams and non-goals

Named reserved seams (deferred with reasons, not gaps): incremental
computation machinery (§7.8); multi-threaded propagation (§9.6); the
ground-program observer surface (§9.6); formal-methods tooling over the
TCB (§12.3); hand-rolled green/red trees as the rowan fallback (§12.4);
additional engine backends beyond clingo and clingcon.

Non-goals for v1: the satellites themselves (anchors, not deliverables);
styled formatting; a language server; a REPL; publishing to crates.io
before the surface stabilizes.

## 14. Repository facts

`~/Projects/themelios`, private remote, MIT licensed, copyright Gregory
Gelfond. This specification lives at `docs/specification.md` and is the
repository's founding artifact; the grammar document (§6.1) joins it at
stage 2.
