//! The construct scan (docs/design/analysis.md §7): which of the language's
//! constructs a program uses, each paired with the first statement that bears it
//! so a consumer can point at one. The simplest facet, and the one the syntactic
//! classes (§6.3) and a formatter's lints read. It is a **fact, not a judgement**
//! (§1): it reports that a program *uses* an aggregate, never that using one is
//! good or bad — a lint's policy over the scan is the lint's.
//!
//! One iterative walk of the program (docs/design/program.md §13): the walk
//! descends the grammar-bounded structural spine — a bounded recursion — and
//! bottoms out in `Term`'s iterative `subterms` (program §3.6) for the term-level
//! constructs, so a deeply nested term cannot overflow the stack. (The program
//! tier's `analyze.rs` substrate walks the same spine the same way.) It reads the
//! program's *structure*, not its provenance (§8): two programs equal up to
//! provenance yield equal scans, and a constructed program (whose statements have
//! no source span) scans as soundly as a parsed one. The witness is the bearing
//! statement, identified by its structural value; a source span is read from its
//! own provenance when a consumer wants one (program §6).

use std::collections::{BTreeMap, BTreeSet};

use themelios_program::program::{
    Aggregate, Atom, Body, BodyElement, Choice, Comparison, Condition, ConditionalLiteral,
    DefaultNegation, Disjunction, HasGuards, Head, HeadAggregate, Literal, LiteralInner,
    OptimizeElement, Program, Project, Rule, SetAggregate, SetElement, Show, Statement, TheoryAtom,
    Weight,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::Sign;
use themelios_program::term::Term;

/// Which constructs a program uses. A set of flags with, for each, the first
/// occurrence's statement so a consumer can point at one (§7). Equality is
/// structural and provenance-blind: the witness carrier erases provenance
/// (program §6.2), so two programs equal up to provenance yield equal scans.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Constructs {
    /// The first statement bearing each construct that occurs — presence is the
    /// flag, the value the witness. A `BTreeMap`, so `all` iterates in `Construct`
    /// order and equality is structural.
    first_occurrence: BTreeMap<Construct, WithProvenance<Statement>>,
}

impl Constructs {
    /// The construct scan of a program (§7): one iterative walk recording, for each
    /// construct, the first statement that bears it. `O(program)`.
    pub fn of(program: &Program) -> Constructs {
        let mut first_occurrence = BTreeMap::new();
        for statement in program.statements() {
            record(statement, &mut first_occurrence);
        }
        Constructs { first_occurrence }
    }

    /// Whether the program uses the construct. `O(1)`.
    pub fn uses(&self, construct: Construct) -> bool {
        self.first_occurrence.contains_key(&construct)
    }

    /// The first statement bearing a construct, if any — the "show me one" a lint
    /// wants (§7). Clones the witness, `O(witness)`.
    pub fn first(&self, construct: Construct) -> Option<WithProvenance<Statement>> {
        self.first_occurrence.get(&construct).cloned()
    }

    /// Each used construct paired with the first statement that bears it, in
    /// `Construct` order (§7).
    pub fn all(&self) -> impl Iterator<Item = (Construct, WithProvenance<Statement>)> {
        self.first_occurrence
            .iter()
            .map(|(construct, statement)| (*construct, statement.clone()))
    }
}

/// The constructs an analysis or a lint distinguishes (grammar §5). Closed; a
/// construct is admitted when a consumer names it (§7).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Construct {
    /// A disjunctive head, `a | b`.
    Disjunction,
    /// A choice head, `{ a; b }`.
    Choice,
    /// A head aggregate deriving atoms.
    HeadAggregate,
    /// A body aggregate — a function or set aggregate at body-element position.
    BodyAggregate,
    /// Strong negation on an atom, `-p`.
    StrongNegation,
    /// Default negation, `not` or `not not`.
    DefaultNegation,
    /// A comparison literal, `X < 1`.
    Comparison,
    /// An interval term, `1 .. 3`.
    Interval,
    /// A pool — a pool term `(a; b)`, or a pooled argument list `p(a; b)`.
    Pool,
    /// An arithmetic operator term — a unary, binary, or absolute-value operation.
    Arithmetic,
    /// A `#minimize`/`#maximize` statement.
    Optimization,
    /// A weak constraint, `:~ … . [w@p]`.
    WeakConstraint,
    /// A theory atom, `&name { … }`.
    TheoryAtom,
    /// An `#external` statement.
    ExternalStatement,
    /// A `#heuristic` statement.
    Heuristic,
    /// An `#edge` statement.
    Edge,
    /// An `@`-call term, a ground-extension call site.
    ExternalCall,
}

// ---- The walk (§7) ----
//
// A statement is processed once; the constructs it bears are collected into a set,
// and each is recorded against this statement unless an earlier statement already
// bears it — so `first_occurrence` keeps the first in the program's statement order.
// Each `scan_*` reads through the public accessors, mirroring the program tier's
// `analyze.rs`; the spine is grammar-bounded and the term walk is `Term::subterms`.

/// Record every construct the statement bears, keeping the first bearing statement
/// per construct.
fn record(
    statement: &WithProvenance<Statement>,
    first_occurrence: &mut BTreeMap<Construct, WithProvenance<Statement>>,
) {
    let mut found = BTreeSet::new();
    scan_statement(statement.get(), &mut found);
    for construct in found {
        first_occurrence
            .entry(construct)
            .or_insert_with(|| statement.clone());
    }
}

fn scan_statement(statement: &Statement, found: &mut BTreeSet<Construct>) {
    match statement {
        Statement::Rule(rule) => scan_rule(rule, found),
        Statement::WeakConstraint(weak) => {
            found.insert(Construct::WeakConstraint);
            scan_body(weak.body().get(), found);
            scan_weight(weak.weight(), found);
            for term in weak.terms() {
                scan_term(term, found);
            }
        }
        Statement::Optimize(optimize) => {
            found.insert(Construct::Optimization);
            for element in optimize.elements() {
                scan_optimize_element(element.get(), found);
            }
        }
        Statement::Show(show) => scan_show(show, found),
        Statement::Project(project) => scan_project(project, found),
        Statement::Edge(edge) => {
            found.insert(Construct::Edge);
            for (from, to) in edge.pairs() {
                scan_term(from, found);
                scan_term(to, found);
            }
            scan_body(edge.body().get(), found);
        }
        Statement::Heuristic(heuristic) => {
            found.insert(Construct::Heuristic);
            scan_atom(heuristic.atom().get(), found);
            scan_body(heuristic.body().get(), found);
            scan_term(heuristic.bias(), found);
            if let Some(priority) = heuristic.priority() {
                scan_term(priority, found);
            }
            scan_term(heuristic.modifier(), found);
        }
        Statement::External(external) => {
            found.insert(Construct::ExternalStatement);
            scan_atom(external.atom().get(), found);
            scan_body(external.body().get(), found);
            if let Some(value) = external.value() {
                scan_term(value, found);
            }
        }
        Statement::Const(constant) => scan_term(&constant.value, found),
        Statement::Query(query) => scan_atom(query.atom().get(), found),
        // The term-free and opaque directives (`#defined`, `#include`, `#script`,
        // `#theory`) bear no construct; `Statement` is non-exhaustive, so a future
        // family bears none until a `Construct` names it (§7).
        _ => {}
    }
}

fn scan_rule(rule: &Rule, found: &mut BTreeSet<Construct>) {
    scan_head(rule.head().get(), found);
    scan_body(rule.body().get(), found);
}

fn scan_head(head: &Head, found: &mut BTreeSet<Construct>) {
    match head {
        Head::Literal(literal) => scan_literal(literal, found),
        Head::Disjunction(disjunction) => {
            found.insert(Construct::Disjunction);
            scan_disjunction(disjunction, found);
        }
        Head::Choice(choice) => {
            found.insert(Construct::Choice);
            scan_choice(choice, found);
        }
        Head::Aggregate(aggregate) => {
            found.insert(Construct::HeadAggregate);
            scan_head_aggregate(aggregate, found);
        }
        Head::TheoryAtom(atom) => {
            found.insert(Construct::TheoryAtom);
            scan_theory_atom(atom, found);
        }
        Head::Falsum | Head::Verum => {}
    }
}

fn scan_disjunction(disjunction: &Disjunction, found: &mut BTreeSet<Construct>) {
    for element in disjunction.elements() {
        scan_literal(element.get().literal(), found);
        scan_condition(element.get().condition(), found);
    }
}

fn scan_choice(choice: &Choice, found: &mut BTreeSet<Construct>) {
    if let Some(guard) = choice.left_guard() {
        scan_term(&guard.get().term, found);
    }
    for element in choice.elements() {
        scan_literal(element.get().literal(), found);
        scan_condition(element.get().condition(), found);
    }
    if let Some(guard) = choice.right_guard() {
        scan_term(&guard.get().term, found);
    }
}

fn scan_head_aggregate(aggregate: &HeadAggregate, found: &mut BTreeSet<Construct>) {
    scan_guards(aggregate, found);
    for element in aggregate.elements() {
        for term in element.get().terms() {
            scan_term(term, found);
        }
        scan_literal(element.get().literal(), found);
        scan_condition(element.get().condition(), found);
    }
}

/// The two guards of a guarded aggregate carry ordinary term bounds (§7).
fn scan_guards(aggregate: &impl HasGuards, found: &mut BTreeSet<Construct>) {
    if let Some(guard) = aggregate.left_guard() {
        scan_term(&guard.get().term, found);
    }
    if let Some(guard) = aggregate.right_guard() {
        scan_term(&guard.get().term, found);
    }
}

fn scan_body(body: &Body, found: &mut BTreeSet<Construct>) {
    for element in body.elements() {
        scan_body_element(element.get(), found);
    }
}

fn scan_body_element(element: &BodyElement, found: &mut BTreeSet<Construct>) {
    match element {
        BodyElement::Literal(literal) => scan_literal(literal, found),
        BodyElement::Conditional(conditional) => scan_conditional(conditional, found),
        BodyElement::Aggregate {
            negation,
            aggregate,
        } => {
            found.insert(Construct::BodyAggregate);
            note_default_negation(*negation, found);
            scan_aggregate(aggregate, found);
        }
        BodyElement::TheoryAtom { negation, atom } => {
            found.insert(Construct::TheoryAtom);
            note_default_negation(*negation, found);
            scan_theory_atom(atom, found);
        }
        // `BodyElement` is non-exhaustive: a future element bears no construct until
        // a `Construct` names it (§7).
        _ => {}
    }
}

fn scan_literal(literal: &Literal, found: &mut BTreeSet<Construct>) {
    note_default_negation(literal.negation, found);
    match &literal.inner {
        LiteralInner::Atom(atom) => scan_atom(atom.get(), found),
        LiteralInner::Comparison(comparison) => {
            found.insert(Construct::Comparison);
            scan_comparison(comparison.get(), found);
        }
        LiteralInner::True | LiteralInner::False => {}
    }
}

fn scan_atom(atom: &Atom, found: &mut BTreeSet<Construct>) {
    if atom.sign == Sign::Negative {
        found.insert(Construct::StrongNegation);
    }
    if atom.is_pooled() {
        // An argument-list pool `p(a; b)` is pooling at the atom's own level — the same
        // `Construct::Pool` a `Term::Pool` reports (program §4.6), so a reader learns pooling once
        // and the faithful scan reports it whichever spelling the source used.
        found.insert(Construct::Pool);
    }
    for term in atom.argument_terms() {
        scan_term(term, found);
    }
}

fn scan_comparison(comparison: &Comparison, found: &mut BTreeSet<Construct>) {
    scan_term(comparison.first(), found);
    for (_relation, term) in comparison.steps() {
        scan_term(term, found);
    }
}

fn scan_condition(condition: &Condition, found: &mut BTreeSet<Construct>) {
    for literal in condition.literals() {
        scan_literal(literal.get(), found);
    }
}

fn scan_conditional(conditional: &ConditionalLiteral, found: &mut BTreeSet<Construct>) {
    scan_literal(&conditional.literal, found);
    scan_condition(&conditional.condition, found);
}

fn scan_aggregate(aggregate: &Aggregate, found: &mut BTreeSet<Construct>) {
    match aggregate {
        Aggregate::Function(function) => {
            scan_guards(function, found);
            for element in function.elements() {
                for term in element.get().terms() {
                    scan_term(term, found);
                }
                scan_condition(element.get().condition(), found);
            }
        }
        Aggregate::Set(set) => scan_set_aggregate(set, found),
    }
}

fn scan_set_aggregate(set: &SetAggregate, found: &mut BTreeSet<Construct>) {
    scan_guards(set, found);
    for element in set.elements() {
        match element.get() {
            SetElement::Literal(literal) => scan_literal(literal, found),
            SetElement::ConditionalLiteral(conditional) => scan_conditional(conditional, found),
        }
    }
}

fn scan_theory_atom(atom: &TheoryAtom, found: &mut BTreeSet<Construct>) {
    // The ordinary-term arguments carry the ordinary term-level constructs; the theory
    // terms are the peer algebra (program §4.9) and carry none of them, so they are not
    // scanned. An element's condition is ordinary literals.
    for term in atom.arguments() {
        scan_term(term, found);
    }
    for element in atom.elements() {
        if let Some(condition) = element.get().condition() {
            scan_condition(condition, found);
        }
    }
}

fn scan_show(show: &Show, found: &mut BTreeSet<Construct>) {
    match show {
        Show::Term(term) => scan_term(term, found),
        Show::TermBody { term, body } => {
            scan_term(term, found);
            scan_body(body.get(), found);
        }
        Show::All | Show::Signature(_) => {}
    }
}

fn scan_project(project: &Project, found: &mut BTreeSet<Construct>) {
    match project {
        Project::Atom { atom, body } => {
            scan_atom(atom.get(), found);
            scan_body(body.get(), found);
        }
        Project::Signature(_) => {}
    }
}

fn scan_optimize_element(element: &OptimizeElement, found: &mut BTreeSet<Construct>) {
    scan_weight(element.weight(), found);
    for term in element.terms() {
        scan_term(term, found);
    }
    scan_condition(element.condition(), found);
}

fn scan_weight(weight: &Weight, found: &mut BTreeSet<Construct>) {
    scan_term(weight.term(), found);
    if let Some(priority) = weight.priority() {
        scan_term(priority, found);
    }
}

fn note_default_negation(negation: DefaultNegation, found: &mut BTreeSet<Construct>) {
    if negation != DefaultNegation::None {
        found.insert(Construct::DefaultNegation);
    }
}

/// The term-level constructs of a term, read from its pre-order `subterms` (program
/// §3.6) — iterative, so a deeply nested term does not recurse the stack. The ground
/// `Symbolic` leaf is not descended: an interval, a pool, an arithmetic operator, and
/// an `@`-call are all non-ground term-formers.
fn scan_term(term: &Term, found: &mut BTreeSet<Construct>) {
    for subterm in term.subterms() {
        match subterm {
            Term::Interval { .. } => {
                found.insert(Construct::Interval);
            }
            Term::Pool(_) => {
                found.insert(Construct::Pool);
            }
            Term::UnaryOperation { .. } | Term::BinaryOperation { .. } | Term::Absolute(_) => {
                found.insert(Construct::Arithmetic);
            }
            Term::External { .. } => {
                found.insert(Construct::ExternalCall);
            }
            Term::Variable(_) | Term::Symbolic(_) | Term::Function { .. } | Term::Tuple(_) => {}
        }
    }
}
