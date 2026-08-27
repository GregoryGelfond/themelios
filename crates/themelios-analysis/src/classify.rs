//! The program classes (docs/design/analysis.md §6): a program's membership in the
//! classes of the literature — tight, stratified, head-cycle-free, normal, Horn,
//! disjunctive, choice — read for a solver's algorithm selection. This module
//! provides `Verdict`, the shared approximation verdict (§6.1) every sound,
//! predicate-level reading of this crate returns; the recursion and syntactic
//! classes (§6.2, §6.3) and the routable projection (§6.4) read off it.

use std::collections::BTreeSet;

use themelios_program::program::{Atom, Disjunction, Head, LiteralInner, Program, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::Signature;

use crate::construct::{Construct, Constructs};
use crate::depend::{Component, DependencyGraph, Rule};

/// A sound approximation of a ground-program property, read at the predicate level
/// (§6.1). `Holds` is **proven** — the property is guaranteed of the ground program.
/// `Unknown` is undecided at the predicate level — the ground program may or may not
/// have it — and carries the `Component` that blocked the proof. There is
/// deliberately **no** third `DoesNotHold` arm: a consumer specializes on a class's
/// *presence*, so a false `Holds` is an unsound result while a missed one is merely
/// slower, and the only safe design asserts `Holds` when it has a proof and folds
/// everything else into `Unknown` (the error direction, §6.1). Concrete over
/// `Component`, not generic: the approximation exists *because* this crate reads the
/// predicate level, so its witness is always a predicate-level component. Grounding
/// finiteness (§5) and — once they land — tightness and head-cycle-freeness (§6.2)
/// all return this one shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// The property is proven of the ground program.
    Holds,
    /// The property is undecided at the predicate level, with the component that
    /// blocked the proof.
    Unknown {
        /// The strongly-connected component whose recursion left the property
        /// unproven.
        witness: Component,
    },
}

/// The classes of the literature a program falls in (§6): the recursion classes read
/// off the dependency graph (§6.2), the syntactic classes read off its structure
/// (§6.3), and the routable projection over them (§6.4). Read once from a program by
/// `Classes::of`; every value it holds is owned plain data, so it is computed on one
/// thread and read on another, kept and compared, without a lifetime (§8).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Classes {
    tightness: Verdict,
    head_cycle_free: Verdict,
    stratification: Stratification,
    normality: Normality,
    horn: HornKind,
    uses_disjunction: bool,
    uses_choice: bool,
}

impl Classes {
    /// Read the classes of a program (§6): its dependency graph and its construct scan,
    /// then the recursion classes off the graph (§6.2) and the syntactic classes off its
    /// structure (§6.3). `O(program + edges)`. Builds the graph and scan and delegates to
    /// `from_parts`; the assembled `Analysis` (§3) builds them once and shares them.
    pub fn of(program: &Program) -> Classes {
        Classes::from_parts(
            program,
            &DependencyGraph::of(program),
            &Constructs::of(program),
        )
    }

    /// Read the classes from a dependency graph and construct scan already built for the
    /// program (§6): the door the assembled `Analysis` (§3) calls so the graph and scan
    /// are built once and shared across the facets. `Classes::of` is this with a freshly
    /// built graph and scan. Only the classes read the positive projection, so it is
    /// taken from the passed graph here.
    pub(crate) fn from_parts(
        program: &Program,
        graph: &DependencyGraph,
        constructs: &Constructs,
    ) -> Classes {
        let positive = graph.positive();
        let normality = normality_of(program);
        let horn = horn_of(program, &normality);
        Classes {
            tightness: tightness_verdict(&positive),
            head_cycle_free: head_cycle_free_verdict(program, &positive),
            stratification: stratification_of(graph),
            normality,
            horn,
            uses_disjunction: constructs.uses(Construct::Disjunction),
            uses_choice: constructs.uses(Construct::Choice),
        }
    }

    /// Tight (Fages): no positive recursion, so the program's completion characterizes
    /// its answer sets. `Holds` when the positive dependency graph is acyclic, else
    /// `Unknown` carrying the positive component that blocked the proof — the ground
    /// program may still be tight, so this crate never claims it is not (§6.2).
    pub fn tightness(&self) -> Verdict {
        self.tightness.clone()
    }

    /// Head-cycle-free (Ben-Eliyahu–Dechter): no two atoms of a disjunctive head lie in
    /// one positive cycle, so the program shifts to a normal one. The same
    /// predicate-level approximation as tightness, its `Unknown` witness the positive
    /// component coupling two head atoms (§6.2).
    pub fn head_cycle_free(&self) -> Verdict {
        self.head_cycle_free.clone()
    }

    /// Stratified: no recursion through a `Negative` or `ThroughAggregate` edge.
    /// Definite for negation (a `Negative` cycle proves non-stratification; its absence,
    /// with no aggregate recursion, proves stratification) and conservative-safe for
    /// aggregates; `NotStratified` carries the offending cycle a solver can itself use
    /// (§6.2).
    pub fn stratification(&self) -> &Stratification {
        &self.stratification
    }

    /// Normal: every head is a single literal — no disjunction, choice, or head
    /// aggregate. `NotNormal` carries the first non-normal head's rule (§6.3).
    pub fn normality(&self) -> Normality {
        self.normality.clone()
    }

    /// Horn (definite): normal and negation-free — no default negation *and* no strong
    /// negation, read from the derivation rules alone (a directive's negation is not part
    /// of the least-model fragment). `NotHorn` carries the disjunction, choice, or
    /// negation that breaks it (§6.3).
    pub fn horn(&self) -> HornKind {
        self.horn.clone()
    }

    /// Whether the program uses disjunctive heads — a head extension a solver's method
    /// must account for (§6.3).
    pub fn uses_disjunction(&self) -> bool {
        self.uses_disjunction
    }

    /// Whether the program uses choice heads — a head extension a solver's method must
    /// account for (§6.3).
    pub fn uses_choice(&self) -> bool {
        self.uses_choice
    }

    /// The classes the program is **provably** in — each method's positive arm (§6.2–6.3)
    /// projected to a routable key, in `ProgramClass` order (§6.4). It inherits the error
    /// direction for free: an `Unknown` tight is simply absent, so the set is sound to
    /// `match` on. It names the specialization-admitting classes — hence
    /// `NonDisjunctive`/`ChoiceFree`, the constructs themselves staying on `uses_*`.
    pub fn confirmed(&self) -> impl Iterator<Item = ProgramClass> {
        let mut classes = Vec::new();
        if matches!(self.tightness, Verdict::Holds) {
            classes.push(ProgramClass::Tight);
        }
        if matches!(self.head_cycle_free, Verdict::Holds) {
            classes.push(ProgramClass::HeadCycleFree);
        }
        if matches!(self.stratification, Stratification::Stratified) {
            classes.push(ProgramClass::Stratified);
        }
        if matches!(self.normality, Normality::Normal) {
            classes.push(ProgramClass::Normal);
        }
        if matches!(self.horn, HornKind::Horn) {
            classes.push(ProgramClass::Horn);
        }
        if !self.uses_disjunction {
            classes.push(ProgramClass::NonDisjunctive);
        }
        if !self.uses_choice {
            classes.push(ProgramClass::ChoiceFree);
        }
        classes.into_iter()
    }
}

/// Whether a program is stratified (§6.2): definite in both directions, since a solver
/// safely specializes on either result — the perfect model when stratified, the general
/// method otherwise.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Stratification {
    /// No recursion runs through a non-monotone dependency.
    Stratified,
    /// A recursive component runs through a `Negative` or `ThroughAggregate` edge — the
    /// cycle a solver dispatches to its general method.
    NotStratified {
        /// The strongly-connected component whose recursion breaks stratification.
        cycle: Component,
    },
}

/// Whether every head is a single literal (§6.3): definite, its `NotNormal` arm carrying
/// the first non-normal head's rule so a consumer can point at what makes it so.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Normality {
    /// Every head is a single literal.
    Normal,
    /// A head is a disjunction, choice, or head aggregate.
    NotNormal {
        /// The first rule whose head is not a single literal.
        rule: Rule,
    },
}

/// Whether a program is Horn — normal and negation-free (§6.3): definite, its `NotHorn`
/// arm carrying the rule that breaks it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HornKind {
    /// Normal and free of default and strong negation — the strictly-positive
    /// least-model fragment.
    Horn,
    /// A disjunction, choice, or negation in a derivation rule breaks it.
    NotHorn {
        /// The first rule that is not Horn.
        reason: Rule,
    },
}

/// A class a program is **provably** in — a dataless, routable key a solver's algorithm
/// selection reads (§6.4). `Ord`, so a `BTreeSet` collects a program's classes when set
/// operations help; `#[non_exhaustive]`, since the order-consistent, call-consistent, and
/// s(CASP)-relevant classes will want in.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum ProgramClass {
    /// Tight (§6.2).
    Tight,
    /// Head-cycle-free (§6.2).
    HeadCycleFree,
    /// Stratified (§6.2).
    Stratified,
    /// Normal (§6.3).
    Normal,
    /// Horn (§6.3).
    Horn,
    /// Free of disjunctive heads (§6.3).
    NonDisjunctive,
    /// Free of choice heads (§6.3).
    ChoiceFree,
}

// ---- The recursion classes, off the dependency graph (§6.2) ----

/// Tight iff the positive graph is acyclic; otherwise `Unknown` carrying a recursive
/// positive component — the ground program may still be tight, so this never claims it is
/// not (§6.2).
fn tightness_verdict(positive: &DependencyGraph) -> Verdict {
    match positive
        .components()
        .find(|component| component.is_recursive())
    {
        Some(component) => Verdict::Unknown {
            witness: component.clone(),
        },
        None => Verdict::Holds,
    }
}

/// Head-cycle-free iff no disjunctive head has two atoms in one recursive positive
/// component; otherwise `Unknown` carrying that component (§6.2). The same predicate-level
/// approximation as tightness.
fn head_cycle_free_verdict(program: &Program, positive: &DependencyGraph) -> Verdict {
    for statement in program.statements() {
        if let Statement::Rule(rule) = statement.get()
            && let Head::Disjunction(disjunction) = rule.head().get()
            && let Some(component) = coupling_component(disjunction, positive)
        {
            return Verdict::Unknown { witness: component };
        }
    }
    Verdict::Holds
}

/// The recursive positive component two atoms of the disjunctive head share, if any — the
/// head cycle Ben-Eliyahu–Dechter forbids (§6.2). Each head atom's recursive component is
/// keyed by its least member, so a second atom landing in one already seen is the cycle:
/// `O(head atoms · log)`, not the pairwise `O(atoms²)` a large head would make quadratic.
fn coupling_component(disjunction: &Disjunction, positive: &DependencyGraph) -> Option<Component> {
    let mut seen: BTreeSet<Signature> = BTreeSet::new();
    for element in disjunction.elements() {
        if let LiteralInner::Atom(atom) = &element.get().literal().inner {
            let signature = atom_signature(atom.get());
            if let Some(component) = positive.component_of(&signature)
                && component.is_recursive()
                && let Some(representative) = component.members().next()
                && !seen.insert(representative.clone())
            {
                return Some(component.clone());
            }
        }
    }
    None
}

/// Stratified unless a recursive component runs through a `Negative` or `ThroughAggregate`
/// edge — definite for negation, conservative-safe for aggregates (§6.2); `NotStratified`
/// carries the offending cycle a solver can itself use.
fn stratification_of(graph: &DependencyGraph) -> Stratification {
    for component in graph.components() {
        if component.is_recursive()
            && (component.has_negative_cycle() || component.has_aggregate_cycle())
        {
            return Stratification::NotStratified {
                cycle: component.clone(),
            };
        }
    }
    Stratification::Stratified
}

// ---- The syntactic classes, off the program structure (§6.3) ----

/// Normal unless a rule's head is a disjunction, choice, or head aggregate — the first
/// such rule witnessing it (§6.3).
fn normality_of(program: &Program) -> Normality {
    for statement in program.statements() {
        if let Statement::Rule(rule) = statement.get()
            && is_non_normal_head(rule.head().get())
        {
            return Normality::NotNormal { rule: rule.clone() };
        }
    }
    Normality::Normal
}

fn is_non_normal_head(head: &Head) -> bool {
    matches!(
        head,
        Head::Disjunction(_) | Head::Choice(_) | Head::Aggregate(_)
    )
}

/// Horn iff normal and no derivation rule bears negation (§6.3). A non-normal head breaks
/// it, witnessed by that rule; otherwise the first rule bearing default or strong
/// negation. Horn's negation is **rule-restricted** — a directive's negation is not part
/// of the least-model fragment, and the program-wide construct scan (§7) cannot witness it
/// as a rule.
fn horn_of(program: &Program, normality: &Normality) -> HornKind {
    if let Normality::NotNormal { rule } = normality {
        return HornKind::NotHorn {
            reason: rule.clone(),
        };
    }
    for statement in program.statements() {
        if let Statement::Rule(rule) = statement.get()
            && rule_has_negation(rule)
        {
            return HornKind::NotHorn {
                reason: rule.clone(),
            };
        }
    }
    HornKind::Horn
}

/// Whether a rule bears default or strong negation, read by scanning the rule alone with
/// the construct scan (§7): this rule-scoped reading is exactly the program-wide scan's
/// notion of negation restricted to a derivation rule, so `horn` never reports a false
/// `Horn` for a rule the scan would count as negated (§6.1's error direction). A directive
/// scanned here would show its negation, so only the rule is scanned.
fn rule_has_negation(rule: &Rule) -> bool {
    let program = Program::of([WithProvenance::constructed(Statement::Rule(rule.clone()))]);
    let scan = Constructs::of(&program);
    scan.uses(Construct::DefaultNegation) || scan.uses(Construct::StrongNegation)
}

/// The predicate signature of an atom — its sign, name, and arity, the node identity in
/// the dependency graph (§4).
fn atom_signature(atom: &Atom) -> Signature {
    Signature {
        sign: atom.sign,
        name: atom.name.clone(),
        // A predicate carries no more arguments than a `Vec` holds, far under `u32::MAX`
        // (the workspace `cast_possible_truncation` allowance).
        arity: atom.arguments.len() as u32,
    }
}
