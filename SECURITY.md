# Security policy

## Supported versions

themelios is developed on `main`. There is no tagged release yet, so the supported version is the current `main`, which clients pin by git revision. Fixes land on `main`, and a client adopts one by bumping its pin; there is no separate maintenance branch.

## Reporting a vulnerability

Please report a suspected vulnerability privately, not as a public issue. Use GitHub's private vulnerability reporting — the **Security** tab of the repository, then **Report a vulnerability** — which opens a private advisory visible only to the maintainer.

Include what a maintainer needs to reproduce it: the input (ASP source text, a program built in Rust, or a sequence of calls through the solving contract), the library call, the revision you pinned, and what happened versus what you expected. Please allow a reasonable window for an acknowledgement and a fix before any public disclosure.

## Scope

themelios is a library: it reads, represents, analyzes, and builds ASP programs, and it defines the contract a solving engine implements. It opens no network connection, runs no solver of its own, and executes nothing it reads. Its security posture is stated per tier and surface in the threat model of record, [`docs/threat-model.md`](docs/threat-model.md) — what each tier defends against however it is embedded, what it trusts, and what an embedder must supply — and that document is the starting point for a security review. The trust architecture it rests on is the specification's, [`docs/specification.md`](docs/specification.md) §12.3. In brief:

- The syntax tier is the surface built to meet untrusted input. Its parser is total on every admitted text — arbitrary bytes are refused at admission with a typed reason — and answers with a tree and typed diagnostics, never a panic.
- The program tier's raise, which lowers a parse to a program, is total on any parse in the same way, answering with a program and typed diagnostics, so it may meet untrusted text behind the parser — and a consumer of untrusted text gates on those diagnostics, since a program raised with one is the raise's best reading, not the text's meaning. What a transformation of the program then spends is set by the program: `unpool` expands pools into a number of rules exponential in the pooled positions, and `substitute` resolves into an output a pathological unifier can make exponentially larger than its input, so an embedder that accepts untrusted programs budgets that work. A value built through the public types rather than the raise is the host's own: a pool written with no alternatives, which the constructors refuse, drops its statement when `unpool` meets it.
- The solving contract's Door A admits a parse only when it is in the language and its raise is clean, refusing it whole with typed diagnostics before any backend is asked, so a consumer that lowers untrusted text through it holds the raise's gate without writing it. Its cost is the raise's, near-linear in the parse. That gate is membership in the language, not a sandbox: a program admitted at Door A, or lowered through Door B, may carry a `#script` block, `#include` directives, and `@`-calls whose arguments come from its text, which a backend honouring them would run, read, or call. The threat model states the policy once, for both doors: no backend in this stack runs a script or resolves an include, and a registered function is trusted code (`docs/threat-model.md` §3.6).
- A registered `@`-function or propagator runs with the host process's full trust. themelios contains an extension author's accidents, never their malice.
- The engine adapter, behind its feature, links libclingo and libclingcon into the host process. A grounding that never ends, or a crash inside the engine, cannot be contained in-process, so a service that runs untrusted programs isolates engine work in a process it can kill (the threat model, §5.9 and §6).

Reports of the following are especially in scope:

- a panic, a hang, or unbounded resource use reachable from source text handed to the lexer, the parser, the raise, or Door A's admission;
- a crash or unbounded resource use reachable from a program handed to the analysis, the rendering, or the solving contract — themelios bounds its own walks and aims to refuse such input with a typed reason, never to abort;
- an unsound answer: an analysis verdict of `Holds` that is false, rendered text that raises to a different program, ground evaluation through `Term::evaluate` that wraps instead of refusing, a model reached through a repair the engine reported — an undefined operation treated as false — or a truncated search reported as complete. The engine's own wrapping of overflowing ground arithmetic, which it does not report, is a divergence the threat model records (§7), not a report to file;
- `unsafe` code, a foreign library, or a build script reaching a crate whose closure forbids it. The structural checks in the crates' `tests/trust.rs` hold those closures (specification §12.3);
- undefined behaviour, a process abort, an unbounded read, or an act beyond the input handed in — a file read, a script run — reachable through the engine adapter's own calls (the threat model, §5).

themelios pins its dependencies in the committed `Cargo.lock` and admits each only with an argued necessity (specification §12.5), so a report tied to a specific dependency version is welcome.
