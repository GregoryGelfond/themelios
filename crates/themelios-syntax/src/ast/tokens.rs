//! Typed tokens over the valued kinds — rowan's `AstToken` idiom
//! (docs/design/syntax.md §8.3) — and the values they carry: numeral
//! radix and digits, string values under the dialect, doc and comment
//! content, the script body's raw text and value.

use std::fmt;

use themelios_base::span::ByteOffset;

use crate::dialect::Dialect;
use crate::tree::{SyntaxKind, SyntaxToken, TokenRole, offset_of, role};

/// A typed token: a view over one token, cast by kind — and, for the
/// two comment wrappers, by the token's role.
pub trait AstToken: Sized {
    /// Whether tokens of `kind` may cast (the role, where it matters, is
    /// read at `cast`).
    fn can_cast(kind: SyntaxKind) -> bool;
    /// The wrapper over `token`, when it is of the wrapper's kind and role.
    fn cast(token: SyntaxToken) -> Option<Self>;
    /// The token.
    fn syntax(&self) -> &SyntaxToken;
    /// The token's text.
    fn text(&self) -> &str {
        self.syntax().text()
    }
}

macro_rules! ast_token {
    ($(#[$meta:meta])* $name:ident, $($kind:ident)|+) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxToken);

        impl AstToken for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, $(SyntaxKind::$kind)|+)
            }

            fn cast(token: SyntaxToken) -> Option<Self> {
                if Self::can_cast(token.kind()) { Some(Self(token)) } else { None }
            }

            fn syntax(&self) -> &SyntaxToken {
                &self.0
            }
        }
    };
}

ast_token! {
    /// An identifier.
    Ident, IDENT
}

ast_token! {
    /// A variable, or the anonymous variable.
    Variable, VARIABLE | ANONYMOUS
}

impl Variable {
    /// Whether this is the anonymous variable `_`.
    pub fn is_anonymous(&self) -> bool {
        self.0.kind() == SyntaxKind::ANONYMOUS
    }
}

ast_token! {
    /// A numeral (grammar §4.3).
    NumberLit, NUMBER
}

/// A numeral's radix, from its prefix.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Radix {
    /// No prefix.
    Decimal,
    /// `0x`
    Hexadecimal,
    /// `0o`
    Octal,
    /// `0b`
    Binary,
}

impl NumberLit {
    /// The radix, from the prefix; total, syntactic.
    pub fn radix(&self) -> Radix {
        match self.0.text().get(..2) {
            Some("0x") => Radix::Hexadecimal,
            Some("0o") => Radix::Octal,
            Some("0b") => Radix::Binary,
            _ => Radix::Decimal,
        }
    }

    /// The text after the prefix; total.
    pub fn digits(&self) -> &str {
        match self.radix() {
            Radix::Decimal => self.0.text(),
            _ => &self.0.text()[2..],
        }
    }
}

ast_token! {
    /// A string literal (grammar §4.4, §6.2).
    StringLit, STRING
}

/// A string token whose spelling is not the dialect's rule, which only a
/// token source other than the file lexer can supply; `at` is where the
/// spelling breaks, in the source's coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidStringLiteral {
    /// Where the spelling breaks.
    pub at: ByteOffset,
}

impl fmt::Display for InvalidStringLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the string literal's spelling breaks the dialect's rule at byte {}",
            self.at.get()
        )
    }
}

impl std::error::Error for InvalidStringLiteral {}

impl StringLit {
    /// The denoted text with the dialect's escapes resolved (grammar
    /// §4.4, §6.2). The dialect is the caller's to state, because the
    /// tree does not carry it — and the caller must state it right:
    /// `"a\nb"` denotes differently under the two rules and a wrong
    /// dialect here yields a plausible wrong `String`, not a refusal, so
    /// a consumer holding the `Parse` uses `Parse::string_value` and
    /// takes the dialect from it. Refuses with `InvalidStringLiteral`
    /// only a token whose spelling is not the dialect's string rule,
    /// which a token source other than the file lexer can supply; the
    /// file lexer's tokens never refuse. O(token).
    pub fn value(&self, dialect: Dialect) -> Result<String, InvalidStringLiteral> {
        let text = self.0.text();
        let start = offset_of(self.0.text_range().start());
        let refuse = |index: usize| InvalidStringLiteral {
            at: ByteOffset::new(start.get() + u32::try_from(index).unwrap_or(u32::MAX)),
        };
        if !text.starts_with('"') || text.len() < 2 || !text.ends_with('"') {
            return Err(refuse(text.len()));
        }
        let inner = &text[1..text.len() - 1];
        let mut value = String::with_capacity(inner.len());
        let mut chars = inner.char_indices().peekable();
        while let Some((index, c)) = chars.next() {
            if c != '\\' {
                if dialect == Dialect::Clingo && (c == '"' || c == '\n') {
                    return Err(refuse(index + 1));
                }
                value.push(c);
                continue;
            }
            match dialect {
                Dialect::Clingo => match chars.next() {
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, 'n')) => value.push('\n'),
                    _ => return Err(refuse(index + 1)),
                },
                Dialect::AspCore2 => {
                    // `\"` is the one escape when a quote follows inside the
                    // literal; the backslash before the closing quote of a
                    // `"…\"`-final literal is itself (grammar §6.2).
                    if chars.peek().is_some_and(|(_, next)| *next == '"') {
                        chars.next();
                        value.push('"');
                    } else {
                        value.push('\\');
                    }
                }
            }
        }
        Ok(value)
    }
}

/// A `DOC_COMMENT` in docs position: a statement's documentation — the
/// cast reads `role`, not the kind alone (docs/design/syntax.md §5.4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DocLine(SyntaxToken);

impl AstToken for DocLine {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DOC_COMMENT
    }

    fn cast(token: SyntaxToken) -> Option<Self> {
        if Self::can_cast(token.kind()) && role(&token) == TokenRole::Documentation {
            Some(DocLine(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl DocLine {
    /// The text after the `%!` marker, untrimmed — comment text whose
    /// meaning is a tool's (grammar §8), trailing whitespace included: a
    /// documentation tool may read it (two trailing spaces are a hard
    /// break in more than one markup), so it is content here and in the
    /// certificates, never layout.
    pub fn content(&self) -> &str {
        &self.0.text()[2..]
    }
}

/// The comment forms.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CommentForm {
    /// `% …`
    Line,
    /// `%* … *%`
    Block,
    /// `#! …`
    Shebang,
    /// `%! …` outside docs position.
    Doc,
}

/// A trivia comment: `LINE_COMMENT`, `BLOCK_COMMENT`, or `SHEBANG_COMMENT`
/// anywhere, or a `DOC_COMMENT` whose role is `Trivia` — the cast reads
/// `role`, not the kind alone (docs/design/syntax.md §5.4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Comment(SyntaxToken);

impl AstToken for Comment {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind.is_comment()
    }

    fn cast(token: SyntaxToken) -> Option<Self> {
        if Self::can_cast(token.kind()) && role(&token) == TokenRole::Trivia {
            Some(Comment(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

/// A line comment or shebang's content: its text without the trailing
/// horizontal whitespace the line rule swallowed on its way to the line
/// end, which is layout (docs/design/syntax.md §8.3). The single home of
/// this rule, shared by `Comment::content` and the certificate's own
/// content (§11.1).
pub(crate) fn line_or_shebang_content(text: &str) -> &str {
    text.trim_end_matches([' ', '\t', '\r'])
}

/// A script body's value: its text with the blanks and tabs before
/// `#end` trimmed (grammar §4.8). The single home of this rule, shared by
/// `ScriptBody::value` and the certificate's content (§11.1).
pub(crate) fn script_body_value(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

impl Comment {
    /// The comment's content: for the line comment and the shebang, the
    /// text minus its trailing horizontal whitespace, since that
    /// whitespace is layout the rule swallowed on its way to the line
    /// end; for a doc comment in trivia position, the whole token text —
    /// the doc form's trailing whitespace is content wherever the token
    /// stands; for a block comment, the whole token text. This is what
    /// the certificates compare.
    pub fn content(&self) -> &str {
        match self.form() {
            CommentForm::Line | CommentForm::Shebang => line_or_shebang_content(self.0.text()),
            CommentForm::Block | CommentForm::Doc => self.0.text(),
        }
    }

    /// The form.
    pub fn form(&self) -> CommentForm {
        match self.0.kind() {
            SyntaxKind::LINE_COMMENT => CommentForm::Line,
            SyntaxKind::BLOCK_COMMENT => CommentForm::Block,
            SyntaxKind::SHEBANG_COMMENT => CommentForm::Shebang,
            _ => CommentForm::Doc,
        }
    }
}

ast_token! {
    /// The `SCRIPT_BODY` token (grammar §4.8).
    ScriptBody, SCRIPT_BODY
}

impl ScriptBody {
    /// The region's value per grammar §4.8: the raw text with trailing
    /// blanks and tabs trimmed before `#end`.
    pub fn value(&self) -> &str {
        script_body_value(self.0.text())
    }
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;
    use crate::tree::SyntaxElement;

    fn comment_form(text: &str, needle: &str) -> CommentForm {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        let root = parse(&source, Dialect::Clingo).syntax();
        let token = root
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|t| t.text() == needle)
            .expect("the comment token");
        Comment::cast(token).expect("a trivia comment").form()
    }

    #[test]
    fn comment_form_names_each_of_the_four_kinds() {
        // Each comment kind maps to its form; a deleted arm would fall through
        // to `Doc`, so every kind is pinned.
        assert!(matches!(comment_form("% a\np.", "% a"), CommentForm::Line));
        assert!(matches!(
            comment_form("%* a *%\np.", "%* a *%"),
            CommentForm::Block
        ));
        assert!(matches!(
            comment_form("#! a\np.", "#! a"),
            CommentForm::Shebang
        ));
        // A `%!` after a statement is a trivia doc comment.
        assert!(matches!(
            comment_form("p. %! stray\n", "%! stray"),
            CommentForm::Doc
        ));
    }
}
