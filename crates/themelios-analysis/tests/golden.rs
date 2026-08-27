//! The analysis golden corpus (docs/design/analysis.md §10), reviewed. The inputs
//! are the syntax tier's vendored corpus (spec §10.3), re-read by path — no new
//! corpus is vendored — raised through the program tier; the snapshots under
//! `tests/golden/analysis/` are the reviewed artifacts, so a change in a program's
//! classification shape shows as a diff a maintainer reads. Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-analysis --test golden`, then review the
//! diff.
//!
//! The dump is a **view function** (§8: the reviewed dumps read a derived Debug *or a
//! view function*), not the derived `Debug`: `Analysis` is provenance-blind for
//! equality (§8), but the derived `Debug` of a witness carries the `Provenance` and
//! `Annotations` structure at every node — noise that buries the classification the
//! golden is meant to review. This view reads the facts the analysis reports —
//! signatures, edges, components, verdicts — and renders each witness rule through the
//! program tier's canonical renderer (§10), so the artifact is provenance-blind and
//! diffable. It is a test-local view; the crate itself keeps its no-`Display` posture.

use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use themelios_analysis::analysis::Analysis;
use themelios_analysis::classify::{HornKind, Normality, Stratification, Verdict};
use themelios_analysis::depend::{Component, Signature};
use themelios_base::source::{Source, SourceId};
use themelios_program::program::{Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::Sign;
use themelios_program::term::Variable;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

// ---- the provenance-blind view (§8, §10) ----

fn signature(signature: &Signature) -> String {
    let sign = match signature.sign {
        Sign::Positive => "",
        Sign::Negative => "-",
    };
    format!("{sign}{}/{}", signature.name.as_str(), signature.arity)
}

fn component(component: &Component) -> String {
    let members: Vec<String> = component.members().map(signature).collect();
    let mut kinds = Vec::new();
    if component.has_positive_cycle() {
        kinds.push("positive");
    }
    if component.has_negative_cycle() {
        kinds.push("negative");
    }
    if component.has_aggregate_cycle() {
        kinds.push("aggregate");
    }
    let cycle = if kinds.is_empty() {
        String::new()
    } else {
        format!(" [{}]", kinds.join(", "))
    };
    format!("{{{}}}{cycle}", members.join(", "))
}

fn verdict(verdict: &Verdict) -> String {
    match verdict {
        Verdict::Holds => "Holds".to_owned(),
        Verdict::Unknown { witness } => format!("Unknown {}", component(witness)),
    }
}

fn stratification(stratification: &Stratification) -> String {
    match stratification {
        Stratification::Stratified => "Stratified".to_owned(),
        Stratification::NotStratified { cycle } => format!("NotStratified {}", component(cycle)),
    }
}

fn variable(variable: &Variable) -> String {
    match variable {
        Variable::Named(name) => name.as_str().to_owned(),
        Variable::Anonymous => "_".to_owned(),
    }
}

/// A single statement rendered through the program tier's canonical renderer (§10),
/// provenance-blind. The witness is placed alone in a program and rendered; a value
/// the dialect cannot spell (§11 of the program design) is named rather than panicked.
fn render_statement(statement: Statement) -> String {
    let program = Program::of([WithProvenance::constructed(statement)]);
    render(&program, Dialect::Clingo)
        .unwrap_or_else(|refusal| format!("<unspellable: {refusal}>"))
        .trim_end()
        .to_owned()
}

fn render_rule(rule: &Rule) -> String {
    render_statement(Statement::Rule(rule.clone()))
}

/// The whole analysis rendered as a provenance-blind, diffable artifact (§10): the
/// constructs with their witness statements, the dependency graph and its components,
/// safety and finiteness, and the program classes with their witnesses.
fn view(analysis: &Analysis) -> String {
    let mut out = String::new();

    out.push_str("constructs:\n");
    for (construct, witness) in analysis.constructs().all() {
        writeln!(
            out,
            "  {construct:?}: {}",
            render_statement(witness.get().clone())
        )
        .expect("writing to a String never fails");
    }

    out.push_str("dependencies:\n");
    let predicates: Vec<String> = analysis
        .dependencies()
        .predicates()
        .map(signature)
        .collect();
    writeln!(out, "  predicates: {}", predicates.join(", "))
        .expect("writing to a String never fails");
    out.push_str("  edges:\n");
    for from in analysis.dependencies().predicates() {
        for (kind, to) in analysis.dependencies().edges_from(from) {
            writeln!(
                out,
                "    {} -[{kind:?}]-> {}",
                signature(from),
                signature(to)
            )
            .expect("writing to a String never fails");
        }
    }
    out.push_str("  components (reverse-topological):\n");
    for scc in analysis.dependencies().components() {
        writeln!(out, "    {}", component(scc)).expect("writing to a String never fails");
    }

    out.push_str("safety:\n");
    writeln!(out, "  safe: {}", analysis.safety().is_safe())
        .expect("writing to a String never fails");
    for unsafe_rule in analysis.safety().unsafe_rules() {
        let unbound: Vec<String> = unsafe_rule.unbound().map(variable).collect();
        writeln!(
            out,
            "  unsafe: {} [unbound: {}]",
            render_rule(unsafe_rule.rule()),
            unbound.join(", ")
        )
        .expect("writing to a String never fails");
    }
    writeln!(
        out,
        "  finiteness: {}",
        verdict(analysis.safety().finiteness())
    )
    .expect("writing to a String never fails");

    out.push_str("classes:\n");
    writeln!(
        out,
        "  tightness: {}",
        verdict(&analysis.classes().tightness())
    )
    .expect("writing to a String never fails");
    writeln!(
        out,
        "  head_cycle_free: {}",
        verdict(&analysis.classes().head_cycle_free())
    )
    .expect("writing to a String never fails");
    writeln!(
        out,
        "  stratification: {}",
        stratification(analysis.classes().stratification())
    )
    .expect("writing to a String never fails");
    let normality = match analysis.classes().normality() {
        Normality::Normal => "Normal".to_owned(),
        Normality::NotNormal { rule } => format!("NotNormal {}", render_rule(&rule)),
    };
    writeln!(out, "  normality: {normality}").expect("writing to a String never fails");
    let horn = match analysis.classes().horn() {
        HornKind::Horn => "Horn".to_owned(),
        HornKind::NotHorn { reason } => format!("NotHorn {}", render_rule(&reason)),
    };
    writeln!(out, "  horn: {horn}").expect("writing to a String never fails");
    writeln!(
        out,
        "  uses_disjunction: {}",
        analysis.classes().uses_disjunction()
    )
    .expect("writing to a String never fails");
    writeln!(out, "  uses_choice: {}", analysis.classes().uses_choice())
        .expect("writing to a String never fails");
    let confirmed: Vec<String> = analysis
        .classes()
        .confirmed()
        .map(|class| format!("{class:?}"))
        .collect();
    writeln!(out, "  confirmed: {}", confirmed.join(", "))
        .expect("writing to a String never fails");

    out
}

// ---- the corpus harness (mirrors the program tier's golden) ----

/// The syntax tier's vendored corpus directory (spec §10.3) — re-read by path.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../themelios-syntax/tests/corpus")
}

/// The analysis of a corpus program, as the reviewed view (§10).
fn analysis_view(text: &str) -> String {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("a corpus input admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    view(&Analysis::of(raised.program()))
}

/// Compare a rendered view to its reviewed snapshot under `subdirectory`, or rewrite it
/// under the bless toggle — the reviewed-artifact discipline the program tier's golden
/// corpus keeps.
fn check(subdirectory: &str, name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(subdirectory)
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a golden parent directory"))
            .expect("the golden directory is writable");
        fs::write(&path, actual).expect("the golden file writes");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; bless it and review the analysis",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "the analysis diverged from the reviewed golden `{name}`"
    );
}

/// A spread of the authority's own programs whose analyses are reviewed snapshots (§10):
/// a simple external directive, the arithmetic of numbers, a choice-and-interval subset,
/// and a rich saturation encoding with disjunction, negation, and deep recursion.
const INPUTS: &[(&str, &str)] = &[
    ("clingo/app/clingo/tests/lp/external.lp", "external"),
    ("clingo/app/clingo/tests/lp/numbers.lp", "numbers"),
    ("clingo/app/clingo/tests/lp/subset.lp", "subset"),
    ("clingo/app/clingo/tests/lp/aggregates.lp", "aggregates"),
];

#[test]
fn the_authoritys_programs_have_reviewed_analyses() {
    for (relative, name) in INPUTS {
        let text = fs::read_to_string(corpus_dir().join(relative))
            .unwrap_or_else(|error| panic!("corpus input `{relative}` reads: {error}"));
        check("analysis", name, &analysis_view(&text));
    }
}

/// A malformed parse raises to a partial program (program §8); the analysis reads it
/// without panic — totality is over the value, not a well-formedness precondition (§8).
#[test]
fn the_analysis_is_total_on_recovered_programs() {
    const SEEDS: &[&str] = &[
        "seeds/clingo/numeral-overflow-unpinned.lp",
        "seeds/clingo/empty-aggregate-elements-in-head.lp",
    ];
    for relative in SEEDS {
        let text = fs::read_to_string(corpus_dir().join(relative))
            .unwrap_or_else(|error| panic!("seed `{relative}` reads: {error}"));
        let source = Source::new(SourceId::new(0), text).expect("a seed admits");
        let raised = raise(&parse(&source, Dialect::Clingo));
        let analysis = Analysis::of(raised.program());
        let _constructs = analysis.constructs().all().count();
        let _components = analysis.dependencies().components().count();
        let _safe = analysis.safety().is_safe();
        assert!(analysis.classes().confirmed().count() <= 7);
    }
}
