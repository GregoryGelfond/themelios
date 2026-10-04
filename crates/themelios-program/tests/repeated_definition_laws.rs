//! The repeated definition and the extra script (docs/design/program.md §6.3, §8): a global
//! definition repeated content-equal within one part is diagnosed at the repeat, since the
//! authority rejects a redefinition the set would merge silently, while a definition repeated
//! across parts, or with different content, is kept and diagnosed nowhere. The raise admits one
//! `#script` block to a program: every block after the first is diagnosed and kept, naming the
//! first — in any part, whatever its content, in any language — since the authority runs each
//! block where it reads it, in an order the set does not keep.

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_program::program::{Program, Script, Statement};
use themelios_program::raise::{LowerErrorKind, Occurrences, Raised, raise, raise_occurrences};
use themelios_program::render::render;
use themelios_program::symbol::Name;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn raised(text: &str) -> Raised {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise(&parse(&source, Dialect::Clingo))
}

fn occurrences(text: &str) -> Occurrences {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise_occurrences(&parse(&source, Dialect::Clingo))
}

fn at(start: u32, end: u32) -> Location {
    Location {
        source: SourceId::new(0),
        span: Span::new(ByteOffset::new(start), ByteOffset::new(end)).expect("ordered"),
    }
}

/// The repeated definitions a raise reports: (the repeat's location, the first's).
fn repeats(raised: &Raised) -> Vec<(Location, Location)> {
    raised
        .diagnostics()
        .iter()
        .filter_map(|error| match error.kind() {
            LowerErrorKind::RepeatedDefinition { first } => Some((*error.location(), *first)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_constant_repeated_in_one_part_is_diagnosed_at_its_name() {
    // `#const n = 1. #const n = 1.` — the names sit at bytes 7..8 and 21..22.
    assert_eq!(
        repeats(&raised("#const n = 1. #const n = 1.")),
        vec![(at(21, 22), at(7, 8))]
    );
}

#[test]
fn a_theory_repeated_in_one_part_is_a_repeated_definition() {
    let text = "#theory t { a { + : 1, unary }; &b/0 : a, any }. #theory t { a { + : 1, unary }; &b/0 : a, any }.";
    assert_eq!(repeats(&raised(text)).len(), 1);
}

#[test]
fn a_definition_repeated_in_a_reopened_part_is_a_repeated_definition() {
    let text = "#const n = 1. #program p. a. #program base. #const n = 1.";
    assert_eq!(repeats(&raised(text)).len(), 1);
}

#[test]
fn a_definition_repeated_in_a_part_reopened_twice_in_a_row_is_a_repeated_definition() {
    // Each `#program base.` opens base under a fresh `Arc` over an equal key; the second must
    // find what the first recorded.
    let text = "#program base. #const n = 1. #program base. #const n = 1.";
    assert_eq!(repeats(&raised(text)).len(), 1);
}

#[test]
fn a_definition_repeated_in_another_part_is_not_diagnosed() {
    assert!(repeats(&raised("#const n = 1. #program p. #const n = 1.")).is_empty());
}

#[test]
fn a_name_defined_twice_with_other_content_is_not_diagnosed() {
    assert!(repeats(&raised("#const n = 1. #const n = 2.")).is_empty());
}

#[test]
fn a_canonically_equal_constant_is_a_repeated_definition() {
    assert_eq!(repeats(&raised("#const x = 5. #const x = - -5.")).len(), 1);
}

#[test]
fn a_repeated_rule_is_no_repeated_definition() {
    assert!(raised("a. a.").diagnostics().is_empty());
}

#[test]
fn the_repeat_s_occurrence_carries_the_diagnostic() {
    let stream = occurrences("#const n = 1. #const n = 1.");
    let carried: Vec<usize> = stream
        .occurrences()
        .iter()
        .map(|each| each.diagnostics().len())
        .collect();
    assert_eq!(carried, vec![0, 1]);
}

#[test]
fn the_diagnostic_names_the_first_definition() {
    let diagnostic = raised("#const n = 1. #const n = 1.").diagnostics()[0].to_diagnostic();
    let secondary: Vec<Location> = diagnostic
        .secondary()
        .iter()
        .map(|label| label.location)
        .collect();
    assert_eq!(secondary, vec![at(7, 8)]);
}

#[test]
fn a_repeated_theory_is_diagnosed_at_its_name() {
    // The two `t`s sit at bytes 8..9 and 57..58.
    let text = "#theory t { a { + : 1, unary }; &b/0 : a, any }. #theory t { a { + : 1, unary }; &b/0 : a, any }.";
    assert_eq!(repeats(&raised(text)), vec![(at(57, 58), at(8, 9))]);
}

/// The extra scripts a raise reports: (the extra block's location, the first's).
fn extra_scripts(raised: &Raised) -> Vec<(Location, Location)> {
    raised
        .diagnostics()
        .iter()
        .filter_map(|error| match error.kind() {
            LowerErrorKind::ExtraScript { first } => Some((*error.location(), *first)),
            _ => None,
        })
        .collect()
}

/// The `#script` statements the raised program holds.
fn scripts(raised: &Raised) -> usize {
    raised
        .program()
        .statements()
        .filter(|statement| matches!(statement.get(), Statement::Script(_)))
        .count()
}

#[test]
fn a_repeated_script_is_diagnosed_at_the_repeat() {
    // The two blocks sit at bytes 0..27 and 28..55; a script has no name, so each is located
    // at its whole statement.
    let text = "#script (python) pass #end. #script (python) pass #end.";
    assert_eq!(extra_scripts(&raised(text)), vec![(at(28, 55), at(0, 27))]);
}

#[test]
fn a_script_in_another_part_is_diagnosed() {
    // The second block sits at bytes 40..67, under the part `q`.
    let text = "#script (python) pass #end. #program q. #script (python) pass #end.";
    assert_eq!(extra_scripts(&raised(text)), vec![(at(40, 67), at(0, 27))]);
}

#[test]
fn a_script_with_other_content_is_diagnosed() {
    let text = "#script (python) pass #end. #script (python) x = 1 #end.";
    assert_eq!(extra_scripts(&raised(text)), vec![(at(28, 56), at(0, 27))]);
}

#[test]
fn a_script_in_another_language_is_diagnosed() {
    let text = "#script (python) pass #end. #script (lua) x = 1 #end.";
    assert_eq!(extra_scripts(&raised(text)), vec![(at(28, 53), at(0, 27))]);
}

#[test]
fn every_script_after_the_first_names_the_first() {
    let text = "#script (python) pass #end. #script (python) x = 1 #end. #script (lua) y = 2 #end.";
    assert_eq!(
        extra_scripts(&raised(text)),
        vec![(at(28, 56), at(0, 27)), (at(57, 82), at(0, 27))]
    );
}

#[test]
fn one_script_raises_without_a_diagnostic() {
    let text = "#script (python) pass #end. p :- q.";
    assert!(raised(text).diagnostics().is_empty());
}

#[test]
fn an_extra_script_is_kept_in_the_program() {
    // Diagnosed and kept: the raise is total, and the value holds both blocks (§6.3, §8).
    let text = "#script (python) pass #end. #script (lua) x = 1 #end.";
    assert_eq!(scripts(&raised(text)), 2);
}

#[test]
fn a_constructed_program_of_two_scripts_round_trips_beside_one_diagnostic() {
    // A second block reaches a program by construction as well as by text: render writes both,
    // and the raise returns the same program beside its one `ExtraScript` (program.md §10).
    // Each body is as the raise reads one, with no whitespace before its `#end`.
    let program = Program::of([
        Script::new(Name::new("python").expect("a name"), " pass"),
        Script::new(Name::new("lua").expect("a name"), " x = 1"),
    ]);
    let rendered = render(&program, Dialect::Clingo).expect("the program renders");
    let raised = raised(&rendered);
    assert_eq!(raised.program(), &program, "rendered `{rendered}`");
    assert_eq!(extra_scripts(&raised).len(), 1, "rendered `{rendered}`");
}

#[test]
fn the_script_diagnostic_names_the_first_block() {
    let text = "#script (python) pass #end. #script (python) x = 1 #end.";
    let diagnostic = raised(text).diagnostics()[0].to_diagnostic();
    let secondary: Vec<Location> = diagnostic
        .secondary()
        .iter()
        .map(|label| label.location)
        .collect();
    assert_eq!(secondary, vec![at(0, 27)]);
}

#[test]
fn the_script_diagnostic_is_identified_as_an_extra_script() {
    let text = "#script (python) pass #end. #script (python) x = 1 #end.";
    let diagnostic = raised(text).diagnostics()[0].to_diagnostic();
    assert_eq!(diagnostic.id().to_string(), "program::extra-script");
}
