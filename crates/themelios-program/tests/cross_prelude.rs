//! The cross-prelude no-collision compile-lock (docs/design/program.md §4; the rule
//! in `themelios_program::prelude`). A consumer that globs BOTH
//! `themelios_program::prelude::*` and `themelios_syntax::prelude::*` compiles: the
//! program tier flat-globs the IR and the infrastructure vocabulary, while the
//! syntax tier keeps its typed AST behind `ast::`. The two tiers share the very
//! spellings `Program`, `Rule`, `Statement`, `Atom`, `Direction`, … — one set the
//! logician's owned IR, the other the parse tree's typed views — so were either
//! prelude to flat-glob a name the other flat-globs, this file would fail to build
//! with `E0659`. That it compiles is the standing proof the two flat sets are
//! disjoint and the shared re-exports (`Dialect`, `base`) are one item each.

use themelios_program::prelude::*;
use themelios_syntax::prelude::*;

// `Source`, `SourceId`, and `TooLarge` are the program prelude's flat base
// re-exports — the syntax prelude keeps the same types behind `base::source::`,
// so these bare names are unambiguously the program tier's.
fn make_source(text: String) -> Result<Source, TooLarge> {
    Source::new(SourceId::new(0), text)
}

#[test]
fn both_preludes_coexist_with_the_syntax_ast_reached_namespaced() {
    // The program IR resolves BARE from the program prelude — no module path. Each
    // of these spellings is also a syntax AST node; that they resolve here with no
    // `E0659` is the proof the syntax prelude keeps its AST behind `ast::`.
    let _: Option<Program> = None;
    let _: Option<Statement> = None;
    let _: Option<Rule> = None;
    let _: Option<Atom> = None;
    let _: Option<Head> = None;
    let _: Option<Body> = None;
    let _: Option<Literal> = None;
    let _: Option<Comparison> = None;
    let _: Option<BodyElement> = None;
    // `Direction` is the named hazard: rowan's `Direction` (syntax) and the program
    // IR's are different types of the same name; the syntax prelude keeps rowan's
    // behind `tree::`, so this bare name is unambiguously the program IR's.
    let _: Option<Direction> = None;

    // The SAME spellings on the syntax side are reached through `ast::`, never bare.
    let _: Option<ast::Program> = None;
    let _: Option<ast::Statement> = None;
    let _: Option<ast::Rule> = None;
    let _: Option<ast::Atom> = None;

    // Build a program through the bare program-tier surface, render it through the
    // flat render doors (program prelude)...
    let program: Program = Program::of([Rule::fact(Atom::constant(
        Name::new("p").expect("a valid identifier"),
    ))]);
    let rendered: String = render(&program, Dialect::Clingo).expect("nothing unspellable");
    let _ = render_documented(&program, Dialect::Clingo).expect("nothing unspellable");

    // ...then parse the rendered text through the syntax door, whose result is
    // `Parse<ast::Program>` — the AST reached namespaced.
    let source: Source = make_source(rendered).expect("within the size bound");
    let parsed: Parse<ast::Program> = parse(&source, Dialect::Clingo);
    assert!(!parsed.has_errors());
    assert_eq!(
        parsed.tree().statements().count(),
        program.statements().count()
    );

    // `base` (the module) is re-exported by BOTH preludes and is ONE item, so its
    // `Source` is the same type the flat re-export above names.
    let _: Option<base::source::Source> = None;

    // The raise door's source-level result is nameable through the prelude, beside
    // `Raised`, its `Parse`-level sibling.
    let _: Option<RaisedSource> = None;
    let _: Option<Raised> = None;
}
