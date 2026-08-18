# themelios-syntax stage 2 — implementation plan

> **For the executor:** work strictly task by task, in the order given.
> Every step is a checkbox (`- [ ]`); check it only when its command has
> run and shown the expected result. Each task ends with the gate green
> (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets
> --locked -- -D warnings`, `cargo test --workspace --locked`,
> `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`)
> and one commit. Stop for review between tasks; do not read ahead and
> batch. Two review stops are fixed in advance and marked below: after
> Task 12 (the parser complete) and after Task 20 (the stage close). A
> step that cannot run as written is a defect of this plan: repair it
> mechanically where the intent is unambiguous, record the repair in
> that task's commit message, and raise it at the next stop; never
> absorb it silently. Where this plan and the design of record disagree,
> the design governs, the disagreement is a defect here, and it is
> raised at the next stop rather than resolved on the spot — an
> amendment to the design, the grammar, or the specification is never
> made from inside a task.

**Goal:** build `themelios-syntax` — the total lexer with its fusion
oracle, the lossless error-resilient tree of the one grammar under a
declared dialect, the typed diagnostics, the typed AST, comment
attachment as owned API, and the two token-stream certificates — exactly
as designed, with every stage-2 assurance instrument green.

**Architecture:** one crate over `themelios-base` and rowan 0.17.0, ten
public modules with one concern each — `dialect`, `tree`, `token`,
`lexer`, `parse`, `diagnostic`, `ast`, `attach`, `fusion`, `equiv` — the
green tree as the model and cursors as views, a hand-written lexer and
recursive-descent parser whose four self-recursive families run on one
explicit frame stack, and depth bounded at construction (syntax.md §1,
§5, §6, §12).

**Tech stack:** Rust, floor and CI pin 1.97 (rustc 1.97.1 is current
stable); `rowan = "=0.17.0"` as the one shipped dependency beyond base;
dev-dependencies `proptest`, `criterion`, and `serde_json` (the last
solely so the trust check reads `cargo metadata`); the fuzz crate on
`libfuzzer-sys` through `cargo-fuzz` on the stable toolchain (coverage
instrumentation only, no sanitizer); the differential against the
pinned authority through a `pixi` environment at the repository root
(conda-forge `clingo ==5.8.2` with its Python module); `cargo-llvm-cov`
and `cargo-mutants` as externally installed tools that never enter a
manifest.

**Design of record:** `docs/design/syntax.md` at commit `6ebfa32` —
every task derives from it and cites the sections it implements. Held
to `docs/grammar.md` at `7c03d23` (the letter the lexer and parser
answer to; §11 is the seed corpus and the divergence register) and
built on `docs/design/base.md` at `0855be0` (the base tier as shipped
in `crates/themelios-base`). Governing context: `docs/specification.md`
at `f31a60b` §5.2, §6, §10–§12. Where this plan and syntax.md disagree,
syntax.md governs and the disagreement is a defect here; where syntax.md
and the grammar or the specification disagree, they govern and the
disagreement is a defect there — raised, never absorbed.

## Postcondition

What this plan must be, stated so a review can check drift against it:

> A faithful and complete derivation of `docs/design/syntax.md` at
> `6ebfa32`: every task builds only what that document states — a
> surface, a law, or an instrument of stage 2 — every surface, law, and
> instrument it states has a task, nothing reaches past it (no reserved
> seam of syntax.md §17, no non-goal, no surface the design does not
> state), and every step is executable from this repository alone by an
> engineer holding nothing else.

This plan has failed when any of the following holds: a public item in
any task departs from the design's signatures or stated semantics; a
design surface, tree law, property law, golden case, instrument, or
standing gate from syntax.md §3–§16 has no task; a task introduces a
public surface, dependency, or behavior the design does not state; a
task resolves a disagreement among the design, the grammar, and the
specification instead of raising it; or a task's steps cannot be
executed as written by an engineer holding only this repository.

## Global constraints

Every task's requirements implicitly include all of these. Values are
copied from the design and specification; the citation is where the
argument lives.

- **The shipped closure is exactly** `themelios-base`, `rowan 0.17.0`,
  `text-size`, `rustc-hash`, `hashbrown`, `countme`, `memoffset` — held
  over Cargo's resolved graph by `tests/trust.rs`; FFI-free; the one
  build script in it is `memoffset`'s, admitted by name; this crate has
  none of its own (syntax.md §1, §14; spec §12.3, §12.5).
- **`#![forbid(unsafe_code)]`** at the crate root under the workspace's
  `unsafe_code = "deny"` (syntax.md §1).
- **`pub use themelios_base as base;`** at the crate root: the base
  vocabulary is reachable through this crate alone (syntax.md §1).
- **`rust-version = "1.97"`** through the workspace in every manifest;
  the CI pin `1.97.1` matches the floor; no `rust-toolchain.toml` (spec
  §10.1).
- **Every public value type is plain data** — `Send + Sync`, owned — with
  the one stated exception: red cursors, the typed AST wrappers, and
  `Attachment` are views (syntax.md §5.1, §12.2).
- **A parse is a pure function** of the token source's text and dialect
  and the entry point; no cache outlives a call; nothing mutates except
  through an explicit `&mut` (syntax.md §6.8, §12.1).
- **Refusal beats repair:** every refusing door refuses with exactly the
  error type syntax.md §13 names; every other public operation is
  total; nothing normalizes, truncates, or guesses (syntax.md §12.4,
  §13).
- **The recursion discipline:** call-stack recursion only where the
  grammar bounds depth; the four term families on the explicit frame
  stack; every walk this crate performs iterative or grammar-bounded;
  the tree's depth bounded at construction by `MAX_NESTING_DEPTH`
  (syntax.md §6.2, §6.6, §12.3; grammar §10).
- **`REQUIRED_STACK_BYTES` is 64 MiB** — `64 * 1024 * 1024`, ruled on
  2026-08-17 as a product choice the design leaves to the plan: eight
  times the 8 MiB main-thread default of the two supported operating
  systems, a size a language server's worker can be given without
  contortion. **`MAX_NESTING_DEPTH` is measured, not guessed** — Task 18
  measures it as the largest frame depth at which every gate walk
  survives every family on a thread of *half* the required stack (the
  headroom factor two), rounded down to a multiple of 1,000, and records
  both bounds beside both constants and in grammar §11 D2 (syntax.md
  §6.6). From Task 8 to Task 17 the constant carries a provisional value
  the rustdoc names as provisional.
- **No magic numbers:** any literal carrying meaning is a named constant
  with its intent (spec §5.2).
- **Lints denied workspace-wide as at stage 1:** `unused` (group),
  `unused_must_use`, `dead_code`, `missing_docs`, `unsafe_code`; clippy
  `pedantic` denied with the five argued exclusions already in the
  workspace manifest. Any denied lint — rustc or clippy — that fires
  unforeseen during execution is repaired in code or allowed in the
  matching lints table with its argument at the task's commit, never
  waived silently (spec §5.2, §10.1, §10.4).
- **Documentation is executable:** every public operation's rustdoc
  names its refusal type (or states totality) and its cost, matching
  syntax.md §13; contract and mechanism only, no deliberation; doc
  examples run as doctests; nothing is claimed that a test does not hold
  (spec §10.4).
- **Vocabulary:** tooling objects take the language-tooling literature's
  names, language constructs the grammar of record's; a departure
  carries its reason in place. Code comments cite
  `docs/design/syntax.md`, `docs/grammar.md`, `docs/design/base.md`, and
  `docs/specification.md` sections and nothing else.
- **Snippets are written at rustfmt's 100 columns;** where a snippet is
  wrapped shorter for the page, `cargo fmt` reflows it and the reflowed
  form is what is committed.
- **Commits:** one per task, exactly as written at the task's end; every
  commit leaves the gate green.

## File structure

```
Cargo.toml                                workspace: members gain
                                          "crates/themelios-syntax/fuzz"
pixi.toml, pixi.lock                      the pinned authority (Task 17)
.gitignore                                /target, /.pixi
.github/workflows/gate.yml                coverage excludes the fuzz crate
crates/themelios-syntax/
  Cargo.toml                              rowan pinned; dev-deps; feature
                                          `differential`; the bench
  src/lib.rs                              crate docs, forbid, base re-export,
                                          the ten module declarations
  src/dialect.rs                          §3 Dialect
  src/tree.rs                             §4.1 SyntaxKind; §5.2 Asp, aliases,
                                          re-exports; §5.3 conversions;
                                          §5.4 TokenRole, role
  src/token.rs                            §4.2–§4.3 Token, LexMode,
                                          TokenSource, the law checker
  src/lexer.rs                            §4.4–§4.6 Lexer; the error-token
                                          classifier the parser reads
  src/parse/mod.rs                        §5.5, §6.1 Parse, EntryPoint,
                                          the entry points; §6.6 constants
  src/parse/machine.rs                    the parser core: token cursor,
                                          builder, trivia, recovery,
                                          aspif, docs
  src/parse/terms.rs                      §6.2 the frame loop
  src/parse/statements.rs                 §6.3 rules, heads, bodies,
                                          aggregates, conditional
                                          literals, the query
  src/parse/theory.rs                     §6.3 theory atoms, definitions
  src/parse/directives.rs                 §6.3 weak constraints, optimize,
                                          directives, annotations, script
  src/diagnostic.rs                       §7 SyntaxError and its roster
  src/ast/mod.rs                          §8 the macro, roots, enums, traits
  src/ast/nodes.rs                        §8.2 the wrappers and accessors
  src/ast/tokens.rs                       §8.3 token wrappers and values
  src/attach.rs                           §9 attachment
  src/fusion.rs                           §10 the oracle, lex_mode_of
  src/equiv.rs                            §11 sequences, certificates,
                                          canonical spelling
  tests/trust.rs                          the trust checks over cargo
                                          metadata; plain-data assertion
  tests/lexer_laws.rs                     §16 lexer and token-source laws
  tests/tree_laws.rs                      §16 the four tree laws, dialect
                                          neutrality, incompleteness
  tests/oracle_laws.rs                    §16 oracle laws, the mode law
  tests/attach_laws.rs                    §16 attachment laws
  tests/equiv_laws.rs                     §16 certificate laws, corollary,
                                          canonical spelling
  tests/ast_completeness.rs               §16 the roster is covered
  tests/golden.rs, tests/golden/**        §16 diagnostics corpus, tree
                                          dumps, attachment dumps,
                                          recovery shapes
  tests/corpus.rs, tests/corpus/**        §16 the vendored corpus and its
                                          expectations
  tests/depth_gate.rs                     §16 the depth gate (subprocess)
  tests/differential.rs                   §16 the differential and the
  tests/differential/authority.py         tree-sitter cross-check
                                          (feature `differential`)
  tests/scaling_shape.rs                  §16 shape assertions in the gate
  benches/scaling.rs                      criterion, out of band
  examples/comments_as_data.rs            the four witness seeds (§16),
  examples/diagnostics_quality.rs         run by the gate
  examples/hostile_input.rs
  examples/asp_core_2.rs
  fuzz/Cargo.toml, fuzz/fuzz_targets/     the fuzz crate (workspace
  fuzz/corpus/**                          member), corpus committed
```

The public surface is exactly the ten modules and the base re-export;
`parse` and `ast` are directories of private submodules under one public
module each — the design's geography, with the parser's size split by
concern behind it.

## The task sequence, and where the two stops fall

Dependency order honoring spec §11 (lexer and oracle; parser, tree, and
attachment; typed AST and equivalence; fuzzing in the first weeks):

| task | builds | design |
|---|---|---|
| 1 | crate scaffold, lint regime, trust checks over the resolved graph | §1, §14, §16 |
| 2 | `dialect`, `tree`: the roster, `Asp`, aliases, conversions, `role` | §3, §4.1, §5.2–§5.4 |
| 3 | `token`, `lexer`: both dialects, three modes, the four laws, the checker | §4.2–§4.6 |
| 4 | `fusion` over texts: `separator_between` | §10, §10.1 |
| 5 | the fuzz crate, lex target; corpus seeded | §16 |
| 6 | `diagnostic`: the typed value, identities, lowering | §7, Appendix B |
| 7 | the parser core: cursor, builder, trivia law, recovery, entry points, roots, aspif, docs | §5.5, §6.1, §6.3, §6.4, §6.7 |
| 8 | the frame loop: terms, restrictions, the depth refusal | §6.2, §6.6 |
| 9 | literals, heads, bodies, rules, aggregates, conditional literals, the query | §6.3, §6.7 |
| 10 | theory atoms, theory mode, `#theory` definitions | §6.3 |
| 11 | weak constraints, optimize, directives, annotations, script region | §6.3 |
| 12 | the corpus vendored; membership harness; the tree, neutrality, and incompleteness laws; recovery and diagnostics goldens; the parse fuzz targets | §3, §5.4, §6.5, §6.7, §16 |
| — | **stop: the mid-stage reading** | |
| 13 | `ast` complete: wrappers, enums, traits, token wrappers, values | §8 |
| 14 | `attach`: the policy, both forms, whitespace facts, kallos and CRLF goldens | §9 |
| 15 | `fusion` over tokens: `separator`, `lex_mode_of`, the mode law | §10.2 |
| 16 | `equiv`: sequences, certificates, `canonical_spelling` | §11 |
| 17 | the differential and the tree-sitter cross-check | §16 |
| 18 | the depth gate; the constants measured; D2 recorded | §6.6, §16 |
| 19 | scaling shapes | §16 |
| 20 | stage close: witness seeds, plain-data assertions, coverage, mutation, the failure walk | §2, §13, §16 |
| — | **stop: the stage close** | |

---

### Task 1: Crate scaffold, lint regime, and the trust checks over the resolved graph

**Files:**
- Create: `crates/themelios-syntax/Cargo.toml`,
  `crates/themelios-syntax/src/lib.rs`,
  `crates/themelios-syntax/tests/trust.rs`
- Modify: `Cargo.lock` (rowan's closure and `serde_json` resolve into it)

**Derives:** syntax.md §1 (crate facts), §14 (the pin, the closure, the
one build script), §16 (the trust checks); spec §10.1–§10.2 (gate,
floor), §12.3, §12.5.

**Interfaces:**
- Consumes: the workspace of stage 1 (`Cargo.toml`, the lint tables, the
  gate) and `crates/themelios-base` as shipped.
- Produces: the crate every later task builds inside; the trust checks
  every later task's manifest must pass; the base re-export
  `themelios_syntax::base`.

- [ ] **Step 1: Write the crate manifest**

`crates/themelios-syntax/Cargo.toml`:

```toml
[package]
name = "themelios-syntax"
version = "0.1.0"
description = "Lexer, lossless syntax tree, parser, typed AST, comment attachment, fusion oracle, and token-stream equivalence for the shared clingo/clingcon syntax and its ASP-Core-2 dialect."
edition.workspace = true
rust-version.workspace = true
license.workspace = true

# The shipped closure: the base tier, and rowan with rowan's own closure
# — the one named exception of docs/specification.md §12.5, pinned
# exactly, its audit note docs/design/syntax.md §14. tests/trust.rs holds
# the closure over Cargo's resolved graph. rowan's one feature (serde1)
# stays off.
[dependencies]
themelios-base = { path = "../themelios-base" }
rowan = "=0.17.0"

# The stage-2 instruments (docs/design/syntax.md §16), outside the
# shipped closure's claim: proptest and criterion as at stage 1, and
# serde_json solely so tests/trust.rs reads `cargo metadata`'s JSON —
# the resolved graph is the structural instrument the design names.
[dev-dependencies]
proptest = "1"
criterion = "0.7"
serde_json = "1"

[features]
# The out-of-band harnesses against the pinned authority — the clingo
# differential and the tree-sitter cross-check (docs/design/syntax.md
# §16) — which need the pixi environment at the repository root and
# never run in the gate.
differential = []

[lints]
workspace = true

[[bench]]
name = "scaling"
harness = false
```

- [ ] **Step 2: Write the crate root**

`crates/themelios-syntax/src/lib.rs`:

```rust
//! The syntax tier: a total lexer with the fusion oracle beside it; a
//! hand-written, error-resilient parser producing a lossless tree of
//! the one grammar under a declared dialect; comment attachment as
//! owned, exposed policy; a typed AST over the tree; the tier's own
//! typed diagnostics, lowering to the base model; and token-stream
//! equivalence, the certificate a layout-only or spelling-preserving
//! transformation claims.
//!
//! Design of record: `docs/design/syntax.md`; the grammar it is held to:
//! `docs/grammar.md`. Every public operation's failure semantics and
//! computational cost are stated on the operation and consolidated in
//! syntax.md §13. A parse is a pure function of its inputs; this crate
//! does no I/O, holds no global state, and hands out no structure whose
//! depth is proportional to the input's nesting.
#![forbid(unsafe_code)]

// The base tier, whole, under one name: the vocabulary every door here
// speaks — Source, ByteOffset, Span, Location, Severity, Diagnostic, the
// line index, the views — is reachable through this crate alone
// (docs/design/syntax.md §1).
pub use themelios_base as base;
```

(The ten `pub mod` declarations land one by one with their modules, in
the design's order — `dialect`, `tree`, `token`, `lexer`, `parse`,
`diagnostic`, `ast`, `attach`, `fusion`, `equiv`.)

- [ ] **Step 3: Verify the crate builds and the lock resolves**

Run: `cargo build --workspace --locked 2>&1 | tail -3 || cargo build --workspace`
Expected: the first command may fail because `Cargo.lock` lacks the new
crates; the second resolves them. Then
`cargo tree -p themelios-syntax -e normal --prefix depth`
Expected, exactly:

```
0themelios-syntax v0.1.0 (…/crates/themelios-syntax)
1rowan v0.17.0
2countme v3.0.1
2hashbrown v0.14.5
2memoffset v0.9.1
2rustc-hash v1.1.0
2text-size v1.1.1
1themelios-base v0.1.0 (…/crates/themelios-base)
```

If a version differs, the registry resolved a newer patch of a
transitive crate; the trust check below holds names and the rowan pin,
not transitive patch versions, so proceed and record the versions seen
in the commit message.

- [ ] **Step 4: Write the trust checks**

`crates/themelios-syntax/tests/trust.rs`:

```rust
//! Structural trust checks over Cargo's resolved graph
//! (docs/design/syntax.md §1, §14, §16; docs/specification.md §12.3):
//! the shipped closure is exactly the enumerated list, FFI-free, with
//! one build script admitted by name and none of this crate's own.
//! What is in the closure is a question about the resolved graph, so it
//! is read from `cargo metadata` — Cargo's own account of it — and never
//! from a manifest's text; the base tier's line scans announced this
//! move for the first non-empty closure.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The shipped closure, exactly, in the design's order
/// (docs/design/syntax.md §14).
const CLOSURE: [&str; 7] = [
    "themelios-base",
    "rowan",
    "text-size",
    "rustc-hash",
    "hashbrown",
    "countme",
    "memoffset",
];

/// The build scripts inside the closure, admitted by name: memoffset's
/// compiler-feature probe through autocfg (docs/design/syntax.md §14;
/// docs/specification.md §12.3). Its retiring condition is stated in
/// the design; when it retires, this list empties and the closure loses
/// the crate.
const BUILD_SCRIPTS_ADMITTED: [&str; 1] = ["memoffset"];

/// The pin (docs/design/syntax.md §14).
const ROWAN_VERSION: &str = "0.17.0";

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Cargo's account of the workspace, resolved under the committed lock.
fn metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(manifest_dir())
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON")
}

/// One resolved package: what the closure checks read of it.
struct Package {
    name: String,
    version: String,
    links: bool,
    has_build_script: bool,
}

fn packages(metadata: &Value) -> BTreeMap<String, Package> {
    metadata["packages"]
        .as_array()
        .expect("packages is an array")
        .iter()
        .map(|package| {
            let id = package["id"].as_str().expect("package id").to_owned();
            let has_build_script = package["targets"]
                .as_array()
                .expect("targets is an array")
                .iter()
                .any(|target| {
                    target["kind"]
                        .as_array()
                        .expect("target kind is an array")
                        .iter()
                        .any(|kind| kind.as_str() == Some("custom-build"))
                });
            let package = Package {
                name: package["name"].as_str().expect("package name").to_owned(),
                version: package["version"].as_str().expect("package version").to_owned(),
                links: !package["links"].is_null(),
                has_build_script,
            };
            (id, package)
        })
        .collect()
}

/// The ids reachable from this crate over normal dependency edges — the
/// shipped closure; dev and build edges are outside the claim
/// (docs/design/base.md §1's reading of docs/specification.md §12.5).
fn shipped_closure(metadata: &Value, packages: &BTreeMap<String, Package>) -> BTreeSet<String> {
    let nodes = metadata["resolve"]["nodes"].as_array().expect("resolve nodes");
    let node_of = |id: &str| {
        nodes
            .iter()
            .find(|node| node["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("resolve node for {id}"))
    };
    let root = packages
        .iter()
        .find(|(_, package)| package.name == "themelios-syntax")
        .map(|(id, _)| id.clone())
        .expect("this crate is in the graph");
    let mut closure = BTreeSet::new();
    let mut frontier = vec![root];
    while let Some(id) = frontier.pop() {
        for dep in node_of(&id)["deps"].as_array().expect("deps is an array") {
            let normal = dep["dep_kinds"]
                .as_array()
                .expect("dep_kinds is an array")
                .iter()
                .any(|kind| kind["kind"].is_null());
            if !normal {
                continue;
            }
            let pkg = dep["pkg"].as_str().expect("dep pkg id").to_owned();
            if closure.insert(pkg.clone()) {
                frontier.push(pkg);
            }
        }
    }
    closure
}

#[test]
fn the_shipped_closure_is_exactly_the_enumerated_list() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let names: BTreeSet<&str> = shipped_closure(&metadata, &packages)
        .iter()
        .map(|id| packages[id].name.as_str())
        .collect();
    assert_eq!(
        names,
        CLOSURE.iter().copied().collect::<BTreeSet<&str>>(),
        "docs/design/syntax.md §14: the shipped closure, exactly"
    );
}

#[test]
fn rowan_is_pinned_with_its_features_off() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let closure = shipped_closure(&metadata, &packages);
    let rowan = closure
        .iter()
        .find(|id| packages[*id].name == "rowan")
        .expect("rowan is in the closure");
    assert_eq!(packages[rowan].version, ROWAN_VERSION, "docs/design/syntax.md §14: the pin");
    let features = metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes")
        .iter()
        .find(|node| node["id"].as_str() == Some(rowan.as_str()))
        .map(|node| node["features"].as_array().expect("features").len())
        .expect("rowan's resolve node");
    assert_eq!(features, 0, "docs/design/syntax.md §14: rowan's serde1 feature stays off");
}

#[test]
fn the_closure_is_ffi_free_with_one_build_script_admitted_by_name() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let closure = shipped_closure(&metadata, &packages);
    let mut scripted = BTreeSet::new();
    for id in &closure {
        let package = &packages[id];
        assert!(!package.links, "docs/specification.md §12.3: {} links native code", package.name);
        assert!(
            !package.name.ends_with("-sys"),
            "docs/specification.md §12.3: {} is a sys crate",
            package.name
        );
        if package.has_build_script {
            scripted.insert(package.name.as_str());
        }
    }
    assert_eq!(
        scripted,
        BUILD_SCRIPTS_ADMITTED.iter().copied().collect::<BTreeSet<&str>>(),
        "docs/design/syntax.md §14: the build scripts in the closure are exactly the admitted list"
    );
}

#[test]
fn this_crate_has_no_build_script() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let this = packages
        .values()
        .find(|package| package.name == "themelios-syntax")
        .expect("this crate is in the graph");
    assert!(!this.has_build_script, "docs/specification.md §12.3: no build script of this crate's own");
    assert!(!manifest_dir().join("build.rs").exists(), "docs/specification.md §12.3: no build.rs");
}

#[test]
fn unsafe_code_is_forbidden_at_the_crate_root() {
    let lib = fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("lib.rs is readable");
    assert!(
        lib.lines().any(|line| line.trim() == "#![forbid(unsafe_code)]"),
        "docs/design/syntax.md §1: forbid, not merely deny, at the root"
    );
}

#[test]
fn rust_version_floor_is_declared() {
    let manifest = fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("manifest is readable");
    assert!(
        manifest.lines().any(|line| line.trim() == "rust-version.workspace = true"),
        "docs/specification.md §10.1: every manifest carries the floor"
    );
}
```

- [ ] **Step 5: Run the trust checks**

Run: `cargo test -p themelios-syntax --test trust`
Expected: 6 passed.

- [ ] **Step 6: Run the full gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
Expected: all four green.

- [ ] **Step 7: Commit**

```bash
git add Cargo.lock crates/themelios-syntax
git commit -m "Scaffold themelios-syntax: rowan pinned at 0.17.0, the base re-export, and the trust checks over the resolved graph"
```

---

### Task 2: The `dialect` and `tree` modules — the roster, the language, the aliases, the conversions, the role of a token

**Files:**
- Create: `crates/themelios-syntax/src/dialect.rs`,
  `crates/themelios-syntax/src/tree.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod dialect;`
  and `pub mod tree;`)

**Derives:** syntax.md §3 (the dialect), §4.1 (the kind roster), §5.2
(the language and the aliases), §5.3 (the coordinate seam), §5.4 (the
role of a token), §12.5 (`Display` on `SyntaxKind` and `Dialect`),
Appendix A.

**Interfaces:**
- Consumes: rowan 0.17.0 (`Language`, `SyntaxKind`, the cursor types,
  `GreenNode`, `TextRange`, `TextSize`), base's `ByteOffset`, `Span`.
- Produces: `dialect::Dialect { Clingo, AspCore2 }` (`Default` =
  `Clingo`, `Display` = `clingo` / `asp-core-2`); `tree::SyntaxKind`
  with `ALL`, `is_trivia`, `is_comment`, `is_keyword`, `is_token`,
  `is_node`, `Display` = the SCREAMING_SNAKE name;
  `tree::Asp` and the aliases `SyntaxNode`, `SyntaxToken`,
  `SyntaxElement`, `SyntaxNodeChildren`, `SyntaxElementChildren`,
  `Preorder`, `PreorderWithTokens`, `SyntaxNodePtr`; the re-exports
  `GreenNode`, `NodeOrToken`, `TextRange`, `TextSize`, `WalkEvent`,
  `Direction`, `TokenAtOffset`, `SyntaxText`, `AstNode`, `AstChildren`,
  `AstPtr`; `tree::{span_of, range_of, offset_of, size_of}`;
  `tree::TokenRole` and `tree::role(&SyntaxToken) -> TokenRole`; the
  crate-private `SyntaxKind::is_statement` the parser, `ast`, `attach`,
  and `fusion` read.

- [ ] **Step 1: Write the `dialect` module**

`crates/themelios-syntax/src/dialect.rs`:

```rust
//! The declared dialect (docs/design/syntax.md §3): which reading of the
//! two lexical regions the lexer applies, and whether the query
//! statement exists.

use std::fmt;

/// The declared parameterization of the one grammar (grammar §1, §6):
/// which reading of the two lexical regions — the string rule and the
/// block-comment rule — and whether the query statement exists.
/// Declared per input, never varied per consumer; the lexer and the
/// parser both read it from the token source, so the two cannot
/// disagree. Closed: a released clingo 6.x language is a third surface
/// until the grammar's upgrade protocol says otherwise (grammar §12).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Dialect {
    /// The clingo dialect — the grammar's own default (grammar §1).
    #[default]
    Clingo,
    /// The ASP-Core-2 dialect: the standard's string rule, its
    /// block-comment rule, and the query statement (grammar §6).
    AspCore2,
}

impl fmt::Display for Dialect {
    /// The dialect's name — `clingo` or `asp-core-2` — stable, being
    /// what dumps and goldens read (docs/design/syntax.md §12.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dialect::Clingo => "clingo",
            Dialect::AspCore2 => "asp-core-2",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_grammars_own() {
        assert_eq!(Dialect::default(), Dialect::Clingo);
    }

    #[test]
    fn the_names_are_stable() {
        assert_eq!(Dialect::Clingo.to_string(), "clingo");
        assert_eq!(Dialect::AspCore2.to_string(), "asp-core-2");
    }
}
```

- [ ] **Step 2: Write the failing tests for `tree`**

Append `pub mod dialect;` and `pub mod tree;` under the base re-export in
`src/lib.rs`. Create `src/tree.rs` holding only this test module (the
items come in Step 4; the module fails to compile until then, which is
the failing state):

```rust
#[cfg(test)]
mod tests {
    use rowan::{GreenNodeBuilder, Language};
    use themelios_base::span::{ByteOffset, Span};

    use super::*;

    /// A tree built by hand: `PROGRAM > RULE > [DOC_COMMENT, WHITESPACE,
    /// IDENT, DOT]`, then a stray `DOC_COMMENT` after the rule.
    fn documented_fact() -> SyntaxNode {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! a fact");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! stray");
        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    fn tokens(root: &SyntaxNode) -> Vec<SyntaxToken> {
        root.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .collect()
    }

    #[test]
    fn the_roster_is_declared_in_order_tokens_first() {
        for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(*kind as usize, index, "{kind:?} sits at its declaration index");
        }
        let first_node = SyntaxKind::ALL
            .iter()
            .position(|kind| kind.is_node() && *kind != SyntaxKind::ERROR)
            .expect("nodes exist");
        assert_eq!(SyntaxKind::ALL[first_node], SyntaxKind::PROGRAM);
        assert!(SyntaxKind::ALL[..first_node].iter().all(|kind| kind.is_token()));
        assert!(SyntaxKind::ALL[first_node..].iter().all(|kind| kind.is_node()));
        assert_eq!(SyntaxKind::ALL[first_node - 1], SyntaxKind::EOF);
    }

    #[test]
    fn error_is_one_kind_for_the_token_and_the_node() {
        assert!(SyntaxKind::ERROR.is_token());
        assert!(SyntaxKind::ERROR.is_node());
        assert!(!SyntaxKind::EOF.is_node());
        assert!(!SyntaxKind::PROGRAM.is_token());
    }

    #[test]
    fn the_predicates_answer_by_kind() {
        assert!(SyntaxKind::WHITESPACE.is_trivia());
        assert!(SyntaxKind::LINE_COMMENT.is_trivia());
        assert!(SyntaxKind::BLOCK_COMMENT.is_trivia());
        assert!(SyntaxKind::SHEBANG_COMMENT.is_trivia());
        assert!(!SyntaxKind::DOC_COMMENT.is_trivia());
        assert!(SyntaxKind::DOC_COMMENT.is_comment());
        assert!(!SyntaxKind::WHITESPACE.is_comment());
        assert!(SyntaxKind::KW_CONST.is_keyword());
        assert!(SyntaxKind::KW_NOT.is_keyword());
        assert!(SyntaxKind::KW_END.is_keyword());
        assert!(!SyntaxKind::IDENT.is_keyword());
        assert!(SyntaxKind::RULE.is_statement());
        assert!(SyntaxKind::QUERY.is_statement());
        assert!(!SyntaxKind::BODY.is_statement());
    }

    #[test]
    fn the_language_round_trips_every_kind() {
        for kind in SyntaxKind::ALL {
            assert_eq!(Asp::kind_from_raw(Asp::kind_to_raw(*kind)), *kind);
        }
        let beyond = rowan::SyntaxKind(SyntaxKind::ALL.len() as u16);
        assert_eq!(Asp::kind_from_raw(beyond), SyntaxKind::ERROR);
    }

    #[test]
    fn display_is_the_screaming_snake_name() {
        assert_eq!(SyntaxKind::L_PAREN.to_string(), "L_PAREN");
        assert_eq!(SyntaxKind::THEORY_OPTERM.to_string(), format!("{:?}", SyntaxKind::THEORY_OPTERM));
    }

    #[test]
    fn the_coordinate_seam_converts_both_ways() {
        let span = Span::new(ByteOffset::new(3), ByteOffset::new(9)).expect("ordered");
        let range = range_of(span);
        assert_eq!(u32::from(range.start()), 3);
        assert_eq!(u32::from(range.end()), 9);
        assert_eq!(span_of(range), span);
        assert_eq!(offset_of(size_of(ByteOffset::new(42))), ByteOffset::new(42));
        assert_eq!(size_of(offset_of(TextSize::new(7))), TextSize::new(7));
    }

    #[test]
    fn a_leading_doc_comment_of_a_statement_is_documentation() {
        let root = documented_fact();
        let tokens = tokens(&root);
        assert_eq!(role(&tokens[0]), TokenRole::Documentation);
        assert_eq!(role(&tokens[1]), TokenRole::Trivia);
        assert_eq!(role(&tokens[2]), TokenRole::Significant);
        assert_eq!(role(&tokens[3]), TokenRole::Significant);
    }

    #[test]
    fn a_doc_comment_outside_docs_position_is_trivia() {
        let root = documented_fact();
        let tokens = tokens(&root);
        assert_eq!(tokens[5].kind(), SyntaxKind::DOC_COMMENT);
        assert_eq!(role(&tokens[5]), TokenRole::Trivia);
    }

    #[test]
    fn a_doc_comment_after_a_significant_token_is_trivia() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! inside");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());
        let tokens = tokens(&root);
        assert_eq!(role(&tokens[2]), TokenRole::Trivia);
    }
}
```

- [ ] **Step 3: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib`
Expected: compile error — `cannot find type SyntaxKind`, `Asp`, ….

- [ ] **Step 4: Write the `tree` module**

Prepend to `src/tree.rs`, above the test module:

```rust
//! The tree's vocabulary and its rowan realization (docs/design/syntax.md
//! §4.1, §5.2–§5.4): the kind roster, the language marker, the cursor
//! aliases, the coordinate seam between rowan's `TextSize`/`TextRange`
//! and base's `ByteOffset`/`Span`, and the role of a token.

use std::fmt;

use themelios_base::span::{ByteOffset, Span};

pub use rowan::ast::{AstChildren, AstNode, AstPtr};
pub use rowan::{
    Direction, GreenNode, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset, WalkEvent,
};

/// Declares the roster: one enum, tokens first and nodes after, each in
/// the grammar of record's order (docs/design/syntax.md Appendix A),
/// with `ALL` in declaration order so a raw kind maps back by index.
macro_rules! syntax_kinds {
    (
        tokens { $( $(#[$token_meta:meta])* $token:ident, )* }
        nodes { $( $(#[$node_meta:meta])* $node:ident, )* }
    ) => {
        /// Every token and node kind of the tree. Tokens first, then
        /// nodes; within each, the grammar of record's order (Appendix A
        /// of docs/design/syntax.md is the complete roster with the
        /// production each kind realizes). `ERROR` is one kind naming
        /// both the lexical error token and the recovery node — the tree
        /// says which it is where it stands. `Debug` and `Display` are
        /// the SCREAMING_SNAKE name, the spelling dumps and goldens use.
        // The variants are the roster's own SCREAMING_SNAKE names — the
        // rowan idiom, and the spelling the goldens read — so the
        // camel-case convention is set aside here by name.
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $( $(#[$token_meta])* $token, )*
            $( $(#[$node_meta])* $node, )*
        }

        impl SyntaxKind {
            /// Every kind, in declaration order: `ALL[k as usize] == k`.
            pub const ALL: &'static [SyntaxKind] = &[
                $( SyntaxKind::$token, )*
                $( SyntaxKind::$node, )*
            ];
        }
    };
}

syntax_kinds! {
    tokens {
        /// Grammar §4.1 `WHITESPACE`; one token per run.
        WHITESPACE,
        /// Grammar §4.1 `LINE-COMMENT`.
        LINE_COMMENT,
        /// Grammar §4.1 `BLOCK-COMMENT`, nesting per dialect (grammar §6.3).
        BLOCK_COMMENT,
        /// Grammar §4.1 `SHEBANG-COMMENT`.
        SHEBANG_COMMENT,
        /// Grammar §4.1 `DOC-COMMENT` — significant in docs position,
        /// trivia elsewhere; `role` answers which (syntax.md §5.4).
        DOC_COMMENT,
        /// Grammar §4.2 `IDENTIFIER`.
        IDENT,
        /// Grammar §4.2 `VARIABLE`.
        VARIABLE,
        /// Grammar §4.2 `ANONYMOUS`, the lone `_`.
        ANONYMOUS,
        /// Grammar §4.3 `NUMBER`, all four radices; the text is preserved.
        NUMBER,
        /// Grammar §4.4 `STRING` under the dialect's rule (grammar §6.2).
        STRING,
        /// `#const` (grammar §4.5).
        KW_CONST,
        /// `#count`.
        KW_COUNT,
        /// `#defined`.
        KW_DEFINED,
        /// `#edge`.
        KW_EDGE,
        /// `#external`.
        KW_EXTERNAL,
        /// `#false`.
        KW_FALSE,
        /// `#heuristic`.
        KW_HEURISTIC,
        /// `#include`.
        KW_INCLUDE,
        /// `#inf` and `#infimum` — synonyms share the kind and keep their text.
        KW_INF,
        /// `#max`.
        KW_MAX,
        /// `#maximize` and `#maximise`.
        KW_MAXIMIZE,
        /// `#min`.
        KW_MIN,
        /// `#minimize` and `#minimise`.
        KW_MINIMIZE,
        /// `#program`.
        KW_PROGRAM,
        /// `#project`.
        KW_PROJECT,
        /// `#script`.
        KW_SCRIPT,
        /// `#show`.
        KW_SHOW,
        /// `#sum`.
        KW_SUM,
        /// `#sum+`.
        KW_SUM_PLUS,
        /// `#sup` and `#supremum`.
        KW_SUP,
        /// `#theory`.
        KW_THEORY,
        /// `#true`.
        KW_TRUE,
        /// `not` — the one reserved word; also the theory operator
        /// spelled `not` (grammar §4.5, §4.7).
        KW_NOT,
        /// `#end`, the script terminator only (grammar §4.8).
        KW_END,
        /// `.`
        DOT,
        /// `..`
        DOTDOT,
        /// `,`
        COMMA,
        /// `;`
        SEMICOLON,
        /// `:`
        COLON,
        /// `:-`
        NECK,
        /// `:~`
        WEAK_NECK,
        /// `|`
        PIPE,
        /// `(`
        L_PAREN,
        /// `)`
        R_PAREN,
        /// `[`
        L_BRACKET,
        /// `]`
        R_BRACKET,
        /// `{`
        L_BRACE,
        /// `}`
        R_BRACE,
        /// `+`
        PLUS,
        /// `-`
        MINUS,
        /// `*`
        STAR,
        /// `**`
        STAR_STAR,
        /// `/`
        SLASH,
        /// `\`
        BACKSLASH,
        /// `^`
        CARET,
        /// `&`
        AMPERSAND,
        /// `~`
        TILDE,
        /// `?`
        QUESTION,
        /// `@`
        AT,
        /// `=` and `==` — synonyms share the kind (grammar §4.6).
        EQ,
        /// `!=` and `<>`.
        NEQ,
        /// `<`
        LT,
        /// `<=`
        LE,
        /// `>`
        GT,
        /// `>=`
        GE,
        /// Grammar §4.7 `THEORY-OP`, under theory mode.
        THEORY_OP,
        /// Grammar §4.8 `SCRIPT-BODY`, under script mode.
        SCRIPT_BODY,
        /// The macro dialect's `splice` marker and operand (grammar §9);
        /// never from text.
        SPLICE,
        /// A lexical error token (syntax.md §4.5), or the recovery node
        /// holding skipped or refused input byte for byte (syntax.md
        /// §6.6, §6.7).
        ERROR,
        /// End of input: returned by a source, never in a tree.
        EOF,
    }
    nodes {
        /// Grammar §5.11 `program`; the program entry's root.
        PROGRAM,
        /// The statement entry's root (syntax.md §6.1).
        STATEMENT_FRAGMENT,
        /// The term and term-value entries' root (syntax.md §6.1).
        TERM_FRAGMENT,
        /// Grammar §5.7 `rule`, all five forms; a constraint has no head child.
        RULE,
        /// Grammar §5.7 `weak-constraint`.
        WEAK_CONSTRAINT,
        /// Grammar §5.7 `optimize-statement`; the keyword token says which.
        OPTIMIZE_STATEMENT,
        /// Grammar §5.7 `optimize-element`.
        OPTIMIZE_ELEMENT,
        /// Grammar §5.9 `show-statement`, all four forms; children say which.
        SHOW_STATEMENT,
        /// Grammar §5.9 `signature`.
        SIGNATURE,
        /// Grammar §5.9 `project-statement`.
        PROJECT_STATEMENT,
        /// Grammar §5.9 `defined-statement`.
        DEFINED_STATEMENT,
        /// Grammar §5.9 `edge-statement`.
        EDGE_STATEMENT,
        /// One `term "," term` pair of grammar §5.9 `edges`.
        EDGE,
        /// Grammar §5.9 `heuristic-statement`.
        HEURISTIC_STATEMENT,
        /// Grammar §5.9 `external-statement`.
        EXTERNAL_STATEMENT,
        /// Grammar §5.9 `const-statement`; its term under the constant
        /// restriction (syntax.md §6.2).
        CONST_STATEMENT,
        /// Grammar §5.9 `script-statement`.
        SCRIPT_STATEMENT,
        /// Grammar §5.9 `include-statement`.
        INCLUDE_STATEMENT,
        /// Grammar §5.9 `program-statement`.
        PROGRAM_STATEMENT,
        /// `"(" [ id-list ] ")"` of a program statement (grammar §5.9).
        PARAMETERS,
        /// Grammar §5.9 `theory-definition`.
        THEORY_DEFINITION,
        /// Grammar §5.9 `term-definition`.
        TERM_DEFINITION,
        /// Grammar §5.9 `op-definition`.
        OP_DEFINITION,
        /// Grammar §5.9 `atom-definition`.
        ATOM_DEFINITION,
        /// Grammar §6.1 `query` (ASP-Core-2 dialect).
        QUERY,
        /// The bracketed annotation after the dot of the four families
        /// (grammar §5.11).
        ANNOTATION,
        /// Grammar §5.6 `body-list`; also the empty body of `h :- .` and `: .`.
        BODY,
        /// Grammar §5.2 `literal`: negation tokens and one of `#true`,
        /// `#false`, `ATOM`, `COMPARISON`.
        LITERAL,
        /// Grammar §5.2 `atom`.
        ATOM,
        /// Grammar §5.2 `comparison`, the whole chain.
        COMPARISON,
        /// Grammar §5.4 `conditional-literal`, and every
        /// `literal ":" [condition]` shape: set-aggregate elements,
        /// disjunction elements with a condition.
        CONDITIONAL_LITERAL,
        /// Grammar §5.3 `condition`; present and empty when the colon is.
        CONDITION,
        /// Grammar §5.5 `disjunction`; separators as tokens.
        DISJUNCTION,
        /// Grammar §5.3 `function-aggregate` with its guards as `GUARD`
        /// children, and in body position its leading negation tokens.
        FUNCTION_AGGREGATE,
        /// Grammar §5.3 `set-aggregate` with its guards, and in body
        /// position its leading negation tokens.
        SET_AGGREGATE,
        /// Grammar §5.3 `lguard` / `rguard`.
        GUARD,
        /// Grammar §5.3 `fn-element` in body position.
        BODY_AGGREGATE_ELEMENT,
        /// Grammar §5.3 `fn-element` in head position.
        HEAD_AGGREGATE_ELEMENT,
        /// Grammar §5.8 `theory-atom`, and in body position its leading
        /// negation tokens.
        THEORY_ATOM,
        /// `"{" [ theory-elements ] "}"` (grammar §5.8).
        THEORY_ELEMENTS,
        /// Grammar §5.8 `theory-element`.
        THEORY_ELEMENT,
        /// Grammar §5.8 `theory-opterm`, flat.
        THEORY_OPTERM,
        /// `theory-op theory-opterm` after the elements (grammar §5.8).
        THEORY_GUARD,
        /// `"{" [ theory-opterms ] "}"` (grammar §5.8).
        THEORY_SET,
        /// `"[" [ theory-opterms ] "]"` (grammar §5.8).
        THEORY_LIST,
        /// The parenthesized theory-term forms (grammar §5.8).
        THEORY_TUPLE,
        /// `IDENTIFIER "(" [ theory-opterms ] ")"` (grammar §5.8).
        THEORY_FUNCTION,
        /// One precedence level's maximal chain of `term BINOP term`,
        /// flat: operands interleaved with operator tokens (syntax.md §6.2).
        BINARY_TERM,
        /// A maximal run of `UNOP` and its one operand, flat (syntax.md §6.2).
        UNARY_TERM,
        /// `"(" pool ")"` (grammar §5.1).
        POOL,
        /// Grammar §5.1 `tuple`, and each `[ terms ]` alternative of `arguments`.
        TUPLE,
        /// `"(" arguments ")"` of a function, an atom, or an external call.
        ARGUMENTS,
        /// `IDENTIFIER "(" arguments ")"` (grammar §5.1).
        FUNCTION_TERM,
        /// `"@" IDENTIFIER [ "(" arguments ")" ]` (grammar §5.1).
        EXTERNAL_TERM,
        /// `"|" abs-arguments "|"` (grammar §5.1).
        ABS_TERM,
        /// `IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"` as a term.
        CONSTANT_TERM,
        /// `VARIABLE | ANONYMOUS` as a term.
        VARIABLE_TERM,
        /// A splice in term or theory-term position (grammar §9).
        SPLICE_TERM,
    }
}

impl SyntaxKind {
    /// The kinds that are trivia wherever they stand: `WHITESPACE`,
    /// `LINE_COMMENT`, `BLOCK_COMMENT`, `SHEBANG_COMMENT`. `DOC_COMMENT`
    /// is not among them — its status is positional, and `role` is the
    /// predicate that answers it for a token. Total, O(1).
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE
                | SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::SHEBANG_COMMENT
        )
    }

    /// The comment forms: `LINE_COMMENT`, `BLOCK_COMMENT`,
    /// `SHEBANG_COMMENT`, `DOC_COMMENT`. Total, O(1).
    pub const fn is_comment(self) -> bool {
        matches!(
            self,
            SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::SHEBANG_COMMENT
                | SyntaxKind::DOC_COMMENT
        )
    }

    /// The keyword tokens: the `#`-keywords of grammar §4.5, `not`, and
    /// the script terminator `#end`. Total, O(1).
    pub const fn is_keyword(self) -> bool {
        (self as u16) >= (SyntaxKind::KW_CONST as u16) && (self as u16) <= (SyntaxKind::KW_END as u16)
    }

    /// A kind a token may carry: everything declared before the first
    /// node kind, `ERROR` and `EOF` included. Total, O(1).
    pub const fn is_token(self) -> bool {
        (self as u16) <= (SyntaxKind::EOF as u16)
    }

    /// A kind a node may carry: `PROGRAM` and every kind after it, and
    /// `ERROR`. Total, O(1).
    pub const fn is_node(self) -> bool {
        (self as u16) >= (SyntaxKind::PROGRAM as u16) || matches!(self, SyntaxKind::ERROR)
    }

    /// The statement kinds — grammar §5.11's `statement` alternatives
    /// and the query — the kinds whose leading `DOC_COMMENT` run is
    /// documentation (docs/design/syntax.md §5.4).
    pub(crate) const fn is_statement(self) -> bool {
        matches!(
            self,
            SyntaxKind::RULE
                | SyntaxKind::WEAK_CONSTRAINT
                | SyntaxKind::OPTIMIZE_STATEMENT
                | SyntaxKind::SHOW_STATEMENT
                | SyntaxKind::PROJECT_STATEMENT
                | SyntaxKind::DEFINED_STATEMENT
                | SyntaxKind::EDGE_STATEMENT
                | SyntaxKind::HEURISTIC_STATEMENT
                | SyntaxKind::EXTERNAL_STATEMENT
                | SyntaxKind::CONST_STATEMENT
                | SyntaxKind::SCRIPT_STATEMENT
                | SyntaxKind::INCLUDE_STATEMENT
                | SyntaxKind::PROGRAM_STATEMENT
                | SyntaxKind::THEORY_DEFINITION
                | SyntaxKind::QUERY
        )
    }
}

impl fmt::Display for SyntaxKind {
    /// The SCREAMING_SNAKE name, as `Debug` renders it — stable, being
    /// what dumps and goldens read (docs/design/syntax.md §12.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The rowan language marker: maps `SyntaxKind` to and from rowan's raw
/// kind. Uninhabited — a type-level tag, never a value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Asp {}

impl rowan::Language for Asp {
    type Kind = SyntaxKind;

    /// The kind a raw kind names. A raw kind this crate never produced —
    /// a green tree of another language wrapped under `Asp`, outside
    /// every contract here — names nothing of the language and reads as
    /// `ERROR`, the kind for what was not understood; the door is total.
    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::ALL.get(usize::from(raw.0)).copied().unwrap_or(SyntaxKind::ERROR)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// A node cursor over the tree — a view, `!Send` (docs/design/syntax.md §5.1).
pub type SyntaxNode = rowan::SyntaxNode<Asp>;
/// A token cursor over the tree — a view.
pub type SyntaxToken = rowan::SyntaxToken<Asp>;
/// A node or a token cursor.
pub type SyntaxElement = rowan::SyntaxElement<Asp>;
/// A node's child nodes, in order.
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<Asp>;
/// A node's child nodes and tokens, in order.
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<Asp>;
/// An iterative preorder over nodes.
pub type Preorder = rowan::api::Preorder<Asp>;
/// An iterative preorder over nodes and tokens.
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<Asp>;
/// Positional identity by kind and range, resolvable against a root.
pub type SyntaxNodePtr = rowan::ast::SyntaxNodePtr<Asp>;

/// The span of a rowan range: total, since a `TextRange`'s start never
/// exceeds its end.
pub fn span_of(range: TextRange) -> Span {
    Span::new(offset_of(range.start()), offset_of(range.end()))
        .expect("a TextRange's start never exceeds its end")
}

/// The rowan range of a span: total.
pub fn range_of(span: Span) -> TextRange {
    TextRange::new(size_of(span.start()), size_of(span.end()))
}

/// The base offset of a rowan size: total.
pub fn offset_of(size: TextSize) -> ByteOffset {
    ByteOffset::new(u32::from(size))
}

/// The rowan size of a base offset: total.
pub fn size_of(offset: ByteOffset) -> TextSize {
    TextSize::new(offset.get())
}

/// What a token is, where it stands (docs/design/syntax.md §5.4).
/// `Documentation`: a `DOC_COMMENT` in docs position — a leading child
/// of a statement node with only trivia and other `DOC_COMMENT` tokens
/// before it. `Trivia`: whitespace, the plain comment forms wherever
/// they stand, and a `DOC_COMMENT` anywhere else. `Significant`: every
/// other token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenRole {
    /// A statement's documentation.
    Documentation,
    /// Whitespace, or a comment that is not documentation.
    Trivia,
    /// Every other token.
    Significant,
}

/// The role of `token` where it stands. Total; O(preceding siblings of
/// the token).
pub fn role(token: &SyntaxToken) -> TokenRole {
    match token.kind() {
        SyntaxKind::DOC_COMMENT if in_docs_position(token) => TokenRole::Documentation,
        SyntaxKind::DOC_COMMENT => TokenRole::Trivia,
        kind if kind.is_trivia() => TokenRole::Trivia,
        _ => TokenRole::Significant,
    }
}

/// A leading child of a statement node with only trivia and doc-comment
/// tokens before it.
fn in_docs_position(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if !parent.kind().is_statement() {
        return false;
    }
    let mut earlier = token.prev_sibling_or_token();
    while let Some(element) = earlier {
        match &element {
            NodeOrToken::Node(_) => return false,
            NodeOrToken::Token(before) => {
                if !(before.kind().is_trivia() || before.kind() == SyntaxKind::DOC_COMMENT) {
                    return false;
                }
            }
        }
        earlier = element.prev_sibling_or_token();
    }
    true
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p themelios-syntax --lib`
Expected: 11 passed (2 in `dialect`, 9 in `tree`).

- [ ] **Step 6: Run the full gate**

Run the four gate commands (Task 1, Step 6).
Expected: green. If `clippy::cast_possible_truncation` fires on
`SyntaxKind::ALL.len() as u16` in the test, it is already allowed
workspace-wide; if `clippy::enum_glob_use` or another pedantic lint
fires, repair the code and record the repair in the commit message.

- [ ] **Step 7: Commit**

```bash
git add crates/themelios-syntax
git commit -m "Add the dialect and the tree's vocabulary: the kind roster, the rowan language, the aliases, the coordinate seam, and the role of a token"
```

---

### Task 3: The `token` and `lexer` modules — the door, the four laws and their checker, and the file lexer

**Files:**
- Create: `crates/themelios-syntax/src/token.rs`,
  `crates/themelios-syntax/src/lexer.rs`,
  `crates/themelios-syntax/tests/lexer_laws.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod token;`
  and `pub mod lexer;`)

**Derives:** syntax.md §4.2 (the token and the modes), §4.3 (the door,
the four laws, the checker), §4.4 (the file lexer), §4.5 (error tokens'
extents), §4.6 (cost), §12.5 (`Display` and `Error` on
`TokenSourceLawViolation`), §13; grammar §4 whole and §6.2–§6.3 (the
letter the lexer is held to), §11's lexical seeds.

**Interfaces:**
- Consumes: `tree::SyntaxKind`, `dialect::Dialect`, base's `Source`,
  `SourceId`, `ByteOffset`, `PositionRefusal`, `OffsetOutOfBounds`,
  `NotCharBoundary`.
- Produces: `token::Token<'a> { kind, text }`, `token::LexMode
  { Normal, Theory, ScriptBody }`, `token::TokenSource` (`id`,
  `dialect`, `text`, `token_at(ByteOffset, LexMode) -> Result<Token,
  PositionRefusal>`), `token::check_token_source_laws(&impl
  TokenSource) -> Vec<TokenSourceLawViolation>`,
  `token::TokenSourceLawViolation`; `lexer::Lexer<'a>` with
  `Lexer::new(&Source, Dialect)` implementing `TokenSource`; the
  crate-private `lexer::lex(text: &str, at: usize, mode: LexMode,
  dialect: Dialect) -> (SyntaxKind, usize)` the oracle (Task 4) and the
  parser (Task 7) read.

- [ ] **Step 1: Write the `token` module**

`crates/themelios-syntax/src/token.rs`:

```rust
//! Tokens, lexical modes, and the token-source door
//! (docs/design/syntax.md §4.2–§4.3): where the parser's tokens come
//! from, the four laws every source is bound by, and their checker.

use std::fmt;

use themelios_base::line::PositionRefusal;
use themelios_base::source::SourceId;
use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::tree::SyntaxKind;

/// One token as a source hands it to the parser: its kind and its text.
/// The text is a slice of the source's own text (the slice law); its
/// length is the token's extent, so no length field can disagree with
/// it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Token<'a> {
    /// The kind.
    pub kind: SyntaxKind,
    /// The text, a slice of the source's text.
    pub text: &'a str,
}

/// The lexical mode in force at a token's start — a language fact
/// (grammar §4.7, §4.8), not an implementation choice: inside a theory
/// atom's elements and guard, and at the operator positions of a
/// `#theory` definition, operator runs are one token and `not` is an
/// operator; between `#script(…)` and `#end`, nothing lexes. The parser
/// owns the modes and tells the source which one it wants for each
/// token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LexMode {
    /// Grammar §4's rules as written.
    Normal,
    /// Grammar §4.7's theory-expression lexing.
    Theory,
    /// Grammar §4.8's script region: the raw body, or `#end`.
    ScriptBody,
}

/// Where tokens come from. The file lexer is one source; the macro tier
/// is another (grammar §9: Rust's lexer is the macro dialect's lexical
/// layer, and the mapping onto the roster is the macro crate's). One
/// parser reads both.
///
/// A source is bound by four laws (docs/design/syntax.md §4.3): tiling —
/// from offset zero under `Normal` mode the tokens partition the text
/// and end at `EOF` at its length; slice — every token's text is a
/// slice of `text()` at its offset; determinism — same offset and mode,
/// same answer; refusal — `token_at` refuses exactly at offsets that are
/// not positions of the text. `check_token_source_laws` holds them.
pub trait TokenSource {
    /// The identity the host minted for this text (base §3.1).
    fn id(&self) -> SourceId;
    /// The dialect the source lexes under; the parser reads it here so
    /// lexer and parser cannot disagree.
    fn dialect(&self) -> Dialect;
    /// The whole text this source owns and tiles. Every token is a
    /// slice of it, and every span this crate hands back is in its
    /// coordinates.
    fn text(&self) -> &str;
    /// The token that begins at `at` under `mode`: the longest token the
    /// mode's rules form there, an `ERROR` token, or `EOF` with empty
    /// text at the text's end. Refuses with `PositionRefusal` exactly
    /// when `at` is not a position of the text — past its end, or inside
    /// a character. Never panics. O(the token returned).
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal>;
}

/// A breach of one of the four laws, as the checker reports it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TokenSourceLawViolation {
    /// Tiling broke at `at`: `EOF` before the text's end (`token` is
    /// `EOF`, `len` zero), a zero-length token, or a token running past
    /// the end.
    Tiling {
        /// Where the tiling broke.
        at: ByteOffset,
        /// The token the source answered there.
        token: SyntaxKind,
        /// Its length.
        len: u32,
    },
    /// The token at `at` is not the slice of the source's text there.
    Slice {
        /// The offending token's offset.
        at: ByteOffset,
    },
    /// The source answered differently when asked again at `at` under
    /// `mode`.
    Determinism {
        /// The offset asked twice.
        at: ByteOffset,
        /// The mode asked under.
        mode: LexMode,
    },
    /// The refusal law broke at `at`: `refused` is `true` where the
    /// source refused an offset it owed a token, `false` where it
    /// answered an offset that is not a position of its text.
    Refusal {
        /// The offset probed.
        at: ByteOffset,
        /// What the source did.
        refused: bool,
    },
}

impl fmt::Display for TokenSourceLawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenSourceLawViolation::Tiling { at, token, len } => write!(
                f,
                "tiling breaks at byte {}: the source answered {token} of length {len} there",
                at.get()
            ),
            TokenSourceLawViolation::Slice { at } => {
                write!(f, "the token at byte {} is not a slice of the source's text", at.get())
            }
            TokenSourceLawViolation::Determinism { at, mode } => write!(
                f,
                "the source answered differently when asked again at byte {} under {mode:?}",
                at.get()
            ),
            TokenSourceLawViolation::Refusal { at, refused: true } => {
                write!(f, "the source refused byte {}, a position it owes a token", at.get())
            }
            TokenSourceLawViolation::Refusal { at, refused: false } => {
                write!(f, "the source answered byte {}, which is not a position of its text", at.get())
            }
        }
    }
}

impl std::error::Error for TokenSourceLawViolation {}

/// The laws, checkable: tiles the source under `Normal` mode from zero,
/// checking tiling and the slice law at every token, probing
/// determinism by re-asking each token, and probing refusal once past
/// the end and once inside a multi-byte character of each token that
/// has one. Total; O(text) for a lawful source; an empty report is the
/// laws holding. What it does not exercise: `Theory` and `ScriptBody`
/// mode, which an implementor holds under its own tests over inputs its
/// parser reaches.
pub fn check_token_source_laws(source: &impl TokenSource) -> Vec<TokenSourceLawViolation> {
    let text = source.text();
    let mut violations = Vec::new();
    // A text longer than the door's coordinates can address cannot be
    // tiled by any source: reported as a tiling breach at the last
    // addressable offset.
    let Ok(end) = u32::try_from(text.len()) else {
        violations.push(TokenSourceLawViolation::Tiling {
            at: ByteOffset::new(u32::MAX),
            token: SyntaxKind::EOF,
            len: 0,
        });
        return violations;
    };
    let mut at = 0u32;
    loop {
        let offset = ByteOffset::new(at);
        let token = match source.token_at(offset, LexMode::Normal) {
            Ok(token) => token,
            Err(_) => {
                violations.push(TokenSourceLawViolation::Refusal { at: offset, refused: true });
                return violations;
            }
        };
        if source.token_at(offset, LexMode::Normal) != Ok(token) {
            violations.push(TokenSourceLawViolation::Determinism { at: offset, mode: LexMode::Normal });
        }
        if token.kind == SyntaxKind::EOF {
            if at != end {
                violations.push(TokenSourceLawViolation::Tiling {
                    at: offset,
                    token: SyntaxKind::EOF,
                    len: 0,
                });
            }
            break;
        }
        let len = u32::try_from(token.text.len()).unwrap_or(u32::MAX);
        let past_the_end = at.checked_add(len).is_none_or(|next| next > end);
        if len == 0 || past_the_end {
            violations.push(TokenSourceLawViolation::Tiling { at: offset, token: token.kind, len });
            break;
        }
        let start = at as usize;
        if text.get(start..start + token.text.len()) != Some(token.text) {
            violations.push(TokenSourceLawViolation::Slice { at: offset });
        }
        if let Some((index, _)) = token.text.char_indices().find(|(_, c)| c.len_utf8() > 1) {
            let inside = ByteOffset::new(at + u32::try_from(index).unwrap_or(0) + 1);
            if source.token_at(inside, LexMode::Normal).is_ok() {
                violations.push(TokenSourceLawViolation::Refusal { at: inside, refused: false });
            }
        }
        at += len;
    }
    if let Some(past) = end.checked_add(1) {
        let past = ByteOffset::new(past);
        if source.token_at(past, LexMode::Normal).is_ok() {
            violations.push(TokenSourceLawViolation::Refusal { at: past, refused: false });
        }
    }
    violations
}
```

- [ ] **Step 2: Write the failing lexer tests**

Append `pub mod token;` and `pub mod lexer;` to `src/lib.rs`. Create
`src/lexer.rs` holding only this test module:

```rust
// The roster's names read as the tokens they are; qualifying a hundred
// of them in these tables adds noise, not information.
#[cfg(test)]
#[allow(clippy::enum_glob_use)]
mod tests {
    use themelios_base::line::PositionRefusal;
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::token::{LexMode, TokenSource};
    use crate::tree::SyntaxKind::{self, *};

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    /// The tokens of `text` tiled under one mode and dialect, as
    /// `(kind, text)` pairs, `EOF` excluded.
    fn tile(text: &str, mode: LexMode, dialect: Dialect) -> Vec<(SyntaxKind, String)> {
        let source = admitted(text);
        let lexer = Lexer::new(&source, dialect);
        let mut at = 0u32;
        let mut tokens = Vec::new();
        loop {
            let token = lexer.token_at(ByteOffset::new(at), mode).expect("a position");
            if token.kind == EOF {
                assert_eq!(at as usize, text.len(), "EOF only at the end");
                return tokens;
            }
            assert!(!token.text.is_empty(), "no zero-length token");
            tokens.push((token.kind, token.text.to_owned()));
            at += u32::try_from(token.text.len()).expect("test text is small");
        }
    }

    fn normal(text: &str) -> Vec<(SyntaxKind, String)> {
        tile(text, LexMode::Normal, Dialect::Clingo)
    }

    fn theory(text: &str) -> Vec<(SyntaxKind, String)> {
        tile(text, LexMode::Theory, Dialect::Clingo)
    }

    fn kinds(tokens: &[(SyntaxKind, String)]) -> Vec<SyntaxKind> {
        tokens.iter().map(|(kind, _)| *kind).collect()
    }

    fn texts(tokens: &[(SyntaxKind, String)]) -> Vec<&str> {
        tokens.iter().map(|(_, text)| text.as_str()).collect()
    }

    #[test]
    fn a_rule_lexes_to_its_tokens() {
        let tokens = normal("p(X) :- q(X), X == 1.");
        assert_eq!(
            kinds(&tokens),
            [
                IDENT, L_PAREN, VARIABLE, R_PAREN, WHITESPACE, NECK, WHITESPACE, IDENT, L_PAREN,
                VARIABLE, R_PAREN, COMMA, WHITESPACE, VARIABLE, WHITESPACE, EQ, WHITESPACE, NUMBER,
                DOT
            ]
        );
    }

    #[test]
    fn numerals_follow_the_grammar_including_its_recorded_octal_oddity() {
        assert_eq!(texts(&normal("0o10")), ["0o1", "0"]);
        assert_eq!(kinds(&normal("0o10")), [NUMBER, NUMBER]);
        assert_eq!(kinds(&normal("0X1F")), [NUMBER, VARIABLE]);
        assert_eq!(kinds(&normal("007")), [NUMBER, NUMBER, NUMBER]);
        assert_eq!(kinds(&normal("0x")), [NUMBER, IDENT]);
        assert_eq!(texts(&normal("0x1F 0b101 0o7 42")), ["0x1F", " ", "0b101", " ", "0o7", " ", "42"]);
    }

    #[test]
    fn names_take_primes_and_underscores_and_the_lone_underscore_is_anonymous() {
        assert_eq!(kinds(&normal("__")), [ANONYMOUS, ANONYMOUS]);
        assert_eq!(kinds(&normal("_1")), [ANONYMOUS, NUMBER]);
        assert_eq!(kinds(&normal("_p 'a a' p'' _X X'")), [
            IDENT, WHITESPACE, IDENT, WHITESPACE, IDENT, WHITESPACE, IDENT, WHITESPACE, VARIABLE,
            WHITESPACE, VARIABLE
        ]);
        assert_eq!(kinds(&normal("not nota default")), [KW_NOT, WHITESPACE, IDENT, WHITESPACE, IDENT]);
    }

    #[test]
    fn hash_words_are_keywords_or_one_unknown_word() {
        assert_eq!(kinds(&normal("#sums")), [ERROR]);
        assert_eq!(texts(&normal("#counting")), ["#counting"]);
        assert_eq!(kinds(&normal("#end")), [ERROR]);
        assert_eq!(kinds(&normal("#sum+")), [KW_SUM_PLUS]);
        assert_eq!(kinds(&normal("#sum +")), [KW_SUM, WHITESPACE, PLUS]);
        assert_eq!(kinds(&normal("#inf #infimum #sup #supremum")), [
            KW_INF, WHITESPACE, KW_INF, WHITESPACE, KW_SUP, WHITESPACE, KW_SUP
        ]);
        assert_eq!(kinds(&normal("#minimise #maximize")), [KW_MINIMIZE, WHITESPACE, KW_MAXIMIZE]);
        assert_eq!(kinds(&normal("#")), [ERROR]);
    }

    #[test]
    fn maximal_munch_resolves_the_operators() {
        assert_eq!(kinds(&normal("...")), [DOTDOT, DOT]);
        assert_eq!(kinds(&normal("***")), [STAR_STAR, STAR]);
        assert_eq!(kinds(&normal(":-:~:")), [NECK, WEAK_NECK, COLON]);
        assert_eq!(kinds(&normal("== = != <> < <= > >=")), [
            EQ, WHITESPACE, EQ, WHITESPACE, NEQ, WHITESPACE, NEQ, WHITESPACE, LT, WHITESPACE, LE,
            WHITESPACE, GT, WHITESPACE, GE
        ]);
        assert_eq!(kinds(&normal("|()[]{}+-*/\\^&~?@,;")), [
            PIPE, L_PAREN, R_PAREN, L_BRACKET, R_BRACKET, L_BRACE, R_BRACE, PLUS, MINUS, STAR, SLASH,
            BACKSLASH, CARET, AMPERSAND, TILDE, QUESTION, AT, COMMA, SEMICOLON
        ]);
    }

    #[test]
    fn characters_that_begin_no_token_join_one_error_run() {
        assert_eq!(kinds(&normal("!")), [ERROR]);
        assert_eq!(texts(&normal("$$$")), ["$$$"]);
        assert_eq!(kinds(&normal("$x")), [ERROR, IDENT]);
        assert_eq!(kinds(&normal("$#foo")), [ERROR, ERROR]);
        assert_eq!(texts(&normal("é€p")), ["é€", "p"]);
        assert_eq!(kinds(&normal("\u{1}\u{2}p")), [ERROR, IDENT]);
        assert_eq!(kinds(&normal("'")), [ERROR]);
    }

    #[test]
    fn strings_under_the_clingo_rule() {
        assert_eq!(kinds(&normal(r#""a\nb" "a\"b" "a\\b""#)), [STRING, WHITESPACE, STRING, WHITESPACE, STRING]);
        assert_eq!(kinds(&normal(r#""a\b""#)), [ERROR]);
        assert_eq!(texts(&normal(r#"p("a\qb"). q."#)), ["p", "(", r#""a\qb""#, ")", ".", " ", "q", "."]);
        assert_eq!(texts(&normal("\"ab\ncd\"")), ["\"ab", "\n", "cd", "\""]);
        assert_eq!(kinds(&normal("\"ab\ncd\"")), [ERROR, WHITESPACE, IDENT, ERROR]);
        assert_eq!(kinds(&normal("\"abc")), [ERROR]);
        assert_eq!(texts(&normal("\"a\\\nb\"")), ["\"a\\", "\n", "b", "\""]);
    }

    #[test]
    fn strings_under_the_asp_core_2_rule() {
        let core = |text: &str| tile(text, LexMode::Normal, Dialect::AspCore2);
        assert_eq!(kinds(&core(r#""a\nb" "a\b""#)), [STRING, WHITESPACE, STRING]);
        assert_eq!(kinds(&core("\"a\nb\"")), [STRING]);
        assert_eq!(texts(&core(r#""a\" b" x"#)), [r#""a\" b""#, " ", "x"]);
        assert_eq!(texts(&core(r#""a\""#)), [r#""a\""#]);
        assert_eq!(kinds(&core(r#""a\""#)), [STRING]);
        assert_eq!(kinds(&core("\"abc")), [ERROR]);
    }

    #[test]
    fn comments_take_their_forms() {
        assert_eq!(kinds(&normal("%! doc\np.")), [DOC_COMMENT, WHITESPACE, IDENT, DOT]);
        assert_eq!(kinds(&normal("%%! x\n% ! x")), [LINE_COMMENT, WHITESPACE, LINE_COMMENT]);
        assert_eq!(kinds(&normal("#! shebang\np.")), [SHEBANG_COMMENT, WHITESPACE, IDENT, DOT]);
        assert_eq!(texts(&normal("% c\r\np.")), ["% c\r", "\n", "p", "."]);
    }

    #[test]
    fn block_comments_nest_and_silence_under_clingo_and_neither_under_asp_core_2() {
        assert_eq!(kinds(&normal("%* a %* b *% c *%p.")), [BLOCK_COMMENT, IDENT, DOT]);
        assert_eq!(kinds(&normal("%* a % *%\nb *%p.")), [BLOCK_COMMENT, IDENT, DOT]);
        assert_eq!(kinds(&normal("%* a % *% b *%")), [ERROR]);
        assert_eq!(kinds(&normal("%* %* *%")), [ERROR]);
        assert_eq!(kinds(&normal("%* %! *%")), [ERROR]);
        let core = |text: &str| tile(text, LexMode::Normal, Dialect::AspCore2);
        assert_eq!(kinds(&core("%* a % *% b *%")), [
            BLOCK_COMMENT, WHITESPACE, IDENT, WHITESPACE, STAR, LINE_COMMENT
        ]);
        assert_eq!(kinds(&core("%* %* *%")), [BLOCK_COMMENT]);
        assert_eq!(kinds(&core("%* %! *%")), [BLOCK_COMMENT]);
    }

    #[test]
    fn theory_mode_forms_operator_runs() {
        assert_eq!(kinds(&theory(":-:")), [THEORY_OP]);
        assert_eq!(texts(&theory(".. ;; :: := :~")), ["..", " ", ";;", " ", "::", " ", ":=", " ", ":~"]);
        assert!(theory(".. ;; :: := :~").iter().filter(|(k, _)| *k == THEORY_OP).count() == 5);
        assert_eq!(kinds(&theory(". ; : :-")), [DOT, WHITESPACE, SEMICOLON, WHITESPACE, COLON, WHITESPACE, NECK]);
        assert_eq!(kinds(&theory("_")), [ERROR]);
        assert_eq!(kinds(&theory("__ _a")), [ERROR, WHITESPACE, IDENT]);
        assert_eq!(kinds(&theory("not #inf #count")), [KW_NOT, WHITESPACE, KW_INF, WHITESPACE, ERROR]);
        assert_eq!(kinds(&theory("x <= 3")), [IDENT, WHITESPACE, THEORY_OP, WHITESPACE, NUMBER]);
        assert_eq!(kinds(&theory("!|@")), [THEORY_OP]);
        assert_eq!(kinds(&theory(",()[]{}")), [COMMA, L_PAREN, R_PAREN, L_BRACKET, R_BRACKET, L_BRACE, R_BRACE]);
        assert_eq!(kinds(&theory("#sum+")), [ERROR, THEORY_OP]);
        assert_eq!(kinds(&theory("$")), [ERROR]);
    }

    #[test]
    fn script_mode_answers_the_body_or_the_terminator() {
        let script = |text: &str| tile(text, LexMode::ScriptBody, Dialect::Clingo);
        assert_eq!(texts(&script("#end")), ["#end"]);
        assert_eq!(kinds(&script("#end")), [KW_END]);
        assert_eq!(kinds(&script(" x = 1 #end")), [SCRIPT_BODY, KW_END]);
        assert_eq!(texts(&script(" x = 1 #end")), [" x = 1 ", "#end"]);
        assert_eq!(kinds(&script("no end")), [ERROR]);
        assert_eq!(kinds(&script("% not a comment #end.")), [SCRIPT_BODY, KW_END, DOT]);
    }

    #[test]
    fn the_door_refuses_where_span_meets_text_as_base_refuses() {
        let source = admitted("é");
        let lexer = Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(
            lexer.token_at(ByteOffset::new(1), LexMode::Normal),
            Err(PositionRefusal::NotCharBoundary(_))
        ));
        assert!(matches!(
            lexer.token_at(ByteOffset::new(3), LexMode::Normal),
            Err(PositionRefusal::OutOfBounds(_))
        ));
        let end = lexer.token_at(ByteOffset::new(2), LexMode::Normal).expect("the end is a position");
        assert_eq!(end.kind, EOF);
        assert_eq!(end.text, "");
    }
}
```

- [ ] **Step 3: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib lexer`
Expected: compile error — `cannot find type Lexer`.

- [ ] **Step 4: Write the lexer**

Prepend to `src/lexer.rs`, above the test module:

```rust
//! The file lexer (docs/design/syntax.md §4.4–§4.6): total on every
//! (offset, mode), hand-written to grammar §4 and §6.2–§6.3, with no
//! state but the source and the dialect. Error tokens take the extents
//! syntax.md §4.5 states.

use themelios_base::line::{OffsetOutOfBounds, PositionRefusal};
use themelios_base::source::{NotCharBoundary, Source, SourceId};
use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::token::{LexMode, Token, TokenSource};
use crate::tree::SyntaxKind;

/// The lexer over an admitted source: total on every (offset, mode),
/// hand-written to the grammar of record's lexical section, with no
/// state but the source and the dialect. Cheap to construct — it holds
/// one reference and the dialect — and a pure function thereafter.
#[derive(Clone, Copy, Debug)]
pub struct Lexer<'a> {
    source: &'a Source,
    dialect: Dialect,
}

impl<'a> Lexer<'a> {
    /// A lexer over `source` under `dialect`. Total, O(1).
    pub fn new(source: &'a Source, dialect: Dialect) -> Lexer<'a> {
        Lexer { source, dialect }
    }
}

impl TokenSource for Lexer<'_> {
    fn id(&self) -> SourceId {
        self.source.id()
    }

    fn dialect(&self) -> Dialect {
        self.dialect
    }

    fn text(&self) -> &str {
        self.source.text()
    }

    /// The token at `at` under `mode`. Refuses with `PositionRefusal`
    /// exactly where base refuses: past the end (`OutOfBounds`) or
    /// inside a character (`NotCharBoundary`). O(the token).
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let text = self.source.text();
        let offset = at.get() as usize;
        if offset > text.len() {
            return Err(PositionRefusal::OutOfBounds(OffsetOutOfBounds {
                offset: at,
                max: self.source.end(),
            }));
        }
        if !text.is_char_boundary(offset) {
            return Err(PositionRefusal::NotCharBoundary(NotCharBoundary { offset: at }));
        }
        let (kind, len) = lex(text, offset, mode, self.dialect);
        Ok(Token { kind, text: &text[offset..offset + len] })
    }
}

/// The token beginning at char boundary `at` of `text` under `mode` and
/// `dialect`, as its kind and byte length: `EOF` with length zero at
/// the end, otherwise never zero. Pure; O(the token). The whole lexer
/// is this function; the door adds only the refusals.
pub(crate) fn lex(text: &str, at: usize, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let rest = &text[at..];
    if rest.is_empty() {
        return (SyntaxKind::EOF, 0);
    }
    match mode {
        LexMode::ScriptBody => script_body(rest),
        LexMode::Normal | LexMode::Theory => shared(rest, mode, dialect),
    }
}

/// Grammar §4.8: `#end` where the region ends, else the raw text through
/// the first `#end`, else an error to the end of input.
fn script_body(rest: &str) -> (SyntaxKind, usize) {
    if rest.starts_with(SCRIPT_END) {
        return (SyntaxKind::KW_END, SCRIPT_END.len());
    }
    match rest.find(SCRIPT_END) {
        Some(end) => (SyntaxKind::SCRIPT_BODY, end),
        None => (SyntaxKind::ERROR, rest.len()),
    }
}

/// The script terminator (grammar §4.8).
const SCRIPT_END: &str = "#end";

/// The rules shared by normal and theory mode, dispatching on the first
/// byte; the two modes differ at `#`-words, names (`_` alone), and the
/// operator alphabet.
fn shared(rest: &str, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    match bytes[0] {
        b' ' | b'\t' | b'\r' | b'\n' => (SyntaxKind::WHITESPACE, run(bytes, is_whitespace)),
        b'%' if rest.starts_with("%!") => (SyntaxKind::DOC_COMMENT, line_end(rest)),
        b'%' if rest.starts_with("%*") => block_comment(rest, dialect),
        b'%' => (SyntaxKind::LINE_COMMENT, line_end(rest)),
        b'#' if rest.starts_with("#!") => (SyntaxKind::SHEBANG_COMMENT, line_end(rest)),
        b'#' => hash_word(rest, mode),
        b'"' => match dialect {
            Dialect::Clingo => string_clingo(rest),
            Dialect::AspCore2 => string_asp_core_2(rest),
        },
        b'0'..=b'9' => (SyntaxKind::NUMBER, number(bytes)),
        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'\'' => match name(rest) {
            Some(token) => token,
            None if bytes[0] == b'_' && mode == LexMode::Normal => (SyntaxKind::ANONYMOUS, 1),
            None => error_run(rest, mode),
        },
        _ => {
            let punctuation = match mode {
                LexMode::Normal => punctuation_normal(bytes),
                LexMode::Theory => punctuation_theory(bytes),
                LexMode::ScriptBody => None,
            };
            punctuation.unwrap_or_else(|| error_run(rest, mode))
        }
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// The length of the maximal prefix of `bytes` whose bytes satisfy
/// `admits`; the callers' predicates hold only for ASCII bytes, so the
/// prefix ends on a character boundary.
fn run(bytes: &[u8], admits: fn(u8) -> bool) -> usize {
    bytes.iter().take_while(|byte| admits(**byte)).count()
}

/// The length through the end of the line: up to and excluding the line
/// break, or to the end of the text.
fn line_end(rest: &str) -> usize {
    rest.find('\n').unwrap_or(rest.len())
}

/// Grammar §4.1's `BLOCK-COMMENT` and grammar §6.3's replacement.
fn block_comment(rest: &str, dialect: Dialect) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    match dialect {
        Dialect::AspCore2 => match rest[2..].find("*%") {
            Some(close) => (SyntaxKind::BLOCK_COMMENT, close + 4),
            None => (SyntaxKind::ERROR, rest.len()),
        },
        Dialect::Clingo => {
            // The depth is a counter, never a stack (grammar §10). Scanning
            // byte-wise is exact: every byte the rule reads is ASCII, and
            // a multi-byte character's bytes are never `%` or `*`.
            let mut depth = 1usize;
            let mut i = 2;
            while i < bytes.len() {
                if bytes[i] == b'%' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'%') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        return (SyntaxKind::BLOCK_COMMENT, i);
                    }
                } else if bytes[i] == b'%' {
                    // Any other `%` silences the rest of its line, openers
                    // and closers included (grammar §4.1).
                    match rest[i..].find('\n') {
                        Some(to_break) => i += to_break + 1,
                        None => return (SyntaxKind::ERROR, rest.len()),
                    }
                } else {
                    i += 1;
                }
            }
            (SyntaxKind::ERROR, rest.len())
        }
    }
}

/// The `#`-keywords of grammar §4.5 by spelling, in the roster's order;
/// `#sum+` is formed beside the table, since `+` is no name character.
const KEYWORDS: [(&str, SyntaxKind); 25] = [
    ("#const", SyntaxKind::KW_CONST),
    ("#count", SyntaxKind::KW_COUNT),
    ("#defined", SyntaxKind::KW_DEFINED),
    ("#edge", SyntaxKind::KW_EDGE),
    ("#external", SyntaxKind::KW_EXTERNAL),
    ("#false", SyntaxKind::KW_FALSE),
    ("#heuristic", SyntaxKind::KW_HEURISTIC),
    ("#include", SyntaxKind::KW_INCLUDE),
    ("#inf", SyntaxKind::KW_INF),
    ("#infimum", SyntaxKind::KW_INF),
    ("#max", SyntaxKind::KW_MAX),
    ("#maximize", SyntaxKind::KW_MAXIMIZE),
    ("#maximise", SyntaxKind::KW_MAXIMIZE),
    ("#min", SyntaxKind::KW_MIN),
    ("#minimize", SyntaxKind::KW_MINIMIZE),
    ("#minimise", SyntaxKind::KW_MINIMIZE),
    ("#program", SyntaxKind::KW_PROGRAM),
    ("#project", SyntaxKind::KW_PROJECT),
    ("#script", SyntaxKind::KW_SCRIPT),
    ("#show", SyntaxKind::KW_SHOW),
    ("#sum", SyntaxKind::KW_SUM),
    ("#sup", SyntaxKind::KW_SUP),
    ("#supremum", SyntaxKind::KW_SUP),
    ("#theory", SyntaxKind::KW_THEORY),
    ("#true", SyntaxKind::KW_TRUE),
];

/// The two `#`-terms theory mode admits (grammar §4.7): the infimum and
/// supremum, both spellings each.
const THEORY_KEYWORDS: [(&str, SyntaxKind); 4] = [
    ("#inf", SyntaxKind::KW_INF),
    ("#infimum", SyntaxKind::KW_INF),
    ("#sup", SyntaxKind::KW_SUP),
    ("#supremum", SyntaxKind::KW_SUP),
];

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// A `#`-word: a keyword by spelling, or one unknown word by maximal
/// munch (grammar §4.5); `#sum+` outranks `#sum` by length.
fn hash_word(rest: &str, mode: LexMode) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let word_len = 1 + run(&bytes[1..], is_name_char);
    let word = &rest[..word_len];
    let table: &[(&str, SyntaxKind)] = match mode {
        LexMode::Theory => &THEORY_KEYWORDS,
        LexMode::Normal | LexMode::ScriptBody => &KEYWORDS,
    };
    if mode == LexMode::Normal && word == "#sum" && bytes.get(word_len) == Some(&b'+') {
        return (SyntaxKind::KW_SUM_PLUS, word_len + 1);
    }
    match table.iter().find(|(spelling, _)| *spelling == word) {
        Some((_, kind)) => (*kind, word_len),
        None => (SyntaxKind::ERROR, word_len),
    }
}

/// Grammar §4.4's string rule: exactly three escapes, no raw line
/// break; a defect makes the whole token an `ERROR` of the extent
/// syntax.md §4.5 states.
fn string_clingo(rest: &str) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let mut defective = false;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let kind = if defective { SyntaxKind::ERROR } else { SyntaxKind::STRING };
                return (kind, i + 1);
            }
            b'\n' => return (SyntaxKind::ERROR, i),
            b'\\' => match bytes.get(i + 1) {
                Some(b'"' | b'\\' | b'n') => i += 2,
                Some(b'\n') | None => return (SyntaxKind::ERROR, i + 1),
                Some(_) => {
                    defective = true;
                    i += 1 + char_len(rest, i + 1);
                }
            },
            _ => i += char_len(rest, i),
        }
    }
    (SyntaxKind::ERROR, rest.len())
}

/// Grammar §6.2's string rule: `\"` is the one escape, every other
/// character denotes itself, raw line breaks included, under maximal
/// munch — at `\"` the escape reading wins whenever a later quote can
/// close the string; else that quote closes it.
fn string_asp_core_2(rest: &str) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (SyntaxKind::STRING, i + 1),
            b'\\' if bytes.get(i + 1) == Some(&b'"') => {
                if rest[i + 2..].contains('"') {
                    i += 2;
                } else {
                    return (SyntaxKind::STRING, i + 2);
                }
            }
            _ => i += char_len(rest, i),
        }
    }
    (SyntaxKind::ERROR, rest.len())
}

/// The byte length of the character at char boundary `at` of `text`.
fn char_len(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(1, char::len_utf8)
}

/// Grammar §4.3's numerals: the prefixed forms take at least one digit
/// of their class, else the token is the `0` alone.
fn number(bytes: &[u8]) -> usize {
    if bytes[0] != b'0' {
        return run(bytes, |byte| byte.is_ascii_digit());
    }
    let class: Option<fn(u8) -> bool> = match bytes.get(1) {
        Some(b'x') => Some(|byte| byte.is_ascii_hexdigit()),
        Some(b'o') => Some(|byte| matches!(byte, b'1'..=b'7')),
        Some(b'b') => Some(|byte| matches!(byte, b'0' | b'1')),
        _ => None,
    };
    match class {
        Some(admits) => match run(&bytes[2..], admits) {
            0 => 1,
            digits => 2 + digits,
        },
        None => 1,
    }
}

/// Grammar §4.2's names: `[_']* [a-z] ['A-Za-z0-9_]*` an identifier,
/// `[_']* [A-Z] ['A-Za-z0-9_]*` a variable; `not` is the keyword; no
/// letter after the prefix is no name.
fn name(rest: &str) -> Option<(SyntaxKind, usize)> {
    let bytes = rest.as_bytes();
    let prefix = run(bytes, |byte| byte == b'_' || byte == b'\'');
    let kind = match bytes.get(prefix)? {
        b'a'..=b'z' => SyntaxKind::IDENT,
        b'A'..=b'Z' => SyntaxKind::VARIABLE,
        _ => return None,
    };
    let len = prefix + 1 + run(&bytes[prefix + 1..], |byte| is_name_char(byte) || byte == b'\'');
    if kind == SyntaxKind::IDENT && &rest[..len] == "not" {
        return Some((SyntaxKind::KW_NOT, len));
    }
    Some((kind, len))
}

/// Grammar §4.6's punctuation and operators under maximal munch.
fn punctuation_normal(bytes: &[u8]) -> Option<(SyntaxKind, usize)> {
    let second = bytes.get(1).copied();
    Some(match bytes[0] {
        b'.' if second == Some(b'.') => (SyntaxKind::DOTDOT, 2),
        b'.' => (SyntaxKind::DOT, 1),
        b',' => (SyntaxKind::COMMA, 1),
        b';' => (SyntaxKind::SEMICOLON, 1),
        b':' if second == Some(b'-') => (SyntaxKind::NECK, 2),
        b':' if second == Some(b'~') => (SyntaxKind::WEAK_NECK, 2),
        b':' => (SyntaxKind::COLON, 1),
        b'|' => (SyntaxKind::PIPE, 1),
        b'(' => (SyntaxKind::L_PAREN, 1),
        b')' => (SyntaxKind::R_PAREN, 1),
        b'[' => (SyntaxKind::L_BRACKET, 1),
        b']' => (SyntaxKind::R_BRACKET, 1),
        b'{' => (SyntaxKind::L_BRACE, 1),
        b'}' => (SyntaxKind::R_BRACE, 1),
        b'+' => (SyntaxKind::PLUS, 1),
        b'-' => (SyntaxKind::MINUS, 1),
        b'*' if second == Some(b'*') => (SyntaxKind::STAR_STAR, 2),
        b'*' => (SyntaxKind::STAR, 1),
        b'/' => (SyntaxKind::SLASH, 1),
        b'\\' => (SyntaxKind::BACKSLASH, 1),
        b'^' => (SyntaxKind::CARET, 1),
        b'&' => (SyntaxKind::AMPERSAND, 1),
        b'~' => (SyntaxKind::TILDE, 1),
        b'?' => (SyntaxKind::QUESTION, 1),
        b'@' => (SyntaxKind::AT, 1),
        b'=' if second == Some(b'=') => (SyntaxKind::EQ, 2),
        b'=' => (SyntaxKind::EQ, 1),
        b'!' if second == Some(b'=') => (SyntaxKind::NEQ, 2),
        b'<' if second == Some(b'>') => (SyntaxKind::NEQ, 2),
        b'<' if second == Some(b'=') => (SyntaxKind::LE, 2),
        b'<' => (SyntaxKind::LT, 1),
        b'>' if second == Some(b'=') => (SyntaxKind::GE, 2),
        b'>' => (SyntaxKind::GT, 1),
        _ => return None,
    })
}

/// Grammar §4.7's operator alphabet.
fn is_theory_operator_char(byte: u8) -> bool {
    matches!(
        byte,
        b'/' | b'!'
            | b'<'
            | b'='
            | b'>'
            | b'+'
            | b'-'
            | b'*'
            | b'\\'
            | b'?'
            | b'&'
            | b'@'
            | b'|'
            | b':'
            | b';'
            | b'~'
            | b'^'
            | b'.'
    )
}

/// Grammar §4.7: the structural punctuation as everywhere; a length-one
/// run that is `.`, `;`, or `:` structural; the exact run `:-` the neck;
/// every other maximal run of the operator alphabet one `THEORY_OP`.
fn punctuation_theory(bytes: &[u8]) -> Option<(SyntaxKind, usize)> {
    match bytes[0] {
        b',' => return Some((SyntaxKind::COMMA, 1)),
        b'(' => return Some((SyntaxKind::L_PAREN, 1)),
        b')' => return Some((SyntaxKind::R_PAREN, 1)),
        b'[' => return Some((SyntaxKind::L_BRACKET, 1)),
        b']' => return Some((SyntaxKind::R_BRACKET, 1)),
        b'{' => return Some((SyntaxKind::L_BRACE, 1)),
        b'}' => return Some((SyntaxKind::R_BRACE, 1)),
        _ => {}
    }
    let len = run(bytes, is_theory_operator_char);
    if len == 0 {
        return None;
    }
    Some(match &bytes[..len] {
        b"." => (SyntaxKind::DOT, 1),
        b";" => (SyntaxKind::SEMICOLON, 1),
        b":" => (SyntaxKind::COLON, 1),
        b":-" => (SyntaxKind::NECK, 2),
        _ => (SyntaxKind::THEORY_OP, len),
    })
}

/// Whether some token begins at the start of `rest` under `mode` — the
/// question that ends an error run (syntax.md §4.5).
fn begins_token(rest: &str, mode: LexMode) -> bool {
    let bytes = rest.as_bytes();
    match bytes[0] {
        b' ' | b'\t' | b'\r' | b'\n' | b'%' | b'#' | b'"' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => {
            true
        }
        b'_' | b'\'' => name(rest).is_some() || (bytes[0] == b'_' && mode == LexMode::Normal),
        _ => match mode {
            LexMode::Normal => punctuation_normal(bytes).is_some(),
            LexMode::Theory => punctuation_theory(bytes).is_some(),
            LexMode::ScriptBody => false,
        },
    }
}

/// The maximal run of characters that begin no token, as one `ERROR`
/// token (syntax.md §4.5).
fn error_run(rest: &str, mode: LexMode) -> (SyntaxKind, usize) {
    let mut len = char_len(rest, 0);
    while len < rest.len() && !begins_token(&rest[len..], mode) {
        len += char_len(rest, len);
    }
    (SyntaxKind::ERROR, len)
}
```

- [ ] **Step 5: Run the lexer tests**

Run: `cargo test -p themelios-syntax --lib lexer`
Expected: 13 passed.

- [ ] **Step 6: Write the token-source and lexer property laws**

`crates/themelios-syntax/tests/lexer_laws.rs`:

```rust
//! The token-source laws on the file lexer under every mode, the checker
//! against deliberately breaching sources, and lexer totality and tiling
//! on generated text heavy in multi-byte characters, `%`, `#`, and
//! operator runs (docs/design/syntax.md §4.3, §16).

use std::cell::Cell;

use proptest::prelude::*;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::token::{
    LexMode, Token, TokenSource, TokenSourceLawViolation, check_token_source_laws,
};
use themelios_syntax::tree::SyntaxKind;

/// Text drawn from the characters that exercise the lexer's corners:
/// names, numerals, the comment and string openers, operator material,
/// braces, and multi-byte characters.
fn corner_text() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just("p".to_owned()),
        Just("X".to_owned()),
        Just("_".to_owned()),
        Just("'".to_owned()),
        Just("0".to_owned()),
        Just("0x".to_owned()),
        Just("1".to_owned()),
        Just("%".to_owned()),
        Just("%*".to_owned()),
        Just("*%".to_owned()),
        Just("%!".to_owned()),
        Just("#".to_owned()),
        Just("#!".to_owned()),
        Just("#sum".to_owned()),
        Just("#end".to_owned()),
        Just("\"".to_owned()),
        Just("\\".to_owned()),
        Just("\n".to_owned()),
        Just(" ".to_owned()),
        Just(":".to_owned()),
        Just("-".to_owned()),
        Just("..".to_owned()),
        Just("*".to_owned()),
        Just("=".to_owned()),
        Just("<".to_owned()),
        Just("!".to_owned()),
        Just("$".to_owned()),
        Just("{".to_owned()),
        Just("}".to_owned()),
        Just("(".to_owned()),
        Just(")".to_owned()),
        Just("é".to_owned()),
        Just("🦀".to_owned()),
        Just("not".to_owned()),
    ];
    prop::collection::vec(atom, 0..40).prop_map(|parts| parts.concat())
}

fn dialects() -> impl Strategy<Value = Dialect> {
    prop_oneof![Just(Dialect::Clingo), Just(Dialect::AspCore2)]
}

fn modes() -> impl Strategy<Value = LexMode> {
    prop_oneof![Just(LexMode::Normal), Just(LexMode::Theory), Just(LexMode::ScriptBody)]
}

fn admitted(text: &str) -> Source {
    Source::new(SourceId::new(0), text.to_owned()).expect("generated text admits")
}

proptest! {
    #[test]
    fn the_file_lexer_keeps_the_four_laws(text in corner_text(), dialect in dialects()) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, dialect);
        prop_assert_eq!(check_token_source_laws(&lexer), Vec::new());
    }

    #[test]
    fn every_mode_tiles_the_text_from_any_boundary(
        text in corner_text(), dialect in dialects(), mode in modes(), start in 0usize..64
    ) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, dialect);
        let mut at = (start.min(text.len())..=text.len())
            .find(|offset| text.is_char_boundary(*offset))
            .expect("the end is a boundary");
        loop {
            let token = lexer
                .token_at(ByteOffset::new(u32::try_from(at).expect("small")), mode)
                .expect("a position");
            if token.kind == SyntaxKind::EOF {
                prop_assert_eq!(at, text.len());
                break;
            }
            prop_assert!(!token.text.is_empty());
            prop_assert_eq!(&text[at..at + token.text.len()], token.text);
            at += token.text.len();
        }
    }

    #[test]
    fn the_door_refuses_exactly_off_position(text in corner_text(), mode in modes()) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, Dialect::Clingo);
        for offset in 0..=text.len() + 2 {
            let answer = lexer.token_at(ByteOffset::new(u32::try_from(offset).expect("small")), mode);
            if offset > text.len() {
                prop_assert!(matches!(answer, Err(PositionRefusal::OutOfBounds(_))));
            } else if !text.is_char_boundary(offset) {
                prop_assert!(matches!(answer, Err(PositionRefusal::NotCharBoundary(_))));
            } else {
                prop_assert!(answer.is_ok());
            }
        }
    }
}

/// A source over one text whose door misbehaves in one named way.
struct Breaching<'a> {
    text: &'a str,
    breach: Breach,
    calls: Cell<u32>,
}

#[derive(Clone, Copy)]
enum Breach {
    /// Answers `EOF` at offset zero whatever the text.
    EarlyEnd,
    /// Answers a token whose text is not the source's slice.
    Synthesized,
    /// Answers a different kind on every call.
    Flaky,
    /// Answers a token past the end of the text.
    Permissive,
}

impl TokenSource for Breaching<'_> {
    fn id(&self) -> SourceId {
        SourceId::new(9)
    }

    fn dialect(&self) -> Dialect {
        Dialect::Clingo
    }

    fn text(&self) -> &str {
        self.text
    }

    fn token_at(&self, at: ByteOffset, _mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let offset = at.get() as usize;
        match self.breach {
            Breach::EarlyEnd => Ok(Token { kind: SyntaxKind::EOF, text: "" }),
            Breach::Synthesized => {
                if offset >= self.text.len() {
                    return Ok(Token { kind: SyntaxKind::EOF, text: "" });
                }
                Ok(Token { kind: SyntaxKind::IDENT, text: "synthesized" })
            }
            Breach::Flaky => {
                let call = self.calls.get();
                self.calls.set(call + 1);
                if offset >= self.text.len() {
                    return Ok(Token { kind: SyntaxKind::EOF, text: "" });
                }
                let kind = if call % 2 == 0 { SyntaxKind::IDENT } else { SyntaxKind::VARIABLE };
                Ok(Token { kind, text: &self.text[offset..offset + 1] })
            }
            Breach::Permissive => {
                if offset >= self.text.len() {
                    return Ok(Token { kind: SyntaxKind::EOF, text: "" });
                }
                // The rest of the text as one token — and an empty answer,
                // never a refusal, inside a character.
                Ok(Token { kind: SyntaxKind::IDENT, text: self.text.get(offset..).unwrap_or("") })
            }
        }
    }
}

fn breaching(text: &str, breach: Breach) -> Breaching<'_> {
    Breaching { text, breach, calls: Cell::new(0) }
}

#[test]
fn the_checker_reports_an_early_end() {
    let report = check_token_source_laws(&breaching("pq", Breach::EarlyEnd));
    assert!(report.contains(&TokenSourceLawViolation::Tiling {
        at: ByteOffset::ZERO,
        token: SyntaxKind::EOF,
        len: 0
    }));
}

#[test]
fn the_checker_reports_a_synthesized_token() {
    let report = check_token_source_laws(&breaching("pq", Breach::Synthesized));
    assert!(report.contains(&TokenSourceLawViolation::Tiling {
        at: ByteOffset::ZERO,
        token: SyntaxKind::IDENT,
        len: 11
    }));
}

#[test]
fn the_checker_reports_nondeterminism() {
    let report = check_token_source_laws(&breaching("pq", Breach::Flaky));
    assert!(report.iter().any(|violation| matches!(
        violation,
        TokenSourceLawViolation::Determinism { at, mode: LexMode::Normal } if *at == ByteOffset::ZERO
    )));
}

#[test]
fn the_checker_reports_a_permissive_door() {
    let report = check_token_source_laws(&breaching("pq", Breach::Permissive));
    assert!(report.contains(&TokenSourceLawViolation::Refusal {
        at: ByteOffset::new(3),
        refused: false
    }));
}

#[test]
fn the_checker_reports_a_door_that_answers_inside_a_character() {
    let report = check_token_source_laws(&breaching("é", Breach::Permissive));
    assert!(report.iter().any(|violation| matches!(
        violation,
        TokenSourceLawViolation::Refusal { refused: false, .. }
    )));
}

#[test]
fn a_violation_displays_and_composes_as_an_error() {
    let violation = TokenSourceLawViolation::Slice { at: ByteOffset::new(4) };
    assert_eq!(violation.to_string(), "the token at byte 4 is not a slice of the source's text");
    let _: &dyn std::error::Error = &violation;
}
```

Note on the synthesized case: the token `synthesized` (eleven bytes)
runs past the two-byte text, so the checker reports it as a tiling
breach before it can compare slices — which is the design's order (the
extent is checked, then the slice); a synthesized token *within* the
text's extent would be a `Slice` violation, and the Task 7 parser test
of a breaching source covers that shape.

- [ ] **Step 7: Run the laws**

Run: `cargo test -p themelios-syntax --test lexer_laws`
Expected: 9 passed.

- [ ] **Step 8: Run the full gate**

Run the four gate commands.
Expected: green. Likely pedantic lints and their repairs: `clippy::too_many_lines` on `shared` or `punctuation_normal` — split or allow with its argument at the function; `clippy::match_same_arms` — merge the arms; `clippy::needless_pass_by_value` — take references. Repair in code and record in the commit message.

- [ ] **Step 9: Commit**

```bash
git add crates/themelios-syntax
git commit -m "Add the token-source door with its four laws and checker, and the file lexer under both dialects and three modes"
```

---

### Task 4: The `fusion` module over texts — the oracle's core

**Files:**
- Create: `crates/themelios-syntax/src/fusion.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod fusion;`)

**Derives:** syntax.md §10 (`Separator`, `LexContext`,
`separator_between`), §10.1 (why relexing is the whole oracle, the
lemma's two shapes), §13; grammar §4.7's first pinned case and §11's
oracle-relevant seeds. The tree-token form `separator` and `lex_mode_of`
land in Task 15, once the parser exists.

**Interfaces:**
- Consumes: `lexer::lex`, `token::LexMode`, `dialect::Dialect`,
  `tree::SyntaxKind`.
- Produces: `fusion::Separator { Nothing, Whitespace, LineBreak }`,
  `fusion::LexContext { dialect, mode }`,
  `fusion::separator_between(&str, &str, LexContext) -> Separator`.

- [ ] **Step 1: Write the failing tests**

Append `pub mod fusion;` to `src/lib.rs`. Create `src/fusion.rs` holding
only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn normal(left: &str, right: &str) -> Separator {
        separator_between(left, right, LexContext { dialect: Dialect::Clingo, mode: LexMode::Normal })
    }

    fn theory(left: &str, right: &str) -> Separator {
        separator_between(left, right, LexContext { dialect: Dialect::Clingo, mode: LexMode::Theory })
    }

    #[test]
    fn the_grammars_named_cases_answer_as_the_grammar_says() {
        // The greedy theory-operator munch (grammar §4.7).
        assert_eq!(theory("<", "="), Separator::Whitespace);
        assert_eq!(theory("-", "-"), Separator::Whitespace);
        assert_eq!(theory(";", "-"), Separator::Whitespace);
        // The rule-neck abutment and the other normal-mode fusions.
        assert_eq!(normal(":", "-"), Separator::Whitespace);
        assert_eq!(normal("#sum", "+"), Separator::Whitespace);
        assert_eq!(normal("0", "x1"), Separator::Whitespace);
        assert_eq!(normal(".", "."), Separator::Whitespace);
        assert_eq!(normal("*", "*"), Separator::Whitespace);
        assert_eq!(normal("not", "p"), Separator::Whitespace);
        assert_eq!(normal("1", "2"), Separator::Whitespace);
        assert_eq!(normal("#inf", "x"), Separator::Whitespace);
        // A line comment before anything.
        assert_eq!(normal("% c", "p"), Separator::LineBreak);
        assert_eq!(normal("%! d", "p"), Separator::LineBreak);
        assert_eq!(normal("#! s", "p"), Separator::LineBreak);
    }

    #[test]
    fn pairs_that_lex_to_themselves_abutted_may_abut() {
        assert_eq!(normal("p", "("), Separator::Nothing);
        assert_eq!(normal(")", "."), Separator::Nothing);
        assert_eq!(normal(";", "-"), Separator::Nothing);
        assert_eq!(normal("-", "-"), Separator::Nothing);
        assert_eq!(normal("%* c *%", "p"), Separator::Nothing);
        assert_eq!(normal("\"a\"", "\"b\""), Separator::Nothing);
        assert_eq!(normal("X", "."), Separator::Nothing);
        assert_eq!(theory("x", "<="), Separator::Nothing);
        assert_eq!(theory("not", "-"), Separator::Nothing);
    }

    #[test]
    fn the_script_region_and_its_terminator() {
        let script = LexContext { dialect: Dialect::Clingo, mode: LexMode::ScriptBody };
        assert_eq!(separator_between("#end", ".", script), Separator::Nothing);
        assert_eq!(separator_between("x = 1 ", "#end", script), Separator::Nothing);
    }

    #[test]
    fn the_asp_core_2_string_ending_in_an_escaped_looking_quote() {
        let core = LexContext { dialect: Dialect::AspCore2, mode: LexMode::Normal };
        assert_eq!(separator_between("\"a\\\"", "\"", core), Separator::Whitespace);
        assert_eq!(separator_between("\"a\\\"", "p", core), Separator::Nothing);
    }

    #[test]
    fn end_of_input_on_the_right_separates_nothing() {
        assert_eq!(normal("p", ""), Separator::Nothing);
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib fusion`
Expected: compile error — `cannot find function separator_between`.

- [ ] **Step 3: Write the oracle over texts**

Prepend to `src/fusion.rs`:

```rust
//! The fusion oracle (docs/design/syntax.md §10): what must stand
//! between two tokens for each to lex as itself — not a theory to
//! maintain but a fact to compute, since this crate owns the lexer and
//! the exact answer is one relex away.

use crate::dialect::Dialect;
use crate::lexer::lex;
use crate::token::LexMode;
use crate::tree::SyntaxKind;

/// What must stand between two tokens for each to lex as itself.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Separator {
    /// The two may abut: `left` followed directly by `right` lexes as
    /// `left`, then `right`. (Not `None`: a Rust reader's `None` is
    /// `Option`'s, and this is an answer, not an absence.)
    Nothing,
    /// Whitespace of any kind is required — abutting would fuse or split
    /// the tokens (`a` `b` → `ab`; `#sum` `+` → `#sum+`; `0` `x1` →
    /// `0x1`; `<` `=` → `<=` under theory mode).
    Whitespace,
    /// A line break is required: `left` runs to the end of its line — a
    /// line comment, a doc comment, a shebang — and swallows anything
    /// after it on that line.
    LineBreak,
}

/// The lexical context an adjacency stands in: the dialect and the mode
/// in force at `left`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LexContext {
    /// The dialect the text was lexed under.
    pub dialect: Dialect,
    /// The mode in force at `left`'s start.
    pub mode: LexMode,
}

/// The oracle over texts: total, exact, O(|left| + |right|). `left` and
/// `right` are token texts of one lexed text; the answer is what the
/// lexer would do to them abutted under `context` — `Nothing` when
/// `left ++ right` lexes `left` first, `LineBreak` when `left` is a
/// line form, `Whitespace` otherwise (docs/design/syntax.md §10.1: a
/// space begins no token's continuation, and no token but the line
/// forms extends across it — for the token pairs a lexed text produces,
/// which is what §10.1's lemma scopes the oracle to).
pub fn separator_between(left: &str, right: &str, context: LexContext) -> Separator {
    let (left_kind, left_len) = lex(left, 0, context.mode, context.dialect);
    let line_form = matches!(
        left_kind,
        SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT | SyntaxKind::SHEBANG_COMMENT
    );
    if line_form && left_len == left.len() {
        return Separator::LineBreak;
    }
    let joined = format!("{left}{right}");
    let (kind, len) = lex(&joined, 0, context.mode, context.dialect);
    if kind == left_kind && len == left.len() {
        Separator::Nothing
    } else {
        Separator::Whitespace
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p themelios-syntax --lib fusion`
Expected: 5 passed.

- [ ] **Step 5: Run the full gate, then commit**

Run the four gate commands. Expected: green.

```bash
git add crates/themelios-syntax
git commit -m "Add the fusion oracle over texts: what must stand between two tokens, computed by one relex"
```

---

### Task 5: The fuzz crate, with the lex target

**Files:**
- Create: `crates/themelios-syntax/fuzz/Cargo.toml`,
  `crates/themelios-syntax/fuzz/fuzz_targets/lex.rs`,
  `crates/themelios-syntax/fuzz/corpus/lex/{rule.lp,theory.lp,script.lp,comments.lp,strings.lp}`
- Modify: `Cargo.toml` (workspace members), `.gitignore`,
  `.github/workflows/gate.yml` (coverage excludes the fuzz crate)

**Derives:** syntax.md §16 (the fuzz crate, committed, corpus committed,
run out of band); spec §10.1 (fuzzing from the first weeks), §10.2 (out
of band, continuously), §12.3 (the fuzz crate is a test harness outside
every shipped closure).

**Interfaces:**
- Consumes: `lexer::Lexer`, `token::{TokenSource, LexMode,
  check_token_source_laws}`, `dialect::Dialect`, base's
  `Source::from_bytes`.
- Produces: the crate `themelios-syntax-fuzz`, a workspace member, and
  the target `lex`; Task 12 adds the parse targets beside it.

- [ ] **Step 1: Write the fuzz crate**

`crates/themelios-syntax/fuzz/Cargo.toml`:

```toml
[package]
name = "themelios-syntax-fuzz"
version = "0.0.0"
publish = false
description = "Fuzz targets over themelios-syntax: run out of band, corpus committed."
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[package.metadata]
cargo-fuzz = true

# libfuzzer-sys carries libFuzzer's C++ runtime, compiled by its build
# script: this crate is a test harness outside every shipped closure —
# it is no dependency of themelios-syntax, and tests/trust.rs holds that
# crate's closure over the resolved graph without it
# (docs/specification.md §12.3; docs/design/syntax.md §14, §16).
[dependencies]
libfuzzer-sys = "0.4"
themelios-base = { path = "../../themelios-base" }
themelios-syntax = { path = ".." }

# The targets are harness bodies under a generated entry point; the
# workspace lint tables address the shipped crates and their tests. The
# gate's clippy still runs over these targets with warnings denied.
[lints]

[[bin]]
name = "lex"
path = "fuzz_targets/lex.rs"
test = false
doc = false
bench = false
```

Add the member to the workspace manifest — `Cargo.toml` at the root,
the `[workspace]` table:

```toml
[workspace]
resolver = "3"
members = ["crates/*", "crates/themelios-syntax/fuzz"]
```

Append to `.gitignore`:

```
/crates/themelios-syntax/fuzz/artifacts
```

- [ ] **Step 2: Write the lex target**

`crates/themelios-syntax/fuzz/fuzz_targets/lex.rs`:

```rust
//! Arbitrary bytes under both dialects and all three modes: admission
//! refuses what is not UTF-8; every admitted text tiles under every mode
//! from offset zero, no token is empty, every token is a slice, and the
//! four token-source laws hold on the file lexer
//! (docs/design/syntax.md §4.3, §16).
#![no_main]

use libfuzzer_sys::fuzz_target;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::token::{LexMode, TokenSource, check_token_source_laws};
use themelios_syntax::tree::SyntaxKind;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    for dialect in [Dialect::Clingo, Dialect::AspCore2] {
        let lexer = Lexer::new(&source, dialect);
        assert!(check_token_source_laws(&lexer).is_empty());
        for mode in [LexMode::Normal, LexMode::Theory, LexMode::ScriptBody] {
            let text = source.text();
            let mut at = 0usize;
            loop {
                let token = lexer
                    .token_at(ByteOffset::new(u32::try_from(at).expect("admitted")), mode)
                    .expect("a position");
                if token.kind == SyntaxKind::EOF {
                    assert_eq!(at, text.len());
                    break;
                }
                assert!(!token.text.is_empty());
                assert_eq!(&text[at..at + token.text.len()], token.text);
                at += token.text.len();
            }
        }
    }
});
```

- [ ] **Step 3: Seed the corpus**

Five seed files under `crates/themelios-syntax/fuzz/corpus/lex/`, one
shape each; the fuzzer grows the directory from them and its minimized
additions are committed as they come:

`rule.lp`:
```
%! a documented rule
p(X) :- q(X), not r(X), X = 1..3, #sum { W,T : t(T,W) } >= 4.
```

`theory.lp`:
```
:- &sum { x :- y ; 1 :- z : p((a;b)), q } <= - not - 3, p.
&a { x <= y ; not z }.
```

`script.lp`:
```
#script (python)
def f(): return "#end is not here"
#end.
p.
```

`comments.lp`:
```
#! shebang
%* a %* b *% % silenced *% still
%* closes here *% p. % trailing
%! not docs
```

`strings.lp`:
```
p("a\nb"). q("a\"b"). r("bad\qb"). s("open
```

- [ ] **Step 4: Build and run the target briefly on the stable toolchain**

Run, from `crates/themelios-syntax`:
`cargo fuzz build -s none && cargo fuzz run lex -s none -- -max_total_time=60`
Expected: the target builds on 1.97.1 with sanitizer coverage
instrumentation and no sanitizer (`-s none`; address sanitizer needs a
nightly toolchain and is not used), runs sixty seconds, and reports no
crash. `cargo fuzz` is an externally installed tool
(`cargo install cargo-fuzz` if absent); it never enters a manifest.

- [ ] **Step 5: Keep the fuzz crate out of the coverage measurement**

The fuzz targets are bins outside every test suite; measuring them would
subtract from the floor without asking anything. In
`.github/workflows/gate.yml`, the coverage step becomes:

```yaml
      - run: cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --locked --fail-under-lines 90
```

- [ ] **Step 6: Run the full gate**

Run the four gate commands. Expected: green — the fuzz crate builds
under `cargo clippy --workspace --all-targets` and `cargo test
--workspace` (its bins compile; it has no tests).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore .github/workflows/gate.yml crates/themelios-syntax/fuzz
git commit -m "Add the fuzz crate with its lex target and seed corpus; coverage measures the shipped crates alone"
```

---

### Task 6: The `diagnostic` module — the typed value, its roster, the identities, and the lowering

**Files:**
- Create: `crates/themelios-syntax/src/diagnostic.rs`,
  `crates/themelios-syntax/tests/golden/identity-table.txt`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod diagnostic;`)

**Derives:** syntax.md §7.1 (the typed value, `SyntaxErrorKind`, the
payload types, `SourceBreach`, `Hint`, the expected set, `GrammarWord`,
`SyntaxClass`), §7.2 (identity and severity), §7.3 (lowering and the
messages), §12.5 (no `Display` on `SyntaxError`), §16 (the identity
table, snapshot-tested), Appendix B.

**Interfaces:**
- Consumes: base's `Diagnostic`, `DiagnosticId`, `Label`, `Location`,
  `Severity`, `ToDiagnostic`; `tree::SyntaxKind`; base's `ByteOffset`.
- Produces: `diagnostic::SyntaxError` (`kind`, `id`, `severity`,
  `primary`, `related`; `impl ToDiagnostic`), the crate-private
  constructors `SyntaxError::new(kind, primary)` and `with_related`;
  `SyntaxErrorKind`, `StringDefect`, `RestrictedForm`, `Restriction`,
  `MisplacedDoc`, `SourceBreach`, `Hint`, `Related`, `RelatedLocus`,
  `ExpectedSet`, `Expected`, `GrammarWord`, `SyntaxClass` — the types the
  parser (Tasks 7–11) emits.

- [ ] **Step 1: Write the failing tests**

Append `pub mod diagnostic;` to `src/lib.rs`. Create `src/diagnostic.rs`
holding only:

```rust
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use themelios_base::source::{SourceId, SourceSet};
    use themelios_base::span::Span;
    use themelios_base::view::human;

    use super::*;

    fn at(start: u32, end: u32) -> Location {
        Location {
            source: SourceId::new(0),
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end)).expect("ordered"),
        }
    }

    fn expected(items: &[Expected]) -> ExpectedSet {
        items.iter().copied().collect()
    }

    /// One representative of every kind, in the roster's order.
    fn representatives() -> Vec<SyntaxErrorKind> {
        vec![
            SyntaxErrorKind::UnexpectedCharacters,
            SyntaxErrorKind::UnknownHashWord,
            SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape('q') },
            SyntaxErrorKind::UnterminatedBlockComment,
            SyntaxErrorKind::UnterminatedScript,
            SyntaxErrorKind::AnonymousInTheoryExpression,
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[Expected::Token(SyntaxKind::DOT)]),
                found: SyntaxKind::COMMA,
                hint: None,
            },
            SyntaxErrorKind::UnexpectedEndOfInput {
                expected: expected(&[Expected::Class(SyntaxClass::Term)]),
                hint: None,
            },
            SyntaxErrorKind::NestingTooDeep { depth: 3 },
            SyntaxErrorKind::AspifInput,
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Refusal { at: ByteOffset::new(2) } },
            SyntaxErrorKind::FormNotAllowedHere {
                form: RestrictedForm::Pool,
                context: Restriction::ConstantTerm,
            },
            SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::NoStatementFollows },
        ]
    }

    #[test]
    fn the_identity_table_matches_its_snapshot() {
        let table: String = representatives()
            .into_iter()
            .map(|kind| {
                let error = SyntaxError::new(kind, at(0, 1));
                format!("{} {}\n", error.id(), error.severity())
            })
            .collect();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/identity-table.txt");
        if std::env::var_os("GOLDEN_BLESS").is_some() {
            fs::write(&path, &table).expect("golden file writes");
            return;
        }
        let shipped = fs::read_to_string(&path).expect("the identity table is shipped");
        assert_eq!(table, shipped, "docs/design/syntax.md Appendix B: the identity table changed");
    }

    #[test]
    fn identities_are_in_the_syntax_namespace_and_only_the_doc_warning_warns() {
        for kind in representatives() {
            let error = SyntaxError::new(kind.clone(), at(0, 1));
            assert_eq!(error.id().namespace(), "syntax");
            let expected = if matches!(kind, SyntaxErrorKind::MisplacedDocComment { .. }) {
                Severity::Warning
            } else {
                Severity::Error
            };
            assert_eq!(error.severity(), expected, "{kind:?}");
        }
    }

    #[test]
    fn lowering_carries_identity_severity_primary_and_related_loci() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedEndOfInput {
                expected: expected(&[Expected::Token(SyntaxKind::R_BRACE)]),
                hint: None,
            },
            at(10, 10),
        )
        .with_related(Related { locus: RelatedLocus::ToClose(SyntaxKind::L_BRACE), location: at(4, 5) });
        let lowered = error.to_diagnostic();
        assert_eq!(lowered.id().to_string(), "syntax::unexpected-end-of-input");
        assert_eq!(lowered.severity(), Severity::Error);
        assert_eq!(lowered.primary().location, at(10, 10));
        assert_eq!(lowered.message(), "expected `}`, found end of input");
        let secondary: Vec<_> = lowered.secondary().iter().collect();
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0].location, at(4, 5));
        assert_eq!(secondary[0].message.as_deref(), Some("to close this `{`"));
    }

    #[test]
    fn expected_sets_render_kinds_then_words_then_classes() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[
                    Expected::Class(SyntaxClass::Term),
                    Expected::Word(GrammarWord::Default),
                    Expected::Token(SyntaxKind::DOT),
                    Expected::Token(SyntaxKind::COMMA),
                ]),
                found: SyntaxKind::R_PAREN,
                hint: None,
            },
            at(3, 4),
        );
        assert_eq!(
            error.to_diagnostic().message(),
            "expected `.`, `,`, `default`, or a term, found `)`"
        );
    }

    #[test]
    fn a_hint_lowers_to_a_help() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[Expected::Class(SyntaxClass::Term)]),
                found: SyntaxKind::R_PAREN,
                hint: Some(Hint::TrailingCommaInArguments),
            },
            at(3, 4),
        );
        let lowered = error.to_diagnostic();
        assert_eq!(lowered.helps().len(), 1);
        assert!(lowered.helps()[0].contains("trailing comma"));
    }

    #[test]
    fn a_kind_never_lowers_to_an_empty_headline() {
        let mut catalog = SourceSet::new();
        let file = catalog.add("x.lp".to_owned(), "p(\"a\\qb\").\n".to_owned()).expect("admits");
        for kind in representatives() {
            let error = SyntaxError::new(
                kind,
                Location {
                    source: file,
                    span: Span::new(ByteOffset::new(2), ByteOffset::new(3)).expect("ordered"),
                },
            );
            let lowered = error.to_diagnostic();
            assert!(!lowered.message().is_empty());
            assert!(human(&lowered, &catalog).starts_with(&format!("{}[", lowered.severity())));
        }
    }

    #[test]
    fn grammar_words_display_their_spellings() {
        assert_eq!(GrammarWord::Override.to_string(), "override");
        assert_eq!(GrammarWord::Directive.to_string(), "directive");
    }
}
```

The golden file the snapshot compares against —
`crates/themelios-syntax/tests/golden/identity-table.txt`, Appendix B
row by row (bless it once with `GOLDEN_BLESS=1 cargo test -p
themelios-syntax --lib diagnostic`, then confirm it reads exactly):

```
syntax::unexpected-characters error
syntax::unknown-hash-word error
syntax::malformed-string error
syntax::unterminated-block-comment error
syntax::unterminated-script error
syntax::anonymous-in-theory-expression error
syntax::unexpected-token error
syntax::unexpected-end-of-input error
syntax::nesting-too-deep error
syntax::aspif-input error
syntax::token-source-breach error
syntax::form-not-allowed-here error
syntax::misplaced-doc-comment warning
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib diagnostic`
Expected: compile error — `cannot find type SyntaxError`.

- [ ] **Step 3: Write the module**

Prepend to `src/diagnostic.rs`:

```rust
//! The tier's typed diagnostics (docs/design/syntax.md §7): a fully
//! typed value — matchable, exhaustive, carrying the expected set as a
//! real type — lowering into base's normal form for rendering and
//! transport. Identities and severities are Appendix B's; message texts
//! are presentation, held by the golden corpus.

use std::collections::BTreeSet;
use std::fmt;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity, ToDiagnostic};
use themelios_base::span::{ByteOffset, Location};

use crate::tree::SyntaxKind;

/// One syntax diagnostic: what happened, where, and what would settle
/// it. Located by construction — the primary span is required.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SyntaxError {
    kind: SyntaxErrorKind,
    primary: Location,
    related: BTreeSet<Related>,
}

impl SyntaxError {
    /// A diagnostic of `kind` at `primary`, with no related loci yet.
    pub(crate) fn new(kind: SyntaxErrorKind, primary: Location) -> SyntaxError {
        SyntaxError { kind, primary, related: BTreeSet::new() }
    }

    /// The diagnostic with a related locus added; a locus already
    /// present stays once — set semantics.
    #[must_use]
    pub(crate) fn with_related(mut self, related: Related) -> SyntaxError {
        self.related.insert(related);
        self
    }

    /// What happened, typed. Total, O(1).
    pub fn kind(&self) -> &SyntaxErrorKind {
        &self.kind
    }

    /// The stable identity, derived from the kind (Appendix B). Total, O(1).
    pub fn id(&self) -> DiagnosticId {
        self.kind.id()
    }

    /// The severity, derived from the kind (Appendix B). Total, O(1).
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// The primary location. Total, O(1).
    pub fn primary(&self) -> Location {
        self.primary
    }

    /// The related loci, a set. Total, O(1).
    pub fn related(&self) -> &BTreeSet<Related> {
        &self.related
    }
}

/// A secondary locus, typed: what the location is, so that its text is
/// derived at lowering like every other text on the diagnostic and a
/// wording change is never a parser change. Closed; a locus is admitted
/// here when a golden shows a reader needs it, as a `Hint` is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Related {
    /// What the location is.
    pub locus: RelatedLocus,
    /// Where.
    pub location: Location,
}

/// The kinds of related locus.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RelatedLocus {
    /// "the statement began here"
    StatementBegan,
    /// "to close this `{`" — the opener a missing closer answers to.
    ToClose(SyntaxKind),
    /// "the literal, whole" — the string a bad escape sits in.
    LiteralExtent,
}

/// The closed roster of what can go wrong, each with its typed payload.
/// Declared in the order a parse meets them: lexical, then structural,
/// then the restrictions, then the warnings.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SyntaxErrorKind {
    /// Characters that begin no token, one run (syntax.md §4.5).
    UnexpectedCharacters,
    /// A `#`-word that spells no keyword (grammar §4.5).
    UnknownHashWord,
    /// A string literal the dialect's rule refuses (grammar §4.4, §6.2).
    MalformedString {
        /// What broke the literal.
        defect: StringDefect,
    },
    /// `%*` never closed (grammar §4.1, §6.3).
    UnterminatedBlockComment,
    /// A `#script` region with no `#end` (grammar §4.8).
    UnterminatedScript,
    /// `_` inside a theory expression, where none is admitted (grammar §4.7).
    AnonymousInTheoryExpression,
    /// A token the grammar does not admit here.
    UnexpectedToken {
        /// What the parser would have accepted.
        expected: ExpectedSet,
        /// The kind it met.
        found: SyntaxKind,
        /// A characteristic mistake recognized here, if any.
        hint: Option<Hint>,
    },
    /// The input ended where more was expected.
    UnexpectedEndOfInput {
        /// What the parser would have accepted.
        expected: ExpectedSet,
        /// A characteristic mistake recognized here, if any.
        hint: Option<Hint>,
    },
    /// A bracket that would open a frame past `MAX_NESTING_DEPTH`
    /// (syntax.md §6.6).
    NestingTooDeep {
        /// The bound that was reached.
        depth: u32,
    },
    /// The input is aspif, not a program (grammar §4.9).
    AspifInput,
    /// The token source breached a law the parser can witness (syntax.md §4.3).
    TokenSourceBreach {
        /// Which breach.
        breach: SourceBreach,
    },
    /// A term form the position's restriction forbids (syntax.md §6.2).
    FormNotAllowedHere {
        /// The form written.
        form: RestrictedForm,
        /// The restriction in force.
        context: Restriction,
    },
    /// A doc comment that documents nothing (grammar §4.1, §5.11) — a
    /// warning; the input stays a member.
    MisplacedDocComment {
        /// Why it documents nothing.
        reason: MisplacedDoc,
    },
}

/// What broke a string literal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StringDefect {
    /// A raw line break inside the literal (the clingo rule).
    RawLineBreak,
    /// A backslash before a character the rule does not admit; the
    /// character.
    InvalidEscape(char),
    /// End of input before the closing quote.
    Unterminated,
}

/// The term forms a restriction can forbid (syntax.md §6.2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RestrictedForm {
    /// A variable.
    Variable,
    /// The anonymous variable.
    AnonymousVariable,
    /// A pool.
    Pool,
    /// An interval.
    Interval,
    /// An `@`-call.
    ExternalCall,
    /// An absolute value over a pooled argument.
    PooledAbsoluteValue,
}

/// The restriction contexts (grammar §5.9, §5.10).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Restriction {
    /// `#const`'s constant term.
    ConstantTerm,
    /// The term-value sublanguage.
    TermValue,
}

/// Why a doc comment documents nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MisplacedDoc {
    /// The run is followed by no statement.
    NoStatementFollows,
    /// The line stands inside a statement.
    InsideStatement,
}

/// The two breaches of the token-source laws the parser can witness in
/// one pass, both at an offset it reached by tiling: `Tiling` — an
/// `EOF` before the text's end, or a token running past it, the kind
/// and length saying which; `Refusal` — the door refused where it owed
/// a token. The slice law is trusted and determinism unobservable in
/// one pass, so neither appears here — the checker's
/// `TokenSourceLawViolation` is the wider type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SourceBreach {
    /// Tiling broke.
    Tiling {
        /// Where.
        at: ByteOffset,
        /// The token answered there.
        token: SyntaxKind,
        /// Its length.
        len: u32,
    },
    /// The door refused a position it owed a token.
    Refusal {
        /// Where.
        at: ByteOffset,
    },
}

/// The characteristic mistakes the parser recognizes at an unexpected
/// token — each a shape the grammar or the corpus names, each carrying
/// one help text at lowering. Closed; a hint is admitted here when a
/// golden case shows a reader needs it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Hint {
    /// `f(a,)` — no trailing comma in an argument list (grammar §5.1).
    TrailingCommaInArguments,
    /// A `?` ending the input under the clingo dialect — the ASP-Core-2
    /// query mark (grammar §6.1).
    QueryMarkNeedsAspCore2,
    /// Two numerals adjacent — a leading zero (grammar §4.3).
    LeadingZeroNumeral,
    /// `p(X) : | q(X)` — the empty-conditioned element before `|`
    /// (grammar §5.5); write `;`.
    EmptyConditionBeforePipe,
    /// `#heuristic … .` without its bracket (grammar §5.9).
    HeuristicNeedsAnnotation,
}

/// What the parser would have accepted at a point: tokens by kind,
/// identifiers by spelling where the grammar wants a word, and grammar
/// classes where listing tokens would mislead. A set — order carries no
/// meaning, duplicates are defects, and rendering derives its order
/// (kinds, then words, then classes).
pub type ExpectedSet = BTreeSet<Expected>;

/// One expectation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Expected {
    /// A token by kind.
    Token(SyntaxKind),
    /// An identifier by spelling.
    Word(GrammarWord),
    /// A grammar class.
    Class(SyntaxClass),
}

/// The words the grammar wants by spelling where it has no token for
/// them (grammar §5.9): the ten identifiers matched by spelling in
/// `#const` annotations and `#theory` definitions. Closed, so an
/// expected set is matchable and a golden can enumerate it; `Display`
/// is the spelling.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GrammarWord {
    /// `default`
    Default,
    /// `override`
    Override,
    /// `unary`
    Unary,
    /// `binary`
    Binary,
    /// `left`
    Left,
    /// `right`
    Right,
    /// `head`
    Head,
    /// `body`
    Body,
    /// `any`
    Any,
    /// `directive`
    Directive,
}

impl GrammarWord {
    /// The spelling the grammar matches.
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            GrammarWord::Default => "default",
            GrammarWord::Override => "override",
            GrammarWord::Unary => "unary",
            GrammarWord::Binary => "binary",
            GrammarWord::Left => "left",
            GrammarWord::Right => "right",
            GrammarWord::Head => "head",
            GrammarWord::Body => "body",
            GrammarWord::Any => "any",
            GrammarWord::Directive => "directive",
        }
    }
}

impl fmt::Display for GrammarWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spelling())
    }
}

/// The grammar's classes a consumer or a message names as one thing.
/// Closed; each is a nonterminal or a family of the grammar of record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SyntaxClass {
    /// A statement.
    Statement,
    /// A head.
    Head,
    /// A body element.
    BodyElement,
    /// A literal.
    Literal,
    /// An atom.
    Atom,
    /// A term.
    Term,
    /// A theory term.
    TheoryTerm,
    /// A theory operator.
    TheoryOperator,
    /// An aggregate guard.
    Guard,
    /// A signature `name/arity`.
    Signature,
    /// A condition.
    Condition,
    /// A bracketed annotation.
    Annotation,
    /// End of input.
    EndOfInput,
}

const NAMESPACE: &str = "syntax";

impl SyntaxErrorKind {
    /// The identity (Appendix B).
    fn id(&self) -> DiagnosticId {
        let name = match self {
            SyntaxErrorKind::UnexpectedCharacters => "unexpected-characters",
            SyntaxErrorKind::UnknownHashWord => "unknown-hash-word",
            SyntaxErrorKind::MalformedString { .. } => "malformed-string",
            SyntaxErrorKind::UnterminatedBlockComment => "unterminated-block-comment",
            SyntaxErrorKind::UnterminatedScript => "unterminated-script",
            SyntaxErrorKind::AnonymousInTheoryExpression => "anonymous-in-theory-expression",
            SyntaxErrorKind::UnexpectedToken { .. } => "unexpected-token",
            SyntaxErrorKind::UnexpectedEndOfInput { .. } => "unexpected-end-of-input",
            SyntaxErrorKind::NestingTooDeep { .. } => "nesting-too-deep",
            SyntaxErrorKind::AspifInput => "aspif-input",
            SyntaxErrorKind::TokenSourceBreach { .. } => "token-source-breach",
            SyntaxErrorKind::FormNotAllowedHere { .. } => "form-not-allowed-here",
            SyntaxErrorKind::MisplacedDocComment { .. } => "misplaced-doc-comment",
        };
        DiagnosticId::new(NAMESPACE, name)
    }

    /// The severity (Appendix B): every kind an error but the doc warning.
    fn severity(&self) -> Severity {
        match self {
            SyntaxErrorKind::MisplacedDocComment { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// The headline, derived from the kind and its payload.
    fn headline(&self) -> String {
        match self {
            SyntaxErrorKind::UnexpectedCharacters => "unexpected characters".to_owned(),
            SyntaxErrorKind::UnknownHashWord => "unknown `#`-word".to_owned(),
            SyntaxErrorKind::MalformedString { defect: StringDefect::RawLineBreak } => {
                "string literal broken by a line break".to_owned()
            }
            SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape('\n') } => {
                "string literal with a backslash at the end of its line".to_owned()
            }
            SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape(c) } => {
                format!("invalid escape `\\{}` in string literal", c.escape_debug())
            }
            SyntaxErrorKind::MalformedString { defect: StringDefect::Unterminated } => {
                "unterminated string literal".to_owned()
            }
            SyntaxErrorKind::UnterminatedBlockComment => "unterminated block comment".to_owned(),
            SyntaxErrorKind::UnterminatedScript => "unterminated `#script` region".to_owned(),
            SyntaxErrorKind::AnonymousInTheoryExpression => {
                "anonymous variable inside a theory expression".to_owned()
            }
            SyntaxErrorKind::UnexpectedToken { expected, found, .. } => {
                format!("expected {}, found {}", render_expected(expected), describe(*found))
            }
            SyntaxErrorKind::UnexpectedEndOfInput { expected, .. } => {
                format!("expected {}, found end of input", render_expected(expected))
            }
            SyntaxErrorKind::NestingTooDeep { depth } => {
                format!("brackets nested deeper than {depth} levels")
            }
            SyntaxErrorKind::AspifInput => "aspif input, not a program".to_owned(),
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Tiling { .. } } => {
                "the token source breached its tiling law".to_owned()
            }
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Refusal { .. } } => {
                "the token source refused a position it owes a token".to_owned()
            }
            SyntaxErrorKind::FormNotAllowedHere { form, context } => {
                format!("{} is not allowed in {}", describe_form(*form), describe_restriction(*context))
            }
            SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::NoStatementFollows } => {
                "doc comment followed by no statement".to_owned()
            }
            SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::InsideStatement } => {
                "doc comment inside a statement".to_owned()
            }
        }
    }

    /// The primary label's text.
    fn primary_text(&self) -> Option<String> {
        Some(
            match self {
                SyntaxErrorKind::UnexpectedCharacters => "no token begins here",
                SyntaxErrorKind::UnknownHashWord => "not a keyword of the language",
                SyntaxErrorKind::MalformedString { defect: StringDefect::RawLineBreak } => {
                    "the literal ends at the line break without its closing quote"
                }
                SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape(_) } => {
                    "not one of the escapes `\\\"`, `\\\\`, `\\n`"
                }
                SyntaxErrorKind::MalformedString { defect: StringDefect::Unterminated } => {
                    "opened here and never closed"
                }
                SyntaxErrorKind::UnterminatedBlockComment => "opened here and never closed",
                SyntaxErrorKind::UnterminatedScript => "the region begins here and no `#end` follows",
                SyntaxErrorKind::AnonymousInTheoryExpression => "`_` is not admitted here",
                SyntaxErrorKind::UnexpectedToken { .. } => return None,
                SyntaxErrorKind::UnexpectedEndOfInput { .. } => "the input ends here",
                SyntaxErrorKind::NestingTooDeep { .. } => {
                    "this bracket would open one level too many; the rest of the statement is carried unparsed"
                }
                SyntaxErrorKind::AspifInput => "the aspif header",
                SyntaxErrorKind::TokenSourceBreach { .. } => "here",
                SyntaxErrorKind::FormNotAllowedHere { .. } => "not allowed here",
                SyntaxErrorKind::MisplacedDocComment { .. } => "a plain comment here",
            }
            .to_owned(),
        )
    }

    /// The notes the kind derives.
    fn notes(&self) -> Vec<String> {
        match self {
            SyntaxErrorKind::UnknownHashWord => vec![
                "`#`-words are recognized whole: a keyword extended by name characters is one unknown word"
                    .to_owned(),
            ],
            SyntaxErrorKind::UnterminatedBlockComment => vec![
                "under the clingo dialect a `%` inside a block comment silences the rest of its line, closers included"
                    .to_owned(),
            ],
            SyntaxErrorKind::AspifInput => vec![
                "the input is in the intermediate format a solver reads; the syntax tier does not parse it"
                    .to_owned(),
            ],
            SyntaxErrorKind::MisplacedDocComment { .. } => vec![
                "a `%!` line documents the statement that follows it; the program is unchanged".to_owned(),
            ],
            _ => Vec::new(),
        }
    }

    /// The helps the kind derives: one per hint.
    fn helps(&self) -> Vec<String> {
        let hint = match self {
            SyntaxErrorKind::UnexpectedToken { hint, .. }
            | SyntaxErrorKind::UnexpectedEndOfInput { hint, .. } => *hint,
            _ => None,
        };
        hint.map(help_text).into_iter().collect()
    }
}

/// The help a hint carries.
fn help_text(hint: Hint) -> String {
    match hint {
        Hint::TrailingCommaInArguments => {
            "remove the trailing comma: an argument list takes no trailing comma".to_owned()
        }
        Hint::QueryMarkNeedsAspCore2 => {
            "a final `?` is the ASP-Core-2 query mark; parse under the ASP-Core-2 dialect to read it as a query"
                .to_owned()
        }
        Hint::LeadingZeroNumeral => {
            "decimal numerals take no leading zero: `007` is three numerals".to_owned()
        }
        Hint::EmptyConditionBeforePipe => {
            "write `;` here: an empty condition directly before `|` does not parse".to_owned()
        }
        Hint::HeuristicNeedsAnnotation => {
            "`#heuristic` takes its bracket after the dot: `[weight@priority, modifier]`".to_owned()
        }
    }
}

/// The text a related locus carries.
fn related_text(locus: RelatedLocus) -> String {
    match locus {
        RelatedLocus::StatementBegan => "the statement began here".to_owned(),
        RelatedLocus::ToClose(opener) => format!("to close this {}", describe(opener)),
        RelatedLocus::LiteralExtent => "the literal, whole".to_owned(),
    }
}

/// An expected set in words: kinds, then words, then classes — the
/// set's own order — joined as a list.
fn render_expected(expected: &ExpectedSet) -> String {
    let items: Vec<String> = expected
        .iter()
        .map(|item| match item {
            Expected::Token(kind) => describe(*kind),
            Expected::Word(word) => format!("`{word}`"),
            Expected::Class(class) => describe_class(*class).to_owned(),
        })
        .collect();
    match items.as_slice() {
        [] => "nothing".to_owned(),
        [one] => one.clone(),
        [first, second] => format!("{first} or {second}"),
        [init @ .., last] => format!("{}, or {last}", init.join(", ")),
    }
}

/// A token kind in words: its spelling in backticks where it has one,
/// its class otherwise.
fn describe(kind: SyntaxKind) -> String {
    let spelling = match kind {
        SyntaxKind::WHITESPACE => return "whitespace".to_owned(),
        SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT | SyntaxKind::SHEBANG_COMMENT => {
            return "a comment".to_owned();
        }
        SyntaxKind::DOC_COMMENT => return "a doc comment".to_owned(),
        SyntaxKind::IDENT => return "an identifier".to_owned(),
        SyntaxKind::VARIABLE => return "a variable".to_owned(),
        SyntaxKind::ANONYMOUS => "_",
        SyntaxKind::NUMBER => return "a number".to_owned(),
        SyntaxKind::STRING => return "a string".to_owned(),
        SyntaxKind::KW_CONST => "#const",
        SyntaxKind::KW_COUNT => "#count",
        SyntaxKind::KW_DEFINED => "#defined",
        SyntaxKind::KW_EDGE => "#edge",
        SyntaxKind::KW_EXTERNAL => "#external",
        SyntaxKind::KW_FALSE => "#false",
        SyntaxKind::KW_HEURISTIC => "#heuristic",
        SyntaxKind::KW_INCLUDE => "#include",
        SyntaxKind::KW_INF => "#inf",
        SyntaxKind::KW_MAX => "#max",
        SyntaxKind::KW_MAXIMIZE => "#maximize",
        SyntaxKind::KW_MIN => "#min",
        SyntaxKind::KW_MINIMIZE => "#minimize",
        SyntaxKind::KW_PROGRAM => "#program",
        SyntaxKind::KW_PROJECT => "#project",
        SyntaxKind::KW_SCRIPT => "#script",
        SyntaxKind::KW_SHOW => "#show",
        SyntaxKind::KW_SUM => "#sum",
        SyntaxKind::KW_SUM_PLUS => "#sum+",
        SyntaxKind::KW_SUP => "#sup",
        SyntaxKind::KW_THEORY => "#theory",
        SyntaxKind::KW_TRUE => "#true",
        SyntaxKind::KW_NOT => "not",
        SyntaxKind::KW_END => "#end",
        SyntaxKind::DOT => ".",
        SyntaxKind::DOTDOT => "..",
        SyntaxKind::COMMA => ",",
        SyntaxKind::SEMICOLON => ";",
        SyntaxKind::COLON => ":",
        SyntaxKind::NECK => ":-",
        SyntaxKind::WEAK_NECK => ":~",
        SyntaxKind::PIPE => "|",
        SyntaxKind::L_PAREN => "(",
        SyntaxKind::R_PAREN => ")",
        SyntaxKind::L_BRACKET => "[",
        SyntaxKind::R_BRACKET => "]",
        SyntaxKind::L_BRACE => "{",
        SyntaxKind::R_BRACE => "}",
        SyntaxKind::PLUS => "+",
        SyntaxKind::MINUS => "-",
        SyntaxKind::STAR => "*",
        SyntaxKind::STAR_STAR => "**",
        SyntaxKind::SLASH => "/",
        SyntaxKind::BACKSLASH => "\\",
        SyntaxKind::CARET => "^",
        SyntaxKind::AMPERSAND => "&",
        SyntaxKind::TILDE => "~",
        SyntaxKind::QUESTION => "?",
        SyntaxKind::AT => "@",
        SyntaxKind::EQ => "=",
        SyntaxKind::NEQ => "!=",
        SyntaxKind::LT => "<",
        SyntaxKind::LE => "<=",
        SyntaxKind::GT => ">",
        SyntaxKind::GE => ">=",
        SyntaxKind::THEORY_OP => return "a theory operator".to_owned(),
        SyntaxKind::SCRIPT_BODY => return "a script body".to_owned(),
        SyntaxKind::SPLICE => return "a splice".to_owned(),
        SyntaxKind::ERROR => return "unrecognized input".to_owned(),
        SyntaxKind::EOF => return "end of input".to_owned(),
        node => return format!("{node}"),
    };
    format!("`{spelling}`")
}

fn describe_class(class: SyntaxClass) -> &'static str {
    match class {
        SyntaxClass::Statement => "a statement",
        SyntaxClass::Head => "a head",
        SyntaxClass::BodyElement => "a body element",
        SyntaxClass::Literal => "a literal",
        SyntaxClass::Atom => "an atom",
        SyntaxClass::Term => "a term",
        SyntaxClass::TheoryTerm => "a theory term",
        SyntaxClass::TheoryOperator => "a theory operator",
        SyntaxClass::Guard => "a guard",
        SyntaxClass::Signature => "a signature",
        SyntaxClass::Condition => "a condition",
        SyntaxClass::Annotation => "an annotation",
        SyntaxClass::EndOfInput => "end of input",
    }
}

fn describe_form(form: RestrictedForm) -> &'static str {
    match form {
        RestrictedForm::Variable => "a variable",
        RestrictedForm::AnonymousVariable => "the anonymous variable",
        RestrictedForm::Pool => "a pool",
        RestrictedForm::Interval => "an interval",
        RestrictedForm::ExternalCall => "an `@`-call",
        RestrictedForm::PooledAbsoluteValue => "an absolute value over a pooled argument",
    }
}

fn describe_restriction(context: Restriction) -> &'static str {
    match context {
        Restriction::ConstantTerm => "a `#const` term",
        Restriction::TermValue => "a term value",
    }
}

impl ToDiagnostic for SyntaxError {
    /// The base normal form: identity, severity, the headline, the
    /// primary label, the related loci as secondary labels, and the
    /// notes and helps the kind derives. O(payload).
    fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            self.kind.id(),
            self.kind.severity(),
            self.kind.headline(),
            Label { location: self.primary, message: self.kind.primary_text() },
        )
        .expect("every headline is non-empty by construction");
        for related in &self.related {
            diagnostic = diagnostic.with_secondary(Label {
                location: related.location,
                message: Some(related_text(related.locus)),
            });
        }
        for note in self.kind.notes() {
            diagnostic = diagnostic.with_note(note);
        }
        for help in self.kind.helps() {
            diagnostic = diagnostic.with_help(help);
        }
        diagnostic
    }
}
```

- [ ] **Step 4: Bless the identity table and run the tests**

Run: `GOLDEN_BLESS=1 cargo test -p themelios-syntax --lib diagnostic`,
read `tests/golden/identity-table.txt` against Appendix B and Step 1's
listing (identical), then `cargo test -p themelios-syntax --lib diagnostic`
Expected: 7 passed.

- [ ] **Step 5: Run the full gate, then commit**

Run the four gate commands. Expected: green — `clippy::too_many_lines`
may fire on `describe`; it is one table, and if the lint fires it is
allowed on that function with the argument that a spelling table is a
table.

```bash
git add crates/themelios-syntax
git commit -m "Add the typed syntax diagnostics: the roster, its identities and severities, the expected set, and the lowering to the base model"
```

---

### Task 7: The parser core — the token cursor, the builder under the trivia law, recovery, the entry points, and the roots

**Files:**
- Create: `crates/themelios-syntax/src/parse/mod.rs`,
  `crates/themelios-syntax/src/parse/machine.rs`,
  `crates/themelios-syntax/src/ast/mod.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod parse;` and
  `pub mod ast;`), `crates/themelios-syntax/src/lexer.rs` (add the
  error-token classifier the parser reads)

**Derives:** syntax.md §4.2 (the parser owns the modes; a lexical
diagnostic is raised when an `ERROR` token is placed), §4.3 (the two
breaches the parser witnesses), §4.5 (one lexical diagnostic per `ERROR`
token placed; the escape's primary and its related locus), §5.4 (law 1
text, law 2 trivia placement, law 4 determinism), §5.5 (`Parse<T>`),
§6.1 (entry points and roots; what each entry admits; the fragment
container shape), §6.3 (the aspif dispatch; the docs production's
mechanism), §6.4, §6.7 (the two forms of defect; the program-level row;
end of input anywhere), §6.8, §8.1 (the roots), §12.1.

**Interfaces:**
- Consumes: `lexer::lex`, `token::{TokenSource, LexMode, Token}`,
  `tree::*`, `diagnostic::*`, base's `Location`, `Span`, `ByteOffset`,
  `SourceId`, rowan's `GreenNodeBuilder`, `Checkpoint`.
- Produces: `parse::Parse<T>` (`syntax`, `tree`, `green`,
  `diagnostics`, `has_errors`, `is_incomplete`, `source`, `dialect`,
  `entry`, `location`; `string_value` lands in Task 13),
  `parse::EntryPoint`, `parse::{parse, parse_program, parse_statement,
  parse_term, parse_term_value}`; `ast::{Program, StatementFragment,
  TermFragment}` and the crate-private `ast_node!` macro; the
  crate-private `parse::machine::Parser` with its cursor, builder, and
  recovery methods that Tasks 8–11 extend (`peek`, `peek_text`,
  `lookahead`, `bump`, `eat`, `start_node`, `start_node_at`,
  `checkpoint`, `finish_node`, `unexpected`, `unexpected_end`,
  `wrap_unexpected`, `skip_into_error`, `set_mode`, `mode`,
  `location`, …); `lexer::classify_error`.

- [ ] **Step 1: Write the error-token classifier**

Append to `src/lexer.rs`, above its test module (the parser reads
this; a foreign token source's `ERROR` tokens classify the same way,
by text):

```rust
/// What a lexical `ERROR` token is, read from its text, the character
/// after it, and the mode and dialect it was formed under
/// (docs/design/syntax.md §4.5): the parser raises exactly this
/// diagnostic when it places the token.
pub(crate) struct LexicalDefect {
    /// The diagnostic's kind.
    pub(crate) kind: crate::diagnostic::SyntaxErrorKind,
    /// The primary span, relative to the token's start.
    pub(crate) primary: std::ops::Range<usize>,
    /// Whether the literal's whole extent is a related locus (a bad
    /// escape inside a string).
    pub(crate) literal_extent: bool,
}

/// Classifies an `ERROR` token of `text`, followed in its source by
/// `following` (`None` at end of input), formed under `mode` and
/// `dialect`. Total.
pub(crate) fn classify_error(
    text: &str,
    following: Option<char>,
    mode: LexMode,
    dialect: Dialect,
) -> LexicalDefect {
    use crate::diagnostic::SyntaxErrorKind;
    let whole = 0..text.len();
    if mode == LexMode::ScriptBody {
        return LexicalDefect { kind: SyntaxErrorKind::UnterminatedScript, primary: whole, literal_extent: false };
    }
    if text.starts_with("%*") {
        return LexicalDefect {
            kind: SyntaxErrorKind::UnterminatedBlockComment,
            primary: 0..2,
            literal_extent: false,
        };
    }
    if text.starts_with('#') {
        return LexicalDefect { kind: SyntaxErrorKind::UnknownHashWord, primary: whole, literal_extent: false };
    }
    if text.starts_with('"') {
        return string_defect(text, following, dialect);
    }
    if mode == LexMode::Theory && text.chars().all(|c| c == '_') {
        return LexicalDefect {
            kind: SyntaxErrorKind::AnonymousInTheoryExpression,
            primary: whole,
            literal_extent: false,
        };
    }
    LexicalDefect { kind: SyntaxErrorKind::UnexpectedCharacters, primary: whole, literal_extent: false }
}

/// The defect of a malformed string token: unterminated at end of
/// input; broken by a raw line break (the clingo rule); or, when the
/// token closed, the first bad escape, at the backslash.
fn string_defect(text: &str, following: Option<char>, dialect: Dialect) -> LexicalDefect {
    use crate::diagnostic::{StringDefect, SyntaxErrorKind};
    let bytes = text.as_bytes();
    let closed = bytes.len() > 1 && bytes[bytes.len() - 1] == b'"' && !text.ends_with("\\\"");
    if dialect == Dialect::Clingo {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                match text[i + 1..].chars().next() {
                    Some('"' | '\\' | 'n') => i += 2,
                    Some(bad) => {
                        return LexicalDefect {
                            kind: SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape(bad) },
                            primary: i..i + 1 + bad.len_utf8(),
                            literal_extent: true,
                        };
                    }
                    None => {
                        let bad = following.unwrap_or('\n');
                        return LexicalDefect {
                            kind: SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape(bad) },
                            primary: i..i + 1,
                            literal_extent: true,
                        };
                    }
                }
            } else {
                i += char_len(text, i);
            }
        }
        if !closed && following == Some('\n') {
            return LexicalDefect {
                kind: SyntaxErrorKind::MalformedString { defect: StringDefect::RawLineBreak },
                primary: 0..text.len(),
                literal_extent: false,
            };
        }
    }
    LexicalDefect {
        kind: SyntaxErrorKind::MalformedString { defect: StringDefect::Unterminated },
        primary: 0..1,
        literal_extent: false,
    }
}
```

The `closed` check reads whether the token ends in an unescaped quote:
a token that closed but carried a bad escape is classified by the escape
above before `closed` is consulted; `closed` only keeps a `\"`-final
unterminated token from being read as closed.

- [ ] **Step 2: Write the roots and the wrapper macro**

Append `pub mod ast;` and `pub mod parse;` to `src/lib.rs` (order:
`parse` before `ast`, the design's order). Create `src/ast/mod.rs`:

```rust
//! The typed AST (docs/design/syntax.md §8): cheap wrappers over the red
//! cursors, one per node kind, an enum per grammar class, accessors
//! mirroring the productions' slots. This file holds the roots and the
//! wrapper macro; the wrappers, enums, and traits land in `nodes`, the
//! token wrappers in `tokens`.

use crate::tree::{Asp, AstNode, SyntaxKind, SyntaxNode};

/// Declares one wrapper over one node kind: a view (`!Send`) that casts
/// on the kind, derives its equality and hash positionally through the
/// cursor, and offers `syntax()` as the escape to the tree.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident => $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

pub(crate) use ast_node;

ast_node! {
    /// Grammar §5.11's `program`: the program entry's root.
    Program => PROGRAM
}

ast_node! {
    /// The statement entry's root: leading trivia, the statement when the
    /// input held one, trailing trivia, and an `ERROR` node when input
    /// remained (docs/design/syntax.md §6.1).
    StatementFragment => STATEMENT_FRAGMENT
}

ast_node! {
    /// The term and term-value entries' root, of the same shape as the
    /// statement fragment's (docs/design/syntax.md §6.1).
    TermFragment => TERM_FRAGMENT
}
```

- [ ] **Step 3: Write the failing tests for the parse module**

Create `src/parse/mod.rs` holding only this test module (the module
declarations and items come in Steps 4–5):

```rust
#[cfg(test)]
mod tests {
    use themelios_base::diagnostic::Severity;
    use themelios_base::line::PositionRefusal;
    use themelios_base::source::{Source, SourceId};
    use themelios_base::span::ByteOffset;

    use super::*;
    use crate::diagnostic::{Expected, MisplacedDoc, SourceBreach, SyntaxClass, SyntaxErrorKind};
    use crate::token::{LexMode, Token, TokenSource};
    use crate::tree::SyntaxKind;

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(7), text.to_owned()).expect("test text admits")
    }

    /// The tree's shape as `KIND@start..end` lines, indented by depth —
    /// rowan's alternate `Debug`.
    fn dump<T: AstNode<Language = Asp>>(parse: &Parse<T>) -> String {
        format!("{:#?}", parse.syntax())
    }

    #[test]
    fn an_empty_text_is_an_empty_program() {
        let source = admitted("");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), "");
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
        assert!(!parse.has_errors());
        assert!(!parse.is_incomplete());
        assert!(parse.diagnostics().is_empty());
        assert_eq!(parse.entry(), EntryPoint::Program);
        assert_eq!(parse.dialect(), Dialect::Clingo);
        assert_eq!(parse.source(), SourceId::new(7));
    }

    #[test]
    fn trivia_alone_belongs_to_the_program() {
        let source = admitted("  % a comment\n%* block *%\n#! shebang\n");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert!(!parse.has_errors());
        assert_eq!(parse.syntax().children().count(), 0);
        assert_eq!(parse.syntax().children_with_tokens().count(), 7);
    }

    #[test]
    fn garbage_is_carried_losslessly_in_error_nodes_with_the_lexical_diagnostics() {
        let source = admitted("$$$ p. ééé");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert!(parse.has_errors());
        let ids: Vec<String> = parse.diagnostics().iter().map(|d| d.id().to_string()).collect();
        assert!(ids.iter().all(|id| id == "syntax::unexpected-characters"), "{ids:?}");
        assert_eq!(ids.len(), 2, "one lexical diagnostic per ERROR token placed");
        assert!(parse.syntax().children().all(|node| node.kind() == SyntaxKind::ERROR));
    }

    #[test]
    fn a_doc_run_that_no_statement_follows_is_diagnosed_trivia() {
        let source = admitted("%! a\n%! b\n");
        let parse = parse(&source, Dialect::Clingo);
        assert!(!parse.has_errors(), "warnings do not affect membership");
        let kinds: Vec<_> = parse.diagnostics().iter().map(SyntaxError::kind).cloned().collect();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.iter().all(|kind| matches!(
            kind,
            SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::NoStatementFollows }
        )));
        assert!(parse.diagnostics().iter().all(|d| d.severity() == Severity::Warning));
    }

    #[test]
    fn aspif_input_is_one_error_child_with_one_diagnostic() {
        let source = admitted("asp 1 0 0\n1 0 1 1 0 0\n0\n");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert_eq!(parse.diagnostics().len(), 1);
        assert_eq!(parse.diagnostics()[0].id().to_string(), "syntax::aspif-input");
        assert_eq!(parse.syntax().children_with_tokens().count(), 1);
        assert_eq!(dump(&parse).lines().count(), 2);
    }

    #[test]
    fn a_source_that_breaches_tiling_stops_the_parse_with_its_diagnostic() {
        struct EarlyEnd(Source);
        impl TokenSource for EarlyEnd {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
                if at.get() >= 3 {
                    return Ok(Token { kind: SyntaxKind::EOF, text: "" });
                }
                crate::lexer::Lexer::new(&self.0, Dialect::Clingo).token_at(at, mode)
            }
        }
        let source = EarlyEnd(admitted("$$$ more"));
        let parse = parse_program(&source);
        assert_eq!(parse.syntax().text(), "$$$", "the prefix tiled");
        assert!(parse.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Tiling { at, token: SyntaxKind::EOF, len: 0 } }
                if *at == ByteOffset::new(3)
        )));
    }

    #[test]
    fn a_source_that_refuses_a_position_stops_the_parse_with_its_diagnostic() {
        struct Refusing(Source);
        impl TokenSource for Refusing {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
                if at.get() >= 3 {
                    return Err(PositionRefusal::NotCharBoundary(themelios_base::source::NotCharBoundary {
                        offset: at,
                    }));
                }
                crate::lexer::Lexer::new(&self.0, Dialect::Clingo).token_at(at, mode)
            }
        }
        let source = Refusing(admitted("$$$ more"));
        let parse = parse_program(&source);
        assert_eq!(parse.syntax().text(), "$$$");
        assert!(parse.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Refusal { at } } if *at == ByteOffset::new(3)
        )));
    }

    #[test]
    fn the_fragment_entries_yield_their_container_roots_on_empty_input() {
        let source = admitted("  ");
        let statement = parse_statement(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(statement.syntax().kind(), SyntaxKind::STATEMENT_FRAGMENT);
        assert_eq!(statement.syntax().text(), "  ");
        assert!(!statement.has_errors());
        assert_eq!(statement.entry(), EntryPoint::Statement);
        let term = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(term.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(term.entry(), EntryPoint::Term);
        let value = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(value.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(value.entry(), EntryPoint::TermValue);
    }

    #[test]
    fn input_after_a_fragment_is_an_error_node_expecting_end_of_input() {
        let source = admitted("p q");
        let fragment = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(fragment.syntax().text(), "p q");
        assert!(fragment.has_errors());
        assert!(fragment.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Class(SyntaxClass::EndOfInput))
        )));
    }

    #[test]
    fn a_parse_is_plain_data_that_clones_and_compares_structurally() {
        let source = admitted("$");
        let one = parse(&source, Dialect::Clingo);
        let two = parse(&source, Dialect::Clingo);
        assert_eq!(one, two);
        assert_eq!(one.clone(), one);
        assert_eq!(one.location(one.syntax().text_range()).source, SourceId::new(7));
        fn plain<T: Send + Sync>(_: &T) {}
        plain(&one);
    }
}
```

- [ ] **Step 4: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib parse`
Expected: compile error — `cannot find function parse`.

- [ ] **Step 5: Write the parse module's public face**

Prepend to `src/parse/mod.rs`:

```rust
//! The parser's public face (docs/design/syntax.md §5.5, §6.1): the
//! entry points, `EntryPoint`, and `Parse` — the green tree, the
//! diagnostics, and the facts a consumer needs to interpret both.

mod machine;

use std::fmt;
use std::marker::PhantomData;

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::Location;

use crate::ast;
use crate::diagnostic::SyntaxError;
use crate::dialect::Dialect;
use crate::lexer::Lexer;
use crate::token::TokenSource;
use crate::tree::{span_of, Asp, AstNode, GreenNode, SyntaxNode, TextRange};

use self::machine::Parser;

/// What the parser is asked to read: a whole program, or one construct
/// family with a named consumer — the statement (the macro tier's
/// statement macros), the term (the macro tier's term positions), and
/// the term-value sublanguage (grammar §5.10: what a string parses to
/// when a caller asks for a symbol — the REPL and the query surface).
/// The REPL is not the statement entry's consumer: it parses a growing
/// buffer through the program entry and reads `is_incomplete`. Closed;
/// a family is admitted here when a consumer names it, and the addition
/// is a breaking one, priced by the pre-1.0 posture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntryPoint {
    /// Grammar §5.11's `program` under the dialect, with the aspif dispatch.
    Program,
    /// One program position: leading docs, one statement with its
    /// annotation, or the ASP-Core-2 query.
    Statement,
    /// Grammar §5.1's `term`.
    Term,
    /// Grammar §5.10's `value-term`, under its restriction.
    TermValue,
}

/// The result of a parse: the green tree, the diagnostics, and the
/// facts a consumer needs to interpret both. Owned, `Send + Sync`,
/// cheap to clone (the tree is reference-counted). `T` is the typed
/// root the entry point yields — a view type, `!Send`, so it is carried
/// as `PhantomData<fn() -> T>`: a phantom that names `T` without
/// inheriting its auto-traits; `Clone`, `PartialEq`, `Eq`, and `Debug`
/// are implemented without a bound on `T`.
pub struct Parse<T: AstNode<Language = Asp>> {
    green: GreenNode,
    diagnostics: Vec<SyntaxError>,
    source: SourceId,
    dialect: Dialect,
    entry: EntryPoint,
    _root: PhantomData<fn() -> T>,
}

impl<T: AstNode<Language = Asp>> Parse<T> {
    pub(crate) fn new(
        green: GreenNode,
        diagnostics: Vec<SyntaxError>,
        source: SourceId,
        dialect: Dialect,
        entry: EntryPoint,
    ) -> Parse<T> {
        Parse { green, diagnostics, source, dialect, entry, _root: PhantomData }
    }

    /// A fresh root cursor over the tree — a view, minted on demand.
    /// Total, O(1).
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The typed root. Total: every entry point yields a root of the
    /// kind its `T` casts, so this never fails. O(1).
    pub fn tree(&self) -> T {
        T::cast(self.syntax()).expect("every entry point yields a root of the kind its T casts")
    }

    /// The green tree, the `Send + Sync` model. Total, O(1).
    pub fn green(&self) -> &GreenNode {
        &self.green
    }

    /// The diagnostics in the order the parser produced them — one
    /// order, by the determinism law; a batch consumer that wants the
    /// shared batch order sorts by base's `canonical_order` after
    /// lowering. Total, O(1).
    pub fn diagnostics(&self) -> &[SyntaxError] {
        &self.diagnostics
    }

    /// Any diagnostic of `Severity::Error`. Membership in the language
    /// (grammar §2) is exactly `!has_errors()`. Total, O(diagnostics).
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity() == Severity::Error)
    }

    /// The input ended before the construct did, and that is the only
    /// kind of error present — the REPL's "read more" signal
    /// (docs/design/syntax.md §6.5). Total, O(diagnostics).
    pub fn is_incomplete(&self) -> bool {
        self.has_errors()
            && self.diagnostics.iter().all(|d| {
                d.severity() != Severity::Error || d.kind().is_incompleteness(self.dialect)
            })
    }

    /// The identity of the source parsed. Total, O(1).
    pub fn source(&self) -> SourceId {
        self.source
    }

    /// The dialect parsed under. Total, O(1).
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// The entry point parsed through. Total, O(1).
    pub fn entry(&self) -> EntryPoint {
        self.entry
    }

    /// The qualified location of an element of this tree (base §4.3):
    /// its range under this parse's source id. Total, O(1).
    pub fn location(&self, range: TextRange) -> Location {
        Location { source: self.source, span: span_of(range) }
    }
}

impl<T: AstNode<Language = Asp>> Clone for Parse<T> {
    fn clone(&self) -> Self {
        Parse {
            green: self.green.clone(),
            diagnostics: self.diagnostics.clone(),
            source: self.source,
            dialect: self.dialect,
            entry: self.entry,
            _root: PhantomData,
        }
    }
}

impl<T: AstNode<Language = Asp>> PartialEq for Parse<T> {
    /// Structural through the green tree; the diagnostics, dialect, and
    /// identity as plain data — what the determinism law is checked
    /// with.
    fn eq(&self, other: &Self) -> bool {
        self.green == other.green
            && self.diagnostics == other.diagnostics
            && self.source == other.source
            && self.dialect == other.dialect
            && self.entry == other.entry
    }
}

impl<T: AstNode<Language = Asp>> Eq for Parse<T> {}

impl<T: AstNode<Language = Asp>> fmt::Debug for Parse<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parse")
            .field("green", &self.green)
            .field("diagnostics", &self.diagnostics)
            .field("source", &self.source)
            .field("dialect", &self.dialect)
            .field("entry", &self.entry)
            .finish()
    }
}

/// The file door: an admitted source under a dialect. Total; O(text).
pub fn parse(source: &Source, dialect: Dialect) -> Parse<ast::Program> {
    parse_program(&Lexer::new(source, dialect))
}

/// The general door for a program: any token source. Total; O(text).
pub fn parse_program(source: &impl TokenSource) -> Parse<ast::Program> {
    Parser::new(source).program()
}

/// The statement door: one program position. Total; O(text).
pub fn parse_statement(source: &impl TokenSource) -> Parse<ast::StatementFragment> {
    Parser::new(source).statement_fragment()
}

/// The term door: grammar §5.1's `term`. Total; O(text).
pub fn parse_term(source: &impl TokenSource) -> Parse<ast::TermFragment> {
    Parser::new(source).term_fragment(EntryPoint::Term)
}

/// The term-value door: grammar §5.10's `value-term`. Total; O(text).
pub fn parse_term_value(source: &impl TokenSource) -> Parse<ast::TermFragment> {
    Parser::new(source).term_fragment(EntryPoint::TermValue)
}
```

Add to `src/diagnostic.rs` — `use crate::dialect::Dialect;` at the top,
and in `impl SyntaxErrorKind` the incompleteness predicate the design
defines in §6.5, crate-private:

```rust
    /// Whether this kind is one of the incompleteness errors
    /// (docs/design/syntax.md §6.5): end of input where more was
    /// expected, an unterminated block comment, an unterminated script
    /// region, or — under the ASP-Core-2 dialect only, where a string
    /// may span lines — an unterminated string.
    pub(crate) fn is_incompleteness(&self, dialect: Dialect) -> bool {
        match self {
            SyntaxErrorKind::UnexpectedEndOfInput { .. }
            | SyntaxErrorKind::UnterminatedBlockComment
            | SyntaxErrorKind::UnterminatedScript => true,
            SyntaxErrorKind::MalformedString { defect: StringDefect::Unterminated } => {
                dialect == Dialect::AspCore2
            }
            _ => false,
        }
    }
```

- [ ] **Step 6: Write the machine**

`crates/themelios-syntax/src/parse/machine.rs`:

```rust
//! The parser core (docs/design/syntax.md §6): the token cursor over the
//! source door, the green builder under the trivia-placement law, the
//! two forms of defect and the synchronization machinery, the docs
//! production's mechanism, the aspif dispatch, and the entry points'
//! loops. The families — terms, statements, theory — extend this type
//! in their own files.

use rowan::{Checkpoint, GreenNodeBuilder, Language};
use themelios_base::span::{ByteOffset, Location, Span};

use crate::ast;
use crate::diagnostic::{
    Expected, ExpectedSet, Hint, MisplacedDoc, Related, RelatedLocus, SourceBreach, SyntaxClass,
    SyntaxError, SyntaxErrorKind,
};
use crate::dialect::Dialect;
use crate::lexer::classify_error;
use crate::token::{LexMode, Token, TokenSource};
use crate::tree::{Asp, SyntaxKind};

use super::{EntryPoint, Parse};

/// The parser over one token source: a cursor, a builder, and the
/// diagnostics it accumulates. Constructed per parse and dropped with
/// it — no state outlives a call (docs/design/syntax.md §12.1).
pub(super) struct Parser<'s, S: TokenSource> {
    source: &'s S,
    text: &'s str,
    dialect: Dialect,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<SyntaxError>,
    /// The offset of the first byte not yet placed in the tree.
    at: u32,
    /// The mode the next token is requested under (docs/design/syntax.md §4.2).
    mode: LexMode,
    /// A breach was witnessed: every peek answers `EOF` from here on.
    /// A legitimate end of input is not recorded — the door answers `EOF`
    /// at the text's end as often as it is asked.
    ended: bool,
    /// How many statement nodes are open: inside one, a doc comment is
    /// trivia with a warning; at program level the loop reads the run.
    statement_depth: u32,
    /// Where a doc comment is trivia diagnosed as documenting nothing
    /// though no statement is open: inside a program-level `ERROR` skip,
    /// throughout a term entry, and after a fragment's construct.
    skipping: bool,
    /// Whether placed `ERROR` tokens raise their lexical diagnostic: off
    /// inside the aspif dispatch and a depth-refused statement, whose
    /// one diagnostic stands for the whole (docs/design/syntax.md §4.5).
    lexical_diagnostics: bool,
    /// The last peek, so a repeated peek at one position costs nothing.
    peeked: Option<Peeked>,
    /// The kind and end offset of the last token placed, trivia included.
    last_placed: Option<(SyntaxKind, u32)>,
}

/// One significant token found ahead of the cursor: where the trivia
/// before it ends, and the token.
#[derive(Clone, Copy)]
struct Peeked {
    from: u32,
    mode: LexMode,
    docs_are_trivia: bool,
    start: u32,
    kind: SyntaxKind,
    len: u32,
}

impl<'s, S: TokenSource> Parser<'s, S> {
    pub(super) fn new(source: &'s S) -> Parser<'s, S> {
        Parser {
            source,
            text: source.text(),
            dialect: source.dialect(),
            builder: GreenNodeBuilder::new(),
            diagnostics: Vec::new(),
            at: 0,
            mode: LexMode::Normal,
            ended: false,
            statement_depth: 0,
            skipping: false,
            lexical_diagnostics: true,
            peeked: None,
            last_placed: None,
        }
    }

    // ---- the cursor -------------------------------------------------

    /// The raw token at `at` under the current mode, with the two
    /// witnessable breaches turned into end of input plus their
    /// diagnostic (docs/design/syntax.md §4.3).
    fn raw(&mut self, at: u32) -> Token<'s> {
        let eof = Token { kind: SyntaxKind::EOF, text: "" };
        if self.ended {
            return eof;
        }
        let end = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        match self.source.token_at(ByteOffset::new(at), self.mode) {
            Err(_) => {
                self.breach(SourceBreach::Refusal { at: ByteOffset::new(at) }, at);
                eof
            }
            Ok(token) if token.kind == SyntaxKind::EOF => {
                if at != end {
                    self.breach(
                        SourceBreach::Tiling { at: ByteOffset::new(at), token: SyntaxKind::EOF, len: 0 },
                        at,
                    );
                }
                eof
            }
            Ok(token) => {
                let len = u32::try_from(token.text.len()).unwrap_or(u32::MAX);
                if len == 0 || at.checked_add(len).is_none_or(|next| next > end) {
                    self.breach(SourceBreach::Tiling { at: ByteOffset::new(at), token: token.kind, len }, at);
                    return eof;
                }
                token
            }
        }
    }

    fn breach(&mut self, breach: SourceBreach, at: u32) {
        self.ended = true;
        let location = self.location(at, at);
        self.diagnostics.push(SyntaxError::new(SyntaxErrorKind::TokenSourceBreach { breach }, location));
    }

    /// Whether a doc comment is trivia where the cursor stands
    /// (docs/design/syntax.md §5.4, §6.3): everywhere but a program
    /// position — inside a statement, or where `skipping` says.
    fn docs_are_trivia(&self) -> bool {
        self.statement_depth > 0 || self.skipping
    }

    fn is_trivia_here(&self, kind: SyntaxKind) -> bool {
        kind.is_trivia() || (kind == SyntaxKind::DOC_COMMENT && self.docs_are_trivia())
    }

    /// The next significant token under the current mode, without
    /// consuming anything: trivia is looked past, never placed, so a
    /// node that finishes after a peek ends where its last significant
    /// token ended (docs/design/syntax.md §5.4, law 2).
    fn peek_token(&mut self) -> Peeked {
        let docs_are_trivia = self.docs_are_trivia();
        let cached = self.peeked.filter(|peeked| {
            peeked.from == self.at && peeked.mode == self.mode && peeked.docs_are_trivia == docs_are_trivia
        });
        if let Some(peeked) = cached {
            return peeked;
        }
        let mut start = self.at;
        loop {
            let token = self.raw(start);
            let len = u32::try_from(token.text.len()).unwrap_or(0);
            if token.kind != SyntaxKind::EOF && self.is_trivia_here(token.kind) {
                start += len;
                continue;
            }
            let peeked = Peeked { from: self.at, mode: self.mode, docs_are_trivia, start, kind: token.kind, len };
            self.peeked = Some(peeked);
            return peeked;
        }
    }

    /// The kind of the next significant token, `EOF` at the end.
    pub(super) fn peek(&mut self) -> SyntaxKind {
        self.peek_token().kind
    }

    /// The text of the next significant token.
    pub(super) fn peek_text(&mut self) -> &'s str {
        let peeked = self.peek_token();
        let start = peeked.start as usize;
        &self.text[start..start + peeked.len as usize]
    }

    /// The kind of the `n`th significant token ahead (`lookahead(0)` is
    /// `peek()`), under the current mode, without consuming anything —
    /// bounded by the caller to the grammar's five (docs/design/syntax.md
    /// §6.2, §6.3).
    pub(super) fn lookahead(&mut self, n: usize) -> SyntaxKind {
        let mut at = self.peek_token().start;
        let mut kind = self.peek_token().kind;
        for _ in 0..n {
            if kind == SyntaxKind::EOF {
                return kind;
            }
            let mut probe = at + u32::try_from(self.raw(at).text.len()).unwrap_or(0);
            loop {
                let token = self.raw(probe);
                if token.kind != SyntaxKind::EOF && self.is_trivia_here(token.kind) {
                    probe += u32::try_from(token.text.len()).unwrap_or(0);
                    continue;
                }
                at = probe;
                kind = token.kind;
                break;
            }
        }
        kind
    }

    /// The offset where the next significant token begins.
    pub(super) fn peek_start(&mut self) -> u32 {
        self.peek_token().start
    }

    pub(super) fn at_end(&mut self) -> bool {
        self.peek() == SyntaxKind::EOF
    }

    /// The mode the next token is requested under.
    pub(super) fn mode(&self) -> LexMode {
        self.mode
    }

    /// Sets the mode for the tokens that follow. A token peeked under
    /// the old mode is dropped unread — the twice-per-token bound of
    /// docs/design/syntax.md §4.2.
    pub(super) fn set_mode(&mut self, mode: LexMode) {
        self.mode = mode;
        self.peeked = None;
    }

    // ---- the builder under the trivia law ---------------------------

    /// Places the trivia before the next significant token into the
    /// open node — the node open where the trivia stood. Doc comments
    /// are placed and warned where they are trivia; at program level the
    /// loop reads them and this stops before them.
    fn eat_trivia(&mut self) {
        loop {
            let token = self.raw(self.at);
            if token.kind == SyntaxKind::EOF || !self.is_trivia_here(token.kind) {
                return;
            }
            let start = self.at;
            let len = u32::try_from(token.text.len()).unwrap_or(0);
            self.builder.token(Asp::kind_to_raw(token.kind), token.text);
            self.at = start + len;
            self.last_placed = Some((token.kind, self.at));
            if token.kind == SyntaxKind::DOC_COMMENT {
                let reason = if self.statement_depth > 0 {
                    MisplacedDoc::InsideStatement
                } else {
                    MisplacedDoc::NoStatementFollows
                };
                let location = self.location(start, start + len);
                self.diagnostics.push(SyntaxError::new(SyntaxErrorKind::MisplacedDocComment { reason }, location));
            }
        }
    }

    /// Consumes the next significant token into the open node, the
    /// trivia before it first. An `ERROR` token placed raises its one
    /// lexical diagnostic (docs/design/syntax.md §4.5).
    pub(super) fn bump(&mut self) {
        let peeked = self.peek_token();
        if peeked.kind == SyntaxKind::EOF {
            return;
        }
        self.eat_trivia();
        let token = self.raw(self.at);
        debug_assert_eq!(token.kind, peeked.kind);
        let start = self.at;
        let len = u32::try_from(token.text.len()).unwrap_or(0);
        self.builder.token(Asp::kind_to_raw(token.kind), token.text);
        self.at = start + len;
        self.last_placed = Some((token.kind, self.at));
        self.peeked = None;
        if token.kind == SyntaxKind::ERROR && self.lexical_diagnostics {
            self.lexical_diagnostic(token.text, start);
        }
    }

    fn lexical_diagnostic(&mut self, text: &str, start: u32) {
        let end = start as usize + text.len();
        let following = self.text[end..].chars().next();
        let defect = classify_error(text, following, self.mode, self.dialect);
        let from = start + u32::try_from(defect.primary.start).unwrap_or(0);
        let to = start + u32::try_from(defect.primary.end).unwrap_or(0);
        let mut error = SyntaxError::new(defect.kind, self.location(from, to));
        if defect.literal_extent {
            error = error.with_related(Related {
                locus: RelatedLocus::LiteralExtent,
                location: self.location(start, start + u32::try_from(text.len()).unwrap_or(0)),
            });
        }
        self.diagnostics.push(error);
    }

    /// Consumes the next significant token if it is `kind`.
    pub(super) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Opens a node at the next significant token: the trivia before it
    /// is placed into the parent first (law 2).
    pub(super) fn start_node(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        self.builder.start_node(Asp::kind_to_raw(kind));
    }

    /// A checkpoint at the next significant token — the trivia before it
    /// placed into the open node — for a node opened retroactively.
    /// Taken only where a significant token is about to be consumed.
    pub(super) fn checkpoint(&mut self) -> Checkpoint {
        self.eat_trivia();
        self.builder.checkpoint()
    }

    /// Opens `kind` retroactively around everything built since `at`.
    pub(super) fn start_node_at(&mut self, at: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(at, Asp::kind_to_raw(kind));
    }

    /// Closes the open node where its last significant token ended:
    /// trivia after it is not consumed here and falls to the parent.
    pub(super) fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    /// A statement begins here: until it leaves, a doc comment is trivia
    /// with a warning (docs/design/syntax.md §6.3), whatever node is
    /// open — the statement's own node may open later, retroactively.
    pub(super) fn enter_statement(&mut self) {
        self.statement_depth += 1;
        self.peeked = None;
    }

    /// The statement left.
    pub(super) fn leave_statement(&mut self) {
        self.statement_depth -= 1;
        self.peeked = None;
    }

    /// Places an empty node — one of the three kinds that may be empty
    /// (docs/design/syntax.md §5.4) — immediately after the token that
    /// licensed it, holding no trivia.
    pub(super) fn empty_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(Asp::kind_to_raw(kind));
        self.builder.finish_node();
    }

    // ---- diagnostics and recovery -----------------------------------

    pub(super) fn location(&self, start: u32, end: u32) -> Location {
        Location {
            source: self.source.id(),
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end)).expect("the parser's spans are ordered"),
        }
    }

    pub(super) fn diagnose(&mut self, error: SyntaxError) {
        self.diagnostics.push(error);
    }

    /// The missing-child form (docs/design/syntax.md §6.7): a diagnostic
    /// at the next significant token — `unexpected-token` with what was
    /// expected and what was found, or `unexpected-end-of-input` — and
    /// nothing consumed. A lexical `ERROR` token found here raises no
    /// structural diagnostic: its lexical diagnostic, raised when it is
    /// placed, is the one report of that mistake. A numeral found
    /// directly after a numeral carries the leading-zero hint (grammar
    /// §4.3): `007` is three numerals, and the parser names the mistake.
    pub(super) fn unexpected(&mut self, expected: ExpectedSet, hint: Option<Hint>) {
        let peeked = self.peek_token();
        let hint = hint.or_else(|| {
            (peeked.kind == SyntaxKind::NUMBER && self.last_placed == Some((SyntaxKind::NUMBER, peeked.start)))
                .then_some(Hint::LeadingZeroNumeral)
        });
        let kind = match peeked.kind {
            SyntaxKind::EOF => SyntaxErrorKind::UnexpectedEndOfInput { expected, hint },
            SyntaxKind::ERROR => return,
            found => SyntaxErrorKind::UnexpectedToken { expected, found, hint },
        };
        let location = self.location(peeked.start, peeked.start + peeked.len);
        self.diagnostics.push(SyntaxError::new(kind, location));
    }

    /// `unexpected` with one expected token.
    pub(super) fn expected_token(&mut self, kind: SyntaxKind) {
        self.unexpected([Expected::Token(kind)].into_iter().collect(), None);
    }

    /// Consumes `kind` if it is next; otherwise the missing-child
    /// diagnostic, and nothing consumed.
    pub(super) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.expected_token(kind);
            false
        }
    }

    /// The unexpected-token form, alone (docs/design/syntax.md §6.7):
    /// the next significant token wrapped in an `ERROR` node, its
    /// diagnostic raised, and the parse continuing after it.
    pub(super) fn wrap_unexpected(&mut self, expected: ExpectedSet, hint: Option<Hint>) {
        self.unexpected(expected, hint);
        self.start_node(SyntaxKind::ERROR);
        self.bump();
        self.finish_node();
    }

    /// The unexpected-token form through a synchronization point: the
    /// next significant token and everything up to (not including) the
    /// first token of `sync` — or to end of input — wrapped in one
    /// `ERROR` node, byte-preserved.
    pub(super) fn skip_into_error(&mut self, expected: ExpectedSet, hint: Option<Hint>, sync: &[SyntaxKind]) {
        self.unexpected(expected, hint);
        if self.at_end() {
            return;
        }
        self.start_node(SyntaxKind::ERROR);
        self.bump();
        while !self.at_end() && !sync.contains(&self.peek()) {
            self.bump();
        }
        self.finish_node();
    }

    /// The program-level row of docs/design/syntax.md §6.7: `ERROR`
    /// through the next `.` — and an immediately following `[…]` group,
    /// since the four annotation families put one after the dot — or to
    /// end of input.
    pub(super) fn recover_program_level(&mut self) {
        let was_skipping = self.skipping;
        self.skipping = true;
        self.peeked = None;
        self.unexpected([Expected::Class(SyntaxClass::Statement)].into_iter().collect(), None);
        self.start_node(SyntaxKind::ERROR);
        while !self.at_end() {
            let kind = self.peek();
            self.bump();
            if kind == SyntaxKind::DOT {
                if self.peek() == SyntaxKind::L_BRACKET {
                    self.skip_bracket_group();
                }
                break;
            }
        }
        self.finish_node();
        self.skipping = was_skipping;
        self.peeked = None;
    }

    /// Consumes a `[`…`]` group, brackets counted, into the open node.
    fn skip_bracket_group(&mut self) {
        let mut depth = 0u32;
        while !self.at_end() {
            let kind = self.peek();
            self.bump();
            match kind {
                SyntaxKind::L_BRACKET => depth += 1,
                SyntaxKind::R_BRACKET => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // ---- the entry points -------------------------------------------

    fn finish<T: crate::tree::AstNode<Language = Asp>>(self, entry: EntryPoint) -> Parse<T> {
        let Parser { builder, diagnostics, source, dialect, .. } = self;
        Parse::new(builder.finish(), diagnostics, source.id(), dialect, entry)
    }

    /// The program entry (docs/design/syntax.md §6.1, §6.3).
    pub(super) fn program(mut self) -> Parse<ast::Program> {
        self.builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        if self.dispatches_aspif() {
            self.aspif();
        } else {
            self.statements();
        }
        self.builder.finish_node();
        self.finish(EntryPoint::Program)
    }

    /// The statement entry: one program position, then end of input.
    pub(super) fn statement_fragment(mut self) -> Parse<ast::StatementFragment> {
        self.builder.start_node(Asp::kind_to_raw(SyntaxKind::STATEMENT_FRAGMENT));
        let checkpoint = self.docs_and_checkpoint();
        if self.statement_begins() {
            self.statement(checkpoint);
        }
        self.expect_end_of_input();
        self.builder.finish_node();
        self.finish(EntryPoint::Statement)
    }

    /// The two term entries: one term under the entry's restriction,
    /// then end of input. A term entry has no program position, so a
    /// doc comment anywhere in it is trivia with its warning. Task 8
    /// supplies the term itself.
    pub(super) fn term_fragment(mut self, entry: EntryPoint) -> Parse<ast::TermFragment> {
        self.builder.start_node(Asp::kind_to_raw(SyntaxKind::TERM_FRAGMENT));
        self.skipping = true;
        self.eat_trivia();
        self.term_at_entry(entry);
        self.expect_end_of_input();
        self.builder.finish_node();
        self.finish(entry)
    }

    /// The trailing check every fragment entry ends with: trailing
    /// trivia admitted — a doc comment among it is trivia with its
    /// warning, no statement following — and anything else one `ERROR`
    /// node with `EndOfInput` in its expected set.
    fn expect_end_of_input(&mut self) {
        self.skipping = true;
        self.peeked = None;
        if !self.at_end() {
            self.unexpected([Expected::Class(SyntaxClass::EndOfInput)].into_iter().collect(), None);
            self.start_node(SyntaxKind::ERROR);
            while !self.at_end() {
                self.bump();
            }
            self.finish_node();
        }
        self.eat_trivia();
    }

    /// Grammar §4.9: the identifier `asp`, one space, a decimal numeral,
    /// at the very start.
    fn dispatches_aspif(&self) -> bool {
        let bytes = self.text.as_bytes();
        bytes.starts_with(b"asp ") && bytes.get(4).is_some_and(u8::is_ascii_digit)
    }

    /// The whole text as one `ERROR` token under `PROGRAM`, lossless,
    /// with the single diagnostic at the header.
    fn aspif(&mut self) {
        let numeral = 4 + self.text.as_bytes()[4..].iter().take_while(|b| b.is_ascii_digit()).count();
        let location = self.location(0, u32::try_from(numeral).unwrap_or(0));
        self.diagnostics.push(SyntaxError::new(SyntaxErrorKind::AspifInput, location));
        self.builder.token(Asp::kind_to_raw(SyntaxKind::ERROR), self.text);
        self.at = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        self.ended = true;
    }

    /// The program's statements: at each position, the leading trivia,
    /// the docs run if any, then a statement opened at the checkpoint
    /// before its docs, or program-level recovery.
    fn statements(&mut self) {
        while !self.at_end() {
            let checkpoint = self.docs_and_checkpoint();
            if self.at_end() {
                return;
            }
            if self.statement_begins() {
                self.statement(checkpoint);
            } else {
                self.recover_program_level();
            }
        }
    }

    /// At a program position: place the trivia before the docs, take the
    /// checkpoint a statement will open at, place the docs run — each
    /// line a `DOC_COMMENT` token, the trivia between them — and, when
    /// no statement follows, warn on each line (grammar §5.11: the run is
    /// trivia then).
    fn docs_and_checkpoint(&mut self) -> Checkpoint {
        self.eat_trivia();
        let checkpoint = self.builder.checkpoint();
        let mut lines: Vec<(u32, u32)> = Vec::new();
        while self.peek() == SyntaxKind::DOC_COMMENT {
            let start = self.peek_start();
            let len = self.peek_token().len;
            self.bump();
            lines.push((start, start + len));
            self.eat_trivia();
        }
        if !lines.is_empty() && !self.statement_begins() {
            for (start, end) in lines {
                let location = self.location(start, end);
                self.diagnostics.push(SyntaxError::new(
                    SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::NoStatementFollows },
                    location,
                ));
            }
        }
        checkpoint
    }

    /// Whether the next significant token begins a statement of the
    /// dialect: a directive keyword, a neck, or anything a head begins
    /// with — Tasks 9–11 realize each family.
    pub(super) fn statement_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::VARIABLE
                | SyntaxKind::ANONYMOUS
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::KW_NOT
                | SyntaxKind::KW_TRUE
                | SyntaxKind::KW_FALSE
                | SyntaxKind::MINUS
                | SyntaxKind::TILDE
                | SyntaxKind::L_PAREN
                | SyntaxKind::PIPE
                | SyntaxKind::AT
                | SyntaxKind::L_BRACE
                | SyntaxKind::AMPERSAND
                | SyntaxKind::KW_COUNT
                | SyntaxKind::KW_SUM
                | SyntaxKind::KW_SUM_PLUS
                | SyntaxKind::KW_MIN
                | SyntaxKind::KW_MAX
                | SyntaxKind::NECK
                | SyntaxKind::WEAK_NECK
                | SyntaxKind::KW_MINIMIZE
                | SyntaxKind::KW_MAXIMIZE
                | SyntaxKind::KW_SHOW
                | SyntaxKind::KW_PROJECT
                | SyntaxKind::KW_DEFINED
                | SyntaxKind::KW_EDGE
                | SyntaxKind::KW_HEURISTIC
                | SyntaxKind::KW_EXTERNAL
                | SyntaxKind::KW_CONST
                | SyntaxKind::KW_SCRIPT
                | SyntaxKind::KW_INCLUDE
                | SyntaxKind::KW_PROGRAM
                | SyntaxKind::KW_THEORY
        )
    }

    /// One statement, its node opened at `checkpoint` (before its docs).
    /// The families land in Tasks 9–11 and replace this dispatch arm by
    /// arm; until a family lands, its statement start recovers at
    /// program level, so every input still yields a tree.
    pub(super) fn statement(&mut self, _checkpoint: Checkpoint) {
        self.enter_statement();
        self.recover_program_level();
        self.leave_statement();
    }

    /// The term at a term entry, under the entry's restriction. Task 8
    /// supplies it; until then no term is read and the fragment holds
    /// its trailing check.
    pub(super) fn term_at_entry(&mut self, _entry: EntryPoint) {}
}
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p themelios-syntax --lib parse`
Expected: 10 passed. Under the interim dispatch every statement start
recovers, so `$$$ p. ééé` yields two program-level `ERROR` nodes (`$$$
p.` is one skip, `ééé` another), each opened at a lexical `ERROR` token,
which raises no structural diagnostic beside its lexical one — two
diagnostics in all, as the test counts.

- [ ] **Step 8: Run the full gate**

Run the four gate commands. Expected: green; `dead_code` may fire on
methods no family calls yet (`lookahead`, `peek_text`, `set_mode`,
`mode`, `empty_node`, `expect`, `wrap_unexpected`, `skip_into_error`,
`diagnose`, `expected_token`) — allow them for this task alone with
`#[allow(dead_code)]` on the `impl` block and the note that Tasks 8–11
consume them, and remove the allowance in the first task that consumes
each (Task 8 removes it entirely).

- [ ] **Step 9: Commit**

```bash
git add crates/themelios-syntax
git commit -m "Add the parser core: the token cursor, the builder under the trivia law, recovery, the aspif dispatch, the entry points, and the roots"
```

---

### Task 8: The frame loop — the term families, the restriction contexts, and the depth refusal

**Files:**
- Create: `crates/themelios-syntax/src/parse/terms.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs` (the constants;
  `mod terms;`; the term entries' body), `src/parse/machine.rs` (the
  related-locus form of `unexpected`, expected-set merging at one
  position, the statement-rest skip, the depth-refusal state),
  `src/tree.rs` (a test-only shape dump), `src/diagnostic.rs` (the
  crate-private `extend_expected`)

**Derives:** syntax.md §5.4 (law 3, the empty node), §6.2 (the parser's
shape; the recursion discipline; the loop's invariant), §6.3 (the query
reading's one-token peek), §6.6 (the constants; the refusal with a
locus), §6.7 (the frame loop's row; hints), §7.1 (`Hint`,
`RestrictedForm`, `Restriction`), §12.3, Appendix A (the term nodes);
grammar §5.1, §5.9 (`constant-term`), §5.10 (`value-term`), §6.1.

**Interfaces:**
- Consumes: the machine of Task 7; `diagnostic::{Hint, RestrictedForm,
  Restriction, Related, RelatedLocus, Expected, SyntaxClass}`.
- Produces: `parse::{MAX_NESTING_DEPTH, REQUIRED_STACK_BYTES,
  TERM_LAYERS_PER_FRAME, FIXED_LAYERS, MAX_TREE_DEPTH}`; the
  crate-private `terms::TermContext { Term, ConstantTerm, TermValue }`,
  `Parser::term(TermContext) -> bool`, `Parser::term_begins()`, and
  `Parser::depth_refused()` / `Parser::end_statement_after_refusal()`
  the statement families read; `parse_term` and `parse_term_value`
  now read a term.

- [ ] **Step 1: Write the constants**

Add to `src/parse/mod.rs`, after the `mod machine;` line, `mod terms;`,
and after the imports:

```rust
/// The deepest nesting of bracket contexts — frames, one per open
/// bracket (docs/design/syntax.md §6.2) — the parser will open. Named
/// because it carries meaning; its value is fixed by measurement between
/// two bounds and recorded here with both. **Provisional:** this value
/// stands until the depth gate measures the constant (the stage-2 plan's
/// Task 18), which replaces it and records the two bounds beside it.
pub const MAX_NESTING_DEPTH: u32 = 1_000;

/// The stack, in bytes, on which every operation this crate performs or
/// hands out over the deepest tree it can build — dropping it, comparing
/// two, rendering one, walking the typed AST, attaching, certifying — is
/// proven to complete: the depth gate runs on a thread of exactly this
/// size and passes with headroom (docs/design/syntax.md §6.6). A
/// consumer's thread that holds a tree needs at least this much. Sixty-
/// four mebibytes: eight times the eight-mebibyte main-thread default of
/// the two supported operating systems, a size a language server's
/// worker can be given without contortion; `MAX_NESTING_DEPTH` is
/// measured against it, and a move of either re-measures the other.
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;

/// The most node layers one frame contributes to the tree's depth
/// (docs/design/syntax.md §5.4, law 3), by inspection of Appendix A: a
/// function or `@`-call frame is its node, `ARGUMENTS`, and `TUPLE`,
/// then the seven binary levels and the unary run of the operand inside
/// — eleven; a pool contributes ten, an absolute value nine, a theory
/// frame two.
pub const TERM_LAYERS_PER_FRAME: u32 = 11;

/// The layers of the tree that do not depend on nesting: the deepest
/// grammar-bounded path from the root to the first frame — `PROGRAM`,
/// `RULE`, `BODY`, `THEORY_ATOM`, `THEORY_ELEMENTS`, `THEORY_ELEMENT`,
/// `CONDITION`, `LITERAL`, `COMPARISON`, and the frame-free operator
/// chain's eight layers — and the one leaf below the last frame: a
/// constant, a variable, a splice, or the `ERROR` node of a refusal.
/// By inspection of Appendix A (docs/design/syntax.md §5.4, §6.6).
pub const FIXED_LAYERS: u32 = 18;

/// The bound on the tree's depth (docs/design/syntax.md §5.4, law 3),
/// derived and carrying no numeral of its own: `MAX_NESTING_DEPTH`
/// frames, each contributing at most `TERM_LAYERS_PER_FRAME` layers,
/// under `FIXED_LAYERS`. Public because a consumer who recurses over the
/// typed AST sizes its own stack from it; `REQUIRED_STACK_BYTES` covers
/// this crate's and rowan's walks, not the consumer's.
pub const MAX_TREE_DEPTH: u32 = MAX_NESTING_DEPTH * TERM_LAYERS_PER_FRAME + FIXED_LAYERS;
```

- [ ] **Step 2: Extend the machine**

In `src/parse/machine.rs`, add two fields to `Parser` and initialize
them in `new` (`depth_refused: false`):

```rust
    /// The frame loop refused a frame past the constant: the rest of the
    /// statement is already carried in an `ERROR` node, and the statement
    /// closes without further diagnostics (docs/design/syntax.md §6.6).
    depth_refused: bool,
```

Add these methods to the `impl` block:

```rust
    /// The dialect parsed under.
    pub(super) fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// The length of the next significant token.
    pub(super) fn peek_len(&mut self) -> u32 {
        self.peek_token().len
    }

    /// The missing-child form with a related locus (a missing closer
    /// names its opener). Two diagnostics at one position merge: a
    /// second expectation at the token that already carries one extends
    /// its expected set rather than repeating the report — one token, one
    /// diagnostic, the whole of what was expected there. A numeral found
    /// directly after a numeral carries the leading-zero hint (grammar
    /// §4.3): `007` is three numerals, and the parser names the mistake.
    pub(super) fn unexpected_related(&mut self, expected: ExpectedSet, hint: Option<Hint>, related: Option<Related>) {
        let peeked = self.peek_token();
        if peeked.kind == SyntaxKind::ERROR {
            return;
        }
        let hint = hint.or_else(|| {
            (peeked.kind == SyntaxKind::NUMBER && self.last_placed == Some((SyntaxKind::NUMBER, peeked.start)))
                .then_some(Hint::LeadingZeroNumeral)
        });
        let location = self.location(peeked.start, peeked.start + peeked.len);
        if let Some(last) = self.diagnostics.last_mut() {
            if last.primary() == location && last.extend_expected(&expected, hint) {
                return;
            }
        }
        let kind = match peeked.kind {
            SyntaxKind::EOF => SyntaxErrorKind::UnexpectedEndOfInput { expected, hint },
            found => SyntaxErrorKind::UnexpectedToken { expected, found, hint },
        };
        let mut error = SyntaxError::new(kind, location);
        if let Some(related) = related {
            error = error.with_related(related);
        }
        self.diagnostics.push(error);
    }

    /// Consumes the rest of the statement into the open node: through the
    /// terminating dot and an immediately following `[…]` group, or to end
    /// of input (docs/design/syntax.md §6.6, §6.7).
    pub(super) fn skip_statement_rest(&mut self) {
        while !self.at_end() {
            let kind = self.peek();
            self.bump();
            if kind == SyntaxKind::DOT {
                if self.peek() == SyntaxKind::L_BRACKET {
                    self.skip_bracket_group();
                }
                return;
            }
        }
    }

    /// Whether the frame loop refused a frame in this statement.
    pub(super) fn depth_refused(&self) -> bool {
        self.depth_refused
    }

    /// The refusal: its diagnostic at the opener, the lexical diagnostics
    /// silenced, and the rest of the statement carried in one `ERROR`
    /// node under the innermost open frame.
    pub(super) fn refuse_depth(&mut self) {
        let start = self.peek_start();
        let end = start + self.peek_len();
        let location = self.location(start, end);
        self.diagnostics.push(SyntaxError::new(
            SyntaxErrorKind::NestingTooDeep { depth: super::MAX_NESTING_DEPTH },
            location,
        ));
        self.lexical_diagnostics = false;
        self.depth_refused = true;
        self.start_node(SyntaxKind::ERROR);
        self.skip_statement_rest();
        self.finish_node();
    }

    /// Restores the parser after a refused statement closed: the lexical
    /// diagnostics back on, and the mode back to normal, whatever region
    /// the refusal fell in.
    pub(super) fn end_statement_after_refusal(&mut self) {
        self.lexical_diagnostics = true;
        self.depth_refused = false;
        self.set_mode(LexMode::Normal);
    }
```

and rewrite `unexpected` and `recover_program_level` to read the new
forms:

```rust
    pub(super) fn unexpected(&mut self, expected: ExpectedSet, hint: Option<Hint>) {
        self.unexpected_related(expected, hint, None);
    }

    pub(super) fn recover_program_level(&mut self) {
        let was_skipping = self.skipping;
        self.skipping = true;
        self.peeked = None;
        self.unexpected([Expected::Class(SyntaxClass::Statement)].into_iter().collect(), None);
        self.start_node(SyntaxKind::ERROR);
        self.skip_statement_rest();
        self.finish_node();
        self.skipping = was_skipping;
        self.peeked = None;
    }
```

In `src/diagnostic.rs`, add to `impl SyntaxError`:

```rust
    /// Extends the expected set of an unexpected-token or
    /// unexpected-end-of-input diagnostic — the merge of two expectations
    /// at one position — and takes `hint` if none stands; false, and
    /// nothing changed, for every other kind.
    pub(crate) fn extend_expected(&mut self, more: &ExpectedSet, hint: Option<Hint>) -> bool {
        match &mut self.kind {
            SyntaxErrorKind::UnexpectedToken { expected, hint: mine, .. }
            | SyntaxErrorKind::UnexpectedEndOfInput { expected, hint: mine } => {
                expected.extend(more.iter().copied());
                if mine.is_none() {
                    *mine = hint;
                }
                true
            }
            _ => false,
        }
    }
```

In `src/tree.rs`, add the test-only shape dump the parser's unit tests
read (a node's kinds and significant token texts as one S-expression,
trivia dropped):

```rust
/// The tree's shape as one line — `(KIND child …)` for nodes, the text
/// for tokens that are not trivia — for the parser's own tests, which
/// read shapes rather than dumps.
#[cfg(test)]
pub(crate) fn sexpr(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push('(');
                out.push_str(&node.kind().to_string());
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                if !token.kind().is_trivia() {
                    out.push(' ');
                    out.push_str(token.text());
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(')'),
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out
}
```

Replace the interim `term_at_entry` in `machine.rs` with:

```rust
    /// The term at a term entry, under the entry's restriction.
    pub(super) fn term_at_entry(&mut self, entry: EntryPoint) {
        let context = match entry {
            EntryPoint::TermValue => super::terms::TermContext::TermValue,
            EntryPoint::Program | EntryPoint::Statement | EntryPoint::Term => super::terms::TermContext::Term,
        };
        self.term(context);
    }
```

- [ ] **Step 3: Write the failing tests**

Create `src/parse/terms.rs` holding only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use crate::diagnostic::{Hint, RestrictedForm, Restriction, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::lexer::Lexer;
    use crate::parse::{parse_term, parse_term_value, MAX_NESTING_DEPTH, MAX_TREE_DEPTH};
    use crate::tree::{sexpr, SyntaxKind};

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    /// The shape of the term the term entry reads from `text` under the
    /// clingo dialect, with the fragment root peeled.
    fn term(text: &str) -> String {
        let source = admitted(text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(TERM_FRAGMENT ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn diagnostics(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse_term(&Lexer::new(&source, Dialect::Clingo)).diagnostics().iter().map(|d| d.kind().clone()).collect()
    }

    #[test]
    fn a_chain_is_one_flat_node_per_level() {
        assert_eq!(term("1 + 2 - 3"), "(BINARY_TERM (CONSTANT_TERM 1) + (CONSTANT_TERM 2) - (CONSTANT_TERM 3))");
        assert_eq!(
            term("1 + 2 * 3"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (BINARY_TERM (CONSTANT_TERM 2) * (CONSTANT_TERM 3)))"
        );
        assert_eq!(
            term("1 * 2 + 3"),
            "(BINARY_TERM (BINARY_TERM (CONSTANT_TERM 1) * (CONSTANT_TERM 2)) + (CONSTANT_TERM 3))"
        );
        assert_eq!(
            term("1 + 2 * 3 + 4"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (BINARY_TERM (CONSTANT_TERM 2) * (CONSTANT_TERM 3)) + (CONSTANT_TERM 4))"
        );
        assert_eq!(term("2 ** 3 ** 4"), "(BINARY_TERM (CONSTANT_TERM 2) ** (CONSTANT_TERM 3) ** (CONSTANT_TERM 4))");
        assert_eq!(
            term("1..3 ^ 2 ? 4 & 5"),
            "(BINARY_TERM (CONSTANT_TERM 1) .. (BINARY_TERM (CONSTANT_TERM 3) ^ (BINARY_TERM (CONSTANT_TERM 2) ? (BINARY_TERM (CONSTANT_TERM 4) & (CONSTANT_TERM 5)))))"
        );
        assert!(diagnostics("1 + 2 * 3 + 4").is_empty());
    }

    #[test]
    fn unary_runs_are_flat_and_bind_tighter_than_every_binary_level() {
        assert_eq!(term("- - x"), "(UNARY_TERM - - (CONSTANT_TERM x))");
        assert_eq!(term("-2**2"), "(BINARY_TERM (UNARY_TERM - (CONSTANT_TERM 2)) ** (CONSTANT_TERM 2))");
        assert_eq!(term("2 ** -3"), "(BINARY_TERM (CONSTANT_TERM 2) ** (UNARY_TERM - (CONSTANT_TERM 3)))");
        assert_eq!(term("~X + 1"), "(BINARY_TERM (UNARY_TERM ~ (VARIABLE_TERM X)) + (CONSTANT_TERM 1))");
        assert_eq!(term("-(1;2)"), "(UNARY_TERM - (POOL ( (TUPLE (CONSTANT_TERM 1)) ; (TUPLE (CONSTANT_TERM 2)) )))");
    }

    #[test]
    fn pools_tuples_and_argument_lists_keep_the_grammars_uniform_shape() {
        assert_eq!(term("()"), "(POOL ( (TUPLE) ))");
        assert_eq!(term("(a)"), "(POOL ( (TUPLE (CONSTANT_TERM a)) ))");
        assert_eq!(term("(a,)"), "(POOL ( (TUPLE (CONSTANT_TERM a) ,) ))");
        assert_eq!(term("(,)"), "(POOL ( (TUPLE ,) ))");
        assert_eq!(term("(a,b;c,d)"), "(POOL ( (TUPLE (CONSTANT_TERM a) , (CONSTANT_TERM b)) ; (TUPLE (CONSTANT_TERM c) , (CONSTANT_TERM d)) ))");
        assert_eq!(term("(;)"), "(POOL ( (TUPLE) ; (TUPLE) ))");
        assert_eq!(term("f()"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE) )))");
        assert_eq!(term("f(;)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE) ; (TUPLE) )))");
        assert_eq!(term("f(a;)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE) )))");
        assert_eq!(term("f (a, b)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) , (CONSTANT_TERM b)) )))");
        assert_eq!(term("f(g(1),X)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (FUNCTION_TERM g (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) , (VARIABLE_TERM X)) )))");
        assert!(diagnostics("(a,b;c,d)").is_empty());
        assert!(diagnostics("f(a;)").is_empty());
    }

    #[test]
    fn absolute_values_external_calls_and_the_constants() {
        assert_eq!(term("|X;Y|"), "(ABS_TERM | (VARIABLE_TERM X) ; (VARIABLE_TERM Y) |)");
        assert_eq!(term("| |x| |"), "(ABS_TERM | (ABS_TERM | (CONSTANT_TERM x) |) |)");
        assert_eq!(term("@f(1)"), "(EXTERNAL_TERM @ f (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) )))");
        assert_eq!(term("@f"), "(EXTERNAL_TERM @ f)");
        assert_eq!(term("@ f"), "(EXTERNAL_TERM @ f)");
        assert_eq!(term("#inf"), "(CONSTANT_TERM #inf)");
        assert_eq!(term("#supremum"), "(CONSTANT_TERM #supremum)");
        assert_eq!(term("\"s\""), "(CONSTANT_TERM \"s\")");
        assert_eq!(term("_"), "(VARIABLE_TERM _)");
    }

    #[test]
    fn trivia_inside_a_frame_belongs_to_the_frames_node_not_the_tuple() {
        let source = admitted("f( a )");
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        let tuple = parse
            .syntax()
            .descendants()
            .find(|node| node.kind() == SyntaxKind::TUPLE)
            .expect("a tuple");
        assert_eq!(tuple.text(), "a");
        let arguments = tuple.parent().expect("inside arguments");
        assert_eq!(arguments.kind(), SyntaxKind::ARGUMENTS);
        assert_eq!(arguments.text(), "( a )");
    }

    #[test]
    fn a_trailing_comma_in_arguments_is_diagnosed_with_its_hint() {
        assert_eq!(term("f(a,)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) ,) )))");
        let kinds = diagnostics("f(a,)");
        assert_eq!(kinds.len(), 1);
        assert!(matches!(
            &kinds[0],
            SyntaxErrorKind::UnexpectedToken { found: SyntaxKind::R_PAREN, hint: Some(Hint::TrailingCommaInArguments), .. }
        ));
    }

    #[test]
    fn an_intruder_in_a_frame_is_wrapped_and_the_frame_continues() {
        assert_eq!(term("f(a b)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) (ERROR b)) )))");
        assert_eq!(diagnostics("f(a b)").len(), 1);
        assert_eq!(term("f($ a)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (ERROR $) (CONSTANT_TERM a)) )))");
        assert_eq!(diagnostics("f($ a)").len(), 1, "the lexical diagnostic alone");
    }

    #[test]
    fn an_unclosed_bracket_closes_at_end_of_input_naming_its_opener() {
        assert_eq!(term("f(a"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a))))");
        let source = admitted("f(a");
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert!(parse.is_incomplete());
        assert_eq!(parse.diagnostics().len(), 1);
        assert_eq!(parse.diagnostics()[0].related().len(), 1);
        assert_eq!(term("f(a,"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) ,)))");
        assert_eq!(diagnostics("f(a,").len(), 1, "a missing operand and a missing closer at one position merge");
        assert_eq!(term("(a; b"), "(POOL ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE (CONSTANT_TERM b)))");
        assert_eq!(term("|a"), "(ABS_TERM | (CONSTANT_TERM a))");
    }

    #[test]
    fn a_missing_operand_after_an_operator() {
        assert_eq!(term("1 +"), "(BINARY_TERM (CONSTANT_TERM 1) +)");
        assert!(matches!(
            diagnostics("1 +").as_slice(),
            [SyntaxErrorKind::UnexpectedEndOfInput { hint: None, .. }]
        ));
    }

    #[test]
    fn adjacent_numerals_carry_the_leading_zero_hint() {
        // At the base the term ends at the first numeral; the fragment
        // wraps the rest, and the diagnostic at the second numeral names
        // the mistake.
        assert_eq!(term("007"), "(CONSTANT_TERM 0) (ERROR 0 7)");
        let kinds = diagnostics("007");
        assert_eq!(kinds.len(), 1);
        assert!(matches!(
            &kinds[0],
            SyntaxErrorKind::UnexpectedToken { hint: Some(Hint::LeadingZeroNumeral), .. }
        ));
        // Inside a frame each intruding numeral is wrapped where it stands.
        assert_eq!(term("f(007)"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM 0) (ERROR 0) (ERROR 7)) )))");
        assert!(diagnostics("f(007)").iter().all(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { hint: Some(Hint::LeadingZeroNumeral), .. }
        )));
        assert!(diagnostics("0 7").iter().all(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { hint: None, .. }
        )));
    }

    #[test]
    fn the_query_mark_is_read_by_dialect() {
        let source = admitted("p ?");
        let clingo = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(sexpr(&clingo.syntax()), "(TERM_FRAGMENT (BINARY_TERM (CONSTANT_TERM p) ?))");
        assert!(matches!(
            clingo.diagnostics()[0].kind(),
            SyntaxErrorKind::UnexpectedEndOfInput { hint: Some(Hint::QueryMarkNeedsAspCore2), .. }
        ));
        let core = parse_term(&Lexer::new(&source, Dialect::AspCore2));
        assert_eq!(sexpr(&core.syntax()), "(TERM_FRAGMENT (CONSTANT_TERM p) (ERROR ?))");
        let source = admitted("p ? q");
        let core = parse_term(&Lexer::new(&source, Dialect::AspCore2));
        assert_eq!(sexpr(&core.syntax()), "(TERM_FRAGMENT (BINARY_TERM (CONSTANT_TERM p) ? (CONSTANT_TERM q)))");
    }

    #[test]
    fn the_term_value_restriction_diagnoses_each_excluded_form_and_builds_the_structure() {
        let value = |text: &str| {
            let source = admitted(text);
            let parse = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
            assert_eq!(parse.syntax().text(), text);
            let forms: Vec<RestrictedForm> = parse
                .diagnostics()
                .iter()
                .filter_map(|d| match d.kind() {
                    SyntaxErrorKind::FormNotAllowedHere { form, context: Restriction::TermValue } => Some(*form),
                    _ => None,
                })
                .collect();
            (sexpr(&parse.syntax()), forms, parse.diagnostics().len())
        };
        assert_eq!(value("X"), ("(TERM_FRAGMENT (VARIABLE_TERM X))".to_owned(), vec![RestrictedForm::Variable], 1));
        assert_eq!(value("_").1, vec![RestrictedForm::AnonymousVariable]);
        assert_eq!(value("1..2").1, vec![RestrictedForm::Interval]);
        assert_eq!(value("(1;2)").1, vec![RestrictedForm::Pool]);
        assert_eq!(value("f(1;2)").1, vec![RestrictedForm::Pool]);
        assert_eq!(value("|1;2|").1, vec![RestrictedForm::PooledAbsoluteValue]);
        assert_eq!(value("@f").1, vec![RestrictedForm::ExternalCall]);
        assert_eq!(value("f(1,2)").2, 0);
        assert_eq!(value("(1,)").2, 0);
        assert_eq!(value("|1|").2, 0);
        assert!(diagnostics("(1;2) .. |X;Y| + @f(_)").is_empty(), "the term family admits every form");
    }

    #[test]
    fn nesting_past_the_constant_is_refused_once_and_carried_losslessly() {
        let depth = MAX_NESTING_DEPTH as usize;
        let admitted_text = format!("{}x{}", "f(".repeat(depth), ")".repeat(depth));
        assert!(diagnostics(&admitted_text).is_empty(), "the constant itself is admitted");
        let refused_text = format!("{}x{} $", "f(".repeat(depth + 1), ")".repeat(depth + 1));
        let source = admitted(&refused_text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(parse.syntax().text(), refused_text, "law 1 under refusal");
        assert_eq!(parse.diagnostics().len(), 1, "one refusal, one diagnostic; the `$` inside is silent");
        assert!(matches!(
            parse.diagnostics()[0].kind(),
            SyntaxErrorKind::NestingTooDeep { depth } if *depth == MAX_NESTING_DEPTH
        ));
        let deepest = parse
            .syntax()
            .descendants()
            .map(|node| node.ancestors().count())
            .max()
            .unwrap_or(0);
        assert!(deepest <= MAX_TREE_DEPTH as usize, "law 3: {deepest} <= {MAX_TREE_DEPTH}");
        assert!(parse.syntax().descendants().any(|node| node.kind() == SyntaxKind::ERROR));
    }

    #[test]
    fn a_frame_free_chain_of_any_length_never_reaches_the_constant() {
        let long = (0..(MAX_NESTING_DEPTH as usize * 4)).map(|_| "1").collect::<Vec<_>>().join("+");
        assert!(diagnostics(&long).is_empty());
        let unary = format!("{}1", "-".repeat(MAX_NESTING_DEPTH as usize * 4));
        assert!(diagnostics(&unary).is_empty());
        let power = (0..(MAX_NESTING_DEPTH as usize * 4)).map(|_| "2").collect::<Vec<_>>().join("**");
        assert!(diagnostics(&power).is_empty());
    }
}
```

- [ ] **Step 4: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib terms`
Expected: compile error — `MAX_NESTING_DEPTH` and the loop are missing
(or, once Step 1 is in, the term entry reads nothing and the shapes
fail).

- [ ] **Step 5: Write the frame loop**

Prepend to `src/parse/terms.rs`, above the test module:

```rust
//! The frame loop (docs/design/syntax.md §6.2, §6.6): the self-recursive
//! term families on one explicit frame stack — a frame per open bracket
//! context, operator structure flat per precedence level, input depth
//! as frame count and never call depth, the depth refusal at the
//! constant, and the restriction contexts read at form emission only.
//! The theory family joins this loop in its own file's arms.
//!
//! The invariant (docs/design/syntax.md §6.2), held at every step: the
//! frame stack mirrors the open bracket contexts of the text, innermost
//! on top; a frame's level stack holds its open precedence levels,
//! strictly tighter from bottom to top, each with the checkpoint taken
//! before its first operand; every operand parsed so far in the frame
//! lies inside the topmost open level or inside a level already closed
//! beneath it; the last operand's checkpoint is kept, so a level can open
//! around it retroactively.

use rowan::Checkpoint;

use crate::diagnostic::{
    Expected, ExpectedSet, Hint, Related, RelatedLocus, RestrictedForm, Restriction, SyntaxClass,
    SyntaxError, SyntaxErrorKind,
};
use crate::dialect::Dialect;
use crate::token::TokenSource;
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::MAX_NESTING_DEPTH;

/// The restriction the loop emits forms under (docs/design/syntax.md
/// §6.2): the general term, `#const`'s constant term (grammar §5.9), or
/// the term-value sublanguage (grammar §5.10). Read at one point — form
/// emission — and never steering the parse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TermContext {
    /// Grammar §5.1's `term`: every form.
    Term,
    /// No variable, no anonymous variable, no pool, no interval, no
    /// pooled absolute value.
    ConstantTerm,
    /// The constant term's exclusions and the `@`-call besides.
    TermValue,
}

impl TermContext {
    fn restriction(self) -> Option<Restriction> {
        match self {
            TermContext::Term => None,
            TermContext::ConstantTerm => Some(Restriction::ConstantTerm),
            TermContext::TermValue => Some(Restriction::TermValue),
        }
    }
}

/// Grammar §5.1's precedence levels, loosest first: a greater level
/// binds tighter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Level {
    Interval,
    BitXor,
    BitOr,
    BitAnd,
    Additive,
    Multiplicative,
    Exponentiation,
}

fn binary_level(kind: SyntaxKind) -> Option<Level> {
    Some(match kind {
        SyntaxKind::DOTDOT => Level::Interval,
        SyntaxKind::CARET => Level::BitXor,
        SyntaxKind::QUESTION => Level::BitOr,
        SyntaxKind::AMPERSAND => Level::BitAnd,
        SyntaxKind::PLUS | SyntaxKind::MINUS => Level::Additive,
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::BACKSLASH => Level::Multiplicative,
        SyntaxKind::STAR_STAR => Level::Exponentiation,
        _ => return None,
    })
}

/// What opened a frame, and so what stands inside it and what closes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    /// The frame-free top of a term: no bracket, no closer; the term
    /// ends at the first token that is not an operator.
    Base,
    /// `( … )` — a pool of tuples (grammar §5.1).
    Pool,
    /// `( … )` after a function's or an `@`-call's name: pooled argument
    /// alternatives, no trailing comma (grammar §5.1).
    Arguments,
    /// `| … |` — the absolute value over a pooled argument.
    Abs,
}

/// One open bracket context.
struct Frame {
    shape: Shape,
    /// The open precedence levels, tighter on top, each with the
    /// checkpoint before its first operand.
    levels: Vec<(Level, Checkpoint)>,
    /// The checkpoint before the last operand — where a level opens
    /// retroactively.
    operand: Option<Checkpoint>,
    /// A `UNARY_TERM` is open, awaiting the one operand that closes it.
    unary_open: bool,
    /// A `TUPLE` is open in this pool or argument list.
    tuple_open: bool,
    /// Terms begun in the open tuple.
    tuple_terms: u32,
    /// The last token consumed in this frame was a `,`.
    after_comma: bool,
    /// A `FUNCTION_TERM` or `EXTERNAL_TERM` node is open around this
    /// argument list, and closes with it.
    wrapper: Option<SyntaxKind>,
    /// The opener's kind and span, for a missing closer's related locus.
    opener: (SyntaxKind, u32, u32),
}

impl Frame {
    fn new(shape: Shape, wrapper: Option<SyntaxKind>, opener: (SyntaxKind, u32, u32)) -> Frame {
        Frame {
            shape,
            levels: Vec::new(),
            operand: None,
            unary_open: false,
            tuple_open: false,
            tuple_terms: 0,
            after_comma: false,
            wrapper,
            opener,
        }
    }

    fn node(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool => Some(SyntaxKind::POOL),
            Shape::Arguments => Some(SyntaxKind::ARGUMENTS),
            Shape::Abs => Some(SyntaxKind::ABS_TERM),
        }
    }

    fn closer(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool | Shape::Arguments => Some(SyntaxKind::R_PAREN),
            Shape::Abs => Some(SyntaxKind::PIPE),
        }
    }
}

/// What the loop does next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Expect an operand.
    Operand,
    /// An operand is complete: expect an operator, a separator, or a closer.
    Operator,
    /// The term is complete, or refused: the loop ends.
    Done,
}

/// The tokens that end a term or a list where an operand or a closer was
/// expected — the loop's synchronization set (docs/design/syntax.md §6.7):
/// nothing is consumed at them; a token outside this set is an intruder,
/// wrapped, and the frame continues.
fn synchronizes(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EOF
            | SyntaxKind::DOT
            | SyntaxKind::COMMA
            | SyntaxKind::SEMICOLON
            | SyntaxKind::R_PAREN
            | SyntaxKind::R_BRACKET
            | SyntaxKind::R_BRACE
            | SyntaxKind::PIPE
            | SyntaxKind::COLON
            | SyntaxKind::NECK
            | SyntaxKind::WEAK_NECK
    )
}

fn expected(items: &[Expected]) -> ExpectedSet {
    items.iter().copied().collect()
}

/// The base frame's opener: it has none.
const NO_OPENER: (SyntaxKind, u32, u32) = (SyntaxKind::EOF, 0, 0);

impl<'s, S: TokenSource> Parser<'s, S> {
    /// Whether the next significant token begins a term.
    pub(super) fn term_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::VARIABLE
                | SyntaxKind::ANONYMOUS
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::MINUS
                | SyntaxKind::TILDE
                | SyntaxKind::L_PAREN
                | SyntaxKind::PIPE
                | SyntaxKind::AT
                | SyntaxKind::SPLICE
        )
    }

    /// One term of the family `context` at the next significant token,
    /// with everything it nests, on the frame stack: false, and nothing
    /// consumed, when no term begins there. After a depth refusal the
    /// rest of the statement has been consumed and `depth_refused()`
    /// holds.
    pub(super) fn term(&mut self, context: TermContext) -> bool {
        if !self.term_begins() {
            return false;
        }
        self.run(context, vec![Frame::new(Shape::Base, None, NO_OPENER)], Next::Operand, false);
        true
    }

    /// The loop resumed after an operand already built at the base — the
    /// atom-shaped prefix the literal parser wraps into a term when an
    /// operator follows it (Task 9): `checkpoint` is that operand's, and
    /// the next token is read as what follows an operand.
    pub(super) fn term_continue(&mut self, context: TermContext, checkpoint: Checkpoint) {
        let mut base = Frame::new(Shape::Base, None, NO_OPENER);
        base.operand = Some(checkpoint);
        self.run(context, vec![base], Next::Operator, false);
    }

    /// An argument list at the next significant token, `(`, on its own
    /// frame — an atom's or a theory atom's arguments, whose enclosing
    /// node is not a term (Task 9, Task 10). Returns when the frame
    /// closes; nothing is read at the base after it.
    pub(super) fn arguments(&mut self, context: TermContext) {
        let mut frames = vec![Frame::new(Shape::Base, None, NO_OPENER)];
        let next = self.open_frame(&mut frames, Shape::Arguments, None, None);
        if next != Next::Done {
            self.run(context, frames, next, true);
        }
    }

    /// The loop itself: one step per iteration, until the term is done —
    /// or, when `stop_at_base` holds, until the frame the caller opened
    /// has closed and only the base remains.
    fn run(&mut self, context: TermContext, mut frames: Vec<Frame>, mut next: Next, stop_at_base: bool) {
        let mut last_operator = None;
        while next != Next::Done {
            next = match next {
                Next::Operand => self.operand(&mut frames, context, last_operator),
                Next::Operator => self.after_operand(&mut frames, context, &mut last_operator),
                Next::Done => Next::Done,
            };
            if stop_at_base && frames.len() == 1 {
                return;
            }
        }
    }

    /// The checkpoint the operand about to begin opens its own node at,
    /// with the trivia before it placed into the open node. It is also
    /// the checkpoint a level opens at retroactively — unless a unary run
    /// is open, whose start already is that checkpoint, the run and its
    /// operand being one operand for the level.
    fn begin_operand(&mut self, frame: &mut Frame) -> Checkpoint {
        frame.after_comma = false;
        if frame.tuple_open {
            frame.tuple_terms += 1;
        }
        let checkpoint = self.checkpoint();
        if !frame.unary_open {
            frame.operand = Some(checkpoint);
        }
        checkpoint
    }

    /// The operand just completed in `frame`: a unary run awaiting it
    /// closes.
    fn complete_operand(&mut self, frame: &mut Frame) {
        if frame.unary_open {
            self.finish_node();
            frame.unary_open = false;
        }
    }

    /// A restricted form at the next significant token: the diagnostic
    /// naming the form and the context, the structure still built.
    fn restricted(&mut self, form: RestrictedForm, context: TermContext) {
        let Some(context) = context.restriction() else {
            return;
        };
        let start = self.peek_start();
        let location = self.location(start, start + self.peek_len());
        self.diagnose(SyntaxError::new(SyntaxErrorKind::FormNotAllowedHere { form, context }, location));
    }

    /// Opens a bracket frame at the next significant token — the opener
    /// — unless it would be the frame past the constant, which is
    /// refused (docs/design/syntax.md §6.6). `retroactive` is the
    /// checkpoint a pool or absolute value opens its node at; an
    /// argument list opens its node here, inside its wrapper. After the
    /// opener a pool or argument list places its first tuple.
    fn open_frame(
        &mut self,
        frames: &mut Vec<Frame>,
        shape: Shape,
        retroactive: Option<Checkpoint>,
        wrapper: Option<SyntaxKind>,
    ) -> Next {
        let nesting = u32::try_from(frames.len() - 1).unwrap_or(u32::MAX);
        if nesting >= MAX_NESTING_DEPTH {
            self.refuse_depth();
            if wrapper.is_some() {
                // The wrapper opened before its frame could: it closes over
                // the refusal like every node above it.
                self.finish_node();
            }
            self.unwind(frames);
            return Next::Done;
        }
        let opener = (self.peek(), self.peek_start(), self.peek_start() + self.peek_len());
        let node = match shape {
            Shape::Pool => SyntaxKind::POOL,
            Shape::Abs => SyntaxKind::ABS_TERM,
            Shape::Arguments => SyntaxKind::ARGUMENTS,
            Shape::Base => unreachable!("the base frame has no opener"),
        };
        match retroactive {
            Some(checkpoint) => self.start_node_at(checkpoint, node),
            None => self.start_node(node),
        }
        self.bump();
        let mut frame = Frame::new(shape, wrapper, opener);
        let next = match shape {
            Shape::Pool | Shape::Arguments => self.tuple_start(&mut frame),
            Shape::Abs | Shape::Base => Next::Operand,
        };
        frames.push(frame);
        next
    }

    /// The tuple after an opener or a pooling `;`: empty — placed
    /// immediately after that token, holding no trivia — when the closer
    /// or the next `;` follows; open otherwise (docs/design/syntax.md
    /// §5.4).
    fn tuple_start(&mut self, frame: &mut Frame) -> Next {
        frame.tuple_terms = 0;
        frame.after_comma = false;
        if matches!(self.peek(), SyntaxKind::R_PAREN | SyntaxKind::SEMICOLON) {
            self.empty_node(SyntaxKind::TUPLE);
            frame.tuple_open = false;
            Next::Operator
        } else {
            self.start_node(SyntaxKind::TUPLE);
            frame.tuple_open = true;
            Next::Operand
        }
    }

    /// Closes the open levels of `frame` tighter than `level`, innermost
    /// first, each wrapped from its checkpoint into its `BINARY_TERM`;
    /// the last closed level's checkpoint becomes the last operand's.
    fn close_levels_tighter_than(&mut self, frame: &mut Frame, level: Option<Level>) {
        while let Some((top, checkpoint)) = frame.levels.last().copied() {
            if level.is_some_and(|level| top <= level) {
                return;
            }
            frame.levels.pop();
            self.finish_node();
            frame.operand = Some(checkpoint);
        }
    }

    /// Closes the top frame's levels, tuple, node, and wrapper, and pops
    /// it; the enclosing frame's operand is then complete.
    fn close_frame(&mut self, frames: &mut Vec<Frame>) -> Next {
        let mut frame = frames.pop().expect("a bracket frame is open");
        self.close_levels_tighter_than(&mut frame, None);
        if frame.tuple_open {
            self.finish_node();
        }
        if frame.node().is_some() {
            self.finish_node();
        }
        if frame.wrapper.is_some() {
            self.finish_node();
        }
        let enclosing = frames.last_mut().expect("the base frame stays");
        self.complete_operand(enclosing);
        Next::Operator
    }

    /// Closes every frame without a diagnostic, after a refusal — the
    /// `ERROR` node stands under the innermost frame, and every open
    /// node above it closes over it (docs/design/syntax.md §6.6).
    fn unwind(&mut self, frames: &mut Vec<Frame>) {
        while let Some(mut frame) = frames.pop() {
            if frame.unary_open {
                self.finish_node();
            }
            self.close_levels_tighter_than(&mut frame, None);
            if frame.tuple_open {
                self.finish_node();
            }
            if frame.node().is_some() {
                self.finish_node();
            }
            if frame.wrapper.is_some() {
                self.finish_node();
            }
        }
    }

    /// The missing closer of the top frame: diagnosed at the token found,
    /// naming the opener, and the frame closed over what it holds.
    fn unclosed(&mut self, frames: &mut Vec<Frame>) -> Next {
        let frame = frames.last().expect("a bracket frame is open");
        let closer = frame.closer().expect("a bracket frame has a closer");
        let (opener_kind, start, end) = frame.opener;
        let related = Related { locus: RelatedLocus::ToClose(opener_kind), location: self.location(start, end) };
        self.unexpected_related(expected(&[Expected::Token(closer)]), None, Some(related));
        self.close_frame(frames)
    }

    /// Expecting an operand: a prefix run, a bracket, a name, a constant,
    /// a variable, a splice — or nothing that begins a term, which is a
    /// missing operand at a synchronizing token and an intruder anywhere
    /// else.
    fn operand(&mut self, frames: &mut Vec<Frame>, context: TermContext, last_operator: Option<SyntaxKind>) -> Next {
        let top = frames.len() - 1;
        match self.peek() {
            SyntaxKind::MINUS | SyntaxKind::TILDE => {
                let frame = &mut frames[top];
                if !frame.unary_open {
                    // The run's start is the level's operand checkpoint; the
                    // operand inside the run opens its own node at its own.
                    let checkpoint = self.begin_operand(frame);
                    self.start_node_at(checkpoint, SyntaxKind::UNARY_TERM);
                    frame.unary_open = true;
                }
                self.bump();
                Next::Operand
            }
            SyntaxKind::L_PAREN => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.open_frame(frames, Shape::Pool, Some(checkpoint), None)
            }
            SyntaxKind::PIPE => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.open_frame(frames, Shape::Abs, Some(checkpoint), None)
            }
            SyntaxKind::AT => {
                self.restricted(RestrictedForm::ExternalCall, context);
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.start_node_at(checkpoint, SyntaxKind::EXTERNAL_TERM);
                self.bump();
                self.expect(SyntaxKind::IDENT);
                if self.peek() == SyntaxKind::L_PAREN {
                    self.open_frame(frames, Shape::Arguments, None, Some(SyntaxKind::EXTERNAL_TERM))
                } else {
                    self.finish_node();
                    self.complete_operand(&mut frames[top]);
                    Next::Operator
                }
            }
            SyntaxKind::IDENT if self.lookahead(1) == SyntaxKind::L_PAREN => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.start_node_at(checkpoint, SyntaxKind::FUNCTION_TERM);
                self.bump();
                self.open_frame(frames, Shape::Arguments, None, Some(SyntaxKind::FUNCTION_TERM))
            }
            SyntaxKind::IDENT
            | SyntaxKind::NUMBER
            | SyntaxKind::STRING
            | SyntaxKind::KW_INF
            | SyntaxKind::KW_SUP => self.leaf(&mut frames[top], SyntaxKind::CONSTANT_TERM),
            SyntaxKind::VARIABLE => {
                self.restricted(RestrictedForm::Variable, context);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::ANONYMOUS => {
                self.restricted(RestrictedForm::AnonymousVariable, context);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::SPLICE => self.leaf(&mut frames[top], SyntaxKind::SPLICE_TERM),
            SyntaxKind::COMMA if frames[top].shape == Shape::Pool && frames[top].tuple_terms == 0 => {
                // `(,)`: the tuple's trailing comma with no terms before it.
                self.bump();
                frames[top].after_comma = true;
                Next::Operator
            }
            kind => {
                let frame = &frames[top];
                let hint = if kind == SyntaxKind::EOF
                    && last_operator == Some(SyntaxKind::QUESTION)
                    && self.dialect() == Dialect::Clingo
                {
                    Some(Hint::QueryMarkNeedsAspCore2)
                } else if kind == SyntaxKind::R_PAREN && frame.shape == Shape::Arguments && frame.after_comma {
                    Some(Hint::TrailingCommaInArguments)
                } else {
                    None
                };
                if synchronizes(kind) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), hint);
                    Next::Operator
                } else {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), hint);
                    Next::Operand
                }
            }
        }
    }

    /// A one-token operand: its node around the token.
    fn leaf(&mut self, frame: &mut Frame, kind: SyntaxKind) -> Next {
        let checkpoint = self.begin_operand(frame);
        self.start_node_at(checkpoint, kind);
        self.bump();
        self.finish_node();
        self.complete_operand(frame);
        Next::Operator
    }

    /// After an operand: a binary operator joins or opens a level; a
    /// separator or a closer acts on the frame; at the base the term
    /// ends at anything else; inside a frame anything else is an
    /// intruder, wrapped, or a synchronizing token that closes the frame
    /// as unclosed.
    fn after_operand(&mut self, frames: &mut Vec<Frame>, context: TermContext, last_operator: &mut Option<SyntaxKind>) -> Next {
        let top = frames.len() - 1;
        let kind = self.peek();
        let query_mark = kind == SyntaxKind::QUESTION
            && self.dialect() == Dialect::AspCore2
            && self.lookahead(1) == SyntaxKind::EOF;
        if let Some(level) = binary_level(kind).filter(|_| !query_mark) {
            if level == Level::Interval {
                self.restricted(RestrictedForm::Interval, context);
            }
            let frame = &mut frames[top];
            self.close_levels_tighter_than(frame, Some(level));
            if frame.levels.last().map(|(open, _)| *open) != Some(level) {
                let checkpoint = frame.operand.expect("an operand precedes an operator");
                self.start_node_at(checkpoint, SyntaxKind::BINARY_TERM);
                frame.levels.push((level, checkpoint));
            }
            self.bump();
            *last_operator = Some(kind);
            return Next::Operand;
        }
        match frames[top].shape {
            Shape::Base => {
                self.close_levels_tighter_than(&mut frames[top], None);
                Next::Done
            }
            Shape::Pool | Shape::Arguments => match kind {
                SyntaxKind::COMMA => {
                    let frame = &mut frames[top];
                    self.close_levels_tighter_than(frame, None);
                    self.bump();
                    frame.after_comma = true;
                    if frame.shape == Shape::Pool && matches!(self.peek(), SyntaxKind::R_PAREN | SyntaxKind::SEMICOLON) {
                        Next::Operator
                    } else {
                        Next::Operand
                    }
                }
                SyntaxKind::SEMICOLON => {
                    self.restricted(RestrictedForm::Pool, context);
                    let frame = &mut frames[top];
                    self.close_levels_tighter_than(frame, None);
                    if frame.tuple_open {
                        self.finish_node();
                        frame.tuple_open = false;
                    }
                    self.bump();
                    self.tuple_start(frame)
                }
                SyntaxKind::R_PAREN => {
                    self.bump_closer_and_close(frames)
                }
                kind if synchronizes(kind) => self.unclosed(frames),
                _ => {
                    self.wrap_unexpected(
                        expected(&[
                            Expected::Token(SyntaxKind::COMMA),
                            Expected::Token(SyntaxKind::SEMICOLON),
                            Expected::Token(SyntaxKind::R_PAREN),
                        ]),
                        None,
                    );
                    Next::Operator
                }
            },
            Shape::Abs => match kind {
                SyntaxKind::PIPE => self.bump_closer_and_close(frames),
                SyntaxKind::SEMICOLON => {
                    self.restricted(RestrictedForm::PooledAbsoluteValue, context);
                    self.close_levels_tighter_than(&mut frames[top], None);
                    self.bump();
                    Next::Operand
                }
                kind if synchronizes(kind) => self.unclosed(frames),
                _ => {
                    self.wrap_unexpected(
                        expected(&[Expected::Token(SyntaxKind::SEMICOLON), Expected::Token(SyntaxKind::PIPE)]),
                        None,
                    );
                    Next::Operator
                }
            },
        }
    }

    /// The closer of the top frame: the levels close, the tuple closes,
    /// the closer is placed, and the frame closes over it.
    fn bump_closer_and_close(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        self.close_levels_tighter_than(&mut frames[top], None);
        if frames[top].tuple_open {
            self.finish_node();
            frames[top].tuple_open = false;
        }
        self.bump();
        self.close_frame(frames)
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p themelios-syntax --lib`
Expected: the `terms` tests pass (14) and every earlier test still
passes; the Task 7 fragment test `input_after_a_fragment_is_an_error_node_expecting_end_of_input`
now reads `p` as a term and still finds the `q` diagnosed. If a shape
differs from the listed S-expression by trivia placement or an empty
node's position, the loop is wrong, not the test: the shapes are the
design's (syntax.md §5.4, §6.2, Appendix A).

- [ ] **Step 7: Run the full gate, then commit**

Run the four gate commands. Expected: green; remove the Task 7
`#[allow(dead_code)]` on the machine — every method now has a caller —
and repair or argue any pedantic lint that fires (`too_many_lines` on
`operand` or `after_operand` is allowed at the function with the
argument that the loop's two phases are each one match over the roster).

```bash
git add crates/themelios-syntax
git commit -m "Add the frame loop: the term families flat per precedence level on one explicit frame stack, the restriction contexts, and the depth refusal at the named constant"
```

---

### Task 9: Rules — literals, atoms, comparisons, heads, bodies, conditional literals, aggregates

**Files:**
- Create: `crates/themelios-syntax/src/parse/statements.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs` (`mod statements;`),
  `src/parse/machine.rs` (the statement dispatch moves to
  `statements.rs`; the loops restore the parser after a refusal)

**Derives:** syntax.md §5.4 (law 2 under docs; the three empty nodes),
§6.2 (the grammar-bounded productions on the call stack), §6.3 (the
docs production; the query reading's one-token peek where an atom
stands), §6.7 (the rows: head, body, condition; literal, atom,
comparison; aggregates; end of input), §7.1 (`Hint::EmptyConditionBeforePipe`),
§8.1 (negation placed inside the element it signs), Appendix A;
grammar §5.2–§5.7.

**Interfaces:**
- Consumes: the machine (Tasks 7–8), `Parser::{term, term_continue,
  arguments, term_begins}`, `terms::TermContext`.
- Produces: `Parser::statement(Checkpoint)` dispatching to
  `Parser::rule`; the crate-private `Parser::{literal_begins,
  element_begins, element, literal, condition, aggregate, body, head,
  statement_end}` and `Position` that Tasks 10–11 read; the query hook
  `Parser::is_query_mark()`.

- [ ] **Step 1: Write the failing tests**

Create `src/parse/statements.rs` holding only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use crate::diagnostic::{Hint, MisplacedDoc, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::sexpr;

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    /// The program's shape with the root peeled: one statement's shape.
    fn shape(text: &str) -> String {
        let source = admitted(text);
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(PROGRAM ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn kinds(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse(&source, Dialect::Clingo).diagnostics().iter().map(|d| d.kind().clone()).collect()
    }

    fn member(text: &str) -> bool {
        let source = admitted(text);
        !parse(&source, Dialect::Clingo).has_errors()
    }

    #[test]
    fn facts_and_rules_take_the_five_forms() {
        assert_eq!(shape("p."), "(RULE (LITERAL (ATOM p)) .)");
        assert_eq!(shape("h :- ."), "(RULE (LITERAL (ATOM h)) :- (BODY) .)");
        assert_eq!(shape(":- ."), "(RULE :- (BODY) .)");
        assert_eq!(shape(":- q."), "(RULE :- (BODY (LITERAL (ATOM q))) .)");
        assert_eq!(
            shape("p :- q, not r; not not s."),
            "(RULE (LITERAL (ATOM p)) :- (BODY (LITERAL (ATOM q)) , (LITERAL not (ATOM r)) ; (LITERAL not not (ATOM s))) .)"
        );
        assert!(member("p :- q, not r; not not s."));
    }

    #[test]
    fn atoms_carry_their_sign_name_and_arguments() {
        assert_eq!(
            shape("-p(X, f(Y))."),
            "(RULE (LITERAL (ATOM - p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X) , (FUNCTION_TERM f (ARGUMENTS ( (TUPLE (VARIABLE_TERM Y)) )))) )))) .)"
        );
        assert_eq!(shape("p(a;b)."), "(RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE (CONSTANT_TERM b)) )))) .)");
        assert_eq!(shape("#true."), "(RULE (LITERAL #true) .)");
        assert_eq!(shape("not #false."), "(RULE (LITERAL not #false) .)");
    }

    #[test]
    fn comparisons_chain_and_a_bare_term_is_no_literal() {
        assert_eq!(
            shape("1 < X < 5."),
            "(RULE (LITERAL (COMPARISON (CONSTANT_TERM 1) < (VARIABLE_TERM X) < (CONSTANT_TERM 5))) .)"
        );
        assert_eq!(
            shape("-p < 3."),
            "(RULE (LITERAL (COMPARISON (UNARY_TERM - (CONSTANT_TERM p)) < (CONSTANT_TERM 3))) .)"
        );
        assert_eq!(
            shape("p(X) + 1 = 3."),
            "(RULE (LITERAL (COMPARISON (BINARY_TERM (FUNCTION_TERM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) + (CONSTANT_TERM 1)) = (CONSTANT_TERM 3))) .)"
        );
        assert_eq!(shape("X = 1."), "(RULE (LITERAL (COMPARISON (VARIABLE_TERM X) = (CONSTANT_TERM 1))) .)");
        assert!(!member("1."));
        assert!(!member("X."));
        assert!(member("p(X) + 1 = 3."));
    }

    #[test]
    fn disjunctions_take_the_three_separators_and_the_singleton_conditioned_head() {
        assert_eq!(
            shape("a ; b | c, d."),
            "(RULE (DISJUNCTION (LITERAL (ATOM a)) ; (LITERAL (ATOM b)) | (LITERAL (ATOM c)) , (LITERAL (ATOM d))) .)"
        );
        assert_eq!(
            shape("p(X) : q(X)."),
            "(RULE (DISJUNCTION (CONDITIONAL_LITERAL (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))))) .)"
        );
        assert_eq!(shape("a : ."), "(RULE (DISJUNCTION (CONDITIONAL_LITERAL (LITERAL (ATOM a)) : (CONDITION))) .)");
        assert!(member("a : b, c; d : ."));
        assert!(!member("p(X) : | q(X)."));
        assert!(kinds("p(X) : | q(X).").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { hint: Some(Hint::EmptyConditionBeforePipe), .. }
        )));
    }

    #[test]
    fn conditional_literals_absorb_commas_and_end_at_the_semicolon_or_the_dot() {
        assert_eq!(
            shape(":- p : q, r, s."),
            "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION (LITERAL (ATOM q)) , (LITERAL (ATOM r)) , (LITERAL (ATOM s))))) .)"
        );
        assert_eq!(shape(":- p : ."), "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION))) .)");
        assert_eq!(
            shape(":- p : q; t."),
            "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION (LITERAL (ATOM q)))) ; (LITERAL (ATOM t))) .)"
        );
        assert!(member(":- p : ."));
    }

    #[test]
    fn aggregates_take_guards_functions_and_position_shaped_elements() {
        assert_eq!(
            shape(":- #sum { W,T : task(T), weight(T,W) } >= 4."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT (VARIABLE_TERM W) , (VARIABLE_TERM T) : (CONDITION (LITERAL (ATOM task (ARGUMENTS ( (TUPLE (VARIABLE_TERM T)) )))) , (LITERAL (ATOM weight (ARGUMENTS ( (TUPLE (VARIABLE_TERM T) , (VARIABLE_TERM W)) )))))) } (GUARD >= (CONSTANT_TERM 4)))) .)"
        );
        assert_eq!(
            shape("1 { p(X) : q(X) } 1 :- r."),
            "(RULE (SET_AGGREGATE (GUARD (CONSTANT_TERM 1)) { (CONDITIONAL_LITERAL (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))))) } (GUARD (CONSTANT_TERM 1))) :- (BODY (LITERAL (ATOM r))) .)"
        );
        assert_eq!(
            shape(":- X = #count { a; b }."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE (GUARD (VARIABLE_TERM X) =) #count { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a)) ; (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM b)) })) .)"
        );
        assert_eq!(
            shape(":- not #count { } 1."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE not #count { } (GUARD (CONSTANT_TERM 1)))) .)"
        );
        assert_eq!(
            shape("#min { X : p(X) : q(X) }."),
            "(RULE (FUNCTION_AGGREGATE #min { (HEAD_AGGREGATE_ELEMENT (VARIABLE_TERM X) : (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))))) }) .)"
        );
        assert_eq!(shape(":- #sum { : }."), "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT : (CONDITION)) })) .)");
        assert_eq!(shape(":- #sum { a : }."), "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a) : (CONDITION)) })) .)");
        assert!(member(":- #sum { : }."));
        assert!(!member("#sum { : }."), "a head element requires its literal");
        assert!(member("{ p; q : r } :- s."));
        assert!(member(":- 1 <= #count { X : p(X) } < 3, not #sum+ { 1 : q } = 2."));
    }

    #[test]
    fn missing_separators_and_dots_are_diagnosed_and_the_parse_continues() {
        assert_eq!(
            shape("p(X) :- q(X) r(X)."),
            "(RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) :- (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) (LITERAL (ATOM r (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)"
        );
        assert_eq!(kinds("p(X) :- q(X) r(X).").len(), 1);
        assert_eq!(shape("p q."), "(RULE (LITERAL (ATOM p))) (RULE (LITERAL (ATOM q)) .)");
        assert_eq!(kinds("p q.").len(), 1);
        assert_eq!(shape(":- p, , q."), "(RULE :- (BODY (LITERAL (ATOM p)) , (ERROR ,) (LITERAL (ATOM q))) .)");
        assert_eq!(shape(":- #count { a ."), "(RULE :- (BODY (FUNCTION_AGGREGATE #count { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a)))) .)");
        assert!(kinds(":- #count { a .").iter().any(|kind| matches!(kind, SyntaxErrorKind::UnexpectedToken { .. })));
    }

    #[test]
    fn documentation_belongs_to_the_statement_and_a_doc_line_inside_one_is_trivia() {
        assert_eq!(shape("%! doc\np."), "(RULE %! doc (LITERAL (ATOM p)) .)");
        assert!(kinds("%! doc\np.").is_empty());
        assert_eq!(shape("%! a\n\n%! b\np."), "(RULE %! a %! b (LITERAL (ATOM p)) .)");
        assert!(kinds("%! a\n\n%! b\np.").is_empty());
        let inside = kinds("p :- %! x\n q.");
        assert_eq!(inside.len(), 1);
        assert!(matches!(inside[0], SyntaxErrorKind::MisplacedDocComment { reason: MisplacedDoc::InsideStatement }));
        assert!(member("p :- %! x\n q."));
    }

    #[test]
    fn the_query_is_read_by_dialect_at_the_programs_end() {
        let query = |text: &str, dialect: Dialect| {
            let source = admitted(text);
            let parse = parse(&source, dialect);
            (sexpr(&parse.syntax()), parse.diagnostics().iter().map(|d| d.kind().clone()).collect::<Vec<_>>())
        };
        let (shape, kinds) = query("p(1)?", Dialect::AspCore2);
        assert_eq!(shape, "(PROGRAM (QUERY (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) ?))");
        assert!(kinds.is_empty());
        let (shape, kinds) = query("%! q\np(1)?", Dialect::AspCore2);
        assert_eq!(shape, "(PROGRAM (QUERY %! q (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) ?))");
        assert!(kinds.is_empty());
        let (_, kinds) = query("p(1)?", Dialect::Clingo);
        assert!(kinds.iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedEndOfInput { hint: Some(Hint::QueryMarkNeedsAspCore2), .. }
        )));
        for dialect in [Dialect::Clingo, Dialect::AspCore2] {
            assert!(!query("p ? q.", dialect).1.is_empty(), "the same error under both dialects");
            assert!(query("p ? q = X.", dialect).1.is_empty());
            assert!(query("p(1)?2 > 3.", dialect).1.is_empty());
            assert!(query("x(1?2).", dialect).1.is_empty());
        }
        assert_eq!(query("p ? q = X.", Dialect::AspCore2).0, query("p ? q = X.", Dialect::Clingo).0);
    }

    #[test]
    fn a_program_of_several_statements_places_trivia_between_them_at_the_program() {
        let source = admitted("p. % c\n\nq :- p.\n");
        let parse = parse(&source, Dialect::Clingo);
        assert!(!parse.has_errors());
        let statements: Vec<String> = parse.syntax().children().map(|node| node.text().to_string()).collect();
        assert_eq!(statements, ["p.", "q :- p."]);
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib statements`
Expected: compile error (the module is not declared) — add
`mod statements;` under `mod terms;` in `src/parse/mod.rs`, run again:
the shapes fail because every statement start still recovers.

- [ ] **Step 3: Move the dispatch and write the families**

Delete the interim `statement` from `src/parse/machine.rs`, and make the
two loops restore the parser after a refused statement — in
`statements()` and `statement_fragment()`, right after
`self.statement(checkpoint);`:

```rust
                if self.depth_refused() {
                    self.end_statement_after_refusal();
                }
```

Prepend to `src/parse/statements.rs`, above the test module:

```rust
//! The grammar-bounded productions (docs/design/syntax.md §6.2, §6.3,
//! §6.7): rules and what they hold — literals, atoms, comparisons,
//! disjunctions, bodies, conditional literals, aggregates — each a
//! function on the call stack, its depth bounded by the grammar and
//! never by the input; the term families below them run on the frame
//! loop. The directives, weak constraints, and optimize statements join
//! the dispatch in Task 11, the theory atoms and definitions in Task 10.

use rowan::Checkpoint;

use crate::diagnostic::{Expected, ExpectedSet, Hint, SyntaxClass};
use crate::dialect::Dialect;
use crate::token::TokenSource;
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

/// Where an element stands, which fixes what may stand there: a head or
/// a body admits literals, aggregates, and theory atoms, and a literal
/// may take a condition; a set-aggregate or disjunction element admits a
/// literal with an optional condition; a condition, and a head
/// aggregate element's literal, admit a plain literal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Position {
    Head,
    Body,
    SetElement,
    ConditionLiteral,
}

impl Position {
    fn admits_aggregates(self) -> bool {
        matches!(self, Position::Head | Position::Body)
    }

    fn admits_condition(self) -> bool {
        !matches!(self, Position::ConditionLiteral)
    }
}

/// What an element parse read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Element {
    /// Nothing begins here; nothing was consumed.
    None,
    /// A literal, conditional when `condition` says so; `empty` when the
    /// condition after the colon held nothing.
    Literal { condition: bool, empty: bool },
    /// An aggregate.
    Aggregate,
    /// A bare atom in head position with the query mark after it — the
    /// ASP-Core-2 query's atom, not wrapped in a literal (grammar §6.1).
    QueryAtom,
}

/// The atom-shaped prefix `[-] IDENT [ arguments ]` of a literal or a
/// guard, kept as tokens until the token after it says what it was: an
/// atom, or the first term of a comparison or a guard.
struct AtomPrefix {
    /// Before the sign, if any.
    start: Checkpoint,
    negated: bool,
    /// Before the name.
    name: Checkpoint,
    has_arguments: bool,
}

/// What begins a literal or a guard: an atom-shaped prefix still open to
/// both readings, a complete term, or nothing.
enum First {
    Atom(AtomPrefix),
    Term(Checkpoint),
    Missing,
}

fn relation(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LT | SyntaxKind::LE | SyntaxKind::GT | SyntaxKind::GE | SyntaxKind::EQ | SyntaxKind::NEQ
    )
}

fn aggregate_opener(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::L_BRACE
            | SyntaxKind::KW_COUNT
            | SyntaxKind::KW_SUM
            | SyntaxKind::KW_SUM_PLUS
            | SyntaxKind::KW_MIN
            | SyntaxKind::KW_MAX
    )
}

fn expected(items: &[Expected]) -> ExpectedSet {
    items.iter().copied().collect()
}

const RELATIONS: [Expected; 6] = [
    Expected::Token(SyntaxKind::LT),
    Expected::Token(SyntaxKind::LE),
    Expected::Token(SyntaxKind::GT),
    Expected::Token(SyntaxKind::GE),
    Expected::Token(SyntaxKind::EQ),
    Expected::Token(SyntaxKind::NEQ),
];

impl<'s, S: TokenSource> Parser<'s, S> {
    /// One statement, its node opened at `checkpoint` (before its docs).
    /// The families of Tasks 10–11 replace their arms; until then their
    /// statement starts recover at program level.
    pub(super) fn statement(&mut self, checkpoint: Checkpoint) {
        self.enter_statement();
        match self.peek() {
            SyntaxKind::WEAK_NECK
            | SyntaxKind::KW_MINIMIZE
            | SyntaxKind::KW_MAXIMIZE
            | SyntaxKind::KW_SHOW
            | SyntaxKind::KW_PROJECT
            | SyntaxKind::KW_DEFINED
            | SyntaxKind::KW_EDGE
            | SyntaxKind::KW_HEURISTIC
            | SyntaxKind::KW_EXTERNAL
            | SyntaxKind::KW_CONST
            | SyntaxKind::KW_SCRIPT
            | SyntaxKind::KW_INCLUDE
            | SyntaxKind::KW_PROGRAM
            | SyntaxKind::KW_THEORY => self.recover_program_level(),
            _ => self.rule(checkpoint),
        }
        self.leave_statement();
    }

    /// Grammar §6.1's query reading: under the ASP-Core-2 dialect only,
    /// a `?` whose next significant token is end of input.
    pub(super) fn is_query_mark(&mut self) -> bool {
        self.dialect() == Dialect::AspCore2
            && self.peek() == SyntaxKind::QUESTION
            && self.lookahead(1) == SyntaxKind::EOF
    }

    /// Whether the next significant token continues a term as a binary
    /// operator — the query mark excluded.
    fn binary_operator_follows(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::DOTDOT
                | SyntaxKind::CARET
                | SyntaxKind::QUESTION
                | SyntaxKind::AMPERSAND
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::BACKSLASH
                | SyntaxKind::STAR_STAR
        ) && !self.is_query_mark()
    }

    /// Whether the next significant token begins a literal.
    pub(super) fn literal_begins(&mut self) -> bool {
        matches!(self.peek(), SyntaxKind::KW_NOT | SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE) || self.term_begins()
    }

    /// Whether the next significant token begins an element in `position`.
    pub(super) fn element_begins(&mut self, position: Position) -> bool {
        self.literal_begins()
            || (position.admits_aggregates() && (aggregate_opener(self.peek()) || self.peek() == SyntaxKind::AMPERSAND))
    }

    /// Grammar §5.7's `rule`, all five forms, opened at `checkpoint` — or
    /// grammar §6.1's query, when the head is a bare atom followed by the
    /// query mark: the statement parser, holding that atom with nothing
    /// before it, closes a `QUERY` (docs/design/syntax.md §6.3).
    pub(super) fn rule(&mut self, checkpoint: Checkpoint) {
        if self.peek() != SyntaxKind::NECK && self.head() {
            self.start_node_at(checkpoint, SyntaxKind::QUERY);
            self.bump();
            self.finish_node();
            return;
        }
        self.start_node_at(checkpoint, SyntaxKind::RULE);
        if !self.depth_refused() && self.eat(SyntaxKind::NECK) {
            if self.peek() == SyntaxKind::DOT {
                self.empty_node(SyntaxKind::BODY);
            } else {
                self.body();
            }
        }
        self.statement_end();
        self.finish_node();
    }

    /// The statement's terminating dot: consumed when next; a missing
    /// dot before a statement start or end of input is diagnosed and
    /// nothing consumed; anything else before the dot is skipped into an
    /// `ERROR` node through to it. After a depth refusal the rest of the
    /// statement is already carried, and nothing happens here.
    pub(super) fn statement_end(&mut self) {
        if self.depth_refused() || self.eat(SyntaxKind::DOT) {
            return;
        }
        if self.at_end() || self.statement_begins() {
            self.expected_token(SyntaxKind::DOT);
            return;
        }
        self.skip_into_error(expected(&[Expected::Token(SyntaxKind::DOT)]), None, &[SyntaxKind::DOT]);
        self.eat(SyntaxKind::DOT);
    }

    /// Grammar §5.5's `head`: a literal, a disjunction, an aggregate, or a
    /// theory atom. A conditioned literal or any separator after the
    /// first element makes the head a `DISJUNCTION`. True when the head
    /// is the query's atom (grammar §6.1), which the caller closes.
    fn head(&mut self) -> bool {
        let checkpoint = self.checkpoint();
        let mut previous = match self.element(Position::Head) {
            Element::None => {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Head)]), None);
                return false;
            }
            Element::QueryAtom => return true,
            Element::Aggregate => return false,
            literal => literal,
        };
        let separator = |kind: SyntaxKind| matches!(kind, SyntaxKind::SEMICOLON | SyntaxKind::PIPE | SyntaxKind::COMMA);
        let conditioned = matches!(previous, Element::Literal { condition: true, .. });
        if !conditioned && !separator(self.peek()) {
            return false;
        }
        self.start_node_at(checkpoint, SyntaxKind::DISJUNCTION);
        loop {
            if self.depth_refused() {
                break;
            }
            let kind = self.peek();
            if separator(kind) {
                if kind == SyntaxKind::PIPE && matches!(previous, Element::Literal { condition: true, empty: true }) {
                    // Grammar §5.5's stated hole: an empty-conditioned element
                    // directly before `|` does not parse; the parser names it
                    // and reads on as if `;` stood there.
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Condition)]), Some(Hint::EmptyConditionBeforePipe));
                }
                self.bump();
            } else if self.element_begins(Position::SetElement) {
                self.unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::PIPE),
                        Expected::Token(SyntaxKind::COMMA),
                        Expected::Token(SyntaxKind::NECK),
                        Expected::Token(SyntaxKind::DOT),
                    ]),
                    None,
                );
            } else {
                break;
            }
            previous = match self.element(Position::SetElement) {
                Element::None => {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    if separator(self.peek()) {
                        self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                        continue;
                    }
                    break;
                }
                element => element,
            };
        }
        self.finish_node();
        false
    }

    /// Grammar §5.6's `body-list`: elements between `,` and `;`. A missing
    /// separator before an element that begins is diagnosed and read
    /// past; a token that begins nothing is wrapped and the list
    /// continues; the neck and the dot end it.
    pub(super) fn body(&mut self) {
        self.start_node(SyntaxKind::BODY);
        loop {
            if self.depth_refused() {
                break;
            }
            if self.element(Position::Body) == Element::None {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::BodyElement)]), None);
                match self.peek() {
                    SyntaxKind::COMMA | SyntaxKind::SEMICOLON => {
                        self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::BodyElement)]), None);
                        continue;
                    }
                    SyntaxKind::DOT | SyntaxKind::EOF | SyntaxKind::NECK | SyntaxKind::WEAK_NECK => break,
                    _ if self.statement_begins() => break,
                    _ => {
                        self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::BodyElement)]), None);
                        continue;
                    }
                }
            }
            match self.peek() {
                SyntaxKind::COMMA | SyntaxKind::SEMICOLON => self.bump(),
                _ if self.element_begins(Position::Body) => self.unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::COMMA),
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::DOT),
                    ]),
                    None,
                ),
                _ => break,
            }
        }
        self.finish_node();
    }

    /// One element in `position` (grammar §5.2–§5.6, §5.8): the negation
    /// run first, placed inside whatever the element turns out to be —
    /// a literal, an aggregate, or a theory atom — then the element by
    /// its first token, and, where the position admits it, a condition
    /// after `:` wrapping the literal into a `CONDITIONAL_LITERAL`.
    pub(super) fn element(&mut self, position: Position) -> Element {
        if !self.element_begins(position) {
            return Element::None;
        }
        let start = self.checkpoint();
        let mut negation = 0;
        while self.peek() == SyntaxKind::KW_NOT && negation < 2 {
            self.bump();
            negation += 1;
        }
        match self.peek() {
            SyntaxKind::AMPERSAND => {
                // Task 10 reads the theory atom here.
                self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                Element::None
            }
            kind if aggregate_opener(kind) => {
                if !position.admits_aggregates() {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                }
                self.aggregate(start, position);
                Element::Aggregate
            }
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE => {
                self.start_node_at(start, SyntaxKind::LITERAL);
                self.bump();
                self.finish_node();
                self.conditional(start, position)
            }
            _ => match self.first() {
                First::Missing => {
                    self.start_node_at(start, SyntaxKind::LITERAL);
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    self.finish_node();
                    Element::Literal { condition: false, empty: false }
                }
                first => {
                    if self.depth_refused() {
                        self.finish_first_after_refusal(first);
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        return Element::Literal { condition: false, empty: false };
                    }
                    let kind = self.peek();
                    let guard_follows = aggregate_opener(kind)
                        || (relation(kind) && aggregate_opener(self.lookahead(1)));
                    if guard_follows {
                        if !position.admits_aggregates() {
                            self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                        }
                        let term = self.as_term(first);
                        self.start_node_at(term, SyntaxKind::GUARD);
                        if relation(self.peek()) {
                            self.bump();
                        }
                        self.finish_node();
                        self.aggregate(start, position);
                        Element::Aggregate
                    } else if relation(kind) {
                        let term = self.as_term(first);
                        self.comparison(term);
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        self.conditional(start, position)
                    } else {
                        match first {
                            First::Atom(prefix) => {
                                self.start_node_at(prefix.start, SyntaxKind::ATOM);
                                self.finish_node();
                                if position == Position::Head && negation == 0 && self.is_query_mark() {
                                    return Element::QueryAtom;
                                }
                            }
                            First::Term(_) => self.unexpected(expected(&RELATIONS), None),
                            First::Missing => unreachable!("matched above"),
                        }
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        self.conditional(start, position)
                    }
                }
            },
        }
    }

    /// The atom-shaped prefix `[-] IDENT [ arguments ]`, or the term,
    /// that begins a literal or a guard (grammar §5.1–§5.2: `-p` is the
    /// atom in literal position and the arithmetic term where a relation
    /// makes the whole a comparison — the token after the prefix
    /// decides). An operator directly after the prefix makes it a term
    /// at once, and the frame loop continues from it.
    fn first(&mut self) -> First {
        let atom_shaped = self.peek() == SyntaxKind::IDENT
            || (self.peek() == SyntaxKind::MINUS && self.lookahead(1) == SyntaxKind::IDENT);
        if atom_shaped {
            let start = self.checkpoint();
            let negated = self.eat(SyntaxKind::MINUS);
            let name = self.checkpoint();
            self.bump();
            let has_arguments = self.peek() == SyntaxKind::L_PAREN;
            if has_arguments {
                self.arguments(TermContext::Term);
            }
            let prefix = AtomPrefix { start, negated, name, has_arguments };
            if !self.depth_refused() && self.binary_operator_follows() {
                self.wrap_prefix_as_term(&prefix);
                self.term_continue(TermContext::Term, start);
                return First::Term(start);
            }
            return First::Atom(prefix);
        }
        if self.term_begins() {
            let start = self.checkpoint();
            self.term(TermContext::Term);
            return First::Term(start);
        }
        First::Missing
    }

    /// The prefix as the term it turned out to be: `FUNCTION_TERM` or
    /// `CONSTANT_TERM` around the name and its arguments, and
    /// `UNARY_TERM` around the sign.
    fn wrap_prefix_as_term(&mut self, prefix: &AtomPrefix) {
        let kind = if prefix.has_arguments { SyntaxKind::FUNCTION_TERM } else { SyntaxKind::CONSTANT_TERM };
        self.start_node_at(prefix.name, kind);
        self.finish_node();
        if prefix.negated {
            self.start_node_at(prefix.start, SyntaxKind::UNARY_TERM);
            self.finish_node();
        }
    }

    /// The first thing as a term, whichever it was: its checkpoint.
    fn as_term(&mut self, first: First) -> Checkpoint {
        match first {
            First::Atom(prefix) => {
                self.wrap_prefix_as_term(&prefix);
                prefix.start
            }
            First::Term(checkpoint) => checkpoint,
            First::Missing => unreachable!("a missing first is handled before"),
        }
    }

    /// After a depth refusal inside the first thing: the tokens stand as
    /// they were placed, an atom-shaped prefix closing as an atom.
    fn finish_first_after_refusal(&mut self, first: First) {
        if let First::Atom(prefix) = first {
            self.start_node_at(prefix.start, SyntaxKind::ATOM);
            self.finish_node();
        }
    }

    /// Grammar §5.2's `comparison` around a first term already built at
    /// `first`: the chain of relations and terms, one flat node.
    fn comparison(&mut self, first: Checkpoint) {
        self.start_node_at(first, SyntaxKind::COMPARISON);
        while relation(self.peek()) {
            self.bump();
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                break;
            }
            if self.depth_refused() {
                break;
            }
        }
        self.finish_node();
    }

    /// The condition a literal in `position` may take: `:` then a
    /// condition, or the empty condition placed immediately after the
    /// colon; the literal opened at `start` becomes a
    /// `CONDITIONAL_LITERAL`.
    fn conditional(&mut self, start: Checkpoint, position: Position) -> Element {
        if !position.admits_condition() || self.peek() != SyntaxKind::COLON {
            return Element::Literal { condition: false, empty: false };
        }
        self.start_node_at(start, SyntaxKind::CONDITIONAL_LITERAL);
        self.bump();
        let empty = !self.literal_begins();
        if empty {
            self.empty_node(SyntaxKind::CONDITION);
        } else {
            self.condition();
        }
        self.finish_node();
        Element::Literal { condition: true, empty }
    }

    /// Grammar §5.3's `condition`: literals between commas, at least one
    /// (the empty condition is its caller's empty node).
    pub(super) fn condition(&mut self) {
        self.start_node(SyntaxKind::CONDITION);
        loop {
            if self.element(Position::ConditionLiteral) == Element::None {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                if self.peek() == SyntaxKind::COMMA {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    continue;
                }
                break;
            }
            if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                break;
            }
        }
        self.finish_node();
    }

    /// Grammar §5.3's aggregate, its node opened at `start` (around the
    /// negation run and the left guard already built): the function
    /// keyword where there is one, the braces with position-shaped
    /// elements between `;`, and the right guard.
    fn aggregate(&mut self, start: Checkpoint, position: Position) {
        let kind = if self.peek() == SyntaxKind::L_BRACE {
            SyntaxKind::SET_AGGREGATE
        } else {
            SyntaxKind::FUNCTION_AGGREGATE
        };
        self.start_node_at(start, kind);
        if kind == SyntaxKind::FUNCTION_AGGREGATE {
            self.bump();
        }
        if self.expect(SyntaxKind::L_BRACE) {
            self.aggregate_elements(kind, position);
            if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        if !self.depth_refused() {
            self.right_guard();
        }
        self.finish_node();
    }

    /// The elements between the braces, separated by `;`, until the
    /// closing brace, the dot, or end of input.
    fn aggregate_elements(&mut self, kind: SyntaxKind, position: Position) {
        loop {
            match self.peek() {
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                _ => {}
            }
            let read = if kind == SyntaxKind::SET_AGGREGATE {
                self.element(Position::SetElement) != Element::None
            } else {
                self.function_aggregate_element(position)
            };
            if self.depth_refused() {
                return;
            }
            if !read {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                match self.peek() {
                    SyntaxKind::SEMICOLON => {}
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                    _ => {
                        self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                        continue;
                    }
                }
            }
            match self.peek() {
                SyntaxKind::SEMICOLON => self.bump(),
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                _ => self.wrap_unexpected(
                    expected(&[Expected::Token(SyntaxKind::SEMICOLON), Expected::Token(SyntaxKind::R_BRACE)]),
                    None,
                ),
            }
        }
    }

    /// Grammar §5.3's `fn-element`: in body position terms with an
    /// optional condition, or a bare colon with one; in head position
    /// terms, the colon, the literal, and an optional condition. Returns
    /// whether an element began.
    fn function_aggregate_element(&mut self, position: Position) -> bool {
        let head = position == Position::Head;
        if !(self.term_begins() || self.peek() == SyntaxKind::COLON) {
            return false;
        }
        let kind = if head { SyntaxKind::HEAD_AGGREGATE_ELEMENT } else { SyntaxKind::BODY_AGGREGATE_ELEMENT };
        self.start_node(kind);
        if self.peek() != SyntaxKind::COLON {
            loop {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    break;
                }
                if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                    break;
                }
            }
        }
        if !self.depth_refused() {
            if head {
                if self.expect(SyntaxKind::COLON) && self.element(Position::ConditionLiteral) == Element::None {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                }
                if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
                    self.condition_or_empty();
                }
            } else if self.eat(SyntaxKind::COLON) {
                self.condition_or_empty();
            }
        }
        self.finish_node();
        true
    }

    /// After a colon: the condition, or the empty condition placed
    /// immediately after the colon.
    fn condition_or_empty(&mut self) {
        if self.literal_begins() {
            self.condition();
        } else {
            self.empty_node(SyntaxKind::CONDITION);
        }
    }

    /// Grammar §5.3's `rguard`: a relation and a term, or a bare term.
    fn right_guard(&mut self) {
        if relation(self.peek()) {
            let checkpoint = self.checkpoint();
            self.bump();
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
            }
            self.start_node_at(checkpoint, SyntaxKind::GUARD);
            self.finish_node();
        } else if self.term_begins() {
            let checkpoint = self.checkpoint();
            self.term(TermContext::Term);
            self.start_node_at(checkpoint, SyntaxKind::GUARD);
            self.finish_node();
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p themelios-syntax --lib`
Expected: the `statements` tests pass (10) and every earlier test still
passes.

- [ ] **Step 5: Run the full gate, then commit**

Run the four gate commands. Expected: green; repair or argue any lint
that fires (`too_many_lines` on `element` is allowed at the function
with the argument that the element's shapes are one decision by first
token; `match_same_arms` — merge the arms).

```bash
git add crates/themelios-syntax
git commit -m "Add rules: literals, atoms, comparisons, disjunctions, bodies, conditional literals, and aggregates, with their recovery"
```

---

### Task 10: Theory atoms, theory mode, and `#theory` definitions

**Files:**
- Create: `crates/themelios-syntax/src/parse/theory.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs` (`mod theory;`),
  `src/parse/terms.rs` (the theory family's frames and arms in the one
  loop), `src/parse/statements.rs` (the `&` arm of `element`, the
  `#theory` arm of `statement`, `Element::TheoryAtom`,
  `condition_or_empty` made `pub(super)`)

**Derives:** syntax.md §4.2 (the parser owns the modes; bounded re-lexing
at region ends), §6.2 (theory terms in the frame loop, one flat sequence
per frame), §6.3 (the theory regions; the guard-end rule, greedy; the
`#theory` operator positions), §6.4 (D1 not adopted), §6.7 (theory atoms
and elements; `#theory` definitions), §7.1 (`GrammarWord`,
`SyntaxClass::{TheoryTerm, TheoryOperator}`), Appendix A; grammar §4.7,
§5.8, §5.9 (`theory-definition`), §7, §11's theory seeds.

**Interfaces:**
- Consumes: the machine's `set_mode`/`mode`, `Parser::{arguments,
  condition_or_empty, statement_end, expect, ...}`.
- Produces: `Parser::theory_atom(Checkpoint)`,
  `Parser::theory_definition(Checkpoint)`, `Parser::theory_opterm() ->
  bool` (the frame loop's theory entry), `TermContext::Theory`,
  `Element::TheoryAtom`; the modes recorded per token as syntax.md §6.3
  states them, which Task 15's `lex_mode_of` reconstructs.

- [ ] **Step 1: Write the failing tests**

Create `src/parse/theory.rs` holding only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use crate::diagnostic::{Expected, GrammarWord, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::sexpr;

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    fn shape(text: &str) -> String {
        let source = admitted(text);
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(PROGRAM ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn kinds(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse(&source, Dialect::Clingo).diagnostics().iter().map(|d| d.kind().clone()).collect()
    }

    fn member(text: &str) -> bool {
        kinds(text).is_empty()
    }

    #[test]
    fn theory_atoms_take_a_name_arguments_elements_and_one_guard() {
        assert_eq!(shape("&a."), "(RULE (THEORY_ATOM & a) .)");
        assert_eq!(shape("&a {}."), "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { })) .)");
        assert_eq!(
            shape("&a(1, X) { }."),
            "(RULE (THEORY_ATOM & a (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1) , (VARIABLE_TERM X)) )) (THEORY_ELEMENTS { })) .)"
        );
        assert_eq!(
            shape("&a { x not y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x) not (CONSTANT_TERM y))) })) .)"
        );
        assert_eq!(
            shape(":- &sum { x } >= 5, not p."),
            "(RULE :- (BODY (THEORY_ATOM & sum (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }) (THEORY_GUARD >= (THEORY_OPTERM (CONSTANT_TERM 5)))) , (LITERAL not (ATOM p))) .)"
        );
        assert_eq!(
            shape("&dom { 0..B } = v."),
            "(RULE (THEORY_ATOM & dom (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM 0) .. (VARIABLE_TERM B))) }) (THEORY_GUARD = (THEORY_OPTERM (CONSTANT_TERM v)))) .)"
        );
        assert!(member("&a { x :-: y ; :- z }."));
        assert_eq!(
            shape("&a { x :-: y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x) :-: (CONSTANT_TERM y))) })) .)"
        );
    }

    #[test]
    fn elements_take_conditions_in_normal_mode_and_the_semicolon_at_element_depth_ends_them() {
        assert_eq!(
            shape("&a { t : p((x;y)), q ; u }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM t)) : (CONDITION (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (POOL ( (TUPLE (CONSTANT_TERM x)) ; (TUPLE (CONSTANT_TERM y)) ))) )))) , (LITERAL (ATOM q)))) ; (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM u))) })) .)"
        );
        assert_eq!(
            shape("&a { : p }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT : (CONDITION (LITERAL (ATOM p)))) })) .)"
        );
        assert_eq!(
            shape("&a { x : }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x)) : (CONDITION)) })) .)"
        );
        assert!(member("&a { x, y : p ; z }."));
        assert_eq!(
            shape("&a { x, y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x)) , (THEORY_OPTERM (CONSTANT_TERM y))) })) .)"
        );
    }

    #[test]
    fn theory_terms_take_the_bracketed_and_function_shapes() {
        assert_eq!(
            shape("&a { {x, y}, [1, 2], (b,), f(g(1)), (), (c) }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (THEORY_SET { (THEORY_OPTERM (CONSTANT_TERM x)) , (THEORY_OPTERM (CONSTANT_TERM y)) })) , (THEORY_OPTERM (THEORY_LIST [ (THEORY_OPTERM (CONSTANT_TERM 1)) , (THEORY_OPTERM (CONSTANT_TERM 2)) ])) , (THEORY_OPTERM (THEORY_TUPLE ( (THEORY_OPTERM (CONSTANT_TERM b)) , ))) , (THEORY_OPTERM (THEORY_FUNCTION f ( (THEORY_OPTERM (THEORY_FUNCTION g ( (THEORY_OPTERM (CONSTANT_TERM 1)) ))) ))) , (THEORY_OPTERM (THEORY_TUPLE ( ))) , (THEORY_OPTERM (THEORY_TUPLE ( (THEORY_OPTERM (CONSTANT_TERM c)) )))) })) .)"
        );
        assert!(member("&a { - - x, not - y, #inf, \"s\", X }."));
    }

    #[test]
    fn the_guard_is_greedy_and_the_first_token_that_does_not_continue_it_is_read_in_normal_mode() {
        assert_eq!(
            shape("&a { x } > - not - , p."),
            "(RULE (DISJUNCTION (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }) (THEORY_GUARD > (THEORY_OPTERM - not -))) , (LITERAL (ATOM p))) .)"
        );
        assert!(!member("&a { x } > - not - , p."));
        assert!(member(":- &a { x } <= 3, &b { y }, p."));
    }

    #[test]
    fn theory_mode_diagnoses_the_anonymous_variable_and_the_nested_colon() {
        assert!(kinds("&a { _ }.").iter().any(|kind| matches!(kind, SyntaxErrorKind::AnonymousInTheoryExpression)));
        assert!(!member("&a { {x : y} }."));
        assert!(!member("&a { x ; }."));
    }

    #[test]
    fn negation_stands_inside_the_theory_atom_in_body_position_only() {
        assert_eq!(
            shape(":- not &a { x }."),
            "(RULE :- (BODY (THEORY_ATOM not & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }))) .)"
        );
        assert!(!member("not &a { x }."));
    }

    #[test]
    fn theory_definitions_take_term_and_atom_definitions_with_operator_positions_in_theory_mode() {
        let text = "#theory cp { var_term { }; sum_term { - : 3, unary; ** : 2, binary, right; + : 0, binary, left }; &sum/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any; &show/0 : sum_term, directive }.";
        assert!(member(text));
        let shape = shape(text);
        assert!(shape.contains("(OP_DEFINITION - : 3 , unary)"));
        assert!(shape.contains("(OP_DEFINITION ** : 2 , binary , right)"));
        assert!(shape.contains("(ATOM_DEFINITION & sum / 0 : sum_term , { <= , = , != , < , > , >= } , sum_term , any)"));
        assert!(shape.contains("(ATOM_DEFINITION & show / 0 : sum_term , directive)"));
        assert!(shape.starts_with("(THEORY_DEFINITION #theory cp { (TERM_DEFINITION var_term { })"));
        assert!(!member("#theory t { x { + : 1, ternary } }."));
        assert!(kinds("#theory t { x { + : 1, ternary } }.").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Word(GrammarWord::Unary)) && expected.contains(&Expected::Word(GrammarWord::Binary))
        )));
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Add `mod theory;` under `mod statements;` in `src/parse/mod.rs`. Run:
`cargo test -p themelios-syntax --lib theory`
Expected: the shapes fail (`&` is still wrapped as unexpected).

- [ ] **Step 3: Give the frame loop its theory family**

In `src/parse/terms.rs`:

Add the variant to `TermContext` (its `restriction()` answers `None`):

```rust
    /// Grammar §5.8's theory terms: no restriction, no precedence — one
    /// flat sequence per frame.
    Theory,
```

Add the four shapes to `Shape`, and their nodes and closers:

```rust
    /// `{ … }` — a theory set (grammar §5.8).
    TheorySet,
    /// `[ … ]` — a theory list.
    TheoryList,
    /// `( … )` — a theory tuple: `()`, `(a)`, `(a,)`, `(a, b)`.
    TheoryTuple,
    /// `IDENT ( … )` — a theory function's arguments; the frame's node
    /// opens before the name.
    TheoryFunction,
```

```rust
    fn node(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool => Some(SyntaxKind::POOL),
            Shape::Arguments => Some(SyntaxKind::ARGUMENTS),
            Shape::Abs => Some(SyntaxKind::ABS_TERM),
            Shape::TheorySet => Some(SyntaxKind::THEORY_SET),
            Shape::TheoryList => Some(SyntaxKind::THEORY_LIST),
            Shape::TheoryTuple => Some(SyntaxKind::THEORY_TUPLE),
            Shape::TheoryFunction => Some(SyntaxKind::THEORY_FUNCTION),
        }
    }

    fn closer(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool | Shape::Arguments | Shape::TheoryTuple | Shape::TheoryFunction => Some(SyntaxKind::R_PAREN),
            Shape::Abs => Some(SyntaxKind::PIPE),
            Shape::TheorySet => Some(SyntaxKind::R_BRACE),
            Shape::TheoryList => Some(SyntaxKind::R_BRACKET),
        }
    }
```

Add to `Frame` the field (initialized `false` in `new`):

```rust
    /// A `THEORY_OPTERM` is open in this frame — the theory family's
    /// operand slot: an opterm per element, per set or list member, per
    /// tuple member, per function argument.
    opterm_open: bool,
```

In `open_frame`, the frame's node for the theory shapes: `Shape::TheorySet
=> SyntaxKind::THEORY_SET`, `Shape::TheoryList => SyntaxKind::THEORY_LIST`,
`Shape::TheoryTuple => SyntaxKind::THEORY_TUPLE`, `Shape::TheoryFunction
=> SyntaxKind::THEORY_FUNCTION`; and after the opener:

```rust
        let next = match shape {
            Shape::Pool | Shape::Arguments => self.tuple_start(&mut frame),
            Shape::TheorySet | Shape::TheoryList | Shape::TheoryTuple | Shape::TheoryFunction => {
                if Some(self.peek()) == frame.closer() { Next::Operator } else { Next::Operand }
            }
            Shape::Abs | Shape::Base => Next::Operand,
        };
```

In `close_frame` and `unwind`, close an open opterm before the frame's
node — insert after the tuple's closing in both:

```rust
        if frame.opterm_open {
            self.finish_node();
        }
```

In `run`, dispatch by family, and close the base opterm when the loop
ends:

```rust
    fn run(&mut self, context: TermContext, mut frames: Vec<Frame>, mut next: Next, stop_at_base: bool) {
        let mut last_operator = None;
        while next != Next::Done {
            next = match (next, context) {
                (Next::Operand, TermContext::Theory) => self.theory_operand(&mut frames),
                (Next::Operator, TermContext::Theory) => self.theory_after_term(&mut frames),
                (Next::Operand, _) => self.operand(&mut frames, context, last_operator),
                (Next::Operator, _) => self.after_operand(&mut frames, context, &mut last_operator),
                (Next::Done, _) => Next::Done,
            };
            if stop_at_base && frames.len() == 1 {
                return;
            }
        }
        if let Some(base) = frames.first_mut() {
            if base.opterm_open {
                self.finish_node();
                base.opterm_open = false;
            }
        }
    }
```

Add the theory entry and the two theory phases:

```rust
    /// Whether the next significant token begins a theory term (grammar §5.8).
    fn theory_term_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::VARIABLE
                | SyntaxKind::SPLICE
                | SyntaxKind::L_BRACE
                | SyntaxKind::L_BRACKET
                | SyntaxKind::L_PAREN
        )
    }

    /// Whether the next significant token is a theory operator — a
    /// `THEORY_OP` run or `not` (grammar §5.8), under theory mode.
    pub(super) fn theory_operator_here(&mut self) -> bool {
        matches!(self.peek(), SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT)
    }

    /// One theory-opterm at the next significant token, under theory
    /// mode: leading operators, a term, and operator runs and terms
    /// after it, one flat `THEORY_OPTERM` node per frame; false, and
    /// nothing consumed, when neither an operator nor a term begins
    /// there. Ends at the first token that continues nothing — greedy,
    /// which is the guard-end rule when the caller is the guard.
    pub(super) fn theory_opterm(&mut self) -> bool {
        if !self.theory_term_begins() && !self.theory_operator_here() {
            return false;
        }
        self.run(TermContext::Theory, vec![Frame::new(Shape::Base, None, NO_OPENER)], Next::Operand, false);
        true
    }

    fn open_opterm(&mut self, frame: &mut Frame) {
        if !frame.opterm_open {
            self.start_node(SyntaxKind::THEORY_OPTERM);
            frame.opterm_open = true;
        }
    }

    /// The tokens that end a theory opterm or a theory list where a term
    /// or a closer was expected. At the base a colon ends the opterm (the
    /// element's condition follows); inside a frame it is an intruder.
    fn theory_synchronizes(kind: SyntaxKind, at_base: bool) -> bool {
        matches!(
            kind,
            SyntaxKind::EOF
                | SyntaxKind::DOT
                | SyntaxKind::COMMA
                | SyntaxKind::SEMICOLON
                | SyntaxKind::R_PAREN
                | SyntaxKind::R_BRACKET
                | SyntaxKind::R_BRACE
                | SyntaxKind::NECK
                | SyntaxKind::WEAK_NECK
        ) || (at_base && kind == SyntaxKind::COLON)
    }

    /// Expecting a theory term: an operator run first, then the term —
    /// a bracketed shape, a function, a constant, a variable, a splice.
    fn theory_operand(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        let at_base = top == 0;
        match self.peek() {
            SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT => {
                self.open_opterm(&mut frames[top]);
                self.bump();
                Next::Operand
            }
            SyntaxKind::L_BRACE => self.theory_bracket(frames, Shape::TheorySet),
            SyntaxKind::L_BRACKET => self.theory_bracket(frames, Shape::TheoryList),
            SyntaxKind::L_PAREN => self.theory_bracket(frames, Shape::TheoryTuple),
            SyntaxKind::IDENT if self.lookahead(1) == SyntaxKind::L_PAREN => {
                self.open_opterm(&mut frames[top]);
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.bump();
                self.open_frame(frames, Shape::TheoryFunction, Some(checkpoint), None)
            }
            SyntaxKind::IDENT | SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::KW_INF | SyntaxKind::KW_SUP => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::CONSTANT_TERM)
            }
            SyntaxKind::VARIABLE => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::SPLICE => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::SPLICE_TERM)
            }
            kind => {
                if Self::theory_synchronizes(kind, at_base) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    Next::Operator
                } else {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    Next::Operand
                }
            }
        }
    }

    /// A bracketed theory term: the opterm opened, the frame opened at
    /// the operand's checkpoint.
    fn theory_bracket(&mut self, frames: &mut Vec<Frame>, shape: Shape) -> Next {
        let top = frames.len() - 1;
        self.open_opterm(&mut frames[top]);
        let checkpoint = self.begin_operand(&mut frames[top]);
        self.open_frame(frames, shape, Some(checkpoint), None)
    }

    /// After a theory term: an operator run continues the opterm; a
    /// comma ends the opterm and begins the next in a bracketed frame,
    /// or ends the opterm at the base; the closer closes the frame; at
    /// the base anything else ends the opterm; inside a frame a
    /// synchronizing token closes it as unclosed and anything else is an
    /// intruder, wrapped.
    fn theory_after_term(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        let kind = self.peek();
        if matches!(kind, SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT) {
            self.bump();
            return Next::Operand;
        }
        let shape = frames[top].shape;
        if shape == Shape::Base {
            return Next::Done;
        }
        let closer = frames[top].closer();
        if kind == SyntaxKind::COMMA {
            self.close_opterm(&mut frames[top]);
            self.bump();
            if shape == Shape::TheoryTuple && Some(self.peek()) == closer {
                return Next::Operator;
            }
            return Next::Operand;
        }
        if Some(kind) == closer {
            self.close_opterm(&mut frames[top]);
            self.bump();
            return self.close_frame(frames);
        }
        if Self::theory_synchronizes(kind, false) {
            self.close_opterm(&mut frames[top]);
            return self.unclosed(frames);
        }
        let mut wanted = vec![Expected::Token(SyntaxKind::COMMA), Expected::Class(SyntaxClass::TheoryOperator)];
        if let Some(closer) = closer {
            wanted.push(Expected::Token(closer));
        }
        self.wrap_unexpected(wanted.into_iter().collect(), None);
        Next::Operator
    }

    fn close_opterm(&mut self, frame: &mut Frame) {
        if frame.opterm_open {
            self.finish_node();
            frame.opterm_open = false;
        }
    }
```

- [ ] **Step 4: Write the theory atoms and definitions**

In `src/parse/statements.rs`: add `TheoryAtom` to `Element`
(`/// A theory atom.`); in `head` return also on `Element::TheoryAtom`
(`Element::Aggregate | Element::TheoryAtom => return false`); make
`condition_or_empty` `pub(super)`; replace the interim `&` arm of
`element` with:

```rust
            SyntaxKind::AMPERSAND => {
                if negation > 0 && position != Position::Body {
                    // Only a body element signs a theory atom (grammar §5.6).
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                }
                self.theory_atom(start);
                Element::TheoryAtom
            }
```

and the `#theory` arm of `statement`:

```rust
            SyntaxKind::KW_THEORY => self.theory_definition(checkpoint),
```

Prepend to `src/parse/theory.rs`, above the test module:

```rust
//! Theory atoms and `#theory` definitions (docs/design/syntax.md §6.3):
//! the theory regions and their modes — the parser tells the source the
//! mode for each token: theory mode from the token after the `{` that
//! opens the elements through the elements and the guard, normal mode
//! for each element's condition and for the `;` or `}` that ends it, and
//! at the operator positions of a definition — the greedy guard end, and
//! the definitions item by item.

use rowan::Checkpoint;

use crate::diagnostic::{Expected, ExpectedSet, GrammarWord, SyntaxClass};
use crate::token::{LexMode, TokenSource};
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

fn expected(items: &[Expected]) -> ExpectedSet {
    items.iter().copied().collect()
}

impl<'s, S: TokenSource> Parser<'s, S> {
    /// Grammar §5.8's `theory-atom`, its node opened at `start` (around
    /// the negation run in body position): the name and its arguments in
    /// normal mode, then the elements and the guard.
    pub(super) fn theory_atom(&mut self, start: Checkpoint) {
        self.start_node_at(start, SyntaxKind::THEORY_ATOM);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.arguments(TermContext::Term);
        }
        if !self.depth_refused() && self.peek() == SyntaxKind::L_BRACE {
            self.theory_elements();
            if !self.depth_refused() {
                self.theory_guard();
            }
        }
        self.finish_node();
    }

    /// `"{" [ theory-elements ] "}"`: the brace taken in normal mode,
    /// theory mode from the token after it; each element's condition
    /// returns the mode to normal, and the `;` or `}` after a condition
    /// is taken in normal mode; theory mode resumes at the token after
    /// that `;`. An unclosed brace recovers at the dot.
    fn theory_elements(&mut self) {
        self.start_node(SyntaxKind::THEORY_ELEMENTS);
        self.bump();
        self.set_mode(LexMode::Theory);
        let mut after_separator = false;
        loop {
            if self.depth_refused() {
                break;
            }
            match self.peek() {
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {
                    if after_separator {
                        // `;` promises an element (grammar §5.8).
                        self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    }
                    if self.peek() != SyntaxKind::R_BRACE {
                        self.expected_token(SyntaxKind::R_BRACE);
                    }
                    break;
                }
                _ => {}
            }
            if !self.theory_element() {
                if self.peek() != SyntaxKind::SEMICOLON {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    continue;
                }
                self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
            }
            if self.depth_refused() {
                break;
            }
            after_separator = false;
            match self.peek() {
                SyntaxKind::SEMICOLON => {
                    self.bump();
                    self.set_mode(LexMode::Theory);
                    after_separator = true;
                }
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                _ => self.wrap_unexpected(
                    expected(&[Expected::Token(SyntaxKind::SEMICOLON), Expected::Token(SyntaxKind::R_BRACE)]),
                    None,
                ),
            }
        }
        self.eat(SyntaxKind::R_BRACE);
        self.finish_node();
    }

    /// Grammar §5.8's `theory-element`: opterms between commas, an
    /// optional condition after a colon at element depth — the colon a
    /// length-one structural run under theory mode, the condition in
    /// normal mode — or a colon and condition alone.
    fn theory_element(&mut self) -> bool {
        if self.peek() != SyntaxKind::COLON && !self.theory_opterm_begins() {
            return false;
        }
        self.start_node(SyntaxKind::THEORY_ELEMENT);
        if self.peek() != SyntaxKind::COLON {
            loop {
                if !self.theory_opterm() {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    break;
                }
                if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                    break;
                }
            }
        }
        if !self.depth_refused() && self.peek() == SyntaxKind::COLON {
            self.bump();
            self.set_mode(LexMode::Normal);
            self.condition_or_empty();
        }
        self.finish_node();
        true
    }

    fn theory_opterm_begins(&mut self) -> bool {
        self.theory_operator_here()
            || matches!(
                self.peek(),
                SyntaxKind::IDENT
                    | SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::KW_INF
                    | SyntaxKind::KW_SUP
                    | SyntaxKind::VARIABLE
                    | SyntaxKind::SPLICE
                    | SyntaxKind::L_BRACE
                    | SyntaxKind::L_BRACKET
                    | SyntaxKind::L_PAREN
            )
    }

    /// The guard after the elements, greedy (docs/design/syntax.md §6.3):
    /// the next token taken under theory mode; not a theory operator —
    /// no guard, and that token re-lexed under normal mode; a theory
    /// operator — the guard opens and its opterm extends while the next
    /// token continues it, and the first token that does not is re-lexed
    /// under normal mode.
    fn theory_guard(&mut self) {
        self.set_mode(LexMode::Theory);
        if !self.theory_operator_here() {
            self.set_mode(LexMode::Normal);
            return;
        }
        self.start_node(SyntaxKind::THEORY_GUARD);
        self.bump();
        if !self.theory_opterm() {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
        }
        self.finish_node();
        self.set_mode(LexMode::Normal);
    }

    /// Grammar §5.9's `theory-definition`, opened at `checkpoint`: items
    /// between semicolons, term definitions and atom definitions
    /// interleaved in any order at the pin.
    pub(super) fn theory_definition(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::THEORY_DEFINITION);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    SyntaxKind::IDENT => self.term_definition(),
                    SyntaxKind::AMPERSAND => self.atom_definition(),
                    _ => {
                        self.skip_into_error(
                            expected(&[Expected::Token(SyntaxKind::IDENT), Expected::Token(SyntaxKind::AMPERSAND)]),
                            None,
                            &[SyntaxKind::SEMICOLON, SyntaxKind::R_BRACE, SyntaxKind::DOT],
                        );
                    }
                }
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                    _ => self.expected_token(SyntaxKind::SEMICOLON),
                }
            }
            if !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.statement_end();
        self.finish_node();
    }

    /// `IDENTIFIER "{" [ op-definitions ] "}"`.
    fn term_definition(&mut self) {
        self.start_node(SyntaxKind::TERM_DEFINITION);
        self.bump();
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    _ => {}
                }
                self.op_definition();
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                    _ => self.skip_into_error(
                        expected(&[Expected::Token(SyntaxKind::SEMICOLON), Expected::Token(SyntaxKind::R_BRACE)]),
                        None,
                        &[SyntaxKind::SEMICOLON, SyntaxKind::R_BRACE, SyntaxKind::DOT],
                    ),
                }
            }
            if !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.finish_node();
    }

    /// `theory-op ":" NUMBER "," ( "unary" | "binary" "," ( "left" | "right" ) )`
    /// — the operator position under theory mode, the rest normal.
    fn op_definition(&mut self) {
        self.start_node(SyntaxKind::OP_DEFINITION);
        self.operator_position();
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::NUMBER);
        self.expect(SyntaxKind::COMMA);
        if self.eat_word(GrammarWord::Unary) {
        } else if self.eat_word(GrammarWord::Binary) {
            self.expect(SyntaxKind::COMMA);
            if !(self.eat_word(GrammarWord::Left) || self.eat_word(GrammarWord::Right)) {
                self.unexpected(
                    expected(&[Expected::Word(GrammarWord::Left), Expected::Word(GrammarWord::Right)]),
                    None,
                );
            }
        } else {
            self.unexpected(
                expected(&[Expected::Word(GrammarWord::Unary), Expected::Word(GrammarWord::Binary)]),
                None,
            );
        }
        self.finish_node();
    }

    /// `"&" IDENTIFIER "/" NUMBER ":" IDENTIFIER "," [ "{" [ theory-op { "," theory-op } ] "}" "," IDENTIFIER "," ] ( "head" | "body" | "any" | "directive" )`.
    fn atom_definition(&mut self) {
        self.start_node(SyntaxKind::ATOM_DEFINITION);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::SLASH);
        self.expect(SyntaxKind::NUMBER);
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COMMA);
        if self.eat(SyntaxKind::L_BRACE) {
            self.set_mode(LexMode::Theory);
            if self.theory_operator_here() {
                loop {
                    self.operator_position();
                    if !self.eat(SyntaxKind::COMMA) {
                        break;
                    }
                    self.set_mode(LexMode::Theory);
                }
            }
            self.set_mode(LexMode::Normal);
            self.expect(SyntaxKind::R_BRACE);
            self.expect(SyntaxKind::COMMA);
            self.expect(SyntaxKind::IDENT);
            self.expect(SyntaxKind::COMMA);
        }
        let words = [GrammarWord::Head, GrammarWord::Body, GrammarWord::Any, GrammarWord::Directive];
        if !words.iter().any(|word| self.eat_word(*word)) {
            self.unexpected(words.iter().map(|word| Expected::Word(*word)).collect(), None);
        }
        self.finish_node();
    }

    /// One operator position of a definition: the token taken under
    /// theory mode, the mode returned to normal after it.
    fn operator_position(&mut self) {
        self.set_mode(LexMode::Theory);
        if self.theory_operator_here() {
            self.bump();
        } else {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryOperator)]), None);
        }
        self.set_mode(LexMode::Normal);
    }

    /// An identifier the grammar wants by spelling (grammar §5.9):
    /// consumed when next.
    fn eat_word(&mut self, word: GrammarWord) -> bool {
        if self.peek() == SyntaxKind::IDENT && self.peek_text() == word.spelling() {
            self.bump();
            true
        } else {
            false
        }
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p themelios-syntax --lib`
Expected: the `theory` tests pass (7) and every earlier test still
passes.

- [ ] **Step 6: Run the full gate, then commit**

Run the four gate commands. Expected: green; repair or argue any lint
that fires (`if self.eat_word(GrammarWord::Unary) {}` may draw
`clippy::if_same_then_else` or an empty-branch lint — rewrite as a
`match` on which word was eaten if it does).

```bash
git add crates/themelios-syntax
git commit -m "Add theory atoms and #theory definitions: theory mode owned by the parser, the greedy guard end, and theory terms on the frame loop"
```

---

### Task 11: Weak constraints, optimize statements, the directives, the annotations after the dot, and the script region

**Files:**
- Create: `crates/themelios-syntax/src/parse/directives.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs` (`mod directives;`),
  `src/parse/statements.rs` (the dispatch arms)

**Derives:** syntax.md §4.4 (the script region's tokens), §6.3 (the
show-signature reading — five tokens of lookahead; the script region's
modes; the annotations after the dot, arity and spelling per family),
§6.7 (directives; `#script`; annotations after the dot), §7.1
(`Hint::HeuristicNeedsAnnotation`, `GrammarWord::{Default, Override}`,
`SyntaxClass::{Signature, Annotation}`), Appendix A; grammar §5.7
(`weak-constraint`, `optimize-statement`), §5.9, §5.11 (the four
annotation families), §4.8.

**Interfaces:**
- Consumes: the machine, `Parser::{term, body, condition_or_empty,
  statement_end, eat_word (moved here from `theory.rs`, made
  `pub(super)`), literal_begins}`, `TermContext::ConstantTerm`.
- Produces: `Parser::{weak_constraint, optimize, show, project,
  defined, edge, heuristic, external, constant, script, include,
  program_statement}` and `Parser::annotation(Annotation)`; every arm
  of `statement` now realized — no statement start recovers by default.

- [ ] **Step 1: Write the failing tests**

Create `src/parse/directives.rs` holding only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use crate::diagnostic::{Expected, GrammarWord, Hint, RestrictedForm, Restriction, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::sexpr;

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    fn shape(text: &str) -> String {
        let source = admitted(text);
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(PROGRAM ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn kinds(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse(&source, Dialect::Clingo).diagnostics().iter().map(|d| d.kind().clone()).collect()
    }

    fn member(text: &str) -> bool {
        kinds(text).is_empty()
    }

    #[test]
    fn weak_constraints_carry_their_annotation_after_the_dot() {
        assert_eq!(
            shape(":~ p(X). [1@2, X]"),
            "(WEAK_CONSTRAINT :~ (BODY (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) . (ANNOTATION [ (CONSTANT_TERM 1) @ (CONSTANT_TERM 2) , (VARIABLE_TERM X) ]))"
        );
        assert_eq!(shape(":~ . [1]"), "(WEAK_CONSTRAINT :~ . (ANNOTATION [ (CONSTANT_TERM 1) ]))");
        assert!(member(":~ p, q. [W@P, a, b]"));
        assert!(!member(":~ p."), "the annotation is required");
        assert!(kinds(":~ p.").iter().any(|kind| matches!(kind, SyntaxErrorKind::UnexpectedEndOfInput { .. })));
    }

    #[test]
    fn optimize_statements_take_weighted_elements() {
        assert_eq!(
            shape("#minimize { W@P,T : p(T,W) ; 1 }."),
            "(OPTIMIZE_STATEMENT #minimize { (OPTIMIZE_ELEMENT (VARIABLE_TERM W) @ (VARIABLE_TERM P) , (VARIABLE_TERM T) : (CONDITION (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM T) , (VARIABLE_TERM W)) )))))) ; (OPTIMIZE_ELEMENT (CONSTANT_TERM 1)) } .)"
        );
        assert!(member("#maximise { }."));
        assert!(member("#minimize { 1 : }."));
    }

    #[test]
    fn show_takes_its_four_forms_by_the_signature_reading() {
        assert_eq!(shape("#show."), "(SHOW_STATEMENT #show .)");
        assert_eq!(shape("#show p/2."), "(SHOW_STATEMENT #show (SIGNATURE p / 2) .)");
        assert_eq!(shape("#show -p/2."), "(SHOW_STATEMENT #show (SIGNATURE - p / 2) .)");
        assert_eq!(
            shape("#show p/2 : q."),
            "(SHOW_STATEMENT #show (BINARY_TERM (CONSTANT_TERM p) / (CONSTANT_TERM 2)) : (BODY (LITERAL (ATOM q))) .)"
        );
        assert_eq!(
            shape("#show (p/2)."),
            "(SHOW_STATEMENT #show (POOL ( (TUPLE (BINARY_TERM (CONSTANT_TERM p) / (CONSTANT_TERM 2))) )) .)"
        );
        assert_eq!(shape("#show f(X) : p(X)."), "(SHOW_STATEMENT #show (FUNCTION_TERM f (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)");
        assert!(!member("#show $x/1."), "grammar §11: both readings are errors");
    }

    #[test]
    fn project_defined_edge_heuristic_and_external_take_their_shapes() {
        assert_eq!(shape("#project p/1."), "(PROJECT_STATEMENT #project (SIGNATURE p / 1) .)");
        assert_eq!(shape("#project p(X) : q(X)."), "(PROJECT_STATEMENT #project (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)");
        assert_eq!(shape("#project p : ."), "(PROJECT_STATEMENT #project (ATOM p) : (BODY) .)");
        assert_eq!(shape("#defined p/2."), "(DEFINED_STATEMENT #defined (SIGNATURE p / 2) .)");
        assert_eq!(
            shape("#edge (a,b; c,d) : e."),
            "(EDGE_STATEMENT #edge ( (EDGE (CONSTANT_TERM a) , (CONSTANT_TERM b)) ; (EDGE (CONSTANT_TERM c) , (CONSTANT_TERM d)) ) : (BODY (LITERAL (ATOM e))) .)"
        );
        assert_eq!(
            shape("#heuristic a. [1@2, sign]"),
            "(HEURISTIC_STATEMENT #heuristic (ATOM a) . (ANNOTATION [ (CONSTANT_TERM 1) @ (CONSTANT_TERM 2) , (CONSTANT_TERM sign) ]))"
        );
        assert!(member("#heuristic a : b. [1,sign]"));
        assert!(!member("#heuristic a."));
        assert!(kinds("#heuristic a.").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedEndOfInput { hint: Some(Hint::HeuristicNeedsAnnotation), .. }
        )));
        assert_eq!(shape("#external p(X) : q(X). [true]"), "(EXTERNAL_STATEMENT #external (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) . (ANNOTATION [ (CONSTANT_TERM true) ]))");
        assert!(member("#external p."));
        assert!(!member("#external p. [a, b]"), "docs/design/syntax.md §6.3's non-member");
    }

    #[test]
    fn const_takes_the_constant_restriction_and_the_policy_word() {
        assert_eq!(shape("#const n = 1."), "(CONST_STATEMENT #const n = (CONSTANT_TERM 1) .)");
        assert_eq!(
            shape("#const n = 1. [override]"),
            "(CONST_STATEMENT #const n = (CONSTANT_TERM 1) . (ANNOTATION [ override ]))"
        );
        assert!(member("#const n = f(1,2) + |3|. [default]"));
        assert!(member("#const default = 1."));
        assert!(!member("#const n = 1. [foo]"), "docs/design/syntax.md §6.3's non-member");
        assert!(kinds("#const n = 1. [foo]").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Word(GrammarWord::Default)) && expected.contains(&Expected::Word(GrammarWord::Override))
        )));
        assert!(kinds("#const x = |1;2|.").contains(&SyntaxErrorKind::FormNotAllowedHere {
            form: RestrictedForm::PooledAbsoluteValue,
            context: Restriction::ConstantTerm,
        }));
        assert!(kinds("#const x = 1..3.").contains(&SyntaxErrorKind::FormNotAllowedHere {
            form: RestrictedForm::Interval,
            context: Restriction::ConstantTerm,
        }));
        assert!(kinds("#const x = X.").contains(&SyntaxErrorKind::FormNotAllowedHere {
            form: RestrictedForm::Variable,
            context: Restriction::ConstantTerm,
        }));
        assert!(member("#const x = @f(1)."), "the constant term admits the `@`-call");
    }

    #[test]
    fn the_script_region_is_one_opaque_token_between_its_header_and_end() {
        assert_eq!(
            shape("#script (python)\ndef f(): return 1\n#end."),
            "(SCRIPT_STATEMENT #script ( python ) \ndef f(): return 1\n #end .)"
        );
        assert_eq!(shape("#script (lua)#end."), "(SCRIPT_STATEMENT #script ( lua ) #end .)");
        assert!(member("#script (python) % not a comment; #end is the terminator\n#end.\np."));
        let unterminated = kinds("#script (lua) x = 1");
        assert!(unterminated.iter().any(|kind| matches!(kind, SyntaxErrorKind::UnterminatedScript)));
        let source = admitted("#script (lua) x = 1");
        assert!(parse(&source, Dialect::Clingo).is_incomplete());
    }

    #[test]
    fn include_and_program_take_their_forms() {
        assert_eq!(shape("#include \"a.lp\"."), "(INCLUDE_STATEMENT #include \"a.lp\" .)");
        assert_eq!(shape("#include < lib > ."), "(INCLUDE_STATEMENT #include < lib > .)");
        assert_eq!(shape("#program base."), "(PROGRAM_STATEMENT #program base .)");
        assert_eq!(shape("#program step(t, k)."), "(PROGRAM_STATEMENT #program step (PARAMETERS ( t , k )) .)");
        assert!(member("#program p()."));
    }

    #[test]
    fn a_bracket_after_any_other_statements_dot_is_the_next_statements_unexpected_token() {
        assert!(!member("p. [1]"));
        assert_eq!(shape("p. [1]"), "(RULE (LITERAL (ATOM p)) .) (ERROR [ 1 ])");
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Add `mod directives;` under `mod theory;` in `src/parse/mod.rs`. Run:
`cargo test -p themelios-syntax --lib directives`
Expected: the shapes fail (the arms still recover).

- [ ] **Step 3: Write the families and wire the dispatch**

In `src/parse/theory.rs`, remove `eat_word` (it moves below, shared).
In `src/parse/statements.rs`, replace the recovering arms of `statement`
with:

```rust
            SyntaxKind::WEAK_NECK => self.weak_constraint(checkpoint),
            SyntaxKind::KW_MINIMIZE | SyntaxKind::KW_MAXIMIZE => self.optimize(checkpoint),
            SyntaxKind::KW_SHOW => self.show(checkpoint),
            SyntaxKind::KW_PROJECT => self.project(checkpoint),
            SyntaxKind::KW_DEFINED => self.defined(checkpoint),
            SyntaxKind::KW_EDGE => self.edge(checkpoint),
            SyntaxKind::KW_HEURISTIC => self.heuristic(checkpoint),
            SyntaxKind::KW_EXTERNAL => self.external(checkpoint),
            SyntaxKind::KW_CONST => self.constant(checkpoint),
            SyntaxKind::KW_SCRIPT => self.script(checkpoint),
            SyntaxKind::KW_INCLUDE => self.include(checkpoint),
            SyntaxKind::KW_PROGRAM => self.program_statement(checkpoint),
            SyntaxKind::KW_THEORY => self.theory_definition(checkpoint),
            _ => self.rule(checkpoint),
```

Prepend to `src/parse/directives.rs`, above the test module:

```rust
//! Weak constraints, optimize statements, the directives, the
//! annotations after the dot, and the script region
//! (docs/design/syntax.md §6.3, §6.7; grammar §5.7, §5.9, §5.11, §4.8).

use rowan::Checkpoint;

use crate::diagnostic::{Expected, ExpectedSet, GrammarWord, Hint, SyntaxClass};
use crate::token::{LexMode, TokenSource};
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

fn expected(items: &[Expected]) -> ExpectedSet {
    items.iter().copied().collect()
}

/// The four families whose dot is followed by a bracketed annotation
/// (grammar §5.11), each with its own interior (docs/design/syntax.md
/// §6.3): the node kind is one because the bracket shape is one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Annotation {
    /// `[ weight [@ priority] [, terms] ]`, required.
    WeakConstraint,
    /// `[ weight [@ priority] , modifier ]`, required.
    Heuristic,
    /// `[ term ]`, optional.
    External,
    /// `[ default | override ]`, optional.
    Const,
}

impl<'s, S: TokenSource> Parser<'s, S> {
    /// An identifier the grammar wants by spelling (grammar §5.9):
    /// consumed when next.
    pub(super) fn eat_word(&mut self, word: GrammarWord) -> bool {
        if self.peek() == SyntaxKind::IDENT && self.peek_text() == word.spelling() {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Grammar §5.7's `weak-constraint`: the neck, an optional body, the
    /// dot, and its required annotation.
    fn weak_constraint(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::WEAK_CONSTRAINT);
        self.bump();
        if self.peek() != SyntaxKind::DOT {
            self.body();
        }
        self.statement_end();
        self.annotation(Annotation::WeakConstraint, true);
        self.finish_node();
    }

    /// Grammar §5.7's `optimize-statement`: elements between `;`, each a
    /// weight, an optional priority, an optional tuple, and an optional
    /// condition.
    fn optimize(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::OPTIMIZE_STATEMENT);
        self.bump();
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    _ => {}
                }
                if self.term_begins() {
                    self.optimize_element();
                } else {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    continue;
                }
                if self.depth_refused() {
                    break;
                }
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                    _ => self.wrap_unexpected(
                        expected(&[Expected::Token(SyntaxKind::SEMICOLON), Expected::Token(SyntaxKind::R_BRACE)]),
                        None,
                    ),
                }
            }
            if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.statement_end();
        self.finish_node();
    }

    /// `term [ "@" term ] [ "," terms ] [ ":" [ condition ] ]`.
    fn optimize_element(&mut self) {
        self.start_node(SyntaxKind::OPTIMIZE_ELEMENT);
        self.weighted_terms();
        if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
            self.condition_or_empty();
        }
        self.finish_node();
    }

    /// `term [ "@" term ] [ "," terms ]` — the weight, the priority, the
    /// tuple, shared by the optimize element and the weak constraint's
    /// annotation.
    fn weighted_terms(&mut self) {
        if !self.term(TermContext::Term) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        if self.depth_refused() {
            return;
        }
        if self.eat(SyntaxKind::AT) && !self.term(TermContext::Term) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        while !self.depth_refused() && self.eat(SyntaxKind::COMMA) {
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                break;
            }
        }
    }

    /// Grammar §5.9's `show-statement`, by the show-signature reading:
    /// `[-] IDENT / NUMBER .` — trivia legal between — is the signature
    /// form; anything else after `#show` is the term form.
    fn show(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::SHOW_STATEMENT);
        self.bump();
        if self.peek() == SyntaxKind::DOT {
        } else if self.signature_follows() {
            self.signature();
        } else if self.term(TermContext::Term) {
            if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
                self.body();
            }
        } else {
            self.unexpected(
                expected(&[
                    Expected::Class(SyntaxClass::Signature),
                    Expected::Class(SyntaxClass::Term),
                    Expected::Token(SyntaxKind::DOT),
                ]),
                None,
            );
        }
        self.statement_end();
        self.finish_node();
    }

    /// The show-signature reading (grammar §5.9): five tokens of lookahead,
    /// the parser's maximum (docs/design/syntax.md §6.2).
    fn signature_follows(&mut self) -> bool {
        let mut at = 0;
        if self.lookahead(at) == SyntaxKind::MINUS {
            at += 1;
        }
        self.lookahead(at) == SyntaxKind::IDENT
            && self.lookahead(at + 1) == SyntaxKind::SLASH
            && self.lookahead(at + 2) == SyntaxKind::NUMBER
            && self.lookahead(at + 3) == SyntaxKind::DOT
    }

    /// Grammar §5.9's `signature`: `[-] IDENT / NUMBER`.
    fn signature(&mut self) {
        self.start_node(SyntaxKind::SIGNATURE);
        self.eat(SyntaxKind::MINUS);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::SLASH);
        self.expect(SyntaxKind::NUMBER);
        self.finish_node();
    }

    /// Grammar §5.2's `atom` where the grammar wants exactly one:
    /// `[-] IDENT [ arguments ]`.
    fn atom(&mut self) {
        if !matches!(self.peek(), SyntaxKind::MINUS | SyntaxKind::IDENT) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Atom)]), None);
            return;
        }
        let checkpoint = self.checkpoint();
        self.eat(SyntaxKind::MINUS);
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.arguments(TermContext::Term);
        }
        self.start_node_at(checkpoint, SyntaxKind::ATOM);
        self.finish_node();
    }

    /// Grammar §5.9's `conditional-dot` before the dot: nothing, or `:`
    /// with an empty body placed after it, or `:` and a body.
    fn conditional_dot(&mut self) {
        if self.depth_refused() || !self.eat(SyntaxKind::COLON) {
            return;
        }
        if self.peek() == SyntaxKind::DOT {
            self.empty_node(SyntaxKind::BODY);
        } else {
            self.body();
        }
    }

    /// Grammar §5.9's `project-statement`: the signature form when the
    /// signature reading holds, else the atom form with its conditional
    /// dot.
    fn project(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::PROJECT_STATEMENT);
        self.bump();
        if self.signature_follows() {
            self.signature();
        } else {
            self.atom();
            self.conditional_dot();
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `defined-statement`.
    fn defined(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::DEFINED_STATEMENT);
        self.bump();
        if matches!(self.peek(), SyntaxKind::MINUS | SyntaxKind::IDENT) {
            self.signature();
        } else {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Signature)]), None);
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `edge-statement`: pairs between `;` inside the
    /// parentheses, then the conditional dot.
    fn edge(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::EDGE_STATEMENT);
        self.bump();
        if self.expect(SyntaxKind::L_PAREN) {
            loop {
                if self.term_begins() {
                    self.start_node(SyntaxKind::EDGE);
                    self.term(TermContext::Term);
                    if !self.depth_refused() && self.expect(SyntaxKind::COMMA) && !self.term(TermContext::Term) {
                        self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    }
                    self.finish_node();
                } else {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if self.depth_refused() || !self.eat(SyntaxKind::SEMICOLON) {
                    break;
                }
            }
            if !self.depth_refused() && !self.eat(SyntaxKind::R_PAREN) {
                self.expected_token(SyntaxKind::R_PAREN);
            }
        }
        self.conditional_dot();
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `heuristic-statement`: the atom, the conditional
    /// dot, and its required annotation — its bracket is mandatory, and
    /// its absence carries the hint.
    fn heuristic(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::HEURISTIC_STATEMENT);
        self.bump();
        self.atom();
        self.conditional_dot();
        self.statement_end();
        self.annotation(Annotation::Heuristic, true);
        self.finish_node();
    }

    /// Grammar §5.9's `external-statement`: the atom, the conditional
    /// dot, and its optional annotation.
    fn external(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::EXTERNAL_STATEMENT);
        self.bump();
        self.atom();
        self.conditional_dot();
        self.statement_end();
        self.annotation(Annotation::External, false);
        self.finish_node();
    }

    /// Grammar §5.9's `const-statement`: the name, `=`, the term under the
    /// constant restriction, the dot, and its optional annotation.
    fn constant(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::CONST_STATEMENT);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.expect(SyntaxKind::EQ) && !self.term(TermContext::ConstantTerm) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        self.statement_end();
        self.annotation(Annotation::Const, false);
        self.finish_node();
    }

    /// The bracketed annotation after the dot (grammar §5.11;
    /// docs/design/syntax.md §6.3): parsed by the family, arity and
    /// spelling included; a violation is `unexpected-token` with the
    /// family's expected set. When `required` and no `[` follows, the
    /// missing bracket is diagnosed — for `#heuristic` with its hint.
    fn annotation(&mut self, family: Annotation, required: bool) {
        if self.depth_refused() {
            return;
        }
        if self.peek() != SyntaxKind::L_BRACKET {
            if required {
                let hint = (family == Annotation::Heuristic).then_some(Hint::HeuristicNeedsAnnotation);
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Annotation)]), hint);
            }
            return;
        }
        self.start_node(SyntaxKind::ANNOTATION);
        self.bump();
        match family {
            Annotation::WeakConstraint => self.weighted_terms(),
            Annotation::Heuristic => {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if !self.depth_refused() && self.eat(SyntaxKind::AT) && !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if !self.depth_refused() && self.expect(SyntaxKind::COMMA) && !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
            }
            Annotation::External => {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
            }
            Annotation::Const => {
                if !(self.eat_word(GrammarWord::Default) || self.eat_word(GrammarWord::Override)) {
                    self.unexpected(
                        expected(&[Expected::Word(GrammarWord::Default), Expected::Word(GrammarWord::Override)]),
                        None,
                    );
                }
            }
        }
        if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACKET) {
            if self.at_end() || self.statement_begins() {
                self.expected_token(SyntaxKind::R_BRACKET);
            } else {
                self.skip_into_error(expected(&[Expected::Token(SyntaxKind::R_BRACKET)]), None, &[SyntaxKind::R_BRACKET]);
                self.eat(SyntaxKind::R_BRACKET);
            }
        }
        self.finish_node();
    }

    /// Grammar §5.9's `script-statement` (grammar §4.8): after the
    /// header the parser asks under script mode and receives the body
    /// token, or `#end` directly when the region is empty; after the
    /// body it asks for `#end` under the same mode, then returns to
    /// normal mode for the dot. An unterminated region is a lexical
    /// `ERROR` to end of input; the missing `#end` and dot are then
    /// missing children.
    fn script(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::SCRIPT_STATEMENT);
        self.bump();
        let header = self.expect(SyntaxKind::L_PAREN) & self.expect(SyntaxKind::IDENT) & self.expect(SyntaxKind::R_PAREN);
        if header {
            self.set_mode(LexMode::ScriptBody);
            match self.peek() {
                SyntaxKind::SCRIPT_BODY | SyntaxKind::ERROR => self.bump(),
                _ => {}
            }
            self.expect(SyntaxKind::KW_END);
            self.set_mode(LexMode::Normal);
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `include-statement`: a string, or the three-token
    /// angle form.
    fn include(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::INCLUDE_STATEMENT);
        self.bump();
        match self.peek() {
            SyntaxKind::STRING => self.bump(),
            SyntaxKind::LT => {
                self.bump();
                self.expect(SyntaxKind::IDENT);
                self.expect(SyntaxKind::GT);
            }
            _ => self.unexpected(
                expected(&[Expected::Token(SyntaxKind::STRING), Expected::Token(SyntaxKind::LT)]),
                None,
            ),
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `program-statement`: the name and an optional
    /// identifier-only parameter list.
    fn program_statement(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::PROGRAM_STATEMENT);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.start_node(SyntaxKind::PARAMETERS);
            self.bump();
            if self.peek() == SyntaxKind::IDENT {
                loop {
                    self.expect(SyntaxKind::IDENT);
                    if !self.eat(SyntaxKind::COMMA) {
                        break;
                    }
                }
            }
            if !self.eat(SyntaxKind::R_PAREN) {
                self.expected_token(SyntaxKind::R_PAREN);
            }
            self.finish_node();
        }
        self.statement_end();
        self.finish_node();
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p themelios-syntax --lib`
Expected: the `directives` tests pass (8) and every earlier test still
passes.

- [ ] **Step 5: Run the full gate, then commit**

Run the four gate commands. Expected: green; repair or argue any lint
that fires (an empty `if self.peek() == SyntaxKind::DOT {}` branch in
`show` may draw a lint — write the three-way decision as a `match` on a
small enum if it does; `&` on two `expect` results in `script` is
deliberate — both must be consumed — and `clippy::nonminimal_bool` does
not fire on it).

```bash
git add crates/themelios-syntax
git commit -m "Add weak constraints, optimize statements, the directives with their annotations after the dot, and the script region"
```

---

### Task 12: The corpus, its harness, the tree laws, the recovery and diagnostics goldens, and the parse fuzz targets

**Files:**
- Create: `crates/themelios-syntax/tests/corpus/**` (vendored sources with
  provenance; the authored seeds), `crates/themelios-syntax/tests/corpus.rs`,
  `crates/themelios-syntax/tests/tree_laws.rs`,
  `crates/themelios-syntax/tests/golden.rs`, `crates/themelios-syntax/tests/golden/**`
  (blessed renderings), `crates/themelios-syntax/fuzz/fuzz_targets/parse.rs`,
  `crates/themelios-syntax/fuzz/corpus/parse/*`
- Modify: `crates/themelios-syntax/fuzz/Cargo.toml` (the second target)

**Derives:** syntax.md §2 (the failure conditions the corpus holds),
§3 (the dialect-neutrality law on the shared subset), §4.5 (the escape
case in the diagnostics corpus), §5.4 (the four tree laws over the
corpus — law 2's placement rule the one not already held by an entry
test), §6.5 (membership is `!has_errors()`; the incompleteness law over
prefixes), §6.6 (the depth refusal on an annotated family among the
goldens), §6.7 (each row's shape held by the golden corpus), §7.3 (every
kind and every characteristic help rendered), §16 (the fuzz crate's
parse targets; the property laws — the four tree laws, dialect
neutrality, incompleteness over prefixes; golden snapshots; the corpus
with provenance), Appendix B; spec §10.3; grammar §3 (the corpus as
reachability evidence), §11 (the seeds and their stated expectations,
D1's obligation to keep its inputs out of the shared corpus).

**Interfaces:**
- Consumes: the parser complete (Tasks 7–11); base's `SourceSet`,
  `view::human`.
- Produces: `tests/corpus/` with `PROVENANCE.md` per source,
  `NON-MEMBERS`, `DIFFERENTIAL-SKIP`; the `.expect` sidecar format the
  differential (Task 17) also reads; `tests/tree_laws.rs` — the four tree
  laws, the dialect-neutrality law, and the incompleteness law over
  prefixes, over the corpus; the golden harness's `check` function and
  the `GOLDEN_BLESS` switch; the `parse` fuzz target.

- [ ] **Step 1: Vendor the corpus with provenance**

Run, from the repository root, in a scratch directory outside the tree
(the clones are not committed; only the inputs are):

```bash
SCRATCH="$(mktemp -d)"
CORPUS="$PWD/crates/themelios-syntax/tests/corpus"
mkdir -p "$CORPUS"

# clingo v5.8.2 — the authority's own examples and test programs.
git -c advice.detachedHead=false clone -q --depth 1 --branch v5.8.2 https://github.com/potassco/clingo "$SCRATCH/clingo"
( cd "$SCRATCH/clingo" && test "$(git rev-parse HEAD)" = a99ffb2a58293c68b28fcc283a1d1c9ccad900fe )
mkdir -p "$CORPUS/clingo"
( cd "$SCRATCH/clingo" && find examples app/clingo/tests -name '*.lp' -size -65k -print0 \
    | sort -z | xargs -0 -I{} sh -c 'mkdir -p "$0/$(dirname "$1")" && cp "$1" "$0/$1"' "$CORPUS/clingo" {} )
cp "$SCRATCH/clingo/LICENSE.md" "$CORPUS/clingo/LICENSE"

# clingcon v5.2.1 — the shared-syntax claim's examples.
git -c advice.detachedHead=false clone -q --depth 1 --branch v5.2.1 https://github.com/potassco/clingcon "$SCRATCH/clingcon"
( cd "$SCRATCH/clingcon" && test "$(git rev-parse HEAD)" = 8c476557facf9fc996ec67053a01b6273fd9baba )
mkdir -p "$CORPUS/clingcon/examples"
cp "$SCRATCH/clingcon"/examples/*.lp "$CORPUS/clingcon/examples/"
cp "$SCRATCH/clingcon/LICENSE.md" "$CORPUS/clingcon/LICENSE"

# kallos — the formatter-inherited inputs (clingofmt-derived, inputs only).
git clone -q https://github.com/GregoryGelfond/kallos "$SCRATCH/kallos"
( cd "$SCRATCH/kallos" && git -c advice.detachedHead=false checkout -q 7db302ce902cccd37050151636281fd5588d8448 )
mkdir -p "$CORPUS/kallos"
cp "$SCRATCH/kallos"/crates/kallos/tests/corpus/clingofmt/* "$CORPUS/kallos/"
cp "$SCRATCH/kallos/LICENSE" "$CORPUS/kallos/LICENSE"

# kr-domains — the textbook encodings.
git clone -q https://github.com/GregoryGelfond/kr-domains "$SCRATCH/kr-domains"
( cd "$SCRATCH/kr-domains" && git -c advice.detachedHead=false checkout -q 38f0660ded448ed268c5a68759ceb0e2840dd497 )
mkdir -p "$CORPUS/kr-domains"
( cd "$SCRATCH/kr-domains" && find encodings scenarios standalone -name '*.lp' -print0 \
    | sort -z | xargs -0 -I{} sh -c 'mkdir -p "$0/$(dirname "$1")" && cp "$1" "$0/$1"' "$CORPUS/kr-domains" {} )
cp "$SCRATCH/kr-domains/LICENSE" "$CORPUS/kr-domains/LICENSE"

find "$CORPUS" -name '*.lp' | wc -l
```

Expected: about 500 `.lp` files (clingo 319, clingcon 10, kallos 17,
kr-domains 155 at the pins; record the exact count in the commit
message). The `-size -65k` filter excludes exactly the four clingo
instances above 64 KiB — `examples/gringo/gbie/instances/{sat_02,sat_03,unsat_02}.lp`
and one more the listing names — bytes, not syntax; name every excluded
file in the provenance.

Write `crates/themelios-syntax/tests/corpus/PROVENANCE.md`:

```markdown
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
| `clingo/` | github.com/potassco/clingo | tag `v5.8.2` = `a99ffb2a58293c68b28fcc283a1d1c9ccad900fe` | MIT (`clingo/LICENSE`) | every `.lp` under `examples/` and `app/clingo/tests/` at most 64 KiB, relative paths kept; excluded, being instance data of size and no syntax: `examples/gringo/gbie/instances/sat_02.lp`, `sat_03.lp`, `unsat_02.lp`, and `<the fourth, named here>` |
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
```

Create `NON-MEMBERS` and `DIFFERENTIAL-SKIP` empty but for a header
comment line each (`# path  identity...` / `# path  reason`); Step 4
fills them from what the harness reports.

- [ ] **Step 2: Author the seeds**

Run from the repository root; each seed is one input file and one
sidecar. Where a text ends in a newline the heredoc supplies it; the
inputs are exactly the grammar's, statement-wrapped where the seed
names a fragment.

```bash
S="crates/themelios-syntax/tests/corpus/seeds"
mkdir -p "$S/clingo" "$S/asp-core-2"
seed() { # seed <dialect-dir> <name> <expectation-lines...> ; input on stdin
  local dir="$1" name="$2"; shift 2
  cat > "$S/$dir/$name.lp"
  printf '%s\n' "$@" > "$S/$dir/$name.expect"
}
# --- lexical seeds (grammar §11) ---
seed clingo octal-oddity clingo non-member syntax::unexpected-token <<'EOF'
p(0o10).
EOF
seed clingo numeral-overflow-unpinned clingo member <<'EOF'
p(4294967296).
EOF
seed clingo uppercase-radix-prefix clingo non-member syntax::unexpected-token <<'EOF'
p(0X1F).
EOF
seed clingo double-underscore clingo non-member syntax::unexpected-token <<'EOF'
p(__).
EOF
seed clingo underscore-digit clingo non-member syntax::unexpected-token <<'EOF'
p(_1).
EOF
seed clingo block-comment-silences-its-line clingo member <<'EOF'
%* a % *%
b *%
p.
EOF
seed clingo block-comment-silenced-closers clingo non-member syntax::unterminated-block-comment <<'EOF'
%* a % *% b *%
EOF
seed asp-core-2 block-comment-no-silencing asp-core-2 non-member syntax::unexpected-end-of-input <<'EOF'
%* a % *% b *%
EOF
seed clingo block-comment-nesting clingo non-member syntax::unterminated-block-comment <<'EOF'
%* %* *%
EOF
seed asp-core-2 block-comment-no-nesting asp-core-2 member <<'EOF'
%* %* *%
EOF
seed clingo string-escapes clingo member <<'EOF'
p("a\nb"). q("a\"b"). r("a\\b").
EOF
seed clingo string-bad-escape clingo non-member syntax::malformed-string <<'EOF'
p("a\b").
EOF
seed asp-core-2 string-backslash-is-ordinary asp-core-2 member <<'EOF'
p("a\b"). q("a\nb").
EOF
seed clingo string-raw-line-break clingo non-member syntax::malformed-string syntax::unexpected-end-of-input <<'EOF'
p("a
b").
EOF
seed asp-core-2 string-spanning-lines asp-core-2 member <<'EOF'
p("a
b").
EOF
seed clingo unknown-hash-words clingo non-member syntax::unknown-hash-word <<'EOF'
#sums. #counting.
EOF
seed clingo anonymous-in-theory clingo non-member syntax::anonymous-in-theory-expression <<'EOF'
&a { _ }.
EOF
seed clingo theory-operator-runs clingo member <<'EOF'
&a { x :-: y }. &b { x ;; y }. &c { x :: y }. &d { x := y }. &e { 1 .. 2 }. &f { x :~ y }.
EOF
seed clingo show-dead-dollar clingo non-member syntax::unexpected-characters <<'EOF'
#show $x/1.
EOF
# --- syntactic seeds ---
seed clingo unary-tighter-than-power clingo member <<'EOF'
p(-2**2). q(~2**2).
EOF
seed clingo trailing-comma-form clingo member <<'EOF'
p((,)).
EOF
seed clingo empty-pooled-arguments clingo member <<'EOF'
p(f(;)). q(f(a;)). r(f()).
EOF
seed clingo trailing-comma-in-arguments clingo non-member syntax::unexpected-token <<'EOF'
f(a,).
EOF
seed clingo comma-separated-head clingo member <<'EOF'
a, b.
EOF
seed clingo empty-condition-before-pipe clingo non-member syntax::unexpected-token <<'EOF'
p(X) : | q(X).
EOF
seed clingo empty-condition-in-body clingo member <<'EOF'
:- p : .
EOF
seed clingo singleton-conditioned-heads clingo member <<'EOF'
a : b. p(X) : q(X) :- r. a : .
EOF
seed clingo heuristic-annotation clingo member <<'EOF'
#heuristic a. [1,sign]
EOF
seed clingo pool-inside-theory-condition clingo member <<'EOF'
&a { t : p((x;y)), q ; u }.
EOF
seed clingo guard-ends-before-comma clingo member <<'EOF'
:- &sum { x } >= 5, not p.
EOF
seed clingo colon-at-depth-opens-nothing clingo non-member syntax::unexpected-token <<'EOF'
&a { {x : y} }.
EOF
seed clingo empty-aggregate-elements-in-body clingo member <<'EOF'
:- #sum { : }. :- #sum { a : }.
EOF
seed clingo empty-aggregate-elements-in-head clingo non-member syntax::unexpected-token <<'EOF'
#sum { : }. #sum { a : }.
EOF
seed clingo comparison-chain clingo member <<'EOF'
:- 1 < X < 5.
EOF
seed clingo pooled-absolute-value clingo member <<'EOF'
p(|X;Y|).
EOF
seed clingo include-angle-form clingo member <<'EOF'
#include < lib > .
EOF
seed clingo default-is-an-identifier clingo member <<'EOF'
#const default = 1.
EOF
seed clingo empty-bodies clingo member <<'EOF'
head :- . :- .
EOF
seed clingo script-end-inside-code clingo non-member syntax::unexpected-token syntax::malformed-string <<'EOF'
#script (lua) x = "#end is here" #end.
EOF
seed clingo bare-theory-atoms clingo member <<'EOF'
&a. &a {}.
EOF
seed clingo not-as-theory-operator clingo member <<'EOF'
&a { x not y }.
EOF
seed clingo aspif-dispatch clingo non-member syntax::aspif-input <<'EOF'
asp 1 0 0
1 0 1 1 0 0
0
EOF
# --- dialect seeds ---
seed asp-core-2 query asp-core-2 member <<'EOF'
p(1)?
EOF
seed clingo query-under-clingo clingo non-member syntax::unexpected-end-of-input <<'EOF'
p(1)?
EOF
seed clingo question-not-final clingo non-member syntax::unexpected-token <<'EOF'
p ? q.
EOF
seed asp-core-2 question-not-final asp-core-2 non-member syntax::unexpected-token <<'EOF'
p ? q.
EOF
seed clingo comparison-headed-with-question clingo member <<'EOF'
p ? q = X. p(1)?2 > 3. x(1?2).
EOF
seed asp-core-2 comparison-headed-with-question asp-core-2 member <<'EOF'
p ? q = X. p(1)?2 > 3. x(1?2).
EOF
seed asp-core-2 string-maximal-munch asp-core-2 member <<'EOF'
p("a\" b"). q("a\").
EOF
seed clingo string-escaped-quote-unterminated clingo non-member syntax::malformed-string syntax::unexpected-end-of-input <<'EOF'
p("a\").
EOF
# --- doc-comment seeds ---
seed clingo docs-then-statement clingo member <<'EOF'
%! doc
p.
EOF
seed clingo docs-across-blank-line clingo member <<'EOF'
%! a

p.
EOF
seed clingo doc-inside-statement clingo member <<'EOF'
p :- %! x
q.
EOF
seed clingo doc-as-last-line clingo member <<'EOF'
p.
%! x
EOF
seed clingo doc-marker-is-exact clingo member <<'EOF'
%%! x
% ! x
p.
EOF
seed clingo doc-marker-inside-block-comment clingo non-member syntax::unterminated-block-comment <<'EOF'
%* %! *%
p.
EOF
seed asp-core-2 doc-marker-inside-block-comment asp-core-2 member <<'EOF'
%* %! *%
p.
EOF
seed clingo shebang-then-docs clingo member <<'EOF'
#! shebang
%! d
p.
EOF
seed asp-core-2 documented-query asp-core-2 member <<'EOF'
%! q
p(1)?
EOF
seed clingo doc-inside-theory-expression clingo member <<'EOF'
&a { %! x
x }.
EOF
# --- this design's own (docs/design/syntax.md §6.3, §16) ---
seed clingo const-pooled-absolute-value clingo non-member syntax::form-not-allowed-here <<'EOF'
#const x = |1;2|.
EOF
seed clingo external-annotation-arity clingo non-member syntax::unexpected-token <<'EOF'
#external p. [a, b]
EOF
seed clingo const-annotation-spelling clingo non-member syntax::unexpected-token <<'EOF'
#const n = 1. [foo]
EOF
seed clingo bad-escape-leaves-the-next-statement-intact clingo non-member syntax::malformed-string <<'EOF'
p("a\qb"). q.
EOF
find "$S" -name '*.lp' | wc -l
```

Expected: 62 seed inputs with 62 sidecars.

- [ ] **Step 3: Write the corpus harness**

`crates/themelios-syntax/tests/corpus.rs`:

```rust
//! The corpus (docs/design/syntax.md §16; docs/specification.md §10.3):
//! every input parsed under its stated dialect with its stated
//! expectation — member, or the diagnostic identities expected — and,
//! over every input, the text law, determinism, and the depth bound.
//! What it proves: reachability, membership as this parser reads it,
//! and the laws over real programs. What it cannot: agreement with the
//! authority — the differential's question.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{parse, MAX_TREE_DEPTH};
use themelios_syntax::tree::{NodeOrToken, SyntaxNode, WalkEvent};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Every `.lp` under the corpus, sorted.
fn inputs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![corpus_dir()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("corpus directory reads") {
            let path = entry.expect("corpus entry reads").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "lp") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// What the corpus says about one input.
struct Expectation {
    dialect: Dialect,
    member: bool,
    /// For a non-member: the identities that must each appear, and
    /// outside which none may.
    identities: BTreeSet<String>,
}

fn expectation(path: &Path, non_members: &[(String, BTreeSet<String>)]) -> Expectation {
    let sidecar = path.with_extension("expect");
    if let Ok(text) = fs::read_to_string(&sidecar) {
        let mut lines = text.lines();
        let dialect = match lines.next() {
            Some("clingo") => Dialect::Clingo,
            Some("asp-core-2") => Dialect::AspCore2,
            other => panic!("{}: dialect line, found {other:?}", sidecar.display()),
        };
        let member = match lines.next() {
            Some("member") => true,
            Some("non-member") => false,
            other => panic!("{}: membership line, found {other:?}", sidecar.display()),
        };
        let identities = lines.map(str::to_owned).collect();
        return Expectation { dialect, member, identities };
    }
    let relative = path.strip_prefix(corpus_dir()).expect("under the corpus").to_string_lossy().into_owned();
    match non_members.iter().find(|(name, _)| *name == relative) {
        Some((_, identities)) => {
            Expectation { dialect: Dialect::Clingo, member: false, identities: identities.clone() }
        }
        None => Expectation { dialect: Dialect::Clingo, member: true, identities: BTreeSet::new() },
    }
}

fn non_members() -> Vec<(String, BTreeSet<String>)> {
    fs::read_to_string(corpus_dir().join("NON-MEMBERS"))
        .expect("NON-MEMBERS is present")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            let path = fields.next().expect("a path").to_owned();
            (path, fields.map(str::to_owned).collect())
        })
        .collect()
}

/// The tree's depth, by an iterative walk.
fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

#[test]
fn every_input_parses_as_the_corpus_says() {
    let non_members = non_members();
    let mut failures = Vec::new();
    let mut count = 0usize;
    for path in inputs() {
        count += 1;
        let text = fs::read_to_string(&path).expect("input reads");
        let expected = expectation(&path, &non_members);
        let source = Source::new(SourceId::new(0), text.clone()).expect("corpus inputs admit");
        let parse = parse(&source, expected.dialect);
        let again = parse(&source, expected.dialect);
        let name = path.strip_prefix(corpus_dir()).expect("under the corpus").display().to_string();
        if parse.syntax().text() != text.as_str() {
            failures.push(format!("{name}: the tree's text is not the input"));
        }
        if parse != again {
            failures.push(format!("{name}: two parses differ"));
        }
        if depth(&parse.syntax()) > MAX_TREE_DEPTH as usize {
            failures.push(format!("{name}: deeper than the bound"));
        }
        let errors: BTreeSet<String> = parse
            .diagnostics()
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .map(|d| d.id().to_string())
            .collect();
        if expected.member {
            if parse.has_errors() {
                failures.push(format!("{name}: expected a member, found {errors:?}"));
            }
        } else if !parse.has_errors() {
            failures.push(format!("{name}: expected a non-member, found a member"));
        } else if errors != expected.identities {
            failures.push(format!("{name}: expected {:?}, found {errors:?}", expected.identities));
        }
    }
    assert!(count > 400, "the corpus is vendored: {count} inputs");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
```

- [ ] **Step 4: Run the harness and triage what it reports**

Run: `cargo test -p themelios-syntax --test corpus -- --nocapture`

The seeds' expectations are the design's and the grammar's: a seed
that fails is a defect of the parser (Tasks 7–11) or of this plan's
stated expectation, and either is raised at the mid-stage stop after
this task, never absorbed by editing the sidecar. A vendored input the
parser reads as a non-member is one of three things, triaged in this
order: (1) a parser defect — repair it in this task, recording the
input in the commit message; (2) a genuine non-member at the pin (a
program the authority also rejects — verified against the pinned
binary once the differential lands, in Task 17) — record it in
`NON-MEMBERS` with its identities; (3) a disagreement with the grammar
of record — a divergence, raised at the stop. Inputs holding a comment
inside a theory expression go into `DIFFERENTIAL-SKIP` with the reason
`D1` (grammar §11) — the harness here still parses them as members.

Expected end state: the test passes; `NON-MEMBERS` and
`DIFFERENTIAL-SKIP` list what triage found, each line with its reason.

- [ ] **Step 5: Write the tree laws, the dialect-neutrality law, and the incompleteness law**

`crates/themelios-syntax/tests/tree_laws.rs`:

```rust
//! The four tree laws, the dialect-neutrality law, and the
//! incompleteness law over prefixes (docs/design/syntax.md §5.4, §3,
//! §6.5, §16), over the vendored corpus. What they prove: the tree's
//! shape invariants hold on real programs; the two dialects agree on
//! their shared subset; a member's prefixes are never wrong, only
//! complete or unfinished. Laws 1, 3, and 4 also stand inside the
//! membership harness (`corpus.rs`); here they are named laws beside
//! law 2, whose placement rule no entry test holds over the whole
//! corpus.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::diagnostic::SyntaxError;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{parse, MAX_TREE_DEPTH};
use themelios_syntax::tree::{role, NodeOrToken, SyntaxKind, SyntaxNode, TokenRole, WalkEvent};

/// Every corpus input with its dialect (the sidecar's, else clingo).
fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let dialect = fs::read_to_string(path.with_extension("expect"))
                    .ok()
                    .and_then(|sidecar| sidecar.lines().next().map(str::to_owned))
                    .map_or(Dialect::Clingo, |line| if line == "asp-core-2" { Dialect::AspCore2 } else { Dialect::Clingo });
                found.push((path.strip_prefix(&dir).expect("under corpus").display().to_string(), text, dialect));
            }
        }
    }
    found.sort();
    found
}

fn root_of(text: &str, dialect: Dialect) -> SyntaxNode {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, dialect).syntax()
}

/// The tree's depth by an iterative walk.
fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

/// The three roots: trivia at a root's edges belongs to it (law 2).
fn is_root(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::PROGRAM | SyntaxKind::STATEMENT_FRAGMENT | SyntaxKind::TERM_FRAGMENT)
}

#[test]
fn the_four_tree_laws_hold_over_the_corpus() {
    let mut failures = Vec::new();
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let one = parse(&source, dialect);
        let two = parse(&source, dialect);
        // Law 1 (text): the tree's text is the input, byte for byte.
        if one.syntax().text() != text.as_str() {
            failures.push(format!("{name}: law 1 — the tree's text is not the input"));
        }
        // Law 4 (determinism): two parses of one text are equal.
        if one != two {
            failures.push(format!("{name}: law 4 — two parses of one text differ"));
        }
        // Law 3 (bounded depth): no tree is deeper than the bound.
        if depth(&one.syntax()) > MAX_TREE_DEPTH as usize {
            failures.push(format!("{name}: law 3 — deeper than MAX_TREE_DEPTH"));
        }
        // Law 2 (placement): every non-empty node but a root begins and
        // ends with a significant token — role not Trivia, so a doc line
        // in docs position (role Documentation) still counts as
        // significant.
        for node in one.syntax().descendants() {
            if is_root(node.kind()) || node.text_range().is_empty() {
                continue;
            }
            for edge in [node.first_token(), node.last_token()].into_iter().flatten() {
                if role(&edge) == TokenRole::Trivia {
                    failures.push(format!("{name}: law 2 — {} begins or ends with trivia {:?}", node.kind(), edge.text()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The non-whitespace token stream — kinds and texts — for comparing two
/// dialects' readings of one text.
fn token_stream(root: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    root.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .map(|t| (t.kind(), t.text().to_owned()))
        .collect()
}

/// The significant-token shape: the preorder over nodes and significant
/// tokens, trivia dropped.
fn shape(root: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => out.push(format!("({}", node.kind())),
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(")".to_owned()),
            WalkEvent::Enter(NodeOrToken::Token(token)) if role(&token) != TokenRole::Trivia => {
                out.push(format!("{}:{}", token.kind(), token.text()));
            }
            _ => {}
        }
    }
    out
}

fn diagnostics(text: &str, dialect: Dialect) -> Vec<SyntaxError> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, dialect).diagnostics().to_vec()
}

#[test]
fn the_two_dialects_agree_on_their_shared_subset() {
    // The shared subset, detected by its consequence: a text whose two
    // dialects lex to the same non-whitespace token stream (so the string
    // and block-comment rules did not bite) and whose last significant
    // token is not `?` (so the query reading cannot bite) must yield
    // structurally equal trees and equal diagnostics — exactly the inputs
    // docs/design/syntax.md §3 names.
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (name, text, _) in corpus() {
        let clingo = root_of(&text, Dialect::Clingo);
        let core = root_of(&text, Dialect::AspCore2);
        if token_stream(&clingo) != token_stream(&core) {
            continue; // the string or block-comment rule bit: not shared.
        }
        // The last *significant* token — trivia (a trailing comment,
        // whitespace) skipped, since the query reading skips it too.
        let last_significant = clingo
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| role(t) == TokenRole::Significant)
            .last();
        if last_significant.is_some_and(|t| t.kind() == SyntaxKind::QUESTION) {
            continue; // a final `?`: the query reading may bite; not shared.
        }
        checked += 1;
        if shape(&clingo) != shape(&core) {
            failures.push(format!("{name}: the shared subset's trees differ by dialect"));
        }
        if diagnostics(&text, Dialect::Clingo) != diagnostics(&text, Dialect::AspCore2) {
            failures.push(format!("{name}: the shared subset's diagnostics differ by dialect"));
        }
    }
    assert!(checked > 50, "the shared subset is a substantial part of the corpus: {checked} inputs");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_prefix_of_a_member_at_a_token_boundary_is_error_free_or_incomplete() {
    // Sampled: up to ~32 boundaries spread across each member, enough to
    // exercise mid-construct cuts without an O(n^2) sweep of every file.
    const SAMPLES_PER_INPUT: usize = 32;
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let whole = parse(&source, dialect);
        if whole.has_errors() {
            continue; // the law speaks of member programs.
        }
        // The token boundaries: the end offset of every token, in order.
        let mut boundaries = Vec::new();
        let mut at = 0usize;
        for token in whole.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
            at += token.text().len();
            boundaries.push(at);
        }
        let stride = boundaries.len().div_ceil(SAMPLES_PER_INPUT).max(1);
        for offset in boundaries.iter().step_by(stride).copied() {
            let prefix = &text[..offset];
            let prefix_source = Source::new(SourceId::new(0), prefix.to_owned()).expect("admits");
            let prefix_parse = parse(&prefix_source, dialect);
            checked += 1;
            if prefix_parse.has_errors() && !prefix_parse.is_incomplete() {
                let ids: Vec<String> = prefix_parse
                    .diagnostics()
                    .iter()
                    .filter(|d| d.severity() == Severity::Error)
                    .map(|d| d.id().to_string())
                    .collect();
                failures.push(format!("{name}: the {offset}-byte prefix is neither error-free nor incomplete: {ids:?}"));
            }
        }
    }
    assert!(checked > 0, "the corpus has member programs to take prefixes of");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
```

- [ ] **Step 6: Run the tree-laws suite**

Run: `cargo test -p themelios-syntax --test tree_laws`
Expected: 3 passed. A law-2 failure names a node that begins or ends
with trivia — a placement defect in the family that built it (§5.4); a
neutrality failure names an input the two dialects read differently
though it is on the shared subset — a dialect leak (§3); an
incompleteness failure names a member prefix reported as wrong rather
than unfinished — a recovery or an `is_incompleteness` defect (§6.5).
Each is raised at the mid-stage stop after this task, never absorbed by
weakening the law.


- [ ] **Step 7: Write the golden harness and its cases**

`crates/themelios-syntax/tests/golden.rs`:

```rust
//! The reviewed goldens (docs/design/syntax.md §16): the diagnostics
//! corpus — the characteristic malformed programs of every family in
//! §6.7 and every identity in Appendix B, rendered through base's human
//! view (the diagnostics-quality witness) — and the recovery shape of
//! each family's row as a tree dump. Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-syntax --test golden`, then
//! review the diff before committing: these files are reviewed
//! artifacts, not incidental output. Attachment dumps join in Task 14.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId, SourceSet};
use themelios_base::span::ByteOffset;
use themelios_base::view::human;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{parse, parse_program, MAX_NESTING_DEPTH};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::SyntaxKind;

fn check(group: &str, name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(group)
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a group directory")).expect("golden directory");
        fs::write(&path, actual).expect("golden file writes");
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden file {}; bless it and review", path.display()));
    assert_eq!(actual, expected, "diverged from the reviewed golden `{group}/{name}`");
}

/// The human view of every diagnostic of `text` under `dialect`, in
/// the parser's order, each rendering separated by a blank line.
fn diagnostics(text: &str, dialect: Dialect) -> String {
    let mut catalog = SourceSet::new();
    let file = catalog.add("input.lp".to_owned(), text.to_owned()).expect("admits");
    let source = Source::new(file, text.to_owned()).expect("admits");
    let parse = parse(&source, dialect);
    let mut out = String::new();
    for diagnostic in parse.diagnostics() {
        out.push_str(&human(&diagnostic.to_diagnostic(), &catalog));
        out.push('\n');
    }
    out
}

/// The tree dump and the diagnostics of `text` — the recovery shape.
fn recovery(text: &str) -> String {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    let mut out = format!("{:#?}", parse.syntax());
    out.push('\n');
    for diagnostic in parse.diagnostics() {
        out.push_str(&format!("{}: {:?}\n", diagnostic.id(), diagnostic.kind()));
    }
    out
}

fn diag(name: &str, text: &str) {
    check("diagnostics", name, &diagnostics(text, Dialect::Clingo));
}

// ---- every identity of Appendix B, and the characteristic helps ------

#[test]
fn unexpected_characters() {
    diag("unexpected-characters", "p($$$).\n");
}

#[test]
fn unknown_hash_word() {
    diag("unknown-hash-word", "#sums { X : p(X) } > 1.\n");
}

#[test]
fn malformed_string_bad_escape_leaves_the_next_statement_intact() {
    diag("malformed-string-escape", "p(\"a\\qb\"). q.\n");
}

#[test]
fn malformed_string_line_break() {
    diag("malformed-string-line-break", "p(\"a\nb\").\n");
}

#[test]
fn malformed_string_unterminated() {
    diag("malformed-string-unterminated", "p(\"abc");
}

#[test]
fn unterminated_block_comment() {
    diag("unterminated-block-comment", "%* a % *% b *%\np.\n");
}

#[test]
fn unterminated_script() {
    diag("unterminated-script", "#script (lua)\nx = 1\n");
}

#[test]
fn anonymous_in_theory_expression() {
    diag("anonymous-in-theory-expression", "&sum { _ } <= 1.\n");
}

#[test]
fn unexpected_token() {
    diag("unexpected-token", "p(X) :- q(X) r(X).\n");
}

#[test]
fn unexpected_end_of_input() {
    diag("unexpected-end-of-input", "p(X) :- q(X),");
}

#[test]
fn nesting_too_deep_on_an_annotated_family() {
    // One opener per line, so the rendering's window around the refused
    // bracket stays legible whatever the constant's value.
    let depth = MAX_NESTING_DEPTH as usize + 1;
    let text = format!(":~ p({}x{}). [1@2]\nq.\n", "f(\n".repeat(depth), ")".repeat(depth));
    diag("nesting-too-deep-annotated", &text);
}

#[test]
fn aspif_input() {
    diag("aspif-input", "asp 1 0 0\n1 0 1 1 0 0\n0\n");
}

#[test]
fn token_source_breach() {
    struct EarlyEnd(Source);
    impl TokenSource for EarlyEnd {
        fn id(&self) -> SourceId {
            self.0.id()
        }
        fn dialect(&self) -> Dialect {
            Dialect::Clingo
        }
        fn text(&self) -> &str {
            self.0.text()
        }
        fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
            if at.get() >= 5 {
                return Ok(Token { kind: SyntaxKind::EOF, text: "" });
            }
            themelios_syntax::lexer::Lexer::new(&self.0, Dialect::Clingo).token_at(at, mode)
        }
    }
    let text = "p(X). q(X).\n";
    let mut catalog = SourceSet::new();
    let file = catalog.add("input.lp".to_owned(), text.to_owned()).expect("admits");
    let source = EarlyEnd(Source::new(file, text.to_owned()).expect("admits"));
    let parse = parse_program(&source);
    let mut out = String::new();
    for diagnostic in parse.diagnostics() {
        out.push_str(&human(&diagnostic.to_diagnostic(), &catalog));
        out.push('\n');
    }
    check("diagnostics", "token-source-breach", &out);
}

#[test]
fn form_not_allowed_here() {
    diag("form-not-allowed-here", "#const x = |1;2|.\n#const y = 1..3.\n#const z = X.\n");
}

#[test]
fn misplaced_doc_comment() {
    diag("misplaced-doc-comment", "p :- %! inside\n  q.\n%! nothing follows\n");
}

#[test]
fn hint_trailing_comma_in_arguments() {
    diag("hint-trailing-comma", "p(a, b,).\n");
}

#[test]
fn hint_query_mark_needs_asp_core_2() {
    diag("hint-query-mark", "p(1)?");
}

#[test]
fn hint_leading_zero_numeral() {
    diag("hint-leading-zero", "p(007).\n");
}

#[test]
fn hint_empty_condition_before_pipe() {
    diag("hint-empty-condition-pipe", "p(X) : | q(X).\n");
}

#[test]
fn hint_heuristic_needs_annotation() {
    diag("hint-heuristic-annotation", "#heuristic a : b.\n");
}

#[test]
fn a_query_under_the_asp_core_2_dialect_renders_nothing() {
    check("diagnostics", "asp-core-2-query", &diagnostics("p(1)?", Dialect::AspCore2));
}

// ---- the recovery shape of each row of docs/design/syntax.md §6.7 ----

#[test]
fn recovery_program_level() {
    check("recovery", "program-level", &recovery(") stray. p.\n#heuristic. [1] q.\n"));
}

#[test]
fn recovery_head_body_condition() {
    check("recovery", "head-body-condition", &recovery("a ; ; b :- p, , q : r, , s.\n"));
}

#[test]
fn recovery_literal_atom_comparison() {
    check("recovery", "literal-atom-comparison", &recovery("p :- 1 <, X = .\n"));
}

#[test]
fn recovery_terms_and_argument_lists() {
    check("recovery", "frame-loop", &recovery("p(f(a b), (c;), |d, 1 +).\n"));
}

#[test]
fn recovery_aggregates() {
    check("recovery", "aggregates", &recovery(":- #count { a; b . q.\n"));
}

#[test]
fn recovery_theory_atoms_and_elements() {
    check("recovery", "theory", &recovery(":- &sum { x, ; y : p( } <= . q.\n"));
}

#[test]
fn recovery_directives() {
    check("recovery", "directives", &recovery("#show p/. #const n = . #include. #program p(1).\n"));
}

#[test]
fn recovery_theory_definitions() {
    check("recovery", "theory-definitions", &recovery("#theory t { x { + : 1, ternary; - : }; &a/1 : x, foo }.\n"));
}

#[test]
fn recovery_script() {
    check("recovery", "script", &recovery("#script (python)\nx = 1\n"));
}

#[test]
fn recovery_annotations() {
    check("recovery", "annotations", &recovery(":~ p. [1@\n#external q. [a\nr.\n"));
}

#[test]
fn recovery_end_of_input() {
    check("recovery", "end-of-input", &recovery(":- p(X), #count { X : q(X"));
}

// ---- tree dumps for the grammar's corner seeds ------------------------

fn dump(name: &str, text: &str) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    check("trees", name, &format!("{:#?}", parse.syntax()));
}

#[test]
fn tree_pooled_arguments_and_tuples() {
    dump("pools-and-tuples", "p(f(a,b;c), (), (a,), (,), (a;b)).\n");
}

#[test]
fn tree_operator_chains_flat() {
    dump("operator-chains", "p(1 + 2 * 3 - -4 ** 2 ** 3, 1..3 ^ 2 ? 4 & 5).\n");
}

#[test]
fn tree_disjunction_separators_and_conditions() {
    dump("disjunction", "a ; b | c, d : e, f.\n");
}

#[test]
fn tree_aggregates_both_positions() {
    dump("aggregates", "1 { p(X) : q(X) } 1 :- 2 <= #sum { W,T : t(T,W) } < 3, not #count { } 1.\n");
}

#[test]
fn tree_theory_atom_with_guard() {
    dump("theory-atom", ":- &sum { x, -y : p ; {a, b}, [1], f(g) } <= - not 3.\n");
}

#[test]
fn tree_documented_statement_and_script() {
    dump("docs-and-script", "%! doc\n%! more\np.\n#script (lua)\nreturn 1\n#end.\n");
}
```

- [ ] **Step 8: Bless, review, accept**

Run: `GOLDEN_BLESS=1 cargo test -p themelios-syntax --test golden`, then
read every file under `tests/golden/diagnostics/`, `tests/golden/recovery/`,
and `tests/golden/trees/` against the design: each recovery dump against
its row of syntax.md §6.7 (the `ERROR` node holds what the row says,
synchronization where the row says); each rendering against the
rust-analyzer bar of spec §2 item 9 (a headline that names the mistake,
a primary label that says what stands there, related loci and helps
where the design gives them). A rendering that reads poorly is a defect
of Task 6's message tables — repair the table, re-bless, re-read; a
recovery shape that departs from its row is a defect of the family's
task — repair it, re-bless. Then `cargo test -p themelios-syntax --test golden`.
Expected: green. Record the acceptance of the corpus in the commit
message: "goldens reviewed and accepted".

- [ ] **Step 9: Add the parse fuzz target**

`crates/themelios-syntax/fuzz/fuzz_targets/parse.rs`:

```rust
//! Arbitrary bytes under both dialects and every entry point: no panic,
//! the tree's text is the input, the parse terminates, `has_errors` and
//! `is_incomplete` are consistent with the diagnostics, and the tree's
//! depth respects the bound (docs/design/syntax.md §16). Attachment,
//! the certificate, and the mode law join this target in Tasks 14–16.
#![no_main]

use libfuzzer_sys::fuzz_target;
use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    parse_program, parse_statement, parse_term, parse_term_value, Parse, MAX_TREE_DEPTH,
};
use themelios_syntax::tree::{Asp, AstNode, NodeOrToken, SyntaxNode, WalkEvent};

fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

fn holds<T: AstNode<Language = Asp>>(parse: &Parse<T>, text: &str) {
    assert_eq!(parse.syntax().text(), text);
    let _ = parse.tree();
    let has_error = parse.diagnostics().iter().any(|d| d.severity() == Severity::Error);
    assert_eq!(parse.has_errors(), has_error);
    if parse.is_incomplete() {
        assert!(parse.has_errors());
        for diagnostic in parse.diagnostics().iter().filter(|d| d.severity() == Severity::Error) {
            let name = diagnostic.id().name();
            assert!(
                matches!(
                    name,
                    "unexpected-end-of-input"
                        | "unterminated-block-comment"
                        | "unterminated-script"
                        | "malformed-string"
                ),
                "{name} is not an incompleteness"
            );
        }
    }
    assert!(depth(&parse.syntax()) <= MAX_TREE_DEPTH as usize);
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    let text = source.text().to_owned();
    for dialect in [Dialect::Clingo, Dialect::AspCore2] {
        let lexer = Lexer::new(&source, dialect);
        holds(&parse_program(&lexer), &text);
        holds(&parse_statement(&lexer), &text);
        holds(&parse_term(&lexer), &text);
        holds(&parse_term_value(&lexer), &text);
    }
});
```

Add the target to `fuzz/Cargo.toml`:

```toml
[[bin]]
name = "parse"
path = "fuzz_targets/parse.rs"
test = false
doc = false
bench = false
```

Seed `fuzz/corpus/parse/` with copies of six seeds — from
`tests/corpus/seeds/clingo/`: `theory-operator-runs.lp`,
`empty-pooled-arguments.lp`, `heuristic-annotation.lp`,
`docs-then-statement.lp`, `script-end-inside-code.lp`,
`string-raw-line-break.lp` — and run, from `crates/themelios-syntax`:
`cargo fuzz build -s none && cargo fuzz run parse -s none -- -max_total_time=300`
Expected: five minutes without a crash. A crash is a parser defect:
minimize it (`cargo fuzz tmin parse <artifact> -s none`), add the input
to `tests/corpus/seeds/` with its expectation once repaired, and repair
before proceeding.

- [ ] **Step 10: Run the full gate, then commit**

Run the four gate commands. Expected: green.

```bash
git add crates/themelios-syntax
git commit -m "Vendor the corpus with its provenance and seeds; hold membership, the text law, determinism, and the depth bound over it; bless the recovery and diagnostics goldens; add the parse fuzz target"
```

---

**STOP — the mid-stage reading.** The parser is complete: the lexer, the
tree, the oracle over texts, the diagnostics, every family, the corpus,
the goldens. Before the typed AST bakes on it, the change from the
scaffold's parent commit to this task's commit is read as a whole
against the design of record; its findings are adjudicated item by
item, applied in one repair commit, confirmed by one further reading,
and only then does Task 13 begin.

---

### Task 13: The `ast` module complete — the wrappers, the enums, the traits, the token wrappers and their values

**Files:**
- Modify: `crates/themelios-syntax/src/ast/mod.rs` (the enums, the
  traits, the helpers, the roots' accessors, the re-exports)
- Create: `crates/themelios-syntax/src/ast/nodes.rs`,
  `crates/themelios-syntax/src/ast/tokens.rs`,
  `crates/themelios-syntax/tests/ast_completeness.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs`
  (`Parse::string_value`)

**Derives:** syntax.md §3 (the dialect at `StringLit::value`;
`Parse::string_value`), §5.4 (`role` read by `DocLine` and `Comment`),
§5.5 (`string_value`), §6.1 (the fragment roots' accessors), §8 whole
(§8.1 the conventions, §8.2 the representative signatures and the three
argued shapes, §8.3 the token wrappers and values), §12.5
(`InvalidStringLiteral: Display + Error`), §13, §16 (the typed AST's
completeness over the roster), Appendix A.

**Interfaces:**
- Consumes: the tree (Task 2), `tree::role`, `dialect::Dialect`, base's
  `ByteOffset`.
- Produces: every wrapper of Appendix A's node kinds under `ast::` (in
  CamelCase); the enums `Statement`, `Head`, `BodyElement`,
  `LiteralInner`, `Term`, `TheoryTerm`, `Aggregate`, `AggregateElement`,
  `SetElement`, `DisjunctionElement`, `TheoryDefItem`, `Constant`,
  `TheoryOpTermItem`; the value enums `Negation`, `Relation`,
  `Precedence`, `Associativity`, `AggregateFunction`, `ConstPolicy`,
  `Radix`, `CommentForm`; the traits `HasDocs`, `HasGuards`, and the
  token idiom's `AstToken`; the token wrappers `Ident`, `Variable`,
  `NumberLit`, `StringLit`, `DocLine`, `Comment`, `ScriptBody`;
  `InvalidStringLiteral`; `Parse::string_value`.

- [ ] **Step 1: Write the failing tests**

`crates/themelios-syntax/tests/ast_completeness.rs`:

```rust
//! The typed AST's completeness over the roster (docs/design/syntax.md
//! §8.1, §16): every node kind is cast by exactly one wrapper — the
//! structural half of the law; that every production slot has an
//! accessor is held by reading `ast` against Appendix A and by the
//! accessor tests in the module.

use themelios_syntax::ast;
use themelios_syntax::tree::{AstNode, SyntaxKind};

#[test]
fn every_node_kind_is_cast_by_exactly_one_wrapper() {
    let wrappers: [(&str, fn(SyntaxKind) -> bool); 59] = [
        ("Program", ast::Program::can_cast),
        ("StatementFragment", ast::StatementFragment::can_cast),
        ("TermFragment", ast::TermFragment::can_cast),
        ("Rule", ast::Rule::can_cast),
        ("WeakConstraint", ast::WeakConstraint::can_cast),
        ("OptimizeStatement", ast::OptimizeStatement::can_cast),
        ("OptimizeElement", ast::OptimizeElement::can_cast),
        ("ShowStatement", ast::ShowStatement::can_cast),
        ("Signature", ast::Signature::can_cast),
        ("ProjectStatement", ast::ProjectStatement::can_cast),
        ("DefinedStatement", ast::DefinedStatement::can_cast),
        ("EdgeStatement", ast::EdgeStatement::can_cast),
        ("Edge", ast::Edge::can_cast),
        ("HeuristicStatement", ast::HeuristicStatement::can_cast),
        ("ExternalStatement", ast::ExternalStatement::can_cast),
        ("ConstStatement", ast::ConstStatement::can_cast),
        ("ScriptStatement", ast::ScriptStatement::can_cast),
        ("IncludeStatement", ast::IncludeStatement::can_cast),
        ("ProgramStatement", ast::ProgramStatement::can_cast),
        ("Parameters", ast::Parameters::can_cast),
        ("TheoryDefinition", ast::TheoryDefinition::can_cast),
        ("TermDefinition", ast::TermDefinition::can_cast),
        ("OpDefinition", ast::OpDefinition::can_cast),
        ("AtomDefinition", ast::AtomDefinition::can_cast),
        ("Query", ast::Query::can_cast),
        ("Annotation", ast::Annotation::can_cast),
        ("Body", ast::Body::can_cast),
        ("Literal", ast::Literal::can_cast),
        ("Atom", ast::Atom::can_cast),
        ("Comparison", ast::Comparison::can_cast),
        ("ConditionalLiteral", ast::ConditionalLiteral::can_cast),
        ("Condition", ast::Condition::can_cast),
        ("Disjunction", ast::Disjunction::can_cast),
        ("FunctionAggregate", ast::FunctionAggregate::can_cast),
        ("SetAggregate", ast::SetAggregate::can_cast),
        ("Guard", ast::Guard::can_cast),
        ("BodyAggregateElement", ast::BodyAggregateElement::can_cast),
        ("HeadAggregateElement", ast::HeadAggregateElement::can_cast),
        ("TheoryAtom", ast::TheoryAtom::can_cast),
        ("TheoryElements", ast::TheoryElements::can_cast),
        ("TheoryElement", ast::TheoryElement::can_cast),
        ("TheoryOpTerm", ast::TheoryOpTerm::can_cast),
        ("TheoryGuard", ast::TheoryGuard::can_cast),
        ("TheorySet", ast::TheorySet::can_cast),
        ("TheoryList", ast::TheoryList::can_cast),
        ("TheoryTuple", ast::TheoryTuple::can_cast),
        ("TheoryFunction", ast::TheoryFunction::can_cast),
        ("BinaryTerm", ast::BinaryTerm::can_cast),
        ("UnaryTerm", ast::UnaryTerm::can_cast),
        ("Pool", ast::Pool::can_cast),
        ("Tuple", ast::Tuple::can_cast),
        ("Arguments", ast::Arguments::can_cast),
        ("FunctionTerm", ast::FunctionTerm::can_cast),
        ("ExternalTerm", ast::ExternalTerm::can_cast),
        ("AbsTerm", ast::AbsTerm::can_cast),
        ("ConstantTerm", ast::ConstantTerm::can_cast),
        ("VariableTerm", ast::VariableTerm::can_cast),
        ("SpliceTerm", ast::SpliceTerm::can_cast),
        ("Error", ast::Error::can_cast),
    ];
    for kind in SyntaxKind::ALL.iter().copied().filter(|kind| kind.is_node()) {
        let casting: Vec<&str> = wrappers.iter().filter(|(_, casts)| casts(kind)).map(|(name, _)| *name).collect();
        assert_eq!(casting.len(), 1, "{kind}: cast by {casting:?}, not by exactly one wrapper");
    }
    for (name, casts) in &wrappers {
        assert!(SyntaxKind::ALL.iter().any(|kind| kind.is_node() && casts(*kind)), "{name} casts no node kind");
    }
}
```

(Fifty-nine: the fifty-eight node kinds after `EOF` in the roster, and
`ERROR`, which is a node kind too.)

Add to `src/ast/mod.rs`, at the end, the accessor tests over parsed
programs (they read the wrappers of `nodes.rs`, so they fail until it
lands):

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;

    fn program(text: &str) -> crate::parse::Parse<Program> {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
    }

    fn first_statement(text: &str) -> Statement {
        program(text).tree().statements().next().expect("a statement")
    }

    fn rule(text: &str) -> Rule {
        match first_statement(text) {
            Statement::Rule(rule) => rule,
            other => panic!("not a rule: {other:?}"),
        }
    }

    #[test]
    fn a_rule_exposes_its_head_neck_body_and_dot() {
        let rule = rule("p(X) :- q(X), not r.");
        assert!(matches!(rule.head(), Some(Head::Literal(_))));
        assert_eq!(rule.neck_token().map(|t| t.text().to_owned()), Some(":-".to_owned()));
        let body = rule.body().expect("a body");
        let elements: Vec<Negation> = body.elements().map(|e| e.negation()).collect();
        assert_eq!(elements, [Negation::None, Negation::Default]);
        assert!(rule.dot_token().is_some());
        assert!(rule.docs_range().is_none());
    }

    #[test]
    fn a_fact_has_no_body_and_a_constraint_no_head() {
        assert!(rule("p.").body().is_none());
        assert!(rule(":- q.").head().is_none());
        assert!(rule("h :- .").body().is_some_and(|body| body.elements().count() == 0));
    }

    #[test]
    fn literals_atoms_and_comparisons() {
        let rule = rule("-p(1) :- 1 < X <= 3, not not #true.");
        let Some(Head::Literal(head)) = rule.head() else { panic!("a literal head") };
        let Some(LiteralInner::Atom(atom)) = head.inner() else { panic!("an atom") };
        assert!(atom.classical_negation_token().is_some());
        assert_eq!(atom.name().map(|n| n.text().to_owned()), Some("p".to_owned()));
        assert_eq!(atom.arguments().expect("arguments").alternatives().count(), 1);
        let body = rule.body().expect("body");
        let mut elements = body.elements();
        let Some(BodyElement::Literal(comparison)) = elements.next() else { panic!("a literal") };
        let Some(LiteralInner::Comparison(comparison)) = comparison.inner() else { panic!("a comparison") };
        assert!(matches!(comparison.first(), Some(Term::Constant(_))));
        let steps: Vec<Relation> = comparison.steps().map(|(relation, _)| relation).collect();
        assert_eq!(steps, [Relation::Lt, Relation::Le]);
        let Some(BodyElement::Literal(truth)) = elements.next() else { panic!("a literal") };
        assert_eq!(truth.negation(), Negation::DoubleDefault);
        assert!(matches!(truth.inner(), Some(LiteralInner::True(_))));
    }

    #[test]
    fn a_chain_is_one_node_whose_associativity_the_ast_states() {
        let rule = rule("p(1 + 2 * 3, 2 ** 3 ** 4).");
        let Some(Head::Literal(head)) = rule.head() else { panic!() };
        let Some(LiteralInner::Atom(atom)) = head.inner() else { panic!() };
        let tuple = atom.arguments().expect("arguments").alternatives().next().expect("a tuple");
        let mut terms = tuple.terms();
        let Some(Term::Binary(additive)) = terms.next() else { panic!("a chain") };
        assert_eq!(additive.level(), Some(Precedence::Additive));
        assert_eq!(additive.associativity(), Some(Associativity::Left));
        assert_eq!(additive.operands().count(), 2);
        assert_eq!(additive.operators().count(), 1);
        let Some(Term::Binary(power)) = terms.next() else { panic!("a chain") };
        assert_eq!(power.level(), Some(Precedence::Exponentiation));
        assert_eq!(power.associativity(), Some(Associativity::Right));
        assert_eq!(power.operands().count(), 3);
    }

    #[test]
    fn pools_keep_the_uniform_shape_and_name_the_parenthesized_case() {
        let rule = rule("p((a), (a,), (a;b)).");
        let Some(Head::Literal(head)) = rule.head() else { panic!() };
        let Some(LiteralInner::Atom(atom)) = head.inner() else { panic!() };
        let tuple = atom.arguments().expect("arguments").alternatives().next().expect("a tuple");
        let pools: Vec<Pool> = tuple.terms().filter_map(|t| match t { Term::Pool(p) => Some(p), _ => None }).collect();
        assert_eq!(pools.len(), 3);
        assert!(pools[0].parenthesized().is_some());
        assert!(pools[1].parenthesized().is_none(), "`(a,)` is a one-element tuple");
        assert!(pools[2].parenthesized().is_none());
        assert_eq!(pools[2].tuples().count(), 2);
    }

    #[test]
    fn aggregates_expose_guards_functions_and_elements() {
        let rule = rule("1 { p(X) : q(X) } 1 :- not 2 <= #sum { W,T : t(T,W) } < 3.");
        let Some(Head::Aggregate(Aggregate::Set(set))) = rule.head() else { panic!("a set aggregate") };
        assert!(set.left_guard().is_some_and(|g| g.relation().is_none()));
        assert!(set.right_guard().is_some());
        assert_eq!(set.elements().count(), 1);
        let Some(BodyElement::Aggregate(Aggregate::Function(sum))) = rule.body().expect("body").elements().next() else {
            panic!("a function aggregate")
        };
        assert_eq!(sum.negation(), Negation::Default);
        assert_eq!(sum.function(), Some(AggregateFunction::Sum));
        assert_eq!(sum.left_guard().and_then(|g| g.relation()), Some(Relation::Le));
        assert_eq!(sum.right_guard().and_then(|g| g.relation()), Some(Relation::Lt));
        let Some(AggregateElement::Body(element)) = sum.elements().next() else { panic!("a body element") };
        assert_eq!(element.terms().count(), 2);
        assert!(element.condition().is_some());
    }

    #[test]
    fn statements_of_every_family_cast_to_their_variants() {
        let text = "p. :~ q. [1@2, x] #minimize { 1 : p }. #show p/1. #project p. #defined p/1. #edge (a,b). #heuristic a. [1,sign] #external p. [true] #const n = 1. [default] #script (lua) x #end. #include \"f\". #program base. #theory t { }.";
        let kinds: Vec<&'static str> = program(text)
            .tree()
            .statements()
            .map(|statement| match statement {
                Statement::Rule(_) => "rule",
                Statement::WeakConstraint(_) => "weak",
                Statement::Optimize(_) => "optimize",
                Statement::Show(_) => "show",
                Statement::Project(_) => "project",
                Statement::Defined(_) => "defined",
                Statement::Edge(_) => "edge",
                Statement::Heuristic(_) => "heuristic",
                Statement::External(_) => "external",
                Statement::Const(_) => "const",
                Statement::Script(_) => "script",
                Statement::Include(_) => "include",
                Statement::ProgramPart(_) => "program",
                Statement::TheoryDefinition(_) => "theory",
                Statement::Query(_) => "query",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "rule", "weak", "optimize", "show", "project", "defined", "edge", "heuristic", "external",
                "const", "script", "include", "program", "theory"
            ]
        );
    }

    #[test]
    fn the_annotations_meanings_live_on_the_statements() {
        let Statement::WeakConstraint(weak) = first_statement(":~ q. [1@2, x, y]") else { panic!() };
        assert!(weak.weight().is_some());
        assert!(weak.priority().is_some());
        assert_eq!(weak.tuple().count(), 2);
        let Statement::Heuristic(heuristic) = first_statement("#heuristic a. [3, sign]") else { panic!() };
        assert!(heuristic.weight().is_some());
        assert!(heuristic.priority().is_none());
        assert!(heuristic.modifier().is_some());
        let Statement::External(external) = first_statement("#external p. [false]") else { panic!() };
        assert!(external.value().is_some());
        let Statement::Const(constant) = first_statement("#const n = 1. [override]") else { panic!() };
        assert_eq!(constant.policy(), Some(ConstPolicy::Override));
        let Statement::Const(constant) = first_statement("#const n = 1.") else { panic!() };
        assert_eq!(constant.policy(), None);
        assert!(constant.annotation().is_none());
    }

    #[test]
    fn docs_are_the_statements_and_the_token_wrappers_read_values() {
        let statement = first_statement("%! one \n%! two\np(\"a\\nb\", 0x1F, X, _).");
        let Statement::Rule(rule) = statement else { panic!() };
        let lines: Vec<String> = rule.doc_lines().map(|line| line.content().to_owned()).collect();
        assert_eq!(lines, [" one ", " two"]);
        assert!(rule.docs_range().is_some());
        let Some(Head::Literal(head)) = rule.head() else { panic!() };
        let Some(LiteralInner::Atom(atom)) = head.inner() else { panic!() };
        let tuple = atom.arguments().expect("arguments").alternatives().next().expect("a tuple");
        let mut terms = tuple.terms();
        let Some(Term::Constant(string)) = terms.next() else { panic!() };
        let Some(Constant::String(string)) = string.constant() else { panic!() };
        assert_eq!(string.value(Dialect::Clingo).expect("a valid literal"), "a\nb");
        assert_eq!(string.value(Dialect::AspCore2).expect("a valid literal"), "a\\nb");
        let Some(Term::Constant(number)) = terms.next() else { panic!() };
        let Some(Constant::Number(number)) = number.constant() else { panic!() };
        assert_eq!(number.radix(), Radix::Hexadecimal);
        assert_eq!(number.digits(), "1F");
        let Some(Term::Variable(variable)) = terms.next() else { panic!() };
        assert!(!variable.variable().expect("a variable").is_anonymous());
        let Some(Term::Variable(anonymous)) = terms.next() else { panic!() };
        assert!(anonymous.variable().expect("a variable").is_anonymous());
    }

    #[test]
    fn the_parse_level_string_door_uses_the_parses_dialect() {
        let source = Source::new(SourceId::new(0), "p(\"a\\nb\").".to_owned()).expect("admits");
        let parse = parse(&source, Dialect::AspCore2);
        let Some(Statement::Rule(rule)) = parse.tree().statements().next() else { panic!() };
        let Some(Head::Literal(head)) = rule.head() else { panic!() };
        let Some(LiteralInner::Atom(atom)) = head.inner() else { panic!() };
        let tuple = atom.arguments().expect("arguments").alternatives().next().expect("a tuple");
        let Some(Term::Constant(string)) = tuple.terms().next() else { panic!() };
        let Some(Constant::String(string)) = string.constant() else { panic!() };
        assert_eq!(parse.string_value(&string).expect("valid"), "a\\nb");
    }

    #[test]
    fn comments_and_script_bodies_read_their_content() {
        let parse = program("p. % trailing  \n#script (lua) x = 1   #end.");
        let root = parse.syntax();
        let comment = root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find_map(Comment::cast)
            .expect("a comment");
        assert_eq!(comment.form(), CommentForm::Line);
        assert_eq!(comment.content(), "% trailing");
        let Some(Statement::Script(script)) = parse.tree().statements().nth(1) else { panic!() };
        assert_eq!(script.language().map(|l| l.text().to_owned()), Some("lua".to_owned()));
        let body = script.body().expect("a body");
        assert_eq!(body.text(), " x = 1   ");
        assert_eq!(body.value(), " x = 1");
        assert!(script.end_token().is_some());
    }

    #[test]
    fn theory_atoms_and_definitions() {
        let rule = rule(":- not &sum(1) { x, -y : p ; {a} } <= 3.");
        let Some(BodyElement::TheoryAtom(atom)) = rule.body().expect("body").elements().next() else { panic!() };
        assert_eq!(atom.negation(), Negation::Default);
        assert_eq!(atom.name().map(|n| n.text().to_owned()), Some("sum".to_owned()));
        assert!(atom.arguments().is_some());
        let elements = atom.elements().expect("elements");
        assert_eq!(elements.elements().count(), 2);
        let first = elements.elements().next().expect("an element");
        assert_eq!(first.opterms().count(), 2);
        assert!(first.condition().is_some());
        let guard = atom.guard().expect("a guard");
        assert_eq!(guard.operator_token().map(|t| t.text().to_owned()), Some("<=".to_owned()));
        assert!(guard.opterm().is_some());
        let Statement::TheoryDefinition(definition) =
            first_statement("#theory t { x { - : 1, unary; + : 0, binary, left }; &a/0 : x, {<=}, x, any }.")
        else {
            panic!()
        };
        assert_eq!(definition.items().count(), 2);
        let Some(TheoryDefItem::Term(term_definition)) = definition.items().next() else { panic!() };
        let ops: Vec<Option<Associativity>> = term_definition.op_definitions().map(|op| op.associativity()).collect();
        assert_eq!(ops, [None, Some(Associativity::Left)]);
    }

    #[test]
    fn the_fragment_roots_answer_their_construct_or_none() {
        let source = Source::new(SourceId::new(0), "  ".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(crate::parse::parse_statement(&lexer).tree().statement().is_none());
        assert!(crate::parse::parse_term(&lexer).tree().term().is_none());
        let source = Source::new(SourceId::new(0), "f(1) + 2".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(crate::parse::parse_term(&lexer).tree().term(), Some(Term::Binary(_))));
        let source = Source::new(SourceId::new(0), "p :- q.".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(crate::parse::parse_statement(&lexer).tree().statement(), Some(Statement::Rule(_))));
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib ast`
Expected: compile errors — the wrappers do not exist.

- [ ] **Step 3: Write the module's spine — enums, traits, helpers, roots**

Replace `src/ast/mod.rs` above its test module with:

```rust
//! The typed AST (docs/design/syntax.md §8): cheap wrappers over the red
//! cursors, one per node kind, an enum per grammar class, accessors
//! mirroring the productions' slots in the productions' order —
//! syntactic accessors, no semantic opinions. Every wrapper is a view:
//! `!Send`, borrowed from the model, positional in its equality. This
//! file holds the wrapper macros, the enums, the traits, and the roots;
//! `nodes` the wrappers, `tokens` the token wrappers and their values.

mod nodes;
mod tokens;

use rowan::ast::support;

use crate::tree::{role, Asp, AstChildren, AstNode, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TokenRole};

pub use self::nodes::{
    AbsTerm, AggregateFunction, Annotation, Arguments, Associativity, Atom, AtomDefinition, BinaryTerm,
    Body, BodyAggregateElement, Comparison, Condition, ConditionalLiteral, ConstPolicy, ConstStatement,
    ConstantTerm, DefinedStatement, Disjunction, Edge, EdgeStatement, Error, ExternalStatement,
    ExternalTerm, FunctionAggregate, FunctionTerm, Guard, HeadAggregateElement, HeuristicStatement,
    IncludeStatement, Literal, Negation, OpDefinition, OptimizeElement, OptimizeStatement, Parameters,
    Pool, Precedence, ProgramStatement, ProjectStatement, Query, Relation, Rule, ScriptStatement,
    SetAggregate, ShowStatement, Signature, SpliceTerm, TermDefinition, TheoryAtom, TheoryDefinition,
    TheoryElement, TheoryElements, TheoryFunction, TheoryGuard, TheoryList, TheoryOpTerm, TheorySet,
    TheoryTuple, Tuple, UnaryTerm, VariableTerm, WeakConstraint,
};
pub use self::tokens::{
    AstToken, Comment, CommentForm, DocLine, Ident, InvalidStringLiteral, NumberLit, Radix, ScriptBody,
    StringLit, Variable,
};

/// Declares one wrapper over one node kind: a view (`!Send`) that casts
/// on the kind, derives its equality and hash positionally through the
/// cursor, and offers `syntax()` as the escape to the tree.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident => $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

/// Declares one enum over node kinds — a grammar class — casting to the
/// first alternative whose kind matches.
macro_rules! ast_enum {
    ($(#[$meta:meta])* $name:ident { $( $(#[$variant_meta:meta])* $variant:ident($inner:ty), )+ }) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant($inner), )+
        }

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                $( <$inner>::can_cast(kind) )||+
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                $(
                    if <$inner>::can_cast(node.kind()) {
                        return <$inner>::cast(node).map(Self::$variant);
                    }
                )+
                None
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $( Self::$variant(inner) => inner.syntax(), )+
                }
            }
        }
    };
}

pub(crate) use {ast_enum, ast_node};

// ---- the helpers every accessor is written with -----------------------

/// The first child that casts to `N`.
pub(crate) fn child<N: AstNode<Language = Asp>>(node: &SyntaxNode) -> Option<N> {
    support::child(node)
}

/// The children that cast to `N`, in order.
pub(crate) fn children<N: AstNode<Language = Asp>>(node: &SyntaxNode) -> AstChildren<N> {
    support::children(node)
}

/// The first child token of `kind`.
pub(crate) fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    support::token(node, kind)
}

/// The child tokens whose kind is among `kinds`, in order.
pub(crate) fn tokens(node: &SyntaxNode, kinds: &'static [SyntaxKind]) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens().filter_map(SyntaxElement::into_token).filter(move |t| kinds.contains(&t.kind()))
}

/// The leading `not` tokens of a node — the negation run every signed
/// element holds inside it (docs/design/syntax.md §8.1).
pub(crate) fn negation_of(node: &SyntaxNode) -> Negation {
    let mut count = 0u32;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_NOT => count += 1,
            SyntaxElement::Token(token) if token.kind().is_trivia() => {}
            _ => break,
        }
    }
    match count {
        0 => Negation::None,
        1 => Negation::Default,
        _ => Negation::DoubleDefault,
    }
}

// ---- the roots --------------------------------------------------------

ast_node! {
    /// Grammar §5.11's `program`: the program entry's root.
    Program => PROGRAM
}

impl Program {
    /// The statements, in order.
    pub fn statements(&self) -> AstChildren<Statement> {
        children(&self.0)
    }
}

ast_node! {
    /// The statement entry's root: leading trivia, the statement when the
    /// input held one, trailing trivia, and an `ERROR` node when input
    /// remained (docs/design/syntax.md §6.1).
    StatementFragment => STATEMENT_FRAGMENT
}

impl StatementFragment {
    /// None when the input held no statement.
    pub fn statement(&self) -> Option<Statement> {
        child(&self.0)
    }
}

ast_node! {
    /// The term and term-value entries' root, of the same shape as the
    /// statement fragment's (docs/design/syntax.md §6.1).
    TermFragment => TERM_FRAGMENT
}

impl TermFragment {
    /// None when the input held no term; `Parse::entry()` says which
    /// restriction the term was read under.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }
}

// ---- the enums, one per grammar class ---------------------------------

ast_enum! {
    /// The forms a program position holds (grammar §5.11, §6.1).
    Statement {
        /// A rule (grammar §5.7).
        Rule(Rule),
        /// A weak constraint.
        WeakConstraint(WeakConstraint),
        /// `#minimize` or `#maximize`.
        Optimize(OptimizeStatement),
        /// `#show`.
        Show(ShowStatement),
        /// `#project`.
        Project(ProjectStatement),
        /// `#defined`.
        Defined(DefinedStatement),
        /// `#edge`.
        Edge(EdgeStatement),
        /// `#heuristic`.
        Heuristic(HeuristicStatement),
        /// `#external`.
        External(ExternalStatement),
        /// `#const`.
        Const(ConstStatement),
        /// `#script`.
        Script(ScriptStatement),
        /// `#include`.
        Include(IncludeStatement),
        /// `#program`: a part (spec §7.1); `Program` is the root.
        ProgramPart(ProgramStatement),
        /// `#theory`.
        TheoryDefinition(TheoryDefinition),
        /// Grammar §6.1's query — outside the grammar's `statement`
        /// class, inside this enum because the enum is the class of forms
        /// a program position holds, and the query holds the last one
        /// under the ASP-Core-2 dialect.
        Query(Query),
    }
}

ast_enum! {
    /// Grammar §5.5's `head`.
    Head {
        /// A literal.
        Literal(Literal),
        /// A disjunction.
        Disjunction(Disjunction),
        /// An aggregate.
        Aggregate(Aggregate),
        /// A theory atom.
        TheoryAtom(TheoryAtom),
    }
}

ast_enum! {
    /// Grammar §5.6's `body-element`.
    BodyElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
        /// An aggregate, signed inside its node.
        Aggregate(Aggregate),
        /// A theory atom, signed inside its node.
        TheoryAtom(TheoryAtom),
    }
}

impl BodyElement {
    /// The element's default-negation prefix (grammar §5.6): the
    /// literal's own for `Literal`; the conditional literal's literal's
    /// for `ConditionalLiteral`; the aggregate's or theory atom's own for
    /// the other two — every variant delegates to its node, whose leading
    /// `not` tokens are inside it. Total; O(leading tokens).
    pub fn negation(&self) -> Negation {
        match self {
            BodyElement::Literal(literal) => literal.negation(),
            BodyElement::ConditionalLiteral(conditional) => {
                conditional.literal().map_or(Negation::None, |literal| literal.negation())
            }
            BodyElement::Aggregate(Aggregate::Function(aggregate)) => aggregate.negation(),
            BodyElement::Aggregate(Aggregate::Set(aggregate)) => aggregate.negation(),
            BodyElement::TheoryAtom(atom) => atom.negation(),
        }
    }
}

ast_enum! {
    /// Grammar §5.3's `aggregate-body`: the function form or the set form.
    Aggregate {
        /// `#count { … }` and its kin.
        Function(FunctionAggregate),
        /// `{ … }`.
        Set(SetAggregate),
    }
}

ast_enum! {
    /// A function aggregate's element: body-position (terms with an
    /// optional condition) or head-position (terms, a literal, an
    /// optional condition) — the parser knows the position and builds
    /// the kind (grammar §5.3).
    AggregateElement {
        /// In a body.
        Body(BodyAggregateElement),
        /// In a head.
        Head(HeadAggregateElement),
    }
}

ast_enum! {
    /// A set aggregate's element: a literal or a conditional literal
    /// (grammar §5.3).
    SetElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
    }
}

ast_enum! {
    /// A disjunction's element: a literal or a conditioned literal
    /// (grammar §5.5).
    DisjunctionElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
    }
}

ast_enum! {
    /// Grammar §5.1's `term`.
    Term {
        /// One precedence level's chain.
        Binary(BinaryTerm),
        /// A run of prefix operators and its operand.
        Unary(UnaryTerm),
        /// `( … )`.
        Pool(Pool),
        /// `f(…)`.
        Function(FunctionTerm),
        /// `@f` or `@f(…)`.
        External(ExternalTerm),
        /// `| … |`.
        Abs(AbsTerm),
        /// A constant.
        Constant(ConstantTerm),
        /// A variable, or the anonymous variable.
        Variable(VariableTerm),
        /// A macro splice.
        Splice(SpliceTerm),
    }
}

ast_enum! {
    /// Grammar §5.8's `theory-term`.
    TheoryTerm {
        /// `{ … }`.
        Set(TheorySet),
        /// `[ … ]`.
        List(TheoryList),
        /// `( … )`.
        Tuple(TheoryTuple),
        /// `f(…)`.
        Function(TheoryFunction),
        /// A constant.
        Constant(ConstantTerm),
        /// A variable.
        Variable(VariableTerm),
        /// A macro splice.
        Splice(SpliceTerm),
    }
}

ast_enum! {
    /// Grammar §5.9's `theory-def-item`: a pure alternation, so no node
    /// of its own.
    TheoryDefItem {
        /// A term definition.
        Term(TermDefinition),
        /// An atom definition.
        Atom(AtomDefinition),
    }
}

/// A literal's inner form (grammar §5.2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum LiteralInner {
    /// `#true`.
    True(SyntaxToken),
    /// `#false`.
    False(SyntaxToken),
    /// An atom.
    Atom(Atom),
    /// A comparison.
    Comparison(Comparison),
}

/// A constant term's constant (grammar §5.1).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Constant {
    /// An identifier.
    Symbol(Ident),
    /// A numeral.
    Number(NumberLit),
    /// A string.
    String(StringLit),
    /// `#inf`.
    Infimum(SyntaxToken),
    /// `#sup`.
    Supremum(SyntaxToken),
}

/// One item of a theory opterm's flat sequence (grammar §5.8): an
/// operator token — `THEORY_OP` or `not` — or a theory term.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TheoryOpTermItem {
    /// An operator.
    Op(SyntaxToken),
    /// A term.
    Term(TheoryTerm),
}

// ---- the traits -------------------------------------------------------

/// Every statement may be documented (grammar §5.11): the leading
/// `DOC_COMMENT` tokens inside the statement's node are its
/// documentation.
pub trait HasDocs: AstNode<Language = Asp> {
    /// The leading DOC_COMMENT tokens, in order — the statement's
    /// documentation. Empty when undocumented. Total; O(leading trivia).
    fn doc_lines(&self) -> impl Iterator<Item = DocLine> {
        self.syntax()
            .children_with_tokens()
            .take_while(|element| match element {
                SyntaxElement::Token(token) => token.kind().is_trivia() || token.kind() == SyntaxKind::DOC_COMMENT,
                SyntaxElement::Node(_) => false,
            })
            .filter_map(SyntaxElement::into_token)
            .filter(|token| role(token) == TokenRole::Documentation)
            .filter_map(DocLine::cast)
    }

    /// The covering range of the documentation, if any. Total.
    fn docs_range(&self) -> Option<TextRange> {
        let mut lines = self.doc_lines();
        let first = lines.next()?;
        let last = lines.last().unwrap_or_else(|| first.clone());
        Some(TextRange::new(first.syntax().text_range().start(), last.syntax().text_range().end()))
    }
}

/// The two aggregate forms take guards on either side (grammar §5.3).
pub trait HasGuards: AstNode<Language = Asp> {
    /// The guard before the aggregate body, if any. Total; O(children).
    fn left_guard(&self) -> Option<Guard> {
        guards(self.syntax()).0
    }

    /// The guard after the aggregate body, if any. Total; O(children).
    fn right_guard(&self) -> Option<Guard> {
        guards(self.syntax()).1
    }
}

/// The guards of an aggregate node: the `GUARD` before its `{` and the
/// one after its `}`.
fn guards(node: &SyntaxNode) -> (Option<Guard>, Option<Guard>) {
    let mut left = None;
    let mut right = None;
    let mut inside_or_after = false;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::L_BRACE => inside_or_after = true,
            SyntaxElement::Node(child) => {
                if let Some(guard) = Guard::cast(child) {
                    if inside_or_after {
                        right = Some(guard);
                    } else {
                        left = Some(guard);
                    }
                }
            }
            SyntaxElement::Token(_) => {}
        }
    }
    (left, right)
}
```

- [ ] **Step 4: Write the wrappers and their accessors**

`crates/themelios-syntax/src/ast/nodes.rs`:

```rust
//! The wrappers over the roster's node kinds and their accessors
//! (docs/design/syntax.md §8.1–§8.2): one per kind, accessors mirroring
//! the production's slots in the production's order — a single child
//! `Option`, a repetition `AstChildren`, a token slot `Option<SyntaxToken>`
//! named for the token, a valued token a typed wrapper.

use crate::tree::{AstChildren, AstNode, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::tokens::{AstToken, Ident, NumberLit, ScriptBody, StringLit, Variable};
use super::{
    ast_node, child, children, negation_of, token, tokens, Aggregate, AggregateElement, BodyElement,
    Constant, DisjunctionElement, HasDocs, HasGuards, Head, LiteralInner, SetElement, Term,
    TheoryDefItem, TheoryOpTermItem, TheoryTerm,
};

/// A literal's or a signed element's default-negation prefix (grammar
/// §5.2, §5.6): none, `not`, or `not not`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Negation {
    /// No prefix.
    None,
    /// `not`.
    Default,
    /// `not not`.
    DoubleDefault,
}

/// Grammar §5.2's `relation`; the spelling stays on the token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Relation {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `=` or `==`
    Eq,
    /// `!=` or `<>`
    Neq,
}

impl Relation {
    fn of(kind: SyntaxKind) -> Option<Relation> {
        Some(match kind {
            SyntaxKind::LT => Relation::Lt,
            SyntaxKind::LE => Relation::Le,
            SyntaxKind::GT => Relation::Gt,
            SyntaxKind::GE => Relation::Ge,
            SyntaxKind::EQ => Relation::Eq,
            SyntaxKind::NEQ => Relation::Neq,
            _ => return None,
        })
    }
}

/// Grammar §5.1's precedence levels, loosest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Precedence {
    /// `..`
    Interval,
    /// `^`
    BitXor,
    /// `?`
    BitOr,
    /// `&`
    BitAnd,
    /// `+` `-`
    Additive,
    /// `*` `/` `\`
    Multiplicative,
    /// `**`
    Exponentiation,
}

impl Precedence {
    fn of(kind: SyntaxKind) -> Option<Precedence> {
        Some(match kind {
            SyntaxKind::DOTDOT => Precedence::Interval,
            SyntaxKind::CARET => Precedence::BitXor,
            SyntaxKind::QUESTION => Precedence::BitOr,
            SyntaxKind::AMPERSAND => Precedence::BitAnd,
            SyntaxKind::PLUS | SyntaxKind::MINUS => Precedence::Additive,
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::BACKSLASH => Precedence::Multiplicative,
            SyntaxKind::STAR_STAR => Precedence::Exponentiation,
            _ => return None,
        })
    }
}

/// The direction a chain folds in: left at every level but
/// exponentiation, right for `**` (grammar §5.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Associativity {
    /// Left.
    Left,
    /// Right.
    Right,
}

/// Grammar §5.3's aggregate functions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateFunction {
    /// `#count`
    Count,
    /// `#sum`
    Sum,
    /// `#sum+`
    SumPlus,
    /// `#min`
    Min,
    /// `#max`
    Max,
}

/// `#const`'s policy word (grammar §5.9).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ConstPolicy {
    /// `[default]`
    Default,
    /// `[override]`
    Override,
}

const OPERATOR_KINDS: [SyntaxKind; 10] = [
    SyntaxKind::DOTDOT,
    SyntaxKind::CARET,
    SyntaxKind::QUESTION,
    SyntaxKind::AMPERSAND,
    SyntaxKind::PLUS,
    SyntaxKind::MINUS,
    SyntaxKind::STAR,
    SyntaxKind::SLASH,
    SyntaxKind::BACKSLASH,
    SyntaxKind::STAR_STAR,
];
const UNARY_KINDS: [SyntaxKind; 2] = [SyntaxKind::MINUS, SyntaxKind::TILDE];
const RELATION_KINDS: [SyntaxKind; 6] =
    [SyntaxKind::LT, SyntaxKind::LE, SyntaxKind::GT, SyntaxKind::GE, SyntaxKind::EQ, SyntaxKind::NEQ];
const SEPARATOR_KINDS: [SyntaxKind; 3] = [SyntaxKind::COMMA, SyntaxKind::SEMICOLON, SyntaxKind::PIPE];
const THEORY_OPERATOR_KINDS: [SyntaxKind; 2] = [SyntaxKind::THEORY_OP, SyntaxKind::KW_NOT];

/// The weight, the priority, and the tuple of a weighted term list —
/// `term [ "@" term ] [ "," terms ]` — read from `node`'s children up
/// to a colon or the closing bracket.
struct Weighted {
    weight: Option<Term>,
    priority: Option<Term>,
    tuple: Vec<Term>,
}

fn weighted(node: &SyntaxNode) -> Weighted {
    let mut weighted = Weighted { weight: None, priority: None, tuple: Vec::new() };
    let mut after_at = false;
    let mut after_comma = false;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) => match token.kind() {
                SyntaxKind::AT => after_at = true,
                SyntaxKind::COMMA => after_comma = true,
                SyntaxKind::COLON | SyntaxKind::R_BRACKET => break,
                _ => {}
            },
            SyntaxElement::Node(child) => {
                let Some(term) = Term::cast(child) else { continue };
                if after_comma {
                    weighted.tuple.push(term);
                } else if after_at {
                    weighted.priority = Some(term);
                } else if weighted.weight.is_none() {
                    weighted.weight = Some(term);
                }
            }
        }
    }
    weighted
}

// ---- statements ---------------------------------------------------------

ast_node! {
    /// Grammar §5.7's `rule`, all five forms.
    Rule => RULE
}

impl HasDocs for Rule {}

impl Rule {
    /// None for a constraint.
    pub fn head(&self) -> Option<Head> {
        child(&self.0)
    }

    /// The `:-`, when the rule has one.
    pub fn neck_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::NECK)
    }

    /// None for a fact; `Some` of an empty body for `h :- .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.7's `weak-constraint`.
    WeakConstraint => WEAK_CONSTRAINT
}

impl HasDocs for WeakConstraint {}

impl WeakConstraint {
    /// The `:~`.
    pub fn weak_neck_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::WEAK_NECK)
    }

    /// The body; None for `:~ .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The bracketed annotation after the dot.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The weight — reads into the annotation.
    pub fn weight(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).weight)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).priority)
    }

    /// The optional term tuple after the weight and priority, in order.
    pub fn tuple(&self) -> impl Iterator<Item = Term> {
        self.annotation().map(|a| weighted(a.syntax()).tuple).unwrap_or_default().into_iter()
    }
}

ast_node! {
    /// Grammar §5.7's `optimize-statement`; the keyword token says which.
    OptimizeStatement => OPTIMIZE_STATEMENT
}

impl HasDocs for OptimizeStatement {}

impl OptimizeStatement {
    /// `#minimize` or `#maximize`, either spelling.
    pub fn keyword_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::KW_MINIMIZE, SyntaxKind::KW_MAXIMIZE]).next()
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<OptimizeElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.7's `optimize-element`.
    OptimizeElement => OPTIMIZE_ELEMENT
}

impl OptimizeElement {
    /// The weight.
    pub fn weight(&self) -> Option<Term> {
        weighted(&self.0).weight
    }

    /// The `@`.
    pub fn at_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::AT)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        weighted(&self.0).priority
    }

    /// The optional term tuple after the weight and priority, in order.
    pub fn tuple(&self) -> impl Iterator<Item = Term> {
        weighted(&self.0).tuple.into_iter()
    }

    /// The `:` before the condition.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.9's `show-statement`, all four forms; the children say
    /// which.
    ShowStatement => SHOW_STATEMENT
}

impl HasDocs for ShowStatement {}

impl ShowStatement {
    /// The `#show`.
    pub fn show_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::KW_SHOW)
    }

    /// The signature, in the signature form.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The term, in the term forms.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }

    /// The `:` before the body.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body, in the conditioned term form.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `signature`: `[-] IDENTIFIER / NUMBER`.
    Signature => SIGNATURE
}

impl Signature {
    /// The `-`, when the signature is classically negated.
    pub fn classical_negation_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MINUS)
    }

    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `/`.
    pub fn slash_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SLASH)
    }

    /// The arity.
    pub fn arity(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }
}

ast_node! {
    /// Grammar §5.9's `project-statement`: the signature form or the atom
    /// form with its conditional dot.
    ProjectStatement => PROJECT_STATEMENT
}

impl HasDocs for ProjectStatement {}

impl ProjectStatement {
    /// The signature, in the signature form.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The atom, in the atom form.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot; empty for `: .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `defined-statement`.
    DefinedStatement => DEFINED_STATEMENT
}

impl HasDocs for DefinedStatement {}

impl DefinedStatement {
    /// The signature.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `edge-statement`.
    EdgeStatement => EDGE_STATEMENT
}

impl HasDocs for EdgeStatement {}

impl EdgeStatement {
    /// The `(`.
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_PAREN)
    }

    /// The edges, in order.
    pub fn edges(&self) -> AstChildren<Edge> {
        children(&self.0)
    }

    /// The `)`.
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_PAREN)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// One `term "," term` pair of grammar §5.9's `edges`.
    Edge => EDGE
}

impl Edge {
    /// The first term.
    pub fn from(&self) -> Option<Term> {
        children::<Term>(&self.0).next()
    }

    /// The `,`.
    pub fn comma_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COMMA)
    }

    /// The second term.
    pub fn to(&self) -> Option<Term> {
        children::<Term>(&self.0).nth(1)
    }
}

ast_node! {
    /// Grammar §5.9's `heuristic-statement`.
    HeuristicStatement => HEURISTIC_STATEMENT
}

impl HasDocs for HeuristicStatement {}

impl HeuristicStatement {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The bracketed annotation, mandatory for this family.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The weight — reads into the annotation.
    pub fn weight(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).weight)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).priority)
    }

    /// The modifier — the term after the comma.
    pub fn modifier(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).tuple.into_iter().next())
    }
}

ast_node! {
    /// Grammar §5.9's `external-statement`.
    ExternalStatement => EXTERNAL_STATEMENT
}

impl HasDocs for ExternalStatement {}

impl ExternalStatement {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The optional bracketed annotation.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The value inside the annotation, if any.
    pub fn value(&self) -> Option<Term> {
        self.annotation().and_then(|a| child(a.syntax()))
    }
}

ast_node! {
    /// Grammar §5.9's `const-statement`; its term under the constant
    /// restriction.
    ConstStatement => CONST_STATEMENT
}

impl HasDocs for ConstStatement {}

impl ConstStatement {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `=`.
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::EQ)
    }

    /// The value.
    pub fn value(&self) -> Option<Term> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The optional bracketed annotation.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// `[default]` or `[override]` (grammar §5.9), read by spelling.
    pub fn policy(&self) -> Option<ConstPolicy> {
        let annotation = self.annotation()?;
        let word = tokens(annotation.syntax(), &[SyntaxKind::IDENT]).next()?;
        match word.text() {
            "default" => Some(ConstPolicy::Default),
            "override" => Some(ConstPolicy::Override),
            _ => None,
        }
    }
}

ast_node! {
    /// A `#script` statement, parsed because the shared syntax has it
    /// (grammar §4.8, §5.9) and carried as opaque text: this crate never
    /// runs, parses, or privileges an embedded script — themelios's own
    /// extension language is Rust (spec §9.6), and an embedded script is
    /// the clingo-world compatibility path, executed only by a backend
    /// that declares the capability, never by themelios.
    ScriptStatement => SCRIPT_STATEMENT
}

impl HasDocs for ScriptStatement {}

impl ScriptStatement {
    /// The named language, as written — an identifier the grammar does
    /// not restrict; what a backend accepts is admission, above.
    pub fn language(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `SCRIPT_BODY` token: the raw region, exact span — what a tool
    /// that handles the region hands to that language's own tooling.
    /// None when the region is empty (`#end` directly after the
    /// parenthesis) or missing under recovery; `end_token` tells the two
    /// apart.
    pub fn body(&self) -> Option<ScriptBody> {
        tokens(&self.0, &[SyntaxKind::SCRIPT_BODY]).find_map(ScriptBody::cast)
    }

    /// The `#end`.
    pub fn end_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::KW_END)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `include-statement`.
    IncludeStatement => INCLUDE_STATEMENT
}

impl HasDocs for IncludeStatement {}

impl IncludeStatement {
    /// The path, in the string form.
    pub fn path(&self) -> Option<StringLit> {
        tokens(&self.0, &[SyntaxKind::STRING]).find_map(StringLit::cast)
    }

    /// The library name, in the angle form.
    pub fn library(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `program-statement`.
    ProgramStatement => PROGRAM_STATEMENT
}

impl HasDocs for ProgramStatement {}

impl ProgramStatement {
    /// The part's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The parameter list, if any.
    pub fn parameters(&self) -> Option<Parameters> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// `"(" [ id-list ] ")"` of a program statement (grammar §5.9).
    Parameters => PARAMETERS
}

impl Parameters {
    /// The parameter names, in order.
    pub fn names(&self) -> impl Iterator<Item = Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast)
    }
}

ast_node! {
    /// Grammar §5.9's `theory-definition`.
    TheoryDefinition => THEORY_DEFINITION
}

impl HasDocs for TheoryDefinition {}

impl TheoryDefinition {
    /// The theory's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The items, term and atom definitions interleaved as written.
    pub fn items(&self) -> AstChildren<TheoryDefItem> {
        children(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `term-definition`.
    TermDefinition => TERM_DEFINITION
}

impl TermDefinition {
    /// The term type's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The operator definitions, in order.
    pub fn op_definitions(&self) -> AstChildren<OpDefinition> {
        children(&self.0)
    }
}

ast_node! {
    /// Grammar §5.9's `op-definition`.
    OpDefinition => OP_DEFINITION
}

impl OpDefinition {
    /// The operator token — a `THEORY_OP` or `not`.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS).next()
    }

    /// The priority.
    pub fn priority(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }

    /// The word `unary` or `binary`.
    pub fn arity_token(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The word `left` or `right`, for a binary operator.
    pub fn associativity_token(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast).nth(1)
    }

    /// The associativity read from its word; None for a unary operator
    /// or a missing word.
    pub fn associativity(&self) -> Option<Associativity> {
        match self.associativity_token()?.text() {
            "left" => Some(Associativity::Left),
            "right" => Some(Associativity::Right),
            _ => None,
        }
    }
}

ast_node! {
    /// Grammar §5.9's `atom-definition`.
    AtomDefinition => ATOM_DEFINITION
}

impl AtomDefinition {
    /// The atom's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arity.
    pub fn arity(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }

    /// The term type after the colon.
    pub fn type_name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast).nth(1)
    }

    /// The guard operators between the braces, in order.
    pub fn guard_operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS)
    }

    /// The guard's term type, when the guard part is present.
    pub fn guard_type_name(&self) -> Option<Ident> {
        let words: Vec<Ident> = tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast).collect();
        if words.len() >= 4 { words.get(2).cloned() } else { None }
    }

    /// The occurrence word: `head`, `body`, `any`, or `directive`.
    pub fn occurrence(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast).last()
    }
}

ast_node! {
    /// Grammar §6.1's `query` (ASP-Core-2 dialect).
    Query => QUERY
}

impl HasDocs for Query {}

impl Query {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `?`.
    pub fn question_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::QUESTION)
    }
}

ast_node! {
    /// The bracketed annotation after the dot of the four families
    /// (grammar §5.11); its meaning is read by the statement's accessors.
    Annotation => ANNOTATION
}

impl Annotation {
    /// The `[`.
    pub fn l_bracket_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACKET)
    }

    /// The terms inside, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The `]`.
    pub fn r_bracket_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACKET)
    }
}

// ---- rule interiors -----------------------------------------------------

ast_node! {
    /// Grammar §5.6's `body-list`; also the empty body of `h :- .` and `: .`.
    Body => BODY
}

impl Body {
    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<BodyElement> {
        children(&self.0)
    }

    /// The `,` and `;` between elements, in order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::COMMA, SyntaxKind::SEMICOLON])
    }
}

ast_node! {
    /// Grammar §5.2's `literal`: negation tokens and one of `#true`,
    /// `#false`, an atom, a comparison.
    Literal => LITERAL
}

impl Literal {
    /// The default-negation prefix.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The inner form; None under recovery.
    pub fn inner(&self) -> Option<LiteralInner> {
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_TRUE => {
                    return Some(LiteralInner::True(token));
                }
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_FALSE => {
                    return Some(LiteralInner::False(token));
                }
                SyntaxElement::Node(node) => {
                    if let Some(atom) = Atom::cast(node.clone()) {
                        return Some(LiteralInner::Atom(atom));
                    }
                    if let Some(comparison) = Comparison::cast(node) {
                        return Some(LiteralInner::Comparison(comparison));
                    }
                }
                SyntaxElement::Token(_) => {}
            }
        }
        None
    }
}

ast_node! {
    /// Grammar §5.2's `atom`.
    Atom => ATOM
}

impl Atom {
    /// The `-` of classical negation.
    pub fn classical_negation_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MINUS)
    }

    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments, if any.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.2's `comparison`, the whole chain.
    Comparison => COMPARISON
}

impl Comparison {
    /// The first term.
    pub fn first(&self) -> Option<Term> {
        children::<Term>(&self.0).next()
    }

    /// The chain after the first term: each step a relation and its
    /// right term (grammar §5.2's guard sequence); the term is None
    /// under recovery.
    pub fn steps(&self) -> impl Iterator<Item = (Relation, Option<Term>)> {
        let mut steps: Vec<(Relation, Option<Term>)> = Vec::new();
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) => {
                    if let Some(relation) = Relation::of(token.kind()) {
                        steps.push((relation, None));
                    }
                }
                SyntaxElement::Node(node) => {
                    if let (Some(term), Some(last)) = (Term::cast(node), steps.last_mut()) {
                        if last.1.is_none() {
                            last.1 = Some(term);
                        }
                    }
                }
            }
        }
        steps.into_iter()
    }
}

ast_node! {
    /// Grammar §5.4's `conditional-literal`, and every `literal ":" [condition]`
    /// shape.
    ConditionalLiteral => CONDITIONAL_LITERAL
}

impl ConditionalLiteral {
    /// The literal.
    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `condition`; present and empty when the colon is.
    Condition => CONDITION
}

impl Condition {
    /// The literals, in order.
    pub fn literals(&self) -> AstChildren<Literal> {
        children(&self.0)
    }
}

ast_node! {
    /// Grammar §5.5's `disjunction`; separators as tokens.
    Disjunction => DISJUNCTION
}

impl Disjunction {
    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<DisjunctionElement> {
        children(&self.0)
    }

    /// The `;`, `|`, and `,` between elements, in order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &SEPARATOR_KINDS)
    }
}

ast_node! {
    /// Grammar §5.3's `function-aggregate` with its guards as `GUARD`
    /// children, and in body position its leading negation tokens.
    FunctionAggregate => FUNCTION_AGGREGATE
}

impl HasGuards for FunctionAggregate {}

impl FunctionAggregate {
    /// The leading `not` tokens in body position (grammar §5.6);
    /// `Negation::None` in head position, where none can stand.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The function.
    pub fn function(&self) -> Option<AggregateFunction> {
        let keyword = tokens(
            &self.0,
            &[SyntaxKind::KW_COUNT, SyntaxKind::KW_SUM, SyntaxKind::KW_SUM_PLUS, SyntaxKind::KW_MIN, SyntaxKind::KW_MAX],
        )
        .next()?;
        Some(match keyword.kind() {
            SyntaxKind::KW_COUNT => AggregateFunction::Count,
            SyntaxKind::KW_SUM => AggregateFunction::Sum,
            SyntaxKind::KW_SUM_PLUS => AggregateFunction::SumPlus,
            SyntaxKind::KW_MIN => AggregateFunction::Min,
            _ => AggregateFunction::Max,
        })
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order — body-shaped or head-shaped, as the
    /// parser built them.
    pub fn elements(&self) -> AstChildren<AggregateElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.3's `set-aggregate` with its guards, and in body
    /// position its leading negation tokens.
    SetAggregate => SET_AGGREGATE
}

impl HasGuards for SetAggregate {}

impl SetAggregate {
    /// The leading `not` tokens in body position; `Negation::None` in
    /// head position.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// Elements are literals or conditional literals (grammar §5.3).
    pub fn elements(&self) -> impl Iterator<Item = SetElement> {
        children::<SetElement>(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.3's `lguard` / `rguard`.
    Guard => GUARD
}

impl Guard {
    /// None means the grammar's default relation for its side (grammar
    /// §5.3): stated as absence, because that is what the author wrote.
    pub fn relation(&self) -> Option<Relation> {
        tokens(&self.0, &RELATION_KINDS).next().and_then(|t| Relation::of(t.kind()))
    }

    /// The guard's term.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `fn-element` in body position.
    BodyAggregateElement => BODY_AGGREGATE_ELEMENT
}

impl BodyAggregateElement {
    /// The term tuple, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `fn-element` in head position.
    HeadAggregateElement => HEAD_AGGREGATE_ELEMENT
}

impl HeadAggregateElement {
    /// The term tuple, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The first `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The literal after the first colon.
    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }

    /// The `:` before the condition.
    pub fn second_colon_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::COLON]).nth(1)
    }

    /// The condition, present and empty when its colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

// ---- theory atoms -------------------------------------------------------

ast_node! {
    /// Grammar §5.8's `theory-atom`, and in body position its leading
    /// negation tokens.
    TheoryAtom => THEORY_ATOM
}

impl TheoryAtom {
    /// The negation, in body position only (grammar §5.6).
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The name after `&`.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The name's arguments, if any.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }

    /// The elements between the braces, if the atom has them.
    pub fn elements(&self) -> Option<TheoryElements> {
        child(&self.0)
    }

    /// The guard after the elements, if any.
    pub fn guard(&self) -> Option<TheoryGuard> {
        child(&self.0)
    }
}

ast_node! {
    /// `"{" [ theory-elements ] "}"` (grammar §5.8).
    TheoryElements => THEORY_ELEMENTS
}

impl TheoryElements {
    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<TheoryElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.8's `theory-element`.
    TheoryElement => THEORY_ELEMENT
}

impl TheoryElement {
    /// The opterms before the colon, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.8's `theory-opterm`, flat.
    TheoryOpTerm => THEORY_OPTERM
}

impl TheoryOpTerm {
    /// Operators and terms in the flat sequence grammar §5.8 admits;
    /// regrouping under a `#theory` definition is admission, above.
    pub fn items(&self) -> impl Iterator<Item = TheoryOpTermItem> {
        self.0.children_with_tokens().filter_map(|element| match element {
            SyntaxElement::Token(token) if THEORY_OPERATOR_KINDS.contains(&token.kind()) => {
                Some(TheoryOpTermItem::Op(token))
            }
            SyntaxElement::Token(_) => None,
            SyntaxElement::Node(node) => TheoryTerm::cast(node).map(TheoryOpTermItem::Term),
        })
    }
}

ast_node! {
    /// `theory-op theory-opterm` after the elements (grammar §5.8).
    TheoryGuard => THEORY_GUARD
}

impl TheoryGuard {
    /// The guard's operator.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS).next()
    }

    /// The guard's opterm.
    pub fn opterm(&self) -> Option<TheoryOpTerm> {
        child(&self.0)
    }
}

ast_node! {
    /// `"{" [ theory-opterms ] "}"` (grammar §5.8).
    TheorySet => THEORY_SET
}

impl TheorySet {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

ast_node! {
    /// `"[" [ theory-opterms ] "]"` (grammar §5.8).
    TheoryList => THEORY_LIST
}

impl TheoryList {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

ast_node! {
    /// The parenthesized theory-term forms (grammar §5.8): `()`, `(a)`,
    /// `(a,)`, `(a, b)`.
    TheoryTuple => THEORY_TUPLE
}

impl TheoryTuple {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }

    /// The trailing comma of `(a,)`.
    pub fn trailing_comma_token(&self) -> Option<SyntaxToken> {
        let mut last_comma = None;
        let mut term_after = false;
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::COMMA => {
                    last_comma = Some(token);
                    term_after = false;
                }
                SyntaxElement::Node(_) => term_after = true,
                SyntaxElement::Token(_) => {}
            }
        }
        if term_after { None } else { last_comma }
    }
}

ast_node! {
    /// `IDENTIFIER "(" [ theory-opterms ] ")"` (grammar §5.8).
    TheoryFunction => THEORY_FUNCTION
}

impl TheoryFunction {
    /// The function's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

// ---- terms --------------------------------------------------------------

ast_node! {
    /// One precedence level's chain, flat (docs/design/syntax.md §6.2):
    /// `1 + 2 - 3` is one node of three operands and two operators; a
    /// tighter level is an operand.
    BinaryTerm => BINARY_TERM
}

impl BinaryTerm {
    /// The operands in source order — at least two when well formed;
    /// under recovery one may be missing.
    pub fn operands(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The operator tokens in source order — one fewer than the operands.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &OPERATOR_KINDS)
    }

    /// The chain's level, read from its first operator (grammar §5.1).
    pub fn level(&self) -> Option<Precedence> {
        self.operators().next().and_then(|t| Precedence::of(t.kind()))
    }

    /// Left at every level but exponentiation, right for `**` — the
    /// grammar's fact, carried here so no consumer re-derives it.
    pub fn associativity(&self) -> Option<Associativity> {
        self.level().map(|level| match level {
            Precedence::Exponentiation => Associativity::Right,
            _ => Associativity::Left,
        })
    }
}

ast_node! {
    /// A run of prefix operators and its one operand, flat: `- - x` is
    /// one node; unary binds tighter than every binary level (grammar §5.1).
    UnaryTerm => UNARY_TERM
}

impl UnaryTerm {
    /// The prefix operators, outermost first.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &UNARY_KINDS)
    }

    /// The operand.
    pub fn operand(&self) -> Option<Term> {
        child(&self.0)
    }
}

ast_node! {
    /// `( … )`: tuples separated by `;` (grammar §5.1).
    Pool => POOL
}

impl Pool {
    /// The tuples, in order.
    pub fn tuples(&self) -> AstChildren<Tuple> {
        children(&self.0)
    }

    /// `(a)` — exactly one tuple of one term with no trailing comma — is
    /// the term `a` parenthesized (grammar §5.1); this names it.
    pub fn parenthesized(&self) -> Option<Term> {
        let mut tuples = self.tuples();
        let tuple = tuples.next()?;
        if tuples.next().is_some() || tuple.trailing_comma_token().is_some() {
            return None;
        }
        let mut terms = tuple.terms();
        let term = terms.next()?;
        if terms.next().is_some() { None } else { Some(term) }
    }
}

ast_node! {
    /// Grammar §5.1's `tuple`, and each `[ terms ]` alternative of
    /// `arguments`.
    Tuple => TUPLE
}

impl Tuple {
    /// The terms, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The trailing comma, when the tuple has one.
    pub fn trailing_comma_token(&self) -> Option<SyntaxToken> {
        let mut last_comma = None;
        let mut term_after = false;
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::COMMA => {
                    last_comma = Some(token);
                    term_after = false;
                }
                SyntaxElement::Node(_) => term_after = true,
                SyntaxElement::Token(_) => {}
            }
        }
        if term_after { None } else { last_comma }
    }
}

ast_node! {
    /// `"(" arguments ")"` of a function, an atom, or an external call.
    Arguments => ARGUMENTS
}

impl Arguments {
    /// The `(`.
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_PAREN)
    }

    /// The pooled alternatives, in order — one for `f(a,b)`, several for
    /// `f(a;b)`, an empty one for `f()`.
    pub fn alternatives(&self) -> AstChildren<Tuple> {
        children(&self.0)
    }

    /// The `)`.
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_PAREN)
    }
}

ast_node! {
    /// `IDENTIFIER "(" arguments ")"` (grammar §5.1).
    FunctionTerm => FUNCTION_TERM
}

impl FunctionTerm {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// `@name` and `@name(args)` — the syntax themelios's Rust
    /// `@`-functions answer to (spec §9.6); the `@` and the name are
    /// separate tokens (grammar §5.1).
    ExternalTerm => EXTERNAL_TERM
}

impl ExternalTerm {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments; None for the bare `@name`.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// `"|" abs-arguments "|"` (grammar §5.1).
    AbsTerm => ABS_TERM
}

impl AbsTerm {
    /// The pooled arguments, in order — one for `|X|`, several for `|X;Y|`.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }
}

ast_node! {
    /// `IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"` as a term.
    ConstantTerm => CONSTANT_TERM
}

impl ConstantTerm {
    /// The constant.
    pub fn constant(&self) -> Option<Constant> {
        let token = self.0.children_with_tokens().filter_map(SyntaxElement::into_token).find(|t| !t.kind().is_trivia())?;
        Some(match token.kind() {
            SyntaxKind::IDENT => Constant::Symbol(Ident::cast(token)?),
            SyntaxKind::NUMBER => Constant::Number(NumberLit::cast(token)?),
            SyntaxKind::STRING => Constant::String(StringLit::cast(token)?),
            SyntaxKind::KW_INF => Constant::Infimum(token),
            SyntaxKind::KW_SUP => Constant::Supremum(token),
            _ => return None,
        })
    }
}

ast_node! {
    /// `VARIABLE | ANONYMOUS` as a term.
    VariableTerm => VARIABLE_TERM
}

impl VariableTerm {
    /// The variable, or the anonymous variable.
    pub fn variable(&self) -> Option<Variable> {
        tokens(&self.0, &[SyntaxKind::VARIABLE, SyntaxKind::ANONYMOUS]).find_map(Variable::cast)
    }
}

ast_node! {
    /// A splice in term or theory-term position (grammar §9).
    SpliceTerm => SPLICE_TERM
}

impl SpliceTerm {
    /// The splice token.
    pub fn splice_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SPLICE)
    }
}

ast_node! {
    /// Skipped or refused input, byte-preserved (docs/design/syntax.md
    /// §6.6, §6.7).
    Error => ERROR
}
```

- [ ] **Step 5: Write the token wrappers and their values**

`crates/themelios-syntax/src/ast/tokens.rs`:

```rust
//! Typed tokens over the valued kinds — rowan's `AstToken` idiom
//! (docs/design/syntax.md §8.3) — and the values they carry: numeral
//! radix and digits, string values under the dialect, doc and comment
//! content, the script body's raw text and value.

use std::fmt;

use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::tree::{offset_of, role, SyntaxKind, SyntaxToken, TokenRole};

/// A typed token: a view over one token, cast by kind — and, for the
/// two comment wrappers, by the token's role.
pub trait AstToken: Sized {
    /// Whether tokens of `kind` may cast (the role, where it matters, is
    /// read at `cast`).
    fn can_cast(kind: SyntaxKind) -> bool;
    /// The wrapper over `token`, when it is of the wrapper's kind and role.
    fn cast(token: SyntaxToken) -> Option<Self>;
    /// The token.
    fn syntax(&self) -> &SyntaxToken;
    /// The token's text.
    fn text(&self) -> &str {
        self.syntax().text()
    }
}

macro_rules! ast_token {
    ($(#[$meta:meta])* $name:ident, $($kind:ident)|+) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxToken);

        impl AstToken for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, $(SyntaxKind::$kind)|+)
            }

            fn cast(token: SyntaxToken) -> Option<Self> {
                if Self::can_cast(token.kind()) { Some(Self(token)) } else { None }
            }

            fn syntax(&self) -> &SyntaxToken {
                &self.0
            }
        }
    };
}

ast_token! {
    /// An identifier.
    Ident, IDENT
}

ast_token! {
    /// A variable, or the anonymous variable.
    Variable, VARIABLE | ANONYMOUS
}

impl Variable {
    /// Whether this is the anonymous variable `_`.
    pub fn is_anonymous(&self) -> bool {
        self.0.kind() == SyntaxKind::ANONYMOUS
    }
}

ast_token! {
    /// A numeral (grammar §4.3).
    NumberLit, NUMBER
}

/// A numeral's radix, from its prefix.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Radix {
    /// No prefix.
    Decimal,
    /// `0x`
    Hexadecimal,
    /// `0o`
    Octal,
    /// `0b`
    Binary,
}

impl NumberLit {
    /// The radix, from the prefix; total, syntactic.
    pub fn radix(&self) -> Radix {
        match self.0.text().get(..2) {
            Some("0x") => Radix::Hexadecimal,
            Some("0o") => Radix::Octal,
            Some("0b") => Radix::Binary,
            _ => Radix::Decimal,
        }
    }

    /// The text after the prefix; total.
    pub fn digits(&self) -> &str {
        match self.radix() {
            Radix::Decimal => self.0.text(),
            _ => &self.0.text()[2..],
        }
    }
}

ast_token! {
    /// A string literal (grammar §4.4, §6.2).
    StringLit, STRING
}

/// A string token whose spelling is not the dialect's rule, which only a
/// token source other than the file lexer can supply; `at` is where the
/// spelling breaks, in the source's coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidStringLiteral {
    /// Where the spelling breaks.
    pub at: ByteOffset,
}

impl fmt::Display for InvalidStringLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the string literal's spelling breaks the dialect's rule at byte {}", self.at.get())
    }
}

impl std::error::Error for InvalidStringLiteral {}

impl StringLit {
    /// The denoted text with the dialect's escapes resolved (grammar
    /// §4.4, §6.2). The dialect is the caller's to state, because the
    /// tree does not carry it — and the caller must state it right:
    /// `"a\nb"` denotes differently under the two rules and a wrong
    /// dialect here yields a plausible wrong `String`, not a refusal, so
    /// a consumer holding the `Parse` uses `Parse::string_value` and
    /// takes the dialect from it. Refuses with `InvalidStringLiteral`
    /// only a token whose spelling is not the dialect's string rule,
    /// which a token source other than the file lexer can supply; the
    /// file lexer's tokens never refuse. O(token).
    pub fn value(&self, dialect: Dialect) -> Result<String, InvalidStringLiteral> {
        let text = self.0.text();
        let start = offset_of(self.0.text_range().start());
        let refuse = |index: usize| InvalidStringLiteral {
            at: ByteOffset::new(start.get() + u32::try_from(index).unwrap_or(u32::MAX)),
        };
        if !text.starts_with('"') || text.len() < 2 || !text.ends_with('"') {
            return Err(refuse(text.len()));
        }
        let inner = &text[1..text.len() - 1];
        let mut value = String::with_capacity(inner.len());
        let mut chars = inner.char_indices().peekable();
        while let Some((index, c)) = chars.next() {
            if c != '\\' {
                if dialect == Dialect::Clingo && (c == '"' || c == '\n') {
                    return Err(refuse(index + 1));
                }
                value.push(c);
                continue;
            }
            match dialect {
                Dialect::Clingo => match chars.next() {
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, 'n')) => value.push('\n'),
                    _ => return Err(refuse(index + 1)),
                },
                Dialect::AspCore2 => {
                    // `\"` is the one escape when a quote follows inside the
                    // literal; the backslash before the closing quote of a
                    // `"…\"`-final literal is itself (grammar §6.2).
                    if chars.peek().is_some_and(|(_, next)| *next == '"') {
                        chars.next();
                        value.push('"');
                    } else {
                        value.push('\\');
                    }
                }
            }
        }
        Ok(value)
    }
}

/// A `DOC_COMMENT` in docs position: a statement's documentation — the
/// cast reads `role`, not the kind alone (docs/design/syntax.md §5.4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DocLine(SyntaxToken);

impl AstToken for DocLine {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DOC_COMMENT
    }

    fn cast(token: SyntaxToken) -> Option<Self> {
        if Self::can_cast(token.kind()) && role(&token) == TokenRole::Documentation {
            Some(DocLine(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl DocLine {
    /// The text after the `%!` marker, untrimmed — comment text whose
    /// meaning is a tool's (grammar §8), trailing whitespace included: a
    /// documentation tool may read it (two trailing spaces are a hard
    /// break in more than one markup), so it is content here and in the
    /// certificates, never layout.
    pub fn content(&self) -> &str {
        &self.0.text()[2..]
    }
}

/// The comment forms.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CommentForm {
    /// `% …`
    Line,
    /// `%* … *%`
    Block,
    /// `#! …`
    Shebang,
    /// `%! …` outside docs position.
    Doc,
}

/// A trivia comment: `LINE_COMMENT`, `BLOCK_COMMENT`, or `SHEBANG_COMMENT`
/// anywhere, or a `DOC_COMMENT` whose role is `Trivia` — the cast reads
/// `role`, not the kind alone (docs/design/syntax.md §5.4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Comment(SyntaxToken);

impl AstToken for Comment {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind.is_comment()
    }

    fn cast(token: SyntaxToken) -> Option<Self> {
        if Self::can_cast(token.kind()) && role(&token) == TokenRole::Trivia { Some(Comment(token)) } else { None }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl Comment {
    /// The comment's content: for the line comment and the shebang, the
    /// text minus its trailing horizontal whitespace, since that
    /// whitespace is layout the rule swallowed on its way to the line
    /// end; for a doc comment in trivia position, the whole token text —
    /// the doc form's trailing whitespace is content wherever the token
    /// stands; for a block comment, the whole token text. This is what
    /// the certificates compare.
    pub fn content(&self) -> &str {
        match self.form() {
            CommentForm::Line | CommentForm::Shebang => self.0.text().trim_end_matches([' ', '\t', '\r']),
            CommentForm::Block | CommentForm::Doc => self.0.text(),
        }
    }

    /// The form.
    pub fn form(&self) -> CommentForm {
        match self.0.kind() {
            SyntaxKind::LINE_COMMENT => CommentForm::Line,
            SyntaxKind::BLOCK_COMMENT => CommentForm::Block,
            SyntaxKind::SHEBANG_COMMENT => CommentForm::Shebang,
            _ => CommentForm::Doc,
        }
    }
}

ast_token! {
    /// The `SCRIPT_BODY` token (grammar §4.8).
    ScriptBody, SCRIPT_BODY
}

impl ScriptBody {
    /// The region's value per grammar §4.8: the raw text with trailing
    /// blanks and tabs trimmed before `#end`.
    pub fn value(&self) -> &str {
        self.0.text().trim_end_matches([' ', '\t'])
    }
}
```

Add to `src/parse/mod.rs`, in `impl<T: AstNode<Language = Asp>> Parse<T>`:

```rust
    /// The denoted text of a string literal, under this parse's dialect
    /// — the door that cannot be handed the wrong one
    /// (docs/design/syntax.md §3). Refuses as `StringLit::value` does: a
    /// spelling that is not the dialect's rule, which only a foreign
    /// token source can supply. O(token).
    pub fn string_value(&self, literal: &ast::StringLit) -> Result<String, ast::InvalidStringLiteral> {
        literal.value(self.dialect)
    }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p themelios-syntax --lib ast && cargo test -p themelios-syntax --test ast_completeness`
Expected: 13 passed in the module, 1 in the completeness test.

- [ ] **Step 7: Run the full gate, then commit**

Run the four gate commands. Expected: green; every public item carries
its doc (the wrappers' docs are the macro's `$meta`), and any pedantic
lint on the long `match` in `Statement`'s test is repaired in code.

```bash
git add crates/themelios-syntax
git commit -m "Add the typed AST: a wrapper per node kind, an enum per grammar class, the docs and guards traits, and the token wrappers with their values"
```

---

### Task 14: The `attach` module — the policy as a total function, its two forms, the whitespace facts, and the scar goldens

**Files:**
- Create: `crates/themelios-syntax/src/attach.rs`,
  `crates/themelios-syntax/tests/attach_laws.rs`,
  `crates/themelios-syntax/tests/golden/attachments/*` (blessed)
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod attach;`),
  `crates/themelios-syntax/tests/golden.rs` (the attachment dumps),
  `crates/themelios-syntax/fuzz/fuzz_targets/parse.rs` (every trivia
  comment attaches)

**Derives:** syntax.md §5.1 (an `Attachment` is a view), §5.4 (the role
of a token; the empty node), §9 whole (§9.1 what attachment is, §9.2
the policy and its four facts, §9.3 the two forms, the whitespace
facts, the costs, why a function and not a table), §12.5
(`NotAttachable: Display + Error`), §13, §16 (attachment laws; the
kallos scar corpus and the CRLF golden); spec §5.1, §6.4.

**Interfaces:**
- Consumes: `tree::{role, TokenRole, SyntaxKind, SyntaxNode,
  SyntaxToken, SyntaxElement, NodeOrToken, WalkEvent}`.
- Produces: `attach::{Slot, Attachment, NotAttachable, attachment,
  comments, attachments, same_line, empty_line_between,
  line_breaks_between}`.

- [ ] **Step 1: Write the failing tests**

Append `pub mod attach;` to `src/lib.rs`. Create `src/attach.rs`
holding only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::{AstNode, SyntaxKind};

    fn root(text: &str) -> SyntaxNode {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo).syntax()
    }

    /// The trivia comments under `root`, in order.
    fn trivia_comments(root: &SyntaxNode) -> Vec<SyntaxToken> {
        root.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind().is_comment() && role(t) == TokenRole::Trivia)
            .collect()
    }

    /// `slot anchor-kind` for the comment's attachment.
    fn describe(comment: &SyntaxToken) -> String {
        match attachment(comment) {
            Ok(Attachment { anchor, slot }) => format!("{slot:?} {}", anchor.kind()),
            Err(refusal) => format!("{refusal:?}"),
        }
    }

    #[test]
    fn a_comment_on_the_line_of_a_rules_dot_trails_the_rule() {
        let root = root("p. % trailing\nq.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Trailing RULE");
    }

    #[test]
    fn a_comment_on_its_own_line_leads_what_follows_unless_a_blank_line_or_a_closer_stands_between() {
        let root = root("% leading\np.\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading RULE");
        let root = root("% a\n\n% b\np.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Dangling PROGRAM");
        assert_eq!(describe(&comments[1]), "Leading RULE");
        let root = root("p(1\n % c\n).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Dangling ARGUMENTS");
    }

    #[test]
    fn a_comment_before_a_comma_leads_the_comma_and_after_it_trails_it() {
        let root = root("p(1\n % c\n , 2).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading COMMA");
        let root = root("p(1, % c\n 2).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Trailing COMMA");
        let root = root("p(1 % c\n , 2).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Trailing CONSTANT_TERM");
    }

    #[test]
    fn the_pipe_is_a_separator_in_a_disjunction_and_a_closer_in_an_absolute_value() {
        let root = root("a\n% c\n| b.\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Leading PIPE");
        let root = root("p(|X\n% c\n|).\n");
        assert_eq!(describe(&trivia_comments(&root)[0]), "Dangling ABS_TERM");
    }

    #[test]
    fn a_multi_line_block_comment_between_prev_and_the_comment_breaks_the_line() {
        let root = root("p. %* a\nb *% % c\nq.\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Trailing RULE");
        assert_eq!(describe(&comments[1]), "Leading RULE");
    }

    #[test]
    fn documentation_and_significant_tokens_are_refused() {
        let root = root("%! doc\np. %! stray\n");
        let tokens: Vec<SyntaxToken> = root.descendants_with_tokens().filter_map(|e| e.into_token()).collect();
        let doc = tokens.iter().find(|t| t.text() == "%! doc").expect("the doc line");
        assert_eq!(attachment(doc), Err(NotAttachable::Documentation));
        let stray = tokens.iter().find(|t| t.text() == "%! stray").expect("the stray line");
        assert_eq!(describe(stray), "Trailing RULE");
        let dot = tokens.iter().find(|t| t.kind() == SyntaxKind::DOT).expect("a dot");
        assert_eq!(attachment(dot), Err(NotAttachable::NotAComment { kind: SyntaxKind::DOT }));
        assert_eq!(
            NotAttachable::NotAComment { kind: SyntaxKind::DOT }.to_string(),
            "the token is DOT, not a comment"
        );
    }

    #[test]
    fn crlf_empty_lines_detach_exactly_as_lf_ones() {
        let root = root("% a\r\n\r\n% b\r\np.\r\n");
        let comments = trivia_comments(&root);
        assert_eq!(describe(&comments[0]), "Dangling PROGRAM");
        assert_eq!(describe(&comments[1]), "Leading RULE");
    }

    #[test]
    fn the_two_forms_agree_and_the_bulk_form_yields_every_comment_once() {
        let root = root("% lead\np(1, % after comma\n 2). % trail\n\n% dangling\n");
        let all: Vec<(SyntaxToken, Attachment)> = attachments(&root).collect();
        assert_eq!(all.len(), 4);
        for (comment, att) in &all {
            assert_eq!(attachment(comment).as_ref(), Ok(att));
            let back: Vec<SyntaxToken> = comments(&att.anchor, att.slot).collect();
            assert!(back.contains(comment), "the inverse form yields {}", comment.text());
        }
        let program = SyntaxElement::Node(root.clone());
        let dangling: Vec<String> = comments(&program, Slot::Dangling).map(|t| t.text().to_owned()).collect();
        assert_eq!(dangling, ["% dangling"]);
    }

    #[test]
    fn the_whitespace_facts() {
        let root = root("p(1,\n\n 2). q.\n");
        let tokens: Vec<SyntaxToken> = root.descendants_with_tokens().filter_map(|e| e.into_token()).collect();
        let comma = tokens.iter().find(|t| t.kind() == SyntaxKind::COMMA).expect("a comma");
        let two = tokens.iter().find(|t| t.text() == "2").expect("the 2");
        let one = tokens.iter().find(|t| t.text() == "1").expect("the 1");
        let a = SyntaxElement::Token(comma.clone());
        let b = SyntaxElement::Token(two.clone());
        assert!(!same_line(&a, &b));
        assert!(same_line(&SyntaxElement::Token(one.clone()), &a));
        assert!(empty_line_between(&a, &b));
        assert_eq!(line_breaks_between(&a, &b), 2);
        let rules: Vec<SyntaxNode> = root.children().collect();
        let first = SyntaxElement::Node(rules[0].clone());
        let second = SyntaxElement::Node(rules[1].clone());
        assert!(same_line(&first, &second));
        assert!(!empty_line_between(&first, &second));
        assert!(!empty_line_between(&SyntaxElement::Token(one.clone()), &b), "a token stands between");
        let _ = crate::ast::Program::cast(root);
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib attach`
Expected: compile error — the module's items do not exist.

- [ ] **Step 3: Write the module**

Prepend to `src/attach.rs`:

```rust
//! Comment attachment, the owned policy (docs/design/syntax.md §9): a
//! pure reading of the tree, a function of exactly four facts, shipped
//! in two forms that agree by law — never a table, since this tree
//! carries every comment in place and nothing can go stale.

use std::collections::VecDeque;
use std::fmt;

use crate::tree::{role, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole, WalkEvent};

/// The slot a comment is attached in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot {
    /// On its own line(s) directly above its anchor.
    Leading,
    /// On its anchor's line, after it.
    Trailing,
    /// Inside its parent, attached to nothing nearer.
    Dangling,
}

/// One comment's attachment: the element it belongs to and how. The
/// anchor is a node or a significant token — a comment before `,` leads
/// the comma, which is what keeps it before the comma when a consumer
/// re-emits (kallos's transposition scar, spec §5.1); a comment on the
/// line of a rule's dot trails the rule. A view, not data: the anchor is
/// a cursor, which is the shape a formatter holding the tree wants — it
/// navigates from the anchor directly — and it lives no longer than the
/// tree it reads.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Attachment {
    /// The element the comment belongs to.
    pub anchor: SyntaxElement,
    /// How.
    pub slot: Slot,
}

/// Why a token has no attachment: it is not a comment, or it is a doc
/// line in docs position — structure the statement owns
/// (docs/design/syntax.md §5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotAttachable {
    /// A significant token, or whitespace.
    NotAComment {
        /// Its kind.
        kind: SyntaxKind,
    },
    /// A statement's documentation.
    Documentation,
}

impl fmt::Display for NotAttachable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotAttachable::NotAComment { kind } => write!(f, "the token is {kind}, not a comment"),
            NotAttachable::Documentation => f.write_str("the token is a statement's documentation, not a comment"),
        }
    }
}

impl std::error::Error for NotAttachable {}

/// The line break (base §5's newline policy: a `\r` is content of its
/// line).
const LINE_BREAK: char = '\n';

/// Whether `token` is a trivia comment: a comment by kind whose role is
/// `Trivia` (docs/design/syntax.md §5.4).
fn is_trivia_comment(token: &SyntaxToken) -> bool {
    token.kind().is_comment() && role(token) == TokenRole::Trivia
}

/// Whether `element` is skipped when looking for `prev` and `next`:
/// trivia, or an empty node (docs/design/syntax.md §5.4, §9.2).
fn is_skipped(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => role(token) == TokenRole::Trivia,
        NodeOrToken::Node(node) => node.text_range().is_empty(),
    }
}

/// Whether `element` is a closer: a token that ends a construct rather
/// than begins an element — `)`, `]`, `}`, `.`, or the `|` of an
/// absolute value; the `|` of a disjunction is a separator and an
/// anchor like `;` (spec §6.4's dual-role-token carve-out, decided
/// structurally).
fn is_closer(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => match token.kind() {
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE | SyntaxKind::DOT => true,
            SyntaxKind::PIPE => token.parent().is_some_and(|parent| parent.kind() == SyntaxKind::ABS_TERM),
            _ => false,
        },
        NodeOrToken::Node(_) => false,
    }
}

/// Whether the text of `element` holds a line break — a whitespace run
/// or a multi-line block comment.
fn breaks_line(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => token.text().contains(LINE_BREAK),
        NodeOrToken::Node(node) => node.text().contains_char(LINE_BREAK),
    }
}

/// Whether `element` is an empty line: a `WHITESPACE` token containing
/// two line breaks with only horizontal whitespace between them — a run
/// with at least two line breaks, since a run holds nothing but
/// whitespace (docs/design/syntax.md §9.2).
fn is_empty_line(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => {
            token.kind() == SyntaxKind::WHITESPACE && token.text().matches(LINE_BREAK).count() >= 2
        }
        NodeOrToken::Node(_) => false,
    }
}

/// A comment's attachment. Refuses a token that is not a trivia comment
/// — a doc line in docs position (structure) or any significant token —
/// with the reason. Total otherwise; O(the trivia between `prev` and
/// `next` around the comment), allocation-free.
pub fn attachment(comment: &SyntaxToken) -> Result<Attachment, NotAttachable> {
    if !comment.kind().is_comment() {
        return Err(NotAttachable::NotAComment { kind: comment.kind() });
    }
    if role(comment) == TokenRole::Documentation {
        return Err(NotAttachable::Documentation);
    }
    let parent = comment.parent().expect("a token in a tree has a parent");
    // Rule 1: trailing — `prev` exists and no line break stands between
    // its end and the comment's start.
    let mut broken = false;
    let mut cursor = comment.prev_sibling_or_token();
    let mut prev = None;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            prev = Some(element);
            break;
        }
        broken |= breaks_line(&element);
        cursor = element.prev_sibling_or_token();
    }
    if let Some(prev) = prev {
        if !broken {
            return Ok(Attachment { anchor: prev, slot: Slot::Trailing });
        }
    }
    // Rule 2: leading — `next` exists, is not a closer, and no empty
    // line stands in the run from the comment to it.
    let mut gap = false;
    let mut cursor = comment.next_sibling_or_token();
    let mut next = None;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            next = Some(element);
            break;
        }
        gap |= is_empty_line(&element);
        cursor = element.next_sibling_or_token();
    }
    if let Some(next) = next {
        if !gap && !is_closer(&next) {
            return Ok(Attachment { anchor: next, slot: Slot::Leading });
        }
    }
    // Rule 3: dangling in the parent — total, since every comment has one.
    Ok(Attachment { anchor: NodeOrToken::Node(parent), slot: Slot::Dangling })
}

/// Every trivia comment among `parent`'s children with its attachment,
/// in order, in one pass: the rules read as cumulative facts along the
/// children — a line break since the last significant sibling, an empty
/// line before the next — so each comment resolves in constant time.
fn resolve_children(parent: &SyntaxNode) -> Vec<(SyntaxToken, Attachment)> {
    let elements: Vec<SyntaxElement> = parent.children_with_tokens().collect();
    let count = elements.len();
    // Forward: the nearest significant sibling before each element and
    // whether a line break stands between it and the element.
    let mut prev_of: Vec<Option<usize>> = Vec::with_capacity(count);
    let mut broken_before: Vec<bool> = Vec::with_capacity(count);
    let mut last_significant = None;
    let mut broken = false;
    for element in &elements {
        prev_of.push(last_significant);
        broken_before.push(broken);
        if is_skipped(element) {
            broken |= breaks_line(element);
        } else {
            last_significant = Some(prev_of.len() - 1);
            broken = false;
        }
    }
    // Backward: the nearest significant sibling after each element and
    // whether an empty line stands between the element and it.
    let mut next_of: Vec<Option<usize>> = vec![None; count];
    let mut gap_after: Vec<bool> = vec![false; count];
    let mut next_significant = None;
    let mut gap = false;
    for (index, element) in elements.iter().enumerate().rev() {
        next_of[index] = next_significant;
        gap_after[index] = gap;
        if is_skipped(element) {
            gap |= is_empty_line(element);
        } else {
            next_significant = Some(index);
            gap = false;
        }
    }
    let mut out = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        let NodeOrToken::Token(token) = element else { continue };
        if !is_trivia_comment(token) {
            continue;
        }
        let attachment = match (prev_of[index], broken_before[index]) {
            (Some(prev), false) => Attachment { anchor: elements[prev].clone(), slot: Slot::Trailing },
            _ => match next_of[index] {
                Some(next) if !gap_after[index] && !is_closer(&elements[next]) => {
                    Attachment { anchor: elements[next].clone(), slot: Slot::Leading }
                }
                _ => Attachment { anchor: NodeOrToken::Node(parent.clone()), slot: Slot::Dangling },
            },
        };
        out.push((token.clone(), attachment));
    }
    out
}

/// The comments attached to `anchor` in `slot`, in source order — the
/// inverse direction, for a consumer walking anchors. Total; O(the
/// trivia adjacent to the anchor) for `Leading` and `Trailing`, O(the
/// anchor's children) for `Dangling`.
pub fn comments(anchor: &SyntaxElement, slot: Slot) -> impl Iterator<Item = SyntaxToken> {
    let found: Vec<SyntaxToken> = match slot {
        Slot::Trailing => trailing(anchor),
        Slot::Leading => leading(anchor),
        Slot::Dangling => match anchor {
            NodeOrToken::Node(node) => resolve_children(node)
                .into_iter()
                .filter(|(_, attachment)| attachment.slot == Slot::Dangling)
                .map(|(comment, _)| comment)
                .collect(),
            NodeOrToken::Token(_) => Vec::new(),
        },
    };
    found.into_iter()
}

/// The comments trailing `anchor`: the trivia comments after it, up to
/// the first line break.
fn trailing(anchor: &SyntaxElement) -> Vec<SyntaxToken> {
    let mut found = Vec::new();
    let mut cursor = anchor.next_sibling_or_token();
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            break;
        }
        if let NodeOrToken::Token(token) = &element {
            if is_trivia_comment(token) {
                found.push(token.clone());
            }
        }
        if breaks_line(&element) {
            break;
        }
        cursor = element.next_sibling_or_token();
    }
    found
}

/// The comments leading `anchor`: the trivia comments in the run before
/// it — back to the previous significant sibling — that trail nothing
/// (no `prev`, or a line break between `prev` and the comment) and
/// stand after the run's last empty line; none when the anchor is a
/// closer.
fn leading(anchor: &SyntaxElement) -> Vec<SyntaxToken> {
    if is_closer(anchor) {
        return Vec::new();
    }
    let mut run: Vec<SyntaxElement> = Vec::new();
    let mut cursor = anchor.prev_sibling_or_token();
    let mut prev_exists = false;
    while let Some(element) = cursor {
        if !is_skipped(&element) {
            prev_exists = true;
            break;
        }
        run.push(element.clone());
        cursor = element.prev_sibling_or_token();
    }
    run.reverse();
    let after_gap = run.iter().rposition(is_empty_line).map_or(0, |gap| gap + 1);
    let mut found = Vec::new();
    // Rule 1 cannot hold where no `prev` exists; where one does, it holds
    // for every comment until the first line break after `prev`.
    let mut not_trailing = !prev_exists;
    for (index, element) in run.iter().enumerate() {
        if let NodeOrToken::Token(token) = element {
            if is_trivia_comment(token) && not_trailing && index >= after_gap {
                found.push(token.clone());
            }
        }
        not_trailing |= breaks_line(element);
    }
    found
}

/// Every trivia comment under `node` with its attachment, in source
/// order, computed in one pass — the bulk form. Total; O(subtree).
pub fn attachments(node: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, Attachment)> {
    let mut out = Vec::new();
    // Per open node, its comments' attachments, resolved once, consumed
    // in token order as the walk meets them.
    let mut open: Vec<VecDeque<(SyntaxToken, Attachment)>> = Vec::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(inner)) => open.push(resolve_children(&inner).into_iter().collect()),
            WalkEvent::Leave(NodeOrToken::Node(_)) => {
                open.pop();
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                if is_trivia_comment(&token) {
                    if let Some(resolved) = open.last_mut().and_then(VecDeque::pop_front) {
                        out.push(resolved);
                    }
                }
            }
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out.into_iter()
}

/// The elements strictly between `a` and `b` among their siblings, in
/// order — empty when `b` does not follow `a` among one node's children.
fn between(a: &SyntaxElement, b: &SyntaxElement) -> Vec<SyntaxElement> {
    let mut found = Vec::new();
    let mut cursor = a.next_sibling_or_token();
    while let Some(element) = cursor {
        if element == *b {
            return found;
        }
        found.push(element.clone());
        cursor = element.next_sibling_or_token();
    }
    Vec::new()
}

/// No line break in the text between `a`'s end and `b`'s start. Total;
/// O(the trivia between the two elements).
pub fn same_line(a: &SyntaxElement, b: &SyntaxElement) -> bool {
    !between(a, b).iter().any(breaks_line)
}

/// An empty line in the whitespace directly between `a` and `b`; false
/// when anything but whitespace — a token, a node, a comment — stands
/// between them, so a non-adjacent pair answers false rather than
/// refusing. Total; O(the trivia between the two elements).
pub fn empty_line_between(a: &SyntaxElement, b: &SyntaxElement) -> bool {
    let middle = between(a, b);
    let whitespace_only = middle
        .iter()
        .all(|element| matches!(element, NodeOrToken::Token(token) if token.kind() == SyntaxKind::WHITESPACE));
    whitespace_only && middle.iter().any(is_empty_line)
}

/// The count of line breaks in the trivia between `a`'s end and `b`'s
/// start. Total; O(the trivia between the two elements).
pub fn line_breaks_between(a: &SyntaxElement, b: &SyntaxElement) -> u32 {
    between(a, b)
        .iter()
        .map(|element| match element {
            NodeOrToken::Token(token) => token.text().matches(LINE_BREAK).count(),
            NodeOrToken::Node(node) => node.text().to_string().matches(LINE_BREAK).count(),
        })
        .sum::<usize>()
        .try_into()
        .unwrap_or(u32::MAX)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p themelios-syntax --lib attach`
Expected: 9 passed.

- [ ] **Step 5: Write the attachment laws**

`crates/themelios-syntax/tests/attach_laws.rs`:

```rust
//! Attachment totality, single-valuedness, the inverse law between the
//! two forms, and stability under re-spacing that preserves the four
//! facts (docs/design/syntax.md §9.2, §16), over the corpus and over
//! generated re-spacings.

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::attach::{attachment, attachments, comments, Slot};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{role, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole};

fn corpus_texts() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                found.push((path.strip_prefix(&dir).expect("under corpus").display().to_string(), text));
            }
        }
    }
    found.sort();
    found
}

fn root(text: &str) -> SyntaxNode {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, Dialect::Clingo).syntax()
}

fn trivia_comments(root: &SyntaxNode) -> Vec<SyntaxToken> {
    root.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind().is_comment() && role(t) == TokenRole::Trivia)
        .collect()
}

/// A comparable record of one attachment: the comment's text, the slot,
/// the anchor's kind and text — everything but positions.
fn record(root: &SyntaxNode) -> Vec<(String, Slot, SyntaxKind, String)> {
    attachments(root)
        .map(|(comment, attachment)| {
            (
                comment.text().to_owned(),
                attachment.slot,
                attachment.anchor.kind(),
                attachment.anchor.to_string(),
            )
        })
        .collect()
}

#[test]
fn every_trivia_comment_of_the_corpus_attaches_once_and_the_two_forms_agree() {
    for (name, text) in corpus_texts() {
        let root = root(&text);
        let comments_in_tree = trivia_comments(&root);
        let bulk: Vec<_> = attachments(&root).collect();
        assert_eq!(bulk.len(), comments_in_tree.len(), "{name}: the bulk form yields each trivia comment once");
        for ((comment, bulk_attachment), in_tree) in bulk.iter().zip(&comments_in_tree) {
            assert_eq!(comment, in_tree, "{name}: in source order");
            let single = attachment(comment).expect("a trivia comment attaches");
            assert_eq!(&single, bulk_attachment, "{name}: the two forms agree on {}", comment.text());
            assert!(
                comments(&single.anchor, single.slot).any(|c| &c == comment),
                "{name}: the inverse form yields {}",
                comment.text()
            );
        }
    }
}

/// A whitespace token's text re-spaced within its class: no line break
/// stays without one; one line break stays with exactly one; two or
/// more stay with two or more — the facts the policy reads, kept.
fn respace(text: &str, choice: u8) -> String {
    let breaks = text.matches('\n').count();
    let horizontal = match choice % 3 {
        0 => " ",
        1 => "\t",
        _ => "  ",
    };
    match breaks {
        0 => horizontal.to_owned(),
        1 => format!("{horizontal}\n{horizontal}"),
        _ => format!("\n{horizontal}\n\n"),
    }
}

/// The text with every whitespace token re-spaced by the choices.
fn respaced(root: &SyntaxNode, choices: &[u8]) -> String {
    let mut out = String::new();
    let mut next_choice = choices.iter().copied().cycle();
    for token in root.descendants_with_tokens().filter_map(|e| e.into_token()) {
        if token.kind() == SyntaxKind::WHITESPACE {
            out.push_str(&respace(token.text(), next_choice.next().unwrap_or(0)));
        } else {
            out.push_str(token.text());
        }
    }
    out
}

const SCAR_TEXT: &str = "% lead\np(1, % c1\n  2 % c2\n , 3). % t\n\n% dangling above a gap\n\n%* block\nacross *% q :- r. % end\n";

proptest! {
    #[test]
    fn re_spacing_that_keeps_the_four_facts_keeps_every_attachment(choices in prop::collection::vec(0u8..3, 1..16)) {
        for text in [SCAR_TEXT, "% a\n%* b *%\np. % t\n% l\nq(X, %c\n Y).\n"] {
            let before = root(text);
            let after = root(&respaced(&before, &choices));
            prop_assert_eq!(record(&before), record(&after));
        }
    }
}

#[test]
fn violating_a_fact_changes_exactly_the_attachments_that_read_it() {
    // Joining a leading comment onto the previous line makes it trailing.
    let before = record(&root("p.\n% lead\nq.\n"));
    let after = record(&root("p. % lead\nq.\n"));
    assert_eq!(before[0].1, Slot::Leading);
    assert_eq!(after[0].1, Slot::Trailing);
    // Opening an empty line inside a leading run detaches the comments above it.
    let before = record(&root("% a\n% b\np.\n"));
    let after = record(&root("% a\n\n% b\np.\n"));
    assert_eq!((before[0].1, before[1].1), (Slot::Leading, Slot::Leading));
    assert_eq!((after[0].1, after[1].1), (Slot::Dangling, Slot::Leading));
}
```

- [ ] **Step 6: Add the attachment goldens and the fuzz check**

Append to `tests/golden.rs`:

```rust
// ---- attachment dumps: kallos's scars and the CRLF input --------------

fn attachment_dump(text: &str) -> String {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    let mut out = String::new();
    for (comment, attachment) in themelios_syntax::attach::attachments(&parse.syntax()) {
        out.push_str(&format!(
            "{:?} {:?} -> {:?} {}@{:?} {:?}\n",
            comment.text_range(),
            comment.text(),
            attachment.slot,
            attachment.anchor.kind(),
            attachment.anchor.text_range(),
            attachment.anchor.to_string(),
        ));
    }
    out
}

#[test]
fn attachments_kallos_scar_corpus() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/kallos");
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the kallos corpus")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "lp"))
        .collect();
    names.sort();
    for path in names {
        let text = fs::read_to_string(&path).expect("input reads");
        let stem = path.file_stem().expect("a name").to_string_lossy().into_owned();
        check("attachments", &format!("kallos-{stem}"), &attachment_dump(&text));
    }
}

#[test]
fn attachments_transposition_dual_role_and_blank_line_detach() {
    check(
        "attachments",
        "scars",
        &attachment_dump("p(1, % after comma\n   % before two\n 2). % trailing\n\n% above gap\n\n% leads q\nq :- a\n  % leads pipe\n  | b. r(|X\n % dangling in abs\n |).\n"),
    );
}

#[test]
fn attachments_crlf() {
    check("attachments", "crlf", &attachment_dump("% a\r\n\r\n% b\r\np. % t\r\nq :- % in body\r\n  r.\r\n"));
}
```

In `fuzz/fuzz_targets/parse.rs`, extend `holds` (after the depth
assertion):

```rust
    let root = parse.syntax();
    let mut trivia_comments = 0usize;
    for token in root.descendants_with_tokens().filter_map(|e| e.into_token()) {
        if token.kind().is_comment() && themelios_syntax::tree::role(&token) == themelios_syntax::tree::TokenRole::Trivia {
            trivia_comments += 1;
            assert!(themelios_syntax::attach::attachment(&token).is_ok());
        }
    }
    assert_eq!(themelios_syntax::attach::attachments(&root).count(), trivia_comments);
```

- [ ] **Step 7: Bless, review, run**

Run: `GOLDEN_BLESS=1 cargo test -p themelios-syntax --test golden attachments`,
read the dumps under `tests/golden/attachments/` against §9.2 (each
comment's slot follows from the four facts; the transposition, the
dual-role `|`, and the blank-line detach read as spec §5.1's scars say),
then `cargo test -p themelios-syntax --test golden --test attach_laws`.
Expected: green. Record the acceptance in the commit message.

- [ ] **Step 8: Run the full gate, then commit**

Run the four gate commands, and `cargo fuzz build -s none` from the
crate directory. Expected: green.

```bash
git add crates/themelios-syntax
git commit -m "Add comment attachment: the policy as a total function of four facts, both forms, the whitespace facts, and the scar and CRLF goldens"
```

---

### Task 15: The `fusion` module over tokens — `separator`, `lex_mode_of`, and the mode law

**Files:**
- Modify: `crates/themelios-syntax/src/fusion.rs` (`separator`,
  `lex_mode_of`), `crates/themelios-syntax/fuzz/fuzz_targets/parse.rs`
  (the mode law under fuzz)
- Create: `crates/themelios-syntax/tests/oracle_laws.rs`

**Derives:** syntax.md §4.2 (the modes the parser requests), §6.3 (the
regions), §10 (`separator`, `lex_mode_of`), §10.1 (the whole-text
lemma), §10.2 (the mode of an adjacency; the law binding the parser's
rule and the reconstruction; the grammar's named cases gain `;-` after
a condition, `#end.`, `#end .`), §16 (the oracle laws; the mode law
held by a parse-time recording of the requested modes compared against
the reconstruction over the whole corpus).

**Interfaces:**
- Consumes: `fusion::separator_between`, `tree::{SyntaxToken,
  SyntaxKind}`, `token::LexMode`, the corpus.
- Produces: `fusion::separator(&SyntaxToken, &SyntaxToken, Dialect) ->
  Separator`, `fusion::lex_mode_of(&SyntaxToken) -> LexMode`.

- [ ] **Step 1: Write the failing tests**

Append to the test module of `src/fusion.rs`:

```rust
    use themelios_base::source::{Source, SourceId};

    use crate::parse::parse;
    use crate::tree::{SyntaxKind, SyntaxToken};

    fn tokens(text: &str) -> Vec<SyntaxToken> {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .collect()
    }

    fn mode_of(text: &str, token_text: &str, nth: usize) -> LexMode {
        let tokens = tokens(text);
        let token = tokens.iter().filter(|t| t.text() == token_text).nth(nth).expect("the token");
        lex_mode_of(token)
    }

    #[test]
    fn the_modes_are_reconstructed_from_the_tree() {
        let text = ":- &sum(1) { x, -y : p((a;b)) ; z } <= 3, not q. #script (lua) x #end. #theory t { x { - : 1, unary }; &a/0 : x, {<=}, x, any }.";
        assert_eq!(mode_of(text, "&", 0), LexMode::Normal);
        assert_eq!(mode_of(text, "sum", 0), LexMode::Normal);
        assert_eq!(mode_of(text, "1", 0), LexMode::Normal, "the arguments lex in normal mode");
        assert_eq!(mode_of(text, "{", 0), LexMode::Normal, "the brace that opens the elements");
        assert_eq!(mode_of(text, "x", 0), LexMode::Theory);
        assert_eq!(mode_of(text, "-", 0), LexMode::Theory);
        assert_eq!(mode_of(text, ":", 0), LexMode::Theory, "the condition-opening colon");
        assert_eq!(mode_of(text, "p", 0), LexMode::Normal, "inside the condition");
        assert_eq!(mode_of(text, ";", 0), LexMode::Normal, "the pool's `;` inside the condition");
        assert_eq!(mode_of(text, ";", 1), LexMode::Normal, "the `;` that ends the condition");
        assert_eq!(mode_of(text, "z", 0), LexMode::Theory, "theory mode resumes after that `;`");
        assert_eq!(mode_of(text, "}", 0), LexMode::Theory, "the `}` after an element without a condition");
        assert_eq!(mode_of(text, "<=", 0), LexMode::Theory, "the guard");
        assert_eq!(mode_of(text, "3", 0), LexMode::Theory);
        assert_eq!(mode_of(text, ",", 1), LexMode::Normal, "the first token after the guard");
        assert_eq!(mode_of(text, "not", 0), LexMode::Normal);
        assert_eq!(mode_of(text, " x ", 0), LexMode::ScriptBody);
        assert_eq!(mode_of(text, "#end", 0), LexMode::ScriptBody);
        assert_eq!(mode_of(text, "-", 1), LexMode::Theory, "the operator position of an op-definition");
        assert_eq!(mode_of(text, "<=", 1), LexMode::Theory, "the operator position of an atom-definition");
        assert_eq!(mode_of(text, "unary", 0), LexMode::Normal);
    }

    #[test]
    fn a_closing_brace_after_a_condition_is_normal_and_the_named_cases_answer() {
        assert_eq!(mode_of("&a { x : p }.", "}", 0), LexMode::Normal);
        let tokens = tokens("&a { x : p ; -y }.");
        let semicolon = tokens.iter().find(|t| t.kind() == SyntaxKind::SEMICOLON).expect(";");
        let minus = tokens.iter().find(|t| t.kind() == SyntaxKind::THEORY_OP).expect("-");
        assert_eq!(separator(semicolon, minus, Dialect::Clingo), Separator::Nothing, "`;-` after a condition");
        let tokens = tokens("#script (lua) x #end.");
        let end = tokens.iter().find(|t| t.kind() == SyntaxKind::KW_END).expect("#end");
        let dot = tokens.iter().find(|t| t.kind() == SyntaxKind::DOT).expect(".");
        assert_eq!(separator(end, dot, Dialect::Clingo), Separator::Nothing, "`#end.`");
    }

    #[test]
    fn the_token_form_reads_the_mode_from_the_left_token() {
        let tokens = tokens("&a { x < = y }.");
        let lt = tokens.iter().find(|t| t.text() == "<").expect("<");
        let eq = tokens.iter().find(|t| t.text() == "=").expect("=");
        assert_eq!(separator(lt, eq, Dialect::Clingo), Separator::Whitespace);
        let tokens = tokens("p :- X < Y, X = Y.");
        let lt = tokens.iter().find(|t| t.text() == "<").expect("<");
        let y = tokens.iter().find(|t| t.text() == "Y").expect("Y");
        assert_eq!(separator(lt, y, Dialect::Clingo), Separator::Nothing);
    }
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib fusion`
Expected: compile error — `lex_mode_of` and `separator` do not exist.

- [ ] **Step 3: Write the token form and the reconstruction**

Add to `src/fusion.rs`, after `separator_between`:

```rust
/// The oracle over tree tokens: derives the mode from `left`'s position
/// (`lex_mode_of`) and answers for the texts. Total; O(|left| + |right|
/// + depth of `left`).
pub fn separator(left: &SyntaxToken, right: &SyntaxToken, dialect: Dialect) -> Separator {
    separator_between(left.text(), right.text(), LexContext { dialect, mode: lex_mode_of(left) })
}

/// The mode the parser requested for `token`, reconstructed from the
/// tree by docs/design/syntax.md §10.2's rule and bound to the parser's
/// own choice by law: `ScriptBody` for a script body and for `#end` —
/// and for the unterminated region's error token; `Theory` inside a
/// theory atom's elements and guard — outside their conditions and
/// outside what follows a condition through the `;` or `}` that ends it
/// — and at a `#theory` definition's operator positions; `Normal`
/// elsewhere, the `{` that opens the elements and the first token after
/// a guard among them. Total; O(depth of the token).
pub fn lex_mode_of(token: &SyntaxToken) -> LexMode {
    match token.kind() {
        SyntaxKind::SCRIPT_BODY | SyntaxKind::KW_END => return LexMode::ScriptBody,
        SyntaxKind::ERROR if in_script_body_position(token) => return LexMode::ScriptBody,
        SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT
            if token.parent().is_some_and(|parent| {
                matches!(parent.kind(), SyntaxKind::OP_DEFINITION | SyntaxKind::ATOM_DEFINITION)
            }) =>
        {
            return LexMode::Theory;
        }
        SyntaxKind::L_BRACE
            if token.parent().is_some_and(|parent| parent.kind() == SyntaxKind::THEORY_ELEMENTS) =>
        {
            return LexMode::Normal;
        }
        _ => {}
    }
    for ancestor in token.ancestors() {
        match ancestor.kind() {
            SyntaxKind::CONDITION => return LexMode::Normal,
            SyntaxKind::THEORY_GUARD => return LexMode::Theory,
            SyntaxKind::THEORY_ELEMENTS => {
                return if follows_a_condition(token, &ancestor) { LexMode::Normal } else { LexMode::Theory };
            }
            _ => {}
        }
    }
    LexMode::Normal
}

/// Whether the error token stands where a script body stands: a child of
/// a script statement, after its `)`.
fn in_script_body_position(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else { return false };
    if parent.kind() != SyntaxKind::SCRIPT_STATEMENT {
        return false;
    }
    let mut cursor = token.prev_sibling_or_token();
    while let Some(element) = cursor {
        match element {
            NodeOrToken::Token(before) if before.kind().is_trivia() => cursor = before.prev_sibling_or_token(),
            NodeOrToken::Token(before) => return before.kind() == SyntaxKind::R_PAREN,
            NodeOrToken::Node(_) => return false,
        }
    }
    false
}

/// Whether `token`, under `elements` (a `THEORY_ELEMENTS` node), stands
/// after an element that ended in a condition with no `;` between — the
/// stretch the parser reads in normal mode: the condition's terminator,
/// and anything recovery placed before it.
fn follows_a_condition(token: &SyntaxToken, elements: &SyntaxNode) -> bool {
    // The child of `elements` that holds the token: the token itself, or
    // the ERROR node it stands in.
    let child = token
        .ancestors()
        .take_while(|ancestor| ancestor != elements)
        .last()
        .map_or(NodeOrToken::Token(token.clone()), NodeOrToken::Node);
    if let NodeOrToken::Node(node) = &child {
        if node.kind() == SyntaxKind::THEORY_ELEMENT {
            return false;
        }
    }
    let mut cursor = child.prev_sibling_or_token();
    while let Some(element) = cursor {
        match &element {
            NodeOrToken::Token(before) if before.kind() == SyntaxKind::SEMICOLON => return false,
            NodeOrToken::Node(node) if node.kind() == SyntaxKind::THEORY_ELEMENT => {
                return node
                    .children_with_tokens()
                    .filter(|e| !matches!(e, NodeOrToken::Token(t) if t.kind().is_trivia()))
                    .last()
                    .is_some_and(|last| matches!(last, NodeOrToken::Node(n) if n.kind() == SyntaxKind::CONDITION));
            }
            _ => {}
        }
        cursor = element.prev_sibling_or_token();
    }
    false
}
```

with `use crate::tree::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken};` at the
top of `fusion.rs`.

- [ ] **Step 4: Run the module tests**

Run: `cargo test -p themelios-syntax --lib fusion`
Expected: 8 passed.

- [ ] **Step 5: Write the oracle laws and the mode law**

`crates/themelios-syntax/tests/oracle_laws.rs`:

```rust
//! The oracle's laws (docs/design/syntax.md §10, §16): for adjacent
//! token pairs drawn from parsed corpus trees, `Nothing` means the pair
//! reparses to itself abutted and `Whitespace` means it does not; the
//! whole-text lemma — a corpus text re-spaced to abut every pair the
//! oracle allows reparses to the same token stream; and the mode law —
//! the parser's recorded modes equal `lex_mode_of` over every corpus
//! token.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::fusion::{lex_mode_of, separator, Separator};
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{parse, parse_program};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Every corpus input with its dialect: the sidecar's, else clingo.
fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let dialect = fs::read_to_string(path.with_extension("expect"))
                    .ok()
                    .and_then(|sidecar| sidecar.lines().next().map(str::to_owned))
                    .map_or(Dialect::Clingo, |line| if line == "asp-core-2" { Dialect::AspCore2 } else { Dialect::Clingo });
                found.push((path.strip_prefix(&dir).expect("under corpus").display().to_string(), text, dialect));
            }
        }
    }
    found.sort();
    found
}

fn tokens_of(root: &SyntaxNode) -> Vec<SyntaxToken> {
    root.descendants_with_tokens().filter_map(|e| e.into_token()).collect()
}

/// The token stream a reparse must keep: every non-whitespace token's
/// kind and text, in order.
fn stream(root: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    tokens_of(root)
        .into_iter()
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .map(|t| (t.kind(), t.text().to_owned()))
        .collect()
}

/// The first token of `text` under `mode` and `dialect`, as (kind, len).
fn first_token(text: &str, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let token = Lexer::new(&source, dialect).token_at(ByteOffset::ZERO, mode).expect("a position");
    (token.kind, token.text.len())
}

#[test]
fn nothing_means_the_pair_reparses_to_itself_and_whitespace_means_it_does_not() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let root = parse(&source, dialect).syntax();
        let tokens: Vec<SyntaxToken> = tokens_of(&root).into_iter().filter(|t| t.kind() != SyntaxKind::WHITESPACE).collect();
        for pair in tokens.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            let joined = format!("{}{}", left.text(), right.text());
            let mode = lex_mode_of(left);
            let (kind, len) = first_token(&joined, mode, dialect);
            let abuts = kind == left.kind() && len == left.text().len();
            match separator(left, right, dialect) {
                Separator::Nothing => assert!(abuts, "{name}: {:?} {:?} answered Nothing", left.text(), right.text()),
                Separator::Whitespace => assert!(!abuts, "{name}: {:?} {:?} answered Whitespace", left.text(), right.text()),
                Separator::LineBreak => assert!(
                    matches!(left.kind(), SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT | SyntaxKind::SHEBANG_COMMENT),
                    "{name}: {:?} answered LineBreak",
                    left.text()
                ),
            }
        }
    }
}

#[test]
fn a_text_respaced_to_abut_every_pair_the_oracle_allows_reparses_to_the_same_token_stream() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let root = parse(&source, dialect).syntax();
        let tokens: Vec<SyntaxToken> = tokens_of(&root).into_iter().filter(|t| t.kind() != SyntaxKind::WHITESPACE).collect();
        let mut respaced = String::new();
        for (index, token) in tokens.iter().enumerate() {
            respaced.push_str(token.text());
            if let Some(next) = tokens.get(index + 1) {
                match separator(token, next, dialect) {
                    Separator::Nothing => {}
                    Separator::Whitespace => respaced.push(' '),
                    Separator::LineBreak => respaced.push('\n'),
                }
            }
        }
        let again = Source::new(SourceId::new(0), respaced).expect("admits");
        let reparsed = parse(&again, dialect).syntax();
        assert_eq!(stream(&root), stream(&reparsed), "{name}: the token stream changed under re-spacing");
    }
}

/// A source that records every request the parser makes of it.
struct Recording<'a> {
    lexer: Lexer<'a>,
    requests: RefCell<Vec<(u32, LexMode)>>,
}

impl TokenSource for Recording<'_> {
    fn id(&self) -> SourceId {
        self.lexer.id()
    }
    fn dialect(&self) -> Dialect {
        self.lexer.dialect()
    }
    fn text(&self) -> &str {
        self.lexer.text()
    }
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        self.requests.borrow_mut().push((at.get(), mode));
        self.lexer.token_at(at, mode)
    }
}

/// The mode the parser consumed each token under: the last request at
/// the token's offset — the parser consumes through its last request
/// there, after any peek under another mode.
fn consumed_modes(requests: &[(u32, LexMode)]) -> std::collections::HashMap<u32, LexMode> {
    let mut modes = std::collections::HashMap::new();
    for (at, mode) in requests {
        modes.insert(*at, *mode);
    }
    modes
}

#[test]
fn the_parsers_recorded_modes_equal_the_reconstruction_over_every_corpus_token() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let recording = Recording { lexer: Lexer::new(&source, dialect), requests: RefCell::new(Vec::new()) };
        let root = parse_program(&recording).syntax();
        let modes = consumed_modes(&recording.requests.borrow());
        for token in tokens_of(&root) {
            let at = u32::from(token.text_range().start());
            let requested = modes.get(&at).copied().expect("every placed token was requested");
            assert_eq!(
                lex_mode_of(&token),
                requested,
                "{name}: {:?} at {at} was requested under {requested:?}",
                token.text()
            );
        }
    }
}

#[test]
fn the_named_cases_hold_under_the_recording() {
    for text in ["&a { x : p ; -y }.", "#script (lua) x #end.", "#script (lua) x #end .", ":- &sum { x } >= 5, not p."] {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        let recording = Recording { lexer: Lexer::new(&source, Dialect::Clingo), requests: RefCell::new(Vec::new()) };
        let root = parse_program(&recording).syntax();
        let modes = consumed_modes(&recording.requests.borrow());
        for token in tokens_of(&root) {
            let at = u32::from(token.text_range().start());
            assert_eq!(lex_mode_of(&token), modes[&at], "{text}: {:?}", token.text());
        }
    }
}
```

- [ ] **Step 6: Extend the fuzz target with the mode law**

In `fuzz/fuzz_targets/parse.rs`, replace the program-entry call with a
recording source, and assert the law — add the `Recording` source (as
in the test above, minus the derive noise) and, in the target body:

```rust
        let recording = Recording { lexer, requests: RefCell::new(Vec::new()) };
        let program = parse_program(&recording);
        holds(&program, &text);
        let modes: HashMap<u32, LexMode> = recording.requests.borrow().iter().copied().collect();
        for token in program.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
            let at = u32::from(token.text_range().start());
            assert_eq!(lex_mode_of(&token), modes[&at]);
        }
```

(with `use std::cell::RefCell; use std::collections::HashMap;` and the
`fusion::lex_mode_of`, `token::{LexMode, Token, TokenSource}`,
`PositionRefusal`, `ByteOffset` imports).

- [ ] **Step 7: Run the laws, the gate, then commit**

Run: `cargo test -p themelios-syntax --test oracle_laws`, the four gate
commands, and `cargo fuzz build -s none`. Expected: green. A mode-law
failure names a token whose region the parser and the reconstruction
read differently: the parser's rule (syntax.md §6.3) and the
reconstruction (§10.2) are two statements of one fact, and the failing
side is repaired to the design's letter — recorded in the commit
message.

```bash
git add crates/themelios-syntax
git commit -m "Add the oracle over tokens and the mode reconstruction, held to the parser's recorded modes over the whole corpus"
```

---

### Task 16: The `equiv` module — the sequence and its projections, the two certificates, canonical spelling

**Files:**
- Create: `crates/themelios-syntax/src/equiv.rs`,
  `crates/themelios-syntax/tests/equiv_laws.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (add `pub mod equiv;`),
  `crates/themelios-syntax/fuzz/fuzz_targets/parse.rs`
  (`equivalent(p, p, ·)` holds)

**Derives:** syntax.md §11 whole (§11.1 the sequence and its two
projections, the content per kind; §11.2 the certificate, the witness,
the corollary named and scoped; §11.3 canonical spelling — the four
printable pairs and the optimize pair fixed by the roster), §12.5
(`Mismatch: Display + Error`), §13, §16 (the certificates' laws, the
corollary, `canonical_spelling` idempotent and closed).

**Interfaces:**
- Consumes: `tree::{role, TokenRole, SyntaxNode, SyntaxToken,
  SyntaxKind}`, `parse::Parse`, base's `Location`.
- Produces: `equiv::{non_whitespace_tokens, token_stream,
  comment_sequence, Certificate, Mismatch, Side, equivalent,
  canonical_spelling}`.

- [ ] **Step 1: Write the failing tests**

Append `pub mod equiv;` to `src/lib.rs`. Create `src/equiv.rs` holding
only this test module:

```rust
#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::ast::Program;
    use crate::dialect::Dialect;
    use crate::parse::{parse, Parse};
    use crate::tree::SyntaxKind;

    fn program(text: &str, id: u32) -> Parse<Program> {
        let source = Source::new(SourceId::new(id), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
    }

    fn certified(left: &str, right: &str, certificate: Certificate) -> Result<(), Mismatch> {
        equivalent(&program(left, 1), &program(right, 2), certificate)
    }

    #[test]
    fn the_sequence_interleaves_significant_tokens_and_trivia_comments() {
        let parse = program("%! d\np. % c\nq :- r.\n", 0);
        let sequence: Vec<String> = non_whitespace_tokens(&parse.syntax()).map(|t| t.text().to_owned()).collect();
        assert_eq!(sequence, ["%! d", "p", ".", "% c", "q", ":-", "r", "."]);
        let stream: Vec<String> = token_stream(&parse.syntax()).map(|t| t.text().to_owned()).collect();
        assert_eq!(stream, ["%! d", "p", ".", "q", ":-", "r", "."]);
        let comments: Vec<String> = comment_sequence(&parse.syntax()).map(|t| t.text().to_owned()).collect();
        assert_eq!(comments, ["% c"]);
    }

    #[test]
    fn layout_only_certifies_exactly_a_change_of_whitespace() {
        assert_eq!(certified("p(X):-q(X).", "p( X )  :-\n  q(X) .", Certificate::LayoutOnly), Ok(()));
        assert!(certified("p.", "q.", Certificate::LayoutOnly).is_err());
        assert!(certified("p. % c\nq.", "p.\nq. % c", Certificate::LayoutOnly).is_err(), "a comment moved across a token");
        assert_eq!(certified("p. % c   \n", "p. % c\n", Certificate::LayoutOnly), Ok(()), "a line comment's trailing whitespace is layout");
        assert!(certified("%! d  \np.", "%! d\np.", Certificate::LayoutOnly).is_err(), "a doc line's trailing whitespace is content");
        assert!(certified("%! one\np.", "%! two\np.", Certificate::LayoutOnly).is_err());
        assert_eq!(
            certified("#script (lua) x = 1   #end.", "#script (lua) x = 1 #end.", Certificate::LayoutOnly),
            Ok(()),
            "the script body compares by its value"
        );
        assert!(certified("p($).", "p(#).", Certificate::LayoutOnly).is_err(), "error tokens are significant");
    }

    #[test]
    fn up_to_spelling_admits_exactly_the_synonym_pairs() {
        let left = "p :- X == 1, X <> 2, Y = #infimum, Z != #supremum. #minimise { 1 }. #maximise { 2 }.";
        let right = "p :- X = 1, X != 2, Y = #inf, Z != #sup. #minimize { 1 }. #maximize { 2 }.";
        assert!(certified(left, right, Certificate::LayoutOnly).is_err());
        assert_eq!(certified(left, right, Certificate::UpToSpelling), Ok(()));
        assert!(certified("p :- X <= 1.", "p :- X < 1.", Certificate::UpToSpelling).is_err());
    }

    #[test]
    fn the_witness_names_the_first_divergence_on_both_sides() {
        let mismatch = certified("p(a, b). q.", "p(a, c). q.", Certificate::LayoutOnly).expect_err("diverges");
        assert_eq!(mismatch.index, 4);
        let left = mismatch.left.expect("a left side");
        let right = mismatch.right.expect("a right side");
        assert_eq!((left.kind, left.content.as_str()), (SyntaxKind::IDENT, "b"));
        assert_eq!((right.kind, right.content.as_str()), (SyntaxKind::IDENT, "c"));
        assert_eq!(left.location.source, SourceId::new(1));
        assert_eq!(right.location.source, SourceId::new(2));
        let shorter = certified("p. q.", "p.", Certificate::LayoutOnly).expect_err("diverges");
        assert_eq!(shorter.index, 2);
        assert!(shorter.left.is_some() && shorter.right.is_none());
        assert!(shorter.to_string().contains("index 2"));
        let _: &dyn std::error::Error = &shorter;
    }

    #[test]
    fn canonical_spelling_is_the_authoritys_where_it_prints_and_the_rosters_for_the_optimize_pair() {
        assert_eq!(canonical_spelling(SyntaxKind::EQ, "=="), "=");
        assert_eq!(canonical_spelling(SyntaxKind::EQ, "="), "=");
        assert_eq!(canonical_spelling(SyntaxKind::NEQ, "<>"), "!=");
        assert_eq!(canonical_spelling(SyntaxKind::KW_INF, "#infimum"), "#inf");
        assert_eq!(canonical_spelling(SyntaxKind::KW_SUP, "#supremum"), "#sup");
        assert_eq!(canonical_spelling(SyntaxKind::KW_MINIMIZE, "#minimise"), "#minimize");
        assert_eq!(canonical_spelling(SyntaxKind::KW_MAXIMIZE, "#maximise"), "#maximize");
        assert_eq!(canonical_spelling(SyntaxKind::IDENT, "abc"), "abc");
        assert_eq!(canonical_spelling(SyntaxKind::LE, "<="), "<=");
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-syntax --lib equiv`
Expected: compile error — the module's items do not exist.

- [ ] **Step 3: Write the module**

Prepend to `src/equiv.rs`:

```rust
//! Token-stream equivalence (docs/design/syntax.md §11): the
//! non-whitespace sequence and its two projections, the two certificates
//! over that one sequence, the first divergence as a witness, and the
//! canonical spellings a spelling-normalizing formatter reads.

use std::borrow::Cow;
use std::fmt;

use themelios_base::span::Location;

use crate::parse::Parse;
use crate::tree::{role, Asp, AstNode, SyntaxKind, SyntaxNode, SyntaxToken, TokenRole};

/// Every non-whitespace token under `node`, in order — the sequence the
/// certificates compare: significant tokens and trivia comments
/// interleaved as they stand. Total; a lazy iterative preorder walk.
pub fn non_whitespace_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() != SyntaxKind::WHITESPACE)
}

/// The significant tokens of the tree under `node`, in order: every
/// token whose role is not `Trivia` — all non-comment, non-whitespace
/// tokens plus `DOC_COMMENT` tokens in docs position. Total; lazy.
pub fn token_stream(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    non_whitespace_tokens(node).filter(|token| role(token) != TokenRole::Trivia)
}

/// The trivia comments under `node`, in order: role `Trivia`, kind a
/// comment. Total; lazy.
pub fn comment_sequence(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    non_whitespace_tokens(node).filter(|token| token.kind().is_comment() && role(token) == TokenRole::Trivia)
}

/// A token's content for the sequence (docs/design/syntax.md §11.1): a
/// line comment or shebang without its trailing horizontal whitespace,
/// which is layout; a doc comment whole, wherever it stands; a script
/// body by its value — the grammar's own trimming of the blanks before
/// `#end`; every other token its text.
fn content(token: &SyntaxToken) -> &str {
    match token.kind() {
        SyntaxKind::LINE_COMMENT | SyntaxKind::SHEBANG_COMMENT => token.text().trim_end_matches([' ', '\t', '\r']),
        SyntaxKind::SCRIPT_BODY => token.text().trim_end_matches([' ', '\t']),
        _ => token.text(),
    }
}

/// Which claim is being certified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Certificate {
    /// Layout only: the non-whitespace sequences equal by kind and
    /// content. Nothing but whitespace changed — exactly that, since
    /// whitespace is all the sequence leaves out.
    LayoutOnly,
    /// Up to spelling: as `LayoutOnly`, save that a token's content is
    /// compared after canonical respelling — the grammar's synonym pairs
    /// may have been normalized, and nothing else.
    UpToSpelling,
}

/// The first divergence, as a witness: the index in the sequence and
/// both sides — a side is `None` where its sequence ended first. Each
/// side carries the token's kind, its content, and its location in its
/// own tree, so a formatter's `--safe` mode reports where in the input
/// and where in the output the claim broke; the kind says whether the
/// element that diverged is a comment or a significant token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mismatch {
    /// The index in the non-whitespace sequence.
    pub index: usize,
    /// The left side's token, if its sequence had one.
    pub left: Option<Side>,
    /// The right side's token, if its sequence had one.
    pub right: Option<Side>,
}

/// One side of a divergence.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side {
    /// The token's kind.
    pub kind: SyntaxKind,
    /// Its content, as compared.
    pub content: String,
    /// Where it stands, in its own source.
    pub location: Location,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the sequences diverge at index {}: ", self.index)?;
        match (&self.left, &self.right) {
            (Some(left), Some(right)) => write!(
                f,
                "left has {} {:?}, right has {} {:?}",
                left.kind, left.content, right.kind, right.content
            ),
            (Some(left), None) => write!(f, "left has {} {:?}, right has ended", left.kind, left.content),
            (None, Some(right)) => write!(f, "left has ended, right has {} {:?}", right.kind, right.content),
            (None, None) => f.write_str("both have ended"),
        }
    }
}

impl std::error::Error for Mismatch {}

/// The certificate: granted, or refused with the first divergence.
/// Compares the two sequences whatever the parses' dialects — a lexical
/// statement about two texts, meaningful across them; both roots are of
/// one family, as the one `T` fixes. Total; O(|left| + |right|); a
/// single zip over two lazy iterative walks. Not a refusal but the
/// answer to the certificate's question (docs/design/syntax.md §12.4).
pub fn equivalent<T: AstNode<Language = Asp>>(
    left: &Parse<T>,
    right: &Parse<T>,
    certificate: Certificate,
) -> Result<(), Mismatch> {
    let left_root = left.syntax();
    let right_root = right.syntax();
    let mut lefts = non_whitespace_tokens(&left_root);
    let mut rights = non_whitespace_tokens(&right_root);
    let compared = |token: &SyntaxToken| -> String {
        let text = content(token);
        match certificate {
            Certificate::LayoutOnly => text.to_owned(),
            Certificate::UpToSpelling => canonical_spelling(token.kind(), text).into_owned(),
        }
    };
    let mut index = 0usize;
    loop {
        match (lefts.next(), rights.next()) {
            (None, None) => return Ok(()),
            (l, r) => {
                let same = match (&l, &r) {
                    (Some(l), Some(r)) => l.kind() == r.kind() && compared(l) == compared(r),
                    _ => false,
                };
                if !same {
                    let side = |token: SyntaxToken, parse: &Parse<T>| Side {
                        kind: token.kind(),
                        content: compared(&token),
                        location: parse.location(token.text_range()),
                    };
                    return Err(Mismatch {
                        index,
                        left: l.map(|token| side(token, left)),
                        right: r.map(|token| side(token, right)),
                    });
                }
                index += 1;
            }
        }
    }
}

/// The canonical spelling of a token that has synonyms (grammar §4.5,
/// §4.6): `=` for `EQ`, `!=` for `NEQ`, `#inf`, `#sup` — the spellings
/// the authority renders when it prints its own tree — and `#minimize`,
/// `#maximize`, the roster's own, since the authority prints an optimize
/// statement as a weak constraint (docs/design/syntax.md §11.3); every
/// other token's content is its own canonical form. Total; the identity
/// on non-synonym kinds; O(1).
pub fn canonical_spelling(kind: SyntaxKind, content: &str) -> Cow<'_, str> {
    let canonical = match kind {
        SyntaxKind::EQ => "=",
        SyntaxKind::NEQ => "!=",
        SyntaxKind::KW_INF => "#inf",
        SyntaxKind::KW_SUP => "#sup",
        SyntaxKind::KW_MINIMIZE => "#minimize",
        SyntaxKind::KW_MAXIMIZE => "#maximize",
        _ => return Cow::Borrowed(content),
    };
    if content == canonical { Cow::Borrowed(content) } else { Cow::Owned(canonical.to_owned()) }
}
```

- [ ] **Step 4: Run the module tests**

Run: `cargo test -p themelios-syntax --lib equiv`
Expected: 5 passed.

- [ ] **Step 5: Write the certificate laws**

`crates/themelios-syntax/tests/equiv_laws.rs`:

```rust
//! The certificates' reflexivity through reparse, symmetry, and the
//! corollary — equal non-whitespace sequences, equal significant-token
//! shapes, under equal dialects and one root family, outside the aspif
//! dispatch — and `canonical_spelling` idempotent and closed over the
//! synonym pairs (docs/design/syntax.md §11, §16).

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{canonical_spelling, equivalent, Certificate};
use themelios_syntax::fusion::{separator, Separator};
use themelios_syntax::parse::{parse, Parse};
use themelios_syntax::tree::{role, NodeOrToken, SyntaxKind, SyntaxNode, TokenRole, WalkEvent};

fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let dialect = fs::read_to_string(path.with_extension("expect"))
                    .ok()
                    .and_then(|sidecar| sidecar.lines().next().map(str::to_owned))
                    .map_or(Dialect::Clingo, |line| if line == "asp-core-2" { Dialect::AspCore2 } else { Dialect::Clingo });
                found.push((path.strip_prefix(&dir).expect("under corpus").display().to_string(), text, dialect));
            }
        }
    }
    found.sort();
    found
}

fn admitted(text: &str, id: u32) -> Source {
    Source::new(SourceId::new(id), text.to_owned()).expect("admits")
}

/// The text re-spaced by the oracle: every pair abutted where the oracle
/// allows, one space or one line break where it does not.
fn respaced_by_the_oracle(root: &SyntaxNode, dialect: Dialect) -> String {
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .collect();
    let mut out = String::new();
    for (index, token) in tokens.iter().enumerate() {
        out.push_str(token.text());
        if let Some(next) = tokens.get(index + 1) {
            match separator(token, next, dialect) {
                Separator::Nothing => {}
                Separator::Whitespace => out.push(' '),
                Separator::LineBreak => out.push('\n'),
            }
        }
    }
    out
}

/// The significant-token shape: the preorder over nodes and significant
/// tokens, trivia dropped.
fn shape(root: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => out.push(format!("({}", node.kind())),
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(")".to_owned()),
            WalkEvent::Enter(NodeOrToken::Token(token)) if role(&token) != TokenRole::Trivia => {
                out.push(format!("{}:{}", token.kind(), token.text()));
            }
            _ => {}
        }
    }
    out
}

fn is_aspif(parse: &Parse<themelios_syntax::ast::Program>) -> bool {
    parse.diagnostics().iter().any(|d| d.id().name() == "aspif-input")
}

#[test]
fn reflexivity_through_reparse_symmetry_and_the_corollary_over_the_corpus() {
    for (name, text, dialect) in corpus() {
        let left = parse(&admitted(&text, 1), dialect);
        let again = parse(&admitted(&text, 2), dialect);
        assert_eq!(equivalent(&left, &again, Certificate::LayoutOnly), Ok(()), "{name}: reflexive through reparse");
        assert_eq!(equivalent(&left, &again, Certificate::UpToSpelling), Ok(()), "{name}");
        let respaced = parse(&admitted(&respaced_by_the_oracle(&left.syntax(), dialect), 3), dialect);
        assert_eq!(equivalent(&left, &respaced, Certificate::LayoutOnly), Ok(()), "{name}: layout only");
        assert_eq!(
            equivalent(&left, &respaced, Certificate::LayoutOnly).is_ok(),
            equivalent(&respaced, &left, Certificate::LayoutOnly).is_ok(),
            "{name}: symmetric"
        );
        if !is_aspif(&left) {
            assert_eq!(shape(&left.syntax()), shape(&respaced.syntax()), "{name}: the corollary");
        }
    }
}

proptest! {
    #[test]
    fn a_whitespace_change_keeps_the_certificate_and_the_shape(
        text in prop::sample::select(vec![
            "p(X) :- q(X), not r(X), X = 1..3.\n% c\n:- #sum { W,T : t(T,W) } >= 4. %* b *%\n",
            "%! doc\na ; b | c : d.\n&sum { x, -y : p ; {a} } <= 3.\n#script (lua) x = 1 #end.\n",
        ]),
        choices in prop::collection::vec(0u8..3, 1..24)
    ) {
        let left = parse(&admitted(text, 1), Dialect::Clingo);
        let mut respaced = String::new();
        let mut next = choices.iter().copied().cycle();
        for token in left.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
            if token.kind() == SyntaxKind::WHITESPACE {
                let breaks = token.text().matches('\n').count();
                let filler = match next.next().unwrap_or(0) { 0 => " ", 1 => "\t", _ => "  " };
                respaced.push_str(filler);
                for _ in 0..breaks {
                    respaced.push('\n');
                }
            } else {
                respaced.push_str(token.text());
            }
        }
        let right = parse(&admitted(&respaced, 2), Dialect::Clingo);
        prop_assert_eq!(equivalent(&left, &right, Certificate::LayoutOnly), Ok(()));
        prop_assert_eq!(shape(&left.syntax()), shape(&right.syntax()));
    }
}

#[test]
fn canonical_spelling_is_idempotent_and_closed_over_the_synonym_pairs() {
    let pairs = [
        (SyntaxKind::EQ, "=", "=="),
        (SyntaxKind::NEQ, "!=", "<>"),
        (SyntaxKind::KW_INF, "#inf", "#infimum"),
        (SyntaxKind::KW_SUP, "#sup", "#supremum"),
        (SyntaxKind::KW_MINIMIZE, "#minimize", "#minimise"),
        (SyntaxKind::KW_MAXIMIZE, "#maximize", "#maximise"),
    ];
    for (kind, canonical, synonym) in pairs {
        assert_eq!(canonical_spelling(kind, canonical), canonical);
        assert_eq!(canonical_spelling(kind, synonym), canonical);
        let once = canonical_spelling(kind, synonym).into_owned();
        assert_eq!(canonical_spelling(kind, &once), once);
    }
    for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_token()) {
        if !pairs.iter().any(|(pair_kind, ..)| *pair_kind == kind) {
            assert_eq!(canonical_spelling(kind, "anything"), "anything", "{kind}: the identity");
        }
    }
}
```

- [ ] **Step 6: Extend the fuzz target and run everything**

In `fuzz/fuzz_targets/parse.rs`, in `holds`, after the attachment
checks:

```rust
    assert_eq!(themelios_syntax::equiv::equivalent(parse, parse, themelios_syntax::equiv::Certificate::LayoutOnly), Ok(()));
    assert_eq!(themelios_syntax::equiv::equivalent(parse, parse, themelios_syntax::equiv::Certificate::UpToSpelling), Ok(()));
```

Run: `cargo test -p themelios-syntax --test equiv_laws`, the four gate
commands, and `cargo fuzz build -s none`. Expected: green.

```bash
git add crates/themelios-syntax
git commit -m "Add token-stream equivalence: the interleaved sequence, its two projections, both certificates with their witness, and canonical spelling"
```

---

### Task 17: The differential against the pinned authority, and the tree-sitter cross-check

**Files:**
- Create: `pixi.toml`, `pixi.lock` (repository root),
  `crates/themelios-syntax/tests/differential.rs`,
  `crates/themelios-syntax/tests/differential/authority.py`,
  `crates/themelios-syntax/tests/corpus/AUTHORITY-DISAGREEMENTS`,
  `crates/themelios-syntax/tests/corpus/TREE-SITTER-DISAGREEMENTS`
- Modify: `.gitignore` (`/.pixi`)

**Derives:** syntax.md §6.5 (membership is the differential's question),
§6.6 (the authority's ceiling per family, the lower bound), §11.3 (the
four printable canonical spellings checked against the authority's
printing), §16 (the differential: feature-gated, out of band per
milestone, clingo the authority; agreement on membership and on
statement count and kinds; disagreements land in the grammar's
register; the tree-sitter-clingo cross-check at the tier's landing);
spec §10.1–§10.2; grammar §3 (the roster and pins; the secondary
cross-check's obligation), §11 (D1's skip; D2's obligation).

**Interfaces:**
- Consumes: the corpus and its `.expect` sidecars, `DIFFERENTIAL-SKIP`;
  the pixi environment (`clingo ==5.8.2` with its Python module,
  `tree-sitter-cli`); the pinned tree-sitter-clingo grammar fetched into
  `target/`.
- Produces: `pixi run differential` (membership, statement kinds,
  canonical spellings), `pixi run measure-ceiling` (the authority's
  per-family nesting ceiling, written to
  `target/differential/authority-ceiling.txt` — Task 18 records it),
  `pixi run cross-check` (the tree-sitter cross-check); the two
  disagreement registers.

- [ ] **Step 1: The pixi manifest at the repository root**

`pixi.toml`:

```toml
# The pinned authority and the secondary cross-check for the syntax
# tier's out-of-band instruments (docs/design/syntax.md §16;
# docs/grammar.md §3): clingo v5.8.2 as conda-forge ships it — the
# binary and its Python module, one package — and the tree-sitter CLI
# that runs the pinned tree-sitter-clingo grammar. Nothing here enters
# any Rust manifest or any shipped closure.
[workspace]
name = "themelios"
channels = ["conda-forge"]
platforms = ["osx-arm64", "osx-64", "linux-64"]

[dependencies]
clingo = "==5.8.2"
python = ">=3.12"
tree-sitter-cli = ">=0.26.12"

[tasks]
# The differential: membership, statement kinds, canonical spellings.
differential = "cargo test -p themelios-syntax --features differential --test differential -- --nocapture --test-threads=1 differential"
# The authority's nesting ceiling per family, for the depth constant's
# lower bound and grammar §11 D2 (a bisection, one process per probe).
measure-ceiling = "cargo test -p themelios-syntax --features differential --test differential -- --nocapture --ignored measure_the_authoritys_nesting_ceiling_per_family"
# The pinned secondary grammar, fetched once into the ignored target directory.
fetch-tree-sitter-clingo = "test -d target/tree-sitter-clingo || (git clone -q https://github.com/potassco/tree-sitter-clingo target/tree-sitter-clingo && git -C target/tree-sitter-clingo -c advice.detachedHead=false checkout -q 58e062c1c6c2ac0bad54fee054573c5a9e6dd759)"
# The tree-sitter cross-check over the corpus.
cross-check = { cmd = "cargo test -p themelios-syntax --features differential --test differential -- --nocapture --ignored the_pinned_tree_sitter_grammar_agrees_with_this_parser_on_the_corpus", depends-on = ["fetch-tree-sitter-clingo"] }
```

Append `/.pixi` to `.gitignore`. Run `pixi install` (an externally
installed tool, like cargo-fuzz; `pixi.lock` is written and committed —
reproducibility, as `Cargo.lock` is), then:
`pixi run python -c "import clingo; print(clingo.__version__)"` and
`pixi run tree-sitter --version`.
Expected: `5.8.2`; `tree-sitter 0.26.12` or later.

- [ ] **Step 2: The authority helper**

`crates/themelios-syntax/tests/differential/authority.py`:

```python
"""The authority's reading of one program (docs/design/syntax.md §16):
the program arrives on stdin, and one JSON object leaves on stdout —
the clingo version this process runs, whether the parser accepted the
program, and the statements it built, each as its AST type and the
authority's own printing of it. Test-only: run by tests/differential.rs
under the pixi environment; never shipped, never imported by anything.

The authority resolves `#include` from the working directory, which the
caller sets to the input's own directory; an include it cannot open is
a syntax error to it, reported here as `include_failed` so the caller
can tell that from a disagreement about the language.
"""

import json
import sys

import clingo
from clingo.ast import parse_string


def read(program: str) -> dict:
    statements: list[dict] = []
    try:
        parse_string(program, lambda statement: statements.append(
            {"type": statement.ast_type.name, "text": str(statement)}
        ))
    except RuntimeError as error:
        message = str(error)
        return {
            "version": clingo.__version__,
            "accepted": False,
            "message": message,
            "include_failed": "file could not be opened" in message,
        }
    return {"version": clingo.__version__, "accepted": True, "statements": statements}


if __name__ == "__main__":
    json.dump(read(sys.stdin.read()), sys.stdout)
```

- [ ] **Step 3: The harness**

`crates/themelios-syntax/tests/differential.rs`:

```rust
//! The differential (docs/design/syntax.md §16; docs/grammar.md §3):
//! every corpus input parsed here and by the pinned clingo — agreement
//! on membership and on statement kinds — the four printable canonical
//! spellings checked against the authority's printing, the authority's
//! nesting ceiling measured per family, and the tree-sitter cross-check.
//! Feature-gated and out of band: run through `pixi run differential`,
//! `pixi run measure-ceiling`, `pixi run cross-check`. What it proves:
//! agreement with the authority on the corpus and the seeds. What it
//! cannot: agreement beyond them.
#![cfg(feature = "differential")]

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::{self, Statement};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{parse, Parse};

/// The pin (docs/grammar.md §3).
const AUTHORITY_VERSION: &str = "5.8.2";
/// The pinned secondary grammar (docs/grammar.md §3).
const TREE_SITTER_CLINGO: &str = "58e062c1c6c2ac0bad54fee054573c5a9e6dd759";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_dir() -> PathBuf {
    manifest_dir().join("tests/corpus")
}

fn workspace_root() -> PathBuf {
    manifest_dir().join("../..").canonicalize().expect("the workspace root resolves")
}

/// The authority's reading of `program`, run from `cwd`.
#[derive(Debug)]
struct Reading {
    accepted: bool,
    include_failed: bool,
    statements: Vec<(String, String)>,
}

fn authority(program: &str, cwd: &Path) -> Reading {
    let mut child = Command::new("python")
        .arg(manifest_dir().join("tests/differential/authority.py"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python runs: run this harness through `pixi run differential`");
    child.stdin.take().expect("piped stdin").write_all(program.as_bytes()).expect("the program is written");
    let output = child.wait_with_output().expect("the authority answers");
    assert!(
        output.status.success(),
        "the authority helper failed (is clingo's Python module installed? run through pixi):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reading: Value = serde_json::from_slice(&output.stdout).expect("the helper emits JSON");
    assert_eq!(
        reading["version"].as_str(),
        Some(AUTHORITY_VERSION),
        "docs/grammar.md §3: the authority is pinned at v{AUTHORITY_VERSION}"
    );
    Reading {
        accepted: reading["accepted"].as_bool().unwrap_or(false),
        include_failed: reading["include_failed"].as_bool().unwrap_or(false),
        statements: reading["statements"]
            .as_array()
            .map(|statements| {
                statements
                    .iter()
                    .map(|s| (s["type"].as_str().unwrap_or("").to_owned(), s["text"].as_str().unwrap_or("").to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Every corpus input, with the parser's own reading under the clingo
/// dialect — the language the authority reads.
fn inputs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![corpus_dir()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// A register file: `# comment` lines dropped, each line a path and a
/// note.
fn register(name: &str) -> BTreeSet<String> {
    fs::read_to_string(corpus_dir().join(name))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(corpus_dir()).expect("under the corpus").display().to_string()
}

/// The authority's statement kinds a statement of ours corresponds to
/// (read from the pinned authority's AST at v5.8.2): a rule is one
/// `Rule`; a weak constraint one `Minimize`; an optimize statement one
/// `Minimize` per element; `#show` a `ShowSignature` for the bare and
/// signature forms and a `ShowTerm` for the term forms; `#project` a
/// `ProjectSignature` or a `ProjectAtom`; `#edge` one `Edge` per pair;
/// `#const` a `Definition`; `#program` a `Program`; the rest their own
/// names. `#include` is resolved by the authority and corresponds to
/// nothing of its own.
fn corresponding(statement: &Statement) -> Vec<&'static str> {
    match statement {
        Statement::Rule(_) => vec!["Rule"],
        Statement::WeakConstraint(_) => vec!["Minimize"],
        Statement::Optimize(optimize) => vec!["Minimize"; optimize.elements().count()],
        Statement::Show(show) => {
            if show.term().is_some() { vec!["ShowTerm"] } else { vec!["ShowSignature"] }
        }
        Statement::Project(project) => {
            if project.atom().is_some() { vec!["ProjectAtom"] } else { vec!["ProjectSignature"] }
        }
        Statement::Defined(_) => vec!["Defined"],
        Statement::Edge(edge) => vec!["Edge"; edge.edges().count()],
        Statement::Heuristic(_) => vec!["Heuristic"],
        Statement::External(_) => vec!["External"],
        Statement::Const(_) => vec!["Definition"],
        Statement::Script(_) => vec!["Script"],
        Statement::Include(_) => Vec::new(),
        Statement::ProgramPart(_) => vec!["Program"],
        Statement::TheoryDefinition(_) => vec!["TheoryDefinition"],
        Statement::Query(_) => Vec::new(),
    }
}

fn parse_here(text: &str) -> Parse<ast::Program> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, Dialect::Clingo)
}

#[test]
fn differential_membership_and_statement_kinds_agree_with_the_authority() {
    let skip = register("DIFFERENTIAL-SKIP");
    let known = register("AUTHORITY-DISAGREEMENTS");
    let mut disagreements = Vec::new();
    let mut compared = 0usize;
    for path in inputs() {
        let name = relative(&path);
        if skip.contains(&name) {
            continue;
        }
        let text = fs::read_to_string(&path).expect("input reads");
        let ours = parse_here(&text);
        let theirs = authority(&text, path.parent().expect("a directory"));
        compared += 1;
        if theirs.include_failed {
            continue;
        }
        let we_accept = !ours.has_errors();
        if we_accept != theirs.accepted {
            if !known.contains(&name) {
                disagreements.push(format!("{name}: membership — here {we_accept}, authority {}", theirs.accepted));
            }
            continue;
        }
        if !we_accept {
            continue;
        }
        let has_include = ours.tree().statements().any(|s| matches!(s, Statement::Include(_)));
        if has_include {
            continue;
        }
        let expected: Vec<&str> = ours.tree().statements().flat_map(|s| corresponding(&s)).collect();
        let mut found: Vec<&str> = theirs.statements.iter().map(|(kind, _)| kind.as_str()).collect();
        if theirs.statements.first().is_some_and(|(kind, text)| kind == "Program" && text == "#program base.") {
            found.remove(0);
        }
        if expected != found && !known.contains(&name) {
            disagreements.push(format!("{name}: kinds — here {expected:?}, authority {found:?}"));
        }
    }
    println!("compared {compared} inputs against the authority");
    assert!(
        disagreements.is_empty(),
        "unrecorded disagreements with the authority (each is a defect here or a divergence for docs/grammar.md §11):\n{}",
        disagreements.join("\n")
    );
}

#[test]
fn differential_the_four_printable_canonical_spellings_are_the_authoritys() {
    let reading = authority("p :- X == 1, X <> 2, Y = #infimum, Z = #supremum.\n", &corpus_dir());
    assert!(reading.accepted);
    let printed = &reading.statements[1].1;
    for (synonym, canonical) in [("==", "= "), ("<>", "!="), ("#infimum", "#inf"), ("#supremum", "#sup")] {
        assert!(printed.contains(canonical), "{printed}: the authority prints {canonical}");
        assert!(!printed.contains(synonym), "{printed}: the authority does not print {synonym}");
    }
}

/// One nested input per family, `depth` levels deep, as a program the
/// authority reads (docs/design/syntax.md §6.6; docs/grammar.md §11 D2).
fn nested(family: &str, depth: usize) -> String {
    match family {
        "term: function arguments" => format!("p({}x{}).\n", "f(".repeat(depth), ")".repeat(depth)),
        "term: parentheses" => format!("p({}x{}).\n", "(".repeat(depth), ")".repeat(depth)),
        "term: absolute value" => format!("p({}x{}).\n", "|".repeat(depth), "|".repeat(depth)),
        "term: pool" => format!("p({}x{}).\n", "(".repeat(depth), ";y)".repeat(depth)),
        "constant term: function arguments" => format!("#const c = {}x{}.\n", "f(".repeat(depth), ")".repeat(depth)),
        "theory term: set" => format!("&a {{ {}x{} }}.\n", "{".repeat(depth), "}".repeat(depth)),
        "theory term: list" => format!("&a {{ {}x{} }}.\n", "[".repeat(depth), "]".repeat(depth)),
        "theory term: tuple" => format!("&a {{ {}x{} }}.\n", "(".repeat(depth), ")".repeat(depth)),
        "theory term: function arguments" => format!("&a {{ {}x{} }}.\n", "f(".repeat(depth), ")".repeat(depth)),
        "chain: exponentiation" => format!("p({}).\n", vec!["2"; depth].join("**")),
        "chain: unary" => format!("p({}x).\n", "-".repeat(depth)),
        _ => unreachable!("a family named above"),
    }
}

const FAMILIES: [&str; 11] = [
    "term: function arguments",
    "term: parentheses",
    "term: absolute value",
    "term: pool",
    "constant term: function arguments",
    "theory term: set",
    "theory term: list",
    "theory term: tuple",
    "theory term: function arguments",
    "chain: exponentiation",
    "chain: unary",
];

/// What the authority does with one probe: accepts, refuses cleanly, or
/// dies (its process ends without an answer).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Accepts,
    Refuses,
    Dies,
}

fn probe(family: &str, depth: usize) -> Outcome {
    let program = nested(family, depth);
    let mut child = Command::new("python")
        .arg(manifest_dir().join("tests/differential/authority.py"))
        .current_dir(corpus_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python runs under pixi");
    child.stdin.take().expect("piped stdin").write_all(program.as_bytes()).expect("written");
    let output = child.wait_with_output().expect("the authority answers or dies");
    if !output.status.success() {
        return Outcome::Dies;
    }
    let reading: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    if reading["accepted"].as_bool() == Some(true) { Outcome::Accepts } else { Outcome::Refuses }
}

/// The measurement (docs/design/syntax.md §6.6, §16): per family, the
/// largest depth the authority accepts, found by doubling until it does
/// not and bisecting the last interval; the failure mode named. Written
/// to `target/differential/authority-ceiling.txt` for Task 18's record.
#[test]
#[ignore = "out of band: pixi run measure-ceiling"]
fn measure_the_authoritys_nesting_ceiling_per_family() {
    const CAP: usize = 1 << 21;
    let mut report = String::from("family | last depth accepted | first depth failing | failure mode\n");
    for family in FAMILIES {
        let mut low = 1usize;
        let mut high = 2usize;
        let mut failing = None;
        while high <= CAP {
            match probe(family, high) {
                Outcome::Accepts => {
                    low = high;
                    high *= 2;
                }
                outcome => {
                    failing = Some((high, outcome));
                    break;
                }
            }
        }
        if let Some((mut fail_at, mode)) = failing {
            while fail_at - low > 1 {
                let middle = low + (fail_at - low) / 2;
                match probe(family, middle) {
                    Outcome::Accepts => low = middle,
                    _ => fail_at = middle,
                }
            }
            report.push_str(&format!("{family} | {low} | {fail_at} | {mode:?}\n"));
        } else {
            report.push_str(&format!("{family} | {low} (accepted at the cap {CAP}) | — | —\n"));
        }
        println!("{}", report.lines().last().unwrap_or(""));
    }
    let out = workspace_root().join("target/differential");
    fs::create_dir_all(&out).expect("target directory");
    fs::write(out.join("authority-ceiling.txt"), &report).expect("the report writes");
    println!("{report}");
}

/// The secondary cross-check (docs/grammar.md §3): every clingo-dialect
/// corpus input parsed by the pinned tree-sitter-clingo grammar; a
/// disagreement on membership not yet recorded in
/// `TREE-SITTER-DISAGREEMENTS` fails, and each recorded one carries the
/// reading against the authority that settled it.
#[test]
#[ignore = "out of band: pixi run cross-check"]
fn the_pinned_tree_sitter_grammar_agrees_with_this_parser_on_the_corpus() {
    let grammar = workspace_root().join("target/tree-sitter-clingo");
    let head = Command::new("git")
        .args(["-C", grammar.to_str().expect("utf-8 path"), "rev-parse", "HEAD"])
        .output()
        .expect("git runs");
    assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), TREE_SITTER_CLINGO, "the grammar is at its pin");
    let known = register("TREE-SITTER-DISAGREEMENTS");
    let mut disagreements = Vec::new();
    for path in inputs() {
        let name = relative(&path);
        let sidecar = fs::read_to_string(path.with_extension("expect")).unwrap_or_default();
        if sidecar.lines().next() == Some("asp-core-2") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("input reads");
        let ours = !parse_here(&text).has_errors();
        let status = Command::new("tree-sitter")
            .args(["parse", "-q"])
            .arg(&path)
            .current_dir(&grammar)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("tree-sitter runs under pixi");
        let theirs = status.success();
        if ours != theirs && !known.contains(&name) {
            disagreements.push(format!("{name}: here {ours}, tree-sitter {theirs}"));
        }
    }
    assert!(
        disagreements.is_empty(),
        "unrecorded disagreements with tree-sitter-clingo (read each against the authority):\n{}",
        disagreements.join("\n")
    );
}
```

Create the two registers with a header line each:
`# path  the reading against the authority that settled it` in
`AUTHORITY-DISAGREEMENTS` (an entry names a grammar §11 divergence or
a defect here that a stop resolved) and `TREE-SITTER-DISAGREEMENTS`.

- [ ] **Step 4: Run the differential and triage**

Run: `pixi run differential`
Expected: the membership and kinds test compares every input not
skipped and passes, or lists unrecorded disagreements. Triage each: a
parser defect is repaired here; a genuine disagreement between the
grammar of record and the authority is a divergence for grammar §11 —
raised at the stage close, never absorbed — and recorded meanwhile in
`AUTHORITY-DISAGREEMENTS` with the note "raised: divergence candidate";
an authority behavior the grammar already records as a divergence (D1's
inputs are skipped; another may surface) is recorded with the entry's
name. Then `pixi run measure-ceiling` — Task 18 reads
`target/differential/authority-ceiling.txt` — and `pixi run cross-check`,
triaged the same way into `TREE-SITTER-DISAGREEMENTS` (tree-sitter is
the secondary: where it disagrees with both this parser and the
authority, the note says so and the disagreement is recorded, not
repaired).

The differential's inputs with a comment inside a theory expression
(`DIFFERENTIAL-SKIP`, reason D1) are never handed to the authority;
confirm the register lists them.

- [ ] **Step 5: Run the gate, then commit**

Run the four gate commands (the differential feature is off in the
gate; the harness compiles to nothing there). Expected: green.

```bash
git add pixi.toml pixi.lock .gitignore crates/themelios-syntax
git commit -m "Add the differential against the pinned authority — membership, statement kinds, canonical spellings, the nesting ceiling per family — and the tree-sitter cross-check, both out of band under pixi"
```

---

### Task 18: The depth gate, the constants measured, and D2 recorded

**Files:**
- Create: `crates/themelios-syntax/tests/depth_gate.rs`
- Modify: `crates/themelios-syntax/src/parse/mod.rs` (the measured
  `MAX_NESTING_DEPTH`; both bounds in the rustdoc of both constants),
  `docs/grammar.md` (§11 D2's two recorded values, its obligation),
  `crates/themelios-syntax/tests/golden/diagnostics/nesting-too-deep-annotated.txt`
  (re-blessed)

**Derives:** syntax.md §5.4 (law 3), §6.6 (the constants; the value
measured against two bounds; the gate's bound governs on conflict),
§12.3 (every walk iterative or grammar-bounded, held per walk), §14
(rowan's drop, equality, and debug rendering recurse in tree depth),
§16 (the depth gate: a thread of exactly `REQUIRED_STACK_BYTES`,
inputs nested far beyond the constant in every self-recursive family
and the bracket-free chains beside them, every walk run, no overflow,
the deepest tree measured against the bound, the headroom measured);
grammar §11 D2 (both measured values recorded beside the entry).

**Interfaces:**
- Consumes: the whole crate; `target/differential/authority-ceiling.txt`
  from Task 17.
- Produces: the gate; the measured constant; the record in the rustdoc
  and in grammar §11 D2.

- [ ] **Step 1: Write the depth gate and the measurement probe**

`crates/themelios-syntax/tests/depth_gate.rs`:

```rust
//! The depth gate (docs/design/syntax.md §6.6, §16): on a thread of
//! exactly `REQUIRED_STACK_BYTES`, inputs nested far beyond
//! `MAX_NESTING_DEPTH` in every self-recursive family — and beside them
//! the bracket-free chains that must not deepen the tree — parsed,
//! walked through the typed AST, attached, certified, printed, compared,
//! and dropped: no overflow, the refusal for the brackets and none for
//! the chains, the deepest tree measured against law 3's bound, and the
//! same again on half the stack at the constant itself — the headroom.
//! What it proves: this crate's and rowan's walks complete on the stated
//! stack over the deepest tree the parser builds. What it cannot: a
//! consumer's own recursion over the typed AST.
//!
//! The measurement (`--ignored measure_the_constant`) finds, on half the
//! stack, the largest frame depth every walk survives over trees of the
//! deepest per-frame shape, built directly and probed one process at a
//! time — an overflow ends a process, not a test.

use std::env;
use std::process::Command;
use std::thread;

use rowan::{GreenNodeBuilder, Language};
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::Term;
use themelios_syntax::attach::attachments;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{equivalent, non_whitespace_tokens, Certificate};
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    parse, parse_term, parse_term_value, Parse, FIXED_LAYERS, MAX_NESTING_DEPTH, MAX_TREE_DEPTH,
    REQUIRED_STACK_BYTES, TERM_LAYERS_PER_FRAME,
};
use themelios_syntax::tree::{Asp, AstNode, NodeOrToken, SyntaxKind, SyntaxNode, WalkEvent};

/// The headroom factor: the gate's claim holds on half the stack at the
/// constant.
const HEADROOM: usize = 2;
/// How far beyond the constant the gate nests.
const BEYOND: u32 = 4;
/// The measured constant is rounded down to a multiple of this.
const GRANULE: u32 = 1_000;

fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

/// The deepest per-frame shape a term frame takes (docs/design/syntax.md
/// §5.4 law 3; `TERM_LAYERS_PER_FRAME`): a function frame whose operand
/// runs through every precedence level and a unary run into the next
/// frame.
fn maximal_term(frames: usize) -> String {
    format!("{}x{}", "f(1..2^3?4&5+6*7**-".repeat(frames), ")".repeat(frames))
}

/// One input per family at `frames` frames, and how it is parsed: a
/// program text, or a term / term-value fragment.
fn family_inputs(frames: usize) -> Vec<(&'static str, String, Entry)> {
    let n = frames;
    vec![
        ("term: the deepest frame shape", maximal_term(n), Entry::Term),
        ("term: parentheses", format!("{}x{}", "(".repeat(n), ")".repeat(n)), Entry::Term),
        ("term: absolute value", format!("{}x{}", "|".repeat(n), "|".repeat(n)), Entry::Term),
        ("term: pools", format!("{}x{}", "(".repeat(n), ";y)".repeat(n)), Entry::Term),
        ("term value: function arguments", format!("{}x{}", "f(".repeat(n), ")".repeat(n)), Entry::TermValue),
        ("constant term: function arguments", format!("#const c = {}x{}.", "f(".repeat(n), ")".repeat(n)), Entry::Program),
        ("theory term: sets", format!("&a {{ {}x{} }}.", "{".repeat(n), "}".repeat(n)), Entry::Program),
        ("theory term: lists", format!("&a {{ {}x{} }}.", "[".repeat(n), "]".repeat(n)), Entry::Program),
        ("theory term: tuples", format!("&a {{ {}x{} }}.", "(".repeat(n), ")".repeat(n)), Entry::Program),
        ("theory term: functions", format!("&a {{ {}x{} }}.", "f(".repeat(n), ")".repeat(n)), Entry::Program),
    ]
}

/// The bracket-free shapes that must not deepen the tree.
fn chain_inputs(length: usize) -> Vec<(&'static str, String)> {
    vec![
        ("additive chain", vec!["1"; length].join("+")),
        ("exponentiation chain", vec!["2"; length].join("**")),
        ("unary run", format!("{}x", "-".repeat(length))),
    ]
}

#[derive(Clone, Copy)]
enum Entry {
    Program,
    Term,
    TermValue,
}

/// Every walk over one parse — the walks the design names — and the
/// tree's depth. Runs on the caller's thread: call it on the gate's.
fn walks<T: AstNode<Language = Asp>>(one: &Parse<T>, two: &Parse<T>) -> usize {
    let root = one.syntax();
    let rendered = format!("{root:#?}");
    assert!(!rendered.is_empty());
    assert_eq!(one, two, "two parses of one text compare equal (rowan's equality recurses)");
    let terms = root.descendants().filter_map(Term::cast).count();
    let _ = terms;
    let attached = attachments(&root).count();
    let _ = attached;
    let tokens = non_whitespace_tokens(&root).count();
    assert!(tokens > 0);
    assert_eq!(equivalent(one, two, Certificate::LayoutOnly), Ok(()));
    assert_eq!(equivalent(one, two, Certificate::UpToSpelling), Ok(()));
    depth(&root)
}

fn parse_twice(text: &str, entry: Entry) -> (usize, bool) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    match entry {
        Entry::Program => {
            let one = parse(&source, Dialect::Clingo);
            let two = parse(&source, Dialect::Clingo);
            let refused = one.diagnostics().iter().any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
        Entry::Term => {
            let one = parse_term(&Lexer::new(&source, Dialect::Clingo));
            let two = parse_term(&Lexer::new(&source, Dialect::Clingo));
            let refused = one.diagnostics().iter().any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
        Entry::TermValue => {
            let one = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
            let two = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
            let refused = one.diagnostics().iter().any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
    }
}

/// The gate's body at `frames` frames: every family, then the chains.
/// Returns the deepest tree seen.
fn gate_body(frames: usize, expect_refusal: bool) -> usize {
    let mut deepest = 0usize;
    for (family, text, entry) in family_inputs(frames) {
        let (tree_depth, refused) = parse_twice(&text, entry);
        assert_eq!(refused, expect_refusal, "{family} at {frames} frames");
        assert!(tree_depth <= MAX_TREE_DEPTH as usize, "{family}: {tree_depth} exceeds the bound");
        deepest = deepest.max(tree_depth);
    }
    for (chain, text) in chain_inputs(frames) {
        let (tree_depth, refused) = parse_twice(&text, Entry::Term);
        assert!(!refused, "{chain}: a chain is never refused");
        assert!(tree_depth <= FIXED_LAYERS as usize, "{chain}: a chain does not deepen the tree");
    }
    deepest
}

fn on_stack<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(bytes: usize, body: F) -> R {
    thread::Builder::new()
        .name(format!("depth-gate-{bytes}"))
        .stack_size(bytes)
        .spawn(body)
        .expect("the gate's thread spawns")
        .join()
        .expect("the gate's thread completes")
}

#[test]
fn the_depth_gate() {
    // Far beyond the constant, on the full stack: refused, walked, dropped.
    let beyond = MAX_NESTING_DEPTH as usize * BEYOND as usize;
    let deepest_beyond = on_stack(REQUIRED_STACK_BYTES, move || gate_body(beyond, true));
    assert!(deepest_beyond <= MAX_TREE_DEPTH as usize);
    // At the constant, on half the stack: admitted, walked, dropped — the
    // headroom — and the deepest shape measured against the bound exactly.
    let at = MAX_NESTING_DEPTH as usize;
    let deepest_at = on_stack(REQUIRED_STACK_BYTES / HEADROOM, move || gate_body(at, false));
    assert!(deepest_at <= MAX_TREE_DEPTH as usize);
    let expected_deepest = 2 + at * TERM_LAYERS_PER_FRAME as usize;
    assert_eq!(
        deepest_at, expected_deepest,
        "the deepest frame shape realizes TERM_LAYERS_PER_FRAME layers per frame under the term root and its leaf"
    );
}

// ---- the measurement --------------------------------------------------

/// A tree of the deepest per-frame shape, `frames` deep, built directly:
/// what the parser builds for `maximal_term(frames)`, without the
/// parser's constant in the way.
fn build_maximal(frames: usize) -> SyntaxNode {
    let raw = Asp::kind_to_raw;
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(raw(SyntaxKind::TERM_FRAGMENT));
    let levels = [
        (SyntaxKind::DOTDOT, "1", ".."),
        (SyntaxKind::CARET, "2", "^"),
        (SyntaxKind::QUESTION, "3", "?"),
        (SyntaxKind::AMPERSAND, "4", "&"),
        (SyntaxKind::PLUS, "5", "+"),
        (SyntaxKind::STAR, "6", "*"),
        (SyntaxKind::STAR_STAR, "7", "**"),
    ];
    for _ in 0..frames {
        builder.start_node(raw(SyntaxKind::FUNCTION_TERM));
        builder.token(raw(SyntaxKind::IDENT), "f");
        builder.start_node(raw(SyntaxKind::ARGUMENTS));
        builder.token(raw(SyntaxKind::L_PAREN), "(");
        builder.start_node(raw(SyntaxKind::TUPLE));
        for (kind, operand, operator) in levels {
            builder.start_node(raw(SyntaxKind::BINARY_TERM));
            builder.start_node(raw(SyntaxKind::CONSTANT_TERM));
            builder.token(raw(SyntaxKind::NUMBER), operand);
            builder.finish_node();
            builder.token(raw(kind), operator);
        }
        builder.start_node(raw(SyntaxKind::UNARY_TERM));
        builder.token(raw(SyntaxKind::MINUS), "-");
    }
    builder.start_node(raw(SyntaxKind::CONSTANT_TERM));
    builder.token(raw(SyntaxKind::IDENT), "x");
    builder.finish_node();
    for _ in 0..frames {
        builder.finish_node(); // UNARY_TERM
        for _ in levels {
            builder.finish_node(); // BINARY_TERM
        }
        builder.finish_node(); // TUPLE
        builder.token(raw(SyntaxKind::R_PAREN), ")");
        builder.finish_node(); // ARGUMENTS
        builder.finish_node(); // FUNCTION_TERM
    }
    builder.finish_node();
    SyntaxNode::new_root(builder.finish())
}

/// The walks over a built tree, on the caller's thread.
fn walks_over_built(frames: usize) {
    let one = build_maximal(frames);
    let two = build_maximal(frames);
    let rendered = format!("{one:#?}");
    assert!(!rendered.is_empty());
    assert!(one.green() == two.green(), "structural equality recurses");
    let _ = one.descendants().filter_map(Term::cast).count();
    let _ = attachments(&one).count();
    let _ = non_whitespace_tokens(&one).count();
    assert!(depth(&one) >= frames * TERM_LAYERS_PER_FRAME as usize);
    drop(one);
    drop(two);
}

const PROBE: &str = "THEMELIOS_DEPTH_PROBE";

/// The probe entry: run by the measurement in a child process, with
/// `THEMELIOS_DEPTH_PROBE=<frames>,<stack bytes>`; a no-op otherwise.
#[test]
fn probe_entry() {
    let Ok(spec) = env::var(PROBE) else { return };
    let (frames, bytes) = spec.split_once(',').expect("frames,bytes");
    let frames: usize = frames.parse().expect("frames");
    let bytes: usize = bytes.parse().expect("bytes");
    on_stack(bytes, move || walks_over_built(frames));
}

/// Whether every walk survives `frames` frames on `bytes` of stack, in a
/// child process.
fn survives(frames: usize, bytes: usize) -> bool {
    Command::new(env::current_exe().expect("this test binary"))
        .args(["--exact", "probe_entry", "--test-threads=1", "--nocapture"])
        .env(PROBE, format!("{frames},{bytes}"))
        .status()
        .expect("the probe runs")
        .success()
}

/// The measurement (docs/design/syntax.md §6.6): on half the required
/// stack, the largest frame depth every walk survives over trees of the
/// deepest per-frame shape, by doubling then bisection; the constant is
/// the largest multiple of the granule not above it.
#[test]
#[ignore = "out of band: cargo test -p themelios-syntax --test depth_gate -- --ignored measure_the_constant --nocapture"]
fn measure_the_constant() {
    let stack = REQUIRED_STACK_BYTES / HEADROOM;
    let mut low = 1usize;
    let mut high = 2usize;
    while survives(high, stack) {
        low = high;
        high *= 2;
        assert!(high < 1 << 26, "no overflow found: the stack is larger than any tree the granule can name");
    }
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if survives(middle, stack) {
            low = middle;
        } else {
            high = middle;
        }
    }
    let constant = u32::try_from(low).expect("fits") / GRANULE * GRANULE;
    println!("on {stack} bytes every walk survives {low} frames of the deepest shape and fails at {high};");
    println!("MAX_NESTING_DEPTH = {constant} (the largest multiple of {GRANULE} not above {low})");
}
```

- [ ] **Step 2: Measure**

Run: `cargo test -p themelios-syntax --test depth_gate -- --ignored measure_the_constant --nocapture`
Expected: the two lines printed — `low`, `high`, and the constant. Read
`target/differential/authority-ceiling.txt` (Task 17) beside it. Record
all of it in the commit message.

- [ ] **Step 3: Fix the constant and record both bounds**

In `src/parse/mod.rs`, replace the provisional constant and its rustdoc:

```rust
/// The deepest nesting of bracket contexts — frames, one per open
/// bracket (docs/design/syntax.md §6.2) — the parser will open. Named
/// because it carries meaning; its value is fixed by measurement between
/// two bounds and recorded here with both (docs/design/syntax.md §6.6).
///
/// **From above (the gate's bound, which governs):** on a thread of half
/// `REQUIRED_STACK_BYTES`, every walk this crate performs or hands out —
/// dropping, comparing, rendering, walking the typed AST, attaching,
/// certifying — survives trees of the deepest per-frame shape to
/// `<low>` frames and fails at `<high>` (rowan 0.17.0, measured
/// `<date>` by `tests/depth_gate.rs`); the constant is the largest
/// multiple of 1,000 not above `<low>`.
///
/// **From below (the authority's ceiling, docs/grammar.md §11 D2):**
/// clingo v5.8.2 accepts nesting to `<per family, from
/// target/differential/authority-ceiling.txt>` and `<its failure mode>`
/// beyond; the corpus reaches no depth near the constant. Where the
/// authority's ceiling lies above the constant, the inputs between are
/// D2's band — admitted by the authority, refused here — recorded beside
/// that entry; safety over parity.
pub const MAX_NESTING_DEPTH: u32 = <constant>;
```

and add to `REQUIRED_STACK_BYTES`'s rustdoc the sentence: "Measured
together with `MAX_NESTING_DEPTH` (see its record); a move of either
re-measures the other." Fill every `<…>` from the measurement.

In `docs/grammar.md`, §11 D2, append to the entry (after "a pin move
re-measures.") the record its obligation asks for:

```
  Recorded <date>: the syntax tier's constant is <constant> frames
  (`themelios-syntax`'s `MAX_NESTING_DEPTH`, set by its depth instrument
  on half its required stack of 64 MiB); the authority at v5.8.2, per
  family — <family: last depth accepted / first depth failing / mode>,
  … — so the band is <constant + 1> to the authority's ceiling in every
  family <or: none, where the ceiling lies below>.
```

If the authority's failure mode is not a refusal but its process
ending, D2's sentence "it refuses input nested past its parser-stack
ceiling" no longer describes the pin: raise it at the stage close as a
grammar defect (the where-documents-disagree rule); record the measured
mode here regardless.

- [ ] **Step 4: Run the gate at the measured constant, re-bless what depends on it**

Run: `cargo test -p themelios-syntax --test depth_gate the_depth_gate`
Expected: passes on the full stack beyond the constant and on half the
stack at it, with the deepest shape's depth exactly
`2 + MAX_NESTING_DEPTH * TERM_LAYERS_PER_FRAME`. Then
`GOLDEN_BLESS=1 cargo test -p themelios-syntax --test golden nesting_too_deep`
and read the re-blessed `nesting-too-deep-annotated.txt` (the window
moves with the constant; the rendering is otherwise the one accepted at
Task 12), then `cargo test -p themelios-syntax`.
Expected: green.

- [ ] **Step 5: Run the full gate, then commit**

Run the four gate commands. Expected: green.

```bash
git add crates/themelios-syntax docs/grammar.md
git commit -m "Add the depth gate; fix MAX_NESTING_DEPTH by measurement between the gate's bound and the authority's ceiling, both recorded beside the constants and in grammar §11 D2"
```

---

### Task 19: Scaling shapes — the assertions in the gate and the criterion benches out of band

**Files:**
- Create: `crates/themelios-syntax/tests/scaling_shape.rs`,
  `crates/themelios-syntax/benches/scaling.rs`

**Derives:** syntax.md §4.6, §6.8, §9.3, §10.2, §11.3, §13 (the costs,
consolidated), §16 (scaling shapes: parse linear in text; the
certificate linear in both texts; bulk attachment linear in the tree;
the oracle constant per pair — shape assertions in the gate, absolute
numbers out of band); spec §10.1–§10.2.

**Interfaces:**
- Consumes: `parse`, `equivalent`, `attachments`, `separator`, the
  corpus's shapes.
- Produces: the shape suite the gate runs and the bench harness
  `cargo bench -p themelios-syntax` runs per milestone.

- [ ] **Step 1: Write the shape assertions**

`crates/themelios-syntax/tests/scaling_shape.rs`:

```rust
//! CI shape assertions (docs/design/syntax.md §16): complexity shape
//! only, held by median-of-five wall-clock ratios with tolerances wide
//! enough for any CI machine — parse linear in text, the certificate
//! linear in both texts, bulk attachment linear in the tree, the oracle
//! constant per pair. What they prove: the claimed class (a quadratic
//! parse, a re-scanning attachment, a certificate that re-walks). What
//! they cannot: absolute speed — that lives in the out-of-band benches.

use std::time::Instant;

use themelios_base::source::{Source, SourceId};
use themelios_syntax::attach::attachments;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{equivalent, Certificate};
use themelios_syntax::fusion::separator;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::SyntaxKind;

/// One rule with a comment run and a theory atom, so every size
/// exercises the parser's families, the comment run, and the modes.
const UNIT: &str = "% leading\np(X, f(Y)) :- q(X; Y), not r(X), X = 1..3, #sum { W,T : t(T,W) } >= 4, &sum { x, -y : p } <= 3. % trailing\n";

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: usize = 16;
/// A linear claim at SIZE_RATIO may cost at most this factor: fourfold
/// noise headroom above linear (x16) and fourfold separation below
/// quadratic (x256).
const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
/// A constant-per-pair claim over SIZE_RATIO more pairs may cost at most
/// this factor per pair (the same fourfold headroom, on the per-pair
/// figure).
const CONSTANT_CEILING: u128 = 4;
/// Wall-clock samples per measurement; the median of them is taken.
const SAMPLES: usize = 5;

fn text_of(units: usize) -> String {
    UNIT.repeat(units)
}

fn admitted(units: usize) -> Source {
    Source::new(SourceId::new(0), text_of(units)).expect("test text admits")
}

fn median_nanos(mut work: impl FnMut()) -> u128 {
    let mut samples = Vec::new();
    for _ in 0..SAMPLES {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[SAMPLES / 2].max(1)
}

#[test]
fn parse_is_linear_in_the_text() {
    let small_source = admitted(64);
    let big_source = admitted(64 * SIZE_RATIO);
    let small = median_nanos(|| {
        std::hint::black_box(parse(&small_source, Dialect::Clingo));
    });
    let big = median_nanos(|| {
        std::hint::black_box(parse(&big_source, Dialect::Clingo));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "parse scaled {small}ns -> {big}ns over x{SIZE_RATIO} text; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn the_certificate_is_linear_in_both_texts() {
    let small_source = admitted(64);
    let big_source = admitted(64 * SIZE_RATIO);
    let small_left = parse(&small_source, Dialect::Clingo);
    let small_right = parse(&small_source, Dialect::Clingo);
    let big_left = parse(&big_source, Dialect::Clingo);
    let big_right = parse(&big_source, Dialect::Clingo);
    let small = median_nanos(|| {
        std::hint::black_box(equivalent(&small_left, &small_right, Certificate::UpToSpelling)).expect("equal");
    });
    let big = median_nanos(|| {
        std::hint::black_box(equivalent(&big_left, &big_right, Certificate::UpToSpelling)).expect("equal");
    });
    assert!(
        big < small * LINEAR_CEILING,
        "equivalent scaled {small}ns -> {big}ns over x{SIZE_RATIO} text; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn bulk_attachment_is_linear_in_the_tree() {
    let small_root = parse(&admitted(64), Dialect::Clingo).syntax();
    let big_root = parse(&admitted(64 * SIZE_RATIO), Dialect::Clingo).syntax();
    let small = median_nanos(|| {
        std::hint::black_box(attachments(&small_root).count());
    });
    let big = median_nanos(|| {
        std::hint::black_box(attachments(&big_root).count());
    });
    assert!(
        big < small * LINEAR_CEILING,
        "attachments scaled {small}ns -> {big}ns over x{SIZE_RATIO} tree; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn the_oracle_is_constant_per_pair() {
    let run = |units: usize| {
        let root = parse(&admitted(units), Dialect::Clingo).syntax();
        let tokens: Vec<_> = root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
            .collect();
        let pairs = tokens.len() - 1;
        let total = median_nanos(|| {
            for pair in tokens.windows(2) {
                std::hint::black_box(separator(&pair[0], &pair[1], Dialect::Clingo));
            }
        });
        total / pairs as u128
    };
    let small_per_pair = run(64);
    let big_per_pair = run(64 * SIZE_RATIO);
    assert!(
        big_per_pair < small_per_pair.max(1) * CONSTANT_CEILING,
        "the oracle's per-pair cost went {small_per_pair}ns -> {big_per_pair}ns over x{SIZE_RATIO} pairs; constant per pair allows at most x{CONSTANT_CEILING}"
    );
}
```

- [ ] **Step 2: Write the benches**

`crates/themelios-syntax/benches/scaling.rs`:

```rust
//! Out-of-band absolute numbers behind the shape claims
//! (docs/design/syntax.md §16; docs/specification.md §10.2). Run per
//! milestone: `cargo bench -p themelios-syntax`. These prove wall-clock
//! magnitudes on one machine; they cannot prove complexity class — the
//! CI shape test holds that.

use criterion::Criterion;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::attach::attachments;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{equivalent, Certificate};
use themelios_syntax::fusion::separator;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::SyntaxKind;

const UNIT: &str = "% leading\np(X, f(Y)) :- q(X; Y), not r(X), X = 1..3, #sum { W,T : t(T,W) } >= 4, &sum { x, -y : p } <= 3. % trailing\n";

fn admitted(units: usize) -> Source {
    Source::new(SourceId::new(0), UNIT.repeat(units)).expect("bench text admits")
}

fn parsing(c: &mut Criterion) {
    for units in [64usize, 1024, 8192] {
        let source = admitted(units);
        c.bench_function(&format!("parse/{units}-rules"), |b| {
            b.iter(|| parse(std::hint::black_box(&source), Dialect::Clingo));
        });
    }
}

fn certifying(c: &mut Criterion) {
    for units in [64usize, 1024] {
        let source = admitted(units);
        let left = parse(&source, Dialect::Clingo);
        let right = parse(&source, Dialect::Clingo);
        c.bench_function(&format!("equivalent/{units}-rules"), |b| {
            b.iter(|| equivalent(std::hint::black_box(&left), &right, Certificate::UpToSpelling));
        });
    }
}

fn attaching(c: &mut Criterion) {
    for units in [64usize, 1024] {
        let root = parse(&admitted(units), Dialect::Clingo).syntax();
        c.bench_function(&format!("attachments/{units}-rules"), |b| {
            b.iter(|| attachments(std::hint::black_box(&root)).count());
        });
    }
}

fn oracle(c: &mut Criterion) {
    let root = parse(&admitted(64), Dialect::Clingo).syntax();
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .collect();
    c.bench_function("separator/every-pair-of-64-rules", |b| {
        b.iter(|| {
            for pair in tokens.windows(2) {
                std::hint::black_box(separator(&pair[0], &pair[1], Dialect::Clingo));
            }
        });
    });
}

// The harness, written out rather than generated by `criterion_group!`
// and `criterion_main!`: the macros expand to exactly this, and the
// generated group is a public function without documentation, which
// the workspace's denied `missing_docs` refuses. Same behavior, one
// documented item.

/// The scaling group: every bench above, under criterion's default
/// configuration as adjusted by the command line.
pub fn scaling() {
    let mut criterion: Criterion = Criterion::default().configure_from_args();
    parsing(&mut criterion);
    certifying(&mut criterion);
    attaching(&mut criterion);
    oracle(&mut criterion);
}

fn main() {
    scaling();
    Criterion::default().configure_from_args().final_summary();
}
```

- [ ] **Step 3: Run the shapes, the bench harness, the gate; commit**

Run: `cargo test -p themelios-syntax --test scaling_shape` (Expected: 4
passed), `cargo bench -p themelios-syntax -- --test` (Expected: the
harness runs every bench once), then the four gate commands.

```bash
git add crates/themelios-syntax
git commit -m "Add the scaling shapes: parse, certificate, and bulk attachment linear, the oracle constant per pair — asserted in the gate, measured out of band"
```

---

### Task 20: Stage close — the witness seeds, the plain-data assertions, the worked example, the coverage floor, the mutation audit, the failure walk

**Files:**
- Create: `crates/themelios-syntax/examples/comments_as_data.rs`,
  `crates/themelios-syntax/examples/diagnostics_quality.rs`,
  `crates/themelios-syntax/examples/hostile_input.rs`,
  `crates/themelios-syntax/examples/asp_core_2.rs`
- Modify: `crates/themelios-syntax/src/lib.rs` (the worked example),
  `crates/themelios-syntax/tests/trust.rs` (the plain-data assertion),
  `.github/workflows/gate.yml` (the examples run; the coverage floor),
  `.cargo/mutants.toml` (accepted survivors, argued)

**Derives:** syntax.md §2 (the failure conditions, walked), §5.1 and
§5.5 (`Parse<T>: Send + Sync` for every root, asserted at compile time),
§12.2 (every other public type plain data), §13 (the table
cross-checked against the code), §15 (the surface a formatter consumes,
exercised by the seeds), §16 (standing gates: mutation per milestone,
the coverage floor, documentation examples that run, the executable-
claims standard; the witnesses this tier seeds — comments-as-data,
diagnostics-quality, the syntax half of hostile-input, the parse half of
asp-core-2); spec §3 (the witnesses), §10.1–§10.2, §10.4.

**Interfaces:**
- Consumes: everything.
- Produces: the stage's standing gates, armed; the four executed
  examples the facade re-hosts at stage 8; no public surface.

- [ ] **Step 1: Write the four witness seeds**

`crates/themelios-syntax/examples/comments_as_data.rs`:

```rust
//! The syntax tier's seed of the *comments-as-data* witness
//! (docs/specification.md §3): a program bearing comments is parsed;
//! each comment and its attachment — trailing, leading, dangling — is
//! retrieved through the public API; the tree's text is the input, byte
//! for byte, so an emit preserves every comment.

use themelios_syntax::attach::{attachment, attachments, comments, Slot};
use themelios_syntax::base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{role, TokenRole};

fn main() {
    let text = "% every route is a road or a rail\nroute(X, Y) :- road(X, Y). % roads\nroute(X, Y) :- rail(X, Y). % rails\n\n% unreachable? not from here\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the program admits");
    let parse = parse(&source, Dialect::Clingo);
    assert!(!parse.has_errors());
    assert_eq!(parse.syntax().text(), text, "emit preserves every byte");
    let root = parse.syntax();
    let mut seen = 0;
    for (comment, attachment) in attachments(&root) {
        seen += 1;
        assert_eq!(role(&comment), TokenRole::Trivia);
        assert_eq!(attachment(&comment).as_ref(), Ok(&attachment));
        println!("{:?} -> {:?} of {}", comment.text(), attachment.slot, attachment.anchor.kind());
    }
    assert_eq!(seen, 4);
    let program = themelios_syntax::tree::SyntaxElement::Node(root.clone());
    assert_eq!(comments(&program, Slot::Dangling).count(), 1, "the comment above the trailing gap dangles in the program");
    let first_rule = themelios_syntax::tree::SyntaxElement::Node(root.children().next().expect("a rule"));
    assert_eq!(comments(&first_rule, Slot::Leading).count(), 1);
    assert_eq!(comments(&first_rule, Slot::Trailing).count(), 1);
}
```

`crates/themelios-syntax/examples/diagnostics_quality.rs`:

```rust
//! The syntax tier's seed of the *diagnostics-quality* witness
//! (docs/specification.md §3): characteristic malformed programs fed to
//! the parser; every diagnostic a typed value with a stable identity
//! and a precise span, rendered through the base tier's human view —
//! the renderings themselves are held to their reviewed goldens by
//! `tests/golden.rs`.

use themelios_syntax::base::diagnostic::ToDiagnostic;
use themelios_syntax::base::source::{Source, SourceSet};
use themelios_syntax::base::view::human;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn main() {
    let programs = ["p(X) :- q(X) r(X).\n", "p(a, b,).\n", "#heuristic a : b.\n", "p(\"a\\qb\"). q.\n", ":- #count { X : p(X) .\n"];
    let mut catalog = SourceSet::new();
    for text in programs {
        let file = catalog.add("input.lp".to_owned(), text.to_owned()).expect("admits");
        let source = Source::new(file, text.to_owned()).expect("admits");
        let parse = parse(&source, Dialect::Clingo);
        assert!(parse.has_errors());
        for diagnostic in parse.diagnostics() {
            assert_eq!(diagnostic.id().namespace(), "syntax");
            assert!(!diagnostic.primary().span.is_empty() || text.ends_with(' '), "a precise span");
            println!("{}", human(&diagnostic.to_diagnostic(), &catalog));
        }
    }
}
```

`crates/themelios-syntax/examples/hostile_input.rs`:

```rust
//! The syntax half of the *hostile-input* witness
//! (docs/specification.md §3, §12.4): the public surface meets
//! adversarial, malformed, and absurdly deep input the way a service
//! boundary receives it — bytes that are not UTF-8 refused at admission
//! with a typed refusal; a megabyte of what begins no token, one token
//! and one diagnostic; nesting past the constant, one refusal with a
//! locus; every unterminated construct, a typed diagnostic; no panic.

use themelios_syntax::base::source::{FromBytesRefusal, Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{parse, MAX_NESTING_DEPTH};

fn main() {
    let refused = Source::from_bytes(SourceId::new(0), vec![0xff, 0xfe, b'p', b'.']);
    assert!(matches!(refused, Err(FromBytesRefusal::InvalidUtf8(_))));

    let hostile = "$".repeat(1024 * 1024);
    let source = Source::new(SourceId::new(1), hostile.clone()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    assert_eq!(parse.syntax().text(), hostile);
    assert_eq!(parse.diagnostics().len(), 1, "one error token, one diagnostic");

    let depth = MAX_NESTING_DEPTH as usize + 1;
    let deep = format!("p({}x{}).", "f(".repeat(depth), ")".repeat(depth));
    let source = Source::new(SourceId::new(2), deep.clone()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    assert_eq!(parse.syntax().text(), deep);
    assert_eq!(parse.diagnostics().len(), 1);
    assert_eq!(parse.diagnostics()[0].id().name(), "nesting-too-deep");

    for text in ["p(\"abc", "%* open", "#script (lua) x = 1", "p(X) :- q(X", "&a { x"] {
        let source = Source::new(SourceId::new(3), text.to_owned()).expect("admits");
        let parse = parse(&source, Dialect::Clingo);
        assert!(parse.has_errors());
        assert!(parse.is_incomplete(), "{text:?} is incomplete, not wrong");
    }
    println!("every hostile input answered with a typed refusal or a typed diagnostic");
}
```

`crates/themelios-syntax/examples/asp_core_2.rs`:

```rust
//! The parse half of the *asp-core-2* witness (docs/specification.md
//! §3): a conformant ASP-Core-2 program — query included — parsed under
//! the declared dialect; the query stands as the program's last
//! statement; the standard's string reading holds.

use themelios_syntax::ast::{Constant, Head, LiteralInner, Statement, Term};
use themelios_syntax::base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn main() {
    let text = "node(1..3). edge(1,2). edge(2,3).\nreach(X) :- node(X), start(X).\nreach(Y) :- reach(X), edge(X,Y).\nlabel(\"a\\b\").\nstart(1).\nreach(3)?\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::AspCore2);
    assert!(!parse.has_errors(), "a conformant program is a member: {:?}", parse.diagnostics());
    let statements: Vec<Statement> = parse.tree().statements().collect();
    assert!(matches!(statements.last(), Some(Statement::Query(_))), "the query holds the last position");
    let Some(Statement::Rule(label)) = statements.get(5) else { panic!("the labelled fact") };
    let Some(Head::Literal(literal)) = label.head() else { panic!("a literal head") };
    let Some(LiteralInner::Atom(atom)) = literal.inner() else { panic!("an atom") };
    let tuple = atom.arguments().expect("arguments").alternatives().next().expect("a tuple");
    let Some(Term::Constant(constant)) = tuple.terms().next() else { panic!("a constant") };
    let Some(Constant::String(string)) = constant.constant() else { panic!("a string") };
    assert_eq!(parse.string_value(&string).expect("valid"), "a\\b", "the standard's string rule: a backslash is itself");
    println!("parsed {} statements under the ASP-Core-2 dialect, the query last", statements.len());
}
```

Add to `.github/workflows/gate.yml`, in the `gate` job after the test
step:

```yaml
      # The witnesses this tier seeds (docs/design/syntax.md §16): executed,
      # not merely compiled (docs/specification.md §10.1).
      - run: |
          for example in comments_as_data diagnostics_quality hostile_input asp_core_2; do
            cargo run -p themelios-syntax --example "$example" --locked
          done
```

Run the four locally: `for e in comments_as_data diagnostics_quality hostile_input asp_core_2; do cargo run -p themelios-syntax --example $e; done`
Expected: each prints and exits 0.

- [ ] **Step 2: The worked example on the crate's front page**

Extend the crate docs in `src/lib.rs` (below the existing paragraphs):

```rust
//! # A worked example
//!
//! ```
//! use themelios_syntax::ast::{Head, LiteralInner, Statement};
//! use themelios_syntax::attach::{attachments, Slot};
//! use themelios_syntax::base::source::{Source, SourceId};
//! use themelios_syntax::dialect::Dialect;
//! use themelios_syntax::equiv::{equivalent, Certificate};
//! use themelios_syntax::parse::parse;
//!
//! let text = "% a fact\np(1). q(X) :- p(X).\n";
//! let source = Source::new(SourceId::new(0), text.to_owned())?;
//! let parse = parse(&source, Dialect::Clingo);
//! assert!(!parse.has_errors());
//! assert_eq!(parse.syntax().text(), text);
//!
//! // The typed AST over the tree.
//! let Some(Statement::Rule(fact)) = parse.tree().statements().next() else { unreachable!() };
//! let Some(Head::Literal(head)) = fact.head() else { unreachable!() };
//! assert!(matches!(head.inner(), Some(LiteralInner::Atom(_))));
//!
//! // Attachment as API: the comment leads the fact.
//! let (comment, attachment) = attachments(&parse.syntax()).next().expect("a comment");
//! assert_eq!(comment.text(), "% a fact");
//! assert_eq!(attachment.slot, Slot::Leading);
//!
//! // The certificate a layout-only change earns.
//! let respaced = Source::new(SourceId::new(1), "% a fact\np(1).\nq(X):-p(X).\n".to_owned())?;
//! let again = parse(&respaced, Dialect::Clingo);
//! assert_eq!(equivalent(&parse, &again, Certificate::LayoutOnly), Ok(()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
```

Run: `cargo test -p themelios-syntax --doc`
Expected: the example runs and passes.

- [ ] **Step 3: Arm the plain-data assertion**

Append to `crates/themelios-syntax/tests/trust.rs`:

```rust
/// docs/design/syntax.md §5.1, §5.5, §12.2: every public value type is
/// plain data — `Send`, `Sync`, owned — and `Parse<T>` is for every root,
/// its `T` a view carried as a phantom. Compiles only while every listed
/// type is `Send + Sync`; the cursors, the typed wrappers, and
/// `Attachment` are the stated exception and are not listed.
#[test]
fn every_public_value_type_is_plain_data() {
    fn plain_data<T: Send + Sync>() {}
    plain_data::<themelios_syntax::parse::Parse<themelios_syntax::ast::Program>>();
    plain_data::<themelios_syntax::parse::Parse<themelios_syntax::ast::StatementFragment>>();
    plain_data::<themelios_syntax::parse::Parse<themelios_syntax::ast::TermFragment>>();
    plain_data::<themelios_syntax::parse::EntryPoint>();
    plain_data::<themelios_syntax::dialect::Dialect>();
    plain_data::<themelios_syntax::tree::SyntaxKind>();
    plain_data::<themelios_syntax::tree::TokenRole>();
    plain_data::<themelios_syntax::tree::GreenNode>();
    plain_data::<themelios_syntax::token::Token<'static>>();
    plain_data::<themelios_syntax::token::LexMode>();
    plain_data::<themelios_syntax::token::TokenSourceLawViolation>();
    plain_data::<themelios_syntax::lexer::Lexer<'static>>();
    plain_data::<themelios_syntax::diagnostic::SyntaxError>();
    plain_data::<themelios_syntax::diagnostic::SyntaxErrorKind>();
    plain_data::<themelios_syntax::diagnostic::Related>();
    plain_data::<themelios_syntax::diagnostic::RelatedLocus>();
    plain_data::<themelios_syntax::diagnostic::StringDefect>();
    plain_data::<themelios_syntax::diagnostic::RestrictedForm>();
    plain_data::<themelios_syntax::diagnostic::Restriction>();
    plain_data::<themelios_syntax::diagnostic::MisplacedDoc>();
    plain_data::<themelios_syntax::diagnostic::SourceBreach>();
    plain_data::<themelios_syntax::diagnostic::Hint>();
    plain_data::<themelios_syntax::diagnostic::Expected>();
    plain_data::<themelios_syntax::diagnostic::GrammarWord>();
    plain_data::<themelios_syntax::diagnostic::SyntaxClass>();
    plain_data::<themelios_syntax::ast::Negation>();
    plain_data::<themelios_syntax::ast::Relation>();
    plain_data::<themelios_syntax::ast::Precedence>();
    plain_data::<themelios_syntax::ast::Associativity>();
    plain_data::<themelios_syntax::ast::AggregateFunction>();
    plain_data::<themelios_syntax::ast::ConstPolicy>();
    plain_data::<themelios_syntax::ast::Radix>();
    plain_data::<themelios_syntax::ast::CommentForm>();
    plain_data::<themelios_syntax::ast::InvalidStringLiteral>();
    plain_data::<themelios_syntax::attach::Slot>();
    plain_data::<themelios_syntax::attach::NotAttachable>();
    plain_data::<themelios_syntax::fusion::Separator>();
    plain_data::<themelios_syntax::fusion::LexContext>();
    plain_data::<themelios_syntax::equiv::Certificate>();
    plain_data::<themelios_syntax::equiv::Mismatch>();
    plain_data::<themelios_syntax::equiv::Side>();
}
```

Run: `cargo test -p themelios-syntax --test trust`
Expected: 7 passed.

- [ ] **Step 4: Set the coverage floor**

Measure: `cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --summary-only`
Compute the floor by stage 1's rule: the measured line coverage rounded
down to a multiple of five, minus five — one number for the workspace.
If the number differs from the committed 90, write the new value into
`.github/workflows/gate.yml`'s coverage step (`--fail-under-lines <N>`)
and record the measurement in the commit message; a lower measurement
is not a reason to relax the rule's arithmetic. Run
`cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --locked --fail-under-lines <N>`.
Expected: green.

- [ ] **Step 5: Run the mutation milestone audit**

Run: `cargo mutants --package themelios-syntax`
(the per-milestone out-of-band audit — spec §10.2 — not a gate step;
`cargo install cargo-mutants` if absent). Triage every survivor, one by
one: a genuine test gap → the killing test, in the survivor's module or
law file, re-run, kill confirmed; an arm no test can reach → an entry in
`.cargo/mutants.toml`, the pattern copied from the printed survivor
description, the argument as a comment beside it. Expected end state:
every mutant caught, unviable, or excluded with a written argument.

- [ ] **Step 6: Walk the failure conditions and cross-check the table**

syntax.md §2 names the design's failure conditions. Verify each is held
and record the walk in the closing commit message:

- *A parse panics, diverges, or yields a tree whose text differs from
  its input* — the fuzz targets (Tasks 5, 12–16), the corpus harness's
  text law (Task 12), the totality of every entry (Task 7).
- *The parser admits or refuses an input the grammar does not, beyond
  D1 and D2* — the differential (Task 17) and its registers; every
  unrecorded disagreement raised.
- *A consumer needs a private API, a fork, or a second grammar* — the
  four seeds (Step 1) and the worked example use the public surface
  alone; the macro law: `parse_program` over any `TokenSource` (Task 7's
  breaching-source tests).
- *A diagnostic lacks a precise span or a stable identity, or its
  expected set is prose* — `SyntaxError` is located by construction, the
  identity table snapshot (Task 6), `ExpectedSet` a typed set.
- *Two consumers derive different attachments, or the answer changes
  under a transformation that preserved the four facts* — the inverse
  law and the re-spacing stability law (Task 14).
- *A certificate granted to a changed token or a moved comment; the
  oracle certifies a fusing adjacency* — the certificate laws (Task 16),
  the oracle laws (Task 15).
- *A walk recurses in the input's nesting; a tree can be dropped only
  with proportional stack* — the depth gate (Task 18) and
  `MAX_TREE_DEPTH`; verify no recursion over user structure remains:
  `rg -n "fn .*\(.*&self" crates/themelios-syntax/src` is not the
  instrument — read each walk in `attach.rs`, `equiv.rs`, `fusion.rs`,
  `ast/`, and the parser's four files, and confirm each is a loop over
  rowan's iterative cursors or the frame stack; record the reading.
- *A dependency beyond rowan's closure, unsafe code, a non-FFI-free
  closure* — `tests/trust.rs` (Task 1).
- *A parse depends on anything but its inputs; hidden mutation* — the
  determinism law over the corpus (Task 12), the fuzz targets, and:
  `rg -n "static|Mutex|RwLock|thread_local|std::fs|std::net|std::env|Cell" crates/themelios-syntax/src`
  Expected: no hits in `src/` (the tests and harnesses may).

Then cross-check syntax.md §13 row by row against the code: each row's
refusal column is exactly the operation's error type; every public
operation not in the table is total. Expected: exact agreement; a
divergence is a defect in the code, not the table.

- [ ] **Step 7: Final gate, clean tree**

Run the four gate commands, `cargo test -p themelios-syntax --doc`, the
four examples, and `cargo fuzz build -s none` from the crate directory,
from a clean working tree. Expected: green.

- [ ] **Step 8: Commit**

```bash
git add crates/themelios-syntax .github/workflows/gate.yml .cargo/mutants.toml
git commit -m "Stage close: the four witness seeds executed by the gate, the worked example, the plain-data assertion, the coverage floor, the mutation audit, the failure walk recorded"
```

---

**STOP — the stage close.** Three blind readings of the whole crate at
this commit against the design of record, then the security review;
their findings adjudicated item by item, applied, confirmed. The stage
exits through the first-consumer checkpoint (spec §11): morphe builds
against this surface next, in its own repository.

---

## Completion

This plan is done when every checkbox above is checked and, from a
clean tree in one sitting:

- [ ] the full gate is green (fmt, clippy as errors, tests including
  doctests, the corpus, the goldens, the depth gate, the shape suite,
  the trust checks; the four examples executed; doc build with warnings
  denied);
- [ ] `cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --locked --fail-under-lines <the committed floor>` passes;
- [ ] `cargo mutants --package themelios-syntax` reports every mutant
  caught, unviable, or excluded with a written argument;
- [ ] `cargo fuzz build -s none` builds both targets, and each has run
  without a crash for at least the minutes its task states; the fuzz
  corpus is committed;
- [ ] `pixi run differential`, `pixi run measure-ceiling`, and
  `pixi run cross-check` have run at this commit; every disagreement is
  recorded in its register with its reading, and every unresolved one is
  raised;
- [ ] `MAX_NESTING_DEPTH` carries its measured value with both bounds in
  its rustdoc, and grammar §11 D2 carries both values;
- [ ] `cargo bench -p themelios-syntax -- --test` runs the harness;
- [ ] every golden (diagnostics, recovery, trees, attachments) has been
  read and accepted by the reviewer, and the acceptance is recorded in a
  commit message;
- [ ] the Task 20 failure walk found every syntax.md §2 condition held;
- [ ] nothing from syntax.md §17 (reserved seams, non-goals) exists in
  the tree.

**Derivation coverage, for the reviewer:** syntax.md §1 → Task 1 (crate
facts, the base re-export, the module map across Tasks 2–16); §2 → the
Task 20 failure walk; §3 → Task 2 (`Dialect`), Task 3 (the two lexical
deltas), Task 9 (the query), Task 12 (the dialect-neutrality law), Task 13
(`StringLit::value`, `Parse::string_value`); §4.1 → Task 2; §4.2–§4.6 → Task 3; §5.1–§5.3 →
Task 2; §5.4 → Task 2 (`role`), Task 7 (laws 1, 2, 4), Task 8 (law 3),
Task 12 (held over the corpus); §5.5 → Task 7 (with Task 13's
`string_value`); §6.1 → Task 7; §6.2 → Task 8; §6.3 → Tasks 7 (aspif,
docs), 9 (query), 10 (theory regions), 11 (show-signature, script,
annotations); §6.4 → Task 7; §6.5 → Task 7 (`is_incomplete`), Task 12;
§6.6 → Tasks 8 and 18; §6.7 → Tasks 7–11 (the rows) and 12 (the
goldens); §6.8 → Task 19; §7 → Task 6; §8 → Task 13 (roots in Task 7);
§9 → Task 14; §10 → Tasks 4 and 15; §11 → Task 16 (§11.3's roster
choice with the differential's four-pair check in Task 17); §12 → the
idiom of every task, §12.5's impls landing with their types; §13 → the
per-operation rustdoc and the Task 20 cross-check; §14 → Task 1 (the
trust checks) and Task 18 (rowan's recursion, measured); §15 → the
public surface as built, exercised by Task 20's seeds; §16 → the fuzz
crate (Tasks 5, 12, 14–16), the property laws (Tasks 3, 14–16, with the
tree laws in Task 12), the differential (Task 17), the goldens (Tasks
12, 14, 18), the corpus (Task 12), the depth gate (Task 18), the scaling
shapes (Task 19), the identity table (Task 6), the trust checks (Task 1),
the standing gates (Task 20), the seeds (Task 20); §17 → nothing,
verified at close; Appendix A → Task 2 (the roster) and Task 13 (the
wrappers); Appendix B → Task 6.
