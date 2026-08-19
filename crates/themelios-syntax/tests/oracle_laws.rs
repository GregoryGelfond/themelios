//! The oracle's laws (docs/design/syntax.md §10, §16): for adjacent
//! token pairs drawn from parsed corpus trees, `Nothing` means the pair
//! reparses to itself abutted and `Whitespace` means it does not; the
//! whole-text lemma — a member's text re-spaced to abut every pair the
//! oracle allows reparses to the same token stream; and the mode law —
//! the region the parser stood in equals `lex_mode_of` over every
//! member's mode-sensitive tokens. The last two are guarantees for
//! members: a non-member's recovery tree need not reflect the modes.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::fusion::{Separator, lex_mode_of, separator};
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{parse, parse_program};
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// Every corpus input with its dialect: the sidecar's, else clingo.
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

fn tokens_of(root: &SyntaxNode) -> Vec<SyntaxToken> {
    root.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .collect()
}

/// The token stream a reparse must keep: every non-whitespace token's
/// kind and text, in order.
fn stream(root: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    tokens_of(root)
        .into_iter()
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
        .map(|t| (t.kind(), t.text().to_owned()))
        .collect()
}

/// The first token of `text` under `mode` and `dialect`, as (kind, len).
fn first_token(text: &str, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let lexer = Lexer::new(&source, dialect);
    let token = lexer.token_at(ByteOffset::ZERO, mode).expect("a position");
    (token.kind, token.text.len())
}

#[test]
fn nothing_means_the_pair_reparses_to_itself_and_whitespace_means_it_does_not() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let root = parse(&source, dialect).syntax();
        let tokens: Vec<SyntaxToken> = tokens_of(&root)
            .into_iter()
            .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
            .collect();
        for pair in tokens.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            let joined = format!("{}{}", left.text(), right.text());
            let mode = lex_mode_of(left);
            let (kind, len) = first_token(&joined, mode, dialect);
            let abuts = kind == left.kind() && len == left.text().len();
            match separator(left, right, dialect) {
                Separator::Nothing => assert!(
                    abuts,
                    "{name}: {:?} {:?} answered Nothing",
                    left.text(),
                    right.text()
                ),
                Separator::Whitespace => assert!(
                    !abuts,
                    "{name}: {:?} {:?} answered Whitespace",
                    left.text(),
                    right.text()
                ),
                Separator::LineBreak => assert!(
                    matches!(
                        left.kind(),
                        SyntaxKind::LINE_COMMENT
                            | SyntaxKind::DOC_COMMENT
                            | SyntaxKind::SHEBANG_COMMENT
                    ),
                    "{name}: {:?} answered LineBreak",
                    left.text()
                ),
            }
        }
    }
}

#[test]
fn a_text_respaced_to_abut_every_pair_the_oracle_allows_reparses_to_the_same_token_stream() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let parsed = parse(&source, dialect);
        // The lemma is a guarantee for members. A non-member's ERROR
        // tokens are artifacts of its malformation — a raw line break that
        // split a string, say — and do not compose under re-spacing, which
        // heals the break into a space (docs/design/syntax.md §10.1).
        if parsed.has_errors() {
            continue;
        }
        let root = parsed.syntax();
        let tokens: Vec<SyntaxToken> = tokens_of(&root)
            .into_iter()
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
        let again = Source::new(SourceId::new(0), respaced).expect("admits");
        let reparsed = parse(&again, dialect).syntax();
        assert_eq!(
            stream(&root),
            stream(&reparsed),
            "{name}: the token stream changed under re-spacing"
        );
    }
}

/// A source that records every request the parser makes of it.
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

/// The region mode the parser stood in at each offset (docs/design/syntax.md
/// §10.2): the non-`Normal` mode if any request there was inside a region
/// — the guard-end peek among them — else `Normal`. A token is never
/// requested under both `Theory` and `ScriptBody` (they are disjoint
/// regions), so the non-`Normal` requests at one offset agree.
fn region_modes(requests: &[(u32, LexMode)]) -> std::collections::HashMap<u32, LexMode> {
    let mut modes = std::collections::HashMap::new();
    for (at, mode) in requests {
        let entry = modes.entry(*at).or_insert(LexMode::Normal);
        if *mode != LexMode::Normal {
            *entry = *mode;
        }
    }
    modes
}

/// A token whose lexing depends on the mode — one that is not trivia and
/// not a comment (docs/design/syntax.md §10.2): whitespace and comments
/// lex the same under `Normal` and `Theory` and never stand alone under
/// `ScriptBody`, so their standpoint is a fact about nothing.
fn is_mode_sensitive(kind: SyntaxKind) -> bool {
    !kind.is_trivia() && !kind.is_comment()
}

#[test]
fn the_parsers_recorded_modes_equal_the_reconstruction_over_every_member() {
    for (name, text, dialect) in corpus() {
        let source = Source::new(SourceId::new(0), text.clone()).expect("admits");
        let recording = Recording {
            lexer: Lexer::new(&source, dialect),
            requests: RefCell::new(Vec::new()),
        };
        let parsed = parse_program(&recording);
        // The law holds for members. Recovery deliberately breaks the
        // structural correspondence the reconstruction reads — a malformed
        // condition's contents land loose under THEORY_ELEMENTS, and the
        // aspif dispatch wraps the whole input as one raw-text ERROR (§4.9)
        // — so a non-member's tree need not reflect the parser's modes.
        if parsed.has_errors() {
            continue;
        }
        let root = parsed.syntax();
        let modes = region_modes(&recording.requests.borrow());
        for token in tokens_of(&root)
            .into_iter()
            .filter(|t| is_mode_sensitive(t.kind()))
        {
            let at = u32::from(token.text_range().start());
            let region = modes
                .get(&at)
                .copied()
                .expect("a member's tokens are all lexed");
            assert_eq!(
                lex_mode_of(&token),
                region,
                "{name}: {:?} at {at} stood in {region:?}",
                token.text()
            );
        }
    }
}

#[test]
fn the_named_cases_hold_under_the_recording() {
    for text in [
        "&a { x : p ; -y }.",
        "#script (lua) x #end.",
        "#script (lua) x #end .",
        ":- &sum { x } >= 5, not p.",
    ] {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        let recording = Recording {
            lexer: Lexer::new(&source, Dialect::Clingo),
            requests: RefCell::new(Vec::new()),
        };
        let root = parse_program(&recording).syntax();
        let modes = region_modes(&recording.requests.borrow());
        for token in tokens_of(&root)
            .into_iter()
            .filter(|t| is_mode_sensitive(t.kind()))
        {
            let at = u32::from(token.text_range().start());
            assert_eq!(
                lex_mode_of(&token),
                modes[&at],
                "{text}: {:?}",
                token.text()
            );
        }
    }
}
