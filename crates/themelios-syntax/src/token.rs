//! Tokens, lexical modes, and the token-source door
//! (docs/design/syntax.md §4.2–§4.3): where the parser's tokens come
//! from, the four laws every source is bound by, and their checker.

use std::fmt;

use themelios_base::line::PositionRefusal;
use themelios_base::source::SourceId;
use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::tree::SyntaxKind;

/// One token as a source hands it to the parser: its kind and its text.
/// The text is a slice of the source's own text (the slice law); its
/// length is the token's extent, so no length field can disagree with
/// it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Token<'a> {
    /// The kind.
    pub kind: SyntaxKind,
    /// The text, a slice of the source's text.
    pub text: &'a str,
}

/// The lexical mode in force at a token's start — a language fact
/// (grammar §4.7, §4.8), not an implementation choice: inside a theory
/// atom's elements and guard, and at the operator positions of a
/// `#theory` definition, operator runs are one token and `not` is an
/// operator; between `#script(…)` and `#end`, nothing lexes. The parser
/// owns the modes and tells the source which one it wants for each
/// token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LexMode {
    /// Grammar §4's rules as written.
    Normal,
    /// Grammar §4.7's theory-expression lexing.
    Theory,
    /// Grammar §4.8's script region: the raw body, or `#end`.
    ScriptBody,
}

/// Where tokens come from. The file lexer is one source; the macro tier
/// is another (grammar §9: Rust's lexer is the macro dialect's lexical
/// layer, and the mapping onto the roster is the macro crate's). One
/// parser reads both.
///
/// A source is bound by four laws (docs/design/syntax.md §4.3): tiling —
/// from offset zero under `Normal` mode the tokens partition the text
/// and end at `EOF` at its length; slice — every token's text is a
/// slice of `text()` at its offset; determinism — same offset and mode,
/// same answer; refusal — `token_at` refuses exactly at offsets that are
/// not positions of the text. `check_token_source_laws` holds them.
pub trait TokenSource {
    /// The identity the host minted for this text (base §3.1).
    fn id(&self) -> SourceId;
    /// The dialect the source lexes under; the parser reads it here so
    /// lexer and parser cannot disagree.
    fn dialect(&self) -> Dialect;
    /// The whole text this source owns and tiles. Every token is a
    /// slice of it, and every span this crate hands back is in its
    /// coordinates.
    fn text(&self) -> &str;
    /// The token that begins at `at` under `mode`: the longest token the
    /// mode's rules form there, an `ERROR` token, or `EOF` with empty
    /// text at the text's end. Refuses with `PositionRefusal` exactly
    /// when `at` is not a position of the text — past its end, or inside
    /// a character. Never panics. O(the token returned).
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal>;
}

/// A breach of one of the four laws, as the checker reports it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TokenSourceLawViolation {
    /// Tiling broke at `at`: `EOF` before the text's end (`token` is
    /// `EOF`, `len` zero), a zero-length token, or a token running past
    /// the end.
    Tiling {
        /// Where the tiling broke.
        at: ByteOffset,
        /// The token the source answered there.
        token: SyntaxKind,
        /// Its length.
        len: u32,
    },
    /// The token at `at` is not the slice of the source's text there.
    Slice {
        /// The offending token's offset.
        at: ByteOffset,
    },
    /// The source answered differently when asked again at `at` under
    /// `mode`.
    Determinism {
        /// The offset asked twice.
        at: ByteOffset,
        /// The mode asked under.
        mode: LexMode,
    },
    /// The refusal law broke at `at`: `refused` is `true` where the
    /// source refused an offset it owed a token, `false` where it
    /// answered an offset that is not a position of its text.
    Refusal {
        /// The offset probed.
        at: ByteOffset,
        /// What the source did.
        refused: bool,
    },
}

impl fmt::Display for TokenSourceLawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenSourceLawViolation::Tiling { at, token, len } => write!(
                f,
                "tiling breaks at byte {}: the source answered {token} of length {len} there",
                at.get()
            ),
            TokenSourceLawViolation::Slice { at } => {
                write!(
                    f,
                    "the token at byte {} is not a slice of the source's text",
                    at.get()
                )
            }
            TokenSourceLawViolation::Determinism { at, mode } => write!(
                f,
                "the source answered differently when asked again at byte {} under {mode:?}",
                at.get()
            ),
            TokenSourceLawViolation::Refusal { at, refused: true } => {
                write!(
                    f,
                    "the source refused byte {}, a position it owes a token",
                    at.get()
                )
            }
            TokenSourceLawViolation::Refusal { at, refused: false } => {
                write!(
                    f,
                    "the source answered byte {}, which is not a position of its text",
                    at.get()
                )
            }
        }
    }
}

impl std::error::Error for TokenSourceLawViolation {}

/// The laws, checkable: tiles the source under `Normal` mode from zero,
/// checking tiling and the slice law at every token, probing
/// determinism by re-asking each token, and probing refusal once past
/// the end and once inside a multi-byte character of each token that
/// has one. Total; O(text) for a lawful source; an empty report is the
/// laws holding. What it does not exercise: `Theory` and `ScriptBody`
/// mode, which an implementor holds under its own tests over inputs its
/// parser reaches.
pub fn check_token_source_laws(source: &impl TokenSource) -> Vec<TokenSourceLawViolation> {
    let text = source.text();
    let mut violations = Vec::new();
    // A text longer than the door's coordinates can address cannot be
    // tiled by any source: reported as a tiling breach at the last
    // addressable offset.
    let Ok(end) = u32::try_from(text.len()) else {
        violations.push(TokenSourceLawViolation::Tiling {
            at: ByteOffset::new(u32::MAX),
            token: SyntaxKind::EOF,
            len: 0,
        });
        return violations;
    };
    let mut at = 0u32;
    loop {
        let offset = ByteOffset::new(at);
        let Ok(token) = source.token_at(offset, LexMode::Normal) else {
            violations.push(TokenSourceLawViolation::Refusal {
                at: offset,
                refused: true,
            });
            return violations;
        };
        if source.token_at(offset, LexMode::Normal) != Ok(token) {
            violations.push(TokenSourceLawViolation::Determinism {
                at: offset,
                mode: LexMode::Normal,
            });
        }
        if token.kind == SyntaxKind::EOF {
            if at != end {
                violations.push(TokenSourceLawViolation::Tiling {
                    at: offset,
                    token: SyntaxKind::EOF,
                    len: 0,
                });
            }
            break;
        }
        let len = u32::try_from(token.text.len()).unwrap_or(u32::MAX);
        let past_the_end = at.checked_add(len).is_none_or(|next| next > end);
        if len == 0 || past_the_end {
            violations.push(TokenSourceLawViolation::Tiling {
                at: offset,
                token: token.kind,
                len,
            });
            break;
        }
        let start = at as usize;
        if text.get(start..start + token.text.len()) != Some(token.text) {
            violations.push(TokenSourceLawViolation::Slice { at: offset });
        }
        if let Some((index, _)) = token.text.char_indices().find(|(_, c)| c.len_utf8() > 1) {
            let inside = ByteOffset::new(at + u32::try_from(index).unwrap_or(0) + 1);
            if source.token_at(inside, LexMode::Normal).is_ok() {
                violations.push(TokenSourceLawViolation::Refusal {
                    at: inside,
                    refused: false,
                });
            }
        }
        at += len;
    }
    if let Some(past) = end.checked_add(1) {
        let past = ByteOffset::new(past);
        if source.token_at(past, LexMode::Normal).is_ok() {
            violations.push(TokenSourceLawViolation::Refusal {
                at: past,
                refused: false,
            });
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;

    /// A source over `é` that answers a position inside that one multi-byte
    /// character instead of refusing it — the refusal-law breach the checker
    /// catches by probing one byte into each multi-byte token.
    struct AnswersInsideChar(Source);

    impl TokenSource for AnswersInsideChar {
        fn id(&self) -> SourceId {
            self.0.id()
        }
        fn dialect(&self) -> Dialect {
            Dialect::Clingo
        }
        fn text(&self) -> &str {
            self.0.text()
        }
        fn token_at(&self, at: ByteOffset, _mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
            match at.get() {
                0 => Ok(Token {
                    kind: SyntaxKind::IDENT,
                    text: self.0.text(),
                }),
                _ => Ok(Token {
                    kind: SyntaxKind::EOF,
                    text: "",
                }),
            }
        }
    }

    #[test]
    fn the_checker_probes_one_byte_into_each_multi_byte_character() {
        // `é` is two bytes; byte one is not a position, so a source that
        // answers there breaches the refusal law. The checker must probe
        // inside the character to see it.
        let source = Source::new(SourceId::new(0), "é".to_owned()).expect("admits");
        let violations = check_token_source_laws(&AnswersInsideChar(source));
        assert!(
            violations.iter().any(|v| matches!(
                v,
                TokenSourceLawViolation::Refusal { at, refused: false } if *at == ByteOffset::new(1)
            )),
            "the inside-character probe is missing: {violations:?}"
        );
    }
}
