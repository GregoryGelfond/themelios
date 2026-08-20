# themelios

θεμέλιος — *foundation*.

A solver-agnostic, library-first foundation for Answer Set Programming in
Rust: lossless syntax tooling, the logician's program representation,
and honest solving over pluggable engines, built to mission-critical
standards so that a family of systems — solver frontends, formatters, test
harnesses, explainers, editor tooling, REPLs, deployment services — are
natural extensions or elegant compositions of its parts.

**Status: v1 specification closed; the base and syntax tiers built.** The
specification is at [`docs/specification.md`](docs/specification.md); build
order and assurance are its §10–§11. Stage 1, `themelios-base` — the
source-text model, spans, line indexing, the diagnostics model, and the
views — is built under [`docs/design/base.md`](docs/design/base.md) with
its stage-1 instruments green (property laws, the `Sources` law checker,
the golden corpus, scaling shapes, coverage floor, mutation audit). Stage
2, `themelios-syntax` — a total lexer with its fusion oracle; an
error-resilient parser producing a lossless tree of the one grammar under
a declared dialect; the owned comment-attachment policy; a typed AST; the
tier's typed diagnostics; and token-stream equivalence — is built under
[`docs/design/syntax.md`](docs/design/syntax.md), held to the grammar of
record at [`docs/grammar.md`](docs/grammar.md), with its stage-2
instruments green (the property laws, the vendored corpus, the goldens,
the depth gate, the scaling shapes, the differential against pinned clingo,
the coverage floor, the mutation audit, and the fuzz targets). Neither tier
is yet published. Next is stage 2's first-consumer checkpoint — morphe, the
formatter, built in its own repository against this surface before later
tiers harden on it (spec §11).

## License

MIT. See [LICENSE](LICENSE).
