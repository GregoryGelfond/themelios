# themelios-base stage 1 — implementation plan

> **For the executor:** work strictly task by task, in the order given.
> Every step is a checkbox (`- [ ]`); check it only when its command has
> run and shown the expected result. Each task ends with the gate green
> (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`) and
> a commit. Stop for review between tasks; do not read ahead and batch.

**Goal:** build `themelios-base` — the source-text model, spans, line
indexing, the diagnostics model, and the views — exactly as designed,
with every stage-1 assurance instrument green.

**Architecture:** one zero-dependency crate, five modules with one
concern each — `source`, `span`, `line`, `diagnostic`, `view` — plain
owned data throughout, refusing constructors where invariants exist,
pure functions everywhere (base.md §1, §8).

**Tech stack:** Rust, floor and CI pin 1.97 (rustc 1.97.1 is current
stable); dev-dependencies exactly `proptest` and `criterion`, the two
instruments the design names (base.md §10); coverage (`cargo-llvm-cov`)
and mutation (`cargo-mutants`) run as externally installed cargo tools
and never enter any manifest.

**Design of record:** `docs/design/base.md` at commit `666f69d` — every
task derives from it and cites the sections it implements. Governing
context: `docs/specification.md` §10–§11 (instruments and build order).
Where this plan and base.md disagree, base.md governs and the
disagreement is a defect here.

## Postcondition

What this plan must be, stated so a review can check drift against it:

> A faithful and complete derivation of `docs/design/base.md` at
> `666f69d`: every task builds a surface, law, or instrument that
> document states for stage 1; every surface, law, and instrument it
> states for stage 1 is built by some task; and no task builds anything
> beyond it — no reserved seam (base.md §11), no non-goal, no surface
> the design does not state.

This plan has failed when any of the following holds: a public item in
any task departs from the design's signatures or stated semantics; a
design surface, property law, golden case, or standing gate from
base.md §3–§10 has no task; a task introduces a public surface,
dependency, or behavior the design does not state; or a task's steps
cannot be executed as written by an engineer holding only this
repository.

## Global constraints

Every task's requirements implicitly include all of these. Values are
copied from the design and specification; the citation is where the
argument lives.

- **Zero dependencies in the shipped closure.** `themelios-base`'s
  `[dependencies]` is empty, forever; `proptest` and `criterion` are
  dev-dependencies outside that claim (base.md §1; spec §12.5).
- **`#![forbid(unsafe_code)]`** at the crate root, plus workspace-level
  `unsafe_code = "deny"`, plus the structural trust checks: empty
  shipped dependency list, no build script (base.md §1; spec §12.3).
- **`rust-version = "1.97"`** declared via the workspace in every
  manifest; the CI toolchain pin is `1.97.1` and matches the floor
  (spec §10.1). No `rust-toolchain.toml`: the floor is a contract, the
  pin is CI reproducibility; local toolchains float at or above the
  floor.
- **Every public type is plain data:** `Send`, `Sync`, owned, no
  interior mutability, no global state, no I/O anywhere in the crate
  (base.md §1, §8.2).
- **Observational purity:** every public operation's result depends
  only on its explicit inputs; nothing mutates except through an
  explicit `&mut` in a signature (base.md §2, §8.1).
- **Refusal beats repair:** no lossy replacement, no normalization, no
  silent truncation; every refusal type is defined once as a struct and
  each operation's error type is exactly what it can produce (base.md
  §3.2; spec §5.2).
- **No magic numbers:** any literal carrying meaning gets a named
  constant with its intent (`Source::MAX_LEN` is the pattern) (spec
  §5.2).
- **Lints denied workspace-wide:** `unused` (group), `unused_must_use`,
  `dead_code`, `missing_docs` (base.md §10; spec §5.2, §10.4).
- **Documentation is executable:** every public operation's rustdoc
  names its refusal type and cost, matching the base.md §9 table; doc
  examples run as doctests; nothing is claimed that a test does not
  hold (spec §10.4).
- **Vocabulary:** public names come from the language-tooling
  literature; a departure carries a stated reason at its introduction
  (base.md §1). Code comments cite `docs/design/base.md` and
  `docs/specification.md` sections and nothing else.
- **Commits:** small, one per task-completing step as written; every
  commit leaves the gate green.

## File structure

```
Cargo.toml                          workspace manifest, lints, floor
Cargo.lock                          committed — reproducibility
.gitignore                          /target
.github/workflows/gate.yml          the gate: fmt, clippy, test, doc
crates/themelios-base/
  Cargo.toml                        empty [dependencies]; proptest,
                                    criterion as dev-dependencies
  src/lib.rs                        crate docs, forbid(unsafe_code),
                                    the five module declarations
  src/source.rs                     §3: SourceId, Source, refusals,
                                    Sources, SourceFacet, SourceSet,
                                    check_sources_laws
  src/span.rs                       §4: ByteOffset, Span,
                                    EndBeforeStart, Location
  src/line.rs                       §5: LineIndex, LineCol,
                                    ColumnEncoding, refusals
  src/diagnostic.rs                 §6: DiagnosticId, Severity, Label,
                                    Diagnostic, EmptyMessage,
                                    ToDiagnostic
  src/view.rs                       §7: human, editor + payload types,
                                    canonical_order
  tests/trust.rs                    structural trust checks
  tests/properties.rs               the §10 property laws (proptest),
                                    over the public surface only
  tests/sources_laws.rs             the Sources law checker exercised
                                    against both outcomes
  tests/golden.rs                   golden-corpus harness (std-only)
  tests/golden/*.txt                the nine seed-corpus renderings
  tests/scaling_shape.rs            CI shape assertions (ratio-based)
  benches/scaling.rs                criterion benches (out-of-band
                                    absolute numbers)
```

Public surface is exactly the five modules; the crate root re-exports
nothing (the design's module map is the API's geography; inventing a
flattened namespace would be surface the design does not state).

Module boundaries are the design's; task order is dependency order and
therefore differs — `source` and `line` each land in two tasks.

---

### Task 1: Workspace scaffold and trust checks

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `crates/themelios-base/Cargo.toml`,
  `crates/themelios-base/src/lib.rs`, `crates/themelios-base/tests/trust.rs`,
  `.github/workflows/gate.yml`
- Commit: `Cargo.lock`

**Derives:** base.md §1 (crate facts); spec §10.1–§10.2 (gate, floor),
§12.3 (trust checks).

**Interfaces:**
- Consumes: nothing.
- Produces: the workspace every later task builds inside; the lint and
  trust regime every later task's code must pass.

- [ ] **Step 1: Write the workspace and crate manifests**

`Cargo.toml` (repository root):

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.97"
license = "MIT"

# Denied workspace-wide: unused code is either a defect or a claim the
# crate does not need (docs/specification.md §5.2, §10.1); missing docs
# break the executable-claims posture (docs/specification.md §10.4).
[workspace.lints.rust]
unsafe_code = "deny"
missing_docs = "deny"
unused = { level = "deny", priority = -1 }
unused_must_use = "deny"
dead_code = "deny"
```

`.gitignore`:

```
/target
```

`crates/themelios-base/Cargo.toml`:

```toml
[package]
name = "themelios-base"
version = "0.1.0"
description = "Source-text model, spans, line indexing, and the diagnostics model."
edition.workspace = true
rust-version.workspace = true
license.workspace = true

# Empty by design, forever: zero dependencies in the shipped library's
# closure (docs/design/base.md §1). tests/trust.rs holds this claim.
[dependencies]

# The stage-1 instruments (docs/design/base.md §10), outside the
# shipped closure's claim.
[dev-dependencies]
proptest = "1"
criterion = "0.7"

[lints]
workspace = true
```

`crates/themelios-base/src/lib.rs`:

```rust
//! Source-text model, spans, line indexing, and the diagnostics model:
//! the shared vocabulary of *location* and *report* under every tier.
//!
//! Design of record: `docs/design/base.md`. Every public operation's
//! failure semantics and computational cost are stated on the
//! operation and consolidated in base.md §9. This crate does no I/O,
//! holds no global state, and knows nothing about any language.
#![forbid(unsafe_code)]
```

- [ ] **Step 2: Verify the empty workspace builds**

Run: `cargo build --workspace && cargo test --workspace`
Expected: clean build, `running 0 tests`, exit 0. `Cargo.lock` now
exists; it is committed (the CI pin is reproducibility — spec §10.1).

- [ ] **Step 3: Write the trust checks**

`crates/themelios-base/tests/trust.rs`:

```rust
//! Structural trust checks: the crate facts of docs/design/base.md §1,
//! held mechanically per docs/specification.md §12.3. These read the
//! manifests this repository owns, so plain line scans are exact.

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The lines of this crate's manifest that sit inside `[dependencies]`.
fn shipped_dependency_lines() -> Vec<String> {
    let manifest = fs::read_to_string(manifest_dir().join("Cargo.toml"))
        .expect("crate manifest is readable");
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn shipped_dependency_closure_is_empty() {
    assert_eq!(
        shipped_dependency_lines(),
        Vec::<String>::new(),
        "docs/design/base.md §1: zero dependencies in the shipped closure"
    );
}

#[test]
fn no_build_script() {
    assert!(
        !manifest_dir().join("build.rs").exists(),
        "docs/specification.md §12.3: no build script outside sys crates"
    );
}

#[test]
fn unsafe_code_is_forbidden_at_the_crate_root() {
    let lib = fs::read_to_string(manifest_dir().join("src/lib.rs"))
        .expect("lib.rs is readable");
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "docs/design/base.md §1: forbid, not merely deny, at the root"
    );
}

#[test]
fn rust_version_floor_is_declared() {
    let workspace = fs::read_to_string(
        manifest_dir().join("../../Cargo.toml"),
    )
    .expect("workspace manifest is readable");
    assert!(
        workspace.contains("rust-version = \"1.97\""),
        "docs/specification.md §10.1: the floor, declared"
    );
    let crate_manifest = fs::read_to_string(manifest_dir().join("Cargo.toml"))
        .expect("crate manifest is readable");
    assert!(
        crate_manifest.contains("rust-version.workspace = true"),
        "docs/specification.md §10.1: every manifest carries the floor"
    );
}
```

- [ ] **Step 4: Run the trust checks**

Run: `cargo test -p themelios-base --test trust`
Expected: 4 passed.

- [ ] **Step 5: Write the CI gate**

`.github/workflows/gate.yml` — the gate is exactly spec §10.2's list:
format check, clippy as errors, the test suite (trust checks and
doctests included), documentation build. Linux and macOS (spec §10.1).

```yaml
name: gate
on:
  push:
  pull_request:

jobs:
  gate:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      # The pin matches the floor (docs/specification.md §10.1).
      - run: |
          rustup toolchain install 1.97.1 --profile minimal \
            --component rustfmt,clippy
          rustup default 1.97.1
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo doc --workspace --no-deps
        env:
          RUSTDOCFLAGS: -D warnings
```

- [ ] **Step 6: Run the full gate locally**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: all four green.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore crates .github
git commit -m "Scaffold the workspace: themelios-base, lint regime, trust checks, CI gate"
```

---

### Task 2: The `span` module

**Files:**
- Create: `crates/themelios-base/src/span.rs`,
  `crates/themelios-base/tests/properties.rs`
- Modify: `crates/themelios-base/src/lib.rs` (add `pub mod span;`)

**Derives:** base.md §4 (spans), §8.5 (std-trait posture), §9 (costs),
§10 (span-algebra property laws).

**Interfaces:**
- Consumes: nothing.
- Produces: `ByteOffset` (`ZERO`, `new(u32)`, `get() -> u32`,
  `checked_add`/`checked_sub -> Option<ByteOffset>`); `Span`
  (`new(ByteOffset, ByteOffset) -> Result<Span, EndBeforeStart>`,
  `empty(at)`, `start`, `end`, `len() -> u32`, `is_empty`, `contains`,
  `contains_span`, `intersect -> Option<Span>`, `join -> Span`);
  `EndBeforeStart { start, end }`; `Location { source, span }` — every
  later task locates with these.

Note: `Location` needs `SourceId`, which Task 3 defines. To keep each
task compiling alone, `Location` lands in **Task 3** with `SourceId`;
this task builds everything else in the module.

- [ ] **Step 1: Write the failing tests**

Append `pub mod span;` under the crate docs in `src/lib.rs`. Create
`src/span.rs` containing only the test module (the types come in Step
3, so this fails to compile — that is the failing state):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_refuses_end_before_start() {
        let refusal = Span::new(ByteOffset::new(5), ByteOffset::new(3));
        assert_eq!(
            refusal,
            Err(EndBeforeStart {
                start: ByteOffset::new(5),
                end: ByteOffset::new(3),
            })
        );
    }

    #[test]
    fn empty_span_has_no_extent_and_contains_nothing() {
        let span = Span::empty(ByteOffset::new(7));
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert!(!span.contains(ByteOffset::new(7)));
    }

    #[test]
    fn contains_is_half_open() {
        let span = Span::new(ByteOffset::new(2), ByteOffset::new(5))
            .expect("ordered endpoints");
        assert!(span.contains(ByteOffset::new(2)));
        assert!(span.contains(ByteOffset::new(4)));
        assert!(!span.contains(ByteOffset::new(5)));
    }

    #[test]
    fn join_is_total_including_disjoint_operands() {
        let a = Span::new(ByteOffset::new(0), ByteOffset::new(2)).unwrap();
        let b = Span::new(ByteOffset::new(6), ByteOffset::new(9)).unwrap();
        let joined = a.join(b);
        assert_eq!(joined.start(), ByteOffset::new(0));
        assert_eq!(joined.end(), ByteOffset::new(9));
    }

    #[test]
    fn intersect_is_interval_intersection() {
        let a = Span::new(ByteOffset::new(0), ByteOffset::new(4)).unwrap();
        let b = Span::new(ByteOffset::new(2), ByteOffset::new(6)).unwrap();
        let c = Span::new(ByteOffset::new(4), ByteOffset::new(6)).unwrap();
        let d = Span::new(ByteOffset::new(5), ByteOffset::new(6)).unwrap();
        assert_eq!(a.intersect(b), Span::new(ByteOffset::new(2), ByteOffset::new(4)).ok());
        // Touching spans intersect in the empty span at the touch point:
        // interval semantics, which is what keeps intersect consistent
        // with contains_span for empty operands (base.md §10).
        assert_eq!(a.intersect(c), Some(Span::empty(ByteOffset::new(4))));
        assert_eq!(a.intersect(d), None);
    }

    #[test]
    fn ordering_is_document_order_with_shorter_first_ties() {
        let early = Span::new(ByteOffset::new(1), ByteOffset::new(9)).unwrap();
        let late = Span::new(ByteOffset::new(2), ByteOffset::new(3)).unwrap();
        let late_longer = Span::new(ByteOffset::new(2), ByteOffset::new(4)).unwrap();
        assert!(early < late);
        assert!(late < late_longer);
    }

    #[test]
    fn checked_arithmetic_refuses_overflow() {
        assert_eq!(ByteOffset::new(u32::MAX).checked_add(1), None);
        assert_eq!(ByteOffset::ZERO.checked_sub(1), None);
        assert_eq!(
            ByteOffset::new(3).checked_add(4),
            Some(ByteOffset::new(7))
        );
    }

    #[test]
    fn end_before_start_displays_the_fixable_question() {
        let refusal = EndBeforeStart {
            start: ByteOffset::new(5),
            end: ByteOffset::new(3),
        };
        assert_eq!(
            refusal.to_string(),
            "span end 3 is before its start 5"
        );
        let _: &dyn std::error::Error = &refusal;
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `cannot find type Span`, `ByteOffset`, …

- [ ] **Step 3: Implement the module**

Prepend to `src/span.rs`, above the test module:

```rust
//! Byte positions and half-open regions in one source's text
//! (docs/design/base.md §4). A `Span` is text-independent arithmetic
//! data; boundary discipline lives where span meets text — see
//! `Source::slice` and the line index.

use std::fmt;

/// A position in a source's UTF-8 text, in bytes. The unit is in the
/// type's name so it is never in a comment (base.md §4.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteOffset(u32);

impl ByteOffset {
    /// The zero offset: the start of any text.
    pub const ZERO: ByteOffset = ByteOffset(0);

    /// Wraps a raw byte count. Total; O(1).
    pub const fn new(raw: u32) -> ByteOffset {
        ByteOffset(raw)
    }

    /// The raw byte count. Total; O(1).
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Checked arithmetic only: overflow answers `None`, never wraps
    /// (base.md §4.1). O(1).
    pub const fn checked_add(self, bytes: u32) -> Option<ByteOffset> {
        match self.0.checked_add(bytes) {
            Some(raw) => Some(ByteOffset(raw)),
            None => None,
        }
    }

    /// Checked arithmetic only: underflow answers `None`, never wraps
    /// (base.md §4.1). O(1).
    pub const fn checked_sub(self, bytes: u32) -> Option<ByteOffset> {
        match self.0.checked_sub(bytes) {
            Some(raw) => Some(ByteOffset(raw)),
            None => None,
        }
    }
}

/// A half-open byte region `[start, end)` in one source's text
/// (base.md §4.2). The one guarded invariant is `start <= end`;
/// derived ordering is (start, end) — document order with
/// shorter-first ties.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Span {
    start: ByteOffset,
    end: ByteOffset,
}

/// The one refusal `Span` construction can issue, carried as the
/// condition itself (base.md §4.2, §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EndBeforeStart {
    /// The offered start.
    pub start: ByteOffset,
    /// The offered end, strictly before the start.
    pub end: ByteOffset,
}

impl fmt::Display for EndBeforeStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "span end {} is before its start {}",
            self.end.get(),
            self.start.get()
        )
    }
}

impl std::error::Error for EndBeforeStart {}

impl Span {
    /// Refuses `EndBeforeStart`; O(1).
    pub fn new(
        start: ByteOffset,
        end: ByteOffset,
    ) -> Result<Span, EndBeforeStart> {
        if end < start {
            Err(EndBeforeStart { start, end })
        } else {
            Ok(Span { start, end })
        }
    }

    /// The empty span at one position. Total; O(1).
    pub const fn empty(at: ByteOffset) -> Span {
        Span { start: at, end: at }
    }

    /// The start offset. Total; O(1).
    pub fn start(self) -> ByteOffset {
        self.start
    }

    /// The one-past-end offset. Total; O(1).
    pub fn end(self) -> ByteOffset {
        self.end
    }

    /// Length in bytes. Total; O(1).
    pub fn len(self) -> u32 {
        // Cannot underflow: start <= end is guarded at construction.
        self.end.get() - self.start.get()
    }

    /// Whether the region is empty. Total; O(1).
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` lies inside the half-open region. Total; O(1).
    pub fn contains(self, offset: ByteOffset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Whether `other` lies entirely within `self`. Total; O(1).
    pub fn contains_span(self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Interval intersection: `Some` exactly when the intervals meet,
    /// including an empty span at a touch point — which keeps this
    /// consistent with `contains_span` on empty operands. Total; O(1).
    pub fn intersect(self, other: Span) -> Option<Span> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if start <= end {
            Some(Span { start, end })
        } else {
            None
        }
    }

    /// The covering span — total, including disjoint operands
    /// (base.md §4.2). O(1).
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `span::tests` pass.

- [ ] **Step 5: Write the span-algebra property laws**

Create `crates/themelios-base/tests/properties.rs` — the §10 property
laws live here, over the public surface only; later tasks append their
laws to this file:

```rust
//! The stage-1 property laws (docs/design/base.md §10), held by
//! proptest over the public surface only.

use proptest::prelude::*;
use themelios_base::span::{ByteOffset, Span};

fn spans() -> impl Strategy<Value = Span> {
    (any::<u32>(), any::<u32>()).prop_map(|(a, b)| {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Span::new(ByteOffset::new(start), ByteOffset::new(end))
            .expect("endpoints were ordered")
    })
}

proptest! {
    #[test]
    fn join_is_idempotent(a in spans()) {
        prop_assert_eq!(a.join(a), a);
    }

    #[test]
    fn join_is_commutative(a in spans(), b in spans()) {
        prop_assert_eq!(a.join(b), b.join(a));
    }

    #[test]
    fn join_is_associative(a in spans(), b in spans(), c in spans()) {
        prop_assert_eq!(a.join(b).join(c), a.join(b.join(c)));
    }

    #[test]
    fn intersect_is_consistent_with_contains_span(
        a in spans(),
        b in spans(),
    ) {
        // Containment means intersection is the contained span; any
        // intersection lies within both operands.
        if a.contains_span(b) {
            prop_assert_eq!(a.intersect(b), Some(b));
        }
        if let Some(both) = a.intersect(b) {
            prop_assert!(a.contains_span(both));
            prop_assert!(b.contains_span(both));
        }
        prop_assert_eq!(a.intersect(b), b.intersect(a));
    }
}
```

- [ ] **Step 6: Run the property laws and the gate**

Run: `cargo test -p themelios-base --test properties`
Expected: 4 passed.
Then: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "span: ByteOffset, Span, EndBeforeStart, and the span-algebra laws"
```

---

### Task 3: The `source` module, admission half — and `Location`

**Files:**
- Create: `crates/themelios-base/src/source.rs`
- Modify: `crates/themelios-base/src/lib.rs` (add `pub mod source;`),
  `crates/themelios-base/src/span.rs` (add `Location`),
  `crates/themelios-base/tests/properties.rs` (append)

**Derives:** base.md §3.1–§3.3 (identity, the Source value, the
embedded-source obligations — discharged by the model's properties, no
extra API), §4.3 (`Location`), §8.5, §9, §10 (the `from_bytes` law).

**Interfaces:**
- Consumes: `ByteOffset`, `Span` from Task 2.
- Produces: `SourceId` (`new(u32)`, `get()`); `Source` (`MAX_LEN`,
  `new(SourceId, String) -> Result<Source, TooLarge>`,
  `from_bytes(SourceId, Vec<u8>) -> Result<Source, FromBytesRefusal>`,
  `id()`, `text() -> &str`, `span() -> Span`, `end() -> ByteOffset`,
  `slice(Span) -> Result<&str, SliceRefusal>`); refusals `TooLarge
  { len }`, `InvalidUtf8 { valid_up_to }`, `FromBytesRefusal`,
  `SliceRefusal`; `span::Location { source, span }`. Tasks 4–11 build
  on all of these.

- [ ] **Step 1: Write the failing tests**

Add `pub mod source;` to `src/lib.rs`. Create `src/source.rs` with only
the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{ByteOffset, Location, Span};

    fn id() -> SourceId {
        SourceId::new(7)
    }

    #[test]
    fn admission_keeps_text_and_identity() {
        let source = Source::new(id(), "p(a).\n".to_owned())
            .expect("small text admits");
        assert_eq!(source.id(), id());
        assert_eq!(source.text(), "p(a).\n");
        assert_eq!(source.end(), ByteOffset::new(6));
        assert_eq!(source.span().start(), ByteOffset::ZERO);
        assert_eq!(source.span().end(), ByteOffset::new(6));
    }

    #[test]
    fn the_admission_ceiling_is_the_offset_width() {
        assert_eq!(Source::MAX_LEN, u32::MAX as usize);
    }

    #[test]
    fn from_bytes_admits_valid_utf8_and_refuses_invalid() {
        let ok = Source::from_bytes(id(), b"q(b).".to_vec());
        assert_eq!(ok.expect("valid UTF-8 admits").text(), "q(b).");

        let refused = Source::from_bytes(id(), vec![0x70, 0xFF, 0x70]);
        assert_eq!(
            refused,
            Err(FromBytesRefusal::InvalidUtf8(InvalidUtf8 {
                valid_up_to: 1
            }))
        );
    }

    #[test]
    fn no_repair_at_the_door() {
        // BOM, CRLF, and a lone CR pass through byte-for-byte
        // (base.md §3.2: author bytes are data).
        let text = "\u{FEFF}a\r\nb\rc";
        let source = Source::new(id(), text.to_owned()).unwrap();
        assert_eq!(source.text(), text);
    }

    #[test]
    fn slice_returns_the_spanned_text() {
        let source = Source::new(id(), "héllo".to_owned()).unwrap();
        let span = Span::new(ByteOffset::new(1), ByteOffset::new(3))
            .unwrap();
        assert_eq!(source.slice(span), Ok("é"));
    }

    #[test]
    fn slice_refuses_out_of_bounds_with_both_facts() {
        let source = Source::new(id(), "abc".to_owned()).unwrap();
        let span = Span::new(ByteOffset::new(1), ByteOffset::new(9))
            .unwrap();
        assert_eq!(
            source.slice(span),
            Err(SliceRefusal::OutOfBounds {
                end: ByteOffset::new(9),
                max: ByteOffset::new(3),
            })
        );
    }

    #[test]
    fn slice_refuses_a_mid_character_boundary() {
        let source = Source::new(id(), "héllo".to_owned()).unwrap();
        // Byte 2 is inside the two-byte 'é'.
        let span = Span::new(ByteOffset::new(2), ByteOffset::new(3))
            .unwrap();
        assert_eq!(
            source.slice(span),
            Err(SliceRefusal::NotCharBoundary {
                offset: ByteOffset::new(2)
            })
        );
    }

    #[test]
    fn location_orders_by_source_then_span() {
        let a = Location {
            source: SourceId::new(1),
            span: Span::new(ByteOffset::new(9), ByteOffset::new(10))
                .unwrap(),
        };
        let b = Location {
            source: SourceId::new(2),
            span: Span::new(ByteOffset::new(0), ByteOffset::new(1))
                .unwrap(),
        };
        assert!(a < b);
    }

    #[test]
    fn refusals_display_the_fixable_question() {
        assert_eq!(
            TooLarge { len: 5_000_000_000 }.to_string(),
            "text is 5000000000 bytes; the admission ceiling \
             Source::MAX_LEN is 4294967295 bytes"
        );
        assert_eq!(
            InvalidUtf8 { valid_up_to: 12 }.to_string(),
            "bytes are not valid UTF-8 past byte 12"
        );
        let _: &dyn std::error::Error = &TooLarge { len: 0 };
        let _: &dyn std::error::Error =
            &SliceRefusal::NotCharBoundary { offset: ByteOffset::ZERO };
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `cannot find` the source types.

- [ ] **Step 3: Implement the module**

Prepend to `src/source.rs`:

```rust
//! The source-text model (docs/design/base.md §3): text with an
//! identity, from anywhere — this crate does no I/O and never sees a
//! path. Admission is the one well-formedness authority for text;
//! everything downstream rides on a `Source` and inherits its
//! guarantees. The module's catalog half — `Sources`, its laws,
//! `SourceSet` — lands beside the line index it resolves.

use std::fmt;

use crate::span::{ByteOffset, Span};

/// An opaque identity for one source text. The embedding host mints it,
/// because the host already has it (base.md §3.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId(u32);

impl SourceId {
    /// Wraps a host-minted identity. Total; O(1).
    pub const fn new(raw: u32) -> SourceId {
        SourceId(raw)
    }

    /// The raw identity. Total; O(1).
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Text longer than `Source::MAX_LEN` bytes (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TooLarge {
    /// The offered length, in bytes.
    pub len: usize,
}

/// Bytes that are not valid UTF-8; `valid_up_to` mirrors the standard
/// library's error detail (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidUtf8 {
    /// How many leading bytes were valid.
    pub valid_up_to: usize,
}

/// What `Source::from_bytes` can refuse (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FromBytesRefusal {
    /// The byte count exceeds the admission ceiling.
    TooLarge(TooLarge),
    /// The bytes are not valid UTF-8.
    InvalidUtf8(InvalidUtf8),
}

/// What `Source::slice` can refuse (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliceRefusal {
    /// The span ends past the text.
    OutOfBounds {
        /// The span's one-past-end offset.
        end: ByteOffset,
        /// The text's one-past-end offset.
        max: ByteOffset,
    },
    /// A span endpoint falls inside a multi-byte character.
    NotCharBoundary {
        /// The offending endpoint.
        offset: ByteOffset,
    },
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "text is {} bytes; the admission ceiling Source::MAX_LEN \
             is {} bytes",
            self.len,
            Source::MAX_LEN
        )
    }
}

impl std::error::Error for TooLarge {}

impl fmt::Display for InvalidUtf8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bytes are not valid UTF-8 past byte {}", self.valid_up_to)
    }
}

impl std::error::Error for InvalidUtf8 {}

impl fmt::Display for FromBytesRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FromBytesRefusal::TooLarge(refusal) => refusal.fmt(f),
            FromBytesRefusal::InvalidUtf8(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for FromBytesRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FromBytesRefusal::TooLarge(refusal) => Some(refusal),
            FromBytesRefusal::InvalidUtf8(refusal) => Some(refusal),
        }
    }
}

impl fmt::Display for SliceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SliceRefusal::OutOfBounds { end, max } => write!(
                f,
                "span ends at byte {}, past the text's end at byte {}",
                end.get(),
                max.get()
            ),
            SliceRefusal::NotCharBoundary { offset } => write!(
                f,
                "byte {} is not a character boundary",
                offset.get()
            ),
        }
    }
}

impl std::error::Error for SliceRefusal {}

/// One owned source text and its identity. UTF-8 by construction
/// (base.md §3.2): arbitrary bytes meet a typed refusal at admission,
/// and everything past the door is valid UTF-8.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Source {
    id: SourceId,
    text: String,
}

impl Source {
    /// The admission ceiling: offsets are `u32`, so text is at most
    /// `u32::MAX` bytes. The name exists so the limit is never a bare
    /// numeral at a call site (base.md §3.2).
    pub const MAX_LEN: usize = u32::MAX as usize;

    /// Admits owned text. Refuses `TooLarge`; O(1) beyond the length
    /// check. No repair at the door: no BOM stripping, no line-ending
    /// normalization — author bytes are data (base.md §3.2).
    pub fn new(id: SourceId, text: String) -> Result<Source, TooLarge> {
        if text.len() > Source::MAX_LEN {
            Err(TooLarge { len: text.len() })
        } else {
            Ok(Source { id, text })
        }
    }

    /// Admits raw bytes. Refuses `FromBytesRefusal` — the length check
    /// first, then UTF-8 validation; O(n) validation (base.md §3.2).
    pub fn from_bytes(
        id: SourceId,
        bytes: Vec<u8>,
    ) -> Result<Source, FromBytesRefusal> {
        if bytes.len() > Source::MAX_LEN {
            return Err(FromBytesRefusal::TooLarge(TooLarge {
                len: bytes.len(),
            }));
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Source { id, text }),
            Err(error) => {
                Err(FromBytesRefusal::InvalidUtf8(InvalidUtf8 {
                    valid_up_to: error.utf8_error().valid_up_to(),
                }))
            }
        }
    }

    /// The identity the host minted. Total; O(1).
    pub fn id(&self) -> SourceId {
        self.id
    }

    /// The admitted text. Total; O(1).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The covering span: `ByteOffset::ZERO` to the one-past-end
    /// offset. Total; O(1).
    pub fn span(&self) -> Span {
        // join is total, so the covering region needs no fallible
        // construction (base.md §4.2).
        Span::empty(ByteOffset::ZERO).join(Span::empty(self.end()))
    }

    /// The one-past-end offset. Total; O(1).
    pub fn end(&self) -> ByteOffset {
        // The cast cannot truncate: admission guards
        // len <= MAX_LEN == u32::MAX.
        ByteOffset::new(self.text.len() as u32)
    }

    /// The spanned text. Refuses out-of-bounds and non-boundary
    /// endpoints (`SliceRefusal`); O(1) — bounds and boundary checks
    /// against the owned text (base.md §3.2).
    pub fn slice(&self, span: Span) -> Result<&str, SliceRefusal> {
        let max = self.end();
        if span.end() > max {
            return Err(SliceRefusal::OutOfBounds { end: span.end(), max });
        }
        let start = span.start().get() as usize;
        let end = span.end().get() as usize;
        if !self.text.is_char_boundary(start) {
            return Err(SliceRefusal::NotCharBoundary {
                offset: span.start(),
            });
        }
        if !self.text.is_char_boundary(end) {
            return Err(SliceRefusal::NotCharBoundary {
                offset: span.end(),
            });
        }
        Ok(&self.text[start..end])
    }
}
```

And append `Location` to `src/span.rs`, after `Span`'s impl:

```rust
/// A span in a named source — the cross-source form (base.md §4.3).
/// Fields are public: any (source, span) pair is a valid value;
/// validity against a particular text is checked where text is in
/// scope. Derived ordering is (source, then span): batch order groups
/// by source.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Location {
    /// The source the span points into.
    pub source: crate::source::SourceId,
    /// The region within that source's text.
    pub span: Span,
}
```

**One untestable arm, named now:** `TooLarge`'s positive branch needs a
text over four gibibytes, which no CI allocation will do. It is held by
inspection (the check is one comparison against the named ceiling) and
is pre-declared an accepted mutation survivor, recorded at Task 13.

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `source::tests` pass; span tests still green.

- [ ] **Step 5: Append the admission property law**

Append to `tests/properties.rs`:

```rust
mod source_admission {
    use proptest::prelude::*;
    use themelios_base::source::{FromBytesRefusal, Source, SourceId};

    proptest! {
        /// base.md §10: `from_bytes` on arbitrary bytes never panics
        /// and refuses exactly when the standard library's validator
        /// does. (`TooLarge` is unreachable at generated sizes.)
        #[test]
        fn from_bytes_agrees_with_the_std_validator(
            bytes in proptest::collection::vec(any::<u8>(), 0..2048),
        ) {
            let admitted =
                Source::from_bytes(SourceId::new(0), bytes.clone());
            match std::str::from_utf8(&bytes) {
                Ok(_) => prop_assert!(admitted.is_ok()),
                Err(error) => match admitted {
                    Err(FromBytesRefusal::InvalidUtf8(refusal)) => {
                        prop_assert_eq!(
                            refusal.valid_up_to,
                            error.valid_up_to()
                        );
                    }
                    other => prop_assert!(
                        false,
                        "expected InvalidUtf8, got {:?}",
                        other
                    ),
                },
            }
        }
    }
}
```

- [ ] **Step 6: Run the property laws and the gate**

Run: `cargo test -p themelios-base --test properties`
Expected: 5 passed.
Then the full gate command from Task 1 Step 6. Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "source: identity, admission, slicing, and Location; the std-validator law"
```

---

### Task 4: The `line` module, structure half

**Files:**
- Create: `crates/themelios-base/src/line.rs`
- Modify: `crates/themelios-base/src/lib.rs` (add `pub mod line;`)

**Derives:** base.md §5 (representation, newline policy, in-bounds
definition, zero-based coordinates, costs), §8.5, §9.

**Interfaces:**
- Consumes: `Source`, `ByteOffset`, `Span` from Tasks 2–3.
- Produces: `LineIndex` (`of(&Source) -> LineIndex` — total, there is
  no `&str` door; `line_count() -> u32`;
  `line_span(u32) -> Result<Span, LineOutOfBounds>`); `LineCol { line,
  col }`; `ColumnEncoding { Utf8Bytes, CodePoints, Utf16Units }`; all
  five refusal condition structs and both refusal enums (`position`
  and `offset` themselves land in Task 5). Tasks 5–6 and 10 build on
  these.

- [ ] **Step 1: Write the failing tests**

Add `pub mod line;` to `src/lib.rs`. Create `src/line.rs` with only the
test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Source, SourceId};
    use crate::span::ByteOffset;

    fn index_of(text: &str) -> LineIndex {
        let source = Source::new(SourceId::new(0), text.to_owned())
            .expect("test text admits");
        LineIndex::of(&source)
    }

    #[test]
    fn empty_text_has_one_empty_line() {
        let index = index_of("");
        assert_eq!(index.line_count(), 1);
        let span = index.line_span(0).expect("line 0 exists");
        assert_eq!(span.start(), ByteOffset::ZERO);
        assert_eq!(span.end(), ByteOffset::ZERO);
    }

    #[test]
    fn lines_break_at_newline_and_content_excludes_the_terminator() {
        let index = index_of("ab\ncd");
        assert_eq!(index.line_count(), 2);
        let first = index.line_span(0).unwrap();
        assert_eq!((first.start().get(), first.end().get()), (0, 2));
        let second = index.line_span(1).unwrap();
        assert_eq!((second.start().get(), second.end().get()), (3, 5));
    }

    #[test]
    fn a_trailing_newline_yields_a_final_empty_line() {
        let index = index_of("ab\n");
        assert_eq!(index.line_count(), 2);
        let last = index.line_span(1).unwrap();
        assert_eq!((last.start().get(), last.end().get()), (3, 3));
    }

    #[test]
    fn carriage_return_stays_in_its_line_content() {
        // CRLF: the '\r' is content, only '\n' terminates
        // (base.md §5, the rust-analyzer convention).
        let index = index_of("ab\r\ncd");
        let first = index.line_span(0).unwrap();
        assert_eq!((first.start().get(), first.end().get()), (0, 3));
    }

    #[test]
    fn a_lone_carriage_return_is_not_a_line_break() {
        let index = index_of("ab\rcd");
        assert_eq!(index.line_count(), 1);
    }

    #[test]
    fn line_span_refuses_out_of_bounds_with_the_count() {
        let index = index_of("ab");
        assert_eq!(
            index.line_span(3),
            Err(LineOutOfBounds { line: 3, line_count: 1 })
        );
    }

    #[test]
    fn refusals_display_the_fixable_question() {
        assert_eq!(
            LineOutOfBounds { line: 3, line_count: 1 }.to_string(),
            "line 3 is out of bounds; the text has 1 line(s)"
        );
        assert_eq!(
            ColumnOutOfBounds { line: 2, col: 9, max: 4 }.to_string(),
            "column 9 is past line 2's extent 4"
        );
        assert_eq!(
            ColumnNotBoundary { line: 2, col: 3 }.to_string(),
            "column 3 on line 2 is not a character boundary"
        );
        assert_eq!(
            OffsetOutOfBounds {
                offset: ByteOffset::new(9),
                max: ByteOffset::new(4),
            }
            .to_string(),
            "byte 9 is past the text's end at byte 4"
        );
        assert_eq!(
            NotCharBoundary { offset: ByteOffset::new(2) }.to_string(),
            "byte 2 is not a character boundary"
        );
        let _: &dyn std::error::Error =
            &LineOutOfBounds { line: 0, line_count: 1 };
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — the line types are not defined.

- [ ] **Step 3: Implement the structure half**

Prepend to `src/line.rs`:

```rust
//! Line and column structure for one source's text (docs/design/
//! base.md §5): an explicit, pure derivation you construct and hold.
//! Zero-based throughout; one-based coordinates exist only inside the
//! human rendering, as presentation. Lines break at `\n` alone; a
//! `\r` stays in its line's content; nothing is normalized, ever.

use std::fmt;

use crate::source::Source;
use crate::span::{ByteOffset, Span};

/// A zero-based line/column coordinate. What `col` counts is named by
/// the encoding the query stated; the coordinate is a transient query
/// result and deliberately does not carry it (base.md §5). Fields are
/// public: any pair is a valid coordinate value; validity against a
/// text is checked at use.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column, in the units the query's encoding named.
    pub col: u32,
}

/// What a column counts. UTF-16 units exist because the editor
/// protocol's default position encoding demands them (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColumnEncoding {
    /// Bytes of UTF-8.
    Utf8Bytes,
    /// Unicode code points.
    CodePoints,
    /// UTF-16 code units.
    Utf16Units,
}

/// An offset strictly past the end-of-text position (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OffsetOutOfBounds {
    /// The offending offset.
    pub offset: ByteOffset,
    /// The end-of-text position — the largest in-bounds offset.
    pub max: ByteOffset,
}

/// An offset inside a multi-byte character (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NotCharBoundary {
    /// The offending offset.
    pub offset: ByteOffset,
}

/// A line at or past the line count (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineOutOfBounds {
    /// The offending line.
    pub line: u32,
    /// How many lines the text has.
    pub line_count: u32,
}

/// A column past its line's extent; `max` is the extent in the
/// encoding the query stated (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnOutOfBounds {
    /// The line queried.
    pub line: u32,
    /// The offending column.
    pub col: u32,
    /// The line's extent in the stated encoding.
    pub max: u32,
}

/// A column inside a multi-unit character in the stated encoding
/// (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnNotBoundary {
    /// The line queried.
    pub line: u32,
    /// The offending column.
    pub col: u32,
}

/// What `LineIndex::position` can refuse (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PositionRefusal {
    /// The offset is past the end of the text.
    OutOfBounds(OffsetOutOfBounds),
    /// The offset is inside a multi-byte character.
    NotCharBoundary(NotCharBoundary),
}

/// What `LineIndex::offset` can refuse (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OffsetRefusal {
    /// The line is past the line count.
    LineOutOfBounds(LineOutOfBounds),
    /// The column is past the line's extent.
    ColumnOutOfBounds(ColumnOutOfBounds),
    /// The column is inside a multi-unit character.
    ColumnNotBoundary(ColumnNotBoundary),
}

impl fmt::Display for OffsetOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "byte {} is past the text's end at byte {}",
            self.offset.get(),
            self.max.get()
        )
    }
}

impl std::error::Error for OffsetOutOfBounds {}

impl fmt::Display for NotCharBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "byte {} is not a character boundary", self.offset.get())
    }
}

impl std::error::Error for NotCharBoundary {}

impl fmt::Display for LineOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {} is out of bounds; the text has {} line(s)",
            self.line, self.line_count
        )
    }
}

impl std::error::Error for LineOutOfBounds {}

impl fmt::Display for ColumnOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "column {} is past line {}'s extent {}",
            self.col, self.line, self.max
        )
    }
}

impl std::error::Error for ColumnOutOfBounds {}

impl fmt::Display for ColumnNotBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "column {} on line {} is not a character boundary",
            self.col, self.line
        )
    }
}

impl std::error::Error for ColumnNotBoundary {}

impl fmt::Display for PositionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionRefusal::OutOfBounds(refusal) => refusal.fmt(f),
            PositionRefusal::NotCharBoundary(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for PositionRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PositionRefusal::OutOfBounds(refusal) => Some(refusal),
            PositionRefusal::NotCharBoundary(refusal) => Some(refusal),
        }
    }
}

impl fmt::Display for OffsetRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OffsetRefusal::LineOutOfBounds(refusal) => refusal.fmt(f),
            OffsetRefusal::ColumnOutOfBounds(refusal) => refusal.fmt(f),
            OffsetRefusal::ColumnNotBoundary(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for OffsetRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OffsetRefusal::LineOutOfBounds(refusal) => Some(refusal),
            OffsetRefusal::ColumnOutOfBounds(refusal) => Some(refusal),
            OffsetRefusal::ColumnNotBoundary(refusal) => Some(refusal),
        }
    }
}

/// One non-ASCII character: where it starts and what it costs in each
/// encoding. With the running surpluses beside it, this is enough to
/// answer all three encodings — and to *refuse* a mid-character offset
/// rather than misplace a caret — without retaining the text
/// (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct WideChar {
    /// Byte offset of the character's first byte.
    offset: u32,
    /// 2..=4 bytes of UTF-8.
    byte_len: u8,
    /// 1 or 2 UTF-16 units.
    utf16_len: u8,
}

/// Line and column structure for one source's text: an explicit, pure
/// derivation you construct and hold. Does not retain the text
/// (base.md §5). Memory is O(lines + non-ASCII characters).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineIndex {
    /// Byte offset where each line starts; entry 0 is always 0, and a
    /// new entry follows every `\n`.
    line_starts: Vec<u32>,
    /// Every non-ASCII character, in offset order.
    wide: Vec<WideChar>,
    /// Running total over `wide[..=i]` of `byte_len - 1`: bytes minus
    /// code points contributed by wide characters. Prefix sums keep
    /// column queries logarithmic (base.md §5's stated cost).
    cp_surplus: Vec<u32>,
    /// Running total over `wide[..=i]` of `byte_len - utf16_len`:
    /// bytes minus UTF-16 units, likewise.
    utf16_surplus: Vec<u32>,
    /// The end-of-text position, in bytes.
    len: u32,
}

impl LineIndex {
    /// Indexes one admitted source. Total: every admitted `Source`
    /// indexes — riding on `Source` admission keeps one authority for
    /// text; there is no `&str` door (base.md §5). O(n) in the text.
    pub fn of(source: &Source) -> LineIndex {
        let text = source.text();
        let mut line_starts = vec![0u32];
        let mut wide = Vec::new();
        let mut cp_surplus = Vec::new();
        let mut utf16_surplus = Vec::new();
        let mut cp_running = 0u32;
        let mut utf16_running = 0u32;
        for (offset, character) in text.char_indices() {
            // The cast cannot truncate: admission guards the length.
            let offset = offset as u32;
            if character == '\n' {
                line_starts.push(offset + 1);
            }
            if !character.is_ascii() {
                let byte_len = character.len_utf8() as u8;
                let utf16_len = character.len_utf16() as u8;
                wide.push(WideChar { offset, byte_len, utf16_len });
                cp_running += u32::from(byte_len) - 1;
                utf16_running +=
                    u32::from(byte_len) - u32::from(utf16_len);
                cp_surplus.push(cp_running);
                utf16_surplus.push(utf16_running);
            }
        }
        LineIndex {
            line_starts,
            wide,
            cp_surplus,
            utf16_surplus,
            len: source.end().get(),
        }
    }

    /// How many lines the text has; empty text has one. Total; O(1) —
    /// a stored count (base.md §5, §9).
    pub fn line_count(&self) -> u32 {
        // The cast cannot truncate: there are at most len + 1 lines
        // and len <= u32::MAX.
        self.line_starts.len() as u32
    }

    /// The span of one line's content, excluding its terminator —
    /// renderers want the content; the terminator is derivable.
    /// Refuses `LineOutOfBounds`; O(1) — a bounds check and an array
    /// lookup; the terminator exclusion is deterministic from adjacent
    /// line starts, because lines break only at `\n` (base.md §5).
    pub fn line_span(&self, line: u32) -> Result<Span, LineOutOfBounds> {
        let line_count = self.line_count();
        if line >= line_count {
            return Err(LineOutOfBounds { line, line_count });
        }
        let start = self.line_starts[line as usize];
        let end = match self.line_starts.get(line as usize + 1) {
            // The byte before the next line's start is this line's
            // terminating '\n'.
            Some(next_start) => next_start - 1,
            None => self.len,
        };
        // Total construction of an ordered region: join of two empty
        // spans, as in Source::span.
        Ok(Span::empty(ByteOffset::new(start))
            .join(Span::empty(ByteOffset::new(end))))
    }
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `line::tests` pass. (`clippy` will flag nothing unused:
the surplus tables and `wide` are read in Task 5's queries; if the
lint fires here, mark nothing — proceed to Task 5 in the same
working tree only if the two tasks are committed together. To keep
commits green, `#[allow(dead_code)]` is **not** used; instead this
task's Step 3 code compiles clean because `LineIndex` derives compare
all fields.)

- [ ] **Step 5: Run the gate**

Run: the full gate command from Task 1 Step 6.
Expected: green. If `dead_code` fires on `wide`/`cp_surplus`/
`utf16_surplus` (they are stored, compared by `PartialEq`, and not yet
queried), fold Task 5 into this commit rather than allow the lint —
denied lints are never waived (spec §5.2).

- [ ] **Step 6: Commit**

```bash
git add crates/themelios-base/src
git commit -m "line: the index representation, line_count, line_span, and the refusal conditions"
```

---

### Task 5: The `line` module, conversion half

**Files:**
- Modify: `crates/themelios-base/src/line.rs`,
  `crates/themelios-base/tests/properties.rs` (append)

**Derives:** base.md §5 (`position`, `offset`, the one-coordinate-type
contract, in-bounds definition), §9 (costs), §10 (round-trip law,
character-walk oracle).

**Interfaces:**
- Consumes: everything Task 4 produced.
- Produces: `LineIndex::position(ByteOffset, ColumnEncoding) ->
  Result<LineCol, PositionRefusal>` and `LineIndex::offset(LineCol,
  ColumnEncoding) -> Result<ByteOffset, OffsetRefusal>` — Task 10's
  editor view is their consumer.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `src/line.rs`:

```rust
    // Layout of "aé🦀\nb": a=0, é=1..3, 🦀=3..7, \n=7, b=8; len 9.
    const MIXED: &str = "aé🦀\nb";

    #[test]
    fn position_answers_in_all_three_encodings() {
        let index = index_of(MIXED);
        let at = |raw: u32, encoding| {
            index.position(ByteOffset::new(raw), encoding)
        };
        use ColumnEncoding::*;
        assert_eq!(at(3, Utf8Bytes), Ok(LineCol { line: 0, col: 3 }));
        assert_eq!(at(3, CodePoints), Ok(LineCol { line: 0, col: 2 }));
        assert_eq!(at(3, Utf16Units), Ok(LineCol { line: 0, col: 2 }));
        // At the terminator: the line's full content extent.
        assert_eq!(at(7, Utf8Bytes), Ok(LineCol { line: 0, col: 7 }));
        assert_eq!(at(7, CodePoints), Ok(LineCol { line: 0, col: 3 }));
        assert_eq!(at(7, Utf16Units), Ok(LineCol { line: 0, col: 4 }));
        // Past the terminator: the next line.
        assert_eq!(at(8, Utf8Bytes), Ok(LineCol { line: 1, col: 0 }));
        // End-of-text is in bounds (base.md §5: offsets 0..=len).
        assert_eq!(at(9, Utf8Bytes), Ok(LineCol { line: 1, col: 1 }));
    }

    #[test]
    fn position_refuses_rather_than_misplaces() {
        let index = index_of(MIXED);
        assert_eq!(
            index.position(ByteOffset::new(2), ColumnEncoding::CodePoints),
            Err(PositionRefusal::NotCharBoundary(NotCharBoundary {
                offset: ByteOffset::new(2),
            }))
        );
        assert_eq!(
            index.position(ByteOffset::new(10), ColumnEncoding::Utf8Bytes),
            Err(PositionRefusal::OutOfBounds(OffsetOutOfBounds {
                offset: ByteOffset::new(10),
                max: ByteOffset::new(9),
            }))
        );
    }

    #[test]
    fn offset_answers_in_all_three_encodings() {
        let index = index_of(MIXED);
        use ColumnEncoding::*;
        let at = |line: u32, col: u32, encoding| {
            index.offset(LineCol { line, col }, encoding)
        };
        assert_eq!(at(0, 3, Utf8Bytes), Ok(ByteOffset::new(3)));
        assert_eq!(at(0, 2, CodePoints), Ok(ByteOffset::new(3)));
        assert_eq!(at(0, 2, Utf16Units), Ok(ByteOffset::new(3)));
        assert_eq!(at(0, 4, Utf16Units), Ok(ByteOffset::new(7)));
        assert_eq!(at(1, 1, Utf8Bytes), Ok(ByteOffset::new(9)));
    }

    #[test]
    fn offset_refuses_each_of_its_three_conditions() {
        let index = index_of(MIXED);
        use ColumnEncoding::*;
        assert_eq!(
            index.offset(LineCol { line: 5, col: 0 }, Utf8Bytes),
            Err(OffsetRefusal::LineOutOfBounds(LineOutOfBounds {
                line: 5,
                line_count: 2,
            }))
        );
        // max is the line's extent in the encoding the query stated.
        assert_eq!(
            index.offset(LineCol { line: 0, col: 9 }, Utf8Bytes),
            Err(OffsetRefusal::ColumnOutOfBounds(ColumnOutOfBounds {
                line: 0,
                col: 9,
                max: 7,
            }))
        );
        assert_eq!(
            index.offset(LineCol { line: 0, col: 4 }, CodePoints),
            Err(OffsetRefusal::ColumnOutOfBounds(ColumnOutOfBounds {
                line: 0,
                col: 4,
                max: 3,
            }))
        );
        // Inside é's bytes.
        assert_eq!(
            index.offset(LineCol { line: 0, col: 2 }, Utf8Bytes),
            Err(OffsetRefusal::ColumnNotBoundary(ColumnNotBoundary {
                line: 0,
                col: 2,
            }))
        );
        // Inside 🦀's surrogate pair.
        assert_eq!(
            index.offset(LineCol { line: 0, col: 3 }, Utf16Units),
            Err(OffsetRefusal::ColumnNotBoundary(ColumnNotBoundary {
                line: 0,
                col: 3,
            }))
        );
    }
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `position` and `offset` are not defined.

- [ ] **Step 3: Implement the conversions**

Add to `impl LineIndex` in `src/line.rs` (plus the two private
helpers):

```rust
    /// Where a byte offset falls, as a coordinate in the stated
    /// encoding. Offsets `0..=len` are in bounds; `len` is the
    /// end-of-text position. Refuses `PositionRefusal` — a
    /// mid-character offset is refused, never misplaced.
    /// O(log lines + log non-ASCII) (base.md §5, §9).
    pub fn position(
        &self,
        offset: ByteOffset,
        encoding: ColumnEncoding,
    ) -> Result<LineCol, PositionRefusal> {
        let max = ByteOffset::new(self.len);
        if offset > max {
            return Err(PositionRefusal::OutOfBounds(
                OffsetOutOfBounds { offset, max },
            ));
        }
        let raw = offset.get();
        let before = self.wide.partition_point(|w| w.offset < raw);
        if before > 0 {
            let last = self.wide[before - 1];
            if raw < last.offset + u32::from(last.byte_len) {
                return Err(PositionRefusal::NotCharBoundary(
                    NotCharBoundary { offset },
                ));
            }
        }
        // The line whose start is last at or before the offset; entry
        // 0 is 0, so the partition point is at least 1.
        let line =
            self.line_starts.partition_point(|&start| start <= raw) - 1;
        let line_start = self.line_starts[line];
        let bytes = raw - line_start;
        let col = match encoding {
            ColumnEncoding::Utf8Bytes => bytes,
            ColumnEncoding::CodePoints => {
                bytes
                    - self.surplus_between(line_start, raw, &self.cp_surplus)
            }
            ColumnEncoding::Utf16Units => {
                bytes
                    - self.surplus_between(
                        line_start,
                        raw,
                        &self.utf16_surplus,
                    )
            }
        };
        // The cast cannot truncate: line < line_count <= u32::MAX.
        Ok(LineCol { line: line as u32, col })
    }

    /// The byte offset of a coordinate in the stated encoding. A
    /// coordinate produced under one encoding and queried under
    /// another breaches the stated contract (base.md §5); on
    /// multi-byte text it surfaces as a refusal or a wrong position.
    /// Refuses `OffsetRefusal`; O(log lines + log non-ASCII)
    /// (base.md §5, §9).
    pub fn offset(
        &self,
        pos: LineCol,
        encoding: ColumnEncoding,
    ) -> Result<ByteOffset, OffsetRefusal> {
        let line_count = self.line_count();
        if pos.line >= line_count {
            return Err(OffsetRefusal::LineOutOfBounds(
                LineOutOfBounds { line: pos.line, line_count },
            ));
        }
        let line_start = self.line_starts[pos.line as usize];
        let content_end =
            match self.line_starts.get(pos.line as usize + 1) {
                Some(next_start) => next_start - 1,
                None => self.len,
            };
        let extent_bytes = content_end - line_start;
        let table = match encoding {
            ColumnEncoding::Utf8Bytes => {
                // Bytes: bound, then boundary, directly.
                if pos.col > extent_bytes {
                    return Err(OffsetRefusal::ColumnOutOfBounds(
                        ColumnOutOfBounds {
                            line: pos.line,
                            col: pos.col,
                            max: extent_bytes,
                        },
                    ));
                }
                let raw = line_start + pos.col;
                let before =
                    self.wide.partition_point(|w| w.offset < raw);
                if before > 0 {
                    let last = self.wide[before - 1];
                    if raw < last.offset + u32::from(last.byte_len) {
                        return Err(OffsetRefusal::ColumnNotBoundary(
                            ColumnNotBoundary {
                                line: pos.line,
                                col: pos.col,
                            },
                        ));
                    }
                }
                return Ok(ByteOffset::new(raw));
            }
            ColumnEncoding::CodePoints => &self.cp_surplus,
            ColumnEncoding::Utf16Units => &self.utf16_surplus,
        };
        // Unit encodings: consume the line's wide characters that
        // start before the column, in O(log) via the prefix tables.
        let up_to = |i: usize| if i == 0 { 0 } else { table[i - 1] };
        let lo = self.wide.partition_point(|w| w.offset < line_start);
        let hi = self.wide.partition_point(|w| w.offset < content_end);
        let max = extent_bytes - (up_to(hi) - up_to(lo));
        if pos.col > max {
            return Err(OffsetRefusal::ColumnOutOfBounds(
                ColumnOutOfBounds { line: pos.line, col: pos.col, max },
            ));
        }
        let unit_pos = |j: usize| {
            (self.wide[j].offset - line_start) - (up_to(j) - up_to(lo))
        };
        let mut consumed = 0;
        let mut open = hi - lo;
        while consumed < open {
            let mid = consumed + (open - consumed) / 2;
            if unit_pos(lo + mid) < pos.col {
                consumed = mid + 1;
            } else {
                open = mid;
            }
        }
        if consumed > 0 {
            let j = lo + consumed - 1;
            let w = self.wide[j];
            // A wide character's width in this encoding's units; for
            // code points it is 1, so the branch below cannot fire
            // there — only a UTF-16 surrogate pair can be entered.
            let unit_len = u32::from(w.byte_len)
                - (up_to(j + 1) - up_to(j));
            if unit_pos(j) + unit_len > pos.col {
                return Err(OffsetRefusal::ColumnNotBoundary(
                    ColumnNotBoundary { line: pos.line, col: pos.col },
                ));
            }
        }
        let bytes = pos.col + (up_to(lo + consumed) - up_to(lo));
        Ok(ByteOffset::new(line_start + bytes))
    }

    /// Total surplus (bytes minus units) of wide characters starting
    /// in `[from, to)`, where both bounds are character boundaries.
    /// O(log non-ASCII) via the prefix tables.
    fn surplus_between(&self, from: u32, to: u32, table: &[u32]) -> u32 {
        let up_to = |i: usize| if i == 0 { 0 } else { table[i - 1] };
        let lo = self.wide.partition_point(|w| w.offset < from);
        let hi = self.wide.partition_point(|w| w.offset < to);
        up_to(hi) - up_to(lo)
    }
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `line::tests` pass.

- [ ] **Step 5: Append the round-trip and oracle laws**

Append to `tests/properties.rs`:

```rust
mod line_conversions {
    use proptest::prelude::*;
    use themelios_base::line::{
        ColumnEncoding, LineCol, LineIndex, NotCharBoundary,
        PositionRefusal,
    };
    use themelios_base::source::{Source, SourceId};
    use themelios_base::span::ByteOffset;

    const ENCODINGS: [ColumnEncoding; 3] = [
        ColumnEncoding::Utf8Bytes,
        ColumnEncoding::CodePoints,
        ColumnEncoding::Utf16Units,
    ];

    /// Multi-byte-heavy generated text (base.md §10): ASCII, two-,
    /// three-, and four-byte characters, plus every newline shape.
    fn multibyte_text() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just("a"),
                Just("Z"),
                Just(" "),
                Just("é"),
                Just("√"),
                Just("你"),
                Just("🦀"),
                Just("\n"),
                Just("\r"),
                Just("\r\n"),
            ],
            0..120,
        )
        .prop_map(|pieces| pieces.concat())
    }

    /// The naive character-walk oracle: recompute the coordinate from
    /// scratch, one character at a time.
    fn oracle(
        text: &str,
        target: usize,
        encoding: ColumnEncoding,
    ) -> LineCol {
        let (mut line, mut col) = (0u32, 0u32);
        for (i, character) in text.char_indices() {
            if i >= target {
                break;
            }
            if character == '\n' {
                line += 1;
                col = 0;
            } else {
                col += match encoding {
                    ColumnEncoding::Utf8Bytes => {
                        character.len_utf8() as u32
                    }
                    ColumnEncoding::CodePoints => 1,
                    ColumnEncoding::Utf16Units => {
                        character.len_utf16() as u32
                    }
                };
            }
        }
        LineCol { line, col }
    }

    proptest! {
        /// base.md §10: oracle agreement on every boundary, the
        /// round-trip identity in every encoding, refusal on every
        /// non-boundary, refusal past the end.
        #[test]
        fn conversions_agree_with_the_oracle_and_round_trip(
            text in multibyte_text(),
        ) {
            let source =
                Source::new(SourceId::new(0), text.clone())
                    .expect("generated text admits");
            let index = LineIndex::of(&source);
            let boundaries: Vec<usize> = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            for &encoding in &ENCODINGS {
                for &boundary in &boundaries {
                    let offset = ByteOffset::new(boundary as u32);
                    let position = index
                        .position(offset, encoding)
                        .expect("boundary offsets position");
                    prop_assert_eq!(
                        position,
                        oracle(&text, boundary, encoding)
                    );
                    prop_assert_eq!(
                        index.offset(position, encoding),
                        Ok(offset)
                    );
                }
                for byte in 0..text.len() {
                    if !text.is_char_boundary(byte) {
                        let offset = ByteOffset::new(byte as u32);
                        prop_assert_eq!(
                            index.position(offset, encoding),
                            Err(PositionRefusal::NotCharBoundary(
                                NotCharBoundary { offset }
                            ))
                        );
                    }
                }
                prop_assert!(matches!(
                    index.position(
                        ByteOffset::new(text.len() as u32 + 1),
                        encoding,
                    ),
                    Err(PositionRefusal::OutOfBounds(_))
                ));
            }
        }
    }
}
```

- [ ] **Step 6: Run the property laws and the gate**

Run: `cargo test -p themelios-base --test properties`
Expected: 6 passed.
Then the full gate command from Task 1 Step 6. Expected: green —
including any `dead_code` deferred from Task 4 Step 5, now consumed.

- [ ] **Step 7: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "line: position and offset in three encodings; the round-trip and oracle laws"
```

---

### Task 6: The `source` module, catalog half

**Files:**
- Modify: `crates/themelios-base/src/source.rs`
- Create: `crates/themelios-base/tests/sources_laws.rs`

**Derives:** base.md §3.4 (the `Sources` trait, its two laws, the law
checker, the shipped implementor), §9, §10 (the checker exercised
against both outcomes).

**Interfaces:**
- Consumes: `Source`, `SourceId`, `TooLarge` (Task 3); `LineIndex`
  (Tasks 4–5).
- Produces: `Sources` (`name`/`text`/`line_index(SourceId) ->
  Option<_>`); `SourceFacet { Name, Text, Index }`;
  `check_sources_laws(&impl Sources, &[SourceId]) ->
  Vec<SourcesLawViolation>`; `SourcesLawViolation`; `SourceSet`
  (`new()`, `add(String, String) -> Result<SourceId, TooLarge>`, and
  its `Sources` impl). Tasks 9–10's views take `&impl Sources`.

- [ ] **Step 1: Write the failing tests**

Create `crates/themelios-base/tests/sources_laws.rs`:

```rust
//! The `Sources` law checker, exercised against both outcomes
//! (docs/design/base.md §10): the shipped catalog passes by
//! construction; deliberately incomplete and incoherent catalogs
//! fail, naming the breach.

use themelios_base::line::LineIndex;
use themelios_base::source::{
    check_sources_laws, Source, SourceFacet, SourceId, SourceSet,
    Sources, SourcesLawViolation,
};

#[test]
fn the_shipped_catalog_satisfies_the_laws_by_construction() {
    let mut catalog = SourceSet::new();
    let first = catalog
        .add("demo.lp".to_owned(), "p(a).\n".to_owned())
        .expect("small text admits");
    let second = catalog
        .add("other.lp".to_owned(), "q(🦀).".to_owned())
        .expect("small text admits");
    assert_eq!(check_sources_laws(&catalog, &[first, second]), vec![]);
    assert_eq!(catalog.name(first), Some("demo.lp"));
    assert_eq!(catalog.text(second), Some("q(🦀)."));
    assert!(catalog.line_index(first).is_some());
    // Unknown identities answer None — a refusal, never a panic.
    let unknown = SourceId::new(99);
    assert_eq!(catalog.name(unknown), None);
    assert_eq!(catalog.text(unknown), None);
    assert!(catalog.line_index(unknown).is_none());
}

#[test]
fn ids_are_minted_sequentially() {
    let mut catalog = SourceSet::new();
    let first = catalog
        .add("a".to_owned(), String::new())
        .expect("empty text admits");
    let second = catalog
        .add("b".to_owned(), String::new())
        .expect("empty text admits");
    assert_eq!(first, SourceId::new(0));
    assert_eq!(second, SourceId::new(1));
}

/// A catalog that resolves name and text but no index — a
/// completeness breach.
struct MissingIndex {
    text: String,
}

impl Sources for MissingIndex {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("partial.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&LineIndex> {
        None
    }
}

#[test]
fn an_incomplete_catalog_is_named_facet_by_facet() {
    let catalog = MissingIndex { text: "p.".to_owned() };
    let id = SourceId::new(0);
    assert_eq!(
        check_sources_laws(&catalog, &[id]),
        vec![SourcesLawViolation::Incomplete {
            id,
            missing: SourceFacet::Index,
        }]
    );
}

/// A catalog whose index was built from an earlier version of the
/// text — the stale-cache breach, the one route to a misplaced caret
/// that no view can see (base.md §3.4).
struct StaleIndex {
    text: String,
    index: LineIndex,
}

impl StaleIndex {
    fn new() -> StaleIndex {
        let old = Source::new(SourceId::new(0), "one line".to_owned())
            .expect("small text admits");
        StaleIndex {
            text: "two\nlines".to_owned(),
            index: LineIndex::of(&old),
        }
    }
}

impl Sources for StaleIndex {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("stale.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&LineIndex> {
        Some(&self.index)
    }
}

#[test]
fn an_incoherent_index_is_caught_by_rederivation() {
    let catalog = StaleIndex::new();
    let id = SourceId::new(0);
    assert_eq!(
        check_sources_laws(&catalog, &[id]),
        vec![SourcesLawViolation::IncoherentIndex { id }]
    );
}

#[test]
fn an_unknown_id_breaches_nothing() {
    let catalog = SourceSet::new();
    assert_eq!(
        check_sources_laws(&catalog, &[SourceId::new(3)]),
        vec![]
    );
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base --test sources_laws`
Expected: compile error — the catalog types are not defined.

- [ ] **Step 3: Implement the catalog half**

Append to `src/source.rs` (and add `use crate::line::LineIndex;` to
its imports):

```rust
/// The view environment: resolves identity to display data. Two laws
/// bind every implementor (base.md §3.4):
///
/// 1. **Completeness.** For a given id, answer all three accessors or
///    none. Partial resolution is a contract breach, not a state the
///    views must interpret.
/// 2. **Coherence.** `line_index(id)` is observationally
///    `LineIndex::of` applied to an admitted `Source` of `text(id)` —
///    the index *of* that text, not of any earlier version of it.
///
/// Unknown identities answer `None` — a refusal, never a panic. The
/// name is whatever the host declares: display data, not a path. What
/// the views can check, they check — a completeness breach is named
/// at view time. Coherence is not cheaply checkable there, so the
/// views trust it; that trust is this contract's stated boundary, and
/// `check_sources_laws` is the test-time instrument that holds it.
pub trait Sources {
    /// The display name the host declares for this source.
    fn name(&self, id: SourceId) -> Option<&str>;
    /// The source's text.
    fn text(&self, id: SourceId) -> Option<&str>;
    /// The line index of exactly that text.
    fn line_index(&self, id: SourceId) -> Option<&LineIndex>;
}

/// Which accessor a resolution lacked — used by the law checker's
/// report, the editor view's refusal, and the human view's
/// placeholder (base.md §3.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceFacet {
    /// `Sources::name` answered `None`.
    Name,
    /// `Sources::text` answered `None`.
    Text,
    /// `Sources::line_index` answered `None`.
    Index,
}

/// One law breach found by `check_sources_laws` (base.md §3.4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SourcesLawViolation {
    /// The id resolved some facets but not all three.
    Incomplete {
        /// The id whose resolution was partial.
        id: SourceId,
        /// A facet that did not resolve.
        missing: SourceFacet,
    },
    /// The resolved index is not the index of the resolved text.
    IncoherentIndex {
        /// The id whose index disagrees with its text.
        id: SourceId,
    },
}

/// The laws, checkable: verifies completeness per id and coherence by
/// re-deriving the index from the resolved text. Deterministic and
/// total — an empty report is the laws holding over those ids;
/// O(total resolved text). Implementors run it in their own tests
/// (base.md §3.4).
///
/// Ids are checked in the order given, one `Incomplete` per missing
/// facet in `Name`, `Text`, `Index` order; coherence is checked
/// whenever text and index both resolve, independent of the
/// completeness verdict.
pub fn check_sources_laws(
    sources: &impl Sources,
    ids: &[SourceId],
) -> Vec<SourcesLawViolation> {
    let mut violations = Vec::new();
    for &id in ids {
        let name = sources.name(id);
        let text = sources.text(id);
        let index = sources.line_index(id);
        let facets = [
            (name.is_some(), SourceFacet::Name),
            (text.is_some(), SourceFacet::Text),
            (index.is_some(), SourceFacet::Index),
        ];
        let resolved =
            facets.iter().filter(|(present, _)| *present).count();
        if 0 < resolved && resolved < facets.len() {
            for (present, facet) in facets {
                if !present {
                    violations.push(SourcesLawViolation::Incomplete {
                        id,
                        missing: facet,
                    });
                }
            }
        }
        if let (Some(text), Some(index)) = (text, index) {
            // A text the admission door refuses cannot have an index
            // that is LineIndex::of over an admitted Source of it —
            // incoherent by definition.
            let coherent = Source::new(id, text.to_owned())
                .map(|admitted| LineIndex::of(&admitted) == *index)
                .unwrap_or(false);
            if !coherent {
                violations
                    .push(SourcesLawViolation::IncoherentIndex { id });
            }
        }
    }
    violations
}

/// A Vec-backed catalog that mints ids sequentially — a host you can
/// use, not this crate seizing minting. Satisfies both laws by
/// construction: it admits under the `Source` doors and builds each
/// `LineIndex` eagerly at `add` — an explicit derivation, not lazy
/// state. Deliberately not a virtual file system, and it never grows
/// toward one: no paths, no watching, no loading (base.md §3.4, §11).
#[derive(Clone, Debug)]
pub struct SourceSet {
    entries: Vec<SourceSetEntry>,
}

#[derive(Clone, Debug)]
struct SourceSetEntry {
    name: String,
    source: Source,
    index: LineIndex,
}

impl SourceSet {
    /// An empty catalog. Total; O(1).
    pub fn new() -> SourceSet {
        SourceSet { entries: Vec::new() }
    }

    /// Admits one source under the `Source` doors, builds its index
    /// eagerly, and mints the next sequential id. Refuses `TooLarge`;
    /// O(n) — admission plus the index build (base.md §3.4, §9).
    pub fn add(
        &mut self,
        name: String,
        text: String,
    ) -> Result<SourceId, TooLarge> {
        // The cast cannot truncate in any real embedding: entries own
        // their text, so a wrapped mint would need more entries than
        // memory holds bytes.
        let id = SourceId::new(self.entries.len() as u32);
        let source = Source::new(id, text)?;
        let index = LineIndex::of(&source);
        self.entries.push(SourceSetEntry { name, source, index });
        Ok(id)
    }

    fn entry(&self, id: SourceId) -> Option<&SourceSetEntry> {
        self.entries.get(id.get() as usize)
    }
}

impl Default for SourceSet {
    // The std idiom for an argument-free constructor;
    // clippy::new_without_default holds it.
    fn default() -> SourceSet {
        SourceSet::new()
    }
}

impl Sources for SourceSet {
    fn name(&self, id: SourceId) -> Option<&str> {
        self.entry(id).map(|entry| entry.name.as_str())
    }

    fn text(&self, id: SourceId) -> Option<&str> {
        self.entry(id).map(|entry| entry.source.text())
    }

    fn line_index(&self, id: SourceId) -> Option<&LineIndex> {
        self.entry(id).map(|entry| &entry.index)
    }
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base --test sources_laws`
Expected: 5 passed.

- [ ] **Step 5: Run the gate**

Run: the full gate command from Task 1 Step 6.
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "source: the Sources contract, its checkable laws, and the SourceSet catalog"
```

---

### Task 7: The `diagnostic` module, identity half

**Files:**
- Create: `crates/themelios-base/src/diagnostic.rs`
- Modify: `crates/themelios-base/src/lib.rs` (add `pub mod diagnostic;`)

**Derives:** base.md §6.1 (identity), §6.2 (severity, with the
closedness tradeoff), §6.3 (labels), §8.5 (the two identity renderings
are contract), §9.

**Interfaces:**
- Consumes: `Location` (Task 3).
- Produces: `DiagnosticId` (`const new(&'static str, &'static str)`,
  `namespace()`, `name()`, `Display` as `namespace::name`); `Severity
  { Note, Warning, Error }` (`Ord` least-severe-first, `Display`
  lowercase); `Label { location, message: Option<String> }` (`Ord`
  location-first). Task 8's `Diagnostic` is built from these.

- [ ] **Step 1: Write the failing tests**

Add `pub mod diagnostic;` to `src/lib.rs`. Create `src/diagnostic.rs`
with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::span::{ByteOffset, Location, Span};

    // The tier-side idiom: identities as compile-time constants.
    const UNEXPECTED: DiagnosticId =
        DiagnosticId::new("syntax", "unexpected-token");

    #[test]
    fn identity_is_namespace_and_name() {
        assert_eq!(UNEXPECTED.namespace(), "syntax");
        assert_eq!(UNEXPECTED.name(), "unexpected-token");
    }

    #[test]
    fn identity_renders_as_namespace_colon_colon_name() {
        // The documented rendering IS the Display impl — contract,
        // stable (base.md §8.5).
        assert_eq!(UNEXPECTED.to_string(), "syntax::unexpected-token");
    }

    #[test]
    fn identity_orders_by_namespace_then_name() {
        let a = DiagnosticId::new("program", "zzz");
        let b = DiagnosticId::new("syntax", "aaa");
        assert!(a < b);
    }

    #[test]
    fn severity_declares_least_severe_first() {
        assert!(Severity::Note < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        // Worst-first sorting is therefore descending order.
        let mut severities =
            [Severity::Warning, Severity::Error, Severity::Note];
        severities.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            severities,
            [Severity::Error, Severity::Warning, Severity::Note]
        );
    }

    #[test]
    fn severity_renders_lowercase() {
        // Contract, stable (base.md §8.5).
        assert_eq!(Severity::Note.to_string(), "note");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Error.to_string(), "error");
    }

    #[test]
    fn labels_order_by_location_first() {
        let early = Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(1), ByteOffset::new(2))
                    .expect("ordered endpoints"),
            },
            message: Some("zzz".to_owned()),
        };
        let late = Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(5), ByteOffset::new(6))
                    .expect("ordered endpoints"),
            },
            message: None,
        };
        assert!(early < late);
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — the diagnostic types are not defined.

- [ ] **Step 3: Implement the identity half**

Prepend to `src/diagnostic.rs`:

```rust
//! The diagnostics model (docs/design/base.md §6): a report about
//! source, located by construction, with a stable machine identity.
//! Solve outcomes, faults, and progress events are not diagnostics —
//! they have their own models — and an unlocated report is not a
//! degenerate diagnostic but a different thing.

use std::fmt;

use crate::span::Location;

/// The stable machine identity of one diagnostic kind: a namespace
/// (the emitting tier) and a kebab-case name (base.md §6.1). No
/// numeric codes: at diagnostic scale, the no-magic-numbers policy
/// means the name *is* the identity.
///
/// The constructor is total and `const` — each emitting tier defines
/// its identities as compile-time constants and owns its table.
/// Quality (kebab-case, non-empty, meaningful) and stability are held
/// by each tier snapshot-testing its complete identity table: an
/// identity, once shipped, is stable; renaming is a visible breaking
/// change. This crate defines the type; it deliberately does not
/// police tables it cannot see.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DiagnosticId {
    namespace: &'static str,
    name: &'static str,
}

impl DiagnosticId {
    /// Names one diagnostic kind. Total, `const`; O(1).
    pub const fn new(
        namespace: &'static str,
        name: &'static str,
    ) -> DiagnosticId {
        DiagnosticId { namespace, name }
    }

    /// The emitting tier's namespace. Total, `const`; O(1).
    pub const fn namespace(self) -> &'static str {
        self.namespace
    }

    /// The kebab-case kind name. Total, `const`; O(1).
    pub const fn name(self) -> &'static str {
        self.name
    }
}

impl fmt::Display for DiagnosticId {
    /// The documented rendering `namespace::name` — contract carried
    /// by the type, stable (base.md §8.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.name)
    }
}

/// Closed. Declared least-severe first, so `Error` is the maximum and
/// worst-first sorting is descending order (base.md §6.2).
///
/// Closedness is a ruled tradeoff, both sides on the page: closed
/// buys every consumer exhaustive matching on the specification's own
/// trichotomy; the price, accepted, is that admitting a later
/// severity (the recorded `Hint` pressure) is a breaking change
/// through every exhaustive match — priced correctly by the pre-1.0
/// stability posture, since this surface will not have frozen before
/// the language-server consumer checkpoint runs. `#[non_exhaustive]`
/// was considered and rejected: it would tax every consumer with a
/// wildcard arm on a closed trichotomy today to hedge a pressure that
/// has no producer yet.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity {
    /// Informational, and a real standalone severity — a solver
    /// frontend ships its engine's informational class as its own
    /// face — not merely an attachment role.
    Note,
    /// A defect worth reporting that does not defeat the operation.
    Warning,
    /// A defect that defeats the operation reported on.
    Error,
}

impl fmt::Display for Severity {
    /// The documented lowercase rendering — contract carried by the
    /// type, stable (base.md §8.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        f.write_str(word)
    }
}

/// A located message (base.md §6.3). Fields are public: any location
/// with any optional message is a valid label; there is no invariant
/// to guard. Derived ordering is location-first, which is what lets
/// render order be *derived* by position.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Label {
    /// Where the label points.
    pub location: Location,
    /// `None` when the diagnostic's headline already covers it — an
    /// honest absence, not an empty-string sentinel.
    pub message: Option<String>,
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `diagnostic::tests` pass.

- [ ] **Step 5: Run the gate and commit**

Run: the full gate command from Task 1 Step 6. Expected: green.

```bash
git add crates/themelios-base/src
git commit -m "diagnostic: stable identity, the closed severity trichotomy, and labels"
```

---

### Task 8: The `diagnostic` module, model half

**Files:**
- Modify: `crates/themelios-base/src/diagnostic.rs`

**Derives:** base.md §6.4 (the Diagnostic, set semantics, what is not
a diagnostic), §6.5 (the lowering contract), §8.3–§8.5, §9 (including
the admitted-empty-attachments rule).

**Interfaces:**
- Consumes: `DiagnosticId`, `Severity`, `Label` (Task 7).
- Produces: `Diagnostic` (`new(DiagnosticId, Severity, String, Label)
  -> Result<Diagnostic, EmptyMessage>`; by-value `with_secondary(
  Label)`, `with_note(String)`, `with_help(String)`; accessors `id()`,
  `severity()`, `message() -> &str`, `primary() -> &Label`,
  `secondary() -> &BTreeSet<Label>`, `notes() -> &[String]`,
  `helps() -> &[String]`); `EmptyMessage`; `ToDiagnostic
  { to_diagnostic(&self) -> Diagnostic }` with its two impls. Tasks
  9–11 consume `Diagnostic`; every tier above implements
  `ToDiagnostic`.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `src/diagnostic.rs`:

```rust
    fn label_at(start: u32, end: u32) -> Label {
        Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(
                    ByteOffset::new(start),
                    ByteOffset::new(end),
                )
                .expect("ordered endpoints"),
            },
            message: None,
        }
    }

    fn demo() -> Diagnostic {
        Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            label_at(10, 14),
        )
        .expect("non-empty headline")
    }

    #[test]
    fn construction_refuses_an_empty_headline() {
        assert_eq!(
            Diagnostic::new(
                UNEXPECTED,
                Severity::Error,
                String::new(),
                label_at(0, 1),
            ),
            Err(EmptyMessage)
        );
    }

    #[test]
    fn chaining_builds_and_accessors_answer() {
        let diagnostic = demo()
            .with_secondary(label_at(2, 5))
            .with_note("the statement began here".to_owned())
            .with_help("add `.`".to_owned());
        assert_eq!(diagnostic.id(), UNEXPECTED);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(
            diagnostic.message(),
            "expected `.` after the rule body"
        );
        assert_eq!(diagnostic.primary(), &label_at(10, 14));
        assert_eq!(diagnostic.secondary().len(), 1);
        assert_eq!(
            diagnostic.notes(),
            ["the statement began here".to_owned()]
        );
        assert_eq!(diagnostic.helps(), ["add `.`".to_owned()]);
    }

    #[test]
    fn secondary_labels_are_a_set() {
        // A duplicate insert yields the same set — set semantics, not
        // repair; equality is set equality: emission order carries no
        // meaning (base.md §6.4).
        let once = demo().with_secondary(label_at(2, 5));
        let twice = demo()
            .with_secondary(label_at(2, 5))
            .with_secondary(label_at(2, 5));
        assert_eq!(once, twice);

        let forward = demo()
            .with_secondary(label_at(2, 5))
            .with_secondary(label_at(7, 9));
        let backward = demo()
            .with_secondary(label_at(7, 9))
            .with_secondary(label_at(2, 5));
        assert_eq!(forward, backward);
        // Iteration is deterministic in exactly the derived order:
        // by position.
        let spans: Vec<u32> = forward
            .secondary()
            .iter()
            .map(|label| label.location.span.start().get())
            .collect();
        assert_eq!(spans, [2, 7]);
    }

    #[test]
    fn notes_are_a_narrative_in_order() {
        let diagnostic = demo()
            .with_note("first".to_owned())
            .with_note("second".to_owned());
        assert_eq!(
            diagnostic.notes(),
            ["first".to_owned(), "second".to_owned()]
        );
    }

    #[test]
    fn empty_attachments_are_admitted_unaltered() {
        // Accepting a value as-is is not repair; attachment quality
        // is the emitting tier's obligation (base.md §9).
        let diagnostic = demo().with_note(String::new());
        assert_eq!(diagnostic.notes(), [String::new()]);
    }

    #[test]
    fn lowering_is_identity_for_the_normal_form() {
        let diagnostic = demo();
        assert_eq!(diagnostic.to_diagnostic(), diagnostic);
        // And composes through references, for uniform pipelines.
        let by_ref: &dyn ToDiagnostic = &&diagnostic;
        assert_eq!(by_ref.to_diagnostic(), diagnostic);
    }

    #[test]
    fn empty_message_displays_the_fixable_question() {
        assert_eq!(
            EmptyMessage.to_string(),
            "the headline message is empty; every view depends on it"
        );
        let _: &dyn std::error::Error = &EmptyMessage;
    }
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `Diagnostic`, `EmptyMessage`, `ToDiagnostic`
are not defined.

- [ ] **Step 3: Implement the model half**

Append to `src/diagnostic.rs` (add `use std::collections::BTreeSet;`
to its imports):

```rust
/// The one refusal construction can issue, carried as the condition
/// itself (base.md §6.4, §3.2): an empty headline would break every
/// view by construction. It is the one structural emptiness this
/// crate refuses — empty attachment strings are admitted unaltered,
/// because accepting a value as-is is not repair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EmptyMessage;

impl fmt::Display for EmptyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the headline message is empty; every view depends on it",
        )
    }
}

impl std::error::Error for EmptyMessage {}

/// A report about source. Located by construction: the primary label
/// is required, so "a diagnostic without a precise span" is
/// unrepresentable (base.md §6.4). Equality and hash are structural —
/// and for the secondary labels that means *set* equality.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    id: DiagnosticId,
    severity: Severity,
    /// The headline; never empty.
    message: String,
    primary: Label,
    /// A set, mathematically: render order is derived by position
    /// (`Label`'s ordering is location-first), so emission order
    /// carries no meaning and a duplicate label is a defect —
    /// `BTreeSet` makes duplicates unrepresentable and iteration
    /// deterministic in exactly the derived order (base.md §6.4,
    /// §8.4).
    secondary: BTreeSet<Label>,
    /// A narrative, in order: order is meaning here (base.md §6.3).
    notes: Vec<String>,
    /// Likewise.
    helps: Vec<String>,
}

impl Diagnostic {
    /// Refuses `EmptyMessage`; O(1) beyond the owned text
    /// (base.md §6.4, §9).
    pub fn new(
        id: DiagnosticId,
        severity: Severity,
        message: String,
        primary: Label,
    ) -> Result<Diagnostic, EmptyMessage> {
        if message.is_empty() {
            return Err(EmptyMessage);
        }
        Ok(Diagnostic {
            id,
            severity,
            message,
            primary,
            secondary: BTreeSet::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        })
    }

    /// Adds a secondary label — by-value chaining, so even building
    /// reads as declaring (base.md §8.3). Inserting a label already
    /// present yields the same set: set semantics, not repair. Total;
    /// O(log secondaries).
    pub fn with_secondary(mut self, label: Label) -> Diagnostic {
        self.secondary.insert(label);
        self
    }

    /// Appends to the note narrative. Total; O(1) beyond owned text.
    pub fn with_note(mut self, note: String) -> Diagnostic {
        self.notes.push(note);
        self
    }

    /// Appends to the help narrative. Total; O(1) beyond owned text.
    pub fn with_help(mut self, help: String) -> Diagnostic {
        self.helps.push(help);
        self
    }

    /// The stable machine identity. Total; O(1).
    pub fn id(&self) -> DiagnosticId {
        self.id
    }

    /// The severity. Total; O(1).
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The headline; never empty. Total; O(1).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The required primary label. Total; O(1).
    pub fn primary(&self) -> &Label {
        &self.primary
    }

    /// The secondary labels — a set; iteration is position order.
    /// Total; O(1).
    pub fn secondary(&self) -> &BTreeSet<Label> {
        &self.secondary
    }

    /// The note narrative, in order. Total; O(1).
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The help narrative, in order. Total; O(1).
    pub fn helps(&self) -> &[String] {
        &self.helps
    }
}

/// Tier-typed diagnostics lower into the normal form by reference:
/// the typed value outlives its transport form (base.md §6.5). Each
/// tier defines its *own* fully typed diagnostics and lowers them
/// into this crate's normal form for uniform rendering and transport;
/// in-process consumers act on the tier's typed values, and pipelines
/// that only render or forward take `impl ToDiagnostic` uniformly.
///
/// The name departs the standard conversion vocabulary deliberately:
/// `Into` consumes and says only "can convert"; this trait borrows
/// and declares a semantic relationship — *this value is a diagnostic
/// in tier-typed form*. One method, no provided machinery: a
/// contract, not a framework.
pub trait ToDiagnostic {
    /// This value, in the normal form.
    fn to_diagnostic(&self) -> Diagnostic;
}

impl ToDiagnostic for Diagnostic {
    /// Identity, by clone: the normal form of a `Diagnostic` is
    /// itself.
    fn to_diagnostic(&self) -> Diagnostic {
        self.clone()
    }
}

impl<T: ToDiagnostic + ?Sized> ToDiagnostic for &T {
    fn to_diagnostic(&self) -> Diagnostic {
        (**self).to_diagnostic()
    }
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `diagnostic::tests` pass.

- [ ] **Step 5: Run the gate and commit**

Run: the full gate command from Task 1 Step 6. Expected: green.

```bash
git add crates/themelios-base/src
git commit -m "diagnostic: the located-by-construction report and the lowering contract"
```

---

### Task 9: `view::human` and the golden seed corpus

**Files:**
- Create: `crates/themelios-base/src/view.rs`,
  `crates/themelios-base/tests/golden.rs`,
  `crates/themelios-base/tests/golden/*.txt` (blessed in Step 6)
- Modify: `crates/themelios-base/src/lib.rs` (add `pub mod view;`),
  `crates/themelios-base/tests/properties.rs` (append)

**Derives:** base.md §7 (views as pure derivations, no view trait),
§7.1 (the human view: layout commitments, full degradation), §3.3
(honest-and-maximal), §9 (totality), §10 (golden snapshots, the
human-totality law).

**Interfaces:**
- Consumes: `Diagnostic`, `Label` (Tasks 7–8); `Sources`,
  `SourceFacet` (Task 6); `LineIndex`, `ColumnEncoding` (Tasks 4–5);
  `Span`, `ByteOffset` (Task 2).
- Produces: `view::human(&Diagnostic, &impl Sources) -> String` —
  total, deterministic, zero options.

**Layout mechanics, committed here** (the design holds these at
implementation level, reviewable by the golden corpus — base.md §7.1):

- Headline: `{severity}[{namespace}::{name}]: {message}`.
- Blocks: the primary's source first, then every other touched source
  in identity order; within a block, labels in position order.
- Block header: ` --> {name}:{line}:{col}`, one-based (the crate's
  sole one-based surface, presentation only), columns in code points;
  coordinates come from the primary label when the block holds it and
  it fits, else from the block's first fitting label, else the header
  carries no coordinates.
- Gutter: right-aligned one-based line numbers, width from the block's
  largest rendered line number; a `..` row marks elided lines between
  non-adjacent rendered lines; only labeled lines render.
- Underlines: `^` for the primary, `-` for secondaries, one row per
  label per touched line covering the label's overlap with the line's
  content (clamped; minimum width one on the label's anchor line); the
  label's message follows the row on its last touched line.
- Then `  = note: {text}` per note, then `  = help: {text}` per help
  (fixed two-space prefix).
- Placeholders (each has a golden case): unresolved id →
  ` --> <source {id}: unresolved>`; missing facet →
  `<source {id}: missing {name|text|index}>` (header for name, a
  gutter row for text/index); a label that does not fit the resolved
  text → a gutter row
  `<label {start}..{end} does not fit source {id}: {reason}>` while
  every coherent label still renders. Output always ends with one
  newline. Nothing panics; a coherence-breaching catalog (stale
  index) may render wrong *content* — the trust boundary base.md §3.4
  states — but never a panic: line text is fetched fallibly and
  substitutes a placeholder when the index and text disagree.

- [ ] **Step 1: Write the failing test pinning the core format**

Add `pub mod view;` to `src/lib.rs`. Create `src/view.rs` with only
the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
    use crate::source::SourceSet;
    use crate::span::{ByteOffset, Location, Span};

    const UNEXPECTED: DiagnosticId =
        DiagnosticId::new("syntax", "unexpected-token");

    #[test]
    fn the_single_span_rendering_is_the_committed_layout() {
        let mut catalog = SourceSet::new();
        let file = catalog
            .add(
                "demo.lp".to_owned(),
                "p(a).\nq(X) :- r(X)\n".to_owned(),
            )
            .expect("small text admits");
        // "r(X)" occupies bytes 14..18, line 2 column 9, one-based.
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            Label {
                location: Location {
                    source: file,
                    span: Span::new(
                        ByteOffset::new(14),
                        ByteOffset::new(18),
                    )
                    .expect("ordered endpoints"),
                },
                message: Some("the rule body ends here".to_owned()),
            },
        )
        .expect("non-empty headline");

        let rendered = human(&diagnostic, &catalog);
        let expected = "\
error[syntax::unexpected-token]: expected `.` after the rule body
 --> demo.lp:2:9
  |
2 | q(X) :- r(X)
  |         ^^^^ the rule body ends here
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn an_unresolvable_source_renders_a_named_placeholder() {
        let catalog = SourceSet::new();
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Warning,
            "w".to_owned(),
            Label {
                location: Location {
                    source: crate::source::SourceId::new(7),
                    span: Span::empty(ByteOffset::ZERO),
                },
                message: None,
            },
        )
        .expect("non-empty headline");
        let rendered = human(&diagnostic, &catalog);
        let expected = "\
warning[syntax::unexpected-token]: w
 --> <source 7: unresolved>
";
        assert_eq!(rendered, expected);
    }
}
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `human` is not defined.

- [ ] **Step 3: Implement the human view**

Prepend to `src/view.rs`:

```rust
//! Views: pure derivations over `(&Diagnostic, &impl Sources)`
//! (docs/design/base.md §7). There is deliberately no view trait —
//! the open extension point for a new view is the model being public
//! plain data; anyone writes a function over it. The polymorphism a
//! view does need is over its *environment*, and that is the
//! `Sources` trait.

use crate::diagnostic::{Diagnostic, Label};
use crate::line::{ColumnEncoding, LineCol, LineIndex, PositionRefusal};
use crate::source::{SourceId, Sources};

/// The human rendering: total and deterministic, with zero options —
/// one canonical output, which is what a reviewable golden corpus
/// requires; color and width knobs are named view evolution, not v1
/// surface (base.md §7.1).
///
/// Degradation is honest and maximal (base.md §3.3, §7.1): an
/// unresolvable id, a completeness breach, and a label that does not
/// fit its resolved text each render a named inline placeholder while
/// every coherent label still renders. Total — nothing panics,
/// nothing is silently dropped; O(rendered size), flat iteration.
pub fn human(diagnostic: &Diagnostic, sources: &impl Sources) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}[{}]: {}\n",
        diagnostic.severity(),
        diagnostic.id(),
        diagnostic.message()
    ));
    // The primary's source first, then other touched sources in
    // identity order (secondary iteration is position order, which
    // groups by identity).
    let primary_source = diagnostic.primary().location.source;
    let mut block_order = vec![primary_source];
    for label in diagnostic.secondary() {
        if !block_order.contains(&label.location.source) {
            block_order.push(label.location.source);
        }
    }
    for source in block_order {
        // Within a block: labels in position order, primary marked.
        let mut labels: Vec<(&Label, bool)> = Vec::new();
        for label in diagnostic.secondary() {
            if label.location.source == source {
                labels.push((label, false));
            }
        }
        if primary_source == source {
            let primary = (diagnostic.primary(), true);
            let at = labels
                .partition_point(|(label, _)| *label < diagnostic.primary());
            labels.insert(at, primary);
        }
        render_block(&mut out, source, &labels, sources);
    }
    for note in diagnostic.notes() {
        out.push_str(&format!("  = note: {note}\n"));
    }
    for help in diagnostic.helps() {
        out.push_str(&format!("  = help: {help}\n"));
    }
    out
}

/// One label the block can render: both span ends positioned.
struct Fitting<'a> {
    label: &'a Label,
    primary: bool,
    start: LineCol,
    /// End position, with an end falling at column 0 of a later line
    /// pulled back so a span covering a terminator underlines its own
    /// line, not the next one's zero columns.
    end: LineCol,
}

fn render_block(
    out: &mut String,
    source: SourceId,
    labels: &[(&Label, bool)],
    sources: &impl Sources,
) {
    let name = sources.name(source);
    let text = sources.text(source);
    let index = sources.line_index(source);
    if name.is_none() && text.is_none() && index.is_none() {
        out.push_str(&format!(
            " --> <source {}: unresolved>\n",
            source.get()
        ));
        return;
    }
    // A partial resolution is a completeness breach (base.md §3.4);
    // the placeholder names the missing facet.
    let display = match name {
        Some(name) => name.to_owned(),
        None => format!("<source {}: missing name>", source.get()),
    };
    let (text, index) = match (text, index) {
        (Some(text), Some(index)) => (text, index),
        (None, _) => {
            out.push_str(&format!(" --> {display}\n"));
            out.push_str(&format!(
                "  | <source {}: missing text>\n",
                source.get()
            ));
            return;
        }
        (_, None) => {
            out.push_str(&format!(" --> {display}\n"));
            out.push_str(&format!(
                "  | <source {}: missing index>\n",
                source.get()
            ));
            return;
        }
    };

    let mut fitting: Vec<Fitting<'_>> = Vec::new();
    let mut misfits: Vec<(&Label, PositionRefusal)> = Vec::new();
    for &(label, primary) in labels {
        let span = label.location.span;
        let cp = ColumnEncoding::CodePoints;
        match (
            index.position(span.start(), cp),
            index.position(span.end(), cp),
        ) {
            (Ok(start), Ok(mut end)) => {
                if end.line > start.line && end.col == 0 {
                    end.line -= 1;
                    end.col = line_extent(index, text, end.line);
                }
                fitting.push(Fitting { label, primary, start, end });
            }
            (Err(refusal), _) | (_, Err(refusal)) => {
                misfits.push((label, refusal));
            }
        }
    }

    // Header coordinates: the primary if it fits, else the first
    // fitting label, else none.
    let anchor = fitting
        .iter()
        .find(|fit| fit.primary)
        .or_else(|| fitting.first());
    match anchor {
        Some(fit) => out.push_str(&format!(
            " --> {display}:{}:{}\n",
            fit.start.line + 1,
            fit.start.col + 1
        )),
        None => out.push_str(&format!(" --> {display}\n")),
    }

    // Only labeled lines render; `..` marks an elision. One pass
    // maps each rendered line to the labels touching it, in position
    // order, so the walk below is flat — O(rendered rows), the
    // stated cost (base.md §7.1, §9) — instead of scanning every
    // label per line.
    let mut rows: std::collections::BTreeMap<u32, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (fit_index, fit) in fitting.iter().enumerate() {
        for line in fit.start.line..=fit.end.line {
            rows.entry(line).or_default().push(fit_index);
        }
    }
    let width = rows
        .keys()
        .next_back()
        .map(|last| (last + 1).to_string().len())
        .unwrap_or(1);
    let pad = " ".repeat(width);
    out.push_str(&format!("{pad} |\n"));

    let mut previous: Option<u32> = None;
    for (&line, touching) in &rows {
        if let Some(previous) = previous {
            if line != previous + 1 {
                out.push_str(&format!("{pad}..\n"));
            }
        }
        previous = Some(line);
        // Fetched fallibly: a coherence-breaching catalog may hand an
        // index that disagrees with its text; trusted never means
        // panic-licensed (base.md §3.4, §7.1).
        let content = index
            .line_span(line)
            .ok()
            .and_then(|span| {
                text.get(
                    span.start().get() as usize..span.end().get() as usize,
                )
            })
            .unwrap_or("<line does not fit the resolved text>");
        out.push_str(&format!(
            "{:>width$} | {content}\n",
            line + 1,
            width = width
        ));
        for &fit_index in touching {
            let fit = &fitting[fit_index];
            let extent = line_extent(index, text, line);
            let from = if line == fit.start.line { fit.start.col } else { 0 };
            let to = if line == fit.end.line {
                fit.end.col.min(extent)
            } else {
                extent
            };
            let anchor_line = line == fit.start.line;
            let width_cols = to.saturating_sub(from);
            if width_cols == 0 && !anchor_line {
                continue;
            }
            let marker = if fit.primary { '^' } else { '-' };
            let underline =
                marker.to_string().repeat(width_cols.max(1) as usize);
            let mut row = format!(
                "{pad} | {}{underline}",
                " ".repeat(from as usize)
            );
            if line == fit.end.line {
                if let Some(message) = &fit.label.message {
                    row.push(' ');
                    row.push_str(message);
                }
            }
            row.push('\n');
            out.push_str(&row);
        }
    }
    for (label, refusal) in misfits {
        let span = label.location.span;
        let reason = match refusal {
            PositionRefusal::OutOfBounds(oob) => {
                format!("the text ends at byte {}", oob.max.get())
            }
            PositionRefusal::NotCharBoundary(ncb) => {
                format!("byte {} splits a character", ncb.offset.get())
            }
        };
        out.push_str(&format!(
            "{pad} | <label {}..{} does not fit source {}: {reason}>\n",
            span.start().get(),
            span.end().get(),
            source.get()
        ));
    }
}

/// A line's content extent in code points, fallibly: zero when the
/// index and text disagree (the coherence trust boundary).
fn line_extent(index: &LineIndex, text: &str, line: u32) -> u32 {
    index
        .line_span(line)
        .ok()
        .and_then(|span| {
            text.get(span.start().get() as usize..span.end().get() as usize)
        })
        .map(|content| content.chars().count() as u32)
        .unwrap_or(0)
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: both `view::tests` pass — the committed layout, exactly.

- [ ] **Step 5: Write the golden harness and the nine seed cases**

Create `crates/themelios-base/tests/golden.rs`. The harness is
std-only — compare, or bless under an environment switch — so the
dev-dependency list stays exactly the two named instruments:

```rust
//! The human renderer's golden seed corpus (docs/design/base.md §10),
//! reviewed against the rust-analyzer bar. Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-base --test golden`, then
//! review the diff before committing: these files are reviewed
//! artifacts, not incidental output.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::{
    Diagnostic, DiagnosticId, Label, Severity,
};
use themelios_base::source::{SourceId, SourceSet};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_base::view::human;

const UNEXPECTED: DiagnosticId =
    DiagnosticId::new("syntax", "unexpected-token");

fn check(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::write(&path, actual).expect("golden file writes");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; bless it and review the rendering",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "rendering diverged from the reviewed golden `{name}`"
    );
}

fn label(source: SourceId, start: u32, end: u32, message: Option<&str>) -> Label {
    Label {
        location: Location {
            source,
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end))
                .expect("ordered endpoints"),
        },
        message: message.map(str::to_owned),
    }
}

fn demo_catalog() -> (SourceSet, SourceId) {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add(
            "demo.lp".to_owned(),
            "p(a).\nq(X) :- r(X)\ns(1..3).\n% done\n".to_owned(),
        )
        .expect("small text admits");
    (catalog, file)
}

fn diagnostic(primary: Label) -> Diagnostic {
    Diagnostic::new(
        UNEXPECTED,
        Severity::Error,
        "expected `.` after the rule body".to_owned(),
        primary,
    )
    .expect("non-empty headline")
}

#[test]
fn single_span() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, Some("the rule body ends here")));
    check("single-span", &human(&d, &catalog));
}

#[test]
fn multiple_spans_on_one_line() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, Some("this literal")))
        .with_secondary(label(file, 6, 10, Some("while parsing this head")));
    check("multiple-spans-one-line", &human(&d, &catalog));
}

#[test]
fn multi_line_span() {
    let (catalog, file) = demo_catalog();
    // Bytes 6..34: from the rule through "% done" — three lines.
    let d = diagnostic(label(file, 6, 34, Some("this whole region")));
    check("multi-line-span", &human(&d, &catalog));
}

#[test]
fn cross_source_secondary() {
    let (mut catalog, file) = demo_catalog();
    let other = catalog
        .add("defs.lp".to_owned(), "r(1).\n".to_owned())
        .expect("small text admits");
    let d = diagnostic(label(file, 14, 18, None))
        .with_secondary(label(other, 0, 5, Some("defined here")));
    check("cross-source-secondary", &human(&d, &catalog));
}

#[test]
fn notes_and_helps() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, None))
        .with_note("expected because of this rule form".to_owned())
        .with_note("the statement began at line 2".to_owned())
        .with_help("add `.` at the end of the rule".to_owned());
    check("notes-and-helps", &human(&d, &catalog));
}

#[test]
fn unresolvable_source() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, None))
        .with_secondary(label(SourceId::new(9), 0, 3, Some("from here")));
    check("unresolvable-source", &human(&d, &catalog));
}

/// A catalog breaching the completeness law: name and text resolve,
/// the index does not.
struct MissingIndexCatalog {
    text: String,
}

impl themelios_base::source::Sources for MissingIndexCatalog {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("partial.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(
        &self,
        _: SourceId,
    ) -> Option<&themelios_base::line::LineIndex> {
        None
    }
}

#[test]
fn missing_facet() {
    let catalog = MissingIndexCatalog { text: "p(a).".to_owned() };
    let d = diagnostic(label(SourceId::new(0), 0, 5, None));
    check("missing-facet", &human(&d, &catalog));
}

#[test]
fn span_text_mismatch() {
    let (catalog, file) = demo_catalog();
    // 90..95 is past the text; 15..16 is coherent and still renders.
    let d = diagnostic(label(file, 90, 95, Some("phantom")))
        .with_secondary(label(file, 15, 16, Some("still renders")));
    check("span-text-mismatch", &human(&d, &catalog));
}

#[test]
fn embedded_snippet_frame() {
    // An embedded source: the host names it in its own terms, and
    // every coordinate is snippet-relative (base.md §3.3).
    let mut catalog = SourceSet::new();
    let snippet = catalog
        .add(
            "rule! at src/scheduler.rs:41".to_owned(),
            "on(T) :- task(T),\n  not off(T)\n".to_owned(),
        )
        .expect("small text admits");
    let d = diagnostic(label(snippet, 20, 31, Some("negated here")));
    check("embedded-snippet-frame", &human(&d, &catalog));
}
```

- [ ] **Step 6: Bless, then review the corpus**

Run: `mkdir -p crates/themelios-base/tests/golden && GOLDEN_BLESS=1 cargo test -p themelios-base --test golden && cargo test -p themelios-base --test golden`
Expected: first run writes nine files; second run passes against them.

Then **read all nine files** and review each rendering against the
committed layout mechanics above and the base.md §7.1 commitments:
headline form; window covers the labeled lines; one-based gutter;
carets on the primary; secondaries with their messages in position
order; notes then helps; each placeholder named, nothing dropped. The
corpus is a reviewed artifact — fix the renderer and re-bless until
the reviewer accepts, and record the acceptance in the commit message.
This corpus is the foundation the stage-2 diagnostics-quality witness
builds on.

- [ ] **Step 7: Append the totality law**

Append to `tests/properties.rs`:

```rust
mod human_totality {
    use proptest::prelude::*;
    use themelios_base::diagnostic::{
        Diagnostic, DiagnosticId, Label, Severity,
    };
    use themelios_base::source::{SourceId, SourceSet};
    use themelios_base::span::{ByteOffset, Location, Span};
    use themelios_base::view::human;

    const IDS: [DiagnosticId; 2] = [
        DiagnosticId::new("syntax", "unexpected-token"),
        DiagnosticId::new("program", "unknown-name"),
    ];

    fn labels() -> impl Strategy<Value = Label> {
        // Sources 0..6 over a three-entry catalog, spans up to byte
        // 40 over shorter texts: unresolvable ids, out-of-bounds and
        // mid-character spans included by construction (base.md §10).
        (0u32..6, 0u32..40, 0u32..8, proptest::option::of("[a-z]{0,6}"))
            .prop_map(|(source, start, extra, message)| Label {
                location: Location {
                    source: SourceId::new(source),
                    span: Span::new(
                        ByteOffset::new(start),
                        ByteOffset::new(start + extra),
                    )
                    .expect("ordered endpoints"),
                },
                message,
            })
    }

    proptest! {
        /// base.md §10: `view::human` total on arbitrary well-formed
        /// diagnostics over arbitrary catalogs.
        #[test]
        fn human_is_total(
            id_choice in 0usize..2,
            severity_choice in 0usize..3,
            message in "[a-z ]{1,20}",
            primary in labels(),
            secondaries in proptest::collection::vec(labels(), 0..4),
            notes in proptest::collection::vec("[a-z ]{0,12}", 0..3),
        ) {
            let mut catalog = SourceSet::new();
            for text in ["p(a).\nq(b).\n", "héllo\n🦀 line\n", ""] {
                catalog
                    .add("gen.lp".to_owned(), text.to_owned())
                    .expect("small text admits");
            }
            let severity = [
                Severity::Note,
                Severity::Warning,
                Severity::Error,
            ][severity_choice];
            let mut diagnostic = Diagnostic::new(
                IDS[id_choice],
                severity,
                message,
                primary,
            )
            .expect("generated headline is non-empty");
            for secondary in secondaries {
                diagnostic = diagnostic.with_secondary(secondary);
            }
            for note in notes {
                diagnostic = diagnostic.with_note(note);
            }
            let rendered = human(&diagnostic, &catalog);
            // Total and never silent: the headline always leads.
            prop_assert!(rendered.starts_with(&format!(
                "{}[{}]: ",
                severity,
                IDS[id_choice]
            )));
            prop_assert!(rendered.ends_with('\n'));
        }
    }
}
```

- [ ] **Step 8: Run everything and the gate**

Run: `cargo test -p themelios-base`
Expected: unit, golden, law-checker, and property tests all pass.
Then the full gate command from Task 1 Step 6. Expected: green.

- [ ] **Step 9: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "view: the human rendering, its degradation placeholders, and the reviewed golden seed corpus"
```

---

### Task 10: `view::editor`

**Files:**
- Modify: `crates/themelios-base/src/view.rs`

**Derives:** base.md §7.2 (the typed payload and its stated projection
argument, refusals with loci, folding, the headline fallback), §8.5
(`EditorRefusal` is a refusal type), §9 (cost).

**Interfaces:**
- Consumes: `Diagnostic` (Task 8), `Sources`/`SourceFacet` (Task 6),
  `LineIndex::position`/`LineCol`/`ColumnEncoding`/`PositionRefusal`
  (Tasks 4–5), `Location` (Task 3).
- Produces: `view::editor(&Diagnostic, &impl Sources, ColumnEncoding)
  -> Result<EditorDiagnostic, EditorRefusal>`; the payload types
  `EditorDiagnostic`, `EditorRange`, `EditorRelated`; `EditorRefusal`.

Committed semantics, from the design: the view refuses rather than
fabricate — a protocol payload with invented ranges is worse than no
payload. Resolution demands all three facets (the completeness law);
none → `UnknownSource`, partial → `IncompleteSource` naming the first
missing facet in `Name`, `Text`, `Index` order. Refusal order is the
primary first, then secondaries in position order. The message folds
the headline, then `note:` lines, then `help:` lines — exactly the
design's stated form, so the primary label's own message is not part
of this projection (it remains reachable on the model, the machine
view). A message-less secondary contributes the headline as its
related message — the location still ships. The `Note` severity's
mapping to the protocol's information class is the JSON layer's
documented step, not this crate's.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `src/view.rs`:

```rust
    use crate::line::{
        ColumnEncoding, LineCol, NotCharBoundary, PositionRefusal,
    };
    use crate::source::{SourceFacet, SourceId};

    fn editor_fixture() -> (SourceSet, crate::source::SourceId, Diagnostic) {
        let mut catalog = SourceSet::new();
        let file = catalog
            .add("demo.lp".to_owned(), "p(a).\nq(é) :- r(é)\n".to_owned())
            .expect("small text admits");
        // Line 2 starts at byte 6: q=6 (=7 é=8..10 )=10 ␣=11 :=12
        // -=13 ␣=14 r=15 (=16 é=17..19 )=19; "r(é)" is bytes 15..20.
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            Label {
                location: Location {
                    source: file,
                    span: Span::new(
                        ByteOffset::new(15),
                        ByteOffset::new(20),
                    )
                    .expect("ordered endpoints"),
                },
                message: Some("dropped by this projection".to_owned()),
            },
        )
        .expect("non-empty headline")
        .with_secondary(Label {
            location: Location {
                source: file,
                span: Span::new(ByteOffset::new(6), ByteOffset::new(10))
                    .expect("ordered endpoints"),
            },
            message: None,
        })
        .with_note("a note".to_owned())
        .with_help("a help".to_owned());
        (catalog, file, diagnostic)
    }

    #[test]
    fn the_payload_is_typed_self_describing_and_folded() {
        let (catalog, file, diagnostic) = editor_fixture();
        let payload =
            editor(&diagnostic, &catalog, ColumnEncoding::Utf16Units)
                .expect("a coherent catalog yields a payload");
        assert_eq!(payload.source, file);
        assert_eq!(payload.encoding, ColumnEncoding::Utf16Units);
        assert_eq!(payload.severity, Severity::Error);
        assert_eq!(payload.code, UNEXPECTED);
        // Bytes 15..20 on "q(é) :- r(é)": the é before the span is
        // one UTF-16 unit for two bytes, so columns are 8..12 in
        // UTF-16 units, zero-based.
        assert_eq!(
            payload.range,
            EditorRange {
                start: LineCol { line: 1, col: 8 },
                end: LineCol { line: 1, col: 12 },
            }
        );
        assert_eq!(
            payload.message,
            "expected `.` after the rule body\nnote: a note\nhelp: a help"
        );
        // The message-less secondary ships its location with the
        // headline as its message — nothing dropped silently.
        assert_eq!(payload.related.len(), 1);
        assert_eq!(
            payload.related[0].message,
            "expected `.` after the rule body"
        );
        assert_eq!(
            payload.related[0].range,
            EditorRange {
                start: LineCol { line: 1, col: 0 },
                end: LineCol { line: 1, col: 3 },
            }
        );
    }

    #[test]
    fn the_view_refuses_an_unknown_source() {
        let (_, _, diagnostic) = editor_fixture();
        let empty = SourceSet::new();
        assert_eq!(
            editor(&diagnostic, &empty, ColumnEncoding::Utf16Units),
            Err(EditorRefusal::UnknownSource { id: SourceId::new(0) })
        );
    }

    #[test]
    fn the_view_refuses_a_completeness_breach_naming_the_facet() {
        struct NameOnly;
        impl crate::source::Sources for NameOnly {
            fn name(&self, _: SourceId) -> Option<&str> {
                Some("partial.lp")
            }
            fn text(&self, _: SourceId) -> Option<&str> {
                None
            }
            fn line_index(
                &self,
                _: SourceId,
            ) -> Option<&crate::line::LineIndex> {
                None
            }
        }
        let (_, _, diagnostic) = editor_fixture();
        assert_eq!(
            editor(&diagnostic, &NameOnly, ColumnEncoding::Utf8Bytes),
            Err(EditorRefusal::IncompleteSource {
                id: SourceId::new(0),
                missing: SourceFacet::Text,
            })
        );
    }

    #[test]
    fn the_view_refuses_a_misfit_span_carrying_its_locus() {
        let (catalog, file, _) = editor_fixture();
        // Byte 9 splits the é on line 2 (line starts at 6: q=6, (=7,
        // é=8..10).
        let location = Location {
            source: file,
            span: Span::new(ByteOffset::new(9), ByteOffset::new(10))
                .expect("ordered endpoints"),
        };
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Warning,
            "w".to_owned(),
            Label { location, message: None },
        )
        .expect("non-empty headline");
        assert_eq!(
            editor(&diagnostic, &catalog, ColumnEncoding::CodePoints),
            Err(EditorRefusal::Position {
                at: location,
                refusal: PositionRefusal::NotCharBoundary(
                    NotCharBoundary { offset: ByteOffset::new(9) }
                ),
            })
        );
    }

    #[test]
    fn editor_refusals_display_the_fixable_question() {
        let refusal = EditorRefusal::IncompleteSource {
            id: SourceId::new(3),
            missing: SourceFacet::Index,
        };
        assert_eq!(
            refusal.to_string(),
            "source 3 resolved partially: missing index"
        );
        let _: &dyn std::error::Error = &refusal;
    }
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — the editor types are not defined.

- [ ] **Step 3: Implement the editor view**

Append to `src/view.rs` (extend its imports with
`use std::fmt;`, `use crate::diagnostic::{DiagnosticId, Severity};`,
`use crate::source::SourceFacet;`, `use crate::span::Location;`):

```rust
/// The editor-protocol payload as a typed value. Serialization is the
/// consumer's step: this crate ships shapes, not bytes (base.md §7.2).
///
/// A typed intermediate shape, deliberately: the honest alternative —
/// a view straight to protocol bytes — is foreclosed by the
/// zero-dependency constraint, and the dishonest one — consumers
/// walking `Diagnostic` per protocol — re-derives position conversion
/// and label linearization in every host. So the view stops at the
/// last typed point before bytes. What keeps this from becoming a
/// second model: exactly one pure function produces it, nothing in
/// this crate holds it, it mirrors the protocol's categories, and
/// nothing maintains it — every instance is a fresh derivation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorDiagnostic {
    /// The primary label's source.
    pub source: SourceId,
    /// The primary label's range.
    pub range: EditorRange,
    /// The encoding every range in this payload was derived under —
    /// the view records the derivation it ran, so the payload is
    /// self-describing when stored, compared, or forwarded.
    pub encoding: ColumnEncoding,
    /// The model's typed severity; the protocol mapping (`Note` to
    /// the information class) is the JSON layer's documented step.
    pub severity: Severity,
    /// Typed identity; the `namespace::name` string is the consumer's
    /// serialization step, via `Display`.
    pub code: DiagnosticId,
    /// The headline, then notes and helps folded as `note:` and
    /// `help:` lines — the protocol convention.
    pub message: String,
    /// The secondary labels, linearized in position order — a view
    /// linearizes (base.md §8.4).
    pub related: Vec<EditorRelated>,
}

/// A protocol range: two zero-based coordinates in the payload's
/// stated encoding (base.md §7.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EditorRange {
    /// Where the range begins.
    pub start: LineCol,
    /// Where the range ends, exclusive.
    pub end: LineCol,
}

/// One related location in the payload (base.md §7.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorRelated {
    /// The related label's source.
    pub source: SourceId,
    /// The related label's range.
    pub range: EditorRange,
    /// The label's message, or the diagnostic's headline when the
    /// label carries none — the location still ships.
    pub message: String,
}

/// Why `view::editor` refused: each refusal names its locus — the
/// question the caller can fix (base.md §7.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorRefusal {
    /// The catalog resolves nothing for this id.
    UnknownSource {
        /// The unresolved id.
        id: SourceId,
    },
    /// The catalog breached the completeness law: it resolved this id
    /// partially, and this is the facet the view lacked.
    IncompleteSource {
        /// The partially resolved id.
        id: SourceId,
        /// The first missing facet, in name, text, index order.
        missing: SourceFacet,
    },
    /// A position query failed; `at` is the label whose location was
    /// being converted, so the caller need not replay the iteration.
    Position {
        /// The label under conversion.
        at: Location,
        /// What the line index refused.
        refusal: PositionRefusal,
    },
}

impl fmt::Display for EditorRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorRefusal::UnknownSource { id } => write!(
                f,
                "source {} resolves nothing in the catalog",
                id.get()
            ),
            EditorRefusal::IncompleteSource { id, missing } => {
                let facet = match missing {
                    SourceFacet::Name => "name",
                    SourceFacet::Text => "text",
                    SourceFacet::Index => "index",
                };
                write!(
                    f,
                    "source {} resolved partially: missing {facet}",
                    id.get()
                )
            }
            EditorRefusal::Position { at, refusal } => write!(
                f,
                "a position query failed at source {} bytes {}..{}: {}",
                at.source.get(),
                at.span.start().get(),
                at.span.end().get(),
                refusal
            ),
        }
    }
}

impl std::error::Error for EditorRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EditorRefusal::Position { refusal, .. } => Some(refusal),
            _ => None,
        }
    }
}

/// The editor view: refuses rather than fabricate — a protocol
/// payload with invented ranges is worse than no payload. Refuses
/// `EditorRefusal`, the primary first, then secondaries in position
/// order; O(labels · log) via the resolved line index (base.md §7.2,
/// §9).
pub fn editor(
    diagnostic: &Diagnostic,
    sources: &impl Sources,
    encoding: ColumnEncoding,
) -> Result<EditorDiagnostic, EditorRefusal> {
    let primary = diagnostic.primary();
    let index = resolve_complete(sources, primary.location.source)?;
    let range = range_of(index, primary.location, encoding)?;
    let mut message = diagnostic.message().to_owned();
    for note in diagnostic.notes() {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    for help in diagnostic.helps() {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }
    let mut related = Vec::new();
    for label in diagnostic.secondary() {
        let index = resolve_complete(sources, label.location.source)?;
        let range = range_of(index, label.location, encoding)?;
        related.push(EditorRelated {
            source: label.location.source,
            range,
            message: label
                .message
                .clone()
                .unwrap_or_else(|| diagnostic.message().to_owned()),
        });
    }
    Ok(EditorDiagnostic {
        source: primary.location.source,
        range,
        encoding,
        severity: diagnostic.severity(),
        code: diagnostic.id(),
        message,
        related,
    })
}

/// Resolution under the completeness law: all three facets or a named
/// refusal.
fn resolve_complete(
    sources: &impl Sources,
    id: SourceId,
) -> Result<&LineIndex, EditorRefusal> {
    let name = sources.name(id);
    let text = sources.text(id);
    let index = sources.line_index(id);
    match (name, text, index) {
        (None, None, None) => Err(EditorRefusal::UnknownSource { id }),
        (Some(_), Some(_), Some(index)) => Ok(index),
        (name, text, _) => {
            let missing = if name.is_none() {
                SourceFacet::Name
            } else if text.is_none() {
                SourceFacet::Text
            } else {
                SourceFacet::Index
            };
            Err(EditorRefusal::IncompleteSource { id, missing })
        }
    }
}

/// Both span ends positioned under the stated encoding, the refusal
/// carrying the label's location as its locus.
fn range_of(
    index: &LineIndex,
    location: Location,
    encoding: ColumnEncoding,
) -> Result<EditorRange, EditorRefusal> {
    let position = |offset| {
        index.position(offset, encoding).map_err(|refusal| {
            EditorRefusal::Position { at: location, refusal }
        })
    };
    Ok(EditorRange {
        start: position(location.span.start())?,
        end: position(location.span.end())?,
    })
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `view::tests` pass, golden corpus untouched and green.

- [ ] **Step 5: Run the gate and commit**

Run: the full gate command from Task 1 Step 6. Expected: green.

```bash
git add crates/themelios-base/src
git commit -m "view: the typed editor payload, refusing with loci rather than fabricating"
```

---

### Task 11: `view::canonical_order`

**Files:**
- Modify: `crates/themelios-base/src/view.rs`,
  `crates/themelios-base/tests/properties.rs` (append)

**Derives:** base.md §7.4 (the order and the free-function argument),
§10 (the total-order law).

**Interfaces:**
- Consumes: `Diagnostic` (Task 8).
- Produces: `view::canonical_order(&Diagnostic, &Diagnostic) ->
  Ordering` — the golden corpus and every batch consumer sort with it.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `src/view.rs`:

```rust
    use std::cmp::Ordering;

    #[test]
    fn canonical_order_sorts_source_span_severity_identity() {
        let at = |source: u32, start: u32, severity, id| {
            Diagnostic::new(
                id,
                severity,
                "m".to_owned(),
                Label {
                    location: Location {
                        source: SourceId::new(source),
                        span: Span::new(
                            ByteOffset::new(start),
                            ByteOffset::new(start + 1),
                        )
                        .expect("ordered endpoints"),
                    },
                    message: None,
                },
            )
            .expect("non-empty headline")
        };
        const OTHER: DiagnosticId =
            DiagnosticId::new("program", "unknown-name");
        // Source groups first.
        assert_eq!(
            canonical_order(
                &at(0, 9, Severity::Note, UNEXPECTED),
                &at(1, 0, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
        // Then primary span, document order.
        assert_eq!(
            canonical_order(
                &at(0, 2, Severity::Note, UNEXPECTED),
                &at(0, 5, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
        // Then severity, worst first.
        assert_eq!(
            canonical_order(
                &at(0, 2, Severity::Error, UNEXPECTED),
                &at(0, 2, Severity::Warning, UNEXPECTED),
            ),
            Ordering::Less
        );
        // Then identity.
        assert_eq!(
            canonical_order(
                &at(0, 2, Severity::Error, OTHER),
                &at(0, 2, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
        // Equal exactly when structurally equal.
        assert_eq!(
            canonical_order(
                &at(0, 2, Severity::Error, UNEXPECTED),
                &at(0, 2, Severity::Error, UNEXPECTED),
            ),
            Ordering::Equal
        );
        // The final tiebreak is full structural comparison.
        let plain = at(0, 2, Severity::Error, UNEXPECTED);
        let with_note =
            at(0, 2, Severity::Error, UNEXPECTED).with_note("n".to_owned());
        assert_ne!(canonical_order(&plain, &with_note), Ordering::Equal);
    }
```

- [ ] **Step 2: Run to verify the failing state**

Run: `cargo test -p themelios-base`
Expected: compile error — `canonical_order` is not defined.

- [ ] **Step 3: Implement the order**

Append to `src/view.rs` (add `use std::cmp::Ordering;` to its
imports):

```rust
/// One deterministic order for batches: source, then primary span,
/// then severity (worst first), then identity, then full structural
/// comparison as the final tiebreak (base.md §7.4).
///
/// Defined once because every batch consumer needs one — the golden
/// corpus first among them — and each inventing its own would
/// diverge. A free function deliberately, not an `Ord` impl: an `Ord`
/// impl would claim *the* natural order of diagnostics, and there is
/// none — this is one batch derivation among possible ones, and
/// ordering a batch for consumption is itself a linearization
/// (base.md §8.4). Total; `Equal` exactly on structural equality;
/// O(structure).
pub fn canonical_order(a: &Diagnostic, b: &Diagnostic) -> Ordering {
    (a.primary().location.source)
        .cmp(&b.primary().location.source)
        .then_with(|| {
            a.primary().location.span.cmp(&b.primary().location.span)
        })
        .then_with(|| b.severity().cmp(&a.severity()))
        .then_with(|| a.id().cmp(&b.id()))
        .then_with(|| a.message().cmp(b.message()))
        .then_with(|| a.primary().message.cmp(&b.primary().message))
        .then_with(|| a.secondary().iter().cmp(b.secondary().iter()))
        .then_with(|| a.notes().cmp(b.notes()))
        .then_with(|| a.helps().cmp(b.helps()))
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test -p themelios-base`
Expected: all `view::tests` pass.

- [ ] **Step 5: Append the total-order law**

Append to `tests/properties.rs`:

```rust
mod canonical_order_law {
    use std::cmp::Ordering;

    use proptest::prelude::*;
    use themelios_base::diagnostic::{
        Diagnostic, DiagnosticId, Label, Severity,
    };
    use themelios_base::source::SourceId;
    use themelios_base::span::{ByteOffset, Location, Span};
    use themelios_base::view::canonical_order;

    fn diagnostics() -> impl Strategy<Value = Diagnostic> {
        // A deliberately collision-heavy space, so every comparison
        // key — including the structural tiebreak — is exercised.
        (
            0u32..2,
            0u32..3,
            0usize..3,
            0usize..2,
            "[ab]{1,2}",
            proptest::collection::vec("[ab]{0,2}", 0..2),
        )
            .prop_map(
                |(source, start, severity, id, message, notes)| {
                    let severity = [
                        Severity::Note,
                        Severity::Warning,
                        Severity::Error,
                    ][severity];
                    let id = [
                        DiagnosticId::new("syntax", "unexpected-token"),
                        DiagnosticId::new("program", "unknown-name"),
                    ][id];
                    let mut diagnostic = Diagnostic::new(
                        id,
                        severity,
                        message,
                        Label {
                            location: Location {
                                source: SourceId::new(source),
                                span: Span::new(
                                    ByteOffset::new(start),
                                    ByteOffset::new(start + 1),
                                )
                                .expect("ordered endpoints"),
                            },
                            message: None,
                        },
                    )
                    .expect("generated headline is non-empty");
                    for note in notes {
                        diagnostic = diagnostic.with_note(note);
                    }
                    diagnostic
                },
            )
    }

    proptest! {
        /// base.md §10: canonical_order is a total order —
        /// antisymmetric, transitive, total — and Equal exactly on
        /// structural equality.
        #[test]
        fn canonical_order_is_a_total_order(
            a in diagnostics(),
            b in diagnostics(),
            c in diagnostics(),
        ) {
            prop_assert_eq!(
                canonical_order(&a, &b),
                canonical_order(&b, &a).reverse()
            );
            if canonical_order(&a, &b) != Ordering::Greater
                && canonical_order(&b, &c) != Ordering::Greater
            {
                prop_assert_ne!(
                    canonical_order(&a, &c),
                    Ordering::Greater
                );
            }
            prop_assert_eq!(
                canonical_order(&a, &b) == Ordering::Equal,
                a == b
            );
            prop_assert_eq!(
                canonical_order(&a, &a),
                Ordering::Equal
            );
        }
    }
}
```

- [ ] **Step 6: Run the property laws and the gate**

Run: `cargo test -p themelios-base --test properties`
Expected: all pass, the new law included.
Then the full gate command from Task 1 Step 6. Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/themelios-base/src crates/themelios-base/tests
git commit -m "view: canonical_order, one batch order held to the total-order law"
```

---

### Task 12: Scaling shapes

**Files:**
- Create: `crates/themelios-base/benches/scaling.rs`,
  `crates/themelios-base/tests/scaling_shape.rs`
- Modify: `crates/themelios-base/Cargo.toml` (declare the bench)

**Derives:** base.md §10 (scaling shapes: `LineIndex::of` linear in
text, `position`/`offset` logarithmic, `human` linear in rendered
output; shape assertions in CI, absolute numbers out-of-band); spec
§10.1–§10.2 (criterion standing from the tier's landing; in-gate
versus out-of-band).

**Interfaces:**
- Consumes: `LineIndex`, `Source`, `Diagnostic`, `view::human` from
  earlier tasks.
- Produces: instruments only — no public surface.

The split, stated: the **shape assertions** run in CI as an ordinary
test using median-of-five wall-clock ratios with tolerances generous
enough to be machine-independent — they catch the wrong complexity
class (quadratic where linear is claimed, linear where logarithmic
is), nothing finer. The **absolute numbers** live in the criterion
benches, run out-of-band on the milestone cadence, never in the
per-change gate. Every instrument is documented with what it proves
and what it cannot.

- [ ] **Step 1: Declare the bench target**

Append to `crates/themelios-base/Cargo.toml`:

```toml
[[bench]]
name = "scaling"
harness = false
```

- [ ] **Step 2: Write the criterion benches**

Create `crates/themelios-base/benches/scaling.rs`:

```rust
//! Out-of-band absolute numbers behind the shape claims
//! (docs/design/base.md §10; docs/specification.md §10.2). Run per
//! milestone: `cargo bench -p themelios-base`. These prove wall-clock
//! magnitudes on one machine; they cannot prove complexity class —
//! the CI shape test holds that.

use criterion::{criterion_group, criterion_main, Criterion};
use themelios_base::diagnostic::{
    Diagnostic, DiagnosticId, Label, Severity,
};
use themelios_base::line::{ColumnEncoding, LineIndex};
use themelios_base::source::{Source, SourceId, SourceSet};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_base::view::human;

/// One repeated line with multi-byte content, so every size exercises
/// the wide-character tables.
const LINE: &str = "p(a). % é🦀 comment\n";

fn text_of(bytes: usize) -> String {
    LINE.repeat(bytes / LINE.len() + 1)
}

fn admitted(bytes: usize) -> Source {
    Source::new(SourceId::new(0), text_of(bytes))
        .expect("bench text admits")
}

fn line_index_of(c: &mut Criterion) {
    for kib in [256usize, 1024, 4096] {
        let source = admitted(kib * 1024);
        c.bench_function(&format!("LineIndex::of/{kib}KiB"), |b| {
            b.iter(|| LineIndex::of(std::hint::black_box(&source)));
        });
    }
}

fn position_queries(c: &mut Criterion) {
    for kib in [64usize, 4096] {
        let source = admitted(kib * 1024);
        let index = LineIndex::of(&source);
        let len = source.end().get() as usize;
        // Boundary offsets: multiples of the repeated line's length.
        let offsets: Vec<ByteOffset> = (0..4096usize)
            .map(|i| (i * (len / 4096)) / LINE.len() * LINE.len())
            .map(|raw| ByteOffset::new(raw as u32))
            .collect();
        c.bench_function(&format!("position/4096-queries/{kib}KiB"), |b| {
            b.iter(|| {
                for &offset in &offsets {
                    let _ = std::hint::black_box(index.position(
                        offset,
                        ColumnEncoding::Utf16Units,
                    ));
                }
            });
        });
    }
}

fn human_rendering(c: &mut Criterion) {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add("bench.lp".to_owned(), text_of(512 * LINE.len()))
        .expect("bench text admits");
    for labels in [16u32, 256] {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::new("syntax", "unexpected-token"),
            Severity::Error,
            "bench".to_owned(),
            label_on_line(file, 0),
        )
        .expect("non-empty headline");
        for line in 1..labels {
            diagnostic =
                diagnostic.with_secondary(label_on_line(file, line));
        }
        c.bench_function(&format!("human/{labels}-labels"), |b| {
            b.iter(|| human(std::hint::black_box(&diagnostic), &catalog));
        });
    }
}

fn label_on_line(source: SourceId, line: u32) -> Label {
    let start = line * LINE.len() as u32;
    Label {
        location: Location {
            source,
            span: Span::new(
                ByteOffset::new(start),
                ByteOffset::new(start + 4),
            )
            .expect("ordered endpoints"),
        },
        message: Some("here".to_owned()),
    }
}

criterion_group!(scaling, line_index_of, position_queries, human_rendering);
criterion_main!(scaling);
```

- [ ] **Step 3: Verify the benches build and run once**

Run: `cargo bench -p themelios-base -- --test`
Expected: each bench executes once and passes (criterion's test mode —
no measurement, proves the harness).

- [ ] **Step 4: Write the CI shape assertions**

Create `crates/themelios-base/tests/scaling_shape.rs`:

```rust
//! CI shape assertions (docs/design/base.md §10): complexity shape
//! only, held by median-of-five wall-clock ratios with tolerances
//! wide enough for any CI machine. What they prove: the claimed
//! class (a quadratic `of`, a linear-scan `position`, a
//! labels-squared `human` all fail loudly). What they cannot prove:
//! absolute speed — that lives in the out-of-band benches.

use std::time::Instant;

use themelios_base::diagnostic::{
    Diagnostic, DiagnosticId, Label, Severity,
};
use themelios_base::line::{ColumnEncoding, LineIndex};
use themelios_base::source::{Source, SourceId, SourceSet};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_base::view::human;

const LINE: &str = "p(a). % é🦀 comment\n";

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: u128 = 16;
/// A linear claim at SIZE_RATIO may cost at most this factor —
/// eightfold noise headroom over linear; quadratic (x256) fails.
const LINEAR_CEILING: u128 = SIZE_RATIO * 8;
/// A logarithmic claim across a 64x data ratio may cost at most this
/// factor — logarithmic is ~1.4x; linear (x64) fails.
const LOG_CEILING: u128 = 8;

fn text_of(bytes: usize) -> String {
    LINE.repeat(bytes / LINE.len() + 1)
}

fn admitted(bytes: usize) -> Source {
    Source::new(SourceId::new(0), text_of(bytes))
        .expect("test text admits")
}

fn median_nanos(mut work: impl FnMut()) -> u128 {
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[2].max(1)
}

#[test]
fn line_index_construction_is_linear_in_the_text() {
    let small_source = admitted(256 * 1024);
    let big_source = admitted(256 * 1024 * SIZE_RATIO as usize);
    let small = median_nanos(|| {
        std::hint::black_box(LineIndex::of(&small_source));
    });
    let big = median_nanos(|| {
        std::hint::black_box(LineIndex::of(&big_source));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "LineIndex::of scaled {small}ns -> {big}ns over x{SIZE_RATIO} \
         data; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn position_and_offset_are_logarithmic() {
    let queries = 4096usize;
    let run = |bytes: usize| {
        let source = admitted(bytes);
        let index = LineIndex::of(&source);
        let len = source.end().get() as usize;
        let offsets: Vec<ByteOffset> = (0..queries)
            .map(|i| (i * (len / queries)) / LINE.len() * LINE.len())
            .map(|raw| ByteOffset::new(raw as u32))
            .collect();
        median_nanos(move || {
            for &offset in &offsets {
                let position = index
                    .position(offset, ColumnEncoding::Utf16Units)
                    .expect("boundary offsets position");
                std::hint::black_box(
                    index
                        .offset(position, ColumnEncoding::Utf16Units)
                        .expect("round trip"),
                );
            }
        })
    };
    // 64x the indexed text, the same number of queries.
    let small = run(64 * 1024);
    let big = run(4096 * 1024);
    assert!(
        big < small * LOG_CEILING,
        "position/offset scaled {small}ns -> {big}ns over x64 data; \
         the logarithmic shape allows at most x{LOG_CEILING}"
    );
}

#[test]
fn human_is_linear_in_rendered_output() {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add("shape.lp".to_owned(), text_of(512 * LINE.len()))
        .expect("test text admits");
    let label_on_line = |line: u32| Label {
        location: Location {
            source: file,
            span: Span::new(
                ByteOffset::new(line * LINE.len() as u32),
                ByteOffset::new(line * LINE.len() as u32 + 4),
            )
            .expect("ordered endpoints"),
        },
        message: Some("here".to_owned()),
    };
    let build = |labels: u32| {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::new("syntax", "unexpected-token"),
            Severity::Error,
            "shape".to_owned(),
            label_on_line(0),
        )
        .expect("non-empty headline");
        for line in 1..labels {
            diagnostic = diagnostic.with_secondary(label_on_line(line));
        }
        diagnostic
    };
    let small_diagnostic = build(16);
    let big_diagnostic = build(16 * SIZE_RATIO as u32);
    let small = median_nanos(|| {
        std::hint::black_box(human(&small_diagnostic, &catalog));
    });
    let big = median_nanos(|| {
        std::hint::black_box(human(&big_diagnostic, &catalog));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "human scaled {small}ns -> {big}ns over x{SIZE_RATIO} output; \
         the linear shape allows at most x{LINEAR_CEILING}"
    );
}
```

- [ ] **Step 5: Run the shape assertions and the gate**

Run: `cargo test -p themelios-base --test scaling_shape`
Expected: 3 passed (a few seconds — sizes are tuned so the suite
stays cheap enough for the per-change gate).
Then the full gate command from Task 1 Step 6. Expected: green —
clippy compiles the bench target too, so the gate holds it without
running it.

- [ ] **Step 6: Commit**

```bash
git add crates/themelios-base
git commit -m "instruments: criterion benches out-of-band, complexity-shape assertions in the gate"
```

---

### Task 13: Stage close — documentation, coverage floor, mutation, the failure walk

**Files:**
- Modify: `crates/themelios-base/src/lib.rs` (the worked example),
  `.github/workflows/gate.yml` (the coverage job)
- Create: `.cargo/mutants.toml` (accepted survivors, argued)

**Derives:** base.md §2 (the failure conditions, walked), §9 (the
table cross-checked against signatures), §10 (standing gates:
mutation per milestone, the coverage floor, documentation examples
that run, executable claims); spec §10.1–§10.2, §10.4.

**Interfaces:**
- Consumes: everything.
- Produces: the stage's standing gates, armed; no public surface.

- [ ] **Step 1: Write the crate-level worked example**

Extend the crate docs in `src/lib.rs` (below the existing paragraph),
so the front page carries a running proof of the crate's one central
claim — the three views from the same value, refusals composing as
errors:

```rust
//! # A worked example
//!
//! ```
//! use themelios_base::diagnostic::{
//!     Diagnostic, DiagnosticId, Label, Severity,
//! };
//! use themelios_base::source::SourceSet;
//! use themelios_base::span::{ByteOffset, Location, Span};
//! use themelios_base::view;
//!
//! let mut catalog = SourceSet::new();
//! let file = catalog.add("demo.lp".into(), "q(X) :- r(X)\n".into())?;
//!
//! const UNEXPECTED: DiagnosticId =
//!     DiagnosticId::new("syntax", "unexpected-token");
//! let diagnostic = Diagnostic::new(
//!     UNEXPECTED,
//!     Severity::Error,
//!     "expected `.` after the rule body".into(),
//!     Label {
//!         location: Location {
//!             source: file,
//!             span: Span::new(ByteOffset::new(8), ByteOffset::new(12))?,
//!         },
//!         message: None,
//!     },
//! )?;
//!
//! // The same value yields every view.
//! let rendered = view::human(&diagnostic, &catalog);
//! assert!(rendered.starts_with("error[syntax::unexpected-token]:"));
//! let payload = view::editor(
//!     &diagnostic,
//!     &catalog,
//!     themelios_base::line::ColumnEncoding::Utf16Units,
//! )?;
//! assert_eq!(payload.code, UNEXPECTED);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
```

Run: `cargo test -p themelios-base --doc`
Expected: the example runs and passes — refusals `?`-compose through
`Box<dyn Error>`, which is the §8.5 posture demonstrated, not merely
claimed.

- [ ] **Step 2: Set the coverage floor**

Measure: `cargo llvm-cov -p themelios-base --summary-only`
(install once with `cargo install cargo-llvm-cov` if absent).

Compute the floor: measured line coverage, rounded down to a multiple
of five, minus five. With this plan's suite the measurement is
expected to land at or above 90, making the floor **85** — if the
measurement computes a different floor, write that value instead and
record the measurement in the commit message.

Append to `.github/workflows/gate.yml`:

```yaml
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: |
          rustup toolchain install 1.97.1 --profile minimal \
            --component llvm-tools
          rustup default 1.97.1
      - uses: taiki-e/install-action@cargo-llvm-cov
      # The floor is a per-change tripwire for the wholly untested
      # module, never a target: raising it is not a goal, gaming it
      # is a defect, and mutation remains the auditor of whether the
      # tests ask anything (docs/specification.md §10.1). Set at the
      # stage-1 landing: measured coverage rounded down to a multiple
      # of five, minus five. One number on every machine.
      - run: cargo llvm-cov --workspace --fail-under-lines 85
```

Coverage runs on the Linux lane only — the metric is
platform-independent, and the platform split stays explicit (spec
§10.1).

Run locally: `cargo llvm-cov --workspace --fail-under-lines 85`
Expected: green.

- [ ] **Step 3: Run the mutation milestone audit**

Run: `cargo mutants --package themelios-base`
(install once with `cargo install cargo-mutants` if absent; this is
the per-milestone out-of-band audit — spec §10.2 — not a gate step).

Triage every survivor, one by one:
- a genuine test gap → write the killing test in the survivor's
  module, re-run, confirm the kill;
- an arm no test can reach → record it in `.cargo/mutants.toml`,
  copying the survivor's printed description into the pattern, with
  the argument as a comment.

Create `.cargo/mutants.toml` with the one pre-declared acceptance
(Task 3 named it when the arm was written):

```toml
# Accepted mutation survivors, each carrying its argument. This list
# is a reviewed artifact: an entry without an argument is a defect.
#
# The TooLarge admission arms (docs/design/base.md §3.2) need a text
# over four gibibytes to exercise, which no test allocation will do;
# each is held by inspection — one comparison against the named
# ceiling Source::MAX_LEN.
exclude_re = [
    "source.rs.*TooLarge",
]
```

Tighten the pattern to the survivors cargo-mutants actually prints;
a pattern broader than its argument is a defect.

Expected end state: `cargo mutants --package themelios-base` reports
every mutant caught, unviable, or excluded-with-argument.

- [ ] **Step 4: Walk the failure conditions**

base.md §2 names the design's failure conditions. Verify each is held,
and record the walk in the commit message of the closing commit:

- *A diagnostic lacks a precise span or stable identity* — 
  unrepresentable: `Diagnostic::new` requires `DiagnosticId` and a
  primary `Label` (Task 8 tests).
- *A consumer must parse rendered prose* — every public result is
  typed; `human` is the only prose producer and is a view. Verify no
  other public operation returns a rendered `String`:
  `rg -n "pub fn" crates/themelios-base/src` and inspect signatures.
- *A panic escapes, or failure semantics undocumented* — the totality
  and refusal laws (Tasks 3, 5, 9, 11); `missing_docs` denied with
  every operation's rustdoc naming its refusal type and cost.
- *A result depends on non-inputs, or hidden mutation* — no ambient
  state in the crate. Verify mechanically:
  `rg -n "static|RefCell|Cell<|Mutex|thread_local|std::fs|std::net|std::env" crates/themelios-base/src`
  Expected: no hits (`tests/` may read the bless switch; `src/` may
  not).
- *A dependency in the shipped closure, unsafe code, or ASP knowledge
  in this crate* — `tests/trust.rs`; plus
  `rg -n -i "clingo|answer set|grounding" crates/themelios-base/src`
  Expected: no hits.
- *The syntax tier cannot express spec §6.6's demands* — namespaced
  identities (`DiagnosticId`), primary and secondary labeled spans
  (Task 8), expected-set reporting as tier-typed diagnostics lowering
  through `ToDiagnostic` (Task 8); both views carry them (Tasks
  9–10).
- *The macro tier's law is inexpressible* — a diagnostic is plain
  data with byte-precise snippet-relative spans and typed identity;
  re-targeting is re-basing `Location` values, no prose parsed
  (public fields, Task 3; the embedded-snippet golden, Task 9).
- *Line/column arithmetic misplaces a position on multi-byte text* —
  the oracle-agreement and round-trip laws plus the refusal tests
  (Task 5).
- *The depth-gate obligation* (base.md §10's last bullet) — attaches
  vacuously and is discharged by inspection: flat data throughout, no
  type in the five modules contains its own type directly or through
  a container, and every walk is a loop over a `Vec` or `BTreeSet`.
  Record the inspection in the closing commit message.

Then cross-check base.md §9 row by row against the code: each row's
refusal column is exactly the operation's error type, and every
public operation not in the table is total. Expected: exact
agreement; any divergence is a defect in the code, not the table.

- [ ] **Step 5: Final gate, clean tree**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: green, from a clean working tree.

- [ ] **Step 6: Commit**

```bash
git add crates/themelios-base/src .github/workflows/gate.yml .cargo/mutants.toml
git commit -m "Stage close: worked example, coverage floor armed, mutation audit clean, failure walk recorded"
```

---

## Completion

This plan is done when every checkbox above is checked and, from a
clean tree in one sitting:

- [ ] the full gate is green (fmt, clippy as errors, tests including
  doctests and golden corpus, doc build with warnings denied);
- [ ] `cargo llvm-cov --workspace --fail-under-lines <the committed
  floor>` passes;
- [ ] `cargo mutants --package themelios-base` reports every mutant
  caught, unviable, or excluded with a written argument;
- [ ] `cargo bench -p themelios-base -- --test` runs the harness;
- [ ] the golden corpus has been read and accepted by the reviewer,
  and the acceptance is recorded in a commit message;
- [ ] the Task 13 failure walk found every base.md §2 condition held;
- [ ] nothing from base.md §11 (reserved seams, non-goals) exists in
  the tree.

**Derivation coverage, for the reviewer:** base.md §3.1–§3.3 → Task 3;
§3.4 → Task 6; §4 → Tasks 2–3; §5 → Tasks 4–5; §6.1–§6.3 → Task 7;
§6.4–§6.5 → Task 8; §7.1 → Task 9; §7.2–§7.3 → Task 10; §7.4 → Task
11; §8 → the idiom of every task, §8.5 impls landing with their types;
§9 → per-operation rustdoc plus the Task 13 cross-check; §10 → the
property laws in Tasks 2, 3, 5, 9, 11, the law checker in Task 6, the
golden corpus in Task 9, the scaling shapes in Task 12, the standing
gates in Tasks 1 and 13; §11 → nothing, verified at close.












