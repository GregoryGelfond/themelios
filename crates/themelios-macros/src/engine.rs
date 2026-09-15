//! The engine pipeline (docs/design/macros.md §5, step 2): driving a
//! `MacroSource` through the syntax tier's fragment doors (syntax §6.1) to
//! the typed AST. A construction site fixes one grammatical category — a
//! program, a statement, a term, or a value-term, the closed set of doors
//! the syntax tier realizes — and this module holds one thin door per
//! category, each the matching fragment entry at `NestingLimit::DEFAULT`
//! (128, syntax §6.6; a macro body is human-bounded). The source already
//! carries the `Clingo` dialect (grammar §3) and the string-input source
//! id, so a door adds only that limit.
//!
//! The `Parse` a door yields is the one typed tree the later passes read:
//! the splice-bearing tree the codegen walks (§5, step 4), carrying each
//! splice structurally as an `ast::Term::Splice` or `ast::TheoryTerm::Splice`
//! node (syntax roster), and the tree the fragment's syntax diagnostics
//! come from (§5, step 3). One parse to a fragment — a sub-statement
//! category, the atom a fact assembles around, is reached by parsing a
//! statement and reading its tree (§8), never a bespoke door.
// These doors have no caller outside this module's own tests until the entry
// points wire the engine, so the not-yet-reached surface would read as dead.
// The allow is removed when the entry points arrive.
#![allow(dead_code)]

use themelios_syntax::ast;
use themelios_syntax::parse::{
    NestingLimit, Parse, parse_program, parse_statement, parse_term, parse_term_value,
};

use crate::source::MacroSource;

/// Parses the assembled source as a whole program (syntax §6.1) at
/// `NestingLimit::DEFAULT`.
pub(crate) fn parse_program_fragment(src: &MacroSource) -> Parse<ast::Program> {
    parse_program(src, NestingLimit::DEFAULT)
}

/// Parses the assembled source as one statement position (syntax §6.1) at
/// `NestingLimit::DEFAULT`. A clean statement's terminating `.` is the
/// assembler's to supply (§5); this door reads whatever the source tiles.
pub(crate) fn parse_statement_fragment(src: &MacroSource) -> Parse<ast::StatementFragment> {
    parse_statement(src, NestingLimit::DEFAULT)
}

/// Parses the assembled source as grammar §5.1's `term` (syntax §6.1) at
/// `NestingLimit::DEFAULT`.
pub(crate) fn parse_term_fragment(src: &MacroSource) -> Parse<ast::TermFragment> {
    parse_term(src, NestingLimit::DEFAULT)
}

/// Parses the assembled source as grammar §5.10's `value-term`, under its
/// restriction (syntax §6.1), at `NestingLimit::DEFAULT`.
pub(crate) fn parse_term_value_fragment(src: &MacroSource) -> Parse<ast::TermFragment> {
    parse_term_value(src, NestingLimit::DEFAULT)
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;
    use themelios_syntax::tree::AstNode;

    use super::*;

    /// The macro source `input` assembles to under the dialect — the token
    /// stream a construction site would hand the engine, with no leading
    /// keyword.
    fn source(input: TokenStream) -> MacroSource {
        MacroSource::build(input, None).expect("maps under the dialect")
    }

    #[test]
    fn a_statement_fragment_parses_to_one_statement() {
        // The assembler supplies a clean statement's terminating `.`; the
        // door under test reads a source whose text already carries it.
        let parse = parse_statement_fragment(&source(quote!(p(1).)));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        assert!(parse.tree().statement().is_some());
    }

    #[test]
    fn a_term_fragment_parses_to_one_term() {
        let parse = parse_term_fragment(&source(quote!(f(X))));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        assert!(parse.tree().term().is_some());
    }

    #[test]
    fn a_value_term_fragment_parses_under_its_restriction() {
        let parse = parse_term_value_fragment(&source(quote!(42)));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        assert!(parse.tree().term().is_some());
    }

    #[test]
    fn a_program_fragment_parses_every_statement() {
        let parse = parse_program_fragment(&source(quote!(p(1). q(2).)));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        assert_eq!(parse.tree().statements().count(), 2);
    }

    #[test]
    fn a_splice_parses_to_a_term_splice_node() {
        // A `$`-splice stands where a term may (grammar §9); the parse
        // carries it structurally as `ast::Term::Splice`, the node the
        // codegen crosses to a ground term.
        let parse = parse_statement_fragment(&source(quote!(p($x).)));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        let carries_splice = parse
            .syntax()
            .descendants()
            .filter_map(ast::Term::cast)
            .any(|term| matches!(term, ast::Term::Splice(_)));
        assert!(carries_splice, "the fragment carries a term splice");
    }

    #[test]
    fn a_theory_term_splice_parses_to_a_theory_splice_node() {
        // `$` under the parser's theory mode is the one behaviour a
        // file-lexer differential cannot cover — the file lexer refuses `$`,
        // yet grammar §9 makes a theory-term splice reachable, so `&a { $x }`
        // is expressible. The parse carries it as `ast::TheoryTerm::Splice`,
        // the node the theory-term codegen crosses to a symbolic term.
        let parse = parse_statement_fragment(&source(quote!(&a { $x }.)));
        assert!(!parse.has_errors(), "{:?}", parse.diagnostics());
        let opterm = parse
            .syntax()
            .descendants()
            .find_map(ast::TheoryOpTerm::cast)
            .expect("the theory atom holds an opterm");
        let carries_theory_splice = opterm.items().any(|item| {
            matches!(
                item,
                ast::TheoryOpTermItem::Term(ast::TheoryTerm::Splice(_))
            )
        });
        assert!(
            carries_theory_splice,
            "the opterm carries a theory-term splice"
        );
    }
}
