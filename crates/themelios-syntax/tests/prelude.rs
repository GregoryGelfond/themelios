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
fn the_source_text_reader_is_reached_through_tree() {
    // The reader composes the coordinate seam and is reached as the seam
    // is — by its module path, not the glob.
    let source = admitted("p(1, 2).", 0);
    let root: SyntaxNode = parse(&source, Dialect::Clingo).syntax();
    assert_eq!(
        themelios_syntax::tree::source_text(&source, root.text_range()),
        Ok("p(1, 2).")
    );
}

#[test]
fn the_bracket_pair_table_is_reached_through_tree() {
    // The kind-level pair table stands beside the roster and is reached as
    // `tree`'s helpers are — by its module path, not the glob.
    assert_eq!(
        themelios_syntax::tree::closer_of(SyntaxKind::L_BRACE),
        Some(SyntaxKind::R_BRACE)
    );
}

#[test]
fn the_role_pass_is_reached_through_tree() {
    // The per-node forward pass over roles stands beside `role` and is
    // reached as `tree`'s helpers are — by its module path, not the glob.
    let parsed = parse(&admitted("%! d\np. % c\n", 0), Dialect::Clingo);
    let rule = parsed.syntax().first_child().expect("the rule");
    let roles: Vec<TokenRole> = themelios_syntax::tree::roles_of(&rule)
        .map(|(_, role)| role)
        .collect();
    assert_eq!(
        roles,
        [
            TokenRole::Documentation,
            TokenRole::Trivia,
            TokenRole::Significant
        ]
    );
}

#[test]
fn the_glob_names_the_attachment_surface() {
    let parsed = parse(&admitted("% lead\np.\n", 0), Dialect::Clingo);
    let (comment, attachment) = attachments(&parsed.syntax()).next().expect("a comment");
    assert_eq!(comment.content(), "% lead");
    assert_eq!(attachment.slot, Slot::Leading);
}

#[test]
fn the_glob_names_the_token_trait() {
    // `Comment`'s own accessors need no trait; the token behind it —
    // `syntax`, `text`, and `cast` back from the token — is `AstToken`'s,
    // in scope through the glob exactly as `AstNode` is.
    let parsed = parse(&admitted("p. % c\n", 0), Dialect::Clingo);
    let (comment, _) = attachments(&parsed.syntax()).next().expect("a comment");
    let token: &SyntaxToken = comment.syntax();
    assert_eq!(token.kind(), SyntaxKind::LINE_COMMENT);
    assert_eq!(comment.text(), "% c");
    let recast = ast::Comment::cast(token.clone());
    assert_eq!(recast, Some(comment));
}

#[test]
fn the_crate_root_names_the_token_trait() {
    // `themelios_syntax::AstToken` resolves beside `themelios_syntax::AstNode`
    // — the path form names the crate-root re-export directly, even though the
    // file's prelude glob (above) also brings the trait into scope.
    let parsed = parse(&admitted("p. % c\n", 0), Dialect::Clingo);
    let (comment, _) = attachments(&parsed.syntax()).next().expect("a comment");
    assert_eq!(themelios_syntax::AstToken::text(&comment), "% c");
}

#[test]
fn the_trivia_walk_is_reached_through_attach() {
    // The element-level trivia classification and the walks over it are
    // reached by their module path; `Direction` is rowan's, behind `tree`.
    use themelios_syntax::attach::{
        is_skipped, non_trivia_sibling, significant_children, skip_trivia_token,
    };
    use themelios_syntax::tree::Direction;

    let parsed = parse(&admitted("p. % c\nq.\n", 0), Dialect::Clingo);
    let root = parsed.syntax();
    let rules: Vec<SyntaxElement> = significant_children(&root).collect();
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|rule| !is_skipped(rule)));
    assert_eq!(
        non_trivia_sibling(rules[0].clone(), Direction::Next),
        Some(rules[1].clone())
    );
    let comment = root
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find(|token| token.text() == "% c")
        .expect("the comment");
    assert!(is_skipped(&SyntaxElement::Token(comment.clone())));
    assert_eq!(
        skip_trivia_token(comment, Direction::Next).map(|token| token.text().to_owned()),
        Some("q".to_owned())
    );
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
fn the_content_projections_are_reached_through_equiv() {
    // The per-token projection the certificates compare — `content`, and
    // `compared` under a certificate — is reached by its module path, not
    // the glob: the bare names are too generic to flatten.
    use themelios_syntax::equiv::{compared, content};

    let parsed = parse(&admitted("p. % c  \n", 0), Dialect::Clingo);
    let comment = non_whitespace_tokens(&parsed.syntax())
        .find(|token: &SyntaxToken| token.kind() == SyntaxKind::LINE_COMMENT)
        .expect("the comment");
    assert_eq!(content(&comment), "% c");
    assert_eq!(compared(&comment, Certificate::LayoutOnly), "% c");
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
