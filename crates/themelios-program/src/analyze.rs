//! The structural-analysis substrate (docs/design/program.md §12.1): the pure
//! structural queries an analysis is written in — a rule's free variables, its
//! groundness, and the head and body predicate signatures a dependency graph is built
//! from — and the `DependencyKind` that tags a body dependency with the semantic mode it
//! runs through. None of these solves or grounds; they are a syntactic reading of the
//! assembled value (§12). The assembled reading is the companion `themelios-analysis`
//! crate, which reuses `DependencyKind`, `Rule`, and `Signature` rather than redefine
//! them (§12.2, analysis §4).
//!
//! The walks descend the grammar-bounded structural spine (a bounded recursion, §13) and
//! bottom out in `Term`'s and `TheoryTerm`'s iterative `subterms` (§3.6, §4.9). They read
//! through the public accessors — the substrate needs no privileged view of the value.

use std::collections::BTreeSet;

use crate::program::{
    Aggregate, Atom, Body, BodyElement, Choice, Comparison, Condition, ConditionalLiteral,
    DefaultNegation, Disjunction, Edge, FunctionAggregate, HasGuards, Head, HeadAggregate,
    Heuristic, Literal, LiteralInner, Optimize, Project, Rule, SetAggregate, SetElement, Show,
    Statement, TheoryAtom, TheoryTerm, WeakConstraint, Weight,
};
use crate::symbol::Signature;
use crate::term::{Term, Variable};

/// How a body predicate is depended on — the semantic mode a dependency graph reads
/// (analysis §4), defined here as its one authority. It is deliberately **not** the
/// syntactic [`DefaultNegation`] prefix (§4.5): that carries the negation *word*, while a
/// graph consumer needs the dependency *mode*, and the mapping also needs the enclosing
/// former (a plain literal, an aggregate, a theory atom), which the prefix does not carry.
/// The three modes are the honest KR distinctions — positive and negative dependency and
/// the non-monotone aggregate edge — and they are **not mutually exclusive**:
/// [`Rule::body_signatures`] yields one `(DependencyKind, Signature)` pair per mode an
/// occurrence carries, so a predicate reached inside a *negated* aggregate yields both
/// `ThroughAggregate` and `Negative` (§12.1, analysis §4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum DependencyKind {
    /// A positive body occurrence — no default negation, not through a non-monotone
    /// former: a monotone dependency, the edge the positive dependency graph keeps.
    Positive,
    /// Through default negation (`not`/`not not`) — the mode stratification reads; double
    /// negation is not monotone, so `NotNot` is `Negative` here too.
    Negative,
    /// Through a non-monotone aggregate or theory atom.
    ThroughAggregate,
}

impl Rule {
    /// The rule's free variables in first-occurrence order — the head before the body, and
    /// within a term its pre-order; across a body's elements, the body being a set (§4.5),
    /// their canonical order (§12.1). A **named** variable appears once however often it
    /// occurs; each **anonymous** `_` is a distinct fresh variable — as the grounder treats
    /// it (`p(X,_), q(_)` binds two independent values) — so every `_` occurrence is
    /// reported. A dependency-free reading of the value; O(nodes).
    pub fn variables(&self) -> impl Iterator<Item = &Variable> {
        let mut seen = BTreeSet::new();
        let mut ordered = Vec::new();
        for variable in self.variable_occurrences() {
            let fresh = match variable {
                Variable::Named(_) => seen.insert(variable),
                Variable::Anonymous => true,
            };
            if fresh {
                ordered.push(variable);
            }
        }
        ordered.into_iter()
    }

    /// Whether the rule is ground — no variable occurs in any term it carries, ordinary or
    /// theory (§12.1). O(nodes).
    pub fn is_ground(&self) -> bool {
        self.variable_occurrences().is_empty()
    }

    /// The signature of each atom the rule **derives** — the head literal's atom, each
    /// disjunction and choice element's atom, and a head aggregate's element atoms (§12.1).
    /// A comparison, a boolean, and a theory atom carry no predicate signature. The head
    /// nodes of the dependency graph (analysis §4). O(nodes).
    pub fn head_signatures(&self) -> impl Iterator<Item = Signature> {
        let mut signatures = Vec::new();
        head_signatures(self.head().get(), &mut signatures);
        signatures.into_iter()
    }

    /// Each predicate the rule **depends on**, its signature paired with the
    /// [`DependencyKind`] it runs through, **one pair per mode** an occurrence carries
    /// (§12.1): a plain literal is `Positive` or `Negative` by its default negation; a
    /// predicate reached through an aggregate or a theory atom is `ThroughAggregate`, and
    /// additionally `Negative` when that former — or the occurrence itself — is
    /// default-negated. Predicates *inside conditions* are reached wherever the condition
    /// sits: a body conditional, a body aggregate, **and a head disjunction/choice/aggregate
    /// element's condition** — a positive cycle through a head-element condition (`a : b.`
    /// with `b :- a.`) is a real dependency the grounder tracks, and missing it would be a
    /// false `Holds` for tightness, the over-claim analysis §6 forbids. The graph's edges
    /// with their kind. O(nodes).
    pub fn body_signatures(&self) -> impl Iterator<Item = (DependencyKind, Signature)> {
        let mut signatures = Vec::new();
        head_dependencies(self.head().get(), &mut signatures);
        body_dependencies(self.body().get(), &mut signatures);
        signatures.into_iter()
    }
}

impl Rule {
    /// Every variable occurrence in the rule, in document order (with repeats) — the raw
    /// list [`variables`](Rule::variables) dedups and [`is_ground`](Rule::is_ground) tests
    /// for emptiness.
    fn variable_occurrences(&self) -> Vec<&Variable> {
        let mut variables = Vec::new();
        head_variables(self.head().get(), &mut variables);
        body_variables(self.body().get(), &mut variables);
        variables
    }
}

/// Every variable occurrence in a **statement**, in document order (with repeats) — the
/// statement-level companion to [`Rule::variables`], sharing its walkers so the two cannot diverge.
/// A rule's occurrences are its head's and body's; a bodied directive's are its non-body term
/// positions' and its body's, and a body descends theory terms for their ordinary variable leaves
/// (§4.9), so a variable only in a directive-borne theory atom is not missed. The match is
/// **exhaustive over `Statement`** (no wildcard), so a new statement kind is a compile error here,
/// never silently unscanned — the one authority a client reading "which variable names does a
/// statement mention" consumes (the safety scan's anonymous freshener, analysis §5). Each `_` occurs
/// as a distinct `Anonymous` (as the grounder reads it); a named variable repeats per occurrence.
/// O(nodes).
pub fn statement_variables(statement: &Statement) -> Vec<&Variable> {
    let mut variables = Vec::new();
    match statement {
        Statement::Rule(rule) => {
            head_variables(rule.head().get(), &mut variables);
            body_variables(rule.body().get(), &mut variables);
        }
        Statement::WeakConstraint(weak) => {
            body_variables(weak.body().get(), &mut variables);
            push_weight_variables(weak.weight(), &mut variables);
            for term in weak.terms() {
                push_term_variables(term, &mut variables);
            }
        }
        Statement::Optimize(optimize) => {
            for element in optimize.elements() {
                let element = element.get();
                push_weight_variables(element.weight(), &mut variables);
                for term in element.terms() {
                    push_term_variables(term, &mut variables);
                }
                push_condition_variables(element.condition(), &mut variables);
            }
        }
        Statement::Show(show) => match show {
            Show::All | Show::Signature(_) => {}
            Show::Term(term) => push_term_variables(term, &mut variables),
            Show::TermBody { term, body } => {
                push_term_variables(term, &mut variables);
                body_variables(body.get(), &mut variables);
            }
        },
        Statement::Project(project) => match project {
            Project::Signature(_) => {}
            Project::Atom { atom, body } => {
                push_atom_variables(atom.get(), &mut variables);
                body_variables(body.get(), &mut variables);
            }
        },
        Statement::Edge(edge) => {
            for (from, to) in edge.pairs() {
                push_term_variables(from, &mut variables);
                push_term_variables(to, &mut variables);
            }
            body_variables(edge.body().get(), &mut variables);
        }
        Statement::Heuristic(heuristic) => {
            push_atom_variables(heuristic.atom().get(), &mut variables);
            body_variables(heuristic.body().get(), &mut variables);
            push_term_variables(heuristic.bias(), &mut variables);
            if let Some(priority) = heuristic.priority() {
                push_term_variables(priority, &mut variables);
            }
            push_term_variables(heuristic.modifier(), &mut variables);
        }
        Statement::External(external) => {
            push_atom_variables(external.atom().get(), &mut variables);
            body_variables(external.body().get(), &mut variables);
            if let Some(value) = external.value() {
                push_term_variables(value, &mut variables);
            }
        }
        Statement::Const(constant) => push_term_variables(&constant.value, &mut variables),
        Statement::Query(query) => push_atom_variables(query.atom().get(), &mut variables),
        // No ordinary variable: a signature, an include target, a script body, a theory definition
        // (§4.8, §4.9).
        Statement::Defined(_)
        | Statement::Include(_)
        | Statement::Script(_)
        | Statement::TheoryDefinition(_) => {}
    }
    variables
}

// ---- The signature of an atom ----

/// The signatures an atom heads or depends on: **one per argument-list alternative**
/// (§8), so a pooled `p(a; b, c)` contributes p/1 and p/2 — the distinct atoms the
/// grounder unpools it into. A `Single` atom yields exactly one. Never reads a single
/// alternative alone, so a pool is not silently truncated (§4.6).
fn alternative_signatures(atom: &Atom) -> impl Iterator<Item = Signature> + '_ {
    atom.alternatives().map(move |tuple| Signature {
        sign: atom.sign,
        name: atom.name.clone(),
        // A predicate carries no more arguments than a `Vec` holds, far under `u32::MAX`
        // (the workspace `cast_possible_truncation` allowance, argued in place).
        arity: tuple.len() as u32,
    })
}

// ---- Variable occurrences ----

fn push_term_variables<'a>(term: &'a Term, out: &mut Vec<&'a Variable>) {
    for subterm in term.subterms() {
        if let Term::Variable(variable) = subterm {
            out.push(variable);
        }
    }
}

fn push_theory_term_variables<'a>(theory_term: &'a TheoryTerm, out: &mut Vec<&'a Variable>) {
    for subterm in theory_term.subterms() {
        if let TheoryTerm::Variable(variable) = subterm {
            out.push(variable);
        }
    }
}

fn push_atom_variables<'a>(atom: &'a Atom, out: &mut Vec<&'a Variable>) {
    for term in atom.argument_terms() {
        push_term_variables(term, out);
    }
}

fn push_comparison_variables<'a>(comparison: &'a Comparison, out: &mut Vec<&'a Variable>) {
    push_term_variables(comparison.first(), out);
    for (_relation, term) in comparison.steps() {
        push_term_variables(term, out);
    }
}

fn push_literal_variables<'a>(literal: &'a Literal, out: &mut Vec<&'a Variable>) {
    match &literal.inner {
        LiteralInner::Atom(atom) => push_atom_variables(atom.get(), out),
        LiteralInner::Comparison(comparison) => push_comparison_variables(comparison.get(), out),
        LiteralInner::True | LiteralInner::False => {}
    }
}

fn push_condition_variables<'a>(condition: &'a Condition, out: &mut Vec<&'a Variable>) {
    for literal in condition.literals() {
        push_literal_variables(literal.get(), out);
    }
}

fn push_conditional_variables<'a>(
    conditional: &'a ConditionalLiteral,
    out: &mut Vec<&'a Variable>,
) {
    push_literal_variables(&conditional.literal, out);
    push_condition_variables(&conditional.condition, out);
}

fn push_guard_variables<'a>(aggregate: &'a impl HasGuards, out: &mut Vec<&'a Variable>) {
    if let Some(guard) = aggregate.left_guard() {
        push_term_variables(&guard.get().term, out);
    }
    if let Some(guard) = aggregate.right_guard() {
        push_term_variables(&guard.get().term, out);
    }
}

fn push_aggregate_variables<'a>(aggregate: &'a Aggregate, out: &mut Vec<&'a Variable>) {
    match aggregate {
        Aggregate::Function(function) => push_function_aggregate_variables(function, out),
        Aggregate::Set(set) => push_set_aggregate_variables(set, out),
    }
}

fn push_function_aggregate_variables<'a>(
    aggregate: &'a FunctionAggregate,
    out: &mut Vec<&'a Variable>,
) {
    push_guard_variables(aggregate, out);
    for element in aggregate.elements() {
        for term in element.get().terms() {
            push_term_variables(term, out);
        }
        push_condition_variables(element.get().condition(), out);
    }
}

fn push_set_aggregate_variables<'a>(aggregate: &'a SetAggregate, out: &mut Vec<&'a Variable>) {
    push_guard_variables(aggregate, out);
    for element in aggregate.elements() {
        match element.get() {
            SetElement::Literal(literal) => push_literal_variables(literal, out),
            SetElement::ConditionalLiteral(conditional) => {
                push_conditional_variables(conditional, out);
            }
        }
    }
}

fn push_head_aggregate_variables<'a>(aggregate: &'a HeadAggregate, out: &mut Vec<&'a Variable>) {
    push_guard_variables(aggregate, out);
    for element in aggregate.elements() {
        for term in element.get().terms() {
            push_term_variables(term, out);
        }
        push_literal_variables(element.get().literal(), out);
        push_condition_variables(element.get().condition(), out);
    }
}

fn push_theory_atom_variables<'a>(atom: &'a TheoryAtom, out: &mut Vec<&'a Variable>) {
    for term in atom.arguments() {
        push_term_variables(term, out);
    }
    for element in atom.elements() {
        for theory_term in element.get().terms() {
            push_theory_term_variables(theory_term, out);
        }
        if let Some(condition) = element.get().condition() {
            push_condition_variables(condition, out);
        }
    }
    if let Some(guard) = atom.guard() {
        push_theory_term_variables(&guard.term, out);
    }
}

fn push_weight_variables<'a>(weight: &'a Weight, out: &mut Vec<&'a Variable>) {
    push_term_variables(weight.term(), out);
    if let Some(priority) = weight.priority() {
        push_term_variables(priority, out);
    }
}

fn head_variables<'a>(head: &'a Head, out: &mut Vec<&'a Variable>) {
    match head {
        Head::Literal(literal) => push_literal_variables(literal, out),
        Head::Disjunction(disjunction) => push_disjunction_variables(disjunction, out),
        Head::Choice(choice) => push_choice_variables(choice, out),
        Head::Aggregate(aggregate) => push_head_aggregate_variables(aggregate, out),
        Head::TheoryAtom(atom) => push_theory_atom_variables(atom, out),
        Head::Falsum | Head::Verum => {}
    }
}

fn push_disjunction_variables<'a>(disjunction: &'a Disjunction, out: &mut Vec<&'a Variable>) {
    for element in disjunction.elements() {
        push_literal_variables(element.get().literal(), out);
        push_condition_variables(element.get().condition(), out);
    }
}

fn push_choice_variables<'a>(choice: &'a Choice, out: &mut Vec<&'a Variable>) {
    if let Some(guard) = choice.left_guard() {
        push_term_variables(&guard.get().term, out);
    }
    for element in choice.elements() {
        push_literal_variables(element.get().literal(), out);
        push_condition_variables(element.get().condition(), out);
    }
    if let Some(guard) = choice.right_guard() {
        push_term_variables(&guard.get().term, out);
    }
}

fn body_variables<'a>(body: &'a Body, out: &mut Vec<&'a Variable>) {
    for element in body.elements() {
        match element.get() {
            BodyElement::Literal(literal) => push_literal_variables(literal, out),
            BodyElement::Conditional(conditional) => push_conditional_variables(conditional, out),
            BodyElement::Aggregate { aggregate, .. } => push_aggregate_variables(aggregate, out),
            BodyElement::TheoryAtom { atom, .. } => push_theory_atom_variables(atom, out),
        }
    }
}

// ---- Head signatures ----

fn head_signatures(head: &Head, out: &mut Vec<Signature>) {
    match head {
        Head::Literal(literal) => push_literal_signature(literal, out),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                push_literal_signature(element.get().literal(), out);
            }
        }
        Head::Choice(choice) => {
            for element in choice.elements() {
                push_literal_signature(element.get().literal(), out);
            }
        }
        Head::Aggregate(aggregate) => {
            for element in aggregate.elements() {
                push_literal_signature(element.get().literal(), out);
            }
        }
        Head::TheoryAtom(_) | Head::Falsum | Head::Verum => {}
    }
}

fn push_literal_signature(literal: &Literal, out: &mut Vec<Signature>) {
    if let LiteralInner::Atom(atom) = &literal.inner {
        out.extend(alternative_signatures(atom.get()));
    }
}

// ---- Body signatures, kind-tagged ----

/// The former a body atom is reached through: a plain body or condition position, or an
/// aggregate/theory-atom former carrying whether *it* is default-negated. Set once as the
/// descent crosses a former, so a nested condition inherits it.
#[derive(Clone, Copy)]
enum Former {
    Plain,
    ThroughAggregate { negated: bool },
}

fn body_dependencies(body: &Body, out: &mut Vec<(DependencyKind, Signature)>) {
    for element in body.elements() {
        match element.get() {
            BodyElement::Literal(literal) => push_literal_dependency(literal, Former::Plain, out),
            BodyElement::Conditional(conditional) => {
                push_conditional_dependencies(conditional, Former::Plain, out);
            }
            BodyElement::Aggregate {
                negation,
                aggregate,
            } => {
                let former = Former::ThroughAggregate {
                    negated: is_negated(*negation),
                };
                aggregate_dependencies(aggregate, former, out);
            }
            BodyElement::TheoryAtom { negation, atom } => {
                let former = Former::ThroughAggregate {
                    negated: is_negated(*negation),
                };
                theory_atom_dependencies(atom, former, out);
            }
        }
    }
}

/// The dependencies a head carries — the predicates in its **element conditions** (§4.4).
/// A head element *derives* its atom ([`head_signatures`]) but its condition is a
/// dependency: `a : b.` derives `a` under the condition `b`, so `a` depends on `b`
/// (analysis §4). A disjunction/choice element's condition is a plain dependency; a head
/// aggregate's or head theory atom's element condition runs through that non-monotone
/// former, exactly as a body one does.
fn head_dependencies(head: &Head, out: &mut Vec<(DependencyKind, Signature)>) {
    match head {
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                push_condition_dependencies(element.get().condition(), Former::Plain, out);
            }
        }
        Head::Choice(choice) => {
            for element in choice.elements() {
                push_condition_dependencies(element.get().condition(), Former::Plain, out);
            }
        }
        Head::Aggregate(aggregate) => {
            let former = Former::ThroughAggregate { negated: false };
            for element in aggregate.elements() {
                push_condition_dependencies(element.get().condition(), former, out);
            }
        }
        Head::TheoryAtom(atom) => {
            theory_atom_dependencies(atom, Former::ThroughAggregate { negated: false }, out);
        }
        Head::Literal(_) | Head::Falsum | Head::Verum => {}
    }
}

fn is_negated(negation: DefaultNegation) -> bool {
    negation != DefaultNegation::None
}

fn push_condition_dependencies(
    condition: &Condition,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    for literal in condition.literals() {
        push_literal_dependency(literal.get(), former, out);
    }
}

fn push_literal_dependency(
    literal: &Literal,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    let LiteralInner::Atom(atom) = &literal.inner else {
        // A comparison or a boolean is not a predicate dependency.
        return;
    };
    let self_negated = is_negated(literal.negation);
    // One dependency per pooled alternative (§8): the grounder unpools a pooled atom into
    // distinct atoms, so each alternative's signature is its own edge.
    for signature in alternative_signatures(atom.get()) {
        match former {
            Former::Plain => {
                let kind = if self_negated {
                    DependencyKind::Negative
                } else {
                    DependencyKind::Positive
                };
                out.push((kind, signature));
            }
            Former::ThroughAggregate { negated } => {
                out.push((DependencyKind::ThroughAggregate, signature.clone()));
                if negated || self_negated {
                    out.push((DependencyKind::Negative, signature));
                }
            }
        }
    }
}

fn push_conditional_dependencies(
    conditional: &ConditionalLiteral,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    push_literal_dependency(&conditional.literal, former, out);
    push_condition_dependencies(&conditional.condition, former, out);
}

fn aggregate_dependencies(
    aggregate: &Aggregate,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    match aggregate {
        Aggregate::Function(function) => {
            for element in function.elements() {
                for literal in element.get().condition().literals() {
                    push_literal_dependency(literal.get(), former, out);
                }
            }
        }
        Aggregate::Set(set) => {
            for element in set.elements() {
                match element.get() {
                    SetElement::Literal(literal) => push_literal_dependency(literal, former, out),
                    SetElement::ConditionalLiteral(conditional) => {
                        push_conditional_dependencies(conditional, former, out);
                    }
                }
            }
        }
    }
}

fn theory_atom_dependencies(
    atom: &TheoryAtom,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    // A theory atom's ordinary predicate dependencies are the atoms of its elements'
    // conditions; the theory terms carry no ordinary predicate.
    for element in atom.elements() {
        if let Some(condition) = element.get().condition() {
            for literal in condition.literals() {
                push_literal_dependency(literal.get(), former, out);
            }
        }
    }
}

// ---- Growth-carrier positions (analysis §5): the finiteness congruence, compiler-checked ----
//
// The grounding-finiteness reading (analysis §5) must collect a growth carrier at every position
// the dependency graph reads a dependency to a head atom, or it can report a false `Holds`. The
// classification lives *here*, in the crate that owns the AST, so the `match`es are **exhaustive** —
// a new `BodyElement` or `Head` kind is a compile error here and cannot be silently dropped by the
// (downstream, non-exhaustive-blocked) growth walk in `themelios-analysis`.

/// How a body element carries variables toward a head atom for the growth reading — classified
/// exhaustively over `BodyElement`, congruent with the positions `body_dependencies` reads.
pub enum BodyCarrier<'a> {
    /// A plain literal — an atom (carrier) or a comparison (`=`-relation).
    Literal(&'a Literal),
    /// An aggregate — its guards, and (for `#max`/`#min`) its element value terms, carry; the
    /// analysis reads them.
    Aggregate(&'a Aggregate),
    /// A conditional or theory atom — binds only element-local variables (§4.9); nothing reaches a
    /// head atom (a head variable is global).
    Inert,
}

/// Classify a body element's growth-carrying role (analysis §5), exhaustively.
pub fn body_carrier(element: &BodyElement) -> BodyCarrier<'_> {
    match element {
        BodyElement::Literal(literal) => BodyCarrier::Literal(literal),
        BodyElement::Aggregate { aggregate, .. } => BodyCarrier::Aggregate(aggregate),
        BodyElement::Conditional(_) | BodyElement::TheoryAtom { .. } => BodyCarrier::Inert,
    }
}

/// The element **conditions** of a head, whose variables reach the derived element literal
/// (`head_dependencies` reads them as dependencies, §4.4) — classified exhaustively over `Head`,
/// so a new head kind bearing a condition cannot be silently dropped by the growth walk.
pub fn head_carrier_conditions(head: &Head) -> Vec<&Condition> {
    match head {
        Head::Disjunction(disjunction) => disjunction
            .elements()
            .map(|element| element.get().condition())
            .collect(),
        Head::Choice(choice) => choice
            .elements()
            .map(|element| element.get().condition())
            .collect(),
        Head::Aggregate(aggregate) => aggregate
            .elements()
            .map(|element| element.get().condition())
            .collect(),
        // A plain head literal has no condition; a head theory atom is the §4.9 boundary and derives
        // no ordinary head atom (never a growth target).
        Head::Literal(_) | Head::TheoryAtom(_) | Head::Falsum | Head::Verum => Vec::new(),
    }
}

// ---- Binding-role positions (analysis §5): the safety congruence, compiler-checked ----
//
// Safety (analysis §5) collects binding and requiring occurrences over a program's statements. Like
// the growth-carrier classification above, the per-kind match lives *here*, in the crate that owns the
// AST, so it is **exhaustive** — a new `BodyElement` or `Statement` kind is a compile error here and
// cannot be silently dropped by the (downstream, non-exhaustive-blocked) safety walk in
// `themelios-analysis`. Each classifier returns a **closed** enum the analysis crate matches without a
// wildcard, so the closure of the guarantee crosses the crate boundary.

/// How a body element binds and requires variables for the safety reading — classified exhaustively
/// over `BodyElement`, so a new kind is a compile error here rather than a silent fail-open in the
/// safety walk. A closed mirror of the binding-relevant `BodyElement` payloads (distinct from
/// [`BodyCarrier`], which collapses the non-carrying kinds to `Inert`; safety scopes all four).
pub enum BodyBinder<'a> {
    /// A plain literal — a positive atom or an `=`-assignment binds; every variable is required.
    Literal(&'a Literal),
    /// A conditional literal — an element-local scope: the literal is required, its condition binds.
    Conditional(&'a ConditionalLiteral),
    /// An aggregate — its guards are required globally, its elements are element-local scopes.
    Aggregate(&'a Aggregate),
    /// A theory atom — its ordinary and theory-term arguments are required, its elements' conditions
    /// bind locally (§4.9).
    TheoryAtom(&'a TheoryAtom),
}

/// Classify a body element's binding role for safety (analysis §5), exhaustively over `BodyElement`.
pub fn body_binder(element: &BodyElement) -> BodyBinder<'_> {
    match element {
        BodyElement::Literal(literal) => BodyBinder::Literal(literal),
        BodyElement::Conditional(conditional) => BodyBinder::Conditional(conditional),
        BodyElement::Aggregate { aggregate, .. } => BodyBinder::Aggregate(aggregate),
        BodyElement::TheoryAtom { atom, .. } => BodyBinder::TheoryAtom(atom),
    }
}

/// How a statement binds and requires variables for the safety reading — classified exhaustively over
/// `Statement`, so a new statement kind is a compile error here rather than a silent fail-open in the
/// safety walk. Safety scopes every statement that can bind a variable: a derivation rule (head
/// requires, body binds), and the bodied directives whose non-body term positions a body binds. A
/// statement admitting no variable — a signature, an include, a query (grammar §6.1) — carries none.
pub enum StatementBinder<'a> {
    /// A derivation rule — the head requires, the body binds (analysis §5).
    Rule(&'a Rule),
    /// A bodied directive — the `required` term positions must be bound by `body` when it has one. A
    /// body-less form (`#show t.`, an empty `conditional-dot ::= "."`, grammar §5.9) binds nothing, so a
    /// required term with a variable is then unsafe. Covers weak constraints, `#show t : body` and the
    /// body-less `#show t.`, `#project a : body`, `#edge : body`, and `#external : body`.
    BodiedDirective {
        /// The term positions whose variables the body must bind.
        required: Vec<&'a Term>,
        /// The body that binds them, if the form has one (`#show t.` has none).
        body: Option<&'a Body>,
    },
    /// A directive whose **atom** domain-matches — its variables bind in their invertible positions, as
    /// the grounder reads them (`#project`, `#heuristic`, grammar §5.9) — plus `required` bracket terms
    /// the atom or body must bind, and a body that also binds. A `#project` has no bracket terms; a
    /// `#heuristic`'s are its bias, optional priority, and modifier. So `#project p(X).` and
    /// `#heuristic p(X). [X@1, true]` are safe (the atom binds `X`), while `#project p(X/2).` and
    /// `#heuristic p(a). [W@1, true]` are not.
    DomainAtom {
        /// The atom whose variables domain-match (binding themselves and the required terms).
        atom: &'a Atom,
        /// Extra term positions the atom or body must bind — a `#heuristic`'s bracket (bias, optional
        /// priority, modifier); empty for `#project`.
        required: Vec<&'a Term>,
        /// The body, which also binds.
        body: &'a Body,
    },
    /// An optimize statement — each element is aggregate-element-like: its weight and terms are
    /// required and its own condition binds them (`#minimize`/`#maximize`, grammar §5.7).
    OptimizeElements(&'a Optimize),
    /// A statement that binds and requires no variable: a signature (`#defined`, `#show p/1`,
    /// `#project p/1`), an include, a script, or a theory definition (the grammar admits no variable);
    /// a query, whose variables are answer variables, not a grounder obligation (grammar §6.1); or a
    /// `#const`, whose value is variable-free by the constant-term subset (a constructed out-of-subset
    /// one is a different facet's well-formedness question, not safety's).
    NoObligation,
}

/// Classify a statement's binding role for safety (analysis §5), exhaustively over `Statement`.
pub fn statement_binder(statement: &Statement) -> StatementBinder<'_> {
    match statement {
        Statement::Rule(rule) => StatementBinder::Rule(rule),
        Statement::WeakConstraint(weak) => StatementBinder::BodiedDirective {
            required: weak_constraint_required(weak),
            body: Some(weak.body().get()),
        },
        Statement::Optimize(optimize) => StatementBinder::OptimizeElements(optimize),
        Statement::Show(Show::TermBody { term, body }) => StatementBinder::BodiedDirective {
            required: vec![term],
            body: Some(body.get()),
        },
        // `#show t.` (no body) requires the shown term's variables with nothing to bind them, so a
        // variable in it is unsafe — clingo flags `#show f(X).`.
        Statement::Show(Show::Term(term)) => StatementBinder::BodiedDirective {
            required: vec![term],
            body: None,
        },
        // A `#project` atom domain-matches — its variables bind in their invertible positions (the
        // grounder reads `#project p(X).` safe, `#project p(X/2).` unsafe), and the body's own variables
        // must be safe too (`#project p(X) : Z > 1.` unsafe on `Z`). Distinct from `#edge` and
        // `#external`, whose terms the grounder requires ground.
        Statement::Project(Project::Atom { atom, body }) => StatementBinder::DomainAtom {
            atom: atom.get(),
            required: Vec::new(),
            body: body.get(),
        },
        Statement::Edge(edge) => StatementBinder::BodiedDirective {
            required: edge_required(edge),
            body: Some(edge.body().get()),
        },
        Statement::Heuristic(heuristic) => StatementBinder::DomainAtom {
            atom: heuristic.atom().get(),
            required: heuristic_brackets(heuristic),
            body: heuristic.body().get(),
        },
        // A `#external a : body. [value]` requires both the atom's arguments and the truth-value term
        // (gringo's `ExternalHeadAtom::collect` collects both with `bound=false`, `input/aggregates.cc`),
        // bound by the body — so `#external p(X) : q(X). [W]` is unsafe on `W`.
        Statement::External(external) => StatementBinder::BodiedDirective {
            required: external
                .atom()
                .get()
                .argument_terms()
                .chain(external.value())
                .collect(),
            body: Some(external.body().get()),
        },
        // No variable-binding obligation: the grammar admits no variable in these positions (signatures,
        // include target, script text, theory definition, `#const`'s constant-term-subset value), or the
        // variables are answer variables (a query, grammar §6.1).
        Statement::Show(Show::All | Show::Signature(_))
        | Statement::Project(Project::Signature(_))
        | Statement::Defined(_)
        | Statement::Const(_)
        | Statement::Include(_)
        | Statement::Script(_)
        | Statement::TheoryDefinition(_)
        | Statement::Query(_) => StatementBinder::NoObligation,
    }
}

/// The term positions a weak constraint requires bound — its weight, optional priority, and tuple
/// terms (grammar §5.7); its body binds them.
fn weak_constraint_required(weak: &WeakConstraint) -> Vec<&Term> {
    let mut required = vec![weak.weight().term()];
    required.extend(weak.weight().priority());
    required.extend(weak.terms());
    required
}

/// The node-pair terms an `#edge` requires bound (grammar §5.9); its body binds them.
fn edge_required(edge: &Edge) -> Vec<&Term> {
    edge.pairs().flat_map(|(from, to)| [from, to]).collect()
}

/// The bracket terms of a `#heuristic` — its bias, optional priority, and modifier (grammar §5.9).
/// They are required, and are bound by the atom's domain match (its variables) or the body: so
/// `#heuristic p(X). [X@1, true]` is safe (the atom binds the bracket `X`) but
/// `#heuristic p(a). [W@1, true]` is not (its bias `W` is bound by neither).
fn heuristic_brackets(heuristic: &Heuristic) -> Vec<&Term> {
    let mut brackets = vec![heuristic.bias()];
    brackets.extend(heuristic.priority());
    brackets.push(heuristic.modifier());
    brackets
}
