//! A neutral test facade over the construction macros (docs/design/macros.md §9, §11). It
//! re-exports the program tier, and forwards each of the nine macros through a declarative
//! wrapper that selects this crate's own re-export through `$crate`, so a consumer that depends
//! on this crate alone, under any name, builds the program tier's own values. It stands in for
//! any crate that re-exports themelios under its own name; a test fixture, never published.

pub use themelios_program::{construct, program, provenance, symbol, term};

/// The paths the wrappers name, which a consumer should not: re-exports carrying no promise
/// beyond the wrappers that use them.
#[doc(hidden)]
pub mod __private {
    pub use themelios_macros;
    pub use themelios_program;
}

/// `atom!(head)` — an atom in head position, built through this crate's runtime: forwards `themelios_macros::atom!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! atom {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::atom! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `fact!(head)` — a fact, built through this crate's runtime: forwards `themelios_macros::fact!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! fact {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::fact! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `rule!(head :- body)` — a rule, built through this crate's runtime: forwards `themelios_macros::rule!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! rule {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::rule! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `constraint!(:- body)` — an integrity constraint, built through this crate's runtime: forwards `themelios_macros::constraint!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! constraint {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::constraint! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `minimize!({ … })` — a `#minimize` statement, built through this crate's runtime: forwards `themelios_macros::minimize!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! minimize {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::minimize! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `maximize!({ … })` — a `#maximize` statement, built through this crate's runtime: forwards `themelios_macros::maximize!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! maximize {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::maximize! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `show!(…)` — a `#show` directive, built through this crate's runtime: forwards `themelios_macros::show!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! show {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::show! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `external!(…)` — an `#external` directive, built through this crate's runtime: forwards `themelios_macros::external!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! external {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::external! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

/// `program!{ … }` — a whole-program block, built through this crate's runtime: forwards `themelios_macros::program!`, selecting
/// this crate's re-export of the program tier (docs/design/macros.md §9).
#[macro_export]
macro_rules! program {
    ($($body:tt)*) => {
        $crate::__private::themelios_macros::program! {
            #![crate = $crate::__private::themelios_program]
            $($body)*
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::program::{Atom, Program, Rule, Statement};
    use crate::provenance::Origin;
    use crate::symbol::Name;
    use crate::term::Term;

    /// `p(1)` as a fact, built by hand through the re-exports.
    fn p_one() -> Rule {
        Rule::fact(Atom::new(
            Name::new("p").expect("an identifier"),
            [Term::from(1i32)],
        ))
    }

    // `#[rustfmt::skip]`: the block writes its statements ASP-side, which rustfmt would read as
    // Rust and reflow.
    #[rustfmt::skip]
    #[test]
    fn a_qualified_wrapper_builds_inside_the_facade() {
        let by_macro: Program = crate::program! { p(1). };
        assert_eq!(by_macro, Program::of([Statement::from(p_one())]));
    }

    #[test]
    fn an_unqualified_wrapper_builds_inside_the_facade() {
        // Textual scope: the wrappers are defined above, in this crate.
        assert_eq!(fact!(p(1)), p_one());
    }

    #[rustfmt::skip]
    #[test]
    fn a_statement_built_inside_the_facade_carries_the_constructed_origin() {
        let by_macro: Program = crate::program! { p(1). };
        let statement = by_macro.statements().next().expect("one statement");
        let origins: Vec<&Origin> = statement.provenance().origins().collect();
        assert_eq!(origins, [&Origin::Constructed]);
    }
}
