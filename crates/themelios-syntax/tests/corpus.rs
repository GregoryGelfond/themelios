//! The corpus (docs/design/syntax.md §16; docs/specification.md §10.3):
//! every input parsed under its stated dialect with its stated
//! expectation — member, or the diagnostic identities expected — and,
//! over every input, the text law, determinism, and the depth bound.
//! What it proves: reachability, membership as this parser reads it,
//! and the laws over real programs. What it cannot: agreement with the
//! authority — the differential's question.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{MAX_TREE_DEPTH, parse};
use themelios_syntax::tree::{NodeOrToken, SyntaxNode, WalkEvent};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Every `.lp` under the corpus, sorted.
fn inputs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![corpus_dir()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("corpus directory reads") {
            let path = entry.expect("corpus entry reads").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "lp") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// What the corpus says about one input.
struct Expectation {
    dialect: Dialect,
    member: bool,
    /// For a non-member: the identities that must each appear, and
    /// outside which none may.
    identities: BTreeSet<String>,
}

fn expectation(path: &Path, non_members: &[(String, BTreeSet<String>)]) -> Expectation {
    let sidecar = path.with_extension("expect");
    if let Ok(text) = fs::read_to_string(&sidecar) {
        let mut lines = text.lines();
        let dialect = match lines.next() {
            Some("clingo") => Dialect::Clingo,
            Some("asp-core-2") => Dialect::AspCore2,
            other => panic!("{}: dialect line, found {other:?}", sidecar.display()),
        };
        let member = match lines.next() {
            Some("member") => true,
            Some("non-member") => false,
            other => panic!("{}: membership line, found {other:?}", sidecar.display()),
        };
        let identities = lines.map(str::to_owned).collect();
        return Expectation {
            dialect,
            member,
            identities,
        };
    }
    let relative = path
        .strip_prefix(corpus_dir())
        .expect("under the corpus")
        .to_string_lossy()
        .into_owned();
    match non_members.iter().find(|(name, _)| *name == relative) {
        Some((_, identities)) => Expectation {
            dialect: Dialect::Clingo,
            member: false,
            identities: identities.clone(),
        },
        None => Expectation {
            dialect: Dialect::Clingo,
            member: true,
            identities: BTreeSet::new(),
        },
    }
}

fn non_members() -> Vec<(String, BTreeSet<String>)> {
    fs::read_to_string(corpus_dir().join("NON-MEMBERS"))
        .expect("NON-MEMBERS is present")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            let path = fields.next().expect("a path").to_owned();
            (path, fields.map(str::to_owned).collect())
        })
        .collect()
}

/// The tree's depth, by an iterative walk.
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

#[test]
fn every_input_parses_as_the_corpus_says() {
    let non_members = non_members();
    let mut failures = Vec::new();
    let mut count = 0usize;
    for path in inputs() {
        count += 1;
        let text = fs::read_to_string(&path).expect("input reads");
        let expected = expectation(&path, &non_members);
        let source = Source::new(SourceId::new(0), text.clone()).expect("corpus inputs admit");
        let again = parse(&source, expected.dialect);
        let parse = parse(&source, expected.dialect);
        let name = path
            .strip_prefix(corpus_dir())
            .expect("under the corpus")
            .display()
            .to_string();
        if parse.syntax().text() != text.as_str() {
            failures.push(format!("{name}: the tree's text is not the input"));
        }
        if parse != again {
            failures.push(format!("{name}: two parses differ"));
        }
        if depth(&parse.syntax()) > MAX_TREE_DEPTH as usize {
            failures.push(format!("{name}: deeper than the bound"));
        }
        let errors: BTreeSet<String> = parse
            .diagnostics()
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .map(|d| d.id().to_string())
            .collect();
        if expected.member {
            if parse.has_errors() {
                failures.push(format!("{name}: expected a member, found {errors:?}"));
            }
        } else if !parse.has_errors() {
            failures.push(format!("{name}: expected a non-member, found a member"));
        } else if errors != expected.identities {
            failures.push(format!(
                "{name}: expected {:?}, found {errors:?}",
                expected.identities
            ));
        }
    }
    assert!(count > 400, "the corpus is vendored: {count} inputs");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
