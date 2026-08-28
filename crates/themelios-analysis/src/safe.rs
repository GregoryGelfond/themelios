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
        // `global_bound` is the read-only base — consulted, never cloned per scope (Finding 3):
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

/// The binding fixpoint of an inner scope over a read-only `base` (the already-closed global
/// bound set): the same worklist as `close`, but a right-side variable already in `base` counts
/// as bound without `base` being copied into the working set. So a scope costs `O(local
/// assignments + local binders)`, and a rule with many local scopes stays `O(rule)` rather than
/// cloning the whole global set once per scope, `O(scopes · global binders)` (Finding 3). The
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

/// Grounding is proven finite (`Holds`) unless a recursive component's rules deepen a
/// term on the recursion — a head former over a carried variable, or a carried variable a
/// body `=`-assignment deepens (§5, the re-derivation's P1–P7) — then `Unknown` carries that
/// component. Conservative: any recursive component with a growth witness is reported, never a
/// false `Holds`.
///
/// Each rule is charged `O(rule)`: its body growth-context — the classes each recursive
/// component carries, the classes that deepen them, and the `=`-aliases — is built once
/// (`BodyGrowth`), and each head atom is a lookup against its own component. Grouping *by
/// component off the body* — never re-deriving by scanning a component's member list per head
/// atom — keeps the pass `O(program + edges)`: a rule deriving into a large component, or many
/// rules deriving into one, would otherwise reread that component's members and turn quadratic,
/// an adversary-controlled cost the committed class rules out.
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
/// variable *deepen* a carried one (`q(X) :- q(Y), X = f(Y)`; the soundness re-derivation's P1–P7).
/// The rule's body growth-context is built once and each head atom is a lookup against its own
/// component, so the rule costs `O(rule)`.
fn growing_component(rule: &Rule, graph: &DependencyGraph) -> Option<Component> {
    let context = BodyGrowth::of(rule, graph);
    for atom in head_atoms(rule.head().get()) {
        let signature = atom_signature(atom);
        let Some(component) = graph.component_of(&signature) else {
            continue;
        };
        if !component.is_recursive() {
            continue;
        }
        let Some(key) = component.members().next() else {
            continue;
        };
        let empty = BTreeSet::new();
        let carried = context.component_roots.get(key).unwrap_or(&empty);
        let deepening = context.deepening_roots.get(key).unwrap_or(&empty);
        if context.atom_deepens(atom, carried, deepening) {
            return Some(component.clone());
        }
    }
    None
}

/// A rule's body growth-context, built once (§5), the sound over-approximation of §6.1: three
/// facts, keyed by a component's least member, so a head atom is a lookup on its component.
/// - `component_roots` — the `=`-class roots *carried* through the component's recursion: the
///   variables of a body atom on a component member, plus a lone-variable aggregate guard over
///   such a member (the `M` in `M = #max { Y : p(Y) }`). These flow around the recursion at the
///   same term depth.
/// - `deepening_roots` — the `=`-class roots that *deepen* a carried one through a body
///   `=`-assignment: `X` in `X = f(Y)` when `Y` is carried (transitively along a chain), so a
///   head using `X` — even bare — grows the component. This closes the P3/P4 hole (the
///   re-derivation): `X = f(Y)` deepens, where the aliasing `X = Y` does not.
/// - `equality_root` — each variable → its `=`-alias class's canonical least member (`X = Y`),
///   so carrying and deepening are read up to aliasing.
///
/// A value reaching the head deepened is never missed (no false `Holds`); the approximation errs
/// only toward `Unknown` (§6.1).
struct BodyGrowth {
    component_roots: BTreeMap<Signature, BTreeSet<Variable>>,
    deepening_roots: BTreeMap<Signature, BTreeSet<Variable>>,
    equality_root: BTreeMap<Variable, Variable>,
}

impl BodyGrowth {
    fn of(rule: &Rule, graph: &DependencyGraph) -> BodyGrowth {
        // Walk the body once: the variables each signature carries (a body atom's arguments, plus
        // a lone-variable aggregate guard over the signature), the `=`-alias groups (`X = Y`), and
        // the `=`-deepenings (`X = f(Y)`: X deepens f's variables).
        let mut carriers: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
        let mut aliases: Vec<BTreeSet<Variable>> = Vec::new();
        let mut deepenings: Vec<(Variable, BTreeSet<Variable>)> = Vec::new();
        for element in rule.body().get().elements() {
            match element.get() {
                BodyElement::Literal(literal) => match &literal.inner {
                    LiteralInner::Atom(atom) => {
                        let entry = carriers.entry(atom_signature(atom.get())).or_default();
                        for term in &atom.get().arguments {
                            term_named_vars(term, entry);
                        }
                    }
                    // A positive `=` comparison aliases (`X = Y`) or deepens (`X = f(Y)`); a
                    // *negated* one is a disequality (`not X = Y` asserts X ≠ Y) — neither.
                    LiteralInner::Comparison(comparison)
                        if literal.negation == DefaultNegation::None =>
                    {
                        collect_equalities(comparison.get(), &mut aliases, &mut deepenings);
                    }
                    _ => {}
                },
                // An aggregate carries its *lone-variable* guards to every signature it ranges
                // over (the `M` in `M = #max { Y : p(Y) }` is an existing member value). Only lone
                // variables: a compound guard `f(X1,…,Xn) = #sum{…}` constrains, it does not bind
                // the Xi, so bounding the guards to ≤ 2 keeps the carry `O(signatures)` (a
                // conditional binds no rule-global variable; theory is carried conservatively,
                // §4.9 — neither adds a carrier).
                BodyElement::Aggregate { aggregate, .. } => {
                    let mut guards = BTreeSet::new();
                    collect_aggregate_guard_vars(aggregate, &mut guards);
                    if !guards.is_empty() {
                        for signature in aggregate_signatures(aggregate) {
                            carriers
                                .entry(signature)
                                .or_default()
                                .extend(guards.iter().cloned());
                        }
                    }
                }
                _ => {}
            }
        }
        // Group the carried variables by their component, off the body — each body signature
        // visited once — so a head atom's carried set is a lookup on its component, not a scan.
        let equality_root = equality_roots(&aliases);
        let mut component_roots: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
        for (signature, variables) in &carriers {
            let Some(component) = graph.component_of(signature) else {
                continue;
            };
            let Some(key) = component.members().next() else {
                continue;
            };
            let roots = component_roots.entry(key.clone()).or_default();
            for variable in variables {
                roots.insert(class_root(&equality_root, variable).clone());
            }
        }
        // Propagate deepening up the `=`-assignment chains: a variable that deepens a carried
        // class is itself deepening-for-that-component (P3/P4), a bare head use of which grows it.
        let deepening_roots = deepen_closure(&deepenings, &equality_root, &component_roots);
        BodyGrowth {
            component_roots,
            deepening_roots,
            equality_root,
        }
    }

    /// The canonical (least) representative of a variable's `=`-alias class — the variable itself
    /// when it is in no alias.
    fn root<'a>(&'a self, variable: &'a Variable) -> &'a Variable {
        class_root(&self.equality_root, variable)
    }

    /// Whether a head atom deepens the component's recursion (§5): an argument that is a
    /// term-former over a *carried* variable's `=`-class (`q(f(Y)) :- q(Y)`), or that mentions —
    /// former or bare — a variable whose class *deepens* a carried one through a body
    /// `=`-assignment (`q(X) :- q(Y), X = f(Y)`; the re-derivation's P1–P7). A bare carried
    /// variable does not deepen (`q(X) :- q(Y), X = Y` is finite); a bare *deepening* one does.
    fn atom_deepens(
        &self,
        atom: &Atom,
        carried_roots: &BTreeSet<Variable>,
        deepening_roots: &BTreeSet<Variable>,
    ) -> bool {
        atom.arguments.iter().any(|term| {
            let former = is_former(term);
            term.subterms().any(|subterm| {
                matches!(subterm, Term::Variable(variable) if {
                    let root = self.root(variable);
                    (former && carried_roots.contains(root)) || deepening_roots.contains(root)
                })
            })
        })
    }
}

/// Classify each `=` step of a comparison as an **alias** (`X = Y`, both lone variables — one
/// class) or a **deepening** (`X = t` with `t` a term-former — `X` deepens `t`'s variables), the
/// two ways a body `=` bears on term growth (the re-derivation). `X = c` (a constant) and
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

/// The `=`-class roots that **deepen** each recursive component's carried classes (§5, the
/// re-derivation's P3/P4): a variable `X` with a body deepening `X = f(…v…)` whose `v` is carried
/// by the component, or which — transitively along an `=`-assignment chain — deepens one that is.
/// Reverse the deepening edges (a carried source root → the roots one former deeper) and walk up
/// from each component's carried roots; the roots reached are its deepening set. Component-scoped
/// like `component_roots`, keyed by least member; the `reached` visited-set bounds a cyclic
/// assignment (`X = f(Y), Y = f(X)`) — treated conservatively as deepening, the safe direction.
fn deepen_closure(
    deepenings: &[(Variable, BTreeSet<Variable>)],
    equality_root: &BTreeMap<Variable, Variable>,
    component_roots: &BTreeMap<Signature, BTreeSet<Variable>>,
) -> BTreeMap<Signature, BTreeSet<Variable>> {
    // Reverse deepening edges: a carried-source class → the classes one `=`-former deeper.
    let mut deeper_than: BTreeMap<Variable, Vec<Variable>> = BTreeMap::new();
    for (deep, sources) in deepenings {
        let deep_root = class_root(equality_root, deep).clone();
        for source in sources {
            deeper_than
                .entry(class_root(equality_root, source).clone())
                .or_default()
                .push(deep_root.clone());
        }
    }
    let mut deepening_roots: BTreeMap<Signature, BTreeSet<Variable>> = BTreeMap::new();
    for (key, carried) in component_roots {
        let mut reached: BTreeSet<Variable> = BTreeSet::new();
        let mut stack: Vec<Variable> = carried.iter().cloned().collect();
        while let Some(current) = stack.pop() {
            if let Some(deeper) = deeper_than.get(&current) {
                for root in deeper {
                    if reached.insert(root.clone()) {
                        stack.push(root.clone());
                    }
                }
            }
        }
        if !reached.is_empty() {
            deepening_roots.insert(key.clone(), reached);
        }
    }
    deepening_roots
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
        // bounding the guards to ≤ 2 keeps the aggregate carry `O(signatures)` (Finding 2, P6).
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
