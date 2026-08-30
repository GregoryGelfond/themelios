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
    Aggregate, Arguments, Atom, Body, BodyAggregateElement, BodyElement, Choice, ChoiceElement,
    Comparison, Condition, ConditionalLiteral, Const, Disjunction, DisjunctionElement, Edge,
    External, FunctionAggregate, Guard, HasGuards, Head, HeadAggregate, HeadAggregateElement,
    Heuristic, Literal, LiteralInner, Optimize, OptimizeElement, Program, Project, Query, Relation,
    Rule, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryElement, WeakConstraint,
    Weight, weight,
};
use crate::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use crate::term::{Term, TermParts};

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
    for term in atom.argument_terms() {
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
    // Rewrite every term, preserving the `Single`/`Pooled` structure (§9) — a pool is
    // eliminated by `unpool`, never by a term rewrite.
    let arguments = match atom.arguments {
        Arguments::Single(terms) => Arguments::Single(
            terms
                .into_iter()
                .map(|term| rewrite_term(r, term))
                .collect(),
        ),
        Arguments::Pooled(alternatives) => Arguments::Pooled(
            alternatives
                .into_iter()
                .map(|tuple| {
                    tuple
                        .into_iter()
                        .map(|term| rewrite_term(r, term))
                        .collect()
                })
                .collect(),
        ),
    };
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

// ===== The unpool pass (§9) =====

/// Eliminate every pool, expanding each node at the level the grounder does so the
/// analysis and solve tiers meet a pool-free program — the grounder's unpool-before-
/// simplify order (`simplify`/`project`/`eval`/`match` run only after `unpool`). A pool in
/// a top-level literal expands into separate rules (a head × body cross-product,
/// `Statement::unpool` in `libgringo/src/input/statement.cc`); a pool in an element or a
/// condition expands within its container. Not a [`Rewrite`] (that is one statement → one);
/// the 1→N shape mirrors the grounder's own `Statement::unpool`, which returns a vector.
///
/// Every produced node carries `Origin::Transformed` unioned with its source origin (§6),
/// so a verdict on `p(a)` traces back to the source `p(a; b)`. Total; `O(output)` — a
/// cross-product of pools is exponential in the number of pooled positions, an output-size
/// fact (like [`substitute`](crate::unify), §9.2), not an algorithm defect.
pub fn unpool(program: &Program) -> Program {
    let tag = TransformTag::new("unpool");
    let mut statements = Vec::new();
    for carrier in program.statements() {
        let provenance = stamp(carrier.provenance().clone(), tag.clone());
        for statement in unpool_statement(carrier.get().clone()) {
            statements.push(WithProvenance::new(statement, provenance.clone()));
        }
    }
    Program::of(statements)
}

fn unpool_statement(statement: Statement) -> Vec<Statement> {
    match statement {
        Statement::Rule(rule) => unpool_rule(&rule)
            .into_iter()
            .map(Statement::Rule)
            .collect(),
        Statement::WeakConstraint(weak) => unpool_weak_constraint(&weak)
            .into_iter()
            .map(Statement::WeakConstraint)
            .collect(),
        Statement::Optimize(optimize) => vec![Statement::Optimize(unpool_optimize(&optimize))],
        Statement::Show(show) => unpool_show(&show)
            .into_iter()
            .map(Statement::Show)
            .collect(),
        Statement::Project(project) => unpool_project(&project)
            .into_iter()
            .map(Statement::Project)
            .collect(),
        Statement::Edge(edge) => unpool_edge(&edge)
            .into_iter()
            .map(Statement::Edge)
            .collect(),
        Statement::Heuristic(heuristic) => unpool_heuristic(&heuristic)
            .into_iter()
            .map(Statement::Heuristic)
            .collect(),
        Statement::External(external) => unpool_external(&external)
            .into_iter()
            .map(Statement::External)
            .collect(),
        Statement::Query(query) => unpool_query(&query)
            .into_iter()
            .map(Statement::Query)
            .collect(),
        // A `#const` pool is refused non-constant and carried (§4.8); a theory definition,
        // `#defined`, `#include`, and a script carry no unpoolable ordinary pool.
        other => vec![other],
    }
}

fn unpool_rule(rule: &Rule) -> Vec<Rule> {
    let heads = unpool_head(rule.head().get());
    let bodies = unpool_body(rule.body().get());
    let mut rules = Vec::with_capacity(heads.len().saturating_mul(bodies.len()));
    for body in &bodies {
        for head in &heads {
            rules.push(Rule::new(head.clone(), body.clone()));
        }
    }
    rules
}

fn unpool_head(head: &Head) -> Vec<Head> {
    match head {
        Head::Literal(literal) => unpool_literal(literal)
            .into_iter()
            .map(Head::Literal)
            .collect(),
        // A disjunction has no guards, so it never multiplies: its elements expand within.
        Head::Disjunction(disjunction) => vec![Head::Disjunction(unpool_disjunction(disjunction))],
        // A choice/head-aggregate's elements expand within; its guards multiply it (a
        // statement-level product).
        Head::Choice(choice) => unpool_choice(choice)
            .into_iter()
            .map(Head::Choice)
            .collect(),
        Head::Aggregate(aggregate) => unpool_head_aggregate(aggregate)
            .into_iter()
            .map(Head::Aggregate)
            .collect(),
        // A head theory atom is deferred (§7); Falsum/Verum carry no pool.
        Head::TheoryAtom(_) | Head::Falsum | Head::Verum => vec![head.clone()],
    }
}

fn unpool_body(body: &Body) -> Vec<Body> {
    // Each element yields alternatives (the outer list), each a conjunctive group of elements
    // (the inner list): a plain literal splits the rule (one element per alternative), a
    // conditional or aggregate expands within (one alternative, several conjunctive elements).
    let per_element: Vec<Vec<Vec<BodyElement>>> = body
        .elements()
        .map(|element| unpool_body_element(element.get()))
        .collect();
    cross_product(per_element)
        .into_iter()
        .map(|groups| Body::new(groups.into_iter().flatten()))
        .collect()
}

/// A body element's expansion as *alternatives* (outer) each a *conjunctive group* (inner):
/// a plain literal is several alternatives of one element each; a conditional or aggregate is
/// one alternative whose group holds its within-expanded elements.
fn unpool_body_element(element: &BodyElement) -> Vec<Vec<BodyElement>> {
    match element {
        BodyElement::Literal(literal) => unpool_literal(literal)
            .into_iter()
            .map(|literal| vec![BodyElement::Literal(literal)])
            .collect(),
        BodyElement::Conditional(conditional) => {
            // A pooled *condition* becomes several conditional literals, all conjunctive in
            // the one body (`r :- t : c(a; b).` is `r :- t : c(a), t : c(b).`). A pooled
            // *head literal* would need a disjunctive clause `(p(a); p(b)) : c` a single-literal
            // conditional cannot hold, so it is left pooled and the fully-unpooled gate fails
            // closed on it (a representation gap, recorded like theory §7).
            if literal_has_pool(&conditional.literal) {
                vec![vec![BodyElement::Conditional(conditional.clone())]]
            } else {
                let group = unpool_condition(&conditional.condition)
                    .into_iter()
                    .map(|condition| {
                        BodyElement::Conditional(ConditionalLiteral {
                            literal: conditional.literal.clone(),
                            condition,
                        })
                    })
                    .collect();
                vec![group]
            }
        }
        BodyElement::Aggregate {
            negation,
            aggregate,
        } => unpool_aggregate(aggregate)
            .into_iter()
            .map(|aggregate| {
                vec![BodyElement::Aggregate {
                    negation: *negation,
                    aggregate,
                }]
            })
            .collect(),
        // A body theory atom is deferred (§7); `BodyElement` is non_exhaustive.
        _ => vec![vec![element.clone()]],
    }
}

fn unpool_literal(literal: &Literal) -> Vec<Literal> {
    match &literal.inner {
        LiteralInner::Atom(atom) => unpool_atom(atom.get())
            .into_iter()
            .map(|atom| Literal {
                negation: literal.negation,
                inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
            })
            .collect(),
        LiteralInner::Comparison(comparison) => unpool_comparison(comparison.get())
            .into_iter()
            .map(|comparison| Literal {
                negation: literal.negation,
                inner: LiteralInner::Comparison(WithProvenance::constructed(comparison)),
            })
            .collect(),
        LiteralInner::True | LiteralInner::False => vec![literal.clone()],
    }
}

/// A pooled atom, or an atom over pooled term arguments, expands into the distinct atoms the
/// grounder unpools it into (§8): each argument-list alternative crossed with each term-pool
/// alternative in its tuple. A `Single` pool-free atom yields exactly itself.
fn unpool_atom(atom: &Atom) -> Vec<Atom> {
    let mut atoms = Vec::new();
    for tuple in atom.alternatives() {
        let per_position: Vec<Vec<Term>> =
            tuple.iter().map(|term| unpool_term(term.clone())).collect();
        for combo in cross_product(per_position) {
            atoms.push(Atom {
                sign: atom.sign,
                name: atom.name.clone(),
                arguments: Arguments::Single(combo),
            });
        }
    }
    atoms
}

/// A comparison expands the cross-product of its first term and each step's term, one
/// comparison per choice (`RelationLiteral::unpool`, `libgringo/src/input/literals.cc`).
fn unpool_comparison(comparison: &Comparison) -> Vec<Comparison> {
    let relations: Vec<Relation> = comparison.steps().map(|(relation, _)| relation).collect();
    let mut positions: Vec<Vec<Term>> = Vec::with_capacity(relations.len() + 1);
    positions.push(unpool_term(comparison.first().clone()));
    for (_, term) in comparison.steps() {
        positions.push(unpool_term(term.clone()));
    }
    cross_product(positions)
        .into_iter()
        .map(|combo| {
            let mut terms = combo.into_iter();
            let first = terms.next().expect("a first term");
            let mut steps = relations.iter();
            let mut comparison = Comparison::new(
                first,
                *steps.next().expect("a comparison has at least one step"),
                terms.next().expect("a term for the first step"),
            );
            for relation in steps {
                comparison = comparison.chain(*relation, terms.next().expect("a term per step"));
            }
            comparison
        })
        .collect()
}

/// A condition's pool-free images (§9): the cross-product of its literals' unpoolings, one
/// conjunction per choice — the within-container expansion of a pooled condition.
fn unpool_condition(condition: &Condition) -> Vec<Condition> {
    let per_literal: Vec<Vec<Literal>> = condition
        .literals()
        .map(|literal| unpool_literal(literal.get()))
        .collect();
    cross_product(per_literal)
        .into_iter()
        .map(Condition::new)
        .collect()
}

/// An optional guard's pool-free images: its bound term unpooled, the relation kept. A pooled
/// guard bound multiplies the aggregate (a statement-level product, `(1;2){p}` is two
/// aggregates), unlike an element pool.
fn unpool_optional_guard(guard: Option<&WithProvenance<Guard>>) -> Vec<Option<Guard>> {
    match guard {
        None => vec![None],
        Some(guard) => {
            let guard = guard.get();
            unpool_term(guard.term.clone())
                .into_iter()
                .map(|term| {
                    Some(Guard {
                        relation: guard.relation,
                        term,
                    })
                })
                .collect()
        }
    }
}

/// A body aggregate's pool-free images: its elements expand within, its guards multiply it.
fn unpool_aggregate(aggregate: &Aggregate) -> Vec<Aggregate> {
    match aggregate {
        Aggregate::Function(function) => unpool_function_aggregate(function)
            .into_iter()
            .map(Aggregate::Function)
            .collect(),
        Aggregate::Set(set) => unpool_set_aggregate(set)
            .into_iter()
            .map(Aggregate::Set)
            .collect(),
    }
}

fn unpool_function_aggregate(aggregate: &FunctionAggregate) -> Vec<FunctionAggregate> {
    let elements: Vec<BodyAggregateElement> = aggregate
        .elements()
        .flat_map(|element| unpool_body_aggregate_element(element.get()))
        .collect();
    let function = aggregate.function();
    let mut result = Vec::new();
    for left in unpool_optional_guard(aggregate.left_guard()) {
        for right in unpool_optional_guard(aggregate.right_guard()) {
            result.push(FunctionAggregate::new(
                left.clone(),
                function,
                elements.clone(),
                right.clone(),
            ));
        }
    }
    result
}

fn unpool_body_aggregate_element(element: &BodyAggregateElement) -> Vec<BodyAggregateElement> {
    let per_term: Vec<Vec<Term>> = element
        .terms()
        .map(|term| unpool_term(term.clone()))
        .collect();
    let conditions = unpool_condition(element.condition());
    let mut result = Vec::new();
    for terms in cross_product(per_term) {
        for condition in &conditions {
            result.push(BodyAggregateElement::new(terms.clone(), condition.clone()));
        }
    }
    result
}

fn unpool_set_aggregate(aggregate: &SetAggregate) -> Vec<SetAggregate> {
    let elements: Vec<SetElement> = aggregate
        .elements()
        .flat_map(|element| unpool_set_element(element.get()))
        .collect();
    let mut result = Vec::new();
    for left in unpool_optional_guard(aggregate.left_guard()) {
        for right in unpool_optional_guard(aggregate.right_guard()) {
            result.push(SetAggregate::new(
                left.clone(),
                elements.clone(),
                right.clone(),
            ));
        }
    }
    result
}

fn unpool_set_element(element: &SetElement) -> Vec<SetElement> {
    match element {
        SetElement::Literal(literal) => unpool_literal(literal)
            .into_iter()
            .map(SetElement::Literal)
            .collect(),
        SetElement::ConditionalLiteral(conditional) => unpool_conditional_literal(conditional)
            .into_iter()
            .map(SetElement::ConditionalLiteral)
            .collect(),
    }
}

/// A conditional literal's within-container expansion (§9): both a pooled derived literal and
/// a pooled condition become more elements of the one choice/disjunction/set — the container
/// holds many, so a pooled head is representable here (unlike a lone body conditional).
fn unpool_conditional_literal(conditional: &ConditionalLiteral) -> Vec<ConditionalLiteral> {
    let literals = unpool_literal(&conditional.literal);
    let conditions = unpool_condition(&conditional.condition);
    let mut result = Vec::new();
    for literal in &literals {
        for condition in &conditions {
            result.push(ConditionalLiteral {
                literal: literal.clone(),
                condition: condition.clone(),
            });
        }
    }
    result
}

fn unpool_choice(choice: &Choice) -> Vec<Choice> {
    let elements: Vec<ChoiceElement> = choice
        .elements()
        .flat_map(|element| unpool_choice_element(element.get()))
        .collect();
    let mut result = Vec::new();
    for left in unpool_optional_guard(choice.left_guard()) {
        for right in unpool_optional_guard(choice.right_guard()) {
            result.push(Choice::new(left.clone(), elements.clone(), right.clone()));
        }
    }
    result
}

fn unpool_choice_element(element: &ChoiceElement) -> Vec<ChoiceElement> {
    let literals = unpool_literal(element.literal());
    let conditions = unpool_condition(element.condition());
    let mut result = Vec::new();
    for literal in &literals {
        for condition in &conditions {
            result.push(ChoiceElement::new(literal.clone(), condition.clone()));
        }
    }
    result
}

fn unpool_disjunction(disjunction: &Disjunction) -> Disjunction {
    let elements: Vec<DisjunctionElement> = disjunction
        .elements()
        .flat_map(|element| unpool_disjunction_element(element.get()))
        .collect();
    Disjunction::new(elements)
}

fn unpool_disjunction_element(element: &DisjunctionElement) -> Vec<DisjunctionElement> {
    // A pooled disjunct literal becomes more disjuncts, a pooled condition more elements —
    // both within the one disjunction (`p(X; a) | q` is `p(X) | p(a) | q`).
    let literals = unpool_literal(element.literal());
    let conditions = unpool_condition(element.condition());
    let mut result = Vec::new();
    for literal in &literals {
        for condition in &conditions {
            result.push(DisjunctionElement::new(literal.clone(), condition.clone()));
        }
    }
    result
}

fn unpool_head_aggregate(aggregate: &HeadAggregate) -> Vec<HeadAggregate> {
    let elements: Vec<HeadAggregateElement> = aggregate
        .elements()
        .flat_map(|element| unpool_head_aggregate_element(element.get()))
        .collect();
    let function = aggregate.function();
    let mut result = Vec::new();
    for left in unpool_optional_guard(aggregate.left_guard()) {
        for right in unpool_optional_guard(aggregate.right_guard()) {
            result.push(HeadAggregate::new(
                left.clone(),
                function,
                elements.clone(),
                right.clone(),
            ));
        }
    }
    result
}

fn unpool_head_aggregate_element(element: &HeadAggregateElement) -> Vec<HeadAggregateElement> {
    let per_term: Vec<Vec<Term>> = element
        .terms()
        .map(|term| unpool_term(term.clone()))
        .collect();
    let literals = unpool_literal(element.literal());
    let conditions = unpool_condition(element.condition());
    let mut result = Vec::new();
    for terms in cross_product(per_term) {
        for literal in &literals {
            for condition in &conditions {
                result.push(HeadAggregateElement::new(
                    terms.clone(),
                    literal.clone(),
                    condition.clone(),
                ));
            }
        }
    }
    result
}

// ---- Directives, optimization, and the query (§9) ----
//
// A directive over pooled parts becomes several directives (a statement-level product over
// its atom, terms, and body, `ExternalHeadAtom::unpool` and its kin, `libgringo/src/input/
// aggregates.cc`); an optimize statement's elements expand within, like an aggregate's.

/// A `weight@priority`'s pool-free images: its weight and priority terms unpooled.
fn unpool_weight(w: &Weight) -> Vec<Weight> {
    let priorities: Vec<Option<Term>> = match w.priority() {
        None => vec![None],
        Some(priority) => unpool_term(priority.clone())
            .into_iter()
            .map(Some)
            .collect(),
    };
    let mut result = Vec::new();
    for term in unpool_term(w.term().clone()) {
        for priority in &priorities {
            result.push(match priority {
                None => weight(term.clone()),
                Some(priority) => weight(term.clone()).at_priority(priority.clone()),
            });
        }
    }
    result
}

fn unpool_optional_value(value: Option<&Term>) -> Vec<Option<Term>> {
    match value {
        None => vec![None],
        Some(value) => unpool_term(value.clone()).into_iter().map(Some).collect(),
    }
}

fn unpool_external(external: &External) -> Vec<External> {
    let bodies = unpool_body(external.body().get());
    let values = unpool_optional_value(external.value());
    let mut result = Vec::new();
    for atom in unpool_atom(external.atom().get()) {
        for body in &bodies {
            for value in &values {
                result.push(External::new(atom.clone(), body.clone(), value.clone()));
            }
        }
    }
    result
}

fn unpool_project(project: &Project) -> Vec<Project> {
    match project {
        Project::Signature(_) => vec![project.clone()],
        Project::Atom { atom, body } => {
            let bodies = unpool_body(body.get());
            let mut result = Vec::new();
            for atom in unpool_atom(atom.get()) {
                for body in &bodies {
                    result.push(Project::atom_body(atom.clone(), body.clone()));
                }
            }
            result
        }
    }
}

fn unpool_show(show: &Show) -> Vec<Show> {
    match show {
        Show::All | Show::Signature(_) => vec![show.clone()],
        Show::Term(term) => unpool_term(term.clone())
            .into_iter()
            .map(Show::Term)
            .collect(),
        Show::TermBody { term, body } => {
            let bodies = unpool_body(body.get());
            let mut result = Vec::new();
            for term in unpool_term(term.clone()) {
                for body in &bodies {
                    result.push(Show::TermBody {
                        term: term.clone(),
                        body: WithProvenance::constructed(body.clone()),
                    });
                }
            }
            result
        }
    }
}

fn unpool_edge(edge: &Edge) -> Vec<Edge> {
    let per_pair: Vec<Vec<(Term, Term)>> = edge
        .pairs()
        .map(|(from, to)| {
            let tos = unpool_term(to.clone());
            let mut pairs = Vec::new();
            for from in unpool_term(from.clone()) {
                for to in &tos {
                    pairs.push((from.clone(), to.clone()));
                }
            }
            pairs
        })
        .collect();
    let bodies = unpool_body(edge.body().get());
    let mut result = Vec::new();
    for pairs in cross_product(per_pair) {
        for body in &bodies {
            result.push(Edge::new(pairs.clone(), body.clone()));
        }
    }
    result
}

fn unpool_heuristic(heuristic: &Heuristic) -> Vec<Heuristic> {
    let bodies = unpool_body(heuristic.body().get());
    let biases = unpool_term(heuristic.bias().clone());
    let priorities = unpool_optional_value(heuristic.priority());
    let modifiers = unpool_term(heuristic.modifier().clone());
    let mut result = Vec::new();
    for atom in unpool_atom(heuristic.atom().get()) {
        for body in &bodies {
            for bias in &biases {
                for priority in &priorities {
                    for modifier in &modifiers {
                        result.push(Heuristic::new(
                            atom.clone(),
                            body.clone(),
                            bias.clone(),
                            priority.clone(),
                            modifier.clone(),
                        ));
                    }
                }
            }
        }
    }
    result
}

fn unpool_weak_constraint(weak: &WeakConstraint) -> Vec<WeakConstraint> {
    let weights = unpool_weight(weak.weight());
    let per_term: Vec<Vec<Term>> = weak.terms().map(|term| unpool_term(term.clone())).collect();
    let term_combos = cross_product(per_term);
    let mut result = Vec::new();
    for body in unpool_body(weak.body().get()) {
        for weight in &weights {
            for terms in &term_combos {
                result.push(WeakConstraint::new(
                    body.clone(),
                    weight.clone(),
                    terms.clone(),
                ));
            }
        }
    }
    result
}

fn unpool_optimize(optimize: &Optimize) -> Optimize {
    let elements: Vec<OptimizeElement> = optimize
        .elements()
        .flat_map(|element| unpool_optimize_element(element.get()))
        .collect();
    Optimize::new(optimize.direction, elements)
}

fn unpool_optimize_element(element: &OptimizeElement) -> Vec<OptimizeElement> {
    let weights = unpool_weight(element.weight());
    let per_term: Vec<Vec<Term>> = element
        .terms()
        .map(|term| unpool_term(term.clone()))
        .collect();
    let term_combos = cross_product(per_term);
    let conditions = unpool_condition(element.condition());
    let mut result = Vec::new();
    for weight in &weights {
        for terms in &term_combos {
            for condition in &conditions {
                result.push(OptimizeElement::new(
                    weight.clone(),
                    terms.clone(),
                    condition.clone(),
                ));
            }
        }
    }
    result
}

fn unpool_query(query: &Query) -> Vec<Query> {
    unpool_atom(query.atom().get())
        .into_iter()
        .map(Query::new)
        .collect()
}

/// Whether a literal still carries a pool — a pooled atom, or a term pool in an atom's
/// arguments or a comparison's terms. The lone-body-conditional gap (a pooled derived
/// literal) is left for the fully-unpooled gate, and this reports it.
fn literal_has_pool(literal: &Literal) -> bool {
    match &literal.inner {
        LiteralInner::Atom(atom) => atom_has_pool(atom.get()),
        LiteralInner::Comparison(comparison) => {
            let comparison = comparison.get();
            term_has_pool(comparison.first())
                || comparison.steps().any(|(_, term)| term_has_pool(term))
        }
        LiteralInner::True | LiteralInner::False => false,
    }
}

fn atom_has_pool(atom: &Atom) -> bool {
    atom.is_pooled() || atom.argument_terms().any(term_has_pool)
}

fn term_has_pool(term: &Term) -> bool {
    term.subterms().any(|node| matches!(node, Term::Pool(_)))
}

/// Every pool-free image of a term (§9): a `Term::Pool` becomes its alternatives, a compound
/// term the cross-product of its children's images — the grounder's term unpool
/// (`libgringo/src/term.cc`). Iterative in the term's depth (§13) through `Term::fold`.
///
/// A **pool-free** term is returned unchanged, without folding: the fold rebuilds every node, and
/// the cross-product would re-clone the growing subterm at each level of a deep chain — `O(depth²)`
/// on a term the analysis tier's deep-term scaling forbids (analysis §12.4). The `term_has_pool`
/// guard is one `O(depth)` iterative walk, so only a term that actually carries a pool is rebuilt.
fn unpool_term(term: Term) -> Vec<Term> {
    if !term_has_pool(&term) {
        return vec![term];
    }
    term.fold(|parts| match parts {
        TermParts::Variable(variable) => vec![Term::Variable(variable)],
        TermParts::Symbolic(symbol) => vec![Term::Symbolic(symbol)],
        TermParts::Pool(alternatives) => alternatives.into_iter().flatten().collect(),
        TermParts::Function { name, arguments } => cross_product(arguments)
            .into_iter()
            .map(|arguments| Term::Function {
                name: name.clone(),
                arguments,
            })
            .collect(),
        TermParts::Tuple(items) => cross_product(items).into_iter().map(Term::Tuple).collect(),
        TermParts::UnaryOperation { operator, argument } => argument
            .into_iter()
            .map(|argument| Term::UnaryOperation {
                operator,
                argument: Box::new(argument),
            })
            .collect(),
        TermParts::BinaryOperation {
            operator,
            left,
            right,
        } => cross_product(vec![left, right])
            .into_iter()
            .map(|pair| {
                let [left, right] = two(pair);
                Term::BinaryOperation {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                }
            })
            .collect(),
        TermParts::Interval { lower, upper } => cross_product(vec![lower, upper])
            .into_iter()
            .map(|pair| {
                let [lower, upper] = two(pair);
                Term::Interval {
                    lower: Box::new(lower),
                    upper: Box::new(upper),
                }
            })
            .collect(),
        TermParts::Absolute(inner) => inner
            .into_iter()
            .map(|inner| Term::Absolute(Box::new(inner)))
            .collect(),
        TermParts::External { name, arguments } => cross_product(arguments)
            .into_iter()
            .map(|arguments| Term::External {
                name: name.clone(),
                arguments,
            })
            .collect(),
    })
}

/// The cartesian product of a list of positions, each a list of alternatives: one output
/// tuple per choice of alternative at each position (an empty input yields one empty tuple).
/// `O(product)` — exponential in the number of pooled positions (§9), an output-size fact.
fn cross_product<T: Clone>(positions: Vec<Vec<T>>) -> Vec<Vec<T>> {
    let mut combos: Vec<Vec<T>> = vec![Vec::new()];
    for position in positions {
        match position.as_slice() {
            // No alternative: an empty pool would delete the statement. It is refused at the
            // door (§1.4), so this is defensive — fail closed to no combos.
            [] => return Vec::new(),
            // One alternative — the pool-free case: extend every combo **in place**, so an
            // N-position pool-free product (an N-literal body) is `O(N)`, not `O(N²)`. Cloning
            // the growing prefix at each of N positions is the quadratic the analysis tier's
            // linear scaling forbids (analysis §8); the in-place extend avoids it.
            [single] => {
                for combo in &mut combos {
                    combo.push(single.clone());
                }
            }
            // Two or more — a genuine pool: branch each combo. The clone is `O(output)` (§9),
            // an output-size cost, since the combos here truly diverge.
            alternatives => {
                let mut next = Vec::with_capacity(combos.len().saturating_mul(alternatives.len()));
                for combo in &combos {
                    for alternative in alternatives {
                        let mut extended = combo.clone();
                        extended.push(alternative.clone());
                        next.push(extended);
                    }
                }
                combos = next;
            }
        }
    }
    combos
}

/// The two elements of a two-position cross-product tuple, in order — the operand pair of
/// a binary operation or an interval (each has exactly two positions).
fn two<T>(mut pair: Vec<T>) -> [T; 2] {
    let second = pair.pop().expect("a two-position tuple has a second");
    let first = pair.pop().expect("a two-position tuple has a first");
    [first, second]
}
