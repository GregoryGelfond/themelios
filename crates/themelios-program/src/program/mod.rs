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
mod directive;
mod rule;

pub use aggregate::{
    Aggregate, AggregateFunction, BodyAggregateElement, Direction, FunctionAggregate, Guard,
    HasGuards, HeadAggregate, HeadAggregateElement, Optimize, OptimizeElement, SetAggregate,
    SetElement, Weight, weight,
};
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
/// (`Default` and every `of`), so `base` is total and the empty program has one form.
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
    /// Build a program by admitting statements into the base part through the one ingest
    /// door (§6.3): each is canonicalized and merged with any content-equal statement
    /// already present. The design leaves the program's public constructor to the
    /// construction surface (§7) and the raise (§8); this names the door they build on.
    /// `Program::of([])` is the empty program, equal to `Program::default()`.
    pub fn of(statements: impl IntoIterator<Item = WithProvenance<Statement>>) -> Program {
        let mut program = Program::default();
        let base = program
            .parts
            .get_mut(&base_key())
            .expect("`Default` seeds the base part");
        for statement in statements {
            ingest(&mut base.statements, statement);
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

    /// Every statement, owned, paired with the key of the part it belongs to — the
    /// consuming complement to [`statements`](Program::statements), for a by-value rewrite
    /// that rebuilds the program part by part (§9).
    pub(crate) fn into_statements(
        self,
    ) -> impl Iterator<Item = (PartKey, WithProvenance<Statement>)> {
        self.parts.into_iter().flat_map(|(key, part)| {
            part.statements
                .into_iter()
                .map(move |statement| (key.clone(), statement))
        })
    }

    /// Admit a statement into the named part through the one ingest door (§6.3),
    /// opening the part with its first statement when it is not yet present — the
    /// part-structured door the raise lifts a `#program` delimiter into (§4.1, §8).
    /// `base` is seeded at construction; every other part is opened by a statement
    /// joining it. Crate-internal: the public doors are `of` (§7) and the raise (§8).
    pub(crate) fn ingest_into(&mut self, key: PartKey, statement: WithProvenance<Statement>) {
        let part = self.parts.entry(key.clone()).or_insert_with(|| Part {
            key,
            statements: BTreeSet::new(),
        });
        ingest(&mut part.statements, statement);
    }
}

/// The `base` part's key — the implicit default part (§4.1).
pub(crate) fn base_key() -> PartKey {
    PartKey {
        name: Name::new("base").expect("base is a valid identifier"),
        formals: Vec::new(),
    }
}

/// The one ingest/merge door (§6.3): canonicalize the statement, then admit it into the
/// part's set through the provenance-merging insert. This is the only path that mutates a
/// part's set, so the preservation law is structural.
fn ingest(set: &mut BTreeSet<WithProvenance<Statement>>, statement: WithProvenance<Statement>) {
    merge_insert(set, statement.map(canonicalize_statement));
}

/// Admit a provenance-carrying node into a set, **unioning** provenance with any
/// content-equal node already present (§6.3) — a raw `BTreeSet::insert` of a content-equal
/// node keeps the existing one and drops the newcomer's provenance, and its symmetric
/// `collect` keeps the first and drops the rest. Generic, so the one merge rule serves the
/// statement set and every set-shaped child a canonicalization re-collects (§6.2).
pub(crate) fn merge_insert<T: Ord>(set: &mut BTreeSet<WithProvenance<T>>, node: WithProvenance<T>) {
    let admitted = match set.take(&node) {
        Some(existing) => {
            let provenance = existing
                .provenance()
                .clone()
                .merge(node.provenance().clone());
            WithProvenance::new(node.into_value(), provenance)
        }
        None => node,
    };
    set.insert(admitted);
}

/// Collect provenance-carrying nodes into a set through [`merge_insert`], so a content-equal
/// collision **unions** provenance rather than dropping it (§6.3). The set-shaped children's
/// canonicalization re-collect uses this, not a raw `collect`.
pub(crate) fn merge_collect<T: Ord>(
    nodes: impl IntoIterator<Item = WithProvenance<T>>,
) -> BTreeSet<WithProvenance<T>> {
    let mut set = BTreeSet::new();
    for node in nodes {
        merge_insert(&mut set, node);
    }
    set
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
    use super::{WithProvenance, merge_collect};
    use crate::provenance::{Origin, Provenance, TransformTag};

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
