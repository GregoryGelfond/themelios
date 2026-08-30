# themelios-analysis — tier design

2026-08-24. Design, pre-implementation. This document is the API
design of `themelios-analysis` — the types, traits, signatures, semantics, and
computational costs of the crate that reads a `Program` and reports its
structural facts — derived from the v1 specification (`docs/specification.md`,
cited as *spec §n*), the program tier design (`docs/design/program.md`,
*program §n*), the base tier design (`docs/design/base.md`, *base §n*), and the
grammar of record (`docs/grammar.md`, *grammar §n*); a bare *§n* cites this
document's own sections. It is written to stand alone in the sense the
specification is: a reader holding this repository and public sources can check
every claim. Where this document and the specification disagree, the
specification governs and the disagreement is a defect here.

---

## 1. What themelios-analysis is

`themelios-analysis` reads a `Program` (program §4) and reports what is
*structurally true* of it: which constructs it uses, how its predicates depend on
one another, whether its statements are safe, whether it will ground finitely, and
which classes of the literature it falls in — tight, stratified,
head-cycle-free, normal, Horn, disjunctive, choice. It is the syntactic
**analysis** a solver's algorithm selection, a formatter's lints, and a program
optimizer's pass conditions all read (spec §1.1). It is a distinct crate built
**alongside the program tier, in the same stage**, because it reads the program
value and *nothing else*: no answer sets, no grounder, no engine. That adjacency
is the mirror of the query client's to the solve tier (program §11.4).

**Facts, not policy — the governing separation.** This crate states what is
structurally true; it never decides what to *do* about it. Which class earns
which solving algorithm, which size estimate trips which threshold, whether a
warning is worth emitting — all of that is the *consuming* system's, a policy
over these facts. No threshold and no routing table lives here. The separation is
the model–view cut (spec §1.5) applied to analysis: the `Analysis` is the model
of facts; a solver's dispatch, a linter's rule set, and an optimizer's guard are
views that read it.

**The boundary this draws with the native-solver horizon.** Spec §1.1 and §13
place "structural analysis and classification" in the horizon of a from-scratch
solver composed of foundation crates. This crate is the *syntactic* half of that,
drawn into the foundation because a program's structural facts are a reading of
the program *value* and want no engine — while the grounding, the solving
algorithms, and the analysis-directed routing remain the satellite's (spec §13).
The line is exact: **facts here; policy and mechanism there.**

**Naming ground (spec §1.4).** The public vocabulary is the ASP literature's —
predicate, dependency, stratification, tight, head-cycle-free, safe, Horn,
disjunctive, choice, strongly-connected component. The class names are the ones
their results are cited under (a reader who knows the literature needs no gloss);
where a name is introduced that the literature does not carry, it owes a reason
in place. As with every crate of this estate, the crate, its types, and its
modules carry that literature vocabulary; the names of external systems that
consume it appear only as named consumers.

**Module map.** `construct` (the construct scan, §7); `depend` (the predicate
dependency graph and its components, §4); `safe` (safety and finiteness, §5);
`classify` (the program classes, §6); `analysis` (the assembled `Analysis`
value, §3).

**Crate facts.** `#![forbid(unsafe_code)]`, an FFI-free dependency closure, no
build script — and, because spec §12.3 enumerates the pure, FFI-free crates *by
name*, this crate is **added to that closed list**, not merely covered by it. It
depends on `themelios-program` (the value it reads, and through whose provenance
it reaches base's `Location` for a witness's span, program §6) and on nothing
else: the strongly-connected-components decomposition is **hand-rolled and
iterative** (§4), not a graph-library dependency — the hand-writing-by-default
rule (spec §12.5) and the depth discipline (spec §5.2) jointly require it. It
emits **no diagnostics** of its own; it reports facts (§1). Every value it
produces is owned plain data — `Send`, `Sync`, `'static` — a report the caller
holds, exactly as a `Program` is (program §1); every operation is a pure, total
function of the program it reads: no global state, no I/O, no panic on any input
(spec §2 item 8).

## 2. What this design is for

The postcondition, stated so a maintainer can check drift against it:

> themelios-analysis gives every consumer a **typed model of a program's
> structural facts** — its constructs, its predicate dependency structure and the
> strongly-connected components of it, its statements' safety and its grounding
> finiteness, and its membership in the classes of the literature — computed as a
> **pure, total** function of the `Program` alone, at the **predicate level and
> before grounding**, where the **recursion-class** verdicts (tightness,
> head-cycle-freeness) are **sound approximations** whose uncertainty is a value
> (`Unknown`) rather than a guess and whose error direction is stated and always
> safe — the syntactic classes and stratification are **definite** — and where
> every negative or `Unknown` verdict carries the **witness** that is its point.
> It states facts; it decides no policy.

This design has failed — independent of any local defect — when any of the
following holds. The list is the checkable form of the postcondition and of the
specification's rules it inherits (spec §4, §1.5).

- A panic escapes any public operation on any input (spec §2 item 8), or an
  operation ships without documented failure semantics.
- A class verdict **over-claims**: a program is called tight, stratified, or
  head-cycle-free when it is not, so a consumer that specialized on the verdict
  would compute an *unsound* result (§6). The error direction is the whole safety
  argument; a verdict that errs toward the strengthening property is a defect,
  never a performance trade.
- A verdict the syntax cannot decide is returned as a definite yes or no rather
  than **`Unknown`** (§6) — a failure to *prove* a property reported as a fact
  about it, the closed-world fallacy §6's discipline forbids.
- A negative verdict is a **bare boolean** with no witness (§6, §3): the
  component, rule, or cycle that produced it is the primary product, and losing it
  fails the consumers that need it (a diagnostic, an optimizer's justification).
- The crate **decides policy** — routes a class to an algorithm, sets a
  threshold, ranks a warning — rather than reporting the fact a policy would read
  (§1, spec §1.5).
- The analysis is **not a pure function of the program** — it consults an engine,
  grounds, solves, or depends on anything but the `Program` it is given.
- A dependency arrives unargued; unsafe code appears; an FFI type enters the
  closure (spec §12.3, §12.5).

**The consumer this serves, named.** The foundation exists to make satellite
systems natural compositions of its parts (spec §1.1). This crate's structural
`Analysis` is the contract a native solver's algorithm selection reads — the
"which method does this program admit" table that spec §1.1's classifier is —
and, secondarily, a formatter's lints and a program optimizer's pass conditions
(program §2, §9). A design under which such a consumer could not read the fact it
needs from the `Analysis`, and had to re-derive it or reach past this crate into
the program value's internals, has failed the composition test (spec §4).

## 3. The Analysis value

The whole of what this crate reports about one program is a single owned value,
computed once and read many ways:

```rust
/// A model of a program's structural facts (§1). Owned plain data — `Send`,
/// `Sync`, `'static` — computed as a pure, total function of a `Program`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Analysis { /* private: constructs, dependencies, safety, classes */ }

impl Analysis {
    /// The one entry point: read a program, report its facts. Total.
    pub fn of(program: &Program) -> Analysis;

    pub fn constructs(&self) -> &Constructs;          // §7
    pub fn dependencies(&self) -> &DependencyGraph;   // §4
    pub fn safety(&self) -> &Safety;                  // §5
    /// A program's membership in each class of the literature (§6), each a
    /// verdict carrying its witness.
    pub fn classes(&self) -> &Classes;                // §6
}
```

`Analysis::of` is the single door: a consumer computes it once and reads the
facets it needs. The facets are computed together because they share the
dependency graph (§4) — safety and the classes both read it — so recomputing per
facet would rebuild the graph each time; one pass builds it once. Every facet is
a typed value, never a rendered string, so a consumer acts on the fact and no
consumer parses prose (spec §1.5).

**Computational cost.** `Analysis::of` is `O(program + edges)` — one iterative
walk of the program (program §13) to scan constructs and collect dependency
edges and safety facts, plus the strongly-connected-components decomposition of
the dependency graph, which is linear in the graph (§4). It allocates the graph
and the facets and nothing per query thereafter; reading a facet is `O(1)`, and
reading a witness is `O(the witness)`. Clone is linear; equality is structural.

## 4. The predicate dependency graph

The core structural object, from which safety's negative half and every recursion
class are read, is the **predicate dependency graph**: a directed graph over a
program's predicate signatures, an edge from a head predicate to each predicate
its rule's body depends on, tagged by *how* it depends.

```rust
/// Three types this crate **reuses from the program tier** rather than redefine
/// (program §4, §12.1, the one authority for all three): `Signature` (a name, an
/// arity, and a strong sign, grammar §5.2, §5.9) is the graph's node identity;
/// `Rule` (program §4.3) is the owned rule a witness carries by value (§6.3) — a
/// source span, when wanted, read from the rule's own provenance (program §6); and
/// `DependencyKind` (program §12.1) is the tag an edge carries — the semantic mode a
/// dependency runs through: `Positive`, a monotone dependency the tight fragment is
/// bounded by; `Negative`, under default negation, which breaks stratification;
/// `ThroughAggregate`, non-monotone through an aggregate or theory atom. It is
/// defined at the substrate because the mode is a structural fact of a rule, read
/// there by `body_signatures`, and the graph reuses it — so there is **one**
/// authority for "how a predicate depends on another," not a substrate tag and a
/// graph tag that could drift. Non-exhaustive there, so a later split — a
/// theory-atom kind distinct from an aggregate one — is a variant, not a break.
pub use themelios_program::{DependencyKind, Rule, Signature};

/// The predicate dependency graph and its strongly-connected components. The SCC
/// decomposition is the **primary product**, not a flag: the recursion classes
/// (§6), their witnesses, and a solver's per-component dispatch are read off it.
/// Its meaningful *projections* are first-class objects in their own right — "the
/// positive dependency graph of this program" is a question a semanticist, a
/// loop-formula computation, a well-founded solver, and a grounder all ask
/// directly, independent of any class verdict — so the graph exposes them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DependencyGraph { /* private: nodes, edges, sccs */ }

impl DependencyGraph {
    pub fn predicates(&self) -> impl Iterator<Item = &Signature>;
    /// The edges out of a predicate, each tagged (a predicate may reach another
    /// by more than one kind; both edges are present).
    pub fn edges_from(&self, from: &Signature) -> impl Iterator<Item = (DependencyKind, &Signature)>;
    /// The strongly-connected components, in reverse-topological order — a
    /// component before every component *that depends on it* — the order a
    /// bottom-up solver grounds them in.
    pub fn components(&self) -> impl Iterator<Item = &Component>;
    pub fn component_of(&self, predicate: &Signature) -> Option<&Component>;
    /// The **positive dependency graph**: this graph with only its `Positive`
    /// edges, and its *own* SCCs (finer than the full graph's, since dropping the
    /// non-monotone edges breaks cycles). A first-class projection, and the object
    /// tightness and head-cycle-freeness are read off (§6) — an acyclic positive
    /// graph *proves* the ground program tight, a positive cycle leaves it
    /// `Unknown` with that cycle its witness. The same type, walked identically.
    pub fn positive(&self) -> DependencyGraph;
    /// Whether the graph has no recursive component. Tightness reads
    /// `graph.positive().is_acyclic()`.
    pub fn is_acyclic(&self) -> bool;
}

/// One strongly-connected component: the mutually-recursive predicates, and how
/// the recursion within it runs — the facts the recursion classes read (§6). A
/// component of the *full* graph reports which kinds its cycle runs through; a
/// component of the *positive* graph is a positive cycle by construction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Component {
    /* private: members: BTreeSet<Signature>, internal_edge_kinds */
}
impl Component {
    pub fn members(&self) -> impl Iterator<Item = &Signature>;
    pub fn is_recursive(&self) -> bool;                  // a cycle within it
    pub fn has_positive_cycle(&self) -> bool;            // recursion through a positive edge
    pub fn has_negative_cycle(&self) -> bool;            // recursion through `not`
    pub fn has_aggregate_cycle(&self) -> bool;           // recursion through an aggregate
}
```

**The graph is over predicate signatures, not ground atoms, and that is the whole
approximation.** This crate runs *before* grounding, so it cannot see the ground
dependency graph that tightness and head-cycle-freeness are classically defined
over. It reads the **predicate-level** graph instead — a sound over-approximation
of the ground one: every ground dependency is an instance of a predicate
dependency, so a program the predicate graph proves acyclic-positive grounds to a
tight program, while a positive predicate cycle *may* ground to a tight program
or may not, which is exactly where a verdict becomes `Unknown` (§6). The edges are
collected by one walk of the rules through the program tier's structural
accessors (program §12.1): a rule contributes an edge from each head predicate to
**every predicate its body reaches** — a body literal's, and, crucially, a
predicate inside a *condition* (a conditional literal's condition, a disjunction
or choice element's condition, an aggregate or optimize element's condition,
program §4), because a positive cycle *through a condition* is a real dependency
and omitting it would be a false `Holds` for tightness, the one over-claim §6
forbids. Each occurrence contributes one edge **per dependency mode it carries**
(`DependencyKind`, program §12.1), and the modes are not mutually exclusive: a
positive literal is one `Positive` edge, a `not`-ed literal one `Negative`, a
predicate inside an aggregate one `ThroughAggregate` — and a predicate inside a
*negated* aggregate contributes **both** `ThroughAggregate` and `Negative`, because
it is non-monotone *and* under negation, and collapsing it to one tag would drop a
fact a projection reads (the `Negative` edge is what keeps stratification from
clearing a recursive negated aggregate, §6.2). This is honest KR, not a symmetric
grid: the modes are the three the literature draws, and a dependency simply carries
each that holds of it. A choice or disjunctive head contributes an edge from *each*
of its head predicates.

**The strong sign is part of the node.** A predicate `p` and its strong
negation `-p` are distinct nodes (grammar §5.2): they are different atoms in an
answer set (program §3.1), so a dependency on one is not a dependency on the
other, and folding them would make the graph — and every class read off it —
unsound.

**Computational cost.** Building the graph is `O(rules · body size)` — one walk,
one edge per body predicate occurrence. The components are a **hand-rolled
iterative** strongly-connected-components decomposition (Tarjan's, on an explicit
stack — a library implementation typically recurses in graph depth and would
breach the depth discipline, spec §5.2, program §13), `O(nodes + edges)`.
`positive()` is `O(nodes + positive edges)` (a filter and a second decomposition);
`component_of` and `edges_from` are `O(log predicates)` and `O(out-degree)`.

## 5. Safety and finiteness

Two facts about whether a program can be ground, and ground finitely.

```rust
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Safety { /* private: unsafe rules, finiteness */ }

impl Safety {
    /// The statements that are not safe — empty when every statement is safe. Safety is the
    /// ASP-Core-2 standard's condition (below): every variable has a *binding*
    /// occurrence. An unsafe statement — a rule or a bodied directive — cannot be grounded, so
    /// this is a well-formedness fact a grounder needs before it runs.
    pub fn unsafe_statements(&self) -> impl Iterator<Item = &UnsafeStatement>;
    pub fn is_safe(&self) -> bool;
    /// Whether grounding is finite — a sound approximation (§6.1), so it is the same
    /// `Verdict` the recursion classes carry: `Holds` (proven finite), or `Unknown`
    /// carrying the recursive `Component` whose term growth blocked the proof. Never
    /// asserted infinite where it might be finite; there is no `DoesNotHold` arm,
    /// because the property a grounder relies on is *finite* and this crate asserts
    /// only what it proves.
    pub fn finiteness(&self) -> &Verdict;
}

/// An unsafe statement and why: the statement — by structural value with its provenance
/// (§8), so it names a rule or bodied directive of any program — and the variables with no
/// binding occurrence, the witness rather than a bare "unsafe". A source span, when wanted,
/// is read from the statement's own provenance (program §6). Equality is provenance-blind,
/// as everywhere in this tier — the witness reads uniformly with the construct scan's (§7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UnsafeStatement { /* private: statement: WithProvenance<Statement>, unbound: BTreeSet<Variable> */ }

// Grounding finiteness has no bespoke type: it is a `Verdict` (§6.1) like tightness
// and head-cycle-freeness — `Holds` (proven finite) or `Unknown` carrying the
// recursive `Component`. Folding it into `Verdict` is what keeps the approximation
// family one shape (there is no `Finiteness` enum, no `Verdict<W>` mono-generic).
```

**Safety is definite; finiteness is approximate.** Safety is a *syntactic* check —
decidable, a variable is safe or it is not — so `Safety` reports it exactly, with
the unsafe statement and its unbound variables as the witness. The condition is the
**ASP-Core-2 standard's** (the working group's, grammar §3, §6): a variable is
safe when it has a *binding occurrence*, and the definition's cases are each
honored so that "definite" is earned rather than glossed. A **positive body atom**
binds a variable where it sits in an **invertible** position, exactly as the
grounder's `simplify` reads it (`libgringo` `term.cc`): through a Herbrand
constructor (a function or tuple argument, whose match unifies) or a **linear**
arithmetic form `m·x+n` — built from `+`/`-`/`*` of one numeric constant and one
linear operand, `*` requiring a *non-zero* constant. Every other former is **not
invertible** and binds nothing: `/`, `\`, `**`, a bitwise operator, `~`, `|·|`, a
`@`-call, an interval, a pool, `*` by zero, an interval or pool *as an arithmetic
operand*, and any arithmetic that leaves two variables — the variable there must be
bound elsewhere. So `p(X)`, `p(f(X))`, `p(X+1)`, and `p(2*X)` bind `X`, while
`p(X/2)`, `p(|X|)`, `p(X..3)`, `p(X+(1..3))`, `p(0*X)`, and `p((X;a))` require it
without binding it. This analysis reads the **unpooled** program (program §9): the pool
pass expands `p((X;a))` into `p(X)` and `p(a)` before safety runs, so it is unsafe
*because the `p(a)` alternative drops `X`* — the every-alternative rule falling out of
the expansion, not a bespoke check. A pool as not invertible (above), and a *pooled atom*
binding nothing, are the **defensive fail-closed guards** for a residual pool the pass
leaves (a theory atom, or a lone body conditional's pooled head, program §9). A global
variable is bound by
the body; an aggregate- or condition-*local* variable within its own element; a
variable by an assignment `X = t` whose right side is itself bound. A
**default-negated** literal binds nothing, but the grounder projects each anonymous
`_` under `not` to a fresh existential, so a `_` there — reachable through
constructor structure only, never an evaluated term — needs no binder (`p :- not
q(_).` is safe). A **body conditional** `l : cond` binds two-stage — its condition
instantiates first, then its literal is matched per instance, binding *locally*
(`h :- X = 1 : q.` is safe; a variable a conditional binds only locally is not
bound globally). These engine readings — the invertible-matching positions, the
`_`-projection, the conditional two-stage — are the clingo grounder's, verified
against the pinned binary (§10) where the strict standard is silent or stricter;
the standard is the floor, clingo the authority for its extensions. Safety scopes
**every statement that can bind a variable** — a
derivation rule (head requiring, body binding) and the **bodied directives** (a
weak constraint, `#show t : body`, `#project`, `#edge`, `#heuristic`, `#external`,
and an optimize element, whose non-body term positions the body binds) — classified
exhaustively over `Statement` in the program tier (`statement_binder`), so a new
statement kind is a compile error there, never a silent fail-open here; a directive
that admits no variable (a signature, an include, a query — grammar §6.1) carries no
obligation. Engine-specific directive positions the differential (§10) settles against the
authority for these clingo-only directives: a `#project` and a `#heuristic` **atom** both
*domain-match* — their variables bind in the same invertible positions a body atom's do — so
`#project p(X).` and `#heuristic p(X). [X@1, sign]` are safe, while `#project p(X/2).` and
`#heuristic p(a). [W@1, sign]` are not (`X` sits under a non-invertible `/2`; `W` is unbound). A
`#heuristic`'s bracket terms (bias/priority/modifier) are then bound by the atom's domain match
or its body; a `#external`'s value bracket and an `#edge`'s node terms are required and bound by
the directive's body (never self-binding), so `#external p(X) : q(X). [W]` and `#edge (X, Y).` are
unsafe on the variable the body leaves unbound. A body-less `#show t.` binds nothing, so a variable in its shown
term is unsafe (`#show f(X).`, agreeing with clingo) — the once-liberal bare-show boundary now
closed. A variable occurring only in a **theory term** is descended for its
ordinary variable leaves — the shared leaves the theory peer algebra meets the
ordinary term algebra at (program §4.9: "a variable and a ground symbol") — and
required, never binding, since a theory term binds no ordinary variable
(`:- &t { X }.` is unsafe, agreeing with the grounder). One question is named and
pinned, not assumed: **clingo's grounder
may admit a safety notion that differs from the strict standard**, so this crate
takes the standard as the authority, records any divergence against the pinned
binary differentially (§10, as the grammar records its own, grammar §11), and —
should they differ materially — parameterizes safety by dialect the way the
grammar parameterizes the language (grammar §6). Finiteness is different:
grounding termination is not decidable for programs with
function symbols (a rule `p(f(X)) :- p(X)` grows terms without bound), so
`finiteness` is a *sound approximation* returning the same `Verdict` (§6.1) the
recursion classes do — `Holds` where this crate can prove grounding bounded,
`Unknown` (carrying the recursive `Component` through which terms grow) where it
cannot. This is the same discipline the recursion classes carry (§6): assert only
the property a consumer safely specializes on, and make the uncertainty a value.

**The growth reading, and its soundness (no false `Holds`).** A recursive
component's grounding is unbounded — in **term depth** — exactly when a rule deepens,
on the recursion, a term the component carries; so `finiteness` proves `Holds` by
finding *no* such deepening, and that proof is only as sound as (a) its enumeration of
**how** a head term can deepen a carried variable, and (b) its collecting carriers
**wherever the dependency graph reads a dependency** (§4), *plus the carrier positions the
graph does not read as a plain atom* — an aggregate's lone-variable guard, which is bound
to a member value of the predicates it ranges over — never a variable the graph makes
recursive that the growth check cannot see. A head-atom argument deepens a carried
variable when it is a term-former (`f`, a tuple, an arithmetic operation) over it, by one
of **four** paths: written directly (`q(f(Y)) :- q(Y)`); reached through a body
`=`-assignment that makes a variable deepen a carried one (`q(X) :- q(Y), X = f(Y)`,
transitively along a chain); over a variable a **head element's own condition** carries
(`p(f(X)) : p(X) :- base.` — the condition `p(X)` makes `p` recursive and carries `X` to
the derived `p(f(X))`, so its carriers are read exactly as a body atom's are); or through a
**`#max`/`#min` aggregate whose element value-term *is or aliases* a former** (`p(X) :- X =
#max { f(Y) : p(Y) }.` — the extremum returns a member value-*term*, so `f(Y)` makes the guard
`X` one former deeper than the members it maxes over, exactly the body `p(Y), X = f(Y)`; and the
value's depth is read from the element's own `=`-relations, so the aliased spelling `X = #max { Z
: p(Y), Z = f(Y) }` deepens the same way; the member variables are carried so the deepening is
reachable). The `=`-assignment case is the one to
hold carefully: `X = f(Y)` embeds a successor — a Church numeral — so it *deepens*, where
the aliasing `X = Y` does not; treating the two alike would miss an unbounded grounding, a
**false `Holds`**. The term successor `X = f(Y)` and the arithmetic successor `X = Y + 1`
are the same act — generating unbounded naturals — and both deepen. This enumeration, the
carrier-graph congruence, and the argument that each path is caught, is the soundness
obligation `Holds` answers to; it is discharged by the growth laws (§10), by a **bounded
grounding differential** now — a `Holds` program must ground within a rule-count cap against
the pinned grounder (§10) — and, for the full ground-level precision, by the grounder
differential the solve tier brings.

**A `#external` generates, so finiteness reads it too.** A `#external a : body` *generates*
the atoms of `a` for each solution of `body` — no rule derives them, so they are invisible to
the rules-only dependency graph, yet a domain-extending external that closes a generation
loop grows the Herbrand universe without bound (`p(a). q(X):-p(X). #external p(f(X)):q(X).`
generates `p(f(a)), p(f(f(a))), …`; clingo grounds it unbounded, a **false `Holds`** were the
external ignored). So finiteness reads a program carrying any external over a **generation
graph** — the derivation graph augmented with each external as a pseudo-rule `atom :- body` —
and vets each external's head atom for deepening exactly as a rule head, on that graph. The
generation edge is *not* a derivation dependency (the atom is declared, not derived), so it
stays out of the shared graph the recursion classes read (§6.2–6.3, whose tightness and
stratification are about derivation): it is built only for finiteness. The reading stays
precise, not blunt — a non-deepening external (`#external p(X):q(X)`) or one on a finite,
non-recursive body generates no unbounded depth and holds; only a term-forming head over a
variable carried around a generation cycle is `Unknown`, witnessed by that recursive
component.

`Holds` is proven for the program's **safe** rules; at an untrusted boundary the sound
gate is `is_safe() && Holds`. A rule reported unsafe is outside the proof: a grounder whose
`=` binds more than the standard's — clingo decomposes `Z = (X, Y)` to bind `X`, `Y` — can
ground it unboundedly, and safety already refuses it. This matching-`=` shape is a
characterized dialect boundary; the solve-tier grounder differential carries it as an
expected divergence.

**Scope: term depth, not integer value.** `finiteness` proves finiteness of the
**Herbrand term** instantiation — unbounded function-symbol nesting. A program can be
Herbrand-depth-finite yet ground infinitely through an unbounded **integer** value
(`q(M) :- M = #count { X : q(X) }` grows `q(0), q(1), …` without deepening any term), and
this crate reports `Holds` for it: a *correct* statement of term-depth boundedness, and
out of this facet's scope — integer-domain growth and its stratification are read
elsewhere (§6). A consumer that needs full grounding finiteness conjoins `Holds` with
those. Within its scope the reading is a sound over-approximation: it may report `Unknown`
where the ground program is in fact finite (a deepening a lower stratum in fact bounds),
never a false `Holds`.

**Computational cost.** Safety is `O(rules · variables)` — one pass per rule
collecting binding occurrences; finiteness reads the components (§4) and the
term structure of the recursive rules, `O(program + edges)`.

## 6. The program classes

A program's membership in the classes of the literature — tight, stratified,
head-cycle-free, normal, Horn, disjunctive, choice — is what a solver's algorithm
selection reads: each class the ground program falls in admits a specialized
method (spec §1.1). This section states the classes and, once, the discipline
that makes them safe to specialize on.

### 6.1 The verdict, and the error direction

The classes split by how this crate can know them. Two are **syntactic** and
**definite** (a program is normal or it is not, read from its constructs, §6.3);
the recursion classes are **structural**, read off the dependency graph (§4), and
here the pre-grounding, predicate-level reading forces a discipline:

```rust
/// A sound approximation of a ground-program property, read at the predicate
/// level. `Holds` is *proven* — the property is guaranteed of the ground program.
/// `Unknown` is undecided at the predicate level — the ground program may or may
/// not have it — and carries the `Component` that blocked a proof. There is
/// deliberately **no** third `DoesNotHold` arm. **Concrete over `Component`, not
/// generic in the witness:** the approximation exists *because* this crate reads the
/// predicate level, so its witness is always a predicate-level `Component`; a type
/// parameter would be idle (used at one type), and the exact ground-level
/// classification a grounder would later give is *definite*, not an approximation,
/// so it is not a `Verdict` at all (§11). Every approximation verdict this crate
/// reports uses this one type — `tightness`, `head_cycle_free`, and grounding
/// `finiteness` (§5) — so a reader meets one shape, not a `Verdict` beside a
/// bespoke twin.
#[derive(Clone, PartialEq, Eq, Debug)]   // closed: `Holds` or `Unknown`, no third arm (below)
pub enum Verdict { Holds, Unknown { witness: Component } }
```

**Why two arms, not three — the error direction, stated once and cited by every
class.** A consumer specializes on a class's *presence*: told a program is tight,
a solver uses completion; told stratified, it computes the perfect model. So the
cost of being *wrong* is asymmetric — a false `Holds` yields an **unsound
result**, a missed `Holds` yields a **merely slower** one — and the only safe
design asserts `Holds` **only when it has a proof**. Everything else — a program
this crate cannot prove tight, whether because the ground program is genuinely not
tight or because the predicate level simply cannot decide — is `Unknown`, which a
consumer reads as "use the general method." Folding "not proven present" into one
`Unknown` is not imprecision; it is the guarantee that `Holds` never lies. A class
whose *absence* a consumer safely specializes on (stratification, whose negative
result a solver *can* use) is reported definite instead (§6.2): definite for
negation — both directions proven — and conservative-safe for aggregates, where a
`NotStratified` is always safe for a solver to use though the ground program may
still be stratifiable.

### 6.2 The recursion classes

```rust
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Classes { /* private */ }

impl Classes {
    /// Tight (Fages): no positive recursion, so the program's completion
    /// characterizes its answer sets. Read off the **positive dependency graph**
    /// (§4): `Holds` when `graph.positive()` is acyclic; `Unknown` carrying the
    /// positive SCC otherwise — the ground program may still be tight, so this
    /// crate does not claim it is not.
    pub fn tightness(&self) -> Verdict;

    /// Head-cycle-free (Ben-Eliyahu–Dechter): no two atoms of a disjunctive head
    /// lie in one positive cycle, so the program shifts to a normal one. Read off
    /// the positive dependency graph (§4) as a predicate-level approximation like
    /// tightness, its witness the positive SCC coupling two head atoms.
    pub fn head_cycle_free(&self) -> Verdict;

    /// Stratified: no recursion through a non-monotone dependency. Read off the
    /// dependency graph as the absence of any cycle through a `Negative` **or** a
    /// `ThroughAggregate` edge (§4) — not `Negative` alone, since a recursive
    /// non-monotone aggregate breaks the perfect-model computation exactly as a
    /// negation cycle does, and the predicate level cannot prove such an aggregate
    /// monotone. `Stratified` is proven (no such cycle → the program stratifies);
    /// `NotStratified` carries the cycle a solver can itself use. Definite for the
    /// negation fragment, conservative-safe for aggregates: it errs only toward
    /// `NotStratified` (the general method), never toward a `Stratified` it has not
    /// proven (§6.1's error direction).
    pub fn stratification(&self) -> &Stratification;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Stratification { Stratified, NotStratified { cycle: Component } }
```

The witness on every negative or `Unknown` verdict is the **strongly-connected
component** (§4) that produced it — the primary product (§3): a solver reads it to
dispatch the offending component to its general method while the tight components
around it take completion, and a diagnostic reads it to point at the recursion.
Stratification is definite for negation — a cycle through a `Negative` edge (§4)
*proves* non-stratification, and its absence, with no non-monotone aggregate
recursion either, proves stratification — and conservative-safe for aggregates: a
`ThroughAggregate` cycle is read as stratum-breaking, since the predicate level
cannot prove a recursive aggregate monotone (§6.1's safe direction). Tightness and
head-cycle-freeness are approximate because a positive predicate cycle does not
prove the ground program has a positive cycle (§4).

### 6.3 The syntactic classes

Read directly from the construct scan (§7), definite, each carrying the construct
that determines it rather than a bare boolean:

```rust
impl Classes {
    /// Normal: every head is a single literal — no disjunction, no choice, no head
    /// aggregate. `NotNormal` carries the first non-normal head's rule.
    pub fn normality(&self) -> Normality;
    /// Horn (definite): normal and **negation-free** — no default negation *and*
    /// no strong negation — the strictly-positive least-model fragment.
    /// (Strong negation drags in the implicit `:- p, -p` coherence check, which
    /// is not Horn; a definite program *with* strong negation is a distinct
    /// class, admitted when a consumer names it, §11.) `NotHorn` carries the
    /// disjunction, choice, or negation that breaks it.
    pub fn horn(&self) -> HornKind;
    /// Whether the program uses disjunctive heads, and whether it uses choice
    /// heads — the two head extensions a solver's method must account for.
    pub fn uses_disjunction(&self) -> bool;
    pub fn uses_choice(&self) -> bool;
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Normality { Normal, NotNormal { rule: Rule } }
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HornKind { Horn, NotHorn { reason: Rule } }
```

`uses_disjunction`/`uses_choice` are the two facts a bare boolean *is* the whole
answer — a program either contains the construct or does not, and there is no
witness beyond "here is one," which the construct scan already lists (§7) — so
they are booleans, not dressed-up verdicts. `normality` and `horn` carry a
witnessing rule because a consumer that learns a program is *not* normal wants the
rule that makes it so. (The witness is the program tier's `Rule` itself, reused
here — owned and identified by its **structural value**, so it names a rule of
*any* program, parsed or constructed — and a consumer reads a source span from
that rule's own provenance, program §6, which a constructed rule simply lacks, §8.
The `Rule` is the first offending rule, read from the program's rules directly:
normality's constructs — disjunction, choice, head aggregate — are head-borne,
always a rule's; and Horn's negation is **rule-restricted**, read from the
derivation rules alone, since a directive's negation — which the construct scan
records program-wide (§7) — is not part of the least-model fragment and could
witness no `Rule`.)

### 6.4 The membership projection

The classes are the *top* of a stack of queries, not a replacement for the ones
beneath them, and the three levels compose rather than compete. A consumer asks for
whichever it needs:

- **the structural object** — `analysis.dependencies()`, and first-class within it
  the positive subgraph `.positive()` and the components `.components()` (§4): the
  graph itself, to reason further, visualize, explain, or classify in a way this
  crate did not foresee. Facts, not only conclusions.
- **a class verdict** — `classes().tightness()`, `.stratification()`, … (§6.2–6.3):
  the property *derived* from that object, carrying its witness and its epistemic
  status in its type.
- **the confirmed classes** — those *provably* present, each a dataless, routable
  key; iterated, and `Ord` so a `BTreeSet` collects them when set operations help:

```rust
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum ProgramClass { Tight, HeadCycleFree, Stratified, Normal, Horn, NonDisjunctive, ChoiceFree }

impl Classes {
    /// The classes the program is **provably** in — each method's positive arm
    /// (§6.2–6.3), projected to a key. The routable, iterable view a solver's
    /// algorithm selection reads (§6 intro); the witness-bearing methods stay the
    /// authority, and the graph they read (§4) stays queryable beneath both.
    pub fn confirmed(&self) -> impl Iterator<Item = ProgramClass>;
}
```

These are three *different questions*, not three spellings of one — the object, the
witnessed verdict, the routable bit — and a consumer routinely wants more than one
at once: a diagnostic reads the class *and* the component that explains it; a router
dispatches on the class *and* hands the offending SCC to the general method. So the
projection sits over the verdicts, the verdicts over the graph, and every layer
stays exposed; classification is never a barrier in front of the primitive it derives
from.

The projection **inherits the error direction** (§6.1) for free: an `Unknown` tight
is simply absent from `confirmed()`, which is exactly the safe reading — never
specialize on a class not proven present — so the set is sound to `match` on by
construction. It names the *specialization-admitting* (restricted) classes, since
those are what a method selection keys on — hence `NonDisjunctive`/`ChoiceFree`, the
constructs themselves staying on `uses_*` (§6.3) — and it is `#[non_exhaustive]`
because the order-consistent, call-consistent, and s(CASP)-relevant classes will
want in. The containment among them — `Horn ⟹ Normal ⟹ NonDisjunctive ∧ ChoiceFree
∧ HeadCycleFree` (a normal program has no multi-atom head, so head-cycle-freeness is
vacuous), tightness orthogonal — is real and nameable now that the classes are
named, but a class *algebra* is a **reserved seam** (§11), not this
crate's charter: this section reports membership, it does not reason over it.

## 7. The construct scan

The simplest facet, and the one the syntactic classes (§6.3) and a formatter's
lints read: which of the language's constructs a program uses.

```rust
/// Which constructs a program uses. A set of flags with, for each, the first
/// occurrence's statement (program §4, §6) so a consumer can point at one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Constructs { /* private */ }

impl Constructs {
    pub fn uses(&self, construct: Construct) -> bool;
    /// The first statement bearing a construct, if any — the "show me one" a lint wants.
    pub fn first(&self, construct: Construct) -> Option<WithProvenance<Statement>>;
    pub fn all(&self) -> impl Iterator<Item = (Construct, WithProvenance<Statement>)>;
}

/// The constructs an analysis or a lint distinguishes (grammar §5). Closed; a
/// construct is admitted when a consumer names it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Construct {
    Disjunction, Choice, HeadAggregate, BodyAggregate, StrongNegation,
    DefaultNegation, Comparison, Interval, Pool, Arithmetic, Optimization,
    WeakConstraint, TheoryAtom, ExternalStatement, Heuristic, Edge, ExternalCall,
}
```

The scan is one iterative walk of the program (program §13), setting a flag and
recording the first bearing statement at the first occurrence of each construct.
It is a fact, not a judgement: it reports that a program *uses* an aggregate,
never that using one is good or bad — a lint's policy over the scan is the
lint's (§1).

**The witness is the bearing statement.** Five of the constructs are
directive-borne — `Optimization`, `WeakConstraint`, `ExternalStatement`,
`Heuristic`, and `Edge` are statements in their own right, with no enclosing
`Rule` — so the witness is the `WithProvenance<Statement>` that bears the
construct, not a `Rule`: this makes *every* construct, rule- or directive-borne,
one a consumer can point at, and read a source span from when it wants one
(program §6, which a constructed statement simply lacks, §8). The carrier's
identity erases provenance (program §6.2), so the scan stays provenance-blind
(§8) — two programs equal up to provenance yield equal scans. The syntactic
classes (§6.3) that carry a `Rule` witness read it from the program's rules. Their
head constructs — disjunction, choice, head aggregate — are always rule-borne, so
the scan's witness for them is a `Statement::Rule`; negation is not, since the scan
records a directive's negation too (`#external -p`, `#show a : not p`), so Horn
reads it from the derivation rules alone (§6.3).

**Computational cost.** `O(program)` — one walk; `uses` is `O(1)`, and reading a
witness clones a statement, `O(witness)` (§8).

## 8. Posture, totality, and cost

Base §8's rules and the program tier's posture (program §14) bind this crate;
this section states what it adds.

- **Every operation is a pure, total function of the program's *content*.**
  `Analysis::of` reads a `Program` and consults nothing else — no grounder, no
  engine, no global state, no I/O — and it reads the program's *structure*, not its
  provenance: two programs equal up to provenance (program §5.2) yield equal
  analyses, and a witness names its rule by structural value (§6.3), not a span, so
  the analysis of a *constructed* program (whose rules have no source span) is as
  sound as that of a parsed one. Where mechanism wants mutation — the graph
  builder, the component stack — it is local and invisible (base §8.1).
- **The value is owned plain data.** `Analysis` and every facet are
  `Send + Sync + 'static`, a report the caller holds and may keep, compare, or
  send across threads — as a `Program` is (program §1).
- **Total on every program, including a recovered one.** A `Program` may have been
  raised from a malformed parse (program §8), so its rules can be partial; the
  analysis walks such a value without panic and reports the facts it can read —
  totality is over the value, not a well-formedness precondition (spec §2 item 8).
- **No refusals, and no `Display`.** Unlike the program tier's construction doors,
  this crate has no refusing operation — reading facts off a value never fails — so
  there is no refusal type, and base §8.5's std-trait posture attaches to nothing
  here: the fact types claim **no `Display`** (matching the program tier, program
  §14), and the reviewed dumps of §10 read a derived, iterative `Debug` or a view
  function, never a second rendering.
- **Facts, not policy, as posture.** The crate reports; it never routes, ranks, or
  thresholds (§1). A convenience that decided a policy — a `should_use(algorithm)`
  — would breach the separation and is refused as a design defect, not added.

**Computational cost, consolidated.** `Analysis::of` is `O(program + edges)`: one
iterative walk builds the construct scan, the dependency edges, and the safety
facts, and the strongly-connected-components decomposition is linear in the graph
(§4). Every read thereafter is `O(1)` for a flag or a facet and `O(witness)` for a
witness. Clone is linear; equality, ordering, and hashing are structural. No walk
recurses on the call stack (the graph decomposition is iterative, §4; the program
walks are the program tier's iterative ones, program §13), so a pathological
program cannot overflow one.

**The `edges` term is the real cost driver, and it can be `Θ(program²)`.** A rule
with an `h`-atom head over a `b`-atom body contributes `h · b` dependency edges, so a
single wide rule `p1;…;pn :- q1,…,qn.` of size `Θ(n)` yields `Θ(n²)` edges, which every
`O(nodes + edges)` reader then pays. This is *faithful*, not a defect: the graph
genuinely has that many edges; `O(program + edges)` names `edges` separately precisely
because it can exceed `program`; and clingo's own grounding is likewise super-linear on
such a program. An adversary at the untrusted boundary (spec §12.4) can push the
analysis to `Θ(program²)` only by handing it a program whose graph *is* that large — the
work equals the output, never a multiplier on top of it. The linear commitments
elsewhere in this crate (safety, finiteness, the SCC build) are `O(program + edges)` in
exactly this sense: linear in the graph the program defines, with no term super-linear
in the program on top of the edges the program spells out.

## 9. Failure semantics, consolidated

Spec §2 item 8's obligation, discharged at design level: **nothing in this crate
panics on any input, and nothing refuses.** `Analysis::of` is total over any
`Program` — well-formed, partial, or absurdly large — and every accessor is total.
There is no refusing door and therefore no table: the crate's whole failure
semantics is *it does not fail*. The one thing it must never do — assert a
strengthening class it has not proven — is a soundness obligation the verdict
types make structural (§6: `Holds` has no unproven path), held by the instruments
of §10, not a refusal.

## 10. Assurance instruments

Per spec §11 the crate is not done until these are green; per spec §10.1 proptest
and criterion are standing from its landing. Every instrument is documented with
what it proves and what it cannot (spec §10.2).

- **Property laws (proptest), over generated programs:**
  - **Totality:** `Analysis::of` never panics on arbitrary generated programs,
    including partial ones a recovered raise produces (program §8), and always
    yields an `Analysis`.
  - **Graph and components:** the dependency edges are exactly the rules' predicate
    dependencies with their kinds (§4); the strongly-connected components agree
    with a naive reachability reference, and the component order is
    reverse-topological.
  - **Definite verdicts, both directions:** `stratification` is `Stratified` iff
    the graph has no cycle through a `Negative` **or** `ThroughAggregate` edge (§5,
    §6.2 — the conservative-safe reading, so a recursive negated aggregate is
    `NotStratified`), and `safety` flags a statement — a rule or a bodied directive —
    iff a variable has no binding occurrence, descending a theory term for its ordinary
    variable leaves — each checked against a naive reference on generated programs whose
    shapes include bodied directives, theory-term variables, and the anonymous `_`.
  - **The approximations are sound, and their witnesses real:** whenever
    `tightness`/`head_cycle_free` is `Holds`, the *ground* program (ground on a
    bounded generated program by a naive reference grounder) has the property; and
    whenever the verdict is `Unknown`, its witness component carries a real cycle
    of the stated kind. This is the load-bearing law — it holds `Holds` honest.
  - **The scan is complete:** every construct that occurs is flagged, and every
    flag's `first` names a statement that bears it.
- **The differential** (out of band, through pixi against the pinned clingo):
  **safety** agrees with the authority's grounder on a corpus spanning the binding
  cases, the bodied directives, and the theory-term leaves — or the divergence is a
  recorded, characterized boundary (the matching-`=` dialect gap); and **finiteness's
  `Holds`-soundness** is backed by a bounded grounding check — a `Holds` program must
  ground within a rule-count cap, and an infinite control confirms the cap fires, so a
  false `Holds` is caught against ground truth. The full **ground-level** classification
  differential — exact tightness / head-cycle-freeness against the ground graph, and
  finiteness *precision* (an `Unknown` the ground program does not need) — still needs a
  ground dependency graph, hence a grounder, and is the solve tier's; there a
  predicate-level `Unknown` where the ground program *does* have the property is expected
  (the approximation's stated imprecision), while a `Holds` the ground program lacks is a
  defect.
- **Scaling shapes (criterion):** `Analysis::of` linear in `program + edges`; the
  component decomposition linear in the graph; the shapes asserted by the test suite,
  absolute numbers measured out of band as benchmarks (spec §10.2).
- **Golden snapshots**, reviewed: a corpus of programs with their `Analysis`
  dumped via a **provenance-blind view function** over the facts (§8, no `Display`)
  — the graph, the components, the verdicts, the witnesses — so the classification's
  shape is a diffable, reviewed artifact. The derived `Debug` is not used for the
  snapshot: it renders each `WithProvenance` node's provenance, which would make the
  golden provenance-sensitive and defeat the crate's provenance-blindness (§8).
- **Standing checks:** mutation per milestone over the graph, classification, and
  safety logic; the workspace coverage floor; unused-code and unused-result
  warnings denied; documentation examples that run; `forbid(unsafe_code)` and the
  structural trust checks (FFI-free closure, no build script).

## 11. Reserved seams and non-goals

Named reserved seams — deferred with their reasons and their arriving consumers:

- **Ground-level classification** — exact tightness and head-cycle-freeness over
  the *ground* dependency graph, which this crate approximates at the predicate
  level (§6). Its consumer is a grounder-having solver that wants the exact class
  where the predicate level returned `Unknown`; it belongs with grounding (spec
  §13), and this crate's `Unknown` is precisely the honest hand-off to it.
- **Grounding-size estimates** — a heuristic of how large the ground program will
  be, which a solver's router weighs against a threshold. It is a heuristic layer
  over the structural facts here; deferred until a router names the need, so that
  v1 ships the *definite* facts and *sound* classes without a heuristic whose
  calibration is policy (§1).
- **Further classes** — the literature carries more (order-consistency, signing,
  and kin); each is admitted when a consumer's method names it, over the same
  graph and the same verdict discipline (§6).
- **A class algebra** — reasoning *over* the classes rather than reporting each:
  the containment and subsumption lattice §6.4 names (`Horn ⟹ Normal ⟹ …`) and the
  entailments among verdicts, so one class could be derived from another or a set of
  them minimized. This crate reports which classes hold; a lattice whose shape a
  consumer *uses* — an explanation tool, a router optimizing its dispatch — waits
  until one names the need, so v1 ships the membership facts without it.

Non-goals, absolutely: solving and grounding (the solve tier and the
grounder-having solver of spec §1.1); any **policy** — algorithm selection,
thresholds, lint severity, warning ranking — which is the consuming system's over
these facts (§1, spec §1.5); semantic analysis that needs answer sets (a
solve-tier reading); and I/O of any kind. This crate reads a value and reports
facts, and stops there.

## 12. Specification touchpoints

Refinements to the specification this crate entails, recorded here — and in
program §18's roster note — rather than left implicit, so the specification's
successor carries them.

- **The crate roster and the trust closure.** `themelios-analysis` is added to
  spec §12.2's roster and to the *named* FFI-free, `forbid(unsafe_code)` closure
  spec §12.3 enumerates: a pure, engine-free foundation crate — the syntactic half
  of the native-solver horizon's "structural analysis and classification" (spec
  §1.1, §13), drawn into the foundation because it reads the program value alone
  (§1).
- **The safety authority.** Safety **adheres to the ASP-Core-2 standard by default,
  and takes clingo as the authority for its extensions** — the ratified doctrine
  (grammar §3, §6). Where the standard is silent or stricter than the grounder, safety
  adopts clingo's reading, verified against the pinned binary (§10): a positive atom
  binds only in an **invertible/matchable position** — mirroring the grounder's own
  `simplify` (a Herbrand constructor or a linear `m·x+n` form), never through `/`, `\`,
  `**`, bitwise, `~`, `|·|`, an interval, a pool, `*` by zero, or an interval/pool as an
  arithmetic operand; the grounder **projects an anonymous `_` under `not`**
  (existential); a **body conditional binds two-stage** (element-locally, in the same
  invertible positions); and the clingo-only directive readings hold (a `#project` and a
  `#heuristic` atom both **domain-match**, a `#heuristic`'s bracket bound by the atom or
  body, a body-less `#show t.` vetted). The one divergence safety does **not** adopt — the
  **matching-`=`** permissive gap — it characterizes differentially (below), because adopting it
  would risk a false `Holds` (§5's direction-sensitive deepening). A **pool**, at either level (a
  term pool `p((X;a))`, or an ordinary atom's argument-list pool `p(X;a)` the grounder unpools into
  *distinct atoms*), is now **represented faithfully** and eliminated by the **unpool pass** (program
  §9) before this analysis reads it, so a pooled program's safety **agrees with the grounder** at
  every position — the every-alternative rule falling out of the expansion, closing the *silent*
  false safe (and, at a head, the false `Holds`) the earlier truncated reading once reached. Two
  **representation-gap** pools stay deferred, fail-closed via the raise's diagnostic or the analysis
  guard (§5, program §17): a **theory-atom pool** (`&t(a;b)`, raised with `PooledArgumentList`), and a
  pooled derived literal in a **lone body conditional** (a disjunctive clause a single-literal
  conditional cannot hold); the composed `is_safe() && Holds` verdict is trustworthy on a faithfully
  raised program, so a consumer and the differential fail it closed on the theory diagnostic. Whether
  clingcon's grounder admits a further notion stays a differential obligation and, if
  material, a dialect parameterization (§5) — recorded so the question is pinned, not
  assumed. Each anonymous `_` in an ordinary term is treated as the distinct
  fresh variable the grounder sees (program §12.1): a `_` in a requiring position is unbound,
  an `X = _` binds the `_` when `X` is bound, and a positive atom's `_` binds itself; the
  freshening runs over the whole **statement**, not just a rule, so a directive-borne `_`
  (`:~ q. [_@1]`) is read the same way. The two once-liberal boundaries are now **closed**:
  a variable only inside a **theory term** is descended for its ordinary variable leaves
  (program §4.9's shared leaves, required-never-binding), and a **bodied directive** is vetted
  like a rule (the witness generalized to `WithProvenance<Statement>`). What remains a
  **characterized dialect boundary** — the standard-vs-engine divergence, not a hidden gap — is
  the **matching-`=`** notion (clingo decomposes `Z = (X, Y)` to bind `X`, `Y`; the strict
  standard does not — §5, with the chained `X = Y = 1` a candidate); the **aggregate-result
  assignment** (clingo binds `N = #count { … }`'s guard, the standard does not). (A **pool** is no
  longer a divergence here: the unpool pass (program §9) expands it before safety runs, so
  `q(X) :- p((X;X)).` is safe and `q(X) :- p((a;X)).` unsafe, both agreeing with the grounder.) Two
  further conservative families are recorded, both stricter than the
  grounder and so never a false safe: **arithmetic over a non-numeric operand** (the grounder accepts
  `f(X)*g(Y)` and `-f(X)` — a type error it grounds vacuously, or unary `-` passing boundness through a
  function — while this tier reads a non-numeric arithmetic operand as not invertible), and an
  **overflowing constant fold** (the grounder wraps; this tier's ground evaluator refuses overflow,
  reading the operand as not invertible). Each is recorded against the pinned binary and is the trigger
  for the dialect parameterization §5 reserves if it proves material. The `#external` **value** bracket
  (`#external p : q. [W]`, grammar §13) is now **vetted** — its variables are required and bound by the
  body, exactly as the atom's are, agreeing with the grounder (which collects the value with `bound =
  false`); an earlier reading left it liberal on the open question of whether the grounder checks it, and
  it does. The bare `#show t.` is likewise now **vetted** (a variable in the shown term is unsafe). A
  `#const`'s value is variable-free by the
  grammar's constant-term subset (grammar §5.9), so a raised `#const` needs no vetting; a *constructed*
  out-of-subset `#const x = X.` reading safe is a well-formedness question of a different facet, not
  safety's charter, deliberately.
- **Finiteness reads a generation graph, classification a derivation graph (§5, §6.1).** A
  `#external a : body` *generates* the atoms of `a` without deriving them, so it grows the
  Herbrand universe for grounding **finiteness** yet is **not** a *derivation* dependency the
  recursion classes read. Finiteness therefore reads a program carrying any external over a
  **generation graph** — the derivation graph plus each external as a pseudo-rule `atom :- body`
  — while tightness, stratification, and head-cycle-freeness (§6.2–6.3) read the derivation graph
  alone. Recorded because the two graphs are deliberately distinct: conflating them would either
  miss the external's unbounded generation (a false `Holds`) or make a declared, never-derived
  atom a spurious derivation cycle (a false non-tight/non-stratified). The generation graph is
  built only when an external is present, so the common program's single shared derivation graph
  is unchanged.
- **The dependency kind is the substrate's, not this crate's.** The edge tag is
  `DependencyKind` (program §12.1), reused here by `pub use` as `Signature` and `Rule`
  are, rather than a crate-local `EdgeKind`. The program substrate's edge accessor
  (`body_signatures`) had returned only the `DefaultNegation` prefix, which
  under-determines the three-mode graph tag; the tag is now defined once, at the
  substrate, and read here. The modes are not mutually exclusive, so a dependency
  carries one edge per mode it holds (§4) — no symmetric grid. Recorded so the single
  authority is visible, not left as a coincidence of two matching enums.
- **Stratification generalized to aggregates (§6.2).** `Stratified` is the absence of
  any cycle through a `Negative` *or* `ThroughAggregate` edge, not `Negative` alone,
  because the predicate level cannot prove a recursive aggregate monotone and a
  recursive non-monotone aggregate breaks the perfect model as a negation cycle does.
  It stays definite for negation and errs conservative-safe for aggregates (§6.1's
  direction) — recorded because it widens the classical negation-only stratification
  the literature names.
- **One approximation verdict, concrete over `Component` (§5, §6.1).** Grounding
  `finiteness` returns the same `Verdict` as `tightness` and `head_cycle_free`, not a
  bespoke `Finiteness` enum of identical shape; and `Verdict` is concrete over
  `Component`, not `Verdict<W>`. The approximation is inherently predicate-level, so
  its witness is inherently a `Component` (a type parameter used at one type is idle,
  and the exact ground-level classification a grounder gives is *definite*, not an
  approximation — §11). This makes the crate's generics obey the estate rule stated in
  program §14 — a parameter only on a carrier/view/decomposition role type, a domain
  object (a verdict included) concrete — so the analysis tier reads uniformly with the
  program tier beneath it.
