//! The token-source laws on the file lexer under every mode, the checker
//! against deliberately breaching sources, and lexer totality and tiling
//! on generated text heavy in multi-byte characters, `%`, `#`, and
//! operator runs (docs/design/syntax.md §4.3, §16).

use std::cell::Cell;

use proptest::prelude::*;
use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::token::{
    LexMode, Token, TokenSource, TokenSourceLawViolation, check_token_source_laws,
};
use themelios_syntax::tree::SyntaxKind;

/// Text drawn from the characters that exercise the lexer's corners:
/// names, numerals, the comment and string openers, operator material,
/// braces, and multi-byte characters.
fn corner_text() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just("p".to_owned()),
        Just("X".to_owned()),
        Just("_".to_owned()),
        Just("'".to_owned()),
        Just("0".to_owned()),
        Just("0x".to_owned()),
        Just("1".to_owned()),
        Just("%".to_owned()),
        Just("%*".to_owned()),
        Just("*%".to_owned()),
        Just("%!".to_owned()),
        Just("#".to_owned()),
        Just("#!".to_owned()),
        Just("#sum".to_owned()),
        Just("#end".to_owned()),
        Just("\"".to_owned()),
        Just("\\".to_owned()),
        Just("\n".to_owned()),
        Just(" ".to_owned()),
        Just(":".to_owned()),
        Just("-".to_owned()),
        Just("..".to_owned()),
        Just("*".to_owned()),
        Just("=".to_owned()),
        Just("<".to_owned()),
        Just("!".to_owned()),
        Just("$".to_owned()),
        Just("{".to_owned()),
        Just("}".to_owned()),
        Just("(".to_owned()),
        Just(")".to_owned()),
        Just("é".to_owned()),
        Just("🦀".to_owned()),
        Just("not".to_owned()),
    ];
    prop::collection::vec(atom, 0..40).prop_map(|parts| parts.concat())
}

fn dialects() -> impl Strategy<Value = Dialect> {
    prop_oneof![Just(Dialect::Clingo), Just(Dialect::AspCore2)]
}

fn modes() -> impl Strategy<Value = LexMode> {
    prop_oneof![
        Just(LexMode::Normal),
        Just(LexMode::Theory),
        Just(LexMode::ScriptBody)
    ]
}

fn admitted(text: &str) -> Source {
    Source::new(SourceId::new(0), text.to_owned()).expect("generated text admits")
}

proptest! {
    #[test]
    fn the_file_lexer_keeps_the_four_laws(text in corner_text(), dialect in dialects()) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, dialect);
        prop_assert_eq!(check_token_source_laws(&lexer), Vec::new());
    }

    #[test]
    fn every_mode_tiles_the_text_from_any_boundary(
        text in corner_text(), dialect in dialects(), mode in modes(), start in 0usize..64
    ) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, dialect);
        let mut at = (start.min(text.len())..=text.len())
            .find(|offset| text.is_char_boundary(*offset))
            .expect("the end is a boundary");
        loop {
            let token = lexer
                .token_at(ByteOffset::new(u32::try_from(at).expect("small")), mode)
                .expect("a position");
            if token.kind == SyntaxKind::EOF {
                prop_assert_eq!(at, text.len());
                break;
            }
            prop_assert!(!token.text.is_empty());
            prop_assert_eq!(&text[at..at + token.text.len()], token.text);
            at += token.text.len();
        }
    }

    #[test]
    fn the_door_refuses_exactly_off_position(text in corner_text(), mode in modes()) {
        let source = admitted(&text);
        let lexer = Lexer::new(&source, Dialect::Clingo);
        for offset in 0..=text.len() + 2 {
            let answer = lexer.token_at(ByteOffset::new(u32::try_from(offset).expect("small")), mode);
            if offset > text.len() {
                prop_assert!(matches!(answer, Err(PositionRefusal::OutOfBounds(_))));
            } else if !text.is_char_boundary(offset) {
                prop_assert!(matches!(answer, Err(PositionRefusal::NotCharBoundary(_))));
            } else {
                prop_assert!(answer.is_ok());
            }
        }
    }
}

/// A source over one text whose door misbehaves in one named way.
struct Breaching<'a> {
    text: &'a str,
    breach: Breach,
    calls: Cell<u32>,
}

#[derive(Clone, Copy)]
enum Breach {
    /// Answers `EOF` at offset zero whatever the text.
    EarlyEnd,
    /// Answers a token whose text is not the source's slice.
    Synthesized,
    /// Answers a different kind on every call.
    Flaky,
    /// Answers a token past the end of the text.
    Permissive,
}

impl TokenSource for Breaching<'_> {
    fn id(&self) -> SourceId {
        SourceId::new(9)
    }

    fn dialect(&self) -> Dialect {
        Dialect::Clingo
    }

    fn text(&self) -> &str {
        self.text
    }

    fn token_at(&self, at: ByteOffset, _mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let offset = at.get() as usize;
        match self.breach {
            Breach::EarlyEnd => Ok(Token {
                kind: SyntaxKind::EOF,
                text: "",
            }),
            Breach::Synthesized => {
                if offset >= self.text.len() {
                    return Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    });
                }
                Ok(Token {
                    kind: SyntaxKind::IDENT,
                    text: "synthesized",
                })
            }
            Breach::Flaky => {
                let call = self.calls.get();
                self.calls.set(call + 1);
                if offset >= self.text.len() {
                    return Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    });
                }
                let kind = if call.is_multiple_of(2) {
                    SyntaxKind::IDENT
                } else {
                    SyntaxKind::VARIABLE
                };
                Ok(Token {
                    kind,
                    text: &self.text[offset..=offset],
                })
            }
            Breach::Permissive => {
                if offset >= self.text.len() {
                    return Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    });
                }
                // The rest of the text as one token — and an empty answer,
                // never a refusal, inside a character.
                Ok(Token {
                    kind: SyntaxKind::IDENT,
                    text: self.text.get(offset..).unwrap_or(""),
                })
            }
        }
    }
}

fn breaching(text: &str, breach: Breach) -> Breaching<'_> {
    Breaching {
        text,
        breach,
        calls: Cell::new(0),
    }
}

#[test]
fn the_checker_reports_an_early_end() {
    let report = check_token_source_laws(&breaching("pq", Breach::EarlyEnd));
    assert!(report.contains(&TokenSourceLawViolation::Tiling {
        at: ByteOffset::ZERO,
        token: SyntaxKind::EOF,
        len: 0
    }));
}

#[test]
fn the_checker_reports_a_synthesized_token() {
    let report = check_token_source_laws(&breaching("pq", Breach::Synthesized));
    assert!(report.contains(&TokenSourceLawViolation::Tiling {
        at: ByteOffset::ZERO,
        token: SyntaxKind::IDENT,
        len: 11
    }));
}

#[test]
fn the_checker_reports_nondeterminism() {
    let report = check_token_source_laws(&breaching("pq", Breach::Flaky));
    assert!(report.iter().any(|violation| matches!(
        violation,
        TokenSourceLawViolation::Determinism { at, mode: LexMode::Normal } if *at == ByteOffset::ZERO
    )));
}

#[test]
fn the_checker_reports_a_permissive_door() {
    let report = check_token_source_laws(&breaching("pq", Breach::Permissive));
    assert!(report.contains(&TokenSourceLawViolation::Refusal {
        at: ByteOffset::new(3),
        refused: false
    }));
}

#[test]
fn the_checker_reports_a_door_that_answers_inside_a_character() {
    let report = check_token_source_laws(&breaching("é", Breach::Permissive));
    assert!(report.iter().any(|violation| matches!(
        violation,
        TokenSourceLawViolation::Refusal { refused: false, .. }
    )));
}

#[test]
fn a_violation_displays_and_composes_as_an_error() {
    let violation = TokenSourceLawViolation::Slice {
        at: ByteOffset::new(4),
    };
    assert_eq!(
        violation.to_string(),
        "the token at byte 4 is not a slice of the source's text"
    );
    let _: &dyn std::error::Error = &violation;
}
