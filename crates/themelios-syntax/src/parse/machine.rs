//! The parser core (docs/design/syntax.md §6): the token cursor over the
//! source door, the green builder under the trivia-placement law, the
//! two forms of defect and the synchronization machinery, the docs
//! production's mechanism, the aspif dispatch, and the entry points'
//! loops. The families — terms, statements, theory — extend this type
//! in their own files.

use rowan::{Checkpoint, GreenNodeBuilder, Language};
use themelios_base::span::{ByteOffset, Location, Span};

use crate::ast;
use crate::diagnostic::{
    Expected, ExpectedSet, Hint, MisplacedDoc, Related, RelatedLocus, SourceBreach, SyntaxClass,
    SyntaxError, SyntaxErrorKind,
};
use crate::dialect::Dialect;
use crate::lexer::classify_error;
use crate::token::{LexMode, Token, TokenSource};
use crate::tree::{Asp, SyntaxKind};

use super::{EntryPoint, Parse};

/// The parser over one token source: a cursor, a builder, and the
/// diagnostics it accumulates. Constructed per parse and dropped with
/// it — no state outlives a call (docs/design/syntax.md §12.1).
// The four flags are distinct pieces of parser state — the witnessed
// breach, the docs-are-trivia region, the silenced lexical diagnostics,
// and the depth refusal (docs/design/syntax.md §4.2, §4.3, §4.5, §6.6);
// folding them into one field would obscure, not clarify.
#[allow(clippy::struct_excessive_bools)]
pub(super) struct Parser<'s, S: TokenSource> {
    source: &'s S,
    text: &'s str,
    dialect: Dialect,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<SyntaxError>,
    /// The offset of the first byte not yet placed in the tree.
    at: u32,
    /// The mode the next token is requested under (docs/design/syntax.md §4.2).
    mode: LexMode,
    /// A breach was witnessed: every peek answers `EOF` from here on.
    /// A legitimate end of input is not recorded — the door answers `EOF`
    /// at the text's end as often as it is asked.
    ended: bool,
    /// How many statement nodes are open: inside one, a doc comment is
    /// trivia with a warning; at program level the loop reads the run.
    statement_depth: u32,
    /// Where a doc comment is trivia diagnosed as documenting nothing
    /// though no statement is open: inside a program-level `ERROR` skip,
    /// throughout a term entry, and after a fragment's construct.
    skipping: bool,
    /// Whether placed `ERROR` tokens raise their lexical diagnostic: off
    /// inside the aspif dispatch and a depth-refused statement, whose
    /// one diagnostic stands for the whole (docs/design/syntax.md §4.5).
    lexical_diagnostics: bool,
    /// The last peek, so a repeated peek at one position costs nothing.
    peeked: Option<Peeked>,
    /// The kind and end offset of the last token placed, trivia included.
    last_placed: Option<(SyntaxKind, u32)>,
    /// The frame loop refused a frame past the constant: the rest of the
    /// statement is already carried in an `ERROR` node, and the statement
    /// closes without further diagnostics (docs/design/syntax.md §6.6).
    depth_refused: bool,
}

/// One significant token found ahead of the cursor: where the trivia
/// before it ends, and the token.
#[derive(Clone, Copy)]
struct Peeked {
    from: u32,
    mode: LexMode,
    docs_are_trivia: bool,
    start: u32,
    kind: SyntaxKind,
    len: u32,
}

impl<'s, S: TokenSource> Parser<'s, S> {
    pub(super) fn new(source: &'s S) -> Parser<'s, S> {
        Parser {
            source,
            text: source.text(),
            dialect: source.dialect(),
            builder: GreenNodeBuilder::new(),
            diagnostics: Vec::new(),
            at: 0,
            mode: LexMode::Normal,
            ended: false,
            statement_depth: 0,
            skipping: false,
            lexical_diagnostics: true,
            peeked: None,
            last_placed: None,
            depth_refused: false,
        }
    }

    // ---- the cursor -------------------------------------------------

    /// The raw token at `at` under the current mode, with the two
    /// witnessable breaches turned into end of input plus their
    /// diagnostic (docs/design/syntax.md §4.3).
    fn raw(&mut self, at: u32) -> Token<'s> {
        let eof = Token {
            kind: SyntaxKind::EOF,
            text: "",
        };
        if self.ended {
            return eof;
        }
        let end = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        match self.source.token_at(ByteOffset::new(at), self.mode) {
            Err(_) => {
                self.breach(
                    SourceBreach::Refusal {
                        at: ByteOffset::new(at),
                    },
                    at,
                );
                eof
            }
            Ok(token) if token.kind == SyntaxKind::EOF => {
                if at != end {
                    self.breach(
                        SourceBreach::Tiling {
                            at: ByteOffset::new(at),
                            token: SyntaxKind::EOF,
                            len: 0,
                        },
                        at,
                    );
                }
                eof
            }
            Ok(token) => {
                let len = u32::try_from(token.text.len()).unwrap_or(u32::MAX);
                if len == 0 || at.checked_add(len).is_none_or(|next| next > end) {
                    self.breach(
                        SourceBreach::Tiling {
                            at: ByteOffset::new(at),
                            token: token.kind,
                            len,
                        },
                        at,
                    );
                    return eof;
                }
                token
            }
        }
    }

    fn breach(&mut self, breach: SourceBreach, at: u32) {
        self.ended = true;
        let location = self.location(at, at);
        self.diagnostics.push(SyntaxError::new(
            SyntaxErrorKind::TokenSourceBreach { breach },
            location,
        ));
    }

    /// Whether a doc comment is trivia where the cursor stands
    /// (docs/design/syntax.md §5.4, §6.3): everywhere but a program
    /// position — inside a statement, or where `skipping` says.
    fn docs_are_trivia(&self) -> bool {
        self.statement_depth > 0 || self.skipping
    }

    fn is_trivia_here(&self, kind: SyntaxKind) -> bool {
        kind.is_trivia() || (kind == SyntaxKind::DOC_COMMENT && self.docs_are_trivia())
    }

    /// The next significant token under the current mode, without
    /// consuming anything: trivia is looked past, never placed, so a
    /// node that finishes after a peek ends where its last significant
    /// token ended (docs/design/syntax.md §5.4, law 2).
    fn peek_token(&mut self) -> Peeked {
        let docs_are_trivia = self.docs_are_trivia();
        let cached = self.peeked.filter(|peeked| {
            peeked.from == self.at
                && peeked.mode == self.mode
                && peeked.docs_are_trivia == docs_are_trivia
        });
        if let Some(peeked) = cached {
            return peeked;
        }
        let mut start = self.at;
        loop {
            let token = self.raw(start);
            let len = u32::try_from(token.text.len()).unwrap_or(0);
            if token.kind != SyntaxKind::EOF && self.is_trivia_here(token.kind) {
                start += len;
                continue;
            }
            let peeked = Peeked {
                from: self.at,
                mode: self.mode,
                docs_are_trivia,
                start,
                kind: token.kind,
                len,
            };
            self.peeked = Some(peeked);
            return peeked;
        }
    }

    /// The kind of the next significant token, `EOF` at the end.
    pub(super) fn peek(&mut self) -> SyntaxKind {
        self.peek_token().kind
    }

    /// The text of the next significant token.
    pub(super) fn peek_text(&mut self) -> &'s str {
        let peeked = self.peek_token();
        let start = peeked.start as usize;
        &self.text[start..start + peeked.len as usize]
    }

    /// The kind of the `n`th significant token ahead (`lookahead(0)` is
    /// `peek()`), under the current mode, without consuming anything —
    /// bounded by the caller to the grammar's five (docs/design/syntax.md
    /// §6.2, §6.3).
    pub(super) fn lookahead(&mut self, n: usize) -> SyntaxKind {
        let mut at = self.peek_token().start;
        let mut kind = self.peek_token().kind;
        for _ in 0..n {
            if kind == SyntaxKind::EOF {
                return kind;
            }
            let mut probe = at + u32::try_from(self.raw(at).text.len()).unwrap_or(0);
            loop {
                let token = self.raw(probe);
                if token.kind != SyntaxKind::EOF && self.is_trivia_here(token.kind) {
                    probe += u32::try_from(token.text.len()).unwrap_or(0);
                    continue;
                }
                at = probe;
                kind = token.kind;
                break;
            }
        }
        kind
    }

    /// The offset where the next significant token begins.
    pub(super) fn peek_start(&mut self) -> u32 {
        self.peek_token().start
    }

    pub(super) fn at_end(&mut self) -> bool {
        self.peek() == SyntaxKind::EOF
    }

    /// The mode the next token is requested under. The setter's query
    /// twin; no reader consumes it yet (the theory family drives the mode
    /// through `set_mode` alone), so the allowance stands until one does.
    #[allow(dead_code)]
    pub(super) fn mode(&self) -> LexMode {
        self.mode
    }

    /// Sets the mode for the tokens that follow. A token peeked under
    /// the old mode is dropped unread — the twice-per-token bound of
    /// docs/design/syntax.md §4.2.
    pub(super) fn set_mode(&mut self, mode: LexMode) {
        self.mode = mode;
        self.peeked = None;
    }

    // ---- the builder under the trivia law ---------------------------

    /// Places the trivia before the next significant token into the
    /// open node — the node open where the trivia stood. Doc comments
    /// are placed and warned where they are trivia; at program level the
    /// loop reads them and this stops before them.
    fn eat_trivia(&mut self) {
        loop {
            let token = self.raw(self.at);
            if token.kind == SyntaxKind::EOF || !self.is_trivia_here(token.kind) {
                return;
            }
            let start = self.at;
            let len = u32::try_from(token.text.len()).unwrap_or(0);
            self.builder.token(Asp::kind_to_raw(token.kind), token.text);
            self.at = start + len;
            self.last_placed = Some((token.kind, self.at));
            if token.kind == SyntaxKind::DOC_COMMENT {
                let reason = if self.statement_depth > 0 {
                    MisplacedDoc::InsideStatement
                } else {
                    MisplacedDoc::NoStatementFollows
                };
                let location = self.location(start, start + len);
                self.diagnostics.push(SyntaxError::new(
                    SyntaxErrorKind::MisplacedDocComment { reason },
                    location,
                ));
            }
        }
    }

    /// Consumes the next significant token into the open node, the
    /// trivia before it first. An `ERROR` token placed raises its one
    /// lexical diagnostic (docs/design/syntax.md §4.5).
    pub(super) fn bump(&mut self) {
        let peeked = self.peek_token();
        if peeked.kind == SyntaxKind::EOF {
            return;
        }
        self.eat_trivia();
        let token = self.raw(self.at);
        debug_assert_eq!(token.kind, peeked.kind);
        let start = self.at;
        let len = u32::try_from(token.text.len()).unwrap_or(0);
        self.builder.token(Asp::kind_to_raw(token.kind), token.text);
        self.at = start + len;
        self.last_placed = Some((token.kind, self.at));
        self.peeked = None;
        if token.kind == SyntaxKind::ERROR && self.lexical_diagnostics {
            self.lexical_diagnostic(token.text, start);
        }
    }

    fn lexical_diagnostic(&mut self, text: &str, start: u32) {
        let end = start as usize + text.len();
        let following = self.text[end..].chars().next();
        let defect = classify_error(text, following, self.mode, self.dialect);
        let from = start + u32::try_from(defect.primary.start).unwrap_or(0);
        let to = start + u32::try_from(defect.primary.end).unwrap_or(0);
        let mut error = SyntaxError::new(defect.kind, self.location(from, to));
        if defect.literal_extent {
            error = error.with_related(Related {
                locus: RelatedLocus::LiteralExtent,
                location: self.location(start, start + u32::try_from(text.len()).unwrap_or(0)),
            });
        }
        self.diagnostics.push(error);
    }

    /// Consumes the next significant token if it is `kind`.
    pub(super) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Opens a node at the next significant token: the trivia before it
    /// is placed into the parent first (law 2).
    pub(super) fn start_node(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        self.builder.start_node(Asp::kind_to_raw(kind));
    }

    /// A checkpoint at the next significant token — the trivia before it
    /// placed into the open node — for a node opened retroactively.
    /// Taken only where a significant token is about to be consumed.
    pub(super) fn checkpoint(&mut self) -> Checkpoint {
        self.eat_trivia();
        self.builder.checkpoint()
    }

    /// Opens `kind` retroactively around everything built since `at`.
    pub(super) fn start_node_at(&mut self, at: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(at, Asp::kind_to_raw(kind));
    }

    /// Closes the open node where its last significant token ended:
    /// trivia after it is not consumed here and falls to the parent.
    pub(super) fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    /// A statement begins here: until it leaves, a doc comment is trivia
    /// with a warning (docs/design/syntax.md §6.3), whatever node is
    /// open — the statement's own node may open later, retroactively.
    pub(super) fn enter_statement(&mut self) {
        self.statement_depth += 1;
        self.peeked = None;
    }

    /// The statement left.
    pub(super) fn leave_statement(&mut self) {
        self.statement_depth -= 1;
        self.peeked = None;
    }

    /// Places an empty node — one of the three kinds that may be empty
    /// (docs/design/syntax.md §5.4) — immediately after the token that
    /// licensed it, holding no trivia.
    pub(super) fn empty_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(Asp::kind_to_raw(kind));
        self.builder.finish_node();
    }

    // ---- diagnostics and recovery -----------------------------------

    pub(super) fn location(&self, start: u32, end: u32) -> Location {
        Location {
            source: self.source.id(),
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end))
                .expect("the parser's spans are ordered"),
        }
    }

    pub(super) fn diagnose(&mut self, error: SyntaxError) {
        self.diagnostics.push(error);
    }

    /// The dialect parsed under.
    pub(super) fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// The length of the next significant token.
    pub(super) fn peek_len(&mut self) -> u32 {
        self.peek_token().len
    }

    /// The missing-child form with a related locus (a missing closer
    /// names its opener). Two diagnostics at one position merge: a
    /// second expectation at the token that already carries one extends
    /// its expected set rather than repeating the report — one token, one
    /// diagnostic, the whole of what was expected there. A numeral found
    /// directly after a numeral carries the leading-zero hint (grammar
    /// §4.3): `007` is three numerals, and the parser names the mistake.
    pub(super) fn unexpected_related(
        &mut self,
        expected: ExpectedSet,
        hint: Option<Hint>,
        related: Option<Related>,
    ) {
        let peeked = self.peek_token();
        if peeked.kind == SyntaxKind::ERROR {
            return;
        }
        let hint = hint.or_else(|| {
            (peeked.kind == SyntaxKind::NUMBER
                && self.last_placed == Some((SyntaxKind::NUMBER, peeked.start)))
            .then_some(Hint::LeadingZeroNumeral)
        });
        let location = self.location(peeked.start, peeked.start + peeked.len);
        if let Some(last) = self.diagnostics.last_mut()
            && last.primary() == location
            && last.extend_expected(&expected, hint)
        {
            return;
        }
        let kind = match peeked.kind {
            SyntaxKind::EOF => SyntaxErrorKind::UnexpectedEndOfInput { expected, hint },
            found => SyntaxErrorKind::UnexpectedToken {
                expected,
                found,
                hint,
            },
        };
        let mut error = SyntaxError::new(kind, location);
        if let Some(related) = related {
            error = error.with_related(related);
        }
        self.diagnostics.push(error);
    }

    /// Consumes the rest of the statement into the open node: through the
    /// terminating dot and an immediately following `[…]` group, or to end
    /// of input (docs/design/syntax.md §6.6, §6.7).
    pub(super) fn skip_statement_rest(&mut self) {
        while !self.at_end() {
            let kind = self.peek();
            self.bump();
            if kind == SyntaxKind::DOT {
                if self.peek() == SyntaxKind::L_BRACKET {
                    self.skip_bracket_group();
                }
                return;
            }
        }
    }

    /// Whether the frame loop refused a frame in this statement.
    pub(super) fn depth_refused(&self) -> bool {
        self.depth_refused
    }

    /// The refusal: its diagnostic at the opener, the lexical diagnostics
    /// silenced, and the rest of the statement carried in one `ERROR`
    /// node under the innermost open frame.
    pub(super) fn refuse_depth(&mut self) {
        let start = self.peek_start();
        let end = start + self.peek_len();
        let location = self.location(start, end);
        self.diagnostics.push(SyntaxError::new(
            SyntaxErrorKind::NestingTooDeep {
                depth: super::MAX_NESTING_DEPTH,
            },
            location,
        ));
        self.lexical_diagnostics = false;
        self.depth_refused = true;
        // The rest of the statement is skipped to its terminating dot; a
        // refusal inside a theory region left the mode at `Theory`, where
        // a dot glued to operator characters lexes as one `THEORY_OP` and
        // the terminator is missed (docs/design/syntax.md §4.2, §6.6). The
        // skip reads in normal mode; the loops' restore does the same.
        self.set_mode(LexMode::Normal);
        self.start_node(SyntaxKind::ERROR);
        self.skip_statement_rest();
        self.finish_node();
    }

    /// Restores the parser after a refused statement closed: the lexical
    /// diagnostics back on, and the mode back to normal, whatever region
    /// the refusal fell in.
    pub(super) fn end_statement_after_refusal(&mut self) {
        self.lexical_diagnostics = true;
        self.depth_refused = false;
        self.set_mode(LexMode::Normal);
    }

    /// The missing-child form (docs/design/syntax.md §6.7): a diagnostic
    /// at the next significant token — `unexpected-token` with what was
    /// expected and what was found, or `unexpected-end-of-input` — and
    /// nothing consumed. A lexical `ERROR` token found here raises no
    /// structural diagnostic: its lexical diagnostic, raised when it is
    /// placed, is the one report of that mistake. A numeral found
    /// directly after a numeral carries the leading-zero hint (grammar
    /// §4.3): `007` is three numerals, and the parser names the mistake.
    pub(super) fn unexpected(&mut self, expected: ExpectedSet, hint: Option<Hint>) {
        self.unexpected_related(expected, hint, None);
    }

    /// `unexpected` with one expected token.
    pub(super) fn expected_token(&mut self, kind: SyntaxKind) {
        self.unexpected([Expected::Token(kind)].into_iter().collect(), None);
    }

    /// Consumes `kind` if it is next; otherwise the missing-child
    /// diagnostic, and nothing consumed.
    pub(super) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.expected_token(kind);
            false
        }
    }

    /// The unexpected-token form, alone (docs/design/syntax.md §6.7):
    /// the next significant token wrapped in an `ERROR` node, its
    /// diagnostic raised, and the parse continuing after it.
    pub(super) fn wrap_unexpected(&mut self, expected: ExpectedSet, hint: Option<Hint>) {
        self.unexpected(expected, hint);
        self.start_node(SyntaxKind::ERROR);
        self.bump();
        self.finish_node();
    }

    /// The unexpected-token form through a synchronization point: the
    /// next significant token and everything up to (not including) the
    /// first token of `sync` — or to end of input — wrapped in one
    /// `ERROR` node, byte-preserved.
    pub(super) fn skip_into_error(
        &mut self,
        expected: ExpectedSet,
        hint: Option<Hint>,
        sync: &[SyntaxKind],
    ) {
        self.unexpected(expected, hint);
        if self.at_end() {
            return;
        }
        self.start_node(SyntaxKind::ERROR);
        self.bump();
        while !self.at_end() && !sync.contains(&self.peek()) {
            self.bump();
        }
        self.finish_node();
    }

    /// The program-level row of docs/design/syntax.md §6.7: `ERROR`
    /// through the next `.` — and an immediately following `[…]` group,
    /// since the four annotation families put one after the dot — or to
    /// end of input.
    pub(super) fn recover_program_level(&mut self) {
        let was_skipping = self.skipping;
        self.skipping = true;
        self.peeked = None;
        self.unexpected(
            [Expected::Class(SyntaxClass::Statement)]
                .into_iter()
                .collect(),
            None,
        );
        self.start_node(SyntaxKind::ERROR);
        self.skip_statement_rest();
        self.finish_node();
        self.skipping = was_skipping;
        self.peeked = None;
    }

    /// Consumes a `[`…`]` group, brackets counted, into the open node.
    fn skip_bracket_group(&mut self) {
        let mut depth = 0u32;
        while !self.at_end() {
            let kind = self.peek();
            self.bump();
            match kind {
                SyntaxKind::L_BRACKET => depth += 1,
                SyntaxKind::R_BRACKET => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // ---- the entry points -------------------------------------------

    fn finish<T: crate::tree::AstNode<Language = Asp>>(self, entry: EntryPoint) -> Parse<T> {
        let Parser {
            builder,
            diagnostics,
            source,
            dialect,
            ..
        } = self;
        Parse::new(builder.finish(), diagnostics, source.id(), dialect, entry)
    }

    /// The program entry (docs/design/syntax.md §6.1, §6.3).
    pub(super) fn program(mut self) -> Parse<ast::Program> {
        self.builder
            .start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        if self.dispatches_aspif() {
            self.aspif();
        } else {
            self.statements();
        }
        self.builder.finish_node();
        self.finish(EntryPoint::Program)
    }

    /// The statement entry: one program position, then end of input.
    pub(super) fn statement_fragment(mut self) -> Parse<ast::StatementFragment> {
        self.builder
            .start_node(Asp::kind_to_raw(SyntaxKind::STATEMENT_FRAGMENT));
        let checkpoint = self.docs_and_checkpoint();
        if self.statement_begins() {
            self.statement(checkpoint);
            if self.depth_refused() {
                self.end_statement_after_refusal();
            }
        }
        self.expect_end_of_input();
        self.builder.finish_node();
        self.finish(EntryPoint::Statement)
    }

    /// The two term entries: one term under the entry's restriction,
    /// then end of input. A term entry has no program position, so a
    /// doc comment anywhere in it is trivia with its warning. Task 8
    /// supplies the term itself.
    pub(super) fn term_fragment(mut self, entry: EntryPoint) -> Parse<ast::TermFragment> {
        self.builder
            .start_node(Asp::kind_to_raw(SyntaxKind::TERM_FRAGMENT));
        self.skipping = true;
        self.eat_trivia();
        self.term_at_entry(entry);
        self.expect_end_of_input();
        self.builder.finish_node();
        self.finish(entry)
    }

    /// The trailing check every fragment entry ends with: trailing
    /// trivia admitted — a doc comment among it is trivia with its
    /// warning, no statement following — and anything else one `ERROR`
    /// node with `EndOfInput` in its expected set.
    fn expect_end_of_input(&mut self) {
        self.skipping = true;
        self.peeked = None;
        if !self.at_end() {
            self.unexpected(
                [Expected::Class(SyntaxClass::EndOfInput)]
                    .into_iter()
                    .collect(),
                None,
            );
            self.start_node(SyntaxKind::ERROR);
            while !self.at_end() {
                self.bump();
            }
            self.finish_node();
        }
        self.eat_trivia();
    }

    /// Grammar §4.9: the identifier `asp`, one space, a decimal numeral,
    /// at the very start.
    fn dispatches_aspif(&self) -> bool {
        let bytes = self.text.as_bytes();
        bytes.starts_with(b"asp ") && bytes.get(4).is_some_and(u8::is_ascii_digit)
    }

    /// The whole text as one `ERROR` token under `PROGRAM`, lossless,
    /// with the single diagnostic at the header.
    fn aspif(&mut self) {
        let numeral = 4 + self.text.as_bytes()[4..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        let location = self.location(0, u32::try_from(numeral).unwrap_or(0));
        self.diagnostics
            .push(SyntaxError::new(SyntaxErrorKind::AspifInput, location));
        self.builder
            .token(Asp::kind_to_raw(SyntaxKind::ERROR), self.text);
        self.at = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        self.ended = true;
    }

    /// The program's statements: at each position, the leading trivia,
    /// the docs run if any, then a statement opened at the checkpoint
    /// before its docs, or program-level recovery.
    fn statements(&mut self) {
        loop {
            let checkpoint = self.docs_and_checkpoint();
            if self.at_end() {
                return;
            }
            if self.statement_begins() {
                self.statement(checkpoint);
                if self.depth_refused() {
                    self.end_statement_after_refusal();
                }
            } else {
                self.recover_program_level();
            }
        }
    }

    /// At a program position: place the trivia before the docs, take the
    /// checkpoint a statement will open at, place the docs run — each
    /// line a `DOC_COMMENT` token, the trivia between them — and, when
    /// no statement follows, warn on each line (grammar §5.11: the run is
    /// trivia then).
    fn docs_and_checkpoint(&mut self) -> Checkpoint {
        self.eat_trivia();
        let checkpoint = self.builder.checkpoint();
        let mut lines: Vec<(u32, u32)> = Vec::new();
        while self.peek() == SyntaxKind::DOC_COMMENT {
            let start = self.peek_start();
            let len = self.peek_token().len;
            self.bump();
            lines.push((start, start + len));
            self.eat_trivia();
        }
        if !lines.is_empty() && !self.statement_begins() {
            for (start, end) in lines {
                let location = self.location(start, end);
                self.diagnostics.push(SyntaxError::new(
                    SyntaxErrorKind::MisplacedDocComment {
                        reason: MisplacedDoc::NoStatementFollows,
                    },
                    location,
                ));
            }
        }
        checkpoint
    }

    /// Whether the next significant token begins a statement of the
    /// dialect: a directive keyword, a neck, or anything a head begins
    /// with — Tasks 9–11 realize each family.
    pub(super) fn statement_begins(&mut self) -> bool {
        matches!(
            self.peek(),
            SyntaxKind::IDENT
                | SyntaxKind::VARIABLE
                | SyntaxKind::ANONYMOUS
                | SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::KW_INF
                | SyntaxKind::KW_SUP
                | SyntaxKind::KW_NOT
                | SyntaxKind::KW_TRUE
                | SyntaxKind::KW_FALSE
                | SyntaxKind::MINUS
                | SyntaxKind::TILDE
                | SyntaxKind::L_PAREN
                | SyntaxKind::PIPE
                | SyntaxKind::AT
                | SyntaxKind::L_BRACE
                | SyntaxKind::AMPERSAND
                | SyntaxKind::KW_COUNT
                | SyntaxKind::KW_SUM
                | SyntaxKind::KW_SUM_PLUS
                | SyntaxKind::KW_MIN
                | SyntaxKind::KW_MAX
                | SyntaxKind::NECK
                | SyntaxKind::WEAK_NECK
                | SyntaxKind::KW_MINIMIZE
                | SyntaxKind::KW_MAXIMIZE
                | SyntaxKind::KW_SHOW
                | SyntaxKind::KW_PROJECT
                | SyntaxKind::KW_DEFINED
                | SyntaxKind::KW_EDGE
                | SyntaxKind::KW_HEURISTIC
                | SyntaxKind::KW_EXTERNAL
                | SyntaxKind::KW_CONST
                | SyntaxKind::KW_SCRIPT
                | SyntaxKind::KW_INCLUDE
                | SyntaxKind::KW_PROGRAM
                | SyntaxKind::KW_THEORY
        )
    }

    /// The term at a term entry, under the entry's restriction.
    pub(super) fn term_at_entry(&mut self, entry: EntryPoint) {
        let context = match entry {
            EntryPoint::TermValue => super::terms::TermContext::TermValue,
            EntryPoint::Program | EntryPoint::Statement | EntryPoint::Term => {
                super::terms::TermContext::Term
            }
        };
        self.term(context);
    }
}
