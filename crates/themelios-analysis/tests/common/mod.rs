#![allow(dead_code)] // each test binary reads a subset of the corpus

/// A labeled finiteness template: `(label, program)`. Its intended verdict is fixed by the class it sits
/// in; its **true** boundedness is argued in the row's own comment and cross-checked against clingo in the
/// feature-gated differential (never read from themelios).
pub type Row = (&'static str, &'static str);

/// SOUNDNESS (load-bearing): every template here grounds **unbounded** in term depth, so the verdict MUST
/// be `Unknown`. A `Holds` here is a false `Holds` — the one failure the verdict forbids. The arithmetic
/// growth here is by **translation** (`m=1, n≠0`) — the one linear invertible form unbounded **under
/// body inversion** as a *single* former: a single `m=-1` former is an involution and a single `|m|>1`
/// former contracts, each bounded **per former** (DOCUMENTED_CONSERVATISM). That boundedness is per
/// former only: composed across mutual recursion, two `m=-1` formers yield a translation
/// (`composed-reflections-translate` below — `p(X) :- q(-X). q(X) :- p(-X + 1).` is `p(X) ⟸ p(X + 1)`,
/// unbounded), which the per-former conservatism (every invertible former is flagged) still catches. A
/// future contraction refinement must account for composition — never treat `m=-1`/`|m|>1` recursion
/// as unconditionally bounded.
pub const SOUNDNESS: &[Row] = &[
    // The inverse-descent family — a translation former in a positive body atom, the fixed defect.
    ("inverse-descent-plus", "p(0).\np(X) :- p(X + 1).\n"), // descends p(0)→p(-1)→… unbounded
    ("inverse-descent-minus", "p(0).\np(X) :- p(X - 1).\n"), // ascends p(0)→p(1)→… unbounded
    // Mutual recursion carrying the arithmetic translation across two predicates.
    (
        "inverse-descent-mutual",
        "p(0).\np(X) :- q(X + 1).\nq(X) :- p(X).\n",
    ),
    // Two `m=-1` formers composed across mutual recursion: each an involution alone, but
    // `q(X) :- p(-X + 1)` then `p(X) :- q(-X)` is `p(X) ⟸ p(X + 1)` — a translation, descending
    // p(0)→p(-1)→… unbounded. The composition a per-former contraction refinement must not overlook.
    (
        "composed-reflections-translate",
        "p(0).\np(X) :- q(-X).\nq(X) :- p(-X + 1).\n",
    ),
    // The already-covered growth positions, asserted at the verdict level too (regression guards).
    ("head-former-grows", "p(0).\np(X + 1) :- p(X).\n"),
    ("head-herbrand-former-grows", "p(a).\np(f(X)) :- p(X).\n"),
    (
        "body-equality-former-grows",
        "p(0).\np(Y) :- p(X), Y = X + 1.\n",
    ),
    // A `#max` over a former element term (extremum value-term depth), unbounded — but it grows by #max
    // RE-EVALUATION (O(cap²) work to reach the rule cap), so its ground truth is NOT cross-checked through
    // the cap in the differential (CAP_TIMING_EXCLUDED, below); its verdict Unknown is asserted here and by
    // the safe_laws extremum laws.
    (
        "max-aggregate-former-grows",
        "p(0).\np(M) :- M = #max { f(Y) : p(Y) }.\n",
    ),
    // An `#external` closing a domain-extending generation loop — grows the Herbrand universe though no
    // rule derives it (finiteness reads the generation graph).
    (
        "external-generation-grows",
        "p(a).\nq(X) :- p(X).\n#external p(f(X)) : q(X).\n",
    ),
    // A grower hidden in a residual pooled disjunct (unpool leaves it; analysis reads every alternative).
    ("pooled-disjunct-grows", "p(a).\np(X; f(X)) | q :- p(X).\n"),
    // A heterogeneous-arity residual pooled disjunct: the grower is the p/2 alternative.
    (
        "heterogeneous-pooled-disjunct-grows",
        "p(a, a).\np(X; f(X), Y) | r :- p(X, Y).\n",
    ),
];

/// PRECISION: every template here is **provably bounded** and the analysis is meant to prove it — the
/// verdict MUST be `Holds`, so the analysis is not vacuously "always Unknown."
pub const PRECISION: &[Row] = &[
    (
        "datalog-transitive-closure",
        "edge(a, b). edge(b, c).\nreach(X, Y) :- edge(X, Y).\nreach(X, Z) :- reach(X, Y), edge(Y, Z).\n",
    ),
    ("non-recursive-former", "p(a).\nq(f(X)) :- p(X).\n"),
    ("herbrand-shrink-body", "p(a).\np(X) :- p(f(X)).\n"), // matched under `f`, derives nothing past the seed
    (
        "recursive-aliasing-not-growth",
        "p(a).\np(X) :- p(Y), X = Y.\n",
    ),
    (
        "mutual-non-growing",
        "a(1).\nb(X) :- a(X).\na(X) :- b(X).\n",
    ),
    // `r` is recursive (`r(X) :- r(Y), n(X)`) but non-growing (bounded by `n`); the former `f(X)` sits in
    // the non-recursive `q`, off the recursion, so the program is bounded.
    (
        "former-off-the-recursion",
        "n(0). n(1).\nr(X) :- n(X).\nr(X) :- r(Y), n(X).\nq(f(X)) :- r(X).\n",
    ),
    (
        "external-finite-body-holds",
        "q(1). q(2).\n#external p(f(X)) : q(X).\n",
    ),
];

/// DOCUMENTED CONSERVATISM: every template here is **bounded**, yet the growth semantics deliberately
/// refuses it — the verdict is `Unknown` **by design**, pinning the known imprecision. (The differential
/// confirms clingo grounds each within the cap — a precision gap, not unsoundness.)
pub const DOCUMENTED_CONSERVATISM: &[Row] = &[
    // A contracting arithmetic former, |m|>1: integer division shrinks, so bounded — but any invertible
    // former is flagged. clingo grounds it to the seed's fixpoint.
    ("contraction-multiply", "p(0).\np(X) :- p(2 * X).\n"),
    // Reflection, m=-1: `p(1)`, `p(-1)` only — bounded — yet `-X` is invertible-linear, so flagged.
    ("reflection-bounded", "p(1).\np(X) :- p(-X).\n"),
    // A negate-with-offset involution (m=-1, n=-1): x ↦ -x-1 satisfies f(f(x))=x, so grounding from p(0)
    // reaches the fixpoint {p(0), p(-1)} — bounded — yet `-X-1` is invertible-linear, so flagged (Unknown
    // by conservatism, not a false Holds). No *single* m=-1 former is unbounded; two composed across
    // mutual recursion can be (SOUNDNESS `composed-reflections-translate`).
    ("negate-offset-involution", "p(0).\np(X) :- p(-X - 1).\n"),
];

/// Labels whose growth is through a **delayed construct the counting observer does not reach at the
/// cap**: aggregate `#max`/`#min` re-evaluation over a former element term (O(cap²) grounding work to
/// reach the rule cap) and — were one ever a SOUNDNESS row — a conditioned head element or a recursive
/// extremum. Such a grower reaches neither the cap nor the end within the differential's wall-clock
/// deadline: its grounding returns `Inconclusive` and **fails** the SOUNDNESS assertion (a witness of
/// nothing, to be marked here or investigated). The exclusion is not there to avoid that failure but
/// to not spend the deadline grinding a witness of nothing, so
/// `the_finiteness_corpus_boundedness_matches_the_authority` skips these rows, and their
/// unboundedness and verdict (`Unknown`) stand on the verdict corpus + the safe_laws extremum laws
/// instead. The tier's existing finiteness backstop already records this exclusion for the aliased
/// sibling.
pub const CAP_TIMING_EXCLUDED: &[&str] = &["max-aggregate-former-grows"];
