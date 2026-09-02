//! The file lexer (docs/design/syntax.md §4.4–§4.6): total on every
//! (offset, mode), hand-written to grammar §4 and §6.2–§6.3, with no
//! state but the source and the dialect. Error tokens take the extents
//! syntax.md §4.5 states.

use themelios_base::line::{OffsetOutOfBounds, PositionRefusal};
use themelios_base::source::{NotCharBoundary, Source, SourceId};
use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::token::{LexMode, Token, TokenSource};
use crate::tree::SyntaxKind;

/// The lexer over an admitted source: total on every (offset, mode),
/// hand-written to the grammar of record's lexical section, with no
/// state but the source and the dialect. Cheap to construct — it holds
/// one reference and the dialect — and a pure function thereafter.
#[derive(Clone, Copy, Debug)]
pub struct Lexer<'a> {
    source: &'a Source,
    dialect: Dialect,
}

impl<'a> Lexer<'a> {
    /// A lexer over `source` under `dialect`. Total, O(1).
    pub fn new(source: &'a Source, dialect: Dialect) -> Lexer<'a> {
        Lexer { source, dialect }
    }
}

impl TokenSource for Lexer<'_> {
    fn id(&self) -> SourceId {
        self.source.id()
    }

    fn dialect(&self) -> Dialect {
        self.dialect
    }

    fn text(&self) -> &str {
        self.source.text()
    }

    /// The token at `at` under `mode`. Refuses with `PositionRefusal`
    /// exactly where base refuses: past the end (`OutOfBounds`) or
    /// inside a character (`NotCharBoundary`). O(the token).
    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let text = self.source.text();
        let offset = at.get() as usize;
        if offset > text.len() {
            return Err(PositionRefusal::OutOfBounds(OffsetOutOfBounds {
                offset: at,
                max: self.source.end(),
            }));
        }
        if !text.is_char_boundary(offset) {
            return Err(PositionRefusal::NotCharBoundary(NotCharBoundary {
                offset: at,
            }));
        }
        let (kind, len) = lex(text, offset, mode, self.dialect);
        Ok(Token {
            kind,
            text: &text[offset..offset + len],
        })
    }
}

/// The token beginning at char boundary `at` of `text` under `mode` and
/// `dialect`, as its kind and byte length: `EOF` with length zero at
/// the end, otherwise never zero. Pure; O(the token). The whole lexer
/// is this function; the door adds only the refusals.
pub(crate) fn lex(text: &str, at: usize, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let rest = &text[at..];
    if rest.is_empty() {
        return (SyntaxKind::EOF, 0);
    }
    match mode {
        LexMode::ScriptBody => script_body(rest),
        LexMode::Normal | LexMode::Theory => shared(rest, mode, dialect),
    }
}

/// Grammar §4.8: `#end` where the region ends, else the raw text through
/// the first `#end`, else an error to the end of input.
fn script_body(rest: &str) -> (SyntaxKind, usize) {
    if rest.starts_with(SCRIPT_END) {
        return (SyntaxKind::KW_END, SCRIPT_END.len());
    }
    match rest.find(SCRIPT_END) {
        Some(end) => (SyntaxKind::SCRIPT_BODY, end),
        None => (SyntaxKind::ERROR, rest.len()),
    }
}

/// The script terminator (grammar §4.8).
const SCRIPT_END: &str = "#end";

/// The rules shared by normal and theory mode, dispatching on the first
/// byte; the two modes differ at `#`-words, names (`_` alone), and the
/// operator alphabet.
fn shared(rest: &str, mode: LexMode, dialect: Dialect) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    match bytes[0] {
        b' ' | b'\t' | b'\r' | b'\n' => (SyntaxKind::WHITESPACE, run(bytes, is_whitespace)),
        b'%' if rest.starts_with("%!") => (SyntaxKind::DOC_COMMENT, line_end(rest)),
        b'%' if rest.starts_with("%*") => block_comment(rest, dialect),
        b'%' => (SyntaxKind::LINE_COMMENT, line_end(rest)),
        b'#' if rest.starts_with("#!") => (SyntaxKind::SHEBANG_COMMENT, line_end(rest)),
        b'#' => hash_word(rest, mode),
        b'"' => match dialect {
            Dialect::Clingo => string_clingo(rest),
            Dialect::AspCore2 => string_asp_core_2(rest),
        },
        b'0'..=b'9' => (SyntaxKind::NUMBER, number(bytes)),
        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'\'' => match name(rest) {
            Some(token) => token,
            None if bytes[0] == b'_' && mode == LexMode::Normal => (SyntaxKind::ANONYMOUS, 1),
            None => error_run(rest, mode),
        },
        _ => {
            let punctuation = match mode {
                LexMode::Normal => punctuation_normal(bytes),
                LexMode::Theory => punctuation_theory(bytes),
                LexMode::ScriptBody => None,
            };
            punctuation.unwrap_or_else(|| error_run(rest, mode))
        }
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// The length of the maximal prefix of `bytes` whose bytes satisfy
/// `admits`; the callers' predicates hold only for ASCII bytes, so the
/// prefix ends on a character boundary.
fn run(bytes: &[u8], admits: fn(u8) -> bool) -> usize {
    bytes.iter().take_while(|byte| admits(**byte)).count()
}

/// The length through the end of the line: up to and excluding the line
/// break, or to the end of the text.
fn line_end(rest: &str) -> usize {
    rest.find('\n').unwrap_or(rest.len())
}

/// Grammar §4.1's `BLOCK-COMMENT` and grammar §6.3's replacement.
fn block_comment(rest: &str, dialect: Dialect) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    match dialect {
        Dialect::AspCore2 => match rest[2..].find("*%") {
            Some(close) => (SyntaxKind::BLOCK_COMMENT, close + 4),
            None => (SyntaxKind::ERROR, rest.len()),
        },
        Dialect::Clingo => {
            // The depth is a counter, never a stack (grammar §10). Scanning
            // byte-wise is exact: every byte the rule reads is ASCII, and
            // a multi-byte character's bytes are never `%` or `*`.
            let mut depth = 1usize;
            let mut i = 2;
            while i < bytes.len() {
                if bytes[i] == b'%' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'%') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        return (SyntaxKind::BLOCK_COMMENT, i);
                    }
                } else if bytes[i] == b'%' {
                    // Any other `%` silences the rest of its line, openers
                    // and closers included (grammar §4.1).
                    match rest[i..].find('\n') {
                        Some(to_break) => i += to_break + 1,
                        None => return (SyntaxKind::ERROR, rest.len()),
                    }
                } else {
                    i += 1;
                }
            }
            (SyntaxKind::ERROR, rest.len())
        }
    }
}

/// The `#`-keywords of grammar §4.5 by spelling, in the roster's order;
/// `#sum+` is formed beside the table, since `+` is no name character.
const KEYWORDS: [(&str, SyntaxKind); 25] = [
    ("#const", SyntaxKind::KW_CONST),
    ("#count", SyntaxKind::KW_COUNT),
    ("#defined", SyntaxKind::KW_DEFINED),
    ("#edge", SyntaxKind::KW_EDGE),
    ("#external", SyntaxKind::KW_EXTERNAL),
    ("#false", SyntaxKind::KW_FALSE),
    ("#heuristic", SyntaxKind::KW_HEURISTIC),
    ("#include", SyntaxKind::KW_INCLUDE),
    ("#inf", SyntaxKind::KW_INF),
    ("#infimum", SyntaxKind::KW_INF),
    ("#max", SyntaxKind::KW_MAX),
    ("#maximize", SyntaxKind::KW_MAXIMIZE),
    ("#maximise", SyntaxKind::KW_MAXIMIZE),
    ("#min", SyntaxKind::KW_MIN),
    ("#minimize", SyntaxKind::KW_MINIMIZE),
    ("#minimise", SyntaxKind::KW_MINIMIZE),
    ("#program", SyntaxKind::KW_PROGRAM),
    ("#project", SyntaxKind::KW_PROJECT),
    ("#script", SyntaxKind::KW_SCRIPT),
    ("#show", SyntaxKind::KW_SHOW),
    ("#sum", SyntaxKind::KW_SUM),
    ("#sup", SyntaxKind::KW_SUP),
    ("#supremum", SyntaxKind::KW_SUP),
    ("#theory", SyntaxKind::KW_THEORY),
    ("#true", SyntaxKind::KW_TRUE),
];

/// The two `#`-terms theory mode admits (grammar §4.7): the infimum and
/// supremum, both spellings each.
const THEORY_KEYWORDS: [(&str, SyntaxKind); 4] = [
    ("#inf", SyntaxKind::KW_INF),
    ("#infimum", SyntaxKind::KW_INF),
    ("#sup", SyntaxKind::KW_SUP),
    ("#supremum", SyntaxKind::KW_SUP),
];

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// A `#`-word: a keyword by spelling, or one unknown word by maximal
/// munch (grammar §4.5); `#sum+` outranks `#sum` by length.
fn hash_word(rest: &str, mode: LexMode) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let word_len = 1 + run(&bytes[1..], is_name_char);
    let word = &rest[..word_len];
    let table: &[(&str, SyntaxKind)] = match mode {
        LexMode::Theory => &THEORY_KEYWORDS,
        LexMode::Normal | LexMode::ScriptBody => &KEYWORDS,
    };
    if mode == LexMode::Normal && word == "#sum" && bytes.get(word_len) == Some(&b'+') {
        return (SyntaxKind::KW_SUM_PLUS, word_len + 1);
    }
    match table.iter().find(|(spelling, _)| *spelling == word) {
        Some((_, kind)) => (*kind, word_len),
        None => (SyntaxKind::ERROR, word_len),
    }
}

/// Grammar §4.4's string rule: exactly three escapes, no raw line
/// break; a defect makes the whole token an `ERROR` of the extent
/// syntax.md §4.5 states.
fn string_clingo(rest: &str) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let mut defective = false;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let kind = if defective {
                    SyntaxKind::ERROR
                } else {
                    SyntaxKind::STRING
                };
                return (kind, i + 1);
            }
            b'\n' => return (SyntaxKind::ERROR, i),
            b'\\' => match bytes.get(i + 1) {
                Some(b'"' | b'\\' | b'n') => i += 2,
                Some(b'\n') | None => return (SyntaxKind::ERROR, i + 1),
                Some(_) => {
                    defective = true;
                    i += 1 + char_len(rest, i + 1);
                }
            },
            _ => i += char_len(rest, i),
        }
    }
    (SyntaxKind::ERROR, rest.len())
}

/// Grammar §6.2's string rule: `\"` is the one escape, every other
/// character denotes itself, raw line breaks included, under maximal
/// munch — at `\"` the escape reading wins whenever a later quote can
/// close the string; else that quote closes it.
fn string_asp_core_2(rest: &str) -> (SyntaxKind, usize) {
    let bytes = rest.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (SyntaxKind::STRING, i + 1),
            b'\\' if bytes.get(i + 1) == Some(&b'"') => {
                if rest[i + 2..].contains('"') {
                    i += 2;
                } else {
                    return (SyntaxKind::STRING, i + 2);
                }
            }
            _ => i += char_len(rest, i),
        }
    }
    (SyntaxKind::ERROR, rest.len())
}

/// The byte length of the character at char boundary `at` of `text`.
fn char_len(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(1, char::len_utf8)
}

/// Grammar §4.3's numerals: the prefixed forms take at least one digit
/// of their class, else the token is the `0` alone.
fn number(bytes: &[u8]) -> usize {
    if bytes[0] != b'0' {
        return run(bytes, |byte| byte.is_ascii_digit());
    }
    let class: Option<fn(u8) -> bool> = match bytes.get(1) {
        Some(b'x') => Some(|byte| byte.is_ascii_hexdigit()),
        Some(b'o') => Some(|byte| matches!(byte, b'1'..=b'7')),
        Some(b'b') => Some(|byte| matches!(byte, b'0' | b'1')),
        _ => None,
    };
    match class {
        Some(admits) => match run(&bytes[2..], admits) {
            0 => 1,
            digits => 2 + digits,
        },
        None => 1,
    }
}

/// Grammar §4.2's names: `[_']* [a-z] ['A-Za-z0-9_]*` an identifier,
/// `[_']* [A-Z] ['A-Za-z0-9_]*` a variable; `not` is the keyword; no
/// letter after the prefix is no name.
fn name(rest: &str) -> Option<(SyntaxKind, usize)> {
    let bytes = rest.as_bytes();
    let prefix = run(bytes, |byte| byte == b'_' || byte == b'\'');
    let kind = match bytes.get(prefix)? {
        b'a'..=b'z' => SyntaxKind::IDENT,
        b'A'..=b'Z' => SyntaxKind::VARIABLE,
        _ => return None,
    };
    let len = prefix
        + 1
        + run(&bytes[prefix + 1..], |byte| {
            is_name_char(byte) || byte == b'\''
        });
    if kind == SyntaxKind::IDENT && &rest[..len] == "not" {
        return Some((SyntaxKind::KW_NOT, len));
    }
    Some((kind, len))
}

/// Grammar §4.6's punctuation and operators under maximal munch.
fn punctuation_normal(bytes: &[u8]) -> Option<(SyntaxKind, usize)> {
    let second = bytes.get(1).copied();
    Some(match bytes[0] {
        b'.' if second == Some(b'.') => (SyntaxKind::DOTDOT, 2),
        b'.' => (SyntaxKind::DOT, 1),
        b',' => (SyntaxKind::COMMA, 1),
        b';' => (SyntaxKind::SEMICOLON, 1),
        b':' if second == Some(b'-') => (SyntaxKind::NECK, 2),
        b':' if second == Some(b'~') => (SyntaxKind::WEAK_NECK, 2),
        b':' => (SyntaxKind::COLON, 1),
        b'|' => (SyntaxKind::PIPE, 1),
        b'(' => (SyntaxKind::L_PAREN, 1),
        b')' => (SyntaxKind::R_PAREN, 1),
        b'[' => (SyntaxKind::L_BRACKET, 1),
        b']' => (SyntaxKind::R_BRACKET, 1),
        b'{' => (SyntaxKind::L_BRACE, 1),
        b'}' => (SyntaxKind::R_BRACE, 1),
        b'+' => (SyntaxKind::PLUS, 1),
        b'-' => (SyntaxKind::MINUS, 1),
        b'*' if second == Some(b'*') => (SyntaxKind::STAR_STAR, 2),
        b'*' => (SyntaxKind::STAR, 1),
        b'/' => (SyntaxKind::SLASH, 1),
        b'\\' => (SyntaxKind::BACKSLASH, 1),
        b'^' => (SyntaxKind::CARET, 1),
        b'&' => (SyntaxKind::AMPERSAND, 1),
        b'~' => (SyntaxKind::TILDE, 1),
        b'?' => (SyntaxKind::QUESTION, 1),
        b'@' => (SyntaxKind::AT, 1),
        b'=' if second == Some(b'=') => (SyntaxKind::EQ, 2),
        b'=' => (SyntaxKind::EQ, 1),
        b'!' if second == Some(b'=') => (SyntaxKind::NEQ, 2),
        b'<' if second == Some(b'>') => (SyntaxKind::NEQ, 2),
        b'<' if second == Some(b'=') => (SyntaxKind::LE, 2),
        b'<' => (SyntaxKind::LT, 1),
        b'>' if second == Some(b'=') => (SyntaxKind::GE, 2),
        b'>' => (SyntaxKind::GT, 1),
        _ => return None,
    })
}

/// Grammar §4.7's operator alphabet.
fn is_theory_operator_char(byte: u8) -> bool {
    matches!(
        byte,
        b'/' | b'!'
            | b'<'
            | b'='
            | b'>'
            | b'+'
            | b'-'
            | b'*'
            | b'\\'
            | b'?'
            | b'&'
            | b'@'
            | b'|'
            | b':'
            | b';'
            | b'~'
            | b'^'
            | b'.'
    )
}

/// Grammar §4.7: the structural punctuation as everywhere; a length-one
/// run that is `.`, `;`, or `:` structural; the exact run `:-` the neck;
/// every other maximal run of the operator alphabet one `THEORY_OP`.
fn punctuation_theory(bytes: &[u8]) -> Option<(SyntaxKind, usize)> {
    match bytes[0] {
        b',' => return Some((SyntaxKind::COMMA, 1)),
        b'(' => return Some((SyntaxKind::L_PAREN, 1)),
        b')' => return Some((SyntaxKind::R_PAREN, 1)),
        b'[' => return Some((SyntaxKind::L_BRACKET, 1)),
        b']' => return Some((SyntaxKind::R_BRACKET, 1)),
        b'{' => return Some((SyntaxKind::L_BRACE, 1)),
        b'}' => return Some((SyntaxKind::R_BRACE, 1)),
        _ => {}
    }
    let len = run(bytes, is_theory_operator_char);
    if len == 0 {
        return None;
    }
    Some(match &bytes[..len] {
        b"." => (SyntaxKind::DOT, 1),
        b";" => (SyntaxKind::SEMICOLON, 1),
        b":" => (SyntaxKind::COLON, 1),
        b":-" => (SyntaxKind::NECK, 2),
        _ => (SyntaxKind::THEORY_OP, len),
    })
}

/// Whether some token begins at the start of `rest` under `mode` — the
/// question that ends an error run (syntax.md §4.5).
fn begins_token(rest: &str, mode: LexMode) -> bool {
    let bytes = rest.as_bytes();
    match bytes[0] {
        b' '
        | b'\t'
        | b'\r'
        | b'\n'
        | b'%'
        | b'#'
        | b'"'
        | b'0'..=b'9'
        | b'a'..=b'z'
        | b'A'..=b'Z' => true,
        b'_' | b'\'' => name(rest).is_some() || (bytes[0] == b'_' && mode == LexMode::Normal),
        _ => match mode {
            LexMode::Normal => punctuation_normal(bytes).is_some(),
            LexMode::Theory => punctuation_theory(bytes).is_some(),
            LexMode::ScriptBody => false,
        },
    }
}

/// The maximal run of characters that begin no token, as one `ERROR`
/// token (syntax.md §4.5).
fn error_run(rest: &str, mode: LexMode) -> (SyntaxKind, usize) {
    let mut len = char_len(rest, 0);
    while len < rest.len() && !begins_token(&rest[len..], mode) {
        len += char_len(rest, len);
    }
    (SyntaxKind::ERROR, len)
}

/// What a lexical `ERROR` token is, read from its text, the character
/// after it, and the mode and dialect it was formed under
/// (docs/design/syntax.md §4.5): the parser raises exactly this
/// diagnostic when it places the token.
pub(crate) struct LexicalDefect {
    /// The diagnostic's kind.
    pub(crate) kind: crate::diagnostic::SyntaxErrorKind,
    /// The primary span, relative to the token's start.
    pub(crate) primary: std::ops::Range<usize>,
    /// Whether the literal's whole extent is a related locus (a bad
    /// escape inside a string).
    pub(crate) literal_extent: bool,
}

/// Classifies an `ERROR` token of `text`, followed in its source by
/// `following` (`None` at end of input), formed under `mode` and
/// `dialect`. Total.
pub(crate) fn classify_error(
    text: &str,
    following: Option<char>,
    mode: LexMode,
    dialect: Dialect,
) -> LexicalDefect {
    use crate::diagnostic::SyntaxErrorKind;
    let whole = 0..text.len();
    if mode == LexMode::ScriptBody {
        return LexicalDefect {
            kind: SyntaxErrorKind::UnterminatedScript,
            primary: whole,
            literal_extent: false,
        };
    }
    if text.starts_with("%*") {
        return LexicalDefect {
            kind: SyntaxErrorKind::UnterminatedBlockComment,
            primary: 0..2,
            literal_extent: false,
        };
    }
    if text.starts_with('#') {
        return LexicalDefect {
            kind: SyntaxErrorKind::UnknownHashWord,
            primary: whole,
            literal_extent: false,
        };
    }
    if text.starts_with('"') {
        return string_defect(text, following, dialect);
    }
    if mode == LexMode::Theory && text.chars().all(|c| c == '_') {
        return LexicalDefect {
            kind: SyntaxErrorKind::AnonymousInTheoryExpression,
            primary: whole,
            literal_extent: false,
        };
    }
    LexicalDefect {
        kind: SyntaxErrorKind::UnexpectedCharacters,
        primary: whole,
        literal_extent: false,
    }
}

/// The defect of a malformed string token (grammar §4.4, §6.2): the
/// first bad escape, at the backslash; a raw line break under the
/// clingo rule; or unterminated at end of input — a lone trailing
/// backslash among them, since it never closed (§4.5, §6.5).
fn string_defect(text: &str, following: Option<char>, dialect: Dialect) -> LexicalDefect {
    use crate::diagnostic::{StringDefect, SyntaxErrorKind};
    let bytes = text.as_bytes();
    if dialect == Dialect::Clingo {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                match text[i + 1..].chars().next() {
                    Some('"' | '\\' | 'n') => i += 2,
                    Some(bad) => {
                        return LexicalDefect {
                            kind: SyntaxErrorKind::MalformedString {
                                defect: StringDefect::InvalidEscape(bad),
                            },
                            primary: i..i + 1 + bad.len_utf8(),
                            literal_extent: true,
                        };
                    }
                    None => {
                        // The backslash is the token's last byte. When a real
                        // character follows it — a raw line break, `\` at the
                        // end of its line — that character is the bad escape.
                        // At true end of input none does: the string simply
                        // never closed (the authority reads it so too), so it
                        // falls through to `Unterminated` below.
                        if let Some(bad) = following {
                            return LexicalDefect {
                                kind: SyntaxErrorKind::MalformedString {
                                    defect: StringDefect::InvalidEscape(bad),
                                },
                                primary: i..i + 1,
                                literal_extent: true,
                            };
                        }
                        break;
                    }
                }
            } else {
                i += char_len(text, i);
            }
        }
        if following == Some('\n') {
            return LexicalDefect {
                kind: SyntaxErrorKind::MalformedString {
                    defect: StringDefect::RawLineBreak,
                },
                primary: 0..text.len(),
                literal_extent: false,
            };
        }
    }
    LexicalDefect {
        kind: SyntaxErrorKind::MalformedString {
            defect: StringDefect::Unterminated,
        },
        primary: 0..1,
        literal_extent: false,
    }
}

// The roster's names read as the tokens they are; qualifying a hundred
// of them in these tables adds noise, not information.
#[cfg(test)]
#[allow(clippy::enum_glob_use)]
mod tests {
    use themelios_base::line::PositionRefusal;
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::token::{LexMode, TokenSource};
    use crate::tree::SyntaxKind::{self, *};

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    /// The tokens of `text` tiled under one mode and dialect, as
    /// `(kind, text)` pairs, `EOF` excluded.
    fn tile(text: &str, mode: LexMode, dialect: Dialect) -> Vec<(SyntaxKind, String)> {
        let source = admitted(text);
        let lexer = Lexer::new(&source, dialect);
        let mut at = 0u32;
        let mut tokens = Vec::new();
        loop {
            let token = lexer
                .token_at(ByteOffset::new(at), mode)
                .expect("a position");
            if token.kind == EOF {
                assert_eq!(at as usize, text.len(), "EOF only at the end");
                return tokens;
            }
            assert!(!token.text.is_empty(), "no zero-length token");
            tokens.push((token.kind, token.text.to_owned()));
            at += u32::try_from(token.text.len()).expect("test text is small");
        }
    }

    fn normal(text: &str) -> Vec<(SyntaxKind, String)> {
        tile(text, LexMode::Normal, Dialect::Clingo)
    }

    fn theory(text: &str) -> Vec<(SyntaxKind, String)> {
        tile(text, LexMode::Theory, Dialect::Clingo)
    }

    fn kinds(tokens: &[(SyntaxKind, String)]) -> Vec<SyntaxKind> {
        tokens.iter().map(|(kind, _)| *kind).collect()
    }

    fn texts(tokens: &[(SyntaxKind, String)]) -> Vec<&str> {
        tokens.iter().map(|(_, text)| text.as_str()).collect()
    }

    #[test]
    fn a_rule_lexes_to_its_tokens() {
        let tokens = normal("p(X) :- q(X), X == 1.");
        assert_eq!(
            kinds(&tokens),
            [
                IDENT, L_PAREN, VARIABLE, R_PAREN, WHITESPACE, NECK, WHITESPACE, IDENT, L_PAREN,
                VARIABLE, R_PAREN, COMMA, WHITESPACE, VARIABLE, WHITESPACE, EQ, WHITESPACE, NUMBER,
                DOT
            ]
        );
    }

    #[test]
    fn numerals_follow_the_grammar_including_its_recorded_octal_oddity() {
        assert_eq!(texts(&normal("0o10")), ["0o1", "0"]);
        assert_eq!(kinds(&normal("0o10")), [NUMBER, NUMBER]);
        assert_eq!(kinds(&normal("0X1F")), [NUMBER, VARIABLE]);
        assert_eq!(kinds(&normal("007")), [NUMBER, NUMBER, NUMBER]);
        assert_eq!(kinds(&normal("0x")), [NUMBER, IDENT]);
        assert_eq!(
            texts(&normal("0x1F 0b101 0o7 42")),
            ["0x1F", " ", "0b101", " ", "0o7", " ", "42"]
        );
    }

    #[test]
    fn names_take_primes_and_underscores() {
        assert_eq!(
            kinds(&normal("_p 'a a' p'' _X X'")),
            [
                IDENT, WHITESPACE, IDENT, WHITESPACE, IDENT, WHITESPACE, IDENT, WHITESPACE,
                VARIABLE, WHITESPACE, VARIABLE
            ]
        );
        // A word that merely begins with a keyword (`nota`) is still a name, not the keyword.
        assert_eq!(
            kinds(&normal("not nota default")),
            [KW_NOT, WHITESPACE, IDENT, WHITESPACE, IDENT]
        );
    }

    #[test]
    fn the_lone_underscore_is_anonymous() {
        assert_eq!(kinds(&normal("__")), [ANONYMOUS, ANONYMOUS]);
        assert_eq!(kinds(&normal("_1")), [ANONYMOUS, NUMBER]);
    }

    #[test]
    fn hash_words_are_keywords_or_one_unknown_word() {
        assert_eq!(kinds(&normal("#sums")), [ERROR]);
        assert_eq!(texts(&normal("#counting")), ["#counting"]);
        assert_eq!(kinds(&normal("#end")), [ERROR]);
        assert_eq!(kinds(&normal("#sum+")), [KW_SUM_PLUS]);
        assert_eq!(kinds(&normal("#sum +")), [KW_SUM, WHITESPACE, PLUS]);
        assert_eq!(
            kinds(&normal("#inf #infimum #sup #supremum")),
            [
                KW_INF, WHITESPACE, KW_INF, WHITESPACE, KW_SUP, WHITESPACE, KW_SUP
            ]
        );
        assert_eq!(
            kinds(&normal("#minimise #maximize")),
            [KW_MINIMIZE, WHITESPACE, KW_MAXIMIZE]
        );
        assert_eq!(kinds(&normal("#")), [ERROR]);
    }

    #[test]
    fn maximal_munch_resolves_the_operators() {
        assert_eq!(kinds(&normal("...")), [DOTDOT, DOT]);
        assert_eq!(kinds(&normal("***")), [STAR_STAR, STAR]);
        assert_eq!(kinds(&normal(":-:~:")), [NECK, WEAK_NECK, COLON]);
        assert_eq!(
            kinds(&normal("== = != <> < <= > >=")),
            [
                EQ, WHITESPACE, EQ, WHITESPACE, NEQ, WHITESPACE, NEQ, WHITESPACE, LT, WHITESPACE,
                LE, WHITESPACE, GT, WHITESPACE, GE
            ]
        );
        assert_eq!(
            kinds(&normal("|()[]{}+-*/\\^&~?@,;")),
            [
                PIPE, L_PAREN, R_PAREN, L_BRACKET, R_BRACKET, L_BRACE, R_BRACE, PLUS, MINUS, STAR,
                SLASH, BACKSLASH, CARET, AMPERSAND, TILDE, QUESTION, AT, COMMA, SEMICOLON
            ]
        );
    }

    #[test]
    fn characters_that_begin_no_token_join_one_error_run() {
        assert_eq!(kinds(&normal("!")), [ERROR]);
        assert_eq!(texts(&normal("$$$")), ["$$$"]);
        assert_eq!(kinds(&normal("$x")), [ERROR, IDENT]);
        assert_eq!(kinds(&normal("$#foo")), [ERROR, ERROR]);
        assert_eq!(texts(&normal("é€p")), ["é€", "p"]);
        assert_eq!(kinds(&normal("\u{1}\u{2}p")), [ERROR, IDENT]);
        assert_eq!(kinds(&normal("'")), [ERROR]);
    }

    #[test]
    fn strings_under_the_clingo_rule() {
        assert_eq!(
            kinds(&normal(r#""a\nb" "a\"b" "a\\b""#)),
            [STRING, WHITESPACE, STRING, WHITESPACE, STRING]
        );
        assert_eq!(kinds(&normal(r#""a\b""#)), [ERROR]);
        assert_eq!(
            texts(&normal(r#"p("a\qb"). q."#)),
            ["p", "(", r#""a\qb""#, ")", ".", " ", "q", "."]
        );
        assert_eq!(texts(&normal("\"ab\ncd\"")), ["\"ab", "\n", "cd", "\""]);
        assert_eq!(
            kinds(&normal("\"ab\ncd\"")),
            [ERROR, WHITESPACE, IDENT, ERROR]
        );
        assert_eq!(kinds(&normal("\"abc")), [ERROR]);
        assert_eq!(texts(&normal("\"a\\\nb\"")), ["\"a\\", "\n", "b", "\""]);
    }

    #[test]
    fn a_trailing_backslash_classifies_by_what_follows_it() {
        use crate::diagnostic::{StringDefect, SyntaxErrorKind::MalformedString};
        // clingo `"abc\` at true end of input: no line ends it, so the
        // string simply never closed — Unterminated, as the authority reads
        // it, not a fabricated "backslash at the end of its line"
        // (docs/design/syntax.md §4.5, §6.5).
        let defect = classify_error("\"abc\\", None, LexMode::Normal, Dialect::Clingo);
        assert!(matches!(
            defect.kind,
            MalformedString {
                defect: StringDefect::Unterminated
            }
        ));
        // A real line break after the backslash is the bad escape it names.
        let defect = classify_error("\"abc\\", Some('\n'), LexMode::Normal, Dialect::Clingo);
        assert!(matches!(
            defect.kind,
            MalformedString {
                defect: StringDefect::InvalidEscape('\n')
            }
        ));
    }

    #[test]
    fn strings_under_the_asp_core_2_rule() {
        let core = |text: &str| tile(text, LexMode::Normal, Dialect::AspCore2);
        assert_eq!(
            kinds(&core(r#""a\nb" "a\b""#)),
            [STRING, WHITESPACE, STRING]
        );
        assert_eq!(kinds(&core("\"a\nb\"")), [STRING]);
        assert_eq!(texts(&core(r#""a\" b" x"#)), [r#""a\" b""#, " ", "x"]);
        assert_eq!(texts(&core(r#""a\""#)), [r#""a\""#]);
        assert_eq!(kinds(&core(r#""a\""#)), [STRING]);
        assert_eq!(kinds(&core("\"abc")), [ERROR]);
    }

    #[test]
    fn comments_take_their_forms() {
        assert_eq!(
            kinds(&normal("%! doc\np.")),
            [DOC_COMMENT, WHITESPACE, IDENT, DOT]
        );
        assert_eq!(
            kinds(&normal("%%! x\n% ! x")),
            [LINE_COMMENT, WHITESPACE, LINE_COMMENT]
        );
        assert_eq!(
            kinds(&normal("#! shebang\np.")),
            [SHEBANG_COMMENT, WHITESPACE, IDENT, DOT]
        );
        assert_eq!(texts(&normal("% c\r\np.")), ["% c\r", "\n", "p", "."]);
    }

    #[test]
    fn block_comments_nest_and_silence_under_clingo() {
        assert_eq!(
            kinds(&normal("%* a %* b *% c *%p.")),
            [BLOCK_COMMENT, IDENT, DOT]
        );
        assert_eq!(
            kinds(&normal("%* a % *%\nb *%p.")),
            [BLOCK_COMMENT, IDENT, DOT]
        );
        assert_eq!(kinds(&normal("%* a % *% b *%")), [ERROR]);
        assert_eq!(kinds(&normal("%* %* *%")), [ERROR]);
        assert_eq!(kinds(&normal("%* %! *%")), [ERROR]);
    }

    #[test]
    fn block_comments_do_neither_under_asp_core_2() {
        let core = |text: &str| tile(text, LexMode::Normal, Dialect::AspCore2);
        assert_eq!(
            kinds(&core("%* a % *% b *%")),
            [
                BLOCK_COMMENT,
                WHITESPACE,
                IDENT,
                WHITESPACE,
                STAR,
                LINE_COMMENT
            ]
        );
        assert_eq!(kinds(&core("%* %* *%")), [BLOCK_COMMENT]);
        assert_eq!(kinds(&core("%* %! *%")), [BLOCK_COMMENT]);
    }

    #[test]
    fn theory_mode_forms_operator_runs() {
        assert_eq!(kinds(&theory(":-:")), [THEORY_OP]);
        assert_eq!(
            texts(&theory(".. ;; :: := :~")),
            ["..", " ", ";;", " ", "::", " ", ":=", " ", ":~"]
        );
        assert_eq!(
            theory(".. ;; :: := :~")
                .iter()
                .filter(|(k, _)| *k == THEORY_OP)
                .count(),
            5
        );
        assert_eq!(
            kinds(&theory(". ; : :-")),
            [
                DOT, WHITESPACE, SEMICOLON, WHITESPACE, COLON, WHITESPACE, NECK
            ]
        );
        assert_eq!(kinds(&theory("_")), [ERROR]);
        assert_eq!(kinds(&theory("__ _a")), [ERROR, WHITESPACE, IDENT]);
        assert_eq!(
            kinds(&theory("not #inf #count")),
            [KW_NOT, WHITESPACE, KW_INF, WHITESPACE, ERROR]
        );
        assert_eq!(
            kinds(&theory("x <= 3")),
            [IDENT, WHITESPACE, THEORY_OP, WHITESPACE, NUMBER]
        );
        assert_eq!(kinds(&theory("!|@")), [THEORY_OP]);
        assert_eq!(
            kinds(&theory(",()[]{}")),
            [
                COMMA, L_PAREN, R_PAREN, L_BRACKET, R_BRACKET, L_BRACE, R_BRACE
            ]
        );
        assert_eq!(kinds(&theory("#sum+")), [ERROR, THEORY_OP]);
        assert_eq!(kinds(&theory("$")), [ERROR]);
    }

    #[test]
    fn script_mode_answers_the_body_or_the_terminator() {
        let script = |text: &str| tile(text, LexMode::ScriptBody, Dialect::Clingo);
        assert_eq!(texts(&script("#end")), ["#end"]);
        assert_eq!(kinds(&script("#end")), [KW_END]);
        assert_eq!(kinds(&script(" x = 1 #end")), [SCRIPT_BODY, KW_END]);
        assert_eq!(texts(&script(" x = 1 #end")), [" x = 1 ", "#end"]);
        assert_eq!(kinds(&script("no end")), [ERROR]);
        assert_eq!(
            kinds(&script("% not a comment #end.")),
            [SCRIPT_BODY, KW_END, ERROR]
        );
    }

    #[test]
    fn the_door_refuses_where_span_meets_text_as_base_refuses() {
        let source = admitted("é");
        let lexer = Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(
            lexer.token_at(ByteOffset::new(1), LexMode::Normal),
            Err(PositionRefusal::NotCharBoundary(_))
        ));
        assert!(matches!(
            lexer.token_at(ByteOffset::new(3), LexMode::Normal),
            Err(PositionRefusal::OutOfBounds(_))
        ));
        let end = lexer
            .token_at(ByteOffset::new(2), LexMode::Normal)
            .expect("the end is a position");
        assert_eq!(end.kind, EOF);
        assert_eq!(end.text, "");
    }

    #[test]
    fn a_nested_block_comment_opener_past_the_second_byte_advances_by_two_not_by_doubling() {
        // The inner `%*` sits at byte four, where `i += 2` (byte six) and
        // `i *= 2` (byte eight, a `%` that would silence the line) part ways:
        // the doubling misreads the comment as an unterminated ERROR.
        assert_eq!(
            lex("%*aa%*b*%c*%", 0, LexMode::Normal, Dialect::Clingo),
            (BLOCK_COMMENT, 12)
        );
    }

    #[test]
    fn a_clingo_escaped_quote_is_read_whole_not_closed_at_its_quote() {
        // `\"` at byte one: `i += 2` steps past the escaped quote; `i *= 2`
        // (byte two) lands on that quote and would close the string one byte
        // in. The maximal-munch length is the witness.
        assert_eq!(
            lex(r#""\"a""#, 0, LexMode::Normal, Dialect::Clingo),
            (STRING, 5)
        );
    }

    #[test]
    fn an_error_run_ends_where_an_underscore_begins_a_token() {
        // `_` begins a token (the anonymous variable), so an error run before
        // it stops there; the `b'_' | b'\''` arm and both its predicates are
        // what `begins_token` answers `true` with.
        assert_eq!(kinds(&normal("$_")), [ERROR, ANONYMOUS]);
    }

    #[test]
    fn a_theory_mode_error_that_is_not_all_underscores_is_unexpected_characters() {
        // The anonymous-in-theory classification is theory mode AND an
        // all-underscore token; a lone `$` in theory mode is neither past the
        // first conjunct, so it is plain unexpected characters.
        assert!(matches!(
            classify_error("$", None, LexMode::Theory, Dialect::Clingo).kind,
            crate::diagnostic::SyntaxErrorKind::UnexpectedCharacters
        ));
        // ...while an all-underscore theory token is the anonymous defect,
        // holding the `&&` against the always-true reading.
        assert!(matches!(
            classify_error("__", None, LexMode::Theory, Dialect::Clingo).kind,
            crate::diagnostic::SyntaxErrorKind::AnonymousInTheoryExpression
        ));
    }

    #[test]
    fn a_trailing_backslash_before_a_line_break_is_flagged_at_the_backslash_alone() {
        // The bad escape is the line break the backslash precedes; the primary
        // span is the backslash itself — one byte wide, `i..i + 1`.
        let defect = string_defect("\"ab\\", Some('\n'), Dialect::Clingo);
        assert!(matches!(
            defect.kind,
            crate::diagnostic::SyntaxErrorKind::MalformedString {
                defect: crate::diagnostic::StringDefect::InvalidEscape('\n')
            }
        ));
        assert_eq!(defect.primary, 3..4, "the backslash alone, one byte wide");
    }

    #[test]
    fn asp_core_2_strings_read_escaped_quotes_by_maximal_munch() {
        let core = |text: &str| tile(text, LexMode::Normal, Dialect::AspCore2);
        // `\"` closes nothing while a later quote can: the whole `"a\"b"` is
        // one string. `bytes.get(i + 1)`, the `i += 2` skip, and the guard all
        // hold here.
        assert_eq!(kinds(&core(r#""a\"b""#)), [STRING]);
        assert_eq!(
            lex(r#""a\"b""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 6)
        );
        // With no later quote the `\"` is the close: `"a\"` is `"a\"`, length
        // four — the `return (STRING, i + 2)` branch, its length the witness.
        assert_eq!(
            lex(r#""a\""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 4)
        );
        // A backslash before a non-quote is literal, not an escaped quote.
        assert_eq!(
            lex(r#""a\b""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 5)
        );
        // The escaped quote at a byte past two, where the index arithmetic
        // (`i + 2` for the later-quote check, the skip, and the closing return)
        // parts from its doubling: `"ab\""` closes after both quotes (len six),
        // `"ab\"` closes at the escaped quote (len five), and `"abcd\"e"` runs
        // to its final quote (len nine) — the skip that steps by two, not to
        // double.
        assert_eq!(
            lex(r#""ab\"""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 6)
        );
        assert_eq!(
            lex(r#""ab\""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 5)
        );
        assert_eq!(
            lex(r#""abcd\"e""#, 0, LexMode::Normal, Dialect::AspCore2),
            (STRING, 9)
        );
    }
}
