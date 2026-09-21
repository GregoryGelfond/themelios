//! Codegen (docs/design/macros.md §5, step 4): the walk from the syntax
//! tier's typed `ast::Term` to a `proc_macro2::TokenStream` of the program
//! tier's §7.1 constructor calls that build the value. Every emitted path is
//! absolute (`::themelios_program::…`), so the expansion names the program
//! tier alone at runtime (the shipped closure, spec §12.5) and cannot be
//! captured by a name in scope at the macro site.
//!
//! The map is the raise's (program §8), one arm per `ast::Term` family, but
//! it *emits the constructor calls* rather than build the `Term`: a constant
//! to `Term::from` / `Term::constant`, a variable to `Term::variable` /
//! `Term::anonymous`, an application to `Term::function` (or the `Term::External`
//! struct literal for an `@`-call), a parenthesized form to a lone term /
//! `Term::tuple` / `Term::pool`, the prefix and infix operators to the operator
//! sugar (program §7.1, construct.rs). Each door canonicalizes one level
//! assuming canonical operands (program §5.1), so a value built bottom-up
//! through these calls is canonical throughout — structurally equal, up to and
//! including provenance (`Origin::Constructed`), to the value the raise builds
//! and to a hand-written constructor chain (the §16 witness, program §5.1/§7.1).
//!
//! **Totality** (docs/design/macros.md §5). Two constructors here are fallible
//! on raw data — `Name::new` refuses a non-identifier, `Term::pool` an empty
//! pool — and each is discharged with a documented `.expect()` naming the
//! invariant that makes the failing case unreachable: a name reaching the
//! codegen was classified `IDENTIFIER`/`VARIABLE` by the dialect mapping (§6;
//! an identifier no class matches is a compile-time dialect error), and a
//! parsed pool is non-empty by the grammar. The expect is unreachable on any
//! input that compiles. A subterm the value cannot represent (a numeral past
//! the engine's width, a token missing under recovery) becomes the raise's
//! recovery [`placeholder`] — the anonymous variable — never a panic; in the
//! wired pipeline (a later increment) that case always coincides with a
//! compile-time lowering diagnostic (§5.3), so the placeholder is never the
//! value a compiling program builds.
// The codegen's surface is reached by the macro entry points a later increment
// wires; until those exist, this module's own tests are its only callers, so
// the not-yet-wired surface would read as dead. The allow is removed when the
// entry points arrive, as on the sibling modules.
#![allow(dead_code)]

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use themelios_syntax::ast::{self, AstToken};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::tree::{AstNode, SyntaxKind};

use crate::source::MacroSource;

/// Emit the program-tier §7.1 constructor calls that build `term`
/// (docs/design/macros.md §5, step 4). Total: every arm emits a value, the
/// fallible constructors discharged by invariant-naming `.expect()` and an
/// unrepresentable subterm by the recovery [`placeholder`] (see the module
/// header). `src` carries the span map the splice seam locates through.
///
/// The macro body parses at `NestingLimit::DEFAULT` (128, docs/design/macros.md
/// §5), so the tree is depth-bounded and this walk recurses within that bound,
/// as the tier's isomorphism walk (§5.3) does; a chain's width is handled by
/// iteration, not recursion.
pub(crate) fn codegen_term(term: &ast::Term, src: &MacroSource) -> TokenStream {
    match term {
        ast::Term::Constant(constant) => codegen_constant(constant),
        ast::Term::Variable(variable) => codegen_variable(variable),
        ast::Term::Function(function) => codegen_application(
            Application::Function,
            function.name(),
            function.arguments(),
            src,
        ),
        ast::Term::External(external) => codegen_application(
            Application::External,
            external.name(),
            external.arguments(),
            src,
        ),
        ast::Term::Pool(pool) => codegen_pool(pool, src),
        ast::Term::Unary(unary) => codegen_unary(unary, src),
        ast::Term::Binary(binary) => codegen_binary(binary, src),
        ast::Term::Abs(abs) => codegen_absolute(abs, src),
        ast::Term::Splice(splice) => codegen_splice(splice, src),
    }
}

/// A constant leaf (grammar §5.1): a numeral to `Term::from(i32)`, a string to
/// `Term::from(&str)`, an identifier to the constant `Term::constant(Name)`, and
/// `#inf`/`#sup` to the order's bounds through `From<Symbol>` — the raise's
/// reading (program §8), emitted as construction. A numeral past the engine's
/// width, or a constant missing under recovery, is the recovery placeholder.
fn codegen_constant(constant: &ast::ConstantTerm) -> TokenStream {
    match constant.constant() {
        Some(ast::Constant::Symbol(identifier)) => {
            let name = codegen_name(identifier.text());
            quote!(::themelios_program::term::Term::constant(#name))
        }
        Some(ast::Constant::Number(number)) => match integer(&number) {
            Some(value) => quote!(::themelios_program::term::Term::from(#value)),
            None => placeholder(),
        },
        Some(ast::Constant::String(string)) => match string.value(Dialect::Clingo) {
            Ok(text) => quote!(::themelios_program::term::Term::from(#text)),
            Err(_) => placeholder(),
        },
        Some(ast::Constant::Infimum(_)) => {
            quote!(::themelios_program::term::Term::from(
                ::themelios_program::symbol::Symbol::Infimum
            ))
        }
        Some(ast::Constant::Supremum(_)) => {
            quote!(::themelios_program::term::Term::from(
                ::themelios_program::symbol::Symbol::Supremum
            ))
        }
        None => placeholder(),
    }
}

/// The `i32` a numeral denotes under its radix, or `None` when it overflows the
/// engine's width (program §3.1) — the raise's `integer`, mirrored so the
/// codegen and the raise read a numeral one way. The dialect maps numerals by
/// value to decimal (§6), so a macro-sourced numeral is `Decimal`; the radix
/// arms keep the reading faithful for any source.
fn integer(number: &ast::NumberLit) -> Option<i32> {
    let radix = match number.radix() {
        ast::Radix::Decimal => 10,
        ast::Radix::Hexadecimal => 16,
        ast::Radix::Octal => 8,
        ast::Radix::Binary => 2,
    };
    i32::from_str_radix(number.digits(), radix).ok()
}

/// A variable leaf (grammar §5.1): the anonymous `_` to `Term::anonymous()`, a
/// named variable to `Term::variable(VarName)`. Missing under recovery, the
/// placeholder.
fn codegen_variable(variable: &ast::VariableTerm) -> TokenStream {
    match variable.variable() {
        Some(inner) if inner.is_anonymous() => {
            quote!(::themelios_program::term::Term::anonymous())
        }
        Some(inner) => {
            let name = codegen_varname(inner.text());
            quote!(::themelios_program::term::Term::variable(#name))
        }
        None => placeholder(),
    }
}

/// Whether an application derives a `Function` or an `External` (`@`-call) term
/// (the raise's distinction, program §8): the two distribute a pooled argument
/// list identically, differing only in the constructor each alternative names.
#[derive(Clone, Copy)]
enum Application {
    Function,
    External,
}

/// A function or `@`-call (grammar §5.1), mirroring the raise (program §8): one
/// argument alternative is a plain application; several — a pooled argument list
/// `f(a; b)` — distribute to a `Term::pool` of applications. A bare `@name` has
/// no argument list; a name missing under recovery is the placeholder.
fn codegen_application(
    application: Application,
    name: Option<ast::Ident>,
    arguments: Option<ast::Arguments>,
    src: &MacroSource,
) -> TokenStream {
    let Some(identifier) = name else {
        return placeholder();
    };
    let name = codegen_name(identifier.text());
    let alternatives: Vec<TokenStream> = arguments
        .into_iter()
        .flat_map(|arguments| arguments.alternatives())
        .map(|alternative| {
            let terms: Vec<TokenStream> = alternative
                .terms()
                .map(|term| codegen_term(&term, src))
                .collect();
            apply(application, &name, &terms)
        })
        .collect();
    match alternatives.len() {
        // A bare `@name` has no argument list; a function always has one.
        0 => apply(application, &name, &[]),
        1 => alternatives.into_iter().next().expect("one alternative"),
        _ => codegen_pool_of(&alternatives),
    }
}

/// One application of `name` to `arguments`: a `Term::function` call for a
/// function, the `Term::External` struct literal for an `@`-call (program §3.3
/// — `Term` is a public enum, so an `@`-term is constructed directly, needing
/// no canonicalization door of its own).
fn apply(application: Application, name: &TokenStream, arguments: &[TokenStream]) -> TokenStream {
    match application {
        Application::Function => {
            quote!(::themelios_program::term::Term::function(#name, [#(#arguments),*]))
        }
        Application::External => quote!(::themelios_program::term::Term::External {
            name: #name,
            arguments: ::std::vec![#(#arguments),*],
        }),
    }
}

/// A parenthesized form (grammar §5.1), mirroring the raise (program §8): a lone
/// term parenthesized is that term, one tuple is `Term::tuple`, several pooled
/// tuples are a `Term::pool` of the tuple-or-terms. A parsed pool is non-empty
/// (the parser wraps every parenthesized form in at least one tuple), so no
/// empty-pool door is reached.
fn codegen_pool(pool: &ast::Pool, src: &MacroSource) -> TokenStream {
    let mut alternatives: Vec<TokenStream> = pool
        .tuples()
        .map(|tuple| codegen_tuple_or_term(&tuple, src))
        .collect();
    if alternatives.len() == 1 {
        alternatives.pop().expect("one alternative")
    } else {
        codegen_pool_of(&alternatives)
    }
}

/// A single tuple alternative (the raise's `tuple_or_term`, program §8): the
/// lone term when it is one term with no trailing comma (`(a)` parenthesizes
/// `a`), a `Term::tuple` otherwise (`(a, b)`, `(a,)`, `()`).
fn codegen_tuple_or_term(tuple: &ast::Tuple, src: &MacroSource) -> TokenStream {
    let terms: Vec<TokenStream> = tuple.terms().map(|term| codegen_term(&term, src)).collect();
    if terms.len() == 1 && tuple.trailing_comma_token().is_none() {
        terms.into_iter().next().expect("one term")
    } else {
        quote!(::themelios_program::term::Term::tuple([#(#terms),*]))
    }
}

/// A run of prefix operators applied to the operand innermost first (the raise's
/// `prefix_run`, program §8): the outermost operator wraps last, so `- ~ X` is
/// `Negate(BitwiseNot(X))` — arithmetic negation as the `-` operator sugar,
/// bitwise complement as `Term::complement` (program §7.1). A run recovered with
/// no operand is the placeholder.
fn codegen_unary(unary: &ast::UnaryTerm, src: &MacroSource) -> TokenStream {
    let Some(operand) = unary.operand() else {
        return placeholder();
    };
    let operators: Vec<SyntaxKind> = unary.operators().map(|token| token.kind()).collect();
    operators
        .into_iter()
        .rev()
        .fold(codegen_term(&operand, src), |argument, kind| {
            if kind == SyntaxKind::TILDE {
                quote!(#argument.complement())
            } else {
                quote!((-#argument))
            }
        })
}

/// A flat per-precedence chain re-associated into the operator tree (the raise's
/// `reassociate`, program §8): left at every level, right for exponentiation,
/// each operator its constructor. A chain recovered below two operands folds
/// what is present.
fn codegen_binary(binary: &ast::BinaryTerm, src: &MacroSource) -> TokenStream {
    let operators: Vec<SyntaxKind> = binary.operators().map(|token| token.kind()).collect();
    let operands: Vec<TokenStream> = binary
        .operands()
        .map(|operand| codegen_term(&operand, src))
        .collect();
    if operands.len() < 2 {
        return operands.into_iter().next().unwrap_or_else(placeholder);
    }
    if matches!(binary.associativity(), Some(ast::Associativity::Right)) {
        // `t₀ ** (t₁ ** (t₂ …))`: fold from the right over (operator, left) pairs.
        let mut operands = operands.into_iter().rev();
        let mut accumulator = operands.next().expect("at least two operands");
        for (operator, left) in operators.into_iter().rev().zip(operands) {
            accumulator = combine(&left, operator, &accumulator);
        }
        accumulator
    } else {
        // `((t₀ op₀ t₁) op₁ t₂) …`: fold from the left over (operator, right) pairs.
        let mut operands = operands.into_iter();
        let mut accumulator = operands.next().expect("at least two operands");
        for (operator, right) in operators.into_iter().zip(operands) {
            accumulator = combine(&accumulator, operator, &right);
        }
        accumulator
    }
}

/// One binary step (the raise's `combine`/`binary_operator`, program §8): the
/// interval former `Term::to` for `..`, exponentiation `Term::pow` for `**`, the
/// arithmetic and bitwise operator sugar otherwise (program §7.1). `+` and any
/// token the grammar's binary operators cannot yield here map through the
/// wildcard, so it is a real arm reached on every `+`.
fn combine(left: &TokenStream, operator: SyntaxKind, right: &TokenStream) -> TokenStream {
    match operator {
        SyntaxKind::DOTDOT => quote!(#left.to(#right)),
        SyntaxKind::STAR_STAR => quote!(#left.pow(#right)),
        SyntaxKind::MINUS => quote!((#left - #right)),
        SyntaxKind::STAR => quote!((#left * #right)),
        SyntaxKind::SLASH => quote!((#left / #right)),
        SyntaxKind::BACKSLASH => quote!((#left % #right)),
        SyntaxKind::AMPERSAND => quote!((#left & #right)),
        SyntaxKind::QUESTION => quote!((#left | #right)),
        SyntaxKind::CARET => quote!((#left ^ #right)),
        _ => quote!((#left + #right)),
    }
}

/// An absolute-value term (the raise's `raise_absolute`, program §8): `|a|` is
/// `Term::abs`, a pooled `|a; b|` distributes to a `Term::pool` of absolute
/// values. An empty `||`, which a recovered parse can reach, is the placeholder.
fn codegen_absolute(abs: &ast::AbsTerm, src: &MacroSource) -> TokenStream {
    let terms: Vec<TokenStream> = abs.terms().map(|term| codegen_term(&term, src)).collect();
    match terms.len() {
        0 => placeholder(),
        1 => {
            let operand = terms.into_iter().next().expect("one operand");
            quote!(#operand.abs())
        }
        _ => {
            let absolutes: Vec<TokenStream> =
                terms.into_iter().map(|term| quote!(#term.abs())).collect();
            codegen_pool_of(&absolutes)
        }
    }
}

/// A splice in term position (docs/design/macros.md §7) — a reserved seam. The
/// conversion crossing (`ToSymbol` then `From<Symbol> for Term`) lands with the
/// increment that owns splice codegen; until then this seam emits a located
/// compile error rather than build a value, keeping `codegen_term` total and its
/// signature stable. The error is located through the source's span map at the
/// splice's Rust token (§6), so it points where a splice would resolve.
fn codegen_splice(splice: &ast::SpliceTerm, src: &MacroSource) -> TokenStream {
    let span = src.span_of(splice.syntax().text_range());
    quote_spanned!(span => compile_error!("a splice is not yet lowered to a term here"))
}

/// A `Term::pool` over `alternatives`, its emptiness discharged: a parsed pool
/// or pooled list is non-empty by the grammar, the invariant the raise §8 rests
/// on, so the `Result` cannot be `Err` on any input that compiles.
fn codegen_pool_of(alternatives: &[TokenStream]) -> TokenStream {
    quote!(::themelios_program::term::Term::pool([#(#alternatives),*])
        .expect("the grammar parsed a non-empty pool"))
}

/// A validated identifier, its refusal discharged: a name reaching the codegen
/// was classified `IDENTIFIER` by the dialect mapping (§6; the raise §8
/// name-class invariant), so `Name::new` cannot refuse on any input that
/// compiles.
fn codegen_name(text: &str) -> TokenStream {
    quote!(::themelios_program::symbol::Name::new(#text)
        .expect("the lexer classified this token IDENTIFIER (raise §8 name-class invariant)"))
}

/// A validated variable name, its refusal discharged as [`codegen_name`]'s: a
/// variable reaching the codegen was classified `VARIABLE` by the dialect
/// mapping (§6).
fn codegen_varname(text: &str) -> TokenStream {
    quote!(::themelios_program::symbol::VarName::new(#text)
        .expect("the lexer classified this token VARIABLE (raise §8 name-class invariant)"))
}

/// The recovery stand-in the raise uses for a subterm the value cannot
/// represent (program §8's `placeholder`): the anonymous variable, so a partial
/// term stays buildable. In the wired pipeline this coincides with a
/// compile-time diagnostic (§5.3); alone, it keeps `codegen_term` total.
fn placeholder() -> TokenStream {
    quote!(::themelios_program::term::Term::anonymous())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;

    use super::*;
    use crate::engine::parse_term_fragment;

    /// The emitted constructor-call stream, as a string, for the term the
    /// macro source `input` assembles and parses to under the dialect. The
    /// goldens assert on this string: a compiled macro (needed for a value
    /// witness) does not exist until the first macro lands, so the arms are
    /// pinned here by their emitted call, and the value-equality witness
    /// accretes with that macro (tests/equality.rs; program §16).
    fn codegen(input: &str) -> String {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let term = parse_term_fragment(&src).tree().term().expect("a term");
        codegen_term(&term, &src).to_string()
    }

    #[test]
    fn codegen_of_a_ground_function_calls_term_function() {
        let ts = codegen("f(1, a)");
        assert!(ts.contains("Term :: function"), "{ts}");
        assert!(ts.contains("Name :: new (\"f\")"), "{ts}");
        assert!(ts.contains("Term :: from (1i32)"), "{ts}");
        assert!(ts.contains("Term :: constant"), "{ts}");
    }

    #[test]
    fn a_named_variable_calls_term_variable() {
        let ts = codegen("X");
        assert!(ts.contains("Term :: variable"), "{ts}");
        assert!(ts.contains("VarName :: new (\"X\")"), "{ts}");
    }

    #[test]
    fn the_anonymous_variable_calls_term_anonymous() {
        assert!(codegen("_").contains("Term :: anonymous ()"));
    }

    #[test]
    fn a_string_constant_calls_term_from_its_value() {
        // The dialect maps a simple string by value (§6); the constant's value
        // under the Clingo dialect is the `&str` `Term::from` takes.
        assert!(codegen("\"hi\"").contains("Term :: from (\"hi\")"));
    }

    #[test]
    fn the_order_bounds_call_term_from_their_symbols() {
        assert!(codegen("#inf").contains("Symbol :: Infimum"));
        assert!(codegen("#sup").contains("Symbol :: Supremum"));
    }

    #[test]
    fn a_tuple_calls_term_tuple() {
        let ts = codegen("(a, b)");
        assert!(ts.contains("Term :: tuple"), "{ts}");
        // A single-term parenthesized form is that term, not a tuple.
        assert!(!codegen("(a)").contains("Term :: tuple"));
    }

    #[test]
    fn a_pool_calls_term_pool_with_its_discharged_expect() {
        let ts = codegen("(a; b)");
        assert!(ts.contains("Term :: pool"), "{ts}");
        assert!(
            ts.contains(". expect (\"the grammar parsed a non-empty pool\")"),
            "{ts}"
        );
    }

    #[test]
    fn a_pooled_argument_list_distributes_to_a_pool_of_applications() {
        // `f(a; b)` is `Term::pool([f(a), f(b)])` (program §8).
        let ts = codegen("f(a; b)");
        assert!(ts.contains("Term :: pool"), "{ts}");
        assert!(ts.contains("Term :: function"), "{ts}");
    }

    #[test]
    fn an_external_call_builds_the_external_struct_literal() {
        let ts = codegen("@f(X)");
        assert!(ts.contains("Term :: External"), "{ts}");
        assert!(ts.contains("name :"), "{ts}");
        assert!(ts.contains("arguments :"), "{ts}");
        // A bare `@name` has no argument list.
        assert!(codegen("@g").contains("Term :: External"));
    }

    #[test]
    fn arithmetic_negation_calls_the_neg_operator() {
        let ts = codegen("-X");
        assert!(ts.contains("(- "), "{ts}");
        assert!(ts.contains("Term :: variable"), "{ts}");
    }

    #[test]
    fn bitwise_complement_calls_term_complement() {
        assert!(codegen("~X").contains(". complement ()"));
    }

    #[test]
    fn a_prefix_run_wraps_the_outermost_operator_last() {
        // `- ~ X` is `Negate(BitwiseNot(X))`: complement inside, negation outside.
        let ts = codegen("- ~ X");
        assert!(ts.contains("(- "), "{ts}");
        assert!(ts.contains(". complement ()"), "{ts}");
    }

    #[test]
    fn an_arithmetic_chain_folds_to_the_binary_operators() {
        let ts = codegen("1 + 2");
        assert!(ts.contains("Term :: from (1i32)"), "{ts}");
        assert!(ts.contains("Term :: from (2i32)"), "{ts}");
        assert!(ts.contains('+'), "{ts}");
    }

    #[test]
    fn every_arithmetic_and_bitwise_operator_maps_to_its_sugar() {
        // One left-associative chain per operator token a Rust token stream can
        // carry, each to the operator sugar its `combine` names (program §7.1).
        // `\` (`Mod`, `%`) is left out: proc-macro2 admits no `\` punctuation
        // (source.rs), so the modulo operator is inexpressible from a macro body
        // — its `combine` arm stays for faithfulness to the raise's full map.
        for (source, needle) in [
            ("1 - 2", " - "),
            ("1 * 2", " * "),
            ("1 / 2", " / "),
            ("1 & 2", " & "),
            ("1 ? 2", " | "),
            ("1 ^ 2", " ^ "),
        ] {
            assert!(codegen(source).contains(needle), "`{source}` -> {needle}");
        }
    }

    #[test]
    fn exponentiation_calls_term_pow() {
        assert!(codegen("2 ** 3").contains(". pow ("));
    }

    #[test]
    fn an_interval_calls_term_to() {
        assert!(codegen("1 .. 3").contains(". to ("));
    }

    #[test]
    fn an_absolute_value_calls_term_abs() {
        assert!(codegen("|X|").contains(". abs ()"));
        // A pooled `|X; Y|` distributes to a pool of absolute values.
        let pooled = codegen("|X; Y|");
        assert!(pooled.contains("Term :: pool"), "{pooled}");
        assert!(pooled.contains(". abs ()"), "{pooled}");
    }

    #[test]
    fn a_numeral_past_the_engine_width_codegens_the_recovery_placeholder() {
        // `9999999999` overflows `i32`; the codegen stays total with the
        // recovery placeholder, the raise's stand-in (program §8), the wired
        // pipeline reporting the numeral through a compile-time diagnostic.
        assert!(codegen("9999999999").contains("Term :: anonymous ()"));
    }

    #[test]
    fn a_splice_emits_the_task_seam_compile_error() {
        // The seam until splice codegen lands: a located compile error, not a
        // built value and not a panic (docs/design/macros.md §7).
        assert!(codegen("$x").contains("compile_error !"));
    }

    #[test]
    fn a_nested_function_recurses_through_its_arguments() {
        let ts = codegen("f(g(h(X)))");
        assert_eq!(ts.matches("Term :: function").count(), 3, "{ts}");
        assert!(ts.contains("Term :: variable"), "{ts}");
    }
}
