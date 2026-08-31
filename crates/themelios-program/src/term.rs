//! The non-ground term algebra generalizing the ground `Symbol`
//! (docs/design/program.md §3.3, §3.6, §3.7, §5.1). `Term` recurses through
//! `Box` (the operator and interval forms) and `Vec` (the applied and grouped
//! forms); like `Symbol` it is owned plain data whose every walk — clone, drop,
//! equality, ordering, hashing, debug, and the `fold` rebuild — is iterative
//! (§13), so a term tens of thousands of levels deep is handled without touching
//! the call stack. Term-level canonicalization collapses maximal ground
//! constructor subterms to symbols, drops a degenerate one-alternative pool, and
//! flattens nested pools (§5.1); operators never fold (§3.5).

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
    /// check: an `External` (`@`-call) with variable-free arguments reads as ground, as the
    /// grounder treats it, though `evaluate` still refuses it at the ground door (§3.5).
    /// Iterative; O(nodes).
    pub fn is_ground(&self) -> bool {
        !self
            .subterms()
            .any(|term| matches!(term, Term::Variable(_)))
    }

    /// A pool of alternatives `(a; b; …)` (§5.1), refusing an empty one — a pool is a disjunction
    /// of one or more alternatives, and a zero-alternative pool is a malformed value, not a normal
    /// form (a validity invariant refused at the door, §7.2). One alternative collapses to its term
    /// at canonicalization, nested pools flatten (§5.1). This is the door a caller builds a pool
    /// through; the `Pool` variant stays public for reading and carries the same non-empty
    /// precondition. `O(alternatives)`.
    pub fn pool(alternatives: impl IntoIterator<Item = Term>) -> Result<Term, EmptyPool> {
        let alternatives: Vec<Term> = alternatives.into_iter().collect();
        if alternatives.is_empty() {
            return Err(EmptyPool);
        }
        Ok(Term::Pool(alternatives).canonicalize())
    }
}

/// A pool constructed with no alternatives — a malformed value refused at the constructor door
/// (§5.1, §7.2). A pool is a disjunction of one or more alternatives; its ≥2 normal form is
/// canonicalization's (§5.1), but the emptiness is a validity invariant a constructor refuses.
/// Returned by [`Term::pool`] and [`Atom::pooled`](crate::program::Atom::pooled).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EmptyPool;

impl std::fmt::Display for EmptyPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a pool has no alternatives")
    }
}
impl std::error::Error for EmptyPool {}

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
    /// one bottom-up pass, O(nodes)). Three syntactic normalizations — the maximal
    /// ground *constructor* collapse to a `Symbolic`, the degenerate one-alternative
    /// pool, and unary minus of a number folded to its negation. That last is the
    /// *only* operator that folds (§3.5): `-5` is the integer −5 (the grammar has no
    /// negative numeral, grammar §4.3), so `Number(-5)` is its canonical form — the
    /// number's spelling, not arithmetic evaluation — and a double `-(-5)` folds to
    /// `Number(5)`, the authority's own reading; `-(1 + 2)` is not a number and stays
    /// a `BinaryOperation`. A pool holding a pool is flattened afterward in one
    /// top-down pass ([`flatten_pools`]), also O(nodes) and only when the fold met one,
    /// so the whole canonicalization is O(nodes) whatever the nesting. A pass, not a
    /// constructor guarantee: a caller can build a non-canonical term directly, and
    /// every door that admits a term into a program runs this.
    #[must_use]
    pub fn canonicalize(self) -> Term {
        let mut nested_pool = false;
        let folded = self.fold(|parts| match parts {
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
                // The ≥2 normal form (§5.1) and a flag for a pool holding a pool; the flattening
                // itself is one top-down pass below (`flatten_pools`), so a nested chain is O(depth)
                // in *either* nesting direction, never re-merging a growing vector. A one-alternative
                // pool collapses to its element (§5.1); an empty pool is refused at the constructor
                // door ([`Term::pool`] returns `Err`), the raw `Pool` variant carrying the same
                // non-empty precondition, so a hand-built empty one is kept, not refused here.
                nested_pool |= items.iter().any(|item| matches!(item, Term::Pool(_)));
                if items.len() == 1 {
                    items.pop().expect("a one-alternative pool has its element")
                } else {
                    Term::Pool(items)
                }
            }
            // Unary minus of a number folds to its negation (§3.5, §5.1): `-5` → `Number(-5)`,
            // so it round-trips — `render` writes `-5` (the grammar has no negative numeral) and
            // the raise reads it back as this fold's input; a double `-(-5)` folds to `Number(5)`.
            TermParts::UnaryOperation {
                operator: UnaryOp::Negate,
                argument,
            } => negate_number(argument),
            other => Term::from(other),
        });
        // Pooling is associative — `((a; b); c)` is `(a; b; c)`, verified against clingo 5.8.2 — so
        // flatten a pool holding a pool, but only when the bottom-up fold met one (the common flat
        // pool skips this entirely). One top-down pass, O(nodes) in *either* nesting direction.
        if nested_pool {
            flatten_pools(folded)
        } else {
            folded
        }
    }
}

/// Fold unary minus of a number to its negation (§5.1, §3.5) — the one operator canonicalization,
/// so a double `-(-5)` folds to `Number(5)` (the authority's reading). A non-number argument, or a
/// value whose negation leaves the `i32` range (`-i32::MIN`), keeps the `Negate` form.
fn negate_number(argument: Term) -> Term {
    if let Term::Symbolic(Symbol::Number(value)) = &argument
        && let Some(negated) = value.checked_neg()
    {
        return Term::Symbolic(Symbol::Number(negated));
    }
    Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(argument),
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

/// Flatten every nested pool of `term` — `((a; b); c)` to `(a; b; c)`, pooling being associative
/// (verified against clingo 5.8.2) — in one top-down pass, O(nodes) in *both* nesting directions,
/// so neither a left- nor a right-nested chain re-merges a growing vector (`canonicalize`'s bottom-up
/// reuse trick was O(depth²) right-nested). Iterative (§13): the work list and the spine descent hold
/// depth on the heap, never the call stack. `canonicalize` runs this only when its fold met a pool
/// holding a pool, so a flat pool — the common case — never reaches here.
fn flatten_pools(term: Term) -> Term {
    enum Frame {
        Enter(Term),
        Assemble(Shell, usize),
    }
    let mut work = vec![Frame::Enter(term)];
    let mut done: Vec<Term> = Vec::new();
    while let Some(frame) = work.pop() {
        match frame {
            Frame::Enter(term) => match term.into_parts() {
                // Splice the whole maximal pool spine into one flat alternative list in a single
                // descent, then re-enter each alternative so a pool nested inside a compound
                // alternative (`f((a; b))`) is flattened too. The spine's inner pools are consumed
                // here, so every gathered alternative is itself pool-free at the top.
                TermParts::Pool(items) => {
                    let alternatives = gather_pool_spine(items);
                    work.push(Frame::Assemble(Shell::Pool, alternatives.len()));
                    for alternative in alternatives.into_iter().rev() {
                        work.push(Frame::Enter(alternative));
                    }
                }
                parts => {
                    let (shell, children) = split_parts(parts);
                    work.push(Frame::Assemble(shell, children.len()));
                    for child in children.into_iter().rev() {
                        work.push(Frame::Enter(child));
                    }
                }
            },
            Frame::Assemble(shell, arity) => {
                let children = done.split_off(done.len() - arity);
                done.push(Term::from(assemble_parts(shell, children)));
            }
        }
    }
    done.pop()
        .expect("flatten_pools leaves exactly the rewritten root")
}

/// The flat alternatives of a (possibly nested) pool, document order preserved — a `Pool`
/// alternative's own alternatives spliced in place of it, pooling being associative. One iterative
/// descent, O(total alternatives) whatever the nesting direction: each alternative is visited once,
/// and the nesting depth lives on the heap work stack, not the call stack (§13). Returned
/// alternatives are pool-free at the top (a nested pool is spliced, never emitted).
fn gather_pool_spine(alternatives: Vec<Term>) -> Vec<Term> {
    let mut flat = Vec::new();
    // Reversed so the first alternative is popped first — a `Pool` is spliced by pushing its own
    // (reversed) alternatives, keeping the left-to-right reading of the whole spine.
    let mut stack: Vec<Term> = alternatives.into_iter().rev().collect();
    while let Some(alternative) = stack.pop() {
        match alternative.into_parts() {
            TermParts::Pool(nested) => stack.extend(nested.into_iter().rev()),
            parts => flat.push(Term::from(parts)),
        }
    }
    flat
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

/// Why a ground evaluation refused (§3.5). Each carries the offending value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EvalError {
    /// A variable occurred — the term is not ground.
    NotGround {
        /// The variable that occurred.
        variable: Variable,
    },
    /// An `@`-call: evaluation needs a registered context (spec §9.6), the solve
    /// tier's, so this tier reports rather than guesses.
    External {
        /// The extension name.
        name: Name,
    },
    /// Division or modulo by zero, arithmetic over a non-number, a set-former (a pool
    /// or interval) where a single symbol is required, or another operation the
    /// authority rejects.
    Undefined,
    /// A result outside the `i32` range. This door refuses rather than wrap — a
    /// silently wrapped sum is the undetectable wrong answer this estate forbids (§3.5,
    /// spec §5.2). The differential records the authority's own overflow behaviour
    /// beside this refusal.
    Overflow,
}

impl Term {
    /// Evaluate a ground term to the symbol it denotes (§3.5), folding arithmetic
    /// faithfully to the authority's ground-term evaluation (grammar §5.10). Iterative
    /// (§13); O(nodes). A variable, an unevaluated `@`-call, an undefined operation, a
    /// set-former, or an out-of-range result refuses. The one place this tier evaluates
    /// arithmetic — a `1 + 2` embedded in a rule stays a `BinaryOperation` (§3.5, §5.1).
    pub fn evaluate(&self) -> Result<Symbol, EvalError> {
        evaluate(self)
    }
}

/// Evaluate a ground term to the symbol it denotes (§3.5). See [`Term::evaluate`].
pub fn evaluate(term: &Term) -> Result<Symbol, EvalError> {
    // Written in the iterative `try_fold` (§13), so a deep term evaluates without
    // recursion; the walk consumes, so this clones first (O(nodes), as evaluation is).
    // Bottom-up: a child's refusal short-circuits before its parent's step.
    term.clone().try_fold(|parts| match parts {
        TermParts::Symbolic(symbol) => Ok(symbol),
        TermParts::Variable(variable) => Err(EvalError::NotGround { variable }),
        TermParts::External { name, .. } => Err(EvalError::External { name }),
        // A term-position functor bears no strong sign (§3.3): the collapsed symbol is
        // Positive. Its children have already evaluated to symbols.
        TermParts::Function { name, arguments } => Ok(Symbol::Function {
            name,
            arguments,
            sign: Sign::Positive,
        }),
        TermParts::Tuple(items) => Ok(Symbol::Tuple(items)),
        TermParts::UnaryOperation { operator, argument } => {
            apply_unary(operator, as_number(&argument)?).map(Symbol::Number)
        }
        TermParts::BinaryOperation {
            operator,
            left,
            right,
        } => {
            let (left, right) = (as_number(&left)?, as_number(&right)?);
            apply_binary(operator, left, right).map(Symbol::Number)
        }
        TermParts::Absolute(inner) => as_number(&inner)?
            .checked_abs()
            .ok_or(EvalError::Overflow)
            .map(Symbol::Number),
        // A pool or interval names a *set*, not a single symbol, so it is not a value term
        // and `evaluate` reports it `Undefined` (§3.5).
        TermParts::Pool(_) | TermParts::Interval { .. } => Err(EvalError::Undefined),
    })
}

/// The `i32` a symbol carries, or `Undefined` when it is not a number — arithmetic
/// over a non-number (§3.5).
fn as_number(symbol: &Symbol) -> Result<i32, EvalError> {
    match symbol {
        Symbol::Number(n) => Ok(*n),
        _ => Err(EvalError::Undefined),
    }
}

/// Apply a prefix operator with checked arithmetic (§3.5, grammar §5.10): overflow
/// refuses rather than wraps.
fn apply_unary(operator: UnaryOp, operand: i32) -> Result<i32, EvalError> {
    match operator {
        UnaryOp::Negate => operand.checked_neg().ok_or(EvalError::Overflow),
        UnaryOp::BitwiseNot => Ok(!operand),
    }
}

/// Apply an infix operator with checked arithmetic (§3.5, grammar §5.10): division or
/// modulo by zero refuses `Undefined`, a result outside `i32` refuses `Overflow`. The
/// exact per-operator semantics — the sign of division and modulo, a negative exponent
/// — are pinned by the differential against the authority (§16).
fn apply_binary(operator: BinaryOp, left: i32, right: i32) -> Result<i32, EvalError> {
    match operator {
        BinaryOp::Add => left.checked_add(right).ok_or(EvalError::Overflow),
        BinaryOp::Sub => left.checked_sub(right).ok_or(EvalError::Overflow),
        BinaryOp::Mul => left.checked_mul(right).ok_or(EvalError::Overflow),
        BinaryOp::Div | BinaryOp::Mod if right == 0 => Err(EvalError::Undefined),
        BinaryOp::Div => left.checked_div(right).ok_or(EvalError::Overflow),
        BinaryOp::Mod => left.checked_rem(right).ok_or(EvalError::Overflow),
        BinaryOp::Pow if right < 0 => Err(EvalError::Undefined),
        BinaryOp::Pow => {
            let exponent = u32::try_from(right).map_err(|_| EvalError::Overflow)?;
            left.checked_pow(exponent).ok_or(EvalError::Overflow)
        }
        BinaryOp::BitAnd => Ok(left & right),
        BinaryOp::BitOr => Ok(left | right),
        BinaryOp::BitXor => Ok(left ^ right),
    }
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::NotGround { variable } => {
                write!(f, "not ground: the variable {variable:?}")
            }
            EvalError::External { name } => write!(f, "cannot evaluate the external call {name:?}"),
            EvalError::Undefined => f.write_str("undefined operation"),
            EvalError::Overflow => f.write_str("arithmetic overflow"),
        }
    }
}
impl std::error::Error for EvalError {}
