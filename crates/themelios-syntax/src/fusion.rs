//! The fusion oracle (docs/design/syntax.md §10): what must stand
//! between two tokens for each to lex as itself — not a theory to
//! maintain but a fact to compute, since this crate owns the lexer and
//! the exact answer is one relex away.

use crate::dialect::Dialect;
use crate::lexer::lex;
use crate::token::LexMode;
use crate::tree::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken};

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

/// The oracle over tree tokens: derives the mode from `left`'s position
/// (`lex_mode_of`) and answers for the texts. Total; O(|left| + |right|
/// + depth of `left`).
pub fn separator(left: &SyntaxToken, right: &SyntaxToken, dialect: Dialect) -> Separator {
    separator_between(
        left.text(),
        right.text(),
        LexContext {
            dialect,
            mode: lex_mode_of(left),
        },
    )
}

/// The mode in force at `token`'s start — the parser's standpoint,
/// reconstructed from the tree by docs/design/syntax.md §10.2's rule and
/// bound to the parser's own choice by law: `ScriptBody` for a script
/// body and for `#end` — and for the unterminated region's error token;
/// `Theory` inside a theory atom's elements and guard — outside their
/// conditions and outside what follows a condition through the `;` or `}`
/// that ends it — at a `#theory` definition's operator positions, and at
/// the first token after a theory atom (the guard-end peek); `Normal`
/// elsewhere, the `{` that opens the elements among them. Total;
/// O(depth of the token).
pub fn lex_mode_of(token: &SyntaxToken) -> LexMode {
    match token.kind() {
        SyntaxKind::SCRIPT_BODY | SyntaxKind::KW_END => return LexMode::ScriptBody,
        SyntaxKind::ERROR if in_script_body_position(token) => return LexMode::ScriptBody,
        SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT
            if token.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    SyntaxKind::OP_DEFINITION | SyntaxKind::ATOM_DEFINITION
                )
            }) =>
        {
            return LexMode::Theory;
        }
        SyntaxKind::L_BRACE
            if token
                .parent()
                .is_some_and(|parent| parent.kind() == SyntaxKind::THEORY_ELEMENTS) =>
        {
            return LexMode::Normal;
        }
        _ => {}
    }
    for ancestor in token.parent_ancestors() {
        match ancestor.kind() {
            SyntaxKind::CONDITION => return LexMode::Normal,
            SyntaxKind::THEORY_GUARD => return LexMode::Theory,
            SyntaxKind::THEORY_ELEMENTS => {
                return if follows_a_condition(token, &ancestor) {
                    LexMode::Normal
                } else {
                    LexMode::Theory
                };
            }
            _ => {}
        }
    }
    if after_a_theory_atom(token) {
        return LexMode::Theory;
    }
    LexMode::Normal
}

/// Whether `token` stands right after a theory atom whose elements the
/// parser has just read — the guard-end peek (docs/design/syntax.md §6.3,
/// §10.2), which it takes under theory mode to decide whether a guard
/// opens or extends before committing under normal. Structurally: the
/// previous significant token is inside a `THEORY_ATOM` this token is
/// outside of, one with a `THEORY_ELEMENTS` child — the `{ … }` whose
/// close makes the parser peek; a bare `&a` has no elements and no peek.
fn after_a_theory_atom(token: &SyntaxToken) -> bool {
    let mut prev = token.prev_token();
    while let Some(before) = prev {
        if before.kind().is_trivia() || before.kind() == SyntaxKind::DOC_COMMENT {
            prev = before.prev_token();
            continue;
        }
        return before.parent_ancestors().any(|ancestor| {
            ancestor.kind() == SyntaxKind::THEORY_ATOM
                && !ancestor.text_range().contains_range(token.text_range())
                && ancestor
                    .children()
                    .any(|child| child.kind() == SyntaxKind::THEORY_ELEMENTS)
        });
    }
    false
}

/// Whether the error token stands where a script body stands: a child of
/// a script statement, after its `)`.
fn in_script_body_position(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if parent.kind() != SyntaxKind::SCRIPT_STATEMENT {
        return false;
    }
    let mut cursor = token.prev_sibling_or_token();
    while let Some(element) = cursor {
        match element {
            NodeOrToken::Token(before) if before.kind().is_trivia() => {
                cursor = before.prev_sibling_or_token();
            }
            NodeOrToken::Token(before) => return before.kind() == SyntaxKind::R_PAREN,
            NodeOrToken::Node(_) => return false,
        }
    }
    false
}

/// Whether `token`, under `elements` (a `THEORY_ELEMENTS` node), stands
/// after an element that ended in a condition with no `;` between — the
/// stretch the parser reads in normal mode: the condition's terminator,
/// and anything recovery placed before it.
fn follows_a_condition(token: &SyntaxToken, elements: &SyntaxNode) -> bool {
    // The child of `elements` that holds the token: the token itself, or
    // the ERROR node it stands in.
    let child = token
        .parent_ancestors()
        .take_while(|ancestor| ancestor != elements)
        .last()
        .map_or(NodeOrToken::Token(token.clone()), NodeOrToken::Node);
    if let NodeOrToken::Node(node) = &child
        && node.kind() == SyntaxKind::THEORY_ELEMENT
    {
        return false;
    }
    let mut cursor = child.prev_sibling_or_token();
    while let Some(element) = cursor {
        match &element {
            NodeOrToken::Token(before) if before.kind() == SyntaxKind::SEMICOLON => return false,
            NodeOrToken::Node(node) if node.kind() == SyntaxKind::THEORY_ELEMENT => {
                return node
                    .children_with_tokens()
                    .filter(|e| !matches!(e, NodeOrToken::Token(t) if t.kind().is_trivia()))
                    .last()
                    .is_some_and(|last| {
                        matches!(last, NodeOrToken::Node(n) if n.kind() == SyntaxKind::CONDITION)
                    });
            }
            _ => {}
        }
        cursor = element.prev_sibling_or_token();
    }
    false
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

    use themelios_base::source::{Source, SourceId};

    use crate::parse::parse;
    use crate::tree::{SyntaxKind, SyntaxToken};

    fn token_vec(text: &str) -> Vec<SyntaxToken> {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
            .syntax()
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .collect()
    }

    fn mode_of(text: &str, token_text: &str, nth: usize) -> LexMode {
        let tokens = token_vec(text);
        let token = tokens
            .iter()
            .filter(|t| t.text() == token_text)
            .nth(nth)
            .expect("the token");
        lex_mode_of(token)
    }

    #[test]
    fn the_modes_are_reconstructed_from_the_tree() {
        let text = ":- &sum(1) { x, -y : p((a;b)) ; z } <= 3, not q. #script (lua) x #end. #theory t { x { - : 1, unary }; &a/0 : x, {<=}, x, any }.";
        assert_eq!(mode_of(text, "&", 0), LexMode::Normal);
        assert_eq!(mode_of(text, "sum", 0), LexMode::Normal);
        assert_eq!(
            mode_of(text, "1", 0),
            LexMode::Normal,
            "the arguments lex in normal mode"
        );
        assert_eq!(
            mode_of(text, "{", 0),
            LexMode::Normal,
            "the brace that opens the elements"
        );
        assert_eq!(mode_of(text, "x", 0), LexMode::Theory);
        assert_eq!(mode_of(text, "-", 0), LexMode::Theory);
        assert_eq!(
            mode_of(text, ":", 0),
            LexMode::Theory,
            "the condition-opening colon"
        );
        assert_eq!(
            mode_of(text, "p", 0),
            LexMode::Normal,
            "inside the condition"
        );
        assert_eq!(
            mode_of(text, ";", 0),
            LexMode::Normal,
            "the pool's `;` inside the condition"
        );
        assert_eq!(
            mode_of(text, ";", 1),
            LexMode::Normal,
            "the `;` that ends the condition"
        );
        assert_eq!(
            mode_of(text, "z", 0),
            LexMode::Theory,
            "theory mode resumes after that `;`"
        );
        assert_eq!(
            mode_of(text, "}", 0),
            LexMode::Theory,
            "the `}}` after an element without a condition"
        );
        assert_eq!(mode_of(text, "<=", 0), LexMode::Theory, "the guard");
        assert_eq!(mode_of(text, "3", 0), LexMode::Theory);
        assert_eq!(
            mode_of(text, ",", 1),
            LexMode::Theory,
            "the guard-end peek after a theory atom, before the normal-mode commit"
        );
        assert_eq!(mode_of(text, "not", 0), LexMode::Normal);
        assert_eq!(mode_of(text, " x ", 0), LexMode::ScriptBody);
        assert_eq!(mode_of(text, "#end", 0), LexMode::ScriptBody);
        assert_eq!(
            mode_of(text, "-", 1),
            LexMode::Theory,
            "the operator position of an op-definition"
        );
        assert_eq!(
            mode_of(text, "<=", 1),
            LexMode::Theory,
            "the operator position of an atom-definition"
        );
        assert_eq!(mode_of(text, "unary", 0), LexMode::Normal);
    }

    #[test]
    fn a_closing_brace_after_a_condition_is_normal_and_the_named_cases_answer() {
        assert_eq!(mode_of("&a { x : p }.", "}", 0), LexMode::Normal);
        let tokens = token_vec("&a { x : p ; -y }.");
        let semicolon = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::SEMICOLON)
            .expect(";");
        let minus = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::THEORY_OP)
            .expect("-");
        assert_eq!(
            separator(semicolon, minus, Dialect::Clingo),
            Separator::Nothing,
            "`;-` after a condition"
        );
        let tokens = token_vec("#script (lua) x #end.");
        let end = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::KW_END)
            .expect("#end");
        let dot = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::DOT)
            .expect(".");
        assert_eq!(
            separator(end, dot, Dialect::Clingo),
            Separator::Nothing,
            "`#end.`"
        );
    }

    #[test]
    fn the_token_form_reads_the_mode_from_the_left_token() {
        let tokens = token_vec("&a { x < = y }.");
        let lt = tokens.iter().find(|t| t.text() == "<").expect("<");
        let eq = tokens.iter().find(|t| t.text() == "=").expect("=");
        assert_eq!(separator(lt, eq, Dialect::Clingo), Separator::Whitespace);
        let tokens = token_vec("p :- X < Y, X = Y.");
        let lt = tokens.iter().find(|t| t.text() == "<").expect("<");
        let y = tokens.iter().find(|t| t.text() == "Y").expect("Y");
        assert_eq!(separator(lt, y, Dialect::Clingo), Separator::Nothing);
    }

    #[test]
    fn the_first_token_after_a_theory_atom_is_the_guard_end_peek() {
        // §10.2: the parser peeks the token after a theory atom under
        // theory mode (the greedy guard-end), so its standpoint is Theory
        // even though it commits under Normal — with a guard and without.
        assert_eq!(
            mode_of("&sum { x } >= 5.", ".", 0),
            LexMode::Theory,
            "the dot after a guard"
        );
        assert_eq!(
            mode_of("&a { x }.", ".", 0),
            LexMode::Theory,
            "the dot after a theory atom without a guard"
        );
        // The oracle must read that standpoint: `.` cannot abut a following
        // `&` — `.&` is one THEORY_OP under theory, two tokens under normal.
        let tokens = token_vec("&a { x }. &b { y }.");
        let dot = tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::DOT)
            .expect(".");
        let amp = tokens
            .iter()
            .filter(|t| t.kind() == SyntaxKind::AMPERSAND)
            .nth(1)
            .expect("&");
        assert_eq!(
            separator(dot, amp, Dialect::Clingo),
            Separator::Whitespace,
            "`.` cannot abut `&` at a guard-end"
        );
    }
}
