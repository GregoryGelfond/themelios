//! Arbitrary bytes under both dialects and every entry point: no panic,
//! the tree's text is the input, the parse terminates, `has_errors` and
//! `is_incomplete` are consistent with the diagnostics, every trivia
//! comment attaches, the tree's depth respects the bound, and
//! `lex_mode_of` equals the region the parser stood in over the program
//! entry's mode-sensitive tokens (docs/design/syntax.md §16). The
//! certificate joins this target in Task 16.
#![no_main]

use std::cell::RefCell;
use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use themelios_base::diagnostic::Severity;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::fusion::lex_mode_of;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    MAX_TREE_DEPTH, Parse, parse_program, parse_statement, parse_term, parse_term_value,
};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::{Asp, AstNode, NodeOrToken, SyntaxNode, WalkEvent};

/// A token source that records every mode the parser requests, so the
/// mode law can be checked against the reconstruction (§10.2, §16).
struct Recording<'a> {
    lexer: Lexer<'a>,
    requests: RefCell<Vec<(u32, LexMode)>>,
}

impl TokenSource for Recording<'_> {
    fn id(&self) -> SourceId {
        self.lexer.id()
    }
    fn dialect(&self) -> Dialect {
        self.lexer.dialect()
    }
    fn text(&self) -> &str {
        self.lexer.text()
    }
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        self.requests.borrow_mut().push((at.get(), mode));
        self.lexer.token_at(at, mode)
    }
}

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
    // Every trivia comment attaches, the bulk form yields each exactly
    // once, and — over arbitrary input — the forward form agrees with the
    // bulk entry and the inverse form yields the comment back
    // (docs/design/syntax.md §9.3, §16).
    let root = parse.syntax();
    let trivia_comments = root
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|token| {
            token.kind().is_comment()
                && themelios_syntax::tree::role(token) == themelios_syntax::tree::TokenRole::Trivia
        })
        .count();
    let attachments: Vec<_> = themelios_syntax::attach::attachments(&root).collect();
    assert_eq!(attachments.len(), trivia_comments);
    for (comment, attachment) in &attachments {
        assert_eq!(
            themelios_syntax::attach::attachment(comment).as_ref(),
            Ok(attachment)
        );
        assert!(
            themelios_syntax::attach::comments(&attachment.anchor, attachment.slot)
                .any(|c| &c == comment)
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    let text = source.text().to_owned();
    for dialect in [Dialect::Clingo, Dialect::AspCore2] {
        let lexer = Lexer::new(&source, dialect);
        // The program entry through a recording source, which also holds
        // the mode law over members: `lex_mode_of` equals the region the
        // parser stood in (§10.2). Recovery breaks the structural
        // correspondence the reconstruction reads, so a non-member's tree
        // — its ERROR nodes, and the aspif dispatch's raw-text wrapper
        // (§4.9) — need not reflect the parser's modes.
        let recording = Recording {
            lexer: Lexer::new(&source, dialect),
            requests: RefCell::new(Vec::new()),
        };
        let program = parse_program(&recording);
        holds(&program, &text);
        if !program.has_errors() {
            let mut modes: HashMap<u32, LexMode> = HashMap::new();
            for (at, mode) in recording.requests.borrow().iter() {
                let entry = modes.entry(*at).or_insert(LexMode::Normal);
                if *mode != LexMode::Normal {
                    *entry = *mode;
                }
            }
            for token in program
                .syntax()
                .descendants_with_tokens()
                .filter_map(NodeOrToken::into_token)
            {
                if token.kind().is_trivia() || token.kind().is_comment() {
                    continue;
                }
                let at = u32::from(token.text_range().start());
                if let Some(region) = modes.get(&at) {
                    assert_eq!(lex_mode_of(&token), *region);
                }
            }
        }
        holds(&parse_statement(&lexer), &text);
        holds(&parse_term(&lexer), &text);
        holds(&parse_term_value(&lexer), &text);
    }
});
