//! The reviewed goldens (docs/design/syntax.md §16): the diagnostics
//! corpus — the characteristic malformed programs of every family in
//! §6.7 and every identity in Appendix B, rendered through base's human
//! view (the diagnostics-quality witness); the recovery shape of each
//! family's row as a tree dump; and the attachment dumps of kallos's
//! scar corpus (spec §5.1) and a CRLF-authored input (§9.2's empty
//! line), each comment resolved to its slot and anchor (§9). Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-syntax --test golden`, then
//! review the diff before committing: these files are reviewed
//! artifacts, not incidental output.

use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId, SourceSet};
use themelios_base::span::ByteOffset;
use themelios_base::view::human;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{MAX_NESTING_DEPTH, parse, parse_program};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::SyntaxKind;

fn check(group: &str, name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(group)
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a group directory")).expect("golden directory");
        fs::write(&path, actual).expect("golden file writes");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; bless it and review",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "diverged from the reviewed golden `{group}/{name}`"
    );
}

/// The human view of every diagnostic of `text` under `dialect`, in
/// the parser's order, each rendering separated by a blank line.
fn diagnostics(text: &str, dialect: Dialect) -> String {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add("input.lp".to_owned(), text.to_owned())
        .expect("admits");
    let source = Source::new(file, text.to_owned()).expect("admits");
    let parse = parse(&source, dialect);
    let mut out = String::new();
    for diagnostic in parse.diagnostics() {
        out.push_str(&human(&diagnostic.to_diagnostic(), &catalog));
        out.push('\n');
    }
    out
}

/// The tree dump and the diagnostics of `text` — the recovery shape.
fn recovery(text: &str) -> String {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    let mut out = format!("{:#?}", parse.syntax());
    out.push('\n');
    for diagnostic in parse.diagnostics() {
        let _ = writeln!(out, "{}: {:?}", diagnostic.id(), diagnostic.kind());
    }
    out
}

fn diag(name: &str, text: &str) {
    check("diagnostics", name, &diagnostics(text, Dialect::Clingo));
}

// ---- every identity of Appendix B, and the characteristic helps ------

#[test]
fn unexpected_characters() {
    diag("unexpected-characters", "p($$$).\n");
}

#[test]
fn unknown_hash_word() {
    diag("unknown-hash-word", "#sums { X : p(X) } > 1.\n");
}

#[test]
fn malformed_string_bad_escape_leaves_the_next_statement_intact() {
    diag("malformed-string-escape", "p(\"a\\qb\"). q.\n");
}

#[test]
fn malformed_string_line_break() {
    diag("malformed-string-line-break", "p(\"a\nb\").\n");
}

#[test]
fn malformed_string_unterminated() {
    diag("malformed-string-unterminated", "p(\"abc");
}

#[test]
fn unterminated_block_comment() {
    diag("unterminated-block-comment", "%* a % *% b *%\np.\n");
}

#[test]
fn unterminated_script() {
    diag("unterminated-script", "#script (lua)\nx = 1\n");
}

#[test]
fn anonymous_in_theory_expression() {
    diag("anonymous-in-theory-expression", "&sum { _ } <= 1.\n");
}

#[test]
fn unexpected_token() {
    diag("unexpected-token", "p(X) :- q(X) r(X).\n");
}

#[test]
fn unexpected_end_of_input() {
    diag("unexpected-end-of-input", "p(X) :- q(X),");
}

#[test]
fn nesting_too_deep_on_an_annotated_family() {
    // One opener per line, so the rendering's window around the refused
    // bracket stays legible whatever the constant's value.
    let depth = MAX_NESTING_DEPTH as usize + 1;
    let text = format!(
        ":~ p({}x{}). [1@2]\nq.\n",
        "f(\n".repeat(depth),
        ")".repeat(depth)
    );
    diag("nesting-too-deep-annotated", &text);
}

#[test]
fn aspif_input() {
    diag("aspif-input", "asp 1 0 0\n1 0 1 1 0 0\n0\n");
}

#[test]
fn token_source_breach() {
    struct EarlyEnd<'a>(themelios_syntax::lexer::Lexer<'a>);
    impl TokenSource for EarlyEnd<'_> {
        fn id(&self) -> SourceId {
            self.0.id()
        }
        fn dialect(&self) -> Dialect {
            Dialect::Clingo
        }
        fn text(&self) -> &str {
            self.0.text()
        }
        fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
            if at.get() >= 5 {
                return Ok(Token {
                    kind: SyntaxKind::EOF,
                    text: "",
                });
            }
            self.0.token_at(at, mode)
        }
    }
    let text = "p(X). q(X).\n";
    let mut catalog = SourceSet::new();
    let file = catalog
        .add("input.lp".to_owned(), text.to_owned())
        .expect("admits");
    let source = Source::new(file, text.to_owned()).expect("admits");
    let parse = parse_program(&EarlyEnd(themelios_syntax::lexer::Lexer::new(
        &source,
        Dialect::Clingo,
    )));
    let mut out = String::new();
    for diagnostic in parse.diagnostics() {
        out.push_str(&human(&diagnostic.to_diagnostic(), &catalog));
        out.push('\n');
    }
    check("diagnostics", "token-source-breach", &out);
}

#[test]
fn form_not_allowed_here() {
    diag(
        "form-not-allowed-here",
        "#const x = |1;2|.\n#const y = 1..3.\n#const z = X.\n",
    );
}

#[test]
fn misplaced_doc_comment() {
    diag(
        "misplaced-doc-comment",
        "p :- %! inside\n  q.\n%! nothing follows\n",
    );
}

#[test]
fn hint_trailing_comma_in_arguments() {
    diag("hint-trailing-comma", "p(a, b,).\n");
}

#[test]
fn hint_query_mark_needs_asp_core_2() {
    diag("hint-query-mark", "p(1)?");
}

#[test]
fn hint_leading_zero_numeral() {
    diag("hint-leading-zero", "p(007).\n");
}

#[test]
fn hint_empty_condition_before_pipe() {
    diag("hint-empty-condition-pipe", "p(X) : | q(X).\n");
}

#[test]
fn hint_heuristic_needs_annotation() {
    diag("hint-heuristic-annotation", "#heuristic a : b.\n");
}

#[test]
fn a_query_under_the_asp_core_2_dialect_renders_nothing() {
    check(
        "diagnostics",
        "asp-core-2-query",
        &diagnostics("p(1)?", Dialect::AspCore2),
    );
}

// ---- the recovery shape of each row of docs/design/syntax.md §6.7 ----

#[test]
fn recovery_program_level() {
    check(
        "recovery",
        "program-level",
        &recovery(") stray. p.\n#heuristic. [1] q.\n"),
    );
}

#[test]
fn recovery_head_body_condition() {
    check(
        "recovery",
        "head-body-condition",
        &recovery("a ; ; b :- p, , q : r, , s.\n"),
    );
}

#[test]
fn recovery_literal_atom_comparison() {
    check(
        "recovery",
        "literal-atom-comparison",
        &recovery("p :- 1 <, X = .\n"),
    );
}

#[test]
fn recovery_terms_and_argument_lists() {
    check(
        "recovery",
        "frame-loop",
        &recovery("p(f(a b), (c;), |d, 1 +).\n"),
    );
}

#[test]
fn recovery_aggregates() {
    check(
        "recovery",
        "aggregates",
        &recovery(":- #count { a; b . q.\n"),
    );
}

#[test]
fn recovery_theory_atoms_and_elements() {
    check(
        "recovery",
        "theory",
        &recovery(":- &sum { x, ; y : p( } <= . q.\n"),
    );
}

#[test]
fn recovery_directives() {
    check(
        "recovery",
        "directives",
        &recovery("#show p/. #const n = . #include. #program p(1).\n"),
    );
}

#[test]
fn recovery_theory_definitions() {
    check(
        "recovery",
        "theory-definitions",
        &recovery("#theory t { x { + : 1, ternary; - : }; &a/1 : x, foo }.\n"),
    );
}

#[test]
fn recovery_script() {
    check("recovery", "script", &recovery("#script (python)\nx = 1\n"));
}

#[test]
fn recovery_annotations() {
    check(
        "recovery",
        "annotations",
        &recovery(":~ p. [1@\n#external q. [a\nr.\n"),
    );
}

#[test]
fn recovery_end_of_input() {
    check(
        "recovery",
        "end-of-input",
        &recovery(":- p(X), #count { X : q(X"),
    );
}

// ---- tree dumps for the grammar's corner seeds ------------------------

fn dump(name: &str, text: &str) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    check("trees", name, &format!("{:#?}", parse.syntax()));
}

#[test]
fn tree_pooled_arguments_and_tuples() {
    dump("pools-and-tuples", "p(f(a,b;c), (), (a,), (,), (a;b)).\n");
}

#[test]
fn tree_operator_chains_flat() {
    dump(
        "operator-chains",
        "p(1 + 2 * 3 - -4 ** 2 ** 3, 1..3 ^ 2 ? 4 & 5).\n",
    );
}

#[test]
fn tree_disjunction_separators_and_conditions() {
    dump("disjunction", "a ; b | c, d : e, f.\n");
}

#[test]
fn tree_aggregates_both_positions() {
    dump(
        "aggregates",
        "1 { p(X) : q(X) } 1 :- 2 <= #sum { W,T : t(T,W) } < 3, not #count { } 1.\n",
    );
}

#[test]
fn tree_theory_atom_with_guard() {
    dump(
        "theory-atom",
        ":- &sum { x, -y : p ; {a, b}, [1], f(g) } <= - not 3.\n",
    );
}

#[test]
fn tree_documented_statement_and_script() {
    dump(
        "docs-and-script",
        "%! doc\n%! more\np.\n#script (lua)\nreturn 1\n#end.\n",
    );
}

// ---- attachment dumps: kallos's scars and the CRLF input --------------

fn attachment_dump(text: &str) -> String {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::Clingo);
    let mut out = String::new();
    for (comment, attachment) in themelios_syntax::attach::attachments(&parse.syntax()) {
        let _ = writeln!(
            out,
            "{:?} {:?} -> {:?} {}@{:?} {:?}",
            comment.text_range(),
            comment.text(),
            attachment.slot,
            attachment.anchor.kind(),
            attachment.anchor.text_range(),
            attachment.anchor.to_string(),
        );
    }
    out
}

#[test]
fn attachments_kallos_scar_corpus() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/kallos");
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the kallos corpus")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "lp"))
        .collect();
    names.sort();
    for path in names {
        let text = fs::read_to_string(&path).expect("input reads");
        let stem = path
            .file_stem()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        check(
            "attachments",
            &format!("kallos-{stem}"),
            &attachment_dump(&text),
        );
    }
}

#[test]
fn attachments_transposition_dual_role_and_blank_line_detach() {
    check(
        "attachments",
        "scars",
        &attachment_dump(
            "p(1, % after comma\n   % before two\n 2). % trailing\n\n% above gap\n\n% leads q\nq :- a\n  % leads pipe\n  | b. r(|X\n % dangling in abs\n |).\n",
        ),
    );
}

#[test]
fn attachments_crlf() {
    check(
        "attachments",
        "crlf",
        &attachment_dump("% a\r\n\r\n% b\r\np. % t\r\nq :- % in body\r\n  r.\r\n"),
    );
}
