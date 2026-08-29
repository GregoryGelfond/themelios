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

use themelios_program::analyze::{BodyCarrier, body_carrier, head_carrier_conditions};
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyElement, Comparison, Condition, DefaultNegation,
    Guard, HasGuards, Head, Literal, LiteralInner, Program, Relation, Rule, SetElement, Statement,
    TheoryAtom,
};
use themelios_program::provenance::{TransformTag, WithProvenance};
use themelios_program::symbol::{Signature, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::transform::Rewrite;

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
    ///
    /// `Holds` proves finiteness for the program's **safe** rules; at an untrusted
    /// boundary read it composed with [`is_safe`](Safety::is_safe), as `is_safe() &&
    /// Holds`. A rule this analysis reports unsafe is outside the proof: under a grounder
    /// whose `=` binds more than ASP-Core-2's — clingo decomposes `Z = (X, Y)` to bind
    /// `X`, `Y`, where the standard leaves them unbound (the same dialect gap safety
    /// already refuses) — such a rule can ground unboundedly, so trusting `Holds` for it
    /// alone would be unsound. The composed gate refuses it, being unsafe; that is the
    /// reading the boundary must use.
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
///
/// Each anonymous `_` is a **distinct** fresh variable, as the grounder reads it (program
/// `Rule::variables`), so the scan first renames every `_` to a distinct named variable and
/// then runs the ordinary binding fixpoint over it. This is exact where a single "anonymous"
/// marker cannot be: a `_` in a requiring position (a head, a negated literal, a non-lone `=`
/// side) is unbound, while `X = _` binds the `_` when `X` is bound — just like `X = Y` — rather
/// than being read as always-unbound. The witness collapses the minted names back to the
/// anonymous variable. (The finiteness carrier walk keeps the `_` as-is: a single-occurrence
/// anonymous cannot carry recursion between two positions, so renaming it there would be noise.)
fn unbound_variables(rule: &Rule) -> BTreeSet<Variable> {
    let mut freshener = FreshenAnonymous::over(rule);
    // The common rule has no `_`; only rebuild it when one is present (the rebuild is O(rule)).
    let renamed;
    let scanned: &Rule = if freshener.has_anonymous {
        renamed = freshener.rewrite_rule(rule.clone());
        &renamed
    } else {
        rule
    };

    let mut global = Scope::default();
    let mut locals: Vec<Scope> = Vec::new();
    collect_head(scanned.head().get(), &mut global, &mut locals);
    collect_body(scanned.body().get(), &mut global, &mut locals);

    let global_bound = close(global.binders.clone(), &global.assignments);
    let mut unbound: BTreeSet<Variable> =
        global.required.difference(&global_bound).cloned().collect();
    for local in &locals {
        // `global_bound` is the read-only base — consulted, never cloned per scope:
        // over many local scopes this keeps the rule `O(rule)`, not `O(scopes · global binders)`.
        let local_bound = close_within(&global_bound, local.binders.clone(), &local.assignments);
        for variable in &local.required {
            if !global.required.contains(variable)
                && !global_bound.contains(variable)
                && !local_bound.contains(variable)
            {
                unbound.insert(variable.clone());
            }
        }
    }
    freshener.collapse_witness(&mut unbound);
    unbound
}

/// Rename every anonymous `_` in a rule to a distinct fresh named variable, so the safety scan can
/// treat each as the distinct variable the grounder sees. The fresh names avoid the rule's own
/// named variables (so they cannot be spuriously bound by, or bind, a real one), and are recorded
/// to collapse the witness back to the anonymous variable. A `Rewrite` whose `rewrite_term` maps
/// each `_` leaf; the bottom-up fold reaches every occurrence and is stack-safe (program §3.6).
struct FreshenAnonymous {
    next: u32,
    used: BTreeSet<String>,
    minted: BTreeSet<Variable>,
    has_anonymous: bool,
    tag: TransformTag,
}

impl FreshenAnonymous {
    fn over(rule: &Rule) -> FreshenAnonymous {
        let mut used = BTreeSet::new();
        let mut has_anonymous = false;
        for variable in rule.variables() {
            match variable {
                Variable::Named(name) => {
                    used.insert(name.as_str().to_string());
                }
                Variable::Anonymous => has_anonymous = true,
            }
        }
        FreshenAnonymous {
            next: 0,
            used,
            minted: BTreeSet::new(),
            has_anonymous,
            tag: TransformTag::new("safety-freshen-anonymous"),
        }
    }

    fn fresh(&mut self) -> Variable {
        loop {
            let candidate = format!("Anonymous{}", self.next);
            self.next += 1;
            if self.used.contains(&candidate) {
                continue;
            }
            if let Ok(name) = VarName::new(candidate) {
                let variable = Variable::Named(name);
                self.minted.insert(variable.clone());
                return variable;
            }
        }
    }

    /// Replace the minted names in a witness with the single anonymous variable, so the reported
    /// unbound set names `_` rather than a synthetic identifier.
    fn collapse_witness(&self, unbound: &mut BTreeSet<Variable>) {
        let minted: Vec<Variable> = self
            .minted
            .iter()
            .filter(|variable| unbound.contains(variable))
            .cloned()
            .collect();
        if !minted.is_empty() {
            for variable in minted {
                unbound.remove(&variable);
            }
            unbound.insert(Variable::Anonymous);
        }
    }
}

impl Rewrite for FreshenAnonymous {
    fn tag(&self) -> TransformTag {
        self.tag.clone()
    }

    fn rewrite_term(&mut self, term: Term) -> Term {
        match term {
            Term::Variable(Variable::Anonymous) => Term::Variable(self.fresh()),
            other => other,
        }
    }
}

fn collect_head(head: &Head, global: &mut Scope, locals: &mut Vec<Scope>) {
    match head {
        Head::Literal(literal) => require_literal(global, literal),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                push_element_scope(element.get().literal(), element.get().condition(), locals);
            }
        }
        Head::Choice(choice) => {
            require_guard(choice.left_guard(), global);
            for element in choice.elements() {
                push_element_scope(element.get().literal(), element.get().condition(), locals);
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
/// scope. The theory-term algebra (program §4.9) is not descended, so a variable occurring
/// *only* in a theory term is invisible here: for safety that is **liberal**, not conservative
/// (`:- &t { X }.` reads safe though the grounder refuses `X`) — a characterized boundary the
/// grounder itself catches and the solve-tier differential records (§5, §10), not a hidden gap.
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

/// A disjunction/choice head element `l : condition` as one local scope: its literal's variables
/// are required *within the element* and bound by the element's own condition, exactly as an
/// aggregate element is (§5) — a choice/disjunction element variable is element-local, so a
/// condition-bound one (`{ p(X) : q(X) }`) is safe. A variable also global (bound by the rule body)
/// is still bound, since the local check consults `global_bound`.
fn push_element_scope(literal: &Literal, condition: &Condition, locals: &mut Vec<Scope>) {
    let mut scope = Scope::default();
    require_literal(&mut scope, literal);
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
/// assignments, an assignment firing when its right side is wholly bound. The global-scope
/// case of `close_within` with no read-only base, so the one worklist implementation cannot
/// drift into two; `O(assignments + variables)`, not the `O(variables²)` of a re-scan.
fn close(
    seed: BTreeSet<Variable>,
    assignments: &[(Variable, BTreeSet<Variable>)],
) -> BTreeSet<Variable> {
    close_within(&BTreeSet::new(), seed, assignments)
}

/// The binding fixpoint of an inner scope over a read-only `base` (the already-closed global
/// bound set): the same worklist as `close`, but a right-side variable already in `base` counts
/// as bound without `base` being copied into the working set. So a scope costs `O(local
/// assignments + local binders)`, and a rule with many local scopes stays `O(rule)` rather than
/// cloning the whole global set once per scope, `O(scopes · global binders)`. The
/// bound set it returns is the scope's *own* bindings; the caller reads a variable as bound iff
/// it is in `base` or in this result.
fn close_within(
    base: &BTreeSet<Variable>,
    seed: BTreeSet<Variable>,
    assignments: &[(Variable, BTreeSet<Variable>)],
) -> BTreeSet<Variable> {
    let mut bound = seed;
    let mut remaining: Vec<usize> = assignments
        .iter()
        .map(|(_, rhs)| {
            rhs.iter()
                .filter(|variable| !base.contains(variable) && !bound.contains(variable))
                .count()
        })
        .collect();
    let mut waiting: BTreeMap<Variable, Vec<usize>> = BTreeMap::new();
    for (index, (_, rhs)) in assignments.iter().enumerate() {
        for variable in rhs {
            if !base.contains(variable) && !bound.contains(variable) {
                waiting.entry(variable.clone()).or_default().push(index);
            }
        }
    }
    let mut ready: Vec<usize> = (0..assignments.len())
        .filter(|&i| remaining[i] == 0)
        .collect();
    while let Some(index) = ready.pop() {
        let lhs = assignments[index].0.clone();
        // A left side already in `base` is bound already; only a fresh local binding fires.
        if !base.contains(&lhs)
            && bound.insert(lhs.clone())
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

/// Grounding is proven finite (`Holds`) unless a recursive component's rules deepen a term on the
/// recursion — a head former over a carried variable (`q(f(Y)) :- q(Y)`), or a carried variable an
/// `=`-assignment deepens (`q(X) :- q(Y), X = f(Y)`), §5 — then `Unknown` carries that component.
/// Conservative: any recursive component with a growth witness is reported, never a false `Holds`.
///
/// Each rule is charged `O(rule)`: its growth-context (`BodyGrowth`) — the classes each recursive
/// component carries, the reverse `=`-deepening graph, and the `=`-aliases — is built once, and each
/// recursive head component's deepening set is a seed traversal computed once and reused across its
/// head atoms. `BodyGrowth` collects carriers and `=`-relations at every position that reaches the
/// head (body literals, aggregate guards, and each head-element condition — atoms *and* comparisons)
/// and soundly skips those that bind only element-local variables, so no variable the graph makes
/// recursive is a carrier the growth check misses (its congruence is stated on `BodyGrowth`).
fn finiteness_verdict(program: &Program, graph: &DependencyGraph) -> Verdict {
    for statement in program.statements() {
        if let Statement::Rule(rule) = statement.get()
            && let Some(component) = growing_component(rule, graph)
        {
            return Verdict::Unknown { witness: component };
        }
    }
    Verdict::Holds
}

/// The recursive component a rule grows, if any (§5): a head atom on a member of a recursive
/// component that deepens, under a term-former, a variable *carried* through that component's
/// recursion — written directly (`q(f(Y)) :- q(Y)`) or through a body `=`-assignment that makes a
/// variable *deepen* a carried one (`q(X) :- q(Y), X = f(Y)`), or a head-element condition (§5).
/// The rule's body growth-context is built once and each head atom is a lookup against its own
/// component, so the rule costs `O(rule)`.
///
/// The `O(rule)` rests on a lemma: **at most one component per rule has a nonempty carried set.**
/// Every carrier position is also a dependency edge from *every* head signature of the rule (the
/// congruence — see `BodyGrowth`), so two components each holding both a head atom and a carrier of
/// this rule would depend on each other and be one component, contradicting SCC maximality. A
/// component with an empty carried set costs `O(1)` (`reaching` of nothing), so the per-component
/// `reaching` traversals sum to `O(rule)`, not `O(head atoms · rule)`.
fn growing_component(rule: &Rule, graph: &DependencyGraph) -> Option<Component> {
    let context = BodyGrowth::of(rule, graph);
    // Each recursive component's deepening set — the roots that deepen one of its carried roots —
    // computed once (a seed traversal) and reused across the head's atoms.
    let mut deepening: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
    for atom in head_atoms(rule.head().get()) {
        let signature = atom_signature(atom);
        let Some(component) = graph.component_of(&signature) else {
            continue;
        };
        if !component.is_recursive() {
            continue;
        }
        // `decompose` gives every component at least one member (each pops its own root), so this
        // arm is unreachable; the guard keeps the read total. Were it ever reached, `continue`
        // skips the growth check — erring toward `Holds` — so it must stay unreachable (§6.1).
        let Some(key) = component.members().next() else {
            continue;
        };
        let empty = BTreeSet::new();
        let carried = context.component_roots.get(key).unwrap_or(&empty);
        let reaching = deepening
            .entry(key.clone())
            .or_insert_with(|| context.reaching(carried));
        if context.atom_deepens(atom, carried, reaching) {
            return Some(component.clone());
        }
    }
    None
}

/// A rule's growth-context, built once (§5), the sound over-approximation of §6.1:
/// - `component_roots` — the `=`-class roots each **recursive** component *carries*: the variables
///   of an atom the dependency graph reads a dependency from — a body literal, or a *head element's
///   condition* (`p(f(X)) : p(X)` carries `X`, matching `head_dependencies`, program §12.1) — plus a
///   lone-variable aggregate guard over a member. Only recursive components are kept.
/// - `deepens_into` — the reverse `=`-deepening graph: each `=`-class root mapped to the roots one
///   term-former *deeper* than it (`X = f(Y)` records `Y → X`). A component's deepening set — the
///   roots that deepen one of its carried roots — is the seed traversal of its carried roots through
///   this graph (`reaching`), so the growth an `=`-assignment carries to a bare head is caught.
/// - `equality_root` — each variable → its `=`-alias class's canonical least member, so carrying
///   and deepening are read up to aliasing.
///
/// Carriers and `=`-relations are collected by one literal walk (`collect_literal`) at each position
/// that reaches the head — body literals, aggregate guards, and every head-element condition (atoms
/// *and* comparisons). The positions the graph reads that bind only element-local variables — a body
/// conditional, an aggregate element's own condition, theory (§4.9) — reach no head atom and are
/// soundly skipped: a head-atom variable is global, hence bound and carried at top level. So a
/// variable the graph makes recursive is never a carrier the growth check misses (§6.1's `Holds`).
struct BodyGrowth {
    component_roots: BTreeMap<Signature, BTreeSet<Variable>>,
    deepens_into: BTreeMap<Variable, Vec<Variable>>,
    equality_root: BTreeMap<Variable, Variable>,
}

impl BodyGrowth {
    fn of(rule: &Rule, graph: &DependencyGraph) -> BodyGrowth {
        // Collect over the whole rule: the variables each signature carries, the `=`-alias groups
        // (`X = Y`), and the `=`-deepenings (`X = f(Y)`).
        let mut carriers: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
        let mut aliases: Vec<BTreeSet<Variable>> = Vec::new();
        let mut deepenings: Vec<(Variable, BTreeSet<Variable>)> = Vec::new();
        for element in rule.body().get().elements() {
            // Each body element's carrier role is classified exhaustively in the program tier
            // (`body_carrier`), so a new body kind is a compile error there — never a silent drop.
            match body_carrier(element.get()) {
                BodyCarrier::Literal(literal) => {
                    collect_literal(literal, &mut carriers, &mut aliases, &mut deepenings);
                }
                BodyCarrier::Aggregate(aggregate) => {
                    collect_aggregate_growth(aggregate, &mut carriers, &mut deepenings);
                }
                BodyCarrier::Inert => {}
            }
        }
        // Every head-element condition carries its variables to the derived literal
        // (`head_carrier_conditions`, exhaustive over `Head`); process each with the same literal
        // walk as the body — atoms *and* `=`-relations.
        for condition in head_carrier_conditions(rule.head().get()) {
            collect_condition(condition, &mut carriers, &mut aliases, &mut deepenings);
        }

        // Group the carried variables by their **recursive** component — each signature visited
        // once — so a head atom's carried set is a lookup. A non-recursive component is never read
        // (a head atom is checked only when recursive), so it is dropped here.
        let equality_root = equality_roots(&aliases);
        let mut component_roots: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
        for (signature, variables) in &carriers {
            let Some(component) = graph.component_of(signature) else {
                continue;
            };
            if !component.is_recursive() {
                continue;
            }
            // Unreachable: `decompose` gives every component a member (see `growing_component`).
            let Some(key) = component.members().next() else {
                continue;
            };
            let roots = component_roots.entry(key.clone()).or_default();
            for variable in variables {
                roots.insert(class_root(&equality_root, variable).clone());
            }
        }
        // The reverse deepening graph, keyed by `=`-class root: `X = f(…Y…)` records `Y → X`, so a
        // seed traversal from a component's carried roots (`reaching`) reaches every deepener.
        let mut deepens_into: BTreeMap<Variable, Vec<Variable>> = BTreeMap::new();
        for (deep, sources) in &deepenings {
            let deep_root = class_root(&equality_root, deep).clone();
            for source in sources {
                deepens_into
                    .entry(class_root(&equality_root, source).clone())
                    .or_default()
                    .push(deep_root.clone());
            }
        }
        BodyGrowth {
            component_roots,
            deepens_into,
            equality_root,
        }
    }

    /// The canonical (least) representative of a variable's `=`-alias class — the variable itself
    /// when it is in no alias.
    fn root<'a>(&'a self, variable: &'a Variable) -> &'a Variable {
        class_root(&self.equality_root, variable)
    }

    /// The `=`-class roots that *deepen* one of `carried_roots` — a seed traversal from the carried
    /// roots through the reverse deepening graph, on a heap stack (§8, spec §5.2). Computed once per
    /// recursive component the head derives into and reused across its head atoms, so the pass stays
    /// `O(program + edges)`; a component whose head never queries it pays nothing. Forward
    /// reachability from a fixed seed, so a cyclic assignment (`X = f(Y), Y = f(X)`) terminates on
    /// the `visited` set with no tentative-`false` hazard. Every node reached as the *target* of a
    /// deepening edge is a strict deepener of a carried root, so it is recorded — **including a
    /// carried root reached that way**: when the deepened value is itself a carrier (an aggregate
    /// guard, `M = #max { f(Y) : p(Y) }`, which both carries `p` and is `f`-deeper than its members),
    /// that carried root is genuine growth, and excluding it would be a false `Holds`.
    fn reaching(&self, carried_roots: &BTreeSet<Variable>) -> BTreeSet<Variable> {
        let mut deepening: BTreeSet<Variable> = BTreeSet::new();
        let mut visited: BTreeSet<Variable> = BTreeSet::new();
        let mut stack: Vec<Variable> = carried_roots.iter().cloned().collect();
        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            if let Some(deepeners) = self.deepens_into.get(&node) {
                for deepener in deepeners {
                    deepening.insert(deepener.clone());
                    stack.push(deepener.clone());
                }
            }
        }
        deepening
    }

    /// Whether a head atom deepens its component's recursion (§5): an argument that is a term-former
    /// over a *carried* variable (`q(f(Y)) :- q(Y)`), or that mentions — former or bare — a variable
    /// that *deepens* a carried one (`q(X) :- q(Y), X = f(Y)`). A bare carried variable does not
    /// deepen (`X = Y` is finite); a bare deepening one does.
    fn atom_deepens(
        &self,
        atom: &Atom,
        carried_roots: &BTreeSet<Variable>,
        deepening: &BTreeSet<Variable>,
    ) -> bool {
        atom.arguments.iter().any(|term| {
            let former = is_former(term);
            term.subterms().any(|subterm| {
                matches!(subterm, Term::Variable(variable) if {
                    let root = self.root(variable);
                    (former && carried_roots.contains(root)) || deepening.contains(root)
                })
            })
        })
    }
}

/// Classify each `=` step of a comparison as an **alias** (`X = Y`, both lone variables — one
/// class) or a **deepening** (`X = t` with `t` a term-former — `X` deepens `t`'s variables), the
/// two ways a body `=` bears on term growth. `X = c` (a constant) and
/// `f(..) = g(..)` (a constraint, no lone-variable side) bear on neither. A non-`=` step (`<`,
/// `!=`) contributes nothing.
fn collect_equalities(
    comparison: &Comparison,
    aliases: &mut Vec<BTreeSet<Variable>>,
    deepenings: &mut Vec<(Variable, BTreeSet<Variable>)>,
) {
    let mut operands: Vec<&Term> = vec![comparison.first()];
    let mut relations: Vec<Relation> = Vec::new();
    for (relation, term) in comparison.steps() {
        relations.push(relation);
        operands.push(term);
    }
    for (index, relation) in relations.iter().enumerate() {
        if *relation != Relation::Eq {
            continue;
        }
        let (left, right) = (operands[index], operands[index + 1]);
        match (as_lone_variable(left), as_lone_variable(right)) {
            // `X = Y` — one alias class.
            (Some(x), Some(y)) => aliases.push([x.clone(), y.clone()].into_iter().collect()),
            // `X = f(Y)` — X deepens f's variables, when the other side is a term-former.
            (Some(x), None) if is_former(right) => {
                let mut vars = BTreeSet::new();
                term_named_vars(right, &mut vars);
                deepenings.push((x.clone(), vars));
            }
            (None, Some(y)) if is_former(left) => {
                let mut vars = BTreeSet::new();
                term_named_vars(left, &mut vars);
                deepenings.push((y.clone(), vars));
            }
            _ => {}
        }
    }
}

/// The lone named variable a term *is*, if it is exactly one — the assignable side of an `=`.
fn as_lone_variable(term: &Term) -> Option<&Variable> {
    match term {
        Term::Variable(variable @ Variable::Named(_)) => Some(variable),
        _ => None,
    }
}

/// The `=`-equivalence classes of a rule's variables, each variable mapped to its class's
/// canonical (least) member — built once so a component's `=`-closure is a lookup, not a
/// per-component re-scan of the groups. A variable in no `=` comparison is its own class
/// (absent from the map). Union-find, near-linear in the equality content.
fn equality_roots(equalities: &[BTreeSet<Variable>]) -> BTreeMap<Variable, Variable> {
    // Index the variables of the `=` groups to `usize` for a cheap union-find.
    let mut index: BTreeMap<Variable, usize> = BTreeMap::new();
    let mut variables: Vec<Variable> = Vec::new();
    for group in equalities {
        for variable in group {
            index.entry(variable.clone()).or_insert_with(|| {
                variables.push(variable.clone());
                variables.len() - 1
            });
        }
    }
    let mut parent: Vec<usize> = (0..variables.len()).collect();
    for group in equalities {
        let mut members = group.iter();
        if let Some(first) = members.next() {
            let root = union_find(&mut parent, index[first]);
            for variable in members {
                let other = union_find(&mut parent, index[variable]);
                parent[other] = root;
            }
        }
    }
    // Each class's canonical member is its least, comparing the variables by value.
    let mut least: Vec<Option<usize>> = vec![None; variables.len()];
    for i in 0..variables.len() {
        let root = union_find(&mut parent, i);
        let better = match least[root] {
            None => true,
            Some(current) => variables[i] < variables[current],
        };
        if better {
            least[root] = Some(i);
        }
    }
    let mut roots = BTreeMap::new();
    for i in 0..variables.len() {
        let root = union_find(&mut parent, i);
        let canonical = least[root].expect("every class has a least member");
        roots.insert(variables[i].clone(), variables[canonical].clone());
    }
    roots
}

/// The representative of `index` in the union-find, with path halving.
fn union_find(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

/// The canonical (least) member of a variable's `=`-alias class — the variable itself when it is
/// in no alias.
fn class_root<'a>(
    equality_root: &'a BTreeMap<Variable, Variable>,
    variable: &'a Variable,
) -> &'a Variable {
    equality_root.get(variable).unwrap_or(variable)
}

/// Push an atom's argument variables as carriers of its signature — the growth check's read of
/// where a variable flows around the recursion, at the same term depth.
fn push_atom_carriers(atom: &Atom, carriers: &mut BTreeMap<Signature, BTreeSet<Variable>>) {
    let entry = carriers.entry(atom_signature(atom)).or_default();
    for term in &atom.arguments {
        term_named_vars(term, entry);
    }
}

/// Feed one literal to the growth-context (§5): an atom's variables carry — **regardless of
/// negation**, a conservative over-collection (a negated atom's variable can still be the carried one
/// a head deepens, and over-carrying only risks a spurious `Unknown`, never a false `Holds`); a
/// *positive* `=` comparison aliases (`X = Y`) or deepens (`X = f(Y)`), while a negated comparison is
/// a disequality that carries nothing. The single walk used for a body literal *and* every
/// head-element condition literal, so a comparison is never handled at one position and dropped at
/// another.
fn collect_literal(
    literal: &Literal,
    carriers: &mut BTreeMap<Signature, BTreeSet<Variable>>,
    aliases: &mut Vec<BTreeSet<Variable>>,
    deepenings: &mut Vec<(Variable, BTreeSet<Variable>)>,
) {
    match &literal.inner {
        LiteralInner::Atom(atom) => push_atom_carriers(atom.get(), carriers),
        LiteralInner::Comparison(comparison) if literal.negation == DefaultNegation::None => {
            collect_equalities(comparison.get(), aliases, deepenings);
        }
        _ => {}
    }
}

/// The literals of a condition, each fed to `collect_literal`.
fn collect_condition(
    condition: &Condition,
    carriers: &mut BTreeMap<Signature, BTreeSet<Variable>>,
    aliases: &mut Vec<BTreeSet<Variable>>,
    deepenings: &mut Vec<(Variable, BTreeSet<Variable>)>,
) {
    for literal in condition.literals() {
        collect_literal(literal.get(), carriers, aliases, deepenings);
    }
}

/// An aggregate's carriers and deepenings for growth (§5): its lone-variable guards carry to every
/// signature it ranges over (the `M` in `M = #max { Y : p(Y) }` — an existing member value). For a
/// `#max`/`#min` the value returned is a member's value-term, so a *former* value-term makes the
/// guard one former deeper than the members it ranges over (`M = #max { f(Y) : p(Y) }` is
/// `M = f(Y)`-deep): the aggregate is exactly the body `p(Y), M = f(Y)`, so its member variables are
/// registered as carriers of the signatures it ranges over — restoring the carried-member ≠
/// deepening-guard asymmetry the growth seed needs — and the guard **deepens** them. Without the
/// member as a carried root, the guard's deepening edge is dead (the seed from the guard's own root
/// never traverses it) and a `#max` over a former element term is a false `Holds`. `#count`/`#sum`/
/// `#sum+` return an integer, out of the term-depth scope (§5), so their element terms do not
/// deepen; a set aggregate has no value-term. A compound guard binds no variable and carries
/// nothing.
fn collect_aggregate_growth(
    aggregate: &Aggregate,
    carriers: &mut BTreeMap<Signature, BTreeSet<Variable>>,
    deepenings: &mut Vec<(Variable, BTreeSet<Variable>)>,
) {
    let mut guards = BTreeSet::new();
    collect_aggregate_guard_vars(aggregate, &mut guards);
    if guards.is_empty() {
        return;
    }
    let signatures = aggregate_signatures(aggregate);
    for signature in &signatures {
        carriers
            .entry(signature.clone())
            .or_default()
            .extend(guards.iter().cloned());
    }
    if is_extremum(aggregate) {
        for value in aggregate_value_terms(aggregate) {
            if !is_former(value) {
                continue;
            }
            let mut members = BTreeSet::new();
            term_named_vars(value, &mut members);
            if members.is_empty() {
                continue;
            }
            // The member variables the guard deepens must themselves be carried roots, or the
            // deepening edge is never traversed (the guard is not reached from its own root). The
            // aggregate ranges over these members, so they are carriers of its signatures.
            for signature in &signatures {
                carriers
                    .entry(signature.clone())
                    .or_default()
                    .extend(members.iter().cloned());
            }
            for guard in &guards {
                deepenings.push((guard.clone(), members.clone()));
            }
        }
    }
}

/// Whether the aggregate is `#max`/`#min` — the functions returning a member's value-*term* (not an
/// integer), so a former value-term is term-depth growth (§5). The match is exhaustive over
/// `AggregateFunction` (not a `matches!` with a silent fallthrough), so a new function forces a
/// term-vs-integer decision here rather than defaulting to a non-growth `false` — a false `Holds`.
fn is_extremum(aggregate: &Aggregate) -> bool {
    match aggregate {
        Aggregate::Function(function) => match function.function() {
            AggregateFunction::Max | AggregateFunction::Min => true,
            AggregateFunction::Count | AggregateFunction::Sum | AggregateFunction::SumPlus => false,
        },
        Aggregate::Set(_) => false,
    }
}

/// The value-terms an aggregate compares — the first term of each element; empty for a set
/// aggregate (whose elements are literals, not value tuples).
fn aggregate_value_terms(aggregate: &Aggregate) -> Vec<&Term> {
    match aggregate {
        Aggregate::Function(function) => function
            .elements()
            .filter_map(|element| element.get().terms().next())
            .collect(),
        Aggregate::Set(_) => Vec::new(),
    }
}

/// The predicate signatures an aggregate's elements range over — every signature whose
/// recursion the aggregate's guard variables therefore carry (§5).
fn aggregate_signatures(aggregate: &Aggregate) -> BTreeSet<Signature> {
    let mut signatures = BTreeSet::new();
    match aggregate {
        Aggregate::Function(function) => {
            for element in function.elements() {
                condition_signatures(element.get().condition(), &mut signatures);
            }
        }
        Aggregate::Set(set) => {
            for element in set.elements() {
                match element.get() {
                    SetElement::Literal(literal) => literal_signature(literal, &mut signatures),
                    SetElement::ConditionalLiteral(conditional) => {
                        literal_signature(&conditional.literal, &mut signatures);
                        condition_signatures(&conditional.condition, &mut signatures);
                    }
                }
            }
        }
    }
    signatures
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
        // Only a *lone-variable* guard binds a member value (`M = #max{…}`); a compound guard
        // (`f(X1,…,Xn) = #sum{…}`) constrains without binding the Xi, so it carries nothing —
        // bounding the guards to ≤ 2 keeps the aggregate carry `O(signatures)`.
        if let Some(variable) = as_lone_variable(&guard.get().term) {
            out.insert(variable.clone());
        }
    }
}

fn condition_signatures(condition: &Condition, out: &mut BTreeSet<Signature>) {
    for literal in condition.literals() {
        literal_signature(literal.get(), out);
    }
}

fn literal_signature(literal: &Literal, out: &mut BTreeSet<Signature>) {
    if let LiteralInner::Atom(atom) = &literal.inner {
        out.insert(atom_signature(atom.get()));
    }
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
