//! The differential (docs/design/syntax.md §16; docs/grammar.md §3):
//! every corpus input parsed here and by the pinned clingo — agreement
//! on membership and on statement kinds — the four printable canonical
//! spellings checked against the authority's printing, the authority's
//! nesting ceiling measured per family, and the tree-sitter cross-check.
//! Feature-gated and out of band: run through `pixi run differential`,
//! `pixi run measure-ceiling`, `pixi run cross-check`. What it proves:
//! agreement with the authority on the corpus and the seeds. What it
//! cannot: agreement beyond them.
#![cfg(feature = "differential")]

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::{self, Statement};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{Parse, parse};

/// The pin (docs/grammar.md §3).
const AUTHORITY_VERSION: &str = "5.8.2";
/// The pinned secondary grammar (docs/grammar.md §3).
const TREE_SITTER_CLINGO: &str = "58e062c1c6c2ac0bad54fee054573c5a9e6dd759";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_dir() -> PathBuf {
    manifest_dir().join("tests/corpus")
}

fn workspace_root() -> PathBuf {
    manifest_dir()
        .join("../..")
        .canonicalize()
        .expect("the workspace root resolves")
}

/// The authority's reading of `program`, run from `cwd`.
#[derive(Debug)]
struct Reading {
    accepted: bool,
    include_failed: bool,
    statements: Vec<(String, String)>,
}

/// Spawn the authority helper on `program` from `cwd` and wait for its
/// full output. A closed stdin pipe means the helper exited before
/// reading it — clingo not importable, say — so the write is not
/// asserted: the caller reads such a failure from the process's own exit
/// status and stderr, not from a `BrokenPipe` panic here.
fn run_authority(program: &str, cwd: &Path) -> Output {
    let mut child = Command::new("python")
        .arg(manifest_dir().join("tests/differential/authority.py"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python runs: run this harness through `pixi run differential`");
    let _ = child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(program.as_bytes());
    child.wait_with_output().expect("the authority answers")
}

fn authority(program: &str, cwd: &Path) -> Reading {
    let output = run_authority(program, cwd);
    assert!(
        output.status.success(),
        "the authority helper failed (is clingo's Python module installed? run through pixi):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reading: Value = serde_json::from_slice(&output.stdout).expect("the helper emits JSON");
    assert_eq!(
        reading["version"].as_str(),
        Some(AUTHORITY_VERSION),
        "docs/grammar.md §3: the authority is pinned at v{AUTHORITY_VERSION}"
    );
    Reading {
        accepted: reading["accepted"].as_bool().unwrap_or(false),
        include_failed: reading["include_failed"].as_bool().unwrap_or(false),
        statements: reading["statements"]
            .as_array()
            .map(|statements| {
                statements
                    .iter()
                    .map(|s| {
                        (
                            s["type"].as_str().unwrap_or("").to_owned(),
                            s["text"].as_str().unwrap_or("").to_owned(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Every corpus input, with the parser's own reading under the clingo
/// dialect — the language the authority reads.
fn inputs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![corpus_dir()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// A register file: `# comment` lines dropped, each line a path and a
/// note.
fn register(name: &str) -> BTreeSet<String> {
    fs::read_to_string(corpus_dir().join(name))
        .unwrap_or_default()
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.is_empty() && !line.starts_with('#')
        })
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(corpus_dir())
        .expect("under the corpus")
        .display()
        .to_string()
}

/// The authority's statement kinds a statement of ours corresponds to
/// (read from the pinned authority's AST at v5.8.2): a rule is one
/// `Rule`; a weak constraint one `Minimize`; an optimize statement one
/// `Minimize` per element; `#show` a `ShowSignature` for the bare and
/// signature forms and a `ShowTerm` for the term forms; `#project` a
/// `ProjectSignature` or a `ProjectAtom`; `#edge` one `Edge` per pair;
/// `#const` a `Definition`; `#program` a `Program`; the rest their own
/// names. `#include` is resolved and inlined by the authority, and the
/// ASP-Core-2 query is no statement of clingo's — both correspond to
/// nothing of the authority's.
fn corresponding(statement: &Statement) -> Vec<&'static str> {
    match statement {
        Statement::Rule(_) => vec!["Rule"],
        Statement::WeakConstraint(_) => vec!["Minimize"],
        Statement::Optimize(optimize) => vec!["Minimize"; optimize.elements().count()],
        Statement::Show(show) => {
            if show.term().is_some() {
                vec!["ShowTerm"]
            } else {
                vec!["ShowSignature"]
            }
        }
        Statement::Project(project) => {
            if project.atom().is_some() {
                vec!["ProjectAtom"]
            } else {
                vec!["ProjectSignature"]
            }
        }
        Statement::Defined(_) => vec!["Defined"],
        Statement::Edge(edge) => vec!["Edge"; edge.edges().count()],
        Statement::Heuristic(_) => vec!["Heuristic"],
        Statement::External(_) => vec!["External"],
        Statement::Const(_) => vec!["Definition"],
        Statement::Script(_) => vec!["Script"],
        Statement::ProgramPart(_) => vec!["Program"],
        Statement::TheoryDefinition(_) => vec!["TheoryDefinition"],
        Statement::Include(_) | Statement::Query(_) => Vec::new(),
    }
}

fn parse_here(text: &str) -> Parse<ast::Program> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, Dialect::Clingo)
}

#[test]
fn differential_membership_and_statement_kinds_agree_with_the_authority() {
    let skip = register("DIFFERENTIAL-SKIP");
    let known = register("AUTHORITY-DISAGREEMENTS");
    let mut disagreements = Vec::new();
    let mut compared = 0usize;
    for path in inputs() {
        let name = relative(&path);
        if skip.contains(&name) {
            continue;
        }
        let text = fs::read_to_string(&path).expect("input reads");
        let ours = parse_here(&text);
        let theirs = authority(&text, path.parent().expect("a directory"));
        if theirs.include_failed {
            continue;
        }
        compared += 1;
        let we_accept = !ours.has_errors();
        if we_accept != theirs.accepted {
            if !known.contains(&name) {
                disagreements.push(format!(
                    "{name}: membership — here {we_accept}, authority {}",
                    theirs.accepted
                ));
            }
            continue;
        }
        if !we_accept {
            continue;
        }
        let has_include = ours
            .tree()
            .statements()
            .any(|s| matches!(s, Statement::Include(_)));
        if has_include {
            continue;
        }
        let expected: Vec<&str> = ours
            .tree()
            .statements()
            .flat_map(|s| corresponding(&s))
            .collect();
        // clingo 5.8.2 surfaces comments as top-level AST nodes; this
        // grammar holds them as trivia (§4.1), not statements, so they
        // are not statement kinds to compare — dropped, as the leading
        // implicit `#program base.` is dropped below.
        let mut found: Vec<&str> = theirs
            .statements
            .iter()
            .map(|(kind, _)| kind.as_str())
            .filter(|&kind| kind != "Comment")
            .collect();
        if theirs
            .statements
            .first()
            .is_some_and(|(kind, text)| kind == "Program" && text == "#program base.")
        {
            found.remove(0);
        }
        if expected != found && !known.contains(&name) {
            disagreements.push(format!(
                "{name}: kinds — here {expected:?}, authority {found:?}"
            ));
        }
    }
    println!("compared {compared} inputs against the authority");
    assert!(
        disagreements.is_empty(),
        "unrecorded disagreements with the authority (each is a defect here or a divergence for docs/grammar.md §11):\n{}",
        disagreements.join("\n")
    );
}

#[test]
fn differential_the_four_printable_canonical_spellings_are_the_authoritys() {
    let reading = authority(
        "p :- X == 1, X <> 2, Y = #infimum, Z = #supremum.\n",
        &corpus_dir(),
    );
    assert!(reading.accepted);
    assert!(
        reading.statements.len() >= 2,
        "the authority builds the base program then the rule, got {:?}",
        reading.statements
    );
    let printed = &reading.statements[1].1;
    for (synonym, canonical) in [
        ("==", "= "),
        ("<>", "!="),
        ("#infimum", "#inf"),
        ("#supremum", "#sup"),
    ] {
        assert!(
            printed.contains(canonical),
            "{printed}: the authority prints {canonical}"
        );
        assert!(
            !printed.contains(synonym),
            "{printed}: the authority does not print {synonym}"
        );
    }
}

/// One nested input per family, `depth` levels deep, as a program the
/// authority reads (docs/design/syntax.md §6.6; docs/grammar.md §11 D2).
fn nested(family: &str, depth: usize) -> String {
    match family {
        "term: function arguments" => format!("p({}x{}).\n", "f(".repeat(depth), ")".repeat(depth)),
        "term: parentheses" => format!("p({}x{}).\n", "(".repeat(depth), ")".repeat(depth)),
        "term: absolute value" => format!("p({}x{}).\n", "|".repeat(depth), "|".repeat(depth)),
        "term: pool" => format!("p({}x{}).\n", "(".repeat(depth), ";y)".repeat(depth)),
        "constant term: function arguments" => {
            format!("#const c = {}x{}.\n", "f(".repeat(depth), ")".repeat(depth))
        }
        "theory term: set" => format!("&a {{ {}x{} }}.\n", "{".repeat(depth), "}".repeat(depth)),
        "theory term: list" => format!("&a {{ {}x{} }}.\n", "[".repeat(depth), "]".repeat(depth)),
        "theory term: tuple" => format!("&a {{ {}x{} }}.\n", "(".repeat(depth), ")".repeat(depth)),
        "theory term: function arguments" => {
            format!("&a {{ {}x{} }}.\n", "f(".repeat(depth), ")".repeat(depth))
        }
        "chain: exponentiation" => format!("p({}).\n", vec!["2"; depth].join("**")),
        "chain: unary" => format!("p({}x).\n", "-".repeat(depth)),
        _ => unreachable!("a family named above"),
    }
}

const FAMILIES: [&str; 11] = [
    "term: function arguments",
    "term: parentheses",
    "term: absolute value",
    "term: pool",
    "constant term: function arguments",
    "theory term: set",
    "theory term: list",
    "theory term: tuple",
    "theory term: function arguments",
    "chain: exponentiation",
    "chain: unary",
];

/// What the authority does with one probe: accepts, refuses cleanly, or
/// dies (its process ends without an answer).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Accepts,
    Refuses,
    Dies,
}

fn probe(family: &str, depth: usize) -> Outcome {
    let output = run_authority(&nested(family, depth), &corpus_dir());
    if !output.status.success() {
        return Outcome::Dies;
    }
    let reading: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    if reading["accepted"].as_bool() == Some(true) {
        Outcome::Accepts
    } else {
        Outcome::Refuses
    }
}

/// The measurement (docs/design/syntax.md §6.6, §16): per family, the
/// largest depth the authority accepts, found by doubling until it does
/// not and bisecting the last interval; the failure mode named. Written
/// to `target/differential/authority-ceiling.txt` for the record.
#[test]
#[ignore = "out of band: pixi run measure-ceiling"]
fn measure_the_authoritys_nesting_ceiling_per_family() {
    const CAP: usize = 1 << 21;
    let mut report =
        String::from("family | last depth accepted | first depth failing | failure mode\n");
    for family in FAMILIES {
        // low = 0 is the honest floor: the depth-0 form has no nesting and
        // is a plain member, never probed. The doubling probes upward from
        // depth 1, so from here low only ever holds a probed-accepted depth.
        let mut low = 0usize;
        let mut high = 1usize;
        let mut failing = None;
        while high <= CAP {
            match probe(family, high) {
                Outcome::Accepts => {
                    low = high;
                    high *= 2;
                }
                outcome => {
                    failing = Some((high, outcome));
                    break;
                }
            }
        }
        if let Some((mut fail_at, mode)) = failing {
            while fail_at - low > 1 {
                let middle = low + (fail_at - low) / 2;
                match probe(family, middle) {
                    Outcome::Accepts => low = middle,
                    _ => fail_at = middle,
                }
            }
            writeln!(report, "{family} | {low} | {fail_at} | {mode:?}").expect("the report writes");
        } else {
            writeln!(
                report,
                "{family} | {low} (accepted at the cap {CAP}) | — | —"
            )
            .expect("the report writes");
        }
        println!("{}", report.lines().last().unwrap_or(""));
    }
    let out = workspace_root().join("target/differential");
    fs::create_dir_all(&out).expect("target directory");
    fs::write(out.join("authority-ceiling.txt"), &report).expect("the report writes");
    println!("{report}");
}

/// The secondary cross-check (docs/grammar.md §3): every clingo-dialect
/// corpus input parsed by the pinned tree-sitter-clingo grammar; a
/// disagreement on membership not yet recorded in
/// `TREE-SITTER-DISAGREEMENTS` fails, and each recorded one carries the
/// reading against the authority that settled it.
#[test]
#[ignore = "out of band: pixi run cross-check"]
fn the_pinned_tree_sitter_grammar_agrees_with_this_parser_on_the_corpus() {
    let grammar = workspace_root().join("target/tree-sitter-clingo");
    let head = Command::new("git")
        .args([
            "-C",
            grammar.to_str().expect("utf-8 path"),
            "rev-parse",
            "HEAD",
        ])
        .output()
        .expect("git runs");
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        TREE_SITTER_CLINGO,
        "the grammar is at its pin"
    );
    let known = register("TREE-SITTER-DISAGREEMENTS");
    let mut disagreements = Vec::new();
    for path in inputs() {
        let name = relative(&path);
        let sidecar = fs::read_to_string(path.with_extension("expect")).unwrap_or_default();
        if sidecar.lines().next() == Some("asp-core-2") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("input reads");
        let ours = !parse_here(&text).has_errors();
        let status = Command::new("tree-sitter")
            .args(["parse", "-q"])
            .arg(&path)
            .current_dir(&grammar)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("tree-sitter runs under pixi");
        let theirs = status.success();
        if ours != theirs && !known.contains(&name) {
            disagreements.push(format!("{name}: here {ours}, tree-sitter {theirs}"));
        }
    }
    assert!(
        disagreements.is_empty(),
        "unrecorded disagreements with tree-sitter-clingo (read each against the authority):\n{}",
        disagreements.join("\n")
    );
}

// ---- the harness's own logic, held without the authority ------------

/// `corresponding` maps each family's statement to the authority's kinds
/// (the mapping the membership test compares), read against clingo 5.8.2.
#[test]
fn differential_corresponding_maps_our_statements_to_the_authoritys_kinds() {
    let cases: [(&str, Vec<&str>); 15] = [
        ("p :- q.", vec!["Rule"]),
        (":~ q. [1@2]", vec!["Minimize"]),
        (
            "#minimize { 1@2 : p ; 3@4 : q }.",
            vec!["Minimize", "Minimize"],
        ),
        ("#show p/1.", vec!["ShowSignature"]),
        ("#show a : b.", vec!["ShowTerm"]),
        ("#project p/1.", vec!["ProjectSignature"]),
        ("#project p(X).", vec!["ProjectAtom"]),
        ("#edge (a,b;c,d).", vec!["Edge", "Edge"]),
        ("#const c = 1.", vec!["Definition"]),
        ("#program p.", vec!["Program"]),
        ("#external p.", vec!["External"]),
        ("#heuristic a : b. [1@2,sign]", vec!["Heuristic"]),
        ("#defined p/1.", vec!["Defined"]),
        ("#theory t { }.", vec!["TheoryDefinition"]),
        ("#include \"f\".", vec![]),
    ];
    for (text, expected) in cases {
        let parse = parse_here(text);
        assert!(!parse.has_errors(), "{text:?} is a member");
        let kinds: Vec<&str> = parse
            .tree()
            .statements()
            .flat_map(|s| corresponding(&s))
            .collect();
        assert_eq!(kinds, expected, "{text:?}");
    }
}

/// `nested` builds each family's `depth`-deep shape.
#[test]
fn differential_nested_builds_the_family_shapes() {
    assert_eq!(nested("term: parentheses", 2), "p(((x))).\n");
    assert_eq!(nested("term: function arguments", 2), "p(f(f(x))).\n");
    assert_eq!(nested("term: pool", 1), "p((x;y)).\n");
    assert_eq!(nested("chain: unary", 3), "p(---x).\n");
    assert_eq!(nested("chain: exponentiation", 3), "p(2**2**2).\n");
    assert_eq!(nested("theory term: set", 1), "&a { {x} }.\n");
    for family in FAMILIES {
        assert!(!nested(family, 1).is_empty(), "{family} builds an input");
    }
}

/// `register` reads each entry's path and drops the `#` header.
#[test]
fn differential_register_reads_paths_and_drops_the_header() {
    let skip = register("DIFFERENTIAL-SKIP");
    assert!(skip.contains("seeds/clingo/doc-inside-theory-expression.lp"));
    assert!(
        !skip.iter().any(|entry| entry.starts_with('#')),
        "the header is dropped"
    );
    // An absent register is empty, not a failure.
    assert!(register("NO-SUCH-REGISTER").is_empty());
}

/// The query-mark family (grammar §6.1, §11) held against the authority.
/// The base-frame query mark stays a term operator (`x(1?2).`), a
/// `?`-final program is the ASP-Core-2 query — a syntax error under the
/// clingo dialect the authority reads (`p ?`, `p(1)?`) — and a directive
/// whose term ends in `?` dangles an operator (`#show p ?`). The
/// base-frame and not-final cases are corpus seeds already; bare `p ?`
/// and the directive-final cases are held here — the mid-stage
/// repair-pass ledger's ruling D kept them on the seed list, and they
/// are not standalone corpus files. Membership only, agreement asserted
/// (not a hardcoded expectation): confirms ruling D by execution.
#[test]
fn differential_the_query_mark_seeds_agree_with_the_authority() {
    for seed in [
        "x(1?2).",
        "p ? q = X.",
        "p(1)?2 > 3.",
        "p ?",
        "p(1)?",
        "p ? q.",
        "#show p ?",
        "#external p ?",
        "#const c = p ?",
    ] {
        let we_accept = !parse_here(seed).has_errors();
        let they_accept = authority(seed, &corpus_dir()).accepted;
        assert_eq!(
            we_accept, they_accept,
            "{seed:?}: membership disagrees with the authority"
        );
    }
}
