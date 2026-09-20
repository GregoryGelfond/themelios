//! Leaf raising keeps the lexical authority, located diagnostics and iterative
//! parent assembly of docs/design/program.md §8 and §13.

use themelios_base::line::PositionRefusal;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_program::raise::{LowerErrorKind, raise_term};
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{NestingLimit, parse_term};
use themelios_syntax::token::{LexMode, Token, TokenSource, check_token_source_laws};
use themelios_syntax::tree::SyntaxKind;

/// A foreign token source keeps the file lexer's boundaries but supplies one
/// leaf's token kind, as the macro door can. The raise must validate its value.
struct ForeignLeaf<'a> {
    lexer: Lexer<'a>,
    spelling: &'a str,
    kind: SyntaxKind,
}

impl TokenSource for ForeignLeaf<'_> {
    fn id(&self) -> SourceId {
        self.lexer.id()
    }

    fn dialect(&self) -> Dialect {
        self.lexer.dialect()
    }

    fn text(&self) -> &str {
        self.lexer.text()
    }

    fn token_at(&self, at: ByteOffset, mode: LexMode) -> Result<Token<'_>, PositionRefusal> {
        let mut token = self.lexer.token_at(at, mode)?;
        if token.text == self.spelling {
            token.kind = self.kind;
        }
        Ok(token)
    }
}

fn anonymous() -> Term {
    Term::Variable(Variable::Anonymous)
}

/// Root and nested occurrences retain every refusal at its original token,
/// in source order, while the partial term keeps its surrounding structure.
fn refused_leaves(spelling: &str, kind: SyntaxKind, expected: LowerErrorKind) {
    for (text, wanted) in [
        (spelling.to_owned(), anonymous()),
        (
            format!("f({spelling},g({spelling}))"),
            Term::Function {
                name: Name::new("f").unwrap(),
                arguments: vec![
                    anonymous(),
                    Term::Function {
                        name: Name::new("g").unwrap(),
                        arguments: vec![anonymous()],
                    },
                ],
            },
        ),
    ] {
        let source = Source::new(SourceId::new(37), text.clone()).unwrap();
        let foreign = ForeignLeaf {
            lexer: Lexer::new(&source, Dialect::Clingo),
            spelling,
            kind,
        };
        assert!(check_token_source_laws(&foreign).is_empty());
        let parsed = parse_term(&foreign, NestingLimit::DEFAULT);
        assert!(parsed.diagnostics().is_empty());
        let (term, errors) = raise_term(&parsed);
        assert_eq!(term, Some(wanted));
        let offsets: Vec<_> = text
            .match_indices(spelling)
            .map(|(start, _)| start)
            .collect();
        assert_eq!(errors.len(), offsets.len());
        for (error, start) in errors.iter().zip(offsets) {
            assert_eq!(*error.kind(), expected);
            assert_eq!(error.location().source, source.id());
            assert_eq!(error.location().span.start().get() as usize, start);
            assert_eq!(
                error.location().span.end().get() as usize,
                start + spelling.len()
            );
            assert_eq!(source.slice(error.location().span).unwrap(), spelling);
        }
    }
}

#[test]
fn foreign_leaf_spellings_remain_checked() {
    for (spelling, kind) in [
        ("Leaf", SyntaxKind::IDENT),
        ("leaf", SyntaxKind::VARIABLE),
        ("leaf", SyntaxKind::STRING),
    ] {
        refused_leaves(spelling, kind, LowerErrorKind::MalformedToken);
    }
}

#[test]
fn unexpanded_splices_retain_their_locations() {
    refused_leaves("leaf", SyntaxKind::SPLICE, LowerErrorKind::UnexpandedSplice);
}

#[test]
fn deep_parent_assembly_keeps_sibling_leaves() {
    let depth = 120;
    let text = format!("{}X{}", "f(0,".repeat(depth), ")".repeat(depth));
    let source = Source::new(SourceId::new(38), text).unwrap();
    let lexer = Lexer::new(&source, Dialect::Clingo);
    let parsed = parse_term(&lexer, NestingLimit::DEFAULT);
    assert!(parsed.diagnostics().is_empty());
    let (actual, errors) = raise_term(&parsed);
    assert!(errors.is_empty());
    let mut expected = Term::Variable(Variable::Named(VarName::new("X").unwrap()));
    for _ in 0..depth {
        expected = Term::Function {
            name: Name::new("f").unwrap(),
            arguments: vec![Term::Symbolic(Symbol::Number(0)), expected],
        };
    }
    assert_eq!(actual, Some(expected));
}
