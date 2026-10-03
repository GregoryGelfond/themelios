//! The repeated definition (docs/design/program.md §6.3, §8): a global definition repeated
//! content-equal within one part is diagnosed at the repeat, since the authority rejects a
//! redefinition the set would merge silently; a definition repeated across parts, or a name
//! defined twice with different content, is kept and diagnosed nowhere.

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_program::raise::{LowerErrorKind, Occurrences, Raised, raise, raise_occurrences};
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
