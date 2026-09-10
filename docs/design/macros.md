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
the syntax tier's parser (through the token-source door the syntax tier built
for this consumer) and the program tier's constructors, which spec §11 places
"after stages 2–3." The extraction and registration attributes, and the
solve-adjacent macros, are named here as deliberate absences (§4) and land with
the tiers they front.

---

## 1. What themelios-macros is

`themelios-macros` is the third *surface* over construction, above the two the
program tier already provides (program §7): the spelled-out constructors, and
the raise. It is **sugar over those constructors — a spelling, not a
representation.** Everything a macro builds is buildable by hand; the macro adds
a register that reads as the logic it declares, and reaches nothing a
hand-written constructor call could not.

It is a **procedural-macro crate**, and thus a *client* of two lower tiers
(program §7.4):

- of **`themelios-syntax`** — the real parser under the macro dialect (grammar
  §9), reached through the **token-source door the syntax tier built and named
  for this consumer** (syntax §4.3, §15): the macro is a `TokenSource`, one of
  the two sources "one parser reads," so law 1 is "discharged by construction."
- of **`themelios-program`** — the smart constructors (program §7.1) a macro
  expands to, and the raise (program §8) whose diagnostics it borrows at compile
  time and whose splice refusal (`UnexpandedSplice`) is its safety net.

The crate adds **no representation of its own**: no ASP grammar, no `Program`
variant, no fragment reader. Its two owned artifacts are both spellings, not
representations: (a) the **`TokenSource`** that maps Rust's token model onto the
roster (grammar §9), emitting the syntax tier's `SPLICE` token where a `$` marks
one; and (b) the **codegen** that walks the parser's *typed AST* and emits the
program tier's constructor calls — the "expands to the public constructors, a
spelling" of program §7.4. The equality witness (§11) holds that spelling exact.

**This tranche delivers nine construction macros** (§4, §8): `atom!`, `fact!`,
`rule!`, `constraint!`, `minimize!`, `maximize!`, `show!`, `external!`, and the
program-level block `program!`. What it is *not*, this tranche: the
`#[derive(Extract)]` / `#[derive(Facts)]` and `#[external]` attributes, and the
solve-adjacent `scenario!` / `query!` — each a named, reasoned absence (§4).

## 2. What this design is for

The specification fixes the macro tier's law and floor (spec §8), the grammar
document fixes the interpolation dialect (grammar §9), the syntax tier builds the
token-source door and the `SPLICE` roster the macro drives (syntax §4.3, §6.1,
§15), and the program tier states what this tier expands to (program §7.1, §7.4).
This document turns those into an implementable design: the expansion
architecture (§5), the dialect realization (§6), the splice surface (§7), the
vocabulary with its signatures (§8), the diagnostics (§9), the dependency and
trust posture (§10), and the assurance that holds it (§11).

Its acceptance is program §16's construction half of the *first-solve* witness:
a program built through the spelled-out constructors and through the macros is
**structurally equal**. That equality is the proof a construction macro is only
sugar, and it is this tier's load-bearing test (§11).

**This design has failed when any of the following holds** (the negations of its
claims, gathered here as program §2 and grammar §2 gather theirs, so a reader
checks drift at a glance):

- The equality witness fails for any macro — a value it builds is not
  structurally equal (up to provenance, program §5.2) to the value built through
  the spelled-out constructors.
- A macro reaches a `Program`, `Statement`, or `Term` other than through the
  public constructors (program §7.1) — a second representation appears (spec §8
  law 2).
- A second reader of ASP syntax appears — anything other than the syntax tier's
  parser, reached through its token-source door, lexes or parses ASP (spec §2
  item 3, §5.2; spec §8 law 1).
- A splice becomes a second door into construction — a spliced value reaches a
  `Program` other than by crossing the conversion pillar (program §3.4) into a
  public constructor argument (grammar §9; spec §8 law 2).
- A macro-site syntax error is not the file parser's diagnostic mapped onto the
  macro's spans (spec §8 law 1; spec §2 item 9).
- A deferral (§4) ships without its reason, or without the clean compile-time
  refusal that marks its boundary.
- A panic escapes a macro on any input, or a documented failure is undocumented
  (spec §2 item 8, §4).

The tier's placement is spec §11, stage 4: the crate "exists from the first
stage it can client; its vocabulary accretes with its enablers — construction
macros after stages 2–3." Stages 2 (syntax) and 3 (program + analysis) are
built; the construction surface and the token-source door are complete; so this
tranche is due.

## 3. The three laws — the correctness spine

Spec §8 binds every macro by three laws. They are this design's backbone, and
every later section realizes one.

1. **One grammar.** A macro that ingests ASP syntax hands its token stream to
   the real parser at compile time; a macro-site syntax error is the same
   rust-analyzer-grade diagnostic the file parser gives, mapped onto the macro's
   spans. *Realized in §5–§6: the macro is a `TokenSource` (syntax §4.3), so "one
   parser reads both" and the law is discharged by construction, not by a bespoke
   reader.*

2. **No second representation.** A macro expands to public smart-constructor
   calls only; everything a macro does is expressible spelled-out. *Realized in
   §5, §7: the expansion is §7.1 constructor calls generated from the parser's
   typed AST — "a spelling" (program §7.4) — and program §16's equality witness
   holds the spelling exact.*

3. **Specified interpolation.** Splicing Rust values into program syntax is the
   grammar document's macro dialect (grammar §9), carried structurally by the
   syntax tier's `SPLICE` token and `SPLICE_TERM` node (syntax roster), not an
   ad-hoc behavior. *Realized in §6 (the `TokenSource` emits `SPLICE`) and §7
   (the splice's conversion crossing).*

The laws also draw this tier's razor, which program §7.4 states and §4 applies:
**a macro that cannot reduce to a trivial expansion over an already-complete
surface is out of scope** — either its target has a gap (a missing constructor,
accessor, or conversion — *stop and report it*, never paper over it in the macro)
or its target does not exist yet (*defer it*, and say so). Cleverness in a macro
is the smell that the surface beneath is incomplete; the syntax tier's
token-source door and `SPLICE` roster (syntax §4.3, §15) are the complete surface
this tier is built to consume, not to reinvent.

## 4. The tranche boundary

program §7.4 lists the macro tier's *eventual* vocabulary. The razor of §3
decides what belongs in *this* tranche: a macro belongs when it reduces to a
trivial expansion over the now-complete syntax (token-source door + `SPLICE`
roster) and construction surfaces. Each absence below is deliberate, so a reader
meets it as a decision, not an omission (program §7.4's own discipline).

**In this tranche.** The construction macros that front the program §7.1
constructors and parse through the syntax tier's fragment entries: `atom!`,
`fact!`, `rule!`, `constraint!`, `minimize!`, `maximize!`, `show!`, `external!`
— the last the `#external` **directive** macro (a statement in the program,
program §4.8), distinct from the `#[external]` attribute below — and the
program-level block `program!`. Their targets all exist at this tier's base (§8
names each), so each reduces to a trivial expansion.

**Theory atoms are in; theory-term splices are deferred by scope.** A theory
atom that arrives *without a splice* — `&sum { 1, 2 } <= n` — is ordinary syntax
to the parser and codegens through the theory-atom constructors like any other
node (the theory-atom argument-list pool remains the raise's stated §17
exception, unchanged by this tier). A **splice into a theory-term position** —
`&sum { $x } <= $bound`, which grammar §9 places in the v1 floor — the syntax
tier *does* carry structurally (`ast::TheoryTerm::Splice`, syntax roster), and
codegen *could* land a ground splice as `TheoryTerm::Symbolic(x.to_symbol())`
(program §4.9). It is deferred as a **deliberate scope line**, not a limitation:
this tranche's splice surface is the ordinary-term position (matching the
theory-atom "no splice" line above), and the theory-term splice is held for a
focused increment alongside the theory-term surface the solve/query stage
exercises (program §17). Until then a `$` in a theory-term position is a
macro-site error with a clear message (§9), not a silent miss.

**Deferred to the solve stage** (they front a surface that does not exist until
solve, and sit behind the pre-solve discussion):

- `scenario!` — a reusable, named assumption configuration (spec §8's coined
  name). Assumptions are a solve-session concept (spec §9.4), and spec §3.2 maps
  `scenario!` to *blame*, a solve witness. **This design departs from program
  §7.4, which groups `scenario!` among the construction macros** — recorded here
  and in §13, not left for a reader to reconcile: its surface is a solve-session
  concept, so it lands with the solve tier, not this tranche.
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
constructor family; the engine is shared.

For an invocation, the engine runs one pipeline, entirely at **compile time**
except the constructor calls it emits:

1. **Become a `TokenSource`** (syntax §4.3). The engine walks the Rust token
   stream per the dialect mapping (§6) and assembles a themelios text from the
   tokens' spellings, of which it is the authoritative tiler: it answers
   `token_at(at, mode)` from its own structured knowledge of where each token
   begins and ends — including forming theory-operator runs and `#`-keywords
   under the mode the parser requests — so **there is no re-lex whose fusion must
   be self-checked** (syntax §4.2). Where a `$` marks a splice it emits the
   syntax tier's `SPLICE` token; the source declares its own `id()` (§9,
   provenance).
2. **Parse through the fragment entry** for the category
   (`parse_statement` / `parse_term` / `parse_program`, syntax §6.1), driven over
   that `TokenSource`. The result is the syntax tier's **typed AST**, carrying
   splices structurally as `ast::Term::Splice` / `ast::TheoryTerm::Splice` nodes
   (syntax roster). `check_token_source_laws` (syntax §4.3) validates the source
   — the honest, tier-provided replacement for any bespoke reconstruction check.
3. **Diagnose at the rust-analyzer bar** (law 1). The engine surfaces the parse's
   syntax diagnostics and, by raising the tree for its lowering diagnostics
   (program §8), the lowering diagnostics too — **filtering the expected
   `UnexpandedSplice`** (a splice is not an error here; the macro owns it, program
   §8). A splice's node is inert to the raise's other checks (it lowers to an
   anonymous placeholder, program §8, which is a well-formed term), so the filter
   is exact. Every surviving diagnostic — and any admission refusal, an
   over-`MAX_LEN` assembled text (syntax §12.4) — is re-emitted as a compile error
   at the **Rust span** the offending token came from (§9). An input that would
   diagnose fails *at compile time*.
4. **Codegen from the typed AST.** The engine walks the AST and emits the §7.1
   constructor calls that build the value: `Term::function` / `Atom::new` /
   `Rule::new` / the directive constructors, one arm per AST node family. At each
   `Splice` node it emits the spliced Rust expression crossed to a ground term
   (§7). The expansion is these public constructor calls and nothing else —
   law 2.

**No lowering at runtime, and no runtime parse.** The expansion is a tree of
constructor calls; at runtime it builds the value directly, splices resolved.
The raise is used only at compile time (for diagnostics) and stands at runtime as
the safety net its `UnexpandedSplice` names: a splice can never reach a built
`Program`, because the macro expands it, and if a future path ever let one
through, the raise refuses it.

**Two spellings of one lowering, held equal.** The codegen (AST → constructor
calls) parallels the raise (AST → `Program`); they are two spellings of the same
lowering, and program §16's equality witness (§11) is the proof they never
diverge — the discipline program §7.4 sets for "a macro adds no representation,
only a spelling." This is not the "one authority" of a single runtime lowering;
it is a spelling whose faithfulness is *tested*, which is what §7.4 asks and §16
delivers.

**Provenance.** Because the expansion calls the constructors, every node a macro
builds carries `Origin::Constructed` (program §6) — identical to a value built by
hand, which is why the §11 witness holds up to *and including* provenance class.
The macro projects no `proc_macro` span into a themelios `Location` (they are
different coordinate systems); a consumer that needs a value located in real
source parses a real file through the raise.

**The honest cost.** The compile-time parse-and-raise is `O(fragment)` and runs
once per invocation site, at compile time; the runtime cost is the constructor
calls alone, the same a hand-written program pays. There is no runtime parse to
memoize and no double lowering at runtime — the codegen replaces both.

## 6. The macro-dialect realization

Grammar §9 defines the dialect over Rust's token model and assigns its
realization to this crate; the syntax tier provides the door (a `TokenSource`)
and the target token (`SPLICE`). The engine implements the mapping and answers
`token_at`; it invents no syntax.

**The token mapping** (grammar §9, as the `TokenSource` answers it):

- A Rust identifier lexes by the name classes — lowercase-initial an
  `IDENTIFIER`, uppercase-initial a `VARIABLE`, `_` alone `ANONYMOUS`, `not` the
  keyword; an identifier no class matches whole (`__`, `_1`) is a dialect error.
- A Rust integer literal is a `NUMBER` **by value**; a Rust string literal a
  `STRING` **by value** (raw strings included). The source chooses the spelling
  it tiles (syntax §4.3); a value grammar §4.4 cannot spell is carried as a
  splice of that value (§7), not respelled.
- `#` forms a keyword exactly when *span-adjacent* to the keyword's word (and,
  for `#sum+`, the `+` beyond it), read from the `proc_macro` spans; a `#`
  separated from its word is a dialect error.
- Rust punctuation maps one-to-one onto the operator roster; a multi-character
  operator exists where its characters are adjacent and joined, and theory-
  operator runs form the same way inside theory expressions — the source forms
  them under the parser's `Theory` mode, as the file lexer forms them from
  adjacent bytes (syntax §4.2).
- Comments do not exist in the dialect (Rust has removed them).
- `$` emits a `SPLICE` token over its marker and operand (§7).
- Every Rust token the mapping does not name is a dialect error at its span —
  float, char, and byte literals, suffixed numerals, lifetimes, raw identifiers
  (`r#not` is an error, never a way to spell the reserved name).

**The source owns its boundaries.** Because the engine answers `token_at` from
its own knowledge, two Rust identifiers that abut do not fuse and an intended
adjacency is never lost — the boundary questions a re-lex would raise do not
arise (syntax §4.2). The four token-source laws (tiling, slice, determinism,
refusal — syntax §4.3) are what the source owes, and `check_token_source_laws`
is the standing check it passes; the `Theory` and `ScriptBody` modes the checker
does not exercise are held under the engine's own tests, over the inputs its
parser reaches (syntax §4.3).

**The span map.** Alongside the assembled text the engine records, for each
token, the `proc_macro` span it came from, so a diagnostic located in the text
(§5.3) maps back to the offending Rust token (§9).

## 7. Splices and the conversion pillar

A splice is grammar §9's interpolation: `$name` splices the value of a Rust
binding, `$( … )` splices any Rust expression. It stands where a **term** may
stand (the theory-term position is deferred by scope, §4). The `TokenSource`
emits a `SPLICE` token for it (§6), and the parser carries it as an
`ast::Term::Splice` node — no placeholder variable, no re-lexing.

**The conversion crossing.** A spliced value is a Rust value that *denotes a
ground symbol*: it crosses the conversion pillar's `ToSymbol` (program §3.4),
the one surface the ground-time `@`-functions, read-time extraction, and these
splices all share, so the three never diverge. At each `Splice` node the codegen
(§5.4) emits the value crossed to a ground term — `ToSymbol::to_symbol` then
`From<Symbol> for Term` (program §3.4, the lossless-inward door) — as the
constructor argument in that position. The splice is thus never a second door
into construction: it is a value handed to the same conversion pillar every
other extension point uses, landing in the same public constructor a spelled-out
term would.

**Refusal is at the door, at compile time.** A spliced value whose type is not
`ToSymbol` is a *compile error* — the trait bound the emitted conversion carries
is exactly "refuses at the constructor doors the expansion calls" (grammar §9;
spec §8 law 2). There is no runtime splice refusal: the admitted ground types
(`i8`…`i32`, `u8`, `u16`, `str`, `String`, and `Name` through `Symbol::constant`)
convert infallibly (program §3.4), and a fallible landing — an `f64` through a
rounding adapter (program §3.4) — is written *by the caller inside the splice*
(`$( round(x)? )`), where its `Result` is the caller's, not the macro's to hide.

**The asymmetries, stated.** By-value literals mean macro bodies admit spellings
files do not and the converse: a Rust string's escapes produce string values
grammar §4.4 cannot spell (carried as a splice of the value, §6), and a Rust
numeral may be `0o17` (the value crosses, the spelling does not). **Primed
names** (`a'`) are inexpressible in macros — Rust identifiers carry no primes —
and remain expressible through the spelled-out constructors, the direction spec
§8 law 2 guarantees; the converse is not promised (grammar §9). A theory-term
splice is deferred by scope (§4).

## 8. The vocabulary

Each macro fixes a grammatical category and a target family, parses ASP under
the dialect (§6) with splices (§7) through the syntax tier's fragment entry
(syntax §6.1), and codegens (§5.4) the value. All parse under `Dialect::Clingo`
— the richer, membership-authority dialect (grammar §3); a consumer wanting
ASP-Core-2 semantics reaches for the raise doors directly. Signatures name the
*value each builds*; the by-hand equivalent it equals is the program §7.1
constructor named.

- **`atom!(-? name(args…))` → `Atom`.** Parsed in **atom (head) position**, so a
  leading `-` is *strong* negation (`Sign::Negative`), the positional reading the
  tree resolves (program §3.3, §8) and `impl Neg for Atom` (program §7.1)
  mirrors — not arithmetic negation of a term. A fragment that is not a single
  atom is a macro-site error. Equals `Atom::new` / `Atom::constant`.
- **`fact!(head)` → `Rule`.** A fact — a head with the empty body. Equals
  `Rule::fact`.
- **`rule!(head :- body)` → `Rule`.** A rule read as the rule. Equals
  `Head::when`.
- **`constraint!(:- body)` → `Rule`.** An integrity constraint. Equals
  `Rule::constraint`.
- **`minimize!(…)` / `maximize!(…)` → `Optimize`.** An optimization statement,
  each element a weighted term at a priority. Equals `minimize` / `maximize`
  (program §4.7).
- **`show!(…)` → `Show`.** A `#show` directive. Equals the `Show` constructor
  (program §4.8).
- **`external!(…)` → `External`.** A `#external` **directive** (the atom, its
  body, and the carried-not-meaningful value, program §4.8). Equals
  `External::new`. Distinct from the `#[external]` attribute (§4).
- **`program!{ s₁. s₂. … }` → `Program`.** A block of statements-with-splices,
  parsed through `parse_program` and codegen'd as one program (assembled through
  `Program::of`, program §7.1) — the natural inline-ASP surface for the ASP
  author (program §7.3), the program-level of spec §8's levels.

**Composition and return types.** A statement macro returns its *specific* family
type (`Rule`, `Show`, `External`, `Optimize`), not an erased `Statement`, because
the specific type is the more useful value and composes into a program through
the program tier's statement coercions (`Into<Statement>`, program §7.1) at no
ceremony. `atom!` returns `Atom`; `program!` returns `Program`. All roads reach a
structurally-equal `Program` (§11).

## 9. Diagnostics

Law 1 requires a macro-site syntax error to read as the file parser's does. The
engine delivers this through the **span map** (§6): the compile-time parse and
raise (§5.3) produce diagnostics located in the assembled text; each is
translated through the span map to the `proc_macro` span of the Rust token that
produced it, and emitted as a compile error there. Because the `TokenSource`
owns its boundaries (§6), a token's text is exactly a Rust token's spelling, so a
diagnostic never lands on synthetic separator text — there is none.

The diagnostics carried are the syntax tier's `SyntaxError` and the program
tier's `LowerError`, both lowering to base's normal form (base §6.5; program §8)
— one model, so a macro-site diagnostic reads exactly as the file parser's, at
the rust-analyzer bar (spec §2 item 9). Dialect errors of the mapping itself
(§6 — a float literal, a detached `#`, `r#not`, a `$` in a theory-term position,
§4) are the engine's own diagnostics, located at the offending Rust token's span
and worded in the same register.

**Hygiene.** The expansion references program-tier items by absolute path
(`::themelios_program::…`), so it compiles regardless of the caller's imports. A
`$( … )` splice's expression is emitted in the caller's context — it *should* see
the caller's bindings, which is the point of a splice. A proc-macro crate can
export only macros, so the constructors the expansion names come from the
caller's dependency on `themelios-program`; the eventual `themelios` facade (spec
§11, stage 8) will re-export the macros beside the runtime so a consumer names
one crate — a stated forward dependency, not this tier's to resolve.

## 10. Dependencies and trust

Spec §12.5 rules the posture: **`-macros` carries the proc-macro toolchain
only.** Concretely:

- **Compile-time dependencies of the macro crate:** `themelios-syntax` (to parse
  through the token-source door) and `themelios-program` (to raise for
  diagnostics), plus **`proc-macro2` and `quote`**. `proc-macro2` is argued: its
  token types can be constructed and manipulated *outside* a compile invocation,
  so the dialect mapping and the `TokenSource` (§6) — the crate's owned artifacts
  — are unit-tested directly against the coverage and property discipline (§11)
  rather than only through a compile harness; `quote` is the ergonomic emission
  of the constructor calls. Both are pinned, ubiquitous, and run only at compile
  time.
- **`syn` is declined.** This crate walks a *bespoke* token grammar (the dialect,
  §6), not Rust's grammar; Rust-AST parsing is the wrong tool, and hand-walking
  the token stream is the dependency policy's default ("hand-writing is the
  default where hand-writing is reasonable", spec §12.5). The one place a Rust
  expression is handled — a `$( … )` splice — is *captured and re-emitted*, a
  token-group operation `proc-macro2` serves without parsing.
- **Runtime dependency of the expansion:** `themelios-program` alone (the
  constructor calls). The expansion names no syntax-tier type, so a consumer's
  runtime graph gains only the program tier it already has.

The proc-macro toolchain runs **at compile time**; it is in no shipped closure.
`forbid(unsafe_code)` holds; no build script; the structural trust checks
(FFI-free, no build script) apply as in the tiers beneath (program §16). The
compile-fail instrument (§11) is a **dev-dependency** (`trybuild`), outside the
shipped closure exactly as `proptest`, `criterion`, and `serde_json` are beneath.

## 11. Assurance instruments

Per spec §11 the stage is not done until these are green; each is documented with
what it proves and what it cannot (spec §10.2).

- **The equality witness — the load-bearing acceptance** (program §16, the
  construction half of *first-solve*, spec §3): for every macro, the value it
  builds is **structurally equal** (up to, and here including, provenance —
  program §5.2, §6) to the value built through the spelled-out constructors it
  names in §8. This is the proof the codegen is a faithful spelling of the
  constructors. A macro that *cannot* be made to satisfy it is the §3 razor
  firing — a signal the surface beneath has a gap — reported (stop), never
  papered over.
- **The token-source laws** (`check_token_source_laws`, syntax §4.3): a standing
  check that the macro `TokenSource` tiles and slices lawfully, over generated
  and corpus inputs; the `Theory` and `ScriptBody` modes the checker does not
  reach are held under the engine's own mode tests (§6).
- **The floor mapping** (spec §3.2): each in-tranche macro carries the witness
  that exercises it — `atom!` / `fact!` / `rule!` / `constraint!` and `program!`
  under *first-solve*, `minimize!` / `maximize!` under *optimization*, `show!`
  under *enumeration*, `external!` under *multi-shot*. Where a witness's
  *behavior* needs the solve tier (multi-shot for `external!`), the
  structural-equality half is proved here and the behavioral half is seeded for
  the stage that runs it; a macro absent from this mapping would be a visible gap
  (spec §3.2). The deferred macros (§4) carry their witnesses when they land.
- **Property laws (proptest)** over the crate's owned logic: the dialect mapping
  (§6) — every named Rust token maps to its roster token, every unnamed token is
  a dialect error; and a **splice round-trip** — a program written with splices
  of generated ground values equals the same program written with those values
  spelled in place (the equality witness, generated).
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

- **Theory-term splices** (§4): feasible under this tranche's codegen (a ground
  splice as `TheoryTerm::Symbolic`, program §4.9) but held by scope; reopen as a
  focused increment with the theory-term surface the solve/query stage exercises
  (program §17).
- **Further splice sites** — names, tuples, statements (grammar §9): future
  vocabulary, each admitted on argument as the tiers accrete; the v1 floor is the
  term (and, deferred by scope, the theory term).
- **The extraction and registration attributes** — `#[derive(Extract)]`,
  `#[derive(Facts)]`, `#[external]` (§4): land with the structured-decode seam
  (program §3.4, §17) and the `@`-function surface (spec §9.6).
- **The solve-adjacent macros** — `scenario!`, `query!` (§4): land with the
  solve session and query surfaces they front.

Non-goals, absolutely: a second parser or grammar of ASP (spec §2 item 3, §5.2)
— the one grammar is the syntax tier's, reached at compile time through its
token-source door; assembling ASP as a runtime string to re-parse (never
render-then-parse — the macro tiles tokens through a `TokenSource`, at the token
level, and codegens the value); styled formatting (the formatter satellite); and
any representation of a program (the program is the program tier's, always). The
codegen (§5.4) is emphatically **not** a non-goal: it is spec §8 law 2's "expands
to the public constructors, a spelling," and program §16's witness is what keeps
it a spelling and not a second representation.

## 13. Revisions

Refinements this design makes to the specification and to the sibling designs,
recorded here rather than left silent so their successors carry them; each a
deliberate evolution with its argument, not a drift.

- **`program!` is in this tranche.** Spec §8's floor set enumerates the
  statement- and element-level construction macros and names "program-level" as
  a level, but lists no `program!` by name. This design admits the program-level
  block now: it is enabled by stages 2–3 (the fragment entries and the
  constructors), it is the natural inline surface for the ASP author (program
  §7.3), and it builds the `Program` the §11 witness compares.
- **`scenario!` is classed solve-adjacent, departing program §7.4** (§4). program
  §7.4 groups `scenario!` among the construction macros; this design defers it to
  the solve stage, because its surface (a named assumption configuration) is a
  solve-session concept (spec §9.4) and spec §3.2 maps it to a solve witness
  (*blame*). Recorded so the two tier documents do not silently disagree.
- **Theory atoms in, theory-term splices deferred by scope** (§4). Grammar §9
  places the theory-term splice in the v1 floor; this design delivers theory
  *atoms* (which parse and codegen like any node) and defers the theory-term
  *splice* — feasible under codegen (`TheoryTerm::Symbolic`) but held as a scope
  line for a focused increment, its reopening named.
- **The proc-macro toolchain, read as `proc-macro2` + `quote`, `syn` declined**
  (§10). Spec §12.5 says "the proc-macro toolchain only"; this design reads that
  as the minimal set the job needs and argues `syn` out, the bespoke token
  grammar making Rust-AST parsing the wrong tool.
