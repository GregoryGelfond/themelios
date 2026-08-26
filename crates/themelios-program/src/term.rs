//! The non-ground term algebra generalizing the ground `Symbol`
//! (docs/design/program.md §3.3, §3.6, §3.7, §5.1). `Term` recurses through
//! `Box` (the operator and interval forms) and `Vec` (the applied and grouped
//! forms); like `Symbol` it is owned plain data whose every walk — clone, drop,
//! equality, ordering, hashing, debug, and the `fold` rebuild — is iterative
//! (§13), so a term tens of thousands of levels deep is handled without touching
//! the call stack. Term-level canonicalization collapses maximal ground
//! constructor subterms to symbols and drops a degenerate one-alternative pool
//! (§5.1); operators never fold (§3.5).

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::symbol::{Name, Sign, Symbol, VarName};

/// The non-ground term algebra (§3.3). A term carries **no** strong sign — the `-`
/// of `-p` is arithmetic `Negate` in term position and the atom's sign in literal
/// position (§4.6), so it never lives on a `Term`.
///
/// No `#[derive]`: `Clone`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, and
/// `Debug` are hand-written and iterative (§13), so a deep term is handled without
/// call-stack recursion; each matches its derived shape (the naive twin,
/// tests/term_laws.rs). `Symbolic` holds the ground sub-algebra whose own walks are
/// iterative (§3.1), so it is a leaf here — the term walks never descend into it.
pub enum Term {
    /// A variable, `X` or `_`.
    Variable(Variable),
    /// The ground leaf. Maximal ground *constructor* subterms collapse here at
    /// canonicalization (§5.1); a ground *operator* term does not.
    Symbolic(Symbol),
    /// A function application `f(t, …)` (a constant when the arguments are empty).
    Function {
        /// The functor name.
        name: Name,
        /// The arguments.
        arguments: Vec<Term>,
    },
    /// The anonymous functor: `(a, b)`, the one-element `(a,)`, the empty `()`.
    Tuple(Vec<Term>),
    /// A pool of alternatives, `(a; b)` — a semantic term-former (grammar §5.1).
    Pool(Vec<Term>),
    /// A prefix operation, `-t` or `~t`.
    UnaryOperation {
        /// The operator.
        operator: UnaryOp,
        /// The operand.
        argument: Box<Term>,
    },
    /// An infix operation, `l + r` and its kin.
    BinaryOperation {
        /// The operator.
        operator: BinaryOp,
        /// The left operand.
        left: Box<Term>,
        /// The right operand.
        right: Box<Term>,
    },
    /// An interval, `l .. u` — a semantic term-former (grammar §5.1).
    Interval {
        /// The lower bound.
        lower: Box<Term>,
        /// The upper bound.
        upper: Box<Term>,
    },
    /// An absolute value, `|t|` (grammar §5.1).
    Absolute(Box<Term>),
    /// A ground-extension call site, `@name` / `@name(args)` (spec §9.6), left
    /// unevaluated by this tier (§3.5).
    External {
        /// The extension name.
        name: Name,
        /// The arguments.
        arguments: Vec<Term>,
    },
}

/// A variable: a named one, `X`, or the anonymous `_` (§3.3). Flat and
/// non-recursive, so it derives; it is not `Copy` (it holds a `VarName`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Variable {
    /// A named variable, `X`.
    Named(VarName),
    /// The anonymous variable, `_`.
    Anonymous,
}

/// The prefix operators (grammar §5.1). `Negate` is arithmetic `-`; `BitwiseNot`
/// is `~`, named apart from `Sign::Negative` (strong `-`) and default `not` — the
/// three-negation discipline (§3.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum UnaryOp {
    /// Arithmetic negation, `-`.
    Negate,
    /// Bitwise complement, `~`.
    BitwiseNot,
}

/// The infix operators (grammar §5.1), less the interval `..` and the pool `;`,
/// which are their own term-formers. `BitOr` is `?`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum BinaryOp {
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `\`.
    Mod,
    /// `**`.
    Pow,
    /// `&`.
    BitAnd,
    /// `?`.
    BitOr,
    /// `^`.
    BitXor,
}

impl From<Symbol> for Term {
    /// A ground symbol is a term (§3.3): the cheap direction, an O(1) move into the
    /// `Symbolic` leaf.
    fn from(symbol: Symbol) -> Term {
        Term::Symbolic(symbol)
    }
}

impl Term {
    /// The functor name — `Some` for a function, `None` otherwise (§3.7). The
    /// operator, interval, pool, external, and variable forms are read by matching
    /// or `into_parts`, never through the point accessors. O(1).
    pub fn name(&self) -> Option<&Name> {
        match self {
            Term::Function { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The immediate arguments of an applied form — a function's arguments or a
    /// tuple's elements; the empty slice otherwise (§3.7). O(1).
    pub fn arguments(&self) -> &[Term] {
        match self {
            Term::Function { arguments, .. } => arguments,
            Term::Tuple(items) => items,
            _ => &[],
        }
    }

    /// The i-th argument, or `None` — total, never a panicking index (§3.7). O(1).
    pub fn arg(&self, i: usize) -> Option<&Term> {
        self.arguments().get(i)
    }

    /// The number of arguments — `0` for a non-applied form (§3.7). O(1).
    pub fn arity(&self) -> u32 {
        // A term carries no more arguments than a `Vec` holds, far under `u32::MAX`
        // on any real machine; the cast cannot truncate (the workspace
        // `cast_possible_truncation` allowance, argued in place).
        self.arguments().len() as u32
    }

    /// Whether the term is ground — no variable occurs (§3.3). A walk, not a variant
    /// check: an `External` with variable-free arguments reads as ground here
    /// (variable-freedom is the reading, its `External` subtlety flagged for review).
    /// Iterative; O(nodes).
    pub fn is_ground(&self) -> bool {
        !self
            .subterms()
            .any(|term| matches!(term, Term::Variable(_)))
    }
}

/// One level of a term unrolled, its children a generic `T`, the leaves kept whole
/// (§3.6). A boxed child of `Term` is an unboxed `T` here (`From` re-boxes). At
/// `T = Term` it is the owned decomposition; inside `fold` it is what the step sees
/// with children already folded.
pub enum TermParts<T> {
    /// A variable.
    Variable(Variable),
    /// A ground symbol.
    Symbolic(Symbol),
    /// A function or constant.
    Function {
        /// The functor name.
        name: Name,
        /// The folded arguments.
        arguments: Vec<T>,
    },
    /// A tuple.
    Tuple(Vec<T>),
    /// A pool.
    Pool(Vec<T>),
    /// A prefix operation.
    UnaryOperation {
        /// The operator.
        operator: UnaryOp,
        /// The folded operand.
        argument: T,
    },
    /// An infix operation.
    BinaryOperation {
        /// The operator.
        operator: BinaryOp,
        /// The folded left operand.
        left: T,
        /// The folded right operand.
        right: T,
    },
    /// An interval.
    Interval {
        /// The folded lower bound.
        lower: T,
        /// The folded upper bound.
        upper: T,
    },
    /// An absolute value.
    Absolute(T),
    /// An external call.
    External {
        /// The extension name.
        name: Name,
        /// The folded arguments.
        arguments: Vec<T>,
    },
}

impl Term {
    /// This term decomposed one level, its children owned (§3.6). O(1) plus the
    /// moved children (a named form clones its small name rather than move it, since
    /// `Term`'s `Drop` forbids a consuming pattern and the husk keeps the original,
    /// dropping at once); `From<TermParts<Term>>` is the inverse.
    pub fn into_parts(mut self) -> TermParts<Term> {
        // `Term` implements `Drop` (the iterative teardown, §13), so its fields
        // cannot be moved out by a consuming pattern (that would drop a partly-moved
        // value, which `forbid(unsafe_code)` gives no way to make sound). Each is
        // taken through `&mut self` — a cheap sentinel replaces a leaf or a box, an
        // empty `Vec` a sequence, a clone a name — and the husk `self` then drops
        // finding no children.
        match &mut self {
            Term::Variable(v) => TermParts::Variable(std::mem::replace(v, Variable::Anonymous)),
            Term::Symbolic(s) => TermParts::Symbolic(std::mem::replace(s, Symbol::Infimum)),
            Term::Function { name, arguments } => TermParts::Function {
                name: name.clone(),
                arguments: std::mem::take(arguments),
            },
            Term::Tuple(items) => TermParts::Tuple(std::mem::take(items)),
            Term::Pool(items) => TermParts::Pool(std::mem::take(items)),
            Term::UnaryOperation { operator, argument } => TermParts::UnaryOperation {
                operator: *operator,
                argument: take_box(argument),
            },
            Term::BinaryOperation {
                operator,
                left,
                right,
            } => TermParts::BinaryOperation {
                operator: *operator,
                left: take_box(left),
                right: take_box(right),
            },
            Term::Interval { lower, upper } => TermParts::Interval {
                lower: take_box(lower),
                upper: take_box(upper),
            },
            Term::Absolute(inner) => TermParts::Absolute(take_box(inner)),
            Term::External { name, arguments } => TermParts::External {
                name: name.clone(),
                arguments: std::mem::take(arguments),
            },
        }
    }

    /// The immediate and transitive subterms in pre-order — the node before its
    /// children, a contract (§3.6). A `Symbolic` leaf is one subterm (the ground
    /// algebra is not descended here). Iterative; O(nodes).
    pub fn subterms(&self) -> impl Iterator<Item = &Term> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let term = stack.pop()?;
            push_child_refs_reversed(term, &mut stack);
            Some(term)
        })
    }

    /// Bottom-up rebuild, iterative (§13): each node's children are folded before it,
    /// in document order, O(nodes) heap, nothing cloned but a name (§3.6). The one
    /// primitive every rebuild over a term is written in.
    pub fn fold<T>(self, mut step: impl FnMut(TermParts<T>) -> T) -> T {
        match self.try_fold::<T, std::convert::Infallible>(|parts| Ok(step(parts))) {
            Ok(folded) => folded,
            Err(never) => match never {},
        }
    }

    /// `fold`, short-circuiting on the first `Err` (§3.6). Iterative.
    pub fn try_fold<T, E>(
        self,
        mut step: impl FnMut(TermParts<T>) -> Result<T, E>,
    ) -> Result<T, E> {
        // An explicit work list of enter/assemble frames; `done` holds finished `T`s,
        // so recursion depth is the heap's, not the stack's. Children are pushed
        // reversed so they enter left-to-right and assemble in document order.
        enum Frame {
            Enter(Term),
            Assemble(Shell, usize),
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

    /// Canonicalize a term (§5.1): idempotent, total, iterative (written in `fold`,
    /// one bottom-up pass, O(nodes)). Two syntactic normalizations — the maximal
    /// ground *constructor* collapse to a `Symbolic` (operators never fold, §3.5),
    /// and the degenerate one-alternative pool. A pass, not a constructor guarantee:
    /// a caller can build a non-canonical term directly, and every door that admits a
    /// term into a program runs this.
    #[must_use]
    pub fn canonicalize(self) -> Term {
        self.fold(|parts| match parts {
            TermParts::Function { name, arguments } => match into_symbols(arguments) {
                // Ground: a term-position function bears no strong sign (§3.3, §4.6),
                // so the collapsed symbol is `Positive`.
                Ok(symbols) => Term::Symbolic(Symbol::Function {
                    name,
                    arguments: symbols,
                    sign: Sign::Positive,
                }),
                Err(arguments) => Term::Function { name, arguments },
            },
            TermParts::Tuple(items) => match into_symbols(items) {
                Ok(symbols) => Term::Symbolic(Symbol::Tuple(symbols)),
                Err(items) => Term::Tuple(items),
            },
            TermParts::Pool(mut items) => {
                if items.len() == 1 {
                    items.pop().expect("a one-alternative pool has its element")
                } else {
                    Term::Pool(items)
                }
            }
            other => Term::from(other),
        })
    }
}

/// If every term is a `Symbolic` leaf, the ground symbols it wraps (moved out);
/// otherwise the terms unchanged. The peek guards the extraction so no branch is
/// unreachable-by-panic on a hostile caller: a non-`Symbolic` never reaches the map.
fn into_symbols(terms: Vec<Term>) -> Result<Vec<Symbol>, Vec<Term>> {
    if terms.iter().all(|term| matches!(term, Term::Symbolic(_))) {
        Ok(terms.into_iter().map(unwrap_symbolic).collect())
    } else {
        Err(terms)
    }
}

/// The ground symbol of a `Symbolic` leaf. The caller (`into_symbols`) checks every
/// element is `Symbolic` first, so the fallback is unreachable — kept total.
fn unwrap_symbolic(term: Term) -> Symbol {
    // Through `into_parts`, whose mem-extraction is the sound way to take a field out
    // of a `Drop`-carrying `Term` (§3.6); the caller guarantees the `Symbolic` arm.
    match term.into_parts() {
        TermParts::Symbolic(symbol) => symbol,
        _ => unreachable!("into_symbols checks every element is Symbolic first"),
    }
}

impl From<TermParts<Term>> for Term {
    fn from(parts: TermParts<Term>) -> Term {
        match parts {
            TermParts::Variable(v) => Term::Variable(v),
            TermParts::Symbolic(s) => Term::Symbolic(s),
            TermParts::Function { name, arguments } => Term::Function { name, arguments },
            TermParts::Tuple(items) => Term::Tuple(items),
            TermParts::Pool(items) => Term::Pool(items),
            TermParts::UnaryOperation { operator, argument } => Term::UnaryOperation {
                operator,
                argument: Box::new(argument),
            },
            TermParts::BinaryOperation {
                operator,
                left,
                right,
            } => Term::BinaryOperation {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            TermParts::Interval { lower, upper } => Term::Interval {
                lower: Box::new(lower),
                upper: Box::new(upper),
            },
            TermParts::Absolute(inner) => Term::Absolute(Box::new(inner)),
            TermParts::External { name, arguments } => Term::External { name, arguments },
        }
    }
}

/// A term node's non-child data, split from its children for the iterative owned
/// walks (`fold`, `clone`) — the reassembly key a work list carries while the
/// children are folded or cloned (§3.6).
enum Shell {
    Variable(Variable),
    Symbolic(Symbol),
    Function(Name),
    Tuple,
    Pool,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Interval,
    Absolute,
    External(Name),
}

/// Split a decomposed term into its shell and its owned children (§3.6).
fn split_parts(parts: TermParts<Term>) -> (Shell, Vec<Term>) {
    match parts {
        TermParts::Variable(v) => (Shell::Variable(v), Vec::new()),
        TermParts::Symbolic(s) => (Shell::Symbolic(s), Vec::new()),
        TermParts::Function { name, arguments } => (Shell::Function(name), arguments),
        TermParts::Tuple(items) => (Shell::Tuple, items),
        TermParts::Pool(items) => (Shell::Pool, items),
        TermParts::UnaryOperation { operator, argument } => {
            (Shell::Unary(operator), vec![argument])
        }
        TermParts::BinaryOperation {
            operator,
            left,
            right,
        } => (Shell::Binary(operator), vec![left, right]),
        TermParts::Interval { lower, upper } => (Shell::Interval, vec![lower, upper]),
        TermParts::Absolute(inner) => (Shell::Absolute, vec![inner]),
        TermParts::External { name, arguments } => (Shell::External(name), arguments),
    }
}

/// Split a borrowed term into its shell (name and leaves cloned) and its borrowed
/// children — `clone`'s decomposition, mirroring `split_parts` (§3.6).
fn split_refs(term: &Term) -> (Shell, Vec<&Term>) {
    match term {
        Term::Variable(v) => (Shell::Variable(v.clone()), Vec::new()),
        Term::Symbolic(s) => (Shell::Symbolic(s.clone()), Vec::new()),
        Term::Function { name, arguments } => {
            (Shell::Function(name.clone()), arguments.iter().collect())
        }
        Term::Tuple(items) => (Shell::Tuple, items.iter().collect()),
        Term::Pool(items) => (Shell::Pool, items.iter().collect()),
        Term::UnaryOperation { operator, argument } => (Shell::Unary(*operator), vec![&**argument]),
        Term::BinaryOperation {
            operator,
            left,
            right,
        } => (Shell::Binary(*operator), vec![&**left, &**right]),
        Term::Interval { lower, upper } => (Shell::Interval, vec![&**lower, &**upper]),
        Term::Absolute(inner) => (Shell::Absolute, vec![&**inner]),
        Term::External { name, arguments } => {
            (Shell::External(name.clone()), arguments.iter().collect())
        }
    }
}

/// Reassemble a shell and its folded children into `TermParts` — the inverse of the
/// splits, shared by `fold` and `clone` (§3.6). The boxed forms pop their children in
/// the reverse of the split's push, restoring document order.
fn assemble_parts<T>(shell: Shell, mut children: Vec<T>) -> TermParts<T> {
    match shell {
        Shell::Variable(v) => TermParts::Variable(v),
        Shell::Symbolic(s) => TermParts::Symbolic(s),
        Shell::Function(name) => TermParts::Function {
            name,
            arguments: children,
        },
        Shell::Tuple => TermParts::Tuple(children),
        Shell::Pool => TermParts::Pool(children),
        Shell::Unary(operator) => TermParts::UnaryOperation {
            operator,
            argument: children.pop().expect("the unary operand"),
        },
        Shell::Binary(operator) => {
            let right = children.pop().expect("the binary right operand");
            let left = children.pop().expect("the binary left operand");
            TermParts::BinaryOperation {
                operator,
                left,
                right,
            }
        }
        Shell::Interval => {
            let upper = children.pop().expect("the interval upper bound");
            let lower = children.pop().expect("the interval lower bound");
            TermParts::Interval { lower, upper }
        }
        Shell::Absolute => TermParts::Absolute(children.pop().expect("the absolute operand")),
        Shell::External(name) => TermParts::External {
            name,
            arguments: children,
        },
    }
}

/// Pushes a term's immediate child references onto `stack`, reversed so a pre-order
/// walk pops them left-to-right (§3.6). A `Symbolic` leaf has no term children.
fn push_child_refs_reversed<'a>(term: &'a Term, stack: &mut Vec<&'a Term>) {
    match term {
        Term::Variable(_) | Term::Symbolic(_) => {}
        Term::Function { arguments, .. } | Term::External { arguments, .. } => {
            stack.extend(arguments.iter().rev());
        }
        Term::Tuple(items) | Term::Pool(items) => stack.extend(items.iter().rev()),
        Term::UnaryOperation { argument, .. } => stack.push(&**argument),
        Term::BinaryOperation { left, right, .. } => {
            stack.push(&**right);
            stack.push(&**left);
        }
        Term::Interval { lower, upper } => {
            stack.push(&**upper);
            stack.push(&**lower);
        }
        Term::Absolute(inner) => stack.push(&**inner),
    }
}

impl Clone for Term {
    fn clone(&self) -> Term {
        // Post-order deep copy (§13): enter each node, then rebuild bottom-up from a
        // stack of finished clones. A `Symbolic` leaf clones its symbol whole (the
        // symbol's own clone is iterative, §3.1).
        enum Frame<'a> {
            Enter(&'a Term),
            Assemble(Shell, usize),
        }
        let mut work = vec![Frame::Enter(self)];
        let mut done: Vec<Term> = Vec::new();
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
                    done.push(Term::from(assemble_parts(shell, children)));
                }
            }
        }
        done.pop().expect("the root's clone")
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        // Dismantle iteratively (§13): move every descendant onto a work list and
        // drop them one at a time, so a deep term drops without recursion. A
        // `Symbolic`'s symbol drops through the symbol's own iterative `Drop`.
        let mut stack: Vec<Term> = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut term) = stack.pop() {
            take_children(&mut term, &mut stack);
        }
    }
}

/// Moves a term's immediate child terms onto `out`, leaving it childless. A boxed
/// child is swapped for a trivial sentinel leaf the box keeps and drops in O(1).
fn take_children(term: &mut Term, out: &mut Vec<Term>) {
    match term {
        Term::Variable(_) | Term::Symbolic(_) => {}
        Term::Function { arguments, .. } | Term::External { arguments, .. } => {
            out.append(arguments);
        }
        Term::Tuple(items) | Term::Pool(items) => out.append(items),
        Term::UnaryOperation { argument, .. } => out.push(take_box(argument)),
        Term::BinaryOperation { left, right, .. } => {
            out.push(take_box(left));
            out.push(take_box(right));
        }
        Term::Interval { lower, upper } => {
            out.push(take_box(lower));
            out.push(take_box(upper));
        }
        Term::Absolute(inner) => out.push(take_box(inner)),
    }
}

/// Moves the term out of a box, leaving a trivial sentinel leaf behind (§3.6).
fn take_box(boxed: &mut Box<Term>) -> Term {
    std::mem::replace(boxed.as_mut(), Term::Variable(Variable::Anonymous))
}

impl PartialEq for Term {
    fn eq(&self, other: &Term) -> bool {
        // Iterative structural equality (§13): a work list of pairs, returning on the
        // first mismatch.
        let mut pairs: Vec<(&Term, &Term)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            match (a, b) {
                (Term::Variable(x), Term::Variable(y)) if x == y => {}
                (Term::Symbolic(x), Term::Symbolic(y)) if x == y => {}
                (
                    Term::Function {
                        name: n1,
                        arguments: a1,
                    },
                    Term::Function {
                        name: n2,
                        arguments: a2,
                    },
                ) if n1 == n2 && a1.len() == a2.len() => pairs.extend(a1.iter().zip(a2)),
                (Term::Tuple(a1), Term::Tuple(a2)) | (Term::Pool(a1), Term::Pool(a2))
                    if a1.len() == a2.len() =>
                {
                    pairs.extend(a1.iter().zip(a2));
                }
                (
                    Term::UnaryOperation {
                        operator: o1,
                        argument: g1,
                    },
                    Term::UnaryOperation {
                        operator: o2,
                        argument: g2,
                    },
                ) if o1 == o2 => pairs.push((&**g1, &**g2)),
                (
                    Term::BinaryOperation {
                        operator: o1,
                        left: l1,
                        right: r1,
                    },
                    Term::BinaryOperation {
                        operator: o2,
                        left: l2,
                        right: r2,
                    },
                ) if o1 == o2 => {
                    pairs.push((&**l1, &**l2));
                    pairs.push((&**r1, &**r2));
                }
                (
                    Term::Interval {
                        lower: lo1,
                        upper: up1,
                    },
                    Term::Interval {
                        lower: lo2,
                        upper: up2,
                    },
                ) => {
                    pairs.push((&**lo1, &**lo2));
                    pairs.push((&**up1, &**up2));
                }
                (Term::Absolute(t1), Term::Absolute(t2)) => pairs.push((&**t1, &**t2)),
                (
                    Term::External {
                        name: n1,
                        arguments: a1,
                    },
                    Term::External {
                        name: n2,
                        arguments: a2,
                    },
                ) if n1 == n2 && a1.len() == a2.len() => pairs.extend(a1.iter().zip(a2)),
                _ => return false,
            }
        }
        true
    }
}
impl Eq for Term {}

impl PartialOrd for Term {
    fn partial_cmp(&self, other: &Term) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Term {
    fn cmp(&self, other: &Term) -> Ordering {
        // A consistent total order agreeing with `Eq` (§3.3): no external authority,
        // unlike `Symbol`'s clingo order — the variant rank, then the head scalars,
        // then the children by count-then-elements (length-major, so equal counts
        // descend and the work list never needs a mid-walk prefix comparison).
        // Iterative, returning on the first difference. The naive twin holds it
        // honest (tests/term_laws.rs).
        let mut pairs: Vec<(&Term, &Term)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            let by_rank = term_rank(a).cmp(&term_rank(b));
            if by_rank != Ordering::Equal {
                return by_rank;
            }
            let here = match (a, b) {
                (Term::Variable(x), Term::Variable(y)) => x.cmp(y),
                (Term::Symbolic(x), Term::Symbolic(y)) => x.cmp(y),
                (
                    Term::Function {
                        name: n1,
                        arguments: a1,
                    },
                    Term::Function {
                        name: n2,
                        arguments: a2,
                    },
                ) => (n1, a1.len()).cmp(&(n2, a2.len())).then_with(|| {
                    push_pairs_reversed(&mut pairs, a1, a2);
                    Ordering::Equal
                }),
                (Term::Tuple(a1), Term::Tuple(a2)) => a1.len().cmp(&a2.len()).then_with(|| {
                    push_pairs_reversed(&mut pairs, a1, a2);
                    Ordering::Equal
                }),
                (Term::Pool(a1), Term::Pool(a2)) => a1.len().cmp(&a2.len()).then_with(|| {
                    push_pairs_reversed(&mut pairs, a1, a2);
                    Ordering::Equal
                }),
                (
                    Term::UnaryOperation {
                        operator: o1,
                        argument: g1,
                    },
                    Term::UnaryOperation {
                        operator: o2,
                        argument: g2,
                    },
                ) => o1.cmp(o2).then_with(|| {
                    pairs.push((&**g1, &**g2));
                    Ordering::Equal
                }),
                (
                    Term::BinaryOperation {
                        operator: o1,
                        left: l1,
                        right: r1,
                    },
                    Term::BinaryOperation {
                        operator: o2,
                        left: l2,
                        right: r2,
                    },
                ) => o1.cmp(o2).then_with(|| {
                    pairs.push((&**r1, &**r2));
                    pairs.push((&**l1, &**l2));
                    Ordering::Equal
                }),
                (
                    Term::Interval {
                        lower: lo1,
                        upper: up1,
                    },
                    Term::Interval {
                        lower: lo2,
                        upper: up2,
                    },
                ) => {
                    pairs.push((&**up1, &**up2));
                    pairs.push((&**lo1, &**lo2));
                    Ordering::Equal
                }
                (Term::Absolute(t1), Term::Absolute(t2)) => {
                    pairs.push((&**t1, &**t2));
                    Ordering::Equal
                }
                (
                    Term::External {
                        name: n1,
                        arguments: a1,
                    },
                    Term::External {
                        name: n2,
                        arguments: a2,
                    },
                ) => (n1, a1.len()).cmp(&(n2, a2.len())).then_with(|| {
                    push_pairs_reversed(&mut pairs, a1, a2);
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

/// The variant's rank in `Term`'s order — declaration order (§3.3).
fn term_rank(term: &Term) -> u8 {
    match term {
        Term::Variable(_) => 0,
        Term::Symbolic(_) => 1,
        Term::Function { .. } => 2,
        Term::Tuple(_) => 3,
        Term::Pool(_) => 4,
        Term::UnaryOperation { .. } => 5,
        Term::BinaryOperation { .. } => 6,
        Term::Interval { .. } => 7,
        Term::Absolute(_) => 8,
        Term::External { .. } => 9,
    }
}

/// Pushes the element pairs of two equal-length slices onto `pairs`, reversed so the
/// leftmost is compared first (the length was compared in the head).
fn push_pairs_reversed<'a>(pairs: &mut Vec<(&'a Term, &'a Term)>, a: &'a [Term], b: &'a [Term]) {
    for pair in a.iter().zip(b.iter()).rev() {
        pairs.push(pair);
    }
}

impl Hash for Term {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Iterative pre-order hash (§13): each node's rank and head scalars, then a
        // length marker for its children, in document order — the same content
        // projection as `Eq` (§5.2), so equal terms hash equal.
        for term in self.subterms() {
            match term {
                Term::Variable(v) => {
                    state.write_u8(0);
                    v.hash(state);
                }
                Term::Symbolic(s) => {
                    state.write_u8(1);
                    s.hash(state);
                }
                Term::Function { name, arguments } => {
                    state.write_u8(2);
                    name.hash(state);
                    state.write_usize(arguments.len());
                }
                Term::Tuple(items) => {
                    state.write_u8(3);
                    state.write_usize(items.len());
                }
                Term::Pool(items) => {
                    state.write_u8(4);
                    state.write_usize(items.len());
                }
                Term::UnaryOperation { operator, .. } => {
                    state.write_u8(5);
                    operator.hash(state);
                }
                Term::BinaryOperation { operator, .. } => {
                    state.write_u8(6);
                    operator.hash(state);
                }
                Term::Interval { .. } => state.write_u8(7),
                Term::Absolute(_) => state.write_u8(8),
                Term::External { name, arguments } => {
                    state.write_u8(9);
                    name.hash(state);
                    state.write_usize(arguments.len());
                }
            }
        }
    }
}

/// A print action for the iterative `Debug` — a node to render or a static separator
/// (no closing carries a runtime value, so no owned string is needed).
enum DebugAct<'a> {
    Node(&'a Term),
    Str(&'static str),
}

impl std::fmt::Debug for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A derived-shaped `Debug`, iterative (§14): rendered from an explicit work
        // list of print actions, so a deep term formats without recursion. A boxed
        // child prints transparently, exactly as a derived `Debug` over `Box` does.
        let mut work = vec![DebugAct::Node(self)];
        while let Some(act) = work.pop() {
            match act {
                DebugAct::Str(s) => f.write_str(s)?,
                DebugAct::Node(term) => match term {
                    Term::Variable(v) => write!(f, "Variable({v:?})")?,
                    Term::Symbolic(s) => write!(f, "Symbolic({s:?})")?,
                    Term::Function { name, arguments } => {
                        write!(f, "Function {{ name: {name:?}, arguments: [")?;
                        work.push(DebugAct::Str("] }"));
                        push_debug_list(&mut work, arguments);
                    }
                    Term::Tuple(items) => {
                        f.write_str("Tuple([")?;
                        work.push(DebugAct::Str("])"));
                        push_debug_list(&mut work, items);
                    }
                    Term::Pool(items) => {
                        f.write_str("Pool([")?;
                        work.push(DebugAct::Str("])"));
                        push_debug_list(&mut work, items);
                    }
                    Term::UnaryOperation { operator, argument } => {
                        write!(f, "UnaryOperation {{ operator: {operator:?}, argument: ")?;
                        work.push(DebugAct::Str(" }"));
                        work.push(DebugAct::Node(argument));
                    }
                    Term::BinaryOperation {
                        operator,
                        left,
                        right,
                    } => {
                        write!(f, "BinaryOperation {{ operator: {operator:?}, left: ")?;
                        work.push(DebugAct::Str(" }"));
                        work.push(DebugAct::Node(right));
                        work.push(DebugAct::Str(", right: "));
                        work.push(DebugAct::Node(left));
                    }
                    Term::Interval { lower, upper } => {
                        f.write_str("Interval { lower: ")?;
                        work.push(DebugAct::Str(" }"));
                        work.push(DebugAct::Node(upper));
                        work.push(DebugAct::Str(", upper: "));
                        work.push(DebugAct::Node(lower));
                    }
                    Term::Absolute(inner) => {
                        f.write_str("Absolute(")?;
                        work.push(DebugAct::Str(")"));
                        work.push(DebugAct::Node(inner));
                    }
                    Term::External { name, arguments } => {
                        write!(f, "External {{ name: {name:?}, arguments: [")?;
                        work.push(DebugAct::Str("] }"));
                        push_debug_list(&mut work, arguments);
                    }
                },
            }
        }
        Ok(())
    }
}

/// Pushes a sequence of children as `Debug` nodes, reversed and comma-separated so
/// they print left-to-right (§14).
fn push_debug_list<'a>(work: &mut Vec<DebugAct<'a>>, items: &'a [Term]) {
    for (i, child) in items.iter().enumerate().rev() {
        work.push(DebugAct::Node(child));
        if i > 0 {
            work.push(DebugAct::Str(", "));
        }
    }
}
