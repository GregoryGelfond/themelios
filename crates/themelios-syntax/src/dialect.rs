//! The declared dialect (docs/design/syntax.md §3): which reading of the
//! two lexical regions the lexer applies, and whether the query
//! statement exists.

use std::fmt;

/// The declared parameterization of the one grammar (grammar §1, §6):
/// which reading of the two lexical regions — the string rule and the
/// block-comment rule — and whether the query statement exists.
/// Declared per input, never varied per consumer; the lexer and the
/// parser both read it from the token source, so the two cannot
/// disagree. Closed: a released clingo 6.x language is a third surface
/// until the grammar's upgrade protocol says otherwise (grammar §12).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Dialect {
    /// The clingo dialect — the grammar's own default (grammar §1).
    #[default]
    Clingo,
    /// The ASP-Core-2 dialect: the standard's string rule, its
    /// block-comment rule, and the query statement (grammar §6).
    AspCore2,
}

impl fmt::Display for Dialect {
    /// The dialect's name — `clingo` or `asp-core-2` — stable, being
    /// what dumps and goldens read (docs/design/syntax.md §12.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dialect::Clingo => "clingo",
            Dialect::AspCore2 => "asp-core-2",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_grammars_own() {
        assert_eq!(Dialect::default(), Dialect::Clingo);
    }

    #[test]
    fn the_names_are_stable() {
        assert_eq!(Dialect::Clingo.to_string(), "clingo");
        assert_eq!(Dialect::AspCore2.to_string(), "asp-core-2");
    }
}
