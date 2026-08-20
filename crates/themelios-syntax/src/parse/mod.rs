//! The parser's public face (docs/design/syntax.md §5.5, §6.1): the
//! entry points, `EntryPoint`, and `Parse` — the green tree, the
//! diagnostics, and the facts a consumer needs to interpret both.

mod builder;
mod machine;
mod statements;
mod theory;
mod directives;
mod terms;
#[cfg(test)]
mod test_util;

use std::fmt;
use std::marker::PhantomData;

use themelios_base::diagnostic::Severity;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::Location;

use crate::ast;
use crate::diagnostic::SyntaxError;
use crate::dialect::Dialect;
use crate::lexer::Lexer;
use crate::token::TokenSource;
use crate::tree::{Asp, AstNode, GreenNode, SyntaxNode, TextRange, span_of};

use self::machine::Parser;

/// How deep a parse may nest bracket contexts — frames, one per open
/// bracket (docs/design/syntax.md §6.2) — before it refuses. The grammar
/// nests without bound (grammar §11 D2), so this limit is the
/// implementation's, not the language's; it never crashes, it *refuses*,
/// with a locus. Two named points span the trade (docs/design/syntax.md
/// §6.6): [`DEFAULT`](NestingLimit::DEFAULT), what a bare [`parse`] uses,
/// the crash-averse floor a naive consumer holds on a modest stack; and
/// [`CEILING`](NestingLimit::CEILING), passed to a general door
/// ([`parse_program`] and its siblings), the definition's unbounded
/// nesting honored as far as the crate can prove safe — held under
/// [`with_required_stack`]. A consumer chooses where on that span it
/// parses; it never has to choose a crash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NestingLimit(u32);

impl NestingLimit {
    /// The default limit — the crash-averse floor. The deepest tree it
    /// builds is safe to *hold* — drop, render, compare, navigate — on a
    /// modest two-mebibyte stack (a quarter of the eight-mebibyte
    /// main-thread default of the two supported operating systems, where a
    /// naive consumer's code runs), so [`parse`] does not overflow there.
    /// Every real program nests far below it — the deepest in the whole
    /// vendored corpus, clingo's own tests included, nests twenty-three
    /// (docs/design/syntax.md §6.6) — and deeper input is *refused*, not
    /// crashed. Set well below what the modest stack survives (the depth
    /// gate measures ~323 frames of the deepest shape there, 2026-08-19,
    /// rowan 0.17.0): 128 — echoing serde_json's recursion floor — leaves
    /// better than twofold margin and clears the corpus more than fivefold.
    /// A consumer parsing on a thread it sized smaller manages that stack
    /// itself, as it must for any deep tree; a consumer wanting the full
    /// range reaches for a general door at [`CEILING`](NestingLimit::CEILING).
    pub const DEFAULT: NestingLimit = NestingLimit(128);

    /// The ceiling: the deepest [`REQUIRED_STACK_BYTES`] is proven to hold
    /// (the depth gate, docs/design/syntax.md §16) — the language's
    /// unbounded nesting honored as far as safety allows, no lower. A
    /// consumer that raises a general door ([`parse_program`] and its
    /// siblings) to it holds the result under [`with_required_stack`]. It
    /// grows with the pole: a larger [`REQUIRED_STACK_BYTES`], re-measured,
    /// raises it. Measured 2026-08-19: the walks survive 5,154 frames on
    /// half the 64 MiB pole; 5,000 is the largest granule below.
    pub const CEILING: NestingLimit = NestingLimit(5_000);

    /// The frame count this limit refuses beyond.
    #[must_use]
    pub const fn frames(self) -> u32 {
        self.0
    }
}

impl Default for NestingLimit {
    /// [`DEFAULT`](NestingLimit::DEFAULT).
    fn default() -> NestingLimit {
        NestingLimit::DEFAULT
    }
}

/// The stack, in bytes, on which every operation this crate performs or
/// hands out over the deepest tree it can build — dropping it, comparing
/// two, rendering one, walking the typed AST, attaching, certifying — is
/// proven to complete: the depth gate runs on a thread of exactly this
/// size and passes with headroom (docs/design/syntax.md §6.6). A
/// consumer's thread that holds a tree needs at least this much. Sixty-
/// four mebibytes: eight times the eight-mebibyte main-thread default of
/// the two supported operating systems, a size a language server's
/// worker can be given without contortion; [`NestingLimit::CEILING`] is
/// measured against it, and a move of either re-measures the other.
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;

/// The most node layers one frame contributes to the tree's depth
/// (docs/design/syntax.md §5.4, law 3), by inspection of Appendix A: a
/// function or `@`-call frame is its node, `ARGUMENTS`, and `TUPLE`,
/// then the seven binary levels and the unary run of the operand inside
/// — eleven; a pool contributes ten, an absolute value nine, a theory
/// frame two.
pub const TERM_LAYERS_PER_FRAME: u32 = 11;

/// The layers of the tree that do not depend on nesting: the deepest
/// grammar-bounded path from the root to the first frame — `PROGRAM`,
/// `RULE`, `BODY`, `THEORY_ATOM`, `THEORY_ELEMENTS`, `THEORY_ELEMENT`,
/// `CONDITION`, `LITERAL`, `COMPARISON`, and the frame-free operator
/// chain's eight layers — and the one leaf below the last frame: a
/// constant, a variable, a splice, or the `ERROR` node of a refusal.
/// By inspection of Appendix A (docs/design/syntax.md §5.4, §6.6).
pub const FIXED_LAYERS: u32 = 18;

/// The bound on the tree's depth (docs/design/syntax.md §5.4, law 3),
/// derived and carrying no numeral of its own: [`NestingLimit::CEILING`]
/// frames — the deepest tree the crate can build — each contributing at
/// most `TERM_LAYERS_PER_FRAME` layers, under `FIXED_LAYERS`. Public
/// because a consumer who recurses over the typed AST sizes its own stack
/// from it; `REQUIRED_STACK_BYTES` covers this crate's and rowan's walks,
/// not the consumer's.
pub const MAX_TREE_DEPTH: u32 =
    NestingLimit::CEILING.frames() * TERM_LAYERS_PER_FRAME + FIXED_LAYERS;

/// What the parser is asked to read: a whole program, or one construct
/// family with a named consumer — the statement (the macro tier's
/// statement macros), the term (the macro tier's term positions), and
/// the term-value sublanguage (grammar §5.10: what a string parses to
/// when a caller asks for a symbol — the REPL and the query surface).
/// The REPL is not the statement entry's consumer: it parses a growing
/// buffer through the program entry and reads `is_incomplete`. Closed;
/// a family is admitted here when a consumer names it, and the addition
/// is a breaking one, priced by the pre-1.0 posture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntryPoint {
    /// Grammar §5.11's `program` under the dialect, with the aspif dispatch.
    Program,
    /// One program position: leading docs, one statement with its
    /// annotation, or the ASP-Core-2 query.
    Statement,
    /// Grammar §5.1's `term`.
    Term,
    /// Grammar §5.10's `value-term`, under its restriction.
    TermValue,
}

/// The result of a parse: the green tree, the diagnostics, and the
/// facts a consumer needs to interpret both. Owned, `Send + Sync`,
/// cheap to clone (the tree is reference-counted). `T` is the typed
/// root the entry point yields — a view type, `!Send`, so it is carried
/// as `PhantomData<fn() -> T>`: a phantom that names `T` without
/// inheriting its auto-traits; `Clone`, `PartialEq`, `Eq`, and `Debug`
/// are implemented without a bound on `T`.
pub struct Parse<T: AstNode<Language = Asp>> {
    green: GreenNode,
    diagnostics: Vec<SyntaxError>,
    source: SourceId,
    dialect: Dialect,
    entry: EntryPoint,
    _root: PhantomData<fn() -> T>,
}

impl<T: AstNode<Language = Asp>> Parse<T> {
    pub(crate) fn new(
        green: GreenNode,
        diagnostics: Vec<SyntaxError>,
        source: SourceId,
        dialect: Dialect,
        entry: EntryPoint,
    ) -> Parse<T> {
        Parse {
            green,
            diagnostics,
            source,
            dialect,
            entry,
            _root: PhantomData,
        }
    }

    /// A fresh root cursor over the tree — a view, minted on demand.
    /// Total, O(1).
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The typed root. Total: every entry point yields a root of the
    /// kind its `T` casts, so this never fails. O(1).
    pub fn tree(&self) -> T {
        T::cast(self.syntax()).expect("every entry point yields a root of the kind its T casts")
    }

    /// The green tree, the `Send + Sync` model. Total, O(1).
    pub fn green(&self) -> &GreenNode {
        &self.green
    }

    /// The diagnostics in the order the parser produced them — one
    /// order, by the determinism law; a batch consumer that wants the
    /// shared batch order sorts by base's `canonical_order` after
    /// lowering. Total, O(1).
    pub fn diagnostics(&self) -> &[SyntaxError] {
        &self.diagnostics
    }

    /// Any diagnostic of `Severity::Error`. Membership in the language
    /// (grammar §2) is exactly `!has_errors()`. Total, O(diagnostics).
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity() == Severity::Error)
    }

    /// The input ended before the construct did, and that is the only
    /// kind of error present — the REPL's "read more" signal
    /// (docs/design/syntax.md §6.5). Total, O(diagnostics).
    pub fn is_incomplete(&self) -> bool {
        self.has_errors()
            && self.diagnostics.iter().all(|d| {
                d.severity() != Severity::Error || d.kind().is_incompleteness(self.dialect)
            })
    }

    /// The identity of the source parsed. Total, O(1).
    pub fn source(&self) -> SourceId {
        self.source
    }

    /// The dialect parsed under. Total, O(1).
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// The entry point parsed through. Total, O(1).
    pub fn entry(&self) -> EntryPoint {
        self.entry
    }

    /// The qualified location of an element of this tree (base §4.3):
    /// its range under this parse's source id. Total, O(1).
    pub fn location(&self, range: TextRange) -> Location {
        Location {
            source: self.source,
            span: span_of(range),
        }
    }

    /// The denoted text of a string literal, under this parse's dialect
    /// — the door that cannot be handed the wrong one
    /// (docs/design/syntax.md §3). Refuses as `StringLit::value` does: a
    /// spelling that is not the dialect's rule, which only a foreign
    /// token source can supply. O(token).
    pub fn string_value(
        &self,
        literal: &ast::StringLit,
    ) -> Result<String, ast::InvalidStringLiteral> {
        literal.value(self.dialect)
    }
}

impl<T: AstNode<Language = Asp>> Clone for Parse<T> {
    fn clone(&self) -> Self {
        Parse {
            green: self.green.clone(),
            diagnostics: self.diagnostics.clone(),
            source: self.source,
            dialect: self.dialect,
            entry: self.entry,
            _root: PhantomData,
        }
    }
}

impl<T: AstNode<Language = Asp>> PartialEq for Parse<T> {
    /// Structural through the green tree; the diagnostics, dialect, and
    /// identity as plain data — what the determinism law is checked
    /// with.
    fn eq(&self, other: &Self) -> bool {
        self.green == other.green
            && self.diagnostics == other.diagnostics
            && self.source == other.source
            && self.dialect == other.dialect
            && self.entry == other.entry
    }
}

impl<T: AstNode<Language = Asp>> Eq for Parse<T> {}

impl<T: AstNode<Language = Asp>> fmt::Debug for Parse<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parse")
            .field("green", &self.green)
            .field("diagnostics", &self.diagnostics)
            .field("source", &self.source)
            .field("dialect", &self.dialect)
            .field("entry", &self.entry)
            .finish()
    }
}

/// The file door: an admitted source under a dialect, at
/// [`NestingLimit::DEFAULT`] — the crash-averse floor, safe to hold on a
/// modest stack. For the deeper [`NestingLimit::CEILING`], reach for
/// [`parse_program`] under [`with_required_stack`]. Total; O(text).
pub fn parse(source: &Source, dialect: Dialect) -> Parse<ast::Program> {
    parse_program(&Lexer::new(source, dialect), NestingLimit::DEFAULT)
}

/// The general door for a program: any token source, at `limit`
/// (docs/design/syntax.md §6.6 — [`NestingLimit::DEFAULT`] holds on a
/// modest stack; [`NestingLimit::CEILING`] wants [`with_required_stack`]).
/// Total; O(text).
pub fn parse_program(source: &impl TokenSource, limit: NestingLimit) -> Parse<ast::Program> {
    Parser::new(source, limit).program()
}

/// The statement door: one program position, at `limit` (see
/// [`parse_program`]). Total; O(text).
pub fn parse_statement(
    source: &impl TokenSource,
    limit: NestingLimit,
) -> Parse<ast::StatementFragment> {
    Parser::new(source, limit).statement_fragment()
}

/// The term door: grammar §5.1's `term`, at `limit` (see
/// [`parse_program`]). Total; O(text).
pub fn parse_term(source: &impl TokenSource, limit: NestingLimit) -> Parse<ast::TermFragment> {
    Parser::new(source, limit).term_fragment(EntryPoint::Term)
}

/// The term-value door: grammar §5.10's `value-term`, at `limit` (see
/// [`parse_program`]). Total; O(text).
pub fn parse_term_value(
    source: &impl TokenSource,
    limit: NestingLimit,
) -> Parse<ast::TermFragment> {
    Parser::new(source, limit).term_fragment(EntryPoint::TermValue)
}

/// Run `work` on a fresh thread of `REQUIRED_STACK_BYTES` and return what
/// it produces. Parsing never needs this — the parser is iterative — but
/// every operation on the *result* that recurses in the tree's depth does:
/// dropping the tree, rendering it through `Display`, comparing two, or
/// navigating one by offset (docs/design/syntax.md §6.6, §14). A consumer
/// that holds a deeply nested parse runs that work here rather than on a
/// thread whose stack the tree's depth can exhaust — the ergonomic form of
/// the requirement `REQUIRED_STACK_BYTES` states, so a language server's
/// worker or a WASM host need not hand-roll the thread. `work` may borrow
/// from its caller; the thread joins before this returns.
///
/// ```
/// use themelios_syntax::base::source::{Source, SourceId};
/// use themelios_syntax::dialect::Dialect;
/// use themelios_syntax::parse::{parse, with_required_stack};
///
/// let source = Source::new(SourceId::new(0), "p(f(g(1))).".to_owned()).unwrap();
/// let member = with_required_stack(|| !parse(&source, Dialect::Clingo).has_errors());
/// assert!(member);
/// ```
pub fn with_required_stack<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(REQUIRED_STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("a thread of the required stack spawns")
            .join()
            .expect("the work on the required stack completes")
    })
}

#[cfg(test)]
mod tests {
    use themelios_base::diagnostic::Severity;
    use themelios_base::line::PositionRefusal;
    use themelios_base::source::{Source, SourceId};
    use themelios_base::span::ByteOffset;

    use super::*;
    use crate::diagnostic::{Expected, MisplacedDoc, SourceBreach, SyntaxClass, SyntaxErrorKind};
    use crate::token::{LexMode, Token, TokenSource};
    use crate::tree::SyntaxKind;

    fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(7), text.to_owned()).expect("test text admits")
    }

    /// The tree's shape as `KIND@start..end` lines, indented by depth —
    /// rowan's alternate `Debug`.
    fn dump<T: AstNode<Language = Asp>>(parse: &Parse<T>) -> String {
        format!("{:#?}", parse.syntax())
    }

    #[test]
    fn an_empty_text_is_an_empty_program() {
        let source = admitted("");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), "");
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
        assert!(!parse.has_errors());
        assert!(!parse.is_incomplete());
        assert!(parse.diagnostics().is_empty());
        assert_eq!(parse.entry(), EntryPoint::Program);
        assert_eq!(parse.dialect(), Dialect::Clingo);
        assert_eq!(parse.source(), SourceId::new(7));
    }

    #[test]
    fn trivia_alone_belongs_to_the_program() {
        let source = admitted("  % a comment\n%* block *%\n#! shebang\n");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert!(!parse.has_errors());
        assert_eq!(parse.syntax().children().count(), 0);
        assert_eq!(parse.syntax().children_with_tokens().count(), 7);
    }

    #[test]
    fn garbage_is_carried_losslessly_in_error_nodes_with_the_lexical_diagnostics() {
        let source = admitted("$$$ p. ééé");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert!(parse.has_errors());
        let ids: Vec<String> = parse
            .diagnostics()
            .iter()
            .map(|d| d.id().to_string())
            .collect();
        assert!(
            ids.iter().all(|id| id == "syntax::unexpected-characters"),
            "{ids:?}"
        );
        assert_eq!(
            ids.len(),
            2,
            "one lexical diagnostic per ERROR token placed"
        );
        assert!(
            parse
                .syntax()
                .children()
                .all(|node| node.kind() == SyntaxKind::ERROR)
        );
    }

    #[test]
    fn a_doc_run_that_no_statement_follows_is_diagnosed_trivia() {
        let source = admitted("%! a\n%! b\n");
        let parse = parse(&source, Dialect::Clingo);
        assert!(!parse.has_errors(), "warnings do not affect membership");
        let kinds: Vec<_> = parse
            .diagnostics()
            .iter()
            .map(SyntaxError::kind)
            .cloned()
            .collect();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.iter().all(|kind| matches!(
            kind,
            SyntaxErrorKind::MisplacedDocComment {
                reason: MisplacedDoc::NoStatementFollows
            }
        )));
        assert!(
            parse
                .diagnostics()
                .iter()
                .all(|d| d.severity() == Severity::Warning)
        );
    }

    #[test]
    fn aspif_input_is_one_error_child_with_one_diagnostic() {
        let source = admitted("asp 1 0 0\n1 0 1 1 0 0\n0\n");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(parse.syntax().text(), source.text());
        assert_eq!(parse.diagnostics().len(), 1);
        assert_eq!(
            parse.diagnostics()[0].id().to_string(),
            "syntax::aspif-input"
        );
        assert_eq!(parse.syntax().children_with_tokens().count(), 1);
        assert_eq!(dump(&parse).lines().count(), 2);
    }

    #[test]
    fn a_source_that_breaches_tiling_stops_the_parse_with_its_diagnostic() {
        struct EarlyEnd<'a>(Lexer<'a>);
        impl TokenSource for EarlyEnd<'_> {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(
                &self,
                at: ByteOffset,
                mode: LexMode,
            ) -> Result<Token<'_>, PositionRefusal> {
                if at.get() >= 3 {
                    return Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    });
                }
                self.0.token_at(at, mode)
            }
        }
        let source = admitted("$$$ more");
        let parse = parse_program(
            &EarlyEnd(Lexer::new(&source, Dialect::Clingo)),
            NestingLimit::DEFAULT,
        );
        assert_eq!(parse.syntax().text(), "$$$", "the prefix tiled");
        assert!(parse.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Tiling { at, token: SyntaxKind::EOF, len: 0 } }
                if *at == ByteOffset::new(3)
        )));
    }

    #[test]
    fn a_source_that_refuses_a_position_stops_the_parse_with_its_diagnostic() {
        struct Refusing<'a>(Lexer<'a>);
        impl TokenSource for Refusing<'_> {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(
                &self,
                at: ByteOffset,
                mode: LexMode,
            ) -> Result<Token<'_>, PositionRefusal> {
                if at.get() >= 3 {
                    return Err(PositionRefusal::NotCharBoundary(
                        themelios_base::source::NotCharBoundary { offset: at },
                    ));
                }
                self.0.token_at(at, mode)
            }
        }
        let source = admitted("$$$ more");
        let parse = parse_program(
            &Refusing(Lexer::new(&source, Dialect::Clingo)),
            NestingLimit::DEFAULT,
        );
        assert_eq!(parse.syntax().text(), "$$$");
        assert!(parse.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::TokenSourceBreach { breach: SourceBreach::Refusal { at } } if *at == ByteOffset::new(3)
        )));
    }

    #[test]
    fn an_unlawful_token_length_landing_mid_character_does_not_panic() {
        // The slice law is trusted, not checked (§4.3); a foreign source may
        // return a token whose byte length lands inside a character. Every
        // entry point stays total on any input, unlawful sources included
        // (§13): the diagnostic site reads the following character by `get`,
        // so the parse yields a tree rather than panicking.
        struct MidChar<'a>(Lexer<'a>);
        impl TokenSource for MidChar<'_> {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(
                &self,
                at: ByteOffset,
                _mode: LexMode,
            ) -> Result<Token<'_>, PositionRefusal> {
                if at.get() == 0 {
                    // Length one over "é" (two bytes): `at + len` is byte 1,
                    // inside the character — within tiling bounds, but a
                    // slice-law violation the parser does not check.
                    Ok(Token {
                        kind: SyntaxKind::ERROR,
                        text: "x",
                    })
                } else {
                    Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    })
                }
            }
        }
        let source = admitted("é");
        let parse = parse_program(
            &MidChar(Lexer::new(&source, Dialect::Clingo)),
            NestingLimit::DEFAULT,
        );
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
    }

    #[test]
    fn the_fragment_entries_yield_their_container_roots_on_empty_input() {
        let source = admitted("  ");
        let statement =
            parse_statement(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert_eq!(statement.syntax().kind(), SyntaxKind::STATEMENT_FRAGMENT);
        assert_eq!(statement.syntax().text(), "  ");
        assert!(!statement.has_errors());
        assert_eq!(statement.entry(), EntryPoint::Statement);
        let term = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert_eq!(term.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(term.entry(), EntryPoint::Term);
        let value = parse_term_value(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert_eq!(value.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(value.entry(), EntryPoint::TermValue);
    }

    #[test]
    fn input_after_a_fragment_is_an_error_node_expecting_end_of_input() {
        let source = admitted("p q");
        let fragment = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert_eq!(fragment.syntax().text(), "p q");
        assert!(fragment.has_errors());
        assert!(fragment.diagnostics().iter().any(|d| matches!(
            d.kind(),
            SyntaxErrorKind::UnexpectedToken { expected, .. }
                if expected.contains(&Expected::Class(SyntaxClass::EndOfInput))
        )));
    }

    #[test]
    fn a_parse_is_plain_data_that_clones_and_compares_structurally() {
        fn plain<T: Send + Sync>(_: &T) {}
        let source = admitted("$");
        let one = parse(&source, Dialect::Clingo);
        let two = parse(&source, Dialect::Clingo);
        assert_eq!(one, two);
        assert_eq!(one.clone(), one);
        assert_eq!(
            one.location(one.syntax().text_range()).source,
            SourceId::new(7)
        );
        plain(&one);
    }

    #[test]
    fn equality_holds_every_field_apart() {
        // `eq` is the conjunction of five fields (§6.8's determinism law is
        // checked with it); each `&&` is load-bearing, so a parse differing in
        // exactly one field is unequal. Green, source, and dialect are varied
        // one at a time here — entry cannot be isolated (it fixes the root
        // kind, hence the green tree).
        let clingo = |id: u32, text: &str| {
            parse(
                &Source::new(SourceId::new(id), text.to_owned()).expect("admits"),
                Dialect::Clingo,
            )
        };
        // Green apart: same source, dialect, entry; a different tree.
        assert_ne!(clingo(1, "p."), clingo(1, "q."));
        // Source apart: same tree, dialect, entry; a different source id.
        assert_ne!(clingo(1, "p."), clingo(2, "p."));
        // Dialect apart: `p.` is dialect-neutral, so the trees are equal; only
        // the dialect differs.
        let neutral = Source::new(SourceId::new(3), "p.".to_owned()).expect("admits");
        assert_ne!(
            parse(&neutral, Dialect::Clingo),
            parse(&neutral, Dialect::AspCore2)
        );
        // Diagnostics apart, tree equal: a bare doc run and the same text as a
        // fact carry the same tree shape only when they do not; here the doc
        // run warns and the plain comment does not, over different trees — so
        // the determinism law's own repeated-parse equality (same everything)
        // is the direct witness the fields all match.
        assert_eq!(clingo(4, "%! d\np."), clingo(4, "%! d\np."));
    }

    #[test]
    fn the_dialect_accessor_reports_the_dialect_parsed_under() {
        // Not a constant: a parse under ASP-Core-2 reports ASP-Core-2, never
        // the default.
        let parse = parse(&admitted("p."), Dialect::AspCore2);
        assert_eq!(parse.dialect(), Dialect::AspCore2);
    }

    #[test]
    fn an_unterminated_string_is_incomplete_only_where_a_string_may_span_lines() {
        // §6.5: under ASP-Core-2 a string may span lines, so an unterminated
        // one is the REPL's "read more"; under clingo it may not, so the same
        // bytes are a wrong program. The dialect is what decides it.
        let source = admitted("p(\"abc");
        assert!(parse(&source, Dialect::AspCore2).is_incomplete());
        let clingo = parse(&source, Dialect::Clingo);
        assert!(clingo.has_errors() && !clingo.is_incomplete());
    }

    #[test]
    fn the_tree_depth_bound_sums_the_frame_layers_over_the_fixed_ones() {
        // MAX_TREE_DEPTH is CEILING frames at TERM_LAYERS_PER_FRAME each, above
        // FIXED_LAYERS — a sum, not a product (§5.4 law 3).
        assert_eq!(
            MAX_TREE_DEPTH,
            NestingLimit::CEILING.frames() * TERM_LAYERS_PER_FRAME + FIXED_LAYERS
        );
        assert_eq!(MAX_TREE_DEPTH, 55_018);
    }

    #[test]
    fn the_nesting_refusal_points_at_the_offending_opener() {
        // The refusal's primary span is the bracket that would open one frame
        // too many — a real one-byte locus (opener start to start-plus-length),
        // not an empty span (§6.6).
        let deep = format!("p({}x{}).", "f(".repeat(200), ")".repeat(200));
        let source = admitted(&deep);
        let parse = parse(&source, Dialect::Clingo);
        let refusal = parse
            .diagnostics()
            .iter()
            .find(|d| d.id().name() == "nesting-too-deep")
            .expect("a refusal");
        let span = refusal.primary().span;
        assert!(!span.is_empty());
        assert_eq!(
            span.end().get() - span.start().get(),
            1,
            "the one-byte opener"
        );
    }

    #[test]
    fn a_bracket_group_after_a_recovered_statement_is_skipped_with_its_brackets_balanced() {
        // `skip_statement_rest` recovers to the dot, then skips a following
        // `[…]` annotation whole — the inner `[b]` stays inside the group, the
        // depth counter balancing it.
        let source = admitted("$$$. [a[b]] q.");
        let parse = parse(&source, Dialect::Clingo);
        assert_eq!(
            crate::tree::sexpr(&parse.syntax()),
            "(PROGRAM (ERROR $$$ . [ a [ b ] ]) (RULE (LITERAL (ATOM q)) .))"
        );
    }

    #[test]
    fn a_token_running_past_the_texts_end_breaches_tiling_rather_than_being_accepted() {
        // A source that answers a token longer than the text remaining breaches
        // the tiling law; the parser witnesses it — either a zero length or an
        // overrun ends tiling — and stops, rather than accepting it (§4.3).
        struct Overrun(Source);
        impl TokenSource for Overrun {
            fn id(&self) -> SourceId {
                self.0.id()
            }
            fn dialect(&self) -> Dialect {
                Dialect::Clingo
            }
            fn text(&self) -> &str {
                self.0.text()
            }
            fn token_at(
                &self,
                at: ByteOffset,
                _mode: LexMode,
            ) -> Result<Token<'_>, PositionRefusal> {
                if at.get() == 0 {
                    // Two bytes over a one-byte text: `at + len` runs past the
                    // end — the overrun half of the tiling check.
                    Ok(Token {
                        kind: SyntaxKind::IDENT,
                        text: "pp",
                    })
                } else {
                    // Reached only if the mutant accepts the overrun and walks
                    // past the end.
                    Ok(Token {
                        kind: SyntaxKind::EOF,
                        text: "",
                    })
                }
            }
        }
        let source = Source::new(SourceId::new(0), "p".to_owned()).expect("admits");
        let parse = parse_program(&Overrun(source), NestingLimit::DEFAULT);
        assert!(
            parse.diagnostics().iter().any(|d| matches!(
                d.kind(),
                SyntaxErrorKind::TokenSourceBreach {
                    breach: SourceBreach::Tiling {
                        token: SyntaxKind::IDENT,
                        len: 2,
                        ..
                    }
                }
            )),
            "{:?}",
            parse.diagnostics()
        );
    }
}
