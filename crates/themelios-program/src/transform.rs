//! The transformation surface (docs/design/program.md §9): a read-only visitor and a
//! `Program -> Program` rewriter, the second load-bearing foundation of the tier. Both are
//! iterative in depth (§13) — the term-level walks in `Term`'s `fold`/`subterms` (§3.6), the
//! structural walks a grammar-bounded descent — so neither overflows on a deep term. The
//! rewrite carries provenance through: every node it rebuilds gains an `Origin::Transformed`
//! tag unioned with the node's own origin (§6), so a rewritten rule traces back to the rule
//! it came from (the *transformation* witness, spec §3), and its output is canonicalized at
//! the ingest door (§5.1). The boundary is **structural**, not semantic (§9.3): the surface
//! transforms `P` into `Q` carrying provenance and claims nothing about answer sets — an
//! optimizer's or an explainer's author proves the semantics, the solve tier's differential
//! checks it. Ordinary terms are the rewrite's reach; a theory term is a distinct peer
//! algebra (§4.9) this structural surface does not descend, as substitution does not (§9.2).

use crate::program::{
    Aggregate, Atom, Body, BodyAggregateElement, BodyElement, Choice, ChoiceElement, Comparison,
    Condition, ConditionalLiteral, Const, Disjunction, DisjunctionElement, Edge, External,
    FunctionAggregate, Guard, HasGuards, Head, HeadAggregate, HeadAggregateElement, Heuristic,
    Literal, LiteralInner, Optimize, OptimizeElement, Program, Project, Query, Rule, SetAggregate,
    SetElement, Show, Statement, TheoryAtom, TheoryElement, WeakConstraint, Weight, weight,
};
use crate::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use crate::term::Term;

// ---- The visitor (§9.1) ----

/// A read-only walk over a program — for analysis and collection. Each method defaults to
/// descending into the node's children; a consumer overrides only the kinds it reads. The
/// walk reaches every node once: a consumer overriding [`visit_atom`](Visit::visit_atom)
/// sees every atom, one overriding [`visit_term`](Visit::visit_term) sees every term node.
/// Iterative in depth (§13). A theory term is not descended (§4.9).
pub trait Visit {
    /// A statement — defaults to descending into its rule or directive.
    fn visit_statement(&mut self, statement: &Statement) {
        descend_statement(self, statement);
    }
    /// A rule — defaults to visiting its head and body.
    fn visit_rule(&mut self, rule: &Rule) {
        descend_rule(self, rule);
    }
    /// A rule head — defaults to descending its shape.
    fn visit_head(&mut self, head: &Head) {
        descend_head(self, head);
    }
    /// A rule body — defaults to visiting its elements.
    fn visit_body(&mut self, body: &Body) {
        descend_body(self, body);
    }
    /// A literal — defaults to visiting its atom or comparison.
    fn visit_literal(&mut self, literal: &Literal) {
        descend_literal(self, literal);
    }
    /// A signed atom — defaults to visiting its argument terms.
    fn visit_atom(&mut self, atom: &Atom) {
        descend_atom(self, atom);
    }
    /// A comparison chain — defaults to visiting its terms.
    fn visit_comparison(&mut self, comparison: &Comparison) {
        descend_comparison(self, comparison);
    }
    /// A condition — defaults to visiting its literals.
    fn visit_condition(&mut self, condition: &Condition) {
        descend_condition(self, condition);
    }
    /// A body aggregate — defaults to visiting its guards, terms, and elements.
    fn visit_aggregate(&mut self, aggregate: &Aggregate) {
        descend_aggregate(self, aggregate);
    }
    /// A theory atom — defaults to visiting its ordinary arguments and its elements'
    /// ordinary conditions (its theory terms are a peer algebra, §4.9, not descended).
    fn visit_theory_atom(&mut self, atom: &TheoryAtom) {
        descend_theory_atom(self, atom);
    }
    /// A term node. The walk reaches every node of every term, so a consumer that reads
    /// terms overrides this and needs no descent of its own; the default reads nothing.
    fn visit_term(&mut self, term: &Term) {
        let _ = term;
    }
}

/// Walk a program, visiting every statement (§9.1). Read-only; the only allocation is the
/// visitor's own.
pub fn visit(program: &Program, visitor: &mut impl Visit) {
    for statement in program.statements() {
        visitor.visit_statement(statement.get());
    }
}

/// Visit every term node of a term — the per-node walk, iterative (§13).
fn visit_terms<V: Visit + ?Sized>(v: &mut V, term: &Term) {
    for node in term.subterms() {
        v.visit_term(node);
    }
}

fn descend_statement<V: Visit + ?Sized>(v: &mut V, statement: &Statement) {
    match statement {
        Statement::Rule(rule) => v.visit_rule(rule),
        Statement::WeakConstraint(weak) => {
            v.visit_body(weak.body().get());
            visit_weight(v, weak.weight());
            for term in weak.terms() {
                visit_terms(v, term);
            }
        }
        Statement::Optimize(optimize) => {
            for element in optimize.elements() {
                visit_optimize_element(v, element.get());
            }
        }
        Statement::Show(show) => match show {
            Show::All | Show::Signature(_) => {}
            Show::Term(term) => visit_terms(v, term),
            Show::TermBody { term, body } => {
                visit_terms(v, term);
                v.visit_body(body.get());
            }
        },
        Statement::Project(project) => match project {
            Project::Signature(_) => {}
            Project::Atom { atom, body } => {
                v.visit_atom(atom.get());
                v.visit_body(body.get());
            }
        },
        Statement::Edge(edge) => {
            for (from, to) in edge.pairs() {
                visit_terms(v, from);
                visit_terms(v, to);
            }
            v.visit_body(edge.body().get());
        }
        Statement::Heuristic(heuristic) => {
            v.visit_atom(heuristic.atom().get());
            v.visit_body(heuristic.body().get());
            visit_terms(v, heuristic.bias());
            if let Some(priority) = heuristic.priority() {
                visit_terms(v, priority);
            }
            visit_terms(v, heuristic.modifier());
        }
        Statement::External(external) => {
            v.visit_atom(external.atom().get());
            v.visit_body(external.body().get());
            if let Some(value) = external.value() {
                visit_terms(v, value);
            }
        }
        Statement::Const(constant) => visit_terms(v, &constant.value),
        Statement::Query(query) => v.visit_atom(query.atom().get()),
        Statement::Defined(_)
        | Statement::Include(_)
        | Statement::Script(_)
        | Statement::TheoryDefinition(_) => {}
    }
}

fn descend_rule<V: Visit + ?Sized>(v: &mut V, rule: &Rule) {
    v.visit_head(rule.head().get());
    v.visit_body(rule.body().get());
}

fn descend_head<V: Visit + ?Sized>(v: &mut V, head: &Head) {
    match head {
        Head::Literal(literal) => v.visit_literal(literal),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                v.visit_literal(element.get().literal());
                v.visit_condition(element.get().condition());
            }
        }
        Head::Choice(choice) => {
            visit_guards(v, choice.left_guard(), choice.right_guard());
            for element in choice.elements() {
                v.visit_literal(element.get().literal());
                v.visit_condition(element.get().condition());
            }
        }
        Head::Aggregate(aggregate) => {
            visit_guards(v, aggregate.left_guard(), aggregate.right_guard());
            for element in aggregate.elements() {
                for term in element.get().terms() {
                    visit_terms(v, term);
                }
                v.visit_literal(element.get().literal());
                v.visit_condition(element.get().condition());
            }
        }
        Head::TheoryAtom(atom) => v.visit_theory_atom(atom),
        Head::Falsum | Head::Verum => {}
    }
}

fn descend_body<V: Visit + ?Sized>(v: &mut V, body: &Body) {
    for element in body.elements() {
        match element.get() {
            BodyElement::Literal(literal) => v.visit_literal(literal),
            BodyElement::Conditional(conditional) => {
                v.visit_literal(&conditional.literal);
                v.visit_condition(&conditional.condition);
            }
            BodyElement::Aggregate { aggregate, .. } => v.visit_aggregate(aggregate),
            BodyElement::TheoryAtom { atom, .. } => v.visit_theory_atom(atom),
        }
    }
}

fn descend_literal<V: Visit + ?Sized>(v: &mut V, literal: &Literal) {
    match &literal.inner {
        LiteralInner::Atom(atom) => v.visit_atom(atom.get()),
        LiteralInner::Comparison(comparison) => v.visit_comparison(comparison.get()),
        LiteralInner::True | LiteralInner::False => {}
    }
}

fn descend_atom<V: Visit + ?Sized>(v: &mut V, atom: &Atom) {
    for term in &atom.arguments {
        visit_terms(v, term);
    }
}

fn descend_comparison<V: Visit + ?Sized>(v: &mut V, comparison: &Comparison) {
    visit_terms(v, comparison.first());
    for (_relation, term) in comparison.steps() {
        visit_terms(v, term);
    }
}

fn descend_condition<V: Visit + ?Sized>(v: &mut V, condition: &Condition) {
    for literal in condition.literals() {
        v.visit_literal(literal.get());
    }
}

fn descend_aggregate<V: Visit + ?Sized>(v: &mut V, aggregate: &Aggregate) {
    match aggregate {
        Aggregate::Function(function) => {
            visit_guards(v, function.left_guard(), function.right_guard());
            for element in function.elements() {
                for term in element.get().terms() {
                    visit_terms(v, term);
                }
                v.visit_condition(element.get().condition());
            }
        }
        Aggregate::Set(set) => {
            visit_guards(v, set.left_guard(), set.right_guard());
            for element in set.elements() {
                match element.get() {
                    SetElement::Literal(literal) => v.visit_literal(literal),
                    SetElement::ConditionalLiteral(conditional) => {
                        v.visit_literal(&conditional.literal);
                        v.visit_condition(&conditional.condition);
                    }
                }
            }
        }
    }
}

fn descend_theory_atom<V: Visit + ?Sized>(v: &mut V, atom: &TheoryAtom) {
    for term in atom.arguments() {
        visit_terms(v, term);
    }
    for element in atom.elements() {
        if let Some(condition) = element.get().condition() {
            v.visit_condition(condition);
        }
    }
}

fn visit_guards<V: Visit + ?Sized>(
    v: &mut V,
    left: Option<&WithProvenance<Guard>>,
    right: Option<&WithProvenance<Guard>>,
) {
    for guard in [left, right].into_iter().flatten() {
        visit_terms(v, &guard.get().term);
    }
}

fn visit_weight<V: Visit + ?Sized>(v: &mut V, weight: &Weight) {
    visit_terms(v, weight.term());
    if let Some(priority) = weight.priority() {
        visit_terms(v, priority);
    }
}

fn visit_optimize_element<V: Visit + ?Sized>(v: &mut V, element: &OptimizeElement) {
    visit_weight(v, element.weight());
    for term in element.terms() {
        visit_terms(v, term);
    }
    v.visit_condition(element.condition());
}

// ---- The rewriter (§9.1) ----

/// A `Program -> Program` rewrite. Each method defaults to descending, rewriting the node's
/// children, and rebuilding; a consumer overrides only the kinds it rewrites. Total and
/// iterative in depth (§13). The framework carries provenance: every node the rewrite
/// rebuilds gains `Origin::Transformed(tag())` unioned with its origin (§6), and the output
/// is canonicalized at the ingest door (§5.1). A theory term is not descended (§4.9).
pub trait Rewrite {
    /// The name of this transformation, stamped on every node it produces (§6, §9.1).
    fn tag(&self) -> TransformTag;
    /// A statement — defaults to rewriting its rule or directive.
    fn rewrite_statement(&mut self, statement: Statement) -> Statement {
        rebuild_statement(self, statement)
    }
    /// A rule — defaults to rewriting its head and body.
    fn rewrite_rule(&mut self, rule: Rule) -> Rule {
        rebuild_rule(self, rule)
    }
    /// A rule head — defaults to rewriting its shape.
    fn rewrite_head(&mut self, head: Head) -> Head {
        rebuild_head(self, head)
    }
    /// A rule body — defaults to rewriting its elements.
    fn rewrite_body(&mut self, body: Body) -> Body {
        rebuild_body(self, body)
    }
    /// A literal — defaults to rewriting its atom or comparison.
    fn rewrite_literal(&mut self, literal: Literal) -> Literal {
        rebuild_literal(self, literal)
    }
    /// A signed atom — defaults to rewriting its argument terms.
    fn rewrite_atom(&mut self, atom: Atom) -> Atom {
        rebuild_atom(self, atom)
    }
    /// A comparison chain — defaults to rewriting its terms.
    fn rewrite_comparison(&mut self, comparison: Comparison) -> Comparison {
        rebuild_comparison(self, comparison)
    }
    /// A condition — defaults to rewriting its literals.
    fn rewrite_condition(&mut self, condition: Condition) -> Condition {
        rebuild_condition(self, condition)
    }
    /// A body aggregate — defaults to rewriting its guards, terms, and elements.
    fn rewrite_aggregate(&mut self, aggregate: Aggregate) -> Aggregate {
        rebuild_aggregate(self, aggregate)
    }
    /// A theory atom — defaults to rewriting its ordinary arguments and its elements'
    /// ordinary conditions (its theory terms are a peer algebra, §4.9, not descended).
    fn rewrite_theory_atom(&mut self, atom: TheoryAtom) -> TheoryAtom {
        rebuild_theory_atom(self, atom)
    }
    /// A term node, applied bottom-up over every node of a term by the framework's fold
    /// (§3.6). The default is the identity — the rebuilt node unchanged.
    fn rewrite_term(&mut self, term: Term) -> Term {
        term
    }
}

/// Rewrite a program, rebuilding it part by part (§9.1): each statement's content is
/// rewritten, its carrier stamped with the transformation tag, and the result routed through
/// the ingest door, which canonicalizes it (§5.1) and merges any newly content-equal
/// statements. Total; `O(output)`.
pub fn rewrite(program: Program, rewriter: &mut impl Rewrite) -> Program {
    let mut result = Program::default();
    for (key, carrier) in program.into_statements() {
        let origin = carrier.provenance().clone();
        let statement = rewriter.rewrite_statement(carrier.into_value());
        let stamped = WithProvenance::new(statement, stamp(origin, rewriter.tag()));
        result.ingest_into(key, stamped);
    }
    result
}

/// Union the transformation tag into a node's origin (§6, §9.1) — nothing lost, the
/// `Transformed` fact added.
fn stamp(origin: Provenance, tag: TransformTag) -> Provenance {
    origin.merge(Provenance::from(Origin::Transformed(tag)))
}

/// Rewrite an owned provenance carrier: consume it, rewrite its content through `f`, and
/// stamp the rebuilt carrier with the transformation tag (§9.1). The door for a carrier a
/// parent owns — a rule's head and body, a set consumed by value.
fn rewrite_owned_carrier<R, T>(
    r: &mut R,
    carrier: WithProvenance<T>,
    f: impl FnOnce(&mut R, T) -> T,
) -> WithProvenance<T>
where
    R: Rewrite + ?Sized,
{
    let origin = carrier.provenance().clone();
    let content = f(r, carrier.into_value());
    WithProvenance::new(content, stamp(origin, r.tag()))
}

/// Rewrite a borrowed provenance carrier: rewrite its content through `f` and stamp the
/// rebuilt carrier (§9.1). The door for a set reached through a borrowing accessor.
fn rewrite_carrier<R, T>(
    r: &mut R,
    carrier: &WithProvenance<T>,
    f: impl FnOnce(&mut R, &T) -> T,
) -> WithProvenance<T>
where
    R: Rewrite + ?Sized,
{
    let content = f(r, carrier.get());
    WithProvenance::new(content, stamp(carrier.provenance().clone(), r.tag()))
}

/// Rewrite every node of a term bottom-up, in `Term`'s iterative fold (§3.6, §13): the
/// (possibly overridden) `rewrite_term` is applied to each rebuilt node, so a deep term is
/// stack-safe and a fold-arithmetic or renaming pass reaches every occurrence.
fn rewrite_term<R: Rewrite + ?Sized>(r: &mut R, term: Term) -> Term {
    term.fold(|parts| r.rewrite_term(Term::from(parts)))
}

fn rebuild_statement<R: Rewrite + ?Sized>(r: &mut R, statement: Statement) -> Statement {
    match statement {
        Statement::Rule(rule) => Statement::Rule(r.rewrite_rule(rule)),
        Statement::WeakConstraint(weak) => {
            Statement::WeakConstraint(rebuild_weak_constraint(r, &weak))
        }
        Statement::Optimize(optimize) => Statement::Optimize(rebuild_optimize(r, &optimize)),
        Statement::Show(show) => Statement::Show(rebuild_show(r, show)),
        Statement::Project(project) => Statement::Project(rebuild_project(r, project)),
        Statement::Edge(edge) => Statement::Edge(rebuild_edge(r, &edge)),
        Statement::Heuristic(heuristic) => Statement::Heuristic(rebuild_heuristic(r, &heuristic)),
        Statement::External(external) => Statement::External(rebuild_external(r, &external)),
        Statement::Const(constant) => Statement::Const(rebuild_const(r, constant)),
        Statement::Query(query) => Statement::Query(Query::from_nodes(rewrite_carrier(
            r,
            query.atom(),
            |r, atom| r.rewrite_atom(atom.clone()),
        ))),
        Statement::Defined(_)
        | Statement::Include(_)
        | Statement::Script(_)
        | Statement::TheoryDefinition(_) => statement,
    }
}

fn rebuild_rule<R: Rewrite + ?Sized>(r: &mut R, rule: Rule) -> Rule {
    let (head, body) = rule.into_parts();
    let head = rewrite_owned_carrier(r, head, Rewrite::rewrite_head);
    let body = rewrite_owned_carrier(r, body, Rewrite::rewrite_body);
    Rule::from_nodes(head, body)
}

fn rebuild_head<R: Rewrite + ?Sized>(r: &mut R, head: Head) -> Head {
    match head {
        Head::Literal(literal) => Head::Literal(r.rewrite_literal(literal)),
        Head::Disjunction(disjunction) => {
            let elements: Vec<_> = disjunction
                .elements()
                .map(|element| rewrite_carrier(r, element, rewrite_disjunction_element))
                .collect();
            Head::Disjunction(Disjunction::from_nodes(elements))
        }
        Head::Choice(choice) => {
            let left = choice.left_guard().map(|guard| rewrite_guard(r, guard));
            let elements: Vec<_> = choice
                .elements()
                .map(|element| rewrite_carrier(r, element, rewrite_choice_element))
                .collect();
            let right = choice.right_guard().map(|guard| rewrite_guard(r, guard));
            Head::Choice(Choice::from_nodes(left, elements, right))
        }
        Head::Aggregate(aggregate) => {
            let left = aggregate.left_guard().map(|guard| rewrite_guard(r, guard));
            let elements: Vec<_> = aggregate
                .elements()
                .map(|element| rewrite_carrier(r, element, rewrite_head_aggregate_element))
                .collect();
            let right = aggregate.right_guard().map(|guard| rewrite_guard(r, guard));
            Head::Aggregate(HeadAggregate::from_nodes(
                left,
                aggregate.function(),
                elements,
                right,
            ))
        }
        Head::TheoryAtom(atom) => Head::TheoryAtom(r.rewrite_theory_atom(atom)),
        Head::Falsum => Head::Falsum,
        Head::Verum => Head::Verum,
    }
}

fn rebuild_body<R: Rewrite + ?Sized>(r: &mut R, body: Body) -> Body {
    let elements: Vec<_> = body
        .into_elements()
        .map(|element| rewrite_owned_carrier(r, element, rewrite_body_element))
        .collect();
    Body::from_nodes(elements)
}

fn rewrite_body_element<R: Rewrite + ?Sized>(r: &mut R, element: BodyElement) -> BodyElement {
    match element {
        BodyElement::Literal(literal) => BodyElement::Literal(r.rewrite_literal(literal)),
        BodyElement::Conditional(conditional) => {
            BodyElement::Conditional(rewrite_conditional(r, conditional))
        }
        BodyElement::Aggregate {
            negation,
            aggregate,
        } => BodyElement::Aggregate {
            negation,
            aggregate: r.rewrite_aggregate(aggregate),
        },
        BodyElement::TheoryAtom { negation, atom } => BodyElement::TheoryAtom {
            negation,
            atom: r.rewrite_theory_atom(atom),
        },
    }
}

fn rewrite_conditional<R: Rewrite + ?Sized>(
    r: &mut R,
    conditional: ConditionalLiteral,
) -> ConditionalLiteral {
    let literal = r.rewrite_literal(conditional.literal);
    let condition = r.rewrite_condition(conditional.condition);
    ConditionalLiteral { literal, condition }
}

fn rebuild_literal<R: Rewrite + ?Sized>(r: &mut R, literal: Literal) -> Literal {
    let inner =
        match literal.inner {
            LiteralInner::Atom(atom) => {
                LiteralInner::Atom(rewrite_owned_carrier(r, atom, Rewrite::rewrite_atom))
            }
            LiteralInner::Comparison(comparison) => LiteralInner::Comparison(
                rewrite_owned_carrier(r, comparison, Rewrite::rewrite_comparison),
            ),
            LiteralInner::True => LiteralInner::True,
            LiteralInner::False => LiteralInner::False,
        };
    Literal {
        negation: literal.negation,
        inner,
    }
}

fn rebuild_atom<R: Rewrite + ?Sized>(r: &mut R, atom: Atom) -> Atom {
    let arguments = atom
        .arguments
        .into_iter()
        .map(|term| rewrite_term(r, term))
        .collect();
    Atom {
        sign: atom.sign,
        name: atom.name,
        arguments,
    }
}

fn rebuild_comparison<R: Rewrite + ?Sized>(r: &mut R, comparison: Comparison) -> Comparison {
    let (first, steps) = comparison.into_parts();
    let mut steps = steps.into_iter();
    let (relation, second) = steps.next().expect("a comparison has at least one step");
    let first = rewrite_term(r, first);
    let second = rewrite_term(r, second);
    let mut result = Comparison::new(first, relation, second);
    for (relation, term) in steps {
        let term = rewrite_term(r, term);
        result = result.chain(relation, term);
    }
    result
}

fn rebuild_condition<R: Rewrite + ?Sized>(r: &mut R, condition: Condition) -> Condition {
    let literals: Vec<_> = condition
        .into_literals()
        .map(|literal| rewrite_owned_carrier(r, literal, Rewrite::rewrite_literal))
        .collect();
    Condition::from_nodes(literals)
}

fn rebuild_aggregate<R: Rewrite + ?Sized>(r: &mut R, aggregate: Aggregate) -> Aggregate {
    match aggregate {
        Aggregate::Function(function) => {
            let left = function.left_guard().map(|guard| rewrite_guard(r, guard));
            let elements: Vec<_> = function
                .elements()
                .map(|element| rewrite_carrier(r, element, rewrite_body_aggregate_element))
                .collect();
            let right = function.right_guard().map(|guard| rewrite_guard(r, guard));
            Aggregate::Function(FunctionAggregate::from_nodes(
                left,
                function.function(),
                elements,
                right,
            ))
        }
        Aggregate::Set(set) => {
            let left = set.left_guard().map(|guard| rewrite_guard(r, guard));
            let elements: Vec<_> = set
                .elements()
                .map(|element| rewrite_carrier(r, element, rewrite_set_element))
                .collect();
            let right = set.right_guard().map(|guard| rewrite_guard(r, guard));
            Aggregate::Set(SetAggregate::from_nodes(left, elements, right))
        }
    }
}

fn rebuild_theory_atom<R: Rewrite + ?Sized>(r: &mut R, atom: TheoryAtom) -> TheoryAtom {
    let (name, arguments, elements, guard) = atom.into_parts();
    let arguments: Vec<_> = arguments
        .into_iter()
        .map(|term| rewrite_term(r, term))
        .collect();
    let elements: Vec<_> = elements
        .map(|element| rewrite_owned_carrier(r, element, rewrite_theory_element))
        .collect();
    TheoryAtom::from_nodes(name, arguments, elements, guard)
}

fn rewrite_theory_element<R: Rewrite + ?Sized>(r: &mut R, element: TheoryElement) -> TheoryElement {
    // The theory terms ride through unchanged (§4.9); the element's ordinary condition is
    // rewritten.
    let (terms, condition) = element.into_parts();
    let condition = condition.map(|condition| r.rewrite_condition(condition));
    TheoryElement::new(terms, condition)
}

fn rewrite_disjunction_element<R: Rewrite + ?Sized>(
    r: &mut R,
    element: &DisjunctionElement,
) -> DisjunctionElement {
    let literal = r.rewrite_literal(element.literal().clone());
    let condition = r.rewrite_condition(element.condition().clone());
    DisjunctionElement::new(literal, condition)
}

fn rewrite_choice_element<R: Rewrite + ?Sized>(
    r: &mut R,
    element: &ChoiceElement,
) -> ChoiceElement {
    let literal = r.rewrite_literal(element.literal().clone());
    let condition = r.rewrite_condition(element.condition().clone());
    ChoiceElement::new(literal, condition)
}

fn rewrite_head_aggregate_element<R: Rewrite + ?Sized>(
    r: &mut R,
    element: &HeadAggregateElement,
) -> HeadAggregateElement {
    let terms: Vec<_> = element
        .terms()
        .map(|term| rewrite_term(r, term.clone()))
        .collect();
    let literal = r.rewrite_literal(element.literal().clone());
    let condition = r.rewrite_condition(element.condition().clone());
    HeadAggregateElement::new(terms, literal, condition)
}

fn rewrite_body_aggregate_element<R: Rewrite + ?Sized>(
    r: &mut R,
    element: &BodyAggregateElement,
) -> BodyAggregateElement {
    let terms: Vec<_> = element
        .terms()
        .map(|term| rewrite_term(r, term.clone()))
        .collect();
    let condition = r.rewrite_condition(element.condition().clone());
    BodyAggregateElement::new(terms, condition)
}

fn rewrite_set_element<R: Rewrite + ?Sized>(r: &mut R, element: &SetElement) -> SetElement {
    match element {
        SetElement::Literal(literal) => SetElement::Literal(r.rewrite_literal(literal.clone())),
        SetElement::ConditionalLiteral(conditional) => {
            SetElement::ConditionalLiteral(rewrite_conditional(r, conditional.clone()))
        }
    }
}

fn rewrite_guard<R: Rewrite + ?Sized>(
    r: &mut R,
    guard: &WithProvenance<Guard>,
) -> WithProvenance<Guard> {
    rewrite_carrier(r, guard, |r, guard| Guard {
        relation: guard.relation,
        term: rewrite_term(r, guard.term.clone()),
    })
}

fn rewrite_weight<R: Rewrite + ?Sized>(r: &mut R, current: &Weight) -> Weight {
    let term = rewrite_term(r, current.term().clone());
    let mut rebuilt = weight(term);
    if let Some(priority) = current.priority() {
        let priority = rewrite_term(r, priority.clone());
        rebuilt = rebuilt.at_priority(priority);
    }
    rebuilt
}

fn rebuild_weak_constraint<R: Rewrite + ?Sized>(
    r: &mut R,
    weak: &WeakConstraint,
) -> WeakConstraint {
    let body = rewrite_carrier(r, weak.body(), |r, body| r.rewrite_body(body.clone()));
    let weight = rewrite_weight(r, weak.weight());
    let terms: Vec<_> = weak
        .terms()
        .map(|term| rewrite_term(r, term.clone()))
        .collect();
    WeakConstraint::from_nodes(body, weight, terms)
}

fn rebuild_optimize<R: Rewrite + ?Sized>(r: &mut R, optimize: &Optimize) -> Optimize {
    let elements: Vec<_> = optimize
        .elements()
        .map(|element| rewrite_carrier(r, element, rewrite_optimize_element))
        .collect();
    Optimize::from_nodes(optimize.direction, elements)
}

fn rewrite_optimize_element<R: Rewrite + ?Sized>(
    r: &mut R,
    element: &OptimizeElement,
) -> OptimizeElement {
    let weight = rewrite_weight(r, element.weight());
    let terms: Vec<_> = element
        .terms()
        .map(|term| rewrite_term(r, term.clone()))
        .collect();
    let condition = r.rewrite_condition(element.condition().clone());
    OptimizeElement::new(weight, terms, condition)
}

fn rebuild_show<R: Rewrite + ?Sized>(r: &mut R, show: Show) -> Show {
    match show {
        Show::All => Show::All,
        Show::Signature(signature) => Show::Signature(signature),
        Show::Term(term) => Show::Term(rewrite_term(r, term)),
        Show::TermBody { term, body } => {
            let term = rewrite_term(r, term);
            let body = rewrite_owned_carrier(r, body, Rewrite::rewrite_body);
            Show::TermBody { term, body }
        }
    }
}

fn rebuild_project<R: Rewrite + ?Sized>(r: &mut R, project: Project) -> Project {
    match project {
        Project::Signature(signature) => Project::Signature(signature),
        Project::Atom { atom, body } => {
            let atom = rewrite_owned_carrier(r, atom, Rewrite::rewrite_atom);
            let body = rewrite_owned_carrier(r, body, Rewrite::rewrite_body);
            Project::Atom { atom, body }
        }
    }
}

fn rebuild_edge<R: Rewrite + ?Sized>(r: &mut R, edge: &Edge) -> Edge {
    let pairs: Vec<_> = edge
        .pairs()
        .map(|(from, to)| {
            let from = rewrite_term(r, from.clone());
            let to = rewrite_term(r, to.clone());
            (from, to)
        })
        .collect();
    let body = rewrite_carrier(r, edge.body(), |r, body| r.rewrite_body(body.clone()));
    Edge::from_nodes(pairs, body)
}

fn rebuild_heuristic<R: Rewrite + ?Sized>(r: &mut R, heuristic: &Heuristic) -> Heuristic {
    let atom = rewrite_carrier(r, heuristic.atom(), |r, atom| r.rewrite_atom(atom.clone()));
    let body = rewrite_carrier(r, heuristic.body(), |r, body| r.rewrite_body(body.clone()));
    let bias = rewrite_term(r, heuristic.bias().clone());
    let priority = heuristic
        .priority()
        .map(|priority| rewrite_term(r, priority.clone()));
    let modifier = rewrite_term(r, heuristic.modifier().clone());
    Heuristic::from_nodes(atom, body, bias, priority, modifier)
}

fn rebuild_external<R: Rewrite + ?Sized>(r: &mut R, external: &External) -> External {
    let atom = rewrite_carrier(r, external.atom(), |r, atom| r.rewrite_atom(atom.clone()));
    let body = rewrite_carrier(r, external.body(), |r, body| r.rewrite_body(body.clone()));
    let value = external.value().map(|value| rewrite_term(r, value.clone()));
    External::from_nodes(atom, body, value)
}

fn rebuild_const<R: Rewrite + ?Sized>(r: &mut R, constant: Const) -> Const {
    Const {
        name: constant.name,
        value: rewrite_term(r, constant.value),
        policy: constant.policy,
    }
}
