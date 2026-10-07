//! The `Program` value: a part-structured set of rules and directives (docs/design/
//! program.md §4). Every structural node is a *content* value paired with the
//! provenance carrier `WithProvenance<T>` (§6.2), and a container field holding such
//! a node holds `WithProvenance<Child>`; the content types derive their identity
//! **over content**, and the carrier erases provenance from it (§5, §6.2). Each
//! content type is grammar-bounded — it does not self-nest — so a derived `Ord`
//! descends a bounded number of levels and bottoms out in `Term`'s iterative one
//! (§13); only `Term`, `Symbol`, and `TheoryTerm` are self-recursive.
//!
//! `program` is a directory of private submodules under one public module (§1); the
//! public surface is re-exported here.

mod aggregate;
mod counted;
mod directive;
mod rule;

pub use aggregate::{
    Aggregate, AggregateFunction, BodyAggregateElement, Direction, FunctionAggregate, Guard,
    HasGuards, HeadAggregate, HeadAggregateElement, Optimize, OptimizeElement, SetAggregate,
    SetElement, Weight, weight,
};
pub use counted::Identity;
pub use directive::{
    Const, ConstPolicy, Defined, Edge, External, Heuristic, Include, IncludeTarget, Project,
    Script, Show, TheoryAtom, TheoryAtomDefinition, TheoryAtomGuardDefinition, TheoryDefinition,
    TheoryElement, TheoryGuard, TheoryOccurrence, TheoryOperator, TheoryOperatorArity,
    TheoryOperatorDefinition, TheoryTerm, TheoryTermDefinition, TheoryTermParts,
};
pub use rule::{
    Arguments, Atom, Body, BodyElement, Choice, ChoiceElement, Comparison, Condition,
    ConditionalLiteral, DefaultNegation, Disjunction, DisjunctionElement, Head, IntoBody, IntoHead,
    Literal, LiteralInner, Relation, Rule, WeakConstraint,
};

use std::collections::{BTreeMap, BTreeSet};

use crate::provenance::WithProvenance;
use crate::symbol::Name;

/// A statement of a part (grammar §5.11), plus the ASP-Core-2 query (grammar §6.1).
/// Non-exhaustive for downstream growth; every internal match is exhaustive with no
/// wildcard, so a new family is a compile error here, never a silent drop (§4.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Statement {
    /// A rule.
    Rule(Rule),
    /// A weak constraint.
    WeakConstraint(WeakConstraint),
    /// An optimization statement.
    Optimize(Optimize),
    /// A `#show`.
    Show(Show),
    /// A `#project`.
    Project(Project),
    /// A `#defined`.
    Defined(Defined),
    /// An `#edge`.
    Edge(Edge),
    /// A `#heuristic`.
    Heuristic(Heuristic),
    /// An `#external`.
    External(External),
    /// A `#const`.
    Const(Const),
    /// An `#include`, parsed and never resolved (§4.8).
    Include(Include),
    /// A `#script`, carried opaque and never run (§4.8).
    Script(Script),
    /// A `#theory` definition.
    TheoryDefinition(TheoryDefinition),
    /// An ASP-Core-2 query (grammar §6.1).
    Query(Query),
}

impl Statement {
    /// A **global definition** — a `#const` or a `#theory` (§4.2): a statement the authority
    /// gathers before it instantiates anything, so its position is free (the render's leading
    /// block, §10), and binds by name at most once, so a repeat is a redefinition (§6.3, §8).
    /// The one classification both read. Total; O(1).
    pub fn is_global_definition(&self) -> bool {
        match self {
            Statement::Const(_) | Statement::TheoryDefinition(_) => true,
            Statement::Rule(_)
            | Statement::WeakConstraint(_)
            | Statement::Optimize(_)
            | Statement::Show(_)
            | Statement::Project(_)
            | Statement::Defined(_)
            | Statement::Edge(_)
            | Statement::Heuristic(_)
            | Statement::External(_)
            | Statement::Include(_)
            | Statement::Script(_)
            | Statement::Query(_) => false,
        }
    }
}

// ---- The statement coercion class closes by a rule (§4.2, §7.1) ----
//
// For every statement family `X`: `From<X> for Statement`, its variant wrapped as built.
// No pass runs here — canonicalization is the ingest door's (§6.3) — so each is O(1) and
// total. These are the intended construction path for a downstream that need not name
// the variants of a `#[non_exhaustive]` sum, and the class `Program::of` admits: a
// `Statement` itself passes by the reflexive `From`, so mixed families meet there.

impl From<Rule> for Statement {
    /// A rule is a statement (§4.2).
    fn from(rule: Rule) -> Statement {
        Statement::Rule(rule)
    }
}

impl From<WeakConstraint> for Statement {
    /// A weak constraint is a statement (§4.2).
    fn from(weak: WeakConstraint) -> Statement {
        Statement::WeakConstraint(weak)
    }
}

impl From<Optimize> for Statement {
    /// An optimization statement is a statement (§4.2).
    fn from(optimize: Optimize) -> Statement {
        Statement::Optimize(optimize)
    }
}

impl From<Show> for Statement {
    /// A `#show` is a statement (§4.2).
    fn from(show: Show) -> Statement {
        Statement::Show(show)
    }
}

impl From<Project> for Statement {
    /// A `#project` is a statement (§4.2).
    fn from(project: Project) -> Statement {
        Statement::Project(project)
    }
}

impl From<Defined> for Statement {
    /// A `#defined` is a statement (§4.2).
    fn from(defined: Defined) -> Statement {
        Statement::Defined(defined)
    }
}

impl From<Edge> for Statement {
    /// An `#edge` is a statement (§4.2).
    fn from(edge: Edge) -> Statement {
        Statement::Edge(edge)
    }
}

impl From<Heuristic> for Statement {
    /// A `#heuristic` is a statement (§4.2).
    fn from(heuristic: Heuristic) -> Statement {
        Statement::Heuristic(heuristic)
    }
}

impl From<External> for Statement {
    /// An `#external` is a statement (§4.2).
    fn from(external: External) -> Statement {
        Statement::External(external)
    }
}

impl From<Const> for Statement {
    /// A `#const` is a statement (§4.2).
    fn from(constant: Const) -> Statement {
        Statement::Const(constant)
    }
}

impl From<Include> for Statement {
    /// An `#include` is a statement (§4.2), parsed and never resolved (§4.8).
    fn from(include: Include) -> Statement {
        Statement::Include(include)
    }
}

impl From<Script> for Statement {
    /// A `#script` is a statement (§4.2), carried opaque and never run (§4.8).
    fn from(script: Script) -> Statement {
        Statement::Script(script)
    }
}

impl From<TheoryDefinition> for Statement {
    /// A `#theory` definition is a statement (§4.2).
    fn from(definition: TheoryDefinition) -> Statement {
        Statement::TheoryDefinition(definition)
    }
}

impl From<Query> for Statement {
    /// An ASP-Core-2 query is a statement (§4.2, grammar §6.1).
    fn from(query: Query) -> Statement {
        Statement::Query(query)
    }
}

/// An ASP-Core-2 query (grammar §6.1): the queried atom — the class of forms a program
/// position holds, so it belongs to the statement enum (§4.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Query {
    atom: WithProvenance<Atom>,
}

impl Query {
    /// A query over the given atom, carrying a `Constructed` origin (§6.2).
    pub fn new(atom: Atom) -> Query {
        Query {
            atom: WithProvenance::constructed(atom),
        }
    }

    /// A query over an already-provenanced atom — the raise's door, carrying the atom's
    /// parsed origin (§6.2, §8). Canonicalization runs at the ingest door (§6.3).
    pub(crate) fn from_nodes(atom: WithProvenance<Atom>) -> Query {
        Query { atom }
    }

    /// The queried atom, with its provenance (§6.2).
    pub fn atom(&self) -> &WithProvenance<Atom> {
        &self.atom
    }

    pub(crate) fn canonicalize(self) -> Query {
        Query {
            atom: self.atom.map(Atom::canonicalize),
        }
    }
}

/// A part's identity: its name and the **spelled** formal parameters (grammar §5.9's
/// `#program name(p, q)`), not its arity (§4.1). Two parts named `step(t)` and `step(u)`
/// therefore coexist rather than merge — merging would rename a formal and could capture
/// a global constant.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PartKey {
    /// The part name.
    pub name: Name,
    /// The spelled formal parameters.
    pub formals: Vec<Name>,
}

/// A part: a keyed set of statements (§4.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Part {
    key: PartKey,
    statements: BTreeSet<WithProvenance<Statement>>,
}

impl Part {
    /// The part's identity.
    pub fn key(&self) -> &PartKey {
        &self.key
    }

    /// The statements — a set, each with its provenance, in `Ord` order (§6.2).
    pub fn statements(&self) -> impl Iterator<Item = &WithProvenance<Statement>> {
        self.statements.iter()
    }
}

/// A part-structured set of statements, giving cheap part-wise access for multi-shot use
/// (§4.1). `base` is the implicit default part, always present — seeded at construction
/// (`Default`, `empty`, `of`, `of_nodes`, and `of_keyed_nodes`), so `base` is total and the
/// empty program has one form.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Program {
    parts: BTreeMap<PartKey, Part>,
}

impl Default for Program {
    /// The empty program: the base part present and empty (§4.1). Hand-written rather than
    /// derived so the "base always present" invariant holds for `Program::default()` too —
    /// a derived `Default` would leave an empty part-map and make `base()` panic.
    fn default() -> Program {
        let mut parts = BTreeMap::new();
        parts.insert(
            base_key(),
            Part {
                key: base_key(),
                statements: BTreeSet::new(),
            },
        );
        Program { parts }
    }
}

impl Program {
    /// The empty program — the base part present and empty (§4.1, §7.1): the named empty
    /// case, as [`Body::empty`](crate::program::Body::empty) and
    /// [`Condition::empty`](crate::program::Condition::empty) give theirs. Equal to
    /// `Program::default()`. Total.
    pub fn empty() -> Program {
        Program::default()
    }

    /// Build a program from bare statements (§7.1): each value becomes a [`Statement`]
    /// through its `From` — a `Statement` itself by the reflexive one — is given a
    /// `Constructed` origin (§6.2), and is admitted into the base part through
    /// [`of_nodes`](Program::of_nodes), so the one ingest door (§6.3) canonicalizes it
    /// and merges it with any content-equal statement already present. The collection
    /// is homogeneous: two families meet at `Statement::from`. The empty program is
    /// [`empty`](Program::empty) (or `Program::default()`), not `of` over a bare `[]`,
    /// whose element type this generic door cannot infer.
    ///
    /// ```
    /// use themelios_program::prelude::*;
    ///
    /// let p = Rule::fact(Atom::constant(Name::new("p").expect("a valid identifier")));
    /// let q = Rule::fact(Atom::constant(Name::new("q").expect("a valid identifier")));
    /// let program = Program::of([p, q]);
    /// assert_eq!(program.base().statements().count(), 2);
    /// ```
    pub fn of(statements: impl IntoIterator<Item = impl Into<Statement>>) -> Program {
        Program::of_nodes(
            statements
                .into_iter()
                .map(|statement| WithProvenance::constructed(statement.into())),
        )
    }

    /// Build a program from statements that carry their provenance — the public door for
    /// a caller holding a `Parsed` or `Transformed` origin (§6.2), which the bare
    /// [`of`](Program::of) would replace with a fresh `Constructed` one. Each is admitted
    /// into the base part through the one ingest door (§6.3): canonicalized, and merged
    /// with any content-equal statement already present, the provenances unioned. The
    /// design leaves the program's public constructor to the construction surface (§7)
    /// and the raise (§8); this names the door they build on.
    pub fn of_nodes(statements: impl IntoIterator<Item = WithProvenance<Statement>>) -> Program {
        let mut program = Program::default();
        program.ingest_run(&base_key(), statements);
        program
    }

    /// Build a program from `(part, node)` pairs — the multi-part analogue of
    /// [`of_nodes`](Program::of_nodes), which fills `base` only. A *node* is a
    /// [`WithProvenance<Statement>`](crate::provenance::WithProvenance), exactly as in
    /// `of_nodes`; each pair places its node in the part its [`PartKey`] names, opening the
    /// part with its first statement. Each statement is admitted through the one ingest door
    /// (§6.3) — canonicalized, and merged with any content-equal statement **already present in
    /// the same part**, provenances unioned; content-equal statements under **different** keys
    /// stay distinct, one per part (part identity, §4.1). The base part is always present.
    /// Total; `O(Σ statement sizes + parts)`.
    ///
    /// This is the programmatic multi-part door for a structural client — the solve tier's agent
    /// rebuilding a knowledge base, or a code generator. An ASP author writes parts as `#program`
    /// text through the macros, [`of`](Program::of), or the raise (§8), and does not assemble
    /// `(PartKey, _)` pairs by hand.
    pub fn of_keyed_nodes(
        keyed_nodes: impl IntoIterator<Item = (PartKey, WithProvenance<Statement>)>,
    ) -> Program {
        let mut program = Program::default();
        for (key, statement) in keyed_nodes {
            program.ingest_into(key, statement);
        }
        program
    }

    /// The parts, in `PartKey` order (§4.1).
    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        self.parts.values()
    }

    /// The part with the given key, if present.
    pub fn part(&self, key: &PartKey) -> Option<&Part> {
        self.parts.get(key)
    }

    /// The base part — always present, seeded at construction (§4.1). Total.
    pub fn base(&self) -> &Part {
        self.parts
            .get(&base_key())
            .expect("the base part is seeded at construction")
    }

    /// Every statement, across parts, each with its provenance (§6.2).
    pub fn statements(&self) -> impl Iterator<Item = &WithProvenance<Statement>> {
        self.parts.values().flat_map(Part::statements)
    }

    /// Each part's key and statements, owned, in `PartKey` order — the consuming complement
    /// to [`parts`](Program::parts), for a by-value rewrite that rebuilds the program part by
    /// part, each part one run (§9). O(1) per part.
    pub(crate) fn into_runs(
        self,
    ) -> impl Iterator<Item = (PartKey, BTreeSet<WithProvenance<Statement>>)> {
        self.parts
            .into_values()
            .map(|part| (part.key, part.statements))
    }

    /// Admit a statement into the named part through the one ingest door (§6.3),
    /// opening the part with its first statement when it is not yet present — the door
    /// for a statement that carries its own key, as `of_keyed_nodes`' do. `base` is seeded
    /// at construction; every other part is opened by a statement joining it.
    /// Crate-internal: of the public doors — `of`, `of_nodes`, and `of_keyed_nodes` (§7.1), and
    /// the raise (§8) — only `of_keyed_nodes` comes here; the others, the raise among them,
    /// collect through `ingest_run`.
    pub(crate) fn ingest_into(&mut self, key: PartKey, statement: WithProvenance<Statement>) {
        ingest(
            &mut self.part_entry(key).statements,
            [statement.map(canonicalize_statement)],
        );
    }

    /// Admit a run of statements into one part through the one ingest door (§6.3), looking
    /// the part up once for the run rather than once per statement — the collection's door
    /// for the raise, which shares one part key across a `#program` delimiter's statements
    /// (§8), and for the rewrites, which rebuild part by part. O(key · log parts) once, then
    /// each statement's ingest.
    pub(crate) fn ingest_run(
        &mut self,
        key: &PartKey,
        statements: impl IntoIterator<Item = WithProvenance<Statement>>,
    ) {
        self.ingest_canonical_run(
            key,
            statements
                .into_iter()
                .map(|statement| statement.map(canonicalize_statement)),
        );
    }

    /// Admit a run of statements already canonical (§5.1) into one part — [`ingest_run`]
    /// without its canonicalization, for the occurrence stream, whose statements are
    /// canonical by construction (§8). Canonicalization is idempotent, so the part holds
    /// what `ingest_run` would build from the same statements.
    ///
    /// [`ingest_run`]: Program::ingest_run
    pub(crate) fn ingest_canonical_run(
        &mut self,
        key: &PartKey,
        statements: impl IntoIterator<Item = WithProvenance<Statement>>,
    ) {
        ingest(&mut self.part_entry(key.clone()).statements, statements);
    }

    /// The part named `key`, opened empty when it is not yet present (§4.1). O(key · log
    /// parts).
    fn part_entry(&mut self, key: PartKey) -> &mut Part {
        self.parts.entry(key).or_insert_with_key(|key| Part {
            key: key.clone(),
            statements: BTreeSet::new(),
        })
    }
}

/// The `base` part's key — the implicit default part (§4.1).
pub(crate) fn base_key() -> PartKey {
    PartKey {
        name: Name::new("base").expect("base is a valid identifier"),
        formals: Vec::new(),
    }
}

/// The one ingest/merge door (§6.3): admit a run of canonical statements — each entry point
/// canonicalizes what it is handed, or holds statements canonical by construction — into the
/// part's set through the provenance merge: collected whole when it opens the part, which merges
/// exactly as admitting it statement by statement would, and statement by statement into a part
/// already holding some. This is the only path that mutates a part's set, so the preservation
/// law is structural.
fn ingest(
    set: &mut BTreeSet<WithProvenance<Statement>>,
    statements: impl IntoIterator<Item = WithProvenance<Statement>>,
) {
    if set.is_empty() {
        *set = merge_collect(statements);
    } else {
        for statement in statements {
            merge_insert(set, statement);
        }
    }
}

/// Admit a provenance-carrying node into a set, **unioning** provenance with any
/// content-equal node already present (§6.3) and keeping the newcomer's content with its
/// nested provenance — the later-written copy (§4.4). A raw `BTreeSet::insert` of a
/// content-equal node keeps the existing one and drops the newcomer's provenance, and its
/// `collect` keeps one of the equal copies — std promises no more — dropping every other's.
/// Generic, so the one merge rule serves the statement set and every set-shaped child a canonicalization re-collects (§6.2) — a
/// counted child merges through its own constructor (§4.4).
pub(crate) fn merge_insert<T: Ord>(set: &mut BTreeSet<WithProvenance<T>>, node: WithProvenance<T>) {
    // A new content is admitted in one search; a collision then takes the newcomer back out
    // and admits it with the displaced node's provenance unioned in. The take finds what
    // `replace` admitted because the order agrees with equality — §5.2's standing
    // precondition, held by each algebra's total-order laws.
    if let Some(existing) = set.replace(node) {
        let mut admitted = set
            .take(&existing)
            .expect("the content `replace` just admitted is present");
        // The union moves the accumulated provenance and extends it with the newcomer's —
        // large with small — so a run of n content-equal nodes costs O(n log n), not the
        // Θ(n²) of cloning the accumulation at every collision.
        admitted.absorb_earlier(existing);
        set.insert(admitted);
    }
}

/// Collect provenance-carrying nodes into a set exactly as [`merge_insert`] admitting them one
/// by one in order would, so a content-equal collision **unions** provenance rather than
/// dropping it (§6.3): each class of content-equal nodes keeps its last node's content, with
/// its nested provenance, and the union of the class's provenances. The positions are sorted
/// by content — stably, so each class stays in the nodes' own order, and by position, so a
/// large node is compared where it lies rather than moved — then each class folds into one
/// node, and the set is built from the ascending result. O(n log n) comparisons, against two
/// searches of a growing set per node. The set-shaped children's canonicalization
/// re-collect uses this, not a raw `collect`; a counted child uses its own constructor (§4.4).
pub(crate) fn merge_collect<T: Ord>(
    nodes: impl IntoIterator<Item = WithProvenance<T>>,
) -> BTreeSet<WithProvenance<T>> {
    let mut slots: Vec<Option<WithProvenance<T>>> = nodes.into_iter().map(Some).collect();
    let mut order: Vec<usize> = (0..slots.len()).collect();
    // The comparison is a total order — §5.2's standing precondition, held by each algebra's
    // total-order laws — which the sort requires.
    order.sort_by(|&left, &right| slots[left].cmp(&slots[right]));
    let mut folded: Vec<WithProvenance<T>> = Vec::with_capacity(slots.len());
    for position in order {
        let node = slots[position]
            .take()
            .expect("the sorted positions name each node once");
        match folded.last_mut() {
            Some(class) if *class == node => class.absorb_later(node),
            _ => folded.push(node),
        }
    }
    folded.into_iter().collect()
}

/// Canonicalize a statement (§5.1): the boolean-head fold, and the term-level collapse
/// (§3.6) across every term the statement reaches — an atom's arguments, a guard's bound,
/// a directive's terms. Idempotent and total. The match is exhaustive with no wildcard, so
/// a new statement family is a compile error here, never a silently un-canonicalized one.
/// The opaque regions (`#script`, `#include`) and the term-free directives (`#defined`,
/// `#theory`) carry nothing to collapse.
pub(crate) fn canonicalize_statement(statement: Statement) -> Statement {
    match statement {
        Statement::Rule(rule) => Statement::Rule(rule.canonicalize()),
        Statement::WeakConstraint(weak) => Statement::WeakConstraint(weak.canonicalize()),
        Statement::Optimize(optimize) => Statement::Optimize(optimize.canonicalize()),
        Statement::Show(show) => Statement::Show(show.canonicalize()),
        Statement::Project(project) => Statement::Project(project.canonicalize()),
        Statement::Edge(edge) => Statement::Edge(edge.canonicalize()),
        Statement::Heuristic(heuristic) => Statement::Heuristic(heuristic.canonicalize()),
        Statement::External(external) => Statement::External(external.canonicalize()),
        Statement::Const(constant) => Statement::Const(constant.canonicalize()),
        Statement::Query(query) => Statement::Query(query.canonicalize()),
        statement @ (Statement::Defined(_)
        | Statement::Include(_)
        | Statement::Script(_)
        | Statement::TheoryDefinition(_)) => statement,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::{WithProvenance, merge_collect, merge_insert};
    use crate::provenance::{Origin, Provenance, TransformTag};

    /// A probe node: content `value`, its own provenance `outer`, and a nested node whose
    /// provenance `inner` tells two content-equal copies apart — the nested provenance a
    /// statement's atoms carry (§6.3).
    type Probe = WithProvenance<WithProvenance<u8>>;

    fn probe((value, outer, inner): (u8, u8, u8)) -> Probe {
        let origin =
            |tag: u8| Provenance::from(Origin::Transformed(TransformTag::new(tag.to_string())));
        WithProvenance::new(WithProvenance::new(value, origin(inner)), origin(outer))
    }

    /// What a merge decides for each kept node: its content, its unioned provenance, and
    /// the nested provenance of the copy it kept.
    fn observed<'a>(
        nodes: impl IntoIterator<Item = &'a Probe>,
    ) -> Vec<(u8, Provenance, Provenance)> {
        nodes
            .into_iter()
            .map(|node| {
                let nested = node.get();
                (
                    *nested.get(),
                    node.provenance().clone(),
                    nested.provenance().clone(),
                )
            })
            .collect()
    }

    /// The merge rule of §6.3, stated naively: each node in turn either joins the kept node
    /// of equal content — its content replacing the kept one's, the provenances unioned — or
    /// is kept itself; the result in content order.
    fn naive_merge(nodes: impl IntoIterator<Item = Probe>) -> Vec<(u8, Provenance, Provenance)> {
        let mut kept: Vec<Probe> = Vec::new();
        for node in nodes {
            match kept.iter().position(|each| *each == node) {
                Some(at) => {
                    let earlier = kept.remove(at).provenance().clone();
                    let (value, later) = (node.get().clone(), node.provenance().clone());
                    kept.push(WithProvenance::new(value, earlier.merge(later)));
                }
                None => kept.push(node),
            }
        }
        kept.sort();
        observed(&kept)
    }

    proptest! {
        /// Collecting a run whole merges exactly as the rule states (§6.3): the same
        /// contents, each with the same unioned provenance and the same kept copy.
        #[test]
        fn collecting_a_run_merges_by_the_rule(
            run in proptest::collection::vec((0_u8..6, 0_u8..8, 0_u8..8), 0..48)
        ) {
            let collected = merge_collect(run.iter().copied().map(probe));
            prop_assert_eq!(observed(&collected), naive_merge(run.iter().copied().map(probe)));
        }

        /// Admitting a run node by node merges exactly as the rule states (§6.3), from an
        /// empty set or one already holding nodes.
        #[test]
        fn admitting_a_run_merges_by_the_rule(
            held in proptest::collection::vec((0_u8..6, 0_u8..8, 0_u8..8), 0..12),
            run in proptest::collection::vec((0_u8..6, 0_u8..8, 0_u8..8), 0..48)
        ) {
            let mut admitted: BTreeSet<Probe> = BTreeSet::new();
            for node in held.iter().chain(&run).copied().map(probe) {
                merge_insert(&mut admitted, node);
            }
            let expected = naive_merge(held.iter().chain(&run).copied().map(probe));
            prop_assert_eq!(observed(&admitted), expected);
        }
    }

    /// The provenance-merging collect the set-shaped children use unions provenance on a
    /// content collision, dropping nothing (§6.3) — the same law the statement door keeps,
    /// held at the generic helper (the raise exercises it on the nested sets, §8).
    #[test]
    fn merge_collect_unions_provenance_on_a_content_collision() {
        let origin = |tag: &str| Provenance::from(Origin::Transformed(TransformTag::new(tag)));
        let here = WithProvenance::new(7_i32, origin("here"));
        let there = WithProvenance::new(7_i32, origin("there"));
        let set = merge_collect([here, there]);
        assert_eq!(set.len(), 1, "the content-equal nodes collapse to one");
        let merged = set.iter().next().expect("one node");
        assert_eq!(
            merged.provenance().origins().count(),
            2,
            "both provenances are unioned"
        );
    }
}
