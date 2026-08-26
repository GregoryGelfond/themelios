//! The term raise (docs/design/program.md §8): the parsing relation's term half.
//! It lowers the syntax tier's parsed term fragment (grammar §5.1) to a `term::Term`,
//! re-associating the flat per-precedence operator chain into the operator tree
//! (exponentiation right-associative, everything else left), reading each string
//! through the parse's own dialect (§8), and collapsing maximal ground constructor
//! subterms to symbols (§5.1). Composed with the syntax tier's `parse_term_value` and
//! `Term::evaluate` (§3.5), `parse_term_value → raise_term → evaluate → Symbol` is the
//! string-to-symbol path a REPL or query surface wants, and this is its middle step.
//!
//! The raise is **total** (§8, syntax §12.4): it never panics and never refuses by
//! `Result`. A fragment that held no term, or ended before its construct did, raises
//! to `(None, …)`; a subterm the parser recovered but the value cannot represent — a
//! numeral beyond the engine's width, a malformed valued token from a foreign token
//! source, a macro splice, a place recovery left a required operand absent — becomes a
//! [`LowerError`] beside a best-effort partial. The assembly descends the borrowed AST
//! on an explicit work list rather than the call stack, so a deep term fragment raises
//! without overflow (§13).

use themelios_base::span::Location;
use themelios_syntax::ast::{self, Associativity, AstToken, Constant, Radix};
use themelios_syntax::parse::Parse;
use themelios_syntax::tree::{AstNode, SyntaxKind, TextRange};

use crate::symbol::{Name, Sign, Symbol, VarName};
use crate::term::{BinaryOp, Term, UnaryOp, Variable};

/// A located diagnostic the raise emits when a recovered fragment holds a term the
/// value cannot represent (program §8): the offending region, by span, and its kind.
/// Owned plain data (`Send + Sync + 'static`); the kinds the statement and directive
/// raise add join the enum later, which is why it is `#[non_exhaustive]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LowerError {
    location: Location,
    kind: LowerErrorKind,
}

impl LowerError {
    /// Where the offending region is (base §4.3).
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// What could not be represented.
    pub fn kind(&self) -> &LowerErrorKind {
        &self.kind
    }
}

/// The ways a recovered term defeats the value (program §8). The term raise emits
/// these; the statement and directive raise add their own, so a consumer's match
/// carries a wildcard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum LowerErrorKind {
    /// A numeral beyond the engine's `i32` range (§3.1).
    NumberOutOfRange,
    /// A valued token — a name, variable, or string — whose spelling the value's rule
    /// refuses; the file lexer never emits one, so only a foreign token source can.
    MalformedToken,
    /// The parser's recovery left a required subterm absent.
    IncompleteTerm,
    /// A macro splice reached the raise unexpanded (the macro surface owns splices).
    UnexpandedSplice,
}

/// Lower a parsed term fragment to a term (program §8, §5.1). Total: `None` when the
/// fragment held no term or ended before its construct did (a read-more signal), a
/// best-effort `Some` otherwise, with a [`LowerError`] for every region the value
/// could not represent. The output term is canonical (§5.1). O(tree).
#[must_use]
pub fn raise_term(parse: &Parse<ast::TermFragment>) -> (Option<Term>, Vec<LowerError>) {
    // Input that ended mid-construct is a read-more signal, not a term to recover; its
    // errors are the parse's (syntax), not the raise's (lowering).
    if parse.is_incomplete() {
        return (None, Vec::new());
    }
    let Some(root) = parse.tree().term() else {
        return (None, Vec::new());
    };
    let mut errors = Vec::new();
    let term = assemble_tree(root, parse, &mut errors).canonicalize();
    (Some(term), errors)
}

/// One work-list step: enter a node (schedule its children, then its own assembly), or
/// assemble a node from the `child_count` already-raised terms atop the result stack.
enum Step {
    Enter(ast::Term),
    Assemble(ast::Term, usize),
}

/// Descend the borrowed AST on an explicit stack, building the `term::Term` bottom-up
/// so the assembly's depth is the heap's, not the call stack's (§13). Every child is
/// entered before its parent assembles, so a parent always assembles over already-raised
/// children.
fn assemble_tree(
    root: ast::Term,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    let mut work = vec![Step::Enter(root)];
    let mut done: Vec<Term> = Vec::new();
    while let Some(step) = work.pop() {
        match step {
            Step::Enter(node) => {
                let children = child_terms(&node);
                work.push(Step::Assemble(node, children.len()));
                // Reversed, so the children process left-to-right and their results
                // land on `done` in source order.
                for child in children.into_iter().rev() {
                    work.push(Step::Enter(child));
                }
            }
            Step::Assemble(node, count) => {
                let start = done.len().saturating_sub(count);
                let children = done.split_off(start);
                let term = assemble(&node, children, parse, errors);
                done.push(term);
            }
        }
    }
    done.pop().unwrap_or_else(placeholder)
}

/// A node's immediate child terms, in source order, flattening the pool and argument
/// structure so `assemble` can re-read that structure and partition these children by
/// index. The leaves have none.
fn child_terms(node: &ast::Term) -> Vec<ast::Term> {
    match node {
        ast::Term::Binary(binary) => binary.operands().collect(),
        ast::Term::Unary(unary) => unary.operand().into_iter().collect(),
        ast::Term::Pool(pool) => pool.tuples().flat_map(|tuple| tuple.terms()).collect(),
        ast::Term::Function(function) => argument_terms(function.arguments()),
        ast::Term::External(external) => argument_terms(external.arguments()),
        ast::Term::Abs(abs) => abs.terms().collect(),
        ast::Term::Constant(_) | ast::Term::Variable(_) | ast::Term::Splice(_) => Vec::new(),
    }
}

fn argument_terms(arguments: Option<ast::Arguments>) -> Vec<ast::Term> {
    arguments
        .into_iter()
        .flat_map(|arguments| arguments.alternatives())
        .flat_map(|alternative| alternative.terms())
        .collect()
}

/// Build a node's term from its already-raised `children`, re-reading the node's
/// structural data (operators, name, tuple and alternative boundaries) and indexing
/// into `children` in the same order `child_terms` flattened them.
fn assemble(
    node: &ast::Term,
    children: Vec<Term>,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    match node {
        ast::Term::Binary(binary) => reassociate(binary, children, parse, errors),
        ast::Term::Unary(unary) => prefix_run(unary, children, parse, errors),
        ast::Term::Pool(pool) => raise_pool(pool, children),
        ast::Term::Function(function) => raise_application(
            Application::Function,
            function.name(),
            function.arguments(),
            node,
            children,
            parse,
            errors,
        ),
        ast::Term::External(external) => raise_application(
            Application::External,
            external.name(),
            external.arguments(),
            node,
            children,
            parse,
            errors,
        ),
        ast::Term::Abs(_) => raise_absolute(children),
        ast::Term::Constant(constant) => raise_constant(constant, parse, errors),
        ast::Term::Variable(variable) => raise_variable(variable, parse, errors),
        ast::Term::Splice(_) => {
            errors.push(located(
                parse,
                node.syntax().text_range(),
                LowerErrorKind::UnexpandedSplice,
            ));
            placeholder()
        }
    }
}

/// Re-associate the flat per-precedence chain into the operator tree (§8): left at
/// every level, right for exponentiation, each operator token its `BinaryOp` — or the
/// interval former for `..`. Recovery that leaves the operand and operator counts
/// mismatched still folds what is present, beside an [`LowerErrorKind::IncompleteTerm`].
fn reassociate(
    binary: &ast::BinaryTerm,
    children: Vec<Term>,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    let operators: Vec<SyntaxKind> = binary.operators().map(|token| token.kind()).collect();
    if operators.len() + 1 != children.len() {
        errors.push(located(
            parse,
            binary.syntax().text_range(),
            LowerErrorKind::IncompleteTerm,
        ));
    }
    if children.len() < 2 {
        return children.into_iter().next().unwrap_or_else(placeholder);
    }
    if matches!(binary.associativity(), Some(Associativity::Right)) {
        // `t₀ ** (t₁ ** (t₂ …))`: fold from the right over (operator, left-operand) pairs.
        let mut operands = children.into_iter().rev();
        let mut accumulator = operands.next().expect("at least two operands");
        for (operator, left) in operators.into_iter().rev().zip(operands) {
            accumulator = combine(left, operator, accumulator);
        }
        accumulator
    } else {
        // `((t₀ op₀ t₁) op₁ t₂) …`: fold from the left over (operator, right-operand) pairs.
        let mut operands = children.into_iter();
        let mut accumulator = operands.next().expect("at least two operands");
        for (operator, right) in operators.into_iter().zip(operands) {
            accumulator = combine(accumulator, operator, right);
        }
        accumulator
    }
}

/// One binary step: the interval former for `..`, an arithmetic or bitwise
/// `BinaryOperation` otherwise (§4.6, grammar §5.1).
fn combine(left: Term, operator: SyntaxKind, right: Term) -> Term {
    if operator == SyntaxKind::DOTDOT {
        Term::Interval {
            lower: Box::new(left),
            upper: Box::new(right),
        }
    } else {
        Term::BinaryOperation {
            operator: binary_operator(operator),
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

/// The `BinaryOp` of an operator token (grammar §5.1). `..` never reaches here (it is
/// the interval former, handled in [`combine`]); the accessor yields only these kinds,
/// so the fallback is unreachable and kept total.
fn binary_operator(kind: SyntaxKind) -> BinaryOp {
    match kind {
        SyntaxKind::CARET => BinaryOp::BitXor,
        SyntaxKind::QUESTION => BinaryOp::BitOr,
        SyntaxKind::AMPERSAND => BinaryOp::BitAnd,
        SyntaxKind::MINUS => BinaryOp::Sub,
        SyntaxKind::STAR => BinaryOp::Mul,
        SyntaxKind::SLASH => BinaryOp::Div,
        SyntaxKind::BACKSLASH => BinaryOp::Mod,
        SyntaxKind::STAR_STAR => BinaryOp::Pow,
        _ => BinaryOp::Add,
    }
}

/// Apply a run of prefix operators to the operand, innermost first (grammar §5.1): the
/// outermost operator wraps last, so `- ~ X` is `Negate(BitwiseNot(X))`. A run the
/// parser recovered with no operand is a placeholder beside an incompleteness.
fn prefix_run(
    unary: &ast::UnaryTerm,
    children: Vec<Term>,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    let Some(operand) = children.into_iter().next() else {
        errors.push(located(
            parse,
            unary.syntax().text_range(),
            LowerErrorKind::IncompleteTerm,
        ));
        return placeholder();
    };
    unary
        .operators()
        .map(|token| token.kind())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .fold(operand, |argument, kind| Term::UnaryOperation {
            operator: unary_operator(kind),
            argument: Box::new(argument),
        })
}

fn unary_operator(kind: SyntaxKind) -> UnaryOp {
    match kind {
        SyntaxKind::TILDE => UnaryOp::BitwiseNot,
        _ => UnaryOp::Negate,
    }
}

/// Raise a parenthesized form (grammar §5.1): a lone term parenthesized is that term;
/// one tuple is a `Tuple`; several pooled tuples are a `Pool` of the tuple-or-terms.
fn raise_pool(pool: &ast::Pool, children: Vec<Term>) -> Term {
    let shape: Vec<(usize, bool)> = pool
        .tuples()
        .map(|tuple| {
            (
                tuple.terms().count(),
                tuple.trailing_comma_token().is_some(),
            )
        })
        .collect();
    let mut children = children.into_iter();
    let mut alternatives: Vec<Term> = shape
        .into_iter()
        .map(|(size, trailing_comma)| {
            tuple_or_term((&mut children).take(size).collect(), trailing_comma)
        })
        .collect();
    if alternatives.len() == 1 {
        alternatives.pop().expect("one alternative")
    } else {
        Term::Pool(alternatives)
    }
}

/// A single tuple alternative: the lone term when it is one term without a trailing
/// comma (`(a)` parenthesizes `a`), a `Tuple` otherwise (`(a, b)`, `(a,)`, `()`).
fn tuple_or_term(terms: Vec<Term>, trailing_comma: bool) -> Term {
    if terms.len() == 1 && !trailing_comma {
        terms.into_iter().next().expect("one term")
    } else {
        Term::Tuple(terms)
    }
}

/// Whether an application derives a `Function` or an `External` (`@`-call) term.
#[derive(Clone, Copy)]
enum Application {
    Function,
    External,
}

/// Raise a function or `@`-call (grammar §5.1). One argument alternative is a plain
/// application; several — a pooled argument list, `f(a; b)` — distribute to a `Pool` of
/// applications, the pool semantics `Term::Pool` denotes. A name the value's rule
/// refuses is a placeholder beside a malformed-token diagnostic.
fn raise_application(
    application: Application,
    name: Option<ast::Ident>,
    arguments: Option<ast::Arguments>,
    node: &ast::Term,
    children: Vec<Term>,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    let Some(name) = raise_name(name, node, parse, errors) else {
        return placeholder();
    };
    let sizes: Vec<usize> = arguments
        .into_iter()
        .flat_map(|arguments| arguments.alternatives())
        .map(|alternative| alternative.terms().count())
        .collect();
    let mut children = children.into_iter();
    let mut alternatives: Vec<Term> = sizes
        .into_iter()
        .map(|size| {
            apply(
                application,
                name.clone(),
                (&mut children).take(size).collect(),
            )
        })
        .collect();
    match alternatives.len() {
        // A bare `@name` has no argument list; a function always has one.
        0 => apply(application, name, Vec::new()),
        1 => alternatives.pop().expect("one alternative"),
        _ => Term::Pool(alternatives),
    }
}

fn apply(application: Application, name: Name, arguments: Vec<Term>) -> Term {
    match application {
        Application::Function => Term::Function { name, arguments },
        Application::External => Term::External { name, arguments },
    }
}

/// Raise an absolute-value term (grammar §5.1): `|a|` is `Absolute(a)`; a pooled
/// `|a; b|` distributes to a `Pool` of absolute values.
fn raise_absolute(children: Vec<Term>) -> Term {
    if children.len() == 1 {
        Term::Absolute(Box::new(children.into_iter().next().expect("one operand")))
    } else {
        Term::Pool(
            children
                .into_iter()
                .map(|term| Term::Absolute(Box::new(term)))
                .collect(),
        )
    }
}

/// Raise a constant leaf (grammar §5.1): a numeral to `Number` (or a diagnostic beyond
/// the engine's width), an identifier to the empty-argument `Function` a constant is, a
/// string read through the parse's dialect, and `#inf`/`#sup` to the order's bounds.
fn raise_constant(
    constant: &ast::ConstantTerm,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    match constant.constant() {
        Some(Constant::Symbol(identifier)) => match Name::new(identifier.text()) {
            Ok(name) => Term::Symbolic(Symbol::Function {
                name,
                arguments: Vec::new(),
                sign: Sign::Positive,
            }),
            Err(_) => malformed(parse, identifier.syntax().text_range(), errors),
        },
        Some(Constant::Number(number)) => {
            if let Some(value) = integer(&number) {
                Term::Symbolic(Symbol::Number(value))
            } else {
                errors.push(located(
                    parse,
                    number.syntax().text_range(),
                    LowerErrorKind::NumberOutOfRange,
                ));
                placeholder()
            }
        }
        Some(Constant::String(string)) => match parse.string_value(&string) {
            Ok(text) => Term::Symbolic(Symbol::String(text)),
            Err(_) => malformed(parse, string.syntax().text_range(), errors),
        },
        Some(Constant::Infimum(_)) => Term::Symbolic(Symbol::Infimum),
        Some(Constant::Supremum(_)) => Term::Symbolic(Symbol::Supremum),
        None => {
            errors.push(located(
                parse,
                constant.syntax().text_range(),
                LowerErrorKind::IncompleteTerm,
            ));
            placeholder()
        }
    }
}

/// The `i32` a numeral denotes under its radix, or `None` when it overflows the
/// engine's width (§3.1).
fn integer(number: &ast::NumberLit) -> Option<i32> {
    let radix = match number.radix() {
        Radix::Decimal => 10,
        Radix::Hexadecimal => 16,
        Radix::Octal => 8,
        Radix::Binary => 2,
    };
    i32::from_str_radix(number.digits(), radix).ok()
}

/// Raise a variable leaf (grammar §5.1): the anonymous `_`, or a named variable.
fn raise_variable(
    variable: &ast::VariableTerm,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Term {
    match variable.variable() {
        Some(inner) if inner.is_anonymous() => Term::Variable(Variable::Anonymous),
        Some(inner) => match VarName::new(inner.text()) {
            Ok(name) => Term::Variable(Variable::Named(name)),
            Err(_) => malformed(parse, inner.syntax().text_range(), errors),
        },
        None => {
            errors.push(located(
                parse,
                variable.syntax().text_range(),
                LowerErrorKind::IncompleteTerm,
            ));
            placeholder()
        }
    }
}

/// The validated name of an identifier, or `None` (with a diagnostic) when it is
/// missing under recovery or its spelling is refused by a foreign token source.
fn raise_name(
    name: Option<ast::Ident>,
    node: &ast::Term,
    parse: &Parse<ast::TermFragment>,
    errors: &mut Vec<LowerError>,
) -> Option<Name> {
    let Some(identifier) = name else {
        errors.push(located(
            parse,
            node.syntax().text_range(),
            LowerErrorKind::IncompleteTerm,
        ));
        return None;
    };
    if let Ok(name) = Name::new(identifier.text()) {
        Some(name)
    } else {
        errors.push(located(
            parse,
            identifier.syntax().text_range(),
            LowerErrorKind::MalformedToken,
        ));
        None
    }
}

/// A malformed valued token: the placeholder, beside a malformed-token diagnostic.
fn malformed(
    parse: &Parse<ast::TermFragment>,
    range: TextRange,
    errors: &mut Vec<LowerError>,
) -> Term {
    errors.push(located(parse, range, LowerErrorKind::MalformedToken));
    placeholder()
}

/// A located lowering error at `range` under the parse's source (base §4.3).
fn located(parse: &Parse<ast::TermFragment>, range: TextRange, kind: LowerErrorKind) -> LowerError {
    LowerError {
        location: parse.location(range),
        kind,
    }
}

/// The best-effort stand-in for a subterm the value cannot represent — an anonymous
/// variable, which keeps a partial term assemblable so the diagnostic beside it is the
/// signal, not a panic.
fn placeholder() -> Term {
    Term::Variable(Variable::Anonymous)
}
