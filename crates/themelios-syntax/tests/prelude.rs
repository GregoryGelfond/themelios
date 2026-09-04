//! The prelude: one glob import names the tier's working vocabulary — the
//! parse doors, the dialect, the tree and its kind roster, comment
//! attachment, the fusion oracle, the certificates, the typed diagnostics —
//! and reaches the typed AST through its module, `ast::Program`, never as
//! a flat glob of the AST's type names: those are the program tier's names
//! too, and a client that globs both tiers' preludes must meet no ambiguity.

use themelios_syntax::prelude::*;

fn admitted(text: &str, id: u32) -> base::source::Source {
    base::source::Source::new(base::source::SourceId::new(id), text.to_owned()).expect("admits")
}

/// The AST is named through its module after the glob — `ast::Program`,
/// not `Program`.
fn statement_count(program: &ast::Program) -> usize {
    program.statements().count()
}

#[test]
fn the_glob_names_the_dialect() {
    assert_eq!(Dialect::Clingo, Dialect::default());
}

#[test]
fn the_glob_names_the_parse_door() {
    let parsed: Parse<ast::Program> = parse(&admitted("p(1). q(X) :- p(X).\n", 0), Dialect::Clingo);
    assert!(!parsed.has_errors());
    assert_eq!(parsed.dialect(), Dialect::Clingo);
}

#[test]
fn the_glob_names_the_string_door() {
    // The door is in the glob; the identity it mints is reached by its
    // module path, as the advanced surfaces are.
    let parsed: Parse<ast::Program> =
        parse_str("p(1). q(X) :- p(X).\n", Dialect::Clingo).expect("admits");
    assert!(!parsed.has_errors());
    assert_eq!(
        parsed.source(),
        themelios_syntax::parse::STRING_INPUT_SOURCE_ID
    );
}

#[test]
fn the_ast_is_reached_through_its_module() {
    let parsed = parse(&admitted("p(1). q(X) :- p(X).\n", 0), Dialect::Clingo);
    assert_eq!(statement_count(&parsed.tree()), 2);
    assert!(matches!(
        parsed.tree().statements().next(),
        Some(ast::Statement::Rule(_))
    ));
}

#[test]
fn the_glob_names_the_tree_vocabulary() {
    let parsed = parse(&admitted("%! d\np. % c\n", 0), Dialect::Clingo);
    let root: SyntaxNode = parsed.syntax();
    assert_eq!(root.kind(), SyntaxKind::PROGRAM);
    let roles: Vec<TokenRole> = root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .map(|token: SyntaxToken| role(&token))
        .collect();
    assert!(roles.contains(&TokenRole::Documentation));
    assert!(roles.contains(&TokenRole::Trivia));
    assert!(roles.contains(&TokenRole::Significant));
}

#[test]
fn the_glob_names_the_attachment_surface() {
    let parsed = parse(&admitted("% lead\np.\n", 0), Dialect::Clingo);
    let (comment, attachment) = attachments(&parsed.syntax()).next().expect("a comment");
    assert_eq!(comment.text(), "% lead");
    assert_eq!(attachment.slot, Slot::Leading);
}

#[test]
fn the_glob_names_the_fusion_oracle() {
    let context = LexContext {
        dialect: Dialect::Clingo,
        mode: LexMode::Normal,
    };
    assert_eq!(
        separator_between("not", "p", context),
        Separator::Whitespace
    );
    assert_eq!(separator_between("p", "(", context), Separator::Nothing);
}

#[test]
fn the_glob_names_the_certificates() {
    let left = parse(&admitted("p(X):-q(X).", 1), Dialect::Clingo);
    let right = parse(&admitted("p( X ) :- q(X) .", 2), Dialect::Clingo);
    assert_eq!(equivalent(&left, &right, Certificate::LayoutOnly), Ok(()));
    let other = parse(&admitted("p(X):-r(X).", 3), Dialect::Clingo);
    let mismatch: Mismatch =
        equivalent(&left, &other, Certificate::UpToSpelling).expect_err("diverges");
    assert_eq!(
        mismatch.left.map(|side: Side| side.content),
        Some("q".to_owned())
    );
}

#[test]
fn the_glob_names_the_typed_diagnostics() {
    let parsed = parse(&admitted("p(1", 0), Dialect::Clingo);
    assert!(parsed.is_incomplete());
    let error: &SyntaxError = parsed.diagnostics().first().expect("a diagnostic");
    assert_eq!(error.severity(), base::diagnostic::Severity::Error);
    assert!(matches!(
        error.kind(),
        SyntaxErrorKind::UnexpectedEndOfInput { .. }
    ));
}
