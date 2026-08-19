//! Theory atoms and `#theory` definitions (docs/design/syntax.md §6.3):
//! the theory regions and their modes — the parser tells the source the
//! mode for each token: theory mode from the token after the `{` that
//! opens the elements through the elements and the guard, normal mode
//! for each element's condition and for the `;` or `}` that ends it, and
//! at the operator positions of a definition — the greedy guard end, and
//! the definitions item by item.

use rowan::Checkpoint;

use crate::diagnostic::{Expected, GrammarWord, SyntaxClass};
use crate::token::{LexMode, TokenSource};
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

use super::machine::expected;

impl<S: TokenSource> Parser<'_, S> {
    /// Grammar §5.8's `theory-atom`, its node opened at `start` (around
    /// the negation run in body position): the name and its arguments in
    /// normal mode, then the elements and the guard.
    pub(super) fn theory_atom(&mut self, start: Checkpoint) {
        self.start_node_at(start, SyntaxKind::THEORY_ATOM);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.arguments(TermContext::Term);
        }
        if !self.depth_refused() && self.peek() == SyntaxKind::L_BRACE {
            self.theory_elements();
            if !self.depth_refused() {
                self.theory_guard();
            }
        }
        self.finish_node();
    }

    /// `"{" [ theory-elements ] "}"`: the brace taken in normal mode,
    /// theory mode from the token after it; each element's condition
    /// returns the mode to normal, and the `;` or `}` after a condition
    /// is taken in normal mode; theory mode resumes at the token after
    /// that `;`. An unclosed brace recovers at the dot.
    fn theory_elements(&mut self) {
        self.start_node(SyntaxKind::THEORY_ELEMENTS);
        self.bump();
        self.set_mode(LexMode::Theory);
        let mut after_separator = false;
        loop {
            if self.depth_refused() {
                break;
            }
            match self.peek() {
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {
                    if after_separator {
                        // `;` promises an element (grammar §5.8).
                        self.unexpected(
                            expected(&[Expected::Class(SyntaxClass::TheoryTerm)]),
                            None,
                        );
                    }
                    if self.peek() != SyntaxKind::R_BRACE {
                        self.expected_token(SyntaxKind::R_BRACE);
                    }
                    break;
                }
                _ => {}
            }
            if !self.theory_element() {
                if self.peek() != SyntaxKind::SEMICOLON {
                    self.wrap_unexpected(
                        expected(&[Expected::Class(SyntaxClass::TheoryTerm)]),
                        None,
                    );
                    continue;
                }
                self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
            }
            if self.depth_refused() {
                break;
            }
            after_separator = false;
            match self.peek() {
                SyntaxKind::SEMICOLON => {
                    self.bump();
                    self.set_mode(LexMode::Theory);
                    after_separator = true;
                }
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                _ => self.wrap_unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::R_BRACE),
                    ]),
                    None,
                ),
            }
        }
        self.eat(SyntaxKind::R_BRACE);
        self.finish_node();
    }

    /// Grammar §5.8's `theory-element`: opterms between commas, an
    /// optional condition after a colon at element depth — the colon a
    /// length-one structural run under theory mode, the condition in
    /// normal mode — or a colon and condition alone.
    fn theory_element(&mut self) -> bool {
        if self.peek() != SyntaxKind::COLON && !self.theory_opterm_begins() {
            return false;
        }
        self.start_node(SyntaxKind::THEORY_ELEMENT);
        if self.peek() != SyntaxKind::COLON {
            loop {
                if !self.theory_opterm() {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    break;
                }
                if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                    break;
                }
            }
        }
        if !self.depth_refused() && self.peek() == SyntaxKind::COLON {
            self.bump();
            self.set_mode(LexMode::Normal);
            self.condition_or_empty();
        }
        self.finish_node();
        true
    }

    /// Whether an opterm begins here — a theory term, or a leading theory
    /// operator run (grammar §5.8).
    fn theory_opterm_begins(&mut self) -> bool {
        self.theory_term_begins() || self.theory_operator_here()
    }

    /// The guard after the elements, greedy (docs/design/syntax.md §6.3):
    /// the next token taken under theory mode; not a theory operator —
    /// no guard, and that token re-lexed under normal mode; a theory
    /// operator — the guard opens and its opterm extends while the next
    /// token continues it, and the first token that does not is re-lexed
    /// under normal mode.
    fn theory_guard(&mut self) {
        self.set_mode(LexMode::Theory);
        if !self.theory_operator_here() {
            self.set_mode(LexMode::Normal);
            return;
        }
        self.start_node(SyntaxKind::THEORY_GUARD);
        self.bump();
        if !self.theory_opterm() {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
        }
        self.finish_node();
        self.set_mode(LexMode::Normal);
    }

    /// Grammar §5.9's `theory-definition`, opened at `checkpoint`: items
    /// between semicolons, term definitions and atom definitions
    /// interleaved in any order at the pin.
    pub(super) fn theory_definition(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::THEORY_DEFINITION);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    SyntaxKind::IDENT => self.term_definition(),
                    SyntaxKind::AMPERSAND => self.atom_definition(),
                    _ => {
                        self.skip_into_error(
                            expected(&[
                                Expected::Token(SyntaxKind::IDENT),
                                Expected::Token(SyntaxKind::AMPERSAND),
                            ]),
                            None,
                            &[SyntaxKind::SEMICOLON, SyntaxKind::R_BRACE, SyntaxKind::DOT],
                        );
                    }
                }
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                    _ => self.expected_token(SyntaxKind::SEMICOLON),
                }
            }
            if !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.statement_end();
        self.finish_node();
    }

    /// `IDENTIFIER "{" [ op-definitions ] "}"`.
    fn term_definition(&mut self) {
        self.start_node(SyntaxKind::TERM_DEFINITION);
        self.bump();
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    _ => {}
                }
                self.op_definition();
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => {}
                    _ => self.skip_into_error(
                        expected(&[
                            Expected::Token(SyntaxKind::SEMICOLON),
                            Expected::Token(SyntaxKind::R_BRACE),
                        ]),
                        None,
                        &[SyntaxKind::SEMICOLON, SyntaxKind::R_BRACE, SyntaxKind::DOT],
                    ),
                }
            }
            if !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.finish_node();
    }

    /// `theory-op ":" NUMBER "," ( "unary" | "binary" "," ( "left" | "right" ) )`
    /// — the operator position under theory mode, the rest normal.
    fn op_definition(&mut self) {
        self.start_node(SyntaxKind::OP_DEFINITION);
        self.operator_position();
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::NUMBER);
        self.expect(SyntaxKind::COMMA);
        if self.eat_word(GrammarWord::Binary) {
            self.expect(SyntaxKind::COMMA);
            if !(self.eat_word(GrammarWord::Left) || self.eat_word(GrammarWord::Right)) {
                self.unexpected(
                    expected(&[
                        Expected::Word(GrammarWord::Left),
                        Expected::Word(GrammarWord::Right),
                    ]),
                    None,
                );
            }
        } else if !self.eat_word(GrammarWord::Unary) {
            self.unexpected(
                expected(&[
                    Expected::Word(GrammarWord::Unary),
                    Expected::Word(GrammarWord::Binary),
                ]),
                None,
            );
        }
        self.finish_node();
    }

    /// `"&" IDENTIFIER "/" NUMBER ":" IDENTIFIER "," [ "{" [ theory-op { "," theory-op } ] "}" "," IDENTIFIER "," ] ( "head" | "body" | "any" | "directive" )`.
    fn atom_definition(&mut self) {
        self.start_node(SyntaxKind::ATOM_DEFINITION);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::SLASH);
        self.expect(SyntaxKind::NUMBER);
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COMMA);
        if self.eat(SyntaxKind::L_BRACE) {
            self.set_mode(LexMode::Theory);
            if self.theory_operator_here() {
                loop {
                    self.operator_position();
                    if !self.eat(SyntaxKind::COMMA) {
                        break;
                    }
                    self.set_mode(LexMode::Theory);
                }
            }
            self.set_mode(LexMode::Normal);
            self.expect(SyntaxKind::R_BRACE);
            self.expect(SyntaxKind::COMMA);
            self.expect(SyntaxKind::IDENT);
            self.expect(SyntaxKind::COMMA);
        }
        let words = [
            GrammarWord::Head,
            GrammarWord::Body,
            GrammarWord::Any,
            GrammarWord::Directive,
        ];
        if !words.iter().any(|word| self.eat_word(*word)) {
            self.unexpected(
                words.iter().map(|word| Expected::Word(*word)).collect(),
                None,
            );
        }
        self.finish_node();
    }

    /// One operator position of a definition: the token taken under
    /// theory mode, the mode returned to normal after it.
    fn operator_position(&mut self) {
        self.set_mode(LexMode::Theory);
        if self.theory_operator_here() {
            self.bump();
        } else {
            self.unexpected(
                expected(&[Expected::Class(SyntaxClass::TheoryOperator)]),
                None,
            );
        }
        self.set_mode(LexMode::Normal);
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Expected, GrammarWord, SyntaxErrorKind};

    use crate::parse::test_util::{kinds, member, shape};

    #[test]
    fn theory_atoms_take_a_name_arguments_elements_and_one_guard() {
        assert_eq!(shape("&a."), "(RULE (THEORY_ATOM & a) .)");
        assert_eq!(
            shape("&a {}."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { })) .)"
        );
        assert_eq!(
            shape("&a(1, X) { }."),
            "(RULE (THEORY_ATOM & a (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1) , (VARIABLE_TERM X)) )) (THEORY_ELEMENTS { })) .)"
        );
        assert_eq!(
            shape("&a { x not y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x) not (CONSTANT_TERM y))) })) .)"
        );
        assert_eq!(
            shape(":- &sum { x } >= 5, not p."),
            "(RULE :- (BODY (THEORY_ATOM & sum (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }) (THEORY_GUARD >= (THEORY_OPTERM (CONSTANT_TERM 5)))) , (LITERAL not (ATOM p))) .)"
        );
        assert_eq!(
            shape("&dom { 0..B } = v."),
            "(RULE (THEORY_ATOM & dom (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM 0) .. (VARIABLE_TERM B))) }) (THEORY_GUARD = (THEORY_OPTERM (CONSTANT_TERM v)))) .)"
        );
        assert!(member("&a { x :-: y ; :-: z }."));
        assert_eq!(
            shape("&a { x :-: y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x) :-: (CONSTANT_TERM y))) })) .)"
        );
    }

    #[test]
    fn elements_take_conditions_in_normal_mode_and_the_semicolon_at_element_depth_ends_them() {
        assert_eq!(
            shape("&a { t : p((x;y)), q ; u }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM t)) : (CONDITION (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (POOL ( (TUPLE (CONSTANT_TERM x)) ; (TUPLE (CONSTANT_TERM y)) ))) )))) , (LITERAL (ATOM q)))) ; (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM u))) })) .)"
        );
        assert_eq!(
            shape("&a { : p }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT : (CONDITION (LITERAL (ATOM p)))) })) .)"
        );
        assert_eq!(
            shape("&a { x : }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x)) : (CONDITION)) })) .)"
        );
        assert!(member("&a { x, y : p ; z }."));
        assert_eq!(
            shape("&a { x, y }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x)) , (THEORY_OPTERM (CONSTANT_TERM y))) })) .)"
        );
    }

    #[test]
    fn theory_terms_take_the_bracketed_and_function_shapes() {
        assert_eq!(
            shape("&a { {x, y}, [1, 2], (b,), f(g(1)), (), (c) }."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (THEORY_SET { (THEORY_OPTERM (CONSTANT_TERM x)) , (THEORY_OPTERM (CONSTANT_TERM y)) })) , (THEORY_OPTERM (THEORY_LIST [ (THEORY_OPTERM (CONSTANT_TERM 1)) , (THEORY_OPTERM (CONSTANT_TERM 2)) ])) , (THEORY_OPTERM (THEORY_TUPLE ( (THEORY_OPTERM (CONSTANT_TERM b)) , ))) , (THEORY_OPTERM (THEORY_FUNCTION f ( (THEORY_OPTERM (THEORY_FUNCTION g ( (THEORY_OPTERM (CONSTANT_TERM 1)) ))) ))) , (THEORY_OPTERM (THEORY_TUPLE ( ))) , (THEORY_OPTERM (THEORY_TUPLE ( (THEORY_OPTERM (CONSTANT_TERM c)) )))) })) .)"
        );
        assert!(member("&a { - - x, not - y, #inf, \"s\", X }."));
    }

    #[test]
    fn the_guard_is_greedy_and_the_first_token_that_does_not_continue_it_is_read_in_normal_mode() {
        assert_eq!(
            shape("&a { x } > - not - , p."),
            "(RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }) (THEORY_GUARD > (THEORY_OPTERM - not -))) (ERROR , p) .)"
        );
        assert!(!member("&a { x } > - not - , p."));
        assert!(member(":- &a { x } <= 3, &b { y }, p."));
    }

    #[test]
    fn theory_mode_diagnoses_the_anonymous_variable_and_the_nested_colon() {
        assert!(
            kinds("&a { _ }.")
                .iter()
                .any(|kind| matches!(kind, SyntaxErrorKind::AnonymousInTheoryExpression))
        );
        assert!(!member("&a { {x : y} }."));
        assert!(!member("&a { x ; }."));
    }

    #[test]
    fn negation_stands_inside_the_theory_atom_in_body_position_only() {
        assert_eq!(
            shape(":- not &a { x }."),
            "(RULE :- (BODY (THEORY_ATOM not & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) }))) .)"
        );
        assert!(!member("not &a { x }."));
    }

    #[test]
    fn theory_definitions_take_term_and_atom_definitions_with_operator_positions_in_theory_mode() {
        let text = "#theory cp { var_term { }; sum_term { - : 3, unary; ** : 2, binary, right; + : 0, binary, left }; &sum/0 : sum_term, {<=,=,!=,<,>,>=}, sum_term, any; &show/0 : sum_term, directive }.";
        assert!(member(text));
        let shape = shape(text);
        assert!(shape.contains("(OP_DEFINITION - : 3 , unary)"));
        assert!(shape.contains("(OP_DEFINITION ** : 2 , binary , right)"));
        assert!(shape.contains(
            "(ATOM_DEFINITION & sum / 0 : sum_term , { <= , = , != , < , > , >= } , sum_term , any)"
        ));
        assert!(shape.contains("(ATOM_DEFINITION & show / 0 : sum_term , directive)"));
        assert!(
            shape.starts_with("(THEORY_DEFINITION #theory cp { (TERM_DEFINITION var_term { })")
        );
        assert!(!member("#theory t { x { + : 1, ternary } }."));
        assert!(kinds("#theory t { x { + : 1, ternary } }.").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Word(GrammarWord::Unary)) && expected.contains(&Expected::Word(GrammarWord::Binary))
        )));
    }
}
