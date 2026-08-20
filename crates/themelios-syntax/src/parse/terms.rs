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

use super::builder::Checkpoint;

use crate::diagnostic::{
    Expected, Hint, Related, RelatedLocus, RestrictedForm, Restriction, SyntaxClass, SyntaxError,
    SyntaxErrorKind,
};
use crate::dialect::Dialect;
use crate::token::TokenSource;
use crate::tree::SyntaxKind;

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
    /// pooled absolute value: the `#const` statement's value term
    /// (grammar §5.9, §6.2), which the directive family constructs.
    ConstantTerm,
    /// The constant term's exclusions and the `@`-call besides.
    TermValue,
    /// Grammar §5.8's theory terms: no restriction, no precedence — one
    /// flat sequence per frame.
    Theory,
}

impl TermContext {
    fn restriction(self) -> Option<Restriction> {
        match self {
            TermContext::Term | TermContext::Theory => None,
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
    /// `{ … }` — a theory set (grammar §5.8).
    TheorySet,
    /// `[ … ]` — a theory list.
    TheoryList,
    /// `( … )` — a theory tuple: `()`, `(a)`, `(a,)`, `(a, b)`.
    TheoryTuple,
    /// `IDENT ( … )` — a theory function's arguments; the frame's node
    /// opens before the name.
    TheoryFunction,
}

/// One open bracket context.
// The four flags are distinct, independently-live pieces of frame state —
// an open unary run, an open tuple, a trailing comma, an open theory
// opterm (docs/design/syntax.md §6.2); folding them into one enum would
// obscure the frame's shape, not clarify it.
#[allow(clippy::struct_excessive_bools)]
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
    /// A `THEORY_OPTERM` is open in this frame — the theory family's
    /// operand slot: an opterm per element, per set or list member, per
    /// tuple member, per function argument.
    opterm_open: bool,
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
            opterm_open: false,
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
            Shape::TheorySet => Some(SyntaxKind::THEORY_SET),
            Shape::TheoryList => Some(SyntaxKind::THEORY_LIST),
            Shape::TheoryTuple => Some(SyntaxKind::THEORY_TUPLE),
            Shape::TheoryFunction => Some(SyntaxKind::THEORY_FUNCTION),
        }
    }

    fn closer(&self) -> Option<SyntaxKind> {
        match self.shape {
            Shape::Base => None,
            Shape::Pool | Shape::Arguments | Shape::TheoryTuple | Shape::TheoryFunction => {
                Some(SyntaxKind::R_PAREN)
            }
            Shape::Abs => Some(SyntaxKind::PIPE),
            Shape::TheorySet => Some(SyntaxKind::R_BRACE),
            Shape::TheoryList => Some(SyntaxKind::R_BRACKET),
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

/// The synchronizers common to the term loop and the theory loop: end of
/// input, the statement and list separators, the three closers, and the
/// necks. Each loop adds its own to these (docs/design/syntax.md §6.7).
fn core_synchronizes(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EOF
            | SyntaxKind::DOT
            | SyntaxKind::COMMA
            | SyntaxKind::SEMICOLON
            | SyntaxKind::R_PAREN
            | SyntaxKind::R_BRACKET
            | SyntaxKind::R_BRACE
            | SyntaxKind::NECK
            | SyntaxKind::WEAK_NECK
    )
}

/// The tokens that end a term or a list where an operand or a closer was
/// expected — the loop's synchronization set (docs/design/syntax.md §6.7):
/// nothing is consumed at them; a token outside this set is an intruder,
/// wrapped, and the frame continues. The absolute-value pipe and the
/// condition colon end a term besides the core set.
fn synchronizes(kind: SyntaxKind) -> bool {
    core_synchronizes(kind) || matches!(kind, SyntaxKind::PIPE | SyntaxKind::COLON)
}

use super::machine::expected;

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
    /// operator follows it: `checkpoint` is that operand's, and
    /// the next token is read as what follows an operand.
    pub(super) fn term_continue(&mut self, context: TermContext, checkpoint: Checkpoint) {
        let mut base = Frame::new(Shape::Base, None, NO_OPENER);
        base.operand = Some(checkpoint);
        self.run(context, vec![base], Next::Operator, false);
    }

    /// An argument list at the next significant token, `(`, on its own
    /// frame — an atom's or a theory atom's arguments, whose enclosing
    /// node is not a term. Returns when the frame
    /// closes; nothing is read at the base after it.
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
            next = match (next, context) {
                (Next::Operand, TermContext::Theory) => self.theory_operand(&mut frames),
                (Next::Operator, TermContext::Theory) => self.theory_after_term(&mut frames),
                (Next::Operand, _) => self.operand(&mut frames, context, last_operator),
                (Next::Operator, _) => self.after_operand(&mut frames, context, &mut last_operator),
                (Next::Done, _) => Next::Done,
            };
            if stop_at_base && frames.len() == 1 {
                // The early return bypasses the base-opterm close below. That
                // is sound only while no `stop_at_base` caller opens a base
                // opterm — an opterm opens under `Theory` alone, and the sole
                // `stop_at_base` caller (`arguments`) is never `Theory`. The
                // assert pins the pairing so a future caller cannot break it.
                debug_assert!(
                    !frames[0].opterm_open,
                    "a stop_at_base run must not leave the base opterm open",
                );
                return;
            }
        }
        if let Some(base) = frames.first_mut()
            && base.opterm_open
        {
            self.finish_node();
            base.opterm_open = false;
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
        if nesting >= self.nesting_limit() {
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
        let mut frame = Frame::new(shape, wrapper, opener);
        // The node kind is the frame's own (`Frame::node`); `open_frame`
        // never opens the base, whose node is `None`.
        let node = frame.node().expect("an opened bracket frame has a node");
        match retroactive {
            Some(checkpoint) => self.start_node_at(checkpoint, node),
            None => self.start_node(node),
        }
        self.bump();
        let next = match shape {
            Shape::Pool | Shape::Arguments => self.tuple_start(&mut frame),
            Shape::TheorySet | Shape::TheoryList | Shape::TheoryTuple | Shape::TheoryFunction => {
                if Some(self.peek()) == frame.closer() {
                    Next::Operator
                } else {
                    Next::Operand
                }
            }
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
        // A fresh tuple holds no operand yet: an operator before its first
        // operand has nothing to bind, and the last tuple's operand
        // checkpoint (now inside a finished node) must not leak into this
        // one (docs/design/syntax.md §6.2, the frame invariant).
        frame.operand = None;
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

    /// Closes a popped frame's open nodes, innermost first: its open
    /// precedence levels, its tuple, its opterm, its own node, and its
    /// wrapper. A dangling unary run is the caller's to close — only the
    /// refusal `unwind` meets one.
    fn finish_frame(&mut self, frame: &mut Frame) {
        self.close_levels_tighter_than(frame, None);
        if frame.tuple_open {
            self.finish_node();
        }
        if frame.opterm_open {
            self.finish_node();
        }
        if frame.node().is_some() {
            self.finish_node();
        }
        if frame.wrapper.is_some() {
            self.finish_node();
        }
    }

    /// Closes the top frame's levels, tuple, node, and wrapper, and pops
    /// it; the enclosing frame's operand is then complete.
    fn close_frame(&mut self, frames: &mut Vec<Frame>) -> Next {
        let mut frame = frames.pop().expect("a bracket frame is open");
        self.finish_frame(&mut frame);
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
            self.finish_frame(&mut frame);
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
    // One dispatch by the operand's first token — a prefix run, an opener,
    // a leaf, a leading comma, an error standing for the operand, or an
    // intruder; splitting it would scatter that single decision.
    #[allow(clippy::too_many_lines)]
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
                // The `@`-call is a `TermValue`-only exclusion; the constant
                // term admits it (docs/design/syntax.md §6.2, grammar §5.9,
                // §5.10).
                if context == TermContext::TermValue {
                    self.restricted(RestrictedForm::ExternalCall, context);
                }
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
            // A lexical `ERROR` token (a malformed string, unexpected
            // characters) standing where an operand would, with a
            // synchronizing token right after it, is itself the operand: it
            // carries its own lexical diagnostic — the one report of that
            // mistake (docs/design/syntax.md §6.7) — so the closer or
            // separator that follows acts on a complete frame and raises no
            // missing-operand diagnostic. A recoverable token after the error
            // instead leaves it an intruder the catch-all wraps, so the term
            // that follows recovers as the operand.
            SyntaxKind::ERROR if synchronizes(self.lookahead(1)) => {
                self.leaf(&mut frames[top], SyntaxKind::ERROR)
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
        // The query reading (grammar §6.1; docs/design/syntax.md §6.3)
        // bites only at the base frame, where the `?` can reach the
        // statement parser and a bare atom before it closes a `QUERY`.
        // Inside an open bracket the `?` is a term still unfinished: it
        // stays the operator, so at end of input its missing operand is an
        // incompleteness (docs/design/syntax.md §6.5), and a member's
        // prefix cut there is unfinished, never wrong.
        let query_mark = top == 0
            && kind == SyntaxKind::QUESTION
            && self.dialect() == Dialect::AspCore2
            && self.lookahead(1) == SyntaxKind::EOF;
        // A binary operator needs a left operand. The empty-element paths
        // (`(,` and a `;` before its first operand) reach here with none;
        // the operator is then an intruder the frame's arm wraps, not a
        // level to open (docs/design/syntax.md §6.2).
        let has_operand = frames[top].operand.is_some();
        if let Some(level) = binary_level(kind).filter(|_| !query_mark && has_operand) {
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
            Shape::TheorySet | Shape::TheoryList | Shape::TheoryTuple | Shape::TheoryFunction => {
                unreachable!("theory frames run on theory_after_term, not after_operand")
            }
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

    /// Whether the next significant token begins a theory term (grammar §5.8).
    pub(super) fn theory_term_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::VARIABLE
                | SyntaxKind::SPLICE
                | SyntaxKind::L_BRACE
                | SyntaxKind::L_BRACKET
                | SyntaxKind::L_PAREN
        )
    }

    /// Whether the next significant token is a theory operator — a
    /// `THEORY_OP` run or `not` (grammar §5.8), under theory mode.
    pub(super) fn theory_operator_here(&mut self) -> bool {
        matches!(self.peek(), SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT)
    }

    /// One theory-opterm at the next significant token, under theory
    /// mode: leading operators, a term, and operator runs and terms
    /// after it, one flat `THEORY_OPTERM` node per frame; false, and
    /// nothing consumed, when neither an operator nor a term begins
    /// there. Ends at the first token that continues nothing — greedy,
    /// which is the guard-end rule when the caller is the guard.
    pub(super) fn theory_opterm(&mut self) -> bool {
        if !self.theory_term_begins() && !self.theory_operator_here() {
            return false;
        }
        self.run(
            TermContext::Theory,
            vec![Frame::new(Shape::Base, None, NO_OPENER)],
            Next::Operand,
            false,
        );
        true
    }

    fn open_opterm(&mut self, frame: &mut Frame) {
        if !frame.opterm_open {
            self.start_node(SyntaxKind::THEORY_OPTERM);
            frame.opterm_open = true;
        }
    }

    /// The tokens that end a theory opterm or a theory list where a term
    /// or a closer was expected. At the base a colon ends the opterm (the
    /// element's condition follows); inside a frame it is an intruder.
    fn theory_synchronizes(kind: SyntaxKind, at_base: bool) -> bool {
        core_synchronizes(kind) || (at_base && kind == SyntaxKind::COLON)
    }

    /// Expecting a theory term: an operator run first, then the term —
    /// a bracketed shape, a function, a constant, a variable, a splice.
    fn theory_operand(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        let at_base = top == 0;
        match self.peek() {
            SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT => {
                self.open_opterm(&mut frames[top]);
                self.bump();
                Next::Operand
            }
            SyntaxKind::L_BRACE => self.theory_bracket(frames, Shape::TheorySet),
            SyntaxKind::L_BRACKET => self.theory_bracket(frames, Shape::TheoryList),
            SyntaxKind::L_PAREN => self.theory_bracket(frames, Shape::TheoryTuple),
            SyntaxKind::IDENT if self.lookahead(1) == SyntaxKind::L_PAREN => {
                self.open_opterm(&mut frames[top]);
                let checkpoint = self.begin_operand(&mut frames[top]);
                self.bump();
                self.open_frame(frames, Shape::TheoryFunction, Some(checkpoint), None)
            }
            SyntaxKind::IDENT
            | SyntaxKind::NUMBER
            | SyntaxKind::STRING
            | SyntaxKind::KW_INF
            | SyntaxKind::KW_SUP => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::CONSTANT_TERM)
            }
            SyntaxKind::VARIABLE => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::VARIABLE_TERM)
            }
            SyntaxKind::SPLICE => {
                self.open_opterm(&mut frames[top]);
                self.leaf(&mut frames[top], SyntaxKind::SPLICE_TERM)
            }
            kind => {
                if Self::theory_synchronizes(kind, at_base) {
                    self.unexpected(expected(&[Expected::Class(SyntaxClass::TheoryTerm)]), None);
                    Next::Operator
                } else {
                    self.wrap_unexpected(
                        expected(&[Expected::Class(SyntaxClass::TheoryTerm)]),
                        None,
                    );
                    Next::Operand
                }
            }
        }
    }

    /// A bracketed theory term: the opterm opened, the frame opened at
    /// the operand's checkpoint.
    fn theory_bracket(&mut self, frames: &mut Vec<Frame>, shape: Shape) -> Next {
        let top = frames.len() - 1;
        self.open_opterm(&mut frames[top]);
        let checkpoint = self.begin_operand(&mut frames[top]);
        self.open_frame(frames, shape, Some(checkpoint), None)
    }

    /// After a theory term: an operator run continues the opterm; a
    /// comma ends the opterm and begins the next in a bracketed frame,
    /// or ends the opterm at the base; the closer closes the frame; at
    /// the base anything else ends the opterm; inside a frame a
    /// synchronizing token closes it as unclosed and anything else is an
    /// intruder, wrapped.
    fn theory_after_term(&mut self, frames: &mut Vec<Frame>) -> Next {
        let top = frames.len() - 1;
        let kind = self.peek();
        if matches!(kind, SyntaxKind::THEORY_OP | SyntaxKind::KW_NOT) {
            self.bump();
            return Next::Operand;
        }
        let shape = frames[top].shape;
        if shape == Shape::Base {
            return Next::Done;
        }
        let closer = frames[top].closer();
        if kind == SyntaxKind::COMMA {
            self.close_opterm(&mut frames[top]);
            self.bump();
            if shape == Shape::TheoryTuple && Some(self.peek()) == closer {
                return Next::Operator;
            }
            return Next::Operand;
        }
        if Some(kind) == closer {
            self.close_opterm(&mut frames[top]);
            self.bump();
            return self.close_frame(frames);
        }
        if Self::theory_synchronizes(kind, false) {
            self.close_opterm(&mut frames[top]);
            return self.unclosed(frames);
        }
        let mut wanted = vec![
            Expected::Token(SyntaxKind::COMMA),
            Expected::Class(SyntaxClass::TheoryOperator),
        ];
        if let Some(closer) = closer {
            wanted.push(Expected::Token(closer));
        }
        self.wrap_unexpected(wanted.into_iter().collect(), None);
        Next::Operator
    }

    fn close_opterm(&mut self, frame: &mut Frame) {
        if frame.opterm_open {
            self.finish_node();
            frame.opterm_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Hint, RestrictedForm, Restriction, SyntaxErrorKind};
    use crate::dialect::Dialect;
    use crate::lexer::Lexer;
    use crate::parse::{MAX_TREE_DEPTH, NestingLimit, parse_term, parse_term_value};
    use crate::tree::{SyntaxKind, sexpr};

    use crate::parse::test_util::admitted;

    /// The shape of the term the term entry reads from `text` under the
    /// clingo dialect, with the fragment root peeled.
    fn term(text: &str) -> String {
        let source = admitted(text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert_eq!(parse.syntax().text(), text, "law 1");
        let shape = sexpr(&parse.syntax());
        shape
            .strip_prefix("(TERM_FRAGMENT ")
            .and_then(|rest| rest.strip_suffix(')'))
            .map_or(shape.clone(), str::to_owned)
    }

    fn diagnostics(text: &str) -> Vec<SyntaxErrorKind> {
        let source = admitted(text);
        parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT)
            .diagnostics()
            .iter()
            .map(|d| d.kind().clone())
            .collect()
    }

    #[test]

    fn a_merged_missing_operand_and_missing_closer_keeps_the_opener_locus() {
        use crate::diagnostic::RelatedLocus;
        // `f(a,` at end of input: the missing operand after `,` and the
        // missing `)` land at the same position and merge; the merge keeps
        // the "to close (" locus the unclosed frame carries, which a bare
        // `f(a` also keeps (docs/design/syntax.md §6.7).
        let source = admitted("f(a,");
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        let loci: Vec<RelatedLocus> = parse
            .diagnostics()
            .iter()
            .flat_map(|d| d.related().iter().map(|related| related.locus))
            .collect();
        assert!(
            loci.contains(&RelatedLocus::ToClose(SyntaxKind::L_PAREN)),
            "the merged diagnostic keeps the opener locus; got {loci:?}"
        );
    }

    #[test]
    fn an_empty_tuple_comma_is_the_pool_frames_alone() {
        // The `(,)` empty-tuple case is a pool-frame comma with no term counted
        // yet: the second comma of `(a,,)` (a term already counted, so
        // `tuple_terms` is one, not zero) is not it, and the comma of `f(,)`
        // (an argument frame, not a pool) is not it either. Both hold the
        // `shape == Pool && tuple_terms == 0` guard and the `tuple_terms`
        // counter against their mutations.
        let sx = |t: &str| sexpr(&crate::parse::parse(&admitted(t), Dialect::Clingo).syntax());
        assert_eq!(
            sx("p((a,,))."),
            "(PROGRAM (RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (POOL ( (TUPLE (CONSTANT_TERM a) , ,) ))) )))) .))"
        );
        assert_eq!(
            sx("p(f(,))."),
            "(PROGRAM (RULE (LITERAL (ATOM p (ARGUMENTS ( (TUPLE (FUNCTION_TERM f (ARGUMENTS ( (TUPLE ,) )))) )))) .))"
        );
    }

    #[test]
    fn a_theory_opterm_ends_at_its_condition_colon_and_recovers_over_stray_input() {
        // The colon that opens a theory element's condition ends the opterm
        // before it (the base-frame `kind == COLON` synchronizer); and a stray
        // `$` between elements is carried into one ERROR node, the elements
        // around it whole — the theory recovery's synchronization.
        let sx = |t: &str| sexpr(&crate::parse::parse(&admitted(t), Dialect::Clingo).syntax());
        assert_eq!(
            sx("&a { x : p ; y : q }."),
            "(PROGRAM (RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x)) : (CONDITION (LITERAL (ATOM p)))) ; (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM y)) : (CONDITION (LITERAL (ATOM q)))) })) .))"
        );
        assert_eq!(
            sx("&a { x $ y }."),
            "(PROGRAM (RULE (THEORY_ATOM & a (THEORY_ELEMENTS { (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM x))) (ERROR $) (THEORY_ELEMENT (THEORY_OPTERM (CONSTANT_TERM y))) })) .))"
        );
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
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
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
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
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
        let clingo = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
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
        let core = parse_term(
            &Lexer::new(&source, Dialect::AspCore2),
            NestingLimit::DEFAULT,
        );
        assert_eq!(
            sexpr(&core.syntax()),
            "(TERM_FRAGMENT (CONSTANT_TERM p) (ERROR ?))"
        );
        let source = admitted("p ? q");
        let core = parse_term(
            &Lexer::new(&source, Dialect::AspCore2),
            NestingLimit::DEFAULT,
        );
        assert_eq!(
            sexpr(&core.syntax()),
            "(TERM_FRAGMENT (BINARY_TERM (CONSTANT_TERM p) ? (CONSTANT_TERM q)))"
        );
    }

    #[test]
    fn the_term_value_restriction_diagnoses_each_excluded_form_and_builds_the_structure() {
        let value = |text: &str| {
            let source = admitted(text);
            let parse =
                parse_term_value(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
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
    fn nesting_past_the_default_limit_is_refused_once_and_carried_losslessly() {
        // At `NestingLimit::DEFAULT` the refused tree is shallow enough to
        // hold on a modest stack, so no `with_required_stack` is needed; the
        // deep tier is exercised at the ceiling by `tests/depth_gate.rs`.
        let depth = NestingLimit::DEFAULT.frames() as usize;
        let admitted_text = format!("{}x{}", "f(".repeat(depth), ")".repeat(depth));
        assert!(
            diagnostics(&admitted_text).is_empty(),
            "the limit itself is admitted"
        );
        let refused_text = format!("{}x{} $", "f(".repeat(depth + 1), ")".repeat(depth + 1));
        let source = admitted(&refused_text);
        let parse = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
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
            SyntaxErrorKind::NestingTooDeep { depth } if *depth == NestingLimit::DEFAULT.frames()
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
        let long = (0..(NestingLimit::DEFAULT.frames() as usize * 4))
            .map(|_| "1")
            .collect::<Vec<_>>()
            .join("+");
        assert!(diagnostics(&long).is_empty());
        let unary = format!(
            "{}1",
            "-".repeat(NestingLimit::DEFAULT.frames() as usize * 4)
        );
        assert!(diagnostics(&unary).is_empty());
        let power = (0..(NestingLimit::DEFAULT.frames() as usize * 4))
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

    #[test]
    fn a_leading_comma_before_an_operator_is_carried_not_a_panic() {
        // `(,` and a `;` before a tuple's first operand leave it empty; a
        // binary operator then has no left operand and is wrapped where it
        // stands, never unwrapped into a panic and never opening a level at
        // a stale checkpoint (docs/design/syntax.md §6.2, the frame
        // invariant).
        assert_eq!(term("(,&)"), "(POOL ( (TUPLE , (ERROR &)) ))");
        assert_eq!(
            term("(1;,..)"),
            "(POOL ( (TUPLE (CONSTANT_TERM 1)) ; (TUPLE , (ERROR ..)) ))"
        );
        for t in ["(,&)", "(,*)", "(1;,..)", "(a;,+)", "(,..1)"] {
            let source = admitted(t);
            // Total (no panic) and lossless.
            assert_eq!(
                parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT)
                    .syntax()
                    .text(),
                t
            );
            assert!(!diagnostics(t).is_empty());
        }
    }
}
