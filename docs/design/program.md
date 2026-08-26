# themelios-program — tier design

2026-08-24. Design for review, pre-implementation. This document is the API
design of `themelios-program` — the types, traits, signatures, semantics, and
computational costs of the foundation's program tier — derived from the v1
specification (`docs/specification.md`, cited as *spec §n*), the base tier
design (`docs/design/base.md`, *base §n*), the syntax tier design
(`docs/design/syntax.md`, *syntax §n*), and the grammar of record
(`docs/grammar.md`, *grammar §n*); a bare *§n* cites this document's own
sections. It is written to stand alone in the sense the specification is: a
reader holding this repository and public sources can check every claim. Where
this document and the specification disagree, the specification governs and the
disagreement is a defect here.

---

## 1. What themelios-program is

`themelios-program` is the logician's representation of an ASP program and the
operations over it: the `Program` value (spec §7.1), the term algebra and the
ground symbol beneath it, smart constructors with typed refusals (spec §7.3),
provenance as in-node model data (spec §7.4), Program → Program transformation
(spec §7.5), canonical round-trippable rendering (spec §7.6), and the pattern
language and unification that back the query surface (spec §7.7). It consumes
the syntax tier (spec §6): a program is *raised* from that tier's lossless tree
into this tier's value. It is consumed by the solve tier (spec §9), by the
macro tier (spec §8, whose expansions call the constructors here), and by the
immediate analysis and query clients this document names (§11, §12).

**The keystone: the `Program` is the abstract object, not the parse tree.** In
the knowledge-representation literature a logic program *is* a set of rules over
a term algebra: the reduct operates on the rules, and answer sets are defined
over the program-as-a-set. The syntax tier's tree (syntax §5) is *one concrete
notation* for that object — the grammar-shaped parse tree, faithful to the
bytes. This tier lifts the logical object into the primary value a consumer
holds, inspects, and transforms, and demotes the tree to the notation it was
raised from. Stated as the relation it makes precise:

> the tree is the **concrete** syntax; the `Program` is the **abstract**
> syntax; the *raise* (§8) is the parsing relation between them; and the answer
> sets (the solve tier) are the denotation.

This is fidelity to the literature, not ergonomics: a reader fluent in ASP
finds `Program` shaped like a program — a part-structured set of rules and
directives, each a head and a body — rather than shaped like a grammar. The
compiler-platform precedent (a high-level intermediate representation above a
parse tree) confirms the shape; the logical warrant is what *requires* it.

**Naming ground, stated once per spec §1.4.** The public vocabulary is the
KR/ASP literature's — rule, atom, literal, term, symbol, head, body, aggregate,
disjunction, choice, comparison, optimization, unification, substitution,
answer set. This tier's subject matter *is* the logical object, and its nearest
audience is the tool builder and the application author fluent in that
literature (spec §1.3); a public name that departs it owes its reason where it
is introduced. One consequence binds this whole estate and is stated here for
this tier: **themelios's own crates, types, and modules carry this
vocabulary**; the names of external projects it enables or is measured against —
the formatter, the testing successor, the solver, the explanation and
optimization tools this document names as consumers (§2) — appear only as those
named consumers, never as a themelios surface.

**Module map.** `term` and `symbol` (the algebra, §3); `program` (the value —
parts, statements, rules, heads, bodies, atoms, comparisons, aggregates,
optimization, directives, theory atoms, §4); `provenance` (in-node origin and
annotations, §6); `construct` (the smart constructors and the declarative
surface, §7); `raise` (the lowering from the syntax tier, §8); `transform` (the
visitor and rewriter machinery, §9); `render` (canonical concrete syntax, §10);
`unify` (patterns, the most general unifier, substitution, §11); `analyze` (the
structural accessors the analysis client reads, §12).

**Crate facts, carried as constraints.** `#![forbid(unsafe_code)]`, with the
workspace trust checks asserting an FFI-free dependency closure and no build
script (spec §12.2, §12.3). Its one lower-tier dependency is
`themelios-base` for locations and diagnostics, and `themelios-syntax` for the
tree it raises from; at most a small hash-map utility beyond them (spec §12.5).
Unlike the syntax tier — whose typed AST is a family of borrowed cursors over a
tree (syntax §5.1, §8) — **every value this tier produces is owned data**:
`Send`, `Sync`, `'static`, holding no borrow of any tree, so a `Program` is
constructed on one thread and solved on another, transformed and kept, without
a lifetime. That ownership is the precondition the solve tier's sessions and
the transformation surface both rest on.

**Engine-transparent by construction.** The `Program` value knows no backend.
clingo and clingcon share one concrete syntax (grammar §5.8, §7), so they share
one representation here — a constraint program's `&`-constraints are first-class
theory atoms (§4.9), with no clingcon-specific form — and the ground symbol
(§3.1) is the shared vocabulary an answer set of either speaks. The identical
experience of construction, solving, outcomes, and queries across the two,
barring backend configuration (spec §2 item 5), is delivered above this tier;
this tier delivers its half by **privileging neither** from the outset.

## 2. What this design is for

The postcondition, stated so a review can check drift against it:

> themelios-program gives every consumer the logician's **owned, total**
> representation of an ASP program — constructed declaratively in Rust or raised
> from the syntax tier through **one well-formedness authority**; compared by
> **structural (canonical-syntactic) equality up to provenance**, never by
> semantic equivalence; transformed as **pure `Program → Program` functions**
> with provenance carried through; rendered to **canonical concrete syntax that
> round-trips** (render → parse → raise is identity up to provenance); and
> queried through **patterns and unification** — with every value total and
> owned, every walk over user-reachable structure bounded independent of value
> depth, and provenance in-node so a program-level report points at source.

This design has failed — independent of any local defect — when any of the
following holds. The list is the checkable form of the postcondition and of the
specification's own rules (spec §4, §7); a §7 amendment owes its form here.

- A panic escapes any public operation on any input — malformed, hostile, or
  absurdly deep (spec §2 item 8, §7.2); or a public operation ships without
  documented failure semantics.
- A constructor or a conversion **repairs or truncates** rather than refusing
  (spec §5.2, §7.3); or admits a lexically ill-formed name; or a construction
  path reaches a `Program` without passing the one well-formedness authority
  (§7), or a second parser of ASP syntax exists anywhere in the tier (spec §2
  item 3).
- The `Eq` this tier carries is **presented as, or silently computes,
  equivalence** — ordinary or strong (spec §7.1); or two structurally-equal
  programs compare unequal, or two structurally-distinct programs compare equal,
  once provenance is set aside.
- A walk over user-reachable structure **recurses on the call stack** (spec
  §7.2, grammar §10); or a value's admissible depth is bounded by anything but a
  **stated, refused** limit; or a value deep enough to overflow a walk can be
  constructed and not refused.
- **render → parse → raise is not identity up to provenance** on a program the
  round-trip law covers (spec §7.6, §10.3).
- Provenance is a **side table** rather than in-node data (spec §7.4), or it
  changes what programs are equal, or it fails to survive a transformation that
  keeps a node's content.
- A satellite-class consumer named below needs a **private API, a fork, or a
  second grammar** to exist (spec §4) — the composition test fails.
- **Criterion of one obvious way** (the estate's DX bar, made checkable here): a
  *common* task — a fact, a rule, a choice, a cardinality bound, an
  optimization, a term, a simple rewrite, a query — has **more than one obvious
  way** to express it, or the obvious way is **not the safe (total, refusing)
  way**, or a conceptually simple thing is **not simple to express**, from
  *either* the ASP-author's vantage (writing the logic) *or* the programmatic
  client's (a machine assembling it). Simple things are simple so that complex
  things are possible; refusal appears only where failure is real; expressiveness
  comes from the composition of a few orthogonal primitives, not from surface
  area.
- A dependency arrives unargued; unsafe code appears; an FFI type enters the
  dependency closure.

**The fitness anchors.** Beyond the witnesses the specification runs (spec §3),
this tier is held to a set of **named systems it must be buildable under** —
library-first, as compositions of its parts (spec §1.1, §1.2). None is a v1
deliverable; each is a design constraint, the way spec §1.1 names the ecosystem
its foundation exists to enable. A design that could not carry one of them, or
that would need a private surface to, has failed the composition test above.

- **A declarative-testing successor** (the elenctic class, spec §5.1) — reads
  the pattern and unification surface (§11) and the query client (§12) for its
  verdicts, and *needs* the three-outcome answer that keeps "cannot decide" a
  value.
- **A probabilistic-ASP implementation** (the P-log class, spec §1.1) — a
  `Program → Program` translation to ASP plus a measure over a set of answer
  sets; exercises the transformation surface (§9) as a compiler back-end.
- **An explanation tool** (the xclingo class, spec §1.1) — trace annotations as
  typed in-node data that cannot drift from their rule (§6), a
  provenance-carrying rewrite (§9, the *transformation* witness, spec §3), and a
  derivation read off patterns and the query, delivered as a **typed model
  rendered as views** (spec §1.5) rather than printed text; the surface that
  lets such a tool be built to a mission-critical bar rather than as an academic
  exercise.
- **A non-ground optimizer** (the ngo class, spec §1.1's transformation
  anchor) — a constrained set of answer-set-preserving `Program → Program` (and
  `Rule → Rule`) rewrites over the transformation surface (§9) and the shared
  substitution core (§11).
- **A native solver's analysis contract** (spec §1.1's horizon) — the structural
  `Analysis` the analysis client (§12) produces, read by a solver's dispatch.

One boundary is drawn **once**, here, and cited wherever a rewrite or an
analysis is discussed: this tier provides the **machinery** and the
**structural** certificates (structural equality, the syntax tier's
token-stream certificates, syntax §11). It **never claims answer-set
preservation** — that is *semantic* equivalence, ordinary or strong, a reserved
seam this tier deliberately does not conflate with structural equality (spec
§7.1, §13). A rewrite's soundness is its author's, verifiable downstream by the
solve tier's differential; a classification's soundness is stated as its error
direction (§12). Reading "founds the optimizer" or "founds the explainer" as
"verifies their semantics" is the conflation this boundary forbids.

## 3. The term algebra and the symbol

The term algebra is the base of the whole tier: heads, bodies, comparisons,
aggregates, and directives are built over terms, and the ground symbol is what
terms denote, what answer sets contain, and what crosses the extension surfaces
(spec §9.6). It is the one place recursion is unbounded in the input (§13,
grammar §10), so its walks and its representation are designed for that from
birth.

### 3.1 The ground symbol

```rust
/// A ground term: the value an answer set contains, an `@`-function exchanges,
/// and a pattern unifies against. The vocabulary of the term order (grammar
/// §5.1): `Infimum` and `Supremum` are its least and greatest elements.
#[derive(Clone, Debug)]   // Eq/Ord/Hash are hand-written and iterative (§13)
pub enum Symbol {
    Infimum,
    Number(i32),
    String(String),
    /// A predicate or constant (a constant is the empty-argument case),
    /// carrying its strong sign. `name` is a validated identifier.
    Function { name: Name, arguments: Vec<Symbol>, sign: Sign },
    /// The anonymous functor: `(a, b)`, the one-element `(a,)`, the empty `()`.
    Tuple(Vec<Symbol>),
    Supremum,
}

/// Strong (explicit) negation — the `-` of `-p` (§4's precise register: strong,
/// not classical-logic, negation). Distinct in the type from default negation (a
/// body-literal sign, §4) and from the bitwise `~` (a term operator, §3.3): the
/// three are three different things and the API holds them apart (spec §1.4 — a
/// collapsed name would lie).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Sign { Positive, Negative }
```

**`i32`, engine-faithful, with its argument.** A `Symbol`'s whole purpose is to
*be* the ground vocabulary the backend speaks — the members of an answer set,
the arguments an `@`-function receives, the values the differential round-trips
(spec §10.1). The pinned authority creates a number from a machine `int`
(32-bit signed): a value outside that range is not a symbol the engine can
construct, so admitting it here would split "a `Symbol`" from "a member of an
answer set" into two types and fracture the round-trip, the extraction, and the
differential at their edges. `i32` keeps the identity exact. A constraint
extension's *assignments* may range wider, but those are theory results — a
solve-tier typed value (spec §2 item 5), a constraint-variable valuation, not a
`Symbol` — so the engine's width lives in that tier and never forces this one
wider. Programs needing larger or non-integer quantities encode them the way
ASP always has: structurally, as function terms or strings, or through the
rounding adapters (§3.4).

**One `Function` shape, and a distinct `Tuple`.** A predicate `p(1)`, a constant
`c` (its arguments empty), and a strongly-negated `-q(a)` are one variant
distinguished by fields, not three variants — the single function-like shape
keeps the term order total and equality consistent with no second
representation to collide with. The anonymous functor — the tuple — is a
*distinct* variant rather than an empty-name function: the grammar makes `(a)`
the term `a` and `(a,)` a one-element tuple (grammar §5.1), and a distinct
variant states that in the type rather than through an empty-string sentinel a
reader must know to check (the criterion of §2 — a simple distinction stays
simple). Its position in the order and its rendering coincide with the
authority's anonymous function, held by the differential (§16).

**The total order is the authority's.** `Symbol: Ord` is the ground-term order
of the literature and the engine (grammar §5.1): `Infimum` least, `Supremum`
greatest, and between them the numbers, strings, functions, and tuples in the
order the pinned authority prints — an order that **crosses the `String`
variant** (a nullary function-like sorts before a string, an arity-bearing one
after) and orders a **tuple as an anonymous function**, so functions and
tuples *interleave* by that key rather than share a rank: a tuple is an
anonymous-named function slotted among the named ones, never a second symbol
at the same position, so no two distinct symbols ever compare equal. The order
is therefore **total up front** — equal only to an identical symbol, the
precondition below — before the authority is consulted; what the differential
(§16) settles is only *where* the anonymous name and the arity bands fall in
the printed order, not *whether* the order is total. No derived `Ord` is
faithful, so the implementation is hand-written (§13) and checked against the
authority by the differential (§16). Every higher structure keys on
`Symbol: Ord` and on `Term: Ord` above it, so this order's agreement with
equality is the precondition the set semantics of §4 and the provenance merge
of §6 rest on.

**Computational cost.** `Symbol` is owned; clone is linear in the term's node
count; equality, ordering, and hashing are linear and iterative (§13). There is
no interning and no shared reference: a `Symbol` is plain owned data, `Send +
Sync + 'static` (the interning question base §11 and syntax §17 forwarded here
is answered *no interning in v1* — identity is structural equality of owned
values; a per-arena interner is a reserved optimization, §17, admitted only if
the scaling benches demand it, never a global table, which spec §1.2 forbids).

### 3.2 Names

```rust
/// A validated identifier — a function or predicate name (grammar §4.2). The
/// invariant "a name is a legal identifier" is guarded at construction, so a
/// `Symbol` or a `Term` cannot carry a name the grammar would reject.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Name(/* private: String, a checked IDENTIFIER */);

/// A validated variable name (grammar §4.2's `VARIABLE`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VarName(/* private: String, a checked VARIABLE */);

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnIdentifier { pub text: String }
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAVariable { pub text: String }

impl Name {
    /// Refuses text that is not the grammar's `IDENTIFIER` class
    /// (`[_']* [a-z] ['A-Za-z0-9_]*`, grammar §4.2). The one lexical authority
    /// for names, shared with the syntax tier's classifier so no second
    /// definition of "a name" exists (spec §2 item 3).
    pub fn new(text: impl Into<String>) -> Result<Name, NotAnIdentifier>;
    pub fn as_str(&self) -> &str;
}
impl VarName {
    pub fn new(text: impl Into<String>) -> Result<VarName, NotAVariable>;
    pub fn as_str(&self) -> &str;
}
```

Names are validated newtypes so that lexical well-formedness is a **type
invariant of the value**, not a check deferred to rendering or to the backend:
a `Symbol` in hand denotes a term the grammar can spell, and a program built
from valid names renders to text that parses back (§10). The classifier is the
syntax tier's — a name is exactly what its lexer admits as `IDENTIFIER` /
`VARIABLE` (grammar §4.2) — exposed once and called here, never re-implemented
(the one-grammar rule applied to the lexical classes). Primed and
underscore-leading names (`a'`, `_p`) are legal identifiers and admitted; the
reserved word `not` is not a name (grammar §4.5).

### 3.3 The term

```rust
/// The non-ground term algebra generalizing `Symbol`. Recursion is via `Box`
/// (the operator and interval forms) and `Vec` (the applied and grouped
/// forms); every walk over it is iterative (§13).
#[derive(Clone, Debug)]   // Eq/Ord/Hash/Drop hand-written and iterative (§13)
pub enum Term {
    Variable(Variable),
    /// The ground leaf. Maximal ground *constructor* subterms collapse here at
    /// construction (§5.1); a ground *operator* term (a ground `1+2`, §3.5) does
    /// not, so `is_ground` is a walk, not a check of this variant.
    Symbolic(Symbol),
    Function { name: Name, arguments: Vec<Term> },
    Tuple(Vec<Term>),
    /// A pool — alternatives, `(a; b)` (grammar §5.1). A semantic term-former:
    /// it names a set and cannot be expanded before grounding.
    Pool(Vec<Term>),
    UnaryOperation { operator: UnaryOp, argument: Box<Term> },
    BinaryOperation { operator: BinaryOp, left: Box<Term>, right: Box<Term> },
    /// `l .. u` — the interval, a semantic term-former like the pool.
    Interval { lower: Box<Term>, upper: Box<Term> },
    /// `|t|` — absolute value (grammar §5.1; a pooled `|a;b|` is an `Absolute`
    /// over a `Pool`, the shape the grammar gives it).
    Absolute(Box<Term>),
    /// `@name` / `@name(args)` — the ground-extension call site (spec §9.6).
    /// Represented here, **left unevaluated by this tier** (§3.5): evaluating
    /// it needs a registered context, a solve-tier concern.
    External { name: Name, arguments: Vec<Term> },
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Variable { Named(VarName), Anonymous }   // `X` / `_`

/// The prefix operators (grammar §5.1). `Negate` is arithmetic `-`; `BitwiseNot`
/// is `~` — named apart from `Sign::Negative` (strong `-`) and default `not`,
/// the three-negation discipline (§3.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum UnaryOp { Negate, BitwiseNot }

/// The infix operators (grammar §5.1), less the interval `..` and the pool `;`,
/// which are their own term-formers above. `BitOr` is `?`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum BinaryOp { Add, Sub, Mul, Div, Mod, Pow, BitAnd, BitOr, BitXor }
```

**`Term` carries no strong sign; `Symbol` does.** The `-` in `-p` is two
different operators depending on where it stands (grammar §5.1, §5.2): in term
position it is arithmetic `Negate`; in literal position it is the *atom's*
strong sign (§4.6). So `-f(X)` in term position is
`UnaryOperation { Negate, Function{f, [X]} }` whether ground or not — the negate
operator does not fold (§5.1) — while a `Symbol`'s `sign: Negative` arises
**only** from a strongly-negated *atom* (§4.6) lowered to the ground, the one
place strong negation lives. The syntax tier resolves the
ambiguity positionally in the tree (syntax §8.2 — an `Atom`'s strong-negation
token, its `strong_negation_token` accessor there, versus a `UnaryTerm`'s
operator), and the raise (§8) reads it from there; this tier never re-derives it.

**`Symbol` is a sub-algebra of `Term`.** Every ground symbol is a term
(`From<Symbol> for Term` yields `Symbolic`), and `Term`'s `Symbolic` leaf is
where the ground fragment lives; the collapse that keeps it maximal is
canonicalization's (§5.1), run at every door that stores a term, because `Term`
is a public enum a caller can build directly and so no constructor can promise
the invariant by itself (the reason canonicalization is a pass, not a
constructor guarantee — §5).

### 3.4 The conversion surface and the numeric bridge

The one surface crossed by three extension points — the ground-time
`@`-functions (spec §9.6), read-time extraction from answer sets (spec §9.6),
and the macro dialect's splices (grammar §9) — is the conversion between Rust
values and `Symbol`s. It is defined once, here, so those three never diverge.

```rust
/// A Rust value that denotes a ground symbol. Not `From`/`Into`: those name
/// only "can convert," while this names a KR relationship — *this value denotes
/// this ground term* — and, being this crate's own trait, a downstream library
/// may implement it for its own types (the orphan rule would block a bare
/// `From<Symbol>`), which is what lets a standard library of `@`-functions
/// (a mathematics, string, or date/time library) bridge its types.
pub trait ToSymbol { fn to_symbol(&self) -> Symbol; }

/// The inverse: extract a Rust value from a ground symbol, refusing with the
/// symbol that did not match (a value, not a rendered string — spec §1.5).
pub trait FromSymbol: Sized {
    fn from_symbol(symbol: &Symbol) -> Result<Self, FromSymbolError>;
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FromSymbolError { pub expected: &'static str, pub found: Symbol }

// Lossless inward conversions only. The wider integer types get no impl, so a
// value that would truncate does not silently convert — the caller reaches for
// a rounding adapter (below) or a checked constructor and states the intent.
impl ToSymbol for i8  { /* Number */ } impl ToSymbol for i16 { /* Number */ }
impl ToSymbol for i32 { /* Number */ } impl ToSymbol for u8  { /* Number */ }
impl ToSymbol for u16 { /* Number */ }
```

There is deliberately **no** `ToSymbol for f64` and **no** `ToSymbol for bool`: a
real has no integer symbol without a stated rounding (below), and a boolean has
no faithful ASP denotation — a caller that wants the constants `true`/`false`
constructs them by name, so the absence is the refusal.

**No blanket `f64` conversion; explicit, fallible rounding instead.** ASP is
integer-valued: there is no float symbol, and a silent `f64 → Symbol` would
either lose the reals or pick a rounding the author did not state — the
"repair" spec §5.2 forbids. The bridge is instead a small set of **explicit,
policy-named, fallible** adapters, the safe replacement for a bare `as` cast
(which saturates `NaN`/`±∞` and truncates out-of-range into plausible garbage):

```rust
/// Land a real in the integer domain under a stated rounding. `NaN`, `±∞`, and
/// any value outside `Symbol`'s integer range refuse — never a garbage integer.
pub fn floor(x: f64) -> Result<Symbol, NotAnInteger>;
pub fn ceil (x: f64) -> Result<Symbol, NotAnInteger>;
pub fn round(x: f64) -> Result<Symbol, NotAnInteger>;
pub fn trunc(x: f64) -> Result<Symbol, NotAnInteger>;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotAnInteger { NotFinite, OutOfRange /* |x| exceeds i32 */ }
```

The policy is the caller's, named at the call (`floor` versus `round` changes
which atoms ground); the edge cases refuse. themelios ships this **general
numeric bridge** and nothing domain-specific: a haversine, a date/time, or a
regular-expression library is a **downstream satellite** that *composes* the
bridge (`@haversine(a, b) → meters` via `round`), never a part of this tier
(spec §1.1 — the satellites are anchors, not deliverables). Multi-valued
`@`-functions — an external *relation* returning several tuples — are served by
the conversion crossing an `IntoIterator<Item = Symbol>`, a shape the solve
tier's registration reads; the shape is fixed here so the vocabulary is one.

### 3.5 Ground evaluation

Grammar §5.10 assigns the evaluation of the term-value sublanguage into ground
symbols to the tiers, and syntax §17 places it here. It is an **explicit door**,
never a pass over a program:

```rust
/// Evaluate a ground term to the symbol it denotes. Arithmetic is folded
/// faithfully to the authority's ground-term evaluation (grammar §5.10); a
/// variable, an unevaluated `@`-call, an undefined operation, or an
/// out-of-range result refuses.
pub fn evaluate(term: &Term) -> Result<Symbol, EvalError>;
impl Term { pub fn evaluate(&self) -> Result<Symbol, EvalError>; }

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EvalError {
    NotGround { variable: Variable },
    /// An `@`-call: evaluation needs a registered context (spec §9.6), which is
    /// the solve tier's, so this tier reports rather than guesses.
    External { name: Name },
    /// Division or modulo by zero, or another operation the authority rejects.
    Undefined,
    /// An `i32`-range result. This door **refuses** rather than wrap: it is a
    /// convenience door, refusal beats repair (spec §5.2), and a silently
    /// wrapped sum is exactly the undetectable wrong answer this estate forbids.
    /// The differential (§16) records the authority's own overflow behavior;
    /// where the authority wraps, this divergence is recorded, not chased.
    Overflow,
}
```

Composed with the syntax tier's `parse_term_value` entry (syntax §6.1) and the
fragment raise (§8), this door is the `parse-a-string-to-a-symbol` capability the
REPL and the query surface want — `parse_term_value` → `raise_term` →
`Term::evaluate` → `Symbol`. It is the **only** place this tier evaluates
arithmetic: a `1+2` embedded in a rule stays a `BinaryOperation` term — the
grounder's to evaluate — and canonicalization (§5.1) collapses ground
*constructor* terms to symbols but never folds an operator, so structural
equality never smuggles in an arithmetic normalization and the representation
stays a faithful record of what was written.

### 3.6 Navigating and rebuilding terms

`Term` and `Symbol` are walked and rebuilt through one iterative scheme, which
is what makes every operation over them — clone, equality, ordering, rendering,
the transformations of §9 — stack-safe on deep input (§13):

```rust
/// One level of a term unrolled, its children a generic `T`, the ground leaf
/// kept whole. At `T = Term` it is the owned decomposition (`From` rebuilds);
/// inside `fold` it is what the step sees with children already folded.
pub enum TermParts<T> {
    Variable(Variable), Symbolic(Symbol),
    Function { name: Name, arguments: Vec<T> }, Tuple(Vec<T>), Pool(Vec<T>),
    UnaryOperation { operator: UnaryOp, argument: T },
    BinaryOperation { operator: BinaryOp, left: T, right: T },
    Interval { lower: T, upper: T }, Absolute(T),
    External { name: Name, arguments: Vec<T> },
}
impl Term {
    pub fn into_parts(self) -> TermParts<Term>;
    pub fn subterms(&self) -> impl Iterator<Item = &Term>;   // pre-order, a contract
    /// Bottom-up rebuild, iterative (§13): children folded before their parent,
    /// in document order, `O(n)` heap, nothing cloned. The one primitive every
    /// rebuild over the term tree is written in.
    pub fn fold<T>(self, step: impl FnMut(TermParts<T>) -> T) -> T;
    pub fn try_fold<T, E>(self, step: impl FnMut(TermParts<T>) -> Result<T, E>)
        -> Result<T, E>;
}
impl From<TermParts<Term>> for Term { /* the inverse of into_parts */ }
```

The round-trip law `x.into_parts()`-rebuilt `== x`, and `x.fold(Term::from) ==
x`, are stated once and held per level (§16). `Symbol` carries the same scheme
(`SymbolParts<T>`, `into_parts`, `subsymbols`, `fold`, `try_fold`) — the
ground rebuild every extraction and lowering is written in.

**Computational cost.** Every operation above is `O(nodes)` in time and heap and
**iterative in depth**: `clone`, equality, ordering, hashing, `Drop`,
`into_parts`, `subterms`, `fold`. Casting a `Symbol` to a `Term` is `O(1)`
(a move into `Symbolic`); collapsing a ground `Term` to a `Symbol` is
`O(nodes)`.

### 3.7 Point access: the functor and its arguments

The scheme of §3.6 rebuilds or traverses a whole term; the most common read is
narrower — *what is this atom's functor, and what are its arguments* — the read
extraction, analysis, and meta-programming make constantly (`s.name() == "edge"`,
then `s.arguments()[0]`). It has its own direct, non-consuming door, because
pattern-matching the enum for it is ceremony on a high-frequency operation (the
criterion of §2), while `into_parts` *consumes* and `subterms` is a *traversal* —
both the wrong tool for a point read:

```rust
impl Symbol {
    /// The functor name — `Some` for a function or constant, `None` for a number,
    /// string, tuple, or `#inf`/`#sup`.
    pub fn name(&self) -> Option<&Name>;
    /// The immediate arguments — a function's arguments or a tuple's elements; the
    /// empty slice for an atomic symbol. Index it, or reach for `arg` below.
    pub fn arguments(&self) -> &[Symbol];
    /// The i-th argument, or `None` — **total**, never a panicking `s[i]`
    /// (`std::ops::Index` cannot return `Option`, and a panic on the public
    /// surface is forbidden, spec §2 item 8). This is how "index into a term" is
    /// spelled.
    pub fn arg(&self, i: usize) -> Option<&Symbol>;
    /// The number of arguments — `0` for an atomic symbol; `u32` to match a
    /// `Signature`'s arity (§4.8).
    pub fn arity(&self) -> u32;
    /// The signature `(sign, name, arity)` — `Some` for a function or
    /// constant, `None` otherwise (a number, string, tuple, or `#inf`/`#sup`).
    /// The key the dependency graph's nodes (analysis.md §4) and the pattern
    /// matcher's range (§11.3) are built from, so a consumer dispatching on
    /// "what kind of atom is this" reads it whole rather than the three pieces.
    pub fn signature(&self) -> Option<Signature>;
}
```

`Term` carries the same point access on its applied forms — `name`, `arguments`,
`arg`, `arity` over a `Function` or `Tuple`; a `Term`'s operator forms hold
*operands*, read by matching or `into_parts`, not "arguments". These are the
accessors a `FromSymbol` impl and the `#[derive(Extract)]` expansion (spec §9.6)
are written in, and the surface clingo's Python API reads its arguments through —
here typed and total. They complement, and never replace, the traversal trio of
§3.6: point access (`arguments`/`name`), read-traversal (`subterms`), and rebuild
(`into_parts`/`fold`) are three distinct jobs.

**Computational cost.** `name`, `arity`, `signature`, and `arg` are `O(1)`;
`arguments` is `O(1)` (a borrow of the stored slice, empty for an atomic symbol).

## 4. The Program value

A `Program` is a part-structured set of rules and directives (spec §7.1). This
section states its shape; §5 states its equality, §6 its provenance, §7 how it is
built, §8 how it is raised from the tree. Two principles cut across every type
here and are stated once:

- **Set where the logic says set, ordered where meaning demands order** (base
  §8.4, spec §7.1). A program is a set of statements; a rule body is a
  conjunction, hence a set; a disjunction and the elements of an aggregate are
  sets. A term's arguments, a comparison's guard chain, an optimization tuple,
  and a part's formal parameters are sequences, because their order is meaning.
  The type carries the shape: `BTreeSet` where the object is a set (so duplicates
  are unrepresentable, iteration is deterministic in `Ord`, and equality is set
  equality — two rules with the body written in two orders are one rule), `Vec`
  where it is a sequence.
- **The three negations are three types, and the vocabulary is precise.** *Strong
  (explicit) negation* is a `Sign` on an atom (`-p`, legal in a head or a body);
  *default negation* is a `DefaultNegation` on a body element (`not p`, `not not
  p`, body-only); the bitwise `~` is a `UnaryOp` on a term (§3.3). The two logical
  negations carry the ASP register deliberately, to keep two confusions out: this
  `-p` is **strong** negation, *not* classical-logic negation — `p ∨ -p` is no
  tautology and a program may assert neither, so "classical negation" (which
  carries the excluded middle `-p` does not) is avoided; and this `not p` is
  **default** negation in the stable-model sense (the Gelfond–Lifschitz reduct),
  *not* Prolog's operational negation-as-failure, so "NAF" is avoided. The API
  never spells default negation as Rust's `!` and never collapses `not not` into
  nothing — `not not p` is not `p` under the stable-model semantics, and a name
  that said so would lie (spec §1.4).

**One presentation convention governs every node type below, stated here once.**
Every structural node carries its provenance (§6), and a node is a *content*
value paired with the provenance carrier `WithProvenance<T>` of §6. The blocks in
this section define the **content** types; a program holds each node as
`WithProvenance<Content>`, and the typed accessors return the wrapped children so
provenance is reachable at every level. The consequence for the blocks: an
identity trait a block **derives is over the content**, which is exactly the
node's identity, because the carrier delegates `Eq`/`Ord`/`Hash` to the content
and **erases provenance** (§6.2) — so a derived `PartialEq` below always means
*content equality up to provenance*, the property §5.2's set semantics and the
merge of §6.3 rest on, never provenance-sensitive equality. Where a content type
has an invariant to guard (a comparison's non-empty chain, §4.6; a name's class,
§3.2) its fields are private behind a constructor and read through accessors;
where it has none, its fields are public — a struct literal is the most
declarative constructor (base §8.3). The pure value types that carry no
provenance — `Sign`, `DefaultNegation`, `Relation`, `AggregateFunction`,
`Direction`, and their kin — are not wrapped and derive normally.

### 4.1 Parts and the program

```rust
/// A part-structured set of statements, giving cheap part-wise access for
/// multi-shot use (spec §7.1). `base` is the implicit default part.
#[derive(Clone, Debug, Default)]   // Eq is structural over the parts (§5)
pub struct Program { /* private: BTreeMap<PartKey, Part> */ }

/// A part's identity: its name and the *spelled* formal parameters (grammar
/// §5.9's `#program name(p, q)`), not its arity. Two parts named `step(t)` and
/// `step(u)` therefore coexist rather than merge, because merging would rename a
/// formal and could capture a global constant — the classical variable-capture
/// failure of naive substitution. How same-named parts combine at ground time
/// is the solve tier's; this value records them distinctly.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PartKey { pub name: Name, pub formals: Vec<Name> }

#[derive(Clone, Debug)]   // Eq structural (§5)
pub struct Part { /* private: key: PartKey, statements: BTreeSet<Statement> */ }

impl Program {
    pub fn parts(&self) -> impl Iterator<Item = &Part>;      // in PartKey order
    pub fn part(&self, key: &PartKey) -> Option<&Part>;
    pub fn base(&self) -> &Part;                              // always present
    pub fn statements(&self) -> impl Iterator<Item = &Statement>;   // all, in order
}
impl Part {
    pub fn key(&self) -> &PartKey;
    pub fn statements(&self) -> impl Iterator<Item = &Statement>;
}
```

The `#program` directive (grammar §5.9) is **not** a `Statement`: it is a
positional delimiter in the tree, and this value lifts the delimiter into
structure (the part a statement belongs to) rather than reify an ordering the
set erases. Statements before any `#program` belong to `base`.

### 4.2 Statements

```rust
/// A statement of a part (grammar §5.11), plus the ASP-Core-2 query (grammar
/// §6.1). Non-exhaustive for downstream growth; every internal match is
/// exhaustive with no wildcard, so a new family is a compile error here, never
/// a silent drop.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]   // derives over content; §13-safe (the leaves are iterative)
#[non_exhaustive]
pub enum Statement {
    Rule(Rule),
    WeakConstraint(WeakConstraint),
    Optimize(Optimize),
    Show(Show), Project(Project), Defined(Defined), Edge(Edge),
    Heuristic(Heuristic), External(External), Const(Const),
    Include(Include),                  // parsed, never resolved (syntax §17)
    Script(Script),                    // carried opaque, never run (syntax §8.2)
    TheoryDefinition(TheoryDefinition),
    /// Grammar §6.1's query, under the ASP-Core-2 dialect: the class of forms a
    /// program position holds, so it belongs to this enum (syntax §8.2).
    Query(Query),
}
```

**Weak constraints and optimize statements are distinct, deliberately.** A `:~`
weak constraint and a `#minimize`/`#maximize` statement are two written forms of
optimization; the engine treats them alike, desugaring maximize to minimize by
**negating weights** and lowering both to the same aspif. That desugaring is
*answer-set-preserving but semantic* (it rewrites weights), so it is **not** a
structural normalization: it happens at the solve tier's lowering, never in this
value. Structural equality therefore distinguishes `:~ b. [w@p]` from
`#minimize { w@p : b }` and `#minimize` from `#maximize` — they are syntactically
different, which is exactly what structural equality reports (§5). This is one of
the two documented carve-outs where this tier's equality is *finer* than the
authority's own print-and-reparse (§5); the theory carve-out is the other.

### 4.3 Rules; facts and constraints as shapes

```rust
/// A rule: a head and a body (grammar §5.7). A *fact* is the shape "a single
/// literal head, an empty body"; a *constraint* is the shape "a falsum head".
/// One type, because a constraint **is** a rule (`⊥ ← body`) and a fact **is** a
/// rule (`h ← ⊤`); reifying three variants would tax every consumer with a
/// three-way match over what is one thing.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]   // over content (the §4 convention)
pub struct Rule { /* private: head: Head, body: Body */ }

impl Rule {
    pub fn head(&self) -> &Head;
    pub fn body(&self) -> &Body;
    pub fn is_fact(&self) -> bool;         // single-literal head, empty body
    pub fn is_constraint(&self) -> bool;   // falsum head
}
```

### 4.4 Heads

```rust
/// A rule head (grammar §5.5). `Falsum` is the head of a constraint — written
/// `:- body.` or `#false :- body.`, one head `⊥`; `Verum` is `#true`, which the
/// engine grounds.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]   // derives over content; §13-safe (the leaves are iterative)
pub enum Head {
    Literal(Literal),
    Disjunction(Disjunction),
    Choice(Choice),
    Aggregate(HeadAggregate),       // a head function aggregate, deriving atoms
                                    // (grammar §5.3); its elements carry the head
                                    // literal (§4.7); a head *set* aggregate is a
                                    // `Choice`, never this
    TheoryAtom(TheoryAtom),         // a head theory atom — unsigned (grammar §5.8)
    Falsum,
    Verum,
}
```

**One representation per head, so the round-trip law holds off the constructed
path.** `Head::Aggregate` admits only the *function* aggregate: a set form in a
head is a `Choice` (§4.4), so there is no second way to spell a head choice that
would render to `{…}` and re-raise unequal (§10). Likewise ⊤ and ⊥ in head
position have one form each — `Verum` and `Falsum` — and canonicalization (§5.1)
folds an un-negated `#true`/`#false` head-literal onto them, so a `#false.` head
and a `:- .` constraint head are one value. A directly-constructed duplicate
therefore cannot survive canonicalization to break `raise(parse(render(P))) == P`.

**`Choice` and `Disjunction` are distinct types, and the reason is
model-theoretic.** `a | b.` — a disjunction — has answer sets `{a}` and `{b}`;
`{ a; b }.` — a choice — has answer sets `∅`, `{a}`, `{b}`, `{a, b}`. The
distinction is not a syntactic nicety a flag could carry: an answer-set oracle
sees it, and it is the boundary between the disjunctive complexity class and the
choice construct. So the two are distinct types, each named for the object it
is, and a consumer walking a head meets `Choice` or `Disjunction`, never one
type it must interrogate. A head aggregate (a function aggregate deriving atoms)
is a third, distinct head; a head theory atom a fourth.

```rust
pub struct Disjunction { /* elements: BTreeSet<DisjunctionElement> */ }   // a | b | … (grammar §5.5)
pub struct Choice {
    /* left_guard: Option<Guard>, elements: BTreeSet<ChoiceElement>,
       right_guard: Option<Guard> */                                // 1 { a; b } 2 (grammar §5.3)
}
/// A disjunction or a choice element: a literal with an optional condition
/// (grammar §5.5, §5.3) — the singleton conditioned head `p(X) : q(X).` among
/// them. Named per parent, symmetric, as syntax §8.2 names its element classes.
pub struct DisjunctionElement { /* literal: Literal, condition: Condition */ }
pub struct ChoiceElement { /* literal: Literal, condition: Condition */ }
```

The set form `{ … }` is a `Choice` in a head and a cardinality aggregate in a
body — one syntax, two meanings by position (grammar §5.3); the raise (§8) knows
the position and builds the right one, so the value carries the meaning, never
the ambiguity.

### 4.5 Bodies

```rust
/// A rule body: a conjunction, hence a set (grammar §5.6). Its one filter axis is
/// the **default-negation partition** (the reduct's B⁺/B⁻, §4); strong negation is
/// a property of an atom, not of a body element (§4.6), so it is not a body axis.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]   // set equality; derives over content; §13-safe (leaves iterative)
pub struct Body { /* private: BTreeSet<BodyElement> */ }
impl Body {
    pub fn elements(&self) -> impl Iterator<Item = &BodyElement>;
    pub fn is_empty(&self) -> bool;
    /// The **positive/negative partition** — B⁺/B⁻ of the Gelfond–Lifschitz reduct
    /// (and its generalizations for aggregates and `not not`): `positive` are the
    /// elements *not* under `not`; `negative` are those that are, over every element
    /// kind (a `not`-ed aggregate is in B⁻). "Negative" here is *default* negation —
    /// the axis every element carries at its own top level — not `Sign::Negative`,
    /// the strong negation that lives on an atom (§4.6). Strong negation is a
    /// property of an atom, not of a body element; a query that needs it — with the
    /// descent choice it implies for conditionals and aggregates — is the consumer's
    /// to compose from `elements()` and a literal's own `sign` (§3.7), or the deep
    /// traversal (§3.6).
    pub fn positive(&self) -> impl Iterator<Item = &BodyElement>;
    pub fn negative(&self) -> impl Iterator<Item = &BodyElement>;
}

/// The two-tier body (grammar §5.6). A literal and a conditional literal are one
/// tier; an aggregate and a theory atom, which may stand **only** at
/// body-element position, are the other, carrying their own default negation.
/// Keeping the aggregate and the theory atom out of `Literal` is what makes
/// `p(X) : #count{…}` — an aggregate where a literal is required —
/// **unrepresentable**, rather than a value that builds and then fails at the
/// engine.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum BodyElement {
    Literal(Literal),                                          // grammar §5.2
    Conditional(ConditionalLiteral),                          // grammar §5.4
    Aggregate { negation: DefaultNegation, aggregate: Aggregate },
    TheoryAtom { negation: DefaultNegation, atom: TheoryAtom },
}

/// `not`, `not not` (grammar §5.2, §5.6). `NotNot` is its own case because
/// double default negation is not the identity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DefaultNegation { None, Not, NotNot }
```

### 4.6 Literals, atoms, comparisons, conditions

```rust
/// A literal (grammar §5.2): a default-negation prefix over an atom, a
/// comparison, or a boolean constant.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Literal { pub negation: DefaultNegation, pub inner: LiteralInner }
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LiteralInner { Atom(Atom), Comparison(Comparison), True, False }

/// An atom (grammar §5.2): a strong sign, a predicate name, and arguments.
/// The sign is strong negation — `-p`; the default negation is the literal's.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Atom { pub sign: Sign, pub name: Name, pub arguments: Vec<Term> }

/// A comparison chain (grammar §5.2): a first term and **one or more**
/// relation/term steps. `1 < X < 5` is one literal carrying a guard sequence,
/// not a conjunction — the chain is the shape. The at-least-one-step invariant
/// is guarded (fields private behind the constructor), so an empty chain — a
/// term with no relation, which grammar §5.2 does not admit — is unrepresentable.
pub struct Comparison { /* private: first: Term, steps: Vec<(Relation, Term)> — steps non-empty */ }
impl Comparison {
    pub fn new(first: impl Into<Term>, relation: Relation, second: impl Into<Term>) -> Comparison;
    /// Extend the chain by one further relation/term step (`… < 5`).
    pub fn chain(self, relation: Relation, term: impl Into<Term>) -> Comparison;
    pub fn first(&self) -> &Term;
    pub fn steps(&self) -> impl Iterator<Item = (Relation, &Term)>;   // one or more
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Relation { Lt, Le, Gt, Ge, Eq, Neq }

/// A condition (grammar §5.4): the literals after a `:`. Present and empty when
/// the colon is (`p : .`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Condition { /* private: Vec<Literal> — a sequence, grammar §5.4 */ }

/// A conditional literal (grammar §5.4): a literal under a condition.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConditionalLiteral { pub literal: Literal, pub condition: Condition }
```

### 4.7 Aggregates and optimization

```rust
/// An aggregate (grammar §5.3): in a body a negatable element (§4.5), in a head a
/// `Head::Aggregate`. A body aggregate's elements *test*; a head aggregate's *derive*,
/// so a head element carries the literal it derives and a body element does not — two
/// **distinct types**, `FunctionAggregate` in a body and `HeadAggregate` in a head, so
/// a body aggregate holding a head element (or the reverse) is unrepresentable, one
/// more of §4.5's invalid states the type forbids rather than deferring to the engine.
/// They are two concrete types, not one generic over the element, so an aggregate
/// stays a plain structural node like `Choice` and `Disjunction` and the taxonomy's
/// regularity holds; the guard-and-function structure they share is small, and
/// `HasGuards` reads the two guards for either.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Aggregate { Function(FunctionAggregate), Set(SetAggregate) }

/// A body function aggregate: two guards, the function, and body elements that test.
pub struct FunctionAggregate {
    /* left_guard: Option<Guard>, function: AggregateFunction,
       elements: BTreeSet<BodyAggregateElement>, right_guard: Option<Guard> */
}
/// A head function aggregate (grammar §5.3): the same two guards and function over
/// head elements that derive. `Head::Aggregate(HeadAggregate)` (§4.4).
pub struct HeadAggregate {
    /* left_guard: Option<Guard>, function: AggregateFunction,
       elements: BTreeSet<HeadAggregateElement>, right_guard: Option<Guard> */
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AggregateFunction { Count, Sum, SumPlus, Min, Max }
pub struct SetAggregate { /* guards + BTreeSet<SetElement> — the body cardinality form */ }

/// The position-specific aggregate elements (grammar §5.3). A body element is a term
/// tuple under a condition — it tests, so it carries no head literal. A head element
/// adds the literal it derives; that literal is what makes it a *head* element and it
/// exists only here, so a `FunctionAggregate` cannot hold one.
pub struct BodyAggregateElement { /* terms: Vec<Term>, condition: Condition */ }
pub struct HeadAggregateElement { /* terms: Vec<Term>, literal: Literal, condition: Condition */ }

/// A guard (grammar §5.3): a relation and a term; `None` relation is the
/// grammar's default (`<=` on its side), stated as absence because that is what
/// the author wrote.
pub struct Guard { pub relation: Option<Relation>, pub term: Term }

/// A weight at an optimization priority level (grammar §5.7's `weight@priority`): a
/// weight term and an optional priority term, its absence the default level 0. **One
/// value**, because `weight@priority` is written and meant as one thing — how much a
/// term tuple contributes, and at which level — and it is identical in a weak
/// constraint and an optimize statement, the two written forms of optimization (§4.2).
/// Built `weight(w).at_priority(p)`, so a construction reads as the `weight@priority` it
/// denotes rather than a bare `Term` weight beside a loose `Option<Term>` priority a
/// reader must reassemble — the meaning lives in the value, not in argument position.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Weight { /* term: Term, priority: Option<Term> */ }
pub fn weight(term: impl Into<Term>) -> Weight;             // at the default level 0
impl Weight {
    pub fn at_priority(self, priority: impl Into<Term>) -> Weight;   // raise to `weight@priority`
    pub fn term(&self) -> &Term;
    pub fn priority(&self) -> Option<&Term>;                // absent = the default level 0
}

/// Optimization by `#minimize`/`#maximize` (grammar §5.7). The direction is a
/// tag; the maximize-to-minimize desugaring is the solve tier's (§4.2), so it is
/// kept structural here and `i32::MIN` never overflows.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Optimize { pub direction: Direction, /* elements: BTreeSet<OptimizeElement> */ }
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Direction { Minimize, Maximize }
pub struct OptimizeElement {
    /* weight: Weight, terms: Vec<Term>, condition: Condition */
}

/// A weak constraint (grammar §5.7): a body and a bracket of a weight at a priority
/// and a term tuple. Distinct from `Optimize` (§4.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WeakConstraint {
    /* body: Body, weight: Weight, terms: Vec<Term> */
}
```

### 4.8 Directives

The remaining directives (grammar §5.9) mirror their productions; each is a
plain owned struct with the accessors its grammar names, and the decisions in
them are two.

- **`Const`** carries `name`, a `value` in the constant-term subset (grammar
  §5.9 — no variables, pools, or intervals; arithmetic and `@`-calls admitted),
  and an optional `ConstPolicy { Default, Override }`. The value is a `Term`,
  **not** a pre-evaluated `Symbol`: this tier does not evaluate (§3.5), so
  `#const x = 1+2.` and `#const x = 3.` are structurally distinct, their
  equivalence a semantic fact this value does not assert. A consumer that wants
  the denoted symbol calls `evaluate` (§3.5).
- **`Show`** has the four forms of grammar §5.9 (`#show.`, a signature, a term, a
  term under a body); **`Project`**, **`Defined`**, **`Edge`**, **`Heuristic`**,
  **`External`**, **`Include`**, and **`Script`** follow their productions.
  `Signature { sign: Sign, name: Name, arity: u32 }` is shared where the grammar
  shares it (`#show`, `#project`, `#defined`). `Script` carries the language name
  and the body text verbatim — an opaque region this tier never parses or runs
  (syntax §8.2); `Include` carries its target and is never resolved (spec §6.8,
  syntax §17: no I/O in this tier).

### 4.9 Theory atoms and definitions

Theory atoms parse grammar-generically (grammar §5.8, spec §6.1): admission of a
theory atom against a `#theory` definition is *not* this tier's — that is a
concern above, for a consumer that reads the definitions (spec §6.1). This value
represents the theory atom and its definition structurally, so an admitting
consumer has them.

```rust
/// A theory atom (grammar §5.8): a name, optional arguments (ordinary terms),
/// optional elements, and an optional guard. In a body it carries default
/// negation (§4.5).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryAtom {
    /* name: Name, arguments: Vec<Term>, elements: BTreeSet<TheoryElement>,
       guard: Option<TheoryGuard> */
}
```

**Theory terms are a distinct peer algebra.** The theory-term grammar (grammar
§5.8) admits operators without precedence and its own bracketed forms; it is
*not* the term algebra of §3, and conflating them would let a theory term stand
where an ordinary term must. The two meet only at their shared leaves — a
variable and a ground symbol — with a lift between them. A `TheoryTerm`'s
operator structure is the flat sequence the grammar admits; how a `#theory`
definition regroups it is admission, above (grammar §5.8). One consequence for
§5: because a build-door theory term (already grouped) and a raised theory term
(flat) reconcile only under a definition, theory-bearing programs carry the
second of the two equality carve-outs (§5) — their equality is *canonical
up-to-grounding*.

**Computational cost.** Construction of a statement is `O(its size)`; part-wise
access is `O(log parts)` for a named part and `O(1)` amortized for `base`; the
set inserts that build a body or a disjunction are `O(log n)` each; clone is
linear; equality, ordering, and hashing are structural, set-aware, and iterative
in depth (§13).

## 5. Canonicalization and equality

### 5.1 Canonicalization is a pass

The term algebra is a public enum a consumer matches on (§3.3, and the
transformation and analysis clients need it), so a caller *can* build a
non-canonical term — a ground `f(1)` as `Function { f, [Symbolic(1)] }` rather
than the collapsed `Symbolic(f(1))`. No constructor can forbid this by itself,
because the enum is open to direct construction. Canonicalization is therefore a
**pass**, idempotent and total, run at every door that admits a value into a
statement or a program and by every smart constructor (§7), so that **every value
reachable through a `Program` is canonical** — the one non-canonical value a
consumer can hold is a raw term it built and never passed through a door, and
`Term::canonicalize` is provided for it.

```rust
impl Term { pub fn canonicalize(self) -> Term; }        // idempotent, iterative (§13)
impl Symbol { /* symbols are canonical by construction */ }
```

Canonicalization does the following, each syntactic:

- **Ground collapse.** A maximal ground constructor term — a function, tuple, or
  ground leaf whose subterms are all ground — becomes a `Symbolic(Symbol)`, the
  same normalization the authority performs (grammar §5.10). It does **not** fold
  an operator: a ground `1+2` stays a `BinaryOperation` (§3.5), because folding
  it would evaluate, which this tier does only at the explicit door.
- **Degenerate forms.** A one-alternative pool is its term (`(a)` is `a`, grammar
  §5.1); the grammar's other degeneracies (an empty tuple, a one-element tuple)
  are kept, because the grammar makes them distinct terms.
- **Boolean heads.** An un-negated `#true`/`#false` head-literal is folded to
  `Head::Verum`/`Head::Falsum` (§4.4) — the one form of ⊤/⊥ in head position, so a
  `#false.` head and a `:- .` constraint head are one value; the fold is checked
  against the authority's printing (§16). A *negated* boolean head (`not #false`)
  is kept as its literal, having no `Verum`/`Falsum` counterpart.
- **Set order.** The set-shaped children (§4) are held in `BTreeSet`, so their
  order is `Ord` and duplicates are gone — this is set membership, not a pass, but
  it is what makes a body written in two orders one value.

Three normalizations are **deliberately not** performed, because each erases a
distinction the author wrote and none is forced by set semantics: the direction
of a two-sided aggregate bound, the direction of a comparison, and the spelling
of a part's formal parameters. Equality reports them as written.

### 5.2 Canonical-syntactic equality, named

The `Eq` a `Program` carries is **canonical-syntactic equality**: two programs
are equal when their canonical forms are the same set of rules, syntactically,
up to provenance (§6). It is named so because it is neither of the equivalences
an ASP practitioner means by the word:

- It is **not** ordinary equivalence (the same answer sets) nor strong
  equivalence (in the Lifschitz–Pearce–Valverde sense). Both are hard decision
  problems this tier deliberately does not conflate with `Eq`; semantic
  equivalence checking is a named reserved seam (spec §7.1, §13).
- It is **strictly finer** than ordinary equivalence: `P == Q` implies
  `AnswerSets(P) = AnswerSets(Q)`, but not conversely. `{ p :- q. q :- p. }` and
  the empty program have the same single answer set `∅` — they are ordinarily,
  indeed strongly, equivalent — yet their canonical forms differ, so they are not
  equal here, and that is correct: the value records the rules, and the rules
  differ.

**The arbiter, and its two carve-outs.** On the theory-free, optimization-free
fragment, this equality **coincides with the authority's own parse-then-unparse
equality** — no finer, no coarser — which is what makes it checkable against the
pinned binary (§16). Two carve-outs where it is deliberately *finer*, each stated
at its home and repeated here so the relation is exact:

1. **Optimization** (§4.2): the authority prints `#minimize`, `#maximize`, and a
   weak constraint alike, folding across them by negating weights — a *semantic*
   normalization. This tier keeps the three forms distinct, so its equality is
   finer there.
2. **Theory-bearing programs** (§4.9): a built (grouped) theory term and a raised
   (flat) theory term reconcile only under a `#theory` definition, so for programs
   bearing theory atoms this equality is *canonical up-to-grounding* — a sound
   under-approximation. A consumer that builds an equality oracle on the universal
   claim, ignoring this, has built something unsound for theory-bearing programs.

**α-equivalence of formals is not performed** (§4.1): `#program step(t)` and
`#program step(u)` are distinct, the authority-faithful reading (renaming a
formal could capture a global constant). And equality is **up to provenance**:
the provenance a node carries (§6) is erased from `Eq`, `Ord`, and `Hash`, so two
programs that differ only in where their rules were parsed from, or which
transformation produced them, are equal.

**Why the order must agree with equality, stated as the standing precondition.**
Every set in §4 is a `BTreeSet` keyed on `Ord`, and the provenance merge (§6)
fires exactly when two content-equal statements collapse in such a set. If `Ord`
and `Eq` disagreed — if `Ord` distinguished two values `Eq` calls equal — the set
would hold both as distinct keys, the collapse would silently never fire, and a
program's provenance and its canonical form would both be wrong. So `Ord`, `Eq`,
and `Hash` are the *same* content projection at every level, hand-written
together and checked against each other and against a derived twin by the mirror
differential (§16).

## 6. Provenance and annotations

Every node of a program can carry origin — where it was parsed from, that it was
constructed, the transformation that produced it — and the tool or modeler
annotations attached to it. This is the durable-identity attachment point at
this tier (spec §7.4): it is what lets a program-level diagnostic point at source
precisely, and it is the ground the explanation, blame, and contract-extraction
consumers stand on (spec §7.4, §2).

### 6.1 In-node, not a side table

Provenance is **in-node model data**, not a table keyed by node identity (spec
§7.4). The argument is transformation: a rewritten node gets a new identity, so a
side table's keys go stale and the table cannot follow `Program → Program`
transformation (§9), while an in-node field composes through every rewrite by
construction. It rides **every structural node** — statements, rules, heads and
their elements, bodies and their elements, atoms, literals, comparisons,
aggregates and their elements, guards, directives, optimize elements, theory
atoms and their elements — at the granularity a blame path and an explanation
need, which is the atom and the rule, not the sub-atomic term. It does **not**
ride `Term` or `Symbol`: the term algebra stays the clean, origin-free value of
§3 — the depth discipline (§13) walks it without a wrapper, and it is the value
that crosses the extension surfaces — and a term's source, when a tool needs it,
is the span of the atom or comparison that carries it.

### 6.2 The carrier, and erasure from identity

```rust
/// A node's provenance: a set of origin facts and a set of annotations, merged
/// by union. Empty is the identity; merge is idempotent, commutative, and
/// associative — a bounded join-semilattice — which is what lets a content-equal
/// collapse (§5) *union* both nodes' provenance rather than keep one arbitrarily.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Provenance { /* origins: BTreeSet<Origin>, annotations: Annotations */ }

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Origin {
    Parsed(Location),        // a span in a source (base §4.3) — the blame workhorse
    Constructed,             // built through the constructors (§7)
    Transformed(TransformTag),   // produced by a named transformation (§9)
}

/// Tool and modeler annotations. Non-exhaustive, typed kinds — a documentation
/// string (from a `%!` doc comment at the raise, §8), a label, a reference, a
/// trace directive an explanation tool attaches (§2). Each kind is a set,
/// unioned on merge.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Annotations { /* private: doc, label, reference, trace — sets */ }
```

**Erasure is written once.** A structural node carries its `Provenance` through a
single carrier whose `PartialEq`, `Eq`, `PartialOrd`, `Ord`, and `Hash` delegate
to the *content* and ignore the provenance:

```rust
/// A structural node and its provenance. Identity is the content's; provenance
/// is erased — so equality is up to provenance (§5) and a set dedupes by content.
/// The one place this erasure is written, so it cannot drift per node (a node
/// whose `Ord` erased provenance but whose `Eq` did not would break every set).
pub struct WithProvenance<T> { /* value: T, provenance: Provenance */ }
impl<T> WithProvenance<T> {
    pub fn new(value: T, provenance: Provenance) -> WithProvenance<T>;
    pub fn constructed(value: T) -> WithProvenance<T>;                 // Origin::Constructed
    pub fn get(&self) -> &T;
    pub fn provenance(&self) -> &Provenance;
    pub fn into_value(self) -> T;                                      // the owned complement to `get`
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithProvenance<U>;  // rewrite content, provenance carried (§9.1)
}
// PartialEq/Eq/PartialOrd/Ord/Hash for WithProvenance<T> delegate to `value`.
```

Per the §4 convention, a block's name (`Rule`, `Atom`, …) names the *content*
type, and the node a program holds is `WithProvenance<Rule>`,
`WithProvenance<Atom>`, and so on; their public accessors read the content, and
`.provenance()` reads the origin and annotations. Because the carrier's identity
is the content's, the
`Ord`/`Eq`/`Hash` agreement §5 requires holds by construction, and the merge below
is the *only* code that reads provenance during a set operation.

### 6.3 Merge, through one door

When a statement enters a part's `BTreeSet` and an equal statement is already
present, the two are **one content** with **two provenances**, and the set holds
the content once with the provenances **unioned**. This merge is performed by the
single ingest door every construction and every transformation routes through — a
program is never assembled by a raw set insert that skips it — so the preservation
law is structural, not a discipline that could be forgotten:

> `provenance(canonicalize(P))` is the **union** of the provenances of the rules
> merged into each canonical rule — *equality* in both directions: nothing
> load-bearing is lost, and nothing is fabricated.

The equality (not mere containment) is the safety half: a consumer that maps a
node's references back to their sources — an explanation tool citing a rule's
origin, a governance tool citing a regulation reference — must never be handed a
*fabricated* origin, so the law forbids inventing one as firmly as it forbids
dropping one. Idempotence is what the safety half leans on when the same
reference recurs across content-equal rules.

**Computational cost.** A node's provenance is one optional heap allocation — the
common no-provenance case is a null pointer, so a program with provenance nowhere
costs nothing per node — and a set of small facts otherwise, `O(facts)` per node
and linear overall (spec §7.4's small constant per node). Merge is `O(facts)` per
collision. Equality, ordering, and hashing skip provenance entirely, so carrying
it never changes their cost or their result.

## 7. Construction: two doors, one authority

A program is built one of two ways, and the design rule is that both reach a
`Program` through **one well-formedness authority**, so a program a human writes
and a program a machine assembles are validated identically and compare with
structural equality (spec §7.3; the *first-solve* witness, spec §3):

- **Spelled-out constructors** — typed Rust values in, a program value out. The
  declarative surface below.
- **The raise** (§8) — ASP concrete syntax through the syntax tier's parser, then
  lowered. This is the *only* text-to-program path, and it exists because the
  syntax tier exists: there is **no second parser of ASP syntax** in this tier,
  no bespoke fragment reader that would become a second grammar (spec §2 item 3,
  §5.2). The macro tier (spec §8) is a third *surface* over the first two — its
  expansions are constructor calls and its ASP fragments go to the real parser
  (grammar §9) — never a third representation.

### 7.1 The declarative surface

The register is a *declaration of the logic*, not an imperative assembly of
nodes: the shape of the Rust expression mirrors the shape of the rule. The
surface that achieves this, on representative constructs:

```rust
// Strong vs arithmetic negation are two operators, and the type picks the
// meaning: `-atom` is strong (an atom with Sign::Negative), `-term` arithmetic.
impl Neg for Atom { type Output = Atom; }   // -p(X)
impl Neg for Term { type Output = Term; }   // -(X + 1)

// Default negation is a word, role-typed so illegal placement is a compile error
// (§4.5): `not` of an atom is a body literal; `not` of an aggregate is a body
// element; there is no `not` that yields a value a literal position accepts.
pub fn not<T: Negatable>(x: T) -> T::Output;
pub fn not_not<T: Negatable>(x: T) -> T::Output;   // `not not`, not the identity

// A rule reads as the rule: a head, and the body it holds when.
impl Head { pub fn when(self, body: impl IntoBody) -> Rule; }    // reach(X,Z).when([reach(X,Y), edge(Y,Z)])
impl Rule {
    pub fn fact(head: impl IntoHead) -> Rule;                    // p(1).
    pub fn constraint(body: impl IntoBody) -> Rule;             // :- body.
}

// Arithmetic and intervals compose as written, over anything term-shaped.
impl Add for Term { /* X + 1 */ }  // and Sub, Mul, …; `a.to(b)` for a .. b
```

Three properties make this the *one obvious way* rather than a menu (the criterion
of §2):

- **Coercion, not variants.** `impl Into<Term>`, `impl IntoHead`, `impl IntoBody`
  let a constructor accept what the caller already holds — a name, a number, an
  atom — so the common case needs no wrapping. Coercions widen the *one* obvious
  spelling; they never add a second. There is no typed-empty sentinel: the nullary
  and empty cases have their own named constructors (`Atom::constant`,
  `Body::empty`), so a simple thing stays simple.
- **A spelled-out register for the logic, terse Rust for the plumbing.** The
  ASP-facing vocabulary is spelled out — `minimum`, not `min`; `Sign::Negative`,
  not a bare bool — because it is the reader's domain; general Rust machinery
  follows Rust idiom. Names departing the literature owe a reason (spec §1.4);
  none here does.
- **Role, not shape.** The conjunction that is a rule body, a condition, and a
  scenario is one *type* but three *roles*; the constructors name the role, so
  `constraint(b)` and a scenario over the same `b` read as the different things
  they are (their model sets are disjoint), not as one shape reused.

### 7.2 Refusal only where failure is real

Totality with typed refusals (spec §5.2, §7.3), placed by the criterion of §2 —
the obvious path is the safe path, and `Result` appears only where failure is
genuinely possible:

- A constructor that **composes already-valid typed values** is **total**:
  `Rule::new(head, body)` cannot fail, because a `Head` and a `Body` are already
  well-formed; a `?` there would be ceremony over a case that cannot arise.
- A constructor that **ingests raw, unvalidated data** **refuses**: `Name::new`
  from a `&str` returns `Result<Name, NotAnIdentifier>` (§3.2); a rounding adapter
  from an `f64` returns `Result<Symbol, NotAnInteger>` (§3.4). The refusal is the
  question the caller can fix, carried as a value (the offending text or number),
  never a rendered string (base §6.5, spec §1.5).

Lexical name classes are enforced at exactly these raw-data doors, once, by the
syntax tier's classifier (§3.2) — the one well-formedness authority for names,
shared so no second definition exists. Canonicalization (§5.1) runs eagerly in
every constructor, so the ergonomic path never yields a non-canonical value.

### 7.3 The two audiences, and exceeding the comparators

The surface serves two audiences with **one value between them**:

- **The ASP author** writes the logic — the declarative constructors above, or
  the macros (spec §8) that read as ASP, or simply *writes ASP* and raises it
  (§8). All three are ASP because they go through the one grammar.
- **A programmatic client** — a generator, an editor, a language model — targets
  the typed algebra directly: predictable, uniform, invalid states
  unrepresentable, and typed refusals as the correction signal. Where such a
  client benefits from a *regular, uniform* spelling over a human-terse one, that
  regularity is its obvious way and is kept; the criterion is *one obvious way per
  audience*, not one spelling for both.

Both converge on a structurally-equal `Program` (the *first-solve* witness). This
is where the tier **exceeds the evidenced comparators** (spec §2 item 10, §3.1),
and the differences are checkable, not asserted: construction here carries **no
mandatory location** — provenance is optional in-node data and equality is up to
it (§6), where the comparator's AST demands a location on every node; invalid
states are **unrepresentable** (`p(X) : #count{…}` does not compile, §4.5), where
the comparator discovers the error at ground time or never; and every ingest door
**refuses with a typed value**, where the comparator's surface is duck-typed. The
same task rendered side by side is stricter, clearer, and diagnostic-superior —
the obligation spec §3.1 sets, met at construction.

### 7.4 The macro tier: sugar over these constructors

The surface macros live in the macro tier (`themelios-macros`) — **not this
tier's**, a later tier in the build order (spec §11) with its own design of record.
They divide by what they front. The **construction macros** — `atom!`, `fact!`,
`rule!`, `constraint!`, `minimize!`, `maximize!`, `show!`, `external!`, `scenario!`
(spec §8) — client *two* lower tiers at once: each expands to the public
constructors above (spec §8's law that a macro adds no representation, only a
spelling) and hands any ASP fragment it reads to the real parser through the macro
dialect (grammar §9), never a bespoke reader. The **`#[derive(Extract)]` and
`#[external]` attributes** (spec §8, §3.2) front different surfaces: extraction
expands over the conversion pillar (§3.4) and the point accessors (§3.7), reading a
`Symbol` rather than parsing text; registration expands to `@`-function
registration (spec §9.6). All are named here so the absence is deliberate, not an
omission: the sugar a reader expects to meet in this section lives one tier up,
resting on this one.

What this tier owes that one, and provides, is the foundation those surfaces stand
on: the **constructors** the construction macros expand to (§7.1); the **`raise_term`
/ `raise_statement` doors** (§8) their parsed fragments lower through; the
**conversion pillar** (§3.4) and **point accessors** (§3.7) the extraction attribute
reads through; and the **first-solve witness** (§16) that holds "built through the
macros" and "built through the constructors" structurally equal — the proof a
construction macro is only sugar. The discipline that keeps this foundation fit is a
plain one: a macro or attribute that cannot reduce to a *trivial* expansion is a
signal that the surface beneath it has a gap — a missing constructor, accessor, or
conversion — not that the macro wants cleverness, so what this tier exposes stays
complete enough to be the target. (The attributes attach later still, with the
extraction and registration surfaces they front; the macro tier's vocabulary
accretes as its enablers land.)

## 8. The raise

The raise is the parsing relation of §1: it lowers the syntax tier's typed AST
(syntax §8) into a `Program`. It is the second construction door and the one
text-to-program path.

```rust
/// Lower a parsed program to a `Program`, under the parse's own dialect. Total:
/// every parse yields a program and a set of lowering diagnostics; a statement
/// the parse could not complete (an error or missing node under recovery, syntax
/// §6.7) is diagnosed and skipped, and the well-formed statements around it still
/// raise — the per-statement resilience an editor-class consumer needs.
pub fn raise(parse: &Parse<ast::Program>) -> Raised;

/// Lower a single parsed statement or term fragment (syntax §6.1). These are the
/// doors the macro tier expands to — it parses an ASP fragment through the one
/// grammar and lowers it here (spec §8), never a second parser — and `raise_term`
/// is the middle step of the parse-a-string-to-a-symbol path (§3.5):
/// `parse_term_value` → `raise_term` → `Term::evaluate` → `Symbol`. `None` when
/// the fragment held no statement or term under recovery; the diagnostics ride
/// alongside as they do for `raise`.
pub fn raise_statement(parse: &Parse<ast::StatementFragment>) -> (Option<Statement>, Vec<LowerError>);
pub fn raise_term(parse: &Parse<ast::TermFragment>) -> (Option<Term>, Vec<LowerError>);

#[derive(Clone, Debug)]
pub struct Raised { /* private: program: Program, diagnostics: Vec<LowerError> */ }
impl Raised {
    pub fn program(&self) -> &Program;
    pub fn diagnostics(&self) -> &[LowerError];      // in source order; base's canonical_order sorts
    pub fn into_program(self) -> Program;
}

/// A lowering diagnostic, located by construction and lowering to base's normal
/// form (base §6.5) — the syntax tier's diagnostics and these share one model, so
/// a consumer renders both alike.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LowerError { /* kind + Location (base §4.3), namespace `program` */ }
impl ToDiagnostic for LowerError { /* … */ }
```

**Dialect-correct by construction.** A string literal's value differs by dialect
(grammar §4.4 against §6.2, syntax §3), so the raise reads it through
`Parse::string_value` (syntax §5.5) — the door that cannot be handed the wrong
dialect — never through the free accessor that can. The dialect reaches the raise
from the parse it is given; a consumer never states it twice.

**Refuse with a span, at the mistake.** Where a raise cannot proceed — a
recovered statement it cannot complete, or a form the grammar admits under
recovery but the value cannot represent — it emits a `LowerError` with the
offending span, at the rust-analyzer bar (spec §2 item 9), and continues. A bad
name never reaches here (the lexer guaranteed the token's class, §3.2), so name
refusal is the spelled-out door's concern (§7.2); the raise's diagnostics are
about structure the parse left incomplete.

**The corners it settles, each read from the tree, none re-derived.** The `-p`
ambiguity is positional and the tree already resolved it (§3.3, syntax §8.2); a
comparison chain becomes one `Comparison` (§4.6); a set form becomes a `Choice`
in a head and a cardinality `Aggregate` in a body by the position the tree
records (§4.4); a `#const` value is checked against the constant-term subset
(grammar §5.9) and carried as an unevaluated term (§4.8); a maximal ground
constructor term is collapsed by canonicalization (§5.1); and a statement's
leading doc comments (grammar §5.11, syntax §8.2) become a `Doc` annotation on the
raised statement (§6), so documentation rides the rule it documents. Every raised
node carries `Origin::Parsed(location)` (§6), so a program-level report points
back at source.

**Computational cost.** The raise is `O(tree)` in time and in the size of the
program it produces — a single iterative walk of the tree (§13), the parse's tree
being finite in its input, so no memoization is owed here (the shared-structure
concern is the solve tier's, where symbols are read *from* an engine, not built
from a tree). Its diagnostics are `O(recovered nodes)`.

## 9. Transformation

`Program → Program` (and `Rule → Rule`) as pure functions, with the visitor and
rewriter utilities a transformation-class consumer needs and provenance carried
through (spec §7.5). This is the second load-bearing foundation of the tier: an
explanation tool's cause-tracking rewrite, a probabilistic translation to ASP,
and a non-ground optimizer's answer-set-preserving passes are all clients of it
(§2), so it is designed as a first-class surface, not a convenience.

### 9.1 The visitor and the rewriter

```rust
/// A read-only walk — for analysis and collection. Each method defaults to
/// recursing; a consumer overrides the kinds it reads. Iterative in depth (§13).
pub trait Visit {
    fn visit_term(&mut self, term: &Term) { /* default: descend */ }
    fn visit_atom(&mut self, atom: &Atom) { /* … */ }
    fn visit_rule(&mut self, rule: &Rule) { /* … */ }
    // … one per structural kind.
}
pub fn visit(program: &Program, v: &mut impl Visit);

/// A `Program → Program` rewrite. Each method defaults to descending and
/// rebuilding; a consumer overrides the kinds it rewrites. Every node the
/// rewrite replaces carries `Origin::Transformed(tag)` unioned with the input
/// node's origin (§6), so a rewritten rule traces back to the rule it came from —
/// the *transformation* witness (spec §3): provenance reaches origins, and a
/// diagnostic on a transformed rule points at source. Total and iterative (§13).
pub trait Rewrite {
    fn tag(&self) -> TransformTag;
    fn rewrite_term(&mut self, term: Term) -> Term { /* default: descend */ }
    fn rewrite_atom(&mut self, atom: Atom) -> Atom { /* … */ }
    fn rewrite_rule(&mut self, rule: Rule) -> Rule { /* … */ }
    // … one per structural kind.
}
pub fn rewrite(program: Program, r: &mut impl Rewrite) -> Program;
```

The term-level rewrites are written in the iterative `fold` of §3.6, so a rewrite
over a deep term is stack-safe; the structure-level rewrites descend by iteration
over the grammar-bounded layers (§13). Canonicalization (§5.1) runs on a rewrite's
output, so a transformation cannot leave a program non-canonical, and the
identity rewrite is the identity function up to a `Transformed` tag no equality
reads (§6).

### 9.2 Substitution and fresh names

Two operations every rewriting client needs, and the substitution is the *same*
machinery the unifier produces (§11) — one substitution core, two families of
client (the query side binds; the transform side rewrites):

```rust
/// Apply a substitution (§11) to a term, a rule, or a program — **resolving**: each
/// variable is replaced by its binding and the binding's own variables are followed
/// to the fixpoint (the substitution is triangular, §11.1), provenance preserved.
/// The workhorse of projection, inlining, and instantiation. Cost is `O(output)` —
/// proportional to the *resolved* result, which a pathological unifier can make
/// exponentially larger than its input (§11.1); that exponential is an output-size
/// fact, not an algorithm defect, and interning (§17) is the seam that shares the
/// structure where a consumer needs the materialisation cheap.
pub fn substitute(rule: Rule, substitution: &Substitution) -> Rule;
impl Term { pub fn substitute(self, s: &Substitution) -> Term; }

/// A source of fresh variables and fresh predicate names that collide with none
/// already in a program — what rename-apart (§11) and an optimizer's auxiliary
/// predicates both draw from.
pub struct Fresh { /* private */ }
impl Fresh {
    pub fn over(program: &Program) -> Fresh;   // seeded to avoid the program's names
    pub fn variable(&mut self) -> Variable;
    pub fn predicate(&mut self, hint: &str) -> Name;
}
```

### 9.3 The boundary this surface does not cross

A transformation here is **structural**: it produces a new program value and
carries provenance. It does **not** verify that the rewrite preserves answer sets
— that is *semantic* equivalence, ordinary or strong, the reserved seam (spec
§7.1, §13, and the boundary drawn once in §2). An optimizer's passes are
answer-set-preserving because *their author* proves them so and the *solve tier's
differential* checks them, not because this tier certified anything; the
certificates this tier issues are structural (structural equality, and the syntax
tier's token-stream certificates, syntax §11). This is the honest line that lets
the tier *found* an optimizer and an explainer without *claiming their semantics*:
"transforms `P` into `Q`, carrying provenance" is the whole of what it promises.

## 10. Rendering

`Program → concrete syntax`, canonical and deterministic, round-trippable (spec
§7.6). The foundation renders correctly and legibly; styled layout — line
breaks, alignment, spelling normalization, precedence-minimal parenthesization —
is the formatter satellite's art (spec §13), and this renderer takes none of it.

```rust
/// Render a program to concrete syntax under a dialect (the dialect decides a
/// string value's spelling, grammar §4.4/§6.2). Canonical: the same program
/// renders the same text, every time. Total and iterative in depth (§13) — a
/// single work-list walk down the whole spine, statements to terms.
pub fn render(program: &Program, dialect: Dialect) -> Result<String, Unspellable>;

/// The one refusal: a string symbol whose value has no spelling under the chosen
/// dialect (grammar §9's owned gap — a macro splice can build a string value
/// grammar §4.4 cannot spell). The caller states the dialect or the value the
/// gap names; nothing is silently mangled.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unspellable { pub value: String, pub dialect: Dialect }
```

**Canonical form.** Binary operators and intervals render fully parenthesized
(`(X + 1)`, `(1 .. 3)`), so the text carries the tree's grouping with no
precedence to re-derive on the way back — the simple canonical choice, which
round-trips by construction; a nullary function renders bare (`a`); a tuple with
its parentheses and the grammar's trailing comma where it distinguishes (`(a,)`).
The set-shaped children render in `Ord` order (§4), so the output is
deterministic. A single applied-form printer serves a function term and an atom,
so the two cannot drift.

**The round-trip law, and the trap it hides.** The law (the *round-trip* witness,
spec §3, §7.6):

> for every program `P` this renderer covers, `raise(parse(render(P, d), d)) == P`
> up to provenance — render, parse, and raise return the same program.

It is a *fixpoint* law, and a fixpoint law has a trap: a renderer and a parser
that share a consistent misreading satisfy it while both being wrong. So the law
is held two ways (§16): against this estate's own parser (the reparse above), and
against the **independent** authority — the rendered text is parsed by the pinned
binary and its acceptance and structure compared (the differential, §16), the
syntax tier's parser being itself differential-tested against that authority
(syntax §16). Two exceptions are stated, not discovered: the authority's own
unparse is non-injective on a pair of forms an empty aggregate can take (`#count
{}` with one empty element versus none), so the law carries those as named
exceptions rather than pretending an identity the notation cannot hold; and the
theory carve-out of §5 rides here too, the reparse being up-to-grounding for
theory-bearing programs.

**Computational cost.** `render` is `O(output)` — proportional to the text it
writes, a flat work-list walk, no recursion in depth; `canonical_spelling` of a
synonym (syntax §11.3) is `O(1)` where the renderer normalizes one.

## 11. Patterns and unification

The pattern language over the term algebra and unification live here, backing the
query surface (spec §7.7). This tier ships the **mechanism** — patterns, the most
general unifier, substitution, the occurs check, rename-apart; the **epistemic
reading** — the three-valued answer, cautious and brave, over a set of answer
sets — is a separate, engine-free client (§11.4). The mechanism is designed to be
that client's clean foundation, and a declarative-testing successor's (spec §5.1),
because both read it for verdicts (§2).

### 11.1 The unifier

The primitive is the most general unifier of two atoms — a *literal* in the sense
of a signed atom (an atom or its strong negation, §4.6), a predicate application
with its strong sign; matching a pattern against a ground symbol is its degenerate
case. The unit is an `Atom`,
not the grammar's `Literal` (§4.6): a `Literal` may be a comparison or a boolean,
which carry no predicate to unify and no signature to range over (§11.3), and its
default negation is the query layer's reading, not the unifier's.

```rust
/// The most general unifier of two atoms, in one shared variable namespace.
/// `Ok(Some(σ))` — a unifier exists; `Ok(None)` — they do not unify; `Err` — an
/// argument is not a pattern, so the question cannot be answered. The three
/// outcomes are distinct on purpose (§11.2).
pub fn mgu(left: &Atom, right: &Atom) -> Result<Option<Substitution>, NotAPattern>;

/// A substitution: variables to bindings, keyed by variable. **Triangular**, not
/// fully resolved: a binding's term may itself mention bound variables (`X ↦ f(Y)`
/// with `Y ↦ a`), which is exactly what lets `mgu` produce it in near-linear space
/// — the fully-resolved (idempotent) form over explicit terms is worst-case
/// *exponential* in the atoms' size (the doubling `Xᵢ ↦ f(Xᵢ₋₁, Xᵢ₋₁)`), the
/// algorithmic-complexity blow-up the near-linear algorithm is chosen to avoid. The
/// triangular map is still a plain map keyed by variable, no scope-qualified key;
/// `substitute` (§9.2) is the resolving reader that follows the chains to the
/// fixpoint. No `Default` and no public empty constructor — the empty substitution
/// means *unified, binding nothing* (the affirmative match) and must arise only
/// from a successful unify, never be asserted (§11.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Substitution { /* private: BTreeMap<Variable, Binding> */ }
impl Substitution {
    pub fn get(&self, v: &Variable) -> Option<&Binding>;   // the *immediate* binding, unresolved
    pub fn iter(&self) -> impl Iterator<Item = (&Variable, &Binding)>;   // Ord order
}

/// A variable's binding. One variant now — a ground or non-ground term — and
/// non-exhaustive, so a later constraint binding (an `X > 5` from a
/// constraint-answering engine) is a new variant, not a migration: the query
/// system this founds keeps that door open (spec §7.7's reserved reading).
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Binding { Bound(Term) }
```

**One namespace, and rename-apart as a separate step.** `mgu` treats its two
atoms as sharing one variable namespace — one namespace, not a scope per rule — so
its substitution is a plain map keyed by variable, with no scope-qualified key to
reason about. Unifying atoms from *different* rules, where each rule's `X` is
its own, is `mgu(a1, rename_apart(a2, &mut fresh))`: the caller standardizes apart
first (drawing fresh variables from §9.2's `Fresh`), and the result is again a
single-namespace substitution the caller interprets. Making rename-apart the
caller's explicit step is what keeps the answer type simple *and* the general,
cross-rule unification expressible — the static "can this rule head feed this body
literal" question a solver's analysis (§12) and an optimizer's inlining (§9) both
ask.

```rust
/// A copy of an atom with its variables renamed to fresh ones, so it shares no
/// variable with anything already in play (§9.2).
pub fn rename_apart(atom: &Atom, fresh: &mut Fresh) -> Atom;
```

**The occurs check is forced, and that is a feature.** A `Term` is a tree (§3.3),
so a cyclic term is *not representable*; omitting the occurs check would diverge
building a rational tree that has no home, rather than silently produce an unsound
binding. So the unifier is sound by construction, where a system that omits the
check (Prolog by default) is not. The occurs check rules out an *infinite* term; it
does not rule out a *finite but exponentially large* one, which is a size fact, not
a soundness one, handled next.

**The algorithm, and where the cost lives.** `mgu` is the near-linear unification of
Martelli and Montanari (1982) — a union-find over the two atoms' term structure with
the occurs check run as the equations are solved — so *deciding* unifiability and
producing the triangular `Substitution` is near-linear in the two atoms (the
worst-case-efficient choice the mission-critical bar and spec §12.4 require, not a
naive quadratic composition). The exponential blow-up unification is infamous for is
**not** in the decision or in `mgu`'s result: it lives only in *materialising* a
resolved term, which `substitute` (§9.2) does on demand at `O(output)`. So the
unifier itself is not an algorithmic-complexity denial-of-service, and the matching
path (§11.3), which range-scans rather than materialises, never pays it; a caller
that explicitly substitutes a pathological unifier into a term pays the output cost
by choice, and interning (§17) is the seam that bounds even that when a consumer
measures the need. This is the reconciliation the substitution representation forces:
near-linear `mgu` (§15), a triangular result, and a resolving `substitute` whose cost
is honestly the size of what it builds.

### 11.2 What a pattern is, and the three outcomes

A **pattern** is a signed `Atom` (§4.6) — signed because an answer set contains
`-p(1)` and a pattern for `p(X)` must not match it — whose argument terms are the
**constructor fragment**: variables, ground symbols, function terms, and tuples.
An argument built from a non-Herbrand term-former is **not** a pattern and
refuses:

```rust
/// Why an atom is not a pattern, carrying the offending argument term. One reason
/// today, and non-exhaustive, so a later reason is a variant, not a migration.
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum NotAPattern {
    /// An argument is an arithmetic term (would need inverting — this tier does
    /// not evaluate in a pattern), or an interval or pool (each *names a set*,
    /// whose all-versus-any reading a term-against-symbol match cannot decide),
    /// or an `@`-call.
    NonDenoting { term: Term },
}
```

The **`Ok(None)` versus `Err` distinction is load-bearing**: `Ok(None)` says *this
pattern does not match this symbol*; `Err(NotAPattern)` says *I cannot answer that
question of this term*. A design that collapsed them — that let a non-pattern
quietly match nothing — would report "your program has no such atom" when the
truth is "I could not decide," the undetectable wrong answer this estate forbids
(spec §5.2). It is exactly the distinction a declarative-testing consumer reads as
"cannot decide is a value, never folded into no" (spec §5.1). A pattern with
ground arithmetic composes with the evaluator (§3.5): a `p(1+2)` query evaluates
`1+2` to `3` and matches `p(3)`; an arithmetic term with a variable stays a
`NonDenoting` refusal, honestly.

### 11.3 Matching against an answer set

An answer set is a `BTreeSet<Symbol>` (a solve-tier value), and a pattern's
predicate, arity, and sign are always concrete (an `Atom` carries them, §4.6), so
the symbols a pattern *could* match form one contiguous range of that ordered set:

```rust
/// The inclusive range of `Symbol`s a pattern (a signed `Atom`, §11.2) could
/// unify with — its predicate, arity, and sign block in the ground-term order
/// (§3.1). Total: the signature is always concrete, so the range never refuses;
/// a non-pattern argument is the *unifier*'s concern (§11.2), not the range's.
/// Lets a match against an answer set be `O(log n + k)` (a range scan), not
/// `O(n)` (a full scan).
pub fn signature_range(pattern: &Atom) -> RangeInclusive<Symbol>;
```

This is the mechanism the query client's binding search stands on; that it finds
exactly what a full scan finds is a stated law (§16).

### 11.4 The seam to the epistemic reading

The three-valued query — is a ground atom *yes*, *no*, or *unknown*; what are a
pattern's bindings, partitioned by that trichotomy; the cautious and brave
consequences — is **not** in this tier. It needs *answer sets* (a solve-tier
value), and it is pure computation over them plus this tier's matching, so it is
**engine-free**: a client crate — `themelios-query` — over the program tier's
patterns and the solve tier's answer sets, depending on no FFI. It is built
*alongside the solve tier*, in the solve stage — the mirror of the adjacency
`themelios-analysis` has to this tier (§12): each reads the output of the tier it
sits beside, so the analysis client is built with the program tier and the query
client with the solve tier. It is a distinct crate rather than folded into the
solve tier because several consumers read the epistemic surface without wanting
the backend contract (a testing successor, a REPL, an explanation tool, a
probabilistic implementation, spec §1.1). This
document names it and the two objects it will carry — a **world view** (the set of
a program's answer sets) and its optimal restriction (the answer sets that are
proven optimal, when the program optimizes) — so that the mechanism here is built
as their foundation; their design is that crate's. (This refines spec §12.2's
roster, which folded the query surface into the solve tier; the refinement is
recorded in §18, not left silent.)

## 12. Structural analysis

A program's *structural facts* — what constructs it uses, how its predicates
depend on one another, whether its rules are safe, what classes of the literature
it falls in — are a syntactic reading of the value, and a solver's dispatch, a
formatter's lints, and an optimizer's pass conditions all read them. This tier
provides the **substrate** for that reading; the assembled reading is
`themelios-analysis` — a distinct crate built *alongside* this tier in the same
stage, because it reads the program value and nothing else (§12.2). Its own design
of record, taken through the same review, is `docs/design/analysis.md`.

### 12.1 The substrate

The `Program` value's accessors already expose a program's structure — its parts,
statements, rules, heads, bodies, atoms, and their predicates and variables (§4) —
and the visitor (§9.1) walks it. On that, this tier adds the pure structural
queries an analysis is written in, none of which solves or grounds:

```rust
impl Rule {
    pub fn variables(&self) -> impl Iterator<Item = &Variable>;   // free variables, in order
    pub fn is_ground(&self) -> bool;
    /// The head and body predicate signatures — the edges a dependency graph is
    /// built from (§12.2). `body_signatures` tags each dependency with the **kind**
    /// it runs through, the semantic mode a dependency graph reads, not the
    /// syntactic negation word (below).
    pub fn head_signatures(&self) -> impl Iterator<Item = Signature>;
    pub fn body_signatures(&self) -> impl Iterator<Item = (DependencyKind, Signature)>;
}

/// How a body predicate is depended on — the semantic mode a dependency graph
/// reads (analysis §4), defined here as its one authority. It is deliberately **not**
/// the syntactic `DefaultNegation` prefix (§4.5): that carries the negation *word*
/// (`not`/`not not`), while a graph consumer needs the dependency *mode*, and
/// mapping one to the other also needs the enclosing former (a plain literal, an
/// aggregate, a theory atom), which the prefix does not carry. The three modes are
/// the honest KR distinctions — the literature's positive/negative dependency and
/// the non-monotone aggregate edge, no artificial symmetry — and they are **not
/// mutually exclusive**: `body_signatures` yields one `(DependencyKind, Signature)`
/// pair per mode an occurrence carries, so a predicate reached inside a *negated*
/// aggregate yields both `ThroughAggregate` and `Negative` (analysis §4). The
/// analysis reuses this type (`pub use`) rather than redefine it, exactly as it
/// reuses `Signature` and `Rule` (analysis §4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum DependencyKind {
    /// A positive body occurrence — no default negation, not through a non-monotone
    /// former: a monotone dependency, the edge the positive dependency graph keeps.
    Positive,
    /// Through default negation (`not`/`not not`) — the mode stratification reads;
    /// double negation is not monotone, so `NotNot` is `Negative` here too.
    Negative,
    /// Through a non-monotone aggregate or theory atom.
    ThroughAggregate,
}
```

### 12.2 `themelios-analysis`

`themelios-analysis` is a distinct crate — engine-free, reading only the program
value — **built alongside this tier in the same stage**, since a program's
structural facts are exactly a syntactic reading of the value here. Its design of
record is the companion `docs/design/analysis.md`; its `Analysis` is a *model of
structural facts*, never a bag of booleans (a boolean per property loses the
witness that is the point). In brief — the substrate (§12.1) is built to found it,
and the companion document elaborates it:

- **What it answers.** The construct scan (which constructs a program uses); the
  predicate dependency graph and its strongly-connected components — the SCC
  decomposition being the *primary product*, not a flag on a predicate; safety
  (is every rule safe) and finiteness (will grounding terminate); and the classes
  of the literature — tight, stratified, head-cycle-free, normal, Horn,
  disjunctive, choice — each as a typed verdict carrying its witness (the SCC that
  blocks tightness, the unsafe rule, the negative cycle that breaks
  stratification) and the rule bearing a construct that falls outside a stated
  fragment — the witness a rule by value, not a span (§6, and analysis.md).
- **Facts, not policy.** It states what is structurally true; the *routing* — which
  category earns which algorithm, which threshold trips — is the consuming
  solver's, never the analysis's.
- **Sound, at the predicate level, with a stated error direction.** It runs
  *before* grounding, so tightness and head-cycle-freeness (properties of the
  ground program) are **sound predicate-level over-approximations**, and where the
  predicate level cannot decide, the verdict is `Unknown` — a value, never a
  guess. The error direction is stated per property and always errs safe: a
  property that would let a consumer pick a specialized method is asserted only
  when proven, because a false "tight" yields an *unsound* result while a missed
  "tight" yields a merely slower one.

This makes precise the boundary spec §1.1 and §13 draw for the native-solver
horizon: the **syntactic analysis and classification** is this estate's, an
immediate client of the program tier; the **grounding, the solving algorithms,
and the routing policy** are the satellite's. (A consuming solver names its
contract in its own vocabulary; this estate names the capability in the
literature's — the analysis of a program — never a borrowed project name, §1.)

## 13. Totality and the depth discipline

All values are owned and total, and no walk over user-reachable structure
recurses on the call stack (spec §5.2, §7.2). Grammar §10 maps exactly where the
discipline bites: the **four self-recursive families** — the term, the
constant-term subset, the value-term subset, and the theory term — nest without
bound, and *only* they. Everything above a term is grammar-bounded: statements
are flat, aggregates do not nest (an aggregate is an element, never a literal),
conditional literals do not nest, a head is one layer. So:

- **Every walk over `Term` and `Symbol` is iterative** — an explicit work list,
  not call-stack recursion — *including the ones the compiler would derive*:
  `Clone`, `Drop`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, the concrete
  rendering (§10), and `into_parts`/`fold`/`subterms`/`canonicalize`/`substitute`/
  `evaluate`. A ground value tens of thousands of levels deep — which recursive
  constructions produce in practice (spec §5.2) — is constructed, compared,
  rendered, and dropped without touching the stack. The hand-written iterative
  `Ord`/`Eq`/`Hash` are the subtle ones: they are written *once each*, as one
  content projection (§5), and checked against a derived twin by the mirror
  differential (§16), which is what catches a hand-written walk that disagrees
  with the naive one.
- **The structural layers recurse by grammar-bounded iteration.** A walk from a
  program to a term crosses a fixed number of layers (§4), so those functions may
  use the call stack: their depth is the grammar's, not the input's.

**No depth refusal is owed here.** The syntax tier refuses past a nesting limit
because its tree builder's dependency recurses in depth (syntax §6.6); this
tier's values are plain trees its own work-list walks traverse, so any depth the
heap holds is *handled*, not refused — a term is `O(nodes)` to walk however deep
it is, and there is no shared structure to unfold (that concern is the solve
tier's, where symbols are read from an engine, §8). The depth gate (§16) proves
each walk stack-independent on a stated stack at a depth far past anything the
raise's bounded tree or a real construction produces.

## 14. Posture

Base §8's four rules and syntax §12's qualifications bind this crate —
observational purity, plain data, declarative construction, the modeled shape
dictating the type — and this section states only what this tier adds.

- **Every operation is a pure function of its inputs.** The raise, every
  transformation, `render`, `mgu`, `evaluate`, `canonicalize` — same inputs, same
  result; no global state, no interning table (§3.1), no ambient context. Where
  mechanism wants mutation — a work list, a `Fresh` counter, a diagnostics vector
  — it is local and invisible at the surface (base §8.1).
- **The value is owned plain data, not a view.** Unlike the syntax tier's borrowed
  cursors (syntax §12.2), every value here is `Send + Sync + 'static` and holds no
  borrow — the property the solve tier's owned sessions and the transformation
  surface both require.
- **One renderer, no second rendering.** Concrete syntax is produced by `render`
  (§10) alone; `Program`, `Term`, and `Symbol` claim **no** `Display`, so no
  second, drifting rendering of a program can exist (syntax §12.5's discipline).
  A `Debug` is derived-shaped but iterative (§13).
- **The std-trait posture.** Every refusal type — `NotAnIdentifier`,
  `NotAVariable`, `NotAnInteger`, `EvalError`, `FromSymbolError`, `NotAPattern`,
  `Unspellable`, `LowerError` — implements `Display` (the question the caller can
  fix, in words) and `std::error::Error` (so a host `?`-composes them). Their
  `Display` texts are presentation and may improve.
- **The criterion of one obvious way is posture, not only a failure condition
  (§2).** A convenience is admitted only when it makes the obvious way *more*
  obvious; boilerplate falls away as a consequence of that, never as the goal, so
  a terse spelling that obscures, or a second way that competes with the obvious
  one, is refused even where it would save keystrokes.
- **A type parameter marks a role, not a domain object.** A public type is generic
  only when it is a *carrier*, a *view*, or a *decomposition* — a shape defined by
  what it holds rather than by the domain: `WithProvenance<T>` (the provenance
  carrier, §6), `TermParts<T>`/`SymbolParts<T>` (the one-level decompositions, §3.6),
  and the generic *operations* over them (`fold`, `map`, the coercions, `not`). A
  **domain object** — a node of the logical taxonomy (`Symbol`, `Term`, `Head`,
  `Body`, `Rule`, an aggregate, a verdict) — is always **concrete**; structure shared
  among sibling concrete nodes is a **trait** (`HasGuards`, `Negatable`), never a type
  parameter. This is the rule the syntax tier already follows (`Parse<T>`,
  `AstChildren<T>` generic; every AST node concrete), carried here so a reader
  predicts a type's shape from its role and no lone generic sits among an
  otherwise-concrete family (§4.7's two aggregate types are the worked instance;
  `themelios-analysis`'s single concrete `Verdict` is the other).

## 15. Failure semantics and computational costs, consolidated

Spec §2 item 8's obligation, discharged at design level: nothing in this crate
panics on any input — malformed, hostile, or absurdly deep — and the table names
every refusing door; every operation not listed is total. Each refusal is exactly
the operation's error type (base §3.2).

| operation | refuses with | cost |
|---|---|---|
| `Name::new` / `VarName::new` | `NotAnIdentifier` / `NotAVariable` | O(text) |
| `floor`/`ceil`/`round`/`trunc` | `NotAnInteger` (`NotFinite` \| `OutOfRange`) | O(1) |
| `Term::evaluate` / `evaluate` | `EvalError` (`NotGround` \| `External` \| `Undefined` \| `Overflow`) | O(nodes) |
| `FromSymbol::from_symbol` | `FromSymbolError` (the unmatched symbol) | O(nodes) |
| `mgu` | `NotAPattern`; `Ok(None)` is *no match*, not a refusal (§11.2) | near-linear in both atoms (§11.1) |
| `render` | `Unspellable` (a string value the dialect cannot spell) | O(output) |

`raise` never refuses — every parse yields a `Raised` carrying the program and its
lowering diagnostics (a diagnostic is a value on a total raise, not a refusal,
syntax §12.4). Total, never refusing, never panicking: every constructor that
composes valid values (§7.2); every accessor; `canonicalize`; every
transformation (§9); `Program`/`Term`/`Symbol` equality, ordering, hashing, clone,
and drop (§13); the conversion `impl`s (§3.4); `subterms`/`fold`/`into_parts`;
`signature_range` (§11.3 — the signature is always concrete).

Costs, consolidated: construction, canonicalization, evaluation, rendering, and
every walk are `O(nodes)` in time and iterative in depth (§13); equality,
ordering, and hashing are `O(nodes)` and proportional to structure (spec §7.1);
clone is linear; part-wise access is `O(log parts)`; `mgu` is near-linear in both
atoms, deciding and producing the triangular substitution (§11.1); `substitute`
resolving that substitution is `O(output)` — the resolved result's size, which a
pathological unifier makes exponential (an output-size lower bound, not an algorithm
cost, §9.2, §11.1); a match against an answer set of `n` symbols is `O(log n + k)`
via `signature_range` (§11.3). The scaling benches (§16) hold these shapes.

## 16. Assurance instruments

Per spec §11 the stage is not done until these are green; per spec §10.1 proptest
and criterion are standing from the tier's landing. Every instrument is documented
with what it proves and what it cannot (spec §10.2).

- **Property laws (proptest), over generated programs, terms, and symbols** whose
  generators draw *both spellings* of every ground term from the first day (a
  generator that only ever built already-collapsed ground terms would hide a
  canonicalization defect — the value must be able to exhibit two spellings of one
  term):
  - **Set and equality semantics:** a body and a disjunction are sets (a duplicate
    element vanishes, a reordering is the same value); `Program` equality is
    canonical-form equality up to provenance; `Ord`/`Eq`/`Hash` are one content
    projection — mutually consistent, a total order, and in agreement with a
    **derived twin** on shallow generated values (the mirror differential, which
    holds the hand-written iterative walks of §13 honest).
  - **The recursion scheme:** `x.fold(From::from) == x` and `into_parts` rebuilt
    `== x`, at every level (term, symbol, and up the structural spine); `fold` and
    `subterms` visit in document order.
  - **Canonicalization:** idempotent, and equality-respecting (it never makes two
    unequal programs equal, nor two equal ones unequal).
  - **Provenance:** the merge is a join-semilattice (union idempotent, commutative,
    associative, empty the identity), and the preservation law holds per
    content-class (nothing lost, nothing fabricated, §6.3).
  - **Unification:** soundness (`σ = mgu(a, b)` implies `aσ` and `bσ` are
    syntactically equal), most-generality, the occurs check (no cyclic binding is
    ever produced), matching as the degenerate case, and `signature_range` finding
    exactly what a full scan finds; the `Ok(None)`-versus-`Err` distinction is
    exercised on both.
  - **Round-trip:** `raise(parse(render(P, d), d)) == P` up to provenance, over
    generated and corpus programs, with the named exceptions of §10.
- **The differential** (feature-gated harness, out of band per milestone, the
  pinned binary the authority — grammar §3): the rendered text parsed by the
  authority agrees on membership and structure (the *independent* oracle the
  fixpoint law needs, §10); `evaluate` agrees with the authority's ground-term
  arithmetic, and the authority's overflow behavior is recorded beside the
  refuse-on-overflow decision (§3.5); `Symbol`'s order agrees with the authority's
  printing order (§3.1); and canonical-syntactic equality agrees with the
  authority's parse-then-unparse on the theory-free, optimization-free fragment
  (§5.2's arbiter).
- **The depth gate** (subprocess, spec §10.1): on a stated stack, a term nested far
  beyond any real program is constructed, canonicalized, compared, hashed,
  rendered, substituted into, evaluated, and dropped, and every walk survives — the
  per-walk proof that §13's discipline holds, no walk excepted.
- **Scaling shapes (criterion):** equality, clone, rendering, and traversal linear
  in structure; equality proportional to structure size; part-wise access cheap;
  the shapes asserted by the test suite, absolute numbers measured out of band as
  benchmarks (spec §10.2).
- **Golden snapshots**, reviewed: canonical renderings of a corpus of programs, and
  the lowering diagnostics of §8 rendered through base's human view at the
  rust-analyzer bar (the *diagnostics-quality* discipline, spec §2 item 9).
- **The witnesses this tier seeds** (spec §3): the construction half of
  *first-solve* — a program built through the spelled-out constructors and through
  the macros is structurally equal; *round-trip*; and *transformation* — a
  `Program → Program` rewrite whose provenance reaches the origins and whose
  diagnostic on a rewritten rule points at source.
- **Standing checks:** mutation per milestone over the constructor, canonicalization,
  transformation, and unification logic; the workspace coverage floor as a
  tripwire; unused-code and unused-result warnings denied (spec §5.2); documentation
  examples that run; the executable-claims standard for anything this crate says
  about itself (spec §10.4); `forbid(unsafe_code)` and the structural trust checks
  (FFI-free closure, no build script).

## 17. Reserved seams and non-goals

Named reserved seams — deferred with their reasons and their arriving consumers,
never gaps:

- **Symbol interning** (§3.1): a per-arena interner for structural dedup and
  `O(1)` equality, never a global table (spec §1.2); its consumers are a program
  large enough that the benches (§16) show the dedup pays, and a caller that
  *materialises* a large unifier — `substitute` resolving a triangular substitution
  is `O(output)`, and a pathological unifier makes that output exponentially larger
  than its input (§9.2, §11.1), which shared structure bounds. v1 is owned by value,
  so v1 pays the output cost; the interner is the measured-need answer to both.
- **Semantic equivalence checking** (spec §7.1, §13): ordinary and strong
  equivalence as decision services, deliberately distinct from structural
  equality; the consumer is a verified-rewrite checker or an across-barrier
  optimizer.
- **Constrained (non-ground) bindings** (§11): the `Binding` variant a
  constraint-answering engine (an s(CASP)-class successor) returns; shaped now
  (`#[non_exhaustive]`), built with that consumer, so it is a migration and not a
  rewrite.
- **The three-valued query and the world view** (§11.4): `themelios-query`,
  engine-free, built with the solve tier in the solve stage — the mirror of
  `themelios-analysis`'s adjacency to this one (§11.4); this tier is built as its
  foundation. (`themelios-analysis` is **not** deferred: it is built alongside
  this tier in this stage, §12, its design of record the companion `analysis.md`.)
- **Incremental computation machinery** (spec §7.8): the preconditions are bought
  here — purity of every derivation, owned values, a cheap total raise, named
  identity-attachment points in provenance (§6) — and the memoization and
  dependency tracking are deferred, their consumer an editor that measures the
  need.
- **The native-solver components** (spec §1.1, §13): grounding, the solving
  algorithms, and the analysis-directed routing are the satellite's; this tier
  provides the value they compose over.

Non-goals, absolutely: solving and grounding (the solve tier); evaluation of
rule-embedded terms (only the explicit ground-value door, §3.5 — a `1+2` in a
rule is the grounder's); admission — `#theory` matching, safety *as a rejection*
(safety is *analyzed*, §12, never a construction refusal), ASP-Core-2 strict
conformance, meaningful `#external` values (grammar §13, syntax §17, carried
forward); the engine-facing lowering (`Program → aspif`, the solve tier's — a
*different* lowering from the raise, the two named distinctly so neither hides
the other); styled formatting (the formatter satellite); I/O of any kind
(`#include` parsed and never resolved, `#script` carried and never run); and
serialization (shapes, not bytes — base §7.2's posture carried).

The **fitness anchors** (§2) are the acceptance lenses this design is held to,
each buildable-under, library-first, with the structural-vs-semantic boundary
(§2, §9.3) between what this tier provides and what its client proves: a
declarative-testing successor, a probabilistic-ASP implementation, an explanation
tool built to a mission-critical bar, a non-ground optimizer, and a solver's
analysis contract.

## 18. Revisions

Refinements this design makes to the specification, recorded here rather than left
silent so the specification's successor carries them; each is a deliberate
evolution with its argument, not a drift.

- **`themelios-analysis`** (§12) — a **program-stage** co-deliverable. The
  syntactic analysis and classification, which spec §1.1 and §13 place in the
  native-solver horizon, is drawn as an immediate, engine-free client of the
  program tier, distinct from the grounding, solving, and routing that remain the
  satellite's. A program's structural facts are a syntactic reading of this tier's
  value — read by a solver's dispatch, a formatter's lints, and an optimizer's
  conditions alike — so it is built *in this stage, beside the tier whose value it
  reads*, with its own design of record (`docs/design/analysis.md`). The facts are
  foundation; only the policy over them is satellite.
- **`themelios-query`** (§11.4) — a **solve-stage** co-deliverable. The query
  surface, which spec §12.2 folds into `themelios-solve`, is factored into its own
  engine-free crate: the three-valued reading is pure computation over answer sets
  and this tier's patterns, and several consumers — a testing successor, a REPL, an
  explanation tool, a probabilistic implementation (spec §1.1) — want it without
  the backend contract. `themelios-solve` is already engine-free (spec §12.2), so
  this is packaging for the multi-client reason, not a new FFI boundary; it is
  built *in the solve stage, beside the tier whose answer sets it reads* — the
  mirror of `themelios-analysis`'s placement.
- **Negation terminology.** `-p` and its relatives are named *strong (explicit)
  negation* — here, in the API (`themelios-syntax`'s `strong_negation_token`), and
  in the specification's KR vocabulary (spec §7.1) — the precise term, since `-p`
  does *not* carry the excluded middle "classical" would imply (§4). The grammar of
  record keeps "classical negation"/"classical literal" (grammar §5.2, §5.9; the
  ASP-Core-2 dialect chapter, §6), which are ASP-Core-2's own terms, so it stays
  faithful to the standard it implements:
  the syntactic register cites the standard, the logic register states the
  semantics, and both name one operator.
- **The substitution representation (§9.2, §11.1, §15, §17).** `Substitution` is
  **triangular**, and `substitute` **resolving**: the original document left the
  representation implicit, under which a near-linear `mgu`, a single-pass `O(nodes)`
  `substitute`, and a fully-resolved substitution over owned terms cannot all hold —
  the resolved form is worst-case exponential. Pinned so the three cohere: `mgu` is
  near-linear deciding and producing the compact triangular substitution, `substitute`
  resolves it at `O(output)` (the honest output-size bound), the matching path never
  materialises, and interning (§17) gains the materialisation consumer. This also
  settles the algorithm question the review raised alongside it (Martelli–Montanari,
  §11.1).
- **The dependency kind (§12.1).** `body_signatures` yields `(DependencyKind,
  Signature)`, and `DependencyKind` — the honest three-mode classification a
  dependency graph reads — is defined **here**, in the substrate, and reused by
  `themelios-analysis` rather than redefined there (it had carried its own `EdgeKind`,
  under-determined by the substrate's older `DefaultNegation` tag). The modes are not
  mutually exclusive, so a dependency carries one pair per mode; no symmetric grid is
  introduced.
- **Position-typed aggregate elements (§4.4, §4.7).** A head aggregate's elements
  derive and carry a literal; a body aggregate's test and do not. The original
  document shared one `FunctionAggregate` across both positions and left the element
  type unnamed, which would let a body aggregate hold a head element. There are now two
  concrete types — `FunctionAggregate` (body) and `HeadAggregate` (head), each over its
  own element type — so the position invariant is held in the type as §4.5 promises for
  its neighbours, and an aggregate stays a plain structural node, keeping the taxonomy's
  regularity (no lone generic among the structural-node types).
- **The generics rule is stated (§14), and the carrier surface named (§6.2).** A
  reader's review of the API found the estate's generic-versus-concrete rule followed
  but never articulated (the concrete side argued at §4.7, the generic side only
  demonstrated), which is where an analysis-tier inconsistency had slipped in. §14 now
  states it — a type parameter marks a carrier/view/decomposition role or a generic
  operation, a domain object (a verdict included) is concrete, shared structure is a
  trait. `themelios-analysis` folds its bespoke `Finiteness` into the one concrete
  `Verdict` accordingly (analysis §5/§6, §12). And §6.2 now names the full
  `WithProvenance` surface (`new`/`constructed`/`into_value`/`map`) the tier builds,
  rather than leaving `map`/`into_value` to a plan-level note.