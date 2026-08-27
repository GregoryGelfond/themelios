//! The analysis tier: the engine-free reading of a `themelios_program::Program`
//! that reports what is *structurally* true of it — which constructs it uses,
//! how its predicates depend on one another, whether its rules are safe and
//! ground finitely, and which classes of the literature it falls in: tight,
//! stratified, head-cycle-free, normal, Horn, disjunctive, choice. It reports
//! *facts*, never *policy*: which class earns which solving algorithm, which
//! estimate trips which threshold, whether a warning is worth emitting is the
//! consuming system's to decide — a view over these facts, not a reading of them
//! (docs/design/analysis.md §1).
//!
//! The reading is organized as five modules: `construct` (the construct scan,
//! §7); `depend` (the predicate dependency graph and its strongly-connected
//! components, §4); `safe` (safety and finiteness, §5); `classify` (the program
//! classes, §6); and `analysis` (the assembled `Analysis` value, §3).
//!
//! Design of record: `docs/design/analysis.md`; the value it reads:
//! `docs/design/program.md`; the tiers beneath it: `docs/design/base.md`,
//! `docs/design/syntax.md`. This crate depends on the program tier and nothing
//! else — the strongly-connected-components decomposition is hand-rolled and
//! iterative (§4), not a graph-library dependency (docs/specification.md §12.5,
//! §5.2) — and it emits **no diagnostics** of its own; it reports facts. Every
//! value it produces is owned plain data — `Send + Sync + 'static`, holding no
//! borrow — so an analysis is computed on one thread and read on another, kept
//! and compared, without a lifetime; and every operation is a pure, total
//! function of the program's content: no global state, no I/O, and no panic on
//! any input, including a program recovered from a malformed parse (§8;
//! docs/specification.md §2 item 8). No walk recurses on the call stack (§4;
//! docs/design/program.md §13).
#![forbid(unsafe_code)]

pub mod construct;
pub mod depend;
pub mod safe;
pub mod classify;
