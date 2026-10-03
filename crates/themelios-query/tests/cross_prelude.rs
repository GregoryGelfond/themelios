//! The cross-prelude no-collision compile-lock (docs/design/query.md §2.1; the
//! rule in `themelios_query::prelude`). A consumer that globs
//! `themelios_query::prelude::*` beside `themelios_program::prelude::*` — and
//! the solve and syntax preludes beside them, as a facade over the whole stack
//! does — compiles: the query prelude is flat with the reading vocabulary only, and the
//! one spelling the two tiers share, `Query`, is flat on the program side (the
//! ASP-Core-2 `a?` statement) and reached by module path on the query side.
//!
//! `E0659` (an ambiguous glob) is reported where an ambiguous name is *used*,
//! not where it is glob-imported: two globs bringing one spelling for two items
//! compile until that spelling is referenced bare. So this file locks the
//! property one name at a time — for each spelling it names bare below, that the
//! file compiles is proof the spelling resolves unambiguously. It names the
//! whole set of spellings the program and syntax tiers share, so that a future
//! edit flat-globbing any of them into the query prelude would fail this build,
//! and it pins bare `Query` to the *program* tier's type by identity.

use themelios_program::prelude::*;
use themelios_query::prelude::*;
use themelios_solve::prelude::*;
use themelios_syntax::prelude::*;

#[test]
fn the_query_prelude_coexists_with_the_program_prelude() {
    // The reading vocabulary resolves bare from the query prelude, the solve
    // tier's `Consequences` one item under both globs.
    let _: Option<Answer> = None;
    let _: Option<BindingPattern> = None;
    let _: Option<Consequences> = None;

    // The driving vocabulary resolves bare from the solve prelude beside it.
    let _: Option<Determination<'static>> = None;
    let _: Option<Model> = None;

    // The program IR resolves bare from the program prelude — every spelling
    // the program and syntax tiers share, each with no `E0659` beside the
    // query prelude.
    let _: Option<Program> = None;
    let _: Option<Statement> = None;
    let _: Option<Rule> = None;
    let _: Option<Atom> = None;
    let _: Option<Head> = None;
    let _: Option<Body> = None;
    let _: Option<Literal> = None;
    let _: Option<Comparison> = None;
    let _: Option<BodyElement> = None;
    let _: Option<Direction> = None;
    let _: Option<Relation> = None;
    let _: Option<Aggregate> = None;
    let _: Option<Term> = None;
    let _: Option<Signature> = None;
    let _: Option<Variable> = None;

    // The syntax tier's AST is reached through `ast::`, never bare.
    let _: Option<Parse<ast::Program>> = None;
}

#[test]
fn a_bare_query_is_the_program_tier_statement() {
    // Assigning `None::<Query>` to the program tier's type compiles only if
    // bare `Query` IS that type: the query prelude does not carry its own.
    let _: Option<themelios_program::program::Query> = None::<Query>;

    // The query tier's `Query` is reached by its module path.
    let _: Option<themelios_query::Query> = None;
}

#[test]
fn consequences_is_one_item_under_two_paths() {
    // The query prelude's `Consequences` is the solve tier's own, re-exported.
    let _: Option<themelios_solve::outcome::Consequences> = None::<Consequences>;
}

#[test]
fn atom_is_one_item_under_two_paths() {
    // The pattern type is the program tier's own, re-exported at the query root:
    // bare `Atom`, globbed from both preludes, is that one item.
    let _: Option<themelios_program::program::Atom> = None::<Atom>;
    let _: Option<themelios_program::program::Atom> = None::<themelios_query::Atom>;
}
