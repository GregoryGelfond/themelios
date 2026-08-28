//! The depth proof (docs/design/program.md §13, §16; docs/specification.md §7.2, §10.1).
//!
//! This tier's values are plain trees its own work-list walks traverse: there is no
//! nesting limit and no depth refusal — any depth the heap holds is *handled* (§13),
//! unlike the syntax tier's bounded parser (syntax §6.6). The in-suite depth canaries
//! (`symbol_laws.rs`, `term_laws.rs`, `theory_term_laws.rs`) already run the derived
//! walks — clone, equality, order, hash, the traversal, and fold — on a ~200,000-deep
//! value, but on the large default test stack, and do not exercise render, substitute,
//! or evaluate: enough to catch an accidental recursion the moment it lands, not a
//! rigorous or complete proof. This makes it
//! rigorous: on a **stated small stack** a recursive walk of the same depth would
//! overflow, every program-tier walk over a value nested ~200,000 levels deep — far past
//! the raise's bounded tree or any real construction — completes without overflow.
//!
//! The walks (§13, §16): clone, drop, `PartialEq`/`Eq`, `PartialOrd`/`Ord`, `Hash`,
//! `render`, `substitute`, `evaluate`, `canonicalize`, `fold`, and the pre-order
//! traversal — over the self-recursive families of grammar §10 (the term, its
//! constant-term and value-term subsets, and the theory term), realized here as `Symbol`,
//! `Term`, and `TheoryTerm`. Each family is nested through **every structural recursion
//! shape**, so no walk is proven on one arm alone: sequence children (`Function`, and the
//! `Symbol::Tuple`/`Term::Tuple` arm the iterative `Drop`'s `take_children` moves apart
//! from Function — `Pool` and `External` share this shape), a single boxed child
//! (`UnaryOperation`; `Absolute` shares it), and two boxed children (`BinaryOperation`;
//! `Interval` shares it). `substitute` is exercised two ways: over a deep term, and
//! resolving a deep **triangular** substitution — a dereference chain `X₀↦f(X₁), X₁↦f(X₂),
//! …` of ~200,000 links that is its own recursion (§11.1), obtained the only public way,
//! from a successful `mgu`. Every walk returns a value, never a refusal: this tier holds
//! no depth limit to consult (§13).
//!
//! The pole is real: a deliberately recursive reference walk of the same shape, on the
//! same stated stack, *does* overflow — so the iterative walks' survival is a proof, not
//! slack. A Rust stack overflow aborts the process rather than unwinding a catchable
//! panic, so an overflow ends a process, not a test: the control runs in a child process
//! (a re-exec of this test binary) whose crash the parent asserts, the pattern the syntax
//! tier's own depth proof uses.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::process::{Command, Stdio};
use std::thread;

use themelios_syntax::dialect::Dialect;

use themelios_program::program::{
    Atom, Head, Program, Rule, Statement, TheoryAtom, TheoryElement, TheoryTerm,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};
use themelios_program::unify::mgu;

/// The depth every walk is proven stack-independent at — far past the raise's bounded
/// tree or any construction a real program performs, and the depth the in-suite canaries
/// already exercise on the default stack.
const DEPTH: usize = 200_000;

/// The stated pole: the small stack every iterative walk survives and a recursive walk of
/// the same depth overflows. Calibrated, not arbitrary: a
/// naive recursive walk overflows this stack at ~1.5k frames — the `DEPTH` control is
/// ~100× past that — while the iterative walks survive it with ≥8× headroom (they clear
/// even a 32 KiB stack over `DEPTH`). Both margins hold across the supported platforms;
/// the frame sizes are the same order everywhere.
const STATED_STACK_BYTES: usize = 256 * 1024;

/// The environment variable the recursive control's child process reads (unset in a
/// normal run, so the control entry is a no-op then).
const CONTROL: &str = "THEMELIOS_PROGRAM_DEPTH_CONTROL";

/// Run `body` on a freshly spawned thread of exactly `bytes` of stack and return its
/// result — the survival side. A walk that overflows here aborts the whole process (a
/// stack overflow is not a catchable panic), which is the regression this proof guards
/// against; a walk that merely panics surfaces as this `join` failing.
fn on_stack<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(bytes: usize, body: F) -> R {
    thread::Builder::new()
        .name(format!("depth-proof-{bytes}"))
        .stack_size(bytes)
        .spawn(body)
        .expect("the proof's thread spawns")
        .join()
        .expect("the proof's thread completes")
}

// ---- the deep-value builders (iterative, so construction itself does not overflow) ----

/// A left-nested ground symbol `f(f(… c …))` of `depth` levels — the shape a recursive
/// construction produces (§13), mirroring `symbol_laws.rs`'s canary builder.
fn deep_symbol(depth: usize) -> Symbol {
    let mut symbol = Symbol::Function {
        name: name("c"),
        arguments: vec![],
        sign: Sign::Positive,
    };
    for _ in 0..depth {
        symbol = Symbol::Function {
            name: name("f"),
            arguments: vec![symbol],
            sign: Sign::Positive,
        };
    }
    symbol
}

/// A left-nested ground constructor term `f(f(… 0 …))` of `depth` levels — ground, so
/// `canonicalize` collapses the whole spine to one deep `Symbolic` and `evaluate` denotes
/// a deep symbol. Mirrors `term_laws.rs`'s `deep_via_function`.
fn deep_ground_term(depth: usize) -> Term {
    let mut term = Term::Symbolic(Symbol::Number(0));
    for _ in 0..depth {
        term = Term::Function {
            name: name("f"),
            arguments: vec![term],
        };
    }
    term
}

/// A nested unary term `-(-(… leaf …))` of `depth` levels over the given leaf — non-ground
/// where the leaf is a variable, so it stays a `Term` under `canonicalize` (unary minus
/// folds only over a *number*, §5.1). Mirrors `term_laws.rs`'s `deep_via_unary`.
fn deep_unary_term(leaf: Term, depth: usize) -> Term {
    let mut term = leaf;
    for _ in 0..depth {
        term = Term::UnaryOperation {
            operator: UnaryOp::Negate,
            argument: Box::new(term),
        };
    }
    term
}

/// A left-nested theory term `f(f(… _ …))` of `depth` levels — the theory algebra's own
/// self-recursive family (§4.9). Mirrors `theory_term_laws.rs`'s `deep_theory_term`.
fn deep_theory_term(depth: usize) -> TheoryTerm {
    let mut term = TheoryTerm::Variable(Variable::Anonymous);
    for _ in 0..depth {
        term = TheoryTerm::Function {
            name: name("f"),
            arguments: vec![term],
        };
    }
    term
}

/// A left-nested single-element ground tuple `((… c …))` of `depth` levels — the
/// `Symbol::Tuple` spine (§3.6). Its walks descend the same sequence-of-children shape as
/// `Function`, but through the iterative `Drop`'s separate `take_children` Tuple arm, so a
/// regression that dropped a tuple recursively is proven against here, not only Function.
fn deep_symbol_tuple(depth: usize) -> Symbol {
    let mut symbol = Symbol::Function {
        name: name("c"),
        arguments: vec![],
        sign: Sign::Positive,
    };
    for _ in 0..depth {
        symbol = Symbol::Tuple(vec![symbol]);
    }
    symbol
}

/// A left-nested single-element tuple term `((… 0 …))` of `depth` levels — the
/// `Term::Tuple` spine, the term algebra's other sequence-child arm.
fn deep_term_tuple(depth: usize) -> Term {
    let mut term = Term::Symbolic(Symbol::Number(0));
    for _ in 0..depth {
        term = Term::Tuple(vec![term]);
    }
    term
}

/// A left-nested binary term `(… ((0 + 0) + 0) …)` of `depth` levels down the left operand,
/// a constant right at each level — the two-boxed-child shape (`BinaryOperation`,
/// `Interval`), the walk shape neither the sequence nor the single box reaches. `2·depth +
/// 1` nodes: a binary node and a right leaf per level, plus the bottom-left leaf.
fn deep_binary_term(depth: usize) -> Term {
    let mut term = Term::Symbolic(Symbol::Number(0));
    for _ in 0..depth {
        term = Term::BinaryOperation {
            operator: BinaryOp::Add,
            left: Box::new(term),
            right: Box::new(Term::Symbolic(Symbol::Number(0))),
        };
    }
    term
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// The `i`-th standardized variable, `X{i}` — an uppercase identifier, so a variable
/// (grammar §5.1).
fn var(index: usize) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(format!("X{index}")).expect("a valid variable"),
    ))
}

/// A one-fact program `p(argument).`, the door through which a term or ground symbol is
/// rendered (`render` renders a program, §10) — its argument canonicalized at the
/// construction door (§5.1), then walked by `render`.
fn fact_of(argument: Term) -> Program {
    let fact = Rule::fact(Atom::new(name("p"), [argument]));
    Program::of([WithProvenance::constructed(Statement::Rule(fact))])
}

/// A one-fact program whose head is the theory atom `&a { theory_term }.` — the door
/// through which a theory term is rendered.
fn theory_fact_of(term: TheoryTerm) -> Program {
    let atom = TheoryAtom::new(name("a"), [], [TheoryElement::new([term], None)], None);
    let fact = Rule::fact(Head::TheoryAtom(atom));
    Program::of([WithProvenance::constructed(Statement::Rule(fact))])
}

/// A deliberately recursive reference walk over a term — what a naive or compiler-derived
/// walk would be, following the single child down the chain on the *call* stack. It is the
/// control: on the stated stack a term this deep overflows it. `black_box` keeps the
/// post-call use live, so the recursion is not turned into a loop.
fn recursive_walk(term: &Term) -> u64 {
    let child = match term {
        Term::UnaryOperation { argument, .. } => Some(&**argument),
        Term::Function { arguments, .. } => arguments.first(),
        _ => None,
    };
    let below = child.map_or(0, recursive_walk);
    std::hint::black_box(below.wrapping_add(1))
}

// ---- the laws (§13, §16): every walk survives the stated stack ----

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn every_walk_over_a_deep_symbol_survives_the_stated_stack() {
    on_stack(STATED_STACK_BYTES, || {
        let deep = deep_symbol(DEPTH); // constructed
        let same = deep.clone(); // Clone
        // Compared through bound bools so a (never-taken) failure does not Debug-render
        // two symbols this deep (the canaries' discipline).
        let clone_is_equal = deep == same; // PartialEq / Eq
        assert!(clone_is_equal);
        assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
        assert_eq!(deep.partial_cmp(&same), Some(Ordering::Equal)); // PartialOrd
        let mut hasher = DefaultHasher::new();
        deep.hash(&mut hasher); // Hash
        let _ = hasher.finish();
        assert_eq!(deep.subsymbols().count(), DEPTH + 1); // pre-order traversal
        let _ = format!("{deep:?}"); // Debug (iterative, §14)
        let rebuilt = deep.clone().fold(Symbol::from); // fold
        let fold_is_equal = rebuilt == same;
        assert!(fold_is_equal);
        let rendered = render(&fact_of(Term::Symbolic(deep.clone())), Dialect::Clingo)
            .expect("a ground symbol renders"); // render (render_symbol, §10)
        assert!(!rendered.is_empty());
        drop(deep); // Drop — the whole tree, iteratively
        drop(same);
        drop(rebuilt);
    });
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn every_walk_over_a_deep_term_survives_the_stated_stack() {
    on_stack(STATED_STACK_BYTES, || {
        // Sequence children (nested Function, ground) and boxed children (nested unary
        // over a variable, non-ground) both.
        let ground = deep_ground_term(DEPTH); // constructed
        let open = deep_unary_term(Term::Variable(Variable::Anonymous), DEPTH);
        for deep in [&ground, &open] {
            let same = deep.clone(); // Clone
            let clone_is_equal = *deep == same; // PartialEq / Eq
            assert!(clone_is_equal);
            assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
            assert_eq!(deep.partial_cmp(&same), Some(Ordering::Equal)); // PartialOrd
            let mut hasher = DefaultHasher::new();
            deep.hash(&mut hasher); // Hash
            let _ = hasher.finish();
            assert_eq!(deep.subterms().count(), DEPTH + 1); // pre-order traversal
            let _ = format!("{deep:?}"); // Debug (iterative, §14)
            let rebuilt = deep.clone().fold(Term::from); // fold
            let fold_is_equal = rebuilt == same;
            assert!(fold_is_equal);
        }
        // canonicalize: the ground spine collapses to one deep Symbolic; the open spine
        // rebuilds unchanged — both walks iterative (§5.1, §13).
        let collapsed = ground.clone().canonicalize();
        assert!(matches!(collapsed, Term::Symbolic(_)));
        let unchanged = open.clone().canonicalize();
        let open_is_unchanged = unchanged == open;
        assert!(open_is_unchanged);
        // evaluate: the ground term denotes a deep symbol (§3.5) — descend and reassemble
        // the whole spine, iteratively.
        let evaluated = ground
            .evaluate()
            .expect("a ground constructor term evaluates");
        assert_eq!(evaluated.subsymbols().count(), DEPTH + 1);
        // render: the open term stays a term, so render walks the term spine (render_term,
        // §10), not the collapsed symbol.
        let rendered =
            render(&fact_of(open.clone()), Dialect::Clingo).expect("an open term renders");
        assert!(!rendered.is_empty());
        drop(ground); // Drop
        drop(open);
    });
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn every_walk_over_a_deep_theory_term_survives_the_stated_stack() {
    on_stack(STATED_STACK_BYTES, || {
        let deep = deep_theory_term(DEPTH); // constructed
        let same = deep.clone(); // Clone
        let clone_is_equal = deep == same; // PartialEq / Eq
        assert!(clone_is_equal);
        assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
        assert_eq!(deep.partial_cmp(&same), Some(Ordering::Equal)); // PartialOrd
        let mut hasher = DefaultHasher::new();
        deep.hash(&mut hasher); // Hash
        let _ = hasher.finish();
        assert_eq!(deep.subterms().count(), DEPTH + 1); // pre-order traversal
        let _ = format!("{deep:?}"); // Debug (iterative, §14)
        let rebuilt = deep.clone().fold(TheoryTerm::from); // fold
        let fold_is_equal = rebuilt == same;
        assert!(fold_is_equal);
        let rendered =
            render(&theory_fact_of(deep.clone()), Dialect::Clingo).expect("a theory term renders"); // render (render_theory_term, §10)
        assert!(!rendered.is_empty());
        drop(deep); // Drop
        drop(same);
        drop(rebuilt);
    });
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn every_walk_over_the_other_recursion_shapes_survives_the_stated_stack() {
    on_stack(STATED_STACK_BYTES, || {
        // The recursion shapes the Function/UnaryOperation laws above do not reach: a
        // `Symbol::Tuple` and a `Term::Tuple` (the sequence-child arm the iterative `Drop`
        // moves apart from Function), and a `Term::BinaryOperation` (two boxed children —
        // the shape neither a sequence nor a single box covers). Each is walked by every
        // derived walk and dropped, all on the stated small stack.
        let symbol_tuple = deep_symbol_tuple(DEPTH); // constructed
        let symbol_same = symbol_tuple.clone(); // Clone
        let symbol_is_equal = symbol_tuple == symbol_same; // PartialEq / Eq
        assert!(symbol_is_equal);
        assert_eq!(symbol_tuple.cmp(&symbol_same), Ordering::Equal); // Ord
        assert_eq!(
            symbol_tuple.partial_cmp(&symbol_same),
            Some(Ordering::Equal)
        ); // PartialOrd
        let mut hasher = DefaultHasher::new();
        symbol_tuple.hash(&mut hasher); // Hash
        let _ = hasher.finish();
        assert_eq!(symbol_tuple.subsymbols().count(), DEPTH + 1); // pre-order traversal
        let _ = format!("{symbol_tuple:?}"); // Debug (iterative, §14)
        let symbol_rebuilt = symbol_tuple.clone().fold(Symbol::from); // fold
        let symbol_fold_is_equal = symbol_rebuilt == symbol_same;
        assert!(symbol_fold_is_equal);

        let term_tuple = deep_term_tuple(DEPTH); // constructed
        let binary = deep_binary_term(DEPTH); // constructed
        for (deep, nodes) in [(&term_tuple, DEPTH + 1), (&binary, 2 * DEPTH + 1)] {
            let same = deep.clone(); // Clone
            let clone_is_equal = *deep == same; // PartialEq / Eq
            assert!(clone_is_equal);
            assert_eq!(deep.cmp(&same), Ordering::Equal); // Ord
            assert_eq!(deep.partial_cmp(&same), Some(Ordering::Equal)); // PartialOrd
            let mut hasher = DefaultHasher::new();
            deep.hash(&mut hasher); // Hash
            let _ = hasher.finish();
            assert_eq!(deep.subterms().count(), nodes); // pre-order traversal
            let _ = format!("{deep:?}"); // Debug (iterative, §14)
            let rebuilt = deep.clone().fold(Term::from); // fold
            let fold_is_equal = rebuilt == same;
            assert!(fold_is_equal);
        }
        drop(symbol_tuple); // Drop — the whole tree, iteratively, through the Tuple arm
        drop(symbol_same);
        drop(symbol_rebuilt);
        drop(term_tuple);
        drop(binary);
    });
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn substitute_resolves_a_deep_term_and_a_deep_triangular_chain_on_the_stated_stack() {
    on_stack(STATED_STACK_BYTES, || {
        // Way one: substitute over a deep term. `-(-(… X …))` with `X ↦ a` splices the
        // binding at the leaf and resolves the whole ~200,000-level spine; `a` is a
        // constant, not a number, so the result stays a deep term (§5.1).
        let leaf = var(0);
        let deep = deep_unary_term(leaf, DEPTH);
        let pattern = Atom::new(name("q"), [var(0)]); // q(X₀)
        let target = Atom::new(name("q"), [Term::Symbolic(a_constant())]); // q(a)
        let binding = mgu(&pattern, &target)
            .expect("two atoms are patterns")
            .expect("they unify"); // X₀ ↦ a
        let substituted = deep.substitute(&binding);
        assert_eq!(substituted.subterms().count(), DEPTH + 1);

        // Way two: resolve a deep triangular substitution. Unifying
        // `p(X₀, …, X₍ₙ₋₁₎)` with `p(f(X₁), …, f(Xₙ))` yields the triangular chain
        // `X₀↦f(X₁), …, X₍ₙ₋₁₎↦f(Xₙ)`; resolving `X₀` follows the ~200,000-link
        // dereference chain to `f(f(… Xₙ …))` — its own recursion, handled iteratively
        // (§11.1), not the term's own depth.
        let left = Atom::new(name("p"), (0..DEPTH).map(var));
        let right = Atom::new(
            name("p"),
            (0..DEPTH).map(|index| Term::Function {
                name: name("f"),
                arguments: vec![var(index + 1)],
            }),
        );
        let chain = mgu(&left, &right)
            .expect("two atoms are patterns")
            .expect("they unify");
        let resolved = var(0).substitute(&chain);
        assert_eq!(resolved.subterms().count(), DEPTH + 1);
    });
}

/// A constant `a` — a nullary ground function symbol, not a number, so unary minus over it
/// does not fold (§5.1).
fn a_constant() -> Symbol {
    Symbol::Function {
        name: name("a"),
        arguments: vec![],
        sign: Sign::Positive,
    }
}

// ---- the control (§13, §16): the pole is real ----

/// Run by the parent in a child process, with `CONTROL` set: the recursive walk on the
/// stated stack, over a term this deep, overflows and ends this process. A no-op in a
/// normal run, where `CONTROL` is unset.
#[test]
fn recursive_control_entry() {
    if env::var(CONTROL).is_err() {
        return;
    }
    on_stack(STATED_STACK_BYTES, || {
        let deep = deep_unary_term(Term::Variable(Variable::Anonymous), DEPTH);
        let _ = recursive_walk(&deep); // overflows the stated stack → aborts the process
    });
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn the_recursive_control_overflows_the_stated_stack() {
    // A recursive walk of the same shape, on the same stated stack, overflows — so the
    // iterative walks' survival above is a proof, not slack. An overflow aborts the
    // process, so the control runs in a child process (a re-exec of this test binary
    // running only the control entry) whose crash-exit the parent asserts.
    let status = Command::new(env::current_exe().expect("this test binary"))
        .args(["--exact", "recursive_control_entry", "--test-threads=1"])
        .env(CONTROL, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the control subprocess runs");
    assert!(
        !status.success(),
        "the recursive control overflows the stated stack (status: {status:?})"
    );
}

// ---- builder validation (that the proof is not vacuous) ----

/// The deep builders realize the depth they claim — cheap, at small counts, so a builder
/// that silently stopped nesting (making every survival above vacuous) is caught here.
#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn the_deep_builders_realize_their_depth() {
    for levels in [1usize, 2, 7] {
        assert_eq!(deep_symbol(levels).subsymbols().count(), levels + 1);
        assert_eq!(deep_ground_term(levels).subterms().count(), levels + 1);
        assert_eq!(
            deep_unary_term(Term::Variable(Variable::Anonymous), levels)
                .subterms()
                .count(),
            levels + 1
        );
        assert_eq!(deep_theory_term(levels).subterms().count(), levels + 1);
        assert_eq!(deep_symbol_tuple(levels).subsymbols().count(), levels + 1);
        assert_eq!(deep_term_tuple(levels).subterms().count(), levels + 1);
        assert_eq!(deep_binary_term(levels).subterms().count(), 2 * levels + 1);
    }
}
