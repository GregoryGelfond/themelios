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
use themelios_program::raise::{raise, raise_statement};
use themelios_syntax::ast;
use themelios_syntax::parse::{NestingLimit, Parse, parse_program, parse_statement};

use crate::codegen::{codegen_head_atom, codegen_program, codegen_statement};
use crate::diagnostics::emit_diagnostics;
use crate::source::MacroSource;

/// The value a construction site fixes (docs/design/macros.md §5): what [`run`] parses and
/// codegens. The seven statement macros fix [`Entry::Statement`], the whole statement; `atom!`
/// fixes [`Entry::Atom`], the single atom its head wraps (§8) — both through the one statement
/// door, a construction assembling a statement, never a bespoke sub-statement fragment.
/// `program!` fixes [`Entry::Program`], a whole-program block through the program door, which
/// the statement entries' shared pipeline is not (§8).
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
    /// A whole-program block (grammar §5.11): the `program!` door. The block carries its own
    /// statement terminators and names no directive keyword, so it is assembled verbatim and
    /// parsed through the program fragment — not the statement pipeline the other entries
    /// share — and raised whole for its lowering diagnostics (§8).
    Program,
}

/// Run the compile-time pipeline for a construction macro (docs/design/macros.md §5): map the
/// Rust token stream to a `MacroSource` under the dialect (§6), parse it through the door
/// `entry` fixes, diagnose it at the rust-analyzer bar over a splice-free view (§5, step 3),
/// and — clean — codegen the §7.1 constructor calls that build the value it names (§5, step 4;
/// §8). The statement entries share one pipeline ([`run_statement`]); `program!` parses a
/// whole block through the program door ([`run_program`]). A dialect error of the mapping, or
/// a syntax or lowering diagnostic, is a compile error at the offending Rust token's span
/// (§9); the pipeline never panics (§2).
pub(crate) fn run(input: TokenStream, entry: Entry, keyword: Option<&str>) -> TokenStream {
    match entry {
        // The statement doors share one pipeline; only the final codegen differs (§5, §8).
        Entry::Statement | Entry::Atom => run_statement(input, entry, keyword),
        // A whole-program block parses through the program door, not the statement one (§8).
        Entry::Program => run_program(input),
    }
}

/// The statement pipeline the seven statement macros and `atom!` share (docs/design/macros.md
/// §5): assemble the payload with the directive `keyword` when one is given and the fragment's
/// terminating `.` appended, parse it as one statement, diagnose it over a splice-free view
/// (§5, step 3), and — clean — codegen the whole statement, or the single atom its head wraps
/// for [`Entry::Atom`] (§5, step 4; §8). Not reached for [`Entry::Program`], which `run` sends
/// to [`run_program`].
fn run_statement(input: TokenStream, entry: Entry, keyword: Option<&str>) -> TokenStream {
    let source = match MacroSource::build(with_terminator(input), keyword) {
        Ok(source) => source,
        Err(error) => return compile_error(error.span, &error.message),
    };
    // The build, parse, and diagnostics are the same for a whole statement and for the atom
    // its head wraps; only the final codegen differs.
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
    // `Statement` builds the whole statement; `Atom` the single atom its head wraps (§8).
    if matches!(entry, Entry::Atom) {
        codegen_head_atom(&statement, &source)
    } else {
        codegen_statement(&statement, &source)
    }
}

/// The `program!` pipeline (docs/design/macros.md §8): a whole-program block, which — unlike
/// the statement doors — carries its own statement terminators and names no directive keyword,
/// so it is assembled verbatim (no prepended keyword, no appended `.`) and parsed through the
/// program fragment door. A `#script` body cannot be recovered from Rust tokens (§7), so it is
/// refused at the macro site before assembly, keeping the assembled text free of a `#script`
/// the parser would meet in script-body mode (grammar §4.8). The block is raised whole for its
/// lowering diagnostics (the program door's `raise`, program §8), diagnosed over a splice-free
/// view like the statement pipeline, and — clean — codegens each statement into `Program::of`
/// (§8). Never panics (§2).
fn run_program(input: TokenStream) -> TokenStream {
    if let Some(span) = script_keyword_span(input.clone()) {
        // The one directive a block cannot carry: a `#script`'s body is opaque Rust-side,
        // recoverable only from a file (§7). Refuse it here — before `MacroSource::build`, so
        // the assembled text never holds a `#script` and the parser never requests script-body
        // mode — with the block-wrapped own-error, clean in expression position (§9).
        return compile_error(
            span,
            "a #script body cannot be recovered from Rust tokens; raise a file — macros §7",
        );
    }
    let source = match MacroSource::build(input, None) {
        Ok(source) => source,
        Err(error) => return compile_error(error.span, &error.message),
    };
    let parse = parse_program_fragment(&source);
    let view = source.splice_free_view();
    // The whole program is raised over the view for its lowering diagnostics — `raise`, the
    // program door (program §8), not `raise_statement` — its `Raised` read for the batch.
    let raised = raise(&parse_program_fragment(&view));
    if let Some(errors) =
        emit_diagnostics(&source, parse.diagnostics(), &view, raised.diagnostics())
    {
        return quote!({ #errors });
    }
    let program = parse.tree();
    codegen_program(&program, &source)
}

/// The span of a `#script` directive keyword in `input` (grammar §4.8), if one is present — a
/// `#` punct immediately followed by the identifier `script`, at any nesting depth. A
/// `program!` block refuses such a body before assembly (§7), so this recognition is
/// deliberately independent of the byte-range span adjacency `map_hash` reads: a detached
/// `# script` — which real proc-macro expansion's inexact byte ranges may still map to
/// `#script` — is caught the same, the imprecision erring toward recognising the keyword, so
/// no spelling the mapping would tile as a `#script` slips past the refusal.
fn script_keyword_span(input: TokenStream) -> Option<Span> {
    let trees: Vec<TokenTree> = input.into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                if let Some(TokenTree::Ident(word)) = trees.get(index + 1)
                    && word == "script"
                {
                    return Some(punct.span());
                }
            }
            TokenTree::Group(group) => {
                if let Some(span) = script_keyword_span(group.stream()) {
                    return Some(span);
                }
            }
            _ => {}
        }
    }
    None
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

/// Parses the assembled source as a whole program (syntax §6.1) at
/// `NestingLimit::DEFAULT` — the door `program!` parses a block through (§8). A block
/// supplies its own statement terminators (§5), so this door reads whatever the source tiles.
pub(crate) fn parse_program_fragment(src: &MacroSource) -> Parse<ast::Program> {
    parse_program(src, NestingLimit::DEFAULT)
}

// The term doors have no pipeline caller — no construction macro parses a bare term (a
// statement macro assembles a statement, `atom!` a fact, §5) — so they are exercised only by
// this crate's tests, kept test-scoped rather than carried as a dead public-crate surface
// until a construction reaches for them.
#[cfg(test)]
use themelios_syntax::parse::{parse_term, parse_term_value};

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
    use std::str::FromStr;

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
    fn run_of_a_program_block_assembles_program_of() {
        // The program entry parses the whole block through the program door and codegens each
        // statement into `Program::of` — no keyword, no appended terminator, the block
        // carrying its own `.`s (§8).
        let expansion = run(
            TokenStream::from_str("p(1). q(2).").expect("lexes"),
            Entry::Program,
            None,
        )
        .to_string();
        assert!(expansion.contains("Program :: of"), "{expansion}");
        assert!(expansion.contains("Statement :: from"), "{expansion}");
        assert!(!expansion.contains("compile_error"), "{expansion}");
    }

    #[test]
    fn run_of_an_empty_program_block_assembles_program_empty() {
        // An empty block is the named empty program, not `Program::of([])` (program §7.1).
        let expansion = run(TokenStream::new(), Entry::Program, None).to_string();
        assert!(expansion.contains("Program :: empty"), "{expansion}");
    }

    #[test]
    fn a_program_refuses_a_script_body_before_assembly() {
        // A `#script` body cannot be recovered from Rust tokens (§7); the program entry
        // refuses it at the macro site with a located compile error, before the source is
        // assembled — so the assembled text never carries a `#script` and the parser never
        // requests script-body mode (grammar §4.8; closing the ScriptBody-unreachability
        // note). The refusal is independent of span adjacency: both an adjacent `#script` and
        // a detached `# script` are caught, the byte-range imprecision (Rust-side, under real
        // expansion) erring toward recognising the keyword.
        for input in ["#script (python) a #end", "# script (python) a #end"] {
            let expansion = run(
                TokenStream::from_str(input).expect("lexes"),
                Entry::Program,
                None,
            )
            .to_string();
            assert!(expansion.contains("compile_error"), "{input}: {expansion}");
            assert!(expansion.contains("script body"), "{input}: {expansion}");
        }
    }

    #[test]
    fn a_program_refuses_a_script_keyword_nested_in_a_group() {
        // The refusal scans into groups, so a `#script` anywhere in the block — not only at
        // the top level — is caught before assembly, and no `#script` tile is ever assembled
        // (§7). A contrived nest, but it pins the guarantee's reach.
        let expansion = run(
            TokenStream::from_str("p(#script)").expect("lexes"),
            Entry::Program,
            None,
        )
        .to_string();
        assert!(expansion.contains("compile_error"), "{expansion}");
        assert!(expansion.contains("script body"), "{expansion}");
    }

    #[test]
    fn a_program_without_a_script_assembles_no_script_keyword() {
        // The other half of the unreachability closure: a program block naming no `#script`
        // assembles a source whose text carries none, so script-body mode never arises (§4.8).
        let source = MacroSource::build(
            TokenStream::from_str("#external p(a). q(b).").expect("lexes"),
            None,
        )
        .expect("maps under the dialect");
        assert!(!source.text().contains("#script"), "{}", source.text());
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
