//! The four tree laws, the dialect-neutrality law, and the
//! incompleteness law over prefixes (docs/design/syntax.md §5.4, §3,
//! §6.5, §16), over the vendored corpus. What they prove: the tree's
//! shape invariants hold on real programs; the two dialects agree on
//! their shared subset; a member's prefixes are never wrong, only
//! complete or unfinished. Laws 1, 3, and 4 also stand inside the
//! membership harness (`corpus.rs`); here they are named laws beside
//! law 2, whose placement rule no entry test holds over the whole
//! corpus.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::diagnostic::SyntaxError;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{MAX_TREE_DEPTH, parse};
use themelios_syntax::tree::{NodeOrToken, SyntaxKind, SyntaxNode, TokenRole, WalkEvent, role};

/// Every corpus input with its dialect (the sidecar's, else clingo).
fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let dialect = fs::read_to_string(path.with_extension("expect"))
                    .ok()
                    .and_then(|sidecar| sidecar.lines().next().map(str::to_owned))
                    .map_or(Dialect::Clingo, |line| {
                        if line == "asp-core-2" {
                            Dialect::AspCore2
                        } else {
                            Dialect::Clingo
                        }
                    });
                found.push((
                    path.strip_prefix(&dir)
                        .expect("under corpus")
                        .display()
                        .to_string(),
                    text,
                    dialect,
                ));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn root_of(text: &str, dialect: Dialect) -> SyntaxNode {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, dialect).syntax()
}

/// The tree's depth by an iterative walk.
fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

/// The three roots: trivia at a root's edges belongs to it (law 2).
fn is_root(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PROGRAM | SyntaxKind::STATEMENT_FRAGMENT | SyntaxKind::TERM_FRAGMENT
    )
}

#[test]
fn the_four_tree_laws_hold_over_the_corpus() {
    let mut failures = Vec::new();
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let one = parse(&source, dialect);
        let two = parse(&source, dialect);
        // Law 1 (text): the tree's text is the input, byte for byte.
        if one.syntax().text() != text.as_str() {
            failures.push(format!("{name}: law 1 — the tree's text is not the input"));
        }
        // Law 4 (determinism): two parses of one text are equal.
        if one != two {
            failures.push(format!("{name}: law 4 — two parses of one text differ"));
        }
        // Law 3 (bounded depth): no tree is deeper than the bound.
        if depth(&one.syntax()) > MAX_TREE_DEPTH as usize {
            failures.push(format!("{name}: law 3 — deeper than MAX_TREE_DEPTH"));
        }
        // Law 2 (placement): every non-empty node but a root begins and
        // ends with a significant token — role not Trivia, so a doc line
        // in docs position (role Documentation) still counts as
        // significant.
        for node in one.syntax().descendants() {
            if is_root(node.kind()) || node.text_range().is_empty() {
                continue;
            }
            for edge in [node.first_token(), node.last_token()]
                .into_iter()
                .flatten()
            {
                if role(&edge) == TokenRole::Trivia {
                    failures.push(format!(
                        "{name}: law 2 — {} begins or ends with trivia {:?}",
                        node.kind(),
                        edge.text()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The non-whitespace token stream — kinds and texts — for comparing two
/// dialects' readings of one text.
fn token_stream(root: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    root.descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .map(|t| (t.kind(), t.text().to_owned()))
        .collect()
}

/// The significant-token shape: the preorder over nodes and significant
/// tokens, trivia dropped.
fn shape(root: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => out.push(format!("({}", node.kind())),
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(")".to_owned()),
            WalkEvent::Enter(NodeOrToken::Token(token)) if role(&token) != TokenRole::Trivia => {
                out.push(format!("{}:{}", token.kind(), token.text()));
            }
            _ => {}
        }
    }
    out
}

fn diagnostics(text: &str, dialect: Dialect) -> Vec<SyntaxError> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    parse(&source, dialect).diagnostics().to_vec()
}

#[test]
fn the_two_dialects_agree_on_their_shared_subset() {
    // The shared subset, detected by its consequence: a text whose two
    // dialects lex to the same non-whitespace token stream (so the string
    // and block-comment rules did not bite) and whose last significant
    // token is not `?` (so the query reading cannot bite) must yield
    // structurally equal trees and equal diagnostics — exactly the inputs
    // docs/design/syntax.md §3 names.
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (name, text, _) in corpus() {
        let clingo = root_of(&text, Dialect::Clingo);
        let core = root_of(&text, Dialect::AspCore2);
        if token_stream(&clingo) != token_stream(&core) {
            continue; // the string or block-comment rule bit: not shared.
        }
        // The last *significant* token — trivia (a trailing comment,
        // whitespace) skipped, since the query reading skips it too.
        let last_significant = clingo
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter(|t| role(t) == TokenRole::Significant)
            .last();
        if last_significant.is_some_and(|t| t.kind() == SyntaxKind::QUESTION) {
            continue; // a final `?`: the query reading may bite; not shared.
        }
        checked += 1;
        if shape(&clingo) != shape(&core) {
            failures.push(format!(
                "{name}: the shared subset's trees differ by dialect"
            ));
        }
        if diagnostics(&text, Dialect::Clingo) != diagnostics(&text, Dialect::AspCore2) {
            failures.push(format!(
                "{name}: the shared subset's diagnostics differ by dialect"
            ));
        }
    }
    assert!(
        checked > 50,
        "the shared subset is a substantial part of the corpus: {checked} inputs"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_prefix_of_a_member_at_a_token_boundary_is_error_free_or_incomplete() {
    // Sampled: up to ~32 boundaries spread across each member, enough to
    // exercise mid-construct cuts without an O(n^2) sweep of every file.
    const SAMPLES_PER_INPUT: usize = 32;
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let whole = parse(&source, dialect);
        if whole.has_errors() {
            continue; // the law speaks of member programs.
        }
        // The token boundaries: the end offset of every token, in order.
        let mut boundaries = Vec::new();
        let mut at = 0usize;
        for token in whole
            .syntax()
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
        {
            at += token.text().len();
            boundaries.push(at);
        }
        let stride = boundaries.len().div_ceil(SAMPLES_PER_INPUT).max(1);
        for offset in boundaries.iter().step_by(stride).copied() {
            let prefix = &text[..offset];
            let prefix_source = Source::new(SourceId::new(0), prefix.to_owned()).expect("admits");
            let prefix_parse = parse(&prefix_source, dialect);
            checked += 1;
            if prefix_parse.has_errors() && !prefix_parse.is_incomplete() {
                let ids: Vec<String> = prefix_parse
                    .diagnostics()
                    .iter()
                    .filter(|d| d.severity() == Severity::Error)
                    .map(|d| d.id().to_string())
                    .collect();
                failures.push(format!(
                    "{name}: the {offset}-byte prefix is neither error-free nor incomplete: {ids:?}"
                ));
            }
        }
    }
    assert!(
        checked > 0,
        "the corpus has member programs to take prefixes of"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
