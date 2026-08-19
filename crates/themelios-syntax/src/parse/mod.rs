//! The parser's public face (docs/design/syntax.md §5.5, §6.1): the
//! entry points, `EntryPoint`, and `Parse` — the green tree, the
//! diagnostics, and the facts a consumer needs to interpret both.

mod machine;
mod statements;
mod theory;
mod directives;
mod terms;

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

/// The deepest nesting of bracket contexts — frames, one per open
/// bracket (docs/design/syntax.md §6.2) — the parser will open. Named
/// because it carries meaning; its value is fixed by measurement between
/// two bounds and recorded here with both. **Provisional:** this value
/// stands until the depth gate measures the constant (the stage-2 plan's
/// Task 18), which replaces it and records the two bounds beside it.
pub const MAX_NESTING_DEPTH: u32 = 1_000;

/// The stack, in bytes, on which every operation this crate performs or
/// hands out over the deepest tree it can build — dropping it, comparing
/// two, rendering one, walking the typed AST, attaching, certifying — is
/// proven to complete: the depth gate runs on a thread of exactly this
/// size and passes with headroom (docs/design/syntax.md §6.6). A
/// consumer's thread that holds a tree needs at least this much. Sixty-
/// four mebibytes: eight times the eight-mebibyte main-thread default of
/// the two supported operating systems, a size a language server's
/// worker can be given without contortion; `MAX_NESTING_DEPTH` is
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
/// derived and carrying no numeral of its own: `MAX_NESTING_DEPTH`
/// frames, each contributing at most `TERM_LAYERS_PER_FRAME` layers,
/// under `FIXED_LAYERS`. Public because a consumer who recurses over the
/// typed AST sizes its own stack from it; `REQUIRED_STACK_BYTES` covers
/// this crate's and rowan's walks, not the consumer's.
pub const MAX_TREE_DEPTH: u32 = MAX_NESTING_DEPTH * TERM_LAYERS_PER_FRAME + FIXED_LAYERS;

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

/// The file door: an admitted source under a dialect. Total; O(text).
pub fn parse(source: &Source, dialect: Dialect) -> Parse<ast::Program> {
    parse_program(&Lexer::new(source, dialect))
}

/// The general door for a program: any token source. Total; O(text).
pub fn parse_program(source: &impl TokenSource) -> Parse<ast::Program> {
    Parser::new(source).program()
}

/// The statement door: one program position. Total; O(text).
pub fn parse_statement(source: &impl TokenSource) -> Parse<ast::StatementFragment> {
    Parser::new(source).statement_fragment()
}

/// The term door: grammar §5.1's `term`. Total; O(text).
pub fn parse_term(source: &impl TokenSource) -> Parse<ast::TermFragment> {
    Parser::new(source).term_fragment(EntryPoint::Term)
}

/// The term-value door: grammar §5.10's `value-term`. Total; O(text).
pub fn parse_term_value(source: &impl TokenSource) -> Parse<ast::TermFragment> {
    Parser::new(source).term_fragment(EntryPoint::TermValue)
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
        let parse = parse_program(&EarlyEnd(Lexer::new(&source, Dialect::Clingo)));
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
        let parse = parse_program(&Refusing(Lexer::new(&source, Dialect::Clingo)));
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
        let parse = parse_program(&MidChar(Lexer::new(&source, Dialect::Clingo)));
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
    }

    #[test]
    fn the_fragment_entries_yield_their_container_roots_on_empty_input() {
        let source = admitted("  ");
        let statement = parse_statement(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(statement.syntax().kind(), SyntaxKind::STATEMENT_FRAGMENT);
        assert_eq!(statement.syntax().text(), "  ");
        assert!(!statement.has_errors());
        assert_eq!(statement.entry(), EntryPoint::Statement);
        let term = parse_term(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(term.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(term.entry(), EntryPoint::Term);
        let value = parse_term_value(&Lexer::new(&source, Dialect::Clingo));
        assert_eq!(value.syntax().kind(), SyntaxKind::TERM_FRAGMENT);
        assert_eq!(value.entry(), EntryPoint::TermValue);
    }

    #[test]
    fn input_after_a_fragment_is_an_error_node_expecting_end_of_input() {
        let source = admitted("p q");
        let fragment = parse_term(&Lexer::new(&source, Dialect::Clingo));
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
}
