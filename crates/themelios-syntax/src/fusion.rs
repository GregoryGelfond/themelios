//! The fusion oracle (docs/design/syntax.md §10): what must stand
//! between two tokens for each to lex as itself — not a theory to
//! maintain but a fact to compute, since this crate owns the lexer and
//! the exact answer is one relex away.

use crate::dialect::Dialect;
use crate::lexer::lex;
use crate::token::LexMode;
use crate::tree::SyntaxKind;

/// What must stand between two tokens for each to lex as itself.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Separator {
    /// The two may abut: `left` followed directly by `right` lexes as
    /// `left`, then `right`. (Not `None`: a Rust reader's `None` is
    /// `Option`'s, and this is an answer, not an absence.)
    Nothing,
    /// Whitespace of any kind is required — abutting would fuse or split
    /// the tokens (`a` `b` → `ab`; `#sum` `+` → `#sum+`; `0` `x1` →
    /// `0x1`; `<` `=` → `<=` under theory mode).
    Whitespace,
    /// A line break is required: `left` runs to the end of its line — a
    /// line comment, a doc comment, a shebang — and swallows anything
    /// after it on that line.
    LineBreak,
}

/// The lexical context an adjacency stands in: the dialect and the mode
/// in force at `left`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LexContext {
    /// The dialect the text was lexed under.
    pub dialect: Dialect,
    /// The mode in force at `left`'s start.
    pub mode: LexMode,
}

/// The oracle over texts: total, exact, O(|left| + |right|). `left` and
/// `right` are token texts of one lexed text; the answer is what the
/// lexer would do to them abutted under `context` — `Nothing` when
/// `left ++ right` lexes `left` first, `LineBreak` when `left` is a
/// line form, `Whitespace` otherwise (docs/design/syntax.md §10.1: a
/// space begins no token's continuation, and no token but the line
/// forms extends across it — for the token pairs a lexed text produces,
/// which is what §10.1's lemma scopes the oracle to).
pub fn separator_between(left: &str, right: &str, context: LexContext) -> Separator {
    let (left_kind, left_len) = lex(left, 0, context.mode, context.dialect);
    let line_form = matches!(
        left_kind,
        SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT | SyntaxKind::SHEBANG_COMMENT
    );
    if line_form && left_len == left.len() {
        return Separator::LineBreak;
    }
    let joined = format!("{left}{right}");
    // §10.1's "is the first token exactly `left`?" is a question of
    // extent: the relex's first token ends where `left` ends. The
    // relexed kind is not compared against `left` lexed in isolation — a
    // context-dependent token would be misjudged, since a SCRIPT_BODY
    // lexed alone is an ERROR for want of its following `#end` (grammar
    // §4.8) yet abuts that `#end` as itself.
    let (_, len) = lex(&joined, 0, context.mode, context.dialect);
    if len == left.len() {
        Separator::Nothing
    } else {
        Separator::Whitespace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(left: &str, right: &str) -> Separator {
        separator_between(
            left,
            right,
            LexContext {
                dialect: Dialect::Clingo,
                mode: LexMode::Normal,
            },
        )
    }

    fn theory(left: &str, right: &str) -> Separator {
        separator_between(
            left,
            right,
            LexContext {
                dialect: Dialect::Clingo,
                mode: LexMode::Theory,
            },
        )
    }

    #[test]
    fn the_grammars_named_cases_answer_as_the_grammar_says() {
        // The greedy theory-operator munch (grammar §4.7).
        assert_eq!(theory("<", "="), Separator::Whitespace);
        assert_eq!(theory("-", "-"), Separator::Whitespace);
        assert_eq!(theory(";", "-"), Separator::Whitespace);
        // The rule-neck abutment and the other normal-mode fusions.
        assert_eq!(normal(":", "-"), Separator::Whitespace);
        assert_eq!(normal("#sum", "+"), Separator::Whitespace);
        assert_eq!(normal("0", "x1"), Separator::Whitespace);
        assert_eq!(normal(".", "."), Separator::Whitespace);
        assert_eq!(normal("*", "*"), Separator::Whitespace);
        assert_eq!(normal("not", "p"), Separator::Whitespace);
        assert_eq!(normal("1", "2"), Separator::Whitespace);
        assert_eq!(normal("#inf", "x"), Separator::Whitespace);
        // A line comment before anything.
        assert_eq!(normal("% c", "p"), Separator::LineBreak);
        assert_eq!(normal("%! d", "p"), Separator::LineBreak);
        assert_eq!(normal("#! s", "p"), Separator::LineBreak);
    }

    #[test]
    fn pairs_that_lex_to_themselves_abutted_may_abut() {
        assert_eq!(normal("p", "("), Separator::Nothing);
        assert_eq!(normal(")", "."), Separator::Nothing);
        assert_eq!(normal(";", "-"), Separator::Nothing);
        assert_eq!(normal("-", "-"), Separator::Nothing);
        assert_eq!(normal("%* c *%", "p"), Separator::Nothing);
        assert_eq!(normal("\"a\"", "\"b\""), Separator::Nothing);
        assert_eq!(normal("X", "."), Separator::Nothing);
        assert_eq!(theory("x", "<="), Separator::Nothing);
        assert_eq!(theory("not", "-"), Separator::Nothing);
    }

    #[test]
    fn the_script_region_and_its_terminator() {
        let script = LexContext {
            dialect: Dialect::Clingo,
            mode: LexMode::ScriptBody,
        };
        assert_eq!(separator_between("#end", ".", script), Separator::Nothing);
        assert_eq!(
            separator_between("x = 1 ", "#end", script),
            Separator::Nothing
        );
    }

    #[test]
    fn the_asp_core_2_string_ending_in_an_escaped_looking_quote() {
        let core = LexContext {
            dialect: Dialect::AspCore2,
            mode: LexMode::Normal,
        };
        assert_eq!(
            separator_between("\"a\\\"", "\"", core),
            Separator::Whitespace
        );
        assert_eq!(separator_between("\"a\\\"", "p", core), Separator::Nothing);
    }

    #[test]
    fn end_of_input_on_the_right_separates_nothing() {
        assert_eq!(normal("p", ""), Separator::Nothing);
    }
}
