//! The predicate dependency graph (docs/design/analysis.md §4): a directed graph
//! over a program's predicate signatures, an edge from a head predicate to each
//! predicate its rule's body depends on, tagged by *how* it depends (the reused
//! `DependencyKind`, program §12.1). The core structural object safety's negative
//! half (§5) and every recursion class (§6) are read from.
//!
//! The graph is over predicate **signatures**, not ground atoms — a sound
//! over-approximation of the ground dependency graph, read *before* grounding (§4):
//! every ground dependency is an instance of a predicate dependency, so a program
//! the predicate graph proves acyclic-positive grounds tight, while a positive
//! predicate cycle may or may not, which is where a verdict becomes `Unknown` (§6).
//! The strong sign is part of the node: `p` and `-p` are distinct predicates
//! (grammar §5.2) — different atoms in an answer set — so folding them would make
//! the graph, and every class read off it, unsound.
//!
//! The edges are collected by one walk of the rules through the program tier's
//! structural accessors (program §12.1): `Rule::head_signatures` for the derived
//! predicates, `Rule::body_signatures` for the dependencies — the latter already
//! tagged by `DependencyKind` and already reaching a predicate inside a *condition*
//! (a conditional literal's, a disjunction or choice element's, an aggregate or
//! optimize element's), one pair per mode an occurrence carries: a predicate inside
//! a *negated* aggregate arrives as both `ThroughAggregate` and `Negative`, and the
//! graph records each. The strongly-connected-components decomposition (§4) follows.

use std::collections::{BTreeMap, BTreeSet};

use themelios_program::program::{Program, Statement};

// Three types reused from the program tier rather than redefined — the one
// authority for each (program §4, §12.1): `Signature` (the node identity),
// `Rule` (the owned rule a §6.3 witness carries), and `DependencyKind` (the
// edge tag, read at the substrate by `body_signatures`), re-exported so a
// consumer of this crate reads them here (analysis §4).
pub use themelios_program::analyze::DependencyKind;
pub use themelios_program::program::Rule;
pub use themelios_program::symbol::Signature;

/// The predicate dependency graph (§4). Nodes are predicate signatures — `p` and
/// its strong negation `-p` distinct (grammar §5.2); edges are tagged by
/// `DependencyKind`, one per mode a dependency runs through. Equality is structural.
/// (The strongly-connected-components decomposition, §4, follows.)
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DependencyGraph {
    /// Every head and body predicate signature — the nodes, in `Signature` order.
    nodes: BTreeSet<Signature>,
    /// The tagged out-edges of each predicate that has any; a predicate reached by
    /// more than one kind holds one edge per kind. A predicate with no out-edge has
    /// no entry (it stays a node), so equality is canonical.
    edges: BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
}

impl DependencyGraph {
    /// The predicate dependency graph of a program (§4): one walk of its rules,
    /// each contributing an edge from every predicate it derives to every predicate
    /// it depends on, tagged. `O(rules · body size)`.
    pub fn of(program: &Program) -> DependencyGraph {
        let mut graph = DependencyGraph {
            nodes: BTreeSet::new(),
            edges: BTreeMap::new(),
        };
        for statement in program.statements() {
            if let Statement::Rule(rule) = statement.get() {
                graph.add_rule(rule);
            }
        }
        graph
    }

    /// Add a rule's nodes and tagged edges (§4): every predicate it derives
    /// (`head_signatures`) and every predicate it depends on (`body_signatures`,
    /// already tagged by kind and already reaching predicates inside conditions) is a
    /// node, and each derived predicate gets one edge to each `(kind, dependency)`.
    /// A directive derives nothing, so only rules contribute (analysis §4).
    fn add_rule(&mut self, rule: &Rule) {
        let heads: Vec<Signature> = rule.head_signatures().collect();
        let dependencies: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
        for head in &heads {
            self.nodes.insert(head.clone());
        }
        for (_, dependency) in &dependencies {
            self.nodes.insert(dependency.clone());
        }
        for head in &heads {
            for (kind, dependency) in &dependencies {
                self.edges
                    .entry(head.clone())
                    .or_default()
                    .insert((*kind, dependency.clone()));
            }
        }
    }

    /// The predicate signatures — the graph's nodes, in `Signature` order (§4). `p`
    /// and `-p` are distinct.
    pub fn predicates(&self) -> impl Iterator<Item = &Signature> {
        self.nodes.iter()
    }

    /// The edges out of a predicate, each tagged by kind (§4). A predicate reached
    /// by more than one kind yields one edge per kind. `O(out-degree)` after an
    /// `O(log predicates)` lookup.
    pub fn edges_from(
        &self,
        from: &Signature,
    ) -> impl Iterator<Item = (DependencyKind, &Signature)> {
        self.edges
            .get(from)
            .into_iter()
            .flatten()
            .map(|(kind, signature)| (*kind, signature))
    }

    /// The **positive dependency graph** — this graph's nodes with only its
    /// `Positive` edges (§4). A first-class projection, the same type; tightness and
    /// head-cycle-freeness are read off it. `O(nodes + positive edges)`.
    #[must_use]
    pub fn positive(&self) -> DependencyGraph {
        let mut edges: BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>> = BTreeMap::new();
        for (from, targets) in &self.edges {
            for (kind, to) in targets {
                if *kind == DependencyKind::Positive {
                    edges
                        .entry(from.clone())
                        .or_default()
                        .insert((*kind, to.clone()));
                }
            }
        }
        DependencyGraph {
            nodes: self.nodes.clone(),
            edges,
        }
    }
}
