# themelios

[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
![Rust 1.97+](https://img.shields.io/badge/rust-1.97%2B-orange?style=flat-square)
[![Coverage 96%](https://img.shields.io/badge/coverage-96%25-brightgreen?style=flat-square)](CONTRIBUTING.md#verification-and-review)
[![checks](https://img.shields.io/github/actions/workflow/status/GregoryGelfond/themelios/checks.yml?branch=main&style=flat-square&label=checks)](https://github.com/GregoryGelfond/themelios/actions/workflows/checks.yml)
[![Documentation](https://img.shields.io/badge/docs-design%20of%20record-blue?style=flat-square)](#documentation)

θεμέλιος, *foundation* — the library-first groundwork for Answer Set Programming tools in Rust.

themelios reads, represents, analyzes, and builds Answer Set Programming (ASP)
programs — the clingo/clingcon language and the ASP-Core-2 standard — as Rust
values, and defines one engine-agnostic contract for solving them. It is the
layer a formatter, a solver front end, a protocol bridge, a test harness, or an
editor tool builds on, so none of them has to write its own parser, program
representation, or analysis.

themelios doesn't ship a solver of its own — an engine plugs in behind its
solving contract, as [zetesis](https://github.com/GregoryGelfond/zetesis)
does. The clingo and clingcon adapters are next.

**Highlights**

- **Lossless syntax.** An error-resilient parser keeps every byte of the source,
  comments and whitespace included, and reports typed diagnostics rather than
  stopping at the first error. It is tested differentially against clingo 5.8.2
  and cross-checked against the tree-sitter-clingo grammar.
- **Programs as values.** A `Program` is an owned, total value that carries its
  provenance in its nodes. Raise one from source, or build one in Rust, with
  compile-time macros if you like. Rendering it back gives canonical text that
  raises to the same program, provenance aside.
- **Analysis with evidence.** Safety, grounding finiteness, the predicate
  dependency graph, and the program classes of the literature (tight,
  stratified, head-cycle-free, normal, Horn, disjunctive, choice) are each
  reported as a typed verdict. A negative or undecided verdict names what is
  responsible: the unsafe variables, the rule, or the cycle.
- **One contract, any engine.** An engine implements the `Backend` trait once.
  The agent, the typed outcomes, and the three-valued query tier then work over
  any engine, and an executable conformance suite checks each adapter against
  the contract.
- **Honest answers.** A search stopped early is inconclusive, never a false
  "no answer set". A request beyond an engine's declared capabilities is a
  typed refusal, never a silent downgrade, and arithmetic that would overflow
  is refused rather than wrapped.
- **No panics at the boundary.** Every foreign input crosses a `Result` with
  typed diagnostics, and the tiers contain no `unsafe` code.

## A first example

Raise a program from source, ask the analysis about it, and render it back:

```rust
use themelios_analysis::{Analysis, ProgramClass};
use themelios_program::{Dialect, raise::raise_str, render};

let source = "
    reach(X, Z) :- reach(X, Y), edge(Y, Z).
    reach(X, Y) :- edge(X, Y).
    edge(1, 2). edge(2, 3).
";

// Parse, then raise the syntax tree to the program it denotes.
let raised = raise_str(source, Dialect::Clingo)?;
assert!(raised.diagnostics().is_empty());
let program = raised.into_program();

// Read its structure: every rule is safe, and the program is stratified.
let analysis = Analysis::of(&program);
assert!(analysis.safety().is_safe());
assert!(analysis.classes().confirmed().any(|class| class == ProgramClass::Stratified));

// Render it back as canonical text.
print!("{}", render(&program, Dialect::Clingo)?);
```

```clingo
edge(1, 2).
edge(2, 3).
reach(X, Y) :- edge(X, Y).
reach(X, Z) :- edge(Y, Z), reach(X, Y).
```

A program is a set of rules, and a rule's body is a set of literals, so
rendering is canonical. Statements and literals come out in one fixed order,
whatever order they were written in.

## Build programs in Rust

The construction macros parse ASP when your crate compiles, so a malformed
program is a compile error. They expand to the program tier's constructors,
which means a program built this way is the same value you would get by
calling the constructors by hand:

```rust
use themelios_macros::{program, rule};
use themelios_program::Program;

let origin = 1;

// A whole program; `$origin` splices in a Rust value.
let reach: Program = program! {
    start($origin).
    reach(X) :- start(X).
    reach(Y) :- reach(X), edge(X, Y).
};

// Or one statement at a time.
let hop = rule!(hop(X, Z) :- edge(X, Y), edge(Y, Z));
let hops = Program::of([hop]);
```

`reach` renders as:

```clingo
reach(X) :- start(X).
reach(Y) :- edge(X, Y), reach(X).
start(1).
```

There are nine macros in all: `program!`, `rule!`, `fact!`, `constraint!`,
`minimize!`, `maximize!`, `show!`, `external!`, and `atom!`. Their expansions
name `themelios_program`; a crate that renames that dependency, or re-exports
the macros under its own name, opens an invocation with `#![crate = path]` to
name the runtime instead ([macros design](docs/design/macros.md) §9).

## Solve through any engine

`themelios-solve` is a contract, not an engine. Code written against it asks a
program the same questions whichever engine answers them:

```rust
use themelios_program::Program;
use themelios_solve::prelude::*;

/// Whether `program` has an answer set, asked through any backend.
fn consistent<B: Backend>(program: Program, backend: B) -> Result<bool, Fault> {
    let mut agent = Agent::new(program, backend);
    Ok(matches!(agent.determination()?, Determination::Consistent(_)))
}
```

The answer is three-valued. `Consistent` and `Inconsistent` are settled
answers. `Inconclusive` means the search stopped before deciding, because a
budget ran out or the search was interrupted, so a truncated search never
passes for a complete one. A backend declares its capabilities up front, and
asking it for more gets a typed refusal. An engine joins by implementing
`Backend` and passing the conformance suite, `themelios_solve::conformance::run`.

`themelios-query` reads the results the way a logician does: cautious and brave
consequences, the three-valued answer to a query, and the bindings of an open
pattern.

## Install

themelios isn't on crates.io yet. Depend on the crates you need by git, and pin
them all to the same revision:

```toml
[dependencies]
themelios-program = { git = "https://github.com/GregoryGelfond/themelios.git", rev = "3339a8a" }
themelios-analysis = { git = "https://github.com/GregoryGelfond/themelios.git", rev = "3339a8a" }
```

The build fetches the repository over HTTPS, and no credentials are needed.
themelios builds on Rust 1.97 or newer.

## The crates

| Crate | What it is |
| --- | --- |
| `themelios-base` | Source text, spans, line indexing, and the diagnostics model every tier reports through. |
| `themelios-syntax` | The lexer, the lossless syntax tree, the error-resilient parser, the typed AST, comment attachment, and token-stream equivalence, for clingo/clingcon and ASP-Core-2. |
| `themelios-program` | The `Program` value: symbols and terms, rules and directives with provenance, construction and the raise from syntax, canonical rendering, transformation, and unification. |
| `themelios-analysis` | Safety, grounding finiteness, the dependency graph and its strongly-connected components, and the program classes, as typed verdicts that name their evidence. |
| `themelios-macros` | Compile-time construction from ASP text, with `$` splices of Rust values. |
| `themelios-solve` | The engine-agnostic solving contract: the `Backend` trait, the agent, the typed outcomes and faults, and the conformance suite. |
| `themelios-query` | Reading the solve tier's results: the three-valued answer, cautious and brave consequences, bindings, and the world view. |
| `themelios-potassco`, `themelios-potassco-sys` | Reserved for the clingo and clingcon adapter; not yet implemented. |

## Documentation

- [`docs/specification.md`](docs/specification.md) — what themelios is, what it
  delivers, and how it is assured.
- [`docs/grammar.md`](docs/grammar.md) — the grammar of record: the shared
  clingo/clingcon syntax and its ASP-Core-2 dialect.
- The design of record for each tier, under [`docs/design/`](docs/design/):
  [base](docs/design/base.md), [syntax](docs/design/syntax.md),
  [program](docs/design/program.md), [analysis](docs/design/analysis.md),
  [macros](docs/design/macros.md), [solve](docs/design/solve.md), and
  [query](docs/design/query.md).
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — the standard the work is held to.
- The API reference: `cargo doc --workspace --no-deps --open`.

A manual in the style of the morphe and keryx books is planned.

## Development

```sh
scripts/check.sh full     # fmt, clippy, tests, docs, coverage, and the clingo differentials
```

`scripts/check.sh` is the single gate entry point; run `full` before you push.
The same checks run in CI on every push and pull request. See
[`CONTRIBUTING.md`](CONTRIBUTING.md#verification-and-review) for the details.

## Status

**Built through the engine-free solving core.** The source and diagnostics
model, the syntax tier, the program and analysis tiers, the construction
macros, and the solve and query tiers are built and documented against their
designs of record. They are held to a high engineering bar: property laws,
golden corpora, differentials against clingo 5.8.2 and the tree-sitter-clingo
grammar, scaling tripwires for the load-bearing algorithms, mutation testing,
fuzzing of the lexer and parser, and a 90% line-coverage floor (96% measured).

**Not yet:**

- **Engine adapters.** The clingo and clingcon backends (`themelios-potassco`)
  are next. Until they land, no engine ships in this repository, and Rust
  `@`-functions and custom theory propagators, both declared in the contract,
  wait on a backend that runs them.
- **crates.io.** For now, themelios is consumed by git revision.

## Used by

themelios is the foundation of a family of ASP tools. Each tool builds on it
at arm's length, pinned to a git revision:

- [morphe](https://github.com/GregoryGelfond/morphe) — an opinionated,
  safety-certified formatter for ASP, built on `themelios-syntax`.
- [keryx](https://github.com/GregoryGelfond/keryx) — a bidirectional bridge
  between Protocol Buffers and ASP, built on `themelios-program`.
- [zetesis](https://github.com/GregoryGelfond/zetesis) — a parallel
  answer-set solver for CPUs and GPUs, with eager and lazy grounding. It reads
  programs through `themelios-syntax`, `themelios-program`, and
  `themelios-analysis`, and implements `themelios-solve`'s backend contract,
  held to its conformance suite.

## License

MIT. See [LICENSE](LICENSE).
