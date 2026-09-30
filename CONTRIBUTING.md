# Contributing to themelios

Start with the examples in the [README](README.md). Then read the specification, [`docs/specification.md`](docs/specification.md), and the design of record for the tier you are changing, alongside the grammar of record, [`docs/grammar.md`](docs/grammar.md). [`docs/design/`](docs/design/) holds a design for each tier: base, syntax, program, analysis, macros, solve, and query. Until the manual lands, the designs are the reader's path; this file is the standard the work is held to.

themelios is the foundation for Answer Set Programming in Rust, and only that. It reads, represents, analyzes, and builds ASP programs, and it defines the contract a solving engine implements. It ships no engine of its own: an engine enters as a backend behind that contract. It is a library throughout. There is no binary, and each tier is a crate a client composes. The MIT license names Gregory Gelfond.

## Design from the logician's questions

Public operations speak in the domain's terms: programs, rules, atoms, answer sets, models, consequences, and world views. They answer the questions a logician asks of a program, not the incidental shapes of a parser, a tree library, or an engine. The `Program` is the logician's abstract object, a set of rules. The raise from the syntax tree is the parsing relation onto it, and rendering gives its canonical text ([`docs/design/program.md`](docs/design/program.md) §1).

The tiers are layered, and each depends only on the tiers beneath it: base, then syntax, then program and analysis, then solve and query, with the macros over syntax and program. **No engine type appears in a tier's public surface.** The solve and query tiers are engine-free: each forbids `unsafe` code and links no foreign library, and a test holds its dependency closure to that. An engine enters as an adapter implementing the contract. A capability the adapter lacks is a typed refusal at the door, never a silent degrade. Values cross to an engine through its doors; rendering a program to text and parsing it back across that seam is forbidden ([`docs/design/solve.md`](docs/design/solve.md) §10.2).

Import only what is truly necessary (specification §12.5). Each dependency carries its argued necessity where it is declared, hand-writing is the default where hand-writing is reasonable, and the burden of proof sits on the import. `themelios-base` has no dependencies, and the solve and query tiers depend on nothing beyond the tiers beneath them.

themelios's clients consume it at arm's length, pinned by git revision. A gap a client surfaces is closed here, in the tier that owns it, and the client adopts the fix by a deliberate pin bump.

## Make represented knowledge inspectable

A distinction in a type must have a producer and a consumer; a variant nothing constructs, or nothing reads, is a false claim about the design. Every foreign input crosses a `Result` boundary returning a typed diagnostic or fault — never a panic, never a bare string. One concept has one name. A program is *raised* from a syntax tree and *rendered* back to text. A boundary that admits external input is a *door*. An analysis verdict *holds*, or is *unknown* and names the component that blocked the proof. Each design of record keeps its tier's vocabulary: two words for one thing is the same defect as one word for two.

Documentation is held to the same standard as code: it must be intelligible without access to private working records, and it can never lie. Keep development notes, session logs, and machine-local paths out of the tree. The code and its design of record say the same thing, so a change that departs from a design revises the design in the same change.

## Carry the soundness through

Where themelios cannot give a sound answer, it refuses with a typed reason rather than guess. Each refusal below is a guarantee the tiers make, not a gap:

- **Analysis never over-claims.** A verdict is `Holds` only when proven; otherwise it is `Unknown`, carrying the component whose recursion blocked the proof. A false `Holds` is the paramount analysis defect.
- **Arithmetic never wraps.** Ground evaluation over the `i32` symbol domain refuses a result it cannot represent rather than wrapping it ([`docs/design/program.md`](docs/design/program.md)).
- **A truncated search never passes for a complete one.** Whether a program has an answer set is kept apart from how the search ended. A search stopped by a budget or an interrupt is `Inconclusive`, never "no answer set" ([`docs/design/solve.md`](docs/design/solve.md) §5).
- **Input depth and size are threats, not accidents.** Every walk over untrusted input is iterative or grammar-bounded, and the parser bounds nesting. A load-bearing algorithm over unbounded input uses the best-known practical algorithm from the start: Martelli–Montanari for unification, an iterative Tarjan for the strongly-connected components. Each carries an in-suite scaling tripwire and a bench for its own adversarial shape, because a passing differential is not a complexity bound.

## Verification and review

Every change is held to the gate, green, before it lands. The single entry point is `scripts/check.sh`:

| mode | scope |
|---|---|
| `portable` | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`; `cargo test --workspace --locked`; the syntax tier's example witnesses; `cargo doc` with `RUSTDOCFLAGS=-D warnings` |
| `coverage` | line coverage, floor **90** (`cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --locked --fail-under-lines 90`) |
| `differential` | the differentials against clingo 5.8.2 (syntax, program, and analysis) and the tree-sitter-clingo cross-check, run through pixi, which supplies the pinned authorities from the committed `pixi.lock` |
| `full` | all of the above |

The coverage floor is measured line coverage rounded down to a multiple of five, minus five: slack for toolchain and measurement drift. It is raised only deliberately and never lowered. The floor is a tripwire for a wholly untested module, never a target, because coverage counts lines run, not behaviour pinned. The adversarial work is done by the rest of the gate and by the out-of-band instruments:

- the property laws and the golden corpora;
- the differentials against the pinned authority;
- the scaling tripwires;
- the lexer and parser fuzz targets (`crates/themelios-syntax/fuzz`, run with `cargo fuzz`);
- mutation testing (`cargo mutants`, configured in `.cargo/mutants.toml`, where each accepted survivor carries its argument).

Scope a mutation run to the files a change touches; a whole-workspace run is long and memory-hungry.

The workspace denies `unsafe_code`, `missing_docs`, `unused`, `dead_code`, and pedantic Clippy. Library code never suppresses `unused` or `dead_code`. The only such allowances are in test-support modules that several test binaries share, each binary reading a subset. Beyond the few Clippy lints the workspace allows by policy, each argued in `Cargo.toml`, a Clippy lint is allowed only on the one item where its concern does not apply. One test name states one proposition; fifty characters is a review cue, not a limit.

**Proposing a change.** Branch off `main`. `main` is protected and lands only by pull request, rebase-merged (`gh pr merge --rebase`) to keep history linear. Run `scripts/check.sh full` locally before you push. The hosted workflow, `.github/workflows/checks.yml`, runs the same checks on every push and pull request: the portable gate on Linux and macOS, coverage, and the differentials. A change lands green on both. Commit subjects are plain and imperative ("Bound the parser's nesting depth", not "feat:"). Keep per-person tooling out of the tree.

## Repository presentation

The GitHub description, topics, badges, and release metadata state what is true and no more. The badges show the license, the Rust version, a coverage figure measured locally by `scripts/check.sh coverage`, and the status of the hosted workflow. A CI badge must identify an active workflow; if the workflow is paused, its badge comes down with it. There is no release yet. Once there is one, a release tag corresponds to what was released.
