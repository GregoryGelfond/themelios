//! Laws of the bridge's doors and its typed aspif surface
//! (docs/design/solve.md §10.2, §10.3): the doors are a closed set of three
//! — the typed tree, the owned program, an aspif-level source — each
//! carrying a borrow of what enters through it; the source and the sinks
//! are trait objects, so a source drives a sink it never names, and what a
//! source emits reaches the sink in order; the id newtypes are plain
//! copyable data; and the seam is typed — a rendered program cannot enter a
//! door as text, and an atom id cannot pass as a literal id — held by the
//! compile-fail witnesses in `tests/ui`.

use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::ptr;

use themelios_program::program::Program;
use themelios_solve::bridge::{AspifAtom, AspifLit, AspifSink, AspifSource, Door, TheorySink};
use themelios_solve::contract::{Fault, TruthValue};
use themelios_syntax::{Dialect, parse_str};

/// The design's letter for each door (docs/design/solve.md §10.2).
const DOOR_A: &str = "A";
const DOOR_B: &str = "B";
const DOOR_C: &str = "C";

/// A one-fact program text: the smallest that parses to a program.
const FACT: &str = "p.";

/// The first atom an engine mints, and the two literals over it: the one
/// that holds when the atom holds, and its negation.
const ATOM: AspifAtom = AspifAtom(1);
const HOLDS: AspifLit = AspifLit(1);
const FAILS: AspifLit = AspifLit(-1);

/// The base minimize level, and a unit weight at it.
const PRIORITY: i32 = 0;
const WEIGHT: i32 = 1;

fn is_plain_copyable_data<T: Copy + Eq + Hash + Debug + Send + Sync + 'static>() {}

/// The design's letter for a door. Exhaustive without a wildcard, so a
/// fourth door is a compile error here: the set is closed.
fn grade(door: &Door<'_>) -> &'static str {
    match door {
        Door::Ast(_) => DOOR_A,
        Door::Program(_) => DOOR_B,
        Door::Aspif(_) => DOOR_C,
    }
}

/// What a sink received: one entry per call, in the order of the calls.
#[derive(PartialEq, Debug)]
enum Received {
    Rule {
        choice: bool,
        head: Vec<AspifAtom>,
        body: Vec<AspifLit>,
    },
    Minimize {
        priority: i32,
        literals: Vec<(AspifLit, i32)>,
    },
    External {
        atom: AspifAtom,
        value: TruthValue,
    },
    Assume {
        literals: Vec<AspifLit>,
    },
}

/// A sink as a consumer implements it: it records every call it receives.
#[derive(Default)]
struct Recorder {
    received: Vec<Received>,
}

impl AspifSink for Recorder {
    fn rule(&mut self, choice: bool, head: &[AspifAtom], body: &[AspifLit]) -> Result<(), Fault> {
        self.received.push(Received::Rule {
            choice,
            head: head.to_vec(),
            body: body.to_vec(),
        });
        Ok(())
    }

    fn minimize(&mut self, priority: i32, literals: &[(AspifLit, i32)]) -> Result<(), Fault> {
        self.received.push(Received::Minimize {
            priority,
            literals: literals.to_vec(),
        });
        Ok(())
    }

    fn external(&mut self, atom: AspifAtom, value: TruthValue) -> Result<(), Fault> {
        self.received.push(Received::External { atom, value });
        Ok(())
    }

    fn assume(&mut self, literals: &[AspifLit]) -> Result<(), Fault> {
        self.received.push(Received::Assume {
            literals: literals.to_vec(),
        });
        Ok(())
    }
}

/// A source as a foreign grounder implements it: one ground object of each
/// kind, over the one atom, in a fixed order.
struct Emitting;

impl AspifSource for Emitting {
    fn drive(&mut self, sink: &mut dyn AspifSink) -> Result<(), Fault> {
        sink.rule(true, &[ATOM], &[])?;
        sink.minimize(PRIORITY, &[(HOLDS, WEIGHT)])?;
        sink.external(ATOM, TruthValue::Free)?;
        sink.assume(&[FAILS])
    }
}

/// The companion theory sink, implemented with nothing to receive.
struct NoTheory;

impl TheorySink for NoTheory {}

// --- the doors (§10.2) ---

#[test]
fn door_b_carries_a_borrow_of_the_program() {
    let program = Program::default();
    let Door::Program(carried) = Door::Program(&program) else {
        panic!("Door B carries the program it was given");
    };
    assert!(ptr::eq(carried, &raw const program));
}

#[test]
fn the_doors_are_a_closed_set_of_three() {
    let parsed = parse_str(FACT, Dialect::Clingo).expect("a one-fact text fits a source");
    let program = Program::default();
    let mut source = Emitting;
    let doors = [
        Door::Ast(&parsed),
        Door::Program(&program),
        Door::Aspif(&mut source),
    ];
    let grades: Vec<&str> = doors.iter().map(grade).collect();
    assert_eq!(grades, [DOOR_A, DOOR_B, DOOR_C]);
}

#[test]
fn a_door_c_source_s_objects_reach_the_sink_in_order() {
    let mut source = Emitting;
    let mut recorder = Recorder::default();
    let Door::Aspif(carried) = Door::Aspif(&mut source) else {
        panic!("Door C carries the source it was given");
    };
    assert_eq!(carried.drive(&mut recorder), Ok(()));
    assert_eq!(
        recorder.received,
        [
            Received::Rule {
                choice: true,
                head: vec![ATOM],
                body: vec![],
            },
            Received::Minimize {
                priority: PRIORITY,
                literals: vec![(HOLDS, WEIGHT)],
            },
            Received::External {
                atom: ATOM,
                value: TruthValue::Free,
            },
            Received::Assume {
                literals: vec![FAILS],
            },
        ]
    );
}

#[test]
fn the_source_and_the_sinks_are_trait_objects() {
    // Door C holds its source as a trait object and a source takes its sink
    // as one, so the lock is the coercion itself: each value below becomes
    // its object, or this does not compile.
    fn as_objects(_: &mut dyn AspifSource, _: &dyn AspifSink, _: &dyn TheorySink) {}
    as_objects(&mut Emitting, &Recorder::default(), &NoTheory);
}

// --- the id newtypes (§10.3) ---

#[test]
fn an_atom_id_is_plain_copyable_data() {
    is_plain_copyable_data::<AspifAtom>();
    // Two copies of one id are one entry: hashing agrees with equality.
    assert_eq!(HashSet::from([ATOM, ATOM]), HashSet::from([ATOM]));
    assert!(format!("{ATOM:?}").contains("AspifAtom"));
}

#[test]
fn a_literal_id_is_plain_copyable_data() {
    is_plain_copyable_data::<AspifLit>();
    // A literal and its negation are two entries; a repeat is not a third.
    assert_eq!(
        HashSet::from([HOLDS, FAILS, HOLDS]),
        HashSet::from([HOLDS, FAILS])
    );
    assert!(format!("{FAILS:?}").contains("AspifLit"));
}

// --- the seam is typed (§10.2, §10.3) ---

#[test]
fn the_seam_refuses_mistyped_values_at_compile_time() {
    // A rendered program has no door to enter, and an atom id does not pass
    // where a literal id is wanted: each `tests/ui` witness tries the
    // crossing and holds the compile error that refuses it.
    let witnesses = trybuild::TestCases::new();
    witnesses.compile_fail("tests/ui/door_*.rs");
    witnesses.compile_fail("tests/ui/sink_*.rs");
}
