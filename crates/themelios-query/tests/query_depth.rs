//! The depth proof for a query (docs/design/query.md §2.1; the depth discipline of
//! docs/design/program.md §13): a query is the caller's own composition, nested as
//! deep as the caller composes it, so every walk over it — reading it against a world
//! view, cloning, comparing, formatting, dropping — is iterative. A query nested
//! 200,000 levels deep is handled on a small stack, never a stack overflow: an
//! overflow aborts the process rather than unwinding a catchable panic, so the
//! reading's no-panic promise rests on it.
//!
//! The stated stack is 256 KiB. A recursive walk needs at least one frame per level —
//! tens of bytes at the very least — so 200,000 levels need megabytes, and a recursive
//! walk of this depth overflows the stated stack many times over; the iterative walks
//! need only their heap-allocated work lists.

mod common;

use std::thread;

use common::{answer_set, with_agent};
use themelios_program::program::{Arguments, Atom};
use themelios_program::symbol::{Name, Sign};
use themelios_query::{AgentReading, Answer, Query};
use themelios_solve::outcome::Conclusion;

/// The depth every walk is proven stack-independent at: far past any query a program
/// composes by hand, and the depth the program tier's own proof uses.
const DEPTH: usize = 200_000;

/// The stated small stack every iterative walk survives and a recursive walk of
/// `DEPTH` levels overflows many times over.
const STATED_STACK_BYTES: usize = 256 * 1024;

/// Run `body` on a thread of exactly `bytes` of stack and return its result. A walk
/// that overflows here aborts the whole test process — the regression this proof
/// guards against.
fn on_stack<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(bytes: usize, body: F) -> R {
    thread::Builder::new()
        .name(format!("query-depth-{bytes}"))
        .stack_size(bytes)
        .spawn(body)
        .expect("the proof's thread spawns")
        .join()
        .expect("the proof's thread completes")
}

/// The positive ground literal query `name`.
fn lit(name: &str) -> Query {
    Query::of(Atom {
        sign: Sign::Positive,
        name: Name::new(name).expect("a valid identifier"),
        arguments: Arguments::Single(vec![]),
    })
    .expect("a ground literal is a query")
}

/// A query nested `depth` levels around the literal `a`, each level a one-part
/// compound, conjunction and disjunction alternating — built iteratively, so the
/// construction itself does not recurse.
fn deep_query(depth: usize) -> Query {
    let mut query = lit("a");
    for level in 0..depth {
        query = if level % 2 == 0 {
            Query::all([query])
        } else {
            Query::any([query])
        };
    }
    query
}

#[cfg_attr(
    not(feature = "scale-proofs"),
    ignore = "depth proof; held out of the mutation loop — see scale-proofs in Cargo.toml"
)]
#[test]
fn every_walk_over_a_deep_query_survives_the_stated_stack() {
    // The world view is built on the default stack; only the query's walks run on the
    // stated one.
    let snapshot = with_agent(vec![answer_set(["a"])], Conclusion::Exhausted, |agent| {
        agent.snapshot().expect("a world view")
    });
    on_stack(STATED_STACK_BYTES, move || {
        let deep = deep_query(DEPTH); // constructed
        assert_eq!(snapshot.answer(&deep), Answer::Yes); // read against each member
        let same = deep.clone(); // Clone
        // Compared through a bound bool, so a failure never formats two values this
        // deep through the assertion.
        let clone_is_equal = deep == same; // PartialEq
        assert!(clone_is_equal);
        let rendered = format!("{deep:?}"); // Debug
        // Every level printed: the conjunctions are the even levels, half of them.
        assert_eq!(rendered.matches("Conjunction([").count(), DEPTH / 2);
        drop(deep); // Drop
        drop(same);
    });
}
