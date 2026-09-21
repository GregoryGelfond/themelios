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
//! The theory-term algebra is the peer walk (program §4.9): `codegen_theory_term`
//! emits the `TheoryTerm` variant each `ast::TheoryTerm` node names — a symbolic
//! leaf lifted through a built `Symbol`, a variable, the bracketed and applied
//! forms, and (at the flat operator sequence, an `ast::TheoryOpTerm`) the
//! `Operation` run — and `codegen_theory_atom` assembles the atom through
//! `TheoryAtom::new` / `TheoryElement::new` / the `TheoryGuard` literal, its
//! ordinary-argument list the raise's §17 exception (the first alternative only,
//! reusing [`codegen_term`]). This mirrors the theory-term raise (program §8)
//! arm for arm, so the built theory value is structurally equal to the raise's
//! and to hand-construction the same way the ordinary walk is (§16).
//!
//! A splice crosses the conversion pillar (docs/design/macros.md §7): the spliced
//! Rust value is taken to a ground `Symbol` through `ToSymbol::to_symbol`, then
//! lands as the leaf its position admits — `From<Symbol> for Term` in term
//! position, `TheoryTerm::Symbolic` in theory-term position. The macro emits the
//! crossing and nothing more; a value whose type is not `ToSymbol` makes the
//! emitted call a compile error at the constructor door (the trait bound is the
//! check — no macro-side type test), and the captured operand's own spans ride
//! through so that error points at the spliced expression.
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
//! wired pipeline that case always coincides with a compile-time lowering diagnostic
//! (§5.3), so the placeholder is never the value a compiling program builds.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use themelios_syntax::ast::{self, AstToken};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::tree::{AstNode, SyntaxKind, TextRange};

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

/// A splice in term position (docs/design/macros.md §7): the crossing that takes
/// the spliced Rust value to a ground `Symbol` through `ToSymbol::to_symbol` and
/// lifts it into a `Term` through `From<Symbol> for Term` — the one door a spliced
/// value enters a term by. `#expr` is the operand the source captured for this
/// splice's byte range (the marker and its operand, grammar §9); the emitted
/// `&(#expr)` carries the operand's own Rust spans, so a value whose type is not
/// `ToSymbol` is refused at this call with the error pointing at the spliced
/// expression (the trait bound is the check). A splice with no captured operand
/// — no `SPLICE` tile matches the node's range, unreachable for a parsed splice —
/// is the recovery [`placeholder`], keeping the walk total.
fn codegen_splice(splice: &ast::SpliceTerm, src: &MacroSource) -> TokenStream {
    match src.splice_at(splice.syntax().text_range()) {
        Some(expr) => quote!(::themelios_program::term::Term::from(
            ::themelios_program::symbol::ToSymbol::to_symbol(&(#expr))
        )),
        None => placeholder(),
    }
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

// ---- theory terms and theory atoms (docs/design/macros.md §4; program §4.9) ----

/// Emit the program-tier constructor calls that build the theory term `theory_term`
/// (grammar §5.8), mirroring the theory-term raise (program §8) arm for arm: a
/// bracketed or applied form recurses through its operand opterms, a symbolic leaf
/// lifts a built [`Symbol`](codegen_theory_symbol) into `TheoryTerm::Symbolic`, a
/// variable lands as `TheoryTerm::Variable`, and a splice crosses the conversion
/// pillar to a symbolic leaf (§7). The `Operation` run is formed one level up, at
/// the flat operator sequence ([`codegen_theory_opterm`]); a leaf missing under
/// recovery is the [`theory_placeholder`].
pub(crate) fn codegen_theory_term(theory_term: &ast::TheoryTerm, src: &MacroSource) -> TokenStream {
    match theory_term {
        ast::TheoryTerm::Set(set) => {
            let items = theory_opterms(set.opterms(), src);
            quote!(::themelios_program::program::TheoryTerm::Set(
                ::std::vec![#(#items),*]
            ))
        }
        ast::TheoryTerm::List(list) => {
            let items = theory_opterms(list.opterms(), src);
            quote!(::themelios_program::program::TheoryTerm::List(
                ::std::vec![#(#items),*]
            ))
        }
        ast::TheoryTerm::Tuple(tuple) => {
            let items = theory_opterms(tuple.opterms(), src);
            quote!(::themelios_program::program::TheoryTerm::Tuple(
                ::std::vec![#(#items),*]
            ))
        }
        ast::TheoryTerm::Function(function) => {
            let Some(identifier) = function.name() else {
                return theory_placeholder();
            };
            let name = codegen_name(identifier.text());
            let arguments = theory_opterms(function.opterms(), src);
            quote!(::themelios_program::program::TheoryTerm::Function {
                name: #name,
                arguments: ::std::vec![#(#arguments),*],
            })
        }
        ast::TheoryTerm::Constant(constant) => {
            let symbol = codegen_theory_symbol(constant);
            quote!(::themelios_program::program::TheoryTerm::Symbolic(#symbol))
        }
        ast::TheoryTerm::Variable(variable) => codegen_theory_variable(variable),
        ast::TheoryTerm::Splice(splice) => codegen_theory_splice(splice, src),
    }
}

/// The constructor calls for a run of theory opterms — a bracketed form's members
/// or an applied form's arguments — each through [`codegen_theory_opterm`].
fn theory_opterms(
    opterms: impl Iterator<Item = ast::TheoryOpTerm>,
    src: &MacroSource,
) -> Vec<TokenStream> {
    opterms
        .map(|opterm| codegen_theory_opterm(&opterm, src))
        .collect()
}

/// A theory opterm to the constructor calls that build its `TheoryTerm` (the raise's
/// `enter_theory_opterm`, program §8): the flat operator sequence is `operators[i]`,
/// the run before `operands[i]`. An opterm of one operand under no operators is that
/// operand; several operands, or any operator, is a `TheoryTerm::Operation` over the
/// operands, each recursed. An opterm recovered with no operand is the
/// [`theory_placeholder`].
fn codegen_theory_opterm(opterm: &ast::TheoryOpTerm, src: &MacroSource) -> TokenStream {
    let (operators, operands) = split_theory_opterm(opterm);
    if operands.is_empty() {
        return theory_placeholder();
    }
    if operands.len() == 1 && operators.iter().all(Vec::is_empty) {
        let operand = operands.into_iter().next().expect("one operand");
        return codegen_theory_term(&operand, src);
    }
    let operator_runs: Vec<TokenStream> = operators
        .iter()
        .map(|run| {
            let operator_calls: Vec<TokenStream> = run
                .iter()
                .map(|symbol| quote!(::themelios_program::program::TheoryOperator::new(#symbol)))
                .collect();
            quote!(::std::vec![#(#operator_calls),*])
        })
        .collect();
    let operand_terms: Vec<TokenStream> = operands
        .iter()
        .map(|operand| codegen_theory_term(operand, src))
        .collect();
    quote!(::themelios_program::program::TheoryTerm::Operation {
        operators: ::std::vec![#(#operator_runs),*],
        operands: ::std::vec![#(#operand_terms),*],
    })
}

/// A theory opterm's operator runs and its operand nodes (the raise's `split_opterm`,
/// program §8): `operators[i]` is the run of operator symbols before `operands[i]`, so
/// a leading run precedes the first operand. A run after the last operand — reachable
/// only under recovery — is dropped, as the raise drops it.
fn split_theory_opterm(opterm: &ast::TheoryOpTerm) -> (Vec<Vec<String>>, Vec<ast::TheoryTerm>) {
    let mut operators = Vec::new();
    let mut operands = Vec::new();
    let mut run = Vec::new();
    for item in opterm.items() {
        match item {
            ast::TheoryOpTermItem::Op(token) => run.push(token.text().to_owned()),
            ast::TheoryOpTermItem::Term(term) => {
                operators.push(std::mem::take(&mut run));
                operands.push(term);
            }
        }
    }
    (operators, operands)
}

/// A theory symbolic leaf to the `Symbol` it lifts (the raise's `raise_theory_constant`,
/// program §8): a constant identifier to a nullary positive `Symbol::Function`, a
/// numeral to `Symbol::Number`, a string to `Symbol::string`, and `#inf`/`#sup` to the
/// order's bounds. A numeral past the engine's width, a string the value cannot spell,
/// or a leaf missing under recovery is `Symbol::Infimum` — the raise's ground stand-in
/// for a theory leaf beside its diagnostic (a `Symbol` has no anonymous-variable form).
fn codegen_theory_symbol(constant: &ast::ConstantTerm) -> TokenStream {
    match constant.constant() {
        Some(ast::Constant::Symbol(identifier)) => {
            let name = codegen_name(identifier.text());
            quote!(::themelios_program::symbol::Symbol::Function {
                name: #name,
                arguments: ::std::vec![],
                sign: ::themelios_program::symbol::Sign::Positive,
            })
        }
        Some(ast::Constant::Number(number)) => match integer(&number) {
            Some(value) => quote!(::themelios_program::symbol::Symbol::Number(#value)),
            None => theory_symbol_infimum(),
        },
        Some(ast::Constant::String(string)) => match string.value(Dialect::Clingo) {
            Ok(text) => quote!(::themelios_program::symbol::Symbol::string(#text)),
            Err(_) => theory_symbol_infimum(),
        },
        Some(ast::Constant::Supremum(_)) => quote!(::themelios_program::symbol::Symbol::Supremum),
        // `#inf` and a leaf no value can stand for share the ground bound.
        Some(ast::Constant::Infimum(_)) | None => theory_symbol_infimum(),
    }
}

/// The ground stand-in for a theory symbolic leaf the value cannot represent (the
/// raise's, program §8): `Symbol::Infimum`. A `Symbol` has no anonymous-variable
/// form, so — unlike the term [`placeholder`] — a theory leaf's recovery is a
/// ground bound, and the wired pipeline reports the leaf through a compile-time
/// diagnostic, so this is never the value a compiling program builds.
fn theory_symbol_infimum() -> TokenStream {
    quote!(::themelios_program::symbol::Symbol::Infimum)
}

/// A theory variable leaf (the raise's `raise_theory_variable`, program §8): the
/// anonymous `_` to `TheoryTerm::Variable(Variable::Anonymous)`, a named variable to
/// `Variable::Named`. Missing under recovery, the [`theory_placeholder`].
fn codegen_theory_variable(variable: &ast::VariableTerm) -> TokenStream {
    match variable.variable() {
        Some(inner) if inner.is_anonymous() => {
            quote!(::themelios_program::program::TheoryTerm::Variable(
                ::themelios_program::term::Variable::Anonymous
            ))
        }
        Some(inner) => {
            let name = codegen_varname(inner.text());
            quote!(::themelios_program::program::TheoryTerm::Variable(
                ::themelios_program::term::Variable::Named(#name)
            ))
        }
        None => theory_placeholder(),
    }
}

/// A splice in theory-term position (docs/design/macros.md §7): the same crossing as a
/// term-position splice ([`codegen_splice`]), landing at the theory algebra's
/// ground-symbol leaf — `ToSymbol::to_symbol` then `TheoryTerm::Symbolic` (program
/// §4.9's leaf-lift between the ordinary and theory algebras). `#expr` carries its own
/// Rust spans, so a non-`ToSymbol` value is refused here pointing at the spliced
/// expression; a splice with no captured operand is the [`theory_placeholder`].
fn codegen_theory_splice(splice: &ast::SpliceTerm, src: &MacroSource) -> TokenStream {
    match src.splice_at(splice.syntax().text_range()) {
        Some(expr) => quote!(::themelios_program::program::TheoryTerm::Symbolic(
            ::themelios_program::symbol::ToSymbol::to_symbol(&(#expr))
        )),
        None => theory_placeholder(),
    }
}

/// Emit the program-tier constructor calls that build the theory atom `atom`
/// (grammar §5.8), mirroring the theory-atom raise (program §8): the name, the
/// ordinary-argument list (the raise's §17 exception — the first alternative only,
/// through [`codegen_term`]; a pooled list is a lowering diagnostic, not distributed),
/// the elements, and the optional guard, assembled through the public
/// `TheoryAtom::new` door (program §7.1). A nameless atom — reachable only under
/// recovery, the parser's `IDENT` never taken — is a located compile error rather
/// than a fabricated name, coincident in the wired pipeline with the syntax
/// diagnostic that already refuses it (§5.3), so it is never the value a compiling
/// program builds.
pub(crate) fn codegen_theory_atom(atom: &ast::TheoryAtom, src: &MacroSource) -> TokenStream {
    let Some(identifier) = atom.name() else {
        return located_compile_error(
            src,
            atom.syntax().text_range(),
            "a theory atom needs a name",
        );
    };
    let name = codegen_name(identifier.text());
    let arguments: Vec<TokenStream> = atom
        .arguments()
        .and_then(|arguments| arguments.alternatives().next())
        .into_iter()
        .flat_map(|tuple| tuple.terms())
        .map(|term| codegen_term(&term, src))
        .collect();
    let elements: Vec<TokenStream> = atom
        .elements()
        .into_iter()
        .flat_map(|elements| elements.elements())
        .map(|element| codegen_theory_element(&element, src))
        .collect();
    let guard = codegen_theory_guard(atom.guard(), src);
    quote!(::themelios_program::program::TheoryAtom::new(
        #name,
        [#(#arguments),*],
        [#(#elements),*],
        #guard,
    ))
}

/// A theory element to `TheoryElement::new` (the raise's `raise_theory_element`,
/// program §8): its opterms as theory terms, under the optional condition the `:`
/// introduces. The condition is a `Condition` of ordinary literals, present and
/// possibly empty when the `:` is, absent otherwise — the raise's reading — and it
/// codegens through the ordinary [`codegen_condition`], the same door a rule body's
/// condition takes; an unconditioned element (no `:`) carries `None`.
fn codegen_theory_element(element: &ast::TheoryElement, src: &MacroSource) -> TokenStream {
    let terms = theory_opterms(element.opterms(), src);
    let condition = if element.colon_token().is_some() {
        let condition = codegen_condition(element.condition().as_ref(), src);
        quote!(::std::option::Option::Some(#condition))
    } else {
        quote!(::std::option::Option::None)
    };
    quote!(::themelios_program::program::TheoryElement::new(
        [#(#terms),*],
        #condition,
    ))
}

/// A theory atom's optional guard to `Option<TheoryGuard>` (the raise's
/// `raise_theory_guard`, program §8): a present guard's operator and its opterm as a
/// `TheoryGuard`, `None` when the atom has no guard or the guard's operator is missing
/// under recovery (as the raise's `?` drops it). A guard whose bound is missing takes
/// the [`theory_placeholder`].
fn codegen_theory_guard(guard: Option<ast::TheoryGuard>, src: &MacroSource) -> TokenStream {
    let Some(guard) = guard else {
        return quote!(::std::option::Option::None);
    };
    let Some(operator) = guard.operator_token() else {
        return quote!(::std::option::Option::None);
    };
    let symbol = operator.text();
    let term = guard.opterm().map_or_else(theory_placeholder, |opterm| {
        codegen_theory_opterm(&opterm, src)
    });
    quote!(::std::option::Option::Some(::themelios_program::program::TheoryGuard {
        operator: ::themelios_program::program::TheoryOperator::new(#symbol),
        term: #term,
    }))
}

/// The recovery stand-in for a theory term the value cannot represent (the raise's
/// `theory_placeholder`, program §8): the anonymous variable lifted into the theory
/// algebra. As with [`placeholder`], in the wired pipeline this coincides with a
/// compile-time diagnostic, so it is never the value a compiling program builds.
fn theory_placeholder() -> TokenStream {
    quote!(::themelios_program::program::TheoryTerm::Variable(
        ::themelios_program::term::Variable::Anonymous
    ))
}

/// A `compile_error!` located at `range`'s Rust token through the source's span map
/// (docs/design/macros.md §5.3, §6): the permitted direction across the seam — a Rust
/// span carried to an error, never a `proc_macro` span projected into a themelios
/// `Location`. Keeps a codegen door total where it meets a form no value can yet
/// stand for — a nameless theory atom, a head or body shape a construction macro does
/// not yet build, a recovered form the value cannot complete: the
/// emitted error is discarded in the wired pipeline, where the same form already
/// raises a compile-time diagnostic (or is a construction-site error of the engine's,
/// §9).
fn located_compile_error(src: &MacroSource, range: TextRange, message: &str) -> TokenStream {
    let span = src.span_of(range);
    quote_spanned!(span => compile_error!(#message))
}

// ---- statements: rules, directives, and their heads and bodies (program §4) ----

/// Emit the program-tier §7.1 constructor calls that build the statement `statement`
/// (docs/design/macros.md §5, step 4; §8) — the codegen half of the seven statement
/// macros. Each family a construction macro builds codegens to its specific family
/// constructor, the value the macro returns (§8): a rule (a fact, a rule, or a
/// constraint), an optimization statement, a `#show`, an `#external`. A statement family
/// no construction macro builds is a located compile error at the construction site —
/// a well-formed statement the caller reached for with the wrong macro — never a
/// fabricated value. The match is exhaustive, so a new statement family is a compile
/// error here, never a silent drop.
pub(crate) fn codegen_statement(statement: &ast::Statement, src: &MacroSource) -> TokenStream {
    match statement {
        ast::Statement::Rule(rule) => codegen_rule(rule, src),
        ast::Statement::Optimize(optimize) => codegen_optimize(optimize, src),
        ast::Statement::Show(show) => codegen_show(show, src),
        ast::Statement::External(external) => codegen_external(external, src),
        ast::Statement::WeakConstraint(_)
        | ast::Statement::Project(_)
        | ast::Statement::Defined(_)
        | ast::Statement::Edge(_)
        | ast::Statement::Heuristic(_)
        | ast::Statement::Const(_)
        | ast::Statement::Script(_)
        | ast::Statement::Include(_)
        | ast::Statement::ProgramPart(_)
        | ast::Statement::TheoryDefinition(_)
        | ast::Statement::Query(_) => located_compile_error(
            src,
            statement.syntax().text_range(),
            "this construction builds a fact, a rule, a constraint, an optimization \
             statement, a `#show`, or an `#external`, not this statement",
        ),
    }
}

/// Emit the program-tier §7.1 constructor calls that build the single head [`Atom`] an
/// `atom!` reaches for (docs/design/macros.md §8) — the codegen half of the head-atom macro.
/// `atom!` assembles a fact and this extracts its head atom: there is no atom fragment door,
/// and the term door reads a leading `-` as arithmetic negation, so a strong-negated atom is
/// reached in head position, where `-p` is the atom's positional strong sign (program §3.3,
/// §8). A fact's head is a literal wrapping the atom, which codegens through [`codegen_atom`]
/// — the same door the statement head takes, so the built atom is structurally equal, up to
/// and including provenance, to hand construction (§16). Every other head is a located
/// compile error at the offending head, never a fabricated atom: a non-rule statement
/// reached for with the wrong macro, a constraint's absent head (a falsum, §4.4), and — the
/// head match exhaustive, so a new `ast::Head` variant is a compile error here — a
/// disjunction, a choice or aggregate, or a theory atom.
pub(crate) fn codegen_head_atom(statement: &ast::Statement, src: &MacroSource) -> TokenStream {
    let ast::Statement::Rule(rule) = statement else {
        return located_compile_error(
            src,
            statement.syntax().text_range(),
            "atom! expects a single atom, not this statement",
        );
    };
    let Some(head) = rule.head() else {
        // No head node is a constraint (`:- body.`, §4.4): a falsum head, not an atom.
        return located_compile_error(
            src,
            rule.syntax().text_range(),
            "atom! expects a single atom, not a constraint",
        );
    };
    match head {
        ast::Head::Literal(literal) => codegen_head_literal_atom(&literal, src),
        ast::Head::Disjunction(disjunction) => located_compile_error(
            src,
            disjunction.syntax().text_range(),
            "atom! expects a single atom, not a disjunction",
        ),
        ast::Head::Aggregate(aggregate) => located_compile_error(
            src,
            aggregate.syntax().text_range(),
            "atom! expects a single atom, not a choice or aggregate",
        ),
        ast::Head::TheoryAtom(atom) => located_compile_error(
            src,
            atom.syntax().text_range(),
            "atom! expects a single atom, not a theory atom",
        ),
    }
}

/// The single ordinary [`Atom`] a fact's head literal wraps (docs/design/macros.md §8), or a
/// located compile error when the literal is not one. A bare atom under no default negation
/// codegens through [`codegen_atom`], carrying its positional strong sign (§8); anything else
/// is not a single ordinary atom — default negation is a *body* property (program §4.5), so a
/// `not p` head is not an atom (its `not` has nowhere to go in an `Atom` value), and a
/// comparison, a boolean (`#true`/`#false`), or — under recovery — a literal with no inner
/// form is not an atom either. Each is a located compile error, coincident in the wired
/// pipeline with the syntax diagnostic that flags it (§5.3), never a silently-stripped atom.
fn codegen_head_literal_atom(literal: &ast::Literal, src: &MacroSource) -> TokenStream {
    match (literal.negation(), literal.inner()) {
        (ast::Negation::None, Some(ast::LiteralInner::Atom(atom))) => codegen_atom(&atom, src),
        _ => located_compile_error(
            src,
            literal.syntax().text_range(),
            "atom! expects a single atom, not a comparison, a boolean, or a negated literal",
        ),
    }
}

/// A rule (§4.3), mirroring the raise (program §8) but emitting the construction door
/// each shape names: a head — its absence a constraint (`⊥ ← body`, §4.4) — and a body.
/// A fact (a head, no body node) is `Rule::fact`, a constraint (no head node)
/// `Rule::constraint`, and a head over a body `Rule::new` (`Head::when`, §7.1) — the
/// three shapes the `fact!`, `constraint!`, and `rule!` macros build, told apart by the
/// head and body the parse carries, so one codegen serves all three.
fn codegen_rule(rule: &ast::Rule, src: &MacroSource) -> TokenStream {
    let Some(head) = rule.head() else {
        // No head node is a constraint: `:- body.` (§4.4).
        let body = codegen_body(rule.body().as_ref(), src);
        return quote!(::themelios_program::program::Rule::constraint(#body));
    };
    let head = codegen_head(&head, src);
    match rule.body() {
        // A head with no body node is a fact: `head.` (§4.3).
        None => quote!(::themelios_program::program::Rule::fact(#head)),
        // A head over a body reads as the rule it denotes (§7.1).
        Some(body) => {
            let body = codegen_body(Some(&body), src);
            quote!(::themelios_program::program::Rule::new(#head, #body))
        }
    }
}

/// A rule head (§4.4), emitting an `IntoHead` value the rule constructor coerces
/// (program §7.1): a literal head — an atom, a comparison, a boolean — through its
/// [`Literal`](codegen_literal), a theory-atom head through [`codegen_theory_atom`]. A
/// disjunction, a choice, or a head aggregate is a located compile error here — a head
/// shape the construction macros do not yet build (§8); total, and never a fabricated
/// head.
fn codegen_head(head: &ast::Head, src: &MacroSource) -> TokenStream {
    match head {
        ast::Head::Literal(literal) => codegen_literal(literal, src),
        ast::Head::TheoryAtom(atom) => codegen_theory_atom(atom, src),
        ast::Head::Disjunction(disjunction) => located_compile_error(
            src,
            disjunction.syntax().text_range(),
            "a disjunctive head is not yet built by a construction macro",
        ),
        ast::Head::Aggregate(aggregate) => located_compile_error(
            src,
            aggregate.syntax().text_range(),
            "a choice or aggregate head is not yet built by a construction macro",
        ),
    }
}

/// A rule body (§4.5), emitting a [`Body`]: its elements through the coercion surface, or
/// the empty body (`Body::empty`) for a fact or a bodiless rule. A body node with no
/// elements — `h :- .` — is the empty body too, the raise's reading (program §8).
fn codegen_body(body: Option<&ast::Body>, src: &MacroSource) -> TokenStream {
    let Some(body) = body else {
        return quote!(::themelios_program::program::Body::empty());
    };
    let elements: Vec<TokenStream> = body
        .elements()
        .map(|element| codegen_body_element(&element, src))
        .collect();
    if elements.is_empty() {
        quote!(::themelios_program::program::Body::empty())
    } else {
        quote!(::themelios_program::program::Body::new([#(#elements),*]))
    }
}

/// A body element (§4.5), emitting a [`BodyElement`] through the coercion surface: a
/// literal or a conditional literal by its `From`, a theory atom under its own default
/// negation (`not`/`not not`, program §7.1) or bare. A body aggregate is a located
/// compile error — a body element the construction macros do not yet build (§8); total.
fn codegen_body_element(element: &ast::BodyElement, src: &MacroSource) -> TokenStream {
    match element {
        ast::BodyElement::Literal(literal) => {
            let literal = codegen_literal(literal, src);
            quote!(::themelios_program::program::BodyElement::from(#literal))
        }
        ast::BodyElement::ConditionalLiteral(conditional) => {
            let conditional = codegen_conditional_literal(conditional, src);
            quote!(::themelios_program::program::BodyElement::from(#conditional))
        }
        ast::BodyElement::TheoryAtom(atom) => {
            let negation = atom.negation();
            let value = codegen_theory_atom(atom, src);
            match negation {
                ast::Negation::None => {
                    quote!(::themelios_program::program::BodyElement::from(#value))
                }
                ast::Negation::Default => quote!(::themelios_program::construct::not(#value)),
                ast::Negation::DoubleDefault => {
                    quote!(::themelios_program::construct::not_not(#value))
                }
            }
        }
        ast::BodyElement::Aggregate(aggregate) => located_compile_error(
            src,
            aggregate.syntax().text_range(),
            "a body aggregate is not yet built by a construction macro",
        ),
    }
}

/// A literal (§4.6), emitting a [`Literal`] value: its default negation over an atom, a
/// comparison, or a boolean constant (program §8). The atom and comparison, already
/// canonical from their own doors ([`codegen_atom`], [`codegen_comparison`]; program
/// §5.1), ride into the literal through the public provenance carrier — the same value
/// `Literal::from` would build, whose canonicalize is idempotent on a canonical operand.
/// A literal missing its inner form under recovery is a located compile error,
/// coincident with the syntax diagnostic that flags it (§5.3).
fn codegen_literal(literal: &ast::Literal, src: &MacroSource) -> TokenStream {
    let negation = default_negation(literal.negation());
    let Some(inner) = literal.inner() else {
        return located_compile_error(
            src,
            literal.syntax().text_range(),
            "this literal is incomplete",
        );
    };
    let inner = match inner {
        ast::LiteralInner::True(_) => quote!(::themelios_program::program::LiteralInner::True),
        ast::LiteralInner::False(_) => quote!(::themelios_program::program::LiteralInner::False),
        ast::LiteralInner::Atom(atom) => {
            let atom = codegen_atom(&atom, src);
            quote!(::themelios_program::program::LiteralInner::Atom(
                ::themelios_program::provenance::WithProvenance::constructed(#atom)
            ))
        }
        ast::LiteralInner::Comparison(comparison) => {
            let comparison = codegen_comparison(&comparison, src);
            quote!(::themelios_program::program::LiteralInner::Comparison(
                ::themelios_program::provenance::WithProvenance::constructed(#comparison)
            ))
        }
    };
    quote!(::themelios_program::program::Literal {
        negation: #negation,
        inner: #inner,
    })
}

/// An atom (§4.6), emitting an [`Atom`] value: a strong sign — a leading `-` the tree
/// resolved positionally (program §3.3, §8) — a name, and an argument list, one tuple
/// (`Atom::new`) or an argument-list pool of two or more (`Atom::pooled`, program §7.1).
/// The name is discharged as elsewhere ([`codegen_name`]); a strong-negated atom wraps in
/// the `Neg` operator (program §4.6), leaving a canonical atom canonical. A nameless atom
/// under recovery is a located compile error, coincident with the syntax diagnostic (§8).
fn codegen_atom(atom: &ast::Atom, src: &MacroSource) -> TokenStream {
    let Some(identifier) = atom.name() else {
        return located_compile_error(src, atom.syntax().text_range(), "this atom has no name");
    };
    let name = codegen_name(identifier.text());
    let alternatives: Vec<Vec<TokenStream>> = atom
        .arguments()
        .into_iter()
        .flat_map(|arguments| arguments.alternatives())
        .map(|tuple| tuple.terms().map(|term| codegen_term(&term, src)).collect())
        .collect();
    let unsigned = match alternatives.len() {
        // One tuple (or none) is a `Single` atom; two or more, an argument-list pool.
        0 | 1 => {
            let terms = alternatives.into_iter().next().unwrap_or_default();
            quote!(::themelios_program::program::Atom::new(#name, [#(#terms),*]))
        }
        _ => {
            let tuples = alternatives
                .iter()
                .map(|terms| quote!(::std::vec![#(#terms),*]));
            quote!(::themelios_program::program::Atom::pooled(#name, [#(#tuples),*])
                .expect("the grammar parsed a non-empty argument-list pool"))
        }
    };
    if atom.strong_negation_token().is_some() {
        quote!((-#unsigned))
    } else {
        unsigned
    }
}

/// A comparison chain (§4.6), emitting a [`Comparison`] through `Comparison::new` and
/// `Comparison::chain` (program §7.1) — `1 < X < 5` is one literal, not a conjunction. A
/// step's term absent under recovery is the [`placeholder`]; a chain the parse left with
/// no step is a located compile error, as the raise's `incomplete` is (program §8),
/// coincident with the syntax diagnostic.
fn codegen_comparison(comparison: &ast::Comparison, src: &MacroSource) -> TokenStream {
    let first = comparison
        .first()
        .map_or_else(placeholder, |term| codegen_term(&term, src));
    let mut steps = comparison.steps().map(|(relation, term)| {
        (
            relation_of(relation),
            term.map_or_else(placeholder, |term| codegen_term(&term, src)),
        )
    });
    let Some((relation, second)) = steps.next() else {
        return located_compile_error(
            src,
            comparison.syntax().text_range(),
            "this comparison is incomplete",
        );
    };
    let mut chain =
        quote!(::themelios_program::program::Comparison::new(#first, #relation, #second));
    for (relation, term) in steps {
        chain = quote!(#chain.chain(#relation, #term));
    }
    chain
}

/// A conditional literal (§4.6), emitting a [`ConditionalLiteral`]: its literal under its
/// condition (grammar §5.4). A conditional missing its literal under recovery is a located
/// compile error, coincident with the syntax diagnostic (§5.3).
fn codegen_conditional_literal(
    conditional: &ast::ConditionalLiteral,
    src: &MacroSource,
) -> TokenStream {
    let Some(literal) = conditional.literal() else {
        return located_compile_error(
            src,
            conditional.syntax().text_range(),
            "this conditional literal is incomplete",
        );
    };
    let literal = codegen_literal(&literal, src);
    let condition = codegen_condition(conditional.condition().as_ref(), src);
    quote!(::themelios_program::program::ConditionalLiteral {
        literal: #literal,
        condition: #condition,
    })
}

/// A condition (§4.6), emitting a [`Condition`]: the literals after a `:`, built through
/// `Condition::new` (program §7.1), or `Condition::empty` when absent or empty — present
/// and empty when the colon is (grammar §5.4).
fn codegen_condition(condition: Option<&ast::Condition>, src: &MacroSource) -> TokenStream {
    let Some(condition) = condition else {
        return quote!(::themelios_program::program::Condition::empty());
    };
    let literals: Vec<TokenStream> = condition
        .literals()
        .map(|literal| codegen_literal(&literal, src))
        .collect();
    if literals.is_empty() {
        quote!(::themelios_program::program::Condition::empty())
    } else {
        quote!(::themelios_program::program::Condition::new([#(#literals),*]))
    }
}

/// A `#show` directive (§4.8), emitting a [`Show`] in one of its four forms (program §8):
/// a signature (`#show p/1.`), a term (`#show t.`), a term under a body
/// (`#show t : body.`, through `Show::term_body`), or all (`#show.`).
fn codegen_show(show: &ast::ShowStatement, src: &MacroSource) -> TokenStream {
    if let Some(signature) = show.signature() {
        let signature = codegen_signature(&signature, src);
        return quote!(::themelios_program::program::Show::Signature(#signature));
    }
    if let Some(term) = show.term() {
        let term = codegen_term(&term, src);
        return if show.colon_token().is_some() {
            let body = codegen_body(show.body().as_ref(), src);
            quote!(::themelios_program::program::Show::term_body(#term, #body))
        } else {
            quote!(::themelios_program::program::Show::Term(#term))
        };
    }
    quote!(::themelios_program::program::Show::All)
}

/// A signature (grammar §5.9), emitting `Signature::new` (program §7.1): a strong sign, a
/// name, and an arity. A name or arity the parse left absent, or an arity past the
/// engine's width, is a located compile error — a signature the value cannot complete,
/// coincident with the raise's `incomplete` (program §8).
fn codegen_signature(signature: &ast::Signature, src: &MacroSource) -> TokenStream {
    let (Some(name), Some(arity)) = (
        signature.name(),
        signature.arity().as_ref().and_then(arity_of),
    ) else {
        return located_compile_error(
            src,
            signature.syntax().text_range(),
            "this signature is incomplete",
        );
    };
    let name = codegen_name(name.text());
    let sign = if signature.strong_negation_token().is_some() {
        quote!(::themelios_program::symbol::Sign::Negative)
    } else {
        quote!(::themelios_program::symbol::Sign::Positive)
    };
    quote!(::themelios_program::symbol::Signature::new(#sign, #name, #arity))
}

/// An `#external` directive (§4.8), emitting `External::new` (program §7.1): the atom, its
/// body, and the optional carried-not-meaningful value (grammar §13). An atom the parse
/// left absent is a located compile error, coincident with the raise's `incomplete` (§8).
fn codegen_external(external: &ast::ExternalStatement, src: &MacroSource) -> TokenStream {
    let Some(atom) = external.atom() else {
        return located_compile_error(
            src,
            external.syntax().text_range(),
            "this `#external` is incomplete",
        );
    };
    let atom = codegen_atom(&atom, src);
    let body = codegen_body(external.body().as_ref(), src);
    let value = external.value().map_or_else(
        || quote!(::std::option::Option::None),
        |term| {
            let term = codegen_term(&term, src);
            quote!(::std::option::Option::Some(#term))
        },
    );
    quote!(::themelios_program::program::External::new(#atom, #body, #value))
}

/// An optimization statement (§4.7), emitting `minimize`/`maximize` (program §7.1): the
/// direction the keyword the parse carries names, over the optimize elements. `#minimize`
/// and `#maximize` are the two directions; a directive macro's own keyword fixes which.
fn codegen_optimize(optimize: &ast::OptimizeStatement, src: &MacroSource) -> TokenStream {
    let elements: Vec<TokenStream> = optimize
        .elements()
        .map(|element| codegen_optimize_element(&element, src))
        .collect();
    let maximize = optimize
        .keyword_token()
        .is_some_and(|token| token.kind() == SyntaxKind::KW_MAXIMIZE);
    if maximize {
        quote!(::themelios_program::construct::maximize([#(#elements),*]))
    } else {
        quote!(::themelios_program::construct::minimize([#(#elements),*]))
    }
}

/// An optimize element (grammar §5.7), emitting `OptimizeElement::new` (program §7.1): a
/// weight at a priority, a term tuple, and a condition (§4.7).
fn codegen_optimize_element(element: &ast::OptimizeElement, src: &MacroSource) -> TokenStream {
    let weight = codegen_weight(element.weight(), element.priority(), src);
    let terms: Vec<TokenStream> = element
        .tuple()
        .map(|term| codegen_term(&term, src))
        .collect();
    let condition = codegen_condition(element.condition().as_ref(), src);
    quote!(::themelios_program::program::OptimizeElement::new(
        #weight,
        [#(#terms),*],
        #condition,
    ))
}

/// A `weight@priority` (§4.7), emitting `weight(w)[.at_priority(p)]` (program §7.1). The
/// weight is mandatory (grammar §5.7); one the recovery left absent is the [`placeholder`]
/// beside the syntax diagnostic that flags it, as the raise's `step_term` is (§8).
fn codegen_weight(
    weight: Option<ast::Term>,
    priority: Option<ast::Term>,
    src: &MacroSource,
) -> TokenStream {
    let weight = weight.map_or_else(placeholder, |term| codegen_term(&term, src));
    let base = quote!(::themelios_program::program::weight(#weight));
    match priority {
        Some(term) => {
            let term = codegen_term(&term, src);
            quote!(#base.at_priority(#term))
        }
        None => base,
    }
}

/// The program-tier default negation an AST negation prefix names (§4.5).
fn default_negation(negation: ast::Negation) -> TokenStream {
    match negation {
        ast::Negation::None => quote!(::themelios_program::program::DefaultNegation::None),
        ast::Negation::Default => quote!(::themelios_program::program::DefaultNegation::Not),
        ast::Negation::DoubleDefault => {
            quote!(::themelios_program::program::DefaultNegation::NotNot)
        }
    }
}

/// The program-tier relation an AST relation names (§4.6).
fn relation_of(relation: ast::Relation) -> TokenStream {
    match relation {
        ast::Relation::Lt => quote!(::themelios_program::program::Relation::Lt),
        ast::Relation::Le => quote!(::themelios_program::program::Relation::Le),
        ast::Relation::Gt => quote!(::themelios_program::program::Relation::Gt),
        ast::Relation::Ge => quote!(::themelios_program::program::Relation::Ge),
        ast::Relation::Eq => quote!(::themelios_program::program::Relation::Eq),
        ast::Relation::Neq => quote!(::themelios_program::program::Relation::Neq),
    }
}

/// The `u32` an arity numeral denotes under its radix, or `None` on overflow (grammar
/// §5.9) — the raise's `number_u32`, mirrored so the codegen reads an arity one way.
fn arity_of(number: &ast::NumberLit) -> Option<u32> {
    let radix = match number.radix() {
        ast::Radix::Decimal => 10,
        ast::Radix::Hexadecimal => 16,
        ast::Radix::Octal => 8,
        ast::Radix::Binary => 2,
    };
    u32::from_str_radix(number.digits(), radix).ok()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;

    use super::*;
    use crate::engine::{parse_statement_fragment, parse_term_fragment};

    /// The emitted constructor-call stream, as a string, for the term the
    /// macro source `input` assembles and parses to under the dialect. These
    /// goldens freeze each arm's emission as a change-detector; the *value*
    /// proof — that the emission builds the right value — is the equality
    /// witness beside them (tests/equality.rs; program §16).
    fn codegen(input: &str) -> String {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let term = parse_term_fragment(&src).tree().term().expect("a term");
        codegen_term(&term, &src).to_string()
    }

    /// The emitted constructor-call stream, as a string, for the statement the
    /// macro source `input` (its own terminating `.` included) assembles and
    /// parses to. The change-detector twin of [`codegen`] at statement grain;
    /// the value proof is the per-macro equality witness (tests/equality.rs).
    fn codegen_stmt(input: &str) -> String {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let statement = parse_statement_fragment(&src)
            .tree()
            .statement()
            .expect("a statement");
        codegen_statement(&statement, &src).to_string()
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
    fn a_term_splice_crosses_to_symbol_then_into_a_term() {
        // A `$x` in term position crosses the conversion pillar: `ToSymbol` to a
        // ground `Symbol`, then `From<Symbol> for Term` (docs/design/macros.md §7).
        let ts = codegen("$x");
        assert!(ts.contains("ToSymbol :: to_symbol"), "{ts}");
        assert!(ts.contains("Term :: from"), "{ts}");
        // The captured Rust operand is spliced into the crossing.
        assert!(ts.contains("& (x)"), "{ts}");
    }

    #[test]
    fn a_parenthesized_term_splice_recovers_its_expression() {
        // The source captures `$( … )`'s inner tokens; the crossing splices them.
        let ts = codegen("$(a + b)");
        assert!(ts.contains("ToSymbol :: to_symbol"), "{ts}");
        assert!(ts.contains("Term :: from"), "{ts}");
        assert!(ts.contains("a + b"), "{ts}");
    }

    #[test]
    fn a_nested_function_recurses_through_its_arguments() {
        let ts = codegen("f(g(h(X)))");
        assert_eq!(ts.matches("Term :: function").count(), 3, "{ts}");
        assert!(ts.contains("Term :: variable"), "{ts}");
    }

    /// The `ast::TheoryAtom` the macro source `input` assembles and parses to, with
    /// its source. `input` is a whole statement carrying the atom (a fact head or a
    /// constraint body), the theory atom read from its tree (docs/design/macros.md
    /// §8) — the door the statement macros reach a theory atom through.
    fn theory_atom_of(input: &str) -> (ast::TheoryAtom, MacroSource) {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let atom = parse_statement_fragment(&src)
            .syntax()
            .descendants()
            .find_map(ast::TheoryAtom::cast)
            .expect("a theory atom");
        (atom, src)
    }

    /// The first `ast::TheoryTerm` (pre-order) in the statement `input` assembles
    /// and parses to, with its source — `input` crafted so the node under test is
    /// that first theory term.
    fn first_theory_term(input: &str) -> (ast::TheoryTerm, MacroSource) {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let term = parse_statement_fragment(&src)
            .syntax()
            .descendants()
            .find_map(ast::TheoryTerm::cast)
            .expect("a theory term");
        (term, src)
    }

    /// The emitted constructor-call stream, as a string, for the first theory term
    /// of `input`. As for [`codegen`], the goldens pin each arm by its emitted call;
    /// the value witness accretes with the first macro (program §16).
    fn theory_term_codegen(input: &str) -> String {
        let (term, src) = first_theory_term(input);
        codegen_theory_term(&term, &src).to_string()
    }

    #[test]
    fn codegen_of_a_theory_atom_builds_through_the_theory_constructors() {
        // A guarded theory atom with an `Operation` element (the numeral kept off
        // the terminator, inside the braces; the guard bound a constant `n`, so no
        // `<digit>.` fuses to a Rust float).
        let (atom, src) = theory_atom_of(":- &sum { X + 1 } <= n.");
        let ts = codegen_theory_atom(&atom, &src).to_string();
        assert!(ts.contains("TheoryAtom :: new"), "{ts}");
        assert!(ts.contains("TheoryElement :: new"), "{ts}");
        assert!(ts.contains("TheoryGuard"), "{ts}");
        assert!(ts.contains("TheoryTerm :: Operation"), "{ts}");
        assert!(ts.contains("TheoryOperator :: new"), "{ts}");
    }

    #[test]
    fn a_theory_atom_carries_its_ordinary_arguments_and_no_guard() {
        // The ordinary-argument list codegens through `codegen_term` (the §17
        // exception); a bare `&p { a }` has no guard, so the guard is `None`.
        let (atom, src) = theory_atom_of("&p(1, k) { a }.");
        let ts = codegen_theory_atom(&atom, &src).to_string();
        assert!(ts.contains("TheoryAtom :: new"), "{ts}");
        assert!(ts.contains("Name :: new (\"p\")"), "{ts}");
        assert!(ts.contains("Term :: from (1i32)"), "{ts}");
        assert!(ts.contains("Option :: None"), "{ts}");
        assert!(!ts.contains("TheoryGuard"), "{ts}");
    }

    #[test]
    fn a_bare_symbolic_theory_term_lifts_a_symbol() {
        let ts = theory_term_codegen("&sum { a }.");
        assert!(ts.contains("TheoryTerm :: Symbolic"), "{ts}");
        assert!(ts.contains("Symbol :: Function"), "{ts}");
        assert!(ts.contains("Name :: new (\"a\")"), "{ts}");
        assert!(ts.contains("Sign :: Positive"), "{ts}");
    }

    #[test]
    fn a_numeric_symbolic_theory_term_lifts_symbol_number() {
        // The numeral sits inside the braces, off the statement terminator.
        let ts = theory_term_codegen("&sum { 7 }.");
        assert!(ts.contains("TheoryTerm :: Symbolic"), "{ts}");
        assert!(ts.contains("Symbol :: Number (7i32)"), "{ts}");
    }

    #[test]
    fn a_variable_theory_term_calls_variable_named() {
        let ts = theory_term_codegen("&sum { X }.");
        assert!(ts.contains("TheoryTerm :: Variable"), "{ts}");
        assert!(ts.contains("Variable :: Named"), "{ts}");
        assert!(ts.contains("VarName :: new (\"X\")"), "{ts}");
    }

    #[test]
    fn a_function_theory_term_recurses_through_its_arguments() {
        let ts = theory_term_codegen("&sum { f(a) }.");
        assert!(ts.contains("TheoryTerm :: Function"), "{ts}");
        assert!(ts.contains("Name :: new (\"f\")"), "{ts}");
        // The argument recurses to a symbolic leaf.
        assert!(ts.contains("TheoryTerm :: Symbolic"), "{ts}");
    }

    #[test]
    fn a_tuple_theory_term_builds_the_tuple_variant() {
        let ts = theory_term_codegen("&sum { (a, b) }.");
        assert!(ts.contains("TheoryTerm :: Tuple"), "{ts}");
    }

    #[test]
    fn a_set_theory_term_builds_the_set_variant() {
        let ts = theory_term_codegen("&sum { {a, b} }.");
        assert!(ts.contains("TheoryTerm :: Set"), "{ts}");
    }

    #[test]
    fn a_list_theory_term_builds_the_list_variant() {
        let ts = theory_term_codegen("&sum { [a, b] }.");
        assert!(ts.contains("TheoryTerm :: List"), "{ts}");
    }

    #[test]
    fn an_operation_run_records_the_operator_before_each_operand() {
        // `X + 1`: a leading empty run before `X`, then the `+` run before `1`
        // (`operators[i]` is the run before `operands[i]`, program §4.9). The run
        // is formed at the opterm level, reached through the atom's element.
        let (atom, src) = theory_atom_of("&sum { X + 1 }.");
        let ts = codegen_theory_atom(&atom, &src).to_string();
        assert!(ts.contains("TheoryTerm :: Operation"), "{ts}");
        // The arranged shape, not merely the pieces: the operator-run vector leads with
        // an *empty* run (before `X`), then the `+` run (before `1`), pinning the
        // `vec![vec![], vec![…"+"…]]` arrangement the value witness proves (§16).
        assert!(
            ts.contains(":: std :: vec ! [:: std :: vec ! [] , :: std :: vec ! ["),
            "{ts}"
        );
        assert!(ts.contains("TheoryOperator :: new (\"+\")"), "{ts}");
        assert!(ts.contains("TheoryTerm :: Variable"), "{ts}");
        assert!(ts.contains("Symbol :: Number (1i32)"), "{ts}");
    }

    #[test]
    fn a_theory_term_splice_crosses_to_symbol_into_a_symbolic_term() {
        // A `$x` in theory-term position crosses `ToSymbol`, then lands as the
        // theory algebra's ground-symbol leaf (docs/design/macros.md §7).
        let (term, src) = first_theory_term("&sum { $x }.");
        let ts = codegen_theory_term(&term, &src).to_string();
        assert!(ts.contains("ToSymbol :: to_symbol"), "{ts}");
        assert!(ts.contains("TheoryTerm :: Symbolic"), "{ts}");
        assert!(ts.contains("& (x)"), "{ts}");
    }

    #[test]
    fn a_conditioned_theory_element_lowers_its_condition() {
        // A theory element's condition is a `Condition` of ordinary literals, lowered
        // through the same door a rule body's condition takes — no longer a deferred
        // seam. The `Some(condition)` is emitted, the condition built through
        // `Condition::new` over the literal's `Literal`, never a dropped condition.
        let (atom, src) = theory_atom_of("&sum { X : p(X) }.");
        let ts = codegen_theory_atom(&atom, &src).to_string();
        assert!(!ts.contains("compile_error"), "{ts}");
        assert!(ts.contains("Option :: Some"), "{ts}");
        assert!(ts.contains("Condition :: new"), "{ts}");
        assert!(ts.contains("Name :: new (\"p\")"), "{ts}");
    }

    // ---- the theory-symbol leaf arms (§4.9): the non-numeric, non-function leaves ----

    #[test]
    fn a_string_symbolic_theory_term_lifts_symbol_string() {
        let ts = theory_term_codegen(r#"&sum { "hi" }."#);
        assert!(ts.contains("TheoryTerm :: Symbolic"), "{ts}");
        assert!(ts.contains("Symbol :: string (\"hi\")"), "{ts}");
    }

    #[test]
    fn the_order_bounds_lift_their_theory_symbols() {
        assert!(theory_term_codegen("&sum { #inf }.").contains("Symbol :: Infimum"));
        assert!(theory_term_codegen("&sum { #sup }.").contains("Symbol :: Supremum"));
    }

    // ---- statement codegen (§8): the change-detector goldens, the value proof beside
    // them in tests/equality.rs ----

    #[test]
    fn a_fact_calls_rule_fact() {
        let ts = codegen_stmt("p(1, a).");
        assert!(ts.contains("Rule :: fact"), "{ts}");
        assert!(ts.contains("Atom :: new"), "{ts}");
    }

    #[test]
    fn a_rule_calls_rule_new_over_head_and_body() {
        let ts = codegen_stmt("q(X) :- p(X).");
        assert!(ts.contains("Rule :: new"), "{ts}");
        assert!(ts.contains("Body :: new"), "{ts}");
        assert!(ts.contains("BodyElement :: from"), "{ts}");
    }

    #[test]
    fn a_constraint_calls_rule_constraint() {
        let ts = codegen_stmt(":- p(X).");
        assert!(ts.contains("Rule :: constraint"), "{ts}");
    }

    #[test]
    fn a_strong_negated_atom_head_wraps_in_neg() {
        // `-p` in a statement head is strong negation the tree resolved positionally;
        // the codegen wraps the atom in the `Neg` operator (program §4.6).
        let ts = codegen_stmt("-p(X).");
        assert!(ts.contains("(- ::"), "{ts}");
        assert!(ts.contains("Atom :: new"), "{ts}");
    }

    #[test]
    fn a_pooled_argument_list_atom_calls_atom_pooled() {
        // `p(a; b)` is an argument-list pool of two alternatives (program §8).
        let ts = codegen_stmt("p(a; b).");
        assert!(ts.contains("Atom :: pooled"), "{ts}");
        assert!(
            ts.contains(". expect (\"the grammar parsed a non-empty argument-list pool\")"),
            "{ts}"
        );
    }

    #[test]
    fn a_default_negated_body_literal_carries_its_negation() {
        assert!(codegen_stmt(":- not p.").contains("DefaultNegation :: Not"));
        assert!(codegen_stmt(":- not not p.").contains("DefaultNegation :: NotNot"));
    }

    #[test]
    fn a_boolean_body_literal_builds_its_inner() {
        assert!(codegen_stmt(":- #true.").contains("LiteralInner :: True"));
        assert!(codegen_stmt(":- #false.").contains("LiteralInner :: False"));
    }

    #[test]
    fn a_comparison_body_literal_calls_comparison_new() {
        let ts = codegen_stmt(":- 1 < X.");
        assert!(ts.contains("Comparison :: new"), "{ts}");
        assert!(ts.contains("Relation :: Lt"), "{ts}");
    }

    #[test]
    fn a_chained_comparison_calls_comparison_chain() {
        // `1 < X < 5` is one literal carrying a guard sequence, not a conjunction.
        let ts = codegen_stmt(":- 1 < X < 5 .");
        assert!(ts.contains("Comparison :: new"), "{ts}");
        assert!(ts.contains(". chain"), "{ts}");
    }

    #[test]
    fn a_conditional_body_literal_builds_a_conditional_literal() {
        let ts = codegen_stmt(":- p : q.");
        assert!(ts.contains("ConditionalLiteral"), "{ts}");
        assert!(ts.contains("Condition :: new"), "{ts}");
    }

    #[test]
    fn a_negated_theory_atom_body_element_calls_not() {
        let ts = codegen_stmt(":- not &sum { X } <= 3 .");
        assert!(ts.contains("construct :: not"), "{ts}");
        assert!(ts.contains("TheoryAtom :: new"), "{ts}");
    }

    #[test]
    fn a_theory_atom_head_fact_builds_through_the_theory_constructor() {
        let ts = codegen_stmt("&sum { X } <= 3 .");
        assert!(ts.contains("Rule :: fact"), "{ts}");
        assert!(ts.contains("TheoryAtom :: new"), "{ts}");
    }

    #[test]
    fn a_show_signature_builds_the_variant() {
        let ts = codegen_stmt("#show p/1 .");
        assert!(ts.contains("Show :: Signature"), "{ts}");
        assert!(ts.contains("Signature :: new"), "{ts}");
        assert!(ts.contains("Sign :: Positive"), "{ts}");
    }

    #[test]
    fn a_show_strong_negated_signature_reads_the_sign() {
        assert!(codegen_stmt("#show -p/1 .").contains("Sign :: Negative"));
    }

    #[test]
    fn a_show_term_and_term_body_build_their_variants() {
        assert!(codegen_stmt("#show a.").contains("Show :: Term"));
        let bodied = codegen_stmt("#show a : p(X).");
        assert!(bodied.contains("Show :: term_body"), "{bodied}");
        assert!(bodied.contains("Body :: new"), "{bodied}");
    }

    #[test]
    fn a_bare_show_builds_show_all() {
        assert!(codegen_stmt("#show.").contains("Show :: All"));
    }

    #[test]
    fn an_external_builds_through_external_new() {
        let ts = codegen_stmt("#external p(X).");
        assert!(ts.contains("External :: new"), "{ts}");
        assert!(ts.contains("Body :: empty"), "{ts}");
        assert!(ts.contains("Option :: None"), "{ts}");
    }

    #[test]
    fn an_external_carries_its_value() {
        // The value rides in the post-dot annotation (`#external p. [v]`, grammar §13);
        // the codegen carries it as `Some`, its span the annotation's.
        let ts = codegen_stmt("#external p(X). [a]");
        assert!(ts.contains("External :: new"), "{ts}");
        assert!(ts.contains("Option :: Some"), "{ts}");
    }

    #[test]
    fn minimize_and_maximize_build_their_directions() {
        let least = codegen_stmt("#minimize { 3@1 }.");
        assert!(least.contains("construct :: minimize"), "{least}");
        assert!(least.contains("OptimizeElement :: new"), "{least}");
        assert!(least.contains("weight"), "{least}");
        assert!(least.contains(". at_priority"), "{least}");
        assert!(
            codegen_stmt("#maximize { 5 }.").contains("construct :: maximize"),
            "maximize"
        );
    }

    #[test]
    fn a_disjunction_head_is_a_located_error_for_now() {
        assert!(codegen_stmt("a | b.").contains("compile_error"));
    }

    #[test]
    fn a_choice_head_is_a_located_error_for_now() {
        assert!(codegen_stmt("{ a }.").contains("compile_error"));
    }

    #[test]
    fn a_body_aggregate_is_a_located_error_for_now() {
        assert!(codegen_stmt(":- 1 <= #count { X : p(X) }.").contains("compile_error"));
    }

    #[test]
    fn a_statement_family_no_macro_builds_is_a_located_error() {
        // A well-formed directive no statement macro targets — reached for with the
        // wrong macro — is a construction-site error, not a fabricated value.
        assert!(codegen_stmt("#project p/1 .").contains("compile_error"));
    }

    // ---- head-atom codegen (§8): the `atom!` door, reaching an atom by extracting a fact's
    // head; the value proof is the equality witness beside these goldens (tests/equality.rs) ----

    /// The emitted constructor-call stream, as a string, for the single head atom the macro
    /// source `input` (an assembled fact, its terminating `.` included) reaches through
    /// [`codegen_head_atom`] — the `atom!` door. The change-detector twin of [`codegen_stmt`]
    /// at head-atom grain; the value proof is the per-macro equality witness.
    fn codegen_head_atom_str(input: &str) -> String {
        let src = MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect");
        let statement = parse_statement_fragment(&src)
            .tree()
            .statement()
            .expect("a statement");
        codegen_head_atom(&statement, &src).to_string()
    }

    #[test]
    fn a_head_atom_calls_atom_new() {
        let ts = codegen_head_atom_str("p(1, a).");
        assert!(ts.contains("Atom :: new"), "{ts}");
        assert!(!ts.contains("compile_error"), "{ts}");
    }

    #[test]
    fn a_strong_negated_head_atom_wraps_in_neg() {
        // `-p` is the atom's positional strong sign (program §3.3), read by `codegen_atom`
        // into the `Neg` operator — not the term door's arithmetic negation.
        let ts = codegen_head_atom_str("-p(1).");
        assert!(ts.contains("(- ::"), "{ts}");
        assert!(ts.contains("Atom :: new"), "{ts}");
    }

    #[test]
    fn a_pooled_argument_head_atom_calls_atom_pooled() {
        // The head atom rides the same `codegen_atom` door as a statement head, so an
        // argument-list pool reaches `Atom::pooled` (program §8).
        assert!(codegen_head_atom_str("p(a; b).").contains("Atom :: pooled"));
    }

    #[test]
    fn a_head_that_is_not_a_single_atom_is_a_located_error() {
        // Every head shape that is not a single ordinary atom refuses with one located error
        // (§8), never a fabricated or silently-stripped atom: a disjunction (both the `|` and
        // `;` spellings), a choice head, a theory-atom head, a comparison, a boolean, a
        // default-negated literal (its `not` has nowhere to go in an `Atom`), a constraint's
        // absent head, and a non-rule statement reached for with the wrong macro.
        for input in [
            "a | b.",
            "a ; b.",
            "{ a }.",
            "&sum { X } <= 3 .",
            "1 < 2 .",
            "#true.",
            "not p.",
            ":- p.",
            "#show p/1 .",
        ] {
            assert!(
                codegen_head_atom_str(input).contains("compile_error"),
                "{input}"
            );
        }
    }
}
