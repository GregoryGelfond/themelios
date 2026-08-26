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
    DefaultNegation, Disjunction, FunctionAggregate, HasGuards, Head, HeadAggregate, Literal,
    LiteralInner, Rule, SetAggregate, SetElement, TheoryAtom, TheoryTerm,
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
    /// The rule's free variables, each once, in first-occurrence document order (§12.1) —
    /// the head's terms before the body's, structurally within each. `_` reads as the one
    /// anonymous variable. A dependency-free reading of the value; O(nodes).
    pub fn variables(&self) -> impl Iterator<Item = &Variable> {
        let mut seen = BTreeSet::new();
        let mut ordered = Vec::new();
        for variable in self.variable_occurrences() {
            if seen.insert(variable) {
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

    /// Each body dependency's signature paired with the [`DependencyKind`] it runs through,
    /// **one pair per mode** an occurrence carries (§12.1): a plain literal is `Positive`
    /// or `Negative` by its default negation; a predicate reached through an aggregate or a
    /// theory atom is `ThroughAggregate`, and additionally `Negative` when that former —
    /// or the occurrence itself — is default-negated; predicates *inside conditions* are
    /// reached, the tightness soundness resting on it (analysis §4). The graph's edges with
    /// their kind. O(nodes).
    pub fn body_signatures(&self) -> impl Iterator<Item = (DependencyKind, Signature)> {
        let mut signatures = Vec::new();
        body_signatures(self.body().get(), &mut signatures);
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

// ---- The signature of an atom ----

fn atom_signature(atom: &Atom) -> Signature {
    Signature {
        sign: atom.sign,
        name: atom.name.clone(),
        // A predicate carries no more arguments than a `Vec` holds, far under `u32::MAX`
        // (the workspace `cast_possible_truncation` allowance, argued in place).
        arity: atom.arguments.len() as u32,
    }
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
    for term in &atom.arguments {
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
        push_term_variables(&guard.term, out);
    }
    if let Some(guard) = aggregate.right_guard() {
        push_term_variables(&guard.term, out);
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
        push_term_variables(&guard.term, out);
    }
    for element in choice.elements() {
        push_literal_variables(element.get().literal(), out);
        push_condition_variables(element.get().condition(), out);
    }
    if let Some(guard) = choice.right_guard() {
        push_term_variables(&guard.term, out);
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
        out.push(atom_signature(atom.get()));
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

fn body_signatures(body: &Body, out: &mut Vec<(DependencyKind, Signature)>) {
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

fn is_negated(negation: DefaultNegation) -> bool {
    negation != DefaultNegation::None
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
    let signature = atom_signature(atom.get());
    let self_negated = is_negated(literal.negation);
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

fn push_conditional_dependencies(
    conditional: &ConditionalLiteral,
    former: Former,
    out: &mut Vec<(DependencyKind, Signature)>,
) {
    push_literal_dependency(&conditional.literal, former, out);
    for literal in conditional.condition.literals() {
        push_literal_dependency(literal.get(), former, out);
    }
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
