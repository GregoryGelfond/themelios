//! Fixtures shared by the parser families' test modules: parse a text,
//! read its one statement's shape, its diagnostic kinds, or whether it
//! is a member. One copy, so `member` is the §6.5 definition
//! (`!has_errors()`) everywhere rather than drifting between families.

use themelios_base::source::{Source, SourceId};

use crate::diagnostic::SyntaxErrorKind;
use crate::dialect::Dialect;
use crate::parse::parse;
use crate::tree::sexpr;

/// The source of `text`, which every test text admits.
pub(crate) fn admitted(text: &str) -> Source {
    Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
}

/// The program's shape with the root peeled: one statement's shape.
pub(crate) fn shape(text: &str) -> String {
    let source = admitted(text);
    let parse = parse(&source, Dialect::Clingo);
    assert_eq!(parse.syntax().text(), text, "law 1");
    let shape = sexpr(&parse.syntax());
    shape
        .strip_prefix("(PROGRAM ")
        .and_then(|rest| rest.strip_suffix(')'))
        .map_or(shape.clone(), str::to_owned)
}

/// The diagnostic kinds of `text`, in the parser's order.
pub(crate) fn kinds(text: &str) -> Vec<SyntaxErrorKind> {
    let source = admitted(text);
    parse(&source, Dialect::Clingo)
        .diagnostics()
        .iter()
        .map(|d| d.kind().clone())
        .collect()
}

/// Whether `text` is a member: it parses with no error (§6.5).
pub(crate) fn member(text: &str) -> bool {
    let source = admitted(text);
    !parse(&source, Dialect::Clingo).has_errors()
}
