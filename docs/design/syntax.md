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
(a growing buffer read statement by statement, the term-value
sublanguage for symbols, and a typed answer to "is this input finished
or wrong?"); the **macro tier**, a compile-time client that hands its
token stream to this parser and no other (spec §8, law 1); **contract
extraction** and every comments-as-data reader; and the **program
tier**, which lowers the typed AST with byte-precise provenance. The
one-grammar rule (spec §2 item 3) is discharged here: this crate carries
the only parser of the language, and its fragment entry points and
token-source door exist so that no consumer needs a second.

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
closure is FFI-free and holds no build script of this workspace's own;
the one build script inside it is admitted by name under spec §12.3's
closure clause and argued in §14 — both asserted structurally over
Cargo's resolved graph (spec §12.3, §14). The base tier is re-exported
whole under a module name — `pub use themelios_base as base;` — so the
vocabulary every door here speaks (`Source`, `ByteOffset`, `Span`,
`Location`, `Severity`, `Diagnostic`, the line index, the views) is
reachable through this crate alone, which is what makes §2's "nothing
outside it" true rather than approximate. The workspace `rust-version`
floor. No I/O, no global state, no runtime (spec §1.2): a parse is a
pure function of its inputs (spec §6.8, §12). Every public *value* type
is plain `Send + Sync` data with one stated exception — rowan's red-tree
cursors, the typed wrappers over them, and an attachment, whose anchor
is a cursor, are views, not data (§5.1).

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
> a solver frontend, a REPL, the macro tier, the program tier, and a
> comments-as-data reader need nothing outside it and no second parser
> exists anywhere; every public operation is total and observationally
> pure; and no walk this crate performs, and no structure it hands out,
> has depth proportional to the input's nesting.

This design has failed — independent of any local defect — when any of
the following holds:

- A parse panics, diverges, or yields a tree whose text differs from its
  input, on any admitted text (spec §2 item 8, spec §6.3, spec §6.5).
- The parser admits or refuses an input the grammar of record does not,
  under either dialect, beyond the grammar's recorded divergences —
  grammar §11's register, whose entry D2 is this tier's depth bound
  (§6.6) (spec §4; grammar §2).
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
  significant token or a comment, or moved one across the other (spec
  §6.7, §11), or the fusion oracle certifies an adjacency the lexer
  would fuse (spec §6.2, §10).
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

**The tree carries no dialect; the parse does.** The green tree is data
of one shape under both dialects — kinds and texts, nothing that says
which rule formed a `STRING` — and it is handed on as subtrees, cursors,
and typed wrappers that have no parse to ask. So every operation whose
answer depends on the dialect takes it from the caller (`StringLit::value`
in §8.3, the oracle in §10), and `Parse` records the dialect it was
parsed under so a consumer holding the parse never guesses (§5.5's
`dialect()`, and `Parse::string_value`, the door that cannot be handed
the wrong one). The hazard, named because it is sharper than base §5's
analogue: `"a\nb"` is a valid string under both dialects and denotes
differently under each (grammar §4.4, §6.2), so a wrong dialect at
`StringLit::value` yields a plausible wrong `String`, not a refusal —
which is why the parse-level door exists and the free accessor's doc
says so.

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
    /// The kinds that are trivia wherever they stand: WHITESPACE,
    /// LINE_COMMENT, BLOCK_COMMENT, SHEBANG_COMMENT. DOC_COMMENT is not
    /// among them — its status is positional, and `role` (§5.4) is the
    /// predicate that answers it for a token.
    pub const fn is_trivia(self) -> bool;
    /// The comment forms: LINE_COMMENT, BLOCK_COMMENT, SHEBANG_COMMENT,
    /// DOC_COMMENT.
    pub const fn is_comment(self) -> bool;
    pub const fn is_keyword(self) -> bool;
    pub const fn is_token(self) -> bool;
    pub const fn is_node(self) -> bool;
    /// Every kind, in declaration order — the roster enumerated. Public
    /// because the completeness test (§16) that reads it is an
    /// integration test, and a consumer that walks the whole vocabulary
    /// reads it too.
    pub const ALL: &'static [SyntaxKind];
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

**Bounded re-lexing at region ends, stated once.** At the end of a
theory region the parser looks at a token under theory mode, decides
it does not extend the region — the guard-end rule of §6.3, which is
greedy — and takes it again under normal mode. The token-source door is
a pure function of `(offset, mode)` (§4.3), so taking a token again is
calling that function again at the same offset; no state is unwound.
Every token is requested at most a fixed number of times (twice, at a
region boundary), which keeps parsing linear (§6.8). A lexical
diagnostic is raised when an `ERROR` token is *placed in the tree*,
never at the door: a token peeked under one mode and discarded at a
region boundary raises nothing, so the token that lexes as an unknown
`#`-word under theory mode and as `#count` under normal mode leaves no
trace of its first reading.

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
    /// The whole text this source owns and tiles. Every token is a
    /// slice of it (the slice law), and every span this crate hands
    /// back is in its coordinates.
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

**A source owns its text.** The file lexer's text is base's admitted
`Source` (§4.4). A foreign source's text is one it declares — the macro
token source, whose host has already lexed the body and hands over Rust
tokens with literals by value (grammar §9), assembles a text of its own
from those tokens' spellings and tiles *that*; nothing in the laws
below asks where a text came from, only that the source answer for it.
Every token, every span, every `Location`, and the tree's text (§5.4)
are in the owning source's coordinates, under the identity it minted
(base §3.1); a host that embeds such a text re-bases into its own
coordinates by its own arithmetic (base §3.3), which is exact because
spans are byte-precise. By-value literals reach the tree as the
spellings the source chose, which is why a `STRING` token from a
foreign source may fail the dialect's rule and `StringLit::value` may
refuse it (§8.3); the file lexer's tokens never do.

**The laws, stated as contract.** A token source is bound by four:

1. **Tiling.** Starting at offset zero under `Normal` mode and advancing
   by each token's length, the tokens partition the text exactly and end
   at `EOF` at the text's length. (Under other modes tiling holds from
   any offset the parser reaches — the parser never asks elsewhere.)
2. **Slice.** `token_at(at, mode)?.text` is `&text()[at .. at + len]`:
   the token's text is a slice of the source's own text, never a
   synthesis at the door. This is what makes the tree's text the
   source's text (§5.4): the tree is built from the tokens' texts and
   nothing else.
3. **Determinism.** Same offset and mode, same answer — a source is a
   pure function of its text.
4. **Refusal.** `token_at` refuses exactly at offsets that are not
   positions of the text, and answers everywhere else.

What the parser can check, it checks: a refusal at an offset the parser
reached by tiling, an `EOF` before the text's end, or a token running
past it is a tiling breach the parser sees and treats as end of input,
with a diagnostic naming the breach (Appendix B; §7.1's `SourceBreach`
carries exactly the two breaches the parser can witness — a tiling
breach and a refusal — because the slice law is trusted and
determinism is unobservable in one pass). The slice law is not cheaply
checkable at every token — verifying it means comparing every token
against the text — so the parser trusts it, and that trust is this
contract's stated boundary, exactly as base §3.4 trusts coherence: what
holds it is test-time machinery:

```rust
/// The laws, checkable: tiles the source under `Normal` mode from zero,
/// checking tiling and the slice law at every token, probing
/// determinism by re-asking each token, and probing refusal once past
/// the end and once inside a multi-byte character of each token that
/// has one. Total; O(text) for a lawful source. Implementors run it in
/// their own tests; the file lexer passes by construction, and §16
/// exercises the checker against deliberately breaching sources. What
/// it does not exercise: `Theory` and `ScriptBody` mode — the modes
/// under which a foreign source forms operator runs and script bodies
/// — so an implementor holds those under its own tests, over inputs
/// its parser reaches.
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
  the closing quote — is one `ERROR` token from the opening quote: for a
  raw line break, through the character before the break (the break
  lexes as whitespace after it); for end of input, to the end; for a bad
  escape, *not* through the offending character alone — the token
  continues under the dialect's string rule past it, through the closing
  quote on that line or to the line break or end of input, whichever
  comes first. So a typo in an escape is one token and one diagnostic
  (its primary the escape, its related locus the literal's extent,
  §7.1), never a literal fragmented into program text whose stray
  closing quote opens a second string and carries the next statement
  into the wreck; `p("a\qb").` followed by `q.` leaves `q.` intact, and
  stands in the diagnostics corpus (§16). Under the ASP-Core-2 dialect
  only the end-of-input case exists (grammar §6.2).
- **An unknown `#`-word** is one `ERROR` token spanning the whole word
  by maximal munch (grammar §4.5).
- **An unterminated block comment or script region** is one `ERROR`
  token to the end of input.
- **Any other character that begins no token** — `!` and `$` outside
  their regions, `_` inside a theory expression (grammar §4.7), control
  characters, non-ASCII text outside strings and comments — joins the
  maximal run of such characters into one `ERROR` token per run. A run
  of `_` alone under theory mode is diagnosed as what it is, the
  anonymous variable where none is admitted
  (`anonymous-in-theory-expression`); every other run as
  `unexpected-characters` (§7.1).

Each `ERROR` token placed in the tree yields exactly one lexical
diagnostic (§7) — save inside an aspif dispatch (§6.3) or a
depth-refused statement (§6.6), whose one diagnostic stands for the
whole — so a hostile input of a megabyte of `$`, or of `_` inside a
theory atom, costs one token and one diagnostic, not a million. Error
tokens are significant tokens for the tree, the token stream, and the
certificates (§11): a formatter carries them verbatim.

### 4.6 Computational cost

`Lexer::new` is O(1). `token_at` is O(length of the token it returns),
with one dialect-inherent exception: an ASP-Core-2 string that closes by
maximal munch — a `\"` taken as the closing quote because no later quote
exists to escape it (grammar §6.2) — is decided by a scan ahead to that
next quote or to end of input, an added cost linear in the text from the
`\"` onward. A trailing `\"` cannot be judged escape-or-closer without
it. The scans do not compound: tiling a text is still O(text). The lexer
allocates nothing: a `Token` borrows the source. Memory is O(1) beyond
the source it borrows.

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
green tree. The red cursors, the typed AST wrappers over them, and an
`Attachment` (§9.1), whose anchor is a cursor, are *views*, `!Send`,
borrowed from the model by construction; a consumer that crosses
threads sends the `Parse` and mints cursors — and re-derives
attachments, a reading and not a record (§9.3) — on the other side.
That is one exception, stated once, and it is the reason the tree is
data first and cursors second.

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

Four laws, each held by an instrument (§16), and two definitions the
laws and the sections after them read — the role of a token, and the
empty node.

1. **Text.** For every parse over a lawful token source (§4.3), the
   root's text equals the source's text —
   `parse.syntax().text() == source.text()`, unconditionally: on valid
   programs, on garbage, on truncated input, under either dialect (spec
   §6.3). It holds because the tree is built from the tokens' texts and
   the tokens tile the text (§4.3); nothing is inserted, dropped, or
   normalized. For the file lexer, lawful by construction, that is every
   admitted text; for a foreign source that breaches the tiling law the
   parser stops at the breach with its diagnostic (§4.3), and the tree's
   text is the prefix tiled — the one case the law does not cover, named
   here rather than absorbed. Author whitespace is a `WHITESPACE` token
   in the tree; a formatter chooses to replace it, and the tree carries
   no opinion. This law is the tier's token-fidelity emit (spec §12.2):
   there is no emitter — `text()` renders the tree byte for byte, and
   `Display` on a node is that text.
2. **Trivia placement.** The parser opens a node at its first
   significant token and closes it at its last, so every trivia token is
   a child of the node that was open where the trivia stood: trivia
   before a node's first token belongs to the parent, trivia between a
   node's children to the node, trivia after its last token to the
   parent. Consequently **every non-empty node but a root begins and
   ends with a significant token**, and trivia between statements
   belongs to `PROGRAM`. Docs are the one shaping rule beyond it: a
   statement's documentation (grammar §5.11) is a leading run of
   `DOC_COMMENT` tokens *inside* the statement's node, significant, with
   the trivia between them — whitespace, and any comment that stands
   between two doc lines, which grammar §5.11 admits; a doc comment
   anywhere else is trivia under this law and diagnosed (§6.3). There
   is no `DOCS` node — the tokens' kind and position say what they are,
   and a wrapper would carry nothing they do not (Appendix A records the
   mapping).
3. **Bounded depth.** No tree this crate produces is deeper than
   `MAX_TREE_DEPTH` (§6.6), a bound derived from `MAX_NESTING_DEPTH` and
   two constants of the grammar: at most `MAX_NESTING_DEPTH` frames,
   each contributing at most the term families' per-frame layer count,
   under the fixed layer count of everything above the term families.
   Every axis along which the roster's kinds can nest is accounted for.
   Bracket contexts — a pool, an argument list, an absolute-value pair,
   a theory set, list, tuple, or function's arguments — are frames, and
   the parser refuses to open one past the constant, flattening the
   surplus losslessly under `ERROR` with a diagnostic. Operator chains
   and unary runs are *flat*: a `BINARY_TERM` holds one precedence
   level's whole chain and a `UNARY_TERM` a whole run of prefix
   operators (§6.2, §8.2), so a chain of any length deepens the tree by
   at most the number of precedence levels. Everything else nests by
   iteration or by the grammar's fixed layers (grammar §10). This is the
   law that makes the tree safe to hold: rowan's own drop and one of its
   queries recurse in tree depth (§14), a fact no work-list discipline
   in this crate can reach, so the depth is bounded at construction.
4. **Determinism.** Same text, same dialect, same entry point — a
   structurally equal green tree and equal diagnostics (spec §6.8).

**The role of a token, named once.** Whether a `DOC_COMMENT` token is a
statement's documentation or a stray comment is a fact of position, not
of kind, and it is read at every seam this document draws — the token
stream and the certificates (§11), attachment (§9), the two token
wrappers over the kind (§8.3), `HasDocs` (§8.2). So it has one name:

```rust
/// What a token is, where it stands. `Documentation`: a DOC_COMMENT in
/// docs position — a leading child of a statement node with only
/// trivia and other DOC_COMMENT tokens before it (law 2). `Trivia`:
/// whitespace, the plain comment forms wherever they stand, and a
/// DOC_COMMENT anywhere else. `Significant`: every other token. Total;
/// O(preceding siblings of the token).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenRole { Documentation, Trivia, Significant }

pub fn role(token: &SyntaxToken) -> TokenRole;
```

`SyntaxKind::is_trivia` (§4.1) answers by kind, for the kinds whose role
never varies; `role` answers by position, and every site that needs the
positional answer reads it here — the token stream is every token whose
role is not `Trivia`; a trivia comment is a token whose role is `Trivia`
and whose kind is a comment; `DocLine` casts exactly the
`Documentation` tokens and `Comment` exactly the trivia comments (§8.3).
When the reserved inner-docs form (§17) arrives, this definition moves
and the sites do not.

**The empty node, defined once.** Exactly three kinds may be empty —
zero-length, holding no token: `BODY` (the empty body of `h :- .` and
`: .`), `CONDITION` (a colon with nothing after it), and `TUPLE` (the
empty tuple of `()`, and the empty pooled alternatives of `f()`, `f(;)`,
and `f(a;)` — grammar §5.1). An empty node is placed immediately after
the token that licenses it — the neck, the colon, the opening
parenthesis or the pooling semicolon — and never holds trivia; trivia
after that token belongs to the parent, as law 2 says. Law 2 speaks of
non-empty nodes because of exactly these three, and §9.2 reads `prev`
and `next` over non-empty siblings for the same reason. No other kind is
ever empty: under recovery a missing child is *absent* (§6.7), never an
empty node.

### 5.5 The parse

```rust
/// The result of a parse: the green tree, the diagnostics, and the
/// facts a consumer needs to interpret both. Owned, `Send + Sync`,
/// cheap to clone (the tree is reference-counted). `T` is the typed
/// root the entry point yields (§6.1) — a view type, `!Send`, so it is
/// carried as `PhantomData<fn() -> T>`: a phantom that names `T`
/// without inheriting its auto-traits, and `Clone`, `PartialEq`, `Eq`,
/// `Debug` are implemented without a bound on `T` (the derive would add
/// one). §16 holds `Send + Sync` at compile time.
pub struct Parse<T: AstNode<Language = Asp>> {
    /* private: green: GreenNode, diagnostics: Vec<SyntaxError>,
       source: SourceId, dialect: Dialect, entry: EntryPoint,
       _root: PhantomData<fn() -> T> */
}

impl<T: AstNode<Language = Asp>> Parse<T> {
    /// A fresh root cursor over the tree — a view, minted on demand.
    pub fn syntax(&self) -> SyntaxNode;
    /// The typed root. Total: every entry point yields a root of the
    /// kind its `T` casts (§6.1), so this never fails.
    pub fn tree(&self) -> T;
    pub fn green(&self) -> &GreenNode;
    /// In the order the parser produced them — one order, by law 4
    /// (§5.4); a batch consumer that wants the shared batch order sorts
    /// by base's `canonical_order` after lowering.
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
    /// The denoted text of a string literal, under this parse's dialect
    /// — the door that cannot be handed the wrong one (§3). Refuses as
    /// `StringLit::value` does (§8.3): a spelling that is not the
    /// dialect's rule, which only a foreign token source can supply.
    pub fn string_value(&self, literal: &StringLit)
        -> Result<String, InvalidStringLiteral>;
}
```

`PartialEq` on `Parse` is structural through the green tree (rowan's
green equality is structural; the diagnostics, dialect, and identity are
plain data), which is what the determinism law (§5.4) is checked with.
Equality is recursive over depth inside rowan and therefore bounded by
law 3.

**Computational cost.** `syntax()` is O(1) (a root cursor); `tree()` is
O(1) (a cast); `has_errors` and `is_incomplete` are O(diagnostics);
`location` O(1); `string_value` O(token); clone O(diagnostics) — the
tree is shared. The tree's memory is O(tokens + nodes), with rowan's
per-parse node cache sharing identical tokens and small identical
subtrees — a per-builder cache, never a global one (spec §1.2).

## 6. The parser

### 6.1 Entry points and roots

```rust
/// What the parser is asked to read: a whole program, or one construct
/// family with a named consumer — the statement (the macro tier's
/// statement macros), the term (the macro tier's term positions), and
/// the term-value sublanguage (grammar §5.10: what a string parses to
/// when a caller asks for a symbol — the REPL and the query surface).
/// The REPL is not the statement entry's consumer: it parses a growing
/// buffer through the program entry and reads `is_incomplete` (§15).
/// Closed; a family is admitted here when a consumer names it (spec
/// §8's tiered vocabulary), and the addition is a breaking one, priced
/// by the pre-1.0 posture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntryPoint { Program, Statement, Term, TermValue }

/// The file door: an admitted source under a dialect. Total.
pub fn parse(source: &Source, dialect: Dialect) -> Parse<ast::Program>;

/// The general doors: any token source (§4.3). Total.
pub fn parse_program(source: &impl TokenSource) -> Parse<ast::Program>;
pub fn parse_statement(source: &impl TokenSource) -> Parse<ast::StatementFragment>;
pub fn parse_term(source: &impl TokenSource) -> Parse<ast::TermFragment>;
pub fn parse_term_value(source: &impl TokenSource) -> Parse<ast::TermFragment>;
```

`parse` is `parse_program` over `Lexer::new(source, dialect)`; it exists
so the common case names no lexer. **Every entry point yields a root of
one fixed kind**, which is what makes `Parse::tree()` total (§5.5): the
program entry's root is `PROGRAM`; the statement entry's root is
`STATEMENT_FRAGMENT` and the two term entries' root is `TERM_FRAGMENT`
— each a container holding leading trivia, the fragment's node when the
input held one, trailing trivia, and — when input remained after the
fragment — an `ERROR` node with the diagnostic that end of input was
expected. The alternative, a root of the fragment's own kind, would
make the root's kind depend on the input (a term entry over an empty
text has no term to be the root), and `tree()` would have to answer
`Option<T>` for some entries and `T` for others; one container kind per
family keeps one honest signature — and one per *family* rather than
one for all three, because a single fragment kind would answer `None`
from its `statement()` for two unrelated reasons (no statement in the
input; never a statement parse), an absence standing for two facts. The
two term entries share a root because they share a shape: both read a
term, and what differs is the restriction (§6.2), a fact of diagnostics
that `Parse::entry()` names. The typed roots for fragments:

```rust
impl ast::StatementFragment {
    /// None when the input held no statement.
    pub fn statement(&self) -> Option<ast::Statement>;
}
impl ast::TermFragment {
    /// None when the input held no term; `Parse::entry()` says which
    /// restriction (§6.2) the term was read under.
    pub fn term(&self) -> Option<ast::Term>;
}
```

**What each entry admits, stated once.** The program entry admits
grammar §5.11's `program` under the dialect — with the ASP-Core-2 query
at its end under that dialect (grammar §6.1) — and the aspif dispatch
(§6.3). The statement entry admits what one program position holds:
leading `docs`, one statement with its annotation family after the dot
where the grammar has one, and — under the ASP-Core-2 dialect — the
query form, an atom and the `?` at the fragment's end; these are the
shapes `Statement` enumerates (§8.2), which is why that enum holds
`Query`. The term entry admits grammar §5.1's `term` (through a macro
token source, the macro dialect's splice-bearing term), and the
term-value entry `value-term` under the restriction of §6.2 — no docs,
no dot, no annotation for either. Trailing trivia is admitted by every
entry; anything else after the construct is the `ERROR` node above,
with `SyntaxClass::EndOfInput` in its expected set (§7.1).

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
and for theory terms a set, list, tuple, or function's arguments). Input
depth becomes frame count, never call depth. Within a frame, operator
structure is built **flat per precedence level**: a `BINARY_TERM` node
holds one level's maximal chain — its operands interleaved with its
operator tokens, so `1 + 2 - 3` is one node of three operands and two
operators — and a tighter level is a child operand (`1 + 2 * 3` is an
additive node whose second operand is a multiplicative node); a
`UNARY_TERM` holds a whole run of prefix operators and its one operand
(`- - x`); `**`, the one right-associative level, is a flat chain like
the others, its associativity a fact the typed AST states rather than a
shape the tree nests (§8.2). This is the lift `COMPARISON` and
`THEORY_OPTERM` already are (Appendix A), applied to the term families'
operators, and it is what bounds the tree's depth per frame (§5.4,
law 3): the levels are the grammar's — interval loosest, then `^`, `?`,
`&`, additive, multiplicative, `**`, with unary tighter than every
binary (grammar §5.1) — so a frame nests at most that many operator
layers however long its chains. Theory terms are simpler still: grammar
§5.8 admits their operators without precedence, so a `THEORY_OPTERM` is
one flat sequence per frame. The constant-term and value-term forms are
the same loop under a **restriction context**, each a set stated
exactly: `ConstantTerm` excludes the variable, the anonymous variable,
the pool, the interval, and the pooled absolute value (grammar §5.9
admits `"|" constant-term "|"` and nothing wider); `TermValue` excludes
those and the `@`-call besides (grammar §5.10). The loop builds the
general shape and emits `form-not-allowed-here` (Appendix B) at each
excluded form, so a consumer sees the structure the author wrote and a
diagnostic naming what the position forbids, rather than an `ERROR`
blob.

**The loop's invariant, stated so its correctness is on the page.** The
frame stack mirrors the open bracket contexts of the text: one frame per
unclosed opener, the innermost on top, each frame carrying its closer,
its restriction context, and a *level stack*. A frame's level stack
holds the precedence levels currently open in it, strictly tighter from
bottom to top, each level with the builder checkpoint taken before its
first operand; every operand parsed so far in the frame lies inside the
topmost open level or inside a level already closed beneath it. Each
operand is parsed behind a checkpoint of its own, and the last
operand's checkpoint is kept, so a level can be opened around that
operand retroactively. On a binary operator of level ℓ after an
operand: the levels tighter than ℓ close, innermost first, each wrapped
from its checkpoint into its `BINARY_TERM`, and the last closed level's
checkpoint becomes the last operand's; if the top level is then ℓ, the
operator joins it; otherwise ℓ is opened at the last operand's
checkpoint. `**` is a level like the others — its chain stays flat and
right-folds in the typed AST. A run of prefix operators opens one
`UNARY_TERM` at the first of them and closes it after the one operand
that follows, which is why unary binds tighter than `**` here as at the
authority: `-2**2` is the `**` chain whose first operand is the unary
node. A closer, a separator, or a token that begins no term closes every
open level of the frame and then the frame; a missing operand or an
unclosed bracket does the same after its diagnostic (§6.7), so recovery
leaves every stack in the state this paragraph describes. The
restriction context is read at one point — form emission — and never
steers the parse. Termination and cost are §6.8's: each token advances
the loop, and each level and each frame is opened at most once for the
token that opens it and closed at most once.

### 6.3 The worded rules, realized

The grammar states five rules in words rather than productions; each is
a fixed, bounded decision in the parser:

- **The show-signature reading** (grammar §5.9): after `#show`, if the
  next significant tokens are `[-] IDENT / NUMBER .` — trivia legal
  between — the statement is the signature form; anything else is the
  term form. Five tokens of lookahead, the parser's maximum.
- **The query reading** (grammar §6.1): under the ASP-Core-2 dialect
  only, a `?` that is the final significant token and stands at the top
  level — enclosed by no open bracket — is the query mark. There the
  term loop does not take the `?` as the bitwise-or operator (one-token
  peek), and the statement parser, holding a bare atom in head position
  with nothing before it, closes a `QUERY`; a top-level `?` after
  something that is not a bare atom meets an ordinary unexpected token
  there. A `?` at end of input nested within an open bracket is instead
  a term still unfinished — the operator whose right operand has not
  arrived — so the parse is incomplete, not in error, and a member's
  prefix cut there is unfinished, never wrong (§6.5). Under the clingo
  dialect the same final `?` is the operator missing its right operand —
  end of input where a term was expected — and the diagnostic carries a
  help naming the other dialect's reading, so the practitioner arriving
  from the standard is told, not puzzled.
- **The theory regions** (grammar §4.7): the mode switches to theory
  after the `{` that opens a theory atom's elements (the `{` itself is
  taken under normal mode); an element's condition colon at element
  depth is taken under theory mode — a length-one structural run there —
  and returns the mode to normal for the condition's literals *and for
  the `;` or `}` that ends the condition*: that token ends a normal-mode
  region, and a `;` taken under theory mode would fuse with an operator
  character after it (`;-` is one THEORY_OP), which is why grammar
  §4.7's letter reads the `;` as the condition's end; theory mode
  resumes at the token after that `;`, and after a `}` continues into
  the guard. **The guard-end rule, greedy:** after the `}` the parser
  takes the next token under theory mode; if it is not a theory operator
  there is no guard, and that token is re-lexed under normal mode
  (§4.2); if it is, the guard opens and extends while the next token,
  taken under theory mode, continues the opterm from where it stands —
  an operator after a term, an operator or a term's first token after an
  operator — and the first token that does not is re-lexed under normal
  mode and the guard closes before it. On every member this coincides
  with grammar §4.7's letter — a member's guard is a complete
  theory-opterm, and the token after it can extend nothing; on a
  non-member whose guard trails off in a run of operators
  (`&a { x } > - not - , p.`) the two readings differ only in how that
  run is tokenized: greedy holds the operators as theory-mode tokens
  and diagnoses the missing term, where the letter, read literally,
  would re-lex an unbounded run. That is this design's own recovery
  choice (grammar §13 leaves recovery to it), named here so §6.2's
  one-token lookahead and §4.2's twice-per-token bound are honest.
  Inside a `#theory` definition, only the operator positions lex under
  theory mode. The mode the parser requested for each token is what
  `lex_mode_of` reconstructs (§10.2), and the law binding the two is
  §10.2's.
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
`#heuristic`, optional for `#external` and `#const` — and its interior
is parsed by the enclosing statement's production, arity and spelling
included: a weight, an optional `@`-priority, and an optional term tuple
for the weak constraint; a weight, an optional priority, and the
modifier for `#heuristic`; exactly one term for `#external`; exactly the
identifier `default` or `override` for `#const`. The node kind is one
because the bracket shape is one; a violation is `unexpected-token` with
the family's expected set, so `#external p. [a, b]` and
`#const n = 1. [foo]` are the non-members the grammar makes them. For
any other statement a `[` there is the next statement's unexpected
token.

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

### 6.6 Bounded depth, the constants, and the two bounds

```rust
/// The deepest nesting of bracket contexts — frames, one per open
/// bracket (§6.2) — the parser will open. Named because it carries
/// meaning (spec §5.2), and documented with the two bounds that fix
/// its value; no numeral stands here, and the plan records the
/// measured value with both bounds beside it.
pub const MAX_NESTING_DEPTH: u32 = /* fixed by measurement, see below */;

/// The stack, in bytes, on which every operation this crate performs
/// or hands out over the deepest tree it can build — dropping it,
/// comparing two, rendering one, walking the typed AST, attaching,
/// certifying — is proven to complete: the depth gate (§16) runs on a
/// thread of exactly this size and passes with headroom. A consumer's
/// thread that holds a tree needs at least this much, and a language
/// server's worker or a WASM host reads it here rather than
/// discovering it. This is the fixed pole, a product choice — sixty-four
/// mebibytes, eight times the eight-mebibyte main-thread default of the
/// two supported operating systems, a size a language server's worker
/// can be given without contortion — against which `MAX_NESTING_DEPTH`
/// is measured (below); a move of either re-measures the other.
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;

/// The bound on the tree's depth (§5.4, law 3), derived and carrying no
/// numeral of its own: `MAX_NESTING_DEPTH` frames, each contributing at
/// most the term families' per-frame layer count, under the fixed layer
/// count of everything above the term families — `TERM_LAYERS_PER_FRAME`
/// and `FIXED_LAYERS`, two public grammar constants named in the crate
/// beside this one, valued by inspection of Appendix A and documented
/// there and read by the depth instrument (§16). Public because a consumer who
/// recurses over the typed AST — the visitor a fluent reader will write
/// — sizes its own stack from it; `REQUIRED_STACK_BYTES` covers this
/// crate's and rowan's walks, not the consumer's.
pub const MAX_TREE_DEPTH: u32 = MAX_NESTING_DEPTH * TERM_LAYERS_PER_FRAME + FIXED_LAYERS;
```

At the token that would open a frame beyond the constant, the parser
emits `nesting-too-deep` at that token and takes the rest of the
statement — through its terminating dot and, for the four families that
carry one, the annotation group after it (grammar §5.11), or to end of
input — into one `ERROR` node under the innermost open frame,
losslessly, opening nothing further and diagnosing nothing further
within that statement; the statement then closes (the open frames close
over the `ERROR` node without missing-closer diagnostics of their own:
one refusal, one diagnostic). The tree's depth is therefore at most the
constant times the term families' per-frame layer count, plus the
grammar's fixed layer count above them — `MAX_TREE_DEPTH` (§5.4, law 3),
whose two grammar constants are named in the crate and valued by
inspection of Appendix A, and which the depth gate holds by measuring
the deepest tree it builds against it. Operator chains never reach the
constant: they are flat (§6.2), so `1+1+…+1` of any length is a member
here as it is under the authority, and only bracket nesting is refused.

This is a refusal with a locus, not a repair: nothing is truncated,
nothing is guessed, and the diagnostic says exactly what was refused and
where. It is the specified behavior for absurdly deep input that spec
§2 item 8 requires, and it applies under both dialects — a conformant
ASP-Core-2 program nested past the constant included.

**The value is measured, not guessed, against two bounds — and the
grammar's register carries the consequence.** From below: no member of
the corpus (§16) may reach it, and it should not fall short of what the
authority itself accepts — clingo's parser refuses very deep nesting at
its own parser-stack ceiling, and the differential (§16) measures that
depth per family at the pin. From above: the depth gate (§16) must pass
with the constant in force — a thread of `REQUIRED_STACK_BYTES` parses
inputs nested far beyond the constant in every family, then walks,
compares, renders, and drops the trees — with headroom, because rowan's
drop and equality recurse in tree depth (§14) and the constant is what
bounds them. Wherever the constant sits relative to the authority's
ceiling, the band between the two is a disagreement with the authority
— inputs one admits and the other refuses — and grammar §11 records it
as divergence D2, whose obligation is to hold both measured values
beside the entry; §2's second failure condition pins to that entry.
**When the two bounds conflict** — the authority's ceiling above what
the gate's stack survives — the gate's bound governs: safety over
parity, the constant stays where the gate proves it, and D2 widens
rather than the stack requirement growing. Both bounds are recorded
beside the constant, and a move of either — a rowan upgrade, a clingo
pin move — re-measures.

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
| terms and argument lists (the frame loop) | a missing operand or unclosed bracket: diagnose, close the open levels and the frame; an unexpected token: wrap and continue in the frame | `,` `;` `)` `]` `}` `\|` (in an absolute-value frame) `.` |
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
work is O(tokens + nodes). Memory is O(text) for the tree plus
O(frames × levels) for the frame and level stacks, themselves bounded
by the constant and the grammar's level count. There is no
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
       related: BTreeSet<Related> */
}

impl SyntaxError {
    pub fn kind(&self) -> &SyntaxErrorKind;
    pub fn id(&self) -> DiagnosticId;         // Appendix B; derived from kind
    pub fn severity(&self) -> Severity;       // likewise
    pub fn primary(&self) -> Location;
    pub fn related(&self) -> &BTreeSet<Related>;
}

/// A secondary locus, typed: what the location is, so that its text is
/// derived at lowering (§7.3) like every other text on the diagnostic
/// and a wording change is never a parser change. Closed; a locus is
/// admitted here when a golden shows a reader needs it, as a `Hint` is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Related { pub locus: RelatedLocus, pub location: Location }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RelatedLocus {
    /// "the statement began here"
    StatementBegan,
    /// "to close this `{`" — the opener a missing closer answers to.
    ToClose(SyntaxKind),
    /// "the literal, whole" — the string a bad escape sits in (§4.5).
    LiteralExtent,
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
    TokenSourceBreach { breach: SourceBreach },
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
pub enum MisplacedDoc { NoStatementFollows, InsideStatement }

/// The two breaches of the token-source laws (§4.3) the parser can
/// witness in one pass, both at an offset it reached by tiling:
/// `Tiling` — an `EOF` before the text's end, or a token running past
/// it, the kind and length saying which; `Refusal` — the door refused
/// where it owed a token. The slice law is trusted and determinism
/// unobservable in one pass, so neither appears here — the checker's
/// `TokenSourceLawViolation` is the wider type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SourceBreach {
    Tiling { at: ByteOffset, token: SyntaxKind, len: u32 },
    Refusal { at: ByteOffset },
}

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
/// `unary`, `left`, … — a `GrammarWord`), and grammar classes where
/// listing tokens would mislead (a term can begin nine ways; "a term"
/// is what the reader wants). A set — order carries no meaning,
/// duplicates are defects, and rendering derives its order (kinds, then
/// words, then classes).
pub type ExpectedSet = BTreeSet<Expected>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Expected {
    Token(SyntaxKind),
    Word(GrammarWord),
    Class(SyntaxClass),
}

/// The words the grammar wants by spelling where it has no token for
/// them (grammar §5.9): the ten identifiers matched by spelling in
/// `#const` annotations and `#theory` definitions. Closed, so an
/// expected set is matchable and a golden can enumerate it; `Display`
/// is the spelling. `ConstPolicy` (§8.2) is the statement-level
/// reading of the first two.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GrammarWord { Default, Override, Unary, Binary, Left, Right,
                       Head, Body, Any, Directive }

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
severity, a headline, the primary label, the related loci as secondary
labels with their text derived from the locus, and the notes and helps
the kind derives. The headline, the secondary texts, and the helps are
derived text — a pure function of the kind, the loci, and the payload,
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
  kind. `Program`, `StatementFragment`, and `TermFragment` (§6.1) are
  the roots.
- **When a production earns a node.** A production that is a pure
  alternation — `head`, `body-element`, `theory-def-item` — earns no
  node: its alternatives' kinds stand in its place. A production that
  owns tokens beyond alternation earns a node (`LITERAL` owns its `not`
  tokens) or hands them to the node it wraps — which is how
  `body-element`'s negation is placed: inside the aggregate or
  theory-atom node it signs (§8.2, Appendix A), the shape `LITERAL`
  already has, so every element's range covers its sign and
  `negation()` reads leading tokens the same way for every kind.
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
    TheoryDefinition(TheoryDefinition),
    /// Grammar §6.1's query — outside the grammar's `statement` class,
    /// inside this enum because the enum is the class of forms a
    /// program position holds, and the query holds the last one under
    /// the ASP-Core-2 dialect "like every statement" (§6.1).
    Query(Query),
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
    /// The element's default-negation prefix (grammar §5.6): the
    /// literal's own for `Literal`; the conditional literal's literal's
    /// for `ConditionalLiteral`; the aggregate's or theory atom's own for
    /// the other two — every variant delegates to its node, whose
    /// leading `not` tokens are inside it (§8.1).
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
    /// The leading `not` tokens in body position (grammar §5.6);
    /// `Negation::None` in head position, where none can stand.
    pub fn negation(&self) -> Negation;
    pub fn function(&self) -> Option<AggregateFunction>;
    pub fn elements(&self) -> AstChildren<AggregateElement>;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateFunction { Count, Sum, SumPlus, Min, Max }
pub struct SetAggregate(SyntaxNode);
impl SetAggregate {
    pub fn negation(&self) -> Negation;   // as FunctionAggregate's
    /// Elements are literals or conditional literals (grammar §5.3).
    pub fn elements(&self) -> impl Iterator<Item = SetElement>;
}
pub enum SetElement { Literal(Literal), ConditionalLiteral(ConditionalLiteral) }
pub struct Disjunction(SyntaxNode);
impl Disjunction { pub fn elements(&self) -> AstChildren<DisjunctionElement>; }
/// A disjunction's element (grammar §5.5): structurally `SetElement`,
/// named apart because a disjunction is not a set aggregate — a consumer
/// walking a disjunction meets `DisjunctionElement`, not a set's type
/// (least surprise; §15's accessor latitude).
pub enum DisjunctionElement { Literal(Literal), ConditionalLiteral(ConditionalLiteral) }
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
/// One precedence level's chain, flat (§6.2): `1 + 2 - 3` is one node
/// of three operands and two operators; a tighter level is an operand.
pub struct BinaryTerm(SyntaxNode);
impl BinaryTerm {
    /// The operands in source order — at least two when well formed;
    /// under recovery one may be missing (§6.7).
    pub fn operands(&self) -> AstChildren<Term>;
    /// The operator tokens in source order — one fewer than the operands.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken>;
    /// The chain's level, read from its first operator (grammar §5.1).
    pub fn level(&self) -> Option<Precedence>;
    /// Left at every level but exponentiation, right for `**` — the
    /// grammar's fact, carried here so no consumer re-derives it.
    pub fn associativity(&self) -> Option<Associativity>;
}
/// Grammar §5.1's levels, loosest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Precedence { Interval, BitXor, BitOr, BitAnd, Additive,
                      Multiplicative, Exponentiation }
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Associativity { Left, Right }
/// A run of prefix operators and its one operand, flat: `- - x` is one
/// node; unary binds tighter than every binary level (grammar §5.1).
pub struct UnaryTerm(SyntaxNode);
impl UnaryTerm {
    /// The prefix operators, outermost first.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken>;
    pub fn operand(&self) -> Option<Term>;
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
    pub fn negation(&self) -> Negation;   // body position only (grammar §5.6)
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

Three decisions in that shape deserve their reasons on the page. **A
chain is one node, and the classic nested shape is a derivation.**
`BinaryTerm` holds a level's whole chain flat because that is what
bounds the tree's depth (§5.4, law 3; §6.2) and because it is the shape
`Comparison` and `TheoryOpTerm` already have; the nested binary reading
— `(1 + 2) - 3`, `2 ** (3 ** 4)` — is a fold a consumer performs over
`operands()` and `operators()` in the direction `associativity()`
states, a loop over the chain and never a recursion in its length, which
is exactly how the program tier's lowering walks it. **One `ANNOTATION`
kind serves the four bracket families**, and each statement's accessors
read their meaning from it — weight, priority, and tuple for a weak
constraint; weight, priority, and modifier for `#heuristic`; the value
for `#external`; the policy word for `#const` — because the bracket
*shape* is one (grammar §5.11) while the interior is parsed by each
family's production (§6.3) and the *meaning* is the statement's; putting
the meaning in the statement's accessors draws every distinction a
consumer must make without four kinds that differ only in their parent.
**`Pool` and `Tuple` are kept in the grammar's uniform shape** — `(a)`
is a pool of one tuple of one term — because the grammar makes `(a)` and
`(a,)` distinct and the shape is what carries that;
`Pool::parenthesized` names the common case so a consumer that wants
"the term inside the parentheses" never re-derives the condition.

### 8.3 Token wrappers and values

```rust
/// Typed tokens over the valued kinds — the crate's own `AstToken`
/// trait (rowan exports `AstNode` but no token analogue; the idiom is
/// hand-rolled, as in rust-analyzer): a wrapper casts on the kind — and,
/// for the two comment wrappers, on the token's role (§5.4).
pub trait AstToken: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(token: SyntaxToken) -> Option<Self>;
    fn syntax(&self) -> &SyntaxToken;
    fn text(&self) -> &str;          // the token's text
}

pub struct Ident(SyntaxToken);       // IDENT
pub struct Variable(SyntaxToken);    // VARIABLE | ANONYMOUS
pub struct NumberLit(SyntaxToken);   // NUMBER
pub struct StringLit(SyntaxToken);   // STRING
pub struct DocLine(SyntaxToken);     // DOC_COMMENT whose role is Documentation (§5.4)
pub struct Comment(SyntaxToken);     // LINE_COMMENT | BLOCK_COMMENT | SHEBANG_COMMENT anywhere;
                                     // DOC_COMMENT whose role is Trivia — the two casts read
                                     // `role`, not the kind alone (§5.4)
pub struct ScriptBody(SyntaxToken);  // SCRIPT_BODY

impl NumberLit {
    pub fn radix(&self) -> Radix;     // from the prefix; total, syntactic
    pub fn digits(&self) -> &str;     // the text after the prefix; total
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Radix { Decimal, Hexadecimal, Octal, Binary }

impl StringLit {
    /// The denoted text with the dialect's escapes resolved (grammar
    /// §4.4, §6.2). The dialect is the caller's to state, because the
    /// tree does not carry it (§3) — and the caller must state it
    /// right: `"a\nb"` denotes differently under the two rules and a
    /// wrong dialect here yields a plausible wrong `String`, not a
    /// refusal, so a consumer holding the `Parse` uses
    /// `Parse::string_value` (§5.5) and takes the dialect from it. The
    /// one refusal is a token whose spelling is not the dialect's string
    /// rule, which a token source other than the file lexer can supply
    /// (grammar §9's by-value literals); the file lexer's tokens never
    /// refuse.
    pub fn value(&self, dialect: Dialect) -> Result<String, InvalidStringLiteral>;
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidStringLiteral { pub at: ByteOffset }

impl DocLine {
    /// The text after the `%!` marker, untrimmed — comment text whose
    /// meaning is a tool's (grammar §8), trailing whitespace included:
    /// a documentation tool may read it (two trailing spaces are a hard
    /// break in more than one markup), so it is content here and in the
    /// certificates (§11.1), never layout.
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
    /// The comment's content: for the line comment and the shebang, the
    /// text minus its trailing horizontal whitespace, since that
    /// whitespace is layout the rule swallowed on its way to the line
    /// end; for a doc comment in trivia position, the whole token text
    /// — the doc form's trailing whitespace is content wherever the
    /// token stands, for `DocLine::content`'s reason; for a block
    /// comment, the whole token text. This is what the certificates
    /// compare (§11).
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

A trivia comment — a token whose role is `Trivia` and whose kind is a
comment (§5.4): a `LINE_COMMENT`, `BLOCK_COMMENT`, or `SHEBANG_COMMENT`
anywhere, or a `DOC_COMMENT` outside docs position — is attached to
exactly one **anchor** in exactly one **slot**:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot { Leading, Trailing, Dangling }

/// One comment's attachment: the element it belongs to and how. The
/// anchor is a node or a significant token — a comment before `,` leads
/// the comma, which is what keeps it before the comma when a consumer
/// re-emits (kallos's transposition scar, spec §5.1); a comment on the
/// line of a rule's dot trails the rule. A view, not data: the anchor
/// is a cursor (§5.1), which is the shape a formatter holding the tree
/// wants — it navigates from the anchor directly — and it lives no
/// longer than the tree it reads.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Attachment { pub anchor: SyntaxElement, pub slot: Slot }
```

Statement documentation is not a comment for this section: a
`DOC_COMMENT` in docs position is structure the statement owns (§5.4),
and asking its attachment is refused as such.

### 9.2 The policy, as a total function

**The whitespace vocabulary, defined once.** A *line break* is `\n` —
base §5's newline policy, under which a `\r` is content of its line;
*horizontal whitespace* is the space, the tab, and `\r`; an *empty line*
is a `WHITESPACE` token containing two line breaks with only horizontal
whitespace between them (the token matches `\n[ \t\r]*\n`), so a CRLF
file's `\r\n\r\n` is an empty line exactly as an LF file's `\n\n` is.
The four facts below read these definitions and nothing else, and a
CRLF golden holds them (§16).

For a trivia comment `c` with parent node `P`, let `prev` be the nearest
preceding sibling of `c` that is neither trivia nor an empty node (§5.4)
— a non-empty node or a significant token — and `next` the nearest
following such sibling; both may be absent. Let a **closer** be a token
of kind `R_PAREN`, `R_BRACKET`, `R_BRACE`, or `DOT`, or a `PIPE` whose
parent is an `ABS_TERM` — the tokens that end a construct rather than
begin an element (the `|` of a disjunction, by contrast, is a separator
and an anchor like `;`; this is spec §6.4's dual-role-token carve-out,
and the tree decides the role structurally rather than by glyph). Then:

1. **Trailing.** If `prev` exists and no line break stands in the text
   between `prev`'s end and `c`'s start — through any trivia between
   them, a multi-line block comment included — then `c` is
   `Trailing(prev)`.
2. **Leading.** Otherwise, if `next` exists, is not a closer, and no
   *empty line* separates any two consecutive members of the run from
   `c` through the trivia comments after it up to `next` — an empty line
   as defined above — then `c` is `Leading(next)`. A comment above a
   blank gap does not lead what lies below the gap; the comments below
   the gap still do (the block-aware detach: computed per adjacent pair
   along the run, so a contiguous run shares one anchor and a gapped run
   splits at the gap).
3. **Dangling.** Otherwise `c` is `Dangling(P)`. Every comment has a
   parent, so this is total; `PROGRAM` is the bottom anchor.

**The four facts, and the stability they buy.** The function reads only:
(a) `c`'s parent and its non-trivia, non-empty siblings; (b) whether a
line break separates `prev`'s end from `c`'s start; (c) whether an empty
line separates each adjacent pair along the run to `next`; (d) whether
`next` is a closer. Nothing else — not indentation, not the number of
spaces, not the position of anything outside `P`. So a transformation
that preserves those four facts preserves every attachment, and that is
the law a formatter needs: emit a trailing comment on its anchor's last
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
/// An empty line in the whitespace directly between `a` and `b`; false
/// when anything but whitespace — a token, a node, a comment — stands
/// between them, so a non-adjacent pair answers false rather than
/// refusing.
pub fn empty_line_between(a: &SyntaxElement, b: &SyntaxElement) -> bool;
/// The count of line breaks in the text between `a`'s end and `b`'s
/// start — all of it, as `same_line` reads, so a significant token
/// between a non-adjacent pair counts.
pub fn line_breaks_between(a: &SyntaxElement, b: &SyntaxElement) -> u32;
```

**Computational cost.** `attachment(c)` is O(the trivia between `prev`
and `next` around `c`) — local, allocation-free; `comments(anchor,
slot)` is O(the trivia adjacent to the anchor) for `Leading` and
`Trailing`, and O(the anchor's children) for `Dangling`, whose comments
are scattered among them — for `PROGRAM`, the whole top level;
`attachments(node)` is O(subtree). A consumer that asks `attachment` for
each comment of a run of *m* comments pays O(m²) across the run, which
is why the bulk form exists: a formatter walks anchors or takes the bulk
pass and pays O(n). The whitespace facts are O(the trivia between the
two elements).

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

/// The mode in force at `token`'s start — the parser's standpoint,
/// reconstructed from the tree by §10.2's rule and bound to the parser's
/// own choice by law (§10.2, §16): `ScriptBody` for a script body and
/// for `KW_END`; `Theory` inside a theory atom's elements and guard —
/// outside their conditions and outside the `;` or `}` that ends a
/// condition — at a `#theory` definition's operator positions, and at
/// the first token after a theory atom (the guard-end peek); `Normal`
/// elsewhere. Total.
pub fn lex_mode_of(token: &SyntaxToken) -> LexMode;
```

### 10.1 Why relexing is the whole oracle

Whether two tokens may abut is one question about the lexer: lexing
`left ++ right` from `left`'s start under `left`'s mode, is the first
token exactly `left`? If it is, the answer is `Nothing`. If it is not,
the pair needs a separator: `Whitespace` suffices for every token that
does not run to end of line (a space begins no token's continuation,
and no token but the line forms extends across it), and the line forms
need `LineBreak`.

**Why the pairwise answer settles a whole text — the induction, and the
lemma it rests on.** A token's extent depends only on the text from its
own start forward (the lexer looks ahead, never back), so a text every
one of whose adjacent pairs answers `Nothing` lexes each token to
itself: the first by the first pair; each next one because the pair
before it fixed where its predecessor ends, hence where it starts, and
its own pair fixes that it ends where it did. The step has one gap,
named so it is not assumed: the pair `(left, right)` relexes only
`left ++ right`, and a token whose recognizer is *alive but not
accepting* after `left ++ right` could complete on characters of the
token after `right`. So the lemma is stated for what the oracle is asked
about — token sequences that arose from lexing a text — and not for
arbitrary sequences of spellings, and over this token language two
shapes have such a recognizer, each closed by its own argument. The
numeral prefixes `0x`, `0o`, `0b`: `0` then a name lexes as two tokens
and a following digit would make one, and the name's own rule closes the
gap — any character that would complete the numeral also extends the
name (grammar §4.2), so the pair `(right, next)` answers `Whitespace`
and the abutment never happens. The ASP-Core-2 string that ends in an
escaped-looking quote, `"a\"` (grammar §6.2): its recognizer, in the
escape reading, is alive after the token and would complete on any later
`"` — but that spelling lexes as a token only in a text where no `"`
follows anywhere after it, since a later quote would already have made
the longer string win at the original lexing, and re-spacing introduces
no quote; so the `next` that would complete it cannot exist. Both are
lemmas about the roster of §4 and §6, checked here and held by an
instrument (§16: corpus texts re-spaced to abut every pair the oracle
allows reparse to the same token stream, and the `"…\"`-final case among
the grammar-named cases); the certificate (§11) is the check a formatter
runs on the whole text regardless. That is the entire theory, and it is
exact rather than *reachable-honest*: spec §6.2's "reachable-honest,
defaulting to keep" names the properties a maintained classification
table needs — kallos maintained one because its lexer was not its own
(spec §5.1) — and an oracle that computes the answer has no default to
fall back on and no reachability to hedge. The grammar's named cases —
the greedy theory-operator munch, the rule-neck abutment `:` `-`, `#sum`
`+`, `0` `x1`, `.` `.`, `*` `*`, `not` before a name, a line comment
before anything — are its regression tests, not its definition (§16).

### 10.2 The mode of an adjacency

An adjacency's mode is `left`'s: the parser lexed `left` under the mode
in force at its start, and lexing `left ++ right` under that mode is
what a reparse would do at that offset. That mode is the parser's
*standpoint* — the region it stands in as it begins to read the token —
not where the token lands. The two part only at the greedy guard-end
(§6.3): there the parser stands inside the theory region while it reads
the first token after a theory atom, deciding under theory mode whether
a guard opens or extends before committing that token under normal mode,
and it is the standpoint, theory, that governs whether the next
character fuses — `.` before `&` is one `THEORY_OP` under theory, two
tokens under normal, so a formatter that read the committed `Normal`
would wrongly abut them. `lex_mode_of` reconstructs the standpoint from
the tree structurally — the same regions §6.3 states, decided by
ancestry and position: `SCRIPT_BODY` and `KW_END` are `ScriptBody`
(both are formed only under it, §4.4); a token under `THEORY_ELEMENTS`
or `THEORY_GUARD` is `Theory` — the condition-opening `:` included —
unless it is inside a `CONDITION` or is the `;` or `}` that directly
follows one, which are `Normal` (§6.3: they end a normal-mode region); a
`THEORY_OP` or `KW_NOT` at an operator position of an `OP_DEFINITION` or
`ATOM_DEFINITION` is `Theory`; the first token after a theory atom is
`Theory`, the guard-end peek; everything else is `Normal`, the `{` that
opens the elements among them (taken under normal mode, §6.3).
**The law binding the two statements.** The mode is a fact only about a
token whose lexing depends on it: whitespace and comments lex the same
under `Normal` and `Theory` and never stand alone under `ScriptBody`, so
their standpoint is a fact about nothing and the law says nothing of
them. For every other token — every one that is not trivia and not a
comment — `lex_mode_of(token)` equals the region the parser stood in at
that token's start: the non-`Normal` mode if it ever requested the token
inside a region there (the guard-end peek among those requests, before
the normal-mode commit), else `Normal`. This holds for a member: its
tree reflects the mode regions the parser walked. It need not hold under
recovery, where the tree serves losslessness, not the modes — a
malformed condition's contents land loose under `THEORY_ELEMENTS`, the
aspif dispatch wraps the whole input as one raw-text `ERROR` (§4.9) — so
the law, like the whole-text lemma, is a guarantee for members. Held by
a parse-time recording of the requested modes, reduced to that region,
compared against the reconstruction over the mode-sensitive tokens of
every member of the corpus (§16). The parser's rule (§6.3) and this
reconstruction are two statements of one fact, and the law is what keeps
them from agreeing by luck; the grammar's named cases gain `;-` after a
condition, `#end.`, and `#end .` (§16).

**Computational cost.** `separator_between` is O(|left| + |right|):
one relex of a two-token text. `separator` adds `lex_mode_of`, which is
O(depth of the token) — bounded by §5.4's law 3 — and is O(1) in
practice. A formatter querying every adjacent pair pays O(text) in
total.

## 11. Token-stream equivalence

Spec §6.7: structural token-stream equivalence plus comment-sequence
comparison, native to the tier — the certificate a consumer claiming a
layout-only or spelling-preserving transformation gets, with its
witness. Two certificates over one sequence, one function.

### 11.1 The sequence and its two projections

For a tree, the **non-whitespace sequence** is the sequence of every
token whose kind is not `WHITESPACE`, in source order — significant
tokens and trivia comments interleaved as they stand — each as
`(kind, content)`. Its two projections are the **token stream**, the
significant tokens (every token whose role is not `Trivia`, §5.4: all
non-comment, non-whitespace tokens plus `DOC_COMMENT` tokens in docs
position), and the **comment sequence**, the trivia comments (role
`Trivia`, kind a comment). `content` is per kind: a `LINE_COMMENT` or
`SHEBANG_COMMENT` contributes its text without trailing horizontal
whitespace, which is layout (§8.3); a `DOC_COMMENT` contributes its
whole text wherever it stands (§8.3: the doc form's trailing whitespace
is content); a `SCRIPT_BODY` contributes its `value()` — the grammar's
own trimming of the blanks before `#end` (grammar §4.8), layout by the
same argument; every other token its text. `ERROR` tokens are
significant tokens; a transformation that changes one has changed the
program's bytes where they were not understood, and the certificate
says so.

```rust
/// Every non-whitespace token under `node`, in order — the sequence the
/// certificates compare.
pub fn non_whitespace_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken>;
/// The significant tokens of the tree under `node`, in order.
pub fn token_stream(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken>;
/// The trivia comments under `node`, in order.
pub fn comment_sequence(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken>;
```

The certificates compare the one interleaved sequence and not the two
projections each on its own, because the projections lose the fact a
formatter must not change: where each comment stands among the tokens
around it. Two texts whose token streams agree and whose comment
sequences agree can still differ by a comment moved across a token —
the transposition kallos recorded (spec §5.1) — and only the
interleaved sequence sees it.

### 11.2 The certificate

```rust
/// Which claim is being certified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Certificate {
    /// Layout only: the non-whitespace sequences equal by kind and
    /// content. Nothing but whitespace changed — exactly that, since
    /// whitespace is all the sequence leaves out.
    LayoutOnly,
    /// Up to spelling: as LayoutOnly, save that a token's content is
    /// compared after canonical respelling (§11.3) — the grammar's
    /// synonym pairs may have been normalized, and nothing else.
    UpToSpelling,
}

/// The first divergence, as a witness: the index in the sequence and
/// both sides — a side is `None` where its sequence ended first. Each
/// side carries the token's kind, its content, and its location in its
/// own tree, so a formatter's `--safe` mode reports where in the input
/// and where in the output the claim broke; the kind says whether the
/// element that diverged is a comment or a significant token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mismatch {
    pub index: usize,
    pub left: Option<Side>,
    pub right: Option<Side>,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side { pub kind: SyntaxKind, pub content: String, pub location: Location }

/// The certificate: granted, or refused with the first divergence.
/// Compares the two sequences whatever the parses' dialects — a lexical
/// statement about two texts, meaningful across them; both roots are of
/// one family, as the one `T` fixes, and the two term entries build one
/// shape (§6.1), so the corollary below needs only equal dialects, which
/// is the caller's obligation. Total; O(|left| + |right|); iterative
/// walks.
pub fn equivalent<T: AstNode<Language = Asp>>(
    left: &Parse<T>, right: &Parse<T>, certificate: Certificate,
) -> Result<(), Mismatch>;
```

The comparison is over the sequence and not over tree shape, and that is
deliberate: it is what a consumer can read off the definition — the
tokens are the same, the comments are the same, and each stands where it
stood among the others — and it stays honest across recovery: two texts
that differ only in layout have equal sequences even where `ERROR`
boundaries would move under a shape comparison, and equal sequences is
exactly the claim. **The corollary, named and scoped.** With one grammar
and a deterministic parser (§5.4, law 4), two texts with equal
non-whitespace sequences parse to equal *significant-token shapes* — the
preorder over nodes and significant tokens, trivia dropped, which is
neither §5.5's green equality (that includes every trivia token) nor the
tree's text — provided they were parsed under equal dialects (a
`?`-final text parses to a `QUERY` under one dialect and to an error
under the other with equal sequences) — the root family is fixed by
`equivalent`'s one `T`, and the two term entries build one shape (§6.1),
so the entry needs no condition of its own — and outside the aspif
dispatch, which reads whitespace (the one space grammar §4.9 names) and
so can differ in shape between two texts of equal sequence. The
corollary is held as an instrument in its own right (§16, the
arity-stream check kallos lifted into its certificate) rather than
folded into the certificate.

Under `LayoutOnly` a formatter that strips trailing whitespace from a
line comment still passes, because that whitespace is layout by
definition (§8.3); one that strips it from a doc line fails, because a
doc line's whitespace is content (§8.3); one that changes a doc
comment's text fails; one that moves a comment across a token fails,
because the sequence's order changed; one that trims the blanks before
`#end` passes, because `SCRIPT_BODY` compares by its value.

### 11.3 Canonical spelling

```rust
/// The canonical spelling of a token that has synonyms (grammar §4.5,
/// §4.6): `=` for EQ, `!=` for NEQ, `#inf`, `#sup`, `#minimize`,
/// `#maximize`; every other token's content is its own canonical form.
/// Total; the identity on non-synonym kinds.
pub fn canonical_spelling(kind: SyntaxKind, content: &str) -> Cow<'_, str>;
```

The canonical member of each pair is the spelling the authority itself
renders when it prints its own syntax tree — `=`, `!=`, `#inf`, `#sup`
— so a formatter normalizing to canonical spellings converges on what
clingo prints and a differential reader sees one form; for those four
pairs the choice is checkable against the pinned binary (§16). The
optimize pair has no printed form to check: the authority's tree lowers
an optimize statement to weak constraints and prints it so, `:~ … [w@p]`
(`libclingo/src/astv2_str.cc:577` at the pin), so that pair's canonical
member is the roster's own spelling — `#minimize` and `#maximize`, the
spellings `KW_MINIMIZE` and `KW_MAXIMIZE` are named for (Appendix A) —
fixed here and not measured. Idempotent, and closed over each synonym
pair (§16). This is the table a spelling-normalizing formatter reads —
which spellings are synonyms is language knowledge, and it lives here
once.

**Computational cost.** `equivalent` is O(|left| + |right|), a single
zip over two lazy iterators; `non_whitespace_tokens`, `token_stream`,
and `comment_sequence` are lazy preorder walks; `canonical_spelling` is
O(1).

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
`Send + Sync`, and every red cursor, typed wrapper, and attachment —
whose anchor is a cursor (§9.1) — is a thread-local view minted from
it. Every other public type — tokens, diagnostics, witnesses, the
oracle's answers — is plain data.

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
| `Parse::string_value` | `InvalidStringLiteral` — as above, under this parse's dialect | O(token) |
| `attach::attachment` | `NotAttachable` (`NotAComment` \| `Documentation`) | O(neighborhood) |
| `equiv::equivalent` | `Err(Mismatch)` — the answer, carrying its witness; not a refusal (§12.4) | O(left + right) |

Total (never refuse, never panic): `Lexer::new`; `check_token_source_laws`
(an empty report is the laws holding); `parse` and every entry point
(every input yields a tree — aspif input, unlawful token sources, and
nesting past the bound included, each with its typed diagnostic);
`Parse`'s accessors, `has_errors`, `is_incomplete`, `location`; every
tree operation of §5.2 within the depth bound, on a thread of at least
`REQUIRED_STACK_BYTES` (§6.6); `role`; the coordinate conversions; every
`ast` cast and accessor (`Option` is absence under recovery, never a
refusal); `NumberLit::radix` and `digits`; `DocLine::content`,
`Comment::content`; `attach::comments`, `attachments`, and the
whitespace facts; `separator_between`, `separator`, `lex_mode_of`;
`non_whitespace_tokens`, `token_stream`, `comment_sequence`,
`canonical_spelling`; `SyntaxError`'s accessors and lowering.

Costs, consolidated: lexing and parsing are O(text) in time and memory
(§4.6, §6.8); tree navigation is O(1) per step and O(children) per
accessor; attachment is O(neighborhood) per query and O(subtree) in
bulk (§9.3); the oracle is O(the two tokens) (§10.2); the certificate is
O(both sequences) (§11.3); every walk is iterative or grammar-bounded, and
tree depth is bounded by `MAX_TREE_DEPTH` (§6.6). The scaling benches
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

**The one build script in the closure, admitted by name.** `memoffset`
carries a build script — a compiler-feature probe through `autocfg`,
running no C and reaching no network. Spec §12.3 states the rule in two
scopes: no build script among the workspace's own crates outside the
sys crates — this crate has none — and, inside a pure crate's
dependency closure, a build script admitted only by name, argued in
that crate's dependency audit note and allowed by name in the
structural check. This paragraph is that argument, and the trust check
(§16) is that allow-list, with `memoffset`'s script its one entry. Its
retiring condition is stated here: `core::mem::offset_of!` has been
stable since Rust 1.77, rowan needs nothing `memoffset` provides beyond
it, and an upstream change dropping the dependency retires the entry
without a design change here.

**The unsafe inventory, a dated reading.** Read from the vendored
sources at the pin when this design was drafted (2026-08-15) — a text
count of `unsafe` occurrences per crate, `rg -c unsafe` being its
reproduction: rowan 44 sites (its thin `Arc`, its cursors, its green
nodes and tokens), hashbrown about 320 (the table), memoffset 23 (macro
bodies), countme 1, text-size 1, rustc-hash 0. These numbers are an
observation with its method and date, not a claim an instrument holds;
what the trust check holds is the closure itself. A rowan upgrade
re-reads this note and re-counts. The trust check (§16) reads Cargo's
resolved graph — a subprocess over `cargo metadata`, the departure
base's trust test announced for stage 2 — and asserts the closure equals
the list above, no crate links native code or is a `-sys` crate, no
build script exists in the closure but the named one, and this crate has
no build script of its own.

**Three facts about rowan's internals this design rests on,
version-scoped (spec §5.2).** At 0.17.0, dropping a green node recurses
through its children — depth of the drop is depth of the tree;
`token_at_offset` recurses likewise (rowan's own source says so at the
call site); and structural equality and the debug rendering of a green
node recurse through the children too. All are why the tree's depth is
bounded at construction (§5.4, law 3; §6.6); none is reachable by any
work-list discipline of this crate — and the reason a crate-owned
iterative drop, equality, or dump is not the guard belongs beside that
claim: consumers hold rowan's own handles through the aliases of §5.2,
so rowan's `Drop` runs on the last clone a consumer holds and is not
this crate's to replace, and a consumer's own recursion over the typed
AST is beyond any discipline here; only a depth bounded at construction
reaches every holder of the tree. And at 0.17.0 rowan carries no
mutable-tree API — the release removed it — so the read-only posture of
§5.1 is rowan's own, and the tree-editing seam (§17) will ride on
green-level splicing if a consumer ever names it.

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
under its aliases, the kind roster, and `role` (§5.2, §5.4, Appendix A);
the typed AST and its token wrappers (§8), including `HasDocs`,
`Comment::content`, and `ScriptStatement::body`; attachment in both
forms with the whitespace facts (§9); the fusion oracle in both forms
(§10); the two certificates, the sequence and its two projections, and
`canonical_spelling` (§11); `Dialect` (§3); the typed diagnostics with
their lowering (§7); and, through this crate's re-export of base (§1),
the line index and the diagnostic views. A formatter of the black class
uses all of it: parse, walk the typed AST emitting tokens with the
whitespace it chooses under the oracle's veto, carry comments to their
anchors, keep docs and script bodies verbatim, normalize spellings
through `canonical_spelling` if it chooses, and refuse to write until
`equivalent(before, after, certificate)` grants the claim.

**Held stable across the checkpoint:** the kind roster's names for
grammar constructs; the tree laws of §5.4 and the role of a token; the
attachment policy's three slots and four facts (§9.2); the two
certificates' definitions (§11); the oracle's exactness (§10.1); the
diagnostic identities (Appendix B); the entry points (§6.1); the
token-source door and its laws (§4.3). **Free to move on the
checkpoint's findings:** accessor names and shapes in `ast`; the
whitespace-fact helpers' names and set; message texts and helps; the
exact `ERROR`-node shapes under recovery; the convenience of the two
forms of attachment and oracle. A finding that one of the stable items
is wrong is the checkpoint firing at the design, and this document
reopens for it.

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
  every trivia comment attaches, `equivalent(p, p, ·)` holds,
  `lex_mode_of` agrees — over a member's mode-sensitive tokens — with the
  region the parser recorded, and the tree's depth respects the bound.
  What it cannot prove: membership agreement with the authority — that is
  the differential's.
- **Property laws (proptest):** the token-source laws on the file lexer
  under every mode, and `check_token_source_laws` failing deliberately
  breaching sources; lexer totality and tiling on generated text heavy
  in multi-byte characters, `%`, `#`, and operator runs; parse
  determinism; the four tree laws; dialect neutrality on the shared
  subset (§3); the incompleteness law over corpus prefixes (§6.5); the
  oracle: for adjacent token pairs drawn from parsed corpus trees,
  `Nothing` means the pair reparses to itself abutted and `Whitespace`
  means it does not, every grammar-named case answers as the grammar
  says (`;-` after a condition, `#end.`, `#end .`, and the ASP-Core-2
  `"…\"`-final string among them), and the whole-text lemma (§10.1): a
  member's text re-spaced to abut every pair the oracle allows reparses
  to the same token stream — a non-member's ERROR tokens do not compose,
  so both this and the mode law are guarantees for members; the mode law
  (§10.2): the parser's recorded region modes equal `lex_mode_of` over
  every member's mode-sensitive tokens; attachment
  totality, single-valuedness, the inverse law between the two forms,
  and stability under re-spacing that preserves the four facts (§9.2);
  the certificates' reflexivity through reparse, symmetry, and the
  corollary (equal non-whitespace sequences, equal significant-token
  shapes, under equal dialects and one root family, outside the aspif
  dispatch); `canonical_spelling` idempotent and closed over the synonym
  pairs; the typed AST's completeness over the roster;
  `Parse<T>: Send + Sync` for every root, asserted at compile time
  (§5.5).
- **The differential** (feature-gated harness, out of band per
  milestone, clingo the authority — grammar §3): every corpus input and
  every §11 seed of the grammar parsed here and by the pinned clingo;
  agreement on membership (`!has_errors()` against the authority's
  acceptance) and on statement count and kinds; disagreements land in
  the grammar's divergence register with their argument. It also
  measures the authority's own nesting ceiling, per family, for §6.6's
  lower bound and grammar §11's D2, and checks the four printable
  canonical spellings (§11.3) against the authority's printing. The
  tree-sitter-clingo cross-check runs beside it at the tier's landing
  and at every pin move (grammar §3).
- **Golden snapshots**, reviewed: the diagnostics corpus — the
  characteristic malformed programs of every family in §6.7 and every
  identity in Appendix B, the bad escape inside a literal (§4.5) and the
  depth refusal on an annotated family (§6.6) among them, rendered
  through base's human view (the *diagnostics-quality* witness, spec
  §3); tree dumps for the grammar's corner seeds; attachment dumps for
  kallos's scar corpus (spec §5.1) and for a CRLF-authored input (§9.2's
  empty line); the recovery shape of each family's row.
- **The corpus** (spec §10.3), vendored with provenance: textbook
  encodings; the formatter-inherited inputs (kallos's clingofmt-derived
  inputs, MIT, inputs only, with their notice); clingo's and clingcon's
  own examples and test programs at the pinned commits (MIT); the
  grammar's §11 seeds as corner cases with stated expectations, and
  this design's own — `#const x = |1;2|.` with its
  `form-not-allowed-here`, `#external p. [a, b]` and `#const n = 1. [foo]`
  as non-members (§6.3); every input parsed under its stated dialect
  with the expected outcome (member, or the diagnostic identities
  expected).
- **The depth gate:** a thread of exactly `REQUIRED_STACK_BYTES` (§6.6)
  parses inputs nested far beyond `MAX_NESTING_DEPTH` in every
  self-recursive family — bracket nesting in each of them, and beside it
  the bracket-free shapes: additive, exponentiation, and unary chains of
  a length far beyond the constant, which must *not* deepen the tree
  (§6.2) — then walks the typed AST, runs attachment and both
  certificates, prints the tree, compares two such trees, and drops them
  — no overflow, the depth refusal reported for the bracketed inputs and
  no refusal for the chains, the deepest tree measured against law 3's
  bound, the constant's headroom measured (§6.6).
- **Scaling shapes (criterion):** parse linear in text; the certificate
  linear in both texts; bulk attachment linear in the tree; the oracle
  constant per pair. Shape assertions in the gate; absolute numbers out
  of band (spec §10.2).
- **The identity table**, snapshot-tested: Appendix B is the shipped
  table; a change is a visible diff.
- **The trust checks:** the closure allow-list over Cargo's resolved
  graph, FFI-free, the one named build script, none of this crate's own,
  `forbid(unsafe_code)` (§14). The check reads `cargo metadata`'s JSON
  through `serde_json`, a dev-dependency outside the shipped closure — a
  test instrument, not a shipped crate.
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
| `STATEMENT_FRAGMENT` | the statement entry's root (§6.1) |
| `TERM_FRAGMENT` | the term and term-value entries' root (§6.1) |
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
| — | `body-element`: no node; the child is one of `LITERAL`, `CONDITIONAL_LITERAL`, an aggregate, `THEORY_ATOM`, each holding its own negation tokens (§8.1) |
| `LITERAL` | `literal`: negation tokens and one of `KW_TRUE`, `KW_FALSE`, `ATOM`, `COMPARISON` |
| `ATOM` | `atom` |
| `COMPARISON` | `comparison`, the whole chain |
| `CONDITIONAL_LITERAL` | `conditional-literal`, and every `literal ":" [condition]` shape: set-aggregate elements, disjunction elements with a condition |
| `CONDITION` | `condition`; present and empty when the colon is |
| `DISJUNCTION` | `disjunction`; separators as tokens |
| `FUNCTION_AGGREGATE` | `function-aggregate` with its guards as `GUARD` children, and in body position its leading negation tokens (§8.1) |
| `SET_AGGREGATE` | `set-aggregate` with its guards, and in body position its leading negation tokens |
| `GUARD` | `lguard` / `rguard` |
| `BODY_AGGREGATE_ELEMENT` | `fn-element` in body position |
| `HEAD_AGGREGATE_ELEMENT` | `fn-element` in head position |
| `THEORY_ATOM` | `theory-atom`, and in body position its leading negation tokens |
| `THEORY_ELEMENTS` | `"{" [ theory-elements ] "}"` |
| `THEORY_ELEMENT` | `theory-element` |
| `THEORY_OPTERM` | `theory-opterm` (flat) |
| `THEORY_GUARD` | `theory-op theory-opterm` after the elements |
| `THEORY_SET` `THEORY_LIST` `THEORY_TUPLE` `THEORY_FUNCTION` | the bracketed and function theory terms; a theory term's constant, variable, or splice is `CONSTANT_TERM`, `VARIABLE_TERM`, `SPLICE_TERM` |
| `BINARY_TERM` | one precedence level's maximal chain of `term BINOP term`, flat: operands interleaved with operator tokens (§6.2) |
| `UNARY_TERM` | a maximal run of `UNOP` and its one operand, flat (§6.2) |
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

## Revisions

Refinements to this document made after its gate, each alongside the
build code that surfaced it — the amend-in-commit pattern the §6.3
query-mark precedent set. The §4.6 and §9.3 refinements are honesty-only,
each correcting a claim to match what the tier does and vetted by its
task's reading; the §10.2 amendment, approved by the principal during
Task 15, corrects the document and the code together.

- **§10.2** (2026-08-19): the mode of an adjacency, restated as the
  parser's *standpoint* — the region in force as it begins reading a
  token, not where the token lands. The prior wording assigned the first
  token after a guard to `Normal`, its commit, contradicting the section's
  own opening ("the mode in force at its start") and leaving the oracle
  inexact: at the greedy guard-end the parser peeks that token under
  theory, where a statement-terminating `.` fuses with a following `&`
  into one `THEORY_OP`, so a formatter reading the committed `Normal`
  would wrongly abut them. `lex_mode_of` now reconstructs the standpoint —
  the first token after a theory atom is `Theory` — and the mode law is
  stated over mode-sensitive tokens (whitespace and comments lex
  mode-independently, so their mode is a fact about nothing) against the
  region the parser stood in. This is the sole behavior change among the
  revisions; it restores §10.1's exactness with no lemma caveat.
- **§4.6** (2026-08-19): the ASP-Core-2 string cost line, corrected from
  a mistaken "quadratic" reading. It is O(token) except an ASP-Core-2
  string's maximal-munch fallback — a final `\"` that closes the string
  only because no later quote exists — which may scan forward to the
  next quote or end of input; strict O(token) is impossible under §6.2's
  maximal munch, and tiling stays O(text).
- **§9.3** (2026-08-19): `line_breaks_between`, described as counting the
  line breaks "in the trivia between" two elements, refined to "in the
  text between" — the reading its positional implementation gives, where
  a significant token between a non-adjacent pair counts, as `same_line`
  reads.
