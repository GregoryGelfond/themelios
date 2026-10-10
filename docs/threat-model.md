# themelios — threat model of record

This is the threat-model statement specification §12.4 commits: its named companion, stating per tier and per
extension surface what themelios defends against regardless of how it is embedded, what it assumes trusted, and
what an embedder must supply to compose a threat model of its own. The shape is the specification's; the shared
frame (§2) is stated once and every entry after it uses its vocabulary.

The Potassco adapter this document constrains is specified in `solve.md` §11 and is not yet built. Everything said
of it here (§5) is an obligation its implementation owes and the security review reads it against — never a
description of present code. Everything said of the built tiers (§3) cites the design section that states it and,
where one exists, the instrument that holds it. A change to a surface named here revises this document in the same
change.

---

## 1. Scope

themelios is a library (`SECURITY.md`): it reads, represents, analyzes, and builds ASP programs, and it defines the
contract a solving engine implements; the Potassco adapter links libclingo and libclingcon into the host process to
serve that contract. It opens no network connection and spawns no process, and — the adapter included — it reads no
file and writes none: §5.2 states how the adapter keeps the engine's file-touching entry points out of reach.

Out of scope: an adversary already executing code in the host process; timing and other side channels; the host's
transport, authentication, and tenancy; malice in code an embedder registers (§4); and denial of service beyond the
per-request bounds an embedder sets (§6).

---

## 2. The shared frame

### 2.1 Deployment contexts

- **The developer tool.** An editor, language server, formatter, or REPL over the developer's own files. The input
  is the developer's own; the threat is an accident — a malformed or pathologically nested file — and the failure
  that matters is a crash or a hang that takes the tool down.
- **The batch pipeline.** A build or CI job over repository content, some of it contributed by strangers — a pull
  request from a fork. The input is semi-trusted; the failures that matter are a crash, a hang, or unbounded
  resource use that stalls or kills the job, and an unsound answer that lets a check pass it should fail.
- **The service boundary.** The pythia-class deployment: source text, programs, queries, and request sequences
  arrive from untrusted callers, and many callers share a host. Every failure class of §2.4 matters, and resource
  bounds are part of correctness.

### 2.2 Adversaries

- **The hostile author.** Controls the bytes, programs, queries, scenarios, and call sequences an embedding hands
  the public surface, and aims at every failure class of §2.4.
- **The careless extension author.** Writes the `@`-function, propagator, or conversion an embedder registers, and
  may — by accident — panic, loop, allocate without bound, or break determinism. themelios contains the accidents it
  can and trusts the author's intent (specification §12.4).
- **The faulty engine.** libclingo and libclingcon at their pinned versions: correct in the main, with characterized
  divergences, behaviours the adapter must compensate, and failure modes — a stack overflow, an abort — that no
  in-process code can catch.
- **The supply chain.** The Rust dependencies, the engines' sources, the build scripts, and the toolchain.

### 2.3 Boundaries

- **The text boundary** — bytes become a `Source` (`base.md` §3.2).
- **The value boundary** — a `Program`, `Term`, `Scenario`, `Query`, or request reaches a public operation, raised
  from text or constructed in Rust. At the engine seam it takes two forms, one per door (`solve.md` §10.2): a
  `Program` (Door B), and an `Admitted` parse (Door A), which the core admits only when the parse is in the
  language and its raise is clean, so no recovered statement crosses. An engine's own input format — an aspif
  stream from a foreign grounder, whose atoms and literals arrive as the engine's own identifiers — is no door
  of the contract but the adapter's own, if it ingests one (`solve.md` §10.3).
- **The engine seam** — the `Backend` contract (`solve.md` §4), the one door between the engine-free core and any
  engine (specification §12.3, criterion 4).
- **The FFI boundary** — inside the adapter, Rust calls into C and C++, and the engine calls back into Rust.
- **The extension boundary** — the engine calls embedder code through a registered extension.
- **The build boundary** — the sources, dependencies, and scripts that produce the binary.

Each surface below answers three questions: what it **defends** regardless of embedding, what it **trusts**, and
what the **embedder supplies**.

### 2.4 Failure classes

- **Crash** — a panic escaping the public surface, a process abort, or an uncatchable fault such as stack
  exhaustion.
- **Hang** — an operation that does not terminate.
- **Exhaustion** — memory, CPU, or stack consumed without a bound the caller can know in advance.
- **Unsound answer** — a result that is wrong and not marked as such: a false `Holds`, a truncated search reported
  complete, wrapped arithmetic, an unfaithful lowering, a theory value laundered into an atom.
- **Corruption** — shared state left inconsistent: the engine's process-global interning, a poisoned lock,
  undefined behaviour.
- **Reach** — an operation acting beyond the input it was handed: reading or writing a file, running a script,
  opening a connection.

### 2.5 The standing mechanisms

Every tier inherits these; the entries of §3–§5 cite where each binds.

- **Totality.** No panic escapes a public operation on any input (specification §2, item 8). Each design's failure
  table names every refusing door, and every operation not listed is total (`base.md` §9, `syntax.md` §13,
  `program.md` §15, `analysis.md` §9).
- **The depth discipline.** Every walk over user-reachable structure is iterative or grammar-bounded (specification
  §5.2, §7.2), and the depth proof proves it per walk on a stated stack (specification §10.1; `syntax.md` §16;
  `program.md` §13, §16).
- **Refusal beats repair.** A value that cannot be represented is refused with a typed reason — never truncated,
  normalized, or wrapped (specification §5.2).
- **Admission ceilings.** A limit is named, measured, and enforced at the one door where data enters — `base.md`
  §3.2's `Source::MAX_LEN`, `syntax.md` §6.6's `NestingLimit` — and never a bare numeral (specification §5.2).
- **Scaling tripwires.** Each load-bearing algorithm carries an in-suite tripwire for its own adversarial shape and
  a bench (specification §10.1; `solve.md` §13.3); the costs each design states are the shapes they hold.
- **The trust architecture.** `unsafe` is forbidden at every crate root but the two potassco crates; the engine-free
  closures link no native code and run only named build scripts; each tier's `tests/trust.rs` holds these facts
  against Cargo's resolved graph (specification §12.3; `solve.md` §16).
- **Honest incompleteness.** A search that did not close the space yields no universal reading — the exhaustion gate
  (`solve.md` §5.2; `query.md` §2.3); an inconsistent conclusion is exhaustive and a cancelled search never is
  (`solve.md` §5.1; the conformance suite checks both, the second over a backend that declares cancellation); a
  verdict is `Holds` only when proven (`analysis.md` §6.1); and `Unknown` is a value of `Answer`, not a missing one
  (`query.md` §2.2).

---

## 3. The tiers

### 3.1 base — the text boundary

- **Defends.** `Source::from_bytes` admits exactly valid UTF-8 of at most `Source::MAX_LEN` bytes and refuses the
  rest with a typed reason, with no lossy replacement, no BOM stripping, no line-ending normalization (`base.md`
  §3.2). Every offset downstream is a `u32` under that ceiling, and offset arithmetic is checked (`base.md` §4.1).
  Every view renders every coherent diagnostic, degrading to a named placeholder rather than failing (`base.md`
  §7.1, §9).
- **Trusts.** The standard library alone: the crate has no dependencies (specification §12.5).
- **Embedder supplies.** An admission size fit for its context. `MAX_LEN` — about four gibibytes — is the
  representable ceiling, not a service limit; lexing and parsing are linear in the text in time and memory
  (`syntax.md` §13), so a service caps request size far below it.

### 3.2 syntax — the text boundary

- **Defends.** The lexer and parser are total on every admitted text and every token source: they never panic, and
  they terminate in time and memory linear in the text (`syntax.md` §13), held over arbitrary input by the fuzz crate
  (`syntax.md` §16). A hostile run — a megabyte of `$` — costs one token and one diagnostic (`syntax.md` §4.5).
  Nesting past the limit in force is refused with a located diagnostic, never a crash: `NestingLimit::DEFAULT` (128
  frames) at the file door, whose deepest tree is safe to hold on a two-mebibyte stack, and `NestingLimit::CEILING`
  (5,000) at a general door, held under `REQUIRED_STACK_BYTES` (64 MiB); the tree's depth is bounded by
  `MAX_TREE_DEPTH` because its dependency's drop, equality, and display recurse in depth (`syntax.md` §5.4 law 3,
  §6.6, §14). The engine's own parser dies of a stack overflow past about 61,600 frames in its most-limited
  families; this tier refuses first, by design (`syntax.md` §6.6; grammar §11 D2). `parse_str` refuses text past
  `Source::MAX_LEN` (`syntax.md` §13).
- **Trusts.** rowan, pinned and audited, its internal `unsafe` acknowledged (`syntax.md` §14); the shipped closure
  is enumerated and held by the trust check.
- **Embedder supplies.** The file door's `DEFAULT` on any thread; a thread of `REQUIRED_STACK_BYTES`, or
  `with_required_stack`, before choosing `CEILING`; and a stack sized from `MAX_TREE_DEPTH` for any recursive visitor
  of its own (`syntax.md` §6.6). This is the surface built to meet untrusted text directly (specification §12.4): a
  service admits source through it and through nothing else.

### 3.3 program — the value boundary

- **Defends.** Every operation is total over any value — malformed, hostile, or deep. Every walk over `Term` and
  `Symbol` is iterative, the compiler-derivable ones included — clone, drop, comparison, hashing, rendering — so a
  ground value tens of thousands of levels deep is handled, never refused (`program.md` §13), held by the depth
  proof and by the mirror differential against a derived twin (`program.md` §16). Constructors refuse raw invalid
  data and never repair (`program.md` §7.2); ground evaluation refuses overflow where the grounder wraps
  (`program.md` §3.5); `render` refuses a value its dialect cannot spell rather than emit text that raises to a
  different program (`program.md` §10, §15). `raise` is total and linear in the tree, and its occurrence stream stays
  linear against an adversarial `#program` formal count (`program.md` §3.1, §8). `mgu` is near-linear
  (`program.md` §11.1), and matching against an answer set is `O(log n + k)` (`program.md` §11.3).
- **The raise, behind the parser.** The raise may meet untrusted text, one step behind the syntax tier: it is total
  on any parse and answers with a program and typed diagnostics, and it drops nothing silently — a statement it
  cannot complete is skipped beside a diagnostic, a value it cannot represent is kept as a stand-in beside one, a
  content-equal global definition repeated within a part is diagnosed (`RepeatedDefinition`), and so is every
  `#script` block after the first (`ExtraScript`) (`program.md` §6.3, §8). A program raised with a diagnostic is
  the raise's best reading, not the text's meaning, so a consumer of untrusted text gates on the diagnostics; Door
  A's admission is that gate, written once (`solve.md` §10.2).
- **Trusts.** The base and syntax tiers; nothing else (specification §12.5). And the caller of its programmatic
  doors: a value built in Rust — through `Program::of`, `of_nodes`, or `of_keyed_nodes`, the element constructors,
  the construction macros, or a program assembled from `raise_statement`'s fragments — is the host's own. The
  constructors refuse raw invalid data, but nothing checks a whole program there: two content-equal `#const`
  definitions merge silently where the raise would diagnose them, a program may carry several `#script` blocks,
  and a hand-built pool with no alternatives drops its statement when `unpool` meets it (`program.md` §6.3, §7,
  §9.1).
- **Embedder supplies.** A bound on the two operations whose output can be exponential in their input. Both are
  linear in their output, and neither refuses: `unpool`, whose cross-product of pools is exponential in the number
  of pooled positions (`program.md` §9.1), and `substitute` resolving a pathological unifier (`program.md` §9.2,
  §11.1). A service that runs either over untrusted programs bounds it (§6).

### 3.4 analysis — the value boundary

- **Defends.** `Analysis::of` is total and never refuses (`analysis.md` §9). A strengthening verdict is `Holds` only
  when proven, and otherwise `Unknown` with the component that blocked the proof (`analysis.md` §6.1). No walk
  recurses on the call stack (`analysis.md` §8). At an untrusted boundary the sound grounding gate is
  `is_safe() && Holds` on finiteness, which rejects unbounded term growth before an engine grounds (`analysis.md` §5).
- **Trusts.** The program tier.
- **Embedder supplies.** Two cautions. Finiteness is term depth only: integer growth through an aggregate
  (`q(M) :- M = #count { X : q(X) }`) is outside its scope and reports `Holds` (`analysis.md` §5), so `Holds` is not
  a proof that grounding terminates. And `Analysis::of` reads the unpooled program (`analysis.md` §5; `program.md`
  §9.1), so its cost, `O(unpooled + edges)` (`analysis.md` §8), inherits `unpool`'s output — exponential in the
  number of pooled positions — and a service bounds a pooled program as it bounds `unpool` (§3.3).

### 3.5 macros — the build boundary

- **Defends.** A macro never panics on any token stream, and it refuses what it cannot carry verbatim rather than
  mangle it: `#script` by design, as a located compile error (`macros.md` §7, §8). An `#include` is a located
  compile error too until its codegen arm lands, and then is carried and never resolved, as every tier carries one
  (`macros.md` §12; `program.md` §4.8). A body denotes
  the program its spelling denotes under the one grammar, and its expansion calls the program tier's public
  constructors, so it can build no value those constructors refuse (`macros.md` §3; specification §7.3). Every
  path it emits is absolute — at the selected runtime root or `::std` — so no name in scope at the call site
  captures the expansion (`macros.md` §9). It opens no file, reads no manifest, and runs nothing at build time.
- **Trusts.** Its input, which is never untrusted data: the compiling crate's own token stream, handed over by
  rustc. The Rust expressions a body splices, which run in the caller's context with the host's trust; the runtime
  root a caller selects with `#![crate = path]`, the caller's own path; and the proc-macro toolchain —
  `proc-macro2` and `quote`, pinned, at compile time only, in no shipped closure (`macros.md` §9, §10). The crate
  forbids `unsafe`, has no build script, and its runtime closure is FFI-free, each held by its trust check.
- **The editor and the build.** An editor's expansion can differ from the compiler's: rust-analyzer never joins a
  forwarded token's last punctuation to what follows it, so through a wrapper it reads `:-` as `: -` and may show
  an error, or another reading, that the build does not have (`macros.md` §9). The build is exact. No adversary
  stands at this boundary: what ships is the compiler's expansion, and the editor's is a display.
- **Embedder supplies.** Nothing at run time. A service that builds programs from requests uses the constructors,
  never the macros, since a macro's input is source code.

### 3.6 The solve core — the value boundary and the engine seam

- **Defends.** The contract is the one door to an engine (specification §12.3, criterion 4). A request beyond a
  backend's declared capabilities is refused with a typed fault before it is paid for, never silently degraded
  (`solve.md` §4.1, §4.2). Faults carry a closed locus and name what they refused — a statement, a part, a parse,
  or the request's failed presupposition — so a consumer acts on a refusal by matching, never by reading prose
  (`solve.md` §5.4). Door A admits a parse only when it is in the language and its raise is clean, refusing it
  whole with typed diagnostics before any backend is asked, so a consumer that lowers untrusted text through it
  holds the raise's gate without writing it (`solve.md` §10.2). The outcome vocabulary makes the named pathologies
  unconstructible (`solve.md` §5.3): enumeration and optimization are distinct requests and handles, so no
  enumeration reports a trajectory; a complete collection exists only for a closed search; one `Conclusion` stands
  orthogonal to the `Determination`, with no second flag to disagree with it; and `Optimum` has no public
  constructor. The two conclusion invariants — an inconsistent conclusion is exhaustive, a cancelled search never
  is — are not among them: the backend supplies the terminal conclusion (`solve.md` §5.2), so they are held by the
  conformance suite, below. Enumeration streams with a constant resident set, asserted by the laziness law
  (`solve.md` §5.2, §13.3). The knowledge ledger's rebuild, `observe`, and `forget` are linear, each held by a
  scaling tripwire (`solve.md` §13.3). The conformance suite checks a backend's honesty: outcome correctness
  through both doors, capability honesty (every declared capability's method answers), the pathologies, the fault
  loci, the two conclusion invariants, and a backend's own state (`solve.md` §13.1).
- **What a program can make a backend do.** Membership in the language is not a sandbox. A program admitted at
  Door A, or lowered through Door B, may carry a `#script` block, `#include` directives, and `@`-calls whose
  arguments come from its text, and the policy is one for both doors. No backend in this stack acts on a script
  or an include: the Potassco adapter refuses both at `lower` (§5.6), and a single-shot backend carries them and
  runs or resolves neither (`solve.md` §6.3), zetesis among them (`solve.md` §12). The conformance suite holds
  the include half on every backend (`solve.md` §13.1, obligation 15); no portable check observes a script run,
  so that half is held per adapter and named a residual (§7.2). A backend that would honour
  either owes its own entry here before it meets an untrusted program. An `@`-call hands its arguments — program
  data — to a registered function, which is trusted code (§4), so an embedder serving untrusted programs
  registers only functions whose effect it accepts on arguments a caller chooses.
- **Parts.** A single-shot backend meeting a part beyond `base` answers the base alone — the language's
  single-shot reading — or refuses the program with a Program fault naming the part by its key, unlocated, its
  content identifier text from the source (`Refused::Part`; `solve.md` §5.4, §6.3). Both are admissible in every
  context, since neither answers other than the program denotes under that reading. Which one a backend takes is
  not disclosed before `lower` until the fragment declaration lands (`solve.md` §14), so an embedder that must
  know reads the program's parts first. A multi-shot backend grounds a named part only through `ground`, and
  refuses an instance that matches no part (`solve.md` §4.1).
- **The interrupt handle, a cross-thread capability.** `Interrupt` is `Send + Sync`, may outlive the agent that
  issued it, and holds the backend's `Cancel` alive. A pull is the caller's own act, and it cuts the agent's
  question in flight and nothing else: a pull with no question in flight is forgotten, and the next question
  asked ends one whose run handle was leaked (`solve.md` §6.3). Its authority after the agent drops rests on the
  backend's slot discipline — a pull reaches a slot the backend owns and clears on drop, never a pointer into the
  engine — so a pull through a handle that outlived its agent and its run cuts nothing (`solve.md` §4.1). The
  conformance suite holds the stale pulls, and each adapter's race harness a pull concurrent with a run's opening
  and closing (`solve.md` §6.3, §13.1, §13.3).
- **Trusts.** The backend. The core cannot re-derive an engine's answer; it trusts the backend to report the
  engine's, and it checks what it can structurally — a model holding an atom beside its contrary is refused as a
  backend contract violation (`query.md` §2.3). The conformance suite and the differentials (§8) are how a backend
  earns that trust. A backend outside this repository — zetesis among them (`solve.md` §12) — is held to the
  contract by the same suite and states its own posture in its own repository.
- **Embedder supplies.** The choice of backend. The reading of an `Inconclusive` determination as no answer. A bound
  on what the readings materialise: a `Snapshot`, and the agent's `answer`, `bindings`, and `entails`, collect the
  whole world view, whose size can be exponential in the program (`query.md` §2.3), and model-count caps are
  reserved (`solve.md` §6.3). And a time bound: a budget is honoured natively or refused today, the core's budget
  timer over a backend's cancellation not yet realised (`solve.md` §6.3); and a deadline is cooperative, not hard
  real-time — a backend checks it at points its engine provides (`solve.md` §6.3), and the Potassco engines
  provide none during grounding (§5.9).

### 3.7 query — the value boundary

- **Defends.** Every reading is total; a query of any depth is evaluated by a work-list fold (`query.md` §2.2). A
  non-Herbrand pattern is refused, not guessed, and an anonymous position is refused by policy, the three-way
  partition being unaffected (`query.md` §2.3). A universal reading refuses over a search that did not close the
  space (`query.md` §2.3).
- **Trusts.** The solve tier's outcomes and the program tier's unifier.
- **Embedder supplies.** The materialisation bound of §3.6: a compound reading costs an enumeration of the world view
  (`query.md` §2.2).

---

## 4. The extension surfaces — the extension boundary

`Facts` is built; `Function`, `Propagator`, and `Extract` are declared, and every backend refuses their registration
today — the adapter declares `functions` and `propagators` false until they are realised (`solve.md` §4.1, §7–§9).
What follows is owed when they are.

- **Defends.** Every engine-to-Rust call crosses a panic-containing trampoline, so an extension's panic becomes a
  typed ground-time or theory fault with a locus and never unwinds into the engine (specification §9.6; `solve.md`
  §7.1, §8.1). Arguments and results cross as typed symbols through the refusing conversions, so a value the engine
  cannot represent is refused, never wrapped (`solve.md` §7.2). An `@`-function interns inside the grounding call
  under the interning discipline (§5.3). A propagator's callbacks are scoped to the call and pinned to one solver
  thread (specification §9.6). `Facts` and `Extract` are conversions over values and inherit the program tier's
  posture.
- **Trusts.** The extension's code: it runs with the host process's full trust, and themelios contains its author's
  accidents, never their malice (specification §12.4). A loop or an unbounded allocation inside an extension is not
  contained — the engine calls it synchronously — and an extension that reads a clock, a random source, or a file
  breaks determinism and auditability (`solve.md` §7.2).
- **Embedder supplies.** Registration of code it trusts, and never code a caller chooses; its own bounds on the work
  an extension does.

---

## 5. The Potassco adapter — the FFI and build boundaries

`themelios-potassco-sys` carries the bindings and builds the engines; `themelios-potassco` is the trusted computing
base. They are the only crates whose lint tables allow `unsafe` (`solve.md` §2.1, §11.3, §16), which the trust check
holds. The adapter binds clingo 5.8.2 and clingcon 5.2.1 (`solve.md` §11.1). The engine behaviours cited in this
section were read in the pinned sources and are version-scoped, each named in the register (§7.1).

### 5.1 Build and supply chain

- **Pristine sources.** The engines are built from Potassco's upstream sources: `potassco/clingo` at the `v5.8.2`
  release commit `a99ffb2` and `potassco/clingcon` at the `v5.2.1` release commit `8c47655`, each a git submodule
  pinned by commit and unmodified. No fork and no patch enters the build; a toolchain incompatibility is met with a
  build flag, never an edit to the sources, so every engine behaviour is Potassco's own.
- **The build script.** Builds both engines static through CMake with Python, Lua, and the applications off —
  clingo's defaults detect a system Python or Lua and build binaries — so no script runtime is compiled in and no
  executable is produced. A version check fails the build unless the headers report 5.8.2 and 5.2.1, because the
  adapter's compensations are scoped to those versions.
- **The bindings.** Generated from the two public C headers only, `libclingo/clingo.h` and `libclingcon/clingcon.h`,
  and committed; regeneration runs out of band behind the `bindgen` feature, never in a default build (specification
  §12.5; `solve.md` §2.1).
- **The native library, claimed.** The `links` key claims the `clingo` library by name, so no second package in a
  build links another copy of it; libclingcon is built inside the same crate, against that one library.
- **The feature gate.** Without the adapter's feature both crates are empty shells and the stack is FFI-free
  (`solve.md` §11.3), which the trust check holds for the engine-free closures. Every tier crate leaves the
  feature off; the facade enables it by default (`solve.md` §2.1), so an FFI-free build of the facade disables
  `potassco`.
- **Trusts.** The pinned upstream sources, git's content addressing of the pinned commits, CMake, and the host's C++
  compiler and runtime.
- **Embedder supplies.** A toolchain it trusts.

### 5.2 The privileged interface

- **The manifest.** Every FFI call the adapter makes is admitted against a per-area manifest, and a build-time lint
  fails on any call not in it (specification §12.3, criterion 2; `solve.md` §11.3). The manifest is also where
  reach is closed: it admits no call that loads a file (`clingo_control_load`, `clingo_control_load_aspif`), writes
  one (`clingo_control_register_backend`), parses program text (`clingo_control_add`, `clingo_ast_parse_string`,
  `clingo_ast_parse_files`), registers a script runtime (`clingo_register_script`, whose registry is process-global),
  or runs the application (`clingo_main`). The only program the engine sees is the one the lowering builds (§5.6)
  and, on the clingcon backend, clingcon's own fixed theory definition (§5.8).
- **Errors.** Every fallible engine call returns `bool`; on `false` the adapter reads the thread-local error code and
  message at once, on the same thread, before another engine call can overwrite them, and returns a typed fault.
- **The calls that end the process.** In the pinned source, `clingo_solve_handle_wait` catches any exception and
  calls `std::terminate` — a positive timeout outside asynchronous mode is one such exception — and a solve-event
  callback that returns `false` on a non-model event reaches `clingo_terminate`, which calls `_Exit`
  (`libclingo/src/control.cc`). Neither is reachable: `clingo_solve_handle_wait` is not in the manifest — the
  yield-mode loop of §5.7 needs only `resume`, `model`, `get`, and `close` — and the adapter registers no solve-event
  callback. One added later returns `true` on every non-model event and stashes its error for the enclosing call.
- **Engine messages.** The adapter installs a logger through the trampoline — a null logger writes to the host
  process's standard error. The logger classifies each message as it arrives and records its class in a per-call set
  of flags that no volume of messages can fill, while the message *text* it keeps is bounded by a named limit, the
  message capture, with a stated default; so a program cannot flood the host's error stream or grow the capture
  without limit, and no message of a class that faults the call is ever lost to the bound — a flood of benign
  messages followed by one that faults still faults. Within the bound the capture keeps a faulting class's text
  before a benign one's, evicting benign text first, and among faulting texts the first to arrive, so the fault names
  the first undefined operation the grounding met, its location intact, whatever flood follows. Each message class
  the pinned engine logs (`clingo_warning_e`, `libclingo/clingo.h`) has one disposition, applied from the flags after
  the engine call that logged it returns, with the kept text as the fault's message where there is any:
  - *undefined operation* — the engine met an arithmetic operation or aggregate weight it could not evaluate, or an
    `@`-call nothing answers (`libclingo/src/scripts.cc`), treated the instance as false, and went on. The adapter
    refuses that repair: the message faults the call with a Program fault naming the statement it maps back to
    (`solve.md` §11.1), as the program tier's own evaluator refuses the same case (`program.md` §3.5);
  - *runtime error* — it accompanies an engine error the call then reports; it becomes that fault's text;
  - *undefined atom* — an atom, or a shown signature, no rule defines. The program is well-defined — such an atom is
    false — so the message is informational and dropped;
  - *file included* — unreachable, since the lowering refuses `#include` (§5.6);
  - *variable unbounded* — declared by the header and raised nowhere in the pinned source (§7.1). Should one arrive,
    it takes the disposition of *other*, below: it faults the call with the adapter locus until the spike suite
    classifies it;
  - *global variable* — a global variable in an aggregate element's tuple: a modelling lint whose program is
    well-defined, dropped here; a lint's home is above the engine;
  - *other* — the solver's own log warnings (`libclingo/src/clingocontrol.cc`), under a configuration the adapter
    pins. None is expected, so one faults the call with the adapter locus until the spike suite classifies it.
- **Engine configuration.** Every engine default that shapes an answer — the enumeration mode, the model count, the
  optimization mode, the number of solving threads — is set per request shape, so a default cannot transmit an ambush
  upward (specification §9.5; `solve.md` §11.3); there is no string passthrough (`solve.md` §6.3).

### 5.3 The interning discipline

The pinned libclingo crashes under concurrent interning despite its header's claim of thread safety (specification
§5.2). The adapter owns the compensation, in one place (`solve.md` §10.5, §11.1):

- **One lock.** A single process-global lock serializes every interning writer: symbol creation, control creation,
  program building, and grounding, which interns as it instantiates. Reading a model's symbols into a caller's buffer
  interns nothing and takes no lock.
- **Nested work inside a critical section.** Some engine calls call back into Rust while they hold the lock —
  grounding calls an `@`-function (`solve.md` §7.2), and clingcon's rewrite calls the adapter to add each rewritten
  statement (§5.8) — and the Rust inside such a callback interns too. The discipline tells that legitimate nesting
  from a re-entry by capability, not by inference: a critical section hands its callbacks the holder's token, and
  nested interning goes through the token, never acquiring the lock. Only the acquiring entry point checks the
  thread's holder flag, and an acquisition by a thread that already holds the lock is the reentrancy the tripwire
  refuses as a typed fault, never left to deadlock. In its true extent: no top-level engine operation — on any
  control, the engine's own or a new one — may start from inside a callback; the token reaches nested interning
  and nothing else, so an `@`-function that opened a second agent to solve a subproblem would be refused.
  Nesting a solve inside a callback is a reserved seam. Authority is explicit, never ambient (specification §12.3,
  criterion 3).
- **A bounded wait.** The pinned engine cannot interrupt grounding (§5.9), so a grounding call that never returns
  holds the lock for good. A writer on another thread therefore waits a bounded time and then fails with a
  resource-locus fault naming the operation that holds the lock: that is what makes a non-returning grounding call a
  loud error on every other thread rather than a silent process-wide wedge (`solve.md` §11.1). The bound is a named
  constant with a stated default an embedder may change.
- **Poisoning.** The lock guards no data — only the order of calls into the engine. A panic can poison it only in
  adapter code between engine calls, since every callback contains its panic before returning to the engine (§5.5),
  so no unwind leaves the engine's tables mid-write. A poisoned lock is recovered and the writer proceeds; the panic
  that poisoned it has already surfaced as a fault of its own.
- **Version scope.** The spike suite reproduces the crash at the pin, and an engine upgrade that no longer crashes
  retires the lock (specification §5.2, §10.1).

### 5.4 Symbols across the seam

- **Width.** Numbers are `i32` on both sides, so nothing is reshaped crossing the seam (`solve.md` §10.5;
  `program.md` §3.1).
- **Into the engine.** Creating an engine symbol is an interning write under §5.3. The engine's string input stops
  at the first NUL and would silently truncate, so the check for an interior NUL runs at every door where a
  `Symbol` becomes an engine symbol, before one is created: a statement holding one is refused with a Program
  fault naming it; on the assumption and external doors such a `Symbol` resolves to no atom and no engine symbol is
  created, so an assumption on it fixes an atom no answer set holds and an external assignment to it refuses
  `NotExternal` — never a truncated symbol resolved to an atom the caller did not name (`solve.md` §11.1;
  conformance obligation 17). The walk is iterative, so a deep owned symbol never
  recurses on the adapter's stack, and the engine builds the symbol from its arguments' stored hashes, recursing
  on no depth either (§5.6).
- **Out of the engine, bounded.** Reading an engine symbol is an iterative walk, so no depth recurses on the adapter's
  stack. The engine hash-conses its symbols — a symbol is a node in a shared graph — while `Symbol` is an owned tree
  (`program.md` §3.1), and `program.md` §13 leaves the unfolding to this tier. A five-line program builds `s(i+1) =
  f(s(i), s(i))`: thirty levels of it are a few dozen engine nodes and about 2³⁰ owned ones. A memo keyed on the
  engine's handle saves the walk but not the output. So the adapter sizes an unfolding before it builds it: one
  memoised pass over the shared graph computes the owned size, saturating at the limit in force, in time linear in the
  shared graph's nodes and edges. Past the limit the read refuses with a resource-locus fault — at that stream item,
  as any mid-enumeration fault does (`solve.md` §5.2); within it, the copy is linear in its output. The limit is named
  — an `UnfoldLimit`, counted in owned nodes per stream item: the owned size of everything one item carries out of
  the engine — a model, a consequence set, one call's arguments — so it bounds each item's memory. Its `DEFAULT` is
  fixed by measurement when the adapter is built, far above the largest item in the corpus, with that measurement
  and the margin recorded beside it, as `NestingLimit::DEFAULT` records its own (`syntax.md` §6.6). An embedder may
  set it either way. There is no
  stack pole here, because the walk is iterative; the limit bounds memory. It governs every door where an engine
  symbol becomes an owned value: a model's atoms and displayed terms, a native consequence set, an `@`-function's
  arguments, and the variable names a theory reports.
- **Order, owned-side.** A model's symbols arrive unsorted. The canonical order is computed on the owned side by the
  program tier's iterative `Ord`, which is the engine's order (`program.md` §3.1), settled by the program tier's
  differential (`program.md` §16), whose shapes stay within the engine-safe depth (§5.6). The adapter never sorts with
  the engine's comparator, which recurses on argument depth in the pinned source (`libgringo/src/symbol.cc`), so that
  comparing two deep symbols could exhaust the calling thread's stack; for the same reason it never prints a symbol
  through the engine.
- **Hashes.** The engine's symbol hash embeds addresses and is not stable across processes; nothing persists or
  compares it.

### 5.5 Callbacks

Every engine-to-Rust call crosses one trampoline, which catches a panic before it can unwind into C++ (undefined
behaviour), stashes it, and returns the engine's failure code; the stashed panic surfaces as an adapter-locus fault
when the enclosing engine call returns (specification §9.6; `solve.md` §11.3). The trampolines are enumerated in the
manifest beside the calls. The callbacks the adapter registers are the logger and, on the clingcon backend, the
rewrite's callback; reserved with their surfaces are the ground callback of the `@`-functions, a Rust propagator's,
and the ground-program observer's. clingcon's own propagator is C++ inside libclingcon, registered engine to engine,
so none of its callbacks crosses into Rust. Four of the five — `init`, `propagate`, `check`, and `decide` — catch
their exceptions and return failure; `undo` returns nothing and catches nothing (`libclingcon/src/clingcon.cc`), so
an exception there would unwind through the engine's propagator wrapper: an engine fault of the class §5.9 names
uncontainable. A pointer the engine lends a callback — a model, an AST node, a symbol array — is valid only for that
callback, and nothing retains it.

### 5.6 The lowering

`lower` turns the statements a door carries into clingo's AST through the builder's constructors, for the grounder to
take at its next grounding (`solve.md` §10.1, §11.1). Faithfulness is soundness: a lowering that changes the program
changes the answers, and no check above the seam can see it (`solve.md` §10.1). So:

- **The built path only.** The lowering never renders text for the engine to parse (`solve.md` §10.2).
- **Both doors.** The adapter takes Door B, the `Program`, in canonical order, and Door A, an `Admitted` parse, in
  source order (`solve.md` §10.2, §11.1). The core admits a parse only when it raises cleanly, so no recovered
  statement reaches the adapter. An engine's own format is no door: the adapter ingests no aspif stream, and an
  ingestion, when built, names every atom by its symbol or refuses the stream (`solve.md` §10.3), its posture
  stated here then, so no foreign atom identifier reaches the engine (§7.2).
- **A measured depth, in two points.** The lowering is iterative over terms of any depth, but the engine's own
  handling of what it is handed is not: the AST builder's add, grounding's term handling, and the symbol comparator
  recurse in term depth (`libgringo/src/symbol.cc`; the text parser's measured ceiling, grammar §11 D2, motivates the
  bound but is off the adapter's path). A `Program` built in Rust can be deeper than any parse admits (`program.md`
  §13). So the adapter refuses any term nested past an **engine-safe depth**, measured on those three paths at the
  pin and named, as the syntax tier's `NestingLimit` is, in two points: a `DEFAULT` whose engine calls hold on a
  two-mebibyte stack, so no embedder owes a thread, and a `CEILING` with its own required stack, for an embedder who
  needs deeper terms and makes its engine calls on a thread of that size. Both are recorded beside their measurement
  and re-measured when the pin moves. A term past it in a lowered rule or an observed fact is refused with a
  Program fault naming the statement. An assumption's atom, and an external's, need no bound: the engine builds a
  symbol from its arguments' stored hashes and finds an atom by hash and identity, recursing on no depth
  (`libgringo/src/symbol.cc`; `libgringo/gringo/domain.hh`; `solve.md` §11.1).
- **No reach.** The lowering refuses the two statements that would make the engine act beyond its input.
  `#include` names a file; the pinned engine resolves inclusion while parsing text, and its AST has no include node
  (`libclingo/clingo.h`), so the only way to honour one would be to read the file. `#script` runs a script runtime;
  a build without Python or Lua has none to run (`libclingo/src/scripts.cc` answers "support not available"), and the
  refusal stands regardless. An embedder that wants inclusion resolves it itself, under its own file policy, and
  hands the adapter the included program.
- **No silent calls.** An `@`-call on a backend that does not evaluate `@`-functions is refused, and once they are
  realised, one whose name no registered function answers: the engine would log it as an undefined operation and drop
  every instance of its rule (`libclingo/src/scripts.cc`), an unsound answer (§2.4).
- **Theory atoms** are resolved to their theory before anything else touches them — the program's own `#theory`
  definitions first, then the evaluated theories' vocabularies — and an atom of a theory the backend does not evaluate
  is refused (`solve.md` §4.1). They lower through the AST's theory nodes; on the clingcon backend every statement then
  passes through clingcon's rewrite before the builder, and must (§5.8).
- **Refusals name their statement.** Each refusal above is a Program fault naming the refused statement — the
  occurrence, through Door A — located where the statement carries a parsed origin; a statement constructed in Rust
  has none, and its fault still names it (`solve.md` §5.4).
- **Held.** The differential against the out-of-band clingo package holds faithfulness over a corpus that includes
  non-ground rules and aggregates, feeding the package only programs the tiers accept (`solve.md` §13.2) — rendering
  is the oracle's input, out of band, and the prohibition is on the engine seam, not the oracle. A scaling tripwire
  holds the lowering linear in the program each backend reads — the program on the clingo backend, the unpooled
  program on the clingcon backend, whose rewrite unpools each statement (`solve.md` §10.1, §13.3).

### 5.7 Solving

- **Borrowed.** The solve handle exclusively borrows its control, so no other call reaches a control mid-solve, and a
  second concurrent solve — undefined behaviour in the engine — does not compile.
- **Closed.** The handle is closed on every path, through `Drop`; the engine refuses the next solve over an unclosed
  handle. A satisfiable search is drained before its result is read, because `get` does not drain it.
- **Copied out.** A model's memory is reused when the search resumes, so its symbols are copied out — under §5.4's
  bound — before the next pull.
- **Assumptions** cross as program literals resolved through the engine's atom table; the adapter never passes a
  literal it did not resolve, and an assumption on an atom the table does not hold — an interior-NUL `Symbol` among
  them (§5.4) — fixes an atom no answer set holds, without reaching the engine as a literal.
- **Externals.** `assign_external` refuses an atom that is not an external. The pinned engine ignores it silently,
  which would let the knowledge base and the engine disagree without a trace (`solve.md` §4.2; the conformance
  suite's check, `solve.md` §13.1).
- **Multi-shot.** Grounding and the engine's automatic cleanup invalidate its atom iterators between steps, so the
  adapter re-reads what it needs after each step; a rebuild resets the control and replays the retained state
  (`solve.md` §6.2). The engine searches only what was grounded, grounds a part over the atoms grounded before it,
  instantiates a statement lowered into a grounded part only when that instance is grounded again, and resets the
  externals a part declares whenever it grounds the part again; so the adapter grounds what is pending — `base`'s
  statements first, then each grounded instance with statements pending — before anything else reads it, restores
  the external values it holds after each grounding it performs, and refuses an instance that matches no lowered
  part, which the engine would ground as nothing and report nowhere (`solve.md` §4.1, §11.1).
- **Cancellation**, when realised, reaches the backend-owned slot and never a pointer into a freed control
  (`solve.md` §4.1). Until then the adapter declares `cancellation` false, and a budgeted request over it refuses
  (`solve.md` §6.3).

### 5.8 The clingcon layer

clingcon's public interface extends a clingo control; it is not an engine of its own (`libclingcon/clingcon.h`). The
clingcon backend is the clingo adapter with clingcon's theory registered, specified in `solve.md` §11.1 and §11.3;
this section states what it costs in trust. The adapter:

- **configures the theory before registering it** — a registered theory can be neither reconfigured nor
  unregistered (`libclingcon/clingcon.h`), so its configuration is fixed for the backend's lifetime and a rebuild
  recreates it — and pins every key that bears on the program's meaning (`solve.md` §11.1): `min-int` and
  `max-int` at clingcon's defaults, 32-bit values bounded to ±(2³⁰−1) (`libclingcon/clingcon/base.hh`), since that
  range is the domain of every variable no `&dom` bounds; `shift-constraints` on, clingcon's default, since it
  decides what a constraint in an integrity constraint's body means; and `translate-opt` at 0, so a theory
  objective stays inside the theory. Two more are pinned for assurance — `check-solution` on and `check-state`
  off — and the rest shape the search and not the model set, at clingcon's defaults but for `order-heuristic`,
  pinned to none (below). `split-all` is among them: at a total assignment clingcon splits an unassigned
  variable's domain, the first or, with the key on, all of them, and accepts a model only once every variable is
  assigned (`libclingcon/src/solver.cc`);
- **registers it.** clingcon parses its fixed `#theory` definition into the control's `base` part through
  `clingo_control_add` and registers its propagator. The text is libclingcon's own and carries no input — clingcon's
  act, not a themelios render-then-parse — and it is an interning write, made under the discipline (§5.3). A program
  that defines the theory itself is refused, since the backend registers that definition;
- **passes every lowered statement through `clingcon_rewrite_ast`** before the builder, once `lower` has resolved
  its theory atoms (§5.6), since the rewrite matches atoms by name. The rewrite is mandatory: the registered
  definition names only the rewritten atoms, so an unrewritten `&sum` has no definition. It also unpools each
  statement, so the adapter's share of the lowering is the unpooled program (§6). Its callback,
  called once per rewritten statement, crosses the trampoline (§5.5), and the rewrite and its callback are one
  interning critical section: the callback adds under the lock its caller holds, through the holder's token (§5.3);
- **calls `clingcon_prepare`** between grounding and solving, only once the request's model count is pinned, so the
  one thing `prepare` does — set an unset count to "all models" when a `&minimize` or `&maximize` is present
  (`libclingcon/src/clingcon.cc`) — never applies;
- **reads each model's values while the model is current** — after the engine hands it out, before the search
  resumes — through `clingcon_assignment_*`, for the thread that found it, into `TheoryAssignments` under
  `Theory::IntegerConstraints` (`solve.md` §4.1, §5.4); clingcon's own theory name stays inside the adapter.

Its obligations:

- **Values stay out of the answer set.** Constraint values reach `TheoryAssignments`, a component distinct from the
  answer set (`solve.md` §5.4). The adapter never calls `clingcon_on_model`, which in the pinned source extends the
  model with `__csp(Var, Value)` and `__csp_cost` atoms for display and tightens a theory objective's bound
  (`libclingcon/src/propagator.cc`). The engine keeps extended symbols on a separate channel, appended to a model's
  symbols only under the `theory` show bit (`libclingo/clingo/clingocontrol.hh`), and the adapter reads a model's
  answer set under the all-atoms selection and its displayed terms under the term selection, neither of which carries
  an extended symbol: a user's atom named `__csp` stays the user's, and nothing is filtered out. Left uncalled, the
  hook also leaves a theory objective inert — its bound is set nowhere else — so `solve` enumerates with the
  objective off, as it does under `#minimize` (`solve.md` §5.2); a theory objective's optimum is realised with
  optimization, which is reserved.
- **Named variables only.** `clingcon_assignment_next` visits every index up to the largest named variable, the
  translation's unnamed auxiliaries included; `clingcon_assignment_has_value` tests whether an index has a name; and
  `clingcon_get_symbol` on an unnamed index is undefined behaviour in a release build — it asserts, then dereferences
  an empty optional (`libclingcon/src/clingcon.cc`). In multi-shot a later step's named variable can follow an
  earlier step's auxiliaries. The adapter names a variable only after `has_value` confirms the index has one.
- **A process-global latch.** The propagator descriptor `clingcon_register` installs is a function-local static, so
  the first registration in a process fixes, for every later one, whether clingcon's decision hook is installed
  (`libclingcon/src/clingcon.cc`). The adapter pins the order heuristic to none on every registration, so the latch is
  always set the same way.
- **Drop order.** The control holds the propagator's state and clingcon offers no unregister, so the theory outlives
  the control it is registered on: the adapter frees the control before destroying the theory on every path, and a
  rebuild makes a fresh control and a fresh theory.
- **Checked integers.** clingcon's arithmetic raises on overflow, and an out-of-range constant or coefficient raises
  when the propagator initializes (`libclingcon/clingcon/base.hh`, `libclingcon/clingcon/util.hh`). The C interface
  returns those as failures, which surface as engine-locus faults — unlocated, since the engine reports no source
  position — never as wrapped values. The door is the first solve after a `lower` that succeeded, where the
  propagator initializes (`solve.md` §11.1).
- **One experience.** A theory-free program on the clingcon backend yields the clingo backend's models, each with an
  empty assignment; the spike suite holds that claim at the pin (specification §9.5). The out-of-band clingcon oracle
  differences the backend's models and values, reading values with the same has-a-name guard — never through clingo's
  own Python `Theory.assignment`, which iterates without it (`solve.md` §11.2, §13.2).

### 5.9 Resource exhaustion and engine faults

The engine runs in the host process, and three of its failure modes cannot be contained there.

- **Grounding is unbounded in memory and time.** A program can ground to a size exponential in its text, or forever
  (`p(0). p(X+1) :- p(X).`), and the pinned engine's interrupt reaches the solver only — `ClingoControl::interrupt`
  calls clasp's (`libclingo/src/clingocontrol.cc`) — so grounding cannot be cut short from another thread. The
  analysis tier's `is_safe() && Holds` gate (§3.4) rejects unbounded term growth before grounding and is the
  adapter-independent pre-check; integer growth through aggregates lies outside it.
- **Solving can take exponential time.** Once cancellation is realised, a caller's handle and the core's budget timer
  cut a solve short (`solve.md` §6.3); until then nothing in-process does.
- **An engine crash ends the process.** A stack overflow in the engine's recursive processing of a deep term — whether
  the adapter handed it over or the grounder built it and then compares it — an abort, or a latent defect:
  no trampoline catches any of them.

So the adapter's defense stops at the seam: every error the engine reports becomes a typed fault; no call the adapter
makes can end the process (§5.2); no read can grow past its bound (§5.4); nothing the adapter hands the engine exceeds
the measured depth (§5.6). Past that, **at the service boundary the only sound defense is process isolation**: run
engine work in a process the embedder can kill, under operating-system limits on memory and CPU time and a
wall-clock supervisor, and read an abnormal exit as an engine fault. themelios supplies the bounds it can state; the
isolation is the embedder's (§6).

### 5.10 Concurrency

A control is a raw engine handle, `!Send` and `!Sync`, and runs one solve at a time, which the borrow enforces
(§5.7). Controls on different threads are independent but for interning, which §5.3 serializes. The adapter pins the
engine to one solving thread by configuration, so every callback runs on a known thread (specification §9.6);
multi-threaded solving is a reserved seam. A cancellation handle is `Send + Sync` and touches only the backend-owned
slot (`solve.md` §4.1).

### 5.11 The adapter, in the three dispositions

- **Defends.** Every engine error as a typed fault; every callback's panic contained; no call of its own that ends
  the process; every read bounded; every depth it hands the engine measured; both doors taken, no recovered
  statement crossing; no reach to files or scripts; no undefined read of clingcon's unnamed variables; no silent
  no-op it knows of — a non-external assignment and an instance of no lowered part refused; the model set pinned
  against engine and theory defaults; honest capabilities, checked by the conformance suite; engine versions fixed
  at build.
- **Trusts.** The pinned engines' correctness within their characterized divergences; the C++ runtime; the
  toolchain.
- **Embedder supplies.** Process isolation at the service boundary (§5.9); a thread of the engine-safe `CEILING`'s
  required stack, only if it raises the depth to that point (§5.6); trusted extensions only (§4); and no direct
  use of the `-sys` bindings, which are the adapter's alone — a second caller of them in the process shares the
  engine's process-global state outside the interning discipline and reaches every call the manifest excludes. The
  facade adds no surface of its own beyond enabling the adapter by default (§5.1).

---

## 6. What an embedder supplies, by context

Each context adds to the one before it and inherits the rest.

- **The developer tool.** The file door's `DEFAULT` nesting on any thread, and `CEILING` only on a
  `REQUIRED_STACK_BYTES` thread (§3.2). The adapter's engine-safe depth likewise: its `DEFAULT` holds on the
  platforms' default stacks and asks for no thread, and its `CEILING` needs a thread of its own required stack
  (§5.6). For a solve that runs long, the line falls between search and grounding: a worker stuck in search burns
  its CPU and keeps its control until cancellation is realised, and can be abandoned; a worker stuck in grounding
  holds the process's interning lock for good (§5.3), so every later engine call in the process fails after the
  bounded wait — a tool that may meet such input runs engine work where it can be killed, as the service does, or
  accepts a restart (§5.9).
- **The batch pipeline** adds the job's own timeout and memory limit around any solving; the `is_safe() && Holds`
  gate before grounding contributed programs (§3.4); and `Inconclusive` and `Unknown` read as not passing (§2.5).
- **The service boundary** adds source admitted only through the syntax tier, under a request-size cap far below
  `Source::MAX_LEN` (§3.1, §3.2); a bound on pooled positions, which bounds `unpool`, `Analysis::of`, and the
  clingcon backend's lowering alike — or those run under the isolation below (§3.3, §3.4, §5.6); all engine work in a
  separate process under memory and CPU limits and a wall-clock supervisor, an abnormal exit read as an engine fault,
  and one process per tenant where one tenant's crash must not reach another (§5.9) — and where one tenant's
  grounding must not stall another's, since a process grounds one program at a time: every interning writer waits on
  the one lock, so the bounded wait is sized against the longest grounding the embedder admits (§5.3, §5.10);
  readings streamed, or capped by count, until model-count caps land (§3.6); inclusion resolved under its own file
  policy, since the adapter refuses `#include` (§5.6); a program's parts read before lowering wherever a backend's
  treatment of them matters (§3.6); and no extension a caller chooses (§4).

---

## 7. Residual risks and version scope

### 7.1 The register of engine claims

Every claim the designs of record make about the pinned engines' behaviour — this document's §5 and `solve.md`'s —
holds of clingo 5.8.2 and clingcon 5.2.1 as their pinned sources build, and this table is the one register of them:
exhaustive over those claims, each with a stable name, the sections that cite it, and its holder — *held* by a case
that fails if the claim does, or *read* in the named source at that version, where no suite can exercise it without
ending the process or where it is a fact about the code's shape. The spike suite holds every claim marked *held* by a
spike, each spike case carrying the claim's name, and `solve.md` §13.2 cites this register rather than listing its
own. An engine upgrade re-runs the held claims and re-reads the read ones, re-establishing each compensation's
necessity or retiring it (specification §5.2, §10.1). A claim cited without a row here is a defect of this document
(§8).

| Name | Claim | Cited at | Holder |
|---|---|---|---|
| `build-defaults` | The engines' build defaults detect a script runtime and build the applications. | §5.1 | *read*, the engines' CMake files; *held* by the build script's flags and version check |
| `thread-local-errors` | The engine's error state is thread-local, unless `CLINGO_NO_THREAD_LOCAL` is defined, which the `-sys` build leaves undefined. | §5.2 | *read*, `libclingo/src/control.cc`; *held* by the race harness |
| `process-ending-calls` | `clingo_solve_handle_wait` with a positive timeout outside asynchronous mode, and a solve-event callback returning `false` on a non-model event, end the process. | §5.2 | *read*, `libclingo/src/control.cc` |
| `null-logger-stderr` | A null logger writes the engine's messages to the process's standard error. | §5.2 | *read*, `libclingo/src/control.cc` |
| `message-classes` | Each message class means what §5.2 reads it to mean; *variable unbounded* is raised nowhere. | §5.2 | *read*, `libclingo/clingo.h` and the sources §5.2 cites per class |
| `undefined-operation` | An unanswered `@`-call and an undefined operation are logged and their instances dropped. | §5.2, §5.6; `solve.md` §11.1 | *held*, the undefined-operation spike |
| `script-registry` | `clingo_register_script` writes a process-global registry. | §5.2 | *read*, `libclingo/src/control.cc` |
| `interning-crash` | Concurrent interning crashes the engine. | §5.3; `solve.md` §10.5, §11.1 | *held*, the interning spike |
| `model-read` | Reading a model's symbols copies existing handles out, interning nothing and taking no lock. | §5.3 | *read*, `libclingo/src/control.cc` (`clingo_model_symbols`), `libgringo/src/output/output.cc` (`OutputBase::atoms`); *held* by the race harness |
| `hash-consing` | The engine hash-conses its symbols, and its symbol hash embeds addresses and is not stable across processes. | §5.4; `solve.md` §11.1 | *read*, `libgringo/src/symbol.cc`; the sharing *held* by the unfold tripwire |
| `symbol-identity` | Creating a symbol and finding an atom recurse on no depth: a symbol is built from its arguments' stored hashes, compared by identity, and found by hash. | §5.4, §5.6; `solve.md` §11.1 | *read*, `libgringo/src/symbol.cc`, `libgringo/gringo/domain.hh` |
| `nul-terminated-input` | The engine's string input stops at the first NUL. | §5.4; `solve.md` §11.1 | *read*, `libclingo/clingo.h`; *held* by conformance obligation 17 |
| `symbol-comparison` | The engine's symbol comparison recurses in argument depth. | §5.4, §5.6; `solve.md` §11.1 | *read*, `libgringo/src/symbol.cc`; measured by the engine-safe depth |
| `symbol-printing` | The engine's symbol printing recurses in depth. | §5.4 | *read*, `libgringo/src/symbol.cc` |
| `lent-pointers` | A pointer the engine lends a callback is valid only for that callback. | §5.5 | *read*, `libclingo/clingo.h` |
| `term-recursion` | The AST builder's add and grounding's term handling recurse in term depth. | §5.6; `solve.md` §11.1 | *read* in `libgringo`; *held* by the engine-safe depth's measurement |
| `no-include-node` | The engine resolves `#include` only while parsing text, its AST having no include node, and a build without a script runtime runs no `#script`. | §5.6; `solve.md` §11.1 | *read*, `libclingo/clingo.h`, `libclingo/src/scripts.cc` |
| `builder-add-partial` | `clingo_program_builder_add` can fail partway, on an allocation, leaving the statements added before it. | `solve.md` §4.1 | *read*, `libclingo/clingo.h` |
| `handle-lifecycle` | The engine refuses the next solve over an unclosed handle, `get` does not drain a satisfiable search, and a model's memory is reused when the search resumes. | §5.7 | *held*, the solve path's tests and the leak harness |
| `iterator-invalidation` | Grounding and automatic cleanup invalidate atom iterators between steps. | §5.7 | *held*, the conformance suite's multi-shot cases |
| `concurrent-solve` | A second concurrent solve on one control is undefined behaviour. | §5.7 | *read*, `libclingo/clingo.h`; no test can exercise it, and the borrow makes it unwritable |
| `external-noop` | `assign_external` of a non-external does nothing, and so does an assignment before the external's part is grounded. | §5.7; `solve.md` §11.1 | *held*, the external spike |
| `external-reset` | Grounding a part again resets every external the part declares. | §5.7; `solve.md` §4.1, §11.1 | *held*, the external spike |
| `search-covers-grounded` | A search covers only what was grounded. | §5.7; `solve.md` §4.1, §11.1 | *held*, the multi-shot spike |
| `ground-order` | A part grounded ahead of the base it reads misses the base's atoms. | §5.7; `solve.md` §4.1, §11.1 | *held*, the multi-shot spike |
| `late-statement` | A statement lowered into a grounded part is instantiated only when that instance is grounded again, for that instance. | §5.7; `solve.md` §4.1, §11.1 | *held*, the multi-shot spike |
| `reground-rules` | Grounding a part again re-emits rules for statements it had instantiated, leaving the answer sets as they were. | `solve.md` §11.1 | *held*, the multi-shot spike |
| `parts-by-arity` | A grounding selects parts by name and arity, and an instance of no declared part grounds nothing and reports nothing. | §5.7; `solve.md` §6.2, §11.1 | *read*, `libgringo/src/input/program.cc`; *held* by conformance obligation 12 and the multi-shot spike |
| `failed-grounding` | A grounding opens its output step before it checks the program and, past the check, streams its instantiation into the solver, so a failed grounding leaves state only a fresh control clears. | `solve.md` §4.1 | *read*, `libclingo/src/clingocontrol.cc`; *held*, the failed-grounding spike |
| `single-shot-run` | With no `main` script and no `#include <incmode>.`, the authority's run grounds `base` and solves. | `solve.md` §6.3 | *read*, `libclingo/src/clingocontrol.cc` (`ClingoControl::main`) |
| `time-limit-alarm` | `--time-limit` is a wall-clock alarm armed before grounding begins. | `solve.md` §6.3 | *read*, `libpotassco/src/application.cpp` |
| `arming-window` | The engine's interrupt cuts the active call or the following one; the adapter's slot forwards a pull only while a search is active. | `solve.md` §4.1 | *held*, the arming spike, with the cancellation increment |
| `display-rule` | A signature-form `#show` turns on the display filter; the term form adds a statement and restricts nothing. | `solve.md` §5.1 | *read*, `libgringo/src/input/programbuilder.cc`, `libgringo/gringo/output/output.hh`; *held*, the display spike |
| `consequence-premise` | The consequence search tracks the displayed atoms, or a `#project` directive's, unless neither is lowered. | `solve.md` §4.1 | *read*, `clasp/src/cb_enumerator.cpp`, `clasp/clasp/shared_context.h`; *held*, the consequence spike |
| `all-atoms-fidelity` | The all-atoms selection omits an atom with no solver literal, dropping no true atom in a single-shot solve. | `solve.md` §13.2 | *read*, `libgringo/src/output/statements.cc` (`Translator::atoms`); *held*, the all-atoms spike, which characterizes it across a multi-shot cleanup |
| `extended-symbols` | Extended model symbols appear only under the `theory` selection. | §5.8; `solve.md` §11.1 | *read*, `libclingo/clingo/clingocontrol.hh`; *held* by the conformance suite's answer-set reads |
| `aspif-naming` | An aspif stream names an atom only where it displays one, and `clingo_backend_add_atom` records an atom's symbol. | `solve.md` §10.3 | *read*, `clasp/libpotassco/potassco/aspif.h`, `libclingo/clingo.h`; *held* by the aspif spike once an ingestion is built |
| `grounding-uninterruptible` | Grounding cannot be interrupted: `ClingoControl::interrupt` reaches the solver alone. | §5.9; `solve.md` §10.1, §11.1 | *read*, `libclingo/src/clingocontrol.cc` |
| `clingcon-fixed` | A registered theory can be neither reconfigured nor unregistered, so the theory outlives its control. | §5.8; `solve.md` §11.1, §11.3 | *read*, `libclingcon/clingcon.h`; *held* by the leak harness's drop paths |
| `clingcon-registration` | Registration parses the fixed definition through `clingo_control_add`. | §5.2, §5.8 | *read*, `libclingcon/src/clingcon.cc` |
| `clingcon-rewrite-callback` | The rewrite calls back once per rewritten statement, within its own call. | §5.3, §5.8 | *read*, `libclingcon/src/clingcon.cc` |
| `clingcon-rewrite-mandatory` | The registered definition names only the rewritten atoms, so the rewrite is mandatory. | §5.8; `solve.md` §11.1 | *held*, the clingcon spikes |
| `clingcon-range` | Values are 32-bit, bounded to ±(2³⁰−1). | §5.8; `solve.md` §11.1 | *read*, `libclingcon/clingcon/base.hh` |
| `clingcon-checked` | Arithmetic raises on overflow and on an out-of-range constant, when the propagator initialises. | §5.8; `solve.md` §11.1 | *read*, `libclingcon/clingcon/util.hh`, `base.hh`; *held*, the out-of-range spike |
| `clingcon-model-hook` | The model hook extends the model with `__csp` and `__csp_cost` atoms and tightens a theory objective's bound. | §5.8; `solve.md` §11.1 | *read*, `libclingcon/src/propagator.cc` |
| `clingcon-assignment-read` | An assignment read while the model is current equals the value the engine's own display reports. | §5.8; `solve.md` §11.1 | *held*, the clingcon spikes |
| `clingcon-objective-inert` | A theory objective, with the model hook uncalled, leaves the model set as it is without the objective. | §5.8; `solve.md` §11.1 | *held*, the clingcon spikes |
| `clingcon-prepare` | `prepare` sets an unset model count under a theory objective and does nothing else. | §5.8; `solve.md` §11.1 | *held*, the clingcon spikes |
| `clingcon-unnamed-index` | `clingcon_get_symbol` on an unnamed index is undefined behaviour, and an assignment read skips unnamed indices across multi-shot steps. | §5.8; `solve.md` §11.3 | *read*, `libclingcon/src/clingcon.cc`; *held*, the clingcon spikes |
| `clingcon-theory-free` | A theory-free program on the clingcon backend yields the clingo backend's models, each with an empty assignment. | §5.8; `solve.md` §11.1 | *held*, the clingcon spikes |
| `clingcon-decide-latch` | The first registration in a process fixes whether the decision hook is installed for every later one. | §5.8; `solve.md` §11.3 | *read*, `libclingcon/src/clingcon.cc` |
| `clingcon-undo` | The propagator's `undo` catches nothing, so an exception there would unwind through the engine. | §5.5 | *read*, `libclingcon/src/clingcon.cc` |
| `clingcon-keys` | `shift-constraints` decides a body constraint's meaning, `translate-opt` keeps an objective in the theory, and `split-all` shapes only the search. | §5.8; `solve.md` §11.1 | *read*, `libclingcon/src/solver.cc` and the configuration sources |

### 7.2 Residual risks

- **What cannot be contained in-process.** A grounding that never returns, a search that runs exponentially long
  before cancellation is realised, and an engine crash — a deep term the grounder builds and then compares among
  them — cannot be contained inside the host process. At the service boundary the only sound defense is process
  isolation, which the embedder supplies (§5.9, §6).
- **Characterized divergences stay divergences.** The grounder wraps overflowing arithmetic where the program tier
  refuses (`program.md` §3.5; `analysis.md` §12), so a program whose ground arithmetic overflows receives the engine's
  wrapped answer: the engine reports nothing, and themelios does not see inside grounding. The engine's other repair —
  an undefined operation treated as false — is not a divergence but a refusal, because the engine reports it (§5.2).
  The safety and finiteness boundaries are recorded against the pinned binary (`analysis.md` §5, §12).
- **Silent no-ops are a class.** A call the engine accepts and then does nothing with, or quietly undoes, is a class,
  not a single case. Its known members are answered by the adapter: `assign_external` of a non-external, or of an
  external whose part is not yet grounded, and `ground` of an instance that matches no part the program declares,
  are refused or grounded first; and the reset a re-grounding gives a part's externals is undone by restoring their
  values (§5.7). The spike suite owns the class, and each further member it finds is refused or characterized here.
- **A script's run is held per adapter.** No portable check observes whether a backend runs a `#script`: the
  Potassco adapter refuses one at `lower` and its own tests hold the refusal (§5.6), and a single-shot backend
  carries one and runs nothing by the contract's text (`solve.md` §6.3), held by that backend's own tests. The
  conformance suite holds the include half of the program-content policy on every backend (`solve.md` §13.1,
  obligation 15), and only that half (§3.6).
- **A backend is trusted.** One that lies consistently is caught only by the conformance suite and the differentials,
  and only over their corpus (§3.6).
- **rowan is audited, not proven** (`syntax.md` §14).
- **Unbuilt surfaces are obligations.** Each lands with its design and is read against this document: the extension
  surfaces (§4); cancellation (§5.7); the ground-program observer; an aspif ingestion, whose posture owes what a
  foreign grounder's atom and literal identifiers can reach through the engine's backend interface and what the
  adapter validates before it calls it (§5.6); the engine-scoped statistics behind the `Statistics` trait
  (`solve.md` §5.4), a read surface over the solver's statistics tree; and a solve nested inside a callback (§5.3).

---

## 8. How this statement is held

- **The hostile-input witness** (specification §3, item 16) exercises the public surface as a service receives it —
  adversarial, malformed, absurdly deep — and every case must answer with a typed refusal or fault. It lands with the
  executed example set (`solve.md` §13.4); until then each tier's own instruments hold its share: the syntax fuzz
  crate, the depth proofs, the totality properties, and the scaling tripwires (`syntax.md` §16, `program.md` §16,
  `analysis.md` §10, `solve.md` §13.3).
- **The conformance suite** holds a backend's honesty (`solve.md` §13.1), and **the differentials** hold faithfulness
  against the out-of-band oracles (`solve.md` §13.2) — the clingcon oracle reading values with the has-a-name guard
  of §5.8.
- **The trust checks** hold the closures and the `unsafe` boundary on every change (specification §12.3).
- **The leak and race harnesses** at the adapter hold the interning discipline and the handle lifecycle
  (specification §10.1; `solve.md` §13.3); **the spike suite** holds the version-scoped engine claims marked *held*,
  each case named by its claim (§7.1).
- **The security review** reads the adapter's code against this document.

This statement fails in three ways, each a defect to repair in the same change that finds it, in the code or here,
never left between: a surface, tier, door, or extension the stack exposes with no entry here; an engine claim with no
row in the register, or a row with no holder (§7.1); and a surface that defends less than stated here.
