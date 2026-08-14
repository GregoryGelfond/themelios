# themelios-base — tier design

2026-08-13. Design for review, pre-implementation. This document is the
API design of `themelios-base` — the types, traits, signatures, semantics,
and computational costs of the foundation's lowest tier — derived from the
v1 specification (`docs/specification.md`), cited throughout as *spec §n*;
a bare *§n* cites this document's own sections. It is written to stand
alone in the same sense the
specification is: a reader holding this repository and public sources can
check every claim. Where this document and the specification disagree, the
specification governs and the disagreement is a defect here.

---

## 1. What themelios-base is

`themelios-base` is the shared foundation under every tier: the
source-text model, spans, line indexing, and the diagnostics model
(spec §11 stage 1, spec §12.2) — the vocabulary of *location* and
*report* that every other crate speaks. It is deliberately the one crate
with no ASP in it: nothing here lexes, parses, solves, or knows the
language exists. A diagnostic about a Rust-embedded snippet, an ASP file,
or any future text this ecosystem touches is the same value.

**Naming ground, stated once per spec §1.4.** The public vocabulary of this
crate — span, diagnostic, severity, label, line index, source — is drawn
from the language-tooling literature (the working vocabulary of compilers,
rust-analyzer, and the editor protocols), not from the KR/ASP literature,
which has no words for these objects. The reason, owed where the
specification's naming rule demands one: this tier's subject matter *is*
tooling infrastructure, and its nearest audience is the tool builder
(spec §1.3), whose literature this is. Names below that depart from common
tooling usage carry their own stated reasons in place.

**Crate facts, carried as constraints.** Zero dependencies in the
shipped library's closure (spec §12.5); the §10 instruments are
dev-dependencies, outside that claim. `#![forbid(unsafe_code)]` with the
workspace trust checks asserting an FFI-free dependency closure and no
build script (spec §12.3). The workspace `rust-version` floor (spec
§10.1). Every public type is plain data: `Send`, `Sync`, no interior
mutability, no global state, no I/O (spec §1.2).

**Module map.** Five modules, one concern each: `source` (§3), `span`
(§4), `line` (§5), `diagnostic` (§6), `view` (§7).

## 2. What this design is for

The postcondition, stated so a review can check drift against it:

> themelios-base gives every tier one shared, typed vocabulary for source
> text, location, and reporting, such that a report from any tier yields
> the human view, the editor-protocol view, and the machine form from the
> same value; no consumer parses rendered prose to act; the crate embeds
> anywhere — no I/O, no globals, no runtime; and every public operation
> is total and observationally pure.

This design has failed — independent of any local defect — when any of
the following holds:

- A diagnostic lacks a precise span or a stable identity (spec §2 item
  9, spec §4).
- A consumer must parse rendered prose to act on any result (spec §1.5).
- A panic escapes any public operation on any input, or an operation
  ships without documented failure semantics (spec §2 item 8).
- A public operation's result depends on anything but its explicit
  inputs, or it mutates anything except through an explicit `&mut` in
  its signature.
- A dependency appears in the shipped library's closure (spec §12.5 —
  the §10 instruments are dev-dependencies outside the claim, the only
  reading consistent with spec §12.5 and spec §10.1 jointly), unsafe
  code appears (spec §12.3), or ASP knowledge appears in this crate.
- **The syntax tier cannot express spec §6.6's demands** — stable
  namespaced identities, primary and secondary labeled spans,
  expected-set reporting at recovery points — through this model and its
  lowering contract without prose-parsing anywhere.
- **The macro tier's law is inexpressible over this model** — spec §8,
  macro law 1: a macro-site syntax error is the same diagnostic the file parser
  gives, mapped onto the macro's spans. Concretely: a diagnostic must be
  re-targetable into an embedding host's coordinate system without loss
  and without parsing prose.
- Line/column arithmetic misplaces a position on multi-byte text.

The last three are serviceability conditions: they bind this tier to its
first real consumers' stated needs, and the instruments in §10 hold the
final one.

## 3. The source-text model

A source is *text with an identity*, from anywhere — file contents, a
REPL line, an editor buffer, a snippet embedded in another document. This
crate does no I/O and never sees a path; text arrives owned, and where it
came from is the host's knowledge.

### 3.1 Identity is minted by the host

```rust
/// An opaque identity for one source text. The embedding host mints it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn new(raw: u32) -> SourceId;
    pub const fn get(self) -> u32;
}
```

The host mints identity because the host already has it: a language
server maps its file identities one-to-one; a command-line tool numbers
its inputs; a macro names the snippet it is expanding. A library that
minted identities itself would force every such host to keep a second
ledger. `u32` because identity pairs into compact locations (§4.3) and
four billion sources exceeds any embedding.

### 3.2 The Source value

```rust
/// One owned source text and its identity. UTF-8 by construction.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Source { /* id, text: private */ }

// Refusal conditions shared across operations are defined once, as
// structs, and wrapped where used; a condition private to one
// operation may live inline in its enum. Every operation's error
// type is exactly the condition — or enum of conditions — it can
// produce, so a signature never claims a refusal its operation
// cannot issue.

/// Text longer than `Source::MAX_LEN` bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TooLarge { pub len: usize }

/// Bytes that are not valid UTF-8; `valid_up_to` mirrors the standard
/// library's error detail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidUtf8 { pub valid_up_to: usize }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FromBytesRefusal {
    TooLarge(TooLarge),
    InvalidUtf8(InvalidUtf8),
}

impl Source {
    /// The admission ceiling: offsets are u32 and every text has one
    /// more line start than newline bytes, so text is at most
    /// u32::MAX - 1 bytes — the line count then fits u32 for every
    /// admissible text. The name exists so the limit is never a bare
    /// numeral at a call site (spec §5.2, no magic numbers).
    pub const MAX_LEN: usize = u32::MAX as usize - 1;

    pub fn new(id: SourceId, text: String)
        -> Result<Source, TooLarge>;
    pub fn from_bytes(id: SourceId, bytes: Vec<u8>)
        -> Result<Source, FromBytesRefusal>;

    pub fn id(&self) -> SourceId;
    pub fn text(&self) -> &str;
    /// The covering span: ByteOffset::ZERO to the one-past-end offset.
    pub fn span(&self) -> Span;
    pub fn end(&self) -> ByteOffset;
    /// The spanned text. Refuses out-of-bounds and non-boundary ends.
    pub fn slice(&self, span: Span) -> Result<&str, SliceRefusal>;
}

/// The shared boundary condition: an offset inside a multi-byte
/// character. Defined once, at the first door where span meets text;
/// §5's position queries wrap the same condition.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NotCharBoundary { pub offset: ByteOffset }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliceRefusal {
    OutOfBounds { end: ByteOffset, max: ByteOffset },
    NotCharBoundary(NotCharBoundary),
}
```

**No repair at the door.** No lossy replacement character, no BOM
stripping, no line-ending normalization: author bytes are data, and
refusal beats repair (spec §5.2). Admission is the one well-formedness
authority for text — everything downstream (line indexes, slicing,
rendering) rides on a `Source` and inherits its guarantees, the
one-authority idiom of spec §7.3 applied to text.

**UTF-8 by construction is what squares spec §6.2 with the tree.** The
specification requires the lexer to be total on every byte sequence and
the syntax tree to hold text losslessly; the tree's text is UTF-8. The
reconciliation: arbitrary bytes meet a typed refusal *here*, at
admission, and everything past the door is valid UTF-8. Totality means no
panic — it never meant no refusal.

**Computational cost.** `new` is O(1) beyond the length check;
`from_bytes` is O(n) validation; accessors are O(1); `slice` is O(1)
(bounds and boundary checks against the owned text). Cloning copies and
is linear.

### 3.3 The embedded source, a named scenario

Text hosted inside another document — the string in a construction
macro's call, a REPL cell, a region of an editor buffer — is a source in
its own right. The host mints its `SourceId`, names it for display (§3.4)
in the host's own terms — for a macro, the call site — and every span
and line in every diagnostic is relative to the *snippet*. A multi-line
snippet genuinely has internal lines, and they are the lines the
diagnostic speaks of; the host document's coordinates are a different
coordinate system, reached by the host's own re-basing arithmetic, which
is exact because spans are byte-precise. This crate never conflates the
two systems because it only ever speaks source-relative.

Two obligations attach, both dischargeable with what this tier ships:

- **Re-targeting is lossless.** A diagnostic is typed data with
  byte-precise spans and a stable identity, never prose — so a host can
  translate it into a foreign reporting system (the Rust compiler's, for
  a macro) without parsing anything. The macro-law serviceability
  condition in §2 binds exactly this.
- **Degradation is honest and maximal.** Where a host platform caps
  re-targeting precision (a compiler toolchain that can only point at a
  whole string literal), the diagnostic still states its exact
  snippet-relative location, because the model carries it
  unconditionally. The worked form: the host emits its platform
  diagnostic pointing where the platform allows, and embeds this crate's
  own rendering of the snippet — caret line, exact column, the same
  message — as an attached note. That rendering is a pure function over
  `(Diagnostic, Sources)` (§7.1), callable wherever the host runs,
  including inside a compile-time macro expansion.

### 3.4 The Sources trait and the shipped implementor

Views resolve identity to display data through one small trait — the
view environment:

```rust
/// The view environment. Two laws bind every implementor — see below.
pub trait Sources {
    fn name(&self, id: SourceId) -> Option<&str>;
    fn text(&self, id: SourceId) -> Option<&str>;
    fn line_index(&self, id: SourceId) -> Option<&LineIndex>;
}

/// Which accessor a resolution lacked — used by the law checker's
/// report, the editor view's refusal, and the human view's placeholder.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceFacet { Name, Text, Index }
```

Unknown identities answer `None` — a refusal, never a panic. The `name`
is whatever the host declares; it is display data, not a path.

**The trait's laws, stated as contract.** The three accessors are not
independently partial; an implementor is bound by:

1. **Completeness.** For a given id, answer all three or none. Partial
   resolution — `text` resolving while `line_index` does not — is a
   contract breach, not a state the views must interpret.
2. **Coherence.** `line_index(id)` is observationally `LineIndex::of`
   applied to an admitted `Source` of `text(id)` — the index *of* that
   text, not of any earlier version of it. A host that caches an index
   across an edit and answers with the stale one has breached this law,
   and the breach is the one route to §2's misplaced-caret failure that
   no in-crate instrument can see.

What the views can check, they check: a completeness breach is
detectable at view time, and each view names it (§7.1's placeholder,
§7.2's refusal — both carry the missing `SourceFacet`). Coherence is
*not* cheaply checkable at view time — verifying it means rebuilding the
index — so the views trust it, and that trust is this contract's stated
boundary. What holds coherence instead is test-time machinery:

```rust
/// The laws, checkable: verifies completeness per id and coherence by
/// re-deriving the index from the resolved text. Deterministic, total,
/// O(total resolved text). Implementors run it in their own tests.
pub fn check_sources_laws(
    sources: &impl Sources,
    ids: &[SourceId],
) -> Vec<SourcesLawViolation>;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SourcesLawViolation {
    Incomplete { id: SourceId, missing: SourceFacet },
    IncoherentIndex { id: SourceId },
}
```

An empty report is the laws holding over those ids. This checker is the
instrument §10 lists for the seam every host crosses.

One implementor ships, for tests, witnesses, and single-file tools:

```rust
/// A Vec-backed catalog that mints ids sequentially — a host you can
/// use, not this crate seizing minting. Satisfies both laws by
/// construction: it admits under the Source doors and builds each
/// LineIndex eagerly at add — an explicit derivation, not lazy state.
#[derive(Clone, Debug)]
pub struct SourceSet { /* private */ }

impl SourceSet {
    pub fn new() -> SourceSet;
    pub fn add(&mut self, name: String, text: String)
        -> Result<SourceId, TooLarge>;
}

impl Default for SourceSet { /* new() — the std idiom for an
                                argument-free constructor */ }

impl Sources for SourceSet { /* ... */ }
```

`SourceSet` is deliberately not a virtual file system and never grows
toward one (§11): no paths, no watching, no loading. **Computational
cost.** `add` is O(n) (admission plus the index build); lookups are O(1).

## 4. Spans

The specification's own word for a region of source is *span*, so the
type is `Span` — not the range vocabulary some tooling uses; the
specification's usage governs (spec §2 item 9, spec §7.4).

### 4.1 The numeric spine

```rust
/// A position in a source's UTF-8 text, in bytes. The unit is in the
/// type's name so it is never in a comment.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteOffset(u32);

impl ByteOffset {
    pub const ZERO: ByteOffset;
    pub const fn new(raw: u32) -> ByteOffset;
    pub const fn get(self) -> u32;
    /// Checked arithmetic only: overflow refuses, never wraps.
    pub const fn checked_add(self, bytes: u32) -> Option<ByteOffset>;
    pub const fn checked_sub(self, bytes: u32) -> Option<ByteOffset>;
}
```

`u32`, with its argument: it is the width rust-analyzer proved sufficient
at scale; it halves location size against `usize` on 64-bit targets, and
locations are the most-copied values in the system; and the ceiling it
imposes is enforced honestly at the one admission door (§3.2), not
scattered across operations.

### 4.2 The Span value

```rust
/// A half-open byte region [start, end) in one source's text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Span { /* start, end: private — the invariant is guarded */ }

/// The one refusal `Span` construction can issue — carried as the
/// condition itself, per the per-operation refusal discipline (§3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EndBeforeStart { pub start: ByteOffset, pub end: ByteOffset }

impl Span {
    pub fn new(start: ByteOffset, end: ByteOffset)
        -> Result<Span, EndBeforeStart>;
    pub const fn empty(at: ByteOffset) -> Span;

    pub fn start(self) -> ByteOffset;
    pub fn end(self) -> ByteOffset;
    pub fn len(self) -> u32;              // bytes
    pub fn is_empty(self) -> bool;
    pub fn contains(self, offset: ByteOffset) -> bool;   // half-open
    pub fn contains_span(self, other: Span) -> bool;
    pub fn intersect(self, other: Span) -> Option<Span>;
    /// The covering span — total, including disjoint operands.
    pub fn join(self, other: Span) -> Span;
}
```

Ordering is lexicographic (start, then end), which is document order with
shorter-first ties — the order every batch consumer wants.

**Char-boundary policy, argued.** A `Span` is text-independent arithmetic
data. It cannot check UTF-8 boundaries at construction because no text is
in scope, and binding every span to a text would make spans unusable as
the plain data they must be inside diagnostics, trees, and
transformations. Boundary discipline lives where span meets text:
`Source::slice` (§3.2) and the line index (§5) refuse non-boundary
offsets with typed refusals. The invariant a `Span` does guard —
`start <= end` — is guarded at construction, because it is checkable
there and every operation's correctness rests on it.

**Computational cost.** Every operation above is O(1); the type is
`Copy`.

### 4.3 Qualified location

```rust
/// A span in a named source — the cross-source form. Fields are public:
/// any (source, span) pair is a valid value; validity against a
/// particular text is checked where text is in scope.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}
```

The program tier's provenance must reach across multiple parsed sources
(spec §7.4), so the qualified form is defined once, here.
It is deliberately parallel to the editor protocol's uri-plus-range
pairing. Ordering is (source, then span): batch order groups by source.

## 5. Line indexing

```rust
/// Line and column structure for one source's text: an explicit, pure
/// derivation you construct and hold. Does not retain the text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineIndex { /* private */ }

/// A zero-based line/column coordinate. What `col` counts is named by
/// the encoding the query stated. Fields are public: any pair is a
/// valid coordinate value; validity against a text is checked at use.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// What a column counts. UTF-16 units exist because the editor
/// protocol's default position encoding demands them (spec §2 item 9
/// makes the editor-protocol view binding).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColumnEncoding { Utf8Bytes, CodePoints, Utf16Units }

// Refusal conditions, each defined once (the §3.2 discipline); every
// query's error type is exactly what that query can produce. The
// out-of-bounds refusals carry the bound that was exceeded, and the
// boundary refusals the offending coordinate — the question the caller
// can fix.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OffsetOutOfBounds { pub offset: ByteOffset, pub max: ByteOffset }

// The boundary condition NotCharBoundary is §3.2's shared struct,
// wrapped here by PositionRefusal.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineOutOfBounds { pub line: u32, pub line_count: u32 }

/// `max` is the line's extent in the encoding the query stated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnOutOfBounds { pub line: u32, pub col: u32, pub max: u32 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnNotBoundary { pub line: u32, pub col: u32 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PositionRefusal {
    OutOfBounds(OffsetOutOfBounds),
    NotCharBoundary(NotCharBoundary),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OffsetRefusal {
    LineOutOfBounds(LineOutOfBounds),
    ColumnOutOfBounds(ColumnOutOfBounds),
    ColumnNotBoundary(ColumnNotBoundary),
}

impl LineIndex {
    /// Total: every admitted Source indexes. Riding on Source admission
    /// keeps one authority for text (§3.2); there is no &str door.
    pub fn of(source: &Source) -> LineIndex;

    pub fn line_count(&self) -> u32;
    /// The span of one line's content, excluding its terminator —
    /// renderers want the content; the terminator is derivable.
    pub fn line_span(&self, line: u32) -> Result<Span, LineOutOfBounds>;

    pub fn position(&self, offset: ByteOffset, encoding: ColumnEncoding)
        -> Result<LineCol, PositionRefusal>;
    pub fn offset(&self, pos: LineCol, encoding: ColumnEncoding)
        -> Result<ByteOffset, OffsetRefusal>;
}
```

**Representation, at design level.** Line-start offsets, plus, per line,
the non-ASCII characters with their byte and UTF-16 widths. That is
enough to answer all three encodings without retaining the text, and —
the part that matters for correctness — enough to *refuse* a
mid-character offset rather than misplace a caret. Memory is
O(lines + non-ASCII characters).

**Newline policy, stated plainly.** Lines break at `\n`; a `\r` stays in
its line's content (the Rust and rust-analyzer convention). Columns stay
exact on CRLF files; nothing is normalized, ever. A lone `\r` is not a
line break.

**In-bounds, defined.** Offsets `0..=len` are in bounds — `len` itself
is the end-of-text position, mirroring the covering span's end — and
`OffsetOutOfBounds` means strictly past `len`. Empty text has one line:
line 0, empty. Every text's positions are therefore queryable, including
both ends of every span.

**Zero-based, with one presentation exception.** `LineCol` is zero-based
because internal arithmetic and the editor protocol are; one-based
coordinates exist only inside the human rendering (§7.1), stated there as
presentation.

**One coordinate type, argued.** A `LineCol` does not carry the encoding
that gives its `col` meaning, deliberately: it is a transient query
result, and the encoding is in the caller's hand at the call that
produced it — the same checked-at-use posture `Location` states for
validity (§4.3). The editor protocol itself negotiates position encoding
once per session, not per value, so taxing every coordinate with the
field would serve no named consumer; and the §10 property laws hold the
arithmetic per encoding. The one value that *outlives* its producing
call — the editor payload — records the encoding it was derived under
(§7.2), which is where the distinction stops being transient and starts
being data. A mismatched pairing at `offset` (a coordinate produced
under one encoding, queried under another) is the caller breaching this
stated contract; on multi-byte text it surfaces as a refusal or a wrong
position, and the contract, not the type system, is what forbids it.

**Computational cost.** `of` is O(n) in the text — this linearity is what
keeps total reparse cheap (spec §6.8), and the scaling
bench holds it. `position` and `offset` are O(log lines + log
non-ASCII). `line_count` is O(1) (a stored count); `line_span` is O(1)
(a bounds check and an array lookup — the terminator exclusion is
deterministic from adjacent line starts). A changed text is a new
`Source` and a new index; there is no edit application here (§11).

**Round-trip law.** For every valid offset,
`offset(position(o, e), e) == o` in every encoding — the first property
law of §10, held by proptest against a naive character-walk oracle.

## 6. The diagnostics model

### 6.1 Identity

```rust
/// The stable machine identity of one diagnostic kind: a namespace (the
/// emitting tier) and a kebab-case name. Renders as
/// `namespace::name`, e.g. `syntax::unexpected-token`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DiagnosticId {
    /* namespace, name: private &'static str */
}

impl DiagnosticId {
    pub const fn new(namespace: &'static str, name: &'static str)
        -> DiagnosticId;
    pub const fn namespace(self) -> &'static str;
    pub const fn name(self) -> &'static str;
}
```

**No numeric codes.** An `E0417` is a magic number with a lookup table;
the no-magic-numbers policy (spec §5.2) wants the intent in the name, and at
diagnostic scale that means the name *is* the identity.

**Stability is discipline with teeth, not validation.** The constructor
is total and `const` — each emitting tier defines its identities as
compile-time constants and owns its table. Quality (kebab-case,
non-empty, meaningful) and stability are held by each tier
snapshot-testing its complete identity table: an identity, once shipped,
is stable; renaming is a visible breaking change; the registry snapshot
is the tripwire. This crate defines the type; it deliberately does not
police tables it cannot see.

### 6.2 Severity

```rust
/// Closed. Declared least-severe first, so Error is the maximum and
/// worst-first sorting is descending order. Renders lowercase.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity { Note, Warning, Error }
```

Exactly the specification's three words (spec §2 item 9): `Note` is a real
standalone severity — a solver frontend ships its engine's informational
class as its own face — not merely an attachment role. An editor-protocol
`Hint` has no v1 producer; it is recorded as known pressure at the
language-server consumer checkpoint (§11), not carried now.

**Closedness is a ruled tradeoff, both sides on the page.** Closed buys
every consumer exhaustive matching on the specification's own
trichotomy. The price, accepted here: if the recorded `Hint` pressure
lands at the language-server checkpoint, admitting it is a breaking
change through every exhaustive match — which the pre-1.0 stability
posture prices correctly (spec §13: stability is earned at consumer
checkpoints, and this surface will not have frozen before that
checkpoint runs). `#[non_exhaustive]` was considered and rejected: it
would tax every consumer with a wildcard arm on a closed trichotomy
today to hedge a pressure that has no producer yet.

### 6.3 Labels and attachments

```rust
/// A located message. Fields are public: any location with any optional
/// message is a valid label; there is no invariant to guard.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Label {
    pub location: Location,
    /// None when the diagnostic's headline already covers it — an
    /// honest absence, not an empty-string sentinel.
    pub message: Option<String>,
}
```

Unspanned attachments are two plain sequences on the diagnostic —
`notes` and `helps` (§6.4). They are sequences *because order is
meaning*: they read as narrative ("note: expected because of this", then
"note: the statement began here"), the ordered half of the modeled-shape
rule (§8.4). A single role-tagged list preserving note/help interleaving
was considered and cut: interleaving is a distinction no consumer reads —
rendering prints notes, then helps, the compiler convention.

### 6.4 The Diagnostic

```rust
/// A report about source. Located by construction: the primary label is
/// required, so "a diagnostic without a precise span" is
/// unrepresentable (spec §4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostic {
    /* private:
       id:        DiagnosticId,
       severity:  Severity,
       message:   String,              // the headline; never empty
       primary:   Label,
       secondary: BTreeSet<Label>,     // a set, mathematically
       notes:     Vec<String>,         // a narrative, in order
       helps:     Vec<String>,         // likewise
    */
}

/// The one refusal construction can issue, carried as the condition
/// itself (the §3.2 discipline): an empty headline, which would break
/// every view by construction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EmptyMessage;

impl Diagnostic {
    pub fn new(
        id: DiagnosticId,
        severity: Severity,
        message: String,
        primary: Label,
    ) -> Result<Diagnostic, EmptyMessage>;

    /// By-value chaining: even building reads as declaring.
    pub fn with_secondary(self, label: Label) -> Diagnostic;
    pub fn with_note(self, note: String) -> Diagnostic;
    pub fn with_help(self, help: String) -> Diagnostic;

    pub fn id(&self) -> DiagnosticId;
    pub fn severity(&self) -> Severity;
    pub fn message(&self) -> &str;
    pub fn primary(&self) -> &Label;
    pub fn secondary(&self) -> &BTreeSet<Label>;
    pub fn notes(&self) -> &[String];
    pub fn helps(&self) -> &[String];
}
```

**The secondary labels are a set, and the type says so.** Their render
order is *derived* (by position — `Label`'s ordering is location-first),
so emission order carries no meaning, and a duplicate label is a defect;
`BTreeSet` makes duplicates unrepresentable, iteration deterministic in
exactly the derived order, and — the consequence a sequence would get
wrong — equality *set* equality: two diagnostics with the same labels
emitted in different orders are the same diagnostic. Inserting a label
already present yields the same set; that is set semantics, not repair.

**What is not a diagnostic.** A `Diagnostic` is a report *about source*,
located by construction. Solve outcomes, faults, and progress events are
not diagnostics — they have their own models (spec §9.2, spec §9.3) —
and an unlocated report is not a degenerate diagnostic
but a different thing (§11, non-goals). A program-scope report locates at
the relevant directive or the source's covering span (`Source::span`).

**Computational cost.** Construction and chaining are O(1) beyond owned
text and O(log n) per secondary insert; clone is linear in carried text;
equality and hash are structural.

### 6.5 The lowering contract

```rust
/// Tier-typed diagnostics lower into the normal form by reference: the
/// typed value outlives its transport form.
pub trait ToDiagnostic {
    fn to_diagnostic(&self) -> Diagnostic;
}

impl ToDiagnostic for Diagnostic { /* identity, by clone */ }
impl<T: ToDiagnostic + ?Sized> ToDiagnostic for &T { /* deref */ }
```

The architecture this contract serves: each tier defines its *own* fully
typed diagnostics — the parser's expected-token set is a real type in the
syntax tier, matchable and exhaustive — and lowers them into this
crate's normal form for uniform rendering and transport. In-process
consumers act on the tier's typed values (library-first means a language
server embeds the tier; it never recovers payload from transport);
pipelines that only render or forward take `impl ToDiagnostic`
uniformly. The name departs the standard conversion vocabulary
deliberately: `Into` consumes and says only "can convert"; this trait
borrows and declares a semantic relationship — *this value is a
diagnostic in tier-typed form*. One method, no provided machinery: a
contract, not a framework.

## 7. Views

Views are derivations over the model (spec §1.5): pure
functions over `(&Diagnostic, &impl Sources)`. There is deliberately no
view trait — the open extension point for a new view is the model being
public plain data; anyone writes a function over it. The polymorphism a
view does need is over its *environment*, and that is the `Sources`
trait.

### 7.1 The human view

```rust
pub fn human(diagnostic: &Diagnostic, sources: &impl Sources) -> String;
```

Total and deterministic, with zero options — one canonical output, which
is what a reviewable golden corpus requires; color and width knobs are
named view evolution (§11), not v1 surface. The layout commitments, at
design level: a headline `error[syntax::unexpected-token]: message`; one
snippet block per source touched, its window covering the labeled lines;
a gutter with one-based line numbers (the sole one-based surface in the
crate, stated as presentation); caret underlining for the primary label,
secondary underlines with their messages, labels in position order; then
notes, then helps. Exact layout mechanics are implementation, held
reviewable by the golden corpus (§10) — the same instrument spec §6.6
names for message quality.

**Degradation, fully specified — the view stays total.** §3.3's
honest-and-maximal rule governs every input the renderer cannot honor,
not only the obvious one: an unresolvable `SourceId` renders an inline
placeholder naming the identity; a catalog breaching the completeness
law (§3.4) renders the placeholder naming the missing `SourceFacet`; and
a label that does not fit its *resolved* text — out of bounds, or ending
mid-character — renders its snippet block with an inline placeholder
naming the defect (the label's span against the text's extent), while
every coherent label still renders. Nothing panics, nothing is silently
dropped, and none of these renderings is an accident: each has a golden
case (§10), so the behavior is reviewed, not inherited from whoever
writes the loop.

**Computational cost.** O(rendered size): proportional to the windowed
lines plus labels; flat iteration, no recursion.

### 7.2 The editor view

```rust
/// The editor-protocol payload as a typed value. Serialization is the
/// consumer's step: this crate ships shapes, not bytes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorDiagnostic {
    pub source: SourceId,
    pub range: EditorRange,
    /// The encoding every range in this payload was derived under —
    /// the view records the derivation it ran, so the payload is
    /// self-describing when stored, compared, or forwarded.
    pub encoding: ColumnEncoding,
    pub severity: Severity,
    /// Typed identity; the "namespace::name" string is the consumer's
    /// serialization step, via Display (§8.5).
    pub code: DiagnosticId,
    pub message: String,       // headline, then notes/helps folded as
                               // "note: ..." / "help: ..." lines — the
                               // protocol convention
    pub related: Vec<EditorRelated>,   // a view linearizes (§8.4);
                                       // order: position
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EditorRange { pub start: LineCol, pub end: LineCol }

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorRelated {
    pub source: SourceId,
    pub range: EditorRange,
    pub message: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorRefusal {
    /// The catalog resolves nothing for this id.
    UnknownSource { id: SourceId },
    /// The catalog breached the completeness law (§3.4): it resolved
    /// this id partially, and this is the facet the view lacked.
    IncompleteSource { id: SourceId, missing: SourceFacet },
    /// A position query failed; `at` is the label whose location was
    /// being converted, so the caller need not replay the iteration.
    Position { at: Location, refusal: PositionRefusal },
}

pub fn editor(
    diagnostic: &Diagnostic,
    sources: &impl Sources,
    encoding: ColumnEncoding,
) -> Result<EditorDiagnostic, EditorRefusal>;
```

This view refuses rather than fabricate: a protocol payload with invented
ranges is worse than no payload, and each refusal names its locus — the
question the caller can fix. A message-less secondary label contributes
the diagnostic's headline as its related message — the location still
ships; nothing is dropped silently. The severity stays this crate's typed
`Severity`; the JSON layer's mapping (`Note` to the protocol's
information class) is the consumer's documented step.

**A typed intermediate shape, deliberately — the argument.** Every other
result in this crate is the model or a rendering of it; `EditorDiagnostic`
is a *structure* a view returns, and that wants its reason stated rather
than assumed. The reason: the honest alternative — a view straight to
protocol bytes — is foreclosed by the zero-dependency constraint (spec
§12.5), since bytes here means JSON and JSON means a serializer; and the
dishonest alternative — consumers walking `Diagnostic` themselves per
protocol — re-derives position conversion and label linearization in
every host. So the view stops at the last typed point before bytes. What
keeps this shape from becoming a second model: it is produced by exactly
one pure function, held by nothing in this crate, mirrors the protocol's
categories rather than the model's, and can never disagree with the
model because nothing maintains it — every instance is a fresh
derivation. **Computational cost.** O(labels), each position query
O(log) via the resolved line index.

### 7.3 The machine view

The model itself. `Diagnostic` is the machine-consumable form — typed
identity, severity, spans — with the tier-typed values above it (§6.5)
for in-process consumers that act on tier-specific structure.

### 7.4 Canonical order

```rust
/// One deterministic order for batches: source, then primary span, then
/// severity (worst first), then identity, then full structural
/// comparison as the final tiebreak.
pub fn canonical_order(a: &Diagnostic, b: &Diagnostic) -> Ordering;
```

Defined once because every batch consumer needs one — the golden corpus
first among them — and each inventing its own would diverge. A free
function deliberately, not an `Ord` impl on `Diagnostic`: an `Ord` impl
would claim *the* natural order of diagnostics, and there is none — this
is one batch derivation among possible ones, and it lives with the views
because ordering a batch for consumption is itself a linearization
(§8.4).

## 8. Posture

Four rules stated as one discipline, and one stated posture (§8.5). The
postcondition and failure conditions (§2) carry the first two rules; all
four bind every API decision here — a departure is a defect owing a
stated reason.

### 8.1 Observational purity

Every public operation is a pure function of its explicit inputs: same
inputs, same result; no ambient state, no effectful channels, no
observable caches. `LineIndex` is explicitly constructed and held, never
a lazy field computed behind `&self`. Where mechanism wants mutation —
work-list interiors, buffer building inside `human` — it is local and
invisible at the surface: mechanism below, policy above (spec §12.3),
applied at function scale. This is spec §7.8's binding purity
precondition adopted as this crate's stated style.

### 8.2 Plain data, everywhere

Every public type is `Send + Sync` owned data with no interior
mutability. Results are declarations, not effects: diagnostics are values
returned from derivations; a collection of them is a value the caller
owns. There is no emitter, sink, or diagnostic-context object threaded
through computation.

### 8.3 Declarative construction

Types with invariants guard them at refusing constructors (`Span`,
`Source`, `Diagnostic`); types without invariants have public fields
(`Label`, `Location`, `LineCol`, the editor-view shapes) — a struct
literal is the most declarative constructor there is. Extension is
by-value chaining, so even building reads as declaring.

### 8.4 The modeled shape dictates the type

When the modeled object is mathematically a set, the type is a set
(`Diagnostic::secondary`); a sequence appears only where order is meaning
(`notes`, `helps` — narrative); a map where the object is a keyed lookup
(`SourceSet`). Models hold the mathematical shape; *views linearize*,
and the linearization order is the view's stated derivation
(`EditorDiagnostic::related`, `canonical_order`). This is spec §7.1's
"set-like where the logic says set" adopted below the Program value, as
standing rule.

### 8.5 The std-trait posture

Stated because the fluent-Rust audience's priors sit exactly here (spec
§1.3): every refusal type — `TooLarge`, `InvalidUtf8`,
`FromBytesRefusal`, `SliceRefusal`, `EndBeforeStart`, the line-index
condition structs and their two enums, `EmptyMessage`, `EditorRefusal` —
implements `Display` (the refusal's question-the-caller-can-fix, in
words) and `std::error::Error` (std only; hosts `?`-compose these
through their own error stacks). `DiagnosticId` and `Severity` implement
`Display`, and the documented renderings — `namespace::name`, lowercase
severity names — *are* those impls: the strings are contract, carried by
the type, not an incident private to the views. No other public type
claims a `Display`; refusal `Display` texts are presentation and may
improve, while the two identity renderings above are stable.

## 9. Failure semantics and computational costs, consolidated

Spec §2 item 8's obligation, discharged at design
level. Nothing in this crate panics on any input; the table names every
refusing door, and every operation not listed is total. Under the
per-operation refusal discipline (§3.2), each row's refusal column is
exactly the operation's error type — the table is derivable from the
signatures, which is the point.

| operation | refuses with | cost |
|---|---|---|
| `Source::new` | `TooLarge` | O(1) |
| `Source::from_bytes` | `FromBytesRefusal` (`TooLarge` \| `InvalidUtf8`) | O(n) |
| `Source::slice` | `SliceRefusal` (`OutOfBounds` \| `NotCharBoundary`) | O(1) |
| `Span::new` | `EndBeforeStart` | O(1) |
| `ByteOffset::checked_add` / `checked_sub` | `None` on overflow | O(1) |
| `LineIndex::position` | `PositionRefusal` (`OutOfBounds` \| `NotCharBoundary`) | O(log) |
| `LineIndex::offset` | `OffsetRefusal` (`LineOutOfBounds` \| `ColumnOutOfBounds` \| `ColumnNotBoundary`) | O(log) |
| `LineIndex::line_span` | `LineOutOfBounds` | O(1) |
| `Diagnostic::new` | `EmptyMessage` | O(1) |
| `view::editor` | `EditorRefusal` (`UnknownSource` \| `IncompleteSource` \| `Position`, locus carried) | O(labels · log) |
| `SourceSet::add` | `TooLarge` | O(n) |
| `Sources::{name, text, line_index}` | `None` on unknown id | O(1)* |

\* for the shipped `SourceSet`; other implementors state their own.

Total (never refuse, never panic): every accessor; all `Span` operations
except `new` (`join` included — the covering span is total even on
disjoint operands); `LineIndex::of` and `line_count`; `Diagnostic` chaining;
`view::human` (every degradation renders a named placeholder — §7.1);
`canonical_order`; `check_sources_laws` (an empty report is the laws
holding); `SourceSet::new`; the `ToDiagnostic` impls.

Empty attachment strings are admitted unaltered — accepting a value
as-is is not repair, and attachment *quality* is the emitting tier's
obligation, held by its golden corpus. The one structural emptiness this
crate refuses is the headline, which every view depends on.

## 10. Assurance instruments for stage 1

Per spec §11, the stage is not done until these are
green; per spec §10.1, proptest and criterion are standing from the
tier's landing.

- **Property laws (proptest), on multi-byte-heavy generated text:**
  `offset → position → offset` identity in all three encodings;
  `LineIndex` agreement with a naive character-walk oracle; span algebra
  (join idempotent, commutative, associative; intersect consistent with
  `contains_span`); `Source::from_bytes` on arbitrary bytes never panics
  and refuses exactly when the standard library's validator does;
  `view::human` total on arbitrary well-formed diagnostics over
  arbitrary catalogs — unresolvable ids, incomplete resolutions, and
  span/text mismatches included; `canonical_order` a total order
  (antisymmetric, transitive, total).
- **The `Sources` law checker, checked:** `check_sources_laws` (§3.4)
  passes `SourceSet` by construction and fails deliberately incoherent
  and incomplete catalogs — the instrument for the one seam every host
  crosses, shipped to implementors and exercised here against both
  outcomes.
- **Golden snapshots:** the human renderer's seed corpus — single span;
  multiple spans on one line; a multi-line span; a cross-source
  secondary; notes and helps; the unresolvable-source, missing-facet,
  and span/text-mismatch placeholders; the embedded-snippet frame —
  reviewed against the rust-analyzer bar. This corpus is the foundation
  the *diagnostics-quality* witness builds on at stage 2.
- **Scaling shapes (criterion):** `LineIndex::of` linear in text;
  `position`/`offset` logarithmic; `human` linear in rendered output.
  Shape assertions in CI; absolute numbers out-of-band (spec §10.2).
- **Standing gates:** mutation per milestone; the workspace coverage
  floor; unused-code and unused-result warnings denied;
  `forbid(unsafe_code)` plus the structural trust checks (FFI-free
  closure, no build script); documentation examples that run; the
  executable-claims standard for anything this crate says about itself
  (spec §10.4).
- **Depth, honestly.** This crate holds no recursive structure — flat
  data throughout — so spec §5.2's per-walk depth-gate obligation attaches
  vacuously and is discharged by inspection plus the totality
  properties. The depth-gate machinery first bites at the program tier.

## 11. Reserved seams and non-goals

Named reserved seams — deferred with reasons and their arriving
consumers, not gaps:

- **Structured-payload passthrough** on the normal form (a typed data
  slot surviving lowering): its consumer is an out-of-process actor on
  tier payloads, which v1 does not have — in-process consumers use the
  tier's typed diagnostics (§6.5).
- **Fix and suggestion machinery:** arrives with the language-server
  consumer class, not v1.
- **`Severity::Hint`:** same checkpoint, recorded pressure (§6.2).
- **Source-origin metadata** (document / snippet / …): attachment point
  is a field on `Source` and a `Sources` accessor; lands with the first
  consumer that acts on the distinction — today none does, and a
  distinction nothing consumes is cut (§3.3 works through host naming).
- **Human-view render options** (color, width, context lines): view
  evolution once a consumer needs them; v1's single canonical output is
  what the golden corpus reviews.
- **Interning:** no client below the syntax tier; decided there, not
  here.
- **Incremental text edits** (ropes, edit application): spec §7.8
  defers incremental machinery; a changed text is a new
  `Source`, and cheap total reparse is the supported path.
- **Expansion-chain provenance** (a source embedded in a source, as
  model data): the host's re-basing arithmetic covers the embedded case
  (§3.3); compiler-scale expansion chains have no v1 consumer.

Non-goals, absolutely: I/O of any kind — no paths, no loading, no
watching; ASP knowledge of any kind; unlocated reports (not diagnostics —
§6.4); serialization (shapes, not bytes — §7.2); anything virtual-file-
system-shaped beyond `SourceSet`'s catalog.
