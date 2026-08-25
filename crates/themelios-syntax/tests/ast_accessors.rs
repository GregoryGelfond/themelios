//! Accessor coverage for the typed AST (docs/design/syntax.md §8): each
//! node type parsed from a canonical, well-formed instance, and every
//! accessor asserted to read its slot — the value enums to their variant,
//! the child and token slots to their presence. The exercise the mutation
//! audit found the typed AST was missing.

use themelios_syntax::ast::{
    AggregateFunction, Associativity, AstToken, Atom, AtomDefinition, BinaryTerm, Body,
    BodyAggregateElement, Comparison, ConditionalLiteral, ConstPolicy, ConstStatement, Constant,
    DefinedStatement, DocLine, Edge, EdgeStatement, ExternalStatement, ExternalTerm,
    FunctionAggregate, FunctionTerm, Guard, HeadAggregateElement, HeuristicStatement,
    IncludeStatement, Literal, LiteralInner, NumberLit, OpDefinition, OptimizeElement,
    OptimizeStatement, Parameters, Precedence, ProgramStatement, ProjectStatement, Query, Radix,
    Relation, ScriptStatement, ShowStatement, Signature, StringLit, TermDefinition, TheoryAtom,
    TheoryDefinition, TheoryElement, TheoryElements, TheoryFunction, TheoryGuard, TheoryList,
    TheoryOpTerm, TheoryOpTermItem, TheorySet, TheoryTuple, UnaryTerm,
};
use themelios_syntax::base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{Asp, AstNode, SyntaxElement, SyntaxNode};

fn tree(text: &str, dialect: Dialect) -> SyntaxNode {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parsed = parse(&source, dialect);
    assert!(
        !parsed.has_errors(),
        "{text:?} is a member: {:?}",
        parsed.diagnostics()
    );
    parsed.syntax()
}

/// The first node of type `N` in a clingo parse of `text`, which is a member.
fn first<N: AstNode<Language = Asp>>(text: &str) -> N {
    tree(text, Dialect::Clingo)
        .descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("{text:?} holds no node of the wanted type"))
}

fn first_in<N: AstNode<Language = Asp>>(text: &str, dialect: Dialect) -> N {
    tree(text, dialect)
        .descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("{text:?} holds no node of the wanted type"))
}

/// The first token of type `T` in a clingo parse of `text`, which is a member.
fn first_token<T: AstToken>(text: &str) -> T {
    tree(text, Dialect::Clingo)
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find_map(T::cast)
        .unwrap_or_else(|| panic!("{text:?} holds no token of the wanted type"))
}

/// A `StringLit` over a hand-built `STRING` token bearing `text` verbatim —
/// the shape only a token source other than the file lexer can supply, which
/// `StringLit::value` is contracted to refuse (docs/design/syntax.md §3, §8).
fn foreign_string(text: &str) -> StringLit {
    use rowan::{GreenToken, Language, NodeOrToken};
    use themelios_syntax::tree::{GreenNode, SyntaxKind};
    let raw = Asp::kind_to_raw;
    let token = NodeOrToken::Token(GreenToken::new(raw(SyntaxKind::STRING), text));
    let root = SyntaxNode::new_root(GreenNode::new(raw(SyntaxKind::PROGRAM), vec![token]));
    root.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find_map(StringLit::cast)
        .expect("the string token")
}

// ---- statements ---------------------------------------------------------

#[test]
fn weak_constraint_reads_its_body_and_annotation() {
    let n: themelios_syntax::ast::WeakConstraint = first(":~ q. [1@2, x]");
    assert!(n.weak_neck_token().is_some());
    assert!(n.body().is_some());
    assert!(n.dot_token().is_some());
    assert!(n.annotation().is_some());
    assert!(n.weight().is_some());
    assert!(n.priority().is_some());
    assert_eq!(n.tuple().count(), 1);
}

#[test]
fn optimize_statement_and_element_read_their_slots() {
    let s: OptimizeStatement = first("#minimize { 1@2, x : q }.");
    assert!(s.keyword_token().is_some());
    assert!(s.l_brace_token().is_some());
    assert!(s.r_brace_token().is_some());
    assert!(s.dot_token().is_some());
    assert_eq!(s.elements().count(), 1);
    let e: OptimizeElement = first("#minimize { 1@2, x : q }.");
    assert!(e.weight().is_some());
    assert!(e.at_token().is_some());
    assert!(e.priority().is_some());
    assert_eq!(e.tuple().count(), 1, "the tuple stops at the colon");
    assert!(e.colon_token().is_some());
    assert!(e.condition().is_some());
}

#[test]
fn show_statement_reads_the_signature_and_the_conditioned_term_forms() {
    let sig: ShowStatement = first("#show p/1.");
    assert!(sig.show_token().is_some());
    assert!(sig.signature().is_some());
    assert!(sig.dot_token().is_some());
    let term: ShowStatement = first("#show a : b.");
    assert!(term.term().is_some());
    assert!(term.colon_token().is_some());
    assert!(term.body().is_some());
}

#[test]
fn signature_reads_its_strong_negation_name_and_arity() {
    let n: Signature = first("#show -p/1.");
    assert!(n.strong_negation_token().is_some());
    assert!(n.name().is_some());
    assert!(n.slash_token().is_some());
    assert!(n.arity().is_some());
}

#[test]
fn project_statement_reads_the_signature_and_atom_forms() {
    let sig: ProjectStatement = first("#project p/1.");
    assert!(sig.signature().is_some());
    assert!(sig.dot_token().is_some());
    let atom: ProjectStatement = first("#project a : b.");
    assert!(atom.atom().is_some());
    assert!(atom.colon_token().is_some());
    assert!(atom.body().is_some());
}

#[test]
fn defined_statement_reads_its_signature() {
    let n: DefinedStatement = first("#defined p/1.");
    assert!(n.signature().is_some());
    assert!(n.dot_token().is_some());
}

#[test]
fn edge_statement_and_edge_read_their_slots() {
    let s: EdgeStatement = first("#edge (a, b) : c.");
    assert!(s.l_paren_token().is_some());
    assert!(s.r_paren_token().is_some());
    assert!(s.colon_token().is_some());
    assert!(s.body().is_some());
    assert!(s.dot_token().is_some());
    assert_eq!(s.edges().count(), 1);
    let e: Edge = first("#edge (a, b).");
    assert!(e.from().is_some());
    assert!(e.comma_token().is_some());
    assert!(e.to().is_some());
}

#[test]
fn heuristic_statement_reads_its_atom_body_and_annotation() {
    let n: HeuristicStatement = first("#heuristic a : b. [1@2, sign]");
    assert!(n.atom().is_some());
    assert!(n.colon_token().is_some());
    assert!(n.body().is_some());
    assert!(n.dot_token().is_some());
    assert!(n.annotation().is_some());
    assert!(n.weight().is_some());
    assert!(n.priority().is_some());
    assert!(n.modifier().is_some());
}

#[test]
fn external_statement_reads_its_atom_body_and_value() {
    let n: ExternalStatement = first("#external a : b. [x]");
    assert!(n.atom().is_some());
    assert!(n.colon_token().is_some());
    assert!(n.body().is_some());
    assert!(n.dot_token().is_some());
    assert!(n.annotation().is_some());
    assert!(n.value().is_some());
}

#[test]
fn const_statement_reads_its_name_value_and_policy() {
    let n: ConstStatement = first("#const c = 1. [override]");
    assert!(n.name().is_some());
    assert!(n.eq_token().is_some());
    assert!(n.value().is_some());
    assert!(n.dot_token().is_some());
    assert!(n.annotation().is_some());
    assert_eq!(n.policy(), Some(ConstPolicy::Override));
    let d: ConstStatement = first("#const c = 1. [default]");
    assert_eq!(d.policy(), Some(ConstPolicy::Default));
}

#[test]
fn script_statement_reads_its_language_body_and_end() {
    let n: ScriptStatement = first("#script (lua) x = 1 #end.");
    assert!(n.language().is_some());
    assert!(n.body().is_some());
    assert!(n.end_token().is_some());
    assert!(n.dot_token().is_some());
}

#[test]
fn include_statement_reads_its_path_and_library_forms() {
    let path: IncludeStatement = first("#include \"file.lp\".");
    assert!(path.path().is_some());
    assert!(path.dot_token().is_some());
    let library: IncludeStatement = first("#include <lib>.");
    assert!(library.library().is_some());
}

#[test]
fn program_statement_and_parameters_read_their_names() {
    let n: ProgramStatement = first("#program part(a, b).");
    assert!(n.name().is_some());
    assert!(n.parameters().is_some());
    assert!(n.dot_token().is_some());
    let p: Parameters = first("#program part(a, b).");
    assert_eq!(p.names().count(), 2);
}

#[test]
fn theory_definition_term_and_op_and_atom_definitions_read_their_slots() {
    let input = "#theory t { x { + : 1, binary, left; - : 2, unary }; &a/0 : x, {<=}, y, any }.";
    let def: TheoryDefinition = first(input);
    assert!(def.name().is_some());
    assert!(def.dot_token().is_some());
    assert_eq!(
        def.items().count(),
        2,
        "a term definition and an atom definition"
    );
    let term: TermDefinition = first(input);
    assert!(term.name().is_some());
    assert_eq!(term.op_definitions().count(), 2);
    let op: OpDefinition = first(input);
    assert!(op.operator_token().is_some());
    assert!(op.priority().is_some());
    assert!(op.arity_token().is_some());
    assert_eq!(op.associativity(), Some(Associativity::Left));
    // Both associativity words, so neither arm is dead.
    assert_eq!(
        first::<OpDefinition>("#theory t { x { + : 1, binary, right } }.").associativity(),
        Some(Associativity::Right)
    );
    let atom: AtomDefinition = first(input);
    assert!(atom.name().is_some());
    assert!(atom.arity().is_some());
    // The term type is the ident *after* the colon (`x`), not the name before
    // it — the skip-to-colon is what places it.
    assert_eq!(atom.type_name().expect("a type name").syntax().text(), "x");
    assert_eq!(atom.guard_operators().count(), 1);
    assert!(atom.occurrence().is_some());
}

#[test]
fn query_reads_its_atom_and_question() {
    let n: Query = first_in("p?", Dialect::AspCore2);
    assert!(n.atom().is_some());
    assert!(n.question_token().is_some());
}

#[test]
fn annotation_reads_its_brackets_and_terms() {
    let n: themelios_syntax::ast::Annotation = first(":~ q. [1@2]");
    assert!(n.l_bracket_token().is_some());
    assert!(n.r_bracket_token().is_some());
    assert!(n.terms().count() >= 1);
}

// ---- rule interiors -----------------------------------------------------

#[test]
fn body_reads_its_elements_and_separators() {
    let n: Body = first(":- a, b; c.");
    assert_eq!(n.elements().count(), 3);
    assert_eq!(n.separator_tokens().count(), 2);
}

#[test]
fn literal_reads_its_inner_forms() {
    assert!(matches!(
        first::<Literal>("#true.").inner(),
        Some(LiteralInner::True(_))
    ));
    assert!(matches!(
        first::<Literal>("#false.").inner(),
        Some(LiteralInner::False(_))
    ));
    assert!(matches!(
        first::<Literal>("p.").inner(),
        Some(LiteralInner::Atom(_))
    ));
    assert!(matches!(
        first::<Literal>(":- a < b.").inner(),
        Some(LiteralInner::Comparison(_))
    ));
}

#[test]
fn atom_reads_its_negation_name_and_arguments() {
    let n: Atom = first("-p(a).");
    assert!(n.strong_negation_token().is_some());
    assert!(n.name().is_some());
    assert!(n.arguments().is_some());
}

#[test]
fn comparison_reads_every_relation() {
    let relation = |op: &str| {
        first::<Comparison>(&format!(":- a {op} b."))
            .steps()
            .next()
            .expect("a step")
            .0
    };
    assert_eq!(relation("<"), Relation::Lt);
    assert_eq!(relation("<="), Relation::Le);
    assert_eq!(relation(">"), Relation::Gt);
    assert_eq!(relation(">="), Relation::Ge);
    assert_eq!(relation("="), Relation::Eq);
    assert_eq!(relation("!="), Relation::Neq);
}

#[test]
fn conditional_literal_reads_its_literal_and_condition() {
    let n: ConditionalLiteral = first("a : b | c.");
    assert!(n.literal().is_some());
    assert!(n.colon_token().is_some());
    assert!(n.condition().is_some());
}

// ---- aggregates ---------------------------------------------------------

#[test]
fn function_aggregate_reads_every_function_and_its_braces() {
    let function = |kw: &str| {
        first::<FunctionAggregate>(&format!(":- {kw} {{ x }} >= 1."))
            .function()
            .expect("a function")
    };
    assert_eq!(function("#count"), AggregateFunction::Count);
    assert_eq!(function("#sum"), AggregateFunction::Sum);
    assert_eq!(function("#sum+"), AggregateFunction::SumPlus);
    assert_eq!(function("#min"), AggregateFunction::Min);
    assert_eq!(function("#max"), AggregateFunction::Max);
    let n: FunctionAggregate = first(":- #count { x } >= 1.");
    assert!(n.l_brace_token().is_some());
    assert!(n.r_brace_token().is_some());
    assert_eq!(n.elements().count(), 1);
}

#[test]
fn set_aggregate_and_guard_read_their_slots() {
    let n: themelios_syntax::ast::SetAggregate = first(":- 1 <= { a } >= 2.");
    assert!(n.l_brace_token().is_some());
    assert!(n.r_brace_token().is_some());
    assert_eq!(n.elements().count(), 1);
    let g: Guard = first(":- 1 <= { a }.");
    assert_eq!(g.relation(), Some(Relation::Le));
    assert!(g.term().is_some());
}

#[test]
fn body_aggregate_element_reads_its_terms_and_condition() {
    let n: BodyAggregateElement = first(":- #count { x : p }.");
    assert_eq!(n.terms().count(), 1);
    assert!(n.colon_token().is_some());
    assert!(n.condition().is_some());
}

#[test]
fn head_aggregate_element_reads_its_terms_literal_and_condition() {
    let n: HeadAggregateElement = first("#count { x : a : b } >= 1.");
    assert_eq!(n.terms().count(), 1);
    assert!(n.colon_token().is_some());
    assert!(n.literal().is_some());
    assert!(n.second_colon_token().is_some());
    assert!(n.condition().is_some());
}

// ---- theory atoms -------------------------------------------------------

#[test]
fn theory_atom_and_its_parts_read_their_slots() {
    let atom: TheoryAtom = first(":- &sum(1) { x + y : p } >= 3.");
    assert!(atom.name().is_some());
    assert!(atom.arguments().is_some());
    assert!(atom.elements().is_some());
    assert!(atom.guard().is_some());
    let elements: TheoryElements = first(":- &sum { x } >= 3.");
    assert!(elements.l_brace_token().is_some());
    assert!(elements.r_brace_token().is_some());
    assert_eq!(elements.elements().count(), 1);
    let element: TheoryElement = first(":- &sum { x + y : p } >= 3.");
    assert_eq!(element.opterms().count(), 1);
    assert!(element.colon_token().is_some());
    assert!(element.condition().is_some());
    let guard: TheoryGuard = first(":- &sum { x } >= 3.");
    assert!(guard.operator_token().is_some());
    assert!(guard.opterm().is_some());
}

#[test]
fn theory_opterm_reads_its_operators_and_terms() {
    let n: TheoryOpTerm = first("&a { x + y }.");
    let items: Vec<_> = n.items().collect();
    assert!(items.iter().any(|i| matches!(i, TheoryOpTermItem::Op(_))));
    assert!(items.iter().any(|i| matches!(i, TheoryOpTermItem::Term(_))));
}

#[test]
fn theory_term_forms_read_their_opterms() {
    assert_eq!(first::<TheorySet>("&a { {x, y} }.").opterms().count(), 2);
    assert_eq!(first::<TheoryList>("&a { [x, y] }.").opterms().count(), 2);
    let tuple: TheoryTuple = first("&a { (x,) }.");
    assert_eq!(tuple.opterms().count(), 1);
    assert!(tuple.trailing_comma_token().is_some());
    let function: TheoryFunction = first("&a { f(x, y) }.");
    assert!(function.name().is_some());
    assert_eq!(function.opterms().count(), 2);
}

// ---- terms --------------------------------------------------------------

#[test]
fn binary_term_reads_every_precedence_level() {
    let level = |op: &str| {
        first::<BinaryTerm>(&format!("p(1 {op} 2)."))
            .level()
            .expect("a level")
    };
    assert_eq!(level(".."), Precedence::Interval);
    assert_eq!(level("^"), Precedence::BitXor);
    assert_eq!(level("?"), Precedence::BitOr);
    assert_eq!(level("&"), Precedence::BitAnd);
    assert_eq!(level("+"), Precedence::Additive);
    assert_eq!(level("*"), Precedence::Multiplicative);
    assert_eq!(level("/"), Precedence::Multiplicative);
    assert_eq!(level("\\"), Precedence::Multiplicative);
    assert_eq!(level("**"), Precedence::Exponentiation);
    let n: BinaryTerm = first("p(1 ** 2).");
    assert_eq!(n.operands().count(), 2);
    assert_eq!(n.operators().count(), 1);
    assert_eq!(n.associativity(), Some(Associativity::Right));
    assert_eq!(
        first::<BinaryTerm>("p(1 + 2).").associativity(),
        Some(Associativity::Left)
    );
}

#[test]
fn unary_term_reads_its_operators_and_operand() {
    let n: UnaryTerm = first("p(- - x).");
    assert_eq!(n.operators().count(), 2);
    assert!(n.operand().is_some());
}

#[test]
fn arguments_and_function_and_external_terms_read_their_slots() {
    let args: themelios_syntax::ast::Arguments = first("p(a).");
    assert!(args.l_paren_token().is_some());
    assert!(args.r_paren_token().is_some());
    assert_eq!(args.alternatives().count(), 1);
    let function: FunctionTerm = first("p(f(x)).");
    assert!(function.name().is_some());
    assert!(function.arguments().is_some());
    let external: ExternalTerm = first("p(@f(x)).");
    assert!(external.name().is_some());
    assert!(external.arguments().is_some());
}

#[test]
fn a_trailing_comma_is_read_only_when_it_trails() {
    // The shared `trailing_comma` reader: a theory tuple `(x,)` has one, `(x,
    // y)` does not (the comma trails nothing only when no term follows it).
    assert!(
        first::<TheoryTuple>("&a { (x,) }.")
            .trailing_comma_token()
            .is_some()
    );
    assert!(
        first::<TheoryTuple>("&a { (x, y) }.")
            .trailing_comma_token()
            .is_none()
    );
}

#[test]
fn constant_term_reads_every_constant_form() {
    assert!(matches!(
        first::<themelios_syntax::ast::ConstantTerm>("p(a).").constant(),
        Some(Constant::Symbol(_))
    ));
    assert!(matches!(
        first::<themelios_syntax::ast::ConstantTerm>("p(1).").constant(),
        Some(Constant::Number(_))
    ));
    assert!(matches!(
        first::<themelios_syntax::ast::ConstantTerm>("p(\"s\").").constant(),
        Some(Constant::String(_))
    ));
    assert!(matches!(
        first::<themelios_syntax::ast::ConstantTerm>("p(#inf).").constant(),
        Some(Constant::Infimum(_))
    ));
    assert!(matches!(
        first::<themelios_syntax::ast::ConstantTerm>("p(#sup).").constant(),
        Some(Constant::Supremum(_))
    ));
}

// ---- valued tokens ------------------------------------------------------

#[test]
fn number_literal_reads_every_radix_and_its_digits() {
    let n: NumberLit = first_token("p(42).");
    assert_eq!(n.radix(), Radix::Decimal);
    assert_eq!(n.digits(), "42");
    assert_eq!(
        first_token::<NumberLit>("p(0xFF).").radix(),
        Radix::Hexadecimal
    );
    assert_eq!(first_token::<NumberLit>("p(0o17).").radix(), Radix::Octal);
    assert_eq!(first_token::<NumberLit>("p(0b101).").radix(), Radix::Binary);
    assert_eq!(first_token::<NumberLit>("p(0o17).").digits(), "17");
    assert_eq!(
        first_token::<NumberLit>("p(42).").digits(),
        "42",
        "no prefix stripped"
    );
}

#[test]
fn string_literal_value_resolves_the_dialect_escapes() {
    // The reachable escape arms, from real lexer strings.
    assert_eq!(
        first_token::<StringLit>("p(\"a\\nb\").")
            .value(Dialect::Clingo)
            .unwrap(),
        "a\nb"
    );
    assert_eq!(
        first_token::<StringLit>("p(\"a\\\\b\").")
            .value(Dialect::Clingo)
            .unwrap(),
        "a\\b"
    );
    assert_eq!(
        first_token::<StringLit>("p(\"a\\\"b\").")
            .value(Dialect::Clingo)
            .unwrap(),
        "a\"b"
    );
    // The ASP-Core-2 rule: a backslash is itself unless a quote follows.
    assert_eq!(
        first_in::<themelios_syntax::ast::ConstantTerm>("label(\"a\\b\").", Dialect::AspCore2)
            .constant()
            .and_then(|c| match c {
                Constant::String(s) => s.value(Dialect::AspCore2).ok(),
                _ => None,
            }),
        Some("a\\b".to_owned())
    );
}

#[test]
fn string_literal_value_refuses_a_foreign_bad_spelling_at_its_break() {
    // The contract paths a token source other than the file lexer reaches: a
    // token with no surrounding quotes, a raw quote or newline inside under
    // clingo, and a bad escape — each refused, `at` the byte the spelling
    // breaks (the token is built at offset zero).
    assert!(foreign_string("xy").value(Dialect::Clingo).is_err());
    assert!(foreign_string("\"a").value(Dialect::Clingo).is_err());
    assert_eq!(
        foreign_string("\"a\"b\"")
            .value(Dialect::Clingo)
            .unwrap_err()
            .at
            .get(),
        2,
        "the raw quote inside, one past the opening quote and the `a`"
    );
    assert_eq!(
        foreign_string("\"a\\qb\"")
            .value(Dialect::Clingo)
            .unwrap_err()
            .at
            .get(),
        2,
        "the bad escape at the backslash"
    );
    // A well-formed foreign token is accepted, so the guard is exact, not
    // always-refusing.
    assert_eq!(
        foreign_string("\"ok\"").value(Dialect::Clingo).unwrap(),
        "ok"
    );
    // A token that ends with a quote but does not start with one is refused —
    // each clause of the guard's disjunction is load-bearing.
    assert!(foreign_string("x\"").value(Dialect::Clingo).is_err());
    // A lone quote (length one) is refused, and the empty string `""` (length
    // two) is accepted — the length bound is `< 2`, not `== 2` or `<= 2`.
    assert!(foreign_string("\"").value(Dialect::Clingo).is_err());
    assert_eq!(foreign_string("\"\"").value(Dialect::Clingo).unwrap(), "");
}

#[test]
fn doc_line_casts_only_a_documentation_doc_comment() {
    // A `%!` in docs position is documentation and casts; the same token as
    // trivia (after a statement) does not — the cast reads role, not kind.
    let doc: DocLine = first_token("%! a\np.");
    assert_eq!(doc.content(), " a");
    let stray = tree("p. %! stray\n", Dialect::Clingo)
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find(|t| t.text() == "%! stray")
        .expect("the stray token");
    assert!(
        DocLine::cast(stray).is_none(),
        "a trivia doc comment is not documentation"
    );
}
