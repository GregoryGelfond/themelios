//! The analysis-tier differential against the pinned authority (docs/design/analysis.md
//! §5, §10; docs/grammar.md §3): the **safety divergence**. Safety follows the ASP-Core-2
//! standard's binding condition (§5, the authority the standard, not the engine); whether
//! clingo's grounder admits a different notion is a differential obligation against the
//! pinned binary, recorded here as the grammar records its own (grammar §11) — since the
//! standard is the authority, a divergence is a characterized boundary, and a material one
//! is the trigger for the dialect parameterization §5 reserves.
//!
//! Feature-gated and out of band: run through pixi, `pixi run differential-analysis`. It
//! reaches the shared clingo driver in the program tier's tests, as that tier's round-trip
//! law reaches the syntax tier's vendored corpus. What it proves: this tier's safety verdict
//! agrees with the authority's grounder on the corpus, or the divergence is one of the
//! recorded, characterized boundaries. What it cannot (spec §10.2): agreement beyond the
//! corpus — the universal law is the naive-reference proptest (safe_laws). The
//! **classification** differential (the predicate-level approximations against ground-level
//! truth, and the classes against a literature-tagged corpus) needs a ground dependency
//! graph, hence a grounder, and is the solve stage's — deferred, but named below so it is
//! not silently dropped (§10, §11).

#![cfg(feature = "differential")]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

use themelios_analysis::safe::Safety;
use themelios_base::source::{Source, SourceId};
use themelios_program::raise::raise;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// The pin (docs/grammar.md §3).
const AUTHORITY_VERSION: &str = "5.8.2";

/// The shared clingo driver lives in the program tier's tests (its `differential.rs` sibling); the
/// analysis differential reaches it by path, as the program tier's round-trip law reaches
/// the syntax tier's vendored corpus. One driver, both tiers.
fn authority_py() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../themelios-program/tests/differential/authority.py")
}

/// Spawn the authority helper in `safety` mode on `program` and wait for its full output.
fn run_authority(program: &str) -> Output {
    let mut child = Command::new("python")
        .arg(authority_py())
        .arg("safety")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python runs: run this harness through `pixi run differential-analysis`");
    let _ = child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(program.as_bytes());
    child.wait_with_output().expect("the authority answers")
}

/// The authority's safety verdict: whether it grounds the program without reporting an
/// unsafe variable (§5, §10). The grounder stops on an unsafe variable and reports it on
/// the diagnostic logger, so "safe" is the absence of that report.
fn authority_safe(program: &str) -> bool {
    let output = run_authority(program);
    assert!(
        output.status.success(),
        "the authority helper failed (is clingo's Python module installed? run through pixi):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("the helper emits JSON");
    assert_eq!(
        value["version"].as_str(),
        Some(AUTHORITY_VERSION),
        "docs/grammar.md §3: the authority is pinned at v{AUTHORITY_VERSION}"
    );
    value["safe"].as_bool().expect("safe is a bool")
}

/// This tier's safety verdict for a program of concrete syntax (§5). A theory rule raises
/// with an up-to-grounding diagnostic (program §4.9) yet still yields a program to read;
/// the verdict is taken on that program, so a theory case is analyzable here.
fn our_safe(text: &str) -> bool {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    Safety::of(lowered.program()).is_safe()
}

/// A corpus of single rules spanning the ASP-Core-2 binding cases (§5): a global variable
/// bound by a positive body literal or an assignment, a local variable bound within its
/// element, and the unsafe shapes — plus the constructions where this tier's *syntactic*
/// safety and the authority's *grounder* safety may part.
const SAFETY_CORPUS: &[(&str, &str)] = &[
    // Safe to both: every variable has a binding occurrence.
    ("positive-body-binds", "p(X) :- q(X).\n"),
    ("constraint-body-binds", ":- p(X).\n"),
    ("assignment-binds", "p(X) :- X = 1.\n"),
    ("assignment-chain-binds", "p(X) :- X = Y, q(Y).\n"),
    ("arithmetic-assignment-binds", "p(X) :- X = Y + 1, q(Y).\n"),
    ("negation-after-positive", "p(X) :- q(X), not r(X).\n"),
    ("two-variables-bound", "p(X, Y) :- q(X), r(Y).\n"),
    ("aggregate-local-binds", "q :- #count { X : p(X) } >= 1.\n"),
    (
        "aggregate-assignment-guard",
        "q(N) :- N = #count { X : p(X) }.\n",
    ),
    ("condition-binds-head", "p(X) : q(X) :- r.\n"),
    // Unsafe to both: a variable with no binding occurrence.
    ("bare-fact-variable", "p(X).\n"),
    ("head-variable-unbound", "p(X) :- q(Y).\n"),
    ("comparison-does-not-bind", "p(X) :- X > 1.\n"),
    ("negation-does-not-bind", "p(X) :- not q(X).\n"),
    ("assignment-both-unbound", "p(X) :- X = Y.\n"),
    ("constraint-negation-only", ":- not p(X).\n"),
    (
        "aggregate-guard-unbound",
        "q :- #count { Y : p(Y) } >= X.\n",
    ),
    // The constructions where the two notions may part — the recorded boundaries.
    ("head-aggregate-bare", "#count { X : p(X) } :- q.\n"),
    ("interval-binds", "p(X) :- X = 1..3.\n"),
];

/// The recorded divergences (§5, §10): the labels where this tier's syntactic safety and
/// the authority's grounder safety part, each a *characterized* boundary with its reason —
/// never an unexplained mismatch. A material divergence here is the trigger for the dialect
/// parameterization §5 reserves. Calibrated against the pinned binary; a change in either
/// verdict fails the check until the boundary is re-read.
const RECORDED_DIVERGENCES: &[(&str, &str)] = &[
    // clingo binds a variable by aggregate-result assignment (`N = #count{...}`), a
    // permissive extension of the strict ASP-Core-2 standard, which binds an aggregate's
    // guard variables only elsewhere. This tier follows the standard (§5) and reports
    // unsafe — the sound direction, never a false safe; a candidate for the dialect
    // parameterization §5 reserves.
    (
        "aggregate-assignment-guard",
        "clingo's aggregate-result assignment binds the guard variable; the strict standard does not",
    ),
    // clingo binds a head conditional literal's variable through its own condition
    // (`p(X) : q(X)`). This tier requires a head literal's variables globally and keeps a
    // condition's binding local to its element, so it reports unsafe — again the sound
    // direction, the conservative reading of a head conditional.
    (
        "condition-binds-head",
        "clingo binds a head literal's variable through its condition; this tier keeps that binding local",
    ),
];

#[test]
fn our_safety_agrees_with_the_authority_or_the_divergence_is_recorded() {
    let recorded: BTreeMap<&str, &str> = RECORDED_DIVERGENCES.iter().copied().collect();
    let mut diverged: Vec<&str> = Vec::new();
    let mut unrecorded: Vec<String> = Vec::new();
    let mut stale: Vec<&str> = Vec::new();
    for (label, rule) in SAFETY_CORPUS {
        let ours = our_safe(rule);
        let theirs = authority_safe(rule);
        println!("[{label}] here safe={ours}, authority safe={theirs}  {rule:?}");
        match (ours != theirs, recorded.get(label)) {
            (true, Some(reason)) => {
                diverged.push(*label);
                println!("  recorded boundary — {reason}");
            }
            (true, None) => unrecorded.push(format!(
                "[{label}] `{}`: here safe={ours}, authority safe={theirs}",
                rule.trim_end()
            )),
            (false, Some(_)) => stale.push(*label),
            (false, None) => {}
        }
    }
    assert!(
        unrecorded.is_empty(),
        "unrecorded safety divergences (each a characterized boundary of §5/§10, or a defect):\n{}",
        unrecorded.join("\n"),
    );
    assert!(
        stale.is_empty(),
        "these no longer diverge from the authority — remove them from the recorded set: {stale:?}",
    );
    assert_eq!(
        diverged.len(),
        recorded.len(),
        "every recorded divergence occurs on the corpus",
    );
}

/// The classification differential — the predicate-level approximations (tightness,
/// head-cycle-freeness, finiteness) against the *ground-level* truth, and the classes
/// against a corpus of programs tagged with their known classification from the literature
/// — is **deferred to the solve stage** (analysis §10, §11): it needs a ground dependency
/// graph, hence a grounder, which no tier of this estate yet has. It is named here so the
/// obligation is not silently dropped; when a grounder exists, a predicate-level `Unknown`
/// where the ground program *does* have the property is the approximation's stated
/// imprecision, while a `Holds` the ground program lacks is a defect. This placeholder
/// asserts nothing.
#[test]
#[ignore = "deferred to the solve stage (analysis §10, §11): the ground-level classification differential needs a grounder"]
fn the_classification_differential_is_the_solve_stages() {
    // Intentionally empty: the recorded, deferred obligation (analysis §10).
}
