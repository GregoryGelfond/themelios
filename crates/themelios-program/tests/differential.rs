//! The program-tier differential against the pinned authority (docs/design/program.md
//! §16; docs/grammar.md §3): the rendered text parsed by clingo 5.8.2 — the
//! *independent* oracle the round-trip law (§10) needs, since a renderer and this
//! estate's own parser sharing a misreading satisfy the reparse law while both wrong,
//! and the authority does not share it — together with `evaluate` against the
//! authority's ground arithmetic, `Symbol` order against its printing order, canonical
//! equality against its parse-then-unparse, and the `i32` number width at the
//! boundaries. Five independent-oracle checks, each with its named boundaries.
//!
//! Feature-gated and out of band: run through pixi, `pixi run differential-program`.
//! What it proves: agreement with the authority on the generated cases and the
//! vendored corpus. What it cannot (spec §10.2): agreement beyond them, nor any
//! universal law — those are the property laws (round_trip_laws, unify_laws,
//! symbol_laws). Every case is an asserted agreement or a boundary characterized *directly* as this
//! tier's refusal (refuse-over-repair, §3.5): it refuses overflow, a zero divisor or modulus, and a
//! negative exponent, where the authority's own behaviour is platform-dependent (the same clingo may
//! wrap, define, or abort the process on a different build), so those are never fed to the authority.

#![cfg(feature = "differential")]

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

use themelios_base::source::{Source, SourceId};
use themelios_program::program::{Program, Statement};
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_program::term::{BinaryOp, EvalError, Term, UnaryOp};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// The pin (docs/grammar.md §3).
const AUTHORITY_VERSION: &str = "5.8.2";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn authority_py() -> PathBuf {
    manifest_dir().join("tests/differential/authority.py")
}

/// The syntax tier's vendored corpus (spec §10.3), re-read by path — no new corpus.
fn corpus_dir() -> PathBuf {
    manifest_dir().join("../themelios-syntax/tests/corpus")
}

// ---- driving the authority ----

/// Spawn the authority helper in `mode` on `stdin` from `cwd` and wait for its full
/// output. A closed stdin pipe means the helper exited before reading it — clingo not
/// importable, say — so the write is not asserted: the caller reads such a failure
/// from the process's own exit status and stderr.
fn run_authority(mode: &str, stdin: &str, cwd: &Path) -> Output {
    let mut child = Command::new("python")
        .arg(authority_py())
        .arg(mode)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python runs: run this harness through `pixi run differential-program`");
    let _ = child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin.as_bytes());
    child.wait_with_output().expect("the authority answers")
}

/// The parsed JSON of an authority run in `mode`, its version pin asserted.
fn authority(mode: &str, stdin: &str, cwd: &Path) -> Value {
    let output = run_authority(mode, stdin, cwd);
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
    value
}

/// The authority's parse of `program` from `cwd`: acceptance, an unresolved include, and
/// the statement kinds it built, less comments (this grammar holds them as trivia, §4.1)
/// and the leading implicit `#program base.`, so the kinds are the real statements'.
struct Parsed {
    accepted: bool,
    include_failed: bool,
    kinds: Vec<String>,
    texts: Vec<String>,
}

fn authority_parse(program: &str, cwd: &Path) -> Parsed {
    let reading = authority("parse", program, cwd);
    let accepted = reading["accepted"].as_bool().unwrap_or(false);
    let include_failed = reading["include_failed"].as_bool().unwrap_or(false);
    let mut statements: Vec<(String, String)> = reading["statements"]
        .as_array()
        .map(|array| {
            array
                .iter()
                .map(|s| {
                    (
                        s["type"].as_str().unwrap_or("").to_owned(),
                        s["text"].as_str().unwrap_or("").to_owned(),
                    )
                })
                .filter(|(kind, _)| kind != "Comment")
                .collect()
        })
        .unwrap_or_default();
    if statements
        .first()
        .is_some_and(|(kind, text)| kind == "Program" && text == "#program base.")
    {
        statements.remove(0);
    }
    Parsed {
        accepted,
        include_failed,
        kinds: statements.iter().map(|(kind, _)| kind.clone()).collect(),
        texts: statements.iter().map(|(_, text)| text.clone()).collect(),
    }
}

/// A statement listing as a multiset — sorted, since this tier renders a program's
/// statements in its own canonical order (§4, the program is a set), not the source's,
/// so the authority reads the rendering's statements in a different sequence.
fn multiset(mut listing: Vec<String>) -> Vec<String> {
    listing.sort();
    listing
}

/// The distinct statement kinds present — a set, since this tier's program is a *set* of
/// statements (§5.2): a duplicated source statement collapses, so the authority reads one
/// more of it in the source than in the rendering. What the rendering must preserve, and
/// this independent oracle checks, is the *kinds* the authority reads; that no statement
/// is dropped or duplicated is the round-trip law's, against this estate's own parser.
fn kinds_present(kinds: &[String]) -> BTreeSet<String> {
    kinds.iter().cloned().collect()
}

/// The authority's evaluation of each ground-term spelling, in order (§3.5).
fn authority_eval(terms: &[String]) -> Vec<Value> {
    let reading = authority(
        "eval",
        &json!({ "terms": terms }).to_string(),
        &corpus_dir(),
    );
    reading["results"]
        .as_array()
        .expect("results is an array")
        .clone()
}

/// The authority's sort of the ground-symbol spellings, and its own printing of each in
/// input order (§3.1).
fn authority_order(symbols: &[String]) -> (Vec<String>, Vec<String>) {
    let reading = authority(
        "order",
        &json!({ "symbols": symbols }).to_string(),
        &corpus_dir(),
    );
    let strings = |key: &str| -> Vec<String> {
        reading[key]
            .as_array()
            .expect("an array")
            .iter()
            .map(|value| value.as_str().expect("a string").to_owned())
            .collect()
    };
    (strings("sorted"), strings("printed"))
}

// ---- spelling a symbol and a term as the authority reads them ----

fn sign_prefix(sign: Sign) -> &'static str {
    match sign {
        Sign::Positive => "",
        Sign::Negative => "-",
    }
}

/// A string as the authority prints one: the quotes and the two escapes it shows.
fn spell_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn commas(spellings: impl Iterator<Item = String>) -> String {
    spellings.collect::<Vec<_>>().join(",")
}

/// A ground symbol as the authority spells it (§3.1), verified against the authority's
/// own printing by the order check's round-trip assertion.
fn spell_symbol(symbol: &Symbol) -> String {
    match symbol {
        Symbol::Infimum => "#inf".to_owned(),
        Symbol::Supremum => "#sup".to_owned(),
        Symbol::Number(value) => value.to_string(),
        Symbol::String(text) => spell_string(text),
        Symbol::Function {
            name,
            arguments,
            sign,
        } if arguments.is_empty() => format!("{}{}", sign_prefix(*sign), name.as_str()),
        Symbol::Function {
            name,
            arguments,
            sign,
        } => format!(
            "{}{}({})",
            sign_prefix(*sign),
            name.as_str(),
            commas(arguments.iter().map(spell_symbol))
        ),
        Symbol::Tuple(items) if items.is_empty() => "()".to_owned(),
        Symbol::Tuple(items) if items.len() == 1 => format!("({},)", spell_symbol(&items[0])),
        Symbol::Tuple(items) => format!("({})", commas(items.iter().map(spell_symbol))),
    }
}

fn binary_operator(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "\\",
        BinaryOp::Pow => "**",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "?",
        BinaryOp::BitXor => "^",
    }
}

/// A ground term as an authority-parseable spelling — every operation fully
/// parenthesized, so the authority evaluates the very expression this term denotes
/// with no precedence to reconcile (§3.5). Only the ground evaluable fragment is
/// spelled here; a variable, pool, interval, or external is not generated for `eval`.
fn spell_term(term: &Term) -> String {
    match term {
        // A negative number leaf is parenthesized so it never abuts an operator
        // (`2 ** -1`, `- -1`); a non-negative one and every other symbol spell plainly.
        Term::Symbolic(Symbol::Number(value)) if *value < 0 => format!("({value})"),
        Term::Symbolic(symbol) => spell_symbol(symbol),
        Term::Function { name, arguments } if arguments.is_empty() => name.as_str().to_owned(),
        Term::Function { name, arguments } => format!(
            "{}({})",
            name.as_str(),
            commas(arguments.iter().map(spell_term))
        ),
        Term::Tuple(items) if items.is_empty() => "()".to_owned(),
        Term::Tuple(items) if items.len() == 1 => format!("({},)", spell_term(&items[0])),
        Term::Tuple(items) => format!("({})", commas(items.iter().map(spell_term))),
        Term::UnaryOperation { operator, argument } => {
            let operator = match operator {
                UnaryOp::Negate => "-",
                UnaryOp::BitwiseNot => "~",
            };
            format!("({operator}{})", spell_term(argument))
        }
        Term::BinaryOperation {
            operator,
            left,
            right,
        } => format!(
            "({}{}{})",
            spell_term(left),
            binary_operator(*operator),
            spell_term(right)
        ),
        Term::Absolute(inner) => format!("|{}|", spell_term(inner)),
        other => unreachable!("the eval fragment is ground and evaluable, got {other:?}"),
    }
}

// ---- building programs from concrete syntax ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// Raise a program from concrete syntax under a dialect, asserting it lowers cleanly —
/// the same door round_trip_laws uses.
fn raised(text: &str, dialect: Dialect) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, dialect));
    assert!(
        lowered.diagnostics().is_empty(),
        "`{text}` raises cleanly under {dialect}: {:?}",
        lowered.diagnostics(),
    );
    lowered.into_program()
}

// ---- a deterministic, dependency-free generator (SplitMix64) ----

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

// ---- check 1: the rendered text against the authority (§16, §10) ----

/// One representative of every ordinary (theory-free) statement, head, body, aggregate,
/// and directive shape — round_trip_laws's `GENERATED`, less the includes (which the
/// authority would resolve from a working directory these inline programs have no file
/// in) and the part headers (which the program value holds structurally, not as
/// statements it iterates).
const GENERATED: &[&str] = &[
    "p.\n",
    "-p(a).\n",
    "p(1, 2, 3).\n",
    "q(X) :- p(X).\n",
    "q(X) :- p(X), r(X).\n",
    ":- p(X), q(X).\n",
    ":-.\n",
    "#true :- p(X).\n",
    "q(X) :- 1 < X, X < 9.\n",
    "q(X) :- X = 5, X != 6.\n",
    "q(X) :- not p(X).\n",
    "q(X) :- not not p(X).\n",
    "p(X + 1) :- q(X).\n",
    "p(X - Y * 2) :- q(X), q(Y).\n",
    "p(1 .. 3).\n",
    "p((a, b)).\n",
    "p((a,)).\n",
    "p(()).\n",
    "p((a; b)) :- q(a), q(b).\n",
    "p(a; b).\n",
    "q(X) :- p(X; a), r(X).\n",
    "{ p(a; b) }.\n",
    "#external p(a; b).\n",
    "#project p(a; b).\n",
    "p(X) :- q(X), X = |Y|, r(Y).\n",
    "a | b :- c.\n",
    "a(X) | b(X) :- p(X).\n",
    "p(X) : q(X) :- r(X).\n",
    "1 { a(X) : b(X) } 2 :- p(X).\n",
    "{ a; b }.\n",
    "q :- #count { X : p(X) } >= 1.\n",
    "q(S) :- S = #sum { W,T : task(T), weight(T, W) }.\n",
    "q :- 3 { p(X) : r(X) } 5.\n",
    "q :- not #sum { X : p(X) } >= 0.\n",
    "2 #sum { X : p(X) } 5 :- q(X).\n",
    ":~ p(X). [X@1, X]\n",
    ":~ p(X), q(Y). [1@2]\n",
    "#minimize { X@1, X : p(X) }.\n",
    "#maximize { X : p(X) }.\n",
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
];

/// The authority accepts `render(P)` and reads it as the same statements it reads the
/// original as (§16). Comparing the authority's reading of the rendering against its
/// reading of the source — kind for kind — is the independent structural check: were
/// this estate's renderer and parser to share a misreading, the reparse law would still
/// hold while the authority's two readings diverged. The two named exceptions (the
/// empty-aggregate non-injectivity, the theory carve-out) never reach here: theory
/// programs raise with a diagnostic and are skipped, and no generated case is the
/// empty-aggregate pair.
#[test]
fn the_rendered_text_is_the_authoritys_program() {
    let mut compared = 0usize;
    for text in GENERATED {
        let program = raised(text, Dialect::Clingo);
        let rendered = render(&program, Dialect::Clingo).expect("the program renders");
        let source = authority_parse(text, &corpus_dir());
        let target = authority_parse(&rendered, &corpus_dir());
        assert!(source.accepted, "the authority accepts the source `{text}`");
        assert!(
            target.accepted,
            "the authority accepts the rendering `{rendered}` of `{text}`",
        );
        assert_eq!(
            multiset(source.kinds),
            multiset(target.kinds),
            "the authority reads `{rendered}` as different statements than `{text}`",
        );
        compared += 1;
    }
    assert!(
        compared > 40,
        "the generated shapes were compared ({compared})"
    );
}

/// Every clean member of the vendored corpus renders to text the authority accepts as
/// the same statements (§16, spec §10.3). A file that is not a member under the clingo
/// dialect, one whose raise reports a diagnostic (a theory program per §5, or a form the
/// value cannot hold), and one whose includes the authority cannot resolve are skipped —
/// the two named exceptions and the error corpus. Every remaining member is rendered and
/// read back by the authority, kind for kind.
#[test]
fn the_vendored_corpus_renders_to_the_authoritys_programs() {
    let mut files = Vec::new();
    let mut pending = vec![corpus_dir()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("the corpus reads") {
            let path = entry.expect("a corpus entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "lp") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut members = 0usize;
    for path in &files {
        let cwd = path.parent().expect("a corpus file has a directory");
        let text = fs::read_to_string(path).expect("a corpus input reads");
        let parsed = parse(
            &Source::new(SourceId::new(0), text.clone()).expect("admits"),
            Dialect::Clingo,
        );
        if parsed.has_errors() {
            continue; // not a member under this dialect, or a deliberate non-member.
        }
        let lowered = raise(&parsed);
        if !lowered.diagnostics().is_empty() {
            continue; // a theory program (§5) or a form the value cannot hold — the exceptions.
        }
        if lowered
            .program()
            .statements()
            .any(|statement| matches!(statement.get(), Statement::Include(_)))
        {
            // The authority resolves and inlines an `#include`; this tier holds it as a
            // statement (resolution is not this tier's, program §4). So the authority
            // reads the source with the include expanded and the rendering with it still
            // a directive — no render defect, an incomparable pair, skipped as the syntax
            // tier's differential skips its include-bearing inputs.
            continue;
        }
        let source = authority_parse(&text, cwd);
        if source.include_failed || !source.accepted {
            continue; // an include the authority cannot open, or a non-member to it.
        }
        if source.kinds.iter().any(|kind| kind == "Program") {
            // A `#program` part header the authority reads as a statement, which this
            // tier holds structurally (program §4.1 — a part is not a statement it
            // iterates), so it normalizes a redundant `#program base.` and orders the
            // parts by its own key. The authority accounts the headers one for one; this
            // tier does not. Render fidelity over parts is the round-trip law's, against
            // this estate's own parser; the authority check keeps to the single-part
            // fragment where the two account statements alike.
            continue;
        }
        let rendered = render(lowered.program(), Dialect::Clingo).expect("the member renders");
        let target = authority_parse(&rendered, cwd);
        assert!(
            target.accepted && !target.include_failed,
            "the authority accepts the rendering of `{}`:\n{rendered}",
            path.display(),
        );
        assert_eq!(
            kinds_present(&source.kinds),
            kinds_present(&target.kinds),
            "the authority reads the rendering of `{}` as different statement kinds",
            path.display(),
        );
        members += 1;
    }
    println!("{members} single-part corpus members render to the authority's programs");
    assert!(
        members > 100,
        "the vendored corpus is present and its members render to the authority's programs ({members})",
    );
}

/// On the order-insensitive fragment — facts, single-literal rules, directives whose
/// bodies this tier's set canonicalization never reorders — the authority's printing of
/// `render(P)` is *exactly* its printing of the source, term for term (§16). This is the
/// finer within-statement fidelity the kind comparison above does not reach: an operator
/// or an argument the renderer spelled wrong shows here, since the authority prints both
/// the same only when the rendering is the same program down to the term.
#[test]
fn the_rendered_text_prints_as_the_source_on_the_order_insensitive_fragment() {
    // Each has at most one body literal, so this tier's body-as-a-set canonicalization
    // (§4) reorders nothing, and the authority's order-preserving printing agrees exactly.
    const FRAGMENT: &[&str] = &[
        "p.\n",
        "-p(a).\n",
        "p(1, 2, 3).\n",
        "p(1 + 2).\n",
        "p(1 - 2 * 3).\n",
        "p(- 5) :- q.\n",
        "p(1 .. 3).\n",
        "p((a, b)).\n",
        "p((a,)).\n",
        "p(()).\n",
        "p(|Y|) :- r(Y).\n",
        "q(X) :- p(X).\n",
        "q(X) :- not p(X).\n",
        "q(X) :- X = 5.\n",
        ":- p(X).\n",
        "#show p/1.\n",
        "#show -q/2.\n",
        "#project q/2.\n",
        "#defined d/1.\n",
        "#edge (a, b).\n",
        "#external e(X) : c(X).\n",
        "#const c = 42.\n",
    ];
    for text in FRAGMENT {
        let program = raised(text, Dialect::Clingo);
        let rendered = render(&program, Dialect::Clingo).expect("renders");
        let source = authority_parse(text, &corpus_dir());
        let target = authority_parse(&rendered, &corpus_dir());
        assert_eq!(
            multiset(source.texts),
            multiset(target.texts),
            "the authority prints the rendering `{rendered}` of `{text}` differently",
        );
    }
}

// ---- check 2: `evaluate` against the authority's ground arithmetic (§3.5, §16) ----

const NUMBER_ALPHABET: [i32; 7] = [0, 1, 2, 3, 5, 7, 10];

/// A random ground arithmetic term over a small alphabet and the four-function operators
/// plus modulo: at a leaf a number, deeper a binary operation. The small alphabet makes
/// division and modulo by zero — where both this tier and the authority refuse — common,
/// and the agreements the overwhelming majority.
fn gen_arith(rng: &mut Rng, depth: u32) -> Term {
    if depth == 0 || rng.below(3) == 0 {
        let value = NUMBER_ALPHABET[rng.below(NUMBER_ALPHABET.len() as u64) as usize];
        return Term::from(value);
    }
    let operator = match rng.below(5) {
        0 => BinaryOp::Add,
        1 => BinaryOp::Sub,
        2 => BinaryOp::Mul,
        3 => BinaryOp::Div,
        _ => BinaryOp::Mod,
    };
    Term::BinaryOperation {
        operator,
        left: Box::new(gen_arith(rng, depth - 1)),
        right: Box::new(gen_arith(rng, depth - 1)),
    }
}

/// Assert this tier's evaluation of a ground term agrees with the authority's — both yield the same
/// symbol. The differential feeds this only terms this tier evaluates to a value (an overflow-free,
/// defined term), so the authority evaluates them without its platform-dependent arithmetic traps and
/// must agree. A refusal or a divergence here is a real defect, not a characterized boundary: this
/// tier's refusals (an overflow, a zero divisor or modulus, a negative exponent) are asserted directly,
/// never fed to the authority.
fn assert_agrees(term: &Term, theirs: &Value) {
    let symbol = term.evaluate().unwrap_or_else(|error| {
        panic!(
            "the differential feeds `assert_agrees` only accepted terms, but `{}` gave {error:?}",
            spell_term(term),
        )
    });
    assert!(
        theirs["ok"].as_bool().unwrap_or(false),
        "the authority refuses `{}`, which this tier evaluates ({theirs})",
        spell_term(term),
    );
    let their_symbol = theirs["symbol"].as_str().expect("a printed symbol");
    assert_eq!(
        spell_symbol(&symbol),
        their_symbol,
        "the authority evaluates `{}` to a different symbol",
        spell_term(term),
    );
}

#[test]
fn evaluate_agrees_with_the_authoritys_ground_arithmetic() {
    // themelios refuses overflow (§3.5); the authority's overflow is platform-dependent — the same
    // clingo 5.8.2 wraps it on osx-arm64 but *aborts the process* on linux-64 (a signed-overflow trap
    // the helper's `except` cannot catch), so an overflowing term is never fed to the authority.
    // themelios's own evaluation is the safe filter: a term it evaluates to a value overflows at no
    // step, so the authority evaluates it on every platform without the trap and the two must agree.
    // The refusal boundaries are characterized separately, below.
    let mut rng = Rng::new(0x1234_5678_9ABC_DEF0);
    let random: Vec<Term> = (0..4000).map(|_| gen_arith(&mut rng, 4)).collect();

    let accepted: Vec<&Term> = random
        .iter()
        .filter(|term| term.evaluate().is_ok())
        .collect();
    let spellings: Vec<String> = accepted.iter().copied().map(spell_term).collect();
    let results = authority_eval(&spellings);
    assert_eq!(
        results.len(),
        accepted.len(),
        "one result per accepted term"
    );
    for (term, theirs) in accepted.iter().copied().zip(&results) {
        assert_agrees(term, theirs);
    }
    assert!(
        accepted.len() > 1000,
        "faithful agreements were exercised ({})",
        accepted.len(),
    );
}

/// A binary-operation term over two integer literals — the arithmetic boundaries' building block.
fn binop(operator: BinaryOp, left: i32, right: i32) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(Term::from(left)),
        right: Box::new(Term::from(right)),
    }
}

#[test]
fn evaluate_characterizes_the_arithmetic_boundaries() {
    // The overflow boundaries: a themelios refusal only — the authority's overflow is platform-dependent
    // (it wraps on some builds and aborts the process on others), so it is not fed these.
    for term in [
        binop(BinaryOp::Add, i32::MAX, 1),
        binop(BinaryOp::Mul, 100_000, 100_000),
        binop(BinaryOp::Pow, 2, 31),
    ] {
        assert!(
            matches!(term.evaluate(), Err(EvalError::Overflow)),
            "themelios refuses the overflow `{}`",
            spell_term(&term),
        );
    }

    // The other arithmetic edge cases are this tier's refusals too, and the authority's behaviour on them
    // is likewise platform-dependent (a build may refuse, define, or abort — the same clingo defines
    // `5 \ 0` as `5` and evaluates `2 ** -1` to `0` on osx-arm64), so none is fed to the authority: the
    // refusal (§3.5) is the characterization.
    for term in [
        binop(BinaryOp::Pow, 2, -1), // a negative exponent
        binop(BinaryOp::Div, 5, 0),  // a zero divisor
        binop(BinaryOp::Mod, 5, 0),  // a zero modulus
    ] {
        assert!(
            matches!(term.evaluate(), Err(EvalError::Undefined)),
            "themelios refuses the undefined `{}`",
            spell_term(&term),
        );
    }

    // Faithful non-arithmetic ground terms — a function, a tuple, a negation, an absolute, two bitwise —
    // carry no arithmetic edge case, so the authority evaluates them on every platform, and agrees.
    let curated = [
        Term::Function {
            name: name("f"),
            arguments: vec![Term::from(1), Term::from(2)],
        },
        Term::Tuple(vec![Term::from(1), Term::from(2)]),
        Term::UnaryOperation {
            operator: UnaryOp::Negate,
            argument: Box::new(Term::from(5)),
        },
        Term::Absolute(Box::new(Term::from(-7))),
        binop(BinaryOp::BitAnd, 5, 3),
        binop(BinaryOp::BitXor, 5, 3),
    ];
    let spellings: Vec<String> = curated.iter().map(spell_term).collect();
    let results = authority_eval(&spellings);
    for (term, theirs) in curated.iter().zip(&results) {
        assert_agrees(term, theirs);
    }
}

// ---- check 3: `Symbol` order against the authority's printing order (§3.1, §16) ----

const SYMBOL_NAMES: [&str; 4] = ["a", "b", "f", "p"];
const SYMBOL_STRINGS: [&str; 3] = ["a", "s", "z"];
const SYMBOL_NUMBERS: [i32; 6] = [i32::MIN, -1, 0, 1, 42, i32::MAX];

fn gen_symbol(rng: &mut Rng, depth: u32) -> Symbol {
    let arms = if depth == 0 { 4 } else { 6 };
    match rng.below(arms) {
        0 => Symbol::Number(SYMBOL_NUMBERS[rng.below(SYMBOL_NUMBERS.len() as u64) as usize]),
        1 => Symbol::String(
            SYMBOL_STRINGS[rng.below(SYMBOL_STRINGS.len() as u64) as usize].to_owned(),
        ),
        2 => Symbol::Function {
            name: name(SYMBOL_NAMES[rng.below(SYMBOL_NAMES.len() as u64) as usize]),
            arguments: Vec::new(),
            sign: if rng.below(2) == 0 {
                Sign::Positive
            } else {
                Sign::Negative
            },
        },
        3 => {
            // Infimum and Supremum, the order's poles, sparingly.
            if rng.below(2) == 0 {
                Symbol::Infimum
            } else {
                Symbol::Supremum
            }
        }
        4 => {
            let arity = 1 + rng.below(2) as usize;
            Symbol::Function {
                name: name(SYMBOL_NAMES[rng.below(SYMBOL_NAMES.len() as u64) as usize]),
                arguments: (0..arity).map(|_| gen_symbol(rng, depth - 1)).collect(),
                sign: if rng.below(2) == 0 {
                    Sign::Positive
                } else {
                    Sign::Negative
                },
            }
        }
        _ => {
            let arity = rng.below(3) as usize;
            Symbol::Tuple((0..arity).map(|_| gen_symbol(rng, depth - 1)).collect())
        }
    }
}

#[test]
fn the_symbol_order_is_the_authoritys() {
    // A curated set across the order's stated boundaries (§3.1): the poles, the numbers,
    // the nullary function-likes that sort *before* a string, the strings, the
    // arity-bearing that sort *after*, the tuple as an anonymous function, and the sign.
    let curated = vec![
        Symbol::Infimum,
        Symbol::Supremum,
        Symbol::Number(i32::MIN),
        Symbol::Number(-1),
        Symbol::Number(0),
        Symbol::Number(i32::MAX),
        Symbol::String("a".to_owned()),
        Symbol::String("s".to_owned()),
        Symbol::Function {
            name: name("a"),
            arguments: vec![],
            sign: Sign::Positive,
        },
        Symbol::Function {
            name: name("b"),
            arguments: vec![],
            sign: Sign::Positive,
        },
        Symbol::Function {
            name: name("p"),
            arguments: vec![],
            sign: Sign::Negative,
        },
        Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        },
        Symbol::Function {
            name: name("p"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Negative,
        },
        Symbol::Tuple(vec![]),
        Symbol::Tuple(vec![Symbol::Number(1)]),
        Symbol::Tuple(vec![Symbol::Number(1), Symbol::Number(2)]),
    ];
    let mut rng = Rng::new(0x0FF1_CE0F_F1CE_0FF1);
    let generated: Vec<Symbol> = (0..2000).map(|_| gen_symbol(&mut rng, 3)).collect();

    let mut symbols: Vec<Symbol> = curated.into_iter().chain(generated).collect();
    symbols.sort(); // this tier's order (§3.1)
    symbols.dedup(); // distinct symbols, so the spellings are a bijection

    let spellings: Vec<String> = symbols.iter().map(spell_symbol).collect();
    let (sorted, printed) = authority_order(&spellings);

    // Every spelling this tier sent is already the authority's own printing — so
    // comparing the sorted spellings compares like against like (no drift in `spell_symbol`).
    for (spelling, print) in spellings.iter().zip(&printed) {
        assert_eq!(
            spelling, print,
            "`spell_symbol` printed a symbol the authority spells `{print}`",
        );
    }
    // The order this tier sorts by is the authority's order.
    assert_eq!(
        spellings, sorted,
        "this tier's `Symbol` order disagrees with the authority's",
    );
}

// ---- check 4: canonical equality against parse-then-unparse (§5.2, §16) ----

/// The authority's canonical reading of a program this tier rendered: the multiset of
/// its printed statements. Two of this tier's programs are equal to the authority exactly
/// when these multisets are equal — the arbiter §5.2 names, on the theory-free,
/// optimization-free fragment where the two coincide.
fn authority_canonical(program: &Program) -> BTreeSet<String> {
    let rendered = render(program, Dialect::Clingo).expect("the program renders");
    authority_parse(&rendered, &corpus_dir())
        .texts
        .into_iter()
        .collect()
}

#[test]
fn canonical_equality_agrees_with_the_authoritys_parse_then_unparse() {
    // Programs on the theory-free, optimization-free fragment. Within a group the members
    // are equal to this tier though spelled differently at the source (a reordered or
    // duplicated body, a `==` for a `=`, a reordered program); across groups they differ.
    // The optimization and theory carve-outs (§5.2), where this tier's equality is
    // deliberately finer, are excluded from the fragment by construction.
    let groups: Vec<Vec<&str>> = vec![
        vec!["p :- q, r.", "p :- r, q.", "p :- q, q, r.", "p :- r, q, r."],
        vec!["p :- X == 1.", "p :- X = 1."],
        vec!["a. b.", "b. a.", "a. b. a."],
        vec!["p :- q.", "p :- q. p :- q."],
        vec!["q(X) :- p(X), r(X).", "q(X) :- r(X), p(X)."],
        vec!["p :- r."],
        vec!["p :- q, s."],
        vec!["a."],
        vec!["p(1)."],
    ];
    // Every program, with the index of the group it belongs to.
    let mut programs: Vec<(usize, Program, BTreeSet<String>)> = Vec::new();
    for (group, members) in groups.iter().enumerate() {
        for text in members {
            let with_stop = format!("{text}\n");
            let program = raised(&with_stop, Dialect::Clingo);
            let canonical = authority_canonical(&program);
            programs.push((group, program, canonical));
        }
    }
    // For every pair: this tier's equality holds exactly when the authority's does. Within
    // a group both hold; across groups neither does — the arbiter, checked both ways.
    for (i, (group_i, program_i, canonical_i)) in programs.iter().enumerate() {
        for (group_j, program_j, canonical_j) in programs.iter().skip(i + 1) {
            let ours = program_i == program_j;
            let theirs = canonical_i == canonical_j;
            assert_eq!(
                ours, theirs,
                "the arbiter disagrees: here {ours}, authority {theirs}\n  a = {canonical_i:?}\n  b = {canonical_j:?}",
            );
            // Within a group, this tier's canonicalization has merged the spellings into
            // one rendered text; across groups it has not — the render is the witness.
            assert_eq!(
                group_i == group_j,
                ours,
                "the grouping and this tier's equality must agree",
            );
        }
    }
}

// ---- check 5: the `i32` number width at the boundaries (§3.1, §16) ----

#[test]
fn the_number_width_is_the_authoritys_at_the_i32_boundaries() {
    // At the boundaries this tier's `Symbol::Number(i32)` and the authority's number
    // width coincide; one step beyond, the authority wraps where this tier's value has no
    // representation at all (a numeral beyond `i32` is a diagnostic at the raise), so the
    // boundary is exact and recorded.
    let terms: Vec<String> = vec![
        spell_symbol(&Symbol::Number(i32::MAX)),
        spell_symbol(&Symbol::Number(i32::MIN)),
        spell_symbol(&Symbol::Number(0)),
        "2147483648".to_owned(), // i32::MAX + 1 — beyond the width.
        "4294967296".to_owned(), // 2^32 — two widths beyond.
    ];
    let results = authority_eval(&terms);
    let number = |i: usize| -> i64 {
        assert!(
            results[i]["ok"].as_bool().unwrap_or(false),
            "the authority reads {}",
            terms[i]
        );
        assert!(
            results[i]["is_number"].as_bool().unwrap_or(false),
            "{} is a number",
            terms[i]
        );
        results[i]["number"].as_i64().expect("an integer")
    };
    // The boundaries coincide.
    assert_eq!(
        number(0),
        i64::from(i32::MAX),
        "the authority's max is this tier's"
    );
    assert_eq!(
        number(1),
        i64::from(i32::MIN),
        "the authority's min is this tier's"
    );
    assert_eq!(number(2), 0);
    // One step beyond, the authority wraps into the width — the recorded boundary where
    // this tier's value type has no member (the raise reports a numeral overflow).
    assert_eq!(
        number(3),
        i64::from(i32::MIN),
        "the authority wraps `i32::MAX + 1`"
    );
    assert_eq!(number(4), 0, "the authority wraps `2^32`");
}

// ---- the harness's own spelling logic, held without the authority ----

/// `spell_symbol` prints each variant as the authority does (§3.1) — the fixed points the
/// order check's round-trip assertion confirms against the live authority.
#[test]
fn spell_symbol_prints_the_stated_forms() {
    assert_eq!(spell_symbol(&Symbol::Infimum), "#inf");
    assert_eq!(spell_symbol(&Symbol::Supremum), "#sup");
    assert_eq!(spell_symbol(&Symbol::Number(-1)), "-1");
    assert_eq!(spell_symbol(&Symbol::String("s".to_owned())), "\"s\"");
    assert_eq!(
        spell_symbol(&Symbol::String("a\"b".to_owned())),
        "\"a\\\"b\""
    );
    assert_eq!(
        spell_symbol(&Symbol::Function {
            name: name("a"),
            arguments: vec![],
            sign: Sign::Positive
        }),
        "a"
    );
    assert_eq!(
        spell_symbol(&Symbol::Function {
            name: name("p"),
            arguments: vec![],
            sign: Sign::Negative
        }),
        "-p"
    );
    assert_eq!(
        spell_symbol(&Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1), Symbol::Number(2)],
            sign: Sign::Positive,
        }),
        "f(1,2)"
    );
    assert_eq!(spell_symbol(&Symbol::Tuple(vec![])), "()");
    assert_eq!(
        spell_symbol(&Symbol::Tuple(vec![Symbol::Number(1)])),
        "(1,)"
    );
    assert_eq!(
        spell_symbol(&Symbol::Tuple(vec![Symbol::Number(1), Symbol::Number(2)])),
        "(1,2)"
    );
}

/// `spell_term` fully parenthesizes each operation (§3.5), so the authority evaluates the
/// very expression the term denotes.
#[test]
fn spell_term_parenthesizes_each_operation() {
    let term = Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(Term::from(1)),
        right: Box::new(Term::BinaryOperation {
            operator: BinaryOp::Mul,
            left: Box::new(Term::from(2)),
            right: Box::new(Term::from(3)),
        }),
    };
    assert_eq!(spell_term(&term), "(1+(2*3))");
    assert_eq!(
        spell_term(&Term::Absolute(Box::new(Term::from(-7)))),
        "|(-7)|"
    );
    assert_eq!(
        spell_term(&Term::UnaryOperation {
            operator: UnaryOp::BitwiseNot,
            argument: Box::new(Term::from(5)),
        }),
        "(~5)"
    );
}
