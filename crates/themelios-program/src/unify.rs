//! Substitution, the unifier, and the pattern language (docs/design/program.md §9.2, §11):
//! the substitution core [`mgu`] produces and the transformation surface consumes (§9) — the
//! query side binds with it, the transform side rewrites with it. The substitution is
//! **triangular** (a binding's term may mention bound variables), which is what lets [`mgu`]
//! produce it in near-linear space; [`Term::substitute`] is the **resolving** reader that
//! follows the chains to the fixpoint. Construction is crate-internal, so a successful unify is
//! the only public producer of a substitution: the empty substitution means *unified, binding
//! nothing*, and must never be asserted without having unified (§11.2). [`mgu`] is the
//! near-linear Martelli–Montanari most general unifier over two atoms in one namespace with a
//! forced occurs check (§11.1); [`rename_apart`] is the caller's standardize-apart step, and
//! [`signature_range`] the range scan a pattern match against an answer set stands on (§11.2,
//! §11.3). [`Fresh`] is the collision-free source of new variable and predicate names that
//! [`rename_apart`] and an optimizer's auxiliaries draw from.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::RangeInclusive;

use crate::program::{
    Aggregate, Arguments, Atom, Body, BodyAggregateElement, BodyElement, Choice, ChoiceElement,
    Comparison, Condition, ConditionalLiteral, Disjunction, DisjunctionElement, FunctionAggregate,
    Guard, HasGuards, Head, HeadAggregate, HeadAggregateElement, Literal, LiteralInner,
    OptimizeElement, Program, Project, Rule, SetAggregate, SetElement, Show, Statement, TheoryAtom,
    TheoryElement, TheoryTerm, Weight,
};
use crate::provenance::WithProvenance;
use crate::symbol::{Name, Sign, Symbol, SymbolParts, VarName};
use crate::term::{BinaryOp, Term, TermParts, UnaryOp, Variable};

// ---- The substitution and its bindings (§11.1) ----

/// A substitution: variables to bindings, keyed by variable, one shared namespace.
/// **Triangular**, not fully resolved — a binding's term may itself mention bound variables
/// (`X ↦ f(Y)` with `Y ↦ a`); this is what lets the unifier produce it in near-linear
/// space, the idempotent form over explicit terms being worst-case exponential (§11.1).
/// [`Term::substitute`] is the resolving reader. There is **no** `Default` and no public
/// empty constructor: the empty substitution means *unified, binding nothing* (the
/// affirmative match) and must arise only from a successful unify, never be asserted
/// (§11.2). Construction is crate-internal, so the unifier is the only public producer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Substitution {
    bindings: BTreeMap<Variable, Binding>,
}

impl Substitution {
    /// The **immediate** binding of a variable, unresolved — the triangular map's own
    /// entry, which may mention further bound variables (§11.1). [`Term::substitute`]
    /// resolves; this reads one link. O(log n).
    pub fn get(&self, variable: &Variable) -> Option<&Binding> {
        self.bindings.get(variable)
    }

    /// The bindings in variable (`Ord`) order (§11.1). O(1) to start.
    pub fn iter(&self) -> impl Iterator<Item = (&Variable, &Binding)> {
        self.bindings.iter()
    }

    // The crate-internal mint doors (§11.2): a consumer reaches a substitution only through
    // a successful unify, [`mgu`] being the sole public producer. They are crate-internal, not
    // public, so the empty substitution — *unified, binding nothing* — can never be asserted
    // without having unified (§11.2); the mint restriction holds without a `Default`.

    /// The empty substitution — the affirmative match (unified, binding nothing), minted
    /// internally so it can arise only from a successful unify (§11.2).
    pub(crate) fn empty() -> Substitution {
        Substitution {
            bindings: BTreeMap::new(),
        }
    }

    /// A substitution over the given variable/binding pairs — the unifier's read-out door
    /// (§11.1), keeping the mint restriction crate-internal.
    pub(crate) fn from_pairs(pairs: impl IntoIterator<Item = (Variable, Binding)>) -> Substitution {
        Substitution {
            bindings: pairs.into_iter().collect(),
        }
    }
}

/// A variable's binding (§11.1). One variant now — a ground or non-ground term — and
/// non-exhaustive, so a later constraint binding is a new variant, not a migration.
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Binding {
    /// Bound to a term.
    Bound(Term),
}

// ---- Applying a substitution to a term (§9.2, §11.1, §13) ----

impl Term {
    /// Apply a substitution to this term — **resolving**: each bound variable is replaced
    /// by its binding *and* the binding's own variables are followed to the fixpoint (the
    /// substitution is triangular, §11.1), so `{X↦f(Y), Y↦a}` takes `X` to `f(a)`, not
    /// `f(Y)`; an unbound variable stays itself. The result is canonicalized at the door
    /// (§5.1) — a substitution can make a ground constructor term collapse (`f(X){X↦1}` is
    /// `Symbolic(f(1))`), while a ground *operator* term does not (`X + 2{X↦1}` is `1 + 2`).
    /// Iterative in depth (§13); cost `O(output)`, the resolved result's size (§9.2).
    #[must_use]
    pub fn substitute(self, substitution: &Substitution) -> Term {
        resolve(self, substitution).canonicalize()
    }
}

/// A compound term's reassembly key, carried on [`resolve`]'s work list while its children
/// resolve — the non-child data split from the children (mirroring the term's own owned
/// walk, but over the public parts, since a bound variable splices a *new* subtree the
/// term's `fold` cannot).
enum Rebuild {
    Function(Name),
    Tuple,
    Pool,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Interval,
    Absolute,
    External(Name),
}

/// A frame on [`resolve`]'s work list.
enum Frame {
    /// A term to resolve — from the input, or a binding spliced in for a bound variable.
    Enter(Term),
    /// A compound node awaiting its resolved children, with the count to pop off `done`.
    Assemble(Rebuild, usize),
}

/// Resolve a substitution into a term, *without* canonicalizing — [`Term::substitute`]'s
/// core. Written as an explicit enter/assemble work list rather than the term's `fold`,
/// because a bound variable resolves by **splicing its binding as a new subtree to enter**
/// and continuing into *that* binding's variables to the fixpoint (the substitution is
/// triangular, §11.1) — which a single bottom-up pass over the input's own children cannot
/// do without call-stack recursion down the dereference chain. So a deep term *and* a long
/// dereference chain both stay stack-safe (§13); the cost is `O(output)`, the resolved
/// term's size (§9.2). Termination is the occurs check's (§11): here a well-formed
/// substitution is assumed acyclic.
fn resolve(root: Term, substitution: &Substitution) -> Term {
    let mut work = vec![Frame::Enter(root)];
    let mut done: Vec<Term> = Vec::new();
    while let Some(frame) = work.pop() {
        match frame {
            Frame::Enter(term) => match term.into_parts() {
                TermParts::Variable(variable) => {
                    if let Some(Binding::Bound(binding)) = substitution.get(&variable) {
                        // Resolve: enter the binding in the variable's place, following its
                        // own variables (triangular → fixpoint).
                        work.push(Frame::Enter(binding.clone()));
                    } else {
                        // Unbound: the variable stays itself.
                        done.push(Term::Variable(variable));
                    }
                }
                TermParts::Symbolic(symbol) => done.push(Term::Symbolic(symbol)),
                TermParts::Function { name, arguments } => {
                    enter_children(&mut work, Rebuild::Function(name), arguments);
                }
                TermParts::Tuple(items) => enter_children(&mut work, Rebuild::Tuple, items),
                TermParts::Pool(items) => enter_children(&mut work, Rebuild::Pool, items),
                TermParts::UnaryOperation { operator, argument } => {
                    enter_children(&mut work, Rebuild::Unary(operator), vec![argument]);
                }
                TermParts::BinaryOperation {
                    operator,
                    left,
                    right,
                } => enter_children(&mut work, Rebuild::Binary(operator), vec![left, right]),
                TermParts::Interval { lower, upper } => {
                    enter_children(&mut work, Rebuild::Interval, vec![lower, upper]);
                }
                TermParts::Absolute(inner) => {
                    enter_children(&mut work, Rebuild::Absolute, vec![inner]);
                }
                TermParts::External { name, arguments } => {
                    enter_children(&mut work, Rebuild::External(name), arguments);
                }
            },
            Frame::Assemble(rebuild, arity) => {
                let children = done.split_off(done.len() - arity);
                done.push(assemble(rebuild, children));
            }
        }
    }
    done.pop()
        .expect("the root's substitution leaves exactly one finished term")
}

/// Push a compound node's assemble frame and then its children, reversed so they resolve
/// left-to-right and reassemble in document order.
fn enter_children(work: &mut Vec<Frame>, rebuild: Rebuild, children: Vec<Term>) {
    work.push(Frame::Assemble(rebuild, children.len()));
    for child in children.into_iter().rev() {
        work.push(Frame::Enter(child));
    }
}

/// Reassemble a compound node from its resolved children through the public parts, the
/// inverse of the split in [`resolve`]. The boxed forms pop their children in the reverse
/// of the enter order, restoring document order.
fn assemble(rebuild: Rebuild, mut children: Vec<Term>) -> Term {
    let parts = match rebuild {
        Rebuild::Function(name) => TermParts::Function {
            name,
            arguments: children,
        },
        Rebuild::Tuple => TermParts::Tuple(children),
        Rebuild::Pool => TermParts::Pool(children),
        Rebuild::Unary(operator) => TermParts::UnaryOperation {
            operator,
            argument: children.pop().expect("the unary operand"),
        },
        Rebuild::Binary(operator) => {
            let right = children.pop().expect("the binary right operand");
            let left = children.pop().expect("the binary left operand");
            TermParts::BinaryOperation {
                operator,
                left,
                right,
            }
        }
        Rebuild::Interval => {
            let upper = children.pop().expect("the interval upper bound");
            let lower = children.pop().expect("the interval lower bound");
            TermParts::Interval { lower, upper }
        }
        Rebuild::Absolute => TermParts::Absolute(children.pop().expect("the absolute operand")),
        Rebuild::External(name) => TermParts::External {
            name,
            arguments: children,
        },
    };
    Term::from(parts)
}

// ---- Applying a substitution to a rule (§9.2) ----

/// Apply a substitution to a rule — every **ordinary term** the rule carries in its head
/// and body is resolved, the rule rebuilt preserving each carrier's provenance (§6.2) and
/// routed through the ingest-door canonicalization (§5.1), which re-folds the boolean heads
/// and re-merges the set-shaped children a substitution can make content-equal. A theory
/// atom's *ordinary-term* arguments and its elements' ordinary conditions are substituted;
/// its **theory terms** are a distinct peer algebra (§4.9) this structural surface (§9.3)
/// does not descend. Total; `O(output)`.
#[must_use]
pub fn substitute(rule: Rule, substitution: &Substitution) -> Rule {
    let (head, body) = rule.into_parts();
    let head = head.map(|value| substitute_head(&value, substitution));
    let body = body.map(|value| substitute_body(&value, substitution));
    Rule::from_nodes(head, body).canonicalize()
}

/// Rebuild a provenance-carrying node, substituting its content while carrying its
/// provenance through unchanged (§6.2).
fn map_carrier<T>(node: &WithProvenance<T>, f: impl FnOnce(&T) -> T) -> WithProvenance<T> {
    WithProvenance::new(f(node.get()), node.provenance().clone())
}

fn substitute_head(head: &Head, s: &Substitution) -> Head {
    match head {
        Head::Literal(literal) => Head::Literal(substitute_literal(literal, s)),
        Head::Disjunction(disjunction) => Head::Disjunction(substitute_disjunction(disjunction, s)),
        Head::Choice(choice) => Head::Choice(substitute_choice(choice, s)),
        Head::Aggregate(aggregate) => Head::Aggregate(substitute_head_aggregate(aggregate, s)),
        Head::TheoryAtom(atom) => Head::TheoryAtom(substitute_theory_atom(atom, s)),
        Head::Falsum => Head::Falsum,
        Head::Verum => Head::Verum,
    }
}

fn substitute_body(body: &Body, s: &Substitution) -> Body {
    Body::from_nodes(
        body.elements()
            .map(|element| map_carrier(element, |value| substitute_body_element(value, s))),
    )
}

fn substitute_body_element(element: &BodyElement, s: &Substitution) -> BodyElement {
    match element {
        BodyElement::Literal(literal) => BodyElement::Literal(substitute_literal(literal, s)),
        BodyElement::Conditional(conditional) => {
            BodyElement::Conditional(substitute_conditional(conditional, s))
        }
        BodyElement::Aggregate {
            negation,
            aggregate,
        } => BodyElement::Aggregate {
            negation: *negation,
            aggregate: substitute_aggregate(aggregate, s),
        },
        BodyElement::TheoryAtom { negation, atom } => BodyElement::TheoryAtom {
            negation: *negation,
            atom: substitute_theory_atom(atom, s),
        },
    }
}

fn substitute_literal(literal: &Literal, s: &Substitution) -> Literal {
    Literal {
        negation: literal.negation,
        inner: match &literal.inner {
            LiteralInner::Atom(atom) => {
                LiteralInner::Atom(map_carrier(atom, |value| substitute_atom(value, s)))
            }
            LiteralInner::Comparison(comparison) => {
                LiteralInner::Comparison(map_carrier(comparison, |value| {
                    substitute_comparison(value, s)
                }))
            }
            LiteralInner::True => LiteralInner::True,
            LiteralInner::False => LiteralInner::False,
        },
    }
}

fn substitute_atom(atom: &Atom, s: &Substitution) -> Atom {
    Atom {
        sign: atom.sign,
        name: atom.name.clone(),
        arguments: atom.arguments.map_terms(|term| term.clone().substitute(s)),
    }
}

fn substitute_comparison(comparison: &Comparison, s: &Substitution) -> Comparison {
    let mut steps = comparison.steps();
    let (relation, first_step) = steps.next().expect("a comparison has at least one step");
    let mut result = Comparison::new(
        comparison.first().clone().substitute(s),
        relation,
        first_step.clone().substitute(s),
    );
    for (relation, term) in steps {
        result = result.chain(relation, term.clone().substitute(s));
    }
    result
}

fn substitute_condition(condition: &Condition, s: &Substitution) -> Condition {
    Condition::from_nodes(
        condition
            .literals()
            .map(|literal| map_carrier(literal, |value| substitute_literal(value, s))),
    )
}

fn substitute_conditional(
    conditional: &ConditionalLiteral,
    s: &Substitution,
) -> ConditionalLiteral {
    ConditionalLiteral {
        literal: substitute_literal(&conditional.literal, s),
        condition: substitute_condition(&conditional.condition, s),
    }
}

fn substitute_disjunction(disjunction: &Disjunction, s: &Substitution) -> Disjunction {
    Disjunction::from_nodes(
        disjunction
            .elements()
            .map(|element| map_carrier(element, |value| substitute_disjunction_element(value, s))),
    )
}

fn substitute_disjunction_element(
    element: &DisjunctionElement,
    s: &Substitution,
) -> DisjunctionElement {
    DisjunctionElement::new(
        substitute_literal(element.literal(), s),
        substitute_condition(element.condition(), s),
    )
}

fn substitute_choice(choice: &Choice, s: &Substitution) -> Choice {
    Choice::from_nodes(
        choice.left_guard().map(|guard| substitute_guard(guard, s)),
        choice
            .elements()
            .map(|element| map_carrier(element, |value| substitute_choice_element(value, s))),
        choice.right_guard().map(|guard| substitute_guard(guard, s)),
    )
}

fn substitute_choice_element(element: &ChoiceElement, s: &Substitution) -> ChoiceElement {
    ChoiceElement::new(
        substitute_literal(element.literal(), s),
        substitute_condition(element.condition(), s),
    )
}

fn substitute_guard(guard: &WithProvenance<Guard>, s: &Substitution) -> WithProvenance<Guard> {
    map_carrier(guard, |guard| Guard {
        relation: guard.relation,
        term: guard.term.clone().substitute(s),
    })
}

fn substitute_head_aggregate(aggregate: &HeadAggregate, s: &Substitution) -> HeadAggregate {
    HeadAggregate::from_nodes(
        aggregate
            .left_guard()
            .map(|guard| substitute_guard(guard, s)),
        aggregate.function(),
        aggregate.elements().map(|element| {
            map_carrier(element, |value| substitute_head_aggregate_element(value, s))
        }),
        aggregate
            .right_guard()
            .map(|guard| substitute_guard(guard, s)),
    )
}

fn substitute_head_aggregate_element(
    element: &HeadAggregateElement,
    s: &Substitution,
) -> HeadAggregateElement {
    HeadAggregateElement::new(
        element.terms().map(|term| term.clone().substitute(s)),
        substitute_literal(element.literal(), s),
        substitute_condition(element.condition(), s),
    )
}

fn substitute_aggregate(aggregate: &Aggregate, s: &Substitution) -> Aggregate {
    match aggregate {
        Aggregate::Function(function) => {
            Aggregate::Function(substitute_function_aggregate(function, s))
        }
        Aggregate::Set(set) => Aggregate::Set(substitute_set_aggregate(set, s)),
    }
}

fn substitute_function_aggregate(
    aggregate: &FunctionAggregate,
    s: &Substitution,
) -> FunctionAggregate {
    FunctionAggregate::from_nodes(
        aggregate
            .left_guard()
            .map(|guard| substitute_guard(guard, s)),
        aggregate.function(),
        aggregate.elements().map(|element| {
            map_carrier(element, |value| substitute_body_aggregate_element(value, s))
        }),
        aggregate
            .right_guard()
            .map(|guard| substitute_guard(guard, s)),
    )
}

fn substitute_body_aggregate_element(
    element: &BodyAggregateElement,
    s: &Substitution,
) -> BodyAggregateElement {
    BodyAggregateElement::new(
        element.terms().map(|term| term.clone().substitute(s)),
        substitute_condition(element.condition(), s),
    )
}

fn substitute_set_aggregate(aggregate: &SetAggregate, s: &Substitution) -> SetAggregate {
    SetAggregate::from_nodes(
        aggregate
            .left_guard()
            .map(|guard| substitute_guard(guard, s)),
        aggregate
            .elements()
            .map(|element| map_carrier(element, |value| substitute_set_element(value, s))),
        aggregate
            .right_guard()
            .map(|guard| substitute_guard(guard, s)),
    )
}

fn substitute_set_element(element: &SetElement, s: &Substitution) -> SetElement {
    match element {
        SetElement::Literal(literal) => SetElement::Literal(substitute_literal(literal, s)),
        SetElement::ConditionalLiteral(conditional) => {
            SetElement::ConditionalLiteral(substitute_conditional(conditional, s))
        }
    }
}

fn substitute_theory_atom(atom: &TheoryAtom, s: &Substitution) -> TheoryAtom {
    // The ordinary-term arguments and the elements' ordinary conditions are substituted;
    // the theory terms (an element's terms and the guard's bound) are a distinct peer
    // algebra (§4.9) this structural surface (§9.3) does not descend, and the guard rides
    // through unchanged.
    TheoryAtom::from_nodes(
        atom.name().clone(),
        atom.arguments()
            .map(|term| term.clone().substitute(s))
            .collect(),
        atom.elements()
            .map(|element| map_carrier(element, |value| substitute_theory_element(value, s))),
        atom.guard().cloned(),
    )
}

fn substitute_theory_element(element: &TheoryElement, s: &Substitution) -> TheoryElement {
    TheoryElement::new(
        element.terms().cloned(),
        element
            .condition()
            .map(|condition| substitute_condition(condition, s)),
    )
}

// ---- Fresh variable and predicate names (§9.2) ----

/// A source of fresh variables and predicate names colliding with none already in a program
/// (§9.2) — what rename-apart (§11) and an optimizer's auxiliary predicates draw from.
/// Seed it with [`of`](Fresh::of), then mint with [`variable`](Fresh::variable) and
/// [`predicate`](Fresh::predicate); each minted name is recorded, so a run of mints is
/// pairwise-distinct as well as free of the program's names.
pub struct Fresh {
    variables: BTreeSet<VarName>,
    predicates: BTreeSet<Name>,
    counter: u64,
}

impl Fresh {
    /// Seed a source from a program, scanning it once for the variable names and the
    /// predicate names occurring in it (§9.2). A theory term contributes its variables — a
    /// variable is the shared leaf of the two term algebras (§4.9) — but the theory
    /// namespace (a theory atom's name, a theory operator) is not the ordinary predicate
    /// namespace and is not collected. `O(program)`.
    #[must_use]
    pub fn of(program: &Program) -> Fresh {
        let mut names = Names::default();
        for statement in program.statements() {
            collect_statement(statement.get(), &mut names);
        }
        Fresh {
            variables: names.variables,
            predicates: names.predicates,
            counter: 0,
        }
    }

    /// A fresh variable — a `V`-prefixed counter, skipping any name already seen and
    /// recording the minted one, so it collides with none in the program and none minted
    /// before it (§9.2). Amortized `O(1)`.
    #[must_use]
    pub fn variable(&mut self) -> Variable {
        loop {
            let candidate = VarName::new(format!("V{}", self.counter))
                .expect("a `V` before a number is a legal variable name");
            self.counter += 1;
            if self.variables.insert(candidate.clone()) {
                return Variable::Named(candidate);
            }
        }
    }

    /// A fresh predicate name from a hint plus a disambiguating suffix, skipping any name
    /// already seen and recording the minted one (§9.2). The hint reads through when it is a
    /// legal identifier; otherwise a legal base stands in, so the minted name is legal by
    /// construction. Amortized `O(1)`.
    #[must_use]
    pub fn predicate(&mut self, hint: &str) -> Name {
        let base = if Name::new(hint).is_ok() { hint } else { "aux" };
        loop {
            let candidate = Name::new(format!("{base}{}", self.counter))
                .expect("a legal identifier before a number is a legal identifier");
            self.counter += 1;
            if self.predicates.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

/// The two seen-sets a [`Fresh`] is seeded from: the variable names and the predicate names
/// a program mentions (§9.2).
#[derive(Default)]
struct Names {
    variables: BTreeSet<VarName>,
    predicates: BTreeSet<Name>,
}

/// Collect a statement's variable and predicate names. The match is exhaustive with no
/// wildcard, so a new statement family is a compile error here, never a silently unscanned
/// one. The opaque regions (`#script`, `#include`) carry neither; a `#theory` definition's
/// names are the theory namespace (§4.9), not the ordinary predicate namespace.
fn collect_statement(statement: &Statement, names: &mut Names) {
    match statement {
        Statement::Rule(rule) => {
            collect_head(rule.head().get(), names);
            collect_body(rule.body().get(), names);
        }
        Statement::WeakConstraint(weak) => {
            collect_body(weak.body().get(), names);
            collect_weight(weak.weight(), names);
            for term in weak.terms() {
                collect_term(term, names);
            }
        }
        Statement::Optimize(optimize) => {
            for element in optimize.elements() {
                collect_optimize_element(element.get(), names);
            }
        }
        Statement::Show(show) => match show {
            Show::All => {}
            Show::Signature(signature) => {
                names.predicates.insert(signature.name.clone());
            }
            Show::Term(term) => collect_term(term, names),
            Show::TermBody { term, body } => {
                collect_term(term, names);
                collect_body(body.get(), names);
            }
        },
        Statement::Project(project) => match project {
            Project::Signature(signature) => {
                names.predicates.insert(signature.name.clone());
            }
            Project::Atom { atom, body } => {
                collect_atom(atom.get(), names);
                collect_body(body.get(), names);
            }
        },
        Statement::Defined(defined) => {
            names.predicates.insert(defined.signature.name.clone());
        }
        Statement::Edge(edge) => {
            for (from, to) in edge.pairs() {
                collect_term(from, names);
                collect_term(to, names);
            }
            collect_body(edge.body().get(), names);
        }
        Statement::Heuristic(heuristic) => {
            collect_atom(heuristic.atom().get(), names);
            collect_body(heuristic.body().get(), names);
            collect_term(heuristic.bias(), names);
            if let Some(priority) = heuristic.priority() {
                collect_term(priority, names);
            }
            collect_term(heuristic.modifier(), names);
        }
        Statement::External(external) => {
            collect_atom(external.atom().get(), names);
            collect_body(external.body().get(), names);
            if let Some(value) = external.value() {
                collect_term(value, names);
            }
        }
        Statement::Const(constant) => {
            names.predicates.insert(constant.name.clone());
            collect_term(&constant.value, names);
        }
        Statement::Query(query) => collect_atom(query.atom().get(), names),
        // Opaque or theory-namespace: no ordinary variable or predicate name (§4.8, §4.9).
        Statement::Include(_) | Statement::Script(_) | Statement::TheoryDefinition(_) => {}
    }
}

/// Collect a term's variable names and its function and external functor names — an ordinary
/// term's identifiers share the predicate namespace, so a fresh predicate must avoid them.
fn collect_term(term: &Term, names: &mut Names) {
    for subterm in term.subterms() {
        match subterm {
            Term::Variable(Variable::Named(name)) => {
                names.variables.insert(name.clone());
            }
            Term::Function { name, .. } | Term::External { name, .. } => {
                names.predicates.insert(name.clone());
            }
            // A ground symbol is a leaf to `subterms`, but its own functors still share the
            // predicate namespace (§9.2): a canonical program stores every fully-ground term
            // as a `Symbolic`, so `p(f0(a))` hides the functor `f0` a fresh predicate must
            // avoid. Descend the symbol and collect its function functors.
            Term::Symbolic(symbol) => {
                for subsymbol in symbol.subsymbols() {
                    if let Symbol::Function { name, .. } = subsymbol {
                        names.predicates.insert(name.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect a theory term's variable names only — a variable is the shared leaf of the two
/// term algebras (§4.9); a theory functor is the theory namespace, not collected.
fn collect_theory_term(term: &TheoryTerm, names: &mut Names) {
    for subterm in term.subterms() {
        if let TheoryTerm::Variable(Variable::Named(name)) = subterm {
            names.variables.insert(name.clone());
        }
    }
}

fn collect_atom(atom: &Atom, names: &mut Names) {
    names.predicates.insert(atom.name.clone());
    for term in atom.argument_terms() {
        collect_term(term, names);
    }
}

fn collect_literal(literal: &Literal, names: &mut Names) {
    match &literal.inner {
        LiteralInner::Atom(atom) => collect_atom(atom.get(), names),
        LiteralInner::Comparison(comparison) => collect_comparison(comparison.get(), names),
        LiteralInner::True | LiteralInner::False => {}
    }
}

fn collect_comparison(comparison: &Comparison, names: &mut Names) {
    collect_term(comparison.first(), names);
    for (_relation, term) in comparison.steps() {
        collect_term(term, names);
    }
}

fn collect_condition(condition: &Condition, names: &mut Names) {
    for literal in condition.literals() {
        collect_literal(literal.get(), names);
    }
}

fn collect_conditional(conditional: &ConditionalLiteral, names: &mut Names) {
    collect_literal(&conditional.literal, names);
    collect_condition(&conditional.condition, names);
}

fn collect_guard(guard: &WithProvenance<Guard>, names: &mut Names) {
    collect_term(&guard.get().term, names);
}

fn collect_head(head: &Head, names: &mut Names) {
    match head {
        Head::Literal(literal) => collect_literal(literal, names),
        Head::Disjunction(disjunction) => {
            for element in disjunction.elements() {
                collect_literal(element.get().literal(), names);
                collect_condition(element.get().condition(), names);
            }
        }
        Head::Choice(choice) => {
            if let Some(guard) = choice.left_guard() {
                collect_guard(guard, names);
            }
            for element in choice.elements() {
                collect_literal(element.get().literal(), names);
                collect_condition(element.get().condition(), names);
            }
            if let Some(guard) = choice.right_guard() {
                collect_guard(guard, names);
            }
        }
        Head::Aggregate(aggregate) => collect_head_aggregate(aggregate, names),
        Head::TheoryAtom(atom) => collect_theory_atom(atom, names),
        Head::Falsum | Head::Verum => {}
    }
}

fn collect_body(body: &Body, names: &mut Names) {
    for element in body.elements() {
        match element.get() {
            BodyElement::Literal(literal) => collect_literal(literal, names),
            BodyElement::Conditional(conditional) => collect_conditional(conditional, names),
            BodyElement::Aggregate { aggregate, .. } => collect_aggregate(aggregate, names),
            BodyElement::TheoryAtom { atom, .. } => collect_theory_atom(atom, names),
        }
    }
}

fn collect_aggregate(aggregate: &Aggregate, names: &mut Names) {
    match aggregate {
        Aggregate::Function(function) => {
            collect_guards(function, names);
            for element in function.elements() {
                for term in element.get().terms() {
                    collect_term(term, names);
                }
                collect_condition(element.get().condition(), names);
            }
        }
        Aggregate::Set(set) => {
            collect_guards(set, names);
            for element in set.elements() {
                match element.get() {
                    SetElement::Literal(literal) => collect_literal(literal, names),
                    SetElement::ConditionalLiteral(conditional) => {
                        collect_conditional(conditional, names);
                    }
                }
            }
        }
    }
}

fn collect_head_aggregate(aggregate: &HeadAggregate, names: &mut Names) {
    collect_guards(aggregate, names);
    for element in aggregate.elements() {
        for term in element.get().terms() {
            collect_term(term, names);
        }
        collect_literal(element.get().literal(), names);
        collect_condition(element.get().condition(), names);
    }
}

fn collect_guards(aggregate: &impl HasGuards, names: &mut Names) {
    if let Some(guard) = aggregate.left_guard() {
        collect_guard(guard, names);
    }
    if let Some(guard) = aggregate.right_guard() {
        collect_guard(guard, names);
    }
}

fn collect_theory_atom(atom: &TheoryAtom, names: &mut Names) {
    for term in atom.arguments() {
        collect_term(term, names);
    }
    for element in atom.elements() {
        for theory_term in element.get().terms() {
            collect_theory_term(theory_term, names);
        }
        if let Some(condition) = element.get().condition() {
            collect_condition(condition, names);
        }
    }
    if let Some(guard) = atom.guard() {
        collect_theory_term(&guard.term, names);
    }
}

fn collect_weight(weight: &Weight, names: &mut Names) {
    collect_term(weight.term(), names);
    if let Some(priority) = weight.priority() {
        collect_term(priority, names);
    }
}

fn collect_optimize_element(element: &OptimizeElement, names: &mut Names) {
    collect_weight(element.weight(), names);
    for term in element.terms() {
        collect_term(term, names);
    }
    collect_condition(element.condition(), names);
}

// ---- The most general unifier and the pattern language (§11) ----

/// Why an atom is not a pattern (§11.2), carrying the offending argument term. One reason
/// today, and non-exhaustive, so a later reason is a new variant, not a migration.
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum NotAPattern {
    /// An argument does not denote a ground pattern term: a *ground* arithmetic term that does
    /// not denote (an undefined operation, or a result out of range, §3.5), an arithmetic term
    /// with a variable (it would need inverting — this tier evaluates only ground arithmetic in a
    /// pattern), an interval or a pool (each *names a set*, whose all-versus-any reading a
    /// term-against-symbol match cannot decide), or an unevaluated `@`-call.
    NonDenoting {
        /// The offending term.
        term: Term,
    },
    /// The atom is an argument-list pool (`p(a; b)`, §4.6): it names a *set* of atoms, not
    /// one, so a term-against-symbol match cannot decide it — the atom-level twin of a pooled
    /// *term* argument's [`NonDenoting`](NotAPattern::NonDenoting) refusal. Unpool (§9) first.
    Pooled,
}

impl std::fmt::Display for NotAPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAPattern::NonDenoting { term } => {
                write!(
                    f,
                    "not a pattern: the term {term:?} does not denote a ground term"
                )
            }
            NotAPattern::Pooled => {
                write!(
                    f,
                    "not a pattern: an argument-list pool names a set of atoms, not one"
                )
            }
        }
    }
}
impl std::error::Error for NotAPattern {}

/// The most general unifier of two atoms, in one shared variable namespace (§11.1).
/// `Ok(Some(σ))` — a unifier exists; `Ok(None)` — the atoms do not unify; `Err` — an argument
/// is not a pattern, so the question cannot be answered (§11.2). The three outcomes are
/// distinct on purpose: `Ok(None)` is *no such match*, `Err` is *cannot decide* (spec §5.2),
/// and collapsing them would answer "no such atom" of a term this cannot decide. The argument
/// unifier is Martelli–Montanari (1982): a union-find over the atoms' term structure carrying
/// the occurs check, read out as a **triangular** [`Substitution`] the resolving
/// [`Term::substitute`] materialises on demand (§9.2). Near-linear in both atoms to decide and
/// produce; iterative in their depth (§13).
pub fn mgu(left: &Atom, right: &Atom) -> Result<Option<Substitution>, NotAPattern> {
    // Both atoms must be patterns before they can be compared or unified: a non-pattern
    // argument is a property of the atom, refusing whatever the other atom is (§11.2), and its
    // *ground* arithmetic evaluates here (§3.5).
    let left_arguments = pattern_arguments(left)?;
    let right_arguments = pattern_arguments(right)?;
    // The sign, name, and arity must agree for two atoms to unify (§11.1); then the arguments
    // unify pairwise.
    if left.sign != right.sign
        || left.name != right.name
        || left_arguments.len() != right_arguments.len()
    {
        return Ok(None);
    }
    Ok(unify_arguments(left_arguments, right_arguments))
}

/// A copy of an atom with its variables renamed to fresh ones (§9.2, §11.1) — the caller's
/// standardize-apart step before a cross-rule unify, [`mgu`] itself being single-namespace. A
/// named variable renames consistently (its every occurrence to one fresh name); each
/// anonymous `_` renames to its own fresh name (each `_` is a distinct variable). The functor,
/// sign, and structure are unchanged. Iterative in depth (§13).
pub fn rename_apart(atom: &Atom, fresh: &mut Fresh) -> Atom {
    let mut renaming: HashMap<VarName, Variable> = HashMap::new();
    Atom {
        sign: atom.sign,
        name: atom.name.clone(),
        arguments: atom
            .arguments
            .map_terms(|term| rename_term(term.clone(), &mut renaming, fresh)),
    }
}

/// Rename one term's variables to fresh ones (§11.1): a named variable consistently through a
/// shared map, each anonymous `_` afresh. Written in the iterative `fold` (§13); the renaming
/// grounds nothing, so a canonical term stays canonical.
fn rename_term(term: Term, renaming: &mut HashMap<VarName, Variable>, fresh: &mut Fresh) -> Term {
    term.fold(|parts| match parts {
        TermParts::Variable(Variable::Named(name)) => Term::Variable(
            renaming
                .entry(name)
                .or_insert_with(|| fresh.variable())
                .clone(),
        ),
        TermParts::Variable(Variable::Anonymous) => Term::Variable(fresh.variable()),
        other => Term::from(other),
    })
}

/// The inclusive range of `Symbol`s a pattern (a signed `Atom`, §11.2) could unify with — its
/// predicate, arity, and sign block in the ground-term order (§3.1): the least such symbol has
/// every argument `Infimum`, the greatest every argument `Supremum`. Total — the signature is
/// always concrete, so it never refuses; a non-pattern argument is the unifier's concern
/// (§11.2), not the range's. Lets a match against an answer set be `O(log n + k)`, not `O(n)`
/// (§11.3). `O(arity)`.
pub fn signature_range(pattern: &Atom) -> RangeInclusive<Symbol> {
    // A pattern is a `Single` atom (§11.2): a pooled atom names a set and is refused as a
    // pattern (`pattern_arguments`), so it does not reach here; guard it with an empty range
    // (`Supremum..=Infimum`), matching nothing.
    let Arguments::Single(terms) = &pattern.arguments else {
        return Symbol::Supremum..=Symbol::Infimum;
    };
    let block = |filler: Symbol| Symbol::Function {
        name: pattern.name.clone(),
        arguments: vec![filler; terms.len()],
        sign: pattern.sign,
    };
    block(Symbol::Infimum)..=block(Symbol::Supremum)
}

/// Normalise every argument of an atom to its pattern form, or refuse (§11.2). Collecting into
/// a `Result` short-circuits on the first non-denoting argument.
fn pattern_arguments(atom: &Atom) -> Result<Vec<Term>, NotAPattern> {
    // A pooled atom names a *set* of atoms, not one — it is not a pattern (§11.2), the
    // atom-level twin of a pooled *term* argument's refusal below. Unpool (§9) before matching.
    let Arguments::Single(terms) = &atom.arguments else {
        return Err(NotAPattern::Pooled);
    };
    terms.iter().map(pattern_normalize).collect()
}

/// The pattern form of one term (§11.2), or a refusal carrying the non-denoting former. The
/// pattern fragment is variable / ground symbol / function / tuple; a *ground* arithmetic
/// former evaluates to its symbol (§3.5), while a variable-bearing arithmetic former, an
/// interval, a pool, or an `@`-call does not denote. Written in the iterative `try_fold` (§13);
/// the result canonicalises, so a newly-ground constructor (`f(1 + 2)` → `f(3)`) collapses to a
/// symbol the way its ground twin already has.
fn pattern_normalize(term: &Term) -> Result<Term, NotAPattern> {
    let normalized = term
        .clone()
        .try_fold::<Term, NotAPattern>(|parts| match parts {
            TermParts::Variable(variable) => Ok(Term::Variable(variable)),
            TermParts::Symbolic(symbol) => Ok(Term::Symbolic(symbol)),
            TermParts::Function { name, arguments } => Ok(Term::Function { name, arguments }),
            TermParts::Tuple(items) => Ok(Term::Tuple(items)),
            // Arithmetic composes with the evaluator (§3.5): ground → its symbol; a variable
            // (or any non-denoting subterm) makes it refuse, carrying the former.
            arithmetic @ (TermParts::UnaryOperation { .. }
            | TermParts::BinaryOperation { .. }
            | TermParts::Absolute(_)) => {
                let former = Term::from(arithmetic);
                match former.evaluate() {
                    Ok(symbol) => Ok(Term::Symbolic(symbol)),
                    Err(_) => Err(NotAPattern::NonDenoting { term: former }),
                }
            }
            // A set-former or an `@`-call never denotes a single ground term in a pattern.
            set_or_call @ (TermParts::Interval { .. }
            | TermParts::Pool(_)
            | TermParts::External { .. }) => Err(NotAPattern::NonDenoting {
                term: Term::from(set_or_call),
            }),
        })?;
    Ok(normalized.canonicalize())
}

/// The Martelli–Montanari argument unifier (§11.1): builds both argument lists into a shared
/// node graph (a variable's every occurrence — across *both* atoms, one namespace — one node),
/// unifies pairwise through a union-find carrying the occurs check, and reads out the triangular
/// substitution. `None` — the arguments clash, or the occurs check finds a cyclic binding;
/// `Some(σ)` — they unify under σ. Near-linear in the arguments' size; iterative in depth (§13).
fn unify_arguments(left: Vec<Term>, right: Vec<Term>) -> Option<Substitution> {
    let mut nodes = Nodes::default();
    let pairs: Vec<(usize, usize)> = left
        .into_iter()
        .zip(right)
        .map(|(l, r)| (nodes.build(l), nodes.build(r)))
        .collect();
    for (a, b) in pairs {
        if !nodes.unify(a, b) {
            return None;
        }
    }
    if !nodes.occurs_check_holds() {
        return None;
    }
    Some(nodes.read_out())
}

/// One node of the unification graph (§11.1): a variable (its name kept for the read-out), a
/// ground **leaf** symbol (a number, string, `Infimum`/`Supremum`, or a strongly-negated function
/// kept whole), or a positive function / tuple over child node indices. A ground symbol is
/// **decomposed** into these cells at build time (a positive `f(1)` becomes a `Function` over a
/// leaf `1`), uniform with a term of the same shape — so unifying a ground `f(1)` with a term
/// `f(X)` is a structural descent, never a per-level re-clone of the symbol; that uniformity is
/// what keeps the unifier near-linear rather than quadratic on a deep ground symbol (§15).
#[derive(Clone)]
enum Cell {
    Variable(Variable),
    Leaf(Symbol),
    Function { name: Name, children: Vec<usize> },
    Tuple { children: Vec<usize> },
}

/// A step on the occurs-check DFS (§11.1): enter a class representative, or paint it black on
/// the way back out.
enum Descent {
    Enter(usize),
    Leave(usize),
}

/// A DFS colour for the occurs check (§11.1): unvisited, on the stack, or finished.
#[derive(Clone, Copy, PartialEq)]
enum Colour {
    White,
    Grey,
    Black,
}

/// A step on the read-out's term rebuild (§11.1), iterative in depth (§13).
enum Reconstruct {
    Enter(usize),
    AssembleFunction(Name, usize),
    AssembleTuple(usize),
}

/// A step on the ground-symbol decomposition (§11.1), iterative in depth (§13).
enum SymbolFrame {
    Enter(Symbol),
    AssembleFunction(Name, usize),
    AssembleTuple(usize),
}

/// The union-find node graph the argument unifier works over (§11.1). Variables are interned so
/// a name's every occurrence is one node (one namespace); a class's representative carries the
/// class's shape, so unifying a variable with a term is a link and unifying two terms recurses
/// on their children at most once — the sharing that keeps Martelli–Montanari near-linear.
#[derive(Default)]
struct Nodes {
    cell: Vec<Cell>,
    parent: Vec<usize>,
    rank: Vec<u8>,
    named: HashMap<VarName, usize>,
}

impl Nodes {
    /// Append a node, its own class initially, returning its index.
    fn push(&mut self, cell: Cell) -> usize {
        let id = self.cell.len();
        self.cell.push(cell);
        self.parent.push(id);
        self.rank.push(0);
        id
    }

    /// The node for a variable: a named variable's every occurrence interns to one node (one
    /// namespace, §11.1); each anonymous `_` is a fresh, distinct node.
    fn variable(&mut self, variable: Variable) -> usize {
        match &variable {
            Variable::Named(name) => {
                if let Some(&id) = self.named.get(name) {
                    return id;
                }
                let name = name.clone();
                let id = self.push(Cell::Variable(variable));
                self.named.insert(name, id);
                id
            }
            Variable::Anonymous => self.push(Cell::Variable(variable)),
        }
    }

    /// Build a pattern-normalised term into the node graph, returning its root node. Written in
    /// the iterative `fold` (§13): each level becomes a node over its children's nodes, and a
    /// ground symbol leaf is decomposed by [`build_symbol`].
    fn build(&mut self, term: Term) -> usize {
        term.fold(|parts| match parts {
            TermParts::Variable(variable) => self.variable(variable),
            TermParts::Symbolic(symbol) => self.build_symbol(symbol),
            TermParts::Function { name, arguments } => self.push(Cell::Function {
                name,
                children: arguments,
            }),
            TermParts::Tuple(items) => self.push(Cell::Tuple { children: items }),
            // A pattern-normalised term carries no other former (§11.2): pattern_normalize has
            // evaluated the ground arithmetic and refused the set-formers and `@`-calls.
            TermParts::UnaryOperation { .. }
            | TermParts::BinaryOperation { .. }
            | TermParts::Absolute(_)
            | TermParts::Interval { .. }
            | TermParts::Pool(_)
            | TermParts::External { .. } => {
                unreachable!("a pattern-normalised term is a variable, symbol, function, or tuple")
            }
        })
    }

    /// Decompose a ground symbol into cells (§11.1), returning its root node. A **positive**
    /// function or a tuple becomes a `Function`/`Tuple` node over its decomposed children —
    /// uniform with a term of the same shape, so a ground `f(1)` and a term `f(X)` unify by
    /// structural descent, not a per-level re-clone of the symbol (the near-linearity §15
    /// promises). An atomic symbol, or a strongly-negated function — which a positive term
    /// function (§3.3) can only clash with — stays a whole `Leaf`. Iterative in depth (§13), a
    /// ground symbol being unbounded (§3.1).
    fn build_symbol(&mut self, symbol: Symbol) -> usize {
        let mut work = vec![SymbolFrame::Enter(symbol)];
        let mut done: Vec<usize> = Vec::new();
        while let Some(frame) = work.pop() {
            match frame {
                SymbolFrame::Enter(symbol) => match symbol.into_parts() {
                    SymbolParts::Infimum => done.push(self.push(Cell::Leaf(Symbol::Infimum))),
                    SymbolParts::Supremum => done.push(self.push(Cell::Leaf(Symbol::Supremum))),
                    SymbolParts::Number(value) => {
                        done.push(self.push(Cell::Leaf(Symbol::Number(value))));
                    }
                    SymbolParts::String(text) => {
                        done.push(self.push(Cell::Leaf(Symbol::String(text))));
                    }
                    SymbolParts::Function {
                        name,
                        arguments,
                        sign,
                    } => {
                        if sign == Sign::Positive {
                            work.push(SymbolFrame::AssembleFunction(name, arguments.len()));
                            for argument in arguments.into_iter().rev() {
                                work.push(SymbolFrame::Enter(argument));
                            }
                        } else {
                            // A strongly-negated symbol has no positive term counterpart (§3.3),
                            // so it is kept whole and only ever clashes with a term function.
                            done.push(self.push(Cell::Leaf(Symbol::Function {
                                name,
                                arguments,
                                sign,
                            })));
                        }
                    }
                    SymbolParts::Tuple(elements) => {
                        work.push(SymbolFrame::AssembleTuple(elements.len()));
                        for element in elements.into_iter().rev() {
                            work.push(SymbolFrame::Enter(element));
                        }
                    }
                },
                SymbolFrame::AssembleFunction(name, arity) => {
                    let children = done.split_off(done.len() - arity);
                    done.push(self.push(Cell::Function { name, children }));
                }
                SymbolFrame::AssembleTuple(arity) => {
                    let children = done.split_off(done.len() - arity);
                    done.push(self.push(Cell::Tuple { children }));
                }
            }
        }
        done.pop()
            .expect("the symbol's decomposition leaves one node")
    }

    /// The class representative of a node, with path halving — so finds stay amortized
    /// near-linear (the class merges are by rank where both sides are shaped or both bare, a
    /// bare variable otherwise linking under the shaped side, which must be the representative).
    fn find(&mut self, mut id: usize) -> usize {
        while self.parent[id] != id {
            let grandparent = self.parent[self.parent[id]];
            self.parent[id] = grandparent;
            id = grandparent;
        }
        id
    }

    /// Link a class under another, which keeps its shape as the representative (§11.1). Used
    /// when one class is a bare variable: it binds to the other.
    fn link(&mut self, child: usize, root: usize) {
        self.parent[child] = root;
    }

    /// Merge two representative classes by rank, returning the new representative (§11.1). Used
    /// when both carry a shape (a function or a tuple).
    fn union(&mut self, left: usize, right: usize) -> usize {
        match self.rank[left].cmp(&self.rank[right]) {
            Ordering::Less => {
                self.parent[left] = right;
                right
            }
            Ordering::Greater => {
                self.parent[right] = left;
                left
            }
            Ordering::Equal => {
                self.parent[right] = left;
                self.rank[left] += 1;
                left
            }
        }
    }

    /// Unify two nodes, returning `false` on a clash (§11.1). Iterative over an explicit work
    /// list of node pairs (§13); each pair of classes is merged at most once (the union-find
    /// short-circuit), so a shared subterm is visited once — the Martelli–Montanari sharing.
    fn unify(&mut self, first: usize, second: usize) -> bool {
        let mut work = vec![(first, second)];
        while let Some((first, second)) = work.pop() {
            let left = self.find(first);
            let right = self.find(second);
            if left == right {
                continue;
            }
            match (self.cell[left].clone(), self.cell[right].clone()) {
                // Two variable classes merge by rank (either may be the representative, both
                // being bare); the read-out canonicalizes the class by its Ord-least variable, so
                // which node wins is immaterial to the result.
                (Cell::Variable(_), Cell::Variable(_)) => {
                    self.union(left, right);
                }
                // A bare variable class links under a shaped one, which keeps its shape.
                (Cell::Variable(_), _) => self.link(left, right),
                (_, Cell::Variable(_)) => self.link(right, left),
                // Two ground leaves: ground, so they unify iff identical.
                (Cell::Leaf(left_symbol), Cell::Leaf(right_symbol)) => {
                    if left_symbol != right_symbol {
                        return false;
                    }
                }
                // Two functions of the same shape — a term or a decomposed ground symbol, held
                // uniformly (§11.1): merge and descend. A ground `f(1)` and a term `f(X)` meet
                // here, not through a per-level lift.
                (
                    Cell::Function {
                        name: left_name,
                        children: left_children,
                    },
                    Cell::Function {
                        name: right_name,
                        children: right_children,
                    },
                ) => {
                    if left_name != right_name || left_children.len() != right_children.len() {
                        return false;
                    }
                    self.union(left, right);
                    work.extend(left_children.into_iter().zip(right_children));
                }
                (
                    Cell::Tuple {
                        children: left_items,
                    },
                    Cell::Tuple {
                        children: right_items,
                    },
                ) => {
                    if left_items.len() != right_items.len() {
                        return false;
                    }
                    self.union(left, right);
                    work.extend(left_items.into_iter().zip(right_items));
                }
                // Distinct constructor kinds — a function against a tuple, or a leaf against
                // either — do not unify.
                _ => return false,
            }
        }
        true
    }

    /// Whether the solved graph is acyclic — the occurs check (§11.1), run once over the whole
    /// binding rather than per bind (which keeps the algorithm near-linear). A class whose shape
    /// reaches itself is an infinite term with no home. Three-colour DFS, iterative (§13).
    fn occurs_check_holds(&mut self) -> bool {
        let count = self.cell.len();
        let mut colour = vec![Colour::White; count];
        for start in 0..count {
            if self.find(start) != start || colour[start] != Colour::White {
                continue;
            }
            let mut stack = vec![Descent::Enter(start)];
            while let Some(step) = stack.pop() {
                match step {
                    Descent::Enter(node) => {
                        if colour[node] != Colour::White {
                            continue;
                        }
                        colour[node] = Colour::Grey;
                        stack.push(Descent::Leave(node));
                        for child in self.shape_children(node) {
                            let rep = self.find(child);
                            match colour[rep] {
                                Colour::Grey => return false,
                                Colour::White => stack.push(Descent::Enter(rep)),
                                Colour::Black => {}
                            }
                        }
                    }
                    Descent::Leave(node) => colour[node] = Colour::Black,
                }
            }
        }
        true
    }

    /// The child node indices of a representative's shape (§11.1); empty for a variable or a
    /// symbol leaf.
    fn shape_children(&self, node: usize) -> Vec<usize> {
        match &self.cell[node] {
            Cell::Function { children, .. } | Cell::Tuple { children } => children.clone(),
            Cell::Variable(_) | Cell::Leaf(_) => Vec::new(),
        }
    }

    /// Read the triangular substitution out of the solved graph (§11.1). Each class is named by
    /// its canonical variable — the `Ord`-least variable member — so a binding references a
    /// class by that name rather than inlining it, keeping the result triangular; a class with
    /// no variable member is inlined structurally where a binding reaches it.
    fn read_out(&mut self) -> Substitution {
        let count = self.cell.len();
        let mut variables: Vec<(usize, Variable)> = Vec::new();
        for id in 0..count {
            if let Cell::Variable(variable) = &self.cell[id] {
                variables.push((id, variable.clone()));
            }
        }
        let mut canonical: HashMap<usize, Variable> = HashMap::new();
        for (id, variable) in &variables {
            let rep = self.find(*id);
            canonical
                .entry(rep)
                .and_modify(|least| {
                    if variable < least {
                        *least = variable.clone();
                    }
                })
                .or_insert_with(|| variable.clone());
        }
        let mut pairs: Vec<(Variable, Binding)> = Vec::new();
        for (id, variable) in &variables {
            let rep = self.find(*id);
            let least = canonical[&rep].clone();
            if *variable != least {
                // A non-canonical variable maps to its class's canonical variable.
                pairs.push((variable.clone(), Binding::Bound(Term::Variable(least))));
            } else if !matches!(self.cell[rep], Cell::Variable(_)) {
                // The canonical variable of a class carrying a shape maps to that shape's term.
                let term = self.class_term(rep, &canonical);
                pairs.push((variable.clone(), Binding::Bound(term)));
            }
            // A pure-variable class's canonical variable is the free representative: no binding.
        }
        if pairs.is_empty() {
            Substitution::empty()
        } else {
            Substitution::from_pairs(pairs)
        }
    }

    /// The term a class denotes, for the read-out (§11.1): its shape with each child class
    /// referenced by its canonical variable (triangular — no inline) or, lacking one, inlined
    /// structurally. Iterative in depth (§13); acyclic by the occurs check, so it terminates.
    fn class_term(&mut self, root: usize, canonical: &HashMap<usize, Variable>) -> Term {
        let mut work = vec![Reconstruct::Enter(root)];
        let mut done: Vec<Term> = Vec::new();
        while let Some(step) = work.pop() {
            match step {
                Reconstruct::Enter(node) => {
                    let rep = self.find(node);
                    // A child class named by a variable resolves to it (triangular, no inline);
                    // the root itself resolves to its own shape.
                    if rep != root
                        && let Some(variable) = canonical.get(&rep)
                    {
                        done.push(Term::Variable(variable.clone()));
                        continue;
                    }
                    match self.cell[rep].clone() {
                        Cell::Variable(variable) => done.push(Term::Variable(variable)),
                        Cell::Leaf(symbol) => done.push(Term::Symbolic(symbol)),
                        Cell::Function { name, children } => {
                            work.push(Reconstruct::AssembleFunction(name, children.len()));
                            for child in children.into_iter().rev() {
                                work.push(Reconstruct::Enter(child));
                            }
                        }
                        Cell::Tuple { children } => {
                            work.push(Reconstruct::AssembleTuple(children.len()));
                            for child in children.into_iter().rev() {
                                work.push(Reconstruct::Enter(child));
                            }
                        }
                    }
                }
                Reconstruct::AssembleFunction(name, arity) => {
                    let arguments = done.split_off(done.len() - arity);
                    done.push(Term::Function { name, arguments });
                }
                Reconstruct::AssembleTuple(arity) => {
                    let items = done.split_off(done.len() - arity);
                    done.push(Term::Tuple(items));
                }
            }
        }
        done.pop().expect("the class term").canonicalize()
    }
}

#[cfg(test)]
mod tests {
    use super::{Fresh, Substitution, substitute};
    use crate::program::{
        Atom, Body, BodyElement, Comparison, DefaultNegation, Head, Literal, LiteralInner, Program,
        Query, Relation, Rule, Statement,
    };
    use crate::provenance::{Origin, Provenance, WithProvenance};
    use crate::symbol::{Name, Sign, Symbol, VarName};
    use crate::term::{BinaryOp, Term, UnaryOp, Variable};

    // ---- Small builders for the laws ----

    fn var(name: &str) -> Variable {
        Variable::Named(VarName::new(name).expect("a valid variable name"))
    }
    fn tvar(name: &str) -> Term {
        Term::Variable(var(name))
    }
    fn name(text: &str) -> Name {
        Name::new(text).expect("a valid identifier")
    }
    fn func(functor: &str, arguments: Vec<Term>) -> Term {
        Term::Function {
            name: name(functor),
            arguments,
        }
    }
    fn num(value: i32) -> Term {
        Term::from(value)
    }
    fn ground_function(functor: &str, arguments: Vec<Symbol>) -> Symbol {
        Symbol::Function {
            name: name(functor),
            arguments,
            sign: Sign::Positive,
        }
    }
    fn pairs(bindings: impl IntoIterator<Item = (Variable, Term)>) -> Substitution {
        Substitution::from_pairs(
            bindings
                .into_iter()
                .map(|(v, t)| (v, super::Binding::Bound(t))),
        )
    }

    #[test]
    fn substitute_replaces_a_variable_and_rebuilds_the_term() {
        let s = pairs([(var("X"), num(1))]);

        // f(X, Y){X↦1} = f(1, Y): Y is unbound and stays; the function is non-ground, so
        // it is not collapsed.
        assert_eq!(
            func("f", vec![tvar("X"), tvar("Y")]).substitute(&s),
            func("f", vec![num(1), tvar("Y")]),
        );

        // f(X){X↦1} = Symbolic(f(1)): ground after the substitution, so it collapses (§5.1).
        assert_eq!(
            func("f", vec![tvar("X")]).substitute(&s),
            Term::Symbolic(ground_function("f", vec![Symbol::Number(1)])),
        );

        // (X + 2){X↦1} = (1 + 2): a ground *operator* term never folds (§3.5).
        let x_plus_2 = Term::BinaryOperation {
            operator: BinaryOp::Add,
            left: Box::new(tvar("X")),
            right: Box::new(num(2)),
        };
        assert_eq!(
            x_plus_2.substitute(&s),
            Term::BinaryOperation {
                operator: BinaryOp::Add,
                left: Box::new(num(1)),
                right: Box::new(num(2)),
            },
        );
    }

    #[test]
    fn substitute_resolves_the_triangular_chain_to_the_fixpoint() {
        // {X↦f(Y), Y↦1}: X resolves to f(1), following the binding's own variables — a
        // single pass would leave f(Y). f(1) is ground, so it collapses.
        let s = pairs([(var("X"), func("f", vec![tvar("Y")])), (var("Y"), num(1))]);
        assert_eq!(
            tvar("X").substitute(&s),
            Term::Symbolic(ground_function("f", vec![Symbol::Number(1)])),
        );

        // A longer chain resolves fully: {X0↦f(X1), X1↦f(X2), X2↦1} takes X0 to f(f(1)).
        let chain = pairs([
            (var("X0"), func("f", vec![tvar("X1")])),
            (var("X1"), func("f", vec![tvar("X2")])),
            (var("X2"), num(1)),
        ]);
        let f_1 = ground_function("f", vec![Symbol::Number(1)]);
        assert_eq!(
            tvar("X0").substitute(&chain),
            Term::Symbolic(ground_function("f", vec![f_1])),
        );
    }

    #[test]
    fn the_empty_substitution_is_the_identity_on_canonical_terms() {
        let empty = Substitution::empty();
        for term in [
            tvar("X"),
            num(5),
            func("f", vec![tvar("Z")]),
            Term::BinaryOperation {
                operator: BinaryOp::Add,
                left: Box::new(tvar("X")),
                right: Box::new(num(1)),
            },
        ] {
            assert_eq!(term.clone().substitute(&empty), term);
        }
    }

    #[test]
    fn rule_substitution_rewrites_the_whole_rule() {
        // p(X) :- q(X). {X↦1} = p(1) :- q(1). — the substitution reaches both the head and the body.
        let rule = Rule::new(
            Atom::new(name("p"), [tvar("X")]),
            Atom::new(name("q"), [tvar("X")]),
        );

        let substituted = substitute(rule, &pairs([(var("X"), num(1))]));

        assert_eq!(
            substituted,
            Rule::new(
                Atom::new(name("p"), [num(1)]),
                Atom::new(name("q"), [num(1)]),
            ),
        );
    }

    #[test]
    fn rule_substitution_preserves_provenance() {
        // p(X) :- q(X). with a distinct annotation on the head and the body carrier.
        let head = WithProvenance::new(
            Head::Literal(Literal::from(Atom::new(name("p"), [tvar("X")]))),
            Provenance::from(Origin::Constructed).with_doc("the head"),
        );
        let body = WithProvenance::new(
            Body::new([BodyElement::from(Atom::new(name("q"), [tvar("X")]))]),
            Provenance::from(Origin::Constructed).with_doc("the body"),
        );
        let rule = Rule::from_nodes(head, body);

        let substituted = substitute(rule, &pairs([(var("X"), num(1))]));

        // The head and body carriers keep their provenance.
        assert!(
            substituted
                .head()
                .provenance()
                .annotations()
                .doc()
                .any(|doc| doc == "the head"),
        );
        assert!(
            substituted
                .body()
                .provenance()
                .annotations()
                .doc()
                .any(|doc| doc == "the body"),
        );
    }

    #[test]
    fn fresh_names_collide_with_none_in_the_program() {
        // A program mentioning the variables X, Y and the predicates p, q.
        let rule = Rule::new(
            Atom::new(name("p"), [tvar("X")]),
            Atom::new(name("q"), [tvar("Y")]),
        );
        let program = Program::of([WithProvenance::constructed(Statement::Rule(rule))]);
        let mut fresh = Fresh::of(&program);

        let first = fresh.variable();
        assert_ne!(first, var("X"));
        assert_ne!(first, var("Y"));
        let second = fresh.variable();
        assert_ne!(first, second);

        let aux = fresh.predicate("p");
        assert_ne!(aux, name("p"));
        assert_ne!(aux, name("q"));
        let aux2 = fresh.predicate("p");
        assert_ne!(aux, aux2);
    }

    #[test]
    fn substitution_into_a_deeply_nested_term_does_not_overflow() {
        // A term nesting X ~200,000 levels deep, substituted stack-safely (§13).
        let mut term = tvar("X");
        for _ in 0..200_000 {
            term = Term::UnaryOperation {
                operator: UnaryOp::Negate,
                argument: Box::new(term),
            };
        }
        let result = term.substitute(&pairs([(var("X"), num(1))]));
        // X became 1 at the bottom, so the whole (still operator-nested) term is ground.
        assert!(result.is_ground());
    }

    // ---- Breadth: every term position of a rule, every statement of a program ----

    /// Raise a program from concrete syntax under the clingo dialect — a rich, compact way
    /// to build fixtures that reach every structural node (§8). The fixture must raise
    /// cleanly, so a diagnostic here is a broken fixture, not a tolerated corner.
    fn raised_program(text: &str) -> Program {
        use themelios_base::source::{Source, SourceId};
        use themelios_syntax::dialect::Dialect;
        use themelios_syntax::parse::parse;

        let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
        let raised = crate::raise::raise(&parse(&source, Dialect::Clingo));
        assert!(
            raised.diagnostics().is_empty(),
            "the breadth fixture raises cleanly: {:?}",
            raised.diagnostics(),
        );
        raised.program().clone()
    }

    fn rules_of(program: &Program) -> Vec<Rule> {
        program
            .statements()
            .filter_map(|statement| match statement.get() {
                Statement::Rule(rule) => Some(rule.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn substitute_reaches_every_ordinary_term_position() {
        // Rules whose only variable X occurs across every head and body shape but a theory
        // term: a disjunction, a choice with guards, a head aggregate, a rich body (a
        // literal, a comparison, a conditional literal, a function aggregate, a negated set
        // aggregate), the booleans, verum, and a constraint (falsum).
        let program = raised_program(
            "diss(X) | dist(X) :- p(X).\n\
             q(X) :- p(X), X < 9, cnd(X) : cndg(X), #count { X : ct(X) } >= 1, \
             3 { su(X); sv(X) : sw(X) } 5, not #sum { X : sm(X) } >= 0.\n\
             1 { cha(X) : chb(X) } 4 :- p(X).\n\
             2 #sum { X : hs(X) } 5 :- p(X).\n\
             tf(X) :- p(X), #true, #false.\n\
             #true :- p(X).\n\
             :- p(X).\n",
        );
        let s = pairs([(var("X"), num(1))]);
        let rules = rules_of(&program);
        assert!(
            rules.len() >= 6,
            "the fixture raised its rules: {}",
            rules.len()
        );
        for rule in rules {
            assert!(
                !substitute(rule, &s)
                    .variables()
                    .any(|variable| variable == &var("X")),
                "every ordinary occurrence of X is substituted",
            );
        }
    }

    #[test]
    fn substitution_does_not_descend_theory_terms() {
        // A theory atom carrying X in an ordinary argument, in an element's theory term, and
        // in an element's ordinary condition — in both head and body position.
        let program = raised_program(
            "th(X) :- &sum(X) { X : tt(X) } >= 0, p(X).\n\
             &sum(X) { X : tt(X) } >= 0 :- p(X).\n",
        );
        let s = pairs([(var("X"), num(1))]);
        for rule in rules_of(&program) {
            let substituted = substitute(rule, &s);
            // The theory term keeps X — a peer algebra this surface does not descend (§4.9).
            assert!(
                substituted
                    .variables()
                    .any(|variable| variable == &var("X")),
                "the theory term keeps X",
            );
            // The theory atom's ordinary argument, though, was substituted.
            let theory_atom = find_theory_atom(&substituted);
            assert!(
                theory_atom.arguments().all(|term| !is_named(term, "X")),
                "the theory atom's ordinary argument is substituted",
            );
            assert!(
                theory_atom
                    .elements()
                    .any(|element| element.get().terms().any(|theory_term| matches!(
                        theory_term,
                        crate::program::TheoryTerm::Variable(variable) if variable == &var("X")
                    ))),
                "the theory element's theory term keeps X",
            );
        }
    }

    fn is_named(term: &Term, text: &str) -> bool {
        matches!(term, Term::Variable(variable) if variable == &var(text))
    }

    fn find_theory_atom(rule: &Rule) -> &crate::program::TheoryAtom {
        if let Head::TheoryAtom(atom) = rule.head().get() {
            return atom;
        }
        for element in rule.body().get().elements() {
            if let BodyElement::TheoryAtom { atom, .. } = element.get() {
                return atom;
            }
        }
        panic!("the fixture rule carries a theory atom");
    }

    #[test]
    fn fresh_scans_every_statement_kind() {
        // A program reaching every statement family and, within a rule, every head and body
        // shape — so seeding sees the whole program's names, not only a rule's.
        let program = raised_program(
            "diss(X) | dist(X) :- p(X).\n\
             q(X) :- p(X), X < 9, cnd(X) : cndg(X), #count { X : ct(X) } >= 1, \
             3 { su(X); sv(X) : sw(X) } 5, not #sum { X : sm(X) } >= 0.\n\
             1 { chd(X) : che(X) } 4 :- p(X).\n\
             2 #sum { X : hs(X) } 5 :- p(X).\n\
             th(X) :- &sum(X) { X : tt(X) } >= 0, p(X).\n\
             &sum(X) { X : tt(X) } >= 0 :- p(X).\n\
             bt(X) :- p(X), #true, #false.\n\
             #true :- p(X).\n\
             :- p(X).\n\
             :~ p(X). [X@1, X]\n\
             #minimize { X@1, X : mn(X) }.\n\
             #maximize { X : mx(X) }.\n\
             #show.\n\
             #show p/1.\n\
             #show shterm.\n\
             #show shf(X) : shg(X).\n\
             #project q/2.\n\
             #project pr(X) : ps(X).\n\
             #defined d/1.\n\
             #edge (ea, eb) : ec(X).\n\
             #heuristic he(X) : hc(X). [X@1, true]\n\
             #external ex(X) : ecx(X).\n\
             #const co = 1 + 2.\n\
             #include \"foo.lp\".\n\
             #theory tdef { }.\n",
        );
        let mut fresh = Fresh::of(&program);

        // The mints avoid the program's names — its variable X and its predicate p, both
        // reached through this whole-program scan — and a run of mints stays distinct.
        let minted_variable = fresh.variable();
        assert_ne!(minted_variable, var("X"));
        assert_ne!(fresh.variable(), minted_variable);
        let minted_predicate = fresh.predicate("p");
        assert_ne!(minted_predicate, name("p"));
        assert_ne!(fresh.predicate("p"), minted_predicate);
    }

    #[test]
    fn substitute_rebuilds_every_term_former() {
        // A tuple gathering the remaining formers, each carrying X: a pool, an interval, an
        // absolute, an external call, and a bitwise complement.
        let former = Term::Tuple(vec![
            Term::Pool(vec![tvar("X"), num(2)]),
            Term::Interval {
                lower: Box::new(tvar("X")),
                upper: Box::new(num(9)),
            },
            Term::Absolute(Box::new(tvar("X"))),
            Term::External {
                name: name("ext"),
                arguments: vec![tvar("X")],
            },
            Term::UnaryOperation {
                operator: UnaryOp::BitwiseNot,
                argument: Box::new(tvar("X")),
            },
        ]);
        let result = former.substitute(&pairs([(var("X"), num(1))]));
        assert!(
            !result
                .subterms()
                .any(|subterm| matches!(subterm, Term::Variable(v) if v == &var("X"))),
            "every former's X is substituted",
        );
    }

    #[test]
    fn substitute_rebuilds_a_multi_step_comparison() {
        // 1 < X < 9 — a chain of more than one step; substituting X leaves no free X.
        let comparison =
            Comparison::new(num(1), Relation::Lt, tvar("X")).chain(Relation::Lt, num(9));
        let rule = Rule::new(
            Atom::new(name("mc"), [tvar("X")]),
            Body::new([BodyElement::Literal(Literal {
                negation: DefaultNegation::None,
                inner: LiteralInner::Comparison(WithProvenance::constructed(comparison)),
            })]),
        );
        assert!(
            !substitute(rule, &pairs([(var("X"), num(1))]))
                .variables()
                .any(|variable| variable == &var("X")),
        );
    }

    #[test]
    fn a_substitution_iterates_its_bindings_in_order() {
        let s = pairs([(var("B"), num(2)), (var("A"), num(1))]);
        let variables: Vec<_> = s.iter().map(|(variable, _)| variable.clone()).collect();
        assert_eq!(variables, vec![var("A"), var("B")]); // Ord order, not insertion order.
    }

    #[test]
    fn fresh_skips_a_name_already_taken() {
        // A program already mentioning the variable V0 and the predicate aux0, so the first
        // candidate of each kind collides and the mint advances past it.
        let rule = Rule::new(
            Atom::constant(name("aux0")),
            Atom::new(name("p"), [tvar("V0")]),
        );
        let program = Program::of([WithProvenance::constructed(Statement::Rule(rule))]);
        assert_ne!(Fresh::of(&program).variable(), var("V0"));
        assert_ne!(Fresh::of(&program).predicate("aux"), name("aux0"));
    }

    #[test]
    fn fresh_scans_a_query() {
        let program = Program::of([WithProvenance::constructed(Statement::Query(Query::new(
            Atom::new(name("qp"), [tvar("X")]),
        )))]);
        let mut fresh = Fresh::of(&program);
        assert_ne!(fresh.variable(), var("X"));
        assert_ne!(fresh.predicate("qp"), name("qp"));
    }
}
