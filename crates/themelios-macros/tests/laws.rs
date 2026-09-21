//! The splice round-trip law (docs/design/macros.md §11): a splice's macro
//! invocation is fixed at compile time, but the value a `$…` carries is a
//! runtime value — so the crossing a splice makes is a property over values,
//! not a frozen golden. For a value `v`, `fact!(p($v))` builds the same `Rule`
//! as the by-hand `Rule::fact(Atom::new(N("p"), [Term::from(v.to_symbol())]))`:
//! the §7 crossing — `ToSymbol::to_symbol` to a ground `Symbol`, then
//! `From<Symbol>` for `Term` (or `TheoryTerm::Symbolic` in theory-term
//! position) — told exact against its spelling. This is the ninth
//! `codegen_term` arm, `Splice`, whose value the construction-equality
//! witnesses over the other eight arms leave to this law (tests/equality.rs;
//! docs/design/macros.md §11).

use proptest::prelude::*;
use themelios_macros::fact;
use themelios_program::program::{
    Atom, Rule, TheoryAtom, TheoryElement, TheoryGuard, TheoryOperator, TheoryTerm,
};
use themelios_program::symbol::{Name, Symbol, ToSymbol};
use themelios_program::term::Term;

/// `Name::new(text)`, discharged as the codegen's `.expect()` is — the fixture
/// text is a valid identifier by inspection (the raise §8 name-class invariant).
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

proptest! {
    // A spliced integer in term position equals the value spelled through the
    // crossing it stands for: `ToSymbol::to_symbol` to the ground `Symbol`, then
    // `From<Symbol>` for `Term` (docs/design/macros.md §7). The invocation
    // `fact!(p($v))` is fixed; only `v` varies, so this is the `Splice` arm's
    // value proof the frozen goldens cannot give.
    #[test]
    fn a_term_splice_of_an_integer_equals_the_spelled_value(v in any::<i32>()) {
        let by_splice = fact!(p($v));
        let by_hand = Rule::fact(Atom::new(
            name("p"),
            [Term::from(ToSymbol::to_symbol(&v))],
        ));
        prop_assert_eq!(by_splice, by_hand);
    }

    // The same crossing for a spliced string value: the `ToSymbol for String`
    // door to `Symbol::String`, under no grammar §4.4 spelling constraint —
    // a splice carries the value, never a lexed literal (docs/design/macros.md
    // §7), so an arbitrary string crosses. (`String`, not `&str`: the codegen
    // borrows the operand as `&(expr)`, and `ToSymbol` has no reference impl,
    // so a `&str` operand would force `Self = &str` and not resolve — the owned
    // `String` is the spliced-string case.)
    #[test]
    fn a_term_splice_of_a_string_equals_the_spelled_value(s in any::<String>()) {
        let by_splice = fact!(p($s));
        let by_hand = Rule::fact(Atom::new(
            name("p"),
            [Term::from(ToSymbol::to_symbol(&s))],
        ));
        prop_assert_eq!(by_splice, by_hand);
    }

    // A spliced value in theory-term position lands at the theory algebra's
    // ground-symbol leaf — `ToSymbol::to_symbol` then `TheoryTerm::Symbolic`
    // (docs/design/macros.md §7; program §4.9) — the theory twin of the
    // term-position crossing. `fact!(&sum { $v } <= 3)` is fixed; `v` varies,
    // and a lone spliced leaf (no operators) stays a bare theory term, not an
    // `Operation` (program §8's single-operand opterm).
    #[test]
    fn a_theory_term_splice_of_an_integer_equals_the_spelled_value(v in any::<i32>()) {
        let by_splice = fact!(&sum { $v } <= 3);
        let by_hand = Rule::fact(TheoryAtom::new(
            name("sum"),
            [],
            [TheoryElement::new(
                [TheoryTerm::Symbolic(ToSymbol::to_symbol(&v))],
                None,
            )],
            Some(TheoryGuard {
                operator: TheoryOperator::new("<="),
                term: TheoryTerm::Symbolic(Symbol::Number(3)),
            }),
        ));
        prop_assert_eq!(by_splice, by_hand);
    }
}
