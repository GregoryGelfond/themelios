//! Laws of the doors (docs/design/solve.md §10.2): a parse the core admits
//! once, or a `Program`; either lends its program as a set, and admission
//! refuses, before any backend is asked, a parse that is not in the language or
//! does not raise cleanly. The doors are a closed set of two, each carrying a
//! borrow of what enters through it, and the seam is typed — a rendered program
//! cannot enter a door as text — held by the compile-fail witness in `tests/ui`.

use std::fmt::Debug;
use std::ptr;

use themelios_base::source::{Source, SourceId};
use themelios_program::raise::LowerErrorKind;
use themelios_solve::bridge::{Admitted, Door, NotAdmitted};
use themelios_solve::contract::{Fault, Locus, Refused};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// Text outside the language: an argument list left open.
const NOT_IN_THE_LANGUAGE: &str = "p(.";

/// The design's letter for each door (docs/design/solve.md §10.2).
const DOOR_A: &str = "A";
const DOOR_B: &str = "B";

/// `text`, parsed in the clingo dialect, admitted at Door A — or refused.
fn admitted(text: &str) -> Result<Admitted, NotAdmitted> {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("a fixture fits a source");
    Admitted::of(&parse(&source, Dialect::Clingo))
}

/// The design's letter for a door. Exhaustive without a wildcard, so a third
/// door is a compile error here: the set is closed.
fn grade(door: &Door<'_>) -> &'static str {
    match door {
        Door::Parsed(_) => DOOR_A,
        Door::Program(_) => DOOR_B,
    }
}

fn is_plain_copyable_data<T: Copy + Debug>() {}

// --- admission (§10.2) ---

#[test]
fn a_clean_parse_is_admitted_with_every_statement_in_source_order() {
    // Every occurrence is kept, the repeat included, where the set would keep
    // one: a backend reading Door A in source order reads the program as
    // written.
    let admitted = admitted("b. a. b.").expect("admitted");
    let starts: Vec<u32> = admitted
        .statements()
        .map(|each| each.location().span.start().get())
        .collect();
    assert_eq!(starts.len(), 3);
    assert!(
        starts.windows(2).all(|pair| pair[0] < pair[1]),
        "{starts:?}"
    );
}

#[test]
fn a_parse_with_a_syntax_error_is_refused_with_its_errors() {
    assert!(matches!(
        admitted(NOT_IN_THE_LANGUAGE),
        Err(NotAdmitted::Syntax(errors)) if !errors.is_empty()
    ));
}

#[test]
fn a_repeated_constant_refuses_door_a() {
    let refusal = admitted("#const n = 1. #const n = 1.").expect_err("refused");
    let NotAdmitted::Lowering(batch) = refusal else {
        panic!("a lowering refusal, not {refusal:?}")
    };
    assert!(
        batch
            .iter()
            .any(|error| matches!(error.kind(), LowerErrorKind::RepeatedDefinition { .. }))
    );
}

#[test]
fn a_warning_alone_does_not_refuse_a_parse() {
    // A doc comment with no statement after it is the syntax tier's warning: it
    // stays on the caller's parse and does not refuse the program.
    assert!(admitted("a. %! a doc comment with nothing after it\n").is_ok());
}

#[test]
fn a_refused_parse_converts_into_a_program_fault_carrying_it() {
    let fault = Fault::from(admitted(NOT_IN_THE_LANGUAGE).expect_err("refused"));
    assert_eq!(fault.locus(), Locus::Program);
    assert!(matches!(
        fault.refused(),
        Refused::Parse(NotAdmitted::Syntax(_))
    ));
}

// --- the doors (§10.2) ---

#[test]
fn door_a_lends_the_program_its_statements_collect_to() {
    let admitted = admitted("b. a. b.").expect("admitted");
    assert_eq!(Door::Parsed(&admitted).program().statements().count(), 2);
}

#[test]
fn door_b_lends_its_own_program() {
    let admitted = admitted("a.").expect("admitted");
    let program = Door::Parsed(&admitted).program().clone();
    assert!(ptr::eq(
        Door::Program(&program).program(),
        &raw const program
    ));
}

#[test]
fn the_doors_are_a_closed_set_of_two() {
    let admitted = admitted("a.").expect("admitted");
    let program = Door::Parsed(&admitted).program().clone();
    let grades: Vec<&str> = [Door::Parsed(&admitted), Door::Program(&program)]
        .iter()
        .map(grade)
        .collect();
    assert_eq!(grades, [DOOR_A, DOOR_B]);
}

#[test]
fn a_door_is_plain_copyable_data() {
    is_plain_copyable_data::<Door<'static>>();
}

// --- the seam is typed (§10.2) ---

#[test]
fn the_seam_refuses_rendered_text_at_compile_time() {
    // A rendered program has no door to enter: the `tests/ui` witness tries the
    // crossing and holds the compile error that refuses it.
    trybuild::TestCases::new().compile_fail("tests/ui/door_*.rs");
}
