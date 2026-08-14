# themelios — grammar of record

2026-08-14. Normative, pre-implementation; the document spec §6.1 commits
to, joining the specification per spec §14 and versioned with the code.
Cited throughout as *spec §n*; a bare *§n* cites this document's own
sections. It is written to stand alone: a reader holding this repository
and the pinned references (§3) can check every claim, and no claim
depends on any other project.

---

## 1. What this document is

This is the grammar of record for the concrete syntax themelios parses:
the language shared by clingo and clingcon and, as a declared dialect
of the same grammar, the ASP-Core-2 standard language — stated once.
One grammar carries both: the **clingo dialect** (the default) and the
**ASP-Core-2 dialect** are parameterizations of the same productions,
differing only in the enumerated delta set of §6 (spec §2 item 3). The
syntax tier's lexer and parser are implemented against this document;
the macros splice into this grammar and no other (spec §8, law 1);
every consumer that claims "parses the shared clingo/clingcon syntax"
means *this* statement of it.

**Scope, exactly.** This document defines the shared clingo/clingcon
language, its ASP-Core-2 dialect (§6), and the macro dialect (§9) —
nothing else. Three boundaries carry the definition:

- **It records; it does not invent.** Every file-syntax construct here
  is the pinned references' language (§3). Where this document and a
  dialect's authority disagree, the authority governs and the
  disagreement is a defect here (§3); where an authority underdetermines
  a question this document must answer, the stated resolution is marked
  as this document's own interpretation. The one deliberate extension —
  the macro dialect — exists only inside Rust macro invocations and is
  marked as such everywhere it appears (§9); the file language admits
  none of it.
- **Admission and semantics live above.** Theory atoms parse
  grammar-generically here; whether `&sum { … }` matches a `#theory`
  definition is a concern of tiers above (spec §6.1). Likewise safety,
  arity discipline, and what any construct *means*: this document says
  what the language *is*, not what programs are admissible or what
  they denote.
- **A language beyond this grammar is a new language, not an extension
  of it.** A frontend wanting syntax neither dialect states — new
  statement forms, new notation — is a different language whose parser
  *composes* the syntax tier's public machinery rather than forking it
  or amending this grammar (§8). This grammar has one definition; its
  two dialects are declared per input, never varied per consumer, and
  never grow beyond §6's enumerated deltas.

**Notation.** The lexical grammar (§4) is stated in character-class
notation: `[a-z]` ranges, `*`/`+`/`?` repetition, `|` alternation,
`( )` grouping, `"…"` literal characters. The syntactic grammar (§5)
is EBNF over the token roster: `UPPERCASE` names are tokens from §4,
`lowercase` names are nonterminals, `::=` defines, `|` alternates,
`[ x ]` is optional, `{ x }` repeats zero or more times, `( )` groups,
and a quoted `"…"` names a token by its spelling where that reads
better than its name. Where a rule resists symbolic statement —
modes, regions, nesting, tie-breaks — a canonical block states it as
a **worded rule**, a name introduced by `:` rather than `=`, and the
words are exactly as normative as the symbols. In the macro dialect's
blocks (§9), `RUST-`-prefixed uppercase names denote Rust's token
model rather than §4's roster. Productions are written for precision,
not for implementation shape: the parser's organization is the syntax
tier's design, held to *this* language.

The fenced grammar blocks of §4, §5, §6, and §9 are the canonical
statement: they carry no commentary or citations, and concatenated in
order they are the complete grammar — §4–§5 the clingo dialect, §6 the
ASP-Core-2 delta set, §9 the macro dialect — extractable as-is into
reference documentation. Everything argued, pinned, or cautioned lives
in the prose around them.

**Audience.** The implementer and reviewer of `themelios-syntax`, for
whom this is the contract; the clingo-world practitioner (spec §1.3)
checking what "shared syntax" claims; and the tool builder deciding
whether a construct is in the language without reading C++.

## 2. What this document is for

The postcondition, stated so a review can check drift against it:

> docs/grammar.md states the concrete syntax shared by clingo and
> clingcon, and the ASP-Core-2 dialect of the same grammar, once —
> precisely enough that the syntax tier's lexer and parser can be
> implemented and reviewed against this document without consulting the
> references' sources; membership of any input in the language, under
> either declared dialect — modes, precedences, and tie-breaks included
> — is decidable from this document alone; every divergence between
> this statement and a dialect's authority is a defect here, settled
> and recorded per §3's discipline; and the macro dialect is specified
> fully enough that spec §8's third law is dischargeable against it.

This document has failed — independent of any local defect — when any
of the following holds:

- An engine-behavior claim appears without the §3 version pin
  (spec §5.2: version-scoped, never eternal).
- Either dialect's authority accepts a construct this grammar rejects,
  or rejects one this grammar accepts, and no divergence entry records
  it (§11).
- The two dialects diverge anywhere beyond §6's enumerated delta set
  (spec §2 item 3).
- A stated construct is unreachable by the corpus and corner set (§11)
  — the reachability-evidence obligation of spec §6.1.
- A macro-dialect form is admitted in file syntax, or a file-syntax
  form is expressible only through the dialect (§9).
- Admission or semantics is stated normatively here — the §1 boundary
  breached.
- Any production of this language is stated authoritatively anywhere
  else in the repository — the one-grammar rule (spec §2 item 3) at
  document scale.
- A claim requires consulting a project outside the pinned roster (§3)
  to check — the self-containment of the preamble broken.
- A self-recursive production is missing from the recursion-discipline
  map (§10).

## 3. The reference roster and the authority

The roster spec §6.1 names, pinned. Every engine-behavior claim in this
document is scoped to these pins (spec §5.2); prose cites the pinned
sources as `file:line`, and the fenced grammar blocks carry no
citations (§1).

| reference | pin | role |
|---|---|---|
| clingo | tag `v5.8.2` = `a99ffb2a58293c68b28fcc283a1d1c9ccad900fe`, released 2026-08-14 | reference implementation; **the authority** |
| clingcon | tag `v5.2.1` = `8c476557facf9fc996ec67053a01b6273fd9baba` | reference implementation for the shared-syntax claim (§7) |
| tree-sitter-clingo | `58e062c1c6c2ac0bad54fee054573c5a9e6dd759` | secondary cross-check |
| ASP-Core-2 | the TPLP 2020 restatement, with the 2.03c working-group document recorded where the editions differ — both pinned by checksum below | **the ASP-Core-2 dialect's authority** |
| the corpus | spec §10.3, vendored with provenance at the syntax tier | reachability evidence |

**The authority, pinned.** Spec §6.1's rule — *where references
disagree, clingo's observed behavior is the authority* — binds at
clingo v5.8.2. That pin is a released, reproducible artifact: the
freshest state of the 5.x language at this document's date, and the
line clingcon v5.2.1 targets, so the *shared* language this document
states is the 5.x language. The concrete syntax at the pin is carried
by four sources, cited throughout:
`libgringo/src/input/nongroundlexer.xch` and
`libgringo/src/input/nongroundgrammar.yy` (the program language),
`libgringo/src/input/groundtermlexer.xh` and
`libgringo/src/input/groundtermgrammar.yy` (the term-value
sublanguage, §5.10). The accepted language has been stable at the
grammar level since before v5.7.1: the only grammar-file change on the
5.7→5.8 line removed declared-but-unused parser terminals, and the
v5.8.0→v5.8.2 lexer changes are generator mechanics.

**The secondary cross-check's obligation.** The pinned
tree-sitter-clingo is consumed out of band: at the syntax tier's
landing, and at every pin move thereafter, the corpus is parsed under
both this grammar's implementation and the pinned tree-sitter grammar,
and every disagreement is read against the authority — agreement is
corroboration, disagreement lands in §11 like any divergence. Its
discharge lives with the syntax tier's instruments (spec §10.1).

**The ASP-Core-2 dialect's authority is a document.** For that dialect
the authority is the standard itself, as the ASP Standardization
Working Group restated it in *ASP-Core-2 Input Language Format*, TPLP
2020 — a text, so divergences settle by reading rather than by
differential run. The earlier working-group edition, version 2.03c
(self-dated 2015-11-03), is recorded wherever the editions differ (§6):
the editions genuinely diverge in grammar, and this document names
which one governs each point. The pinned artifacts:

- TPLP 2020 restatement — `https://arxiv.org/pdf/1911.04326`, sha256
  `fb8248742fc7977d6f128f15b4882c73bd779ec97b9a55ae2df4b3ea76cd6348`.
- Working-group 2.03c —
  `https://www.mat.unical.it/aspcomp2013/files/ASP-CORE-2.03c.pdf`,
  sha256
  `87bfe7126eec398bc4eda593cfd27a54f822766b3aa070d523d0596e3929eae9`.

The standard defines token recognition and productions; it is silent on
some questions a parser must answer — string value semantics above all.
Where it underdetermines, §6 states this document's resolution and
marks it as an interpretation (§1).

**What the dialect is, and is not.** The ASP-Core-2 dialect exists so
that a conformant ASP-Core-2 program is accepted and means what the
standard says (spec §4). It is additive and replacing, never
restrictive: the delta set (§6) admits the standard's query statement
and gives the standard's reading to the two lexical regions where the
same text means different things in the two languages — strings and
block comments — and clingo's extensions remain available under it,
because a conformant program never contains them and rejecting them
serves no stated obligation. *Strict conformance validation* — flagging
everything beyond the standard — is admission, a profile over the
parsed tree for tiers above, exactly as `#theory` matching is (§1).

**Evidence classes, stated honestly.** At authoring, every claim here
is read from the pinned sources and cited. From the syntax tier's
first weeks, the differential harness (spec §10.1) holds the
observed-behavior claims against the pinned binary; §11's corner set
is that harness's seed corpus, and any case where the sources read one
way and the binary behaves another is a divergence entry the moment it
is found.

**The upgrade protocol.** An engine upgrade — any move of the clingo or
clingcon pin — re-runs the differential and the spike suite
(spec §5.2, §10.1); each recorded divergence is re-established or
retired; and the pin moves only together with a revision of this
document. The development line that will someday be clingo 6.0 is
real, unreleased, and moving — it is recorded as intelligence, dated
and non-normative, in §12, and it changes nothing above until it
releases and the protocol runs.

## 4. Lexical grammar

The lexer is total (spec §6.2): every byte sequence lexes to a token
stream, with input matching no rule in this section becoming error
tokens, and nothing dropped. This section states the token language of
the clingo dialect — the default. The ASP-Core-2 dialect's lexical
deltas are exactly two, both stated in §6: the string rule and comment
nesting; every other token below is shared by both dialects.

**The matching discipline, stated once and normative.**

```
MATCHING : tokens are recognized by maximal munch — at each point the
           longest match wins; among rules matching the same length,
           a quoted spelling beats a character-class rule.
```

Every tie this document relies on is resolved by that rule: `not` is
the keyword, never an IDENTIFIER; a lone `;` inside a theory
expression is structural, never a one-character THEORY-OP; `***` is
`**` `*`.

### 4.1 Whitespace and comments

```
WHITESPACE      = [ \t\r\n]+

LINE-COMMENT    = "%" not beginning "%*", through end of line
SHEBANG-COMMENT = "#!", through end of line

BLOCK-COMMENT   : "%*" opens, "*%" closes, and they nest by depth.
                  Scanning inside a block comment recognizes, in
                  order: "%*" (one level deeper), "*%" (one level
                  shallower), then any other "%" — which silences the
                  rest of its line, openers and closers included. End
                  of input inside a block comment is a lexical error.
```

Whitespace and comments may appear between any two tokens and bind to
none (the lossless tree carries them as trivia; attachment is the
tier's owned policy, spec §6.4). Both comment forms end at end of input
without error; only an unterminated block comment is a lexical error.
Block-comment nesting and the line-silencing rule are behavior read
from the authority (`nongroundlexer.xch:149,178–217`): `%* a % *%` on
one line does *not* close at that `*%` — the `%` before it silenced the
line. The shebang form exists so executable program files lex cleanly;
it is a comment anywhere, not only on the first line.

### 4.2 Names

```
IDENTIFIER = [_']* [a-z] ['A-Za-z0-9_]*
VARIABLE   = [_']* [A-Z] ['A-Za-z0-9_]*
ANONYMOUS  = "_"
```

Leading underscores and primes are legal, and primes may appear
anywhere after the first letter: `_p`, `'a`, `a'`, `p''` are
identifiers; `_X`, `X'` are variables (`nongroundlexer.xch:48–50`).
`_` alone is the anonymous variable; because no rule matches a longer
run of bare underscores, `__` is two ANONYMOUS tokens and `_1` is
ANONYMOUS followed by NUMBER — neither is a name. Every ASP-Core-2
name (`[a-z][A-Za-z0-9_]*`, `[A-Z][A-Za-z0-9_]*`) is inside these
classes, so §6 carries no name delta. `not` is never a name (§4.5).

### 4.3 Numerals

```
DEC    = "0" | [1-9] [0-9]*
HEX    = "0x" [0-9A-Fa-f]+
OCT    = "0o" [1-7]+
BIN    = "0b" [0-1]+
NUMBER = DEC | HEX | OCT | BIN
```

Decimal numerals have no leading zeros: `007` is three NUMBER tokens,
which no production accepts adjacent, so it is a syntax error — not the
number seven. The prefixes are lowercase: `0X1F` is NUMBER `0` followed
by VARIABLE `X1F`. There are no negative numerals — `-1` is unary
minus applied to `1` (§5.1) — and no digit grouping at the authority
pin (§12 records the development line adding it). One oddity is
recorded rather than repaired: the authority's octal digit class is
`[1-7]` — it excludes zero (`nongroundlexer.xch:40`) — so `0o10` lexes
as NUMBER `0o1` followed by NUMBER `0`, a syntax error downstream; §11
carries the differential obligation. The token admits digit strings of
any length; value range is the tiers' concern, not the grammar's, and
the authority's behavior at 32-bit overflow is a §11 test obligation.

### 4.4 Strings

```
STRING = "\"" ( [^\\"\n] | "\\\"" | "\\\\" | "\\n" )* "\""
```

Exactly three escapes — `\"`, `\\`, `\n` — and no raw newline: a string
containing an unescaped line break, or a backslash before any other
character, matches no rule and lexes as error tokens. The token's
value is the text with escapes resolved: `\"` denotes `"`, `\\`
denotes `\`, `\n` denotes the line feed. The ASP-Core-2 dialect
replaces this rule entirely (§6): under the standard, raw newlines are
legal, `\"` is the only escape, and a backslash is otherwise an
ordinary character — the same spelling can be a different string, which
is precisely why the dialect is declared rather than inferred.

### 4.5 Keywords

```
"#const"     "#count"     "#defined"   "#edge"      "#external"
"#false"     "#heuristic" "#include"   "#inf"       "#infimum"
"#max"       "#maximize"  "#maximise"  "#min"       "#minimize"
"#minimise"  "#program"   "#project"   "#script"    "#show"
"#sum"       "#sum+"      "#sup"       "#supremum"  "#theory"
"#true"

"not"
```

`not` is the language's one reserved word: it always lexes as the
keyword, so no atom, function, or constant is named `not`. The words
`default` and `override` are *not* reserved — they are ordinary
identifiers, and the two productions that want them (§5.9) match them
by spelling. `#`-words are recognized whole: `#inf`/`#infimum` and
`#sup`/`#supremum` are synonym pairs, the optimize keywords admit both
the -ize and -ise spellings (`nongroundlexer.xch:83–84`), and **any
other `#`-word is a lexical error** — including a keyword extended by
name characters (`#sums`, `#counting`), which is one long unknown
`#`-word by maximal munch, never a keyword plus residue. `#end` is not
a keyword of the file language; it exists only as the script
terminator (§4.8), and outside a script region it is an unknown
`#`-word like any other.

### 4.6 Operators and punctuation

```
"."   ".."   ","   ";"   ":"   ":-"   ":~"   "|"
"("   ")"   "["   "]"   "{"   "}"
"+"   "-"   "*"   "**"  "/"   "\\"   "^"   "&"   "~"   "?"   "@"
"="   "=="  "!="  "<>"  "<"   "<="   ">"   ">="
```

`=` and `==` are two spellings of the one equality token; `!=` and
`<>` of the one disequality token — the tree preserves which was
written, and the token kind is the same. Maximal munch resolves every
adjacency: `***` is `**` `*`, `...` is `..` `.`, `:-` and `:~` win
over `:`. `!` and `$` appear in no rule alone — outside strings,
comments, script regions, and theory expressions (where `!` is
operator material, §4.7), they are error bytes, as is any other byte
this section gives no rule.

### 4.7 Theory-expression lexing

Inside a theory atom (§5.8), token recognition changes — this is a
language fact, not an implementation choice: the authority lexes these
regions in a distinct mode (`nongroundlexer.xch:148`,
`nongroundgrammar.yy:954–964`), and membership depends on it.

```
THEORY-OP = a maximal nonempty run of characters from
            [/ ! < = > + - * \ ? & @ | : ; ~ ^ .]

Within a theory expression:
  - the punctuation "," "(" ")" "[" "]" "{" "}" is structural, as
    everywhere;
  - a length-one run that is "." ";" or ":" is structural;
  - the exact run ":-" is the rule-neck token;
  - every other maximal run of the operator alphabet is one THEORY-OP;
  - "not" is the theory operator spelled "not";
  - IDENTIFIER, VARIABLE, NUMBER, STRING, "#inf", "#sup" lex as in
    normal mode; every other "#"-word is a lexical error;
  - ANONYMOUS does not lex: a bare "_" is an error byte here;
  - whitespace and comments as everywhere.

The region where these rules hold: from the "{" that opens a theory
atom's elements through its matching "}", and continuing through the
atom's guard when one follows that "}". Each element's condition is
excluded: a structural ":" at the element's own depth — zero unclosed
"(" "[" "{" since the element began — opens the condition, which runs
to the element's end, the first ";" or the closing "}" at that same
depth; a ":" at greater depth opens nothing. The guard extends from
the theory operator after the "}" through the longest token sequence
derivable as theory-opterm; the first token that cannot extend it —
"," ";" "." among them — lexes in normal mode. The atom's name and
its optional parenthesized arguments lex in normal mode.
```

The consequences worth spelling out, each a §11 seed: `..`, `:=`,
`;;`, `::`, and `:~` are single theory operators here — the interval
token and the weak-constraint neck do not exist inside theory
expressions; `:-` is the rule neck only as an exact run, so `:-:` is
one three-character THEORY-OP; a lone `.` still ends the statement,
which is what makes an unclosed theory atom recover at a statement
boundary; and `|`, structural elsewhere, is operator material here.
This munching is the reason a formatter must never introduce or remove
whitespace between operator characters in these regions — adjacency
*is* token identity, the first pinned case of the fusion oracle
(spec §6.2).

Within `#theory` definitions (§5.9), operator positions lex by these
same rules — `not` included — while everything else in the definition
body lexes in normal mode; the definition's type words (`left`,
`right`, `unary`, `binary`, `head`, `body`, `any`, `directive`) are
ordinary identifiers matched by spelling, usable as names everywhere
else.

One authority behavior is recorded as a divergence rather than adopted
into the region rule: at the pin, *any comment inside a theory
expression resets lexing to normal mode* until the next element
boundary, closing brace, or guard position
(`nongroundlexer.xch:183,212`) — so an operator run after an
in-element comment lexes by §4.6 and generally fails to parse. This
document states the region rule without that quirk — the reading under
which comments are trivia everywhere, which the authority's own
development line adopts (§12) — and §11 carries it as recorded
divergence D1 with its differential obligation.

### 4.8 The script region

```
SCRIPT-BODY : after "#script" "(" IDENTIFIER ")", the raw text through
              the first occurrence of "#end"; nothing within it lexes.
              The region's value is that text with trailing blanks and
              tabs trimmed before "#end". End of input before "#end"
              is a lexical error.
```

Between the closing parenthesis and `#end`, the input is carried
verbatim — comments do not comment, strings do not open, braces do not
nest. The first `#end` always ends the region, so a script whose own
text contains `#end` cannot be written inline; that is the authority's
rule (`nongroundlexer.xch:155–176`), recorded as-is.

### 4.9 The aspif exclusion

An input whose first bytes are `asp` followed by a space and a decimal
numeral is not a program of this language: the authority dispatches
such input to its intermediate-format reader before any of the rules
above apply (`nongroundlexer.xch:69`). The aspif interchange format is
out of this document's scope; the syntax tier's treatment of such
input — a typed refusal naming the format — is the tier design's
concern, and this grammar records only that the text language begins
after that dispatch has not fired.

## 5. Syntactic grammar

The productions of the clingo dialect, in §1's EBNF over §4's tokens.
Together with §4's token rules, the precedence table (§5.1), and three
stated disambiguation rules — the show-signature reading (§5.9), the
disjunction-separator rules (§5.5), and the condition-termination rule
(§5.4) — membership is deterministic: one parse per input, or no
parse. Whitespace and comments may stand between any two tokens and
appear in no production. The whole grammar is derived from the
authority's parser (`nongroundgrammar.yy`, cited by line below);
divergences found by the differential land in §11.

### 5.1 Terms

```
term ::= term BINOP term
       | UNOP term
       | "(" pool ")"
       | IDENTIFIER "(" arguments ")"
       | "@" IDENTIFIER "(" arguments ")"
       | "@" IDENTIFIER
       | "|" abs-arguments "|"
       | IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"
       | VARIABLE | ANONYMOUS

BINOP, loosest to tightest; all left-associative except "**":
    ".."
    "^"
    "?"
    "&"
    "+"  "-"
    "*"  "/"  "\"
    "**"            (right-associative)
UNOP ::= "-" | "~"      (bind tighter than every BINOP)

pool          ::= tuple { ";" tuple }
tuple         ::= [ terms ] [ "," ]
terms         ::= term { "," term }
arguments     ::= [ terms ] { ";" [ terms ] }
abs-arguments ::= term { ";" term }
```

**The precedence table is the authority's, stated once.** It is read
from the parser's declarations (`nongroundgrammar.yy:298–306`):
interval loosest, then the bitwise family `^` `?` `&`, then additive,
multiplicative, exponentiation right-associative — and **unary
operators bind tighter than `**`**, so `-2**2` is `(-2)**2` and
`~2**2` is `(~2)**2` at the authority pin. That last binding is not
universal among languages, it flips on the development line (§12), and
§11 carries its differential obligation.

**Parentheses are tuples and pools, not mere grouping.** `(a)` is the
term `a` parenthesized; `(a,)` is a one-element tuple, distinct from
it; `()` is the empty tuple; `(,)` is grammatical
(`nongroundgrammar.yy:431`) and §11 records what the authority makes
of it. Semicolons inside parentheses pool: `(a;b)` is a pool of two
terms, `(a,b;c,d)` a pool of two tuples. Argument lists pool the same
way — `f(a,b; c,d)` — and each pooled alternative may be *empty*
(`nongroundgrammar.yy:423–426,442–445`): `f()` has one empty argument
tuple, `f(;)` two, `f(a;)` a nonempty and an empty one — all
grammatical, with the authority's treatment a §11 obligation. A
trailing comma inside an argument list is not grammatical: `f(a,)` is
a syntax error.

**The remaining forms.** `@name(…)` and bare `@name` are external
function calls — the `@` and the name are separate tokens, so
whitespace between them is legal. The absolute-value bars accept a
semicolon-separated list — `|X;Y|` is one absolute-value term over a
pooled argument (`nongroundgrammar.yy:398,411–414`) — and because `|`
also separates head disjunctions, §5.5 states the one place the two
uses collide. `#inf` and `#sup` are terms: the least and greatest
elements of the term order. A `-` before a term is the unary
operator; the classically negated *atom* is §5.2's separate form, and
which one `-p` is depends on where it stands — in term position it is
arithmetic negation, in literal position classical negation.

### 5.2 Literals

```
literal    ::= [ "not" [ "not" ] ]
               ( "#true" | "#false" | atom | comparison )
atom       ::= [ "-" ] IDENTIFIER [ "(" arguments ")" ]
comparison ::= term relation term { relation term }
relation   ::= "<" | "<=" | ">" | ">=" | "=" | "!="
```

Default negation is `not` and doubles: `not not p` is a literal
(`nongroundgrammar.yy:479–492`). Comparisons chain: `1 < X < 5` is a
single literal carrying a guard sequence, each step its own relation
(`nongroundgrammar.yy:474–477`) — not a conjunction, and not
ASP-Core-2 (§6 carries no chains; conformant programs have none). A
bare term with no relation is a literal only when it has atom shape —
possibly signed, a name with optional arguments — so `1.` and `X.`
are syntax errors while `p.` and `-p(X).` are facts. In `-p < 3` the
`-p` is the arithmetic term, not the negated atom: the relation makes
the whole a comparison, and comparisons range over terms.

### 5.3 Aggregates

```
aggregate      ::= [ lguard ] aggregate-body [ rguard ]
lguard         ::= term [ relation ]
rguard         ::= relation term | term

aggregate-body ::= function-aggregate | set-aggregate

function-aggregate ::= function "{" [ fn-elements ] "}"
function       ::= "#count" | "#sum" | "#sum+" | "#min" | "#max"

set-aggregate  ::= "{" [ set-elements ] "}"
set-elements   ::= set-element { ";" set-element }
set-element    ::= literal [ ":" [ condition ] ]

condition      ::= literal { "," literal }

fn-elements    ::= fn-element { ";" fn-element }

in body position:
fn-element     ::= terms [ ":" [ condition ] ]
                 | ":" [ condition ]

in head position:
fn-element     ::= [ terms ] ":" literal [ ":" [ condition ] ]
```

One aggregate syntax serves both positions; only the function-form
*element* differs. In a body, an element is a term tuple with an
optional condition — `#sum { W,T : task(T), weight(T,W) }` — and a
bare-colon element with no tuple is legal. In a head, the colon and a
*literal* after it are required — `#min { X : p(X) : q(X) }` reads
tuple, head literal, condition — because a head aggregate derives
atoms, and the derived literal is not optional
(`nongroundgrammar.yy:523–526,568–571`). Empty braces are legal
everywhere: `#sum {}` and `{}` in both positions. Conditions may be
empty after their colon — `#sum { a : }` is grammatical — and §11
seeds the corpus with these emptiness corners.

The set form — bare braces — is the choice construct in a head
(`1 { p(X) : q(X) } 1`) and a cardinality-style aggregate in a body;
the *syntax* is one form with guards, stated here once. Guards on
either side; a guard term without a relation means `<=` on its side
(`nongroundgrammar.yy:553–557`): `1 {…} 2` bounds below and above,
`{…} 3` above only, `X = #count {…}` takes an explicit relation on
the left. In body position an aggregate may be negated — `not`, `not
not` — as part of the body element (§5.6); head aggregates carry no
sign.

### 5.4 Conditional literals

```
conditional-literal ::= literal ":" [ condition ]

termination : after the ":", a "," extends the condition; a
              conditional literal ends only at ";" or at the
              statement's ".".
```

**The condition-termination rule, canonical above.** After the colon,
commas extend the condition: in
`:- p : q, r, s.` all of `q, r, s` condition `p`. A conditional
literal in a body is therefore ended only by `;` or by the statement's
dot (`nongroundgrammar.yy:639,648`) — to continue the body after one,
write `;`. The condition may be empty — `:- p : .` is grammatical —
and a conditional literal is a body element, not a term: it cannot be
nested inside anything.

### 5.5 Heads

```
head ::= literal
       | disjunction
       | aggregate
       | theory-atom

disjunction ::= disjunction-element separator disjunction-element
                { separator disjunction-element }
              | literal ":" [ condition ]
separator   ::= ";" | "|" | ","
disjunction-element ::= literal [ ":" [ condition ] ]

separation : a "," may follow only an element without a condition;
             after a condition, the next separator is ";" or "|".
```

A disjunction has at least two elements — or exactly one, when that
one carries a condition: the singleton conditioned head
`p(X) : q(X).` is a one-element disjunction, derived by the second
alternative exactly as the authority derives it
(`nongroundgrammar.yy:622–625`); its condition may be empty
(`a : .`); a lone *unconditioned* literal is a `literal` head, never
a disjunction. The separators are `;`, `|`, and `,`, mixable in one
head — with the asymmetry the separation rule states, read from the
authority's own machinery (`nongroundgrammar.yy:604–625`): after a
condition, commas extend the condition (§5.4's rule again), so the
next separator must be `;` or `|`. The comma-separated head —
`a, b.` — parses as a disjunction node; what the engine makes of it
is the tiers' concern, and this document records only the shape. One
stated hole, present at the authority pin and documented in its
grammar's own comments (`nongroundgrammar.yy:609–610`): an
*empty-conditioned* element directly before `|` — `p(X) : | q(X)` —
does not parse. The collision §5.1 points here is the cause: after
the empty condition, a `|` could equally open an absolute-value term
inside a condition literal, and the authority's grammar declines to
decide between the readings — write `;` there. `not p.` and
`#false.` are grammatical heads (a head is a literal, sign included);
head aggregates and theory atoms stand in head position unsigned.

### 5.6 Bodies

```
body-list ::= body-element { element-separator body-element }
element-separator ::= "," | ";"
body-element ::= literal
              | conditional-literal
              | [ "not" [ "not" ] ] aggregate
              | [ "not" [ "not" ] ] theory-atom
```

Between ordinary elements, `,` and `;` are interchangeable
(`nongroundgrammar.yy:630–641`); the difference exists only around
conditional literals, whose conditions absorb commas (§5.4). Negation
applies to aggregates and theory atoms as body elements; a negated
conditional literal is expressed by negating its literal, not the
whole.

### 5.7 Rules, weak constraints, optimization

```
rule ::= head "."
       | head ":-" "."
       | head ":-" body-list "."
       | ":-" body-list "."
       | ":-" "."

weak-constraint ::= ":~" [ body-list ] "."
                    "[" term [ "@" term ] [ "," terms ] "]"

optimize-statement ::= ( "#minimize" | "#maximize" )
                       "{" [ optimize-elements ] "}" "."
optimize-elements ::= optimize-element { ";" optimize-element }
optimize-element  ::= term [ "@" term ] [ "," terms ]
                      [ ":" [ condition ] ]
```

`head :- .` and the empty constraint `:- .` are both grammatical
(`nongroundgrammar.yy:662–668`). The weak constraint's bracket comes
*after* the statement's dot — the first of the four
annotation-after-dot families §5.11 enumerates — and carries weight,
optional `@`-priority, and an optional term tuple. Optimize statements
carry semicolon-separated elements of the same weight-priority-tuple
shape with an optional condition; empty braces are legal; a bare colon
with no condition is legal (`nongroundgrammar.yy:687–691`).

### 5.8 Theory atoms

```
theory-atom ::= "&" IDENTIFIER [ "(" arguments ")" ]
                [ "{" [ theory-elements ] "}"
                  [ theory-op theory-opterm ] ]

theory-elements ::= theory-element { ";" theory-element }
theory-element  ::= theory-opterms [ ":" [ condition ] ]
                  | ":" [ condition ]
theory-opterms  ::= theory-opterm { "," theory-opterm }

theory-opterm ::= [ theory-ops ] theory-term
                  { theory-ops theory-term }
theory-ops    ::= theory-op { theory-op }
theory-op     ::= THEORY-OP | "not"

theory-term ::= "{" [ theory-opterms ] "}"
              | "[" [ theory-opterms ] "]"
              | "(" ")"
              | "(" theory-opterm ")"
              | "(" theory-opterm "," ")"
              | "(" theory-opterm "," theory-opterms ")"
              | IDENTIFIER "(" [ theory-opterms ] ")"
              | IDENTIFIER | NUMBER | STRING
              | "#inf" | "#sup" | VARIABLE
```

Theory atoms parse grammar-generically (spec §6.1): `&name` alone,
`&name(args) { elements } op guard` at the fullest — the name's
arguments are ordinary §5.1 terms lexed normally, the elements and
guard are theory expressions under §4.7's lexing, and each element's
condition returns to ordinary literals. Operator structure is admitted
without precedence: an opterm is terms interleaved with operator runs,
several operators in a row legal (`- - a`, `not - a`), and how a
`#theory` definition's tables regroup that flat sequence is admission,
above this document (§1). Theory tuples distinguish `(a)` from
`(a,)`; theory sets and lists take the same element syntax between
`{}` and `[]`. There is one guard at most, introduced by any theory
operator (`nongroundgrammar.yy:873–877`).

The shared-syntax consequence, stated once: a clingcon program is a
program of this grammar with no additions — §7 demonstrates it against
the pinned clingcon.

### 5.9 Directives

```
show-statement ::= "#show" "."
                 | "#show" signature "."
                 | "#show" term "."
                 | "#show" term ":" body-list "."
signature      ::= [ "-" ] IDENTIFIER "/" NUMBER

signature-reading : "#show" followed by [ "-" ] IDENTIFIER "/" NUMBER
                    and the statement "." — trivia legal throughout —
                    is the signature form; anything else after
                    "#show" is the term form.

project-statement ::= "#project" signature "."
                    | "#project" atom conditional-dot
defined-statement ::= "#defined" signature "."
edge-statement    ::= "#edge" "(" edges ")" conditional-dot
edges             ::= term "," term { ";" term "," term }
heuristic-statement ::= "#heuristic" atom conditional-dot
                        "[" term [ "@" term ] "," term "]"
external-statement  ::= "#external" atom conditional-dot
                        [ "[" term "]" ]
conditional-dot ::= "." | ":" "." | ":" body-list "."

const-statement ::= "#const" IDENTIFIER "=" constant-term "."
                    [ "[" ( "default" | "override" ) "]" ]

constant-term ::= constant-term BINOP-NO-INTERVAL constant-term
                | UNOP constant-term
                | "(" ")" | "(" "," ")"
                | "(" constant-terms [ "," ] ")"
                | IDENTIFIER "(" [ constant-terms ] ")"
                | "@" IDENTIFIER [ "(" [ constant-terms ] ")" ]
                | "|" constant-term "|"
                | IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"
constant-terms ::= constant-term { "," constant-term }
BINOP-NO-INTERVAL ::= "^" | "?" | "&" | "+" | "-"
                    | "*" | "/" | "\" | "**"

script-statement  ::= "#script" "(" IDENTIFIER ")"
                      SCRIPT-BODY "#end" "."
include-statement ::= "#include" ( STRING | "<" IDENTIFIER ">" ) "."
program-statement ::= "#program" IDENTIFIER
                      [ "(" [ id-list ] ")" ] "."
id-list           ::= IDENTIFIER { "," IDENTIFIER }

theory-definition ::= "#theory" IDENTIFIER
                      "{" [ theory-def-items ] "}" "."
theory-def-items  ::= theory-def-item { ";" theory-def-item }
theory-def-item   ::= term-definition | atom-definition
term-definition   ::= IDENTIFIER "{" [ op-definitions ] "}"
op-definitions    ::= op-definition { ";" op-definition }
op-definition     ::= theory-op ":" NUMBER ","
                      ( "unary" | "binary" "," ( "left" | "right" ) )
atom-definition   ::= "&" IDENTIFIER "/" NUMBER ":" IDENTIFIER ","
                      [ "{" [ theory-op { "," theory-op } ] "}"
                        "," IDENTIFIER "," ]
                      ( "head" | "body" | "any" | "directive" )
```

**The show-signature reading, canonical above.** The
authority implements it as bounded lookahead to the dot
(`nongroundlexer.xch:53,81`), so `#show p/2.` is a signature while
`#show p/2 : q.` and `#show (p/2).` are term forms — the trailing
context decides.

**Annotations after the dot.** `#external`, `#const`, and
`#heuristic` join the weak constraint (§5.7) in the four-family
enumeration §5.11 states: `#external p. [t]` with any term,
`#const n = c. [default]` or `[override]` with exactly those two
spellings as identifiers (`nongroundgrammar.yy:760–806`), and
`#heuristic`'s bracket — *mandatory*, where the other three are
optional — per its production above. An `#external` without the
bracket takes the engine's default; the value inside is any term,
with the meaningful vocabulary an admission concern.

**The constant-term subset.** `#const` bodies exclude variables,
anonymous variables, pools, and intervals — the production above is
§5.1 restricted to ground, pool-free, interval-free construction
(`nongroundgrammar.yy:341–366`); `#const x = 1..3.` is a syntax
error.

**The remaining shapes.** `#project` has both the signature form and
the atom form with an optional condition; `#edge` takes
semicolon-separated pairs; `#heuristic`'s bracket is weight, optional
priority, and modifier. `#include`'s angle form is three tokens, so
`#include < lib > .` is grammatical at the pin — the development line
disagrees (§12). `#program` declares a name with an optional
identifier-only parameter list. Inside a `#theory` definition, term
and atom definitions may interleave in any order at the pin
(`nongroundgrammar.yy:936–941`) — the development line orders them
(§12) — and the type words are matched by spelling per §4.7.

### 5.10 The term-value sublanguage

```
value-term ::= value-term BINOP-NO-INTERVAL value-term
             | UNOP value-term
             | "|" value-term "|"
             | "(" ")" | "(" "," ")"
             | "(" value-terms [ "," ] ")"
             | IDENTIFIER "(" [ value-terms ] ")"
             | IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"
value-terms ::= value-term { "," value-term }
```

The language of *term values* — what a string parses to when a caller
asks for a symbol rather than a program. It is §5.1 with everything
non-ground removed: no variables, no anonymous variable, no pools, no
intervals, no `@`-calls, single-term absolute value only. The
authority realizes it as a second parser
(`groundtermgrammar.yy:117–152`); under the one-grammar rule it is a
designated subset with its own entry point, stated here so no second
grammar ever grows around it. Its arithmetic is evaluated at parse
into values, with undefined operations refused — evaluation semantics
belong to the tiers, membership to this block. One inconsistency at
the authority pin is recorded rather than inherited silently: this
parser reads `(,)` as the empty tuple, the program parser marks it as
a trailing-comma form (§11).

### 5.11 Programs

```
program   ::= { statement }
statement ::= rule
            | weak-constraint
            | optimize-statement
            | show-statement | project-statement | defined-statement
            | edge-statement | heuristic-statement | external-statement
            | const-statement | script-statement | include-statement
            | program-statement | theory-definition
```

A program is a sequence of statements, empty included. Every
statement contains a dot, and for exactly four families the dot is
followed by a bracketed annotation — **weak constraints** and
**`#heuristic`** always, **`#external`** and **`#const`** optionally —
so **the dot is not always the statement's last token**: a fact every
tool that scans for statement boundaries must carry, stated here once
(§5.7 and §5.9 cite it) so none rediscovers it, and for `#heuristic`
the dot is *never* the last token. The clingo dialect has no query
statement; §6 adds it for the ASP-Core-2 dialect as the final
statement of a program.

## 6. The ASP-Core-2 dialect

The dialect's delta set, complete: one production and two lexical
replacements. Everything not stated in this section is shared with the
clingo dialect exactly as §4–§5 state it — that completeness is a §2
failure condition, so a fourth delta discovered later is a defect
here, not an amendment of the architecture. The dialect is declared
per input (§1) and is additive and replacing, never restrictive (§3):
clingo's extensions remain available under it; strict conformance
checking is admission, above.

### 6.1 The query statement

```
program (ASP-Core-2 dialect) ::= { statement } [ query ]
query ::= atom "?"

query-reading : the query reading applies exactly when the "?" is the
                program's final token, trivia aside; a "?" anywhere
                else is the bitwise-or operator.
```

The standard's one construct with no clingo counterpart
(2.03c §4; TPLP §6): a program may end — and only end — with a single
query, an atom followed by the query mark, no dot. Variables are legal
in it; the standard defines non-ground query answering by
substitution, and the answer semantics is cautious — which is what
lowers it onto the query surface (spec §9.7). The query-reading rule
makes the disambiguation positional, deterministic, and *additive*:
`p(1)?` ending the program is the query; `p ? q = X.` parses in this
dialect exactly as in clingo's — a comparison-headed rule, since the
`?` is not final; `p ? q.` is the same syntax error in both dialects
(a term where a literal is required); and `x(1?2).` stays a fact
about a term. No clingo-dialect program changes membership under this
dialect — the additive posture holds without exception, and every
conformant program's query, standing at the program's end as the
standard requires, is recognized. The atom here is §5.2's shape — a
superset of the standard's classical literal — per the additive
posture.

### 6.2 The string rule

```
STRING (ASP-Core-2 dialect) = "\"" ( [^"] | "\\\"" )* "\""

value: the pair \" denotes the quote character; every other
character — backslash and raw line break included — denotes itself.
```

Replaces §4.4 entirely for this dialect, per the standard's lexical
table (2.03c §5; TPLP §6). Raw newlines inside strings are legal; the
only escape is `\"`; a backslash anywhere else is an ordinary
character. Three consequences, each a §11 seed because the same
spelling changes meaning across dialects: `"a\nb"` is
a-backslash-n-b here and a-newline-b in the clingo dialect; `"a\b"`
is a string here and a lexical error there; a string spanning lines
is legal here and an error there. The rule is maximal-munch like
every rule in this document, which resolves the standard's own
regex ambiguity the way Flex does: when a later closing quote
exists, `\"` reads as the escape and the longer string wins, so
`"a\" b"` is one string containing a quote; `"a\"` with no later
quote closes at its final character and denotes `a` followed by a
backslash. The value column above is this document's interpretation,
marked as such (§3): the standard defines token recognition and is
silent on denotation.

### 6.3 Block comments

```
BLOCK-COMMENT (ASP-Core-2 dialect) : "%*" opens, the first following
"*%" closes; no nesting, and no line-silencing inside.
```

Replaces §4.1's block-comment rule for this dialect, per the
standard's `MULTI_LINE_COMMENT` pattern (2.03c §5; TPLP §6). The
membership consequence cuts both ways and seeds §11: `%* %* *%` is a
closed comment here and an unterminated one under the clingo dialect;
`%* a % *% b *%` closes at the first `*%` here (the standard has no
line-silencing) and at the second there. Line comments and `#!` are
§4.1's, unchanged — the standard's line-comment pattern agrees, and
`#!` is a clingo-dialect extension available under the additive
posture.

### 6.4 The standard, mapped

For the reader arriving from the standard, where its grammar lands in
this document: its rules, integrity constraints, and disjunctive heads
(`|`-separated) are §5.7 and §5.5 — the standard has only `|`, and
`;`/`,` head separators are clingo extensions; its choice rules are
§5.3's set-aggregates in head position with §5.3's guards; its
aggregate atoms are §5.3's function-form in body position; its weak
constraints and optimize statements are §5.7 with identical bracket
and element shapes, both optimize spellings included; its built-in
atoms are §5.2's comparisons restricted to a single relation — the
standard has no chains; its anonymous variables, classical negation,
and `not` are §4–§5's. Its names and numerals are strict subsets of
§4.2–§4.3, so those carry no delta.

Between the standard's editions, this document follows TPLP 2020 (§3)
and records the divergences: the TPLP edition restricts aggregate
element tuples to *basic terms* — constants, strings, signed numerals,
variables, anonymous variables — where 2.03c admitted full terms; this
dialect admits §5.3's full tuples (the additive posture: conformant
programs of either edition are inside it, and the restriction is
admission for a conformance profile). The TPLP edition also repairs
the safety definition and adds the infinite-collection conventions —
semantics outside this document's scope, noted only so nobody reads
the editions as interchangeable.

## 7. clingcon: the shared syntax, demonstrated

clingcon adds **zero productions and zero tokens**: at the pinned
clingcon (§3), every constraint program is a program of §4–§5, and
everything clingcon-specific is `#theory`-relative admission above
this document — the spec's shared-syntax bet (spec §6.1), checked
against source. The propagator registers this theory definition
(`libclingcon/clingcon/parsing.hh:39–84`, quoted verbatim as
evidence, not as grammar):

```
#theory cp {
    var_term  { };
    sum_term {
    -  : 3, unary;
    ** : 2, binary, right;
    *  : 1, binary, left;
    /  : 1, binary, left;
    \  : 1, binary, left;
    +  : 0, binary, left;
    -  : 0, binary, left
    };
    dom_term {
    -  : 4, unary;
    ** : 3, binary, right;
    *  : 2, binary, left;
    /  : 2, binary, left;
    \  : 2, binary, left;
    +  : 1, binary, left;
    -  : 1, binary, left;
    .. : 0, binary, left
    };
    disjoint_term {
    -  : 4, unary;
    ** : 3, binary, right;
    *  : 2, binary, left;
    /  : 2, binary, left;
    \  : 2, binary, left;
    +  : 1, binary, left;
    -  : 1, binary, left;
    @  : 0, binary, left
    };
    &__diff_h/0 : sum_term, {<=}, sum_term, any;
    &__diff_b/0 : sum_term, {<=}, sum_term, any;
    &__sum_h/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any;
    &__sum_b/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any;
    &__nsum_h/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any;
    &__nsum_b/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any;
    &minimize/0 : sum_term, directive;
    &maximize/0 : sum_term, directive;
    &show/0 : sum_term, directive;
    &distinct/0 : sum_term, head;
    &disjoint/0 : disjoint_term, head;
    &dom/0 : dom_term, {=}, var_term, head
}.
```

The user-facing vocabulary is `&sum`, `&diff`, `&nsum`, `&dom`,
`&distinct`, `&disjoint`, `&show`, `&minimize`, `&maximize` — the
`__`-prefixed head/body pairs above exist because clingcon rewrites
occurrences by position before grounding
(`libclingcon/src/parsing.cc:223–241`), a transformation invisible to
syntax. Note what the definition confirms about §4.7: `..` and `@` are
ordinary theory operators inside `&dom` and `&disjoint` elements —
`&dom { 0..B } = v` contains a THEORY-OP, not the interval token —
and the guards are §5.8's single-guard form. The pinned repository's
worked examples are corpus inputs (§3), and every one parses under
this document.

## 8. Extension regions

The one grammar carries exactly three extension regions — the places
where vocabulary grows without this document changing — and beyond
them, growth means a new language:

1. **Theory atoms** (§5.8). Solve-time vocabularies define themselves
   in `#theory` and occupy the grammar-generic `&`-region; admission
   is above. §7 is the demonstration, and user-defined *directives*
   already exist through it (`&show/0 : …, directive`).
2. **Comments as data.** The tier exposes comments and their
   attachment as API (spec §6.4), and doc comments are first-class
   syntax; tool-owned languages ride on comment text — contract
   extraction is the spec's named consumer class (spec §11). Their
   grammars are their own, over comment text, and outside this
   document by construction.
3. **The macro dialect** (§9). Compile-time interpolation for Rust
   macro bodies — the one deliberate extension this document itself
   defines, marked dialect-only, never file syntax.

A frontend needing what none of the three admit — new statement
forms, new notation — is a new language whose parser composes the
syntax tier's public machinery (§1). What that composition requires
of the tier — public construct-family entry points, a reusable lexer,
an extensible tree — is the syntax tier design's commitment, recorded
there as a named seam; this grammar's part is only the boundary just
stated.

## 9. The macro dialect

The interpolation forms by which macros splice Rust values into
program syntax (spec §6.1, §8 law 3) — the one extension this document
defines rather than records. It exists only inside Rust macro
invocations: nothing in this section is file syntax, and the splice
marker is an error byte in files (§4.6), so the two languages cannot
be confused by construction.

**The ground rule: Rust's lexer is the dialect's lexical layer.** A
macro body arrives as a Rust token stream — Rust has already lexed it,
and its comments are gone. The dialect is therefore defined over
Rust's token model, with this mapping onto §4's roster:

```
In macro bodies:
  - a Rust identifier lexes by the name classes: lowercase-initial is
    IDENTIFIER, uppercase-initial is VARIABLE, "_" alone is ANONYMOUS;
    "not" is the keyword; an identifier no class matches whole
    ("__", "_1") is a dialect error;
  - a Rust integer literal is NUMBER, by value;
  - a Rust string literal is STRING, by value — raw strings included;
  - span adjacency is part of the dialect: "#" forms a keyword exactly
    when it is span-adjacent to the keyword's word — and, for "#sum+",
    to the "+" beyond it; a "#" separated from its word is a dialect
    error;
  - Rust punctuation maps one-to-one onto the operator roster; a
    multi-character operator exists where its characters are adjacent
    and joined, and theory-operator runs form the same way inside
    theory expressions;
  - comments do not exist in the dialect;
  - "$" begins a splice by token order alone: it takes the next
    identifier or parenthesized group, spacing irrelevant, and exists
    only here;
  - every Rust token this mapping does not name is a dialect error at
    the macro site: float, char, and byte literals, suffixed
    numerals, lifetimes, and raw identifiers — "r#not" is an error,
    never a way to spell the reserved name.

splice ::= "$" RUST-IDENTIFIER
         | "$" "(" RUST-EXPRESSION ")"

term (macro dialect)        ::= any term        | splice
theory-term (macro dialect) ::= any theory-term | splice
```

Two of the mapping's rules deserve their mechanism named. Rust's token
model records adjacency only between punctuation tokens, so the
`#`-keyword rule cannot ride on it: *span adjacency* — the tokens'
source positions abutting — is declared part of the dialect's
definition, which is what decides `# const` (an error) and assembles
`#sum+` from its three Rust tokens. The splice marker needs no such
rule: `$` binds its operand by token order, so `$x`, `$ x`, and
`$ (…)` all splice.

**Splices.** `$name` splices the value of a Rust binding; `$( … )`
splices any Rust expression. A splice stands where a term *or a
theory term* may stand — both positions are the v1 floor, so
`&sum { $x } <= $bound` is expressible, and the marker's absence from
the theory-operator alphabet (§4.7) is what keeps it unambiguous
there. Further splice sites (names, tuples, statements) are future
vocabulary, each admitted on argument as the macro tiers accrete
(spec §8). The spliced value
crosses through the conversion traits, and what fails to convert
refuses at the constructor doors the expansion calls (spec §7.3; §8
law 2): a macro expands to the public constructors, so a splice is
never a second door into program construction.

**What the mapping changes, stated honestly.** By-value literals mean
macro bodies admit spellings files do not and vice versa: a Rust
string's escapes (`\t`, `\u{…}`) produce string *values* §4.4 cannot
spell, and a Rust numeral may be `0o17` or `1_000` — the value
crosses, the spelling does not. Primed names (`a'`) are inexpressible
in macros — Rust identifiers carry no primes — and remain expressible
through the spelled-out constructors, which is the direction §8 law 2
guarantees; the converse is not promised. Rendering a program whose
string values have no §4.4 spelling is the rendering tier's concern
(spec §7.6), noted here so the gap is owned, not discovered.

## 10. The recursion discipline, mapped

The rule, binding on the syntax tier's implementation:

> Call-stack recursion where the grammar bounds depth; explicit
> stacks where the grammar is self-recursive.

This section maps it onto §4–§9 so no production's obligation is
implicit — a missing self-recursive production here is a §2 failure.

**Self-recursive — explicit stacks.** Exactly the four term families
nest without bound, through parentheses, argument lists,
absolute-value bars, operator chains, and (for theory terms) sets,
lists, and tuples:

- `term` (§5.1), and with it the macro dialect's term (§9);
- `constant-term` (§5.9);
- `value-term` (§5.10);
- `theory-term` / `theory-opterm` (§5.8).

Every walk that parses, traverses, or renders these families runs on
an explicit stack; input depth never becomes call depth (spec §5.2's
depth constraint, §7.2's discipline, held per walk by the depth gate,
spec §10.1).

**Grammar-bounded — the call stack is licensed.** Everything above
the term families composes by *iteration* (statement sequences, body
lists, element lists, condition lists, guard chains, disjunction
elements) plus a fixed number of layers: a path from `program` down
to a term family crosses a constant number of productions — for
instance `program → rule → head → disjunction → disjunction-element →
literal → comparison → term` — fixed by this grammar, independent of
input.
Aggregates do not nest (an aggregate is a body or head element, never
a literal, and conditions hold literals only); conditional literals
do not nest; statements are flat. The lexer recurses nowhere: §4 is
regular plus one counter (block-comment depth is a counter, not a
stack).

The boundary is not hypothetical: the authority's own development
line, reimplementing its parser by hand, runs term parsing on an
explicit production stack and everything above it as plain functions
(§12) — independent convergence on the same split at the same
boundary.

## 11. Corners, seeds, and recorded divergences

This section is three things: the differential harness's seed corpus
(spec §10.1) — each entry an input shape with this document's stated
expectation, to be held against the pinned binary; the honest register
of what source reading could not settle; and the landing place the
authority rule names (§3) for divergences, where each future finding
arrives as a numbered entry with its resolution — a repair here, or a
recorded exception with its argument.

**Recorded divergences at authoring.**

- **D1 — comments inside theory expressions.** At the pin, any
  comment inside a theory expression resets lexing to normal mode
  until the next element boundary, closing brace, or guard position
  (§4.7; `nongroundlexer.xch:183,212`), so following operator runs
  mis-lex. This document deliberately states the region rule without
  the quirk: comments are trivia everywhere, the reading the
  authority's development line adopts. Obligation: pin the
  authority's exact behavior differentially; keep such inputs out of
  the shared corpus; revisit when the pin moves.

**Lexical seeds.**

- `0o10` — expected: NUMBER `0o1` then NUMBER `0`, a syntax error
  (§4.3's recorded octal oddity); confirm against the binary.
- `p(4294967296)` — numeral overflow behavior is unpinned (§4.3);
  observe and record.
- `0X1F` — NUMBER `0` then VARIABLE `X1F`; syntax error downstream.
- `__` and `_1` — two tokens each, never names (§4.2).
- `%* a % *%` followed, on the next line, by `b *%` — one comment
  closing at the second `*%`: the first line's closer is silenced, the
  next line's counts (§4.1). All on one line, `%* a % *% b *%` is an
  unterminated comment — both closers silenced — and a lexical error.
  Under the ASP-Core-2 dialect either input closes at the first `*%`
  (§6.3).
- `%* %* *%` — unterminated under the clingo dialect; closed under
  ASP-Core-2 (§6.3).
- `"a\nb"`, `"a\b"`, a string spanning lines — the string meaning
  and membership divergences across dialects (§4.4, §6.2).
- `#sums`, `#counting` — single unknown-`#`-word lexical errors
  (§4.5).
- `&a { _ }` — `_` is an error byte inside theory expressions at the
  pin (§4.7); confirm.
- `&a { x :-: y }`, `;;`, `::`, `:=`, `..`, `:~` inside theory
  expressions — single THEORY-OP runs (§4.7).
- `#show $x/1.` — residue note: a dead `$` in the authority's
  lookahead regex (`nongroundlexer.xch:53`) makes this lex as a
  signature head before erroring; both readings are errors, so no
  membership consequence — recorded to explain the source.

**Syntactic seeds.**

- `-2**2`, `~2**2` — unary binds tighter at the pin: `(-2)**2` = 4
  (§5.1); the development line flips it (§12); differential-confirm.
- `(,)` — grammatical in the program grammar as a trailing-comma
  form, the empty tuple in the value grammar (§5.1, §5.10); observe
  what the authority builds.
- `f(;)`, `f(a;)` — empty pooled argument alternatives are
  grammatical (§5.1); observe.
- `f(a,)` — a syntax error; no trailing comma in argument lists
  (§5.1).
- `a, b.` — parses as a two-element disjunction head (§5.5); the
  engine's reading is the tiers' concern; confirm the parse shape.
- `p(X) : | q(X)` — the stated hole (§5.5); error at the pin.
- `:- p : .` — empty condition after a conditional literal, legal
  (§5.4).
- `a : b.`, `p(X) : q(X) :- r.`, `a : .` — singleton conditioned
  heads, legal as one-element disjunctions (§5.5).
- `#heuristic a. [1,sign]` — the mandatory bracket after the dot; the
  fourth annotation family (§5.9, §5.11).
- `&a { t : p((x;y)), q ; u }` — the `;` inside the condition's pool
  stays in the condition; only the depth-zero `;` ends the element
  (§4.7).
- `:- &sum { x } >= 5, not p.` — the guard ends before the `,`; `not
  p` lexes in normal mode (§4.7).
- `&a { {x : y} }` — a `:` at depth one opens no condition and no
  production admits it; a syntax error (§4.7, §5.8).
- `#sum { : }`, `#sum { a : }` — empty elements and conditions,
  legal (§5.3).
- `1 < X < 5` — one chained comparison literal (§5.2).
- `|X;Y|` — pooled absolute value (§5.1).
- `#include < lib > .` — legal at the pin, three tokens (§5.9); the
  development line disagrees (§12).
- `#const default = 1.` — `default` as an ordinary identifier
  (§4.5, §5.9).
- `head :- .` and `:- .` — legal (§5.7).
- `#script (lua) … #end` with `#end` inside the code — impossible;
  the first `#end` ends the region (§4.8).
- `&a` alone and `&a {}` — grammatical theory atoms (§5.8).
- `&a { x not y }` — `not` as a theory operator (§5.8).
- `asp 1 0 0` as a program's first bytes — the aspif dispatch fires;
  not this language (§4.9).

**Dialect seeds.**

- `p(1)?` as the program's final token — the query, ASP-Core-2
  dialect only (§6.1); a syntax error under the clingo dialect.
- `p ? q.` — the same syntax error in both dialects: the `?` is not
  final, so the term reading holds and no literal results (§6.1).
- `p ? q = X.` and `p(1)?2 > 3.` — comparison-headed rules,
  grammatical and identical under both dialects; the query-reading
  rule does not fire because the `?` is not the program's final token
  (§6.1).
- `x(1?2).` — a fact in both dialects; `?` stays bitwise-or inside
  terms (§6.1).
- `"a\" b"` and `"a\"` — the standard's maximal-munch string
  readings (§6.2).

**Macro-dialect seeds** (held by the macro tier's tests rather than
the differential; listed here so the seed corpus is one list).

- `# const` — a dialect error: keyword formation requires span
  adjacency (§9).
- `#sum+` inside a macro aggregate — one keyword assembled from three
  span-adjacent Rust tokens (§9).
- `$x`, `$ x`, `$( base + margin )` — splices; spacing irrelevant
  (§9).
- `&sum { $x } <= $bound` — splices in theory-term position (§9).
- `r#not`, `1.5`, `1u8`, `'a`, `__` — dialect errors at the macro
  site (§9).
- `r"raw"` — STRING, by value (§9).

## 12. The clingo 6.0 watch

*Dated 2026-08-14; non-normative.* Nothing in this section binds the
grammar above; it exists so the next reader starts where this one
stopped, and it is rewritten under §3's upgrade protocol when a 6.x
releases.

At this document's date there is no released clingo 6.x: the 5.8 line
is current (v5.8.2 released this same day), and the 6.0 line is the
`wip-20` branch of the authority's repository — observed at commit
`e13de4d1` (2026-08-10), self-declared version 6.0.0, a ground-up
rework: tree restructured, the bison grammar replaced by a
hand-written parser (`lib/input/src/parse/`), input rewriting split
into named passes. Surface changes observed by source reading at that
commit, none normative here:

- **f-strings**: `f"…{term}…"` with a format-spec mini-language
  (accessors, conversions, alignment, grouping, type) — a new term
  form.
- **Digit grouping** in decimal numerals: `1'000'000`, groups of
  exactly three.
- **String escapes** add `\t`, `\r`, and `\u{…}`.
- **`#parts`** — a new statement with a `[default|override]`
  annotation; `#show name/arity.` gains a `[true|false]` annotation.
- **`*` as a projection argument** in tuples and argument lists —
  in active flux at the observed commit.
- **`#include <lib>`** becomes a single spaceless token — the pin's
  spaced form (§5.9) stops parsing.
- **`#theory` definitions ordered** — term definitions before atom
  definitions, where the pin admits interleaving (§5.9).
- **Precedence flip**: `**` binds tighter than unary minus — `-2**2`
  becomes `-(2**2)`, reversing the pin (§5.1, §11).
- **Comments become data in every mode**, and a comment inside a
  theory expression no longer disturbs lexing — the D1 divergence
  (§11) resolved upstream.
- NUL bytes handled explicitly throughout; `#end` a real token; an
  aspif `symbols` extension.

When a 6.x releases: the differential and spike suites re-run against
it (§3), each §11 entry is re-established or retired, the dialect
question is re-posed (a 6.x language is a *third* surface until ruled
otherwise), and this section's contents move into the grammar or into
§11 as the evidence directs.

## 13. Non-goals

This document defers nothing of its own scope: the grammar is stated
whole, and its two dialects are complete (§2). What it deliberately
does not cover, with where each concern lives:

- **Admission**: `#theory` matching of theory atoms, safety, arity
  discipline, the meaningful `#external` values, numeral value
  ranges, and ASP-Core-2 conformance validation — tiers above
  (spec §6.1; §3).
- **Semantics**: what any construct means, including the
  comma-separated head and the engine's readings — the program and
  solve tiers.
- **The aspif interchange format** (§4.9).
- **Error recovery**: how the parser continues on ill-formed input is
  the syntax tier design's ground (spec §6.5); this document defines
  only membership.
- **Rendering and formatting style** (spec §7.6).
- **The engine's command-line definition mini-language** (`-c
  name=value`) — an API surface of the tiers, not file syntax.
