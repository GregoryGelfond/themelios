//! The grammar-bounded productions (docs/design/syntax.md §6.2, §6.3,
//! §6.7): rules and what they hold — literals, atoms, comparisons,
//! disjunctions, bodies, conditional literals, aggregates — each a
//! function on the call stack, its depth bounded by the grammar and
//! never by the input; the term families below them run on the frame
//! loop. The directives, weak constraints, and optimize statements, and
//! the theory atoms and definitions, join the dispatch from their own
//! modules.

use super::builder::Checkpoint;

use crate::diagnostic::{Expected, Hint, SyntaxClass};
use crate::dialect::Dialect;
use crate::token::TokenSource;
use crate::tree::SyntaxKind;

use super::machine::Parser;
use super::terms::TermContext;

/// Where an element stands, which fixes what may stand there: a head or
/// a body admits literals, aggregates, and theory atoms, and a literal
/// may take a condition; a set-aggregate or disjunction element admits a
/// literal with an optional condition; a condition, and a head
/// aggregate element's literal, admit a plain literal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Position {
    Head,
    Body,
    SetElement,
    ConditionLiteral,
}

impl Position {
    fn admits_aggregates(self) -> bool {
        matches!(self, Position::Head | Position::Body)
    }

    fn admits_condition(self) -> bool {
        !matches!(self, Position::ConditionLiteral)
    }
}

/// What an element parse read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Element {
    /// Nothing begins here; nothing was consumed.
    None,
    /// A literal, conditional when `condition` says so; `empty` when the
    /// condition after the colon held nothing.
    Literal { condition: bool, empty: bool },
    /// An aggregate.
    Aggregate,
    /// A theory atom.
    TheoryAtom,
    /// A bare atom in head position with the query mark after it — the
    /// ASP-Core-2 query's atom, not wrapped in a literal (grammar §6.1).
    QueryAtom,
}

/// The atom-shaped prefix `[-] IDENT [ arguments ]` of a literal or a
/// guard, kept as tokens until the token after it says what it was: an
/// atom, or the first term of a comparison or a guard.
struct AtomPrefix {
    /// Before the sign, if any.
    start: Checkpoint,
    negated: bool,
    /// Before the name.
    name: Checkpoint,
    has_arguments: bool,
}

/// What begins a literal or a guard: an atom-shaped prefix still open to
/// both readings, a complete term, or nothing.
enum First {
    Atom(AtomPrefix),
    Term(Checkpoint),
    Missing,
}

fn relation(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LT
            | SyntaxKind::LE
            | SyntaxKind::GT
            | SyntaxKind::GE
            | SyntaxKind::EQ
            | SyntaxKind::NEQ
    )
}

fn aggregate_opener(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::L_BRACE
            | SyntaxKind::KW_COUNT
            | SyntaxKind::KW_SUM
            | SyntaxKind::KW_SUM_PLUS
            | SyntaxKind::KW_MIN
            | SyntaxKind::KW_MAX
    )
}

use super::machine::expected;

const RELATIONS: [Expected; 6] = [
    Expected::Token(SyntaxKind::LT),
    Expected::Token(SyntaxKind::LE),
    Expected::Token(SyntaxKind::GT),
    Expected::Token(SyntaxKind::GE),
    Expected::Token(SyntaxKind::EQ),
    Expected::Token(SyntaxKind::NEQ),
];

impl<S: TokenSource> Parser<'_, S> {
    /// One statement, its node opened at `checkpoint` (before its docs):
    /// dispatched to its family by the first significant token, or a rule
    /// when no directive keyword stands there (grammar §5.7, §5.9).
    pub(super) fn statement(&mut self, checkpoint: Checkpoint) {
        self.enter_statement();
        match self.peek() {
            SyntaxKind::WEAK_NECK => self.weak_constraint(checkpoint),
            SyntaxKind::KW_MINIMIZE | SyntaxKind::KW_MAXIMIZE => self.optimize(checkpoint),
            SyntaxKind::KW_SHOW => self.show(checkpoint),
            SyntaxKind::KW_PROJECT => self.project(checkpoint),
            SyntaxKind::KW_DEFINED => self.defined(checkpoint),
            SyntaxKind::KW_EDGE => self.edge(checkpoint),
            SyntaxKind::KW_HEURISTIC => self.heuristic(checkpoint),
            SyntaxKind::KW_EXTERNAL => self.external(checkpoint),
            SyntaxKind::KW_CONST => self.constant(checkpoint),
            SyntaxKind::KW_SCRIPT => self.script(checkpoint),
            SyntaxKind::KW_INCLUDE => self.include(checkpoint),
            SyntaxKind::KW_PROGRAM => self.program_statement(checkpoint),
            SyntaxKind::KW_THEORY => self.theory_definition(checkpoint),
            _ => self.rule(checkpoint),
        }
        self.leave_statement();
    }

    /// Grammar §6.1's query reading: under the ASP-Core-2 dialect only,
    /// a `?` whose next significant token is end of input.
    pub(super) fn is_query_mark(&mut self) -> bool {
        self.dialect() == Dialect::AspCore2
            && self.peek() == SyntaxKind::QUESTION
            && self.lookahead(1) == SyntaxKind::EOF
    }

    /// Whether the next significant token continues a term as a binary
    /// operator — the query mark excluded.
    fn binary_operator_follows(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::DOTDOT
                | SyntaxKind::CARET
                | SyntaxKind::QUESTION
                | SyntaxKind::AMPERSAND
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::BACKSLASH
                | SyntaxKind::STAR_STAR
        ) && !self.is_query_mark()
    }

    /// Whether the next significant token begins a literal.
    pub(super) fn literal_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::KW_NOT | SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE
        ) || self.term_begins()
    }

    /// Whether the next significant token begins an element in `position`.
    pub(super) fn element_begins(&mut self, position: Position) -> bool {
        self.literal_begins()
            || (position.admits_aggregates()
                && (aggregate_opener(self.peek()) || self.peek() == SyntaxKind::AMPERSAND))
    }

    /// Grammar §5.7's `rule`, all five forms, opened at `checkpoint` — or
    /// grammar §6.1's query, when the head is a bare atom followed by the
    /// query mark: the statement parser, holding that atom with nothing
    /// before it, closes a `QUERY` (docs/design/syntax.md §6.3).
    pub(super) fn rule(&mut self, checkpoint: Checkpoint) {
        if self.peek() != SyntaxKind::NECK && self.head() {
            self.start_node_at(checkpoint, SyntaxKind::QUERY);
            self.bump();
            self.finish_node();
            return;
        }
        self.start_node_at(checkpoint, SyntaxKind::RULE);
        if !self.depth_refused() {
            if self.eat(SyntaxKind::NECK) {
                if self.peek() == SyntaxKind::DOT {
                    self.empty_node(SyntaxKind::BODY);
                } else {
                    self.body();
                }
                self.statement_end();
            } else {
                self.head_end();
            }
        }
        self.finish_node();
    }

    /// The terminator after a head that took no neck: a `.` ends the rule
    /// (a fact), and a `:-` would begin a body — the two ways to complete
    /// it, both named when neither is found so a reader or a tool can act
    /// (the authority names them too, more tersely). Recovers through to
    /// the dot.
    fn head_end(&mut self) {
        if self.eat(SyntaxKind::DOT) {
            return;
        }
        let wanted = || {
            expected(&[
                Expected::Token(SyntaxKind::DOT),
                Expected::Token(SyntaxKind::NECK),
            ])
        };
        if self.at_end() || self.statement_begins() {
            self.unexpected(wanted(), None);
            return;
        }
        self.skip_into_error(wanted(), None, &[SyntaxKind::DOT]);
        self.eat(SyntaxKind::DOT);
    }

    /// The statement's terminating dot: consumed when next; a missing
    /// dot before a statement start or end of input is diagnosed and
    /// nothing consumed; anything else before the dot is skipped into an
    /// `ERROR` node through to it. After a depth refusal the rest of the
    /// statement is already carried, and nothing happens here.
    pub(super) fn statement_end(&mut self) {
        if self.depth_refused() || self.eat(SyntaxKind::DOT) {
            return;
        }
        if self.at_end() || self.statement_begins() {
            self.expected_token(SyntaxKind::DOT);
            return;
        }
        self.skip_into_error(
            expected(&[Expected::Token(SyntaxKind::DOT)]),
            None,
            &[SyntaxKind::DOT],
        );
        self.eat(SyntaxKind::DOT);
    }

    /// Grammar §5.5's `head`: a single literal, a disjunction of
    /// literals, a single aggregate, or a single theory atom. Only a
    /// literal first element continues into a `DISJUNCTION` — on a
    /// conditioned literal or a following separator; an aggregate or a
    /// theory atom is a complete head, and a separator after it is a
    /// rule-level error, as under the authority (grammar §5.5: only
    /// literals disjoin). True when the head is the query's atom (grammar
    /// §6.1), which the caller closes.
    fn head(&mut self) -> bool {
        let checkpoint = self.checkpoint();
        let mut previous = match self.element(Position::Head) {
            Element::None => {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Head)]), None);
                return false;
            }
            Element::QueryAtom => return true,
            Element::Aggregate | Element::TheoryAtom => return false,
            element @ Element::Literal { .. } => element,
        };
        let separator = |kind: SyntaxKind| {
            matches!(
                kind,
                SyntaxKind::SEMICOLON | SyntaxKind::PIPE | SyntaxKind::COMMA
            )
        };
        let conditioned = matches!(
            previous,
            Element::Literal {
                condition: true,
                ..
            }
        );
        if !conditioned && !separator(self.peek()) {
            return false;
        }
        self.start_node_at(checkpoint, SyntaxKind::DISJUNCTION);
        loop {
            if self.depth_refused() {
                break;
            }
            let kind = self.peek();
            if separator(kind) {
                if kind == SyntaxKind::PIPE
                    && matches!(
                        previous,
                        Element::Literal {
                            condition: true,
                            empty: true
                        }
                    )
                {
                    // Grammar §5.5's stated hole: an empty-conditioned element
                    // directly before `|` does not parse; the parser names it
                    // and reads on as if `;` stood there.
                    self.unexpected(
                        expected(&[Expected::Class(SyntaxClass::Condition)]),
                        Some(Hint::EmptyConditionBeforePipe),
                    );
                }
                self.bump();
            } else if self.element_begins(Position::SetElement) {
                self.unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::PIPE),
                        Expected::Token(SyntaxKind::COMMA),
                        Expected::Token(SyntaxKind::NECK),
                        Expected::Token(SyntaxKind::DOT),
                    ]),
                    None,
                );
            } else {
                break;
            }
            previous = match self.element(Position::SetElement) {
                Element::None => {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    if separator(self.peek()) {
                        self.wrap_unexpected(
                            expected(&[Expected::Class(SyntaxClass::Literal)]),
                            None,
                        );
                        continue;
                    }
                    break;
                }
                element => element,
            };
        }
        self.finish_node();
        false
    }

    /// Grammar §5.6's `body-list`: elements between `,` and `;`. A missing
    /// separator before an element that begins is diagnosed and read
    /// past; a token that begins nothing is wrapped and the list
    /// continues; the neck and the dot end it.
    pub(super) fn body(&mut self) {
        self.start_node(SyntaxKind::BODY);
        loop {
            if self.depth_refused() {
                break;
            }
            if self.element(Position::Body) == Element::None {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::BodyElement)]), None);
                match self.peek() {
                    SyntaxKind::COMMA | SyntaxKind::SEMICOLON => {
                        self.wrap_unexpected(
                            expected(&[Expected::Class(SyntaxClass::BodyElement)]),
                            None,
                        );
                        continue;
                    }
                    SyntaxKind::DOT
                    | SyntaxKind::EOF
                    | SyntaxKind::NECK
                    | SyntaxKind::WEAK_NECK => break,
                    _ if self.statement_begins() => break,
                    _ => {
                        self.wrap_unexpected(
                            expected(&[Expected::Class(SyntaxClass::BodyElement)]),
                            None,
                        );
                        continue;
                    }
                }
            }
            // A depth refusal inside the element consumed the rest of the
            // statement, its dot included; the separator check below must not
            // run on the next statement's first token, or it reports a
            // spurious missing separator there — one refusal, one diagnostic
            // (docs/design/syntax.md §6.6, §4.5), as the sibling loops hold.
            if self.depth_refused() {
                break;
            }
            match self.peek() {
                SyntaxKind::COMMA | SyntaxKind::SEMICOLON => self.bump(),
                _ if self.element_begins(Position::Body) => self.unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::COMMA),
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::DOT),
                    ]),
                    None,
                ),
                _ => break,
            }
        }
        self.finish_node();
    }

    /// One element in `position` (grammar §5.2–§5.6, §5.8): the negation
    /// run first, placed inside whatever the element turns out to be —
    /// a literal, an aggregate, or a theory atom — then the element by
    /// its first token, and, where the position admits it, a condition
    /// after `:` wrapping the literal into a `CONDITIONAL_LITERAL`.
    // One decision by first token, each arm a distinct element shape (a
    // theory atom, an aggregate, a truth constant, a literal or guard),
    // with the two disallowed-position arms beside their admitted twins;
    // splitting it would scatter that single dispatch.
    #[allow(clippy::too_many_lines)]
    pub(super) fn element(&mut self, position: Position) -> Element {
        if !self.element_begins(position) {
            return Element::None;
        }
        let start = self.checkpoint();
        let mut negation = 0;
        while self.peek() == SyntaxKind::KW_NOT && negation < 2 {
            self.bump();
            negation += 1;
        }
        match self.peek() {
            SyntaxKind::AMPERSAND if !position.admits_aggregates() => {
                // A theory atom is not a literal; only a head or a body
                // admits one (grammar §5.6). In a condition or a set
                // element it is an error, carried whole without descending
                // — the descent would recurse without bound on nested
                // disallowed theory atoms, and the grammar admits no
                // nesting to descend into (docs/design/syntax.md §6.6).
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                self.skip_disallowed_element(start);
                Element::Literal {
                    condition: false,
                    empty: false,
                }
            }
            SyntaxKind::AMPERSAND => {
                if negation > 0 && position != Position::Body {
                    // Only a body element signs a theory atom (grammar §5.6).
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                }
                self.theory_atom(start);
                Element::TheoryAtom
            }
            kind if aggregate_opener(kind) && !position.admits_aggregates() => {
                // An aggregate is not a literal; only a head or a body
                // admits one (grammar §5.6). Carried whole without
                // descending — the same unbounded-recursion bound as the
                // theory atom above (docs/design/syntax.md §6.6).
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                self.skip_disallowed_element(start);
                Element::Literal {
                    condition: false,
                    empty: false,
                }
            }
            kind if aggregate_opener(kind) => {
                self.aggregate(start, position);
                Element::Aggregate
            }
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE => {
                self.start_node_at(start, SyntaxKind::LITERAL);
                self.bump();
                self.finish_node();
                self.conditional(start, position)
            }
            _ => match self.first() {
                First::Missing => {
                    // A negation run with no atom, `#true`, or `#false` after
                    // it (a third `not`, or a bare `not` at a boundary): a
                    // `LITERAL` holds one of those (tree.rs), so an atomless
                    // one is no literal — the negation is carried in an
                    // `ERROR` node instead (docs/design/syntax.md §6.7).
                    self.start_node_at(start, SyntaxKind::ERROR);
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    self.finish_node();
                    Element::Literal {
                        condition: false,
                        empty: false,
                    }
                }
                first => {
                    if self.depth_refused() {
                        self.finish_first_after_refusal(first);
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        return Element::Literal {
                            condition: false,
                            empty: false,
                        };
                    }
                    let kind = self.peek();
                    let guard_follows = aggregate_opener(kind)
                        || (relation(kind) && aggregate_opener(self.lookahead(1)));
                    if guard_follows {
                        if !position.admits_aggregates() {
                            self.unexpected(
                                expected(&[Expected::Class(SyntaxClass::Literal)]),
                                None,
                            );
                        }
                        let term = self.as_term(first);
                        self.start_node_at(term, SyntaxKind::GUARD);
                        if relation(self.peek()) {
                            self.bump();
                        }
                        self.finish_node();
                        self.aggregate(start, position);
                        Element::Aggregate
                    } else if relation(kind) {
                        let term = self.as_term(first);
                        self.comparison(term);
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        self.conditional(start, position)
                    } else {
                        match first {
                            First::Atom(prefix) => {
                                self.start_node_at(prefix.start, SyntaxKind::ATOM);
                                self.finish_node();
                                if position == Position::Head
                                    && negation == 0
                                    && self.is_query_mark()
                                {
                                    return Element::QueryAtom;
                                }
                            }
                            First::Term(_) => self.unexpected(expected(&RELATIONS), None),
                            First::Missing => unreachable!("matched above"),
                        }
                        self.start_node_at(start, SyntaxKind::LITERAL);
                        self.finish_node();
                        self.conditional(start, position)
                    }
                }
            },
        }
    }

    /// The atom-shaped prefix `[-] IDENT [ arguments ]`, or the term,
    /// that begins a literal or a guard (grammar §5.1–§5.2: `-p` is the
    /// atom in literal position and the arithmetic term where a relation
    /// makes the whole a comparison — the token after the prefix
    /// decides). An operator directly after the prefix makes it a term
    /// at once, and the frame loop continues from it.
    /// Carries a construct that opens like an element but is not admitted
    /// where it stands — an aggregate or a theory atom in a condition or a
    /// set element (grammar §5.3, §5.6) — from `start` (over the negation
    /// already placed) into one `ERROR` node, its bracket groups balanced,
    /// up to the boundary that ends the element. Lossless and iterative,
    /// never a descent: the descent recurses without bound on nested
    /// disallowed constructs, which the grammar never admits
    /// (docs/design/syntax.md §6.6).
    fn skip_disallowed_element(&mut self, start: Checkpoint) {
        self.start_node_at(start, SyntaxKind::ERROR);
        let mut depth = 0u32;
        while !self.at_end() {
            match self.peek() {
                SyntaxKind::L_PAREN | SyntaxKind::L_BRACKET | SyntaxKind::L_BRACE => {
                    depth += 1;
                    self.bump();
                }
                SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.bump();
                }
                SyntaxKind::DOT
                | SyntaxKind::SEMICOLON
                | SyntaxKind::COMMA
                | SyntaxKind::COLON
                | SyntaxKind::NECK
                | SyntaxKind::WEAK_NECK
                    if depth == 0 =>
                {
                    break;
                }
                _ => self.bump(),
            }
        }
        self.finish_node();
    }

    fn first(&mut self) -> First {
        let atom_shaped = self.peek() == SyntaxKind::IDENT
            || (self.peek() == SyntaxKind::MINUS && self.lookahead(1) == SyntaxKind::IDENT);
        if atom_shaped {
            let start = self.checkpoint();
            let negated = self.eat(SyntaxKind::MINUS);
            let name = self.checkpoint();
            self.bump();
            let has_arguments = self.peek() == SyntaxKind::L_PAREN;
            if has_arguments {
                self.arguments(TermContext::Term);
            }
            let prefix = AtomPrefix {
                start,
                negated,
                name,
                has_arguments,
            };
            if !self.depth_refused() && self.binary_operator_follows() {
                self.wrap_prefix_as_term(&prefix);
                self.term_continue(TermContext::Term, start);
                return First::Term(start);
            }
            return First::Atom(prefix);
        }
        if self.term_begins() {
            let start = self.checkpoint();
            self.term(TermContext::Term);
            return First::Term(start);
        }
        First::Missing
    }

    /// The prefix as the term it turned out to be: `FUNCTION_TERM` or
    /// `CONSTANT_TERM` around the name and its arguments, and
    /// `UNARY_TERM` around the sign.
    fn wrap_prefix_as_term(&mut self, prefix: &AtomPrefix) {
        let kind = if prefix.has_arguments {
            SyntaxKind::FUNCTION_TERM
        } else {
            SyntaxKind::CONSTANT_TERM
        };
        self.start_node_at(prefix.name, kind);
        self.finish_node();
        if prefix.negated {
            self.start_node_at(prefix.start, SyntaxKind::UNARY_TERM);
            self.finish_node();
        }
    }

    /// The first thing as a term, whichever it was: its checkpoint.
    fn as_term(&mut self, first: First) -> Checkpoint {
        match first {
            First::Atom(prefix) => {
                self.wrap_prefix_as_term(&prefix);
                prefix.start
            }
            First::Term(checkpoint) => checkpoint,
            First::Missing => unreachable!("a missing first is handled before"),
        }
    }

    /// After a depth refusal inside the first thing: the tokens stand as
    /// they were placed, an atom-shaped prefix closing as an atom.
    fn finish_first_after_refusal(&mut self, first: First) {
        if let First::Atom(prefix) = first {
            self.start_node_at(prefix.start, SyntaxKind::ATOM);
            self.finish_node();
        }
    }

    /// Grammar §5.2's `comparison` around a first term already built at
    /// `first`: the chain of relations and terms, one flat node.
    fn comparison(&mut self, first: Checkpoint) {
        self.start_node_at(first, SyntaxKind::COMPARISON);
        while relation(self.peek()) {
            self.bump();
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                break;
            }
            if self.depth_refused() {
                break;
            }
        }
        self.finish_node();
    }

    /// The condition a literal in `position` may take: `:` then a
    /// condition, or the empty condition placed immediately after the
    /// colon; the literal opened at `start` becomes a
    /// `CONDITIONAL_LITERAL`.
    fn conditional(&mut self, start: Checkpoint, position: Position) -> Element {
        if !position.admits_condition() || self.peek() != SyntaxKind::COLON {
            return Element::Literal {
                condition: false,
                empty: false,
            };
        }
        self.start_node_at(start, SyntaxKind::CONDITIONAL_LITERAL);
        self.bump();
        // Grammar §5.5's stated hole: an empty-conditioned element directly
        // before `|` reads the `|` as the disjunction separator — the
        // caller's loop names the mistake — not as the abs-value opener a
        // condition could otherwise begin with. The `|`-first abs-value
        // comparison is the bounded reading the differential holds against
        // the authority (grammar §5.5; docs/design/syntax.md §6.3, §11).
        let empty = !self.literal_begins() || self.peek() == SyntaxKind::PIPE;
        if empty {
            self.empty_node(SyntaxKind::CONDITION);
        } else {
            self.condition();
        }
        self.finish_node();
        Element::Literal {
            condition: true,
            empty,
        }
    }

    /// Grammar §5.3's `condition`: literals between commas, at least one
    /// (the empty condition is its caller's empty node).
    pub(super) fn condition(&mut self) {
        self.start_node(SyntaxKind::CONDITION);
        loop {
            if self.element(Position::ConditionLiteral) == Element::None {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                if self.peek() == SyntaxKind::COMMA {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                    continue;
                }
                break;
            }
            if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                break;
            }
        }
        self.finish_node();
    }

    /// Grammar §5.3's aggregate, its node opened at `start` (around the
    /// negation run and the left guard already built): the function
    /// keyword where there is one, the braces with position-shaped
    /// elements between `;`, and the right guard.
    fn aggregate(&mut self, start: Checkpoint, position: Position) {
        let kind = if self.peek() == SyntaxKind::L_BRACE {
            SyntaxKind::SET_AGGREGATE
        } else {
            SyntaxKind::FUNCTION_AGGREGATE
        };
        self.start_node_at(start, kind);
        if kind == SyntaxKind::FUNCTION_AGGREGATE {
            self.bump();
        }
        if self.expect(SyntaxKind::L_BRACE) {
            self.aggregate_elements(kind, position);
            if !self.depth_refused() && !self.eat(SyntaxKind::R_BRACE) {
                self.expected_token(SyntaxKind::R_BRACE);
            }
        }
        if !self.depth_refused() {
            self.right_guard();
        }
        self.finish_node();
    }

    /// The elements between the braces, separated by `;`, until the
    /// closing brace, the dot, or end of input.
    fn aggregate_elements(&mut self, kind: SyntaxKind, position: Position) {
        loop {
            match self.peek() {
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                _ => {}
            }
            let read = if kind == SyntaxKind::SET_AGGREGATE {
                self.element(Position::SetElement) != Element::None
            } else {
                self.function_aggregate_element(position)
            };
            if self.depth_refused() {
                return;
            }
            if !read {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                match self.peek() {
                    SyntaxKind::SEMICOLON => {}
                    SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                    _ => {
                        self.wrap_unexpected(
                            expected(&[Expected::Class(SyntaxClass::Literal)]),
                            None,
                        );
                        continue;
                    }
                }
            }
            match self.peek() {
                SyntaxKind::SEMICOLON => self.bump(),
                SyntaxKind::R_BRACE | SyntaxKind::DOT | SyntaxKind::EOF => return,
                _ => self.wrap_unexpected(
                    expected(&[
                        Expected::Token(SyntaxKind::SEMICOLON),
                        Expected::Token(SyntaxKind::R_BRACE),
                    ]),
                    None,
                ),
            }
        }
    }

    /// Grammar §5.3's `fn-element`: in body position terms with an
    /// optional condition, or a bare colon with one; in head position
    /// terms, the colon, the literal, and an optional condition. Returns
    /// whether an element began.
    fn function_aggregate_element(&mut self, position: Position) -> bool {
        let head = position == Position::Head;
        if !(self.term_begins() || self.peek() == SyntaxKind::COLON) {
            return false;
        }
        let kind = if head {
            SyntaxKind::HEAD_AGGREGATE_ELEMENT
        } else {
            SyntaxKind::BODY_AGGREGATE_ELEMENT
        };
        self.start_node(kind);
        if self.peek() != SyntaxKind::COLON {
            loop {
                if !self.term(TermContext::Term) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
                    break;
                }
                if self.depth_refused() || !self.eat(SyntaxKind::COMMA) {
                    break;
                }
            }
        }
        if !self.depth_refused() {
            if head {
                if self.expect(SyntaxKind::COLON)
                    && self.element(Position::ConditionLiteral) == Element::None
                {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Literal)]), None);
                }
                if !self.depth_refused() && self.eat(SyntaxKind::COLON) {
                    self.condition_or_empty();
                }
            } else if self.eat(SyntaxKind::COLON) {
                self.condition_or_empty();
            }
        }
        self.finish_node();
        true
    }

    /// After a colon: the condition, or the empty condition placed
    /// immediately after the colon.
    pub(super) fn condition_or_empty(&mut self) {
        if self.literal_begins() {
            self.condition();
        } else {
            self.empty_node(SyntaxKind::CONDITION);
        }
    }

    /// Grammar §5.3's `rguard`: a relation and a term, or a bare term.
    fn right_guard(&mut self) {
        if relation(self.peek()) {
            let checkpoint = self.checkpoint();
            self.bump();
            if !self.term(TermContext::Term) {
                self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), None);
            }
            self.start_node_at(checkpoint, SyntaxKind::GUARD);
            self.finish_node();
        } else if self.term_begins() {
            let checkpoint = self.checkpoint();
            self.term(TermContext::Term);
            self.start_node_at(checkpoint, SyntaxKind::GUARD);
            self.finish_node();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Expected, Hint, MisplacedDoc, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::parse::test_util::{admitted, kinds, member, shape};
    use crate::parse::{NestingLimit, parse};
    use crate::tree::{SyntaxKind, sexpr};

    #[test]
    fn a_disallowed_negated_aggregate_is_skipped_whole_with_its_brackets_balanced() {
        // `not #count{…}` in a set element is disallowed (an aggregate is not a
        // literal), so it is carried into one `ERROR` node with its `{…}`
        // balanced: the inner `;` does not end the skip (it stands at depth
        // one), and the following `}` and a depth-zero `;` do. This pins every
        // step of the balanced walk — the `at_end` guard, the open/close arms,
        // the depth counter, and the depth-zero terminator.
        assert_eq!(
            shape(":- 1 { not #count{a; b} } 2."),
            "(RULE :- (BODY (SET_AGGREGATE (GUARD (CONSTANT_TERM 1)) { (ERROR not #count { a ; b }) } (GUARD (CONSTANT_TERM 2)))) .)"
        );
        assert_eq!(
            shape(":- 1 { not #count{a(1,2); c} ; e } 2."),
            "(RULE :- (BODY (SET_AGGREGATE (GUARD (CONSTANT_TERM 1)) { (ERROR not #count { a ( 1 , 2 ) ; c }) ; (LITERAL (ATOM e)) } (GUARD (CONSTANT_TERM 2)))) .)"
        );
    }

    #[test]
    fn a_body_aggregate_takes_semicolon_separated_elements_up_to_its_brace() {
        // The element loop separates on `;` and stops at `}` — the two
        // terminator arms. A two-element body aggregate with a guard is a
        // member, and its shape names both elements.
        assert!(member(":- #count { X : p(X) ; Y : q } >= 1."));
        assert_eq!(
            shape(":- #count { X : p(X) ; Y : q } >= 1."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #count { (BODY_AGGREGATE_ELEMENT (VARIABLE_TERM X) : (CONDITION (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))))) ; (BODY_AGGREGATE_ELEMENT (VARIABLE_TERM Y) : (CONDITION (LITERAL (ATOM q)))) } (GUARD >= (CONSTANT_TERM 1)))) .)"
        );
    }

    #[test]
    fn a_missing_aggregate_element_before_a_separator_or_brace_is_diagnosed_in_place() {
        // The `!read` recovery's two terminator arms: a `;` where an element
        // was promised is diagnosed in place, kept as the separator (not
        // wrapped as unexpected input); and a `}` after a trailing `;` ends the
        // element loop rather than being wrapped and looped on. The first is a
        // non-member with the `;` still a bare separator; the second is a
        // member.
        assert_eq!(
            shape(":- #count { ; } >= 1."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #count { ; } (GUARD >= (CONSTANT_TERM 1)))) .)"
        );
        assert_eq!(kinds(":- #count { ; } >= 1.").len(), 1);
        assert!(member(":- #count { x ; } >= 1."));
    }

    #[test]
    fn a_body_of_only_stray_input_is_one_error_node() {
        // A body that begins no element and is not a statement boundary is
        // carried into one ERROR node — the `body` recovery's `statement_begins`
        // guard deciding to stop rather than to skip.
        assert_eq!(shape(":- $ ."), "(RULE :- (BODY (ERROR $)) .)");
    }

    #[test]
    fn facts_and_rules_take_the_five_forms() {
        assert_eq!(shape("p."), "(RULE (LITERAL (ATOM p)) .)");
        assert_eq!(shape("h :- ."), "(RULE (LITERAL (ATOM h)) :- (BODY) .)");
        assert_eq!(shape(":- ."), "(RULE :- (BODY) .)");
        assert_eq!(shape(":- q."), "(RULE :- (BODY (LITERAL (ATOM q))) .)");
        assert_eq!(
            shape("p :- q, not r; not not s."),
            "(RULE (LITERAL (ATOM p)) :- (BODY (LITERAL (ATOM q)) , (LITERAL not (ATOM r)) ; (LITERAL not not (ATOM s))) .)"
        );
        assert!(member("p :- q, not r; not not s."));
    }

    #[test]
    fn atoms_carry_their_sign_name_and_arguments() {
        assert_eq!(
            shape("-p(X, f(Y))."),
            "(RULE (LITERAL (ATOM - p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X) , (FUNCTION_TERM f (ARGUMENTS ( (TUPLE (VARIABLE_TERM Y)) )))) )))) .)"
        );
        assert_eq!(
            shape("p(a;b)."),
            "(RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE (CONSTANT_TERM b)) )))) .)"
        );
        assert_eq!(shape("#true."), "(RULE (LITERAL #true) .)");
        assert_eq!(shape("not #false."), "(RULE (LITERAL not #false) .)");
    }

    #[test]
    fn comparisons_chain_and_a_bare_term_is_no_literal() {
        assert_eq!(
            shape("1 < X < 5."),
            "(RULE (LITERAL (COMPARISON (CONSTANT_TERM 1) < (VARIABLE_TERM X) < (CONSTANT_TERM 5))) .)"
        );
        assert_eq!(
            shape("-p < 3."),
            "(RULE (LITERAL (COMPARISON (UNARY_TERM - (CONSTANT_TERM p)) < (CONSTANT_TERM 3))) .)"
        );
        assert_eq!(
            shape("p(X) + 1 = 3."),
            "(RULE (LITERAL (COMPARISON (BINARY_TERM (FUNCTION_TERM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))) + (CONSTANT_TERM 1)) = (CONSTANT_TERM 3))) .)"
        );
        assert_eq!(
            shape("X = 1."),
            "(RULE (LITERAL (COMPARISON (VARIABLE_TERM X) = (CONSTANT_TERM 1))) .)"
        );
        assert!(!member("1."));
        assert!(!member("X."));
        assert!(member("p(X) + 1 = 3."));
    }

    #[test]
    fn disjunctions_take_the_three_separators_and_the_singleton_conditioned_head() {
        assert_eq!(
            shape("a ; b | c, d."),
            "(RULE (DISJUNCTION (LITERAL (ATOM a)) ; (LITERAL (ATOM b)) | (LITERAL (ATOM c)) , (LITERAL (ATOM d))) .)"
        );
        assert_eq!(
            shape("p(X) : q(X)."),
            "(RULE (DISJUNCTION (CONDITIONAL_LITERAL (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))))) .)"
        );
        assert_eq!(
            shape("a : ."),
            "(RULE (DISJUNCTION (CONDITIONAL_LITERAL (LITERAL (ATOM a)) : (CONDITION))) .)"
        );
        assert!(member("a : b, c; d : ."));
        assert!(!member("p(X) : | q(X)."));
        assert!(kinds("p(X) : | q(X).").iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken {
                hint: Some(Hint::EmptyConditionBeforePipe),
                ..
            }
        )));
    }

    #[test]
    fn conditional_literals_absorb_commas_and_end_at_the_semicolon_or_the_dot() {
        assert_eq!(
            shape(":- p : q, r, s."),
            "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION (LITERAL (ATOM q)) , (LITERAL (ATOM r)) , (LITERAL (ATOM s))))) .)"
        );
        assert_eq!(
            shape(":- p : ."),
            "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION))) .)"
        );
        assert_eq!(
            shape(":- p : q; t."),
            "(RULE :- (BODY (CONDITIONAL_LITERAL (LITERAL (ATOM p)) : (CONDITION (LITERAL (ATOM q)))) ; (LITERAL (ATOM t))) .)"
        );
        assert!(member(":- p : ."));
    }

    #[test]
    fn aggregates_take_guards_functions_and_position_shaped_elements() {
        assert_eq!(
            shape(":- #sum { W,T : task(T), weight(T,W) } >= 4."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT (VARIABLE_TERM W) , (VARIABLE_TERM T) : (CONDITION (LITERAL (ATOM task (ARGUMENTS ( (TUPLE (VARIABLE_TERM T)) )))) , (LITERAL (ATOM weight (ARGUMENTS ( (TUPLE (VARIABLE_TERM T) , (VARIABLE_TERM W)) )))))) } (GUARD >= (CONSTANT_TERM 4)))) .)"
        );
        assert_eq!(
            shape("1 { p(X) : q(X) } 1 :- r."),
            "(RULE (SET_AGGREGATE (GUARD (CONSTANT_TERM 1)) { (CONDITIONAL_LITERAL (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))))) } (GUARD (CONSTANT_TERM 1))) :- (BODY (LITERAL (ATOM r))) .)"
        );
        assert_eq!(
            shape(":- X = #count { a; b }."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE (GUARD (VARIABLE_TERM X) =) #count { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a)) ; (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM b)) })) .)"
        );
        assert_eq!(
            shape(":- not #count { } 1."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE not #count { } (GUARD (CONSTANT_TERM 1)))) .)"
        );
        assert_eq!(
            shape("#min { X : p(X) : q(X) }."),
            "(RULE (FUNCTION_AGGREGATE #min { (HEAD_AGGREGATE_ELEMENT (VARIABLE_TERM X) : (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) : (CONDITION (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))))) }) .)"
        );
        assert_eq!(
            shape(":- #sum { : }."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT : (CONDITION)) })) .)"
        );
        assert_eq!(
            shape(":- #sum { a : }."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #sum { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a) : (CONDITION)) })) .)"
        );
        assert!(member(":- #sum { : }."));
        assert!(
            !member("#sum { : }."),
            "a head element requires its literal"
        );
        assert!(member("{ p; q : r } :- s."));
        assert!(member(
            ":- 1 <= #count { X : p(X) } < 3, not #sum+ { 1 : q } = 2."
        ));
    }

    #[test]
    fn missing_separators_and_dots_are_diagnosed_and_the_parse_continues() {
        assert_eq!(
            shape("p(X) :- q(X) r(X)."),
            "(RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) :- (BODY (LITERAL (ATOM q (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) )))) (LITERAL (ATOM r (ARGUMENTS ( (TUPLE (VARIABLE_TERM X)) ))))) .)"
        );
        assert_eq!(kinds("p(X) :- q(X) r(X).").len(), 1);
        assert_eq!(
            shape("p q."),
            "(RULE (LITERAL (ATOM p))) (RULE (LITERAL (ATOM q)) .)"
        );
        assert_eq!(kinds("p q.").len(), 1);
        assert_eq!(
            shape(":- p, , q."),
            "(RULE :- (BODY (LITERAL (ATOM p)) , (ERROR ,) (LITERAL (ATOM q))) .)"
        );
        assert_eq!(
            shape(":- #count { a ."),
            "(RULE :- (BODY (FUNCTION_AGGREGATE #count { (BODY_AGGREGATE_ELEMENT (CONSTANT_TERM a)))) .)"
        );
        assert!(
            kinds(":- #count { a .")
                .iter()
                .any(|kind| matches!(kind, SyntaxErrorKind::UnexpectedToken { .. }))
        );
    }

    #[test]
    fn documentation_belongs_to_the_statement_and_a_doc_line_inside_one_is_trivia() {
        assert_eq!(shape("%! doc\np."), "(RULE %! doc (LITERAL (ATOM p)) .)");
        assert!(kinds("%! doc\np.").is_empty());
        assert_eq!(
            shape("%! a\n\n%! b\np."),
            "(RULE %! a %! b (LITERAL (ATOM p)) .)"
        );
        assert!(kinds("%! a\n\n%! b\np.").is_empty());
        let inside = kinds("p :- %! x\n q.");
        assert_eq!(inside.len(), 1);
        assert!(matches!(
            inside[0],
            SyntaxErrorKind::MisplacedDocComment {
                reason: MisplacedDoc::InsideStatement
            }
        ));
        assert!(member("p :- %! x\n q."));
    }

    #[test]
    fn the_query_is_read_by_dialect_at_the_programs_end() {
        let query = |text: &str, dialect: Dialect| {
            let source = admitted(text);
            let parse = parse(&source, dialect);
            (
                sexpr(&parse.syntax()),
                parse
                    .diagnostics()
                    .iter()
                    .map(|d| d.kind().clone())
                    .collect::<Vec<_>>(),
            )
        };
        let (shape, kinds) = query("p(1)?", Dialect::AspCore2);
        assert_eq!(
            shape,
            "(PROGRAM (QUERY (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) ?))"
        );
        assert!(kinds.is_empty());
        let (shape, kinds) = query("%! q\np(1)?", Dialect::AspCore2);
        assert_eq!(
            shape,
            "(PROGRAM (QUERY %! q (ATOM p (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) ?))"
        );
        assert!(kinds.is_empty());
        let (_, kinds) = query("p(1)?", Dialect::Clingo);
        assert!(kinds.iter().any(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedEndOfInput {
                hint: Some(Hint::QueryMarkNeedsAspCore2),
                ..
            }
        )));
        for dialect in [Dialect::Clingo, Dialect::AspCore2] {
            assert!(
                !query("p ? q.", dialect).1.is_empty(),
                "the same error under both dialects"
            );
            assert!(query("p ? q = X.", dialect).1.is_empty());
            assert!(query("p(1)?2 > 3.", dialect).1.is_empty());
            assert!(query("x(1?2).", dialect).1.is_empty());
        }
        assert_eq!(
            query("p ? q = X.", Dialect::AspCore2).0,
            query("p ? q = X.", Dialect::Clingo).0
        );
    }

    #[test]
    fn a_program_of_several_statements_places_trivia_between_them_at_the_program() {
        let source = admitted("p. % c\n\nq :- p.\n");
        let parse = parse(&source, Dialect::Clingo);
        assert!(!parse.has_errors());
        let statements: Vec<String> = parse
            .syntax()
            .children()
            .map(|node| node.text().to_string())
            .collect();
        assert_eq!(statements, ["p.", "q :- p."]);
    }

    #[test]
    fn an_aggregate_or_theory_atom_head_is_standalone_not_a_disjunction_element() {
        // Only a literal disjoins in a head (grammar §5.5), as the
        // authority has it: a separator after an aggregate or theory-atom
        // head is a rule-level error, and the head names both ways to
        // complete it — `.` or `:-` — so a reader or a tool can act.
        assert!(!member("&a { x } ; p."));
        assert!(!member("&a { x } , p."));
        assert!(!member("1 { p } 1 ; q."));
        assert!(!shape("&a { x } ; p.").contains("DISJUNCTION"));
        assert!(kinds("&a { x } ; p.").iter().any(|k| matches!(
            k,
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Token(SyntaxKind::DOT))
                    && expected.contains(&Expected::Token(SyntaxKind::NECK))
        )));
    }

    #[test]
    fn a_negation_run_with_no_atom_is_an_error_not_a_childless_literal() {
        // A `LITERAL` holds an atom, `#true`, or `#false` (tree.rs); a
        // `not` with none after it — a bare `not`, or a third past the
        // double-negation cap — is carried in an `ERROR` instead.
        assert_eq!(shape(":- not."), "(RULE :- (BODY (ERROR not)) .)");
        assert!(!member(":- not not not p."));
        assert!(!shape(":- not not not p.").contains("(LITERAL not not)"));
    }

    #[test]
    fn a_disallowed_aggregate_or_theory_atom_is_carried_whole_without_unbounded_recursion() {
        // A leading `not` reaches the aggregate/theory-atom arm even where
        // the position forbids one; the parser carries the construct in one
        // error node rather than descending, so nesting is bounded by the
        // grammar's constant path, not the input (docs/design/syntax.md
        // §6.6). A tiny thread stack proves the recovery is iterative.
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let aggregates = format!(":- {}", "not { ".repeat(30_000));
                assert!(!member(&aggregates));
                let theory_atoms = format!(
                    ":- &a{{ : {}x{} }}.",
                    "not &b{ : ".repeat(15_000),
                    " }".repeat(15_000)
                );
                assert!(!member(&theory_atoms));
            })
            .expect("thread spawns")
            .join()
            .expect("no stack overflow");
    }

    #[test]
    fn a_theory_mode_depth_refusal_ends_at_the_dot_not_a_glued_operator() {
        // The refusal skips to the terminating dot; inside a theory region
        // the mode is `Theory`, where `=.` is one operator run and the dot
        // is missed, swallowing the next statement — the skip reads in
        // normal mode (docs/design/syntax.md §4.2, §6.6). `parse` refuses
        // at `NestingLimit::DEFAULT`, shallow enough to hold on a modest stack.
        {
            let refused = format!(
                "&a {{ {}x=.q.",
                "(".repeat(NestingLimit::DEFAULT.frames() as usize + 1)
            );
            let parse = parse(&admitted(&refused), Dialect::Clingo);
            assert_eq!(parse.syntax().text(), refused.as_str(), "law 1");
            // Two statements: the refused theory atom, then `q.` — not one
            // that swallowed `q` past a glued dot.
            assert_eq!(parse.syntax().children().count(), 2);
        }
    }
}
