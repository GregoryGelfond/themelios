//! The macro-dialect token source (grammar §9; docs/design/macros.md §6):
//! the engine that walks a Rust token stream, maps each token onto the
//! syntax roster, and assembles a themelios text of which it is the
//! authoritative tiler — answering [`token_at`](TokenSource::token_at)
//! from its own structured tiles, so no re-lex arises and abutting tokens
//! never fuse. Alongside the text it keeps a span map (each tile's
//! originating `proc_macro` span) and the captured splices.
//!
//! The four token-source laws (tiling, slice, determinism, refusal —
//! syntax §4.3) are what this source owes; because it answers from tiles,
//! they hold by construction over the whole assembled text.
// The mapping engine's public surface is reached by the macro entry
// points a later increment wires; until those exist, this module's own
// tests are its only callers, so the not-yet-wired surface would read as
// dead. The allow is removed when the entry points arrive.
#![allow(dead_code)]

use proc_macro2::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};
use themelios_syntax::base::line::{OffsetOutOfBounds, PositionRefusal};
use themelios_syntax::base::source::{NotCharBoundary, SourceId};
use themelios_syntax::base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::fusion::{LexContext, Separator, separator_between};
use themelios_syntax::parse::STRING_INPUT_SOURCE_ID;
use themelios_syntax::token::{LexMode, Token, TokenSource};
use themelios_syntax::tree::{SyntaxKind, TextRange};

/// A themelios text assembled from a Rust token stream, tiled by the
/// macro dialect (grammar §9). It owns the assembled `text`, an ordered
/// gap-free cover of it in `tiles` (one per token, the trailing `EOF`
/// included), the `proc_macro` span each tile came from in `spans`, and
/// the captured Rust expression of each splice in `splices`.
#[derive(Debug)]
pub struct MacroSource {
    text: String,
    tiles: Vec<Tile>,
    spans: Vec<Span>,
    splices: Vec<Splice>,
    dialect: Dialect,
}

/// One tile: a half-open byte region `[start, start + len)` of the
/// assembled text and the roster kind it carries.
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    start: u32,
    len: u32,
    kind: SyntaxKind,
}

/// One captured splice: the byte region of its `SPLICE` tile in the
/// assembled text, and the Rust expression the marker bound (grammar §9),
/// held for the codegen a later increment writes.
#[derive(Debug)]
pub struct Splice {
    range: std::ops::Range<u32>,
    expr: TokenStream,
}

/// A dialect error: a Rust token the mapping does not name (grammar §9),
/// carrying the `proc_macro` span to blame and the message to state.
#[derive(Debug)]
pub struct MapError {
    /// The span of the offending Rust token.
    pub span: Span,
    /// What was wrong.
    pub message: String,
}

/// The lexical context every separator question is asked under: the one
/// dialect this crate parses (grammar §3) and normal mode. The assembled
/// tiles carry normal-mode operator kinds, so the fusion oracle
/// (syntax §10) is consulted under the same mode it must agree with.
const CONTEXT: LexContext = LexContext {
    dialect: Dialect::Clingo,
    mode: LexMode::Normal,
};

impl MacroSource {
    /// Assembles the token source for `input` under the macro dialect
    /// (grammar §9). When `entry_keyword` is `Some(word)` the assembled
    /// text opens with the `#word` keyword tile a directive macro
    /// supplies from its own name (docs/design/macros.md §8). Refuses
    /// [`MapError`] at the span of the first Rust token the dialect does
    /// not name.
    ///
    /// # Errors
    ///
    /// Returns [`MapError`] for any token grammar §9 leaves unnamed — a
    /// float, char, or byte literal, a suffixed numeral, a raw identifier,
    /// an identifier no name class matches, a detached `#`, a `$` without
    /// an operand — and for the corners a later increment maps by value (a
    /// string whose value grammar §4.4 cannot spell verbatim).
    pub fn build(input: TokenStream, entry_keyword: Option<&str>) -> Result<MacroSource, MapError> {
        let mut assembler = Assembler::default();
        if let Some(word) = entry_keyword {
            let kind = keyword_kind(word).ok_or_else(|| MapError {
                span: Span::call_site(),
                message: format!("`#{word}` is not a directive keyword"),
            })?;
            // The directive keyword is the macro's own, not a Rust token,
            // so it maps to the call site (docs/design/macros.md §8).
            assembler.emit(kind, &format!("#{word}"), Span::call_site(), false);
        }
        let trees: Vec<TokenTree> = input.into_iter().collect();
        assembler.walk(&trees)?;
        assembler.push_eof();
        Ok(MacroSource {
            text: assembler.text,
            tiles: assembler.tiles,
            spans: assembler.spans,
            splices: assembler.splices,
            dialect: Dialect::Clingo,
        })
    }

    /// The `proc_macro` span the tile covering `range`'s start came from —
    /// the span map a diagnostic located in the assembled text
    /// (docs/design/macros.md §5.3) reads to blame the offending Rust token
    /// (§6). Total, as `token_at` is: the tiles cover the assembled text
    /// gap-free from offset zero, so this resolves the same way — a range
    /// beginning past the text lands on the trailing `EOF` tile's span
    /// rather than on nothing.
    pub fn span_of(&self, range: TextRange) -> Span {
        let start = u32::from(range.start());
        let index = self
            .tiles
            .partition_point(|tile| tile.start <= start)
            .saturating_sub(1);
        self.spans[index]
    }

    /// The Rust expression the splice at `range` captured (grammar §9),
    /// for the codegen a later increment writes. `None` when no `SPLICE`
    /// tile has exactly this range.
    pub fn splice_at(&self, range: TextRange) -> Option<&TokenStream> {
        let start = u32::from(range.start());
        let end = u32::from(range.end());
        self.splices
            .iter()
            .find(|splice| splice.range.start == start && splice.range.end == end)
            .map(|splice| &splice.expr)
    }
}

impl TokenSource for MacroSource {
    fn id(&self) -> SourceId {
        STRING_INPUT_SOURCE_ID
    }

    fn dialect(&self) -> Dialect {
        self.dialect
    }

    fn text(&self) -> &str {
        &self.text
    }

    /// The tile covering `at`. The mode goes unread: the source is the
    /// authoritative tiler and answers from its own boundaries
    /// (docs/design/macros.md §6), so a token's kind does not depend on
    /// the mode it is asked under. Refuses exactly where base refuses —
    /// past the end (`OutOfBounds`) or inside a character
    /// (`NotCharBoundary`) — and never panics.
    fn token_at(&self, at: ByteOffset, _mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let offset = at.get();
        let end_of_text = length_of(&self.text);
        if offset > end_of_text {
            return Err(PositionRefusal::OutOfBounds(OffsetOutOfBounds {
                offset: at,
                max: ByteOffset::new(end_of_text),
            }));
        }
        let offset_usize = offset as usize;
        if !self.text.is_char_boundary(offset_usize) {
            return Err(PositionRefusal::NotCharBoundary(NotCharBoundary {
                offset: at,
            }));
        }
        // The tiles cover `[0, len]` gap-free with an `EOF` tile at `len`,
        // so the last tile whose start is at or before `offset` is the one
        // that covers it; at a tile's own start this is that whole tile.
        let index = self
            .tiles
            .partition_point(|tile| tile.start <= offset)
            .saturating_sub(1);
        let tile = self.tiles[index];
        let tile_end = (tile.start + tile.len) as usize;
        Ok(Token {
            kind: tile.kind,
            text: &self.text[offset_usize..tile_end],
        })
    }
}

/// The assembly state: the text under construction, its tiles, their
/// spans, the captured splices, and the previous tile (to decide
/// separators).
#[derive(Default)]
struct Assembler {
    text: String,
    tiles: Vec<Tile>,
    spans: Vec<Span>,
    splices: Vec<Splice>,
    previous: Option<Previous>,
}

/// The previous significant tile's text and whether it was a splice — the
/// two facts the separator decision needs.
struct Previous {
    text: String,
    is_splice: bool,
}

impl Assembler {
    /// Emits one tile, inserting a single `WHITESPACE` separator first
    /// when the previous tile would otherwise fuse with this one
    /// (syntax §10). A splice's marker (`$x`) is no themelios token the
    /// fusion oracle reads, so a splice on either side always takes a
    /// separator — safe, since whitespace is trivia everywhere.
    fn emit(&mut self, kind: SyntaxKind, text: &str, span: Span, is_splice: bool) {
        if let Some(previous) = &self.previous {
            let fuses = if previous.is_splice || is_splice {
                true
            } else {
                separator_between(&previous.text, text, CONTEXT) != Separator::Nothing
            };
            if fuses {
                let start = length_of(&self.text);
                self.text.push(' ');
                self.tiles.push(Tile {
                    start,
                    len: 1,
                    kind: SyntaxKind::WHITESPACE,
                });
                self.spans.push(span);
            }
        }
        let start = length_of(&self.text);
        self.text.push_str(text);
        self.tiles.push(Tile {
            start,
            len: length_of(text),
            kind,
        });
        self.spans.push(span);
        self.previous = Some(Previous {
            text: text.to_owned(),
            is_splice,
        });
    }

    /// Appends the final zero-length `EOF` tile at the text's end
    /// (syntax §4.3), directly — `EOF` neither fuses nor is a splice.
    fn push_eof(&mut self) {
        self.tiles.push(Tile {
            start: length_of(&self.text),
            len: 0,
            kind: SyntaxKind::EOF,
        });
        self.spans.push(Span::call_site());
    }

    /// Walks `trees` in order, mapping each onto the roster (grammar §9).
    fn walk(&mut self, trees: &[TokenTree]) -> Result<(), MapError> {
        let mut index = 0;
        while index < trees.len() {
            index = self.map_tree(trees, index)?;
        }
        Ok(())
    }

    /// Maps the tree at `index` (looking ahead as the dialect requires),
    /// emitting its tiles and returning the index of the next unconsumed
    /// tree.
    fn map_tree(&mut self, trees: &[TokenTree], index: usize) -> Result<usize, MapError> {
        match &trees[index] {
            TokenTree::Ident(ident) => {
                let (kind, text) = classify_ident(ident)?;
                self.emit(kind, &text, ident.span(), false);
                Ok(index + 1)
            }
            TokenTree::Literal(literal) => {
                let (kind, text) = classify_literal(literal)?;
                self.emit(kind, &text, literal.span(), false);
                Ok(index + 1)
            }
            TokenTree::Group(group) => {
                self.map_group(group)?;
                Ok(index + 1)
            }
            TokenTree::Punct(punct) if punct.as_char() == '#' => self.map_hash(trees, index, punct),
            TokenTree::Punct(punct) if punct.as_char() == '$' => {
                self.map_splice(trees, index, punct)
            }
            TokenTree::Punct(_) => self.map_operator_run(trees, index),
        }
    }

    /// A `Group`'s bracket tokens around its recursively-mapped stream;
    /// a `None`-delimited group is transparent (no bracket tiles).
    fn map_group(&mut self, group: &Group) -> Result<(), MapError> {
        let brackets = match group.delimiter() {
            Delimiter::Parenthesis => Some((SyntaxKind::L_PAREN, "(", SyntaxKind::R_PAREN, ")")),
            Delimiter::Bracket => Some((SyntaxKind::L_BRACKET, "[", SyntaxKind::R_BRACKET, "]")),
            Delimiter::Brace => Some((SyntaxKind::L_BRACE, "{", SyntaxKind::R_BRACE, "}")),
            Delimiter::None => None,
        };
        let inner: Vec<TokenTree> = group.stream().into_iter().collect();
        if let Some((open, open_text, close, close_text)) = brackets {
            self.emit(open, open_text, group.span_open(), false);
            self.walk(&inner)?;
            self.emit(close, close_text, group.span_close(), false);
        } else {
            self.walk(&inner)?;
        }
        Ok(())
    }

    /// A `#`-keyword: the `#` at `index` must be span-adjacent to a
    /// following identifier (grammar §9), whose word names the keyword;
    /// `#sum` span-adjacent to a `+` beyond it is `#sum+`. A detached `#`
    /// is a dialect error.
    fn map_hash(
        &mut self,
        trees: &[TokenTree],
        index: usize,
        hash: &Punct,
    ) -> Result<usize, MapError> {
        if let Some(TokenTree::Ident(word)) = trees.get(index + 1)
            && adjacent(hash.span(), word.span())
        {
            let spelling = word.to_string();
            if spelling == "sum"
                && let Some(TokenTree::Punct(plus)) = trees.get(index + 2)
                && plus.as_char() == '+'
                && adjacent(word.span(), plus.span())
            {
                self.emit(SyntaxKind::KW_SUM_PLUS, "#sum+", hash.span(), false);
                return Ok(index + 3);
            }
            let kind = keyword_kind(&spelling).ok_or_else(|| MapError {
                span: word.span(),
                message: format!("`#{spelling}` is not a keyword"),
            })?;
            self.emit(kind, &format!("#{spelling}"), hash.span(), false);
            return Ok(index + 2);
        }
        Err(MapError {
            span: hash.span(),
            message: "`#` must be joined to the keyword it opens".to_owned(),
        })
    }

    /// A splice (grammar §9): the `$` at `index` takes the next tree — an
    /// identifier (`$x`) or a parenthesized group (`$( expr )`) — as its
    /// operand, emitting one `SPLICE` tile over marker and operand and
    /// capturing the operand's token stream.
    fn map_splice(
        &mut self,
        trees: &[TokenTree],
        index: usize,
        marker: &Punct,
    ) -> Result<usize, MapError> {
        match trees.get(index + 1) {
            Some(TokenTree::Ident(name)) => {
                let text = format!("${name}");
                let expr = TokenStream::from(TokenTree::Ident(name.clone()));
                self.emit_splice(&text, marker.span(), expr);
                Ok(index + 2)
            }
            Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis => {
                // The tile's text stands in for the operand in the
                // assembled text; the token source answers `SPLICE` from
                // the tile, so the text is never re-lexed (§6).
                let text = format!("$({})", group.stream());
                self.emit_splice(&text, marker.span(), group.stream());
                Ok(index + 2)
            }
            _ => Err(MapError {
                span: marker.span(),
                message: "`$` must be followed by a name or a parenthesized expression".to_owned(),
            }),
        }
    }

    /// Emits a `SPLICE` tile of `text` and records its captured `expr`.
    fn emit_splice(&mut self, text: &str, span: Span, expr: TokenStream) {
        let start = self.next_tile_start(text, true);
        self.emit(SyntaxKind::SPLICE, text, span, true);
        self.splices.push(Splice {
            range: start..start + length_of(text),
            expr,
        });
    }

    /// The byte offset the next `emit` of `text` will place its tile at —
    /// after any separator it inserts — so a splice's captured range
    /// matches the tile `emit` records.
    fn next_tile_start(&self, text: &str, is_splice: bool) -> u32 {
        let separator = self.previous.as_ref().is_some_and(|previous| {
            previous.is_splice || is_splice || {
                separator_between(&previous.text, text, CONTEXT) != Separator::Nothing
            }
        });
        length_of(&self.text) + u32::from(separator)
    }

    /// A maximal run of punctuation glued by `Spacing::Joint` (grammar §9,
    /// syntax §4.2), munched into operator tiles by the roster's
    /// longest-match, exactly as the file lexer reads adjacent bytes.
    fn map_operator_run(&mut self, trees: &[TokenTree], index: usize) -> Result<usize, MapError> {
        let mut run: Vec<(char, Span)> = Vec::new();
        let mut cursor = index;
        while let Some(TokenTree::Punct(punct)) = trees.get(cursor) {
            run.push((punct.as_char(), punct.span()));
            let glued = punct.spacing() == Spacing::Joint
                && matches!(trees.get(cursor + 1), Some(TokenTree::Punct(_)));
            if glued {
                cursor += 1;
            } else {
                break;
            }
        }
        self.munch_operators(&run)?;
        Ok(cursor + 1)
    }

    /// Munches a punctuation run into operator tiles, longest match first
    /// (grammar §4.6): a two-character operator where its characters lead,
    /// else the one-character operator, else a dialect error at the
    /// offending character.
    fn munch_operators(&mut self, run: &[(char, Span)]) -> Result<(), MapError> {
        let mut position = 0;
        while position < run.len() {
            let pair = run.get(position + 1).map(|next| (run[position].0, next.0));
            let (kind, width) = match pair {
                Some(('.', '.')) => (SyntaxKind::DOTDOT, 2),
                Some(('*', '*')) => (SyntaxKind::STAR_STAR, 2),
                Some((':', '-')) => (SyntaxKind::NECK, 2),
                Some((':', '~')) => (SyntaxKind::WEAK_NECK, 2),
                Some(('=', '=')) => (SyntaxKind::EQ, 2),
                Some(('!', '=') | ('<', '>')) => (SyntaxKind::NEQ, 2),
                Some(('<', '=')) => (SyntaxKind::LE, 2),
                Some(('>', '=')) => (SyntaxKind::GE, 2),
                _ => (single_operator(run[position])?, 1),
            };
            let text: String = run[position..position + width]
                .iter()
                .map(|(c, _)| c)
                .collect();
            self.emit(kind, &text, run[position].1, false);
            position += width;
        }
        Ok(())
    }
}

/// The one-character operator a punctuation character names (grammar §4.6),
/// or a dialect error for a character the roster has only in a
/// multi-character operator (`!`) or not at all.
fn single_operator((character, span): (char, Span)) -> Result<SyntaxKind, MapError> {
    Ok(match character {
        '.' => SyntaxKind::DOT,
        ',' => SyntaxKind::COMMA,
        ';' => SyntaxKind::SEMICOLON,
        ':' => SyntaxKind::COLON,
        '|' => SyntaxKind::PIPE,
        '+' => SyntaxKind::PLUS,
        '-' => SyntaxKind::MINUS,
        '*' => SyntaxKind::STAR,
        '/' => SyntaxKind::SLASH,
        '\\' => SyntaxKind::BACKSLASH,
        '^' => SyntaxKind::CARET,
        '&' => SyntaxKind::AMPERSAND,
        '~' => SyntaxKind::TILDE,
        '?' => SyntaxKind::QUESTION,
        '@' => SyntaxKind::AT,
        '=' => SyntaxKind::EQ,
        '<' => SyntaxKind::LT,
        '>' => SyntaxKind::GT,
        other => {
            return Err(MapError {
                span,
                message: format!("`{other}` is not an operator in the macro dialect"),
            });
        }
    })
}

/// The name class of a Rust identifier (grammar §9): `not` the keyword,
/// `_` alone the anonymous variable, an ASCII lowercase-initial name an
/// `IDENTIFIER`, an ASCII uppercase-initial name a `VARIABLE`; a raw
/// identifier (`r#not`, `r#Foo`) and every other identifier (`__`, `_1`, a
/// leading-underscore or non-ASCII word) match no class and are a dialect
/// error — `r#not` names no reserved word, it is only an error
/// (docs/design/macros.md §6).
fn classify_ident(ident: &Ident) -> Result<(SyntaxKind, String), MapError> {
    let spelling = ident.to_string();
    // A raw identifier renders with its `r#` prefix (proc-macro2's
    // round-tripping `Display`), so `r#not`/`r#Foo` would otherwise slip
    // through as a name carrying `#`. Grammar §9 makes a raw identifier an
    // explicit dialect error — never a way to spell a reserved word — so it
    // is refused here, before any class match (docs/design/macros.md §6).
    if spelling.starts_with("r#") {
        return Err(MapError {
            span: ident.span(),
            message: "a raw identifier is not part of the macro dialect".to_owned(),
        });
    }
    let kind = if spelling == "not" {
        SyntaxKind::KW_NOT
    } else if spelling == "_" {
        SyntaxKind::ANONYMOUS
    } else {
        match spelling.as_bytes().first() {
            Some(first) if spelling.is_ascii() && first.is_ascii_lowercase() => SyntaxKind::IDENT,
            Some(first) if spelling.is_ascii() && first.is_ascii_uppercase() => {
                SyntaxKind::VARIABLE
            }
            _ => {
                return Err(MapError {
                    span: ident.span(),
                    message: format!("`{spelling}` matches no name class of the macro dialect"),
                });
            }
        }
    };
    Ok((kind, spelling))
}

/// The roster kind of a Rust literal (grammar §9): an integer literal a
/// `NUMBER` by value, a string literal a `STRING` by value whenever grammar
/// §4.4 can spell that value — for a raw string (`r"raw"` → `raw`) as much
/// as a plain one. A float, char, byte, or byte-string literal and a
/// suffixed numeral are dialect errors. The one string corner §4.4 cannot
/// spell is what a later increment carries as a splice of the value
/// (docs/design/macros.md §6); until then an escape-needing or raw string
/// is refused rather than mapped.
fn classify_literal(literal: &Literal) -> Result<(SyntaxKind, String), MapError> {
    let spelling = literal.to_string();
    let refuse = |message: String| MapError {
        span: literal.span(),
        message,
    };
    if spelling.starts_with('\'') {
        return Err(refuse(
            "a character literal is not part of the macro dialect".to_owned(),
        ));
    }
    if spelling.starts_with('b') {
        return Err(refuse(
            "a byte or byte-string literal is not part of the macro dialect".to_owned(),
        ));
    }
    if spelling.starts_with('"') {
        return simple_string(&spelling)
            .map(|text| (SyntaxKind::STRING, text))
            .ok_or_else(|| {
                refuse("this string literal's value is not yet mapped by value".to_owned())
            });
    }
    if spelling.starts_with('r') {
        // A raw string's value is usually §4.4-spellable (`r"raw"` → `raw`);
        // when the by-value mapping is closed such a value becomes a `STRING`
        // tile directly, and only a value §4.4 cannot spell goes through the
        // splice of the value (docs/design/macros.md §6). Until then a raw
        // string is refused, never mapped by a guess.
        return Err(refuse(
            "a raw string literal's value is not yet mapped by value".to_owned(),
        ));
    }
    integer_value(&spelling)
        .map(|value| (SyntaxKind::NUMBER, value))
        .ok_or_else(|| {
            refuse(
                "only an unsuffixed integer literal is mapped here; this is a float, a suffixed \
                 numeral, or out of range"
                    .to_owned(),
            )
        })
}

/// The themelios spelling of a non-raw Rust string whose value grammar
/// §4.4 can spell verbatim — no escape, no character outside printable
/// ASCII. `None` for a string whose value needs unescaping or a wider
/// spelling; a later increment maps such a value directly to a `STRING`
/// when §4.4 can spell it and carries it as a splice of the value only when
/// §4.4 cannot (docs/design/macros.md §6).
fn simple_string(spelling: &str) -> Option<String> {
    let inner = spelling.strip_prefix('"')?.strip_suffix('"')?;
    let spellable = inner
        .chars()
        .all(|character| (character.is_ascii_graphic() || character == ' ') && character != '\\');
    spellable.then(|| spelling.to_owned())
}

/// The decimal spelling of a Rust integer literal's value (grammar §9's
/// "by value"): underscores stripped, the radix prefix honored. `None`
/// for a float, a suffixed numeral, or a value past `u128`.
fn integer_value(spelling: &str) -> Option<String> {
    let clean: String = spelling
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (radix, digits) = if let Some(rest) = clean.strip_prefix("0x").or(clean.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = clean.strip_prefix("0o").or(clean.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = clean.strip_prefix("0b").or(clean.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, clean.as_str())
    };
    if radix == 10 && clean.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        return None;
    }
    u128::from_str_radix(digits, radix)
        .ok()
        .map(|value| value.to_string())
}

/// The keyword kind a `#`-word names (grammar §4.5), the leading `#`
/// stripped; `None` for a word that is no keyword. Mirrors the file
/// lexer's table, `#end` excepted — it is the script terminator alone, not
/// a keyword a `#`-word forms (grammar §4.8).
fn keyword_kind(word: &str) -> Option<SyntaxKind> {
    Some(match word {
        "const" => SyntaxKind::KW_CONST,
        "count" => SyntaxKind::KW_COUNT,
        "defined" => SyntaxKind::KW_DEFINED,
        "edge" => SyntaxKind::KW_EDGE,
        "external" => SyntaxKind::KW_EXTERNAL,
        "false" => SyntaxKind::KW_FALSE,
        "heuristic" => SyntaxKind::KW_HEURISTIC,
        "include" => SyntaxKind::KW_INCLUDE,
        "inf" | "infimum" => SyntaxKind::KW_INF,
        "max" => SyntaxKind::KW_MAX,
        "maximize" | "maximise" => SyntaxKind::KW_MAXIMIZE,
        "min" => SyntaxKind::KW_MIN,
        "minimize" | "minimise" => SyntaxKind::KW_MINIMIZE,
        "program" => SyntaxKind::KW_PROGRAM,
        "project" => SyntaxKind::KW_PROJECT,
        "script" => SyntaxKind::KW_SCRIPT,
        "show" => SyntaxKind::KW_SHOW,
        "sum" => SyntaxKind::KW_SUM,
        "sup" | "supremum" => SyntaxKind::KW_SUP,
        "theory" => SyntaxKind::KW_THEORY,
        "true" => SyntaxKind::KW_TRUE,
        _ => return None,
    })
}

/// Whether `left` ends exactly where `right` begins — the span adjacency
/// grammar §9 reads a `#`-keyword by. Byte ranges come from
/// proc-macro2's `span-locations`; they are exact when a source text
/// backs the tokens, as under a compile and this crate's tests.
fn adjacent(left: Span, right: Span) -> bool {
    left.byte_range().end == right.byte_range().start
}

/// The byte length of a text as a `u32`. Total for a macro body, whose
/// assembled text stays far below the `u32` ceiling.
fn length_of(text: &str) -> u32 {
    u32::try_from(text.len()).expect("a macro body's assembled text stays below 4 GiB")
}

// The roster's screaming-snake kinds read as the tokens they are; the
// glob is what lets these tables name them without a hundred qualifiers.
#[cfg(test)]
#[allow(clippy::enum_glob_use)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;
    use themelios_syntax::token::{LexMode, TokenSource, check_token_source_laws};
    use themelios_syntax::tree::SyntaxKind::{self, *};

    use super::*;

    /// The `(kind, text)` of every tile from offset zero, walking under
    /// `Normal` mode to the `EOF`, which is included.
    fn tiles(source: &MacroSource) -> Vec<(SyntaxKind, String)> {
        let mut at = 0u32;
        let mut out = Vec::new();
        loop {
            let token = source
                .token_at(ByteOffset::new(at), LexMode::Normal)
                .expect("a position");
            out.push((token.kind, token.text.to_owned()));
            if token.kind == EOF {
                return out;
            }
            at += length_of(token.text);
        }
    }

    fn build(source: &str) -> MacroSource {
        MacroSource::build(TokenStream::from_str(source).expect("lexes"), None)
            .expect("maps under the dialect")
    }

    fn kinds(source: &MacroSource) -> Vec<SyntaxKind> {
        tiles(source).into_iter().map(|(kind, _)| kind).collect()
    }

    #[test]
    fn maps_a_ground_fact_to_normal_tokens() {
        let source = build("p(1, a)");
        assert_eq!(
            tiles(&source),
            [
                (IDENT, "p"),
                (L_PAREN, "("),
                (NUMBER, "1"),
                (COMMA, ","),
                (IDENT, "a"),
                (R_PAREN, ")"),
                (EOF, ""),
            ]
            .map(|(kind, text)| (kind, text.to_owned()))
        );
        assert_eq!(source.text(), "p(1,a)");
    }

    #[test]
    fn the_name_classes_map_by_initial() {
        assert_eq!(kinds(&build("X")), [VARIABLE, EOF]);
        assert_eq!(kinds(&build("_")), [ANONYMOUS, EOF]);
        assert_eq!(kinds(&build("not")), [KW_NOT, EOF]);
        assert_eq!(kinds(&build("p")), [IDENT, EOF]);
    }

    #[test]
    fn an_identifier_no_class_matches_is_a_dialect_error() {
        for word in ["__", "_1", "_p"] {
            assert!(
                MacroSource::build(TokenStream::from_str(word).unwrap(), None).is_err(),
                "{word} maps to no name class"
            );
        }
    }

    #[test]
    fn a_raw_identifier_is_a_dialect_error() {
        // A raw identifier renders with its `r#` prefix, so admitting it
        // would forge a name carrying `#`; grammar §9 makes it an explicit
        // dialect error, never a way to spell a reserved word. A raw keyword
        // (`r#type`) is refused on the same ground.
        for word in ["not", "Foo", "type"] {
            let raw = TokenStream::from(TokenTree::Ident(Ident::new_raw(word, Span::call_site())));
            assert!(
                MacroSource::build(raw, None).is_err(),
                "r#{word} maps to no name class"
            );
        }
    }

    #[test]
    fn strong_negation_abuts_its_atom() {
        let source = build("-p");
        assert_eq!(kinds(&source), [MINUS, IDENT, EOF]);
        assert_eq!(source.text(), "-p");
    }

    #[test]
    fn a_joint_punctuation_run_munches_into_one_operator() {
        assert_eq!(kinds(&build("!=")), [NEQ, EOF]);
        assert_eq!(build("!=").text(), "!=");
        assert_eq!(kinds(&build("X <= Y")), [VARIABLE, LE, VARIABLE, EOF]);
        assert_eq!(kinds(&build(":-")), [NECK, EOF]);
    }

    #[test]
    fn a_keyword_forms_from_a_span_adjacent_hash() {
        let source = build("#show");
        assert_eq!(kinds(&source), [KW_SHOW, EOF]);
        assert_eq!(source.text(), "#show");
        assert_eq!(kinds(&build("#sum+")), [KW_SUM_PLUS, EOF]);
        // `#sum` and `+` would fuse back into `#sum+`, so the oracle keeps
        // them apart with a separator.
        assert_eq!(kinds(&build("#sum +")), [KW_SUM, WHITESPACE, PLUS, EOF]);
        assert_eq!(build("#sum +").text(), "#sum +");
    }

    #[test]
    fn a_detached_hash_is_a_dialect_error() {
        assert!(MacroSource::build(TokenStream::from_str("# show").unwrap(), None).is_err());
        assert!(MacroSource::build(TokenStream::from_str("#").unwrap(), None).is_err());
    }

    #[test]
    fn a_theory_atom_maps_through_its_brace_group() {
        let source = build("&sum{1}");
        assert_eq!(
            kinds(&source),
            [AMPERSAND, IDENT, L_BRACE, NUMBER, R_BRACE, EOF]
        );
        assert_eq!(source.text(), "&sum{1}");
    }

    #[test]
    fn a_float_literal_is_a_dialect_error() {
        assert!(MacroSource::build(TokenStream::from_str("1.5").unwrap(), None).is_err());
    }

    #[test]
    fn integers_map_by_value() {
        assert_eq!(build("0o17").text(), "15");
        assert_eq!(build("1_000").text(), "1000");
        assert_eq!(build("0x1F").text(), "31");
        assert_eq!(kinds(&build("42")), [NUMBER, EOF]);
    }

    #[test]
    fn a_suffixed_or_char_or_byte_literal_is_a_dialect_error() {
        for literal in ["1i32", "'a'", "b\"x\"", "1.0f64"] {
            assert!(
                MacroSource::build(TokenStream::from_str(literal).unwrap(), None).is_err(),
                "{literal} is not named by the dialect"
            );
        }
    }

    #[test]
    fn a_simple_string_maps_by_value() {
        let source = build(r#"p("hi")"#);
        assert_eq!(kinds(&source), [IDENT, L_PAREN, STRING, R_PAREN, EOF]);
        assert_eq!(source.text(), r#"p("hi")"#);
    }

    #[test]
    fn a_string_needing_unescaping_is_refused_for_now() {
        assert!(MacroSource::build(TokenStream::from_str(r#""a\tb""#).unwrap(), None).is_err());
    }

    #[test]
    fn a_splice_names_a_single_token_over_marker_and_operand() {
        let source = build("$x");
        assert_eq!(kinds(&source), [SPLICE, EOF]);
        assert_eq!(source.text(), "$x");
        let range = TextRange::new(0.into(), length_of("$x").into());
        assert_eq!(
            source.splice_at(range).map(ToString::to_string),
            Some("x".to_owned())
        );
    }

    #[test]
    fn a_parenthesized_splice_captures_its_expression() {
        let source = build("$(a + b)");
        assert_eq!(kinds(&source), [SPLICE, EOF]);
        let range = TextRange::new(0.into(), length_of(source.text()).into());
        assert_eq!(
            source.splice_at(range).map(ToString::to_string),
            Some("a + b".to_owned())
        );
    }

    #[test]
    fn a_splice_never_abuts_its_neighbours() {
        // A splice's marker is no themelios token, so it always takes a
        // separator — the assembled text keeps its tiles apart.
        let source = build("p($x)");
        assert_eq!(
            kinds(&source),
            [IDENT, L_PAREN, WHITESPACE, SPLICE, WHITESPACE, R_PAREN, EOF]
        );
        assert_eq!(source.text(), "p( $x )");
    }

    #[test]
    fn a_bare_dollar_is_a_dialect_error() {
        assert!(MacroSource::build(TokenStream::from_str("$ + 1").unwrap(), None).is_err());
    }

    #[test]
    fn an_entry_keyword_opens_the_text() {
        let source =
            MacroSource::build(TokenStream::from_str("p/1").unwrap(), Some("show")).expect("maps");
        // `#show` and `p` would fuse into one `#`-word, so the directive
        // keyword takes a separator before the payload.
        assert_eq!(
            kinds(&source),
            [KW_SHOW, WHITESPACE, IDENT, SLASH, NUMBER, EOF]
        );
        assert_eq!(source.text(), "#show p/1");
    }

    #[test]
    fn fusing_tokens_take_a_separator() {
        assert_eq!(build("p q").text(), "p q");
        assert_eq!(build("not p").text(), "not p");
        assert_eq!(kinds(&build("p q")), [IDENT, WHITESPACE, IDENT, EOF]);
    }

    #[test]
    fn the_source_is_clingo_under_the_string_input_id() {
        let source = build("p");
        assert_eq!(source.dialect(), Dialect::Clingo);
        assert_eq!(source.id(), STRING_INPUT_SOURCE_ID);
    }

    #[test]
    fn the_span_map_points_at_the_originating_rust_token() {
        // `p(1, a)` assembles to `p(1,a)` — the comma drops its space, so the
        // `a` tile sits at assembled offset 4 though its Rust token is at
        // input byte 5. The map must answer with that token's own span, not
        // the tile's assembled position, so a diagnostic blames the right
        // source (docs/design/macros.md §5.3). proc-macro2's fallback reports
        // a byte range 0-based over the parsed text, so these are exact.
        let source = build("p(1, a)");
        let p = TextRange::new(0.into(), 1.into());
        let a = TextRange::new(4.into(), 5.into());
        assert_eq!(source.span_of(p).byte_range(), 0..1);
        assert_eq!(source.span_of(a).byte_range(), 5..6);
    }

    #[test]
    fn token_at_refuses_off_the_text_as_base_refuses() {
        let source = build("p");
        let past = ByteOffset::new(length_of(source.text()) + 1);
        assert!(matches!(
            source.token_at(past, LexMode::Normal),
            Err(PositionRefusal::OutOfBounds(_))
        ));
        let end = source
            .token_at(ByteOffset::new(length_of(source.text())), LexMode::Normal)
            .expect("the end is a position");
        assert_eq!(end.kind, EOF);
    }

    #[test]
    fn the_assembled_source_obeys_the_token_source_laws() {
        for program in [
            "p(1, a)",
            "-p",
            "X != Y",
            "&sum{1}",
            "p($x)",
            "#show",
            "a :- b, not c",
        ] {
            let source = build(program);
            assert_eq!(
                check_token_source_laws(&source),
                Vec::new(),
                "{program} tiles lawfully"
            );
        }
    }

    #[test]
    fn a_character_outside_the_operator_roster_is_a_dialect_error() {
        // A lone `!` is no operator on its own (only `!=` is), so its run
        // munches to a dialect error.
        assert!(MacroSource::build(TokenStream::from_str("!").unwrap(), None).is_err());
    }

    #[test]
    fn every_two_character_operator_and_more_map() {
        assert_eq!(kinds(&build("..")), [DOTDOT, EOF]);
        assert_eq!(kinds(&build("**")), [STAR_STAR, EOF]);
        assert_eq!(kinds(&build(":~")), [WEAK_NECK, EOF]);
        assert_eq!(kinds(&build("==")), [EQ, EOF]);
        assert_eq!(kinds(&build("<>")), [NEQ, EOF]);
        assert_eq!(kinds(&build(">=")), [GE, EOF]);
        // A run that is no single operator splits by longest match.
        assert_eq!(kinds(&build("|;?@")), [PIPE, SEMICOLON, QUESTION, AT, EOF]);
    }

    #[test]
    fn an_unknown_entry_keyword_is_refused() {
        assert!(MacroSource::build(TokenStream::new(), Some("nope")).is_err());
    }

    #[test]
    fn a_splice_operand_must_be_a_name_or_a_parenthesized_expression() {
        // `$[x]` — a bracketed group is neither a name nor a parenthesized
        // expression, so the marker has no operand.
        assert!(MacroSource::build(TokenStream::from_str("$[x]").unwrap(), None).is_err());
    }

    #[test]
    fn the_accessors_answer_none_off_their_targets() {
        let source = build("p");
        let past = TextRange::new(length_of("p").into(), length_of("p").into());
        // No splice was captured, so `splice_at` finds none.
        assert!(source.splice_at(past).is_none());
        // A none-delimited group is transparent: its stream tiles with no
        // bracket tokens of its own.
        let mut none_group = TokenStream::new();
        none_group.extend(std::iter::once(TokenTree::Group(proc_macro2::Group::new(
            Delimiter::None,
            TokenStream::from_str("q").unwrap(),
        ))));
        let source = MacroSource::build(none_group, None).expect("maps");
        assert_eq!(kinds(&source), [IDENT, EOF]);
    }
}
