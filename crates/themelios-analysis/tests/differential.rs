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
use themelios_program::raise::{LowerErrorKind, Raised, raise};
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

/// A raise that faithfully represents the program — no diagnostic marking a *lossy* reading (a
/// best-effort partial the value could not fully represent) of text the **authority itself admits**.
/// Today the one such kind is a truncated *theory*-atom argument-list pool (`PooledArgumentList`,
/// program §8): the grounder unpools `&t(a; b)` into distinct theory atoms, but this tier defers
/// theory terms to the solve stage (§7, program §4.9) and reads only the first alternative, so safety
/// and finiteness of that reading are not the grounder's. The composed verdict fails closed on it —
/// agreeing with the grounder — pending the solve stage. An *ordinary*-atom argument-list pool
/// `p(X; a)` raises faithfully and `unpool` (program §9) eliminates it before this reads it, so it is
/// not lossy and draws a genuine comparison. The *other* `LowerErrorKind`s do not arise here: each
/// marks input the authority **refuses** at admission (a recovered/incomplete construct, a malformed or
/// out-of-range token, an unexpanded splice, a non-constant `#const`), so no safety comparison is drawn
/// on it. `LowerErrorKind` is `#[non_exhaustive]`: a future lossy kind reachable from admitted text
/// must join this gate (the obligation is recorded on the enum, program tier).
fn raised_faithfully(lowered: &Raised) -> bool {
    !lowered
        .diagnostics()
        .iter()
        .any(|error| matches!(error.kind(), LowerErrorKind::PooledArgumentList))
}

/// This tier's safety verdict for a program of concrete syntax (§5). A theory rule raises
/// with an up-to-grounding diagnostic (program §4.9) yet still yields a program to read;
/// the verdict is taken on that program, so a theory case is analyzable here. An ordinary-atom
/// pool raises faithfully and is unpooled before safety reads it; a *theory*-atom pool the raise
/// could not faithfully represent fails closed (§7): the composed safety verdict is trustworthy
/// only on a faithful raise (§5).
fn our_safe(text: &str) -> bool {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    raised_faithfully(&lowered) && Safety::of(lowered.program()).is_safe()
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
    // A `#project`/`#heuristic` atom domain-matches: its variables bind in the same invertible
    // positions a body atom's do (a bare-variable atom is safe; `#project p(X/2).` is not, below);
    // the body's own variables must be safe too.
    ("project-atom-body-binds", "#project p(X) : q(X).\n"),
    ("project-atom-binds", "#project p(X).\n"),
    ("project-body-unsafe", "#project p(X) : Z > 1.\n"),
    ("edge-body-binds", "#edge (X, Y) : q(X), r(Y).\n"),
    ("edge-empty-body-unbound", "#edge (X, Y).\n"),
    // A `#heuristic` atom *domain-matches* (as a `#project` atom does): its variables bind its required
    // bracket terms (bias/priority/modifier), so a bracket variable the atom carries is safe, while one
    // it does not is not.
    (
        "heuristic-body-binds",
        "#heuristic p(X) : q(X). [1@1, true]\n",
    ),
    ("heuristic-ground-bracket", "#heuristic p(X). [1@1, true]\n"),
    // The discriminating row: the bracket bias is `X`, bound by the atom's domain match — safe under the
    // domain-match reading, unsafe under the old wildcard reading.
    (
        "heuristic-atom-binds-bracket",
        "#heuristic p(X). [X@1, true]\n",
    ),
    (
        "heuristic-bias-unbound",
        "#heuristic p(a) : q(a). [W@1, true]\n",
    ),
    ("external-body-binds", "#external p(X) : q(X).\n"),
    ("external-empty-body-unbound", "#external p(X).\n"),
    // The `#external` truth-value bracket is required and bound by the body, like the atom (gringo's
    // `ExternalHeadAtom::collect` collects it with `bound=false`): a variable there with no binder is
    // unsafe, a bound one is safe.
    ("external-value-unbound", "#external p(X) : q(X). [W]\n"),
    ("external-value-bound", "#external p(X) : q(X). [X]\n"),
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
    // ---- Body-less `#show`: `#show t.` has no body, so a variable in its shown term has
    // no binder and is unsafe — clingo flags `#show f(X).`. (The bodied `#show t : body.` is above.)
    ("show-bare-term-unbound", "#show f(X).\n"),
    ("show-bare-term-ground", "#show f(a).\n"),
    // ---- Anonymous `_` projected under default negation: the grounder rewrites each `_`
    // under `not` to a fresh existential, so a `_` there (reachable through constructor structure only)
    // needs no binder, while a genuine named variable under `not` is required, and a `_` under an
    // evaluated term (division) is not projected.
    ("neg-anon-projected", "p :- not q(_).\n"),
    ("neg-anon-nested-projected", "p :- not q(f(_)).\n"),
    ("neg-anon-beside-bound", "p(X) :- q(X), not r(X, _).\n"),
    ("neg-named-required", "p :- not q(Y).\n"),
    (
        "neg-anon-evaluated-required",
        "p(X) :- q(X), not r(_ / 2).\n",
    ),
    // ---- Body-conditional two-stage binding: a conditional's condition instantiates
    // first, then its literal binds *locally* — a positive literal by matching, an assignment by its
    // lone side — so `X = 1 : q` is safe and `s :- p(X) : r(Y)` is safe (X local), while a global X the
    // conditional binds only locally (`s(X) :- p(X) : r(Y)`) is unsafe.
    ("cond-assignment-binds-local", "h :- X = 1 : q(_).\n"),
    ("cond-positive-binds-local", "s :- p(X) : r(Y).\n"),
    ("cond-comparison-unbound", "h :- X < 5 : q(_).\n"),
    ("cond-global-not-bound", "s(X) :- p(X) : r(Y).\n"),
    // ---- Positive-atom binding, following the grounder's `simplify` (§5): a positive atom binds a
    // variable only in an invertible/matchable position — through the Herbrand constructors and a linear
    // arithmetic form (`+`/`-`/`*` of a numeric constant and a linear operand, `*` non-zero) — not
    // through a non-invertible former (division, absolute, interval, a zero multiplier, an interval as an
    // arithmetic operand). A variable also present in a bindable position elsewhere is still bound.
    ("atom-function-binds", "r(X) :- p(f(X)).\n"),
    ("atom-invertible-add-binds", "r(X) :- p(X + 1).\n"),
    ("atom-negation-binds", "r(X) :- p(-X).\n"),
    ("atom-multiplier-binds", "r(X) :- p(2 * X).\n"),
    ("atom-division-unbound", "r(X) :- p(X / 2).\n"),
    ("atom-absolute-unbound", "r(X) :- p(|X|).\n"),
    ("atom-interval-position-unbound", "r(X) :- p(X .. 3).\n"),
    ("atom-two-variable-arith-unbound", "r(X, Y) :- p(X + Y).\n"),
    ("atom-bindable-elsewhere", "r(X) :- p(X / 2, X).\n"),
    // A zero multiplier is not invertible (the grounder keeps `0*X` untouched): `X` is required, unbound.
    ("atom-multiply-by-zero-unbound", "r(X) :- p(X * 0).\n"),
    ("atom-zero-multiplier-unbound", "r(X) :- p(0 * X).\n"),
    // A zero product (`0*3`) is likewise kept untouched *before* folding — it never becomes a constant
    // that could bind through an enclosing linear form, so `X + 0*3` leaves X unbound.
    (
        "atom-zero-product-operand-unbound",
        "r(X) :- p(X + 0 * 3).\n",
    ),
    // An interval as an arithmetic operand introduces a range variable, so the arithmetic leaves two
    // variables — not invertible; `X` is unbound (a bare interval position is `atom-interval-position`).
    (
        "atom-interval-operand-unbound",
        "r(X) :- p(X + (1 .. 3)).\n",
    ),
    // A `#project`/`#heuristic` atom domain-matches with the same invertible-position rule: a variable in
    // a non-invertible atom position is required and unbound.
    (
        "project-atom-non-invertible-unbound",
        "#project p(X / 2).\n",
    ),
    (
        "heuristic-atom-non-invertible-unbound",
        "#heuristic p(X / 2). [1@1, true]\n",
    ),
    // A positive conditional literal binds locally in its invertible positions, so a non-invertible one
    // is unsafe just as a body atom is (the head-element path already requires all its variables).
    ("cond-non-invertible-unbound", "t :- p(X / 2) : r.\n"),
    // ---- Pools (§5, §9): the raise represents both a term pool (`(a;X)`, `f(a;X)`) and an atom-level
    // argument-list pool (`p(X;a)`, whole argument tuples) faithfully, and the unpool pass (program §9)
    // expands each into the distinct atoms the grounder does — before this analysis reads it. So a
    // pooled program's safety agrees with the grounder at every position: a variable is safe iff it
    // binds in *every* alternative (`p((X;X))` safe — both bind X; `p((a;X))` unsafe — the `a`
    // alternative drops X).
    ("atom-pool-position-unbound", "q(X) :- p((a;X)).\n"),
    ("pool-every-alternative-binds", "q(X) :- p((X;X)).\n"),
    //   A body argument-list pool `unpool` expands to `q(X):-p(X).` and `q(X):-p(a).`; the second
    //   leaves the head's `X` unbound — unsafe, agreeing with the grounder.
    ("atom-argument-list-pool", "q(X) :- p(X; a).\n"),
    //   A directive pool — `#external p(a; X) : q.` leaves `X` unbound in the body — unsafe.
    ("external-atom-list-pool", "#external p(a; X) : q.\n"),
    //   A head pool where every unpooled rule is safe — `p(X):-p(X).` and `p(f(X)):-p(X).` — safe to
    //   both; finiteness catches the unbounded growth of the `p(f(X))` rule (the finiteness corpus).
    ("atom-argument-list-pool-head", "p(X; f(X)) :- p(X).\n"),
    //   Element-level pools expand *within* their container (never lifting the pool to the rule),
    //   agreeing with the grounder: an aggregate element, a choice element, a disjunction element, and a
    //   conditional literal's condition — each here a safe agreement case.
    ("aggregate-element-pool", "q :- 1 <= #count { p(a; b) }.\n"),
    ("choice-element-pool", "{ p(a; b) }.\n"),
    ("disjunction-element-pool", "p(X; a) | q :- r(X).\n"),
    //   A residual pooled disjunct of HETEROGENEOUS arities is several head predicates read
    //   per-alternative: `p(X; f(X), Y)` is p/1 and p/2, and `Y` lives only in the p/2 alternative,
    //   unbound by the body — unsafe to both, the safety-side proof that every alternative's variables
    //   are read (not just the first alternative's).
    (
        "disjunction-element-pool-heterogeneous",
        "p(X; f(X), Y) | q :- r(X).\n",
    ),
    (
        "disjunction-element-pool-both-unbound",
        "p(X; Y) | q :- r.\n",
    ),
    ("conditional-condition-pool", "q :- p : r(a; b).\n"),
    // ---- The matching-`=` dialect boundary (§5): clingo decomposes an `=` against a tuple, and chains
    // a multi-step `=`, to bind — where the strict ASP-Core-2 standard does not. Recorded divergences;
    // this tier is the conservative side (never a false safe).
    ("tuple-decomposition", "p(X, Y) :- q(Z), Z = (X, Y).\n"),
    ("chained-equality", "p(X, Y) :- X = Y = 1.\n"),
    // ---- Formers on both sides of `=` (the matching-`=` family). With neither side bound,
    // `f(X) = f(Y)` is unsafe to both — decomposition to `X = Y` binds nothing without a value.
    ("equality-formers-both-sides", "p(X, Y) :- f(X) = f(Y).\n"),
    // But with one side bound, clingo decomposes `f(X) = f(Y)` to bind the other (`q(X)` binds X, so Y
    // binds); the strict standard binds only a lone side, so this tier reports Y unsafe. Recorded
    // divergence, conservative side.
    (
        "equality-former-decomposition",
        "p(Y) :- q(X), f(X) = f(Y).\n",
    ),
    // ---- Arithmetic inversion in an assignment: clingo inverts `Y = X + 1` (Y bound by
    // q(Y)) to bind X; the strict standard binds only a lone-variable side. Recorded divergence. A
    // safety-only fix here would OPEN a false `Holds` (the deepening graph is direction-sensitive, §5),
    // so it stays characterized, not adopted.
    (
        "equality-arithmetic-inversion",
        "p(X) :- q(Y), Y = X + 1.\n",
    ),
    // ---- Arithmetic over a non-numeric (Herbrand) operand: the grounder reads `f(X)*g(Y)` and `-f(X)`
    // safe — a type error it accepts vacuously, or unary `-` passing boundness through the function
    // (gringo `UnOpTerm::collect`) — while this tier reads a non-numeric arithmetic operand as not
    // invertible. Recorded divergences, the conservative side (never a false safe).
    (
        "atom-arith-over-functions",
        "r(X) :- p(f(X) * g(Y)), s(Y).\n",
    ),
    ("atom-negate-over-function", "r(X) :- p(-f(X)).\n"),
    // An overflowing constant fold: the grounder wraps (a non-zero coefficient stays invertible); this
    // tier's ground evaluator refuses overflow, reading the operand as not invertible — a refusal can
    // only push toward unsafe, never a false safe.
    (
        "atom-overflow-coefficient",
        "r(X) :- p((1073741824 * 2) * X).\n",
    ),
    // A pooled directive atom: the grounder unpools `#project p((X;a))` per alternative, and this
    // tier's `unpool` (program §9) does the same before analysis, so their readings agree.
    ("project-atom-pool", "#project p((X;a)).\n"),
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
    // (The pool cases `pool-every-alternative-binds`, `atom-argument-list-pool-head`, and
    // `project-atom-pool` were recorded divergences while the tier fail-closed on a pool; the faithful
    // representation + the unpool pass (program §9) now agree with the grounder, so they are corpus
    // agreement rows, not divergences.)
    // clingo decomposes a former on both sides of `=` when one side is bound (`q(X)` binds X, so
    // `f(X) = f(Y)` binds Y); the strict standard binds only a lone side, so this tier reports Y unsafe
    // — the matching-`=` family, the conservative side, never a false safe.
    (
        "equality-former-decomposition",
        "clingo decomposes `f(X) = f(Y)` with X bound to bind Y; the strict standard binds only a lone side",
    ),
    // clingo inverts arithmetic in an assignment (`Y = X + 1`, Y bound, binds X); this tier binds only a
    // lone-variable side, so it reports X unsafe. Characterized, NOT adopted: a safety-only
    // fix would open a false `Holds`, since the finiteness deepening graph is direction-sensitive (§5) —
    // any future fix must first add the inverse-deepening edge.
    (
        "equality-arithmetic-inversion",
        "clingo inverts `Y = X + 1` to bind X; this tier binds only a lone-variable side (a false `Holds` risk if adopted)",
    ),
    // clingo accepts arithmetic over a non-numeric operand *vacuously* (`f(X) * g(Y)` is a
    // grounding-time type error, so the rule never fires) and reads its variables safe; this tier's
    // binding rule reads a non-numeric arithmetic operand as not invertible, so it reports the
    // arithmetic-only variable unsafe — the conservative side, never a false safe.
    (
        "atom-arith-over-functions",
        "clingo binds X in `f(X) * g(Y)`; this tier's certain-occurrence binding binds neither function operand",
    ),
    (
        "atom-negate-over-function",
        "clingo binds X in `-f(X)` (unary `-` passes boundness through the function); this tier reads a non-numeric arithmetic operand as not invertible",
    ),
    (
        "atom-overflow-coefficient",
        "clingo wraps an overflowing constant fold (a non-zero coefficient stays invertible); this tier's evaluator refuses overflow — a refusal only pushes toward unsafe",
    ),
    // Not recorded here: the head conditional `p(X) : q(X)` agrees. A disjunction/choice element is
    // scoped to itself, its literal bound by its own condition, so the tier reads it safe with clingo
    // — the `condition-binds-head` corpus row is an agreement case, not a divergence.
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
    raised_faithfully(&lowered)
        && matches!(Safety::of(lowered.program()).finiteness(), Verdict::Holds)
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
    // A domain-extending `#external` on a **finite** body: it generates only p(f(1)), p(f(2))
    // — a non-recursive generation component — so grounding is finite (`Holds`), precisely, not the blunt
    // over-conservative `Unknown`. Grounds within the cap.
    (
        "external-finite-body-holds",
        "q(1). q(2).\n#external p(f(X)) : q(X).\n",
    ),
];

/// A program this tier reports `Unknown` and that grounds unboundedly (a term former deepens the
/// recursion): the control that proves the backstop is **live** — the cap fires on an infinite
/// grounding, and this tier's `Unknown` correctly flags the infinity, so the `Holds` corpus above is
/// not passing vacuously.
const FINITENESS_INFINITE_CONTROLS: &[(&str, &str)] = &[
    ("recursive-former-grows", "p(a).\np(f(X)) :- p(X).\n"),
    // External-borne generation growth: the external generates p(f(a)), p(f(f(a))), … and q derives
    // around the loop, so grounding is unbounded — `Unknown` here, and clingo hits the cap through the
    // `external` observer (the external-only-growth path the counter now covers).
    (
        "external-generation-grows",
        "p(a).\nq(X) :- p(X).\n#external p(f(X)) : q(X).\n",
    ),
    // NB: extremum growth aliased through an element-local `=` (`X = #max { Z : p(Y), Z = f(Y) }`) is a
    // genuine unbounded grounding this tier reads `Unknown` (regression-guarded by a unit law in
    // safe_laws), but it grows through `#max` *re-evaluation* — O(cap²) grounding work — so it does not
    // reach the rule/external cap fast enough for the bounded backstop here. Its ground truth was
    // established directly against the grounder.
];

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

    // The controls: this tier says Unknown, and the authority hits the cap — so the backstop can fire.
    // Both a rule-borne and an external-borne growth, so the cap is proven live on each observer path.
    for (label, control) in FINITENESS_INFINITE_CONTROLS {
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
}

/// The pooled-head false `Holds` the grounder differential surfaced, and the pass that shuts it.
/// `p(X; f(X)) :- p(X).` is an atom-level head pool: `unpool` (program §9) expands it into the grower
/// `p(f(X)) :- p(X).` (alongside `p(X) :- p(X).`), which grounds unboundedly, so finiteness — reading
/// the unpooled program — correctly does *not* report `Holds`. The truncated first-alternative reading
/// `p(X) :- p(X).` alone IS `Holds`, the latent false `Holds`; it is shut not by trusting that
/// truncation but by the faithful unpool exposing the grower to the growth check (§5) — the direct
/// proof that the pass holds the grounding-DoS boundary the pool truncation had opened.
#[test]
fn a_pooled_head_atom_does_not_yield_a_false_holds() {
    let grower = "p(a).\np(X; f(X)) :- p(X).\n";
    // The truncated reading the raise produces IS `Holds` — the latent false `Holds` the gate shuts.
    let truncated = "p(a).\np(X) :- p(X).\n";
    assert!(
        our_holds(truncated),
        "the truncated reading is Holds — the false Holds the gate must shut: {truncated:?}",
    );
    // But `unpool` exposes the grower `p(f(X)) :- p(X).`, so finiteness does not report a false Holds.
    assert!(
        !our_holds(grower),
        "the pooled head, unpooled, exposes the grower and is not a false Holds: {grower:?}",
    );
    // And the grounder confirms the pool grounds unboundedly — a real false Holds, were the reading
    // trusted.
    let (grounded, capped) = authority_ground(grower);
    assert!(
        capped && !grounded,
        "the grounder unpools the pool into a grower and hits the cap \
         (grounded={grounded}, capped={capped}): {grower:?}",
    );
}

#[test]
fn a_pooled_disjunct_grower_does_not_yield_a_false_holds() {
    // `p(f(X); X) | q :- p(X).` — a grower hidden in a pooled disjunct. `unpool` LEAVES the pool
    // (a pooled disjunct literal needs a conjunctive head group a single-literal element cannot
    // hold, program §9; the statement split that would realise it is exponential over K disjuncts,
    // so the pool is left, read per-alternative by analysis). Finiteness reads every alternative of
    // the residual head, so it sees the `p(f(X))` growth and reports no false `Holds` — in either
    // pooled position, so the reading is not first-alternative-only. The grounder confirms unbounded.
    for grower in [
        "p(a).\np(f(X); X) | q :- p(X).\n",
        "p(a).\np(X; f(X)) | q :- p(X).\n",
    ] {
        assert!(
            !our_holds(grower),
            "a grower in a residual pooled disjunct must not be a false Holds: {grower:?}",
        );
        let (grounded, capped) = authority_ground(grower);
        assert!(
            capped && !grounded,
            "the grounder grows the pooled disjunct and hits the cap \
             (grounded={grounded}, capped={capped}): {grower:?}",
        );
    }
}

#[test]
fn a_theory_atom_pool_defers_the_analysis_gate() {
    // A theory-atom argument-list pool `&sum(a; b)` is deferred to the solve stage (program §4.9,
    // §8): the raise reads only the first alternative and marks the rest `PooledArgumentList`, so
    // `unpool` cannot expose the dropped alternatives and the composed gate must fail closed — never
    // certifying safety or `Holds` on that truncated reading. The `raised_faithfully` guard is what
    // shuts it; here it is exercised end to end at the analysis boundary, not just at the raise.
    let theory_pool = ":- &sum(a; b) { x }.\n";
    assert!(
        !our_safe(theory_pool),
        "a theory-atom pool defers — the safety gate fails closed on the truncated reading: {theory_pool:?}",
    );
    assert!(
        !our_holds(theory_pool),
        "a theory-atom pool defers — the finiteness gate fails closed on the truncated reading: {theory_pool:?}",
    );
    // Its structural twin, the *ordinary*-atom pool `p(a; b)`, raises faithfully and IS analyzed
    // (unpooled to the ground facts `p(a). p(b).`, safe), so the deferral is specific to theory terms.
    let ordinary_pool = "p(a; b).\n";
    assert!(
        our_safe(ordinary_pool),
        "an ordinary-atom pool raises faithfully and is analyzed, not deferred: {ordinary_pool:?}",
    );
}

#[test]
fn a_heterogeneous_arity_pooled_disjunct_grower_does_not_yield_a_false_holds() {
    // A residual pooled disjunct of HETEROGENEOUS arities — `p(X; f(X), Y)` is p/1 (`p(X)`) and p/2
    // (`p(f(X), Y)`) — hides the grower in the p/2 alternative, which feeds the p/2 body. Were the
    // finiteness check to find the atom's component by its FIRST alternative's signature (p/1, here
    // non-recursive), it would never reach the growing p/2 — a false `Holds` (the F4 gap option B
    // reopened). The per-alternative signature read (depend `atom_alternative_signatures`) visits
    // p/2's component and catches it, in either alternative order. The grounder confirms unbounded.
    for grower in [
        "p(a, a).\np(X; f(X), Y) | r :- p(X, Y).\n",
        // the same grower with the alternatives reversed, so the recursive one is not first
        "p(a, a).\np(f(X), Y; X) | r :- p(X, Y).\n",
        // deeper heterogeneity — the grower is the THIRD alternative (p/3), neither p/1 nor p/2
        "p(a, a, a).\np(X; Y, X; f(X), Y, Z) | r :- p(X, Y, Z).\n",
        // two independent pooled disjuncts, each a grower, in one head
        "p(a).\nq(a).\np(X; f(X)) | q(Y; g(Y)) :- p(X), q(Y).\n",
    ] {
        assert!(
            !our_holds(grower),
            "a grower in a heterogeneous-arity residual pooled disjunct must not be a false Holds: {grower:?}",
        );
        let (grounded, capped) = authority_ground(grower);
        assert!(
            capped && !grounded,
            "the grounder grows the pooled disjunct's higher-arity alternative and hits the cap \
             (grounded={grounded}, capped={capped}): {grower:?}",
        );
    }
}

#[test]
fn a_lone_conditional_pooled_head_fails_closed_in_safety() {
    // A body conditional whose head literal is pooled — `q :- p(X; a) : r.` — is the residual `unpool`
    // leaves (it would need a disjunctive clause `(p(X); p(a)) : r` a single-literal conditional cannot
    // hold, program §9), the structural twin of a pooled disjunct. Its pooled head binds nothing
    // (safe.rs `bind_positive_atom`), so `X` stays required and unbound and the rule is **unsafe** —
    // fail-closed, where its non-pooled twin `q :- p(X) : r.` binds `X` locally and is safe. This is
    // the safety-side witness that the conditional residual fails closed, as the disjunct residual does
    // above for finiteness.
    assert!(
        !our_safe("q :- p(X; a) : r.\n"),
        "a pooled conditional head binds nothing, so its variable is unbound → unsafe",
    );
    assert!(
        our_safe("q :- p(X) : r.\n"),
        "the non-pooled twin binds X locally → safe",
    );
}
