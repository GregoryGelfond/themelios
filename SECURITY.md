# Security policy

## Supported versions

themelios is developed on `main`. There is no tagged release yet, so the supported version is the current `main`, which clients pin by git revision. Fixes land on `main`, and a client adopts one by bumping its pin; there is no separate maintenance branch.

## Reporting a vulnerability

Please report a suspected vulnerability privately, not as a public issue. Use GitHub's private vulnerability reporting — the **Security** tab of the repository, then **Report a vulnerability** — which opens a private advisory visible only to the maintainer.

Include what a maintainer needs to reproduce it: the input (ASP source text, a program built in Rust, or a sequence of calls through the solving contract), the library call, the revision you pinned, and what happened versus what you expected. Please allow a reasonable window for an acknowledgement and a fix before any public disclosure.

## Scope

themelios is a library: it reads, represents, analyzes, and builds ASP programs, and it defines the contract a solving engine implements. It opens no network connection, runs no solver of its own, and executes nothing it reads. Its security posture is stated per tier and surface in the specification, [`docs/specification.md`](docs/specification.md) §12.3–§12.4, and in each tier's design of record:

- The syntax tier is the surface built to meet untrusted input. Its parser is total on arbitrary input and answers with a tree and typed diagnostics, never a panic.
- The program tier's raise, which lowers a parse to a program, is total on any parse in the same way, answering with a program and typed diagnostics, so it may meet untrusted text behind the parser. What a transformation of the program then spends is set by the program: `unpool` expands pools into a number of rules exponential in the pooled positions, so an embedder that accepts untrusted programs budgets that work.
- A registered `@`-function or propagator runs with the host process's full trust. themelios contains an extension author's accidents, never their malice.

The full threat-model statement lands before the engine adapters do (specification §12.4), and it will be the right starting point for a security review of them.

Reports of the following are especially in scope:

- a panic, a hang, or unbounded resource use reachable from source text handed to the lexer, the parser, or the raise;
- a crash or unbounded resource use reachable from a program handed to the analysis, the rendering, or the solving contract — themelios bounds its own walks and aims to refuse such input with a typed reason, never to abort;
- an unsound answer: an analysis verdict of `Holds` that is false, rendered text that raises to a different program, ground arithmetic that wraps instead of refusing, or a truncated search reported as complete;
- `unsafe` code, a foreign library, or a build script reaching a crate whose closure forbids it. The structural checks in the crates' `tests/trust.rs` hold those closures (specification §12.3).

themelios pins its dependencies in the committed `Cargo.lock` and admits each only with an argued necessity (specification §12.5), so a report tied to a specific dependency version is welcome.
