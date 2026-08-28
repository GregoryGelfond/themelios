//! The owned-plain-data assurance (docs/design/analysis.md §1, §8; program §1, §14): every
//! public value type this crate hands out is `Send + Sync + 'static` and holds no borrow, so
//! an analysis is computed on one thread and read on another, kept and compared, without a
//! lifetime — as a `Program` is. The proof is **structural, not by inspection**: the generic
//! `assert_owned_plain_data::<T>()` compiles only when `T: Send + Sync + 'static`, and this
//! file instantiates it over the whole surface this crate defines, so a facet that grew a
//! borrow or a non-`Send` field would fail to compile here rather than pass a reviewer's eye.
//!
//! Coverage is the whole surface this crate *defines* — the assembled `Analysis` and its four
//! facets (`Constructs`, `DependencyGraph`, `Safety`, `Classes`) with their component, verdict,
//! and witness types. The program types this crate re-exports (`DependencyKind`, `Rule`,
//! `Signature`) are the program tier's to prove, and are asserted in that crate's twin of this
//! file (program §14), so they are not re-asserted here.

use themelios_analysis::analysis::Analysis;
use themelios_analysis::classify::{
    Classes, HornKind, Normality, ProgramClass, Stratification, Verdict,
};
use themelios_analysis::construct::{Construct, Constructs};
use themelios_analysis::depend::{Component, DependencyGraph};
use themelios_analysis::safe::{Safety, UnsafeRule};

/// Compiles only for an owned, thread-safe, borrow-free type. Instantiating it *is* the
/// assertion (§8): the bound is checked at monomorphization, so the whole file compiling is
/// the proof for every type below.
fn assert_owned_plain_data<T: Send + Sync + 'static>() {}

#[test]
fn every_public_value_type_is_owned_plain_data() {
    // The assembled report (§3).
    assert_owned_plain_data::<Analysis>();

    // The construct scan (§7).
    assert_owned_plain_data::<Constructs>();
    assert_owned_plain_data::<Construct>();

    // The dependency graph and its strongly-connected components (§4).
    assert_owned_plain_data::<DependencyGraph>();
    assert_owned_plain_data::<Component>();

    // Safety and finiteness, and the witness a flagged rule carries (§5).
    assert_owned_plain_data::<Safety>();
    assert_owned_plain_data::<UnsafeRule>();

    // The program classes and their verdicts (§6).
    assert_owned_plain_data::<Classes>();
    assert_owned_plain_data::<Verdict>();
    assert_owned_plain_data::<Stratification>();
    assert_owned_plain_data::<Normality>();
    assert_owned_plain_data::<HornKind>();
    assert_owned_plain_data::<ProgramClass>();
}
