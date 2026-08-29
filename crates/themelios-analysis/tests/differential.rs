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
//! corpus — the universal law is the naive-reference proptest (safe_laws). Finiteness's
//! `Holds`-**soundness** is backed here now, by a bounded grounding check
//! (`a_holds_verdict_grounds_within_the_bound`): a `Holds` program must ground within a rule-count
//! cap, and one that does not is a false `Holds`. The full ground-level **classification** differential
//! (exact tightness / head-cycle-freeness against the ground graph, and finiteness *precision*) still
//! needs a ground dependency graph, hence a grounder, and is the solve stage's — deferred, named in
//! that test's doc so it is not silently dropped (§10, §11).

#![cfg(feature = "differential")]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

use themelios_analysis::classify::Verdict;
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
    // A choice element is scoped to itself: element two's `s(X)` is unbound though element one's
    // `q(X)` binds a same-named X — no cross-element discharge.
    ("choice-cross-element", "{ p(X) : q(X); s(X) } :- r.\n"),
    // A choice cardinality guard variable is required, never bound.
    ("choice-guard-unbound", "W { p(X) : q(X) } :- r.\n"),
    // The constructions where the two notions may part — the recorded boundaries.
    ("head-aggregate-bare", "#count { X : p(X) } :- q.\n"),
    ("interval-binds", "p(X) :- X = 1..3.\n"),
    // ---- Bodied directives (the directive-scope close, §5): a directive's non-body term positions
    // are bound by its body, exactly as a rule's head is — vetted like a rule, agreeing with clingo.
    ("weak-constraint-body-binds", ":~ q(W). [W@1]\n"),
    ("weak-constraint-weight-unbound", ":~ q. [W@1]\n"),
    ("show-term-body-binds", "#show f(X) : q(X).\n"),
    ("show-term-body-unbound", "#show f(X) : q(Y).\n"),
    // `#project`/`#heuristic` treat their atom's variables as a schema wildcard (ranging over the
    // atom's instances), so a bare-variable atom is safe; only the body's own variables must be safe.
    ("project-atom-body-binds", "#project p(X) : q(X).\n"),
    ("project-atom-wildcard", "#project p(X).\n"),
    ("project-body-unsafe", "#project p(X) : Z > 1.\n"),
    ("edge-body-binds", "#edge (X, Y) : q(X), r(Y).\n"),
    ("edge-empty-body-unbound", "#edge (X, Y).\n"),
    // A `#heuristic` atom is a wildcard like `#project`'s, but its bracket terms (bias/priority/
    // modifier) are required and bound by the body.
    (
        "heuristic-body-binds",
        "#heuristic p(X) : q(X). [1@1, true]\n",
    ),
    ("heuristic-atom-wildcard", "#heuristic p(X). [1@1, true]\n"),
    (
        "heuristic-bias-unbound",
        "#heuristic p(a) : q(a). [W@1, true]\n",
    ),
    ("external-body-binds", "#external p(X) : q(X).\n"),
    ("external-empty-body-unbound", "#external p(X).\n"),
    (
        "minimize-element-binds",
        "#minimize { W@P, X : q(X, W, P) }.\n",
    ),
    ("minimize-element-unbound", "#minimize { W@P, X : q(X) }.\n"),
    // ---- Theory-term-local close (§4.9): a theory atom's ordinary variable leaves are required and
    // bound like a rule's, agreeing with clingo. Each row carries a minimal `#theory` definition, or
    // clingo errors on *admission* (not safety) and the "safe = absence of an unsafe report" reading
    // would misread it.
    (
        "theory-element-unbound",
        "#theory t { term { }; &t/0 : term, {>=}, term, any }.\n:- &t { X }.\n",
    ),
    (
        "theory-element-bound",
        "#theory t { term { }; &t/0 : term, {>=}, term, any }.\n:- &t { X }, q(X).\n",
    ),
    (
        "theory-element-condition-binds",
        "#theory t { term { }; &t/0 : term, {>=}, term, any }.\n:- &t { X : q(X) }.\n",
    ),
    (
        "theory-guard-unbound",
        "#theory t { term { }; &t/0 : term, {>=}, term, any }.\n:- &t { 0 } >= Y.\n",
    ),
    // ---- The matching-`=` dialect boundary (§5): clingo decomposes an `=` against a tuple, and chains
    // a multi-step `=`, to bind — where the strict ASP-Core-2 standard does not. Recorded divergences;
    // this tier is the conservative side (never a false safe).
    ("tuple-decomposition", "p(X, Y) :- q(Z), Z = (X, Y).\n"),
    ("chained-equality", "p(X, Y) :- X = Y = 1.\n"),
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
    // clingo decomposes an `=` against a tuple to bind the tuple's variables (`Z = (X, Y)` binds X, Y
    // when Z is bound); the strict ASP-Core-2 standard binds only the lone side (Z), so this tier
    // reports X, Y unsafe — the conservative direction, never a false safe. The matching-`=` boundary
    // §5 names, a candidate for the dialect parameterization.
    (
        "tuple-decomposition",
        "clingo decomposes `Z = (X, Y)` to bind X, Y; the strict standard binds only the lone side",
    ),
    // clingo binds through a chained comparison `X = Y = 1` (each step an assignment); this tier reads
    // a multi-step comparison as a single non-binding constraint (only a single-step `X = t` binds),
    // so it reports X, Y unsafe — again the conservative side. The chained-`=` candidate for the
    // dialect parameterization (§5).
    (
        "chained-equality",
        "clingo binds through a chained `X = Y = 1`; this tier's `=` binds only as a single step",
    ),
    // The head conditional `p(X) : q(X)` was once recorded here as a divergence — the tier read a
    // head literal's variables as rule-global. It no longer diverges: a disjunction/choice element
    // is scoped to itself, its literal bound by its own condition, so the tier now agrees with
    // clingo that the idiom is safe. The `condition-binds-head` corpus row stays below as an
    // agreement case.
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

/// This tier's grounding-finiteness verdict for a program of concrete syntax: whether grounding is
/// proven **term-depth**-finite (`Holds`, §5), the property a grounder relies on.
fn our_holds(text: &str) -> bool {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    matches!(Safety::of(lowered.program()).finiteness(), Verdict::Holds)
}

/// Whether the authority grounds the program within a rule-count cap (§5, §10): `(grounded, capped)`.
/// A term-depth-finite program grounds; one that grounds unboundedly hits the cap. Bounded in memory
/// and time — a counting backend observer aborts past the cap, so there is no timeout and no
/// exhaustion (the driver's `ground` mode; clingo interns nested terms, so the aborted grounding
/// stays bounded).
fn authority_ground(program: &str) -> (bool, bool) {
    let mut child = Command::new("python")
        .arg(authority_py())
        .arg("ground")
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
    let output = child.wait_with_output().expect("the authority answers");
    assert!(
        output.status.success(),
        "the authority helper failed (run through pixi):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("the helper emits JSON");
    assert_eq!(
        value["version"].as_str(),
        Some(AUTHORITY_VERSION),
        "docs/grammar.md §3: the authority is pinned at v{AUTHORITY_VERSION}"
    );
    (
        value["grounded"].as_bool().expect("grounded is a bool"),
        value["capped"].as_bool().expect("capped is a bool"),
    )
}

/// The programs this tier proves grounding-finite (`Holds`, §5): non-recursive term formers and
/// non-growing (Datalog) recursion, each **seeded** so grounding actually produces rules. Each MUST
/// ground within the authority's cap — a `Holds` the ground program lacks (a program that grounds
/// unboundedly) would hit the cap, the false `Holds` the backstop exists to catch. **Integer**-value
/// growth is out of this facet's **term-depth** scope (§5) — a program can be `Holds` yet ground
/// infinitely through an unbounded integer — so no unbounded-integer aggregate appears here.
const FINITENESS_HOLDS_CORPUS: &[(&str, &str)] = &[
    ("non-recursive-former", "p(a).\nq(f(X)) :- p(X).\n"),
    (
        "nested-non-recursive-former",
        "p(a).\nq(f(g(X))) :- p(X).\n",
    ),
    (
        "bounded-former-over-facts",
        "p(a). p(b). p(c).\nq(f(X)) :- p(X).\n",
    ),
    (
        "datalog-transitive-closure",
        "edge(a,b). edge(b,c). edge(c,d).\nreach(X,Y) :- edge(X,Y).\nreach(X,Z) :- reach(X,Y), edge(Y,Z).\n",
    ),
    (
        "mutual-non-growing-recursion",
        "a(1).\nb(X) :- a(X).\na(X) :- b(X).\n",
    ),
    (
        "former-off-the-recursion",
        "n(0). n(1).\nr(X) :- n(X).\nq(f(X)) :- n(X), r(X).\n",
    ),
    // The aliasing boundary: `X = Y` aliases (same depth) where `X = f(Y)` deepens — the distinction
    // the false-`Holds` rounds hinged on. Recursive but non-growing, so `Holds`; clingo grounds the
    // tautology `p(Y) :- p(Y)` to just the seed.
    (
        "recursive-aliasing-not-growth",
        "p(a).\np(X) :- p(Y), X = Y.\n",
    ),
];

/// A program this tier reports `Unknown` and that grounds unboundedly (a term former deepens the
/// recursion): the control that proves the backstop is **live** — the cap fires on an infinite
/// grounding, and this tier's `Unknown` correctly flags the infinity, so the `Holds` corpus above is
/// not passing vacuously.
const FINITENESS_INFINITE_CONTROL: (&str, &str) =
    ("recursive-former-grows", "p(a).\np(f(X)) :- p(X).\n");

/// The bounded finiteness backstop (§5, §6.1, §10): the ground-truth net for a false `Holds`, the one
/// failure §6.1 forbids. The full ground-level *classification* differential (exact tightness /
/// head-cycle-freeness, and finiteness *precision* — an `Unknown` the ground program does not need)
/// still needs a ground dependency graph, hence a grounder, and is the solve stage's (§10, §11); but
/// finiteness's `Holds`-**soundness** is checkable now, by grounding: a `Holds` program must ground
/// within a bound, and one that does not is the defect. This closes the "argument, not ground truth"
/// gap for the `Holds` direction — the direction that fails open at the grounding-DoS boundary.
#[test]
fn a_holds_verdict_grounds_within_the_bound() {
    for (label, program) in FINITENESS_HOLDS_CORPUS {
        assert!(
            our_holds(program),
            "[{label}] this tier must prove Holds for a finiteness-corpus program: {program:?}",
        );
        let (grounded, capped) = authority_ground(program);
        assert!(
            grounded && !capped,
            "[{label}] this tier proves Holds, but the authority did not ground it within the cap — \
             a false Holds (grounded={grounded}, capped={capped}): {program:?}",
        );
    }

    // The control: this tier says Unknown, and the authority hits the cap — so the backstop can fire.
    let (label, control) = FINITENESS_INFINITE_CONTROL;
    assert!(
        !our_holds(control),
        "[{label}] the infinite control must be Unknown, not Holds: {control:?}",
    );
    let (grounded, capped) = authority_ground(control);
    assert!(
        capped && !grounded,
        "[{label}] the infinite control must hit the cap, or the backstop is vacuous \
         (grounded={grounded}, capped={capped}): {control:?}",
    );
}
