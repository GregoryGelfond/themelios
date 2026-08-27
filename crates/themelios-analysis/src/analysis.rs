//! The assembled analysis value (docs/design/analysis.md §3): the whole of what
//! this crate reports about one program, computed once and read many ways. A
//! consumer builds it with `Analysis::of` and reads the facets it needs — the
//! construct scan (§7), the predicate dependency graph and its strongly-connected
//! components (§4), safety and finiteness (§5), and the program classes (§6).
//!
//! `Analysis::of` is the single door, and it computes the facets **together**
//! because they share the predicate dependency graph: safety's finiteness (§5) and
//! every recursion class (§6.2) read it, so a per-facet recompute would rebuild the
//! graph each time. One pass builds the graph and the construct scan once, and the
//! facets read them — `O(program + edges)`, a facet read `O(1)`, a witness read
//! `O(the witness)`; clone is linear and equality structural (§3, §8).
//!
//! The reading is total on **every** program, including one recovered from a
//! malformed parse (program §8) — totality is over the value, not a well-formedness
//! precondition — and it reads the program's *structure*, not its provenance (§8):
//! two programs equal up to provenance yield equal analyses. It never refuses and
//! carries no `Display`; a reviewed dump reads a view over the facts, never a second
//! rendering (§8, §10). No walk recurses on the call stack — the graph decomposition
//! is iterative (§4) and the program walks are the program tier's iterative ones
//! (program §13) — so a pathological program cannot overflow one.

use themelios_program::program::Program;

use crate::classify::Classes;
use crate::construct::Constructs;
use crate::depend::DependencyGraph;
use crate::safe::Safety;

/// A model of a program's structural facts (§1): its construct scan, its predicate
/// dependency graph, its safety and finiteness, and its program classes, computed
/// once as a pure, total function of a `Program` and read many ways (§3). Owned
/// plain data — `Send + Sync + 'static` — so it is computed on one thread and read
/// on another, kept and compared, without a lifetime (§8). Equality is structural
/// and provenance-blind (§8); clone is linear.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Analysis {
    constructs: Constructs,
    dependencies: DependencyGraph,
    safety: Safety,
    classes: Classes,
}

impl Analysis {
    /// Read a program and report its facts (§3): the single door. One pass builds the
    /// construct scan and the dependency graph, and the facets that share the graph —
    /// safety (§5) and the classes (§6) — read it rather than rebuild it.
    /// `O(program + edges)`. Total on every program, including one recovered from a
    /// malformed parse (§8).
    pub fn of(program: &Program) -> Analysis {
        // The one shared-graph pass (§3): the construct scan and the dependency graph
        // (with its strongly-connected components) are each built once, and the facets
        // that share them — safety reads the graph for finiteness (§5), the classes
        // read the graph and the scan (§6) — take them rather than rebuild them.
        let constructs = Constructs::of(program);
        let dependencies = DependencyGraph::of(program);
        let safety = Safety::from_graph(program, &dependencies);
        let classes = Classes::from_parts(program, &dependencies, &constructs);
        Analysis {
            constructs,
            dependencies,
            safety,
            classes,
        }
    }

    /// The construct scan (§7): which of the language's constructs the program uses,
    /// each with the first statement that bears it.
    pub fn constructs(&self) -> &Constructs {
        &self.constructs
    }

    /// The predicate dependency graph and its strongly-connected components (§4).
    pub fn dependencies(&self) -> &DependencyGraph {
        &self.dependencies
    }

    /// Safety and grounding finiteness (§5): which rules are not safe, and whether
    /// grounding is proven finite.
    pub fn safety(&self) -> &Safety {
        &self.safety
    }

    /// The classes of the literature the program falls in (§6), each a verdict
    /// carrying its witness.
    pub fn classes(&self) -> &Classes {
        &self.classes
    }
}
