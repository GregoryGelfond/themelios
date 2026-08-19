//! Arbitrary bytes under both dialects and every entry point: no panic,
//! the tree's text is the input, the parse terminates, `has_errors` and
//! `is_incomplete` are consistent with the diagnostics, every trivia
//! comment attaches, and the tree's depth respects the bound
//! (docs/design/syntax.md §16). The certificate and the mode law join
//! this target in Tasks 15–16.
#![no_main]

use libfuzzer_sys::fuzz_target;
use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    MAX_TREE_DEPTH, Parse, parse_program, parse_statement, parse_term, parse_term_value,
};
use themelios_syntax::tree::{Asp, AstNode, NodeOrToken, SyntaxNode, WalkEvent};

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

fn holds<T: AstNode<Language = Asp>>(parse: &Parse<T>, text: &str) {
    assert_eq!(parse.syntax().text(), text);
    let _ = parse.tree();
    let has_error = parse
        .diagnostics()
        .iter()
        .any(|d| d.severity() == Severity::Error);
    assert_eq!(parse.has_errors(), has_error);
    if parse.is_incomplete() {
        assert!(parse.has_errors());
        for diagnostic in parse
            .diagnostics()
            .iter()
            .filter(|d| d.severity() == Severity::Error)
        {
            let name = diagnostic.id().name();
            assert!(
                matches!(
                    name,
                    "unexpected-end-of-input"
                        | "unterminated-block-comment"
                        | "unterminated-script"
                        | "malformed-string"
                ),
                "{name} is not an incompleteness"
            );
        }
    }
    assert!(depth(&parse.syntax()) <= MAX_TREE_DEPTH as usize);
    let root = parse.syntax();
    let mut trivia_comments = 0usize;
    for token in root
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
    {
        if token.kind().is_comment()
            && themelios_syntax::tree::role(&token) == themelios_syntax::tree::TokenRole::Trivia
        {
            trivia_comments += 1;
            assert!(themelios_syntax::attach::attachment(&token).is_ok());
        }
    }
    assert_eq!(
        themelios_syntax::attach::attachments(&root).count(),
        trivia_comments
    );
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    let text = source.text().to_owned();
    for dialect in [Dialect::Clingo, Dialect::AspCore2] {
        let lexer = Lexer::new(&source, dialect);
        holds(&parse_program(&lexer), &text);
        holds(&parse_statement(&lexer), &text);
        holds(&parse_term(&lexer), &text);
        holds(&parse_term_value(&lexer), &text);
    }
});
