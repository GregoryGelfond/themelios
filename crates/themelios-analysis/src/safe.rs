//! Safety and grounding finiteness (docs/design/analysis.md §5): two facts about
//! whether a program can be ground, and ground finitely.
//!
//! **Safety is definite; finiteness is approximate.** Safety is the ASP-Core-2
//! standard's syntactic condition (grammar §3, §6) — a variable is safe when it has a
//! *binding* occurrence — so `Safety` reports it exactly, with the unsafe rule and its
//! unbound variables as the witness. Finiteness is undecidable with function symbols
//! (`p(f(X)) :- p(X)` grows terms without bound), so it is a sound approximation
//! returning the shared `Verdict` (§6.1): `Holds` where grounding is proven bounded,
//! `Unknown` carrying the recursive component through which terms grow — never
//! asserted infinite where it might be finite.

use std::collections::{BTreeMap, BTreeSet};

use themelios_program::program::{
    Aggregate, Atom, Body, BodyElement, Comparison, Condition, DefaultNegation, Guard, HasGuards,
    Head, Literal, LiteralInner, Program, Relation, Rule, SetElement, Statement, TheoryAtom,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::Signature;
use themelios_program::term::{Term, Variable};

use crate::classify::Verdict;
use crate::depend::{Component, DependencyGraph, atom_signature};

/// Two facts about grounding a program (§5): which rules are not safe, and whether
/// grounding is finite.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Safety {
    unsafe_rules: Vec<UnsafeRule>,
    finiteness: Verdict,
}

impl Safety {
    /// Read a program's safety and grounding finiteness (§5). `O(rules · variables +
    /// program + edges)`. Builds the dependency graph and delegates to `from_graph`;
    /// the assembled `Analysis` (§3) builds the graph once and shares it.
    pub fn of(program: &Program) -> Safety {
        Safety::from_graph(program, &DependencyGraph::of(program))
    }

    /// Read a program's safety against a dependency graph already built for it (§5):
    /// the door the assembled `Analysis` (§3) calls so the graph is built once and
    /// shared across the facets rather than rebuilt per facet. `Safety::of` is this
    /// with a freshly built graph; safety consults the graph only for `finiteness`,
    /// since `unsafe_rules` is a program walk.
    pub(crate) fn from_graph(program: &Program, graph: &DependencyGraph) -> Safety {
        let mut unsafe_rules = Vec::new();
        for statement in program.statements() {
            if let Statement::Rule(rule) = statement.get() {
                let unbound = unbound_variables(rule);
                if !unbound.is_empty() {
                    unsafe_rules.push(UnsafeRule {
                        rule: rule.clone(),
                        unbound,
                    });
                }
            }
        }
        let finiteness = finiteness_verdict(program, graph);
        Safety {
            unsafe_rules,
            finiteness,
        }
    }

    /// The rules that are not safe — empty when every rule is safe (§5). An unsafe rule
    /// cannot be grounded, so this is a well-formedness fact a grounder needs.
    pub fn unsafe_rules(&self) -> impl Iterator<Item = &UnsafeRule> {
        self.unsafe_rules.iter()
    }

    /// Whether every rule is safe (§5).
    pub fn is_safe(&self) -> bool {
        self.unsafe_rules.is_empty()
    }

    /// Whether grounding is finite — a sound approximation (§6.1): `Holds` (proven
    /// finite) or `Unknown` carrying the recursive component whose term growth blocked
    /// the proof. Never asserted infinite where it might be finite (§5).
    pub fn finiteness(&self) -> &Verdict {
        &self.finiteness
    }
}

/// An unsafe rule and why (§5): the rule by structural value (so it names a rule of
/// any program), and the variables with no binding occurrence — the witness, not a
/// bare "unsafe". A source span, when wanted, is read from the rule's own provenance
/// (program §6).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UnsafeRule {
    rule: Rule,
    unbound: BTreeSet<Variable>,
}

impl UnsafeRule {
    /// The unsafe rule, by structural value (§5).
    pub fn rule(&self) -> &Rule {
        &self.rule
    }

    /// The variables with no binding occurrence — the witness (§5).
    pub fn unbound(&self) -> impl Iterator<Item = &Variable> {
        self.unbound.iter()
    }
}

// ---- Safety: the ASP-Core-2 binding fixpoint (§5) ----
//
// A rule's variables are scoped: a **global** variable occurs at the rule's top level;
// a variable occurring only inside an aggregate element or a condition is **local** to
// that element. A global variable is bound by a positive top-level literal or a global
// assignment; a local variable is bound *within its element*, so a local binding never
// leaks to bind a global one. Only default-negation-free literals bind; a comparison
// binds only as a single-step `=` assignment with a lone-variable side, whose right
// side's variables must themselves be bound — the fixpoint.

/// A binding scope: the variables that must be bound, the variables a positive atom
/// binds directly, and the `X = t` assignments to close over.
#[derive(Default)]
struct Scope {
    required: BTreeSet<Variable>,
    binders: BTreeSet<Variable>,
    assignments: Vec<(Variable, BTreeSet<Variable>)>,
}

/// The variables of a rule with no binding occurrence in their scope — empty iff the
/// rule is safe (§5).
fn unbound_variables(rule: &Rule) -> BTreeSet<Variable> {
    let mut global = Scope::default();
    let mut locals: Vec<Scope> = Vec::new();
    collect_head(rule.head().get(), &mut global, &mut locals);
    collect_body(rule.body().get(), &mut global, &mut locals);

    let global_bound = close(global.binders.clone(), &global.assignments);
    let mut unbound: BTreeSet<Variable> =
        global.required.difference(&global_bound).cloned().collect();
    for local in &locals {
        let mut seed = global_bound.clone();
        seed.extend(local.binders.iter().cloned());
        let local_bound = close(seed, &local.assignments);
        for variable in &local.required {
            if !global.required.contains(variable) && !local_bound.contains(variable) {
                unbound.insert(variable.clone());
            }
        }
    }
    unbound
}

fn collect_head(head: &Head, global: &mut Scope, locals: &mut Vec<Scope>) {
    match head {
        Head::Literal(literal) => require_literal(global, literal),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                require_literal(global, element.get().literal());
                push_condition_scope(element.get().condition(), locals);
            }
        }
        Head::Choice(choice) => {
            require_guard(choice.left_guard(), global);
            for element in choice.elements() {
                require_literal(global, element.get().literal());
                push_condition_scope(element.get().condition(), locals);
            }
            require_guard(choice.right_guard(), global);
        }
        Head::Aggregate(aggregate) => {
            collect_guards(aggregate, global);
            for element in aggregate.elements() {
                let mut scope = Scope::default();
                for term in element.get().terms() {
                    term_named_vars(term, &mut scope.required);
                }
                require_literal(&mut scope, element.get().literal());
                process_condition(&mut scope, element.get().condition());
                locals.push(scope);
            }
        }
        Head::TheoryAtom(atom) => collect_theory_atom(atom, global, locals),
        Head::Falsum | Head::Verum => {}
    }
}

fn collect_body(body: &Body, global: &mut Scope, locals: &mut Vec<Scope>) {
    for element in body.elements() {
        match element.get() {
            BodyElement::Literal(literal) => bind_literal(global, literal),
            BodyElement::Conditional(conditional) => {
                let mut scope = Scope::default();
                require_literal(&mut scope, &conditional.literal);
                process_condition(&mut scope, &conditional.condition);
                locals.push(scope);
            }
            BodyElement::Aggregate { aggregate, .. } => {
                collect_aggregate(aggregate, global, locals);
            }
            BodyElement::TheoryAtom { atom, .. } => collect_theory_atom(atom, global, locals),
            // `BodyElement` is non-exhaustive; a future kind binds nothing until named.
            _ => {}
        }
    }
}

fn collect_aggregate(aggregate: &Aggregate, global: &mut Scope, locals: &mut Vec<Scope>) {
    match aggregate {
        Aggregate::Function(function) => {
            collect_guards(function, global);
            for element in function.elements() {
                let mut scope = Scope::default();
                for term in element.get().terms() {
                    term_named_vars(term, &mut scope.required);
                }
                process_condition(&mut scope, element.get().condition());
                locals.push(scope);
            }
        }
        Aggregate::Set(set) => {
            collect_guards(set, global);
            for element in set.elements() {
                let mut scope = Scope::default();
                match element.get() {
                    SetElement::Literal(literal) => bind_literal(&mut scope, literal),
                    SetElement::ConditionalLiteral(conditional) => {
                        bind_literal(&mut scope, &conditional.literal);
                        process_condition(&mut scope, &conditional.condition);
                    }
                }
                locals.push(scope);
            }
        }
    }
}

/// A theory atom's ordinary arguments are global; each element's condition is a local
/// scope. The theory-term algebra (program §4.9) is carried conservatively — its own
/// local variables are not analyzed here, a divergence the differential records (§10).
fn collect_theory_atom(atom: &TheoryAtom, global: &mut Scope, locals: &mut Vec<Scope>) {
    for term in atom.arguments() {
        term_named_vars(term, &mut global.required);
    }
    for element in atom.elements() {
        let mut scope = Scope::default();
        if let Some(condition) = element.get().condition() {
            process_condition(&mut scope, condition);
        }
        locals.push(scope);
    }
}

fn collect_guards(aggregate: &impl HasGuards, scope: &mut Scope) {
    require_guard(aggregate.left_guard(), scope);
    require_guard(aggregate.right_guard(), scope);
}

fn require_guard(guard: Option<&WithProvenance<Guard>>, scope: &mut Scope) {
    if let Some(guard) = guard {
        term_named_vars(&guard.get().term, &mut scope.required);
    }
}

fn push_condition_scope(condition: &Condition, locals: &mut Vec<Scope>) {
    let mut scope = Scope::default();
    process_condition(&mut scope, condition);
    locals.push(scope);
}

/// A binding literal (a body or condition literal): a positive standard atom binds its
/// variables; a positive `=` comparison is an assignment; every variable is required.
fn bind_literal(scope: &mut Scope, literal: &Literal) {
    match &literal.inner {
        LiteralInner::Atom(atom) => {
            let mut vars = BTreeSet::new();
            for term in &atom.get().arguments {
                term_named_vars(term, &mut vars);
            }
            scope.required.extend(vars.iter().cloned());
            if literal.negation == DefaultNegation::None {
                scope.binders.extend(vars);
            }
        }
        LiteralInner::Comparison(comparison) => {
            comparison_named_vars(comparison.get(), &mut scope.required);
            if literal.negation == DefaultNegation::None {
                assignments_of(comparison.get(), &mut scope.assignments);
            }
        }
        LiteralInner::True | LiteralInner::False => {}
    }
}

/// A requiring-only literal (a head atom): its variables must be bound, but a head
/// derives rather than binds.
fn require_literal(scope: &mut Scope, literal: &Literal) {
    match &literal.inner {
        LiteralInner::Atom(atom) => {
            for term in &atom.get().arguments {
                term_named_vars(term, &mut scope.required);
            }
        }
        LiteralInner::Comparison(comparison) => {
            comparison_named_vars(comparison.get(), &mut scope.required);
        }
        LiteralInner::True | LiteralInner::False => {}
    }
}

fn process_condition(scope: &mut Scope, condition: &Condition) {
    for literal in condition.literals() {
        bind_literal(scope, literal.get());
    }
}

fn assignments_of(comparison: &Comparison, out: &mut Vec<(Variable, BTreeSet<Variable>)>) {
    let steps: Vec<(Relation, &Term)> = comparison.steps().collect();
    if steps.len() != 1 || steps[0].0 != Relation::Eq {
        return;
    }
    add_assignment(comparison.first(), steps[0].1, out);
    add_assignment(steps[0].1, comparison.first(), out);
}

/// `lone = other` binds the named variable `lone` when `other`'s variables are bound
/// and `lone` does not occur in `other`.
fn add_assignment(lone: &Term, other: &Term, out: &mut Vec<(Variable, BTreeSet<Variable>)>) {
    if let Term::Variable(variable @ Variable::Named(_)) = lone {
        let mut other_vars = BTreeSet::new();
        term_named_vars(other, &mut other_vars);
        if !other_vars.contains(variable) {
            out.push((variable.clone(), other_vars));
        }
    }
}

fn comparison_named_vars(comparison: &Comparison, out: &mut BTreeSet<Variable>) {
    term_named_vars(comparison.first(), out);
    for (_, term) in comparison.steps() {
        term_named_vars(term, out);
    }
}

fn term_named_vars(term: &Term, out: &mut BTreeSet<Variable>) {
    for subterm in term.subterms() {
        if let Term::Variable(variable @ Variable::Named(_)) = subterm {
            out.insert(variable.clone());
        }
    }
}

/// The binding fixpoint (§5): close a seed set of bound variables under the
/// assignments, an assignment firing when its right side is wholly bound. A worklist
/// keyed on each assignment's remaining unbound right-side variables, so it is
/// `O(assignments + variables)` — not the `O(variables²)` of a re-scan.
fn close(
    seed: BTreeSet<Variable>,
    assignments: &[(Variable, BTreeSet<Variable>)],
) -> BTreeSet<Variable> {
    let mut bound = seed;
    let mut remaining: Vec<usize> = assignments
        .iter()
        .map(|(_, rhs)| {
            rhs.iter()
                .filter(|variable| !bound.contains(variable))
                .count()
        })
        .collect();
    let mut waiting: BTreeMap<Variable, Vec<usize>> = BTreeMap::new();
    for (index, (_, rhs)) in assignments.iter().enumerate() {
        for variable in rhs {
            if !bound.contains(variable) {
                waiting.entry(variable.clone()).or_default().push(index);
            }
        }
    }
    let mut ready: Vec<usize> = (0..assignments.len())
        .filter(|&i| remaining[i] == 0)
        .collect();
    while let Some(index) = ready.pop() {
        let lhs = assignments[index].0.clone();
        if bound.insert(lhs.clone())
            && let Some(dependents) = waiting.get(&lhs)
        {
            for &dependent in dependents {
                remaining[dependent] -= 1;
                if remaining[dependent] == 0 {
                    ready.push(dependent);
                }
            }
        }
    }
    bound
}

// ---- Finiteness: the sound growth approximation (§5, §6.1) ----

/// Grounding is proven finite (`Holds`) unless a recursive component's rules deepen a
/// term on the recursion — then `Unknown` carries that component (§5). Conservative:
/// any recursive component with a growth witness is reported, never a false `Holds`.
///
/// The rules that can grow a component are exactly those deriving one of its member
/// predicates, so a single pass indexes every rule under its head signatures and each
/// component reads only its own deriving rules. That keeps the whole pass
/// `O(program + edges)`: a per-component rescan of the program would be
/// `O(recursive-components · program)` — quadratic on a program of many small recursive
/// components, an adversary-controlled cost the design's committed linear class rules out.
fn finiteness_verdict(program: &Program, graph: &DependencyGraph) -> Verdict {
    let statements: Vec<&WithProvenance<Statement>> = program.statements().collect();
    // Head signature → the indices of the rules deriving it.
    let mut deriving: BTreeMap<Signature, Vec<usize>> = BTreeMap::new();
    for (index, statement) in statements.iter().enumerate() {
        if let Statement::Rule(rule) = statement.get() {
            for signature in rule.head_signatures() {
                deriving.entry(signature).or_default().push(index);
            }
        }
    }
    for component in graph.components() {
        if component.is_recursive() && component_grows(component, &statements, &deriving) {
            return Verdict::Unknown {
                witness: component.clone(),
            };
        }
    }
    Verdict::Holds
}

/// Whether a recursive component's rules introduce a deeper term on the recursion: a
/// rule deriving a component predicate whose head wraps, under a term-former, a variable
/// carried through the recursion (§5). Only the rules deriving a member predicate are
/// examined — read off the `deriving` index, each once — so the pass stays linear.
fn component_grows(
    component: &Component,
    statements: &[&WithProvenance<Statement>],
    deriving: &BTreeMap<Signature, Vec<usize>>,
) -> bool {
    let members: BTreeSet<Signature> = component.members().cloned().collect();
    let mut candidates: BTreeSet<usize> = BTreeSet::new();
    for member in &members {
        if let Some(indices) = deriving.get(member) {
            candidates.extend(indices);
        }
    }
    for &index in &candidates {
        let Statement::Rule(rule) = statements[index].get() else {
            continue;
        };
        let recursive_vars = recursive_body_vars(rule, &members);
        if head_deepens(rule.head().get(), &members, &recursive_vars) {
            return true;
        }
    }
    false
}

/// The named variables carried through the recursion — those that can hold a value drawn
/// from a recursive body occurrence of a component predicate and reach the head (§5).
/// Seeded from the arguments of every top-level body literal on a member predicate;
/// widened by an aggregate whose element ranges over a member predicate (its guard
/// variables carry the aggregate's value to the rule scope — the `X` in
/// `X = #max { Y : p(Y) }`); and closed over the rule's `=` comparisons (an equality
/// aliases a variable to whatever it equals — `Y = X` carries the recursion from `X` to
/// `Y`). A body conditional binds no rule-global variable and theory terms are carried
/// conservatively (§4.9), so neither adds a carrier here. The set is a sound
/// over-approximation: it only grows, so a value reaching the head deepened is never
/// missed — that miss would be the false `Holds` §6.1 rules out — while a variable
/// equated to a constant, or drawn from a non-recursive predicate, stays out.
fn recursive_body_vars(rule: &Rule, members: &BTreeSet<Signature>) -> BTreeSet<Variable> {
    let mut carried = BTreeSet::new();
    let mut equalities: Vec<BTreeSet<Variable>> = Vec::new();
    for element in rule.body().get().elements() {
        match element.get() {
            BodyElement::Literal(literal) => match &literal.inner {
                LiteralInner::Atom(atom) if members.contains(&atom_signature(atom.get())) => {
                    for term in &atom.get().arguments {
                        term_named_vars(term, &mut carried);
                    }
                }
                LiteralInner::Comparison(comparison) => {
                    collect_equalities(comparison.get(), &mut equalities);
                }
                // A non-member atom, and the boolean literals, carry nothing.
                _ => {}
            },
            BodyElement::Aggregate { aggregate, .. }
                if aggregate_ranges_over_member(aggregate, members) =>
            {
                collect_aggregate_guard_vars(aggregate, &mut carried);
            }
            // A conditional's variables are element-local; a theory term is carried
            // conservatively (§4.9); an aggregate over no member carries nothing.
            _ => {}
        }
    }
    close_over_equalities(&mut carried, &equalities);
    carried
}

/// The variable groups an `=` comparison equates: each `=` step contributes the named
/// variables of its two operands as one group, so `Y = X` groups `{X, Y}` and `X = t`
/// groups `X` with `t`'s variables. A non-equality step (`<`, `!=`) contributes nothing.
fn collect_equalities(comparison: &Comparison, out: &mut Vec<BTreeSet<Variable>>) {
    let mut operands: Vec<&Term> = vec![comparison.first()];
    let mut relations: Vec<Relation> = Vec::new();
    for (relation, term) in comparison.steps() {
        relations.push(relation);
        operands.push(term);
    }
    for (index, relation) in relations.iter().enumerate() {
        if *relation == Relation::Eq {
            let mut group = BTreeSet::new();
            term_named_vars(operands[index], &mut group);
            term_named_vars(operands[index + 1], &mut group);
            if !group.is_empty() {
                out.push(group);
            }
        }
    }
}

/// Close a carried-variable set over the `=` groups: a group any of whose variables is
/// already carried joins the set wholesale — the transitive `=`-alias closure. A group
/// touching nothing carried (an equality to a constant, or between non-recursive
/// variables) leaves the set unchanged, so the closure does not over-flag.
fn close_over_equalities(carried: &mut BTreeSet<Variable>, equalities: &[BTreeSet<Variable>]) {
    loop {
        let mut changed = false;
        for group in equalities {
            if group.iter().any(|variable| carried.contains(variable)) {
                for variable in group {
                    changed |= carried.insert(variable.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Whether any of an aggregate's elements ranges over a member predicate — a member atom
/// in an element's literal or condition, the occurrence that makes the aggregate carry
/// the recursion.
fn aggregate_ranges_over_member(aggregate: &Aggregate, members: &BTreeSet<Signature>) -> bool {
    match aggregate {
        Aggregate::Function(function) => function
            .elements()
            .any(|element| condition_mentions_member(element.get().condition(), members)),
        Aggregate::Set(set) => set.elements().any(|element| match element.get() {
            SetElement::Literal(literal) => literal_mentions_member(literal, members),
            SetElement::ConditionalLiteral(conditional) => {
                literal_mentions_member(&conditional.literal, members)
                    || condition_mentions_member(&conditional.condition, members)
            }
        }),
    }
}

/// The named variables of an aggregate's guards — the `X` in `X = #max { … }` and the
/// bound in `#count { … } >= N` — collected when the aggregate ranges over a member, so
/// the value it carries reaches the head's growth check.
fn collect_aggregate_guard_vars(aggregate: &Aggregate, out: &mut BTreeSet<Variable>) {
    match aggregate {
        Aggregate::Function(function) => guard_vars(function, out),
        Aggregate::Set(set) => guard_vars(set, out),
    }
}

fn guard_vars(aggregate: &impl HasGuards, out: &mut BTreeSet<Variable>) {
    for guard in [aggregate.left_guard(), aggregate.right_guard()]
        .into_iter()
        .flatten()
    {
        term_named_vars(&guard.get().term, out);
    }
}

fn condition_mentions_member(condition: &Condition, members: &BTreeSet<Signature>) -> bool {
    condition
        .literals()
        .any(|literal| literal_mentions_member(literal.get(), members))
}

fn literal_mentions_member(literal: &Literal, members: &BTreeSet<Signature>) -> bool {
    matches!(&literal.inner, LiteralInner::Atom(atom) if members.contains(&atom_signature(atom.get())))
}

fn head_deepens(
    head: &Head,
    members: &BTreeSet<Signature>,
    recursive_vars: &BTreeSet<Variable>,
) -> bool {
    head_atoms(head).into_iter().any(|atom| {
        members.contains(&atom_signature(atom))
            && atom
                .arguments
                .iter()
                .any(|term| term_deepens(term, recursive_vars))
    })
}

fn head_atoms(head: &Head) -> Vec<&Atom> {
    let mut atoms = Vec::new();
    match head {
        Head::Literal(literal) => push_atom(literal, &mut atoms),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                push_atom(element.get().literal(), &mut atoms);
            }
        }
        Head::Choice(choice) => {
            for element in choice.elements() {
                push_atom(element.get().literal(), &mut atoms);
            }
        }
        Head::Aggregate(aggregate) => {
            for element in aggregate.elements() {
                push_atom(element.get().literal(), &mut atoms);
            }
        }
        Head::TheoryAtom(_) | Head::Falsum | Head::Verum => {}
    }
    atoms
}

fn push_atom<'a>(literal: &'a Literal, atoms: &mut Vec<&'a Atom>) {
    if let LiteralInner::Atom(atom) = &literal.inner {
        atoms.push(atom.get());
    }
}

/// Whether a head argument deepens a recursive variable: it is a term-former (not a
/// bare variable or ground symbol) and contains a variable carried by the recursion.
fn term_deepens(term: &Term, recursive_vars: &BTreeSet<Variable>) -> bool {
    is_former(term)
        && term.subterms().any(|subterm| {
            matches!(subterm, Term::Variable(variable) if recursive_vars.contains(variable))
        })
}

fn is_former(term: &Term) -> bool {
    match term {
        Term::Function { arguments, .. } => !arguments.is_empty(),
        Term::Tuple(_)
        | Term::Pool(_)
        | Term::UnaryOperation { .. }
        | Term::BinaryOperation { .. }
        | Term::Interval { .. }
        | Term::Absolute(_)
        | Term::External { .. } => true,
        Term::Variable(_) | Term::Symbolic(_) => false,
    }
}
