# Working on themelios

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before making changes. It is the authoritative contributor guide; this file is a concise entry point for coding agents. Apply the same standards to implementation, tests, and documentation.

## Find the relevant boundary

Repository knowledge is in the specification and the designs of record, not a directory tour:

- What themelios is, its tiers and crates, how it is assured, and its trust architecture: [`docs/specification.md`](docs/specification.md) (§1–§2, §10, §12).
- The language: the grammar of record, [`docs/grammar.md`](docs/grammar.md) — the shared clingo/clingcon syntax and its ASP-Core-2 dialect.
- Each tier's surface, laws, and costs: its design of record under [`docs/design/`](docs/design/) — `base`, `syntax`, `program`, `analysis`, `macros`, `solve`, and `query`.
- The security posture: specification §12.3–§12.4 and [`SECURITY.md`](SECURITY.md).
- Worked usage: the examples in [`README.md`](README.md) and the syntax tier's example witnesses in [`crates/themelios-syntax/examples/`](crates/themelios-syntax/examples/).

## Preserve the foundation's contracts

- **Each tier depends only on the tiers beneath it** — base, syntax, program and analysis, then solve and query, with the macros over syntax and program — and no engine type appears in a tier's public surface.
- **The `Program` is the logician's abstract object**: a set of rules carrying provenance in its nodes. The raise from the syntax tree is the parsing relation onto it, and rendering is canonical: render, parse, and raise is the identity up to provenance.
- **The solve and query tiers are engine-free.** They forbid `unsafe` code and link no foreign library, and `tests/trust.rs` holds their closure. An engine enters only as an adapter that implements the `Backend` contract and passes `themelios_solve::conformance::run`. A capability it lacks is a typed refusal, never a silent degrade. Never render a program to text and parse it back across the engine seam.
- **Refuse rather than guess.** An analysis verdict is `Holds` only when proven, otherwise `Unknown` with the component that blocked the proof; ground arithmetic refuses rather than wraps; a truncated search is `Inconclusive`, never "no answer set". A false `Holds` is the paramount analysis defect.
- **Input depth and size are threats.** Every walk over untrusted input is iterative or grammar-bounded, and the parser bounds nesting. A load-bearing algorithm over unbounded input uses the best-known practical algorithm from the start and carries a scaling tripwire and a bench for its own adversarial shape.
- **The test authorities stay out of band.** The differentials run clingo 5.8.2 and the pinned tree-sitter-clingo grammar through pixi, and neither enters a tier's dependency closure. Binding clingo is the adapter crates' work, feature-gated and never in a default build (specification §12.5).
- **Clients consume themelios at arm's length**, pinned by git revision. A change to a public surface reaches every client, and a gap a client surfaces is closed here, in the tier that owns it.

## Make the correctness argument readable

- Library throughout: there is no binary, and each tier is a crate a client composes.
- Every foreign input returns a typed diagnostic or fault, never a panic or a bare string. One concept, one name: each design of record keeps its tier's vocabulary.
- Resolve every rustfmt, pedantic Clippy, and rustdoc diagnostic in the code. The workspace denies `unsafe_code`, `missing_docs`, `unused`, `dead_code`, pedantic Clippy, and `#[allow]` itself. Set a lint aside only with a narrow `#[expect(…, reason = "…")]` where its concern does not apply, and never `unused` or `dead_code` in library code.
- One test name states one proposition. The code and its design of record say the same thing, so a change that departs from a design revises it in the same change.

## Validate the affected contract

Run the gate through the single entry point before pushing. The hosted workflow runs the same checks on every push and pull request:

| mode | scope |
|---|---|
| `scripts/check.sh portable` | fmt, clippy (all features, `-D warnings`), test, the syntax examples, doc (`-D warnings`) |
| `scripts/check.sh coverage` | line coverage, floor 90 |
| `scripts/check.sh differential` | the clingo differentials and the tree-sitter cross-check (via pixi) |
| `scripts/check.sh full` | all of the above |

Report the checks you actually ran. Never weaken a gate to accommodate a change. Scope mutation testing (`cargo mutants`) to the files a change touches, and run a mutant only in a scratch copy of the tree, never the working tree: a mutated loop bound can exhaust the machine's memory.

## Keep public documentation current

Update the designs of record, the rustdoc, and the README with the implementation they describe, and check that the README's examples still compile and run. Keep development diaries, session notes, and machine-local paths out of the tree — documentation must be intelligible without them.
