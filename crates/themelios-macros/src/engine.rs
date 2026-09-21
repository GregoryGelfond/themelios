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
use proc_macro2::{Punct, Spacing, Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use themelios_program::raise::raise_statement;
use themelios_syntax::ast;
use themelios_syntax::parse::{NestingLimit, Parse, parse_statement};

use crate::codegen::{codegen_head_atom, codegen_statement};
use crate::diagnostics::emit_diagnostics;
use crate::source::MacroSource;

/// The value a construction site fixes (docs/design/macros.md §5): what [`run`] codegens
/// from the one statement a construction assembles and parses. The seven statement macros
/// fix [`Entry::Statement`], the whole statement; `atom!` fixes [`Entry::Atom`], the single
/// atom its head wraps (§8). Both reach their value through the one statement door — a
/// construction assembles a statement, never a bespoke sub-statement fragment (§8).
#[derive(Clone, Copy)]
pub(crate) enum Entry {
    /// A single statement (grammar §5.11): the door the statement macros parse through,
    /// each assembling the terminating `.` its fragment needs (§5).
    Statement,
    /// The single atom a fact's head wraps (§8): the `atom!` door. There is no atom
    /// fragment, and the term door reads a leading `-` as arithmetic negation, so `atom!`
    /// assembles a fact and codegens the atom its head wraps — where `-p` is the atom's
    /// positional strong negation, not the term door's arithmetic negation.
    Atom,
}

/// Run the compile-time pipeline for a construction macro (docs/design/macros.md §5): map
/// the Rust token stream to a `MacroSource` under the dialect (§6), prepending the directive
/// `keyword` when one is given and appending the fragment's terminating `.`; parse it as one
/// statement; diagnose it at the rust-analyzer bar over a splice-free view (§5, step 3); and,
/// when it is clean, codegen the §7.1 constructor calls that build the value `entry` names —
/// the whole statement, or the single atom in its head (§5, step 4; §8). A dialect error of
/// the mapping, or a syntax or lowering diagnostic, is a compile error at the offending Rust
/// token's span (§9); the pipeline never panics (§2).
pub(crate) fn run(input: TokenStream, entry: Entry, keyword: Option<&str>) -> TokenStream {
    let source = match MacroSource::build(with_terminator(input), keyword) {
        Ok(source) => source,
        Err(error) => return compile_error(error.span, &error.message),
    };
    // Every construction assembles and parses one statement, diagnoses it at the
    // rust-analyzer bar over a splice-free view (§5, step 3), and — clean — codegens the
    // value its `entry` names (§5, step 4). The build, parse, and diagnostics are the same
    // for a whole statement and for the atom its head wraps; only the final codegen differs.
    let parse = parse_statement_fragment(&source);
    let view = source.splice_free_view();
    let lowering = raise_statement(&parse_statement_fragment(&view)).1;
    if let Some(errors) = emit_diagnostics(&source, parse.diagnostics(), &view, &lowering) {
        // The diagnostics are a sequence of `compile_error!(…);` statements; a construction
        // macro stands in expression position (`let r = fact!(…)`), so they are wrapped in a
        // block — one expression, no stray-`;` noise beside the real diagnostic (§9).
        return quote!({ #errors });
    }
    let Some(statement) = parse.tree().statement() else {
        // A clean parse of a construction carries its statement; a fragment holding none —
        // an empty invocation — is a construction-site error, not a fabricated value (§9).
        // Unreachable once the diagnostics above pass.
        return compile_error(Span::call_site(), "this construction has no statement");
    };
    match entry {
        Entry::Statement => codegen_statement(&statement, &source),
        // `atom!` reaches head position: codegen the single atom the fact's head wraps (§8).
        Entry::Atom => codegen_head_atom(&statement, &source),
    }
}

/// The macro's assembled input with the fragment's terminating `.` appended (§5): a clean
/// statement's terminator is the assembler's to supply, so a caller writes the payload
/// alone. The `.` is a fresh `Alone` punct after the last input token, so the dialect
/// mapping (§6) tiles it as the `DOT` the parser expects, kept apart from a fusing
/// neighbour by the source's own separator discipline.
fn with_terminator(input: TokenStream) -> TokenStream {
    let mut input = input;
    input.extend(std::iter::once(TokenTree::Punct(Punct::new(
        '.',
        Spacing::Alone,
    ))));
    input
}

/// A `compile_error!(message)` at `span` — the engine's own diagnostics (§9): a dialect
/// error of the mapping, or a construction with no statement. `quote_spanned!` stamps
/// `span` on the generated tokens, so the compiler blames the Rust token it came from.
///
/// The `compile_error!(…);` is wrapped in a block, exactly as the diagnostics path wraps
/// its sequence (§9): a construction macro stands in expression position (`let r =
/// fact!(…)`), so a bare `compile_error!(…);` there draws a stray "macro expansion ignores
/// token `;`" error beside the real one. The block makes it one expression — one clean error.
fn compile_error(span: Span, message: &str) -> TokenStream {
    quote_spanned! { span => { compile_error!(#message); } }
}

/// Parses the assembled source as one statement position (syntax §6.1) at
/// `NestingLimit::DEFAULT`. A clean statement's terminating `.` is the
/// assembler's to supply (§5); this door reads whatever the source tiles.
pub(crate) fn parse_statement_fragment(src: &MacroSource) -> Parse<ast::StatementFragment> {
    parse_statement(src, NestingLimit::DEFAULT)
}

// The program-fragment and term doors have no pipeline caller yet — no construction
// macro parses a whole program or a bare term (a statement macro assembles a statement,
// §5) — so they are exercised only by this crate's tests until a construction reaches
// for them. Kept test-scoped meanwhile, rather than carried as a dead public-crate
// surface.
#[cfg(test)]
use themelios_syntax::parse::{parse_program, parse_term, parse_term_value};

/// Parses the assembled source as a whole program (syntax §6.1) at
/// `NestingLimit::DEFAULT`.
#[cfg(test)]
pub(crate) fn parse_program_fragment(src: &MacroSource) -> Parse<ast::Program> {
    parse_program(src, NestingLimit::DEFAULT)
}

/// Parses the assembled source as grammar §5.1's `term` (syntax §6.1) at
/// `NestingLimit::DEFAULT`.
#[cfg(test)]
pub(crate) fn parse_term_fragment(src: &MacroSource) -> Parse<ast::TermFragment> {
    parse_term(src, NestingLimit::DEFAULT)
}

/// Parses the assembled source as grammar §5.10's `value-term`, under its
/// restriction (syntax §6.1), at `NestingLimit::DEFAULT`.
#[cfg(test)]
pub(crate) fn parse_term_value_fragment(src: &MacroSource) -> Parse<ast::TermFragment> {
    parse_term_value(src, NestingLimit::DEFAULT)
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;
    use themelios_syntax::token::TokenSource;
    use themelios_syntax::tree::AstNode;

    use super::*;

    /// The macro source `input` assembles to under the dialect — the token
    /// stream a construction site would hand the engine, with no leading
    /// keyword.
    fn source(input: TokenStream) -> MacroSource {
        MacroSource::build(input, None).expect("maps under the dialect")
    }

    #[test]
    fn run_of_a_clean_statement_emits_the_constructor_calls() {
        // A caller writes the payload alone; `run` appends the terminator, parses,
        // finds it clean, and codegens the constructor calls (§5) — no diagnostic.
        let expansion = run(quote!(p(1, a)), Entry::Statement, None).to_string();
        assert!(expansion.contains("Rule :: fact"), "{expansion}");
        assert!(expansion.contains("Atom :: new"), "{expansion}");
        assert!(!expansion.contains("compile_error"), "{expansion}");
    }

    #[test]
    fn run_of_a_directive_prepends_its_keyword() {
        // A directive macro supplies its `#`-keyword from its own name (§5): `run`
        // hands `Some("show")` to the source, which opens the text with `#show`.
        let expansion = run(quote!(p / 1), Entry::Statement, Some("show")).to_string();
        assert!(expansion.contains("Show :: Signature"), "{expansion}");
    }

    #[test]
    fn run_of_a_dialect_error_is_a_block_expression() {
        // A float literal is no token the dialect names (§6): the mapping refuses it and
        // `run` returns a `compile_error!` at its span, never a panic (§2). Like the
        // lowering path, the engine's own error is wrapped in a block, so it stands in
        // expression position without a stray-`;` secondary beside it (§9).
        let expansion = run(quote!(p(1.5)), Entry::Statement, None).to_string();
        assert!(expansion.contains("compile_error"), "{expansion}");
        assert!(expansion.starts_with('{'), "{expansion}");
    }

    #[test]
    fn run_of_a_lowering_error_is_a_block_expression() {
        // A numeral past the engine's width is a lowering diagnostic (program §8): the
        // `compile_error!` is wrapped in a block, so it stands in expression position (§9).
        let expansion = run(quote!(p(9999999999)), Entry::Statement, None).to_string();
        assert!(expansion.contains("compile_error"), "{expansion}");
        assert!(expansion.starts_with('{'), "{expansion}");
    }

    #[test]
    fn with_terminator_appends_the_dot() {
        // The door reads a source whose text carries the statement terminator, so a
        // caller writes the payload alone (§5).
        let assembled = MacroSource::build(with_terminator(quote!(p(1))), None).expect("maps");
        assert!(assembled.text().ends_with('.'), "{}", assembled.text());
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
