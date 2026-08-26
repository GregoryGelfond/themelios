//! Canonical rendering of a program to concrete syntax (docs/design/program.md §10). The
//! same program renders the same text every time, and the text round-trips: parsing and
//! raising it returns the same program up to provenance (§10, §16). The canonical form is
//! the simple one — binary operators and intervals fully parenthesized, so the tree's
//! grouping is carried in the text with no precedence to re-derive on the way back; a
//! nullary function bare; a one-element tuple keeping the comma that distinguishes it from
//! a grouped term; the set-shaped children in `Ord` order (§4). A single applied-form
//! printer serves a function term and an atom, so the two cannot drift. One value refuses:
//! a string whose value the chosen dialect cannot spell (grammar §4.4/§6.2/§9).
//!
//! The walk down the structural spine — program to statement to head and body to literal
//! and atom — is a bounded recursion (§13): those layers do not self-nest, so the depth is
//! the grammar's, not the input's. The self-recursive families — the term, the ground
//! symbol, and the theory term (§13) — are each rendered by an explicit work list of print
//! actions, so a value tens of thousands of levels deep renders without touching the call
//! stack. Total, and `O(output)`.

use std::fmt;

use themelios_syntax::dialect::Dialect;

use crate::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, ConstPolicy, DefaultNegation,
    Defined, Direction, Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard,
    HasGuards, Head, HeadAggregate, HeadAggregateElement, Heuristic, Include, IncludeTarget,
    Literal, LiteralInner, Optimize, OptimizeElement, Part, Program, Project, Query, Relation,
    Rule, Script, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryAtomDefinition,
    TheoryDefinition, TheoryElement, TheoryGuard, TheoryOccurrence, TheoryOperatorArity,
    TheoryOperatorDefinition, TheoryTerm, TheoryTermDefinition, WeakConstraint, Weight, base_key,
};
use crate::provenance::WithProvenance;
use crate::symbol::{Name, Sign, Signature, Symbol};
use crate::term::{BinaryOp, Term, UnaryOp, Variable};

/// Render a program to concrete syntax under a dialect (the dialect decides a string value's
/// spelling, grammar §4.4/§6.2). Canonical: the same program renders the same text, every
/// time (§10). Total and iterative in depth (§13) — one work-list walk down each
/// self-recursive family, a bounded recursion across the flat layers between them.
/// `O(output)`. The one refusal is [`Unspellable`].
pub fn render(program: &Program, dialect: Dialect) -> Result<String, Unspellable> {
    let mut out = String::new();
    // The base part carries the statements before any `#program` delimiter, so it renders
    // first, without a header (§4.1); every other part renders under its `#program` header,
    // in `PartKey` order, so its statements land back in it on the reparse.
    let base = base_key();
    render_part_statements(&mut out, program.base(), dialect)?;
    for part in program.parts() {
        if part.key() == &base {
            continue;
        }
        render_part_header(&mut out, part);
        render_part_statements(&mut out, part, dialect)?;
    }
    Ok(out)
}

/// The one refusal (§10): a string symbol whose value has no spelling under the chosen
/// dialect (grammar §9's owned gap — a macro splice can build a string value grammar §4.4
/// cannot spell, such as one bearing a tab). Carries the value and the dialect; nothing is
/// silently mangled.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unspellable {
    /// The string value with no spelling under the dialect.
    pub value: String,
    /// The dialect that cannot spell it.
    pub dialect: Dialect,
}

impl fmt::Display for Unspellable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the string value {:?} has no spelling under the {} dialect",
            self.value, self.dialect,
        )
    }
}

impl std::error::Error for Unspellable {}

// ---- the program: parts and their statements ----

/// A part's `#program name(formals).` header (grammar §5.9) — the delimiter that opens a
/// non-base part, so its statements reparse back into it.
fn render_part_header(out: &mut String, part: &Part) {
    out.push_str("#program ");
    out.push_str(part.key().name.as_str());
    let formals = &part.key().formals;
    if !formals.is_empty() {
        out.push('(');
        for (index, formal) in formals.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(formal.as_str());
        }
        out.push(')');
    }
    out.push_str(".\n");
}

/// A part's statements, in `Ord` order (§4), one per line.
fn render_part_statements(
    out: &mut String,
    part: &Part,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    for statement in part.statements() {
        render_statement(out, statement.get(), dialect)?;
        out.push('\n');
    }
    Ok(())
}

/// A statement, including its terminating dot and any bracket the four annotation-after-dot
/// families carry (grammar §5.11) — the dot is not always the last token. The match is
/// exhaustive with no wildcard, so a new statement family is a compile error here.
fn render_statement(
    out: &mut String,
    statement: &Statement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    match statement {
        Statement::Rule(rule) => render_rule(out, rule, dialect)?,
        Statement::WeakConstraint(weak) => render_weak_constraint(out, weak, dialect)?,
        Statement::Optimize(optimize) => render_optimize(out, optimize, dialect)?,
        Statement::Show(show) => render_show(out, show, dialect)?,
        Statement::Project(project) => render_project(out, project, dialect)?,
        Statement::Defined(defined) => render_defined(out, defined),
        Statement::Edge(edge) => render_edge(out, edge, dialect)?,
        Statement::Heuristic(heuristic) => render_heuristic(out, heuristic, dialect)?,
        Statement::External(external) => render_external(out, external, dialect)?,
        Statement::Const(constant) => render_const(out, constant, dialect)?,
        Statement::Include(include) => render_include(out, include, dialect)?,
        Statement::Script(script) => render_script(out, script),
        Statement::TheoryDefinition(definition) => render_theory_definition(out, definition),
        Statement::Query(query) => render_query(out, query, dialect)?,
    }
    Ok(())
}

// ---- rules, heads, and bodies (grammar §5.5–§5.7) ----

/// A rule (grammar §5.7). A falsum head renders as the constraint `:- body.`, the idiomatic
/// shape a `#false` head folds to (§5.1); every other head renders `head.` for an empty body
/// or `head :- body.` for a non-empty one.
fn render_rule(out: &mut String, rule: &Rule, dialect: Dialect) -> Result<(), Unspellable> {
    let head = rule.head().get();
    let body = rule.body().get();
    if matches!(head, Head::Falsum) {
        if body.is_empty() {
            out.push_str(":-.");
        } else {
            out.push_str(":- ");
            render_body(out, body, dialect)?;
            out.push('.');
        }
    } else {
        render_head(out, head, dialect)?;
        if !body.is_empty() {
            out.push_str(" :- ");
            render_body(out, body, dialect)?;
        }
        out.push('.');
    }
    Ok(())
}

/// A rule head (grammar §5.5). `Falsum` renders `#false` and `Verum` `#true` — the boolean
/// head-literals (§4.4); a falsum head in a rule renders as `:- body.` at [`render_rule`],
/// so this arm is the value's own spelling where a head stands alone.
fn render_head(out: &mut String, head: &Head, dialect: Dialect) -> Result<(), Unspellable> {
    match head {
        Head::Literal(literal) => render_literal(out, literal, dialect)?,
        Head::Disjunction(disjunction) => render_disjunction(out, disjunction, dialect)?,
        Head::Choice(choice) => render_choice(out, choice, dialect)?,
        Head::Aggregate(aggregate) => render_head_aggregate(out, aggregate, dialect)?,
        Head::TheoryAtom(atom) => render_theory_atom(out, atom, dialect)?,
        Head::Falsum => out.push_str("#false"),
        Head::Verum => out.push_str("#true"),
    }
    Ok(())
}

/// A disjunctive head (grammar §5.5): elements in `Ord` order joined by ` | `. A one-element
/// disjunction is the singleton conditioned head `p(X) : q(X)`, whose colon is forced — it is
/// what distinguishes it from a lone-literal head (§4.4).
fn render_disjunction(
    out: &mut String,
    disjunction: &Disjunction,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    let elements: Vec<&DisjunctionElement> =
        disjunction.elements().map(WithProvenance::get).collect();
    if let [single] = elements.as_slice() {
        render_literal(out, single.literal(), dialect)?;
        out.push_str(" :");
        if !single.condition().is_empty() {
            out.push(' ');
            render_condition(out, single.condition(), dialect)?;
        }
        return Ok(());
    }
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            out.push_str(" | ");
        }
        render_literal(out, element.literal(), dialect)?;
        if !element.condition().is_empty() {
            out.push_str(" : ");
            render_condition(out, element.condition(), dialect)?;
        }
    }
    Ok(())
}

/// A choice head (grammar §5.3): guards over a `Ord`-ordered set of conditioned literals,
/// `1 { a; b } 2` — the set form with guards, in head position (§4.4).
fn render_choice(out: &mut String, choice: &Choice, dialect: Dialect) -> Result<(), Unspellable> {
    render_left_guard(out, choice.left_guard(), dialect)?;
    out.push('{');
    let elements: Vec<&ChoiceElement> = choice.elements().map(WithProvenance::get).collect();
    render_set_body(out, &elements, dialect, |out, element, dialect| {
        render_literal(out, element.literal(), dialect)?;
        if !element.condition().is_empty() {
            out.push_str(" : ");
            render_condition(out, element.condition(), dialect)?;
        }
        Ok(())
    })?;
    out.push('}');
    render_right_guard(out, choice.right_guard(), dialect)?;
    Ok(())
}

/// A rule body (grammar §5.6): its elements in `Ord` order (§4). The separator is `,`, save
/// after a conditional literal — whose condition absorbs a following comma (§5.4) — where it
/// is `;`, the one separator a condition does not swallow.
fn render_body(out: &mut String, body: &Body, dialect: Dialect) -> Result<(), Unspellable> {
    let mut previous_was_conditional = false;
    for (index, element) in body.elements().enumerate() {
        let element = element.get();
        if index > 0 {
            out.push_str(if previous_was_conditional { "; " } else { ", " });
        }
        render_body_element(out, element, dialect)?;
        previous_was_conditional = matches!(element, BodyElement::Conditional(_));
    }
    Ok(())
}

/// A body element (grammar §5.6): a literal, a conditional literal, or a negatable aggregate
/// or theory atom.
fn render_body_element(
    out: &mut String,
    element: &BodyElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    match element {
        BodyElement::Literal(literal) => render_literal(out, literal, dialect)?,
        BodyElement::Conditional(conditional) => {
            render_conditional_literal(out, conditional, dialect)?;
        }
        BodyElement::Aggregate {
            negation,
            aggregate,
        } => {
            render_default_negation(out, *negation);
            render_aggregate(out, aggregate, dialect)?;
        }
        BodyElement::TheoryAtom { negation, atom } => {
            render_default_negation(out, *negation);
            render_theory_atom(out, atom, dialect)?;
        }
    }
    Ok(())
}

/// A conditional literal (grammar §5.4): a literal, then always its colon, then the
/// condition — the colon is what makes it conditional, present even when the condition is
/// empty (`p :`).
fn render_conditional_literal(
    out: &mut String,
    conditional: &ConditionalLiteral,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_literal(out, &conditional.literal, dialect)?;
    out.push_str(" :");
    if !conditional.condition.is_empty() {
        out.push(' ');
        render_condition(out, &conditional.condition, dialect)?;
    }
    Ok(())
}

/// A condition (grammar §5.4): the literals after a colon, a comma-separated sequence in
/// written order (§4.6).
fn render_condition(
    out: &mut String,
    condition: &Condition,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    for (index, literal) in condition.literals().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        render_literal(out, literal.get(), dialect)?;
    }
    Ok(())
}

// ---- literals, atoms, and comparisons (grammar §5.2) ----

/// A literal (grammar §5.2): a default-negation prefix over an atom, a comparison, or a
/// boolean constant (§4.6).
fn render_literal(
    out: &mut String,
    literal: &Literal,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_default_negation(out, literal.negation);
    match &literal.inner {
        LiteralInner::Atom(atom) => render_atom(out, atom.get(), dialect)?,
        LiteralInner::Comparison(comparison) => render_comparison(out, comparison.get(), dialect)?,
        LiteralInner::True => out.push_str("#true"),
        LiteralInner::False => out.push_str("#false"),
    }
    Ok(())
}

/// The default-negation prefix (grammar §5.2): `not ` or `not not ` (§4.6).
fn render_default_negation(out: &mut String, negation: DefaultNegation) {
    match negation {
        DefaultNegation::None => {}
        DefaultNegation::Not => out.push_str("not "),
        DefaultNegation::NotNot => out.push_str("not not "),
    }
}

/// An atom (grammar §5.2): a strong sign, a name, and arguments — through the one applied-form
/// printer a function term also uses (§10), so the two cannot drift.
fn render_atom(out: &mut String, atom: &Atom, dialect: Dialect) -> Result<(), Unspellable> {
    let mut work = Vec::new();
    push_applied(&mut work, out, Some(atom.sign), &atom.name, &atom.arguments);
    drain_terms(out, work, dialect)
}

/// A comparison chain (grammar §5.2): a first term and one or more relation/term steps,
/// `1 < X < 5`, in written direction (§5.1 keeps it).
fn render_comparison(
    out: &mut String,
    comparison: &Comparison,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_term(out, comparison.first(), dialect)?;
    for (relation, term) in comparison.steps() {
        out.push(' ');
        out.push_str(relation_str(relation));
        out.push(' ');
        render_term(out, term, dialect)?;
    }
    Ok(())
}

/// A comparison relation's spelling (grammar §5.2). `=` and `!=` are the canonical spellings
/// of the equality and disequality tokens (grammar §4.6).
fn relation_str(relation: Relation) -> &'static str {
    match relation {
        Relation::Lt => "<",
        Relation::Le => "<=",
        Relation::Gt => ">",
        Relation::Ge => ">=",
        Relation::Eq => "=",
        Relation::Neq => "!=",
    }
}

// ---- aggregates and optimization (grammar §5.3, §5.7) ----

/// A body aggregate (grammar §5.3): a function aggregate or a set (cardinality) aggregate.
fn render_aggregate(
    out: &mut String,
    aggregate: &Aggregate,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    match aggregate {
        Aggregate::Function(function) => render_function_aggregate(out, function, dialect),
        Aggregate::Set(set) => render_set_aggregate(out, set, dialect),
    }
}

/// A body function aggregate (grammar §5.3): guards over `#count`/`#sum`/… and body elements
/// that test.
fn render_function_aggregate(
    out: &mut String,
    aggregate: &FunctionAggregate,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_left_guard(out, aggregate.left_guard(), dialect)?;
    out.push_str(aggregate_function_str(aggregate.function()));
    out.push(' ');
    out.push('{');
    let elements: Vec<&BodyAggregateElement> =
        aggregate.elements().map(WithProvenance::get).collect();
    render_set_body(out, &elements, dialect, render_body_aggregate_element)?;
    out.push('}');
    render_right_guard(out, aggregate.right_guard(), dialect)?;
    Ok(())
}

/// A head function aggregate (grammar §5.3): the same guards and function over head elements
/// that derive (§4.4).
fn render_head_aggregate(
    out: &mut String,
    aggregate: &HeadAggregate,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_left_guard(out, aggregate.left_guard(), dialect)?;
    out.push_str(aggregate_function_str(aggregate.function()));
    out.push(' ');
    out.push('{');
    let elements: Vec<&HeadAggregateElement> =
        aggregate.elements().map(WithProvenance::get).collect();
    render_set_body(out, &elements, dialect, render_head_aggregate_element)?;
    out.push('}');
    render_right_guard(out, aggregate.right_guard(), dialect)?;
    Ok(())
}

/// A set (cardinality) aggregate (grammar §5.3): guards over set elements, `{ … }` in a body
/// (§4.7).
fn render_set_aggregate(
    out: &mut String,
    aggregate: &SetAggregate,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_left_guard(out, aggregate.left_guard(), dialect)?;
    out.push('{');
    let elements: Vec<&SetElement> = aggregate.elements().map(WithProvenance::get).collect();
    render_set_body(out, &elements, dialect, render_set_element)?;
    out.push('}');
    render_right_guard(out, aggregate.right_guard(), dialect)?;
    Ok(())
}

/// An aggregate function's keyword (grammar §5.3). `#sum+` is `SumPlus`.
fn aggregate_function_str(function: AggregateFunction) -> &'static str {
    match function {
        AggregateFunction::Count => "#count",
        AggregateFunction::Sum => "#sum",
        AggregateFunction::SumPlus => "#sum+",
        AggregateFunction::Min => "#min",
        AggregateFunction::Max => "#max",
    }
}

/// A body aggregate element (grammar §5.3): a term tuple under an optional condition. An
/// element with no terms and no condition is the bare-colon empty element, kept distinct from
/// the absent element (§10's named exception owns the pair an empty aggregate makes of it).
fn render_body_aggregate_element(
    out: &mut String,
    element: &BodyAggregateElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    let terms: Vec<&Term> = element.terms().collect();
    render_term_refs(out, &terms, dialect)?;
    if !element.condition().is_empty() {
        out.push_str(if terms.is_empty() { ": " } else { " : " });
        render_condition(out, element.condition(), dialect)?;
    } else if terms.is_empty() {
        out.push(':');
    }
    Ok(())
}

/// A head aggregate element (grammar §5.3): a term tuple, the literal it derives, and an
/// optional condition — the colon before the derived literal is required (§4.4).
fn render_head_aggregate_element(
    out: &mut String,
    element: &HeadAggregateElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    let terms: Vec<&Term> = element.terms().collect();
    render_term_refs(out, &terms, dialect)?;
    out.push_str(if terms.is_empty() { ": " } else { " : " });
    render_literal(out, element.literal(), dialect)?;
    if !element.condition().is_empty() {
        out.push_str(" : ");
        render_condition(out, element.condition(), dialect)?;
    }
    Ok(())
}

/// A set aggregate element (grammar §5.3): a literal or a conditional literal (§4.7).
fn render_set_element(
    out: &mut String,
    element: &SetElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    match element {
        SetElement::Literal(literal) => render_literal(out, literal, dialect),
        SetElement::ConditionalLiteral(conditional) => {
            render_conditional_literal(out, conditional, dialect)
        }
    }
}

/// A left guard (grammar §5.3): the bound term, then its relation where the author wrote one —
/// its absence is the grammar's `<=` on this side (§4.7). Rendered before the aggregate body,
/// with a trailing space where present.
fn render_left_guard(
    out: &mut String,
    guard: Option<&Guard>,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    if let Some(guard) = guard {
        render_term(out, &guard.term, dialect)?;
        if let Some(relation) = guard.relation {
            out.push(' ');
            out.push_str(relation_str(relation));
        }
        out.push(' ');
    }
    Ok(())
}

/// A right guard (grammar §5.3): the relation where the author wrote one, then the bound term
/// — the mirror of [`render_left_guard`], with a leading space where present.
fn render_right_guard(
    out: &mut String,
    guard: Option<&Guard>,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    if let Some(guard) = guard {
        out.push(' ');
        if let Some(relation) = guard.relation {
            out.push_str(relation_str(relation));
            out.push(' ');
        }
        render_term(out, &guard.term, dialect)?;
    }
    Ok(())
}

/// A weak constraint (grammar §5.7): a body, then the bracket after the dot carrying the
/// weight at its priority and the term tuple.
fn render_weak_constraint(
    out: &mut String,
    weak: &WeakConstraint,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push_str(":~");
    if !weak.body().is_empty() {
        out.push(' ');
        render_body(out, weak.body(), dialect)?;
    }
    out.push_str(". [");
    render_weight(out, weak.weight(), dialect)?;
    let terms: Vec<&Term> = weak.terms().collect();
    if !terms.is_empty() {
        out.push_str(", ");
        render_term_refs(out, &terms, dialect)?;
    }
    out.push(']');
    Ok(())
}

/// An optimization statement (grammar §5.7): `#minimize`/`#maximize` over a `Ord`-ordered set
/// of elements.
fn render_optimize(
    out: &mut String,
    optimize: &Optimize,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push_str(match optimize.direction {
        Direction::Minimize => "#minimize",
        Direction::Maximize => "#maximize",
    });
    out.push(' ');
    out.push('{');
    let elements: Vec<&OptimizeElement> = optimize.elements().map(WithProvenance::get).collect();
    render_set_body(out, &elements, dialect, render_optimize_element)?;
    out.push_str("}.");
    Ok(())
}

/// An optimize element (grammar §5.7): a weight at its priority, a term tuple, and an optional
/// condition.
fn render_optimize_element(
    out: &mut String,
    element: &OptimizeElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    render_weight(out, element.weight(), dialect)?;
    let terms: Vec<&Term> = element.terms().collect();
    if !terms.is_empty() {
        out.push_str(", ");
        render_term_refs(out, &terms, dialect)?;
    }
    if !element.condition().is_empty() {
        out.push_str(" : ");
        render_condition(out, element.condition(), dialect)?;
    }
    Ok(())
}

/// A weight at a priority (grammar §5.7): the weight term, then `@priority` where the level is
/// above the default (§4.7).
fn render_weight(out: &mut String, weight: &Weight, dialect: Dialect) -> Result<(), Unspellable> {
    render_term(out, weight.term(), dialect)?;
    if let Some(priority) = weight.priority() {
        out.push('@');
        render_term(out, priority, dialect)?;
    }
    Ok(())
}

// ---- directives (grammar §5.9) ----

/// A `#show` directive (grammar §5.9): the four forms (§4.8).
fn render_show(out: &mut String, show: &Show, dialect: Dialect) -> Result<(), Unspellable> {
    match show {
        Show::All => out.push_str("#show."),
        Show::Signature(signature) => {
            out.push_str("#show ");
            render_signature(out, signature);
            out.push('.');
        }
        Show::Term(term) => {
            out.push_str("#show ");
            render_term(out, term, dialect)?;
            out.push('.');
        }
        Show::TermBody { term, body } => {
            out.push_str("#show ");
            render_term(out, term, dialect)?;
            out.push_str(" : ");
            render_body(out, body, dialect)?;
            out.push('.');
        }
    }
    Ok(())
}

/// A `#project` directive (grammar §5.9): a signature, or an atom under an optional body
/// (§4.8).
fn render_project(
    out: &mut String,
    project: &Project,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    match project {
        Project::Signature(signature) => {
            out.push_str("#project ");
            render_signature(out, signature);
            out.push('.');
        }
        Project::Atom { atom, body } => {
            out.push_str("#project ");
            render_atom(out, atom, dialect)?;
            if !body.is_empty() {
                out.push_str(" : ");
                render_body(out, body, dialect)?;
            }
            out.push('.');
        }
    }
    Ok(())
}

/// A `#defined` directive (grammar §5.9): a signature.
fn render_defined(out: &mut String, defined: &Defined) {
    out.push_str("#defined ");
    render_signature(out, &defined.signature);
    out.push('.');
}

/// An `#edge` directive (grammar §5.9): node pairs under an optional body (§4.8).
fn render_edge(out: &mut String, edge: &Edge, dialect: Dialect) -> Result<(), Unspellable> {
    out.push_str("#edge (");
    for (index, (from, to)) in edge.pairs().enumerate() {
        if index > 0 {
            out.push_str("; ");
        }
        render_term(out, from, dialect)?;
        out.push_str(", ");
        render_term(out, to, dialect)?;
    }
    out.push(')');
    if !edge.body().is_empty() {
        out.push_str(" : ");
        render_body(out, edge.body(), dialect)?;
    }
    out.push('.');
    Ok(())
}

/// A `#heuristic` directive (grammar §5.9): an atom under an optional body, then its mandatory
/// bracket of a bias, an optional priority, and a modifier (§4.8).
fn render_heuristic(
    out: &mut String,
    heuristic: &Heuristic,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push_str("#heuristic ");
    render_atom(out, heuristic.atom(), dialect)?;
    if !heuristic.body().is_empty() {
        out.push_str(" : ");
        render_body(out, heuristic.body(), dialect)?;
    }
    out.push_str(". [");
    render_term(out, heuristic.bias(), dialect)?;
    if let Some(priority) = heuristic.priority() {
        out.push('@');
        render_term(out, priority, dialect)?;
    }
    out.push_str(", ");
    render_term(out, heuristic.modifier(), dialect)?;
    out.push(']');
    Ok(())
}

/// An `#external` directive (grammar §5.9): an atom under an optional body, then its optional
/// value bracket (§4.8).
fn render_external(
    out: &mut String,
    external: &External,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push_str("#external ");
    render_atom(out, external.atom(), dialect)?;
    if !external.body().is_empty() {
        out.push_str(" : ");
        render_body(out, external.body(), dialect)?;
    }
    out.push('.');
    if let Some(value) = external.value() {
        out.push_str(" [");
        render_term(out, value, dialect)?;
        out.push(']');
    }
    Ok(())
}

/// A `#const` directive (grammar §5.9): a name, a value in the constant-term subset, and an
/// optional policy bracket (§4.8).
fn render_const(out: &mut String, constant: &Const, dialect: Dialect) -> Result<(), Unspellable> {
    out.push_str("#const ");
    out.push_str(constant.name.as_str());
    out.push_str(" = ");
    render_term(out, &constant.value, dialect)?;
    out.push('.');
    if let Some(policy) = constant.policy {
        out.push_str(match policy {
            ConstPolicy::Default => " [default]",
            ConstPolicy::Override => " [override]",
        });
    }
    Ok(())
}

/// An `#include` directive (grammar §5.9): a quoted path or an angle-bracketed system name
/// (§4.8). The path is a string, spelled under the dialect (grammar §4.4/§6.2).
fn render_include(
    out: &mut String,
    include: &Include,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push_str("#include ");
    match include.target() {
        IncludeTarget::Path(path) => out.push_str(&spell_string(path, dialect)?),
        IncludeTarget::System(name) => {
            out.push('<');
            out.push_str(name.as_str());
            out.push('>');
        }
    }
    out.push('.');
    Ok(())
}

/// A `#script` directive (grammar §5.9): a language and its verbatim body between the marker
/// and `#end` (§4.8) — carried opaque, rendered as read. The body already carries the
/// whitespace the region captured, so it is written between `)` and `#end` untouched, no
/// separator added (grammar §4.8).
fn render_script(out: &mut String, script: &Script) {
    out.push_str("#script(");
    out.push_str(script.language().as_str());
    out.push(')');
    out.push_str(script.body());
    out.push_str("#end.");
}

/// An ASP-Core-2 query (grammar §6.1): the queried atom and the query mark, no dot — the
/// query stands last (§6.1), which its `Ord` placement gives it.
fn render_query(out: &mut String, query: &Query, dialect: Dialect) -> Result<(), Unspellable> {
    render_atom(out, query.atom(), dialect)?;
    out.push('?');
    Ok(())
}

/// A predicate signature (grammar §5.9): an optional strong sign, a name, and an arity,
/// `p/2` or `-q/1`.
fn render_signature(out: &mut String, signature: &Signature) {
    if matches!(signature.sign, Sign::Negative) {
        out.push('-');
    }
    out.push_str(signature.name.as_str());
    out.push('/');
    push_u32(out, signature.arity);
}

// ---- theory atoms and definitions (grammar §5.8, §5.9) ----

/// A theory atom (grammar §5.8): a name, optional ordinary-term arguments, optional elements,
/// and an optional guard (§4.9). The braces stand when elements or a guard do — the guard
/// follows them.
fn render_theory_atom(
    out: &mut String,
    atom: &TheoryAtom,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push('&');
    out.push_str(atom.name().as_str());
    let arguments: Vec<&Term> = atom.arguments().collect();
    if !arguments.is_empty() {
        out.push('(');
        render_term_refs(out, &arguments, dialect)?;
        out.push(')');
    }
    let elements: Vec<&TheoryElement> = atom.elements().map(WithProvenance::get).collect();
    if !elements.is_empty() || atom.guard().is_some() {
        out.push_str(" {");
        render_set_body(out, &elements, dialect, render_theory_element)?;
        out.push('}');
    }
    if let Some(guard) = atom.guard() {
        render_theory_guard(out, guard, dialect)?;
    }
    Ok(())
}

/// A theory element (grammar §5.8): the theory terms of the element under an optional
/// condition of ordinary literals (§4.9).
fn render_theory_element(
    out: &mut String,
    element: &TheoryElement,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    let terms: Vec<&TheoryTerm> = element.terms().collect();
    for (index, term) in terms.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        render_theory_term(out, term, dialect)?;
    }
    if let Some(condition) = element.condition() {
        out.push_str(if terms.is_empty() { ": " } else { " : " });
        render_condition(out, condition, dialect)?;
    }
    Ok(())
}

/// A theory atom's guard (grammar §5.8): an operator and a theory term, ` op term` after the
/// braces.
fn render_theory_guard(
    out: &mut String,
    guard: &TheoryGuard,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    out.push(' ');
    out.push_str(guard.operator.as_str());
    out.push(' ');
    render_theory_term(out, &guard.term, dialect)
}

/// A `#theory` definition (grammar §5.9): a name and its term- and atom-definitions, each set
/// in `Ord` order, semicolon-separated (§4.9).
fn render_theory_definition(out: &mut String, definition: &TheoryDefinition) {
    out.push_str("#theory ");
    out.push_str(definition.name.as_str());
    out.push_str(" {");
    let mut first = true;
    for term in &definition.terms {
        if !first {
            out.push(';');
        }
        first = false;
        out.push(' ');
        render_theory_term_definition(out, term);
    }
    for atom in &definition.atoms {
        if !first {
            out.push(';');
        }
        first = false;
        out.push(' ');
        render_theory_atom_definition(out, atom);
    }
    if !first {
        out.push(' ');
    }
    out.push_str("}.");
}

/// A term-definition (grammar §5.9): a name and its operator definitions.
fn render_theory_term_definition(out: &mut String, term: &TheoryTermDefinition) {
    out.push_str(term.name.as_str());
    out.push_str(" {");
    for (index, operator) in term.operators.iter().enumerate() {
        if index > 0 {
            out.push(';');
        }
        out.push(' ');
        render_theory_operator_definition(out, operator);
    }
    if !term.operators.is_empty() {
        out.push(' ');
    }
    out.push('}');
}

/// An operator definition (grammar §5.9): the operator, its priority, and its arity and
/// associativity.
fn render_theory_operator_definition(out: &mut String, operator: &TheoryOperatorDefinition) {
    out.push_str(operator.operator.as_str());
    out.push_str(" : ");
    push_u32(out, operator.priority);
    out.push_str(", ");
    out.push_str(match operator.arity {
        TheoryOperatorArity::Unary => "unary",
        TheoryOperatorArity::BinaryLeft => "binary, left",
        TheoryOperatorArity::BinaryRight => "binary, right",
    });
}

/// An atom-definition (grammar §5.9): the atom signature, its element term-definition, an
/// optional guard, and where it may occur.
fn render_theory_atom_definition(out: &mut String, atom: &TheoryAtomDefinition) {
    out.push('&');
    out.push_str(atom.name.as_str());
    out.push('/');
    push_u32(out, atom.arity);
    out.push_str(" : ");
    out.push_str(atom.term_definition.as_str());
    out.push_str(", ");
    if let Some(guard) = &atom.guard {
        out.push('{');
        for (index, operator) in guard.operators.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(operator.as_str());
        }
        out.push_str("}, ");
        out.push_str(guard.term_definition.as_str());
        out.push_str(", ");
    }
    out.push_str(theory_occurrence_str(atom.occurrence));
}

/// A theory atom's occurrence (grammar §5.9).
fn theory_occurrence_str(occurrence: TheoryOccurrence) -> &'static str {
    match occurrence {
        TheoryOccurrence::Head => "head",
        TheoryOccurrence::Body => "body",
        TheoryOccurrence::Any => "any",
        TheoryOccurrence::Directive => "directive",
    }
}

// ---- the theory term: an explicit work-list walk (§13) ----

/// A print action for the iterative theory-term walk — a node to render, a static separator,
/// or an operator run resolved to a string.
enum TheoryAct<'a> {
    Node(&'a TheoryTerm),
    Str(&'static str),
    Owned(String),
}

/// A theory term (grammar §5.8): the peer algebra's applied, tuple, list, set, and flat
/// operator-sequence forms, rendered from an explicit work list so a deep one renders without
/// call-stack recursion (§13). Its round-trip is up-to-grounding (§5).
fn render_theory_term(
    out: &mut String,
    term: &TheoryTerm,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    let mut work = vec![TheoryAct::Node(term)];
    while let Some(act) = work.pop() {
        match act {
            TheoryAct::Str(text) => out.push_str(text),
            TheoryAct::Owned(text) => out.push_str(&text),
            TheoryAct::Node(node) => match node {
                TheoryTerm::Symbolic(symbol) => render_symbol(out, symbol, dialect)?,
                TheoryTerm::Variable(variable) => render_variable(out, variable),
                TheoryTerm::Function { name, arguments } => {
                    out.push_str(name.as_str());
                    out.push('(');
                    work.push(TheoryAct::Str(")"));
                    push_theory_list(&mut work, arguments);
                }
                TheoryTerm::Tuple(items) => push_theory_bracketed(&mut work, out, "(", items, true),
                TheoryTerm::List(items) => push_theory_bracketed(&mut work, out, "[", items, false),
                TheoryTerm::Set(items) => push_theory_bracketed(&mut work, out, "{", items, false),
                TheoryTerm::Operation {
                    operators,
                    operands,
                } => {
                    for index in (0..operands.len()).rev() {
                        work.push(TheoryAct::Node(&operands[index]));
                        if let Some(run) = operators.get(index)
                            && !run.is_empty()
                        {
                            let mut text = String::new();
                            for (position, operator) in run.iter().enumerate() {
                                if position > 0 {
                                    text.push(' ');
                                }
                                text.push_str(operator.as_str());
                            }
                            text.push(' ');
                            work.push(TheoryAct::Owned(text));
                        }
                        if index > 0 {
                            work.push(TheoryAct::Str(" "));
                        }
                    }
                }
            },
        }
    }
    Ok(())
}

/// Push a comma-separated theory-term list, reversed so it renders left to right (§13).
fn push_theory_list<'a>(work: &mut Vec<TheoryAct<'a>>, items: &'a [TheoryTerm]) {
    for (index, item) in items.iter().enumerate().rev() {
        work.push(TheoryAct::Node(item));
        if index > 0 {
            work.push(TheoryAct::Str(", "));
        }
    }
}

/// Push a bracketed theory-term form — a tuple `( )`, list `[ ]`, or set `{ }`. A tuple keeps
/// the trailing comma that distinguishes its one-element form (grammar §5.8).
fn push_theory_bracketed<'a>(
    work: &mut Vec<TheoryAct<'a>>,
    out: &mut String,
    open: &'static str,
    items: &'a [TheoryTerm],
    tuple: bool,
) {
    out.push_str(open);
    let close = match open {
        "(" => ")",
        "[" => "]",
        _ => "}",
    };
    if tuple && items.len() == 1 {
        work.push(TheoryAct::Str(match close {
            ")" => ",)",
            other => other,
        }));
        work.push(TheoryAct::Node(&items[0]));
    } else {
        work.push(TheoryAct::Str(close));
        push_theory_list(work, items);
    }
}

// ---- the term: an explicit work-list walk (§13) ----

/// A print action for the iterative term walk — a node to render or a static separator (a
/// dynamic leaf renders in place, so no owned action is needed here).
enum TermAct<'a> {
    Node(&'a Term),
    Str(&'static str),
}

/// A term (grammar §5.1), rendered from an explicit work list so a term tens of thousands of
/// levels deep renders without call-stack recursion (§13). Binary operators and intervals are
/// fully parenthesized (§10). `O(output)`.
fn render_term(out: &mut String, term: &Term, dialect: Dialect) -> Result<(), Unspellable> {
    drain_terms(out, vec![TermAct::Node(term)], dialect)
}

/// A comma-separated list of borrowed terms, each rendered iteratively (§13) — a
/// grammar-bounded count of them, so the loop is bounded (§13).
fn render_term_refs(
    out: &mut String,
    terms: &[&Term],
    dialect: Dialect,
) -> Result<(), Unspellable> {
    for (index, term) in terms.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        render_term(out, term, dialect)?;
    }
    Ok(())
}

/// Drain a term work list, rendering each action (§13). A ground `Symbolic` leaf is rendered
/// by the symbol walk, which the term walk never descends into (§3.1).
fn drain_terms(
    out: &mut String,
    mut work: Vec<TermAct<'_>>,
    dialect: Dialect,
) -> Result<(), Unspellable> {
    while let Some(act) = work.pop() {
        match act {
            TermAct::Str(text) => out.push_str(text),
            TermAct::Node(term) => match term {
                Term::Variable(variable) => render_variable(out, variable),
                Term::Symbolic(symbol) => render_symbol(out, symbol, dialect)?,
                Term::Function { name, arguments } => {
                    push_applied(&mut work, out, None, name, arguments);
                }
                Term::Tuple(items) => push_tuple(&mut work, out, items),
                Term::Pool(items) => {
                    out.push('(');
                    work.push(TermAct::Str(")"));
                    push_separated(&mut work, items, "; ");
                }
                Term::UnaryOperation { operator, argument } => {
                    out.push_str(match operator {
                        UnaryOp::Negate => "-",
                        UnaryOp::BitwiseNot => "~",
                    });
                    work.push(TermAct::Node(argument));
                }
                Term::BinaryOperation {
                    operator,
                    left,
                    right,
                } => {
                    out.push('(');
                    work.push(TermAct::Str(")"));
                    work.push(TermAct::Node(right));
                    work.push(TermAct::Str(binary_op_str(*operator)));
                    work.push(TermAct::Node(left));
                }
                Term::Interval { lower, upper } => {
                    out.push('(');
                    work.push(TermAct::Str(")"));
                    work.push(TermAct::Node(upper));
                    work.push(TermAct::Str(" .. "));
                    work.push(TermAct::Node(lower));
                }
                Term::Absolute(inner) => {
                    out.push('|');
                    work.push(TermAct::Str("|"));
                    work.push(TermAct::Node(inner));
                }
                Term::External { name, arguments } => {
                    out.push('@');
                    out.push_str(name.as_str());
                    if !arguments.is_empty() {
                        out.push('(');
                        work.push(TermAct::Str(")"));
                        push_separated(&mut work, arguments, ", ");
                    }
                }
            },
        }
    }
    Ok(())
}

/// The single applied-form printer (§10): `[-]name` for a strongly-negated or plain form, then
/// `(a, b, …)` for a non-empty argument list — a function term (no sign) and an atom (its
/// sign) alike, so the two cannot drift. The name is written in place; the arguments join the
/// work list.
fn push_applied<'a>(
    work: &mut Vec<TermAct<'a>>,
    out: &mut String,
    sign: Option<Sign>,
    name: &Name,
    arguments: &'a [Term],
) {
    if matches!(sign, Some(Sign::Negative)) {
        out.push('-');
    }
    out.push_str(name.as_str());
    if !arguments.is_empty() {
        out.push('(');
        work.push(TermAct::Str(")"));
        push_separated(work, arguments, ", ");
    }
}

/// Push a tuple's elements (§10): `()` empty, `(a,)` keeping the one-element comma, `(a, b)`
/// otherwise. The open paren is written in place.
fn push_tuple<'a>(work: &mut Vec<TermAct<'a>>, out: &mut String, items: &'a [Term]) {
    out.push('(');
    if let [single] = items {
        work.push(TermAct::Str(",)"));
        work.push(TermAct::Node(single));
    } else {
        work.push(TermAct::Str(")"));
        push_separated(work, items, ", ");
    }
}

/// Push a separator-joined term list, reversed so it renders left to right (§13).
fn push_separated<'a>(work: &mut Vec<TermAct<'a>>, items: &'a [Term], separator: &'static str) {
    for (index, item) in items.iter().enumerate().rev() {
        work.push(TermAct::Node(item));
        if index > 0 {
            work.push(TermAct::Str(separator));
        }
    }
}

/// A variable (grammar §5.1): a named one or the anonymous `_`.
fn render_variable(out: &mut String, variable: &Variable) {
    match variable {
        Variable::Named(name) => out.push_str(name.as_str()),
        Variable::Anonymous => out.push('_'),
    }
}

/// A binary operator's spelling, spaces included (grammar §5.1, §4.6).
fn binary_op_str(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => " + ",
        BinaryOp::Sub => " - ",
        BinaryOp::Mul => " * ",
        BinaryOp::Div => " / ",
        BinaryOp::Mod => " \\ ",
        BinaryOp::Pow => " ** ",
        BinaryOp::BitAnd => " & ",
        BinaryOp::BitOr => " ? ",
        BinaryOp::BitXor => " ^ ",
    }
}

// ---- the ground symbol: an explicit work-list walk (§13) ----

/// A print action for the iterative symbol walk — a node to render or a static separator.
enum SymbolAct<'a> {
    Node(&'a Symbol),
    Str(&'static str),
}

/// A ground symbol (grammar §5.1's leaf forms), rendered from an explicit work list so a deep
/// ground value renders without call-stack recursion (§13). A string value is spelled under
/// the dialect, the one place a render can refuse (§10).
fn render_symbol(out: &mut String, symbol: &Symbol, dialect: Dialect) -> Result<(), Unspellable> {
    let mut work = vec![SymbolAct::Node(symbol)];
    while let Some(act) = work.pop() {
        match act {
            SymbolAct::Str(text) => out.push_str(text),
            SymbolAct::Node(node) => match node {
                Symbol::Infimum => out.push_str("#inf"),
                Symbol::Supremum => out.push_str("#sup"),
                Symbol::Number(value) => push_i32(out, *value),
                Symbol::String(value) => out.push_str(&spell_string(value, dialect)?),
                Symbol::Function {
                    name,
                    arguments,
                    sign,
                } => {
                    if matches!(sign, Sign::Negative) {
                        out.push('-');
                    }
                    out.push_str(name.as_str());
                    if !arguments.is_empty() {
                        out.push('(');
                        work.push(SymbolAct::Str(")"));
                        push_symbol_list(&mut work, arguments);
                    }
                }
                Symbol::Tuple(elements) => {
                    out.push('(');
                    if let [single] = elements.as_slice() {
                        work.push(SymbolAct::Str(",)"));
                        work.push(SymbolAct::Node(single));
                    } else {
                        work.push(SymbolAct::Str(")"));
                        push_symbol_list(&mut work, elements);
                    }
                }
            },
        }
    }
    Ok(())
}

/// Push a comma-separated symbol list, reversed so it renders left to right (§13).
fn push_symbol_list<'a>(work: &mut Vec<SymbolAct<'a>>, items: &'a [Symbol]) {
    for (index, item) in items.iter().enumerate().rev() {
        work.push(SymbolAct::Node(item));
        if index > 0 {
            work.push(SymbolAct::Str(", "));
        }
    }
}

// ---- string spelling: the one refusal (grammar §4.4/§6.2/§9) ----

/// Spell a string value under a dialect (grammar §4.4/§6.2), or refuse (§10). The clingo rule
/// (§4.4) has exactly three escapes — `\"`, `\\`, `\n` — and no other control character has a
/// spelling, so a value bearing one is refused (grammar §9's owned gap). The ASP-Core-2 rule
/// (§6.2) escapes only the quote and spells every other character raw, a backslash and a raw
/// line break included, so it never refuses.
fn spell_string(value: &str, dialect: Dialect) -> Result<String, Unspellable> {
    let mut spelled = String::with_capacity(value.len() + 2);
    spelled.push('"');
    match dialect {
        Dialect::Clingo => {
            for character in value.chars() {
                match character {
                    '"' => spelled.push_str("\\\""),
                    '\\' => spelled.push_str("\\\\"),
                    '\n' => spelled.push_str("\\n"),
                    other if other.is_control() => {
                        return Err(Unspellable {
                            value: value.to_owned(),
                            dialect,
                        });
                    }
                    other => spelled.push(other),
                }
            }
        }
        Dialect::AspCore2 => {
            for character in value.chars() {
                if character == '"' {
                    spelled.push_str("\\\"");
                } else {
                    spelled.push(character);
                }
            }
        }
    }
    spelled.push('"');
    Ok(spelled)
}

// ---- small leaf writers ----

/// Append a decimal `i32` (grammar §4.3's `NUMBER`). A short-lived buffer keeps it clear;
/// `O(1)` in the value's digits.
fn push_i32(out: &mut String, value: i32) {
    out.push_str(&value.to_string());
}

/// Append a decimal `u32` — an arity or priority (grammar §4.3, §5.9).
fn push_u32(out: &mut String, value: u32) {
    out.push_str(&value.to_string());
}

// ---- the set-shaped brace body, shared by the aggregates and the choice (§4) ----

/// Render a set-shaped brace body's elements in `Ord` order, semicolon-separated with the
/// grammar's inner spacing — `{}` empty, `{ e1; e2 }` otherwise (grammar §5.3). The one shape
/// the choice and the three aggregates share (§4.7).
fn render_set_body<T>(
    out: &mut String,
    elements: &[&T],
    dialect: Dialect,
    mut render_element: impl FnMut(&mut String, &T, Dialect) -> Result<(), Unspellable>,
) -> Result<(), Unspellable> {
    for (index, element) in elements.iter().enumerate() {
        out.push_str(if index == 0 { " " } else { "; " });
        render_element(out, element, dialect)?;
    }
    if !elements.is_empty() {
        out.push(' ');
    }
    Ok(())
}
