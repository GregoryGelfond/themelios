//! The predicate dependency graph (docs/design/analysis.md §4): a directed graph
//! over a program's predicate signatures, an edge from a head predicate to each
//! predicate its rule's body depends on, tagged by *how* it depends (the reused
//! `DependencyKind`, program §12.1), and its strongly-connected-components
//! decomposition. The core structural object safety's negative half (§5) and every
//! recursion class (§6) are read from.
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
//! graph records each.
//!
//! The strongly-connected-components decomposition (§4) is a **hand-rolled iterative
//! Tarjan** on an explicit work stack — a library implementation typically recurses
//! in graph depth and would breach the depth discipline (spec §5.2, program §13), so
//! a graph deep in its dependency chain is decomposed without overflowing the call
//! stack. It runs once at construction, `O(nodes + edges)`, and its natural output
//! order is **reverse-topological** — a component before every component that
//! depends on it, the bottom-up grounding order. The positive graph is decomposed
//! independently (its components are finer, dropping the non-monotone edges breaks
//! cycles).

use std::collections::{BTreeMap, BTreeSet};

use themelios_program::program::{Atom, Program, Statement};

// Three types reused from the program tier rather than redefined — the one
// authority for each (program §4, §12.1): `Signature` (the node identity),
// `Rule` (the owned rule a §6.3 witness carries), and `DependencyKind` (the
// edge tag, read at the substrate by `body_signatures`), re-exported so a
// consumer of this crate reads them here (analysis §4).
pub use themelios_program::analyze::DependencyKind;
pub use themelios_program::program::Rule;
pub use themelios_program::symbol::Signature;

/// The predicate signature of an atom — its sign, name, and arity, the node identity in
/// the dependency graph (§4). The crate-local atom→signature the facets share (safety's
/// §5, classify's §6), reading it the one way, matching the substrate's own (program §12.1).
pub(crate) fn atom_signature(atom: &Atom) -> Signature {
    Signature {
        sign: atom.sign,
        name: atom.name.clone(),
        // A predicate carries no more arguments than a `Vec` holds, far under `u32::MAX`
        // (the workspace `cast_possible_truncation` allowance).
        arity: atom.arguments.len() as u32,
    }
}

/// The predicate dependency graph (§4). Nodes are predicate signatures — `p` and
/// its strong negation `-p` distinct (grammar §5.2); edges are tagged by
/// `DependencyKind`, one per mode a dependency runs through; and the
/// strongly-connected components are decomposed once at construction. Equality is
/// structural.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DependencyGraph {
    /// Every head and body predicate signature — the nodes, in `Signature` order.
    nodes: BTreeSet<Signature>,
    /// The tagged out-edges of each predicate that has any; a predicate reached by
    /// more than one kind holds one edge per kind. A predicate with no out-edge has
    /// no entry (it stays a node), so equality is canonical.
    edges: BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
    /// The strongly-connected components, in reverse-topological order (§4).
    components: Vec<Component>,
    /// Each node's component, by index into `components` — the `component_of` lookup.
    component_index: BTreeMap<Signature, usize>,
}

impl DependencyGraph {
    /// The predicate dependency graph of a program (§4): one walk of its rules,
    /// each contributing an edge from every predicate it derives to every predicate
    /// it depends on, tagged, then the strongly-connected-components decomposition.
    /// `O(rules · body size + nodes + edges)`.
    pub fn of(program: &Program) -> DependencyGraph {
        let mut nodes = BTreeSet::new();
        let mut edges: BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>> = BTreeMap::new();
        for statement in program.statements() {
            if let Statement::Rule(rule) = statement.get() {
                collect_rule(rule, &mut nodes, &mut edges);
            }
        }
        DependencyGraph::from_parts(nodes, edges)
    }

    /// Assemble a graph from its nodes and tagged edges, decomposing the
    /// strongly-connected components once (§4). The one door `of` and `positive`
    /// share, so the full graph and its positive projection are decomposed the same
    /// way — the positive one independently, its components finer.
    fn from_parts(
        nodes: BTreeSet<Signature>,
        edges: BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
    ) -> DependencyGraph {
        let (components, component_index) = decompose(&nodes, &edges);
        DependencyGraph {
            nodes,
            edges,
            components,
            component_index,
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

    /// The strongly-connected components, in reverse-topological order — a component
    /// before every component that depends on it, the bottom-up grounding order (§4).
    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.components.iter()
    }

    /// The component a predicate belongs to, if it is a node (§4). `O(log predicates)`.
    pub fn component_of(&self, predicate: &Signature) -> Option<&Component> {
        self.component_index
            .get(predicate)
            .map(|&index| &self.components[index])
    }

    /// Whether the graph has no recursive component (§4) — a DAG of predicates.
    /// Tightness reads `graph.positive().is_acyclic()` (§6).
    pub fn is_acyclic(&self) -> bool {
        self.components
            .iter()
            .all(|component| !component.is_recursive())
    }

    /// The **positive dependency graph** — this graph's nodes with only its
    /// `Positive` edges, decomposed independently (§4): its components are finer,
    /// since dropping the non-monotone edges breaks cycles. A first-class projection,
    /// the same type; tightness and head-cycle-freeness are read off it.
    /// `O(nodes + positive edges)`.
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
        DependencyGraph::from_parts(self.nodes.clone(), edges)
    }
}

/// One strongly-connected component (§4): the mutually-recursive predicates, and how
/// the recursion within it runs — the facts the recursion classes read (§6). A
/// component of the *positive* graph is a positive cycle by construction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Component {
    /// The mutually-recursive predicates, in `Signature` order.
    members: BTreeSet<Signature>,
    /// The kinds present on the edges *internal* to the component (both endpoints in
    /// it) — empty exactly when the component is a single non-self-looping node. In a
    /// strongly-connected component every internal edge lies on a cycle, so a kind
    /// present is a cycle running through that kind.
    internal_edge_kinds: BTreeSet<DependencyKind>,
}

impl Component {
    /// The component's predicates, in `Signature` order (§4).
    pub fn members(&self) -> impl Iterator<Item = &Signature> {
        self.members.iter()
    }

    /// Whether the component has an internal cycle (§4) — more than one member, or a
    /// single member with a self-loop.
    pub fn is_recursive(&self) -> bool {
        self.members.len() > 1 || !self.internal_edge_kinds.is_empty()
    }

    /// Whether the component's recursion runs through a `Positive` edge (§4).
    pub fn has_positive_cycle(&self) -> bool {
        self.internal_edge_kinds.contains(&DependencyKind::Positive)
    }

    /// Whether the component's recursion runs through a `Negative` (default-negated)
    /// edge (§4) — the fact stratification reads.
    pub fn has_negative_cycle(&self) -> bool {
        self.internal_edge_kinds.contains(&DependencyKind::Negative)
    }

    /// Whether the component's recursion runs through a non-monotone aggregate edge
    /// (§4).
    pub fn has_aggregate_cycle(&self) -> bool {
        self.internal_edge_kinds
            .contains(&DependencyKind::ThroughAggregate)
    }
}

/// Add a rule's nodes and tagged edges (§4): every predicate it derives
/// (`head_signatures`) and every predicate it depends on (`body_signatures`, already
/// tagged by kind and already reaching predicates inside conditions) is a node, and
/// each derived predicate gets one edge to each `(kind, dependency)`. A directive
/// derives nothing, so only rules contribute (analysis §4).
fn collect_rule(
    rule: &Rule,
    nodes: &mut BTreeSet<Signature>,
    edges: &mut BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
) {
    let heads: Vec<Signature> = rule.head_signatures().collect();
    let dependencies: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    for head in &heads {
        nodes.insert(head.clone());
    }
    for (_, dependency) in &dependencies {
        nodes.insert(dependency.clone());
    }
    for head in &heads {
        for (kind, dependency) in &dependencies {
            edges
                .entry(head.clone())
                .or_default()
                .insert((*kind, dependency.clone()));
        }
    }
}

/// The strongly-connected-components decomposition (§4): the hand-rolled iterative
/// Tarjan. Returns the components in reverse-topological order (a component before
/// every one that depends on it) and each node's component index. `O(nodes + edges)`.
///
/// The walk operates on `usize` node ids (cheap and `Copy`, unlike a `Signature`),
/// assigned in `Signature` order so the decomposition is deterministic.
fn decompose(
    nodes: &BTreeSet<Signature>,
    edges: &BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
) -> (Vec<Component>, BTreeMap<Signature, usize>) {
    let order: Vec<Signature> = nodes.iter().cloned().collect();
    let id_of: BTreeMap<&Signature, usize> =
        order.iter().enumerate().map(|(id, s)| (s, id)).collect();
    let successors = successor_ids(edges, &id_of, order.len());
    let (component_of, count) = tarjan(&successors);

    // The members of each component, and the kinds on its internal edges (both
    // endpoints in it) — in a strongly-connected component every internal edge is on
    // a cycle, so a kind present is a cycle running through it.
    let mut members: Vec<BTreeSet<Signature>> = vec![BTreeSet::new(); count];
    for id in 0..order.len() {
        members[component_of[id]].insert(order[id].clone());
    }
    let mut internal_kinds: Vec<BTreeSet<DependencyKind>> = vec![BTreeSet::new(); count];
    for (from, targets) in edges {
        if let Some(&f) = id_of.get(from) {
            let component = component_of[f];
            for (kind, to) in targets {
                if let Some(&t) = id_of.get(to)
                    && component_of[t] == component
                {
                    internal_kinds[component].insert(*kind);
                }
            }
        }
    }

    let components: Vec<Component> = (0..count)
        .map(|component| Component {
            members: std::mem::take(&mut members[component]),
            internal_edge_kinds: std::mem::take(&mut internal_kinds[component]),
        })
        .collect();
    let component_index: BTreeMap<Signature, usize> = (0..order.len())
        .map(|id| (order[id].clone(), component_of[id]))
        .collect();
    (components, component_index)
}

/// Each node's successor ids, deduplicated — the edge kind does not affect
/// reachability, so a predicate reached by more than one kind is one successor.
fn successor_ids(
    edges: &BTreeMap<Signature, BTreeSet<(DependencyKind, Signature)>>,
    id_of: &BTreeMap<&Signature, usize>,
    node_count: usize,
) -> Vec<Vec<usize>> {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for (from, targets) in edges {
        if let Some(&f) = id_of.get(from) {
            let mut seen = BTreeSet::new();
            for (_, to) in targets {
                if let Some(&t) = id_of.get(to)
                    && seen.insert(t)
                {
                    successors[f].push(t);
                }
            }
        }
    }
    successors
}

/// Tarjan's strongly-connected-components on an explicit work stack (§4, program
/// §13): a frame carries a node and its next-successor position, so the recursion is
/// the heap's, not the call stack's — a graph deep in its dependency chain does not
/// overflow. Returns each node's component and the component count; components are
/// numbered in pop order, which is **reverse-topological**.
fn tarjan(successors: &[Vec<usize>]) -> (Vec<usize>, usize) {
    let node_count = successors.len();
    let mut index: Vec<Option<usize>> = vec![None; node_count];
    let mut lowlink: Vec<usize> = vec![0; node_count];
    let mut on_stack: Vec<bool> = vec![false; node_count];
    let mut scc_stack: Vec<usize> = Vec::new();
    let mut counter = 0;
    let mut component_of: Vec<usize> = vec![0; node_count];
    let mut count = 0;
    for start in 0..node_count {
        if index[start].is_some() {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(v, i)) = work.last() {
            if i == 0 {
                index[v] = Some(counter);
                lowlink[v] = counter;
                counter += 1;
                scc_stack.push(v);
                on_stack[v] = true;
            }
            if i < successors[v].len() {
                let w = successors[v][i];
                work.last_mut().expect("the current frame").1 = i + 1;
                if index[w].is_none() {
                    work.push((w, 0));
                } else if on_stack[w] {
                    lowlink[v] = lowlink[v].min(index[w].expect("a visited node has an index"));
                }
            } else {
                if lowlink[v] == index[v].expect("an index assigned on entry") {
                    pop_component(v, &mut scc_stack, &mut on_stack, &mut component_of, count);
                    count += 1;
                }
                work.pop();
                if let Some(&(u, _)) = work.last() {
                    lowlink[u] = lowlink[u].min(lowlink[v]);
                }
            }
        }
    }
    (component_of, count)
}

/// Pop a root's component off the SCC stack — every node down to and including the
/// root `v` (§4).
fn pop_component(
    v: usize,
    scc_stack: &mut Vec<usize>,
    on_stack: &mut [bool],
    component_of: &mut [usize],
    component: usize,
) {
    loop {
        let w = scc_stack
            .pop()
            .expect("the component's members down to its root");
        on_stack[w] = false;
        component_of[w] = component;
        if w == v {
            break;
        }
    }
}
