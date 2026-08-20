//! Arbitrary bytes under both dialects and every entry point: no panic,
//! the tree's text is the input, the parse terminates, `has_errors` and
//! `is_incomplete` are consistent with the diagnostics, every trivia
//! comment attaches, both certificates hold reflexively, the tree's
//! depth respects the bound, and `lex_mode_of` equals the region the
//! parser stood in over the program entry's mode-sensitive tokens
//! (docs/design/syntax.md §16).
#![no_main]

use std::cell::RefCell;
use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use themelios_base::diagnostic::Severity;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{Certificate, equivalent};
use themelios_syntax::fusion::{Separator, lex_mode_of, separator};
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    MAX_TREE_DEPTH, Parse, parse_program, parse_statement, parse_term, parse_term_value,
    with_required_stack,
};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::{Asp, AstNode, NodeOrToken, SyntaxKind, SyntaxNode, WalkEvent};

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
    // The certificate is reflexive: a parse certifies against itself under
    // both certificates (docs/design/syntax.md §11.2, §16).
    assert_eq!(
        themelios_syntax::equiv::equivalent(
            parse,
            parse,
            themelios_syntax::equiv::Certificate::LayoutOnly
        ),
        Ok(())
    );
    assert_eq!(
        themelios_syntax::equiv::equivalent(
            parse,
            parse,
            themelios_syntax::equiv::Certificate::UpToSpelling
        ),
        Ok(())
    );
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    // A deeply nested input builds a tree that recurses in depth on drop
    // and on the walks below (§6.6, §14); a large enough input reaches a
    // depth the fuzzer's own thread cannot hold. A large input — rare, and
    // never at the default corpus size — runs on `REQUIRED_STACK_BYTES`, as
    // a consumer holding such a tree must; the rest run in place, no spawn.
    if source.text().len() > 4096 {
        with_required_stack(move || run(source));
    } else {
        run(source);
    }
});

fn run(source: Source) {
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
            // The certificate under re-spacing (a member's whole-text lemma,
            // §11.2, §16, §10.1): the program re-spaced to abut every pair the
            // oracle allows reparses to the same non-whitespace sequence, so it
            // certifies layout-only. Members only — a non-member's ERROR tokens
            // do not compose under re-spacing.
            let tokens: Vec<_> = program
                .syntax()
                .descendants_with_tokens()
                .filter_map(NodeOrToken::into_token)
                .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
                .collect();
            let mut respaced = String::new();
            for (index, token) in tokens.iter().enumerate() {
                respaced.push_str(token.text());
                if let Some(next) = tokens.get(index + 1) {
                    match separator(token, next, dialect) {
                        Separator::Nothing => {}
                        Separator::Whitespace => respaced.push(' '),
                        Separator::LineBreak => respaced.push('\n'),
                    }
                }
            }
            if let Ok(respaced_source) = Source::new(SourceId::new(1), respaced) {
                let reparsed = parse_program(&Lexer::new(&respaced_source, dialect));
                assert_eq!(
                    equivalent(&program, &reparsed, Certificate::LayoutOnly),
                    Ok(())
                );
            }
        }
        holds(&parse_statement(&lexer), &text);
        holds(&parse_term(&lexer), &text);
        holds(&parse_term_value(&lexer), &text);
    }
}
