# The syntax corpus

The parser's reachability evidence and the differential's inputs
(docs/specification.md §10.3; docs/grammar.md §3, §11): every input is
parsed under its stated dialect with its stated expectation by
`tests/corpus.rs`; the differential (`tests/differential.rs`, feature
`differential`) parses the same inputs through the pinned authority.
Each source below is vendored as inputs only, with its license beside
it; nothing here is edited.

| directory | source | pinned state | license | what |
|---|---|---|---|---|
| `clingo/` | github.com/potassco/clingo | tag `v5.8.2` = `a99ffb2a58293c68b28fcc283a1d1c9ccad900fe` | MIT (`clingo/LICENSE`) | every `.lp` under `examples/` and `app/clingo/tests/` at most 64 KiB, relative paths kept (319 inputs); excluded, being instance data of size and no syntax, are the three `.lp` above 64 KiB the `find` listing named: `examples/gringo/gbie/instances/sat_02.lp`, `sat_03.lp`, `unsat_02.lp` |
| `clingcon/` | github.com/potassco/clingcon | tag `v5.2.1` = `8c476557facf9fc996ec67053a01b6273fd9baba` | MIT (`clingcon/LICENSE`) | `examples/*.lp` |
| `kallos/` | github.com/GregoryGelfond/kallos | `7db302ce902cccd37050151636281fd5588d8448` | MIT (`kallos/LICENSE`); the inputs derive from github.com/potassco/clingofmt at `c52fba46c6f4b6b7d7dce27325fc8502b516498f`, MIT, Copyright (c) 2021 Sven Thiele / Potassco — see `kallos/NOTICE` | `crates/kallos/tests/corpus/clingofmt/*`: seventeen inputs and their notice |
| `kr-domains/` | github.com/GregoryGelfond/kr-domains | `38f0660ded448ed268c5a68759ceb0e2840dd497` | MIT (`kr-domains/LICENSE`) | every `.lp` under `encodings/`, `scenarios/`, `standalone/` |
| `seeds/` | authored here | — | this repository's | docs/grammar.md §11's seeds and docs/design/syntax.md's own, each with its `.expect` sidecar |

## Expectations

An input without a sidecar is a member under the clingo dialect unless
`NON-MEMBERS` names it (path, then the identities expected, one line
per input). An input `X.lp` with a sidecar `X.expect` is read as the
sidecar says: line one the dialect (`clingo` or `asp-core-2`); line two
`member` or `non-member`; the remaining lines, for a non-member, the
diagnostic identities that must each appear, and outside which none
may. `DIFFERENTIAL-SKIP` names, with a reason each, the inputs the
differential does not hand to the authority: those with a comment
inside a theory expression (grammar §11 D1).
