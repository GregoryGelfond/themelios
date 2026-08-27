//! The round-trip law (docs/design/program.md §10, §16, spec §7.6): for every program a
//! rendering covers, `raise(parse(render(P, d), d)) == P` up to provenance — render, parse,
//! and raise return the same program. It is a fixpoint law with a trap (a renderer and a
//! parser sharing a consistent misreading satisfy it while both wrong), so it is held two
//! ways: here, against this estate's own parser (the reparse); and against the independent
//! authority, the pinned binary parsing the rendered text (the differential, held elsewhere).
//!
//! Two exceptions are stated, not discovered. The authority's own unparse is non-injective
//! on a pair of forms an empty aggregate can take (`#count {}` with one empty element versus
//! none), so that pair is a named exception. And a theory-bearing program reparses only
//! up-to-grounding (§5): a built theory term and a raised one reconcile under a `#theory`
//! definition alone, so for such programs the law is weakened to "renders and reparses
//! cleanly", the exact identity being the grounding tier's.

use std::fs;
use std::path::{Path, PathBuf};

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::Program;
use themelios_program::raise::raise;
use themelios_program::render::render;

// ---- harness ----

/// Raise a program from concrete syntax under a dialect, asserting it lowers cleanly.
fn raised(text: &str, dialect: Dialect) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, dialect));
    assert!(
        lowered.diagnostics().is_empty(),
        "the fixture raises cleanly under {dialect}: {:?}",
        lowered.diagnostics(),
    );
    lowered.program().clone()
}

/// The round-trip witness (§10, §16): a program, rendered and reparsed under the same
/// dialect, is the same program up to provenance.
fn round_trips(text: &str, dialect: Dialect) {
    let program = raised(text, dialect);
    let rendered = render(&program, dialect).expect("the program renders");
    let source = Source::new(SourceId::new(0), rendered.clone()).expect("the rendering admits");
    let reparsed = raise(&parse(&source, dialect));
    assert!(
        reparsed.diagnostics().is_empty(),
        "the rendering `{rendered}` reparses cleanly: {:?}",
        reparsed.diagnostics(),
    );
    assert_eq!(
        &program,
        reparsed.program(),
        "round-trip up to provenance for `{text}` (rendered `{rendered}`)",
    );
}

/// A rendering reparses cleanly and to the same program *shape* — the weakened law a theory
/// program keeps up-to-grounding (§5): render then reparse never introduces an error.
fn renders_and_reparses_cleanly(text: &str, dialect: Dialect) {
    let program = raised(text, dialect);
    let rendered = render(&program, dialect).expect("the program renders");
    let source = Source::new(SourceId::new(0), rendered.clone()).expect("the rendering admits");
    let reparsed = parse(&source, dialect);
    assert!(
        !reparsed.has_errors(),
        "the rendering `{rendered}` reparses without a syntax error",
    );
    let relowered = raise(&reparsed);
    assert!(
        relowered.diagnostics().is_empty(),
        "the rendering `{rendered}` re-lowers without a diagnostic: {:?}",
        relowered.diagnostics(),
    );
}

/// The round-trip witness for a *constructed* program (§10): one that never came from text, so
/// it can carry a value the raise itself never makes (a negative `Number`).
fn constructed_round_trips(program: &Program, dialect: Dialect) {
    let rendered = render(program, dialect).expect("the program renders");
    let source = Source::new(SourceId::new(0), rendered.clone()).expect("the rendering admits");
    let reparsed = raise(&parse(&source, dialect));
    assert!(
        reparsed.diagnostics().is_empty(),
        "the rendering `{rendered}` reparses cleanly: {:?}",
        reparsed.diagnostics(),
    );
    assert_eq!(
        program,
        reparsed.program(),
        "a constructed program round-trips (rendered `{rendered}`)",
    );
}

#[test]
fn a_constructed_negative_number_round_trips() {
    // A negative integer reaches a program only by construction — the raise reads `-5` as unary
    // minus of 5. render writes `-5`, and the reparse canonicalizes it back to `Number(-5)` (the
    // §5.1 numeral fold); before that fold the reparse was `UnaryOp(Negate, 5)`, and this failed.
    use themelios_program::program::{Atom, Rule, Statement};
    use themelios_program::provenance::WithProvenance;
    use themelios_program::symbol::Name;
    use themelios_program::term::Term;

    let predicate = Name::new("p").expect("a valid identifier");
    let f = Name::new("f").expect("a valid identifier");
    for value in [-5_i32, -1, i32::MIN + 1] {
        // Flat `p(-5)` and nested-and-mixed `p(f(-5), -5)`.
        let flat = Atom::new(predicate.clone(), [Term::from(value)]);
        let nested = Atom::new(
            predicate.clone(),
            [
                Term::Function {
                    name: f.clone(),
                    arguments: vec![Term::from(value)],
                },
                Term::from(value),
            ],
        );
        for atom in [flat, nested] {
            let program = Program::of([WithProvenance::constructed(Statement::Rule(Rule::fact(
                atom,
            )))]);
            constructed_round_trips(&program, Dialect::Clingo);
        }
    }
}

// ---- the round-trip law, over generated programs ----

/// The breadth of ordinary (theory-free, optimization-distinct) programs the estate parser
/// round-trips exactly — one representative of every statement, head, body, aggregate, and
/// directive shape §4 gives.
const GENERATED: &[&str] = &[
    // Facts, rules, constraints, the empty constraint.
    "p.\n",
    "-p(a).\n",
    "p(1, 2, 3).\n",
    "q(X) :- p(X).\n",
    "q(X) :- p(X), r(X).\n",
    ":- p(X), q(X).\n",
    ":-.\n",
    "#true :- p(X).\n",
    // Comparisons, chained, and default negation single and double.
    "q(X) :- 1 < X, X < 9.\n",
    "q(X) :- X = 5, X != 6.\n",
    "q(X) :- not p(X).\n",
    "q(X) :- not not p(X).\n",
    // Arithmetic, intervals, tuples, pools, absolute value, external calls.
    "p(X + 1) :- q(X).\n",
    "p(X - Y * 2) :- q(X), q(Y).\n",
    "p(1 .. 3).\n",
    "p((a, b)).\n",
    "p((a,)).\n",
    "p(()).\n",
    "p((a; b)) :- q(a), q(b).\n",
    "p(X) :- q(X), X = |Y|, r(Y).\n",
    "p(X) :- X = @f(1, 2).\n",
    "p(X) :- X = @g.\n",
    // Disjunctive and choice heads, singleton conditioned heads.
    "a | b :- c.\n",
    "a(X) | b(X) :- p(X).\n",
    "p(X) : q(X) :- r(X).\n",
    "1 { a(X) : b(X) } 2 :- p(X).\n",
    "{ a; b }.\n",
    // Body aggregates: count, sum, set, guards on either side, conditions.
    "q :- #count { X : p(X) } >= 1.\n",
    "q(S) :- S = #sum { W,T : task(T), weight(T, W) }.\n",
    "q :- 3 { p(X) : r(X) } 5.\n",
    "q :- not #sum { X : p(X) } >= 0.\n",
    // Head aggregate.
    "2 #sum { X : p(X) } 5 :- q(X).\n",
    // Weak constraints and optimization.
    ":~ p(X). [X@1, X]\n",
    ":~ p(X), q(Y). [1@2]\n",
    "#minimize { X@1, X : p(X) }.\n",
    "#maximize { X : p(X) }.\n",
    // The body-bearing and body-free directives.
    "#show.\n",
    "#show p/1.\n",
    "#show -q/2.\n",
    "#show f(X) : g(X).\n",
    "#project q/2.\n",
    "#project p(X) : q(X).\n",
    "#defined d/1.\n",
    "#edge (a, b).\n",
    "#edge (a, b; c, d) : e(X).\n",
    "#heuristic h(X) : c(X). [X@1, true]\n",
    "#external e(X) : c(X).\n",
    "#external e(X). [true]\n",
    "#const c = 42.\n",
    "#const c = (1 + 2). [default]\n",
    "#include \"lib.lp\".\n",
    "#include <incmode>.\n",
];

#[test]
fn generated_programs_round_trip_under_the_clingo_dialect() {
    for text in GENERATED {
        round_trips(text, Dialect::Clingo);
    }
}

#[test]
fn a_program_of_many_statements_round_trips_as_a_whole() {
    round_trips(
        "diss(X) | dist(X) :- p(X).\n\
         q(X) :- p(X), X < 9, cnd(X) : cndg(X), #count { X : ct(X) } >= 1.\n\
         1 { cha(X) : chb(X) } 4 :- p(X).\n\
         2 #sum { X : hs(X) } 5 :- p(X).\n\
         :- p(X).\n\
         :~ p(X). [X@1, X]\n\
         #minimize { X@1, X : mn(X) }.\n\
         #show shf(X) : shg(X).\n\
         #project pr(X) : ps(X).\n\
         #edge (ea, eb) : ec(X).\n\
         #heuristic he(X) : hc(X). [X@1, true]\n\
         #external ex(X) : ecx(X).\n\
         #const co = (1 + 2).\n",
        Dialect::Clingo,
    );
}

#[test]
fn a_part_structured_program_round_trips_with_its_program_headers() {
    round_trips("p.\n#program acid(k).\nq(k).\n", Dialect::Clingo);
    round_trips("#program step(t).\nq(t).\n", Dialect::Clingo);
    round_trips("#program acid.\nr.\n", Dialect::Clingo);
}

// ---- the ASP-Core-2 dialect: the query, and the string rule that spells everything ----

#[test]
fn asp_core_2_programs_round_trip_including_the_query() {
    round_trips("p.\nq(1)?\n", Dialect::AspCore2);
    round_trips("p(X) :- q(X).\nr(a)?\n", Dialect::AspCore2);
}

#[test]
fn strings_round_trip_under_both_dialects() {
    round_trips("p(\"abc\").\n", Dialect::Clingo);
    round_trips("p(\"a b c\").\n", Dialect::AspCore2);
    // The ASP-Core-2 rule spells a backslash, which the clingo rule reads as an escape lead.
    round_trips("p(\"a\\b\").\n", Dialect::AspCore2);
}

// ---- the theory carve-out: the reparse is up-to-grounding (§5) ----

#[test]
fn theory_bearing_programs_render_and_reparse_cleanly_up_to_grounding() {
    // A built (grouped) theory term and a raised (flat) theory term reconcile only under a
    // `#theory` definition, so a theory program's exact identity is the grounding tier's; the
    // law here is the weakened one — the rendering reparses and re-lowers without error.
    renders_and_reparses_cleanly(
        "th(X) :- &sum(X) { X : t(X) } >= 0, p(X).\n",
        Dialect::Clingo,
    );
    renders_and_reparses_cleanly("&sum { X : t(X) } >= 0 :- p(X).\n", Dialect::Clingo);
    renders_and_reparses_cleanly("#theory t { }.\n", Dialect::Clingo);
}

// ---- the vendored corpus: the authority's own programs (spec §10.3) ----

/// The syntax tier's vendored corpus directory (spec §10.3) — re-read by path.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../themelios-syntax/tests/corpus")
}

/// Every `.lp` file under a directory, recursively — the authority's own corpus (spec §10.3).
fn corpus_programs(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            corpus_programs(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("lp") {
            out.push(path);
        }
    }
}

/// Every clean member of the authority's own vendored corpus round-trips exactly (§10.3, §16).
/// A file that is not a member under the clingo dialect (a syntax error, a deliberate
/// non-member of the error corpus) is skipped, as is one whose raise reports a diagnostic (a
/// theory program, up-to-grounding per §5, or a form the value cannot represent) — the two
/// named exceptions and the error corpus. Every remaining member — the great majority — is
/// rendered and reparsed to the same program.
#[test]
fn the_authoritys_own_corpus_round_trips() {
    let mut files = Vec::new();
    corpus_programs(&corpus_dir(), &mut files);
    files.sort();
    let mut members = 0_usize;
    for path in &files {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("corpus input `{}` reads: {error}", path.display()));
        let source = Source::new(SourceId::new(0), text.clone()).expect("a corpus input admits");
        let parsed = parse(&source, Dialect::Clingo);
        if parsed.has_errors() {
            continue; // not a member under this dialect, or a deliberate non-member.
        }
        let program = raise(&parsed);
        if !program.diagnostics().is_empty() {
            continue; // a theory program (§5) or a form the value cannot represent — the exceptions.
        }
        let rendered = render(program.program(), Dialect::Clingo)
            .unwrap_or_else(|refusal| panic!("`{}` renders: {refusal}", path.display()));
        let reparsed_source =
            Source::new(SourceId::new(0), rendered.clone()).expect("the rendering admits");
        let reparsed = raise(&parse(&reparsed_source, Dialect::Clingo));
        assert!(
            reparsed.diagnostics().is_empty(),
            "the rendering of `{}` reparses cleanly: {:?}",
            path.display(),
            reparsed.diagnostics(),
        );
        assert_eq!(
            program.program(),
            reparsed.program(),
            "round-trip up to provenance for corpus member `{}`",
            path.display(),
        );
        members += 1;
    }
    assert!(
        members > 100,
        "the vendored corpus is present and its members round-trip (found {members})",
    );
}

// ---- the round-trip witness, run as a test so the suite exercises it ----

#[test]
fn the_round_trip_witness_holds() {
    // The runnable demonstrator (examples/round_trip.rs) as a law: construct, render, reparse,
    // and raise, then assert equal.
    let program = raised(
        "reachable(Y) :- reachable(X), edge(X, Y).\nreachable(a).\n",
        Dialect::Clingo,
    );
    let rendered = render(&program, Dialect::Clingo).expect("renders");
    let source = Source::new(SourceId::new(0), rendered).expect("the rendering admits");
    let reparsed = raise(&parse(&source, Dialect::Clingo));
    assert_eq!(&program, reparsed.program());
}
