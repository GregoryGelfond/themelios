# themelios

θεμέλιος — *foundation*.

A solver-agnostic, library-first foundation for Answer Set Programming in
Rust: lossless syntax tooling, the logician's program representation,
and honest solving over pluggable engines, built to mission-critical
standards so that a family of systems — solver frontends, formatters, test
harnesses, explainers, editor tooling, REPLs, deployment services — are
natural extensions or elegant compositions of its parts.

**Status: v1 specification closed; the base, syntax, program, and analysis
tiers built and validated; the syntax tier's first-consumer checkpoint met;
and the program and analysis tiers' first-consumer checkpoint next.** The
specification is at
[`docs/specification.md`](docs/specification.md); build order and assurance
are its §10–§11.

Stage 1, `themelios-base` — the source-text model, spans, line indexing, the
diagnostics model, and the views — is built under
[`docs/design/base.md`](docs/design/base.md) with its stage-1 instruments
green (property laws, the `Sources` law checker, the golden corpus, scaling
shapes, coverage floor, mutation audit). Stage 2, `themelios-syntax` — a
total lexer with its fusion oracle; an error-resilient parser producing a
lossless tree of the one grammar under a declared dialect; the owned
comment-attachment policy; a typed AST; the tier's typed diagnostics; and
token-stream equivalence — is built under
[`docs/design/syntax.md`](docs/design/syntax.md), held to the grammar of
record at [`docs/grammar.md`](docs/grammar.md), with its stage-2 instruments
green (the property laws, the vendored corpus, the goldens, the depth proof,
the scaling shapes, the differential against pinned clingo, the coverage
floor, the mutation audit, and the fuzz targets). The syntax tier's
first-consumer checkpoint (spec §11) is met: morphe, the formatter, is built
against this surface in its own repository, a satellite composing the
lossless tree and the typed AST.

Stage 3 is the program tier and, co-built beside it, the structural-analysis
tier, and both are now built and validated. `themelios-program` — the
logician's owned, total representation of an ASP program: the ground-symbol
and term algebra; the `Program` value, a part-structured set of rules and
directives with provenance as in-node model data; the two construction
doors (spelled-out Rust constructors and the raise from the syntax tier)
under one well-formedness authority; canonical, round-trippable rendering;
pure `Program → Program` transformation; and the pattern language with the
most general unifier — is built under
[`docs/design/program.md`](docs/design/program.md). `themelios-analysis` — a
pure, total reading of a `Program` that reports its structural facts: the
constructs it uses, its predicate dependency graph and strongly-connected
components, its rules' safety and grounding finiteness, and its membership in
the classes of the literature (tight, stratified, head-cycle-free, normal,
Horn, disjunctive, choice), each a typed verdict carrying its witness — is
built under [`docs/design/analysis.md`](docs/design/analysis.md), with the
stage's instruments green (the property laws, the differential against pinned
clingo for both tiers, the goldens, the scaling shapes, the coverage floor,
and the mutation audit). The program and analysis tiers' first-consumer
checkpoint (spec §11) is next: keryx, a protobuf–ASP bridge that builds and
reads programs through the construct, render, raise, and provenance surface,
in its own repository. The solve tier — owned sessions over pluggable
engines, answer sets, and the three-valued query — follows (spec §9, §11). No
crate is yet published to crates.io.

## License

MIT. See [LICENSE](LICENSE).
