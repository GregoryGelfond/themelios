//! Theory atoms, the theory-term peer algebra, theory definitions, and the body-free
//! directives (docs/design/program.md §4.8, §4.9). The theory term is a **distinct
//! peer** of `Term` (§4.9): its operator structure is the flat sequence the grammar
//! admits (grammar §5.8), regrouped only under a `#theory` definition (admission,
//! above this tier). It is the fourth self-recursive family (§13), so its
//! `Clone`/`Drop`/`Eq`/`Ord`/`Hash`/`Debug` and its `fold` are hand-written and
//! iterative, exactly as `Term`'s. Its `Ord`, like `Term`'s, has no external
//! authority — a consistent total order agreeing with `Eq` suffices. The Body-bearing
//! directives (`Show`, `Project`, `Edge`, `Heuristic`, `External`) carry a `Body`.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use super::rule::{Atom, Body, Condition};
use crate::provenance::WithProvenance;
use crate::symbol::{Name, Signature, Symbol};
use crate::term::{Term, Variable};

/// A theory operator symbol (grammar §5.8's `THEORY-OP` or `not`). This tier does not
/// interpret it — a `#theory` definition gives it precedence and associativity, above
/// this tier (§4.9).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryOperator(String);

impl TheoryOperator {
    /// A theory operator over the given symbol.
    pub fn new(symbol: impl Into<String>) -> TheoryOperator {
        TheoryOperator(symbol.into())
    }

    /// The operator symbol.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The theory-term algebra (grammar §5.8): a distinct peer of `Term` (§4.9), meeting it
/// only at the shared leaves (a variable, a ground symbol). Its applied and bracketed
/// forms group with parentheses, brackets, and braces; its `Operation` form is the flat
/// operator sequence — a run of operators precedes each operand — regrouped only by a
/// `#theory` definition.
///
/// No `#[derive]`: `Clone`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, and `Debug`
/// are hand-written and iterative (§13), so a deep theory term is handled without
/// call-stack recursion; each matches its derived shape (the naive twin,
/// tests/theory_term_laws.rs).
pub enum TheoryTerm {
    /// A ground symbol leaf.
    Symbolic(Symbol),
    /// A variable leaf.
    Variable(Variable),
    /// A function application, `f(t, …)`.
    Function {
        /// The functor name.
        name: Name,
        /// The arguments.
        arguments: Vec<TheoryTerm>,
    },
    /// A tuple, `(t, …)`.
    Tuple(Vec<TheoryTerm>),
    /// A list, `[t, …]`.
    List(Vec<TheoryTerm>),
    /// A set, `{t, …}`.
    Set(Vec<TheoryTerm>),
    /// A flat operator sequence: `operators[i]` is the run before `operands[i]`
    /// (`operators[0]` the leading run), regrouped only by a `#theory` definition (§4.9).
    Operation {
        /// The operator run before each operand.
        operators: Vec<Vec<TheoryOperator>>,
        /// The operands.
        operands: Vec<TheoryTerm>,
    },
}

/// One level of a theory term unrolled, its children a generic `T`, the leaves kept
/// whole (§3.6, applied to the theory peer). At `T = TheoryTerm` it is the owned
/// decomposition (`From` rebuilds); inside `fold` it is what the step sees with children
/// already folded.
pub enum TheoryTermParts<T> {
    /// A ground symbol.
    Symbolic(Symbol),
    /// A variable.
    Variable(Variable),
    /// A function.
    Function {
        /// The functor name.
        name: Name,
        /// The folded arguments.
        arguments: Vec<T>,
    },
    /// A tuple.
    Tuple(Vec<T>),
    /// A list.
    List(Vec<T>),
    /// A set.
    Set(Vec<T>),
    /// A flat operator sequence.
    Operation {
        /// The operator runs.
        operators: Vec<Vec<TheoryOperator>>,
        /// The folded operands.
        operands: Vec<T>,
    },
}

impl TheoryTerm {
    /// This theory term decomposed one level, its children owned (§3.6). Extracts each
    /// field through `&mut self` (this type implements `Drop`, so a consuming pattern is
    /// unsound under `forbid(unsafe_code)`), leaving an emptied husk it then drops.
    pub fn into_parts(mut self) -> TheoryTermParts<TheoryTerm> {
        match &mut self {
            TheoryTerm::Symbolic(s) => {
                TheoryTermParts::Symbolic(std::mem::replace(s, Symbol::Infimum))
            }
            TheoryTerm::Variable(v) => {
                TheoryTermParts::Variable(std::mem::replace(v, Variable::Anonymous))
            }
            TheoryTerm::Function { name, arguments } => TheoryTermParts::Function {
                name: name.clone(),
                arguments: std::mem::take(arguments),
            },
            TheoryTerm::Tuple(items) => TheoryTermParts::Tuple(std::mem::take(items)),
            TheoryTerm::List(items) => TheoryTermParts::List(std::mem::take(items)),
            TheoryTerm::Set(items) => TheoryTermParts::Set(std::mem::take(items)),
            TheoryTerm::Operation {
                operators,
                operands,
            } => TheoryTermParts::Operation {
                operators: std::mem::take(operators),
                operands: std::mem::take(operands),
            },
        }
    }

    /// The immediate and transitive subterms in pre-order — the node before its children
    /// (§3.6). Iterative; O(nodes).
    pub fn subterms(&self) -> impl Iterator<Item = &TheoryTerm> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let term = stack.pop()?;
            for child in children_refs(term).into_iter().rev() {
                stack.push(child);
            }
            Some(term)
        })
    }

    /// Bottom-up rebuild, iterative (§13): each node's children are folded before it, in
    /// document order (§3.6). The one primitive every rebuild over a theory term is
    /// written in.
    pub fn fold<T>(self, mut step: impl FnMut(TheoryTermParts<T>) -> T) -> T {
        match self.try_fold::<T, std::convert::Infallible>(|parts| Ok(step(parts))) {
            Ok(folded) => folded,
            Err(never) => match never {},
        }
    }

    /// `fold`, short-circuiting on the first `Err` (§3.6). Iterative.
    pub fn try_fold<T, E>(
        self,
        mut step: impl FnMut(TheoryTermParts<T>) -> Result<T, E>,
    ) -> Result<T, E> {
        enum Frame {
            Enter(TheoryTerm),
            Assemble(TheoryShell, usize),
        }
        let mut work = vec![Frame::Enter(self)];
        let mut done: Vec<T> = Vec::new();
        while let Some(frame) = work.pop() {
            match frame {
                Frame::Enter(term) => {
                    let (shell, children) = split_parts(term.into_parts());
                    work.push(Frame::Assemble(shell, children.len()));
                    for child in children.into_iter().rev() {
                        work.push(Frame::Enter(child));
                    }
                }
                Frame::Assemble(shell, arity) => {
                    let children = done.split_off(done.len() - arity);
                    done.push(step(assemble_parts(shell, children))?);
                }
            }
        }
        Ok(done.pop().expect("the root's fold"))
    }
}

impl From<TheoryTermParts<TheoryTerm>> for TheoryTerm {
    fn from(parts: TheoryTermParts<TheoryTerm>) -> TheoryTerm {
        match parts {
            TheoryTermParts::Symbolic(s) => TheoryTerm::Symbolic(s),
            TheoryTermParts::Variable(v) => TheoryTerm::Variable(v),
            TheoryTermParts::Function { name, arguments } => {
                TheoryTerm::Function { name, arguments }
            }
            TheoryTermParts::Tuple(items) => TheoryTerm::Tuple(items),
            TheoryTermParts::List(items) => TheoryTerm::List(items),
            TheoryTermParts::Set(items) => TheoryTerm::Set(items),
            TheoryTermParts::Operation {
                operators,
                operands,
            } => TheoryTerm::Operation {
                operators,
                operands,
            },
        }
    }
}

/// A theory term node's non-child data, split from its children for the iterative owned
/// walks (§3.6).
enum TheoryShell {
    Symbolic(Symbol),
    Variable(Variable),
    Function(Name),
    Tuple,
    List,
    Set,
    Operation(Vec<Vec<TheoryOperator>>),
}

/// Split a decomposed theory term into its shell and its owned children (§3.6).
fn split_parts(parts: TheoryTermParts<TheoryTerm>) -> (TheoryShell, Vec<TheoryTerm>) {
    match parts {
        TheoryTermParts::Symbolic(s) => (TheoryShell::Symbolic(s), Vec::new()),
        TheoryTermParts::Variable(v) => (TheoryShell::Variable(v), Vec::new()),
        TheoryTermParts::Function { name, arguments } => (TheoryShell::Function(name), arguments),
        TheoryTermParts::Tuple(items) => (TheoryShell::Tuple, items),
        TheoryTermParts::List(items) => (TheoryShell::List, items),
        TheoryTermParts::Set(items) => (TheoryShell::Set, items),
        TheoryTermParts::Operation {
            operators,
            operands,
        } => (TheoryShell::Operation(operators), operands),
    }
}

/// Split a borrowed theory term into its shell (leaves and operators cloned) and its
/// borrowed children — `clone`'s decomposition (§3.6).
fn split_refs(term: &TheoryTerm) -> (TheoryShell, Vec<&TheoryTerm>) {
    match term {
        TheoryTerm::Symbolic(s) => (TheoryShell::Symbolic(s.clone()), Vec::new()),
        TheoryTerm::Variable(v) => (TheoryShell::Variable(v.clone()), Vec::new()),
        TheoryTerm::Function { name, arguments } => (
            TheoryShell::Function(name.clone()),
            arguments.iter().collect(),
        ),
        TheoryTerm::Tuple(items) => (TheoryShell::Tuple, items.iter().collect()),
        TheoryTerm::List(items) => (TheoryShell::List, items.iter().collect()),
        TheoryTerm::Set(items) => (TheoryShell::Set, items.iter().collect()),
        TheoryTerm::Operation {
            operators,
            operands,
        } => (
            TheoryShell::Operation(operators.clone()),
            operands.iter().collect(),
        ),
    }
}

/// Reassemble a shell and its folded children into `TheoryTermParts` — the inverse of
/// the splits, shared by `fold` and `clone` (§3.6).
fn assemble_parts<T>(shell: TheoryShell, children: Vec<T>) -> TheoryTermParts<T> {
    match shell {
        TheoryShell::Symbolic(s) => TheoryTermParts::Symbolic(s),
        TheoryShell::Variable(v) => TheoryTermParts::Variable(v),
        TheoryShell::Function(name) => TheoryTermParts::Function {
            name,
            arguments: children,
        },
        TheoryShell::Tuple => TheoryTermParts::Tuple(children),
        TheoryShell::List => TheoryTermParts::List(children),
        TheoryShell::Set => TheoryTermParts::Set(children),
        TheoryShell::Operation(operators) => TheoryTermParts::Operation {
            operators,
            operands: children,
        },
    }
}

/// A theory term's immediate child references, in document order (§3.6). The operators
/// are leaf data, not children.
fn children_refs(term: &TheoryTerm) -> Vec<&TheoryTerm> {
    match term {
        TheoryTerm::Symbolic(_) | TheoryTerm::Variable(_) => Vec::new(),
        TheoryTerm::Function { arguments, .. } => arguments.iter().collect(),
        TheoryTerm::Tuple(items) | TheoryTerm::List(items) | TheoryTerm::Set(items) => {
            items.iter().collect()
        }
        TheoryTerm::Operation { operands, .. } => operands.iter().collect(),
    }
}

impl Clone for TheoryTerm {
    fn clone(&self) -> TheoryTerm {
        enum Frame<'a> {
            Enter(&'a TheoryTerm),
            Assemble(TheoryShell, usize),
        }
        let mut work = vec![Frame::Enter(self)];
        let mut done: Vec<TheoryTerm> = Vec::new();
        while let Some(frame) = work.pop() {
            match frame {
                Frame::Enter(term) => {
                    let (shell, children) = split_refs(term);
                    work.push(Frame::Assemble(shell, children.len()));
                    for child in children.into_iter().rev() {
                        work.push(Frame::Enter(child));
                    }
                }
                Frame::Assemble(shell, arity) => {
                    let children = done.split_off(done.len() - arity);
                    done.push(TheoryTerm::from(assemble_parts(shell, children)));
                }
            }
        }
        done.pop().expect("the root's clone")
    }
}

impl Drop for TheoryTerm {
    fn drop(&mut self) {
        let mut stack: Vec<TheoryTerm> = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut term) = stack.pop() {
            take_children(&mut term, &mut stack);
        }
    }
}

/// Moves a theory term's immediate child terms onto `out`, leaving it childless.
fn take_children(term: &mut TheoryTerm, out: &mut Vec<TheoryTerm>) {
    match term {
        TheoryTerm::Symbolic(_) | TheoryTerm::Variable(_) => {}
        TheoryTerm::Function { arguments, .. } => out.append(arguments),
        TheoryTerm::Tuple(items) | TheoryTerm::List(items) | TheoryTerm::Set(items) => {
            out.append(items);
        }
        TheoryTerm::Operation { operands, .. } => out.append(operands),
    }
}

/// The variant's rank in `TheoryTerm`'s order — declaration order (§4.9).
fn theory_rank(term: &TheoryTerm) -> u8 {
    match term {
        TheoryTerm::Symbolic(_) => 0,
        TheoryTerm::Variable(_) => 1,
        TheoryTerm::Function { .. } => 2,
        TheoryTerm::Tuple(_) => 3,
        TheoryTerm::List(_) => 4,
        TheoryTerm::Set(_) => 5,
        TheoryTerm::Operation { .. } => 6,
    }
}

impl PartialEq for TheoryTerm {
    fn eq(&self, other: &TheoryTerm) -> bool {
        let mut pairs: Vec<(&TheoryTerm, &TheoryTerm)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            match (a, b) {
                (TheoryTerm::Symbolic(x), TheoryTerm::Symbolic(y)) if x == y => {}
                (TheoryTerm::Variable(x), TheoryTerm::Variable(y)) if x == y => {}
                (
                    TheoryTerm::Function {
                        name: n1,
                        arguments: a1,
                    },
                    TheoryTerm::Function {
                        name: n2,
                        arguments: a2,
                    },
                ) if n1 == n2 && a1.len() == a2.len() => pairs.extend(a1.iter().zip(a2)),
                (TheoryTerm::Tuple(a1), TheoryTerm::Tuple(a2))
                | (TheoryTerm::List(a1), TheoryTerm::List(a2))
                | (TheoryTerm::Set(a1), TheoryTerm::Set(a2))
                    if a1.len() == a2.len() =>
                {
                    pairs.extend(a1.iter().zip(a2));
                }
                (
                    TheoryTerm::Operation {
                        operators: o1,
                        operands: p1,
                    },
                    TheoryTerm::Operation {
                        operators: o2,
                        operands: p2,
                    },
                ) if o1 == o2 && p1.len() == p2.len() => pairs.extend(p1.iter().zip(p2)),
                _ => return false,
            }
        }
        true
    }
}
impl Eq for TheoryTerm {}

impl PartialOrd for TheoryTerm {
    fn partial_cmp(&self, other: &TheoryTerm) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TheoryTerm {
    fn cmp(&self, other: &TheoryTerm) -> Ordering {
        // A consistent total order agreeing with `Eq` (§4.9): no external authority, like
        // `Term`. Rank, then the head scalars, then the children by count-then-elements;
        // iterative, the naive twin holds it honest (tests/theory_term_laws.rs).
        let mut pairs: Vec<(&TheoryTerm, &TheoryTerm)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            let by_rank = theory_rank(a).cmp(&theory_rank(b));
            if by_rank != Ordering::Equal {
                return by_rank;
            }
            let here = match (a, b) {
                (TheoryTerm::Symbolic(x), TheoryTerm::Symbolic(y)) => x.cmp(y),
                (TheoryTerm::Variable(x), TheoryTerm::Variable(y)) => x.cmp(y),
                (
                    TheoryTerm::Function {
                        name: n1,
                        arguments: a1,
                    },
                    TheoryTerm::Function {
                        name: n2,
                        arguments: a2,
                    },
                ) => (n1, a1.len()).cmp(&(n2, a2.len())).then_with(|| {
                    push_pairs_reversed(&mut pairs, a1, a2);
                    Ordering::Equal
                }),
                (TheoryTerm::Tuple(a1), TheoryTerm::Tuple(a2))
                | (TheoryTerm::List(a1), TheoryTerm::List(a2))
                | (TheoryTerm::Set(a1), TheoryTerm::Set(a2)) => {
                    a1.len().cmp(&a2.len()).then_with(|| {
                        push_pairs_reversed(&mut pairs, a1, a2);
                        Ordering::Equal
                    })
                }
                (
                    TheoryTerm::Operation {
                        operators: o1,
                        operands: p1,
                    },
                    TheoryTerm::Operation {
                        operators: o2,
                        operands: p2,
                    },
                ) => (o1, p1.len()).cmp(&(o2, p2.len())).then_with(|| {
                    push_pairs_reversed(&mut pairs, p1, p2);
                    Ordering::Equal
                }),
                // Unreachable: equal rank implies the same variant.
                _ => Ordering::Equal,
            };
            if here != Ordering::Equal {
                return here;
            }
        }
        Ordering::Equal
    }
}

/// Pushes the element pairs of two equal-length slices onto `pairs`, reversed so the
/// leftmost is compared first.
fn push_pairs_reversed<'a>(
    pairs: &mut Vec<(&'a TheoryTerm, &'a TheoryTerm)>,
    a: &'a [TheoryTerm],
    b: &'a [TheoryTerm],
) {
    for pair in a.iter().zip(b.iter()).rev() {
        pairs.push(pair);
    }
}

impl Hash for TheoryTerm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Iterative pre-order hash (§13): each node's rank and head scalars, then a
        // length marker for its children — the same content projection as `Eq` (§5.2).
        for term in self.subterms() {
            match term {
                TheoryTerm::Symbolic(s) => {
                    state.write_u8(0);
                    s.hash(state);
                }
                TheoryTerm::Variable(v) => {
                    state.write_u8(1);
                    v.hash(state);
                }
                TheoryTerm::Function { name, arguments } => {
                    state.write_u8(2);
                    name.hash(state);
                    state.write_usize(arguments.len());
                }
                TheoryTerm::Tuple(items) => {
                    state.write_u8(3);
                    state.write_usize(items.len());
                }
                TheoryTerm::List(items) => {
                    state.write_u8(4);
                    state.write_usize(items.len());
                }
                TheoryTerm::Set(items) => {
                    state.write_u8(5);
                    state.write_usize(items.len());
                }
                TheoryTerm::Operation {
                    operators,
                    operands,
                } => {
                    state.write_u8(6);
                    operators.hash(state);
                    state.write_usize(operands.len());
                }
            }
        }
    }
}

/// A print action for the iterative `Debug` — a node to render or a static separator.
enum DebugAct<'a> {
    Node(&'a TheoryTerm),
    Str(&'static str),
}

impl std::fmt::Debug for TheoryTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A derived-shaped `Debug`, iterative (§14): the operators are leaf data, printed
        // whole; the operands are recursive nodes on the work list.
        let mut work = vec![DebugAct::Node(self)];
        while let Some(act) = work.pop() {
            match act {
                DebugAct::Str(s) => f.write_str(s)?,
                DebugAct::Node(term) => match term {
                    TheoryTerm::Symbolic(s) => write!(f, "Symbolic({s:?})")?,
                    TheoryTerm::Variable(v) => write!(f, "Variable({v:?})")?,
                    TheoryTerm::Function { name, arguments } => {
                        write!(f, "Function {{ name: {name:?}, arguments: [")?;
                        work.push(DebugAct::Str("] }"));
                        push_debug_list(&mut work, arguments);
                    }
                    TheoryTerm::Tuple(items) => {
                        f.write_str("Tuple([")?;
                        work.push(DebugAct::Str("])"));
                        push_debug_list(&mut work, items);
                    }
                    TheoryTerm::List(items) => {
                        f.write_str("List([")?;
                        work.push(DebugAct::Str("])"));
                        push_debug_list(&mut work, items);
                    }
                    TheoryTerm::Set(items) => {
                        f.write_str("Set([")?;
                        work.push(DebugAct::Str("])"));
                        push_debug_list(&mut work, items);
                    }
                    TheoryTerm::Operation {
                        operators,
                        operands,
                    } => {
                        write!(f, "Operation {{ operators: {operators:?}, operands: [")?;
                        work.push(DebugAct::Str("] }"));
                        push_debug_list(&mut work, operands);
                    }
                },
            }
        }
        Ok(())
    }
}

/// Pushes a sequence of children as `Debug` nodes, reversed and comma-separated so they
/// print left-to-right (§14).
fn push_debug_list<'a>(work: &mut Vec<DebugAct<'a>>, items: &'a [TheoryTerm]) {
    for (i, child) in items.iter().enumerate().rev() {
        work.push(DebugAct::Node(child));
        if i > 0 {
            work.push(DebugAct::Str(", "));
        }
    }
}

/// A theory element (grammar §5.8): the theory terms of an element, under an optional
/// condition (present when the `:` is, §5.4). A `FunctionAggregate`-like set member of a
/// theory atom (§4.9).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryElement {
    terms: Vec<TheoryTerm>,
    condition: Option<Condition>,
}

impl TheoryElement {
    /// A theory element over the given terms and optional condition.
    pub fn new(
        terms: impl IntoIterator<Item = TheoryTerm>,
        condition: Option<Condition>,
    ) -> TheoryElement {
        TheoryElement {
            terms: terms.into_iter().collect(),
            condition,
        }
    }

    /// The theory terms, in order.
    pub fn terms(&self) -> impl Iterator<Item = &TheoryTerm> {
        self.terms.iter()
    }

    /// The condition, if the `:` was present.
    pub fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }
}

/// A theory atom's guard (grammar §5.8): a single operator and a theory term.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryGuard {
    /// The guard operator.
    pub operator: TheoryOperator,
    /// The bound.
    pub term: TheoryTerm,
}

/// A theory atom (grammar §5.8): a name, optional ordinary-term arguments, optional
/// elements, and an optional guard (§4.9). Admission against a `#theory` definition is a
/// concern above this tier. Its elements are a set.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryAtom {
    name: Name,
    arguments: Vec<Term>,
    elements: BTreeSet<WithProvenance<TheoryElement>>,
    guard: Option<TheoryGuard>,
}

impl TheoryAtom {
    /// A theory atom, its ordinary-term arguments canonicalized at the door (§5.1) and
    /// its elements carrying a `Constructed` origin (§6.2). O(size).
    pub fn new(
        name: Name,
        arguments: impl IntoIterator<Item = Term>,
        elements: impl IntoIterator<Item = TheoryElement>,
        guard: Option<TheoryGuard>,
    ) -> TheoryAtom {
        TheoryAtom {
            name,
            arguments: arguments.into_iter().map(Term::canonicalize).collect(),
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
            guard,
        }
    }

    /// A theory atom over already-provenanced elements, unioning provenance on any
    /// content collision (§6.3) — the raise's door, carrying each element's parsed
    /// origin (§6.2, §8). The ordinary-term arguments canonicalize at the ingest
    /// door with the rest of the statement, so they are stored as read. O(size).
    pub(crate) fn from_nodes(
        name: Name,
        arguments: Vec<Term>,
        elements: impl IntoIterator<Item = WithProvenance<TheoryElement>>,
        guard: Option<TheoryGuard>,
    ) -> TheoryAtom {
        TheoryAtom {
            name,
            arguments,
            elements: super::merge_collect(elements),
            guard,
        }
    }

    /// The atom name.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// The ordinary-term arguments, in order.
    pub fn arguments(&self) -> impl Iterator<Item = &Term> {
        self.arguments.iter()
    }

    /// The elements — a set, each with its provenance (§6.2).
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<TheoryElement>> {
        self.elements.iter()
    }

    /// The guard, if any.
    pub fn guard(&self) -> Option<&TheoryGuard> {
        self.guard.as_ref()
    }
}

/// A theory operator's arity in a `#theory` definition (grammar §5.9).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TheoryOperatorArity {
    /// A prefix (`unary`) operator.
    Unary,
    /// A left-associative (`binary`, `left`) operator.
    BinaryLeft,
    /// A right-associative (`binary`, `right`) operator.
    BinaryRight,
}

/// Where a defined theory atom may occur (grammar §5.9).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TheoryOccurrence {
    /// `head`.
    Head,
    /// `body`.
    Body,
    /// `any`.
    Any,
    /// `directive`.
    Directive,
}

/// An operator's definition in a term-definition (grammar §5.9): its priority and arity.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryOperatorDefinition {
    /// The operator.
    pub operator: TheoryOperator,
    /// The priority (`NUMBER`).
    pub priority: u32,
    /// The arity and associativity.
    pub arity: TheoryOperatorArity,
}

/// A term-definition in a `#theory` definition (grammar §5.9): a name and its operators.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryTermDefinition {
    /// The term-definition name.
    pub name: Name,
    /// The operator definitions, a set.
    pub operators: BTreeSet<TheoryOperatorDefinition>,
}

/// An atom-definition's guard in a `#theory` definition (grammar §5.9): the guard
/// operators and the guard's term-definition name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryAtomGuardDefinition {
    /// The guard operators, a set.
    pub operators: BTreeSet<TheoryOperator>,
    /// The guard's term-definition name.
    pub term_definition: Name,
}

/// An atom-definition in a `#theory` definition (grammar §5.9): the atom signature, its
/// term-definition, an optional guard, and where it may occur.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryAtomDefinition {
    /// The atom name.
    pub name: Name,
    /// The atom arity.
    pub arity: u32,
    /// The element term-definition name.
    pub term_definition: Name,
    /// The guard definition, if any.
    pub guard: Option<TheoryAtomGuardDefinition>,
    /// Where the atom may occur.
    pub occurrence: TheoryOccurrence,
}

/// A `#theory` definition (grammar §5.9): a name and its term- and atom-definitions. This
/// tier represents it structurally; the admission it drives is a concern above (§4.9).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TheoryDefinition {
    /// The definition name.
    pub name: Name,
    /// The term-definitions, a set.
    pub terms: BTreeSet<TheoryTermDefinition>,
    /// The atom-definitions, a set.
    pub atoms: BTreeSet<TheoryAtomDefinition>,
}

/// A constant directive's policy (grammar §5.9).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ConstPolicy {
    /// `[default]`.
    Default,
    /// `[override]`.
    Override,
}

/// A constant directive (grammar §5.9): a name, a value in the constant-term subset, and
/// an optional policy. The value is a `Term`, **not** a pre-evaluated `Symbol` (§4.8):
/// `#const x = 1+2.` and `#const x = 3.` are structurally distinct — a consumer that
/// wants the denoted symbol calls `evaluate` (§3.5). The constant-term-subset check is
/// the raise's (grammar §5.9); this value carries the term as written.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Const {
    /// The constant name.
    pub name: Name,
    /// The value, an unevaluated term.
    pub value: Term,
    /// The policy, if any.
    pub policy: Option<ConstPolicy>,
}

/// A `#defined` directive (grammar §5.9): a signature.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Defined {
    /// The declared signature.
    pub signature: Signature,
}

/// The target of an `#include` directive (grammar §5.9): a quoted path or an
/// angle-bracketed system name. Carried, never resolved — no I/O in this tier (§4.8,
/// spec §6.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum IncludeTarget {
    /// A quoted path, `"file.lp"`.
    Path(String),
    /// A system include, `<incmode>`.
    System(Name),
}

/// An `#include` directive (grammar §5.9): its target, parsed and never resolved (§4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Include {
    target: IncludeTarget,
}

impl Include {
    /// An include of the given target.
    pub fn new(target: IncludeTarget) -> Include {
        Include { target }
    }

    /// The include target — carried, never resolved (§4.8).
    pub fn target(&self) -> &IncludeTarget {
        &self.target
    }
}

/// A `#script` directive (grammar §5.9): a language name and its body text, carried
/// opaque and never run — no I/O in this tier (§4.8, syntax §8.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Script {
    language: Name,
    body: String,
}

impl Script {
    /// A script in the given language with the given verbatim body.
    pub fn new(language: Name, body: impl Into<String>) -> Script {
        Script {
            language,
            body: body.into(),
        }
    }

    /// The script language.
    pub fn language(&self) -> &Name {
        &self.language
    }

    /// The verbatim body text — carried, never run (§4.8).
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// A `#show` directive (grammar §5.9): one of four forms (§4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Show {
    /// `#show.` — show nothing.
    All,
    /// `#show p/1.` — a signature.
    Signature(Signature),
    /// `#show t.` — a term.
    Term(Term),
    /// `#show t : body.` — a term under a body.
    TermBody {
        /// The shown term.
        term: Term,
        /// The body it shows under.
        body: Body,
    },
}

/// A `#project` directive (grammar §5.9): a signature or an atom under a body (§4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Project {
    /// `#project p/1.` — a signature.
    Signature(Signature),
    /// `#project a : body.` — an atom under a body (empty when the `:` is absent).
    Atom {
        /// The projected atom.
        atom: Atom,
        /// The body.
        body: Body,
    },
}

/// An `#edge` directive (grammar §5.9): node pairs under a body (§4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Edge {
    pairs: Vec<(Term, Term)>,
    body: Body,
}

impl Edge {
    /// An edge over the given node pairs and body, the node terms canonicalized (§5.1).
    pub fn new(pairs: impl IntoIterator<Item = (Term, Term)>, body: Body) -> Edge {
        Edge {
            pairs: pairs
                .into_iter()
                .map(|(from, to)| (from.canonicalize(), to.canonicalize()))
                .collect(),
            body,
        }
    }

    /// The node pairs, in order.
    pub fn pairs(&self) -> impl Iterator<Item = (&Term, &Term)> {
        self.pairs.iter().map(|(from, to)| (from, to))
    }

    /// The body.
    pub fn body(&self) -> &Body {
        &self.body
    }
}

/// A `#heuristic` directive (grammar §5.9): an atom under a body, with a bias, an
/// optional priority, and a modifier (§4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Heuristic {
    atom: Atom,
    body: Body,
    bias: Term,
    priority: Option<Term>,
    modifier: Term,
}

impl Heuristic {
    /// A heuristic over the given parts, its bias, priority, and modifier canonicalized
    /// (§5.1).
    pub fn new(
        atom: Atom,
        body: Body,
        bias: impl Into<Term>,
        priority: Option<Term>,
        modifier: impl Into<Term>,
    ) -> Heuristic {
        Heuristic {
            atom,
            body,
            bias: bias.into().canonicalize(),
            priority: priority.map(Term::canonicalize),
            modifier: modifier.into().canonicalize(),
        }
    }

    /// The atom.
    pub fn atom(&self) -> &Atom {
        &self.atom
    }

    /// The body.
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// The heuristic bias term.
    pub fn bias(&self) -> &Term {
        &self.bias
    }

    /// The priority term, if any.
    pub fn priority(&self) -> Option<&Term> {
        self.priority.as_ref()
    }

    /// The heuristic modifier term.
    pub fn modifier(&self) -> &Term {
        &self.modifier
    }
}

/// An `#external` directive (grammar §5.9): an atom under a body, with an optional value
/// — carried, never meaningful (grammar §13, §4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct External {
    atom: Atom,
    body: Body,
    value: Option<Term>,
}

impl External {
    /// An external over the given atom, body, and optional value (canonicalized, §5.1).
    pub fn new(atom: Atom, body: Body, value: Option<Term>) -> External {
        External {
            atom,
            body,
            value: value.map(Term::canonicalize),
        }
    }

    /// The atom.
    pub fn atom(&self) -> &Atom {
        &self.atom
    }

    /// The body.
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// The value, if any — carried, never meaningful (grammar §13).
    pub fn value(&self) -> Option<&Term> {
        self.value.as_ref()
    }
}

// ---- Canonicalization (§5.1) ----
//
// The directive spine of the pass (see `rule.rs`): a directive's ordinary terms, atoms,
// and bodies are canonicalized; a theory term never ground-collapses (§4.9), so only the
// ordinary terms and the atoms inside an element's condition move. Provenance preserved
// through the carrier's `map` (§6.2). Grammar-bounded, so a bounded recursion (§13).

impl TheoryAtom {
    pub(crate) fn canonicalize(self) -> TheoryAtom {
        TheoryAtom {
            name: self.name,
            arguments: self.arguments.into_iter().map(Term::canonicalize).collect(),
            elements: super::merge_collect(
                self.elements
                    .into_iter()
                    .map(|element| element.map(TheoryElement::canonicalize)),
            ),
            guard: self.guard,
        }
    }
}

impl TheoryElement {
    pub(crate) fn canonicalize(self) -> TheoryElement {
        // Theory terms do not ground-collapse (§4.9); the ordinary literals of a
        // condition do.
        TheoryElement {
            terms: self.terms,
            condition: self.condition.map(Condition::canonicalize),
        }
    }
}

impl Show {
    pub(crate) fn canonicalize(self) -> Show {
        match self {
            Show::All => Show::All,
            Show::Signature(signature) => Show::Signature(signature),
            Show::Term(term) => Show::Term(term.canonicalize()),
            Show::TermBody { term, body } => Show::TermBody {
                term: term.canonicalize(),
                body: body.canonicalize(),
            },
        }
    }
}

impl Project {
    pub(crate) fn canonicalize(self) -> Project {
        match self {
            Project::Signature(signature) => Project::Signature(signature),
            Project::Atom { atom, body } => Project::Atom {
                atom: atom.canonicalize(),
                body: body.canonicalize(),
            },
        }
    }
}

impl Edge {
    pub(crate) fn canonicalize(self) -> Edge {
        Edge {
            pairs: self
                .pairs
                .into_iter()
                .map(|(from, to)| (from.canonicalize(), to.canonicalize()))
                .collect(),
            body: self.body.canonicalize(),
        }
    }
}

impl Heuristic {
    pub(crate) fn canonicalize(self) -> Heuristic {
        Heuristic {
            atom: self.atom.canonicalize(),
            body: self.body.canonicalize(),
            bias: self.bias.canonicalize(),
            priority: self.priority.map(Term::canonicalize),
            modifier: self.modifier.canonicalize(),
        }
    }
}

impl External {
    pub(crate) fn canonicalize(self) -> External {
        External {
            atom: self.atom.canonicalize(),
            body: self.body.canonicalize(),
            value: self.value.map(Term::canonicalize),
        }
    }
}

impl Const {
    pub(crate) fn canonicalize(self) -> Const {
        Const {
            name: self.name,
            value: self.value.canonicalize(),
            policy: self.policy,
        }
    }
}
