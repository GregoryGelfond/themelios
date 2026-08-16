# themelios-syntax — tier design

2026-08-15. Design for review, pre-implementation. This document is the
API design of `themelios-syntax` — the types, traits, signatures,
semantics, and computational costs of the syntax tier — derived from the
v1 specification (`docs/specification.md`, cited as *spec §n*), held to
the grammar of record (`docs/grammar.md`, cited as *grammar §n*), and
built on the base tier's design (`docs/design/base.md`, cited as
*base §n*); a bare *§n* cites this document's own sections. It is written
to stand alone in the same sense those documents are: a reader holding
this repository and public sources can check every claim. Where this
document and the specification or the grammar disagree, they govern and
the disagreement is a defect here.

---

## 1. What themelios-syntax is

`themelios-syntax` is the syntax tier (spec §6, spec §11 stage 2, spec
§12.2): a total lexer with the fusion oracle beside it; a hand-written,
error-resilient parser producing a lossless tree of the one grammar under
a declared dialect; the trivia and comment-attachment policy, owned here
and exposed as API; a typed AST over the tree; the tier's own typed
diagnostics, lowering to the base model; and token-stream equivalence,
the certificate a layout-only or spelling-preserving transformation
claims. It is importable wholesale (spec §2 item 2): a consumer needs
nothing outside its public surface to parse, walk, diagnose, attach,
re-space, or certify.

**Whose surprise this surface is calibrated for.** The consumers spec
§1.1 names, each with the demand it puts on this tier: the **formatter**
(morphe, a formatter of the rustfmt and black class — it re-spaces, may
normalize the grammar's synonym spellings, and proves to itself before
writing that it changed nothing else); the **language server** (error
resilience, structured diagnostics, positional identity, cheap total
reparse); a **solver frontend** for which this tier is the parser of
record and whose face these diagnostics are (spec §2 item 9); a **REPL**
(statement-at-a-time reading, the term-value sublanguage for symbols,
and a typed answer to "is this input finished or wrong?"); the **macro
tier**, a compile-time client that hands its token stream to this parser
and no other (spec §8, law 1); **contract extraction** and every
comments-as-data reader; and the **program tier**, which lowers the typed
AST with byte-precise provenance. The one-grammar rule (spec §2 item 3)
is discharged here: this crate carries the only parser of the language,
and its fragment entry points and token-source door exist so that no
consumer needs a second.

**Naming ground, stated once per spec §1.4.** Two vocabularies meet here
and both are the field's. Tooling objects — token, lexer, parser, syntax
tree, node, trivia, AST, cursor, diagnostic — take the language-tooling
literature's names, for base §1's reason: this tier's subject matter is
tooling infrastructure and its nearest audience is the tool builder (spec
§1.3). Language constructs — rule, head, body, literal, atom, term,
aggregate, conditional literal, theory atom, and the directives — take
the names the grammar of record gives them, which are the ASP
literature's; the tree's kind roster (Appendix A) is that grammar's
production roster in the tooling idiom's spelling. Names below that
depart either usage carry their reasons in place.

**Assumed fluency.** Fluent Rust; ASP as the literature and the grammar
of record state it. Not assumed: rust-analyzer's internals or rowan's
API — §5 teaches exactly the green/red delta a consumer needs and no
more.

**Crate facts, carried as constraints.** `#![forbid(unsafe_code)]`. The
shipped library's dependency closure is `themelios-base` and **rowan**
with rowan's own closure — the one named exception of spec §12.5, pinned,
with its audit note in §14 and the hand-rolled green/red tree recorded as
the reserved fallback (§17). No lexer or parser dependency; no
hash-map utility of this crate's own (rowan's suffice internally). The
closure is FFI-free and holds no build script of this workspace's own,
asserted structurally over Cargo's resolved graph (spec §12.3, §14). The
workspace `rust-version` floor. No I/O, no global state, no runtime
(spec §1.2): a parse is a pure function of its inputs (spec §6.8, §12).
Every public *value* type is plain `Send + Sync` data with one stated
exception — rowan's red-tree cursors are views, not data (§5.1).

**Module map.** Ten modules, one concern each: `dialect` (§3), `tree`
(§4–§5: the kind roster, the language, the tree aliases, coordinate
conversions), `token` (§4: `Token`, `LexMode`, `TokenSource`), `lexer`
(§4), `parse` (§5.5–§6: `Parse`, the entry points, `EntryPoint`), `diagnostic`
(§7), `ast` (§8), `attach` (§9), `fusion` (§10), `equiv` (§11).

## 2. What this design is for

The postcondition, stated so a review can check drift against it:

> themelios-syntax turns any admitted source text into a lossless,
> error-resilient tree of the one grammar under a declared dialect — the
> tree's text is the input, unconditionally — with typed diagnostics at
> the rust-analyzer bar, an owned and exposed attachment policy, an exact
> fusion oracle, and two token-stream certificates, all on one public
> surface importable wholesale, such that a formatter, a language server,
> a solver frontend, a REPL, the macro tier, and a comments-as-data
> reader need nothing outside it and no second parser exists anywhere;
> every public operation is total and observationally pure; and no walk
> this crate performs, and no structure it hands out, has depth
> proportional to the input's nesting.

This design has failed — independent of any local defect — when any of
the following holds:

- A parse panics, diverges, or yields a tree whose text differs from its
  input, on any admitted text (spec §2 item 8, spec §6.3, spec §6.5).
- The parser admits or refuses an input the grammar of record does not,
  under either dialect, beyond the grammar's recorded divergences (spec
  §4; grammar §2).
- A consumer spec §1.1 names needs a private API, a fork, or a second
  grammar — the composition test (spec §4) — including the macro tier's
  law that a macro-site syntax error is the file parser's own diagnostic
  (spec §8, law 1).
- A diagnostic lacks a precise span or a stable identity, or its expected
  set is prose (spec §6.6, spec §4).
- Two consumers can derive different attachments for one comment
  because the tier's answer was not usable, or the tier's answer changes
  under a transformation that preserved everything the policy reads
  (spec §6.4, §9).
- A certificate is granted to a transformation that changed a
  significant token or a comment (spec §6.7, §11), or the fusion oracle
  certifies an adjacency the lexer would fuse (spec §6.2, §10).
- Any walk over user-reachable structure recurses in the input's
  nesting, or a tree handed to a consumer can be dropped or traversed
  only with stack proportional to that nesting (spec §5.2, grammar §10,
  §6.6).
- A dependency beyond rowan's stated closure appears in the shipped
  library, unsafe code appears, or the closure stops being FFI-free
  (spec §12.3, spec §12.5, §14).
- A parse depends on anything but its token source, its dialect, and its
  entry point, or a public operation mutates anything except through an
  explicit `&mut` in its signature (spec §6.8, base §8.1).

The third and the fifth are serviceability conditions: they bind this
tier to its named consumers' stated needs; the second is the
differential's question; and the instruments in §16 hold each where an
instrument can.

## 3. The dialect

```rust
/// The declared parameterization of the one grammar (grammar §1, §6):
/// which reading of the two lexical regions and whether the query
/// statement exists. Declared per input, never varied per consumer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dialect { Clingo, AspCore2 }

impl Default for Dialect { /* Clingo — the grammar's own default (grammar §1) */ }
```

Exactly what the dialect changes, and nothing else, held by a test: the
string rule (grammar §4.4 against §6.2), the block-comment rule (grammar
§4.1 against §6.3), and the query statement (grammar §6.1). The dialect
reaches the lexer through the token source (§4.3) and the parser through
the same source, so the two cannot disagree. **Closed, argued as base
§6.2 argued `Severity`:** a released clingo 6.x language is a third
surface until the grammar's upgrade protocol says otherwise (grammar
§12), and admitting it then is a breaking addition through every
exhaustive match — priced correctly by the pre-1.0 posture (spec §13).
The dialect-neutrality law: on the shared subset — inputs containing no
`?` as the final significant token, no backslash inside a string, no
string spanning a line, and no `%*` or `%` inside a block comment — the
two dialects yield structurally equal trees and equal diagnostics
(§16).

## 4. Tokens and the lexer

### 4.1 The kind roster

One `#[repr(u16)]` enum names every token kind and every node kind — the
rowan idiom, where a tree's whole vocabulary is one closed set the
`Language` implementation (§5) maps to rowan's raw kind. It lives in the
`tree` module, being the tree's vocabulary; the lexer produces it:

```rust
/// Every token and node kind of the tree. Tokens first, then nodes;
/// within each, the grammar of record's order (Appendix A is the
/// complete roster with the production each kind realizes).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u16)]
pub enum SyntaxKind { /* Appendix A */ }

impl SyntaxKind {
    /// Trivia: whitespace and the comment forms — including a doc
    /// comment, whose trivia-or-structure status is positional (§5.4)
    /// and therefore not a property of the kind alone.
    pub const fn is_trivia(self) -> bool;
    /// The comment forms: LINE_COMMENT, BLOCK_COMMENT, SHEBANG_COMMENT,
    /// DOC_COMMENT.
    pub const fn is_comment(self) -> bool;
    pub const fn is_keyword(self) -> bool;
    pub const fn is_token(self) -> bool;
    pub const fn is_node(self) -> bool;
}
```

Every kind carries `Debug` as its SCREAMING_SNAKE name, which is the
spelling the tree dumps and goldens use (§16). Synonym spellings share a
kind and keep their text: `=` and `==` are both `EQ`, `!=` and `<>` both
`NEQ`, `#inf`/`#infimum` both `KW_INF`, `#sup`/`#supremum` both
`KW_SUP`, `#minimize`/`#minimise` both `KW_MINIMIZE`, and likewise
`KW_MAXIMIZE` — the grammar's own statement (grammar §4.5, §4.6), and the
ground of the spelling certificate (§11.3). Two kinds exist for exactly
one token source each: `SPLICE`, which only a macro token source produces
(grammar §9; the file lexer never yields it, which is what keeps the
macro dialect out of file syntax by construction), and `KW_END`, which
only the script region produces (grammar §4.5, §4.8). `EOF` is a token
the source returns and the tree never holds.

### 4.2 The token and the modes

```rust
/// One token as a source hands it to the parser: its kind and its text.
/// The text is a slice of the source's own text (§4.3); its length is
/// the token's extent, so no length field can disagree with it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Token<'a> { pub kind: SyntaxKind, pub text: &'a str }

/// The lexical mode in force at a token's start — a language fact
/// (grammar §4.7, §4.8), not an implementation choice: inside a theory
/// atom's elements and guard, and at the operator positions of a
/// `#theory` definition, operator runs are one token and `not` is an
/// operator; between `#script(…)` and `#end`, nothing lexes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LexMode { Normal, Theory, ScriptBody }
```

**The parser owns the modes.** Which mode is in force at a position is
decided by structure — from the `{` that opens a theory atom's elements
through its matching `}` and on through the guard, back to normal for
each element's condition, and so on, exactly as grammar §4.7 states the
region — and structure is the parser's knowledge. So the parser tells the
token source the mode for each token it wants (§4.3), and the mode logic
lives in one place. The source's obligation is only to form tokens
correctly under a stated mode. This is the arrangement that lets a macro
token source (§4.3) form theory-operator runs from adjacent Rust
punctuation the same way the file lexer forms them from adjacent bytes:
the mode arrives from the one parser; each source applies its own
formation rules under it (grammar §9: "theory-operator runs form the
same way inside theory expressions").

**Bounded re-lexing at region ends, stated once.** Grammar §4.7's guard
rule — the guard extends through the longest token sequence derivable as
a theory-opterm, and "the first token that cannot extend it lexes in
normal mode" — means the parser must sometimes look at a token under
theory mode, decide it does not extend the guard, and take it again
under normal mode. The token-source door is a pure function of
`(offset, mode)` (§4.3), so taking a token again is calling that
function again at the same offset; no state is unwound. Every token is
requested at most a fixed number of times (twice, at a region boundary),
which keeps parsing linear (§6.8).

### 4.3 The token-source door

```rust
/// Where tokens come from. The file lexer is one source; the macro tier
/// is another (grammar §9: Rust's lexer is the macro dialect's lexical
/// layer, and the mapping onto §4's roster is the macro crate's). One
/// parser reads both, which is spec §8's first law discharged by
/// construction.
pub trait TokenSource {
    /// The identity the host minted for this text (base §3.1).
    fn id(&self) -> SourceId;
    /// The dialect the source lexes under; the parser reads it here so
    /// lexer and parser cannot disagree (§3).
    fn dialect(&self) -> Dialect;
    /// The whole text. Every token is a slice of it (the slice law).
    fn text(&self) -> &str;
    /// The token that begins at `at` under `mode`: the longest token the
    /// mode's rules form there, an ERROR token (§4.5), or `EOF` with
    /// empty text at the text's end. Refuses exactly when `at` is not a
    /// position of the text — past its end, or inside a character — with
    /// base's own condition for exactly that (base §5): the door speaks
    /// base's coordinates, and refuses where span meets text as base
    /// refuses. Never panics.
    fn token_at(&self, at: ByteOffset, mode: LexMode)
        -> Result<Token<'_>, PositionRefusal>;
}
```

**The laws, stated as contract.** A token source is bound by four:

1. **Tiling.** Starting at offset zero under `Normal` mode and advancing
   by each token's length, the tokens partition the text exactly and end
   at `EOF` at the text's length. (Under other modes tiling holds from
   any offset the parser reaches — the parser never asks elsewhere.)
2. **Slice.** `token_at(at, mode)?.text` is `&text()[at .. at + len]`:
   the token's text is the source's text, never a synthesis. This is
   what makes the tree's text the input (§5.4): the tree is built from
   the tokens' texts and nothing else.
3. **Determinism.** Same offset and mode, same answer — a source is a
   pure function of its text.
4. **Refusal.** `token_at` refuses exactly at offsets that are not
   positions of the text, and answers everywhere else.

What the parser can check, it checks: a refusal at an offset the parser
reached by tiling, an `EOF` before the text's end, or a token running
past it is a tiling breach the parser sees and treats as end of input,
with a diagnostic naming the breach (Appendix B). The slice law is not
cheaply checkable at every token — verifying it means comparing every
token against the text — so the parser trusts it, and that trust is this
contract's stated boundary, exactly as base §3.4 trusts coherence: what
holds it is test-time machinery,

```rust
/// The laws, checkable: tiles the source under `Normal` mode from zero,
/// checking tiling and the slice law at every token, probing
/// determinism by re-asking each token, and probing refusal once past
/// the end and once inside a multi-byte character of each token that
/// has one. Total; O(text) for a lawful source. Implementors run it in
/// their own tests; the file lexer passes by construction, and §16
/// exercises the checker against deliberately breaching sources.
pub fn check_token_source_laws(source: &impl TokenSource)
    -> Vec<TokenSourceLawViolation>;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TokenSourceLawViolation {
    Tiling { at: ByteOffset, token: SyntaxKind, len: u32 },
    Slice { at: ByteOffset },
    Determinism { at: ByteOffset, mode: LexMode },
    Refusal { at: ByteOffset, refused: bool },
}
```

An empty report is the laws holding — the shape of base's
`check_sources_laws`, for the same reason: this is the seam every foreign
source crosses.

### 4.4 The file lexer

```rust
/// The lexer over an admitted source: total on every (offset, mode),
/// hand-written to the grammar of record's lexical section, with no
/// state but the source and the dialect. Cheap to construct — it holds
/// one reference and the dialect — and a pure function thereafter.
#[derive(Clone, Copy, Debug)]
pub struct Lexer<'a> { /* source: &'a Source, dialect: Dialect */ }

impl<'a> Lexer<'a> {
    pub fn new(source: &'a Source, dialect: Dialect) -> Lexer<'a>;
}

impl TokenSource for Lexer<'_> { /* the four laws, by construction */ }
```

The lexer rides on base's `Source` (base §3.2) and has no `&str` door,
for base's reason: admission is the one authority for text, so every
byte the lexer sees is valid UTF-8 and every text is under the
admission ceiling — which is exactly the reconciliation base §3.2 states
between spec §6.2's totality over byte sequences and the tree's text
being a string. What reaches the lexer is total *and* well-formed
Unicode; totality here means every character sequence lexes.

**Maximal munch, and one tie the grammar names.** The lexer implements
grammar §4's matching discipline verbatim: longest match wins; among
equal lengths a quoted spelling beats a class rule. The doc comment and
the line comment cannot tie: `LINE-COMMENT` excludes the `%!` prefix
by its own rule (grammar §4.1).

**Whitespace is one token per run**; comments are one token each; a
block comment is one token from `%*` to its matching `*%` under the
dialect's nesting rule, its depth a counter inside the lexer's loop and
never a stack (grammar §10). Under `ScriptBody` mode the lexer answers
`KW_END` when the text at the offset begins with `#end`, and otherwise
one `SCRIPT_BODY` token — the raw text up to the first `#end` (grammar
§4.8), or an `ERROR` to end of input when there is none — so a script
region that is empty has no body token, only its terminator, and there
is never a zero-length token; the parser returns the mode to `Normal`
for the dot.

### 4.5 Error tokens: their extent, stated once

Spec §6.2: unknown input becomes error tokens, nothing dropped. The
grammar says *what* is a lexical error; this design says *how much* one
`ERROR` token spans, so every implementation and every consumer agrees:

- **A malformed string** — a raw line break, a backslash before a
  character the dialect's rule does not admit, or end of input before
  the closing quote — is one `ERROR` token from the opening quote
  through the character at which matching failed (the offending
  character after a backslash is included; a line break is not, and
  lexes as whitespace after it). Under the ASP-Core-2 dialect only the
  end-of-input case exists (grammar §6.2).
- **An unknown `#`-word** is one `ERROR` token spanning the whole word
  by maximal munch (grammar §4.5).
- **An unterminated block comment or script region** is one `ERROR`
  token to the end of input.
- **A `_` inside a theory expression** is one `ERROR` token of one
  character (grammar §4.7).
- **Any other character that begins no token** — `!` and `$` outside
  their regions, control characters, non-ASCII text outside strings and
  comments — joins the maximal run of such characters into one `ERROR`
  token per run.

Each `ERROR` token yields exactly one lexical diagnostic (§7), so a
hostile input of a megabyte of `$` costs one token and one diagnostic,
not a million. Error tokens are significant tokens for the tree, the
token stream, and the certificates (§11): a formatter carries them
verbatim.

### 4.6 Computational cost

`Lexer::new` is O(1). `token_at` is O(length of the token it returns);
tiling a text is O(text). The lexer allocates nothing: a `Token` borrows
the source. Memory is O(1) beyond the source it borrows.

## 5. The tree

### 5.1 Green and red, in one paragraph

rowan (§14) keeps a syntax tree as two structures. The **green tree** is
the data: an immutable, position-free tree of nodes and tokens, each node
holding its kind and its children, each token its kind and its text; it
is shared by reference count, `Send + Sync`, and structurally comparable.
A **red tree** is a view over it: a cursor (`SyntaxNode`, `SyntaxToken`)
that carries a green pointer plus an absolute offset and a parent link,
minted lazily as one navigates, thread-local and cheap to clone. Two
cursors are equal when they name the same green element at the same
position in the same tree — *positional identity*, the identity spec
§6.8 says node identity is now. Nothing here is mutable: rowan 0.17.0
carries no tree-editing API (§14), and this tier exposes none — text is
the edit medium and total reparse the supported path (spec §6.8, §7.8);
tree editing is a named seam (§17). This is the whole of the rowan idiom
a consumer needs; the rest is naming.

**The stated exception to plain data.** Every public *value* in this
crate is `Send + Sync` owned data (base §8.2 adopted) — `Parse`, `Token`,
the diagnostics, the certificates' witnesses — because the model is the
green tree. The red cursors and the typed AST wrappers over them are
*views*, `!Send`, borrowed from the model by construction; a consumer
that crosses threads sends the `Parse` and mints cursors on the other
side. That is one exception, stated once, and it is the reason the tree
is data first and cursors second.

### 5.2 The language and the aliases

```rust
/// The rowan language marker: maps `SyntaxKind` to and from rowan's raw
/// kind. Uninhabited — a type-level tag, never a value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Asp {}
impl rowan::Language for Asp { type Kind = SyntaxKind; /* … */ }

pub type SyntaxNode     = rowan::SyntaxNode<Asp>;
pub type SyntaxToken    = rowan::SyntaxToken<Asp>;
pub type SyntaxElement  = rowan::SyntaxElement<Asp>;   // NodeOrToken
pub type SyntaxNodeChildren    = rowan::SyntaxNodeChildren<Asp>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<Asp>;
pub type Preorder           = rowan::api::Preorder<Asp>;
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<Asp>;
pub type SyntaxNodePtr      = rowan::ast::SyntaxNodePtr<Asp>;

pub use rowan::{GreenNode, NodeOrToken, TextRange, TextSize, WalkEvent,
                Direction, TokenAtOffset, SyntaxText};
pub use rowan::ast::{AstNode, AstChildren, AstPtr};
```

**The public rowan surface, listed so the fallback's obligation is
bounded (spec §12.5).** These aliases and re-exports, and on them the
operations this design's signatures and examples use: on nodes —
`kind`, `text_range`, `text`, `parent`, `ancestors`, `children`,
`children_with_tokens`, `first_child`, `last_child`,
`first_child_or_token`, `last_child_or_token`, `next_sibling`,
`prev_sibling`, `next_sibling_or_token`, `prev_sibling_or_token`,
`first_token`, `last_token`, `siblings`, `siblings_with_tokens`,
`descendants`, `descendants_with_tokens`, `preorder`,
`preorder_with_tokens`, `covering_element`, `token_at_offset`; on
tokens — `kind`, `text_range`, `text`, `parent`, `ancestors`,
`next_sibling_or_token`, `prev_sibling_or_token`, `next_token`,
`prev_token`; `Display` (the text) and `Debug` (`KIND@start..end`; the
alternate form dumps the tree) on both; positional `Eq`/`Hash`; the
`AstNode` trait (`can_cast`, `cast`, `syntax`), `AstChildren`,
`SyntaxNodePtr`/`AstPtr` (positional identity by kind and range,
resolvable against a root); `GreenNode` as the `Send + Sync` handle. The
hand-rolled fallback (§17) owes exactly this list. `token_at_offset` is
on the list because the alias exposes it; this tier does not call it,
and §5.4's depth bound is what makes its recursion (§14) safe for a
consumer who does.

### 5.3 Two coordinate vocabularies, one seam

The tree speaks rowan's `TextSize`/`TextRange`; the doors, the
diagnostics, and provenance speak base's `ByteOffset`/`Span`/`Location`
(base §4). Both are `u32` byte offsets under the same admission ceiling
(base §3.2 — rowan's `TextSize` is a `u32`, and every admitted text fits
it), so the conversion is total both ways and lives in one place:

```rust
pub fn span_of(range: TextRange) -> Span;      // total: start <= end holds
pub fn range_of(span: Span) -> TextRange;      // total
pub fn offset_of(size: TextSize) -> ByteOffset; // total
pub fn size_of(offset: ByteOffset) -> TextSize; // total
```

The doors take base's types (§4.3's `token_at`, §7's spans, §5.5's
`location`) because base's are the vocabulary of location every tier
and every host already speaks; the tree keeps rowan's because
wrapping every cursor operation to rename a `u32` would be exactly the
surface §14 declines to duplicate. Two spellings of one number, each
where its audience expects it, converted at the seam and nowhere else.

### 5.4 The tree's laws

Four laws, each held by an instrument (§16):

1. **Text.** For every parse, the root's text equals the source's text —
   `parse.syntax().text() == source.text()`, unconditionally: on valid
   programs, on garbage, on truncated input, under either dialect (spec
   §6.3). It holds because the tree is built from the tokens' texts and
   the tokens tile the text (§4.3); nothing is inserted, dropped, or
   normalized. Author whitespace is a `WHITESPACE` token in the tree; a
   formatter chooses to replace it, and the tree carries no opinion.
   This law is the tier's token-fidelity emit (spec §12.2): there is no
   emitter — `text()` renders the tree byte for byte, and `Display` on
   a node is that text.
2. **Trivia placement.** The parser opens a node at its first
   significant token and closes it at its last, so every trivia token is
   a child of the node that was open where the trivia stood: trivia
   before a node's first token belongs to the parent, trivia between a
   node's children to the node, trivia after its last token to the
   parent. Consequently **every node but a root begins and ends with a
   significant token**, and trivia between statements belongs to
   `PROGRAM`. Docs are the one shaping rule beyond it: a statement's
   documentation (grammar §5.11) is a leading run of `DOC_COMMENT` tokens
   *inside* the statement's node, significant, with the whitespace
   between them; a doc comment anywhere else is trivia under this law
   and diagnosed (§6.3). There is no `DOCS` node — the tokens' kind and
   position say what they are, and a wrapper would carry nothing they
   do not (Appendix A records the mapping).
3. **Bounded depth.** No tree this crate produces is deeper than
   `MAX_NESTING_DEPTH` plus the grammar's fixed layer count (§6.6): the
   parser refuses to open nesting past the constant, and the surplus is
   flattened losslessly under `ERROR` with a diagnostic. This is the law
   that makes the tree safe to hold: rowan's own drop and one of its
   queries recurse in tree depth (§14), a fact no work-list discipline
   in this crate can reach, so the depth is bounded at construction.
4. **Determinism.** Same text, same dialect, same entry point — a
   structurally equal green tree and equal diagnostics (spec §6.8).

### 5.5 The parse

```rust
/// The result of a parse: the green tree, the diagnostics, and the
/// facts a consumer needs to interpret both. Owned, `Send + Sync`,
/// cheap to clone (the tree is reference-counted). `T` is the typed
/// root the entry point yields (§6.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Parse<T: AstNode<Language = Asp>> {
    /* private: green: GreenNode, diagnostics: Vec<SyntaxError>,
       source: SourceId, dialect: Dialect, entry: EntryPoint, _root: PhantomData<T> */
}

impl<T: AstNode<Language = Asp>> Parse<T> {
    /// A fresh root cursor over the tree — a view, minted on demand.
    pub fn syntax(&self) -> SyntaxNode;
    /// The typed root. Total: every entry point yields a root of the
    /// kind its `T` casts (§6.1), so this never fails.
    pub fn tree(&self) -> T;
    pub fn green(&self) -> &GreenNode;
    /// In the order the parser produced them, which is source order by
    /// primary span; a batch consumer sorts by base's `canonical_order`
    /// after lowering if it wants the shared batch order.
    pub fn diagnostics(&self) -> &[SyntaxError];
    /// Any diagnostic of `Severity::Error`. Membership in the language
    /// (grammar §2) is exactly `!has_errors()` (§6.5).
    pub fn has_errors(&self) -> bool;
    /// The input ended before the construct did, and that is the only
    /// kind of error present — the REPL's "read more" signal (§6.5).
    pub fn is_incomplete(&self) -> bool;
    pub fn source(&self) -> SourceId;
    pub fn dialect(&self) -> Dialect;
    pub fn entry(&self) -> EntryPoint;
    /// The qualified location of an element of this tree (base §4.3):
    /// its range under this parse's source id. Total.
    pub fn location(&self, range: TextRange) -> Location;
}
```

`PartialEq` on `Parse` is structural through the green tree (rowan's
green equality is structural; the diagnostics, dialect, and identity are
plain data), which is what the determinism law (§5.4) is checked with.
Equality is recursive over depth inside rowan and therefore bounded by
law 3.

**Computational cost.** `syntax()` is O(1) (a root cursor); `tree()` is
O(1) (a cast); `has_errors` and `is_incomplete` are O(diagnostics);
`location` O(1); clone O(diagnostics) — the tree is shared. The tree's
memory is O(tokens + nodes), with rowan's per-parse node cache sharing
identical tokens and small identical subtrees — a per-builder cache,
never a global one (spec §1.2).

## 6. The parser

### 6.1 Entry points and roots

```rust
/// What the parser is asked to read: a whole program, or one construct
/// family with a named consumer — the statement (a REPL reads one at a
/// time; the macro tier's statement macros), the term (the macro tier's
/// term positions), and the term-value sublanguage (grammar §5.10: what
/// a string parses to when a caller asks for a symbol — the REPL and
/// the query surface). Closed; a family is admitted here when a
/// consumer names it (spec §8's tiered vocabulary), and the addition is
/// a breaking one, priced by the pre-1.0 posture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntryPoint { Program, Statement, Term, TermValue }

/// The file door: an admitted source under a dialect. Total.
pub fn parse(source: &Source, dialect: Dialect) -> Parse<ast::Program>;

/// The general doors: any token source (§4.3). Total.
pub fn parse_program(source: &impl TokenSource) -> Parse<ast::Program>;
pub fn parse_statement(source: &impl TokenSource) -> Parse<ast::Fragment>;
pub fn parse_term(source: &impl TokenSource) -> Parse<ast::Fragment>;
pub fn parse_term_value(source: &impl TokenSource) -> Parse<ast::Fragment>;
```

`parse` is `parse_program` over `Lexer::new(source, dialect)`; it exists
so the common case names no lexer. **Every entry point yields a root of
one fixed kind**, which is what makes `Parse::tree()` total (§5.5): the
program entry's root is `PROGRAM`; every fragment entry's root is
`FRAGMENT`, a container holding leading trivia, the fragment's node when
the input held one, trailing trivia, and — when input remained after the
fragment — an `ERROR` node with the diagnostic that end of input was
expected. The alternative, a root of the fragment's own kind, would make
the root's kind depend on the input (a term entry over an empty text has
no term to be the root), and `tree()` would have to answer `Option<T>`
for some entries and `T` for others; one container kind keeps one honest
signature. The typed root for fragments:

```rust
impl ast::Fragment {
    pub fn statement(&self) -> Option<ast::Statement>;   // Statement entry
    pub fn term(&self) -> Option<ast::Term>;             // Term, TermValue
}
```

### 6.2 The parser's shape

A hand-written recursive-descent parser to grammar §5 (spec §6.5),
building the green tree through rowan's builder as it goes: one function
per grammar-bounded production, each opening its node at its first
significant token and closing it at its last (§5.4's placement law);
rowan's *checkpoint* — a mark that lets a node be opened retroactively
around already-built children — is what realizes left-recursive and
precedence shapes (`term BINOP term`, a comparison's chain, the docs
wrapped into their statement) without a second pass. The parser reads
its tokens through the source door under the mode it decides (§4.2),
looks ahead by at most five tokens (the show-signature reading, §6.3),
one token everywhere else, and re-lexes at most once per region boundary
(§4.2). Diagnostics are values it accumulates and hands back on the
`Parse` — no sink, no context object (base §8.2).

**The recursion discipline, applied as grammar §10 maps it.** Everything
above the term families composes by iteration and a fixed number of
layers — `program → rule → head → disjunction → literal → comparison →
term` is a constant path — so those functions may recurse on the call
stack, and their depth is bounded by the grammar, not the input. The
four self-recursive families — `term` (and with it the macro dialect's
splice-bearing term), `constant-term`, `value-term`, and
`theory-term`/`theory-opterm` — are parsed by **one loop over an
explicit frame stack**: a frame per open bracket context (a
parenthesized pool or tuple, an argument list, an absolute-value pair,
and for theory terms a set, list, tuple, or function's arguments), each
frame holding its own operator stack for the precedence table of grammar
§5.1 (interval loosest; the bitwise family; additive; multiplicative;
`**` right-associative; unary tighter than `**`) or, for theory terms,
the flat operator-and-term sequence grammar §5.8 admits without
precedence. Input depth becomes frame count, never call depth. The
constant-term and value-term forms are the same loop under a
**restriction context** — variables, the anonymous variable, pools, and
intervals excluded from `#const` bodies (grammar §5.9); those and
`@`-calls and multi-term absolute values excluded from term values
(grammar §5.10) — that builds the general shape and emits
`form-not-allowed-here` (Appendix B) at each excluded form, so a
consumer sees the structure the author wrote and a diagnostic naming
what the position forbids, rather than an `ERROR` blob.

### 6.3 The worded rules, realized

The grammar states five rules in words rather than productions; each is
a fixed, bounded decision in the parser:

- **The show-signature reading** (grammar §5.9): after `#show`, if the
  next significant tokens are `[-] IDENT / NUMBER .` — trivia legal
  between — the statement is the signature form; anything else is the
  term form. Five tokens of lookahead, the parser's maximum.
- **The query reading** (grammar §6.1): under the ASP-Core-2 dialect
  only, a `?` whose next significant token is end of input is the query
  mark. The term loop does not take such a `?` as the bitwise-or
  operator (one-token peek), and the statement parser, holding a bare
  atom in head position with nothing before it, closes a `QUERY`; a
  statement that is not a bare atom meets an ordinary unexpected token
  there. Under the clingo dialect the same `?` is the operator missing
  its right operand — end of input where a term was expected — and the
  diagnostic carries a help naming the other dialect's reading, so the
  practitioner arriving from the standard is told, not puzzled.
- **The theory regions** (grammar §4.7): the mode switches at the `{`
  that opens a theory atom's elements, back to normal at each element's
  condition colon at element depth and forward again at its `;` or the
  closing `}`, on through the guard while a theory operator or theory
  term extends it, and to normal at the first token that cannot; inside
  a `#theory` definition, only the operator positions lex under theory
  mode.
- **The script region** (grammar §4.8): after `#script ( IDENT )` the
  parser asks under `ScriptBody` and receives the body token, or `KW_END`
  directly when the region is empty (§4.4); after the body it asks for
  `KW_END` under the same mode, then returns to `Normal` for the dot.
- **The docs production** (grammar §5.11): at statement position, a run
  of `DOC_COMMENT` tokens is taken at program level behind a checkpoint;
  when a statement start follows, the statement's node is opened at the
  checkpoint and the run becomes its leading children (§5.4); when end
  of input or a token that begins no statement follows, the run stays
  trivia and each line is diagnosed `misplaced-doc-comment` at warning
  severity — the input is still a member (grammar §4.1). A doc comment
  met inside a statement is trivia there with the same warning.

Two further fixed decisions belong beside them. **The aspif dispatch**
(grammar §4.9): under the program entry, when the text's first token is
the identifier `asp` followed by exactly one space and a decimal
numeral, the input is not program text — the whole of it becomes one
`ERROR` child of `PROGRAM`, lossless, with the single diagnostic
`aspif-input` at the header; no other diagnostic is emitted, because
every further error would be noise about a language this parser was
told the input is not in. And **the annotations after the dot** (grammar
§5.11): after a statement's `.`, a `[` opens the `ANNOTATION` for
exactly the four families — required for weak constraints and
`#heuristic`, optional for `#external` and `#const` — and for any other
statement a `[` there is the next statement's unexpected token.

### 6.4 Trivia and docs, at parse time

Trivia is taken between any two tokens under §5.4's placement law and is
never a production's business — with the docs exception above, and one
lexical caveat the parser inherits from grammar §4.7's recorded
divergence D1: at the authority pin, a comment inside a theory expression
resets the authority's lexer, and this document, like the grammar,
states the region rule without that quirk; the differential (§16) pins
the authority's behavior and keeps such inputs out of the shared corpus.

### 6.5 Membership, error, and incompleteness

Three predicates, each defined once:

- **Membership.** An input is in the language under a dialect (grammar
  §2) exactly when its parse has no diagnostic of `Severity::Error`:
  `!parse.has_errors()`. Warnings — misplaced doc comments — do not
  affect membership, which is what makes the doc-comment extension
  membership-neutral in the tree as well as in the grammar. This
  equivalence is the differential's question (§16).
- **Error** is any diagnostic of error severity, lexical or structural,
  including the depth refusal and the aspif dispatch.
- **Incompleteness.** `parse.is_incomplete()` holds when the parse has
  errors and every error-severity diagnostic is one of: end of input
  where more was expected (`unexpected-end-of-input`), an unterminated
  block comment, an unterminated script region, or — under the
  ASP-Core-2 dialect only, where strings may span lines — an
  unterminated string. That is the REPL's typed answer to "keep reading
  or report?" and the language server's "the author is mid-edit". Its
  law: for every member program and every prefix of it that ends at a
  token boundary, the prefix's parse is either error-free or
  incomplete (§16).

### 6.6 Bounded depth, the constant, and its two bounds

```rust
/// The deepest nesting of the self-recursive term families (§6.2) the
/// parser will open — frames, one per bracket context. Named because
/// it carries meaning (spec §5.2), and documented with the two bounds
/// that fix its value.
pub const MAX_NESTING_DEPTH: u32 = /* fixed by measurement, see below */;
```

At the token that would open a frame beyond the constant, the parser
emits `nesting-too-deep` at that token and takes the rest of the
statement — through its terminating dot, or to end of input — into one
`ERROR` node under the innermost open frame, losslessly, opening nothing
further and diagnosing nothing further within that statement (the open
frames close over the `ERROR` node without missing-closer diagnostics of
their own: one refusal, one diagnostic). The tree's depth is therefore
at most the constant plus the grammar's fixed layer count (§5.4,
law 3).
This is a refusal with a locus, not a repair: nothing is truncated,
nothing is guessed, and the diagnostic says exactly what was refused and
where.

**The value is measured, not guessed, against two bounds.** From below:
no member of the corpus (§16) may reach it, and it must not fall short
of what the authority itself accepts — clingo's parser refuses very deep
nesting at its own parser-stack limit, and the differential (§16)
measures that depth at the pin; a value below it would be a membership
divergence, which the grammar's register would then have to carry with
its argument, and a value at or above it is not. From above: the depth
gate (§16) must pass with the constant in force — a thread of a small,
named stack size parses inputs nested far beyond the constant, then
walks, compares, and drops the tree — with headroom, because rowan's
drop and equality recurse in tree depth (§14) and the constant is what
bounds them. Both bounds are recorded beside the constant, and a move of
either — a rowan upgrade, a clingo pin move — re-measures.

### 6.7 Error recovery, per construct family

Spec §6.5: never fails, never panics; every input yields a tree and
diagnostics, and recovery is documented per family so consumers degrade
gracefully — the language server formats the statements around a broken
one, the solver frontend reports precisely. Two forms of defect appear
in the tree, and only two: a **missing** required child — the parser
emits `unexpected-token` (or `unexpected-end-of-input`) with the
expected set at the position, consumes nothing, and continues as if the
child were there, so the typed accessor for that slot answers `None`;
and an **unexpected** token — wrapped in an `ERROR` node, byte-preserved,
either alone (when the family can continue past it) or with everything
through the family's synchronization point (when it cannot). Every
parsing function has a synchronization set; the statement's terminating
dot — a `DOT` token, which no term, theory term, or bracketed construct
contains, so its depth is irrelevant — is in every set, which is what
makes an unclosed theory atom, an unclosed brace, or a runaway term
recover at the statement boundary (grammar §4.7's own note on the lone
`.`).

| family | on an unexpected token | synchronizes at |
|---|---|---|
| program level (no statement begins here) | `ERROR` through the next `.` — and an immediately following `[…]` group, since the four annotation families put one after the dot — or to end of input | the next statement start |
| head, body, condition | wrap the token; if it is a body separator or the neck, continue at the next element | `,` `;` `:-` `.` |
| literal, atom, comparison | missing-child diagnostics; the token stays for the enclosing family | the enclosing family's set |
| terms and argument lists (the frame loop) | a missing operand or unclosed bracket: diagnose, close the frame; an unexpected token: wrap and continue in the frame | `,` `;` `)` `]` `}` `\|` (in an absolute-value frame) `.` |
| aggregates | an unclosed `{`: diagnose the missing `}` at the statement's end | `;` `}` `.` |
| theory atoms and elements | as terms, under theory mode; an unclosed `{` recovers at the dot | `;` `}` at element depth, `.` |
| directives (`#show`, `#const`, …) | missing-child diagnostics; wrong words where the grammar wants a spelling (`default`, `unary`, `left`, …) are `unexpected-token` with the words in the expected set | `.` |
| `#theory` definitions | as directives, item by item | `;` `}` `.` |
| `#script` | an unterminated region is a lexical `ERROR` to end of input; the missing `#end` and dot are then missing children | end of input |
| annotations after the dot | an unclosed `[`: diagnose at end of statement | `]` and the next statement start |
| end of input anywhere | `unexpected-end-of-input` with the expected set; every open node closes | — |

Each row's shape is held by the golden corpus (§16): the tree dump and
diagnostics for a characteristic malformed input of each family are
reviewed artifacts, so recovery is a decision on the page and not
whatever the loop happened to do.

### 6.8 Purity and computational cost

A parse is a pure function of the token source's text and dialect and
the entry point (§5.4, law 4). Time is O(text): every token is requested
at most twice (§4.2), consumed once, and lookahead is bounded by a
constant; the frame loop does constant work per token; the builder's
work is O(tokens + nodes). Memory is O(text) for the tree plus O(depth)
for the frame stack, itself bounded by the constant. There is no
backtracking: every decision above is a bounded peek.

## 7. Diagnostics

Base §6.5's architecture, instantiated: this tier defines its own fully
typed diagnostic — matchable, exhaustive, carrying the expected set as a
real type — and lowers it into base's normal form for uniform rendering
and transport. In-process consumers act on the typed value; pipelines
that only render take `impl ToDiagnostic`.

### 7.1 The typed value

```rust
/// One syntax diagnostic: what happened, where, and what would settle
/// it. Located by construction — the primary span is required.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SyntaxError {
    /* private: kind: SyntaxErrorKind, primary: Location,
       related: BTreeSet<Label> (secondary loci: "the statement began
       here", "to close this `{`"), */
}

impl SyntaxError {
    pub fn kind(&self) -> &SyntaxErrorKind;
    pub fn id(&self) -> DiagnosticId;         // Appendix B; derived from kind
    pub fn severity(&self) -> Severity;       // likewise
    pub fn primary(&self) -> Location;
    pub fn related(&self) -> &BTreeSet<Label>;
}

impl ToDiagnostic for SyntaxError { /* §7.3 */ }

/// The closed roster of what can go wrong, each with its typed payload.
/// Declared in the order a parse meets them: lexical, then structural,
/// then the restrictions, then the warnings.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SyntaxErrorKind {
    // lexical — one per ERROR token (§4.5)
    UnexpectedCharacters,
    UnknownHashWord,
    MalformedString { defect: StringDefect },
    UnterminatedBlockComment,
    UnterminatedScript,
    AnonymousInTheoryExpression,
    // structural
    UnexpectedToken { expected: ExpectedSet, found: SyntaxKind, hint: Option<Hint> },
    UnexpectedEndOfInput { expected: ExpectedSet, hint: Option<Hint> },
    NestingTooDeep { depth: u32 },
    AspifInput,
    TokenSourceBreach { violation: TokenSourceLawViolation },
    // restrictions (§6.2)
    FormNotAllowedHere { form: RestrictedForm, context: Restriction },
    // warnings
    MisplacedDocComment { reason: MisplacedDoc },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StringDefect { RawLineBreak, InvalidEscape(char), Unterminated }

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RestrictedForm { Variable, AnonymousVariable, Pool, Interval,
                          ExternalCall, PooledAbsoluteValue }
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Restriction { ConstantTerm, TermValue }

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MisplacedDoc { NothingFollows, InsideStatement }

/// The characteristic mistakes the parser recognizes at an unexpected
/// token — each a shape the grammar or the corpus names, each carrying
/// one help text at lowering (§7.3). Closed; a hint is admitted here
/// when a golden case shows a reader needs it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Hint {
    /// `f(a,)` — no trailing comma in an argument list (grammar §5.1).
    TrailingCommaInArguments,
    /// A `?` ending the input under the clingo dialect — the ASP-Core-2
    /// query mark (grammar §6.1).
    QueryMarkNeedsAspCore2,
    /// Two numerals adjacent — a leading zero (grammar §4.3).
    LeadingZeroNumeral,
    /// `p(X) : | q(X)` — the empty-conditioned element before `|`
    /// (grammar §5.5); write `;`.
    EmptyConditionBeforePipe,
    /// `#heuristic … .` without its bracket (grammar §5.9).
    HeuristicNeedsAnnotation,
}
```

**The expected set is a set, and typed.**

```rust
/// What the parser would have accepted at a point: tokens by kind,
/// identifiers by spelling where the grammar wants a word (`default`,
/// `unary`, `left`, …), and grammar classes where listing tokens would
/// mislead (a term can begin nine ways; "a term" is what the reader
/// wants). A set — order carries no meaning, duplicates are defects, and
/// rendering derives its order (kinds, then words, then classes).
pub type ExpectedSet = BTreeSet<Expected>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Expected {
    Token(SyntaxKind),
    Word(&'static str),
    Class(SyntaxClass),
}

/// The grammar's classes a consumer or a message names as one thing.
/// Closed; each is a nonterminal or a family of the grammar of record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SyntaxClass {
    Statement, Head, BodyElement, Literal, Atom, Term, TheoryTerm,
    TheoryOperator, Guard, Signature, Condition, Annotation, EndOfInput,
}
```

Spec §6.6's "expected-set reporting at recovery points" is this type:
the language server completes from it, the solver frontend renders it,
and neither parses prose. `SyntaxClass::EndOfInput` is the expectation
the fragment entries carry after their construct (§6.1).

### 7.2 Identity and severity

Every kind lowers to exactly one identity in the `syntax` namespace and
one severity, both fixed by Appendix B and held by a snapshot of the
complete table (base §6.1's discipline): an identity, once shipped, is
stable, and renaming is a visible breaking change. Thirteen identities
serve the whole roster because richness lives in the typed payload and
in the derived helps, not in the identity: `unexpected-token` is one
identity whether the expected set holds a dot or a whole class, and a
characteristic mistake earns a typed `Hint` and its *help* on the same
identity — a trailing comma in an argument list, a `?` at the end of a
clingo-dialect input, a numeral with a leading zero, an empty-conditioned
head element before `|` (grammar §5.5's stated hole, with the help
"write `;` here") — not a new name. The hint is on the value, not
inferred at rendering, so the view derives and never decides.

### 7.3 Lowering and the messages

`ToDiagnostic` produces base's `Diagnostic` with the identity, the
severity, a headline, the primary label, the related labels as
secondaries, and the notes and helps the kind derives. The headline and
helps are derived text — a pure function of the kind and its payload,
naming tokens in backticks and classes in words, at the rust-analyzer
bar (spec §2 item 9) — and their quality is a reviewed artifact, not an
accident: every kind and every characteristic help has a golden
rendering through base's human view in the diagnostics corpus (§16, the
*diagnostics-quality* witness). Message texts are presentation and may
improve; identities, severities, and payload types are contract.

**Computational cost.** Construction is O(1) beyond the expected set
(O(log) per insert); lowering is O(payload); `id` and `severity` are
O(1).

## 8. The typed AST

Spec §6.3: cheap node wrappers, no second tree; spec §12.2: syntactic
accessors, no semantic opinions, so a solver frontend that wants
structure without the program tier has it. The `ast` module is a
wrapper per node kind and an enum per grammar class, all over the red
cursors of §5.

### 8.1 The conventions, stated once

- **One wrapper per node kind** in Appendix A, named by the kind in
  CamelCase (`RULE` → `ast::Rule`); **one enum per grammar class** whose
  alternatives are node kinds — `Statement`, `Head`, `BodyElement`,
  `Literal`'s inner form, `Term`, `TheoryTerm`, `Aggregate`,
  `AggregateElement` — each implementing `AstNode` by casting on the
  kind. `Fragment` (§6.1) and `Program` are the roots.
- **Accessors mirror the production's slots, in the production's
  order.** A single child is `Option<Child>` — `Option` because a
  recovered tree may lack it (§6.7), never as an opinion; repetition is
  `AstChildren<Child>` (rowan's typed child iterator); a token slot is
  `Option<SyntaxToken>` named for the token (`dot_token()`,
  `neck_token()`, `l_brace_token()`); a token that carries a value has a
  typed token wrapper (§8.3). The declaration order of every enum and
  every accessor list is the grammar of record's order — the roster of
  Appendix A is that order — so a reader following either follows both.
- **Every wrapper** derives `Clone, PartialEq, Eq, Hash, Debug` (positional,
  through the cursor), offers `syntax(&self) -> &SyntaxNode` (the
  escape to the tree), and is a view: `!Send`, borrowed from the model
  (§5.1).
- **Completeness is a law, not a hope:** every node kind has a wrapper,
  every production slot an accessor, held by a structural test over the
  roster (§16). Spec §1.1's lint-class consumers — a formatter's lint
  face, a solver frontend's own checks — read the whole language through
  this layer or the layer is defective.

### 8.2 Representative signatures

The shape, shown on the constructs whose accessors carry a decision;
the rest follow the conventions mechanically from Appendix A.

```rust
pub struct Program(SyntaxNode);
impl Program { pub fn statements(&self) -> AstChildren<Statement>; }

pub enum Statement {
    Rule(Rule), WeakConstraint(WeakConstraint), Optimize(OptimizeStatement),
    Show(ShowStatement), Project(ProjectStatement), Defined(DefinedStatement),
    Edge(EdgeStatement), Heuristic(HeuristicStatement),
    External(ExternalStatement), Const(ConstStatement), Script(ScriptStatement),
    Include(IncludeStatement), ProgramPart(ProgramStatement),   // `#program`: a part (spec §7.1); `Program` is the root
    TheoryDefinition(TheoryDefinition), Query(Query),
}

/// Every statement may be documented (grammar §5.11).
pub trait HasDocs: AstNode<Language = Asp> {
    /// The leading DOC_COMMENT tokens, in order — the statement's
    /// documentation. Empty when undocumented.
    fn doc_lines(&self) -> impl Iterator<Item = DocLine>;
    /// The covering range of the documentation, if any.
    fn docs_range(&self) -> Option<TextRange>;
}

pub struct Rule(SyntaxNode);
impl Rule {
    pub fn head(&self) -> Option<Head>;          // None for a constraint
    pub fn neck_token(&self) -> Option<SyntaxToken>;
    pub fn body(&self) -> Option<Body>;          // None for a fact; Some(empty) for `h :- .`
    pub fn dot_token(&self) -> Option<SyntaxToken>;
}

pub enum Head { Literal(Literal), Disjunction(Disjunction),
                Aggregate(Aggregate), TheoryAtom(TheoryAtom) }

pub struct Body(SyntaxNode);
impl Body { pub fn elements(&self) -> AstChildren<BodyElement>; }
pub enum BodyElement { Literal(Literal), ConditionalLiteral(ConditionalLiteral),
                       Aggregate(Aggregate), TheoryAtom(TheoryAtom) }
impl BodyElement {
    /// The default-negation prefix on aggregates and theory atoms in
    /// body position (grammar §5.6); a literal carries its own (§8.2's
    /// `Literal::negation`).
    pub fn negation(&self) -> Negation;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Negation { None, Default, DoubleDefault }   // `not`, `not not`

pub struct Literal(SyntaxNode);
impl Literal {
    pub fn negation(&self) -> Negation;
    pub fn inner(&self) -> Option<LiteralInner>;
}
pub enum LiteralInner { True(SyntaxToken), False(SyntaxToken),
                        Atom(Atom), Comparison(Comparison) }

pub struct Atom(SyntaxNode);
impl Atom {
    pub fn classical_negation_token(&self) -> Option<SyntaxToken>;  // the `-`
    pub fn name(&self) -> Option<Ident>;
    pub fn arguments(&self) -> Option<Arguments>;
}

pub struct Comparison(SyntaxNode);
impl Comparison {
    pub fn first(&self) -> Option<Term>;
    /// The chain after the first term: each step a relation and its
    /// right term (grammar §5.2's guard sequence).
    pub fn steps(&self) -> impl Iterator<Item = (Relation, Option<Term>)>;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Relation { Lt, Le, Gt, Ge, Eq, Neq }   // spelling on the token

pub enum Aggregate { Function(FunctionAggregate), Set(SetAggregate) }
pub trait HasGuards: AstNode<Language = Asp> {
    fn left_guard(&self) -> Option<Guard>;
    fn right_guard(&self) -> Option<Guard>;
}
pub struct Guard(SyntaxNode);
impl Guard {
    /// None means the grammar's default relation for its side (grammar
    /// §5.3): stated as absence, because that is what the author wrote.
    pub fn relation(&self) -> Option<Relation>;
    pub fn term(&self) -> Option<Term>;
}
pub struct FunctionAggregate(SyntaxNode);
impl FunctionAggregate {
    pub fn function(&self) -> Option<AggregateFunction>;
    pub fn elements(&self) -> AstChildren<AggregateElement>;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateFunction { Count, Sum, SumPlus, Min, Max }
pub struct SetAggregate(SyntaxNode);
impl SetAggregate {
    /// Elements are literals or conditional literals (grammar §5.3).
    pub fn elements(&self) -> impl Iterator<Item = SetElement>;
}
pub enum SetElement { Literal(Literal), ConditionalLiteral(ConditionalLiteral) }
/// A function aggregate's element: body-position (terms with an optional
/// condition) or head-position (terms, a literal, an optional
/// condition) — the parser knows the position and builds the kind
/// (grammar §5.3).
pub enum AggregateElement { Body(BodyAggregateElement), Head(HeadAggregateElement) }

pub enum Term {
    Binary(BinaryTerm), Unary(UnaryTerm), Pool(Pool), Function(FunctionTerm),
    External(ExternalTerm), Abs(AbsTerm), Constant(ConstantTerm),
    Variable(VariableTerm), Splice(SpliceTerm),
}
pub struct Pool(SyntaxNode);   // `( … )`: tuples separated by `;`
impl Pool {
    pub fn tuples(&self) -> AstChildren<Tuple>;
    /// `(a)` — exactly one tuple of one term with no trailing comma —
    /// is the term `a` parenthesized (grammar §5.1); this names it.
    pub fn parenthesized(&self) -> Option<Term>;
}
pub struct Tuple(SyntaxNode);
impl Tuple {
    pub fn terms(&self) -> AstChildren<Term>;
    pub fn trailing_comma_token(&self) -> Option<SyntaxToken>;
}
pub struct ConstantTerm(SyntaxNode);
impl ConstantTerm { pub fn constant(&self) -> Option<Constant>; }
pub enum Constant { Symbol(Ident), Number(NumberLit), String(StringLit),
                    Infimum(SyntaxToken), Supremum(SyntaxToken) }
/// `@name` and `@name(args)` — the syntax themelios's Rust `@`-functions
/// answer to (spec §9.6); the `@` and the name are separate tokens
/// (grammar §5.1).
pub struct ExternalTerm(SyntaxNode);
impl ExternalTerm {
    pub fn name(&self) -> Option<Ident>;
    pub fn arguments(&self) -> Option<Arguments>;   // None for the bare `@name`
}

pub struct TheoryAtom(SyntaxNode);
impl TheoryAtom {
    pub fn name(&self) -> Option<Ident>;
    pub fn arguments(&self) -> Option<Arguments>;
    pub fn elements(&self) -> Option<TheoryElements>;
    pub fn guard(&self) -> Option<TheoryGuard>;
}
pub struct TheoryOpTerm(SyntaxNode);
impl TheoryOpTerm {
    /// Operators and terms in the flat sequence grammar §5.8 admits;
    /// regrouping under a `#theory` definition is admission, above.
    pub fn items(&self) -> impl Iterator<Item = TheoryOpTermItem>;
}
pub enum TheoryOpTermItem { Op(SyntaxToken), Term(TheoryTerm) }   // THEORY_OP or KW_NOT

/// A `#script` statement, parsed because the shared syntax has it
/// (grammar §4.8, §5.9) and carried as opaque text: this crate never
/// runs, parses, or privileges an embedded script. themelios's own
/// extension language is Rust — ground-time `@`-functions and solve-time
/// propagators against public traits (spec §9.6), the syntax of which is
/// the `@`-call, `EXTERNAL_TERM` (§8.2) — and an embedded script is the
/// clingo-world compatibility path, executed only by a backend that
/// declares the capability (spec §9.1), never by themelios.
pub struct ScriptStatement(SyntaxNode);
impl ScriptStatement {
    /// The named language, as written — an identifier the grammar does
    /// not restrict; what a backend accepts is admission, above.
    pub fn language(&self) -> Option<Ident>;
    /// The SCRIPT_BODY token: the raw region, exact span — what a tool
    /// that handles the region (a formatter, an editor) hands to that
    /// language's own tooling (§17). None when the region is empty
    /// (`#end` directly after the parenthesis, §4.4) or missing under
    /// recovery; `end_token` tells the two apart.
    pub fn body(&self) -> Option<ScriptBody>;
    pub fn end_token(&self) -> Option<SyntaxToken>;
}

pub struct WeakConstraint(SyntaxNode);
impl WeakConstraint {
    pub fn body(&self) -> Option<Body>;
    pub fn annotation(&self) -> Option<Annotation>;
    pub fn weight(&self) -> Option<Term>;      // reads into the annotation
    pub fn priority(&self) -> Option<Term>;
    pub fn tuple(&self) -> impl Iterator<Item = Term>;
}
pub struct ConstStatement(SyntaxNode);
impl ConstStatement {
    pub fn name(&self) -> Option<Ident>;
    pub fn value(&self) -> Option<Term>;
    pub fn annotation(&self) -> Option<Annotation>;
    /// `[default]` or `[override]` (grammar §5.9), read by spelling.
    pub fn policy(&self) -> Option<ConstPolicy>;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ConstPolicy { Default, Override }
```

Two decisions in that shape deserve their reasons on the page. **One
`ANNOTATION` kind serves the four bracket families**, and each
statement's accessors read their meaning from it — weight, priority, and
tuple for a weak constraint; weight, priority, and modifier for
`#heuristic`; the value for `#external`; the policy word for `#const` —
because the *shape* is one (a bracketed term list after the dot, grammar
§5.11) while the *meaning* is the statement's; putting the meaning in
the statement's accessors draws every distinction a consumer must make
without four kinds that differ only in their parent. **`Pool` and
`Tuple` are kept in the grammar's uniform shape** — `(a)` is a pool of
one tuple of one term — because the grammar makes `(a)` and `(a,)`
distinct and the shape is what carries that; `Pool::parenthesized`
names the common case so a consumer that wants "the term inside the
parentheses" never re-derives the condition.

### 8.3 Token wrappers and values

```rust
/// Typed tokens over the valued kinds — rowan's `AstToken` idiom.
pub struct Ident(SyntaxToken);       // IDENT
pub struct Variable(SyntaxToken);    // VARIABLE | ANONYMOUS
pub struct NumberLit(SyntaxToken);   // NUMBER
pub struct StringLit(SyntaxToken);   // STRING
pub struct DocLine(SyntaxToken);     // DOC_COMMENT
pub struct Comment(SyntaxToken);     // LINE_COMMENT | BLOCK_COMMENT | SHEBANG_COMMENT | DOC_COMMENT (as trivia)
pub struct ScriptBody(SyntaxToken);  // SCRIPT_BODY

impl NumberLit {
    pub fn radix(&self) -> Radix;     // from the prefix; total, syntactic
    pub fn digits(&self) -> &str;     // the text after the prefix; total
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Radix { Decimal, Hexadecimal, Octal, Binary }

impl StringLit {
    /// The denoted text with the dialect's escapes resolved (grammar
    /// §4.4, §6.2). The dialect is the caller's to state — the same
    /// checked-at-use posture base §5 takes for column encodings — and
    /// the one refusal is a token whose spelling is not the dialect's
    /// string rule, which a token source other than the file lexer can
    /// supply (grammar §9's by-value literals); the file lexer's tokens
    /// never refuse.
    pub fn value(&self, dialect: Dialect) -> Result<String, InvalidStringLiteral>;
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidStringLiteral { pub at: ByteOffset }

impl DocLine {
    /// The text after the `%!` marker, untrimmed — comment text whose
    /// meaning is a tool's (grammar §8).
    pub fn content(&self) -> &str;
}
impl ScriptBody {
    /// The raw region, byte for byte.
    pub fn text(&self) -> &str;
    /// The region's value per grammar §4.8: the raw text with trailing
    /// blanks and tabs trimmed before `#end`.
    pub fn value(&self) -> &str;
}
impl Comment {
    /// The comment's content: for the line forms (line, shebang, doc)
    /// the text minus its trailing horizontal whitespace, since that
    /// whitespace is layout the rule swallowed on its way to the line
    /// end; for a block comment, the whole token text. This is what the
    /// certificates compare (§11).
    pub fn content(&self) -> &str;
    pub fn form(&self) -> CommentForm;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CommentForm { Line, Block, Shebang, Doc }
```

Numeral *values* are deliberately not here: the token's radix and digit
string are syntax; the integer they denote, its range, and the
authority's behavior at overflow (grammar §4.3, §11) belong to the
program tier's term algebra, which owns the numeric type. String values
are here because the grammar defines them lexically, per dialect.

**Computational cost.** Casting is O(1); a single-child accessor is
O(children of the node) — rowan iterates children — and a token accessor
likewise; iterators are lazy and O(1) per step; `StringLit::value` is
O(token); every other value accessor is O(1) or O(token).

## 9. Comment attachment, the owned policy

Spec §6.4: the attachment semantics — trailing beats leading beats
dangling, block-aware blank-line detach, the dual-role-token carve-out —
is specified, tested, and exposed as API by this tier, once, for every
consumer. It is a pure reading of the tree of §5, defined here as a
function of exactly four facts, and shipped in two forms.

### 9.1 What attachment is

A trivia comment — a `LINE_COMMENT`, `BLOCK_COMMENT`, or
`SHEBANG_COMMENT` token, or a `DOC_COMMENT` token in trivia position
(§5.4) — is attached to exactly one **anchor** in exactly one **slot**:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot { Leading, Trailing, Dangling }

/// One comment's attachment: the element it belongs to and how. The
/// anchor is a node or a significant token — a comment before `,` leads
/// the comma, which is what keeps it before the comma when a consumer
/// re-emits (kallos's transposition scar, spec §5.1); a comment on the
/// line of a rule's dot trails the rule.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Attachment { pub anchor: SyntaxElement, pub slot: Slot }
```

Statement documentation is not a comment for this section: a
`DOC_COMMENT` in docs position is structure the statement owns (§5.4),
and asking its attachment is refused as such.

### 9.2 The policy, as a total function

For a trivia comment `c` with parent node `P`, let `prev` be the nearest
preceding sibling of `c` that is not trivia (a node or a significant
token) and `next` the nearest following such sibling; both may be
absent. Let a **closer** be a token of kind `R_PAREN`, `R_BRACKET`,
`R_BRACE`, or `DOT`, or a `PIPE` whose parent is an `ABS_TERM` — the
tokens that end a construct rather than begin an element (the `|` of a
disjunction, by contrast, is a separator and an anchor like `;`; this is
spec §6.4's dual-role-token carve-out, and the tree decides the role
structurally rather than by glyph). Then:

1. **Trailing.** If `prev` exists and no line break stands in the text
   between `prev`'s end and `c`'s start — through any trivia between
   them, a multi-line block comment included — then `c` is
   `Trailing(prev)`.
2. **Leading.** Otherwise, if `next` exists, is not a closer, and no
   *empty line* separates any two consecutive members of the run from
   `c` through the trivia comments after it up to `next` — an empty
   line being a `WHITESPACE` token holding a line break, horizontal
   whitespace only, and a further line break — then `c` is
   `Leading(next)`. A comment above a blank gap does not lead what lies
   below the gap; the comments below the gap still do (the block-aware
   detach: computed per adjacent pair along the run, so a contiguous
   run shares one anchor and a gapped run splits at the gap).
3. **Dangling.** Otherwise `c` is `Dangling(P)`. Every comment has a
   parent, so this is total; `PROGRAM` is the bottom anchor.

**The four facts, and the stability they buy.** The function reads only:
(a) `c`'s parent and its non-trivia siblings; (b) whether a line break
separates `prev`'s end from `c`'s start; (c) whether an empty line
separates each adjacent pair along the run to `next`; (d) whether `next`
is a closer. Nothing else — not indentation, not the number of spaces,
not the position of anything outside `P`. So a transformation that
preserves those four facts preserves every attachment, and that is the
law a formatter needs: emit a trailing comment on its anchor's last
line, a leading run on its own lines directly above its anchor with no
empty line inside the run or before the anchor, a dangling comment
separated by an empty line or standing before a closer, and the reparse
attaches everything as before. Held as a property (§16): re-spacing
corpus inputs at random while preserving the four facts changes no
attachment; violating one fact deliberately changes exactly the
attachments that read it.

### 9.3 The two forms

```rust
/// A comment's attachment. Refuses a token that is not a trivia comment
/// — a doc line in docs position (structure, §5.4) or any significant
/// token — with the reason.
pub fn attachment(comment: &SyntaxToken) -> Result<Attachment, NotAttachable>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotAttachable { NotAComment { kind: SyntaxKind }, Documentation }

/// The comments attached to `anchor` in `slot`, in source order — the
/// inverse direction, for a consumer walking anchors.
pub fn comments(anchor: &SyntaxElement, slot: Slot)
    -> impl Iterator<Item = SyntaxToken>;

/// Every trivia comment under `node` with its attachment, in source
/// order, computed in one pass — the bulk form.
pub fn attachments(node: &SyntaxNode)
    -> impl Iterator<Item = (SyntaxToken, Attachment)>;
```

The two directions agree by law: `comments(a, s)` yields exactly the
comments `c` with `attachment(c) == Ok(Attachment { anchor: a, slot: s })`,
and `attachments(root)` yields each trivia comment under the root exactly
once — totality and single-valuedness (§16).

**Whitespace facts, exposed because the policy reads them and every
consumer re-derives them otherwise:**

```rust
/// No line break in the text between `a`'s end and `b`'s start.
pub fn same_line(a: &SyntaxElement, b: &SyntaxElement) -> bool;
/// An empty line in the whitespace directly between `a` and `b`
/// (adjacent elements; a comment between them is not "whitespace").
pub fn empty_line_between(a: &SyntaxElement, b: &SyntaxElement) -> bool;
/// The count of line breaks in the trivia between `a`'s end and `b`'s
/// start.
pub fn line_breaks_between(a: &SyntaxElement, b: &SyntaxElement) -> u32;
```

**Computational cost.** `attachment(c)` is O(the trivia between `prev`
and `next` around `c`) — local, allocation-free; `comments(anchor,
slot)` is O(the trivia adjacent to the anchor); `attachments(node)` is
O(subtree). A consumer that asks `attachment` for each comment of a run
of *m* comments pays O(m²) across the run, which is why the bulk form
exists: a formatter walks anchors or takes the bulk pass and pays O(n).
The whitespace facts are O(the trivia between the two elements).

**Why a function and not a table.** kallos kept attachment in a side
table keyed by node identity because its tree was not lossless and its
comments had to be re-injected (spec §5.1). This tree carries every
comment in place, so attachment is a *reading*, not a record: nothing
can go stale, nothing is stored, and the two directions cannot disagree
because both are the same rule read from the same tree. Base §7.4
rejected the side table for the program tier on a different ground —
tables cannot follow transformation — and the two arguments converge on
one shape.

## 10. The fusion oracle

Spec §6.2: beside the lexer lives the fusion oracle, the lexical
spacing theory answering "may this adjacency lose its whitespace". Here
it is not a theory to maintain but a fact to compute: this tier owns the
lexer, so the exact answer is one relex away.

```rust
/// What must stand between two tokens for each to lex as itself.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Separator {
    /// The two may abut: `left` followed directly by `right` lexes as
    /// `left`, then `right`. (Not `None`: a Rust reader's `None` is
    /// `Option`'s, and this is an answer, not an absence.)
    Nothing,
    /// Whitespace of any kind is required — abutting would fuse or split
    /// the tokens (`a` `b` → `ab`; `#sum` `+` → `#sum+`; `0` `x1` →
    /// `0x1`; `<` `=` → `<=` under theory mode).
    Whitespace,
    /// A line break is required: `left` runs to the end of its line — a
    /// line comment, a doc comment, a shebang — and swallows anything
    /// after it on that line.
    LineBreak,
}

/// The lexical context an adjacency stands in: the dialect and the mode
/// in force at `left`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LexContext { pub dialect: Dialect, pub mode: LexMode }

/// The oracle over texts: total, exact, O(|left| + |right|). `left` and
/// `right` are token texts; the answer is what the lexer would do to
/// them abutted under `context`.
pub fn separator_between(left: &str, right: &str, context: LexContext) -> Separator;

/// The oracle over tree tokens: derives the mode from `left`'s position
/// (§10.2) and answers for the texts. Total.
pub fn separator(left: &SyntaxToken, right: &SyntaxToken, dialect: Dialect) -> Separator;

/// The mode in force at a token, read from the tree: `ScriptBody` for a
/// script body, `Theory` inside a theory atom's elements or guard
/// (outside their conditions) and at a `#theory` definition's operator
/// positions, `Normal` elsewhere. Total.
pub fn lex_mode_of(token: &SyntaxToken) -> LexMode;
```

### 10.1 Why relexing is the whole oracle

Whether two tokens may abut is one question about the lexer: lexing
`left ++ right` from `left`'s start under `left`'s mode, is the first
token exactly `left`? If it is, `right` lexes as before — its own lexing
starts at its own offset, looks only forward, and sees the same text it
saw — so the answer is `Nothing`. If it is not, the pair needs a separator:
`Whitespace` suffices for every token that does not run to end of line
(a space begins no token's continuation, and no token but the line
forms extends across it), and the line forms need `LineBreak`. That is
the entire theory, and it is exact rather than *reachable-honest*: spec
§6.2's "reachable-honest, defaulting to keep" names the properties a
maintained classification table needs — kallos maintained one because
its lexer was not its own (spec §5.1) — and an oracle that computes the
answer has no default to fall back on and no reachability to hedge. The
grammar's named cases — the greedy theory-operator munch, the rule-neck
abutment `:` `-`, `#sum` `+`, `0` `x1`, `.` `.`, `*` `*`, `not` before a
name, a line comment before anything — are its regression tests, not
its definition (§16).

### 10.2 The mode of an adjacency

An adjacency's mode is `left`'s: the parser lexed `left` under the mode
in force at its start, and lexing `left ++ right` under that mode is
what a reparse would do at that offset. `lex_mode_of` reads it from the
tree structurally — the same regions §6.3 states, decided by ancestry:
`SCRIPT_BODY` is its own mode; a token under `THEORY_ELEMENTS` or
`THEORY_GUARD` is under `Theory` unless a `CONDITION` intervenes; a
`THEORY_OP` or `KW_NOT` at an operator position of an `OP_DEFINITION` or
`ATOM_DEFINITION` is under `Theory`; everything else is `Normal`.

**Computational cost.** `separator_between` is O(|left| + |right|):
one relex of a two-token text. `separator` adds `lex_mode_of`, which is
O(depth of the token) — bounded by §5.4's law 3 — and is O(1) in
practice. A formatter querying every adjacent pair pays O(text) in
total.

## 11. Token-stream equivalence

Spec §6.7: structural token-stream equivalence plus comment-sequence
comparison, native to the tier — the certificate a consumer claiming a
layout-only or spelling-preserving transformation gets, with its
witness. Two relations, one function.

### 11.1 The two streams

For a tree, the **token stream** is the sequence of its significant
tokens — every token whose kind is not trivia in its position: all
non-comment, non-whitespace tokens, plus `DOC_COMMENT` tokens in docs
position — in source order, each as `(kind, content)`; the **comment
sequence** is the sequence of its trivia comments in source order, each
as `(kind, content)`; `content` is §8.3's: the token's text, save that a
line-form comment contributes its text without trailing horizontal
whitespace, which is layout. `ERROR` tokens are significant tokens; a
transformation that changes one has changed the program's bytes where
they were not understood, and the certificate says so.

```rust
/// The significant tokens of the tree under `node`, in order.
pub fn token_stream(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken>;
/// The trivia comments under `node`, in order.
pub fn comment_sequence(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken>;
```

### 11.2 The certificate

```rust
/// Which claim is being certified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Certificate {
    /// Layout only: token streams equal by kind and content, comment
    /// sequences equal by kind and content. Nothing but whitespace
    /// changed.
    LayoutOnly,
    /// Up to spelling: as LayoutOnly, save that a token's content is
    /// compared after canonical respelling (§11.3) — the grammar's
    /// synonym pairs may have been normalized, and nothing else.
    UpToSpelling,
}

/// The first divergence, as a witness: which stream, the index in it,
/// and both sides — a side is `None` where its stream ended first. Each
/// side carries the token's kind, its content, and its location in its
/// own tree, so a formatter's `--safe` mode reports where in the input
/// and where in the output the claim broke.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mismatch {
    pub stream: Stream,
    pub index: usize,
    pub left: Option<Side>,
    pub right: Option<Side>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stream { Tokens, Comments }
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side { pub kind: SyntaxKind, pub content: String, pub location: Location }

/// The certificate: granted, or refused with the first divergence.
/// Total; O(|left| + |right|); iterative walks.
pub fn equivalent<T: AstNode<Language = Asp>>(
    left: &Parse<T>, right: &Parse<T>, certificate: Certificate,
) -> Result<(), Mismatch>;
```

The comparison is over token streams and not over tree shape, and that
is deliberate: with one grammar and a deterministic parser (§5.4, law
4), two texts with equal token streams under equal dialects parse to
structurally equal trees — a corollary held as an instrument in its own
right (§16, the arity-stream check kallos lifted into its certificate)
rather than folded into the certificate, so the certificate stays the
plain statement "the tokens are the same and the comments are the same"
that a consumer can read off the definition. It also makes the
certificate honest across recovery: two texts that differ only in
layout parse to equal token streams even where `ERROR` boundaries
would move under a shape comparison, and equal token streams is
exactly the claim.

Under `LayoutOnly` a formatter that strips trailing whitespace from a
comment line still passes, because that whitespace is layout by
definition (§8.3); a formatter that changes a doc comment's *content*
fails, because docs are significant tokens; one that moves a comment
across a token fails on the comment sequence's order.

### 11.3 Canonical spelling

```rust
/// The canonical spelling of a token that has synonyms (grammar §4.5,
/// §4.6): `=` for EQ, `!=` for NEQ, `#inf`, `#sup`, `#minimize`,
/// `#maximize`; every other token's content is its own canonical form.
/// Total; the identity on non-synonym kinds.
pub fn canonical_spelling(kind: SyntaxKind, content: &str) -> Cow<'_, str>;
```

The canonical member of each pair is the spelling the authority itself
renders when it prints its own syntax tree, so a formatter normalizing
to canonical spellings converges on what clingo prints and a differential
reader sees one form; the choice is checkable against the pinned binary
(§16). Idempotent, and closed over each synonym pair (§16). This is the
table a spelling-normalizing formatter reads — which spellings are
synonyms is language knowledge, and it lives here once.

**Computational cost.** `equivalent` is O(|left| + |right|), a single
zip over two lazy iterators; `token_stream` and `comment_sequence` are
lazy preorder walks; `canonical_spelling` is O(1).

## 12. Posture

Base §8's four rules and its std-trait posture bind this crate as they
bind base — observational purity, plain data, declarative construction,
the modeled shape dictating the type — and this section states only what
this tier adds or qualifies.

### 12.1 A parse is a pure function

Same text, dialect, and entry, same tree and diagnostics (§5.4, law 4).
No cache survives a call: rowan's node cache lives inside one builder
and dies with it; there is no interning table, no global, no
memoization — spec §7.8's incrementality preconditions bought without
its machinery. Where mechanism wants mutation — the frame stack, the
builder, the diagnostics vector under construction — it is local and
invisible at the surface (base §8.1).

### 12.2 The model is the green tree; cursors are views

The one qualification of base §8.2, stated in §5.1: `Parse` is data,
`Send + Sync`, and every red cursor and typed wrapper is a thread-local
view minted from it. Every other public type — tokens, diagnostics,
attachments, witnesses, the oracle's answers — is plain data.

### 12.3 The recursion discipline and the depth bound

Grammar §10's rule is binding on this crate's implementation and stated
here as posture: call-stack recursion where the grammar bounds depth;
explicit stacks where it is self-recursive; and, because the tree is
handed to consumers whose dependency recurses in its depth (§14), depth
bounded at construction (§6.6). Every walk this crate performs —
lexing, parsing, the typed accessors, attachment, the oracle, the
certificates, tree text — is iterative or grammar-bounded, and the
depth gate (§16) holds it per walk.

### 12.4 Refusal, and what is not a refusal

Refusal beats repair (spec §5.2): the doors that can refuse do so with
base's per-operation discipline (§13); a `Mismatch` is not a refusal but
the answer to the certificate's question; a diagnostic is not a refusal
but a value on a total parse. Nothing here normalizes, truncates, or
guesses: the tree holds every byte, `ERROR` nodes hold what was not
understood, and the depth refusal names its locus.

### 12.5 The std-trait posture

Every refusal type — `PositionRefusal` (base's, at the token door),
`InvalidStringLiteral`, `NotAttachable` — implements `Display` and
`std::error::Error`; `Mismatch` and `TokenSourceLawViolation` implement
`Display` (a witness in words) and `Error`, since a formatter's safe
mode `?`-composes them; `SyntaxKind` and `Dialect` implement `Display`
— the SCREAMING_SNAKE name and the dialect's name — and those renderings
are stable, being what dumps and goldens read; `SyntaxError` renders
through `ToDiagnostic` and base's views and claims no `Display` of its
own, so no second rendering of a diagnostic exists.

## 13. Failure semantics and computational costs, consolidated

Spec §2 item 8's obligation, discharged at design level. Nothing in this
crate panics on any input; the table names every refusing door, and
every operation not listed is total. Each row's refusal column is
exactly the operation's error type (base §3.2's discipline).

| operation | refuses with | cost |
|---|---|---|
| `TokenSource::token_at` (the file lexer's) | `PositionRefusal` (`OutOfBounds` \| `NotCharBoundary`) — base's condition, at the door where offset meets text | O(token) |
| `StringLit::value` | `InvalidStringLiteral` — a spelling not the dialect's | O(token) |
| `attach::attachment` | `NotAttachable` (`NotAComment` \| `Documentation`) | O(neighborhood) |
| `equiv::equivalent` | `Err(Mismatch)` — the answer, carrying its witness; not a refusal (§12.4) | O(left + right) |

Total (never refuse, never panic): `Lexer::new`; `check_token_source_laws`
(an empty report is the laws holding); `parse` and every entry point
(every input yields a tree — aspif input, unlawful token sources, and
nesting past the bound included, each with its typed diagnostic);
`Parse`'s accessors, `has_errors`, `is_incomplete`, `location`; every
tree operation of §5.2 within the depth bound; the coordinate
conversions; every `ast` cast and accessor (`Option` is absence under
recovery, never a refusal); `NumberLit::radix` and `digits`;
`DocLine::content`, `Comment::content`; `attach::comments`,
`attachments`, and the whitespace facts; `separator_between`,
`separator`, `lex_mode_of`; `token_stream`, `comment_sequence`,
`canonical_spelling`; `SyntaxError`'s accessors and lowering.

Costs, consolidated: lexing and parsing are O(text) in time and memory
(§4.6, §6.8); tree navigation is O(1) per step and O(children) per
accessor; attachment is O(neighborhood) per query and O(subtree) in
bulk (§9.3); the oracle is O(the two tokens) (§10.2); the certificate is
O(both streams) (§11.3); every walk is iterative or grammar-bounded, and
tree depth is bounded by the named constant (§6.6). The scaling benches
(§16) hold the shapes.

## 14. The dependency: rowan, audited

Spec §12.5 admits **rowan** as the one named exception to hand-writing —
pinned, with a written audit note, internal unsafe acknowledged,
exercised at scale daily by rust-analyzer, and the hand-rolled green/red
tree recorded as the reserved fallback. This is the note.

**Pin.** `rowan = "=0.17.0"` — the release of 2026-08-02, and the
version rust-analyzer's own manifest pins at this document's date, so
the "exercised at scale daily" claim is checkable against
rust-analyzer's lock file. Every rowan upgrade re-reads this note.

**What it is used for.** The green/red tree of §5 — structural sharing,
positional identity, cursors with parent links, iterative preorder,
`text()`, `SyntaxNodePtr`, and the builder with checkpoints and its
per-builder node cache. The `serde1` feature is off. Nothing else of
rowan's is reached for.

**The closure, enumerated, and held by the trust check.** With default
features off where rowan turns them off, the shipped closure of this
crate is exactly: `themelios-base`; `rowan`; `text-size` (the `u32`
offset newtype); `rustc-hash` (a hasher); `hashbrown` with default
features off (the node cache's table — the implementation std's own
`HashMap` is built on); `countme` (an object counter, a no-op unless
its feature is enabled, which rowan does not); `memoffset` (an offset
macro).
`memoffset` carries a build script — a compiler-feature probe through
`autocfg`, running no C and reaching no network — which is a closure fact
the auditor should hold and the trust check names as the one build
script in the pure closure, allowed by name; spec §12.3's "no build
scripts outside the sys crates" binds this workspace's crates, and this
crate has none. Its retiring condition is stated here: `core::mem::
offset_of!` has been stable since Rust 1.77, rowan needs nothing
`memoffset` provides beyond it, and an upstream change dropping the
dependency retires the exception without a design change here. The
unsafe inventory at the pin, by crate, so a rise is visible: rowan 44
sites (its thin `Arc`, its cursors, its green nodes and tokens),
hashbrown roughly 320 (the table), memoffset 23 (macro bodies), countme
1, text-size 1, rustc-hash 0. The trust check (§16) reads Cargo's
resolved graph — a subprocess over `cargo metadata`, the departure base's
trust test announced for stage 2 — and asserts the closure equals that
list, no crate links native code or is a `-sys` crate, no build script
exists in the closure but the named one, and this crate has no build
script of its own.

**Three facts about rowan's internals this design rests on,
version-scoped (spec §5.2).** At 0.17.0, dropping a green node recurses
through its children — depth of the drop is depth of the tree;
`token_at_offset` recurses likewise (rowan's own source says so at the
call site); and structural equality and the debug rendering of a green
node recurse through the children too. All are why the tree's depth is
bounded at construction (§5.4, law 3; §6.6); none is reachable by any
work-list discipline of this crate. And at
0.17.0 rowan carries no mutable-tree API — the release removed it — so
the read-only posture of §5.1 is rowan's own, and the tree-editing seam
(§17) will ride on green-level splicing if a consumer ever names it.

**The reserved fallback and its obligation.** Should the exception be
withdrawn — a security finding, an abandoned upstream, a closure that
grows — the fallback is a hand-rolled green/red tree implementing the
public rowan surface §5.2 lists, and only that; the aliases become
concrete types, and no consumer signature changes. That list is the
whole obligation, which is why §5.2 keeps it short and names it.

## 15. The formatter-facing surface, and what holds across the checkpoint

Spec §11: stage 2 exits through a first-consumer checkpoint — the
formatter, morphe, builds against this surface before later tiers harden
on it, and what it surfaces folds back into the tier. The checkpoint
tests the tier, not the consumer, so its object is named here: **the
surface a formatter consumes**, and **what this design holds stable
across the checkpoint** so that findings are findings about the tier's
ergonomics and not about a moving target.

**The surface, exactly:** `parse` and `Parse` (§5.5, §6.1); the tree
under its aliases and the kind roster (§5.2, Appendix A); the typed AST
and its token wrappers (§8), including `HasDocs`, `Comment::content`,
and `ScriptStatement::body`; attachment in both forms with the whitespace
facts (§9); the fusion oracle in both forms (§10); the two certificates,
the two streams, and `canonical_spelling` (§11); `Dialect` (§3); the
typed diagnostics with their lowering (§7); and from base, the line
index and the diagnostic views. A formatter of the black class uses all
of it: parse, walk the typed AST emitting tokens with the whitespace it
chooses under the oracle's veto, carry comments to their anchors, keep
docs and script bodies verbatim, normalize spellings through
`canonical_spelling` if it chooses, and refuse to write until
`equivalent(before, after, certificate)` grants the claim.

**Held stable across the checkpoint:** the kind roster's names for
grammar constructs; the tree laws of §5.4; the attachment policy's three
slots and four facts (§9.2); the two certificates' definitions (§11);
the oracle's exactness (§10.1); the diagnostic identities (Appendix B);
the entry points (§6.1); the token-source door and its laws (§4.3).
**Free to move on the checkpoint's findings:** accessor names and
shapes in `ast`; the whitespace-fact helpers' names and set; message
texts and helps; the exact `ERROR`-node shapes under recovery; the
convenience of the two forms of attachment and oracle. A finding that
one of the stable items is wrong is the checkpoint firing at the design,
and this document reopens for it.

**The other consumers' surfaces, named so the checkpoint's residue is
visible:** the language server takes `Parse`, the diagnostics and their
editor view, `is_incomplete`, `SyntaxNodePtr`, and base's line index;
the solver frontend takes the typed AST, `has_errors`, and the
diagnostics as its face; the REPL takes `parse` over a growing buffer,
`is_incomplete`, and `parse_term_value`; the macro tier takes
`TokenSource`, `check_token_source_laws`, `SPLICE`, and the fragment
entries; the program tier takes the typed AST and `Parse::location`;
comments-as-data readers take attachment and the content accessors.
Each of these hardens unexercised by a real consumer at stage 2's close
save the formatter's; that residual is spec §11's, named.

## 16. Assurance instruments for stage 2

Per spec §11 the stage is not done until these are green; per spec §10.1
the fuzz crate exists from the first weeks and proptest and criterion are
standing from the tier's landing. Every instrument is documented with
what it proves and what it cannot (spec §10.2).

- **The fuzz crate**, committed, corpus committed, run out of band and
  continuously: over arbitrary text under both dialects and every entry
  point — no panic, `text()` is the input, the parse terminates,
  `has_errors` and `is_incomplete` are consistent with the diagnostics,
  every trivia comment attaches, `equivalent(p, p, ·)` holds, and the
  tree's depth respects the bound. What it cannot prove: membership
  agreement with the authority — that is the differential's.
- **Property laws (proptest):** the token-source laws on the file lexer
  under every mode, and `check_token_source_laws` failing deliberately
  breaching sources; lexer totality and tiling on generated text heavy in
  multi-byte characters, `%`, `#`, and operator runs; parse determinism;
  the four tree laws; dialect neutrality on the shared subset (§3); the
  incompleteness law over corpus prefixes (§6.5); the oracle: for
  adjacent token pairs drawn from parsed corpus trees, `Nothing` means
  the pair reparses to itself abutted and `Whitespace` means it does not,
  and every grammar-named case answers as the grammar says; attachment
  totality, single-valuedness, the inverse law between the two forms,
  and stability under re-spacing that preserves the four facts (§9.2);
  the certificates' reflexivity through reparse, symmetry, and the
  arity-stream corollary (equal token streams, equal tree shapes);
  `canonical_spelling` idempotent and closed over the synonym pairs; the
  typed AST's completeness over the roster.
- **The differential** (feature-gated harness, out of band per
  milestone, clingo the authority — grammar §3): every corpus input and
  every §11 seed of the grammar parsed here and by the pinned clingo;
  agreement on membership (`!has_errors()` against the authority's
  acceptance) and on statement count and kinds; disagreements land in
  the grammar's divergence register with their argument. It also
  measures the authority's own nesting limit for §6.6's lower bound and
  checks the canonical spellings against the authority's printing. The
  tree-sitter-clingo cross-check runs beside it at the tier's landing
  and at every pin move (grammar §3).
- **Golden snapshots**, reviewed: the diagnostics corpus — the
  characteristic malformed programs of every family in §6.7 and every
  identity in Appendix B, rendered through base's human view (the
  *diagnostics-quality* witness, spec §3); tree dumps for the grammar's
  corner seeds; attachment dumps for kallos's scar corpus (spec §5.1);
  the recovery shape of each family's row.
- **The corpus** (spec §10.3), vendored with provenance: textbook
  encodings; the formatter-inherited inputs (kallos's clingofmt-derived
  inputs, MIT, inputs only, with their notice); clingo's and clingcon's
  own examples and test programs at the pinned commits (MIT); the
  grammar's §11 seeds as corner cases with stated expectations; every
  input parsed under its stated dialect with the expected outcome
  (member, or the diagnostic identities expected).
- **The depth gate:** a thread of a small, named stack size
  (`DEPTH_GATE_STACK_BYTES`) parses inputs nested far beyond
  `MAX_NESTING_DEPTH` in every self-recursive family, then walks the
  typed AST, runs attachment and both certificates, prints the tree,
  compares two such trees, and drops them — no overflow, the depth
  refusal reported, the constant's headroom measured (§6.6).
- **Scaling shapes (criterion):** parse linear in text; the certificate
  linear in both texts; bulk attachment linear in the tree; the oracle
  constant per pair. Shape assertions in the gate; absolute numbers out
  of band (spec §10.2).
- **The identity table**, snapshot-tested: Appendix B is the shipped
  table; a change is a visible diff.
- **The trust checks:** the closure allow-list over Cargo's resolved
  graph, FFI-free, the one named build script, none of this crate's own,
  `forbid(unsafe_code)` (§14).
- **Standing gates:** mutation per milestone; the workspace coverage
  floor; unused-code and unused-result warnings denied; documentation
  examples that run; the executable-claims standard for anything this
  crate says about itself (spec §10.4).
- **The witnesses this tier seeds** for the facade (spec §3):
  *comments-as-data*, *diagnostics-quality*, the syntax half of
  *hostile-input*, and the parse half of *asp-core-2*.

## 17. Reserved seams and non-goals

Named reserved seams — deferred with reasons and their arriving
consumers, not gaps:

- **Tree editing** (rust-analyzer's assist idiom): arrives with the
  language-server consumer class, beside base's fix-and-suggestion seam;
  rowan 0.17.0 keeps green-level splicing for it (§14). Until then, text
  is the edit medium (spec §6.8).
- **Durable identity across edits** — syntax-pointer *paths* at the
  typed-AST layer (spec §6.8): `SyntaxNodePtr` is positional identity
  now; the durable form arrives with the first incremental consumer.
- **Incremental reparse** (spec §7.8): rowan supports reparsing a
  balanced subtree; total reparse is the supported path until a
  consumer measures the need.
- **The composing-frontend seam** (grammar §8): the public entry points
  and the reusable `Lexer` exist now; an *extensible* kind space — a
  frontend's own node kinds beside this roster in one tree — is designed
  for (rowan kinds are a `u16`; a range can be reserved) and not built,
  its consumer being a frontend that does not yet exist.
- **A file-level documentation marker** (an inner-docs form beside
  `%!`): arrives when a documentation consumer names it; a `%!` run that
  no statement follows is diagnosed today, not repurposed.
- **Embedded-language handling for script bodies:** the region is
  opaque text with an exact span (`ScriptStatement::body`); a Python or
  Lua parser never enters this crate (spec §12.5), and a formatter or a
  language server that handles the region hands the text to that
  language's own tool — the shape editors and tree-sitter use for
  embedded languages. Nothing about it is a seam of themelios's
  extension story: that story is Rust (spec §9.6), and `#script` is
  compatibility with the clingo world, carried and never run here.
- **A second consumer of `UpToSpelling`** beyond the formatter: the
  relation exists because a formatter of the black class normalizes; a
  program-tier renderer that wants it composes it.
- **Interning:** none here — base §11 sent the question to this tier,
  and this tier's answer is that identifiers are token texts in a
  shared green tree, and symbol identity is the program tier's, where
  the term algebra lives.
- **`Severity::Hint`** and human-view options: base's seams, unchanged.
- **The clingo 6.x language:** a third surface until the grammar's
  upgrade protocol runs (grammar §12); nothing here anticipates it.

Non-goals, absolutely: styled formatting (spec §13); a language server; a
REPL; I/O of any kind — `#include` is parsed and never resolved;
admission — `#theory` matching of theory atoms, safety, arity, ASP-Core-2
strict conformance, the meaningful `#external` values (grammar §13);
semantics of any construct; evaluation of term values (grammar §5.10's
arithmetic is the program tier's); the aspif format beyond naming it;
parsing any embedded language.

## Appendix A. The kind roster

The complete `SyntaxKind`, in declaration order, with the grammar
production each realizes. Tokens are named for their spelling or class;
nodes for the grammar's nonterminal in SCREAMING_SNAKE. Where a
production has no node of its own the table says so and names what
carries it. This is the roster the typed AST mirrors (§8.1) and the
completeness test reads (§16).

**Tokens.**

| kind | realizes |
|---|---|
| `WHITESPACE` | grammar §4.1 `WHITESPACE`; one token per run |
| `LINE_COMMENT` | `LINE-COMMENT` |
| `BLOCK_COMMENT` | `BLOCK-COMMENT` (nesting per dialect) |
| `SHEBANG_COMMENT` | `SHEBANG-COMMENT` |
| `DOC_COMMENT` | `DOC-COMMENT` — significant in docs position, trivia elsewhere (§5.4) |
| `IDENT` | `IDENTIFIER` |
| `VARIABLE` | `VARIABLE` |
| `ANONYMOUS` | `ANONYMOUS` |
| `NUMBER` | `NUMBER` (all four radices; text preserved) |
| `STRING` | `STRING` (the dialect's rule) |
| `KW_CONST` … `KW_TRUE` | the `#`-keywords of grammar §4.5, one kind each, in the grammar's order: `KW_CONST`, `KW_COUNT`, `KW_DEFINED`, `KW_EDGE`, `KW_EXTERNAL`, `KW_FALSE`, `KW_HEURISTIC`, `KW_INCLUDE`, `KW_INF` (`#inf`, `#infimum`), `KW_MAX`, `KW_MAXIMIZE` (`#maximize`, `#maximise`), `KW_MIN`, `KW_MINIMIZE` (`#minimize`, `#minimise`), `KW_PROGRAM`, `KW_PROJECT`, `KW_SCRIPT`, `KW_SHOW`, `KW_SUM`, `KW_SUM_PLUS`, `KW_SUP` (`#sup`, `#supremum`), `KW_THEORY`, `KW_TRUE` |
| `KW_NOT` | `"not"` — the one reserved word; also the theory operator spelled `not` |
| `KW_END` | `"#end"`, the script terminator only (grammar §4.8) |
| `DOT` `DOTDOT` `COMMA` `SEMICOLON` `COLON` `NECK` `WEAK_NECK` `PIPE` | `.` `..` `,` `;` `:` `:-` `:~` `\|` |
| `L_PAREN` `R_PAREN` `L_BRACKET` `R_BRACKET` `L_BRACE` `R_BRACE` | the brackets |
| `PLUS` `MINUS` `STAR` `STAR_STAR` `SLASH` `BACKSLASH` `CARET` `AMPERSAND` `TILDE` `QUESTION` `AT` | `+` `-` `*` `**` `/` `\` `^` `&` `~` `?` `@` |
| `EQ` `NEQ` `LT` `LE` `GT` `GE` | `=`/`==`, `!=`/`<>`, `<`, `<=`, `>`, `>=` — synonyms share the kind |
| `THEORY_OP` | `THEORY-OP` (grammar §4.7), under theory mode |
| `SCRIPT_BODY` | `SCRIPT-BODY` (grammar §4.8), under script mode |
| `SPLICE` | the macro dialect's `splice` marker and operand (grammar §9); never from text |
| `ERROR` | a lexical error token (§4.5) |
| `EOF` | end of input; returned by a source, never in a tree |

**Nodes.**

| kind | realizes |
|---|---|
| `PROGRAM` | `program`; the program entry's root |
| `FRAGMENT` | the fragment entries' root (§6.1) |
| — | `docs`: no node; a statement's leading `DOC_COMMENT` tokens (§5.4) |
| `RULE` | `rule` (all five forms; a constraint has no head child) |
| `WEAK_CONSTRAINT` | `weak-constraint` |
| `OPTIMIZE_STATEMENT` | `optimize-statement`; the keyword token says which |
| `OPTIMIZE_ELEMENT` | `optimize-element` |
| `SHOW_STATEMENT` | `show-statement`, all four forms; children say which |
| `SIGNATURE` | `signature` |
| `PROJECT_STATEMENT` | `project-statement` |
| `DEFINED_STATEMENT` | `defined-statement` |
| `EDGE_STATEMENT` | `edge-statement` |
| `EDGE` | one `term "," term` pair of `edges` |
| `HEURISTIC_STATEMENT` | `heuristic-statement` |
| `EXTERNAL_STATEMENT` | `external-statement` |
| `CONST_STATEMENT` | `const-statement`; its term under the constant restriction |
| `SCRIPT_STATEMENT` | `script-statement` |
| `INCLUDE_STATEMENT` | `include-statement` |
| `PROGRAM_STATEMENT` | `program-statement` |
| `PARAMETERS` | `"(" [ id-list ] ")"` of a program statement |
| `THEORY_DEFINITION` | `theory-definition` |
| `TERM_DEFINITION` | `term-definition` |
| `OP_DEFINITION` | `op-definition` |
| `ATOM_DEFINITION` | `atom-definition` |
| `QUERY` | `query` (ASP-Core-2 dialect) |
| `ANNOTATION` | the bracketed annotation after the dot of the four families (grammar §5.11) |
| — | `conditional-dot`: no node; an optional `COLON` and `BODY` in the statement |
| `BODY` | `body-list`; also the empty body of `h :- .` and `: .` |
| — | `head`: no node; the child is one of `LITERAL`, `DISJUNCTION`, an aggregate, `THEORY_ATOM` |
| — | `body-element`: no node; the child is one of `LITERAL`, `CONDITIONAL_LITERAL`, an aggregate, `THEORY_ATOM`, with its negation tokens beside it in `BODY` |
| `LITERAL` | `literal`: negation tokens and one of `KW_TRUE`, `KW_FALSE`, `ATOM`, `COMPARISON` |
| `ATOM` | `atom` |
| `COMPARISON` | `comparison`, the whole chain |
| `CONDITIONAL_LITERAL` | `conditional-literal`, and every `literal ":" [condition]` shape: set-aggregate elements, disjunction elements with a condition |
| `CONDITION` | `condition`; present and empty when the colon is |
| `DISJUNCTION` | `disjunction`; separators as tokens |
| `FUNCTION_AGGREGATE` | `function-aggregate` with its guards as `GUARD` children |
| `SET_AGGREGATE` | `set-aggregate` with its guards |
| `GUARD` | `lguard` / `rguard` |
| `BODY_AGGREGATE_ELEMENT` | `fn-element` in body position |
| `HEAD_AGGREGATE_ELEMENT` | `fn-element` in head position |
| `THEORY_ATOM` | `theory-atom` |
| `THEORY_ELEMENTS` | `"{" [ theory-elements ] "}"` |
| `THEORY_ELEMENT` | `theory-element` |
| `THEORY_OPTERM` | `theory-opterm` (flat) |
| `THEORY_GUARD` | `theory-op theory-opterm` after the elements |
| `THEORY_SET` `THEORY_LIST` `THEORY_TUPLE` `THEORY_FUNCTION` | the bracketed and function theory terms; a theory term's constant, variable, or splice is `CONSTANT_TERM`, `VARIABLE_TERM`, `SPLICE_TERM` |
| `BINARY_TERM` | `term BINOP term`, by the precedence table |
| `UNARY_TERM` | `UNOP term` |
| `POOL` | `"(" pool ")"` |
| `TUPLE` | `tuple`, and each `[ terms ]` alternative of `arguments` |
| `ARGUMENTS` | `"(" arguments ")"` of a function, an atom, or an external call |
| `FUNCTION_TERM` | `IDENTIFIER "(" arguments ")"` |
| `EXTERNAL_TERM` | `"@" IDENTIFIER [ "(" arguments ")" ]` |
| `ABS_TERM` | `"\|" abs-arguments "\|"` |
| `CONSTANT_TERM` | `IDENTIFIER \| NUMBER \| STRING \| "#inf" \| "#sup"` as a term |
| `VARIABLE_TERM` | `VARIABLE \| ANONYMOUS` as a term |
| `SPLICE_TERM` | a splice in term or theory-term position (grammar §9) |
| `ERROR` | skipped or refused input, byte-preserved (§6.7, §6.6) |

## Appendix B. The diagnostic identities

The shipped table, snapshot-tested. Namespace `syntax`; severity fixed;
the typed kind each lowers from (§7.1).

| identity | severity | from |
|---|---|---|
| `syntax::unexpected-characters` | error | `UnexpectedCharacters` |
| `syntax::unknown-hash-word` | error | `UnknownHashWord` |
| `syntax::malformed-string` | error | `MalformedString` |
| `syntax::unterminated-block-comment` | error | `UnterminatedBlockComment` |
| `syntax::unterminated-script` | error | `UnterminatedScript` |
| `syntax::anonymous-in-theory-expression` | error | `AnonymousInTheoryExpression` |
| `syntax::unexpected-token` | error | `UnexpectedToken` |
| `syntax::unexpected-end-of-input` | error | `UnexpectedEndOfInput` |
| `syntax::nesting-too-deep` | error | `NestingTooDeep` |
| `syntax::aspif-input` | error | `AspifInput` |
| `syntax::token-source-breach` | error | `TokenSourceBreach` |
| `syntax::form-not-allowed-here` | error | `FormNotAllowedHere` |
| `syntax::misplaced-doc-comment` | warning | `MisplacedDocComment` |
