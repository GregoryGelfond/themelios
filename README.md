# themelios

θεμέλιος — *foundation*.

A solver-agnostic, library-first foundation for Answer Set Programming in
Rust: lossless syntax tooling, the logician's program representation,
and honest solving over pluggable engines, built to mission-critical
standards so that a family of systems — solver frontends, formatters, test
harnesses, explainers, editor tooling, REPLs, deployment services — are
natural extensions or elegant compositions of its parts.

**Status: v1 specification closed; stage 1 built.** The specification is
at [`docs/specification.md`](docs/specification.md); build order and
assurance are its §10–§11. Stage 1, `themelios-base` — the source-text
model, spans, line indexing, the diagnostics model, and the views — is
built under [`docs/design/base.md`](docs/design/base.md) with its stage-1
instruments green (property laws, the `Sources` law checker, the golden
corpus, scaling shapes, coverage floor, mutation audit); it is not yet
published. Stage 2, `themelios-syntax`, is next.

## License

MIT. See [LICENSE](LICENSE).
