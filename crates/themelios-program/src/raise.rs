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

use std::collections::BTreeSet;
use std::fmt;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity, ToDiagnostic};
use themelios_base::span::Location;
use themelios_syntax::ast::{
    self, Associativity, AstToken, Constant, HasDocs, HasGuards, Negation, Radix,
};
use themelios_syntax::parse::Parse;
use themelios_syntax::tree::{Asp, AstNode, SyntaxKind, SyntaxNode, TextRange};

use crate::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, ConstPolicy, DefaultNegation,
    Defined, Direction, Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard,
    Head, HeadAggregate, HeadAggregateElement, Heuristic, Include, IncludeTarget, Literal,
    LiteralInner, Optimize, OptimizeElement, PartKey, Program, Project, Query, Relation, Rule,
    Script, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryAtomDefinition,
    TheoryAtomGuardDefinition, TheoryDefinition, TheoryElement, TheoryGuard, TheoryOccurrence,
    TheoryOperator, TheoryOperatorArity, TheoryOperatorDefinition, TheoryTerm,
    TheoryTermDefinition, WeakConstraint, Weight, base_key, weight,
};
use crate::provenance::{Origin, Provenance, WithProvenance};
use crate::symbol::{Name, Sign, Signature, Symbol, VarName};
use crate::term::{BinaryOp, Term, UnaryOp, Variable};

/// What the term assembly reads from the parse it lowers under: a located span and a
/// dialect-correct string read (§8). Both are `Parse<T>` operations for any typed root
/// (syntax §5.5), so the term door and the statement door share one term assembly.
trait Reads {
    fn locate(&self, range: TextRange) -> Location;
    fn read_string(&self, literal: &ast::StringLit) -> Result<String, ast::InvalidStringLiteral>;
}

impl<T: AstNode<Language = Asp>> Reads for Parse<T> {
    fn locate(&self, range: TextRange) -> Location {
        self.location(range)
    }

    fn read_string(&self, literal: &ast::StringLit) -> Result<String, ast::InvalidStringLiteral> {
        self.string_value(literal)
    }
}

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
    /// A recovered statement the value cannot complete — a required node the parser's
    /// recovery left absent (§8), diagnosed and skipped so its neighbors still raise.
    IncompleteStatement,
    /// A `#const` value outside the constant-term subset (grammar §5.9): a variable, a pool,
    /// or an interval where a constant term must stand (§4.8). The value is carried
    /// unevaluated all the same; this marks it as one a constant may not take.
    NonConstantValue,
}

impl LowerErrorKind {
    /// The stable machine identity and headline of this diagnostic kind (base §6.1,
    /// §6.5): a `program`-namespace identity and a message at the rust-analyzer bar.
    fn report(self) -> (DiagnosticId, &'static str) {
        match self {
            LowerErrorKind::NumberOutOfRange => (
                DiagnosticId::new("program", "number-out-of-range"),
                "this numeral is outside the representable range",
            ),
            LowerErrorKind::MalformedToken => (
                DiagnosticId::new("program", "malformed-token"),
                "this token cannot be read as the value it stands for",
            ),
            LowerErrorKind::IncompleteTerm => (
                DiagnosticId::new("program", "incomplete-term"),
                "this term is incomplete",
            ),
            LowerErrorKind::UnexpandedSplice => (
                DiagnosticId::new("program", "unexpanded-splice"),
                "this splice reached the program without being expanded",
            ),
            LowerErrorKind::IncompleteStatement => (
                DiagnosticId::new("program", "incomplete-statement"),
                "this statement is incomplete",
            ),
            LowerErrorKind::NonConstantValue => (
                DiagnosticId::new("program", "non-constant-value"),
                "a constant's value must be a constant term",
            ),
        }
    }
}

impl fmt::Display for LowerErrorKind {
    /// The kind's headline — the message base's human view leads with (§6.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.report().1)
    }
}

impl ToDiagnostic for LowerError {
    /// The lowering diagnostic in base's normal form (base §6.5): the `program`-space
    /// identity, its headline, and the offending region as the primary label — the
    /// syntax tier's diagnostics and these share one model, so a consumer renders both
    /// alike (§8).
    fn to_diagnostic(&self) -> Diagnostic {
        let (id, message) = self.kind.report();
        Diagnostic::new(
            id,
            Severity::Error,
            message.to_owned(),
            Label {
                location: self.location,
                message: None,
            },
        )
        .expect("a lowering diagnostic's headline is never empty")
    }
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
fn assemble_tree(root: ast::Term, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Term {
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
                // Net-one-push invariant: entering a node pushed one `Enter` per child, and
                // every subtree leaves exactly one result on `done`, so exactly `count`
                // results sit atop `done` now — `done.len() - count` is exact, never underflows.
                let children = done.split_off(done.len() - count);
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
    parse: &dyn Reads,
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
    parse: &dyn Reads,
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
/// the interval former, handled in [`combine`]). The listed arms name the operators whose
/// `BinaryOp` is not `Add`; `+` (`PLUS`) and any token kind the grammar's binary operators
/// cannot yield both map to `Add` through the wildcard — so the wildcard is a **real arm,
/// reached on every `+`**, not a dead fallback. Keep it total: a panic here would fault on
/// a `+` (a public surface must not panic, §15).
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
    parse: &dyn Reads,
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
    parse: &dyn Reads,
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
    parse: &dyn Reads,
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
        Some(Constant::String(string)) => match parse.read_string(&string) {
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
    parse: &dyn Reads,
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
    parse: &dyn Reads,
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
fn malformed(parse: &dyn Reads, range: TextRange, errors: &mut Vec<LowerError>) -> Term {
    errors.push(located(parse, range, LowerErrorKind::MalformedToken));
    placeholder()
}

/// A located lowering error at `range` under the parse's source (base §4.3).
fn located(parse: &dyn Reads, range: TextRange, kind: LowerErrorKind) -> LowerError {
    LowerError {
        location: parse.locate(range),
        kind,
    }
}

/// The best-effort stand-in for a subterm the value cannot represent — an anonymous
/// variable, which keeps a partial term assemblable so the diagnostic beside it is the
/// signal, not a panic.
fn placeholder() -> Term {
    Term::Variable(Variable::Anonymous)
}

// ============================================================================
// The statement and program raise (docs/design/program.md §8): the AST → Program
// lowering. A `#program` directive is lifted into part structure (§4.1); each
// statement is lowered, tagged with its parsed origin and its documentation (§6),
// and routed through the ingest door (§6.3). A recovered statement the value cannot
// complete is diagnosed and skipped, and its neighbors still raise (§8).
// ============================================================================

/// Lower a parsed program to a [`Program`], under the parse's own dialect (§8). Total:
/// every parse yields a [`Raised`] — the program it assembled and the lowering
/// diagnostics beside it, never a refusal or a panic (§15). A `#program name(formals)`
/// directive is not a statement but a positional delimiter opening the part its
/// following statements join; statements before any `#program` join `base` (§4.1).
/// O(tree).
#[must_use]
pub fn raise(parse: &Parse<ast::Program>) -> Raised {
    let mut program = Program::default();
    let mut errors = Vec::new();
    let mut part = base_key();
    for statement in parse.tree().statements() {
        if let ast::Statement::ProgramPart(directive) = &statement {
            match part_key(directive) {
                Some(key) => part = key,
                None => errors.push(located(
                    parse,
                    directive.syntax().text_range(),
                    LowerErrorKind::IncompleteStatement,
                )),
            }
            continue;
        }
        if let Some(raised) = raise_one(&statement, parse, &mut errors) {
            let provenance = statement_provenance(&statement, parse);
            program.ingest_into(part.clone(), WithProvenance::new(raised, provenance));
        }
    }
    Raised {
        program,
        diagnostics: errors,
    }
}

/// Lower a single parsed statement fragment (§8) — the door the macro tier expands to,
/// lowering an ASP fragment through the one grammar and here, never a second parser
/// (spec §8). `None` when the fragment held no statement under recovery, or held a
/// `#program` delimiter, which is not a statement (§4.1). The statement is canonical
/// (§5.1), as a raised term is. O(tree).
#[must_use]
pub fn raise_statement(
    parse: &Parse<ast::StatementFragment>,
) -> (Option<Statement>, Vec<LowerError>) {
    let mut errors = Vec::new();
    let statement = parse
        .tree()
        .statement()
        .filter(|statement| !matches!(statement, ast::Statement::ProgramPart(_)))
        .and_then(|statement| raise_one(&statement, parse, &mut errors))
        .map(crate::program::canonicalize_statement);
    (statement, errors)
}

/// The result of a raise (§8): the lowered program and the lowering diagnostics beside
/// it. Owned plain data; a diagnostic is a value on a total raise, not a refusal (§15).
#[derive(Clone, Debug)]
pub struct Raised {
    program: Program,
    diagnostics: Vec<LowerError>,
}

impl Raised {
    /// The lowered program (§8).
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The lowering diagnostics, in source order; base's `canonical_order` sorts a
    /// batch for consumption (base §7.4).
    pub fn diagnostics(&self) -> &[LowerError] {
        &self.diagnostics
    }

    /// The owned program, dropping the diagnostics (§8).
    pub fn into_program(self) -> Program {
        self.program
    }
}

/// The part a `#program name(formals)` directive opens (§4.1): its name and the spelled
/// formals, the key by which parts coexist rather than merge. `None` when recovery left
/// the name absent — the directive cannot open a part without one.
fn part_key(directive: &ast::ProgramStatement) -> Option<PartKey> {
    let name = Name::new(directive.name()?.text()).ok()?;
    let formals = directive
        .parameters()
        .map(|parameters| {
            parameters
                .names()
                .filter_map(|ident| Name::new(ident.text()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(PartKey { name, formals })
}

/// The provenance of a raised statement (§6): its parsed origin, and a `Doc` annotation
/// from its leading doc comments (grammar §5.11), so documentation rides the rule it
/// documents.
fn statement_provenance(statement: &ast::Statement, parse: &dyn Reads) -> Provenance {
    let mut provenance = Provenance::from(Origin::Parsed(
        parse.locate(statement.syntax().text_range()),
    ));
    let lines = doc_lines(statement);
    if !lines.is_empty() {
        provenance = provenance.with_doc(lines.join("\n"));
    }
    provenance
}

/// The content of a statement's leading doc comments, one string per line, in order
/// (grammar §5.11); read through each node's `HasDocs`.
fn doc_lines(statement: &ast::Statement) -> Vec<String> {
    fn lines_of(node: &impl HasDocs) -> Vec<String> {
        node.doc_lines()
            .map(|line| line.content().to_owned())
            .collect()
    }
    match statement {
        ast::Statement::Rule(node) => lines_of(node),
        ast::Statement::WeakConstraint(node) => lines_of(node),
        ast::Statement::Optimize(node) => lines_of(node),
        ast::Statement::Show(node) => lines_of(node),
        ast::Statement::Project(node) => lines_of(node),
        ast::Statement::Defined(node) => lines_of(node),
        ast::Statement::Edge(node) => lines_of(node),
        ast::Statement::Heuristic(node) => lines_of(node),
        ast::Statement::External(node) => lines_of(node),
        ast::Statement::Const(node) => lines_of(node),
        ast::Statement::Script(node) => lines_of(node),
        ast::Statement::Include(node) => lines_of(node),
        ast::Statement::ProgramPart(node) => lines_of(node),
        ast::Statement::TheoryDefinition(node) => lines_of(node),
        ast::Statement::Query(node) => lines_of(node),
    }
}

/// Lower one statement (a `#program` delimiter is handled by [`raise`] before this
/// door) to a program statement (§8), or diagnose-and-skip a recovered one the value
/// cannot complete. The match is exhaustive with no wildcard, so a new statement family
/// is a compile error here, never a silent drop.
fn raise_one(
    statement: &ast::Statement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    match statement {
        ast::Statement::Rule(rule) => raise_rule(rule, parse, errors),
        ast::Statement::WeakConstraint(weak) => Some(raise_weak_constraint(weak, parse, errors)),
        ast::Statement::Optimize(optimize) => Some(raise_optimize(optimize, parse, errors)),
        ast::Statement::Show(show) => raise_show(show, parse, errors),
        ast::Statement::Project(project) => raise_project(project, parse, errors),
        ast::Statement::Defined(defined) => raise_defined(defined, parse, errors),
        ast::Statement::Edge(edge) => Some(raise_edge(edge, parse, errors)),
        ast::Statement::Heuristic(heuristic) => raise_heuristic(heuristic, parse, errors),
        ast::Statement::External(external) => raise_external(external, parse, errors),
        ast::Statement::Const(constant) => raise_const(constant, parse, errors),
        ast::Statement::Script(script) => raise_script(script, parse, errors),
        ast::Statement::Include(include) => raise_include(include, parse, errors),
        ast::Statement::TheoryDefinition(definition) => {
            raise_theory_definition(definition, parse, errors)
        }
        ast::Statement::Query(query) => raise_query(query, parse, errors),
        ast::Statement::ProgramPart(_) => None,
    }
}

// ---- rules, heads, bodies, and their interiors (§4.3–§4.7) ----

/// Raise a rule (§4.3): a head — a missing head node is a constraint (`⊥ ← body`, §4.4)
/// — and a body, each carrying its parsed origin (§6). A head the value cannot
/// represent skips the whole rule (its diagnostic already stands).
fn raise_rule(
    rule: &ast::Rule,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let head = match rule.head() {
        Some(node) => {
            let range = node.syntax().text_range();
            wrap(raise_head(node, parse, errors)?, range, parse)
        }
        None => wrap(Head::Falsum, rule.syntax().text_range(), parse),
    };
    let body_range = rule.body().as_ref().map_or_else(
        || rule.syntax().text_range(),
        |body| body.syntax().text_range(),
    );
    let body = wrap(raise_body(rule.body(), parse, errors), body_range, parse);
    Some(Statement::Rule(Rule::from_nodes(head, body)))
}

/// Raise a head (§4.4): the `#true`/`#false` fold to `Verum`/`Falsum` runs at the ingest
/// door (§5.1), so a boolean head-literal is raised as its literal here; a set form is a
/// [`Head::Choice`], a function aggregate a [`Head::Aggregate`] — the position the tree
/// records (§4.4).
fn raise_head(head: ast::Head, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Option<Head> {
    match head {
        ast::Head::Literal(literal) => Some(Head::Literal(raise_literal(&literal, parse, errors)?)),
        ast::Head::Disjunction(disjunction) => Some(Head::Disjunction(raise_disjunction(
            &disjunction,
            parse,
            errors,
        ))),
        ast::Head::Aggregate(ast::Aggregate::Set(set)) => {
            Some(Head::Choice(raise_choice(&set, parse, errors)))
        }
        ast::Head::Aggregate(ast::Aggregate::Function(function)) => Some(Head::Aggregate(
            raise_head_aggregate(&function, parse, errors)?,
        )),
        ast::Head::TheoryAtom(atom) => {
            Some(Head::TheoryAtom(raise_theory_atom(&atom, parse, errors)?))
        }
    }
}

/// Raise a literal (§4.6): its default negation, and its inner atom, comparison, or
/// boolean constant. The atom and comparison carry their parsed origin (§6).
fn raise_literal(
    literal: &ast::Literal,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Literal> {
    let negation = negation_to_default(literal.negation());
    let Some(inner) = literal.inner() else {
        return incomplete(literal.syntax(), parse, errors);
    };
    let inner = match inner {
        ast::LiteralInner::True(_) => LiteralInner::True,
        ast::LiteralInner::False(_) => LiteralInner::False,
        ast::LiteralInner::Atom(atom) => {
            let range = atom.syntax().text_range();
            LiteralInner::Atom(wrap(raise_atom(&atom, parse, errors)?, range, parse))
        }
        ast::LiteralInner::Comparison(comparison) => {
            let range = comparison.syntax().text_range();
            LiteralInner::Comparison(wrap(
                raise_comparison(&comparison, parse, errors)?,
                range,
                parse,
            ))
        }
    };
    Some(Literal { negation, inner })
}

/// Raise an atom (§4.6): a strong sign read from the tree — a leading `-` is
/// `Sign::Negative`, the positional `-p` ambiguity the tree already resolved (§8) — its
/// name, and its argument terms. `None` when the name is absent under recovery.
fn raise_atom(atom: &ast::Atom, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Option<Atom> {
    let Some(name) = atom.name().and_then(|ident| Name::new(ident.text()).ok()) else {
        return incomplete(atom.syntax(), parse, errors);
    };
    let sign = if atom.strong_negation_token().is_some() {
        Sign::Negative
    } else {
        Sign::Positive
    };
    let arguments = atom_arguments(atom.arguments(), parse, errors);
    Some(Atom {
        sign,
        name,
        arguments,
    })
}

/// The argument terms of an atom (§8): the first argument alternative's terms. A pooled
/// atom argument list (`p(a; b)`) is the grounder's to expand; this reads the written
/// tuple.
fn atom_arguments(
    arguments: Option<ast::Arguments>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Vec<Term> {
    let terms = arguments
        .and_then(|arguments| arguments.alternatives().next())
        .into_iter()
        .flat_map(|tuple| tuple.terms());
    raise_terms(terms, parse, errors)
}

/// Raise a comparison chain (§4.6): a first term and one or more relation/term steps —
/// `1 < X < 5` is one literal, not a conjunction (grammar §5.2).
fn raise_comparison(
    comparison: &ast::Comparison,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Comparison> {
    let Some(first) = comparison.first() else {
        return incomplete(comparison.syntax(), parse, errors);
    };
    let first = raise_term_node(&first, parse, errors);
    let mut steps = comparison.steps();
    let Some((relation, term)) = steps.next() else {
        return incomplete(comparison.syntax(), parse, errors);
    };
    let second = step_term(term, comparison.syntax(), parse, errors);
    let mut chain = Comparison::new(first, relation_of(relation), second);
    for (relation, term) in steps {
        let term = step_term(term, comparison.syntax(), parse, errors);
        chain = chain.chain(relation_of(relation), term);
    }
    Some(chain)
}

/// Raise a body (§4.5): its elements, each carrying its parsed origin (§6). A missing
/// body node is the empty body (`h :- .` and a fact both). Elements the value cannot
/// represent are skipped, their diagnostics already emitted.
fn raise_body(body: Option<ast::Body>, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Body {
    let Some(body) = body else {
        return Body::empty();
    };
    let mut elements = Vec::new();
    for element in body.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_body_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    Body::from_nodes(elements)
}

/// Raise a body element (§4.5): a literal, a conditional literal, or a negatable
/// aggregate or theory atom carrying its own default negation.
fn raise_body_element(
    element: ast::BodyElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<BodyElement> {
    match element {
        ast::BodyElement::Literal(literal) => Some(BodyElement::Literal(raise_literal(
            &literal, parse, errors,
        )?)),
        ast::BodyElement::ConditionalLiteral(conditional) => Some(BodyElement::Conditional(
            raise_conditional_literal(&conditional, parse, errors)?,
        )),
        ast::BodyElement::Aggregate(aggregate) => {
            let negation = negation_to_default(aggregate_negation(&aggregate));
            Some(BodyElement::Aggregate {
                negation,
                aggregate: raise_body_aggregate(aggregate, parse, errors)?,
            })
        }
        ast::BodyElement::TheoryAtom(atom) => {
            let negation = negation_to_default(atom.negation());
            Some(BodyElement::TheoryAtom {
                negation,
                atom: raise_theory_atom(&atom, parse, errors)?,
            })
        }
    }
}

/// Raise a conditional literal (§4.6): a literal under a condition (grammar §5.4).
fn raise_conditional_literal(
    conditional: &ast::ConditionalLiteral,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<ConditionalLiteral> {
    let Some(literal) = conditional.literal() else {
        return incomplete(conditional.syntax(), parse, errors);
    };
    let literal = raise_literal(&literal, parse, errors)?;
    let condition = raise_condition(conditional.condition(), parse, errors);
    Some(ConditionalLiteral { literal, condition })
}

/// Raise a condition (§4.6): the literals after a `:`, each carrying its parsed origin
/// (§6). A sequence, present and empty when the colon is (grammar §5.4).
fn raise_condition(
    condition: Option<ast::Condition>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Condition {
    let Some(condition) = condition else {
        return Condition::empty();
    };
    let mut literals = Vec::new();
    for literal in condition.literals() {
        let range = literal.syntax().text_range();
        if let Some(content) = raise_literal(&literal, parse, errors) {
            literals.push(wrap(content, range, parse));
        }
    }
    Condition::from_nodes(literals)
}

/// Raise a disjunctive head (§4.4): its conditioned literals, each carrying its parsed
/// origin (§6).
fn raise_disjunction(
    disjunction: &ast::Disjunction,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Disjunction {
    let mut elements = Vec::new();
    for element in disjunction.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_disjunction_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    Disjunction::from_nodes(elements)
}

fn raise_disjunction_element(
    element: ast::DisjunctionElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<DisjunctionElement> {
    match element {
        ast::DisjunctionElement::Literal(literal) => Some(DisjunctionElement::new(
            raise_literal(&literal, parse, errors)?,
            Condition::empty(),
        )),
        ast::DisjunctionElement::ConditionalLiteral(conditional) => {
            let ConditionalLiteral { literal, condition } =
                raise_conditional_literal(&conditional, parse, errors)?;
            Some(DisjunctionElement::new(literal, condition))
        }
    }
}

/// Raise a head set form to a choice (§4.4): its guards and conditioned elements, each
/// carrying its parsed origin (§6). A set form is a `Choice` in a head, a cardinality
/// aggregate in a body — by the position the tree records (§8).
fn raise_choice(
    set: &ast::SetAggregate,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Choice {
    let left = raise_guard(set.left_guard(), parse, errors);
    let right = raise_guard(set.right_guard(), parse, errors);
    let mut elements = Vec::new();
    for element in set.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_choice_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    Choice::from_nodes(left, elements, right)
}

fn raise_choice_element(
    element: ast::SetElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<ChoiceElement> {
    match element {
        ast::SetElement::Literal(literal) => Some(ChoiceElement::new(
            raise_literal(&literal, parse, errors)?,
            Condition::empty(),
        )),
        ast::SetElement::ConditionalLiteral(conditional) => {
            let ConditionalLiteral { literal, condition } =
                raise_conditional_literal(&conditional, parse, errors)?;
            Some(ChoiceElement::new(literal, condition))
        }
    }
}

/// Raise an aggregate guard (§4.7): a relation — absent means the grammar's default for
/// its side — and a bound term.
fn raise_guard(
    guard: Option<ast::Guard>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<WithProvenance<Guard>> {
    let guard = guard?;
    let range = guard.syntax().text_range();
    let relation = guard.relation().map(relation_of);
    let term = step_term(guard.term(), guard.syntax(), parse, errors);
    Some(wrap(Guard { relation, term }, range, parse))
}

/// Raise a body aggregate (§4.7): a function aggregate over testing elements, or a set
/// (cardinality) aggregate.
fn raise_body_aggregate(
    aggregate: ast::Aggregate,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Aggregate> {
    match aggregate {
        ast::Aggregate::Function(function) => Some(Aggregate::Function(raise_function_aggregate(
            &function, parse, errors,
        )?)),
        ast::Aggregate::Set(set) => Some(Aggregate::Set(raise_set_aggregate(&set, parse, errors))),
    }
}

fn raise_function_aggregate(
    function: &ast::FunctionAggregate,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<FunctionAggregate> {
    let Some(kind) = function.function() else {
        return incomplete(function.syntax(), parse, errors);
    };
    let left = raise_guard(function.left_guard(), parse, errors);
    let right = raise_guard(function.right_guard(), parse, errors);
    let mut elements = Vec::new();
    for element in function.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_body_aggregate_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    Some(FunctionAggregate::from_nodes(
        left,
        aggregate_function_of(kind),
        elements,
        right,
    ))
}

fn raise_body_aggregate_element(
    element: ast::AggregateElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<BodyAggregateElement> {
    match element {
        ast::AggregateElement::Body(body) => {
            let terms = raise_terms(body.terms(), parse, errors);
            let condition = raise_condition(body.condition(), parse, errors);
            Some(BodyAggregateElement::new(terms, condition))
        }
        // A head-shaped element in a body aggregate is a position the value cannot
        // represent (§4.7): diagnosed and skipped.
        ast::AggregateElement::Head(head) => incomplete(head.syntax(), parse, errors),
    }
}

fn raise_set_aggregate(
    set: &ast::SetAggregate,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> SetAggregate {
    let left = raise_guard(set.left_guard(), parse, errors);
    let right = raise_guard(set.right_guard(), parse, errors);
    let mut elements = Vec::new();
    for element in set.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_set_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    SetAggregate::from_nodes(left, elements, right)
}

fn raise_set_element(
    element: ast::SetElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<SetElement> {
    match element {
        ast::SetElement::Literal(literal) => {
            Some(SetElement::Literal(raise_literal(&literal, parse, errors)?))
        }
        ast::SetElement::ConditionalLiteral(conditional) => Some(SetElement::ConditionalLiteral(
            raise_conditional_literal(&conditional, parse, errors)?,
        )),
    }
}

fn raise_head_aggregate(
    function: &ast::FunctionAggregate,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<HeadAggregate> {
    let Some(kind) = function.function() else {
        return incomplete(function.syntax(), parse, errors);
    };
    let left = raise_guard(function.left_guard(), parse, errors);
    let right = raise_guard(function.right_guard(), parse, errors);
    let mut elements = Vec::new();
    for element in function.elements() {
        let range = element.syntax().text_range();
        if let Some(content) = raise_head_aggregate_element(element, parse, errors) {
            elements.push(wrap(content, range, parse));
        }
    }
    Some(HeadAggregate::from_nodes(
        left,
        aggregate_function_of(kind),
        elements,
        right,
    ))
}

fn raise_head_aggregate_element(
    element: ast::AggregateElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<HeadAggregateElement> {
    match element {
        ast::AggregateElement::Head(head) => {
            let terms = raise_terms(head.terms(), parse, errors);
            let Some(literal) = head.literal() else {
                return incomplete(head.syntax(), parse, errors);
            };
            let literal = raise_literal(&literal, parse, errors)?;
            let condition = raise_condition(head.condition(), parse, errors);
            Some(HeadAggregateElement::new(terms, literal, condition))
        }
        // A body-shaped element in a head aggregate cannot derive an atom (§4.7).
        ast::AggregateElement::Body(body) => incomplete(body.syntax(), parse, errors),
    }
}

// ---- optimization (§4.7) ----

fn raise_weak_constraint(
    weak: &ast::WeakConstraint,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Statement {
    let body = raise_body_node(weak.body(), weak.syntax().text_range(), parse, errors);
    let weight = raise_weight(weak.weight(), weak.priority(), weak.syntax(), parse, errors);
    let terms = raise_terms(weak.tuple(), parse, errors);
    Statement::WeakConstraint(WeakConstraint::from_nodes(body, weight, terms))
}

fn raise_optimize(
    optimize: &ast::OptimizeStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Statement {
    let direction = match optimize.keyword_token() {
        Some(token) if token.kind() == SyntaxKind::KW_MAXIMIZE => Direction::Maximize,
        _ => Direction::Minimize,
    };
    let mut elements = Vec::new();
    for element in optimize.elements() {
        let range = element.syntax().text_range();
        let content = raise_optimize_element(&element, parse, errors);
        elements.push(wrap(content, range, parse));
    }
    Statement::Optimize(Optimize::from_nodes(direction, elements))
}

fn raise_optimize_element(
    element: &ast::OptimizeElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> OptimizeElement {
    let weight = raise_weight(
        element.weight(),
        element.priority(),
        element.syntax(),
        parse,
        errors,
    );
    let terms = raise_terms(element.tuple(), parse, errors);
    let condition = raise_condition(element.condition(), parse, errors);
    OptimizeElement::new(weight, terms, condition)
}

/// Raise a `weight@priority` (§4.7): the weight term, at an optional priority. The weight
/// is mandatory (grammar §5.7); one the recovery left absent is a placeholder beside an
/// `IncompleteTerm`, exactly as every other required term is ([`step_term`], §8) — never
/// silently defaulted, which would repair a recovered value out of sight (§2, §5.2).
fn raise_weight(
    weight_term: Option<ast::Term>,
    priority: Option<ast::Term>,
    at: &SyntaxNode,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Weight {
    let base = weight(step_term(weight_term, at, parse, errors));
    match priority {
        Some(term) => base.at_priority(raise_term_node(&term, parse, errors)),
        None => base,
    }
}

// ---- the body-free directives (§4.8) ----

fn raise_show(
    show: &ast::ShowStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    if let Some(signature) = show.signature() {
        return Some(Statement::Show(Show::Signature(raise_signature(
            &signature, parse, errors,
        )?)));
    }
    if let Some(term) = show.term() {
        let term = raise_term_node(&term, parse, errors);
        return Some(Statement::Show(if show.colon_token().is_some() {
            Show::TermBody {
                term,
                body: raise_body_node(show.body(), show.syntax().text_range(), parse, errors),
            }
        } else {
            Show::Term(term)
        }));
    }
    // `#show.` — show nothing.
    Some(Statement::Show(Show::All))
}

/// Raise a signature (grammar §5.9): a strong sign, a name, and an arity.
fn raise_signature(
    signature: &ast::Signature,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Signature> {
    let Some(name) = signature
        .name()
        .and_then(|ident| Name::new(ident.text()).ok())
    else {
        return incomplete(signature.syntax(), parse, errors);
    };
    let Some(arity) = signature.arity().and_then(|number| number_u32(&number)) else {
        return incomplete(signature.syntax(), parse, errors);
    };
    let sign = if signature.strong_negation_token().is_some() {
        Sign::Negative
    } else {
        Sign::Positive
    };
    Some(Signature { sign, name, arity })
}

fn raise_project(
    project: &ast::ProjectStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    if let Some(signature) = project.signature() {
        return Some(Statement::Project(Project::Signature(raise_signature(
            &signature, parse, errors,
        )?)));
    }
    let Some(atom) = project.atom() else {
        return incomplete(project.syntax(), parse, errors);
    };
    let atom = raise_atom_node(&atom, parse, errors)?;
    let body = raise_body_node(project.body(), project.syntax().text_range(), parse, errors);
    Some(Statement::Project(Project::Atom { atom, body }))
}

fn raise_defined(
    defined: &ast::DefinedStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(signature) = defined.signature() else {
        return incomplete(defined.syntax(), parse, errors);
    };
    Some(Statement::Defined(Defined {
        signature: raise_signature(&signature, parse, errors)?,
    }))
}

fn raise_edge(
    edge: &ast::EdgeStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Statement {
    let mut pairs = Vec::new();
    for pair in edge.edges() {
        let from = pair
            .from()
            .map(|term| raise_term_node(&term, parse, errors));
        let to = pair.to().map(|term| raise_term_node(&term, parse, errors));
        if let (Some(from), Some(to)) = (from, to) {
            pairs.push((from, to));
        } else {
            errors.push(located(
                parse,
                pair.syntax().text_range(),
                LowerErrorKind::IncompleteStatement,
            ));
        }
    }
    let body = raise_body_node(edge.body(), edge.syntax().text_range(), parse, errors);
    Statement::Edge(Edge::from_nodes(pairs, body))
}

fn raise_heuristic(
    heuristic: &ast::HeuristicStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(atom) = heuristic.atom() else {
        return incomplete(heuristic.syntax(), parse, errors);
    };
    let atom = raise_atom_node(&atom, parse, errors)?;
    let body = raise_body_node(
        heuristic.body(),
        heuristic.syntax().text_range(),
        parse,
        errors,
    );
    let bias = step_term(heuristic.weight(), heuristic.syntax(), parse, errors);
    let priority = heuristic
        .priority()
        .map(|term| raise_term_node(&term, parse, errors));
    let modifier = step_term(heuristic.modifier(), heuristic.syntax(), parse, errors);
    Some(Statement::Heuristic(Heuristic::from_nodes(
        atom, body, bias, priority, modifier,
    )))
}

fn raise_external(
    external: &ast::ExternalStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(atom) = external.atom() else {
        return incomplete(external.syntax(), parse, errors);
    };
    let atom = raise_atom_node(&atom, parse, errors)?;
    let body = raise_body_node(
        external.body(),
        external.syntax().text_range(),
        parse,
        errors,
    );
    let value = external
        .value()
        .map(|term| raise_term_node(&term, parse, errors));
    Some(Statement::External(External::from_nodes(atom, body, value)))
}

fn raise_const(
    constant: &ast::ConstStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(name) = constant
        .name()
        .and_then(|ident| Name::new(ident.text()).ok())
    else {
        return incomplete(constant.syntax(), parse, errors);
    };
    let Some(value_node) = constant.value() else {
        return incomplete(constant.syntax(), parse, errors);
    };
    let value = raise_term_node(&value_node, parse, errors);
    // The value is carried unevaluated (§4.8); a value outside the constant-term subset
    // (grammar §5.9) is diagnosed at its span, never silently evaluated (§8).
    if !is_constant_term(&value) {
        errors.push(located(
            parse,
            value_node.syntax().text_range(),
            LowerErrorKind::NonConstantValue,
        ));
    }
    let policy = constant.policy().map(const_policy_of);
    Some(Statement::Const(Const {
        name,
        value,
        policy,
    }))
}

fn raise_script(
    script: &ast::ScriptStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(language) = script
        .language()
        .and_then(|ident| Name::new(ident.text()).ok())
    else {
        return incomplete(script.syntax(), parse, errors);
    };
    let body = script
        .body()
        .map(|body| body.value().to_owned())
        .unwrap_or_default();
    Some(Statement::Script(Script::new(language, body)))
}

fn raise_include(
    include: &ast::IncludeStatement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    if let Some(path) = include.path() {
        let Ok(text) = parse.read_string(&path) else {
            errors.push(located(
                parse,
                path.syntax().text_range(),
                LowerErrorKind::MalformedToken,
            ));
            return None;
        };
        return Some(Statement::Include(Include::new(IncludeTarget::Path(text))));
    }
    if let Some(library) = include
        .library()
        .and_then(|ident| Name::new(ident.text()).ok())
    {
        return Some(Statement::Include(Include::new(IncludeTarget::System(
            library,
        ))));
    }
    incomplete(include.syntax(), parse, errors)
}

fn raise_query(
    query: &ast::Query,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(atom) = query.atom() else {
        return incomplete(query.syntax(), parse, errors);
    };
    Some(Statement::Query(Query::from_nodes(raise_atom_node(
        &atom, parse, errors,
    )?)))
}

// ---- theory definitions and theory atoms (§4.9) ----

fn raise_theory_definition(
    definition: &ast::TheoryDefinition,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<Statement> {
    let Some(name) = definition
        .name()
        .and_then(|ident| Name::new(ident.text()).ok())
    else {
        return incomplete(definition.syntax(), parse, errors);
    };
    let mut terms = BTreeSet::new();
    let mut atoms = BTreeSet::new();
    for item in definition.items() {
        match item {
            ast::TheoryDefItem::Term(term) => {
                if let Some(definition) = raise_term_definition(&term) {
                    terms.insert(definition);
                }
            }
            ast::TheoryDefItem::Atom(atom) => {
                if let Some(definition) = raise_atom_definition(&atom) {
                    atoms.insert(definition);
                }
            }
        }
    }
    Some(Statement::TheoryDefinition(TheoryDefinition {
        name,
        terms,
        atoms,
    }))
}

fn raise_term_definition(definition: &ast::TermDefinition) -> Option<TheoryTermDefinition> {
    let name = Name::new(definition.name()?.text()).ok()?;
    let mut operators = BTreeSet::new();
    for operator in definition.op_definitions() {
        if let Some(operator) = raise_op_definition(&operator) {
            operators.insert(operator);
        }
    }
    Some(TheoryTermDefinition { name, operators })
}

fn raise_op_definition(operator: &ast::OpDefinition) -> Option<TheoryOperatorDefinition> {
    let operator_symbol = TheoryOperator::new(operator.operator_token()?.text());
    let priority = operator.priority().and_then(|number| number_u32(&number))?;
    let arity = match (
        operator.arity_token().map(|ident| ident.text().to_owned()),
        operator.associativity(),
    ) {
        (Some(word), _) if word == "unary" => TheoryOperatorArity::Unary,
        (Some(word), Some(Associativity::Left)) if word == "binary" => {
            TheoryOperatorArity::BinaryLeft
        }
        (Some(word), Some(Associativity::Right)) if word == "binary" => {
            TheoryOperatorArity::BinaryRight
        }
        _ => return None,
    };
    Some(TheoryOperatorDefinition {
        operator: operator_symbol,
        priority,
        arity,
    })
}

fn raise_atom_definition(definition: &ast::AtomDefinition) -> Option<TheoryAtomDefinition> {
    let name = Name::new(definition.name()?.text()).ok()?;
    let arity = definition.arity().and_then(|number| number_u32(&number))?;
    let term_definition = Name::new(definition.type_name()?.text()).ok()?;
    let occurrence = theory_occurrence(definition.occurrence()?.text())?;
    let guard = raise_atom_guard(definition);
    Some(TheoryAtomDefinition {
        name,
        arity,
        term_definition,
        guard,
        occurrence,
    })
}

fn raise_atom_guard(definition: &ast::AtomDefinition) -> Option<TheoryAtomGuardDefinition> {
    let operators: BTreeSet<TheoryOperator> = definition
        .guard_operators()
        .map(|token| TheoryOperator::new(token.text()))
        .collect();
    if operators.is_empty() {
        return None;
    }
    let term_definition = Name::new(definition.guard_type_name()?.text()).ok()?;
    Some(TheoryAtomGuardDefinition {
        operators,
        term_definition,
    })
}

fn theory_occurrence(word: &str) -> Option<TheoryOccurrence> {
    Some(match word {
        "head" => TheoryOccurrence::Head,
        "body" => TheoryOccurrence::Body,
        "any" => TheoryOccurrence::Any,
        "directive" => TheoryOccurrence::Directive,
        _ => return None,
    })
}

fn raise_theory_atom(
    atom: &ast::TheoryAtom,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<TheoryAtom> {
    let Some(name) = atom.name().and_then(|ident| Name::new(ident.text()).ok()) else {
        return incomplete(atom.syntax(), parse, errors);
    };
    let arguments = atom_arguments(atom.arguments(), parse, errors);
    let mut elements = Vec::new();
    if let Some(theory_elements) = atom.elements() {
        for element in theory_elements.elements() {
            let range = element.syntax().text_range();
            let content = raise_theory_element(&element, parse, errors);
            elements.push(wrap(content, range, parse));
        }
    }
    let guard = raise_theory_guard(atom.guard(), parse, errors);
    Some(TheoryAtom::from_nodes(name, arguments, elements, guard))
}

fn raise_theory_element(
    element: &ast::TheoryElement,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> TheoryElement {
    let mut terms = Vec::new();
    for opterm in element.opterms() {
        terms.push(raise_theory_opterm(&opterm, parse, errors));
    }
    let condition = element
        .colon_token()
        .is_some()
        .then(|| raise_condition(element.condition(), parse, errors));
    TheoryElement::new(terms, condition)
}

fn raise_theory_guard(
    guard: Option<ast::TheoryGuard>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<TheoryGuard> {
    let guard = guard?;
    let operator = TheoryOperator::new(guard.operator_token()?.text());
    let term = guard.opterm().map_or_else(theory_placeholder, |opterm| {
        raise_theory_opterm(&opterm, parse, errors)
    });
    Some(TheoryGuard { operator, term })
}

/// Raise one theory opterm to a theory term (grammar §5.8, §4.9), iteratively so a deep
/// theory term is built without call-stack recursion (§13). The operators are a flat run
/// before each operand, regrouped only by a `#theory` definition (above this tier); an
/// opterm of one operand under no operators is that operand.
fn raise_theory_opterm(
    opterm: &ast::TheoryOpTerm,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> TheoryTerm {
    let mut work = vec![TheoryStep::EnterOpTerm(opterm.clone())];
    let mut done: Vec<TheoryTerm> = Vec::new();
    while let Some(step) = work.pop() {
        // Net-one-push invariant (as in `assemble_tree`): a parent's `count` is the number
        // of child steps it pushed, and every child leaves exactly one result on `done`, so
        // `done.len() - count` is exact at each assembly below — never underflows.
        match step {
            TheoryStep::EnterOpTerm(opterm) => enter_theory_opterm(&opterm, &mut work, &mut done),
            TheoryStep::EnterTerm(term) => {
                enter_theory_term(term, &mut work, &mut done, parse, errors);
            }
            TheoryStep::Operation(operators, count) => {
                let operands = done.split_off(done.len() - count);
                done.push(TheoryTerm::Operation {
                    operators,
                    operands,
                });
            }
            TheoryStep::Function(name, count) => {
                let arguments = done.split_off(done.len() - count);
                done.push(TheoryTerm::Function { name, arguments });
            }
            TheoryStep::Tuple(count) => {
                let items = done.split_off(done.len() - count);
                done.push(TheoryTerm::Tuple(items));
            }
            TheoryStep::List(count) => {
                let items = done.split_off(done.len() - count);
                done.push(TheoryTerm::List(items));
            }
            TheoryStep::Set(count) => {
                let items = done.split_off(done.len() - count);
                done.push(TheoryTerm::Set(items));
            }
        }
    }
    done.pop().unwrap_or_else(theory_placeholder)
}

/// One step of the theory-term work list (§13): a node to enter, or a parent to
/// assemble from the `usize` children already atop the result stack.
enum TheoryStep {
    EnterOpTerm(ast::TheoryOpTerm),
    EnterTerm(ast::TheoryTerm),
    Operation(Vec<Vec<TheoryOperator>>, usize),
    Function(Name, usize),
    Tuple(usize),
    List(usize),
    Set(usize),
}

/// Enter an opterm (§5.8): its single unoperated operand is that operand; otherwise its
/// operator runs and operands become an `Operation` over the operands entered next.
fn enter_theory_opterm(
    opterm: &ast::TheoryOpTerm,
    work: &mut Vec<TheoryStep>,
    done: &mut Vec<TheoryTerm>,
) {
    let (operators, operands) = split_opterm(opterm);
    if operands.is_empty() {
        done.push(theory_placeholder());
    } else if operands.len() == 1 && operators.iter().all(Vec::is_empty) {
        work.push(TheoryStep::EnterTerm(
            operands.into_iter().next().expect("one operand"),
        ));
    } else {
        let count = operands.len();
        work.push(TheoryStep::Operation(operators, count));
        for operand in operands.into_iter().rev() {
            work.push(TheoryStep::EnterTerm(operand));
        }
    }
}

/// Enter a theory term (§5.8): a leaf is raised and pushed; a bracketed or applied form
/// schedules its assembly and enters its opterm children.
fn enter_theory_term(
    term: ast::TheoryTerm,
    work: &mut Vec<TheoryStep>,
    done: &mut Vec<TheoryTerm>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) {
    let schedule =
        |work: &mut Vec<TheoryStep>, step: TheoryStep, opterms: Vec<ast::TheoryOpTerm>| {
            work.push(step);
            for opterm in opterms.into_iter().rev() {
                work.push(TheoryStep::EnterOpTerm(opterm));
            }
        };
    match term {
        ast::TheoryTerm::Function(function) => {
            let Some(name) = function
                .name()
                .and_then(|ident| Name::new(ident.text()).ok())
            else {
                errors.push(located(
                    parse,
                    function.syntax().text_range(),
                    LowerErrorKind::IncompleteStatement,
                ));
                done.push(theory_placeholder());
                return;
            };
            let opterms: Vec<_> = function.opterms().collect();
            schedule(work, TheoryStep::Function(name, opterms.len()), opterms);
        }
        ast::TheoryTerm::Tuple(tuple) => {
            let opterms: Vec<_> = tuple.opterms().collect();
            schedule(work, TheoryStep::Tuple(opterms.len()), opterms);
        }
        ast::TheoryTerm::List(list) => {
            let opterms: Vec<_> = list.opterms().collect();
            schedule(work, TheoryStep::List(opterms.len()), opterms);
        }
        ast::TheoryTerm::Set(set) => {
            let opterms: Vec<_> = set.opterms().collect();
            schedule(work, TheoryStep::Set(opterms.len()), opterms);
        }
        ast::TheoryTerm::Constant(constant) => {
            done.push(TheoryTerm::Symbolic(raise_theory_constant(
                &constant, parse, errors,
            )));
        }
        ast::TheoryTerm::Variable(variable) => {
            done.push(raise_theory_variable(&variable, parse, errors));
        }
        ast::TheoryTerm::Splice(splice) => {
            errors.push(located(
                parse,
                splice.syntax().text_range(),
                LowerErrorKind::UnexpandedSplice,
            ));
            done.push(theory_placeholder());
        }
    }
}

/// A theory opterm's operator runs and its operand terms (grammar §5.8): `operators[i]`
/// is the run of operators before `operands[i]`.
fn split_opterm(opterm: &ast::TheoryOpTerm) -> (Vec<Vec<TheoryOperator>>, Vec<ast::TheoryTerm>) {
    let mut operators = Vec::new();
    let mut operands = Vec::new();
    let mut run = Vec::new();
    for item in opterm.items() {
        match item {
            ast::TheoryOpTermItem::Op(token) => run.push(TheoryOperator::new(token.text())),
            ast::TheoryOpTermItem::Term(term) => {
                operators.push(std::mem::take(&mut run));
                operands.push(term);
            }
        }
    }
    (operators, operands)
}

/// Raise a theory constant leaf to a ground symbol (grammar §5.8): a numeral, a string
/// under the parse's dialect, a constant identifier, or the order's bounds; a leaf the
/// value cannot represent stands in as `#inf` beside its diagnostic.
fn raise_theory_constant(
    constant: &ast::ConstantTerm,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Symbol {
    match constant.constant() {
        Some(Constant::Symbol(identifier)) => match Name::new(identifier.text()) {
            Ok(name) => Symbol::Function {
                name,
                arguments: Vec::new(),
                sign: Sign::Positive,
            },
            Err(_) => theory_symbol_error(parse, identifier.syntax().text_range(), errors),
        },
        Some(Constant::Number(number)) => {
            if let Some(value) = integer(&number) {
                Symbol::Number(value)
            } else {
                errors.push(located(
                    parse,
                    number.syntax().text_range(),
                    LowerErrorKind::NumberOutOfRange,
                ));
                Symbol::Infimum
            }
        }
        Some(Constant::String(string)) => match parse.read_string(&string) {
            Ok(text) => Symbol::String(text),
            Err(_) => theory_symbol_error(parse, string.syntax().text_range(), errors),
        },
        Some(Constant::Infimum(_)) => Symbol::Infimum,
        Some(Constant::Supremum(_)) => Symbol::Supremum,
        None => {
            errors.push(located(
                parse,
                constant.syntax().text_range(),
                LowerErrorKind::IncompleteTerm,
            ));
            Symbol::Infimum
        }
    }
}

fn theory_symbol_error(
    parse: &dyn Reads,
    range: TextRange,
    errors: &mut Vec<LowerError>,
) -> Symbol {
    errors.push(located(parse, range, LowerErrorKind::MalformedToken));
    Symbol::Infimum
}

fn raise_theory_variable(
    variable: &ast::VariableTerm,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> TheoryTerm {
    match variable.variable() {
        Some(inner) if inner.is_anonymous() => TheoryTerm::Variable(Variable::Anonymous),
        Some(inner) => {
            if let Ok(name) = VarName::new(inner.text()) {
                TheoryTerm::Variable(Variable::Named(name))
            } else {
                errors.push(located(
                    parse,
                    inner.syntax().text_range(),
                    LowerErrorKind::MalformedToken,
                ));
                theory_placeholder()
            }
        }
        None => {
            errors.push(located(
                parse,
                variable.syntax().text_range(),
                LowerErrorKind::IncompleteTerm,
            ));
            theory_placeholder()
        }
    }
}

/// The best-effort stand-in for a theory term the value cannot complete (§4.9).
fn theory_placeholder() -> TheoryTerm {
    TheoryTerm::Variable(Variable::Anonymous)
}

// ---- the small mappings and shared helpers ----

/// Wrap a raised content value with its parsed origin (§6): the node's span under the
/// parse's source.
fn wrap<T>(content: T, range: TextRange, parse: &dyn Reads) -> WithProvenance<T> {
    WithProvenance::new(
        content,
        Provenance::from(Origin::Parsed(parse.locate(range))),
    )
}

/// Raise an atom node and wrap it with its parsed span (§6.1) — the directive analogue of
/// a rule head/body carrier (`raise_rule`), so a directive's atom carries its own origin.
fn raise_atom_node(
    atom: &ast::Atom,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Option<WithProvenance<Atom>> {
    let range = atom.syntax().text_range();
    Some(wrap(raise_atom(atom, parse, errors)?, range, parse))
}

/// Raise a directive's optional body and wrap it with its parsed span, falling back to the
/// directive's own span when the body is absent (§6.1) — the directive analogue of a rule's
/// body carrier (`raise_rule`).
fn raise_body_node(
    body: Option<ast::Body>,
    fallback: TextRange,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> WithProvenance<Body> {
    let range = body
        .as_ref()
        .map_or(fallback, |body| body.syntax().text_range());
    wrap(raise_body(body, parse, errors), range, parse)
}

/// Diagnose a recovered node the value cannot complete, and skip it (§8): an
/// `IncompleteStatement` at the node's span, and `None`.
fn incomplete<T>(node: &SyntaxNode, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Option<T> {
    errors.push(located(
        parse,
        node.text_range(),
        LowerErrorKind::IncompleteStatement,
    ));
    None
}

/// Raise one AST term to a raw `term::Term` (§8): the statement door's terms canonicalize
/// with the rest of the statement at the ingest door (§5.1), so this returns the term as
/// assembled — iteratively (§13).
fn raise_term_node(term: &ast::Term, parse: &dyn Reads, errors: &mut Vec<LowerError>) -> Term {
    assemble_tree(term.clone(), parse, errors)
}

/// Raise a sequence of AST terms in order (§8).
fn raise_terms(
    terms: impl Iterator<Item = ast::Term>,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Vec<Term> {
    let mut raised = Vec::new();
    for term in terms {
        raised.push(raise_term_node(&term, parse, errors));
    }
    raised
}

/// A required step term, or a placeholder beside an incompleteness (§8): a comparison
/// step, a guard bound, a heuristic term, or an optimization weight the recovery left absent.
fn step_term(
    term: Option<ast::Term>,
    at: &SyntaxNode,
    parse: &dyn Reads,
    errors: &mut Vec<LowerError>,
) -> Term {
    let Some(term) = term else {
        errors.push(located(
            parse,
            at.text_range(),
            LowerErrorKind::IncompleteTerm,
        ));
        return placeholder();
    };
    raise_term_node(&term, parse, errors)
}

/// The value model's default negation for an AST negation prefix (§4.5).
fn negation_to_default(negation: Negation) -> DefaultNegation {
    match negation {
        Negation::None => DefaultNegation::None,
        Negation::Default => DefaultNegation::Not,
        Negation::DoubleDefault => DefaultNegation::NotNot,
    }
}

/// The default negation an AST body aggregate carries (grammar §5.6).
fn aggregate_negation(aggregate: &ast::Aggregate) -> Negation {
    match aggregate {
        ast::Aggregate::Function(function) => function.negation(),
        ast::Aggregate::Set(set) => set.negation(),
    }
}

/// The value model's relation for an AST relation (§4.6).
fn relation_of(relation: ast::Relation) -> Relation {
    match relation {
        ast::Relation::Lt => Relation::Lt,
        ast::Relation::Le => Relation::Le,
        ast::Relation::Gt => Relation::Gt,
        ast::Relation::Ge => Relation::Ge,
        ast::Relation::Eq => Relation::Eq,
        ast::Relation::Neq => Relation::Neq,
    }
}

/// The value model's aggregate function for an AST one (§4.7).
fn aggregate_function_of(function: ast::AggregateFunction) -> AggregateFunction {
    match function {
        ast::AggregateFunction::Count => AggregateFunction::Count,
        ast::AggregateFunction::Sum => AggregateFunction::Sum,
        ast::AggregateFunction::SumPlus => AggregateFunction::SumPlus,
        ast::AggregateFunction::Min => AggregateFunction::Min,
        ast::AggregateFunction::Max => AggregateFunction::Max,
    }
}

/// The value model's constant policy for an AST one (§4.8).
fn const_policy_of(policy: ast::ConstPolicy) -> ConstPolicy {
    match policy {
        ast::ConstPolicy::Default => ConstPolicy::Default,
        ast::ConstPolicy::Override => ConstPolicy::Override,
    }
}

/// The `u32` a numeral denotes under its radix, or `None` on overflow (grammar §5.9):
/// an arity or a theory-operator priority.
fn number_u32(number: &ast::NumberLit) -> Option<u32> {
    let radix = match number.radix() {
        Radix::Decimal => 10,
        Radix::Hexadecimal => 16,
        Radix::Octal => 8,
        Radix::Binary => 2,
    };
    u32::from_str_radix(number.digits(), radix).ok()
}

/// Whether a term is in the constant-term subset (grammar §5.9): no variable, pool, or
/// interval — a value a constant may take. Arithmetic and an `@`-call are admitted (§4.8),
/// carried unevaluated and evaluated only when a consumer asks (§3.5). Walked iteratively (§13).
fn is_constant_term(term: &Term) -> bool {
    !term.subterms().any(|subterm| {
        matches!(
            subterm,
            Term::Variable(_) | Term::Pool(_) | Term::Interval { .. }
        )
    })
}
