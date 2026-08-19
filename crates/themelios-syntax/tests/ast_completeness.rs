//! The typed AST's completeness over the roster (docs/design/syntax.md
//! §8.1, §16): every node kind is cast by exactly one wrapper — the
//! structural half of the law; that every production slot has an
//! accessor is held by reading `ast` against Appendix A and by the
//! accessor tests in the module.

use themelios_syntax::ast;
use themelios_syntax::tree::{AstNode, SyntaxKind};

#[test]
fn every_node_kind_is_cast_by_exactly_one_wrapper() {
    type Caster = fn(SyntaxKind) -> bool;
    let wrappers: [(&str, Caster); 59] = [
        ("Program", ast::Program::can_cast),
        ("StatementFragment", ast::StatementFragment::can_cast),
        ("TermFragment", ast::TermFragment::can_cast),
        ("Rule", ast::Rule::can_cast),
        ("WeakConstraint", ast::WeakConstraint::can_cast),
        ("OptimizeStatement", ast::OptimizeStatement::can_cast),
        ("OptimizeElement", ast::OptimizeElement::can_cast),
        ("ShowStatement", ast::ShowStatement::can_cast),
        ("Signature", ast::Signature::can_cast),
        ("ProjectStatement", ast::ProjectStatement::can_cast),
        ("DefinedStatement", ast::DefinedStatement::can_cast),
        ("EdgeStatement", ast::EdgeStatement::can_cast),
        ("Edge", ast::Edge::can_cast),
        ("HeuristicStatement", ast::HeuristicStatement::can_cast),
        ("ExternalStatement", ast::ExternalStatement::can_cast),
        ("ConstStatement", ast::ConstStatement::can_cast),
        ("ScriptStatement", ast::ScriptStatement::can_cast),
        ("IncludeStatement", ast::IncludeStatement::can_cast),
        ("ProgramStatement", ast::ProgramStatement::can_cast),
        ("Parameters", ast::Parameters::can_cast),
        ("TheoryDefinition", ast::TheoryDefinition::can_cast),
        ("TermDefinition", ast::TermDefinition::can_cast),
        ("OpDefinition", ast::OpDefinition::can_cast),
        ("AtomDefinition", ast::AtomDefinition::can_cast),
        ("Query", ast::Query::can_cast),
        ("Annotation", ast::Annotation::can_cast),
        ("Body", ast::Body::can_cast),
        ("Literal", ast::Literal::can_cast),
        ("Atom", ast::Atom::can_cast),
        ("Comparison", ast::Comparison::can_cast),
        ("ConditionalLiteral", ast::ConditionalLiteral::can_cast),
        ("Condition", ast::Condition::can_cast),
        ("Disjunction", ast::Disjunction::can_cast),
        ("FunctionAggregate", ast::FunctionAggregate::can_cast),
        ("SetAggregate", ast::SetAggregate::can_cast),
        ("Guard", ast::Guard::can_cast),
        ("BodyAggregateElement", ast::BodyAggregateElement::can_cast),
        ("HeadAggregateElement", ast::HeadAggregateElement::can_cast),
        ("TheoryAtom", ast::TheoryAtom::can_cast),
        ("TheoryElements", ast::TheoryElements::can_cast),
        ("TheoryElement", ast::TheoryElement::can_cast),
        ("TheoryOpTerm", ast::TheoryOpTerm::can_cast),
        ("TheoryGuard", ast::TheoryGuard::can_cast),
        ("TheorySet", ast::TheorySet::can_cast),
        ("TheoryList", ast::TheoryList::can_cast),
        ("TheoryTuple", ast::TheoryTuple::can_cast),
        ("TheoryFunction", ast::TheoryFunction::can_cast),
        ("BinaryTerm", ast::BinaryTerm::can_cast),
        ("UnaryTerm", ast::UnaryTerm::can_cast),
        ("Pool", ast::Pool::can_cast),
        ("Tuple", ast::Tuple::can_cast),
        ("Arguments", ast::Arguments::can_cast),
        ("FunctionTerm", ast::FunctionTerm::can_cast),
        ("ExternalTerm", ast::ExternalTerm::can_cast),
        ("AbsTerm", ast::AbsTerm::can_cast),
        ("ConstantTerm", ast::ConstantTerm::can_cast),
        ("VariableTerm", ast::VariableTerm::can_cast),
        ("SpliceTerm", ast::SpliceTerm::can_cast),
        ("Error", ast::Error::can_cast),
    ];
    for kind in SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|kind| kind.is_node())
    {
        let casting: Vec<&str> = wrappers
            .iter()
            .filter(|(_, casts)| casts(kind))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            casting.len(),
            1,
            "{kind}: cast by {casting:?}, not by exactly one wrapper"
        );
    }
    for (name, casts) in &wrappers {
        assert!(
            SyntaxKind::ALL
                .iter()
                .any(|kind| kind.is_node() && casts(*kind)),
            "{name} casts no node kind"
        );
    }
}
