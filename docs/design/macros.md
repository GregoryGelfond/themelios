# themelios-macros — tier design

2026-09-10. Design, pre-implementation. This document is the API design of
`themelios-macros` — the surface-macro tier, its vocabulary, expansion
architecture, semantics, and assurance — derived from the v1 specification
(`docs/specification.md`, cited as *spec §n*), the syntax tier design
(`docs/design/syntax.md`, *syntax §n*), the program tier design
(`docs/design/program.md`, *program §n*), and the grammar of record
(`docs/grammar.md`, *grammar §n*); a bare *§n* cites this document's own
sections. It is written to stand alone in the sense the specification is: a
reader holding this repository and public sources can check every claim. Where
this document and the specification disagree, the specification governs and the
disagreement is a defect here.

This design covers the **construction-macros tranche** — the sugar that fronts
the syntax tier's parser and the program tier's constructors and raise doors,
which spec §11 places "after stages 2–3." The extraction and registration
attributes, and the solve-adjacent macros, are named here as deliberate
absences (§4) and land with the tiers they front.

---

## 1. What themelios-macros is

`themelios-macros` is the third *surface* over construction, above the two the
program tier already provides (program §7): the spelled-out constructors, and
the raise. It is **sugar over those constructors — a spelling, not a
representation.** Everything a macro builds is buildable by hand; the macro adds
a register that reads as the logic it declares, and nothing a hand-written call
could not reach.

It is a **procedural-macro crate**, and thus a *client* of two lower tiers at
once (program §7.4):

- of **`themelios-syntax`** — the real parser under the macro dialect
  (grammar §9). A macro that ingests ASP syntax hands it to *that* parser; there
  is no second reader of ASP anywhere in this crate.
- of **`themelios-program`** — the smart constructors (program §7.1), the raise
  doors `raise_statement` / `raise_term` (program §8) a parsed fragment lowers
  through, the conversion pillar (program §3.4) a spliced value crosses, and the
  transformation surface (program §9) a splice is injected through.

The crate adds **no representation of its own**: no AST, no `Program` variant,
no fragment reader. Its output is calls into the two tiers beneath it, and its
one owned artifact is the *mapping* from Rust's token model onto the roster
(grammar §9) — the mapping the grammar document assigns to "the macro crate."

**This tranche delivers nine construction macros** (§4, §8): `atom!`, `fact!`,
`rule!`, `constraint!`, `minimize!`, `maximize!`, `show!`, `external!`, and the
program-level block `program!`. What it is *not*, this tranche: the
`#[derive(Extract)]` / `#[derive(Facts)]` and `#[external]` attributes, and the
solve-adjacent `scenario!` / `query!` — each a named, reasoned absence (§4), each
landing with the surface it fronts.

## 2. What this design is for

The specification fixes the macro tier's law and floor (spec §8), the grammar
document fixes the interpolation dialect (grammar §9), and the program tier's
design states what this tier stands on (program §7.4). This document turns those
into an implementable design: the expansion architecture (§5), the dialect
realization (§6), the splice surface (§7), the vocabulary with its signatures
(§8), the diagnostics (§9), the dependency and trust posture (§10), and the
assurance that holds it (§11).

Its acceptance is program §16's construction half of the *first-solve* witness:
a program built through the spelled-out constructors and through the macros is
**structurally equal**. That equality is the proof a construction macro is only
sugar, and it is this tier's load-bearing test (§11).

The tier's placement is spec §11, stage 4: the crate "exists from the first
stage it can client; its vocabulary accretes with its enablers — construction
macros after stages 2–3." Stages 2 (syntax) and 3 (program + analysis) are
built; the construction surface is regular and complete; so this tranche is due.

## 3. The three laws — the correctness spine

Spec §8 binds every macro by three laws. They are this design's backbone, and
every later section realizes one of them.

1. **One grammar.** A macro that ingests ASP syntax hands its token stream to
   the real parser at compile time; a macro-site syntax error is the same
   rust-analyzer-grade diagnostic the file parser gives, mapped onto the macro's
   spans. *Realized in §5 (the parser is reached at compile time) and §9 (the
   span mapping).* No bespoke reader exists in this crate (§1).

2. **No second representation.** A macro expands to public smart-constructor and
   raise calls only; everything a macro does is expressible spelled-out.
   *Realized in §5 (the expansion is a parse+raise call and a transformation-
   surface injection — all public program-tier doors) and enforced by §11's
   equality witness.* The one lowering authority is the raise; this crate does
   not re-express it.

3. **Specified interpolation.** Splicing Rust values into program syntax is the
   grammar document's macro dialect (grammar §9), not an ad-hoc behavior.
   *Realized in §6 (the token mapping) and §7 (the splice surface and its
   conversion crossing).*

The laws also draw this tier's razor, which program §7.4 states and §4 applies:
**a macro that cannot reduce to a trivial expansion over an already-complete
surface is out of scope** — either its target has a gap (a missing constructor,
accessor, or conversion — *stop and report it*, never paper over it in the
macro) or its target does not exist yet (*defer it*, and say so). Cleverness in
a macro is the smell that the surface beneath is incomplete.

## 4. The tranche boundary

program §7.4 lists the macro tier's *eventual* vocabulary. The razor of §3
decides what belongs in *this* tranche: a macro belongs when it reduces to a
trivial expansion over the now-complete syntax + construction + raise surface.
Each absence below is deliberate, so a reader meets it as a decision, not an
omission (program §7.4's own discipline).

**In this tranche.** The construction macros that front the program §7.1
constructors and the program §8 raise doors: `atom!`, `fact!`, `rule!`,
`constraint!`, `minimize!`, `maximize!`, `show!`, `external!` — the last the
`#external` **directive** macro (a statement in the program, program §4.8),
distinct from the `#[external]` attribute below — and the program-level block
`program!`. Their targets all exist at this tier's base (§8 names each), so each
reduces to a trivial expansion.

**Theory atoms are in; theory-term splices are deferred.** A theory atom that
arrives *without a splice* — `&sum { 1, 2 } <= n` — is ordinary syntax to the
parser and lowers through the raise like any other (the raise carries theory
atoms opaquely, program §4.9, §8; the theory-atom argument-list pool remains the
raise's stated §17 exception, unchanged by this tier). It costs the macros
nothing and is delivered. A **splice into a theory-term position** —
`&sum { $x } <= $bound`, which grammar §9 places in the v1 floor — is **deferred
with its reason**: theory terms are a *peer algebra* (program §4.9) whose only
shared leaf with the ordinary term algebra is the variable, so the placeholder-
and-inject mechanism of §7 (which replaces an ordinary-term variable with an
ordinary term through the §9 transformation surface) does not reach a theory-
term position — the transformation surface substitutes the shared variable leaf,
not an ordinary term into the theory algebra (program §9.2). A ground splice
*could* land as `TheoryTerm::Symbolic` were the injection expressed as generated
constructor code, but this tier's architecture (§5) rejects a value-to-code
generator as the second representation law 2 forbids. The theory-term splice
therefore reopens when its injection is settled — with the theory-term surface
the solve/query stage brings (program §17). Until then a `$` in a theory-term
position is a macro-site error with a clear message (§9), not a silent miss.

**Deferred to the solve stage** (they front a surface that does not exist until
solve, and sit behind the pre-solve discussion):

- `scenario!` — a reusable, named assumption configuration (spec §8's coined
  name). Assumptions are a solve-session concept; the surface does not exist.
- `query!` — the query surface (spec §9.7) is the solve/query tier's.
- The **`#[external]` attribute** — the `@`-function *registration* of spec §9.6
  (distinct from the `external!` directive macro above), which expands to the
  extension-surface registration the solve tier defines.

**Deferred pending the extraction seam.** The `#[derive(Extract)]` and
`#[derive(Facts)]` attributes expand over the conversion pillar (program §3.4)
and the point accessors (program §3.7), reading a `Symbol` rather than parsing
text. That surface is the **structured-decode seam**, which the program tier
left provisional (program §3.4, §17) pending the by-value `FromSymbol` door its
extraction consumer needs. These attributes land when that seam settles — a
reasoned deferral against a named-provisional surface, not a gap.

## 5. The expansion architecture

Every construction macro is a procedural macro. `macro_rules!` has no role:
law 1 requires the real parser at compile time, and a declarative macro cannot
run it. The nine macros are **one engine behind N entry points** — each entry
fixes a grammatical category (a term, a statement, a program) and a target
constructor; the engine is shared.

For an invocation, the engine runs one pipeline:

1. **Walk the Rust token stream** per the dialect mapping (§6), classifying each
   token onto the roster (grammar §9). It emits, in lockstep, two artifacts: a
   **themelios skeleton source string**, and a **span map** — an ordered table
   from each skeleton byte range to the `proc_macro` span of the Rust token that
   produced it.
2. **Holes become fresh placeholder variables.** A `$`-splice, and the rare
   by-value literal with no themelios spelling (§7), is written into the
   skeleton as a fresh variable whose name is chosen disjoint from every
   variable the invocation contains (all tokens are in hand at compile time, so
   the disjoint choice is exact). Every other token — names, operators,
   `#`-keywords, and by-value literals that *do* have a themelios spelling — is
   reconstructed faithfully (§6).
3. **Validate through the real parser, at compile time.** The engine mints a
   `Source` under `STRING_INPUT_SOURCE_ID` (syntax's public id-less sentinel)
   over the skeleton and parses it through the category's door
   (`parse_statement`, `parse_term`, or `parse_program`) under `Dialect::Clingo`
   (§8), then raises it (`raise_statement` / `raise_term` / `raise`). **Any
   syntax or lowering diagnostic — and any admission refusal (an over-`MAX_LEN`
   skeleton, syntax §12.4) — is re-emitted as a compile error at the mapped Rust
   span** (§9), at the rust-analyzer bar (spec §2 item 9) — law 1. Because this
   step admits and parses the very skeleton the expansion re-parses, an input
   that would refuse or diagnose fails *at compile time*: the expansion's
   construction is total on every input that compiled, and its internal unwraps
   rest on that compile-time proof, never on a runtime input (program §13's
   no-panic totality preserved).
4. **Emit the expansion.** The expansion is runtime code that (a) parses and
   raises the same constant skeleton through the same public doors, and (b)
   injects each splice value by replacing its placeholder variable, through the
   program tier's transformation surface (§7). The injected value is the
   spliced Rust value crossed to a ground term (`to_symbol()` then
   `From<Symbol>`, program §3.4). Nothing but public program-tier and syntax-tier
   doors appears in the expansion — law 2.
5. **Nothing else.** The value the expansion builds is exactly what the raise
   builds for the skeleton, with the splices resolved. There is no third
   representation between the tokens and the constructors.

**One lowering authority.** The construction logic — how a parsed tree becomes a
`Program` — lives once, in the raise (program §8). This crate never re-expresses
it; it reconstructs a *source* and calls the raise. That is what makes the §11
equality witness hold by construction rather than by coincidence: "built through
the macros" *is* "raised from source," which program §7 already holds equal to
"built through the constructors," up to provenance.

**Provenance.** Because the skeleton is minted under `STRING_INPUT_SOURCE_ID`,
every node the macro builds carries `Origin::Parsed` at that unresolvable
sentinel (program §6; syntax's string door). base's views render it as an
unresolved source — the honest statement that the value was parsed from a
synthetic fragment, not a catalogued file. This tier does **not** project a
`proc_macro` span into a themelios `Location`: they are different coordinate
systems (a themelios `Location` names a themelios `Source`), and a consumer that
needs a value located in real source parses a real file through the raise doors.
The §11 witness compares up to provenance (program §5.2), so the sentinel origin
does not disturb it.

**The honest cost, and its seam.** The skeleton is parsed twice — once at
compile time for law 1's diagnostics, once at runtime for the value. Both are
`O(fragment)`, and a fragment is small. The compile-time parse is *required*
(diagnostics are a compile-time obligation); the runtime parse is *inherent* to
expanding to the raise rather than to a value-to-code generator (§4's rejected
alternative). Where a benchmark (§11) shows a hot invocation pays for the
runtime parse, the constant skeleton's parse-and-raise is memoizable behind a
`OnceLock`, leaving only the per-call injection (`O(output)`, program §9.2) — a
seam, not built until measured.

**The reconstruction self-check.** Reconstruction (§6) is the one place the
engine could mistranslate — two adjacent tokens fusing, an intended adjacency
lost. The engine guards it: after lexing the skeleton it confirms the lexer's
token *kinds* match the sequence it intended to write. A mismatch is an engine
defect, caught by the suite (§11), never shipped; it is not a user-facing
condition (a user's malformed input is caught by step 3 as a real diagnostic).

## 6. The macro-dialect realization

Grammar §9 defines the dialect over Rust's token model and assigns its
realization to this crate. The engine implements exactly that mapping; it
invents no syntax.

**The token mapping** (grammar §9, restated as the engine reads it):

- A Rust identifier lexes by the name classes — lowercase-initial an
  `IDENTIFIER`, uppercase-initial a `VARIABLE`, `_` alone `ANONYMOUS`, `not` the
  keyword; an identifier no class matches whole (`__`, `_1`) is a dialect error.
- A Rust integer literal is a `NUMBER` **by value**; a Rust string literal a
  `STRING` **by value** (raw strings included).
- `#` forms a keyword exactly when *span-adjacent* to the keyword's word (and,
  for `#sum+`, to the `+` beyond it); a `#` separated from its word is a dialect
  error. Rust records adjacency only between punctuation, so span adjacency —
  the tokens' source positions abutting — is read from the `proc_macro` spans.
- Rust punctuation maps one-to-one onto the operator roster; a multi-character
  operator exists where its characters are adjacent and joined, and theory-
  operator runs form the same way inside theory expressions.
- Comments do not exist in the dialect (Rust has already removed them).
- `$` begins a splice by token order (§7).
- Every Rust token the mapping does not name is a dialect error at its span —
  float, char, and byte literals, suffixed numerals, lifetimes, raw identifiers
  (`r#not` is an error, never a way to spell the reserved name).

**Reconstruction.** From the classified tokens the engine writes a themelios
source string:

- A structural token (name, operator, `#`-keyword, punctuation) is written in
  its themelios spelling, with a separating space inserted exactly where two
  written tokens would otherwise fuse or lose an intended adjacency — the
  inverse of the span-adjacency rule, and the reconstruction's core obligation.
- A **by-value literal is re-encoded to its themelios spelling** where one
  exists: a numeral to its decimal (`0o17` becomes `15`, `1_000` becomes
  `1000` — the value crosses, the Rust spelling does not, grammar §9), a string
  to a grammar §4.4 string whose value equals the Rust string's value. The rare
  string value grammar §4.4 *cannot* spell (an escape §4.4 lacks) is not
  respelled: it becomes a placeholder hole (§7), carried as its `&str` value —
  the same honest asymmetry grammar §9 already owns, handled, not discovered.
- A `$`-splice becomes a fresh placeholder variable (§7).

The span map records, for each written token, its byte range in the skeleton and
the `proc_macro` span it came from, so a diagnostic located in the skeleton
maps back to the offending Rust token (§9).

## 7. Splices and the conversion pillar

A splice is grammar §9's interpolation: `$name` splices the value of a Rust
binding, `$( … )` splices any Rust expression. A splice stands where a **term**
may stand (the theory-term position is deferred, §4). Both forms are read by
token order: `$` takes the next identifier or parenthesized group.

**The conversion crossing.** A spliced value is a Rust value that *denotes a
ground symbol*: it crosses the conversion pillar's `ToSymbol` (program §3.4),
the one surface the ground-time `@`-functions, read-time extraction, and these
splices all share, so the three never diverge. The engine emits, at each splice
site, the value crossed to a ground term — `ToSymbol::to_symbol` then
`From<Symbol> for Term` (program §3.4, the lossless-inward door) — and injects
that term at the splice's placeholder variable through the program tier's
transformation surface (program §9.1's rewrite over any statement family,
program §9.2's substitution over a term), which canonicalizes at its door
(program §5.1). Injection reaches every position a construction macro builds —
a directive's term as surely as a rule body's — because the rewrite descends all
statement families (program §9.1).

**Refusal is at the door, at compile time.** A spliced value whose type is not
`ToSymbol` is a *compile error* — the trait bound the expansion's conversion
call carries is exactly "refuses at the constructor doors the expansion calls"
(grammar §9; spec §8 law 2). There is no runtime splice refusal to design: the
admitted ground types (`i8`…`i32`, `u8`, `u16`, `str`, `String`, and `Name`
through `Symbol::constant`) convert infallibly (program §3.4), and a fallible
landing — an `f64` through a rounding adapter (program §3.4) — is written *by
the caller inside the splice* (`$( round(x)? )`), where its `Result` is the
caller's to handle, not the macro's to hide. So a splice is never a second door
into construction: it is a value handed to the same conversion pillar every
other extension point uses.

**The asymmetries, stated.** By-value literals mean macro bodies admit spellings
files do not and the converse: a Rust string's escapes produce string values
grammar §4.4 cannot spell, and a Rust numeral may be `0o17` (the value crosses,
§6). **Primed names** (`a'`) are inexpressible in macros — Rust identifiers
carry no primes — and remain expressible through the spelled-out constructors,
the direction spec §8 law 2 guarantees; the converse is not promised (grammar
§9). A theory-term splice is deferred (§4).

## 8. The vocabulary

Each macro fixes a grammatical category and a target, and reads ASP under the
dialect (§6) with splices (§7). All parse under `Dialect::Clingo` — the richer,
membership-authority dialect (grammar §3); a consumer wanting ASP-Core-2
semantics reaches for the raise doors directly. Signatures name the *value each
builds*; the by-hand equivalent it equals is the program §7.1 constructor named.

- **`atom!(-? name(args…))` → `Atom`.** Reads an atom in **head context**, so a
  leading `-` is *strong* negation (`Sign::Negative`), the positional reading the
  tree resolves (program §3.3, §8) and `impl Neg for Atom` (program §7.1) mirrors
  — not arithmetic negation of a term. Lowers through the statement door and
  yields the head's `Atom`; a fragment that is not a single atom is a macro-site
  error. Equals `Atom::new` / `Atom::constant` (program §7.1).
- **`fact!(head)` → `Rule`.** A fact — a head with the empty body. Equals
  `Rule::fact` (program §7.1).
- **`rule!(head :- body)` → `Rule`.** A rule read as the rule. Equals
  `Head::when` (program §7.1).
- **`constraint!(:- body)` → `Rule`.** An integrity constraint. Equals
  `Rule::constraint` (program §7.1).
- **`minimize!(…)` / `maximize!(…)` → `Optimize`.** An optimization statement,
  each element a weighted term at a priority. Equals `minimize` / `maximize`
  (program §7.1, §4.7).
- **`show!(…)` → `Show`.** A `#show` directive. Equals the `Show` family's
  constructor (program §4.8, §7.1).
- **`external!(…)` → `External`.** A `#external` **directive** (the atom, its
  body, and the carried-not-meaningful value, program §4.8). Equals
  `External::new` (program §7.1). Distinct from the `#[external]` attribute (§4).
- **`program!{ s₁. s₂. … }` → `Program`.** A block of statements-with-splices,
  parsed and raised as one program (program §8's `raise`) — the natural inline-
  ASP surface for the ASP author (program §7.3). Equals a `Program` assembled
  through `Program::of_nodes` over the same statements (program §7.1); the
  program-level of spec §8's levels, enabled by stages 2–3.

**Composition.** A statement macro's value is a specific family type; a program
is assembled from several through the program tier's statement coercions
(`Program::of_nodes` over `Into<Statement>` values, program §7.1), or written
whole with `program!`. All roads reach a structurally-equal `Program` (§11).

**Return-type note.** The statement macros return their *specific* family type
(`Rule`, `Show`, `External`, `Optimize`), not an erased `Statement`, because the
specific type is the more useful value and composes into a program through the
existing coercions at no ceremony (program §7.1). `atom!` returns `Atom`;
`program!` returns `Program`.

## 9. Diagnostics

Law 1 requires a macro-site syntax error to read as the file parser's does. The
engine delivers this through the **span map** (§5, §6): the compile-time parse
and raise (step 3) produce diagnostics located in the skeleton source; each is
translated through the span map to the `proc_macro` span of the Rust token that
produced the offending skeleton text, and emitted as a compile error there. A
diagnostic whose skeleton span falls on reconstructed structure (a separating
space the engine inserted, §6) attributes to the nearest owning token, so the
underline never lands on synthetic text.

The diagnostics carried are the syntax tier's `SyntaxError` and the program
tier's `LowerError`, both lowering to base's normal form (base §6.5; program §8)
— one model, so a macro-site diagnostic reads exactly as the file parser's, at
the rust-analyzer bar (spec §2 item 9). Dialect errors of the mapping itself
(§6 — a float literal, a detached `#`, `r#not`) are the engine's own diagnostics,
located at the offending Rust token's span and worded in the same register.

**Hygiene.** The expansion references program- and syntax-tier items by absolute
path (`::themelios_program::…`, `::themelios_syntax::…`), so it compiles
regardless of the caller's imports. A `$( … )` splice's expression is emitted in
the caller's context — it *should* see the caller's bindings, which is the point
of a splice — and placeholder variables never enter Rust's namespace (they exist
only inside the skeleton string). A proc-macro crate can export only macros, so
the runtime doors the expansion names come from the caller's dependency on
`themelios-program` and `themelios-syntax`; the eventual `themelios` facade
(spec §11, stage 8) will re-export the macros beside the runtime so a consumer
names one crate — a stated forward dependency, not this tier's to resolve.

## 10. Dependencies and trust

Spec §12.5 rules the posture: **`-macros` carries the proc-macro toolchain
only.** Concretely:

- **`proc-macro2` and `quote`**, argued: `proc-macro2`'s token types can be
  constructed and manipulated *outside* a compile invocation, so the dialect
  mapping and reconstruction (§6) — the crate's one owned artifact — are
  unit-tested directly against the coverage and property discipline (§11) rather
  than only through a compile harness; `quote` is the ergonomic emission of the
  expansion. Both are pinned, ubiquitous, and compile-time only.
- **`syn` is declined.** This crate walks a *bespoke* token grammar (the dialect,
  §6), not Rust's grammar; Rust-AST parsing is the wrong tool, and hand-walking
  the token stream is the dependency policy's default ("hand-writing is the
  default where hand-writing is reasonable", spec §12.5). The one place a Rust
  expression is handled — a `$( … )` splice — is *captured and re-emitted*, a
  token-group operation `proc-macro2` serves without parsing.

The proc-macro toolchain runs **at compile time**; it is not in the shipped
closure of anything the macros expand to (the expansion's runtime dependencies
are `themelios-program` and `themelios-syntax`). `forbid(unsafe_code)` holds; no
build script; the structural trust checks (FFI-free, no build script) apply as
in the tiers beneath (program §16). The compile-fail instrument (§11) is a
**dev-dependency** (`trybuild`), outside the shipped closure exactly as
`proptest`, `criterion`, and `serde_json` are in the tiers beneath.

## 11. Assurance instruments

Per spec §11 the stage is not done until these are green; each is documented
with what it proves and what it cannot (spec §10.2).

- **The equality witness — the load-bearing acceptance** (program §16, the
  construction half of *first-solve*, spec §3): for every macro, the value it
  builds is **canonical-syntactically equal, up to provenance** (program §5.2,
  §6.2), to the value built through the spelled-out constructors it names in §8.
  This is the proof a construction macro is only sugar. A macro that *cannot* be
  made to satisfy it is the §3 razor firing — a signal the surface beneath has a
  gap — reported (stop), never papered over.
- **The floor mapping** (spec §3.2): each in-tranche macro carries the witness
  that exercises it — `atom!` / `fact!` / `rule!` / `constraint!` and
  `program!` under *first-solve*, `minimize!` / `maximize!` under *optimization*,
  `show!` under *enumeration*, `external!` under *multi-shot*. Where a witness's
  *behavior* needs the solve tier (multi-shot for `external!`), the structural-
  equality half is proved here and the behavioral half is seeded for the stage
  that runs it; a macro absent from this mapping would be a visible gap (spec
  §3.2). The deferred macros (§4) carry their witnesses when they land.
- **Property laws (proptest)** over the crate's owned logic: the dialect mapping
  (§6) — every named Rust token maps to its roster token, every unnamed token is
  a dialect error; the reconstruction self-check (§5) as a law — a reconstructed
  skeleton lexes to the intended kind sequence, over generated fragments; and a
  **splice round-trip** — a program written with splices of generated ground
  values equals the same program written with those values spelled in place.
- **Golden snapshots**, reviewed: representative expansions, and the macro-site
  diagnostics rendered through base's human view at the rust-analyzer bar (the
  diagnostics-quality discipline, spec §2 item 9).
- **Compile-fail tests** (`trybuild`, §10): a macro-site syntax error, a dialect
  error (§6), a non-`ToSymbol` splice (§7), and a theory-term splice (§4) each
  produce the expected compile error at the expected span — the direct test of
  law 1 and of the deferrals' clean refusal.
- **Standing checks:** the workspace coverage floor as a tripwire, at the
  estate's per-file bar; documentation examples that run; `forbid(unsafe_code)`
  and the structural trust checks; unused-code and unused-result denied (spec
  §5.2).

## 12. Reserved seams and non-goals

Named reserved seams — deferred with their reasons and arriving consumers, never
gaps (the deferrals of §4, gathered):

- **Theory-term splices** (§4): reopen with the theory-term surface the solve/
  query stage brings (program §17), when a ground splice's injection into the
  theory peer algebra (program §4.9) is expressible without a value-to-code
  generator.
- **Further splice sites** — names, tuples, statements (grammar §9): future
  vocabulary, each admitted on argument as the tiers accrete; the v1 floor is the
  term (and, deferred, the theory term).
- **The extraction and registration attributes** — `#[derive(Extract)]`,
  `#[derive(Facts)]`, `#[external]` (§4): land with the structured-decode seam
  (program §3.4, §17) and the `@`-function surface (spec §9.6).
- **The solve-adjacent macros** — `scenario!`, `query!` (§4): land with the
  solve session and query surfaces they front.

Non-goals, absolutely: a second parser or grammar of ASP (spec §2 item 3, §5.2)
— the one grammar is the syntax tier's, reached at compile time; a value-to-code
generator that re-expresses the raise's lowering (law 2) — the rejected
alternative of §5; assembling ASP as a runtime string to re-parse (never render-
then-parse — the macro reconstructs a skeleton *once*, at the token level, under
the one grammar); styled formatting (the formatter satellite); and any
representation of a program (the program is the program tier's, always).

## 13. Revisions

Refinements this design makes to the specification, recorded here rather than
left silent so the specification's successor carries them; each a deliberate
evolution with its argument, not a drift.

- **`program!` is in this tranche.** Spec §8's floor set enumerates the
  statement- and element-level construction macros and names "program-level" as
  a level, but lists no `program!` by name. This design admits the program-level
  block now: it is enabled by stages 2–3 (it reaches only the existing `parse`
  and `raise`), it is the natural inline surface for the ASP author (program
  §7.3), and it builds directly the `Program` the §11 witness compares. Its
  absence would leave the tier's headline surface for a later increment with no
  enabling reason to wait.
- **Theory atoms in, theory-term splices deferred** (§4). Grammar §9 places the
  theory-term splice in the v1 floor; this design defers *that splice* (the peer-
  algebra injection, program §4.9) while delivering theory *atoms* (which the
  parser and raise already handle) — the razor of §3 applied to a single
  sub-position, with its reopening named.
- **The proc-macro toolchain, read as `proc-macro2` + `quote`, `syn` declined**
  (§10). Spec §12.5 says "the proc-macro toolchain only"; this design reads that
  as the minimal set the job needs and argues `syn` out, the bespoke token
  grammar making Rust-AST parsing the wrong tool.
- **Macro-built provenance is `Parsed` at the id-less sentinel** (§5). The design
  routes construction through the raise over a skeleton minted under
  `STRING_INPUT_SOURCE_ID`, so a macro-built value's provenance is the honest
  "parsed from an unresolved fragment," and no `proc_macro` span is projected
  into a themelios `Location`. The §11 witness compares up to provenance, so the
  choice is invisible to acceptance and explicit to a reader.
