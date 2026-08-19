//! Weak constraints, optimize statements, the directives, the
//! annotations after the dot, and the script region
//! (docs/design/syntax.md §6.3, §6.7; grammar §5.7, §5.9, §5.11, §4.8).

use rowan::Checkpoint;

use crate::diagnostic::{Expected, GrammarWord, Hint, SyntaxClass};
use crate::token::{LexMode, TokenSource};
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

use super::machine::expected;

/// The four families whose dot is followed by a bracketed annotation
/// (grammar §5.11), each with its own interior (docs/design/syntax.md
/// §6.3): the node kind is one because the bracket shape is one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Annotation {
    /// `[ weight [@ priority] [, terms] ]`, required.
    WeakConstraint,
    /// `[ weight [@ priority] , modifier ]`, required.
    Heuristic,
    /// `[ term ]`, optional.
    External,
    /// `[ default | override ]`, optional.
    Const,
}

impl<S: TokenSource> Parser<'_, S> {
    /// An identifier the grammar wants by spelling (grammar §5.9):
    /// consumed when next.
    pub(super) fn eat_word(&mut self, word: GrammarWord) -> bool {
        if self.peek() == SyntaxKind::IDENT && self.peek_text() == word.spelling() {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Grammar §5.7's `weak-constraint`: the neck, an optional body, the
    /// dot, and its required annotation.
    pub(super) fn weak_constraint(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::WEAK_CONSTRAINT);
        self.bump();
        if self.peek() != SyntaxKind::DOT {
            self.body();
        }
        self.statement_end();
        self.annotation(Annotation::WeakConstraint, true);
        self.finish_node();
    }

    /// Grammar §5.7's `optimize-statement`: elements between `;`, each a
    /// weight, an optional priority, an optional tuple, and an optional
    /// condition.
    pub(super) fn optimize(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::OPTIMIZE_STATEMENT);
        self.bump();
        if self.expect(SyntaxKind::L_BRACE) {
            loop {
                match self.peek() {
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => break,
                    _ => {}
                }
                if self.term_begins() {
                    self.optimize_element();
                } else {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    continue;
                }
                if self.depth_refused() {
                    break;
                }
                match self.peek() {
                    SyntaxKind::SEMICOLON => self.bump(),
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
            if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        self.statement_end();
        self.finish_node();
    }

    /// `term [ "@" term ] [ "," terms ] [ ":" [ condition ] ]`.
    fn optimize_element(&mut self) {
        self.start_node(SyntaxKind::OPTIMIZE_ELEMENT);
        self.weighted_terms();
        if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
            self.condition_or_empty();
        }
        self.finish_node();
    }

    /// `term [ "@" term ] [ "," terms ]` — the weight, the priority, the
    /// tuple, shared by the optimize element and the weak constraint's
    /// annotation.
    fn weighted_terms(&mut self) {
        if !self.term(TermContext::Term) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        if self.depth_refused() {
            return;
        }
        if self.eat(SyntaxKind::AT) && !self.term(TermContext::Term) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        while !self.depth_refused() && self.eat(SyntaxKind::COMMA) {
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                break;
            }
        }
    }

    /// Grammar §5.9's `show-statement`, by the show-signature reading:
    /// `[-] IDENT / NUMBER .` — trivia legal between — is the signature
    /// form; anything else after `#show` is the term form.
    pub(super) fn show(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::SHOW_STATEMENT);
        self.bump();
        if self.peek() == SyntaxKind::DOT {
        } else if self.signature_follows() {
            self.signature();
        } else if self.term(TermContext::Term) {
            if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
                self.body();
            }
        } else {
            self.unexpected(
                expected(&[
                    Expected::Class(SyntaxClass::Signature),
                    Expected::Class(SyntaxClass::Term),
                    Expected::Token(SyntaxKind::DOT),
                ]),
                None,
            );
        }
        self.statement_end();
        self.finish_node();
    }

    /// The show-signature reading (grammar §5.9): five tokens of lookahead,
    /// the parser's maximum (docs/design/syntax.md §6.2).
    fn signature_follows(&mut self) -> bool {
        let mut at = 0;
        if self.lookahead(at) == SyntaxKind::MINUS {
            at += 1;
        }
        self.lookahead(at) == SyntaxKind::IDENT
            && self.lookahead(at + 1) == SyntaxKind::SLASH
            && self.lookahead(at + 2) == SyntaxKind::NUMBER
            && self.lookahead(at + 3) == SyntaxKind::DOT
    }

    /// Grammar §5.9's `signature`: `[-] IDENT / NUMBER`.
    fn signature(&mut self) {
        self.start_node(SyntaxKind::SIGNATURE);
        self.eat(SyntaxKind::MINUS);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::SLASH);
        self.expect(SyntaxKind::NUMBER);
        self.finish_node();
    }

    /// Grammar §5.2's `atom` where the grammar wants exactly one:
    /// `[-] IDENT [ arguments ]`.
    fn atom(&mut self) {
        if !matches!(self.peek(), SyntaxKind::MINUS | SyntaxKind::IDENT) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Atom)]), None);
            return;
        }
        let checkpoint = self.checkpoint();
        self.eat(SyntaxKind::MINUS);
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.arguments(TermContext::Term);
        }
        self.start_node_at(checkpoint, SyntaxKind::ATOM);
        self.finish_node();
    }

    /// Grammar §5.9's `conditional-dot` before the dot: nothing, or `:`
    /// with an empty body placed after it, or `:` and a body.
    fn conditional_dot(&mut self) {
        if self.depth_refused() || !self.eat(SyntaxKind::COLON) {
            return;
        }
        if self.peek() == SyntaxKind::DOT {
            self.empty_node(SyntaxKind::BODY);
        } else {
            self.body();
        }
    }

    /// Grammar §5.9's `project-statement`: the signature form when the
    /// signature reading holds, else the atom form with its conditional
    /// dot.
    pub(super) fn project(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::PROJECT_STATEMENT);
        self.bump();
        if self.signature_follows() {
            self.signature();
        } else {
            self.atom();
            self.conditional_dot();
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `defined-statement`.
    pub(super) fn defined(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::DEFINED_STATEMENT);
        self.bump();
        if matches!(self.peek(), SyntaxKind::MINUS | SyntaxKind::IDENT) {
            self.signature();
        } else {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Signature)]), None);
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `edge-statement`: pairs between `;` inside the
    /// parentheses, then the conditional dot.
    pub(super) fn edge(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::EDGE_STATEMENT);
        self.bump();
        if self.expect(SyntaxKind::L_PAREN) {
            loop {
                if self.term_begins() {
                    self.start_node(SyntaxKind::EDGE);
                    self.term(TermContext::Term);
                    if !self.depth_refused()
                        && self.expect(SyntaxKind::COMMA)
                        && !self.term(TermContext::Term)
                    {
                        self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    }
                    self.finish_node();
                } else {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if self.depth_refused() || !self.eat(SyntaxKind::SEMICOLON) {
                    break;
                }
            }
            if !self.depth_refused() && !self.eat(SyntaxKind::R_PAREN) {
                self.expected_token(SyntaxKind::R_PAREN);
            }
        }
        self.conditional_dot();
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `heuristic-statement`: the atom, the conditional
    /// dot, and its required annotation — its bracket is mandatory, and
    /// its absence carries the hint.
    pub(super) fn heuristic(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::HEURISTIC_STATEMENT);
        self.bump();
        self.atom();
        self.conditional_dot();
        self.statement_end();
        self.annotation(Annotation::Heuristic, true);
        self.finish_node();
    }

    /// Grammar §5.9's `external-statement`: the atom, the conditional
    /// dot, and its optional annotation.
    pub(super) fn external(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::EXTERNAL_STATEMENT);
        self.bump();
        self.atom();
        self.conditional_dot();
        self.statement_end();
        self.annotation(Annotation::External, false);
        self.finish_node();
    }

    /// Grammar §5.9's `const-statement`: the name, `=`, the term under the
    /// constant restriction, the dot, and its optional annotation.
    pub(super) fn constant(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::CONST_STATEMENT);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.expect(SyntaxKind::EQ) && !self.term(TermContext::ConstantTerm) {
            self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
        }
        self.statement_end();
        self.annotation(Annotation::Const, false);
        self.finish_node();
    }

    /// The bracketed annotation after the dot (grammar §5.11;
    /// docs/design/syntax.md §6.3): parsed by the family, arity and
    /// spelling included; a violation is `unexpected-token` with the
    /// family's expected set. When `required` and no `[` follows, the
    /// missing bracket is diagnosed — for `#heuristic` with its hint.
    fn annotation(&mut self, family: Annotation, required: bool) {
        if self.depth_refused() {
            return;
        }
        if self.peek() != SyntaxKind::L_BRACKET {
            if required {
                let hint =
                    (family == Annotation::Heuristic).then_some(Hint::HeuristicNeedsAnnotation);
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Annotation)]), hint);
            }
            return;
        }
        self.start_node(SyntaxKind::ANNOTATION);
        self.bump();
        match family {
            Annotation::WeakConstraint => self.weighted_terms(),
            Annotation::Heuristic => {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if !self.depth_refused()
                    && self.eat(SyntaxKind::AT)
                    && !self.term(TermContext::Term)
                {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
                if !self.depth_refused()
                    && self.expect(SyntaxKind::COMMA)
                    && !self.term(TermContext::Term)
                {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
            }
            Annotation::External => {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                }
            }
            Annotation::Const => {
                if !(self.eat_word(GrammarWord::Default) || self.eat_word(GrammarWord::Override)) {
                    self.unexpected(
                        expected(&[
                            Expected::Word(GrammarWord::Default),
                            Expected::Word(GrammarWord::Override),
                        ]),
                        None,
                    );
                }
            }
        }
        if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACKET) {
            if self.at_end() || self.statement_begins() {
                self.expected_token(SyntaxKind::R_BRACKET);
            } else {
                self.skip_into_error(
                    expected(&[Expected::Token(SyntaxKind::R_BRACKET)]),
                    None,
                    &[SyntaxKind::R_BRACKET],
                );
                self.eat(SyntaxKind::R_BRACKET);
            }
        }
        self.finish_node();
    }

    /// Grammar §5.9's `script-statement` (grammar §4.8): after the
    /// header the parser asks under script mode and receives the body
    /// token, or `#end` directly when the region is empty; after the
    /// body it asks for `#end` under the same mode, then returns to
    /// normal mode for the dot. An unterminated region is a lexical
    /// `ERROR` to end of input; the missing `#end` and dot are then
    /// missing children.
    pub(super) fn script(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::SCRIPT_STATEMENT);
        self.bump();
        // `&`, not `&&`: run all three `expect`s even when an earlier one
        // fails, so a malformed header still places each token it does have
        // and reports every miss (docs/design/syntax.md §6.7).
        let header = self.expect(SyntaxKind::L_PAREN)
            & self.expect(SyntaxKind::IDENT)
            & self.expect(SyntaxKind::R_PAREN);
        if header {
            self.set_mode(LexMode::ScriptBody);
            match self.peek() {
                SyntaxKind::SCRIPT_BODY | SyntaxKind::ERROR => self.bump(),
                _ => {}
            }
            self.expect(SyntaxKind::KW_END);
            self.set_mode(LexMode::Normal);
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `include-statement`: a string, or the three-token
    /// angle form.
    pub(super) fn include(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::INCLUDE_STATEMENT);
        self.bump();
        match self.peek() {
            SyntaxKind::STRING => self.bump(),
            SyntaxKind::LT => {
                self.bump();
                self.expect(SyntaxKind::IDENT);
                self.expect(SyntaxKind::GT);
            }
            _ => self.unexpected(
                expected(&[
                    Expected::Token(SyntaxKind::STRING),
                    Expected::Token(SyntaxKind::LT),
                ]),
                None,
            ),
        }
        self.statement_end();
        self.finish_node();
    }

    /// Grammar §5.9's `program-statement`: the name and an optional
    /// identifier-only parameter list.
    pub(super) fn program_statement(&mut self, checkpoint: Checkpoint) {
        self.start_node_at(checkpoint, SyntaxKind::PROGRAM_STATEMENT);
        self.bump();
        self.expect(SyntaxKind::IDENT);
        if self.peek() == SyntaxKind::L_PAREN {
            self.start_node(SyntaxKind::PARAMETERS);
            self.bump();
            if self.peek() == SyntaxKind::IDENT {
                loop {
                    self.expect(SyntaxKind::IDENT);
                    if !self.eat(SyntaxKind::COMMA) {
                        break;
                    }
                }
            }
            if !self.eat(SyntaxKind::R_PAREN) {
                if self.at_end() || self.peek() == SyntaxKind::DOT {
                    self.expected_token(SyntaxKind::R_PAREN);
                } else {
                    // A token that is neither a parameter nor the closer —
                    // `#program p(1)`, the numeral where an identifier is
                    // wanted: skip it and any that follow into one `ERROR`,
                    // up to the closer or the statement's own dot, so the
                    // directive owns its extent and synchronizes at its dot
                    // instead of spilling the tail into a following rule
                    // (docs/design/syntax.md §6.7).
                    self.skip_into_error(
                        expected(&[Expected::Token(SyntaxKind::R_PAREN)]),
                        None,
                        &[SyntaxKind::R_PAREN, SyntaxKind::DOT],
                    );
                    self.eat(SyntaxKind::R_PAREN);
                }
            }
            self.finish_node();
        }
        self.statement_end();
        self.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        Expected, GrammarWord, Hint, RestrictedForm, Restriction, SyntaxErrorKind,
    };
    use crate::dialect::Dialect;
    use crate::parse::parse;

    use crate::parse::test_util::{admitted, kinds, member, shape};

    #[test]
    fn weak_constraints_carry_their_annotation_after_the_dot() {
        assert_eq!(
            shape(":~ p(X). [1@2, X]"),
            "(WEAK_CONSTRAINT :~ (BODY (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) . (ANNOTATION [ (CONSTANT_TERM 1) @ (CONSTANT_TERM 2) , (VARIABLE_TERM X) ]))"
        );
        assert_eq!(
            shape(":~ . [1]"),
            "(WEAK_CONSTRAINT :~ . (ANNOTATION [ (CONSTANT_TERM 1) ]))"
        );
        assert!(member(":~ p, q. [W@P, a, b]"));
        assert!(!member(":~ p."), "the annotation is required");
        assert!(
            kinds(":~ p.")
                .iter()
                .any(|kind| matches!(kind, SyntaxErrorKind::UnexpectedEndOfInput { .. }))
        );
    }

    #[test]
    fn optimize_statements_take_weighted_elements() {
        assert_eq!(
            shape("#minimize { W@P,T : p(T,W) ; 1 }."),
            "(OPTIMIZE_STATEMENT #minimize { (OPTIMIZE_ELEMENT (VARIABLE_TERM W) @ (VARIABLE_TERM P) , (VARIABLE_TERM T) : (CONDITION (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM T) , (VARIABLE_TERM W)) )))))) ; (OPTIMIZE_ELEMENT (CONSTANT_TERM 1)) } .)"
        );
        assert!(member("#maximise { }."));
        assert!(member("#minimize { 1 : }."));
    }

    #[test]
    fn show_takes_its_four_forms_by_the_signature_reading() {
        assert_eq!(shape("#show."), "(SHOW_STATEMENT #show .)");
        assert_eq!(
            shape("#show p/2."),
            "(SHOW_STATEMENT #show (SIGNATURE p / 2) .)"
        );
        assert_eq!(
            shape("#show -p/2."),
            "(SHOW_STATEMENT #show (SIGNATURE - p / 2) .)"
        );
        assert_eq!(
            shape("#show p/2 : q."),
            "(SHOW_STATEMENT #show (BINARY_TERM (CONSTANT_TERM p) / (CONSTANT_TERM 2)) : (BODY (LITERAL (ATOM q))) .)"
        );
        assert_eq!(
            shape("#show (p/2)."),
            "(SHOW_STATEMENT #show (POOL ( (TUPLE (BINARY_TERM (CONSTANT_TERM p) / (CONSTANT_TERM 2))) )) .)"
        );
        assert_eq!(
            shape("#show f(X) : p(X)."),
            "(SHOW_STATEMENT #show (FUNCTION_TERM f (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)"
        );
        assert!(
            !member("#show $x/1."),
            "grammar §11: both readings are errors"
        );
    }

    #[test]
    fn project_defined_edge_heuristic_and_external_take_their_shapes() {
        assert_eq!(
            shape("#project p/1."),
            "(PROJECT_STATEMENT #project (SIGNATURE p / 1) .)"
        );
        assert_eq!(
            shape("#project p(X) : q(X)."),
            "(PROJECT_STATEMENT #project (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)"
        );
        assert_eq!(
            shape("#project p : ."),
            "(PROJECT_STATEMENT #project (ATOM p) : (BODY) .)"
        );
        assert_eq!(
            shape("#defined p/2."),
            "(DEFINED_STATEMENT #defined (SIGNATURE p / 2) .)"
        );
        assert_eq!(
            shape("#edge (a,b; c,d) : e."),
            "(EDGE_STATEMENT #edge ( (EDGE (CONSTANT_TERM a) , (CONSTANT_TERM b)) ; (EDGE (CONSTANT_TERM c) , (CONSTANT_TERM d)) ) : (BODY (LITERAL (ATOM e))) .)"
        );
        assert_eq!(
            shape("#heuristic a. [1@2, sign]"),
            "(HEURISTIC_STATEMENT #heuristic (ATOM a) . (ANNOTATION [ (CONSTANT_TERM 1) @ (CONSTANT_TERM 2) , (CONSTANT_TERM sign) ]))"
        );
        assert!(member("#heuristic a : b. [1,sign]"));
        assert!(!member("#heuristic a."));
        assert!(kinds("#heuristic a.").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedEndOfInput {
                hint: Some(Hint::HeuristicNeedsAnnotation),
                ..
            }
        )));
        assert_eq!(
            shape("#external p(X) : q(X). [true]"),
            "(EXTERNAL_STATEMENT #external (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) : (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) . (ANNOTATION [ (CONSTANT_TERM true) ]))"
        );
        assert!(member("#external p."));
        assert!(
            !member("#external p. [a, b]"),
            "docs/design/syntax.md §6.3's non-member"
        );
    }

    #[test]
    fn const_takes_the_constant_restriction_and_the_policy_word() {
        assert_eq!(
            shape("#const n = 1."),
            "(CONST_STATEMENT #const n = (CONSTANT_TERM 1) .)"
        );
        assert_eq!(
            shape("#const n = 1. [override]"),
            "(CONST_STATEMENT #const n = (CONSTANT_TERM 1) . (ANNOTATION [ override ]))"
        );
        assert!(member("#const n = f(1,2) + |3|. [default]"));
        assert!(member("#const default = 1."));
        assert!(
            !member("#const n = 1. [foo]"),
            "docs/design/syntax.md §6.3's non-member"
        );
        assert!(kinds("#const n = 1. [foo]").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Word(GrammarWord::Default)) && expected.contains(&Expected::Word(GrammarWord::Override))
        )));
        assert!(
            kinds("#const x = |1;2|.").contains(&SyntaxErrorKind::FormNotAllowedHere {
                form: RestrictedForm::PooledAbsoluteValue,
                context: Restriction::ConstantTerm,
            })
        );
        assert!(
            kinds("#const x = 1..3.").contains(&SyntaxErrorKind::FormNotAllowedHere {
                form: RestrictedForm::Interval,
                context: Restriction::ConstantTerm,
            })
        );
        assert!(
            kinds("#const x = X.").contains(&SyntaxErrorKind::FormNotAllowedHere {
                form: RestrictedForm::Variable,
                context: Restriction::ConstantTerm,
            })
        );
        assert!(
            member("#const x = @f(1)."),
            "the constant term admits the `@`-call"
        );
    }

    #[test]
    fn the_script_region_is_one_opaque_token_between_its_header_and_end() {
        assert_eq!(
            shape("#script (python)\ndef f(): return 1\n#end."),
            "(SCRIPT_STATEMENT #script ( python ) \ndef f(): return 1\n #end .)"
        );
        assert_eq!(
            shape("#script (lua)#end."),
            "(SCRIPT_STATEMENT #script ( lua ) #end .)"
        );
        assert!(member(
            "#script (python) % not a comment; end is the terminator\n#end.\np."
        ));
        let unterminated = kinds("#script (lua) x = 1");
        assert!(
            unterminated
                .iter()
                .any(|kind| matches!(kind, SyntaxErrorKind::UnterminatedScript))
        );
        let source = admitted("#script (lua) x = 1");
        assert!(parse(&source, Dialect::Clingo).is_incomplete());
    }

    #[test]
    fn include_and_program_take_their_forms() {
        assert_eq!(
            shape("#include \"a.lp\"."),
            "(INCLUDE_STATEMENT #include \"a.lp\" .)"
        );
        assert_eq!(
            shape("#include < lib > ."),
            "(INCLUDE_STATEMENT #include < lib > .)"
        );
        assert_eq!(
            shape("#program base."),
            "(PROGRAM_STATEMENT #program base .)"
        );
        assert_eq!(
            shape("#program step(t, k)."),
            "(PROGRAM_STATEMENT #program step (PARAMETERS ( t , k )) .)"
        );
        assert!(member("#program p()."));
    }

    #[test]
    fn a_bracket_after_any_other_statements_dot_is_the_next_statements_unexpected_token() {
        assert!(!member("p. [1]"));
        assert_eq!(shape("p. [1]"), "(RULE (LITERAL (ATOM p)) .) (ERROR [ 1 ])");
    }
}
