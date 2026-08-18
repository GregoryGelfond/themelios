//! The frame loop (docs/design/syntax.md §6.2, §6.6): the self-recursive
//! term families on one explicit frame stack — a frame per open bracket
//! context, operator structure flat per precedence level, input depth
//! as frame count and never call depth, the depth refusal at the
//! constant, and the restriction contexts read at form emission only.
//! The theory family joins this loop in its own file's arms.
//!
//! The invariant (docs/design/syntax.md §6.2), held at every step: the
//! frame stack mirrors the open bracket contexts of the text, innermost
//! on top; a frame's level stack holds its open precedence levels,
//! strictly tighter from bottom to top, each with the checkpoint taken
//! before its first operand; every operand parsed so far in the frame
//! lies inside the topmost open level or inside a level already closed
//! beneath it; the last operand's checkpoint is kept, so a level can open
//! around it retroactively.

use rowan::Checkpoint;

use crate::diagnostic::{
    Expected, ExpectedSet, Hint, Related, RelatedLocus, RestrictedForm, Restriction, SyntaxClass,
    SyntaxError, SyntaxErrorKind,
};
use crate::dialect::Dialect;
use crate::token::TokenSource;
use crate::tree::SyntaxKind;

use super::MAX_NESTING_DEPTH;
use super::machine::Parser;

/// The restriction the loop emits forms under (docs/design/syntax.md
/// §6.2): the general term, `#const`'s constant term (grammar §5.9), or
/// the term-value sublanguage (grammar §5.10). Read at one point — form
/// emission — and never steering the parse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TermContext {
    /// Grammar §5.1's `term`: every form.
    Term,
    /// No variable, no anonymous variable, no pool, no interval, no
    /// pooled absolute value.
    // Emitted by the `#const` statement (grammar §5.9), which lands with
    // the directive family; unconstructed until then.
    #[allow(dead_code)]
    ConstantTerm,
    /// The constant term's exclusions and the `@`-call besides.
    TermValue,
}

impl TermContext {
    fn restriction(self) -> Option<Restriction> {
        match self {
            TermContext::Term => None,
            TermContext::ConstantTerm => Some(Restriction::ConstantTerm),
            TermContext::TermValue => Some(Restriction::TermValue),
        }
    }
}

/// Grammar §5.1's precedence levels, loosest first: a greater level
/// binds tighter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Level {
    Interval,
    BitXor,
    BitOr,
    BitAnd,
    Additive,
    Multiplicative,
    Exponentiation,
}

fn binary_level(kind: SyntaxKind) -> Option<Level> {
    Some(match kind {
        SyntaxKind::DOTDOT => Level::Interval,
        SyntaxKind::CARET => Level::BitXor,
        SyntaxKind::QUESTION => Level::BitOr,
        SyntaxKind::AMPERSAND => Level::BitAnd,
        SyntaxKind::PLUS | SyntaxKind::MINUS => Level::Additive,
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::BACKSLASH => Level::Multiplicative,
        SyntaxKind::STAR_STAR => Level::Exponentiation,
        _ => return None,
    })
}

/// What opened a frame, and so what stands inside it and what closes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    /// The frame-free top of a term: no bracket, no closer; the term
    /// ends at the first token that is not an operator.
    Base,
    /// `( … )` — a pool of tuples (grammar §5.1).
    Pool,
    /// `( … )` after a function's or an `@`-call's name: pooled argument
    /// alternatives, no trailing comma (grammar §5.1).
    Arguments,
    /// `| … |` — the absolute value over a pooled argument.
    Abs,
}

/// One open bracket context.
struct Frame {
    shape: Shape,
    /// The open precedence levels, tighter on top, each with the
    /// checkpoint before its first operand.
    levels: Vec<(Level, Checkpoint)>,
    /// The checkpoint before the last operand — where a level opens
    /// retroactively.
    operand: Option<Checkpoint>,
    /// A `UNARY_TERM` is open, awaiting the one operand that closes it.
    unary_open: bool,
    /// A `TUPLE` is open in this pool or argument list.
    tuple_open: bool,
    /// Terms begun in the open tuple.
    tuple_terms: u32,
    /// The last token consumed in this frame was a `,`.
    after_comma: bool,
    /// A `FUNCTION_TERM` or `EXTERNAL_TERM` node is open around this
    /// argument list, and closes with it.
    wrapper: Option<SyntaxKind>,
    /// The opener's kind and span, for a missing closer's related locus.
    opener: (SyntaxKind, u32, u32),
}

impl Frame {
    fn new(shape: Shape, wrapper: Option<SyntaxKind>, opener: (SyntaxKind, u32, u32)) -> Frame {
        Frame {
            shape,
            levels: Vec::new(),
            operand: None,
            unary_open: false,
            tuple_open: false,
            tuple_terms: 0,
            after_comma: false,
            wrapper,
            opener,
        }
    }

    fn node(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool => Some(SyntaxKind::POOL),
            Shape::Arguments => Some(SyntaxKind::ARGUMENTS),
            Shape::Abs => Some(SyntaxKind::ABS_TERM),
        }
    }

    fn closer(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool | Shape::Arguments => Some(SyntaxKind::R_PAREN),
            Shape::Abs => Some(SyntaxKind::PIPE),
        }
    }
}

/// What the loop does next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Expect an operand.
    Operand,
    /// An operand is complete: expect an operator, a separator, or a closer.
    Operator,
    /// The term is complete, or refused: the loop ends.
    Done,
}

/// The tokens that end a term or a list where an operand or a closer was
/// expected — the loop's synchronization set (docs/design/syntax.md §6.7):
/// nothing is consumed at them; a token outside this set is an intruder,
/// wrapped, and the frame continues.
fn synchronizes(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EOF
            | SyntaxKind::DOT
            | SyntaxKind::COMMA
            | SyntaxKind::SEMICOLON
            | SyntaxKind::R_PAREN
            | SyntaxKind::R_BRACKET
            | SyntaxKind::R_BRACE
            | SyntaxKind::PIPE
            | SyntaxKind::COLON
            | SyntaxKind::NECK
            | SyntaxKind::WEAK_NECK
    )
}

fn expected(items: &[Expected]) -> ExpectedSet {
    items.iter().copied().collect()
}

/// The base frame's opener: it has none.
const NO_OPENER: (SyntaxKind, u32, u32) = (SyntaxKind::EOF, 0, 0);

impl<S: TokenSource> Parser<'_, S> {
    /// Whether the next significant token begins a term.
    pub(super) fn term_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::VARIABLE
                | SyntaxKind::ANONYMOUS
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::MINUS
                | SyntaxKind::TILDE
                | SyntaxKind::L_PAREN
                | SyntaxKind::PIPE
                | SyntaxKind::AT
                | SyntaxKind::SPLICE
        )
    }

    /// One term of the family `context` at the next significant token,
    /// with everything it nests, on the frame stack: false, and nothing
    /// consumed, when no term begins there. After a depth refusal the
    /// rest of the statement has been consumed and `depth_refused()`
    /// holds.
    pub(super) fn term(&mut self, context: TermContext) -> bool {
        if !self.term_begins() {
            return false;
        }
        self.run(
            context,
            vec![Frame::new(Shape::Base, None, NO_OPENER)],
            Next::Operand,
            false,
        );
        true
    }

    /// The loop resumed after an operand already built at the base — the
    /// atom-shaped prefix the literal parser wraps into a term when an
    /// operator follows it (Task 9): `checkpoint` is that operand's, and
    /// the next token is read as what follows an operand.
    // Consumed by the literal and comparison families (§6.3); unused until
    // they land.
    #[allow(dead_code)]
    pub(super) fn term_continue(&mut self, context: TermContext, checkpoint: Checkpoint) {
        let mut base = Frame::new(Shape::Base, None, NO_OPENER);
        base.operand = Some(checkpoint);
        self.run(context, vec![base], Next::Operator, false);
    }

    /// An argument list at the next significant token, `(`, on its own
    /// frame — an atom's or a theory atom's arguments, whose enclosing
    /// node is not a term (Task 9, Task 10). Returns when the frame
    /// closes; nothing is read at the base after it.
    // Consumed by the atom and theory-atom families (§6.3); unused until
    // they land.
    #[allow(dead_code)]
    pub(super) fn arguments(&mut self, context: TermContext) {
        let mut frames = vec![Frame::new(Shape::Base, None, NO_OPENER)];
        let next = self.open_frame(&mut frames, Shape::Arguments, None, None);
        if next != Next::Done {
            self.run(context, frames, next, true);
        }
    }

    /// The loop itself: one step per iteration, until the term is done —
    /// or, when `stop_at_base` holds, until the frame the caller opened
    /// has closed and only the base remains.
    fn run(
        &mut self,
        context: TermContext,
        mut frames: Vec<Frame>,
        mut next: Next,
        stop_at_base: bool,
    ) {
        let mut last_operator = None;
        while next != Next::Done {
            next = match next {
                Next::Operand => self.operand(&mut frames, context, last_operator),
                Next::Operator => self.after_operand(&mut frames, context, &mut last_operator),
                Next::Done => Next::Done,
            };
            if stop_at_base && frames.len() == 1 {
                return;
            }
        }
    }

    /// The checkpoint the operand about to begin opens its own node at,
    /// with the trivia before it placed into the open node. It is also
    /// the checkpoint a level opens at retroactively — unless a unary run
    /// is open, whose start already is that checkpoint, the run and its
    /// operand being one operand for the level.
    fn begin_operand(&mut self, frame: &mut Frame) -> Checkpoint {
        frame.after_comma = false;
        if frame.tuple_open {
            frame.tuple_terms += 1;
        }
        let checkpoint = self.checkpoint();
        if !frame.unary_open {
            frame.operand = Some(checkpoint);
        }
        checkpoint
    }

    /// The operand just completed in `frame`: a unary run awaiting it
    /// closes.
    fn complete_operand(&mut self, frame: &mut Frame) {
        if frame.unary_open {
            self.finish_node();
            frame.unary_open = false;
        }
    }

    /// A restricted form at the next significant token: the diagnostic
    /// naming the form and the context, the structure still built.
    fn restricted(&mut self, form: RestrictedForm, context: TermContext) {
        let Some(context) = context.restriction() else {
            return;
        };
        let start = self.peek_start();
        let end = start + self.peek_len();
        let location = self.location(start, end);
        self.diagnose(SyntaxError::new(
            SyntaxErrorKind::FormNotAllowedHere { form, context },
            location,
        ));
    }

    /// Opens a bracket frame at the next significant token — the opener
    /// — unless it would be the frame past the constant, which is
    /// refused (docs/design/syntax.md §6.6). `retroactive` is the
    /// checkpoint a pool or absolute value opens its node at; an
    /// argument list opens its node here, inside its wrapper. After the
    /// opener a pool or argument list places its first tuple.
    fn open_frame(
        &mut self,
        frames: &mut Vec<Frame>,
        shape: Shape,
        retroactive: Option<Checkpoint>,
        wrapper: Option<SyntaxKind>,
    ) -> Next {
        let nesting = u32::try_from(frames.len() - 1).unwrap_or(u32::MAX);
        if nesting >= MAX_NESTING_DEPTH {
            self.refuse_depth();
            if wrapper.is_some() {
                // The wrapper opened before its frame could: it closes over
                // the refusal like every node above it.
                self.finish_node();
            }
            self.unwind(frames);
            return Next::Done;
        }
        let opener = (
            self.peek(),
            self.peek_start(),
            self.peek_start() + self.peek_len(),
        );
        let node = match shape {
            Shape::Pool => SyntaxKind::POOL,
            Shape::Abs => SyntaxKind::ABS_TERM,
            Shape::Arguments => SyntaxKind::ARGUMENTS,
            Shape::Base => unreachable!("the base frame has no opener"),
        };
        match retroactive {
            Some(checkpoint) => self.start_node_at(checkpoint, node),
            None => self.start_node(node),
        }
        self.bump();
        let mut frame = Frame::new(shape, wrapper, opener);
        let next = match shape {
            Shape::Pool | Shape::Arguments => self.tuple_start(&mut frame),
            Shape::Abs | Shape::Base => Next::Operand,
        };
        frames.push(frame);
        next
    }

    /// The tuple after an opener or a pooling `;`: empty — placed
    /// immediately after that token, holding no trivia — when the closer
    /// or the next `;` follows; open otherwise (docs/design/syntax.md
    /// §5.4).
    fn tuple_start(&mut self, frame: &mut Frame) -> Next {
        frame.tuple_terms = 0;
        frame.after_comma = false;
        if matches!(self.peek(), SyntaxKind::R_PAREN | SyntaxKind::SEMICOLON) {
            self.empty_node(SyntaxKind::TUPLE);
            frame.tuple_open = false;
            Next::Operator
        } else {
            self.start_node(SyntaxKind::TUPLE);
            frame.tuple_open = true;
            Next::Operand
        }
    }

    /// Closes the open levels of `frame` tighter than `level`, innermost
    /// first, each wrapped from its checkpoint into its `BINARY_TERM`;
    /// the last closed level's checkpoint becomes the last operand's.
    fn close_levels_tighter_than(&mut self, frame: &mut Frame, level: Option<Level>) {
        while let Some((top, checkpoint)) = frame.levels.last().copied() {
            if level.is_some_and(|level| top <= level) {
                return;
            }
            frame.levels.pop();
            self.finish_node();
            frame.operand = Some(checkpoint);
        }
    }

    /// Closes the top frame's levels, tuple, node, and wrapper, and pops
    /// it; the enclosing frame's operand is then complete.
    fn close_frame(&mut self, frames: &mut Vec<Frame>) -> Next {
        let mut frame = frames.pop().expect("a bracket frame is open");
        self.close_levels_tighter_than(&mut frame, None);
        if frame.tuple_open {
            self.finish_node();
        }
        if frame.node().is_some() {
            self.finish_node();
        }
        if frame.wrapper.is_some() {
            self.finish_node();
        }
        let enclosing = frames.last_mut().expect("the base frame stays");
        self.complete_operand(enclosing);
        Next::Operator
    }

    /// Closes every frame without a diagnostic, after a refusal — the
    /// `ERROR` node stands under the innermost frame, and every open
    /// node above it closes over it (docs/design/syntax.md §6.6).
    fn unwind(&mut self, frames: &mut Vec<Frame>) {
        while let Some(mut frame) = frames.pop() {
            if frame.unary_open {
                self.finish_node();
            }
            self.close_levels_tighter_than(&mut frame, None);
            if frame.tuple_open {
                self.finish_node();
            }
            if frame.node().is_some() {
                self.finish_node();
            }
            if frame.wrapper.is_some() {
                self.finish_node();
            }
        }
    }

    /// The missing closer of the top frame: diagnosed at the token found,
    /// naming the opener, and the frame closed over what it holds.
    fn unclosed(&mut self, frames: &mut Vec<Frame>) -> Next {
        let frame = frames.last().expect("a bracket frame is open");
        let closer = frame.closer().expect("a bracket frame has a closer");
        let (opener_kind, start, end) = frame.opener;
        let related = Related {
            locus: RelatedLocus::ToClose(opener_kind),
            location: self.location(start, end),
        };
        self.unexpected_related(expected(&[Expected::Token(closer)]), None, Some(related));
        self.close_frame(frames)
    }

    /// Expecting an operand: a prefix run, a bracket, a name, a constant,
    /// a variable, a splice — or nothing that begins a term, which is a
    /// missing operand at a synchronizing token and an intruder anywhere
    /// else.
    fn operand(
        &mut self,
        frames: &mut Vec<Frame>,
        context: TermContext,
        last_operator: Option<SyntaxKind>,
    ) -> Next {
        let top = frames.len() - 1;
        match self.peek() {
            SyntaxKind::MINUS | SyntaxKind::TILDE => {
                let frame = &mut frames[top];
                if !frame.unary_open {
                    // The run's start is the level's operand checkpoint; the
                    // operand inside the run opens its own node at its own.
                    let checkpoint = self.begin_operand(frame);
                    self.start_node_at(checkpoint, SyntaxKind::UNARY_TERM);
                    frame.unary_open = true;
                }
                self.bump();
                Next::Operand
            }
            SyntaxKind::L_PAREN => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.open_frame(frames, Shape::Pool, Some(checkpoint), None)
            }
            SyntaxKind::PIPE => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.open_frame(frames, Shape::Abs, Some(checkpoint), None)
            }
            SyntaxKind::AT => {
                self.restricted(RestrictedForm::ExternalCall, context);
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.start_node_at(checkpoint, SyntaxKind::EXTERNAL_TERM);
                self.bump();
                self.expect(SyntaxKind::IDENT);
                if self.peek() == SyntaxKind::L_PAREN {
                    self.open_frame(
                        frames,
                        Shape::Arguments,
                        None,
                        Some(SyntaxKind::EXTERNAL_TERM),
                    )
                } else {
                    self.finish_node();
                    self.complete_operand(&mut frames[top]);
                    Next::Operator
                }
            }
            SyntaxKind::IDENT if self.lookahead(1) == SyntaxKind::L_PAREN => {
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.start_node_at(checkpoint, SyntaxKind::FUNCTION_TERM);
                self.bump();
                self.open_frame(
                    frames,
                    Shape::Arguments,
                    None,
                    Some(SyntaxKind::FUNCTION_TERM),
                )
            }
            SyntaxKind::IDENT
            | SyntaxKind::NUMBER
            | SyntaxKind::STRING
            | SyntaxKind::KW_INF
            | SyntaxKind::KW_SUP => self.leaf(&mut frames[top], SyntaxKind::CONSTANT_TERM),
            SyntaxKind::VARIABLE => {
                self.restricted(RestrictedForm::Variable, context);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::ANONYMOUS => {
                self.restricted(RestrictedForm::AnonymousVariable, context);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::SPLICE => self.leaf(&mut frames[top], SyntaxKind::SPLICE_TERM),
            SyntaxKind::COMMA
                if frames[top].shape == Shape::Pool && frames[top].tuple_terms == 0 =>
            {
                // `(,)`: the tuple's trailing comma with no terms before it.
                self.bump();
                frames[top].after_comma = true;
                Next::Operator
            }
            kind => {
                let frame = &frames[top];
                let hint = if kind == SyntaxKind::EOF
                    && last_operator == Some(SyntaxKind::QUESTION)
                    && self.dialect() == Dialect::Clingo
                {
                    Some(Hint::QueryMarkNeedsAspCore2)
                } else if kind == SyntaxKind::R_PAREN
                    && frame.shape == Shape::Arguments
                    && frame.after_comma
                {
                    Some(Hint::TrailingCommaInArguments)
                } else {
                    None
                };
                if synchronizes(kind) {
                    // A dangling prefix run whose operand never came closes
                    // here, as it does on the refusal path (unwind), so the
                    // frame's closer or separator acts on the frame and not
                    // on the still-open run (docs/design/syntax.md §6.2).
                    self.complete_operand(&mut frames[top]);
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), hint);
                    Next::Operator
                } else {
                    self.wrap_unexpected(expected(&[Expected::Class(SyntaxClass::Term)]), hint);
                    Next::Operand
                }
            }
        }
    }

    /// A one-token operand: its node around the token.
    fn leaf(&mut self, frame: &mut Frame, kind: SyntaxKind) -> Next {
        let checkpoint = self.begin_operand(frame);
        self.start_node_at(checkpoint, kind);
        self.bump();
        self.finish_node();
        self.complete_operand(frame);
        Next::Operator
    }

    /// After an operand: a binary operator joins or opens a level; a
    /// separator or a closer acts on the frame; at the base the term
    /// ends at anything else; inside a frame anything else is an
    /// intruder, wrapped, or a synchronizing token that closes the frame
    /// as unclosed.
    fn after_operand(
        &mut self,
        frames: &mut Vec<Frame>,
        context: TermContext,
        last_operator: &mut Option<SyntaxKind>,
    ) -> Next {
        let top = frames.len() - 1;
        let kind = self.peek();
        let query_mark = kind == SyntaxKind::QUESTION
            && self.dialect() == Dialect::AspCore2
            && self.lookahead(1) == SyntaxKind::EOF;
        if let Some(level) = binary_level(kind).filter(|_| !query_mark) {
            if level == Level::Interval {
                self.restricted(RestrictedForm::Interval, context);
            }
            let frame = &mut frames[top];
            self.close_levels_tighter_than(frame, Some(level));
            if frame.levels.last().map(|(open, _)| *open) != Some(level) {
                let checkpoint = frame.operand.expect("an operand precedes an operator");
                self.start_node_at(checkpoint, SyntaxKind::BINARY_TERM);
                frame.levels.push((level, checkpoint));
            }
            self.bump();
            *last_operator = Some(kind);
            return Next::Operand;
        }
        match frames[top].shape {
            Shape::Base => {
                self.close_levels_tighter_than(&mut frames[top], None);
                Next::Done
            }
            Shape::Pool | Shape::Arguments => match kind {
                SyntaxKind::COMMA => {
                    let frame = &mut frames[top];
                    self.close_levels_tighter_than(frame, None);
                    self.bump();
                    frame.after_comma = true;
                    if frame.shape == Shape::Pool
                        && matches!(self.peek(), SyntaxKind::R_PAREN | SyntaxKind::SEMICOLON)
                    {
                        Next::Operator
                    } else {
                        Next::Operand
                    }
                }
                SyntaxKind::SEMICOLON => {
                    self.restricted(RestrictedForm::Pool, context);
                    let frame = &mut frames[top];
                    self.close_levels_tighter_than(frame, None);
                    if frame.tuple_open {
                        self.finish_node();
                        frame.tuple_open = false;
                    }
                    self.bump();
                    self.tuple_start(frame)
                }
                SyntaxKind::R_PAREN => self.bump_closer_and_close(frames),
                kind if synchronizes(kind) => self.unclosed(frames),
                _ => {
                    self.wrap_unexpected(
                        expected(&[
                            Expected::Token(SyntaxKind::COMMA),
                            Expected::Token(SyntaxKind::SEMICOLON),
                            Expected::Token(SyntaxKind::R_PAREN),
                        ]),
                        None,
                    );
                    Next::Operator
                }
            },
            Shape::Abs => match kind {
                SyntaxKind::PIPE => self.bump_closer_and_close(frames),
                SyntaxKind::SEMICOLON => {
                    self.restricted(RestrictedForm::PooledAbsoluteValue, context);
                    self.close_levels_tighter_than(&mut frames[top], None);
                    self.bump();
                    Next::Operand
                }
                kind if synchronizes(kind) => self.unclosed(frames),
                _ => {
                    self.wrap_unexpected(
                        expected(&[
                            Expected::Token(SyntaxKind::SEMICOLON),
                            Expected::Token(SyntaxKind::PIPE),
                        ]),
                        None,
                    );
                    Next::Operator
                }
            },
        }
    }

    /// The closer of the top frame: the levels close, the tuple closes,
    /// the closer is placed, and the frame closes over it.
    fn bump_closer_and_close(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        self.close_levels_tighter_than(&mut frames[top], None);
        if frames[top].tuple_open {
            self.finish_node();
            frames[top].tuple_open = false;
        }
        self.bump();
        self.close_frame(frames)
    }
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use crate::diagnostic::{Hint, RestrictedForm, Restriction, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::lexer::Lexer;
    use crate::parse::{MAX_NESTING_DEPTH, MAX_TREE_DEPTH, parse_term, parse_term_value};
    use crate::tree::{SyntaxKind, sexpr};

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(0), text.to_owned()).expect("test text admits")
    }

    /// The shape of the term the term entry reads from `text` under the
    /// clingo dialect, with the fragment root peeled.
    fn term(text: &str) -> String {
        let source = admitted(text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(TERM_FRAGMENT ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn diagnostics(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse_term(&Lexer::new(&source, Dialect::Clingo))
            .diagnostics()
            .iter()
            .map(|d| d.kind().clone())
            .collect()
    }

    #[test]
    fn a_chain_is_one_flat_node_per_level() {
        assert_eq!(
            term("1 + 2 - 3"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (CONSTANT_TERM 2) - (CONSTANT_TERM 3))"
        );
        assert_eq!(
            term("1 + 2 * 3"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (BINARY_TERM (CONSTANT_TERM 2) * (CONSTANT_TERM 3)))"
        );
        assert_eq!(
            term("1 * 2 + 3"),
            "(BINARY_TERM (BINARY_TERM (CONSTANT_TERM 1) * (CONSTANT_TERM 2)) + (CONSTANT_TERM 3))"
        );
        assert_eq!(
            term("1 + 2 * 3 + 4"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (BINARY_TERM (CONSTANT_TERM 2) * (CONSTANT_TERM 3)) + (CONSTANT_TERM 4))"
        );
        assert_eq!(
            term("2 ** 3 ** 4"),
            "(BINARY_TERM (CONSTANT_TERM 2) ** (CONSTANT_TERM 3) ** (CONSTANT_TERM 4))"
        );
        assert_eq!(
            term("1..3 ^ 2 ? 4 & 5"),
            "(BINARY_TERM (CONSTANT_TERM 1) .. (BINARY_TERM (CONSTANT_TERM 3) ^ (BINARY_TERM (CONSTANT_TERM 2) ? (BINARY_TERM (CONSTANT_TERM 4) & (CONSTANT_TERM 5)))))"
        );
        assert!(diagnostics("1 + 2 * 3 + 4").is_empty());
    }

    #[test]
    fn unary_runs_are_flat_and_bind_tighter_than_every_binary_level() {
        assert_eq!(term("- - x"), "(UNARY_TERM - - (CONSTANT_TERM x))");
        assert_eq!(
            term("-2**2"),
            "(BINARY_TERM (UNARY_TERM - (CONSTANT_TERM 2)) ** (CONSTANT_TERM 2))"
        );
        assert_eq!(
            term("2 ** -3"),
            "(BINARY_TERM (CONSTANT_TERM 2) ** (UNARY_TERM - (CONSTANT_TERM 3)))"
        );
        assert_eq!(
            term("~X + 1"),
            "(BINARY_TERM (UNARY_TERM ~ (VARIABLE_TERM X)) + (CONSTANT_TERM 1))"
        );
        assert_eq!(
            term("-(1;2)"),
            "(UNARY_TERM - (POOL ( (TUPLE (CONSTANT_TERM 1)) ; (TUPLE (CONSTANT_TERM 2)) )))"
        );
    }

    #[test]
    fn pools_tuples_and_argument_lists_keep_the_grammars_uniform_shape() {
        assert_eq!(term("()"), "(POOL ( (TUPLE) ))");
        assert_eq!(term("(a)"), "(POOL ( (TUPLE (CONSTANT_TERM a)) ))");
        assert_eq!(term("(a,)"), "(POOL ( (TUPLE (CONSTANT_TERM a) ,) ))");
        assert_eq!(term("(,)"), "(POOL ( (TUPLE ,) ))");
        assert_eq!(
            term("(a,b;c,d)"),
            "(POOL ( (TUPLE (CONSTANT_TERM a) , (CONSTANT_TERM b)) ; (TUPLE (CONSTANT_TERM c) , (CONSTANT_TERM d)) ))"
        );
        assert_eq!(term("(;)"), "(POOL ( (TUPLE) ; (TUPLE) ))");
        assert_eq!(term("f()"), "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE) )))");
        assert_eq!(
            term("f(;)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE) ; (TUPLE) )))"
        );
        assert_eq!(
            term("f(a;)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE) )))"
        );
        assert_eq!(
            term("f (a, b)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) , (CONSTANT_TERM b)) )))"
        );
        assert_eq!(
            term("f(g(1),X)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (FUNCTION_TERM g (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) ))) , (VARIABLE_TERM X)) )))"
        );
        assert!(diagnostics("(a,b;c,d)").is_empty());
        assert!(diagnostics("f(a;)").is_empty());
    }

    #[test]
    fn absolute_values_external_calls_and_the_constants() {
        assert_eq!(
            term("|X;Y|"),
            "(ABS_TERM | (VARIABLE_TERM X) ; (VARIABLE_TERM Y) |)"
        );
        assert_eq!(
            term("| |x| |"),
            "(ABS_TERM | (ABS_TERM | (CONSTANT_TERM x) |) |)"
        );
        assert_eq!(
            term("@f(1)"),
            "(EXTERNAL_TERM @ f (ARGUMENTS ( (TUPLE (CONSTANT_TERM 1)) )))"
        );
        assert_eq!(term("@f"), "(EXTERNAL_TERM @ f)");
        assert_eq!(term("@ f"), "(EXTERNAL_TERM @ f)");
        assert_eq!(term("#inf"), "(CONSTANT_TERM #inf)");
        assert_eq!(term("#supremum"), "(CONSTANT_TERM #supremum)");
        assert_eq!(term("\"s\""), "(CONSTANT_TERM \"s\")");
        assert_eq!(term("_"), "(VARIABLE_TERM _)");
    }

    #[test]
    fn trivia_inside_a_frame_belongs_to_the_frames_node_not_the_tuple() {
        let source = admitted("f( a )");
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        let tuple = parse
            .syntax()
            .descendants()
            .find(|node| node.kind() == SyntaxKind::TUPLE)
            .expect("a tuple");
        assert_eq!(tuple.text(), "a");
        let arguments = tuple.parent().expect("inside arguments");
        assert_eq!(arguments.kind(), SyntaxKind::ARGUMENTS);
        assert_eq!(arguments.text(), "( a )");
    }

    #[test]
    fn a_trailing_comma_in_arguments_is_diagnosed_with_its_hint() {
        assert_eq!(
            term("f(a,)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) ,) )))"
        );
        let kinds = diagnostics("f(a,)");
        assert_eq!(kinds.len(), 1);
        assert!(matches!(
            &kinds[0],
            SyntaxErrorKind::UnexpectedToken {
                found: SyntaxKind::R_PAREN,
                hint: Some(Hint::TrailingCommaInArguments),
                ..
            }
        ));
    }

    #[test]
    fn an_intruder_in_a_frame_is_wrapped_and_the_frame_continues() {
        assert_eq!(
            term("f(a b)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) (ERROR b)) )))"
        );
        assert_eq!(diagnostics("f(a b)").len(), 1);
        assert_eq!(
            term("f($ a)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (ERROR $) (CONSTANT_TERM a)) )))"
        );
        assert_eq!(
            diagnostics("f($ a)").len(),
            1,
            "the lexical diagnostic alone"
        );
    }

    #[test]
    fn an_unclosed_bracket_closes_at_end_of_input_naming_its_opener() {
        assert_eq!(
            term("f(a"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a))))"
        );
        let source = admitted("f(a");
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert!(parse.is_incomplete());
        assert_eq!(parse.diagnostics().len(), 1);
        assert_eq!(parse.diagnostics()[0].related().len(), 1);
        assert_eq!(
            term("f(a,"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM a) ,)))"
        );
        assert_eq!(
            diagnostics("f(a,").len(),
            1,
            "a missing operand and a missing closer at one position merge"
        );
        assert_eq!(
            term("(a; b"),
            "(POOL ( (TUPLE (CONSTANT_TERM a)) ; (TUPLE (CONSTANT_TERM b)))"
        );
        assert_eq!(term("|a"), "(ABS_TERM | (CONSTANT_TERM a))");
    }

    #[test]
    fn a_missing_operand_after_an_operator() {
        assert_eq!(term("1 +"), "(BINARY_TERM (CONSTANT_TERM 1) +)");
        assert!(matches!(
            diagnostics("1 +").as_slice(),
            [SyntaxErrorKind::UnexpectedEndOfInput { hint: None, .. }]
        ));
    }

    #[test]
    fn adjacent_numerals_carry_the_leading_zero_hint() {
        // At the base the term ends at the first numeral; the fragment
        // wraps the rest, and the diagnostic at the second numeral names
        // the mistake.
        assert_eq!(term("007"), "(CONSTANT_TERM 0) (ERROR 0 7)");
        let kinds = diagnostics("007");
        assert_eq!(kinds.len(), 1);
        assert!(matches!(
            &kinds[0],
            SyntaxErrorKind::UnexpectedToken {
                hint: Some(Hint::LeadingZeroNumeral),
                ..
            }
        ));
        // Inside a frame each intruding numeral is wrapped where it stands.
        assert_eq!(
            term("f(007)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (CONSTANT_TERM 0) (ERROR 0) (ERROR 7)) )))"
        );
        assert!(diagnostics("f(007)").iter().all(|kind| matches!(
            kind,
            SyntaxErrorKind::UnexpectedToken {
                hint: Some(Hint::LeadingZeroNumeral),
                ..
            }
        )));
        assert!(
            diagnostics("0 7")
                .iter()
                .all(|kind| matches!(kind, SyntaxErrorKind::UnexpectedToken { hint: None, .. }))
        );
    }

    #[test]
    fn the_query_mark_is_read_by_dialect() {
        let source = admitted("p ?");
        let clingo = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(
            sexpr(&clingo.syntax()),
            "(TERM_FRAGMENT (BINARY_TERM (CONSTANT_TERM p) ?))"
        );
        assert!(matches!(
            clingo.diagnostics()[0].kind(),
            SyntaxErrorKind::UnexpectedEndOfInput {
                hint: Some(Hint::QueryMarkNeedsAspCore2),
                ..
            }
        ));
        let core = parse_term(&Lexer::new(&source, Dialect::AspCore2));
        assert_eq!(
            sexpr(&core.syntax()),
            "(TERM_FRAGMENT (CONSTANT_TERM p) (ERROR ?))"
        );
        let source = admitted("p ? q");
        let core = parse_term(&Lexer::new(&source, Dialect::AspCore2));
        assert_eq!(
            sexpr(&core.syntax()),
            "(TERM_FRAGMENT (BINARY_TERM (CONSTANT_TERM p) ? (CONSTANT_TERM q)))"
        );
    }

    #[test]
    fn the_term_value_restriction_diagnoses_each_excluded_form_and_builds_the_structure() {
        let value = |text: &str| {
            let source = admitted(text);
            let parse = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
            assert_eq!(parse.syntax().text(), text);
            let forms: Vec<RestrictedForm> = parse
                .diagnostics()
                .iter()
                .filter_map(|d| match d.kind() {
                    SyntaxErrorKind::FormNotAllowedHere {
                        form,
                        context: Restriction::TermValue,
                    } => Some(*form),
                    _ => None,
                })
                .collect();
            (sexpr(&parse.syntax()), forms, parse.diagnostics().len())
        };
        assert_eq!(
            value("X"),
            (
                "(TERM_FRAGMENT (VARIABLE_TERM X))".to_owned(),
                vec![RestrictedForm::Variable],
                1
            )
        );
        assert_eq!(value("_").1, vec![RestrictedForm::AnonymousVariable]);
        assert_eq!(value("1..2").1, vec![RestrictedForm::Interval]);
        assert_eq!(value("(1;2)").1, vec![RestrictedForm::Pool]);
        assert_eq!(value("f(1;2)").1, vec![RestrictedForm::Pool]);
        assert_eq!(value("|1;2|").1, vec![RestrictedForm::PooledAbsoluteValue]);
        assert_eq!(value("@f").1, vec![RestrictedForm::ExternalCall]);
        assert_eq!(value("f(1,2)").2, 0);
        assert_eq!(value("(1,)").2, 0);
        assert_eq!(value("|1|").2, 0);
        assert!(
            diagnostics("(1;2) .. |X;Y| + @f(_)").is_empty(),
            "the term family admits every form"
        );
    }

    #[test]
    fn nesting_past_the_constant_is_refused_once_and_carried_losslessly() {
        let depth = MAX_NESTING_DEPTH as usize;
        let admitted_text = format!("{}x{}", "f(".repeat(depth), ")".repeat(depth));
        assert!(
            diagnostics(&admitted_text).is_empty(),
            "the constant itself is admitted"
        );
        let refused_text = format!("{}x{} $", "f(".repeat(depth + 1), ")".repeat(depth + 1));
        let source = admitted(&refused_text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(
            parse.syntax().text(),
            refused_text.as_str(),
            "law 1 under refusal"
        );
        assert_eq!(
            parse.diagnostics().len(),
            1,
            "one refusal, one diagnostic; the `$` inside is silent"
        );
        assert!(matches!(
            parse.diagnostics()[0].kind(),
            SyntaxErrorKind::NestingTooDeep { depth } if *depth == MAX_NESTING_DEPTH
        ));
        let deepest = parse
            .syntax()
            .descendants()
            .map(|node| node.ancestors().count())
            .max()
            .unwrap_or(0);
        assert!(
            deepest <= MAX_TREE_DEPTH as usize,
            "law 3: {deepest} <= {MAX_TREE_DEPTH}"
        );
        assert!(
            parse
                .syntax()
                .descendants()
                .any(|node| node.kind() == SyntaxKind::ERROR)
        );
    }

    #[test]
    fn a_frame_free_chain_of_any_length_never_reaches_the_constant() {
        let long = (0..(MAX_NESTING_DEPTH as usize * 4))
            .map(|_| "1")
            .collect::<Vec<_>>()
            .join("+");
        assert!(diagnostics(&long).is_empty());
        let unary = format!("{}1", "-".repeat(MAX_NESTING_DEPTH as usize * 4));
        assert!(diagnostics(&unary).is_empty());
        let power = (0..(MAX_NESTING_DEPTH as usize * 4))
            .map(|_| "2")
            .collect::<Vec<_>>()
            .join("**");
        assert!(diagnostics(&power).is_empty());
    }

    #[test]
    fn a_dangling_prefix_operator_closes_as_a_unary_term_without_corrupting_the_frame() {
        // The unary run's operand never comes; the run still closes as a
        // UNARY_TERM, the input is lossless, and the terminator acts on the
        // frame, not on the open run (docs/design/syntax.md §6.2, §6.7).
        assert_eq!(term("-"), "(UNARY_TERM -)");
        assert_eq!(term("~"), "(UNARY_TERM ~)");
        assert_eq!(
            term("1 + -"),
            "(BINARY_TERM (CONSTANT_TERM 1) + (UNARY_TERM -))"
        );
        assert_eq!(
            term("f(-)"),
            "(FUNCTION_TERM f (ARGUMENTS ( (TUPLE (UNARY_TERM -)) )))"
        );
        assert_eq!(
            term("(-;a)"),
            "(POOL ( (TUPLE (UNARY_TERM -)) ; (TUPLE (CONSTANT_TERM a)) ))"
        );
        assert_eq!(term("-$"), "(UNARY_TERM - (ERROR $))");
    }
}
