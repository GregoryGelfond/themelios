//! The typed AST (docs/design/syntax.md §8): cheap wrappers over the red
//! cursors, one per node kind, an enum per grammar class, accessors
//! mirroring the productions' slots in the productions' order —
//! syntactic accessors, no semantic opinions. Every wrapper is a view:
//! `!Send`, borrowed from the model, positional in its equality. This
//! file holds the wrapper macros, the enums, the traits, and the roots;
//! `nodes` the wrappers, `tokens` the token wrappers and their values.

mod nodes;
mod tokens;

use rowan::ast::support;

use crate::tree::{
    Asp, AstChildren, AstNode, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextRange,
    TokenRole, role,
};

pub use self::nodes::{
    AbsTerm, AggregateFunction, Annotation, Arguments, Associativity, Atom, AtomDefinition,
    BinaryTerm, Body, BodyAggregateElement, Comparison, Condition, ConditionalLiteral, ConstPolicy,
    ConstStatement, ConstantTerm, DefinedStatement, Disjunction, Edge, EdgeStatement, Error,
    ExternalStatement, ExternalTerm, FunctionAggregate, FunctionTerm, Guard, HeadAggregateElement,
    HeuristicStatement, IncludeStatement, Literal, Negation, OpDefinition, OptimizeElement,
    OptimizeStatement, Parameters, Pool, Precedence, ProgramStatement, ProjectStatement, Query,
    Relation, Rule, ScriptStatement, SetAggregate, ShowStatement, Signature, SpliceTerm,
    TermDefinition, TheoryAtom, TheoryDefinition, TheoryElement, TheoryElements, TheoryFunction,
    TheoryGuard, TheoryList, TheoryOpTerm, TheorySet, TheoryTuple, Tuple, UnaryTerm, VariableTerm,
    WeakConstraint,
};
pub use self::tokens::{
    AstToken, Comment, CommentForm, DocLine, Ident, InvalidStringLiteral, NumberLit, Radix,
    ScriptBody, StringLit, Variable,
};
pub(crate) use self::tokens::{line_or_shebang_content, script_body_value};

/// Declares one wrapper over one node kind: a view (`!Send`) that casts
/// on the kind, derives its equality and hash positionally through the
/// cursor, and offers `syntax()` as the escape to the tree.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident => $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

/// Declares one enum over node kinds — a grammar class — casting to the
/// first alternative whose kind matches.
macro_rules! ast_enum {
    ($(#[$meta:meta])* $name:ident { $( $(#[$variant_meta:meta])* $variant:ident($inner:ty), )+ }) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant($inner), )+
        }

        impl AstNode for $name {
            type Language = Asp;

            fn can_cast(kind: SyntaxKind) -> bool {
                $( <$inner>::can_cast(kind) )||+
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                $(
                    if <$inner>::can_cast(node.kind()) {
                        return <$inner>::cast(node).map(Self::$variant);
                    }
                )+
                None
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $( Self::$variant(inner) => inner.syntax(), )+
                }
            }
        }
    };
}

pub(crate) use ast_node;

// ---- the helpers every accessor is written with -----------------------

/// The first child that casts to `N`.
pub(crate) fn child<N: AstNode<Language = Asp>>(node: &SyntaxNode) -> Option<N> {
    support::child(node)
}

/// The children that cast to `N`, in order.
pub(crate) fn children<N: AstNode<Language = Asp>>(node: &SyntaxNode) -> AstChildren<N> {
    support::children(node)
}

/// The first child token of `kind`.
pub(crate) fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    support::token(node, kind)
}

/// The child tokens whose kind is among `kinds`, in order.
pub(crate) fn tokens(
    node: &SyntaxNode,
    kinds: &'static [SyntaxKind],
) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(move |t| kinds.contains(&t.kind()))
}

/// The leading `not` tokens of a node — the negation run every signed
/// element holds inside it (docs/design/syntax.md §8.1).
pub(crate) fn negation_of(node: &SyntaxNode) -> Negation {
    let mut count = 0u32;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_NOT => count += 1,
            SyntaxElement::Token(token) if token.kind().is_trivia() => {}
            _ => break,
        }
    }
    match count {
        0 => Negation::None,
        1 => Negation::Default,
        _ => Negation::DoubleDefault,
    }
}

// ---- the roots --------------------------------------------------------

ast_node! {
    /// Grammar §5.11's `program`: the program entry's root.
    Program => PROGRAM
}

impl Program {
    /// The statements, in order.
    pub fn statements(&self) -> AstChildren<Statement> {
        children(&self.0)
    }
}

ast_node! {
    /// The statement entry's root: leading trivia, the statement when the
    /// input held one, trailing trivia, and an `ERROR` node when input
    /// remained (docs/design/syntax.md §6.1).
    StatementFragment => STATEMENT_FRAGMENT
}

impl StatementFragment {
    /// None when the input held no statement.
    pub fn statement(&self) -> Option<Statement> {
        child(&self.0)
    }
}

ast_node! {
    /// The term and term-value entries' root, of the same shape as the
    /// statement fragment's (docs/design/syntax.md §6.1).
    TermFragment => TERM_FRAGMENT
}

impl TermFragment {
    /// None when the input held no term; `Parse::entry()` says which
    /// restriction the term was read under.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }
}

// ---- the enums, one per grammar class ---------------------------------

ast_enum! {
    /// The forms a program position holds (grammar §5.11, §6.1).
    Statement {
        /// A rule (grammar §5.7).
        Rule(Rule),
        /// A weak constraint.
        WeakConstraint(WeakConstraint),
        /// `#minimize` or `#maximize`.
        Optimize(OptimizeStatement),
        /// `#show`.
        Show(ShowStatement),
        /// `#project`.
        Project(ProjectStatement),
        /// `#defined`.
        Defined(DefinedStatement),
        /// `#edge`.
        Edge(EdgeStatement),
        /// `#heuristic`.
        Heuristic(HeuristicStatement),
        /// `#external`.
        External(ExternalStatement),
        /// `#const`.
        Const(ConstStatement),
        /// `#script`.
        Script(ScriptStatement),
        /// `#include`.
        Include(IncludeStatement),
        /// `#program`: a part (spec §7.1); `Program` is the root.
        ProgramPart(ProgramStatement),
        /// `#theory`.
        TheoryDefinition(TheoryDefinition),
        /// Grammar §6.1's query — outside the grammar's `statement`
        /// class, inside this enum because the enum is the class of forms
        /// a program position holds, and the query holds the last one
        /// under the ASP-Core-2 dialect.
        Query(Query),
    }
}

ast_enum! {
    /// Grammar §5.5's `head`.
    Head {
        /// A literal.
        Literal(Literal),
        /// A disjunction.
        Disjunction(Disjunction),
        /// An aggregate.
        Aggregate(Aggregate),
        /// A theory atom.
        TheoryAtom(TheoryAtom),
    }
}

ast_enum! {
    /// Grammar §5.6's `body-element`.
    BodyElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
        /// An aggregate, signed inside its node.
        Aggregate(Aggregate),
        /// A theory atom, signed inside its node.
        TheoryAtom(TheoryAtom),
    }
}

impl BodyElement {
    /// The element's default-negation prefix (grammar §5.6): the
    /// literal's own for `Literal`; the conditional literal's literal's
    /// for `ConditionalLiteral`; the aggregate's or theory atom's own for
    /// the other two — every variant delegates to its node, whose leading
    /// `not` tokens are inside it. Total; O(leading tokens).
    pub fn negation(&self) -> Negation {
        match self {
            BodyElement::Literal(literal) => literal.negation(),
            BodyElement::ConditionalLiteral(conditional) => conditional
                .literal()
                .map_or(Negation::None, |literal| literal.negation()),
            BodyElement::Aggregate(Aggregate::Function(aggregate)) => aggregate.negation(),
            BodyElement::Aggregate(Aggregate::Set(aggregate)) => aggregate.negation(),
            BodyElement::TheoryAtom(atom) => atom.negation(),
        }
    }
}

ast_enum! {
    /// Grammar §5.3's `aggregate-body`: the function form or the set form.
    Aggregate {
        /// `#count { … }` and its kin.
        Function(FunctionAggregate),
        /// `{ … }`.
        Set(SetAggregate),
    }
}

ast_enum! {
    /// A function aggregate's element: body-position (terms with an
    /// optional condition) or head-position (terms, a literal, an
    /// optional condition) — the parser knows the position and builds
    /// the kind (grammar §5.3).
    AggregateElement {
        /// In a body.
        Body(BodyAggregateElement),
        /// In a head.
        Head(HeadAggregateElement),
    }
}

ast_enum! {
    /// A set aggregate's element: a literal or a conditional literal
    /// (grammar §5.3).
    SetElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
    }
}

ast_enum! {
    /// A disjunction's element: a literal or a conditioned literal
    /// (grammar §5.5).
    DisjunctionElement {
        /// A literal.
        Literal(Literal),
        /// A conditional literal.
        ConditionalLiteral(ConditionalLiteral),
    }
}

ast_enum! {
    /// Grammar §5.1's `term`.
    Term {
        /// One precedence level's chain.
        Binary(BinaryTerm),
        /// A run of prefix operators and its operand.
        Unary(UnaryTerm),
        /// `( … )`.
        Pool(Pool),
        /// `f(…)`.
        Function(FunctionTerm),
        /// `@f` or `@f(…)`.
        External(ExternalTerm),
        /// `| … |`.
        Abs(AbsTerm),
        /// A constant.
        Constant(ConstantTerm),
        /// A variable, or the anonymous variable.
        Variable(VariableTerm),
        /// A macro splice.
        Splice(SpliceTerm),
    }
}

ast_enum! {
    /// Grammar §5.8's `theory-term`.
    TheoryTerm {
        /// `{ … }`.
        Set(TheorySet),
        /// `[ … ]`.
        List(TheoryList),
        /// `( … )`.
        Tuple(TheoryTuple),
        /// `f(…)`.
        Function(TheoryFunction),
        /// A constant.
        Constant(ConstantTerm),
        /// A variable.
        Variable(VariableTerm),
        /// A macro splice.
        Splice(SpliceTerm),
    }
}

ast_enum! {
    /// Grammar §5.9's `theory-def-item`: a pure alternation, so no node
    /// of its own.
    TheoryDefItem {
        /// A term definition.
        Term(TermDefinition),
        /// An atom definition.
        Atom(AtomDefinition),
    }
}

/// A literal's inner form (grammar §5.2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum LiteralInner {
    /// `#true`.
    True(SyntaxToken),
    /// `#false`.
    False(SyntaxToken),
    /// An atom.
    Atom(Atom),
    /// A comparison.
    Comparison(Comparison),
}

/// A constant term's constant (grammar §5.1).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Constant {
    /// An identifier.
    Symbol(Ident),
    /// A numeral.
    Number(NumberLit),
    /// A string.
    String(StringLit),
    /// `#inf`.
    Infimum(SyntaxToken),
    /// `#sup`.
    Supremum(SyntaxToken),
}

/// One item of a theory opterm's flat sequence (grammar §5.8): an
/// operator token — `THEORY_OP` or `not` — or a theory term.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TheoryOpTermItem {
    /// An operator.
    Op(SyntaxToken),
    /// A term.
    Term(TheoryTerm),
}

// ---- the traits -------------------------------------------------------

/// Every statement may be documented (grammar §5.11): the leading
/// `DOC_COMMENT` tokens inside the statement's node are its
/// documentation.
pub trait HasDocs: AstNode<Language = Asp> {
    /// The leading DOC_COMMENT tokens, in order — the statement's
    /// documentation. Empty when undocumented. Total; O(leading trivia).
    fn doc_lines(&self) -> impl Iterator<Item = DocLine> {
        self.syntax()
            .children_with_tokens()
            .take_while(|element| match element {
                SyntaxElement::Token(token) => {
                    token.kind().is_trivia() || token.kind() == SyntaxKind::DOC_COMMENT
                }
                SyntaxElement::Node(_) => false,
            })
            .filter_map(SyntaxElement::into_token)
            .filter(|token| role(token) == TokenRole::Documentation)
            .filter_map(DocLine::cast)
    }

    /// The covering range of the documentation, if any. Total.
    fn docs_range(&self) -> Option<TextRange> {
        let mut lines = self.doc_lines();
        let first = lines.next()?;
        let last = lines.last().unwrap_or_else(|| first.clone());
        Some(TextRange::new(
            first.syntax().text_range().start(),
            last.syntax().text_range().end(),
        ))
    }
}

/// The two aggregate forms take guards on either side (grammar §5.3).
pub trait HasGuards: AstNode<Language = Asp> {
    /// The guard before the aggregate body, if any. Total; O(children).
    fn left_guard(&self) -> Option<Guard> {
        guards(self.syntax()).0
    }

    /// The guard after the aggregate body, if any. Total; O(children).
    fn right_guard(&self) -> Option<Guard> {
        guards(self.syntax()).1
    }
}

/// The guards of an aggregate node: the `GUARD` before its `{` and the
/// one after its `}`.
fn guards(node: &SyntaxNode) -> (Option<Guard>, Option<Guard>) {
    let mut left = None;
    let mut right = None;
    let mut inside_or_after = false;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::L_BRACE => {
                inside_or_after = true;
            }
            SyntaxElement::Node(child) => {
                if let Some(guard) = Guard::cast(child) {
                    if inside_or_after {
                        right = Some(guard);
                    } else {
                        left = Some(guard);
                    }
                }
            }
            SyntaxElement::Token(_) => {}
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;

    fn program(text: &str) -> crate::parse::Parse<Program> {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
        parse(&source, Dialect::Clingo)
    }

    fn first_statement(text: &str) -> Statement {
        program(text)
            .tree()
            .statements()
            .next()
            .expect("a statement")
    }

    fn rule(text: &str) -> Rule {
        match first_statement(text) {
            Statement::Rule(rule) => rule,
            other => panic!("not a rule: {other:?}"),
        }
    }

    #[test]
    fn a_rule_exposes_its_head_neck_body_and_dot() {
        let rule = rule("p(X) :- q(X), not r.");
        assert!(matches!(rule.head(), Some(Head::Literal(_))));
        assert_eq!(
            rule.neck_token().map(|t| t.text().to_owned()),
            Some(":-".to_owned())
        );
        let body = rule.body().expect("a body");
        let elements: Vec<Negation> = body.elements().map(|e| e.negation()).collect();
        assert_eq!(elements, [Negation::None, Negation::Default]);
        assert!(rule.dot_token().is_some());
        assert!(rule.docs_range().is_none());
    }

    #[test]
    fn a_fact_has_no_body_and_a_constraint_no_head() {
        assert!(rule("p.").body().is_none());
        assert!(rule(":- q.").head().is_none());
        assert!(
            rule("h :- .")
                .body()
                .is_some_and(|body| body.elements().count() == 0)
        );
    }

    #[test]
    fn literals_atoms_and_comparisons() {
        let rule = rule("-p(1) :- 1 < X <= 3, not not #true.");
        let Some(Head::Literal(head)) = rule.head() else {
            panic!("a literal head")
        };
        let Some(LiteralInner::Atom(atom)) = head.inner() else {
            panic!("an atom")
        };
        assert!(atom.strong_negation_token().is_some());
        assert_eq!(
            atom.name().map(|n| n.text().to_owned()),
            Some("p".to_owned())
        );
        assert_eq!(
            atom.arguments().expect("arguments").alternatives().count(),
            1
        );
        let body = rule.body().expect("body");
        let mut elements = body.elements();
        let Some(BodyElement::Literal(comparison)) = elements.next() else {
            panic!("a literal")
        };
        let Some(LiteralInner::Comparison(comparison)) = comparison.inner() else {
            panic!("a comparison")
        };
        assert!(matches!(comparison.first(), Some(Term::Constant(_))));
        let steps: Vec<Relation> = comparison.steps().map(|(relation, _)| relation).collect();
        assert_eq!(steps, [Relation::Lt, Relation::Le]);
        let Some(BodyElement::Literal(truth)) = elements.next() else {
            panic!("a literal")
        };
        assert_eq!(truth.negation(), Negation::DoubleDefault);
        assert!(matches!(truth.inner(), Some(LiteralInner::True(_))));
    }

    #[test]
    fn a_chain_is_one_node_whose_associativity_the_ast_states() {
        let rule = rule("p(1 + 2 * 3, 2 ** 3 ** 4).");
        let Some(Head::Literal(head)) = rule.head() else {
            panic!()
        };
        let Some(LiteralInner::Atom(atom)) = head.inner() else {
            panic!()
        };
        let tuple = atom
            .arguments()
            .expect("arguments")
            .alternatives()
            .next()
            .expect("a tuple");
        let mut terms = tuple.terms();
        let Some(Term::Binary(additive)) = terms.next() else {
            panic!("a chain")
        };
        assert_eq!(additive.level(), Some(Precedence::Additive));
        assert_eq!(additive.associativity(), Some(Associativity::Left));
        assert_eq!(additive.operands().count(), 2);
        assert_eq!(additive.operators().count(), 1);
        let Some(Term::Binary(power)) = terms.next() else {
            panic!("a chain")
        };
        assert_eq!(power.level(), Some(Precedence::Exponentiation));
        assert_eq!(power.associativity(), Some(Associativity::Right));
        assert_eq!(power.operands().count(), 3);
    }

    #[test]
    fn pools_keep_the_uniform_shape_and_name_the_parenthesized_case() {
        let rule = rule("p((a), (a,), (a;b)).");
        let Some(Head::Literal(head)) = rule.head() else {
            panic!()
        };
        let Some(LiteralInner::Atom(atom)) = head.inner() else {
            panic!()
        };
        let tuple = atom
            .arguments()
            .expect("arguments")
            .alternatives()
            .next()
            .expect("a tuple");
        let pools: Vec<Pool> = tuple
            .terms()
            .filter_map(|t| match t {
                Term::Pool(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(pools.len(), 3);
        assert!(pools[0].parenthesized().is_some());
        assert!(
            pools[1].parenthesized().is_none(),
            "`(a,)` is a one-element tuple"
        );
        assert!(pools[2].parenthesized().is_none());
        assert_eq!(pools[2].tuples().count(), 2);
    }

    #[test]
    fn aggregates_expose_guards_functions_and_elements() {
        let rule = rule("1 { p(X) : q(X) } 1 :- not 2 <= #sum { W,T : t(T,W) } < 3.");
        let Some(Head::Aggregate(Aggregate::Set(set))) = rule.head() else {
            panic!("a set aggregate")
        };
        assert!(set.left_guard().is_some_and(|g| g.relation().is_none()));
        assert!(set.right_guard().is_some());
        assert_eq!(set.elements().count(), 1);
        let Some(BodyElement::Aggregate(Aggregate::Function(sum))) =
            rule.body().expect("body").elements().next()
        else {
            panic!("a function aggregate")
        };
        assert_eq!(sum.negation(), Negation::Default);
        assert_eq!(sum.function(), Some(AggregateFunction::Sum));
        assert_eq!(
            sum.left_guard().and_then(|g| g.relation()),
            Some(Relation::Le)
        );
        assert_eq!(
            sum.right_guard().and_then(|g| g.relation()),
            Some(Relation::Lt)
        );
        let Some(AggregateElement::Body(element)) = sum.elements().next() else {
            panic!("a body element")
        };
        assert_eq!(element.terms().count(), 2);
        assert!(element.condition().is_some());
    }

    #[test]
    fn statements_of_every_family_cast_to_their_variants() {
        let text = "p. :~ q. [1@2, x] #minimize { 1 : p }. #show p/1. #project p. #defined p/1. #edge (a,b). #heuristic a. [1,sign] #external p. [true] #const n = 1. [default] #script (lua) x #end. #include \"f\". #program base. #theory t { }.";
        let kinds: Vec<&'static str> = program(text)
            .tree()
            .statements()
            .map(|statement| match statement {
                Statement::Rule(_) => "rule",
                Statement::WeakConstraint(_) => "weak",
                Statement::Optimize(_) => "optimize",
                Statement::Show(_) => "show",
                Statement::Project(_) => "project",
                Statement::Defined(_) => "defined",
                Statement::Edge(_) => "edge",
                Statement::Heuristic(_) => "heuristic",
                Statement::External(_) => "external",
                Statement::Const(_) => "const",
                Statement::Script(_) => "script",
                Statement::Include(_) => "include",
                Statement::ProgramPart(_) => "program",
                Statement::TheoryDefinition(_) => "theory",
                Statement::Query(_) => "query",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "rule",
                "weak",
                "optimize",
                "show",
                "project",
                "defined",
                "edge",
                "heuristic",
                "external",
                "const",
                "script",
                "include",
                "program",
                "theory"
            ]
        );
    }

    #[test]
    fn the_annotations_meanings_live_on_the_statements() {
        let Statement::WeakConstraint(weak) = first_statement(":~ q. [1@2, x, y]") else {
            panic!()
        };
        assert!(weak.weight().is_some());
        assert!(weak.priority().is_some());
        assert_eq!(weak.tuple().count(), 2);
        let Statement::Heuristic(heuristic) = first_statement("#heuristic a. [3, sign]") else {
            panic!()
        };
        assert!(heuristic.weight().is_some());
        assert!(heuristic.priority().is_none());
        assert!(heuristic.modifier().is_some());
        let Statement::External(external) = first_statement("#external p. [false]") else {
            panic!()
        };
        assert!(external.value().is_some());
        let Statement::Const(constant) = first_statement("#const n = 1. [override]") else {
            panic!()
        };
        assert_eq!(constant.policy(), Some(ConstPolicy::Override));
        let Statement::Const(constant) = first_statement("#const n = 1.") else {
            panic!()
        };
        assert_eq!(constant.policy(), None);
        assert!(constant.annotation().is_none());
    }

    #[test]
    fn docs_are_the_statements_and_the_token_wrappers_read_values() {
        let statement = first_statement("%! one \n%! two\np(\"a\\nb\", 0x1F, X, _).");
        let Statement::Rule(rule) = statement else {
            panic!()
        };
        let lines: Vec<String> = rule
            .doc_lines()
            .map(|line| line.content().to_owned())
            .collect();
        assert_eq!(lines, [" one ", " two"]);
        assert!(rule.docs_range().is_some());
        let Some(Head::Literal(head)) = rule.head() else {
            panic!()
        };
        let Some(LiteralInner::Atom(atom)) = head.inner() else {
            panic!()
        };
        let tuple = atom
            .arguments()
            .expect("arguments")
            .alternatives()
            .next()
            .expect("a tuple");
        let mut terms = tuple.terms();
        let Some(Term::Constant(string)) = terms.next() else {
            panic!()
        };
        let Some(Constant::String(string)) = string.constant() else {
            panic!()
        };
        assert_eq!(
            string.value(Dialect::Clingo).expect("a valid literal"),
            "a\nb"
        );
        assert_eq!(
            string.value(Dialect::AspCore2).expect("a valid literal"),
            "a\\nb"
        );
        let Some(Term::Constant(number)) = terms.next() else {
            panic!()
        };
        let Some(Constant::Number(number)) = number.constant() else {
            panic!()
        };
        assert_eq!(number.radix(), Radix::Hexadecimal);
        assert_eq!(number.digits(), "1F");
        let Some(Term::Variable(variable)) = terms.next() else {
            panic!()
        };
        assert!(!variable.variable().expect("a variable").is_anonymous());
        let Some(Term::Variable(anonymous)) = terms.next() else {
            panic!()
        };
        assert!(anonymous.variable().expect("a variable").is_anonymous());
    }

    #[test]
    fn the_parse_level_string_door_uses_the_parses_dialect() {
        let source = Source::new(SourceId::new(0), "p(\"a\\nb\").".to_owned()).expect("admits");
        let parse = parse(&source, Dialect::AspCore2);
        let Some(Statement::Rule(rule)) = parse.tree().statements().next() else {
            panic!()
        };
        let Some(Head::Literal(head)) = rule.head() else {
            panic!()
        };
        let Some(LiteralInner::Atom(atom)) = head.inner() else {
            panic!()
        };
        let tuple = atom
            .arguments()
            .expect("arguments")
            .alternatives()
            .next()
            .expect("a tuple");
        let Some(Term::Constant(string)) = tuple.terms().next() else {
            panic!()
        };
        let Some(Constant::String(string)) = string.constant() else {
            panic!()
        };
        assert_eq!(parse.string_value(&string).expect("valid"), "a\\nb");
    }

    #[test]
    fn comments_and_script_bodies_read_their_content() {
        let parse = program("p. % trailing  \n#script (lua) x = 1   #end.");
        let root = parse.syntax();
        let comment = root
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find_map(Comment::cast)
            .expect("a comment");
        assert_eq!(comment.form(), CommentForm::Line);
        assert_eq!(comment.content(), "% trailing");
        let Some(Statement::Script(script)) = parse.tree().statements().nth(1) else {
            panic!()
        };
        assert_eq!(
            script.language().map(|l| l.text().to_owned()),
            Some("lua".to_owned())
        );
        let body = script.body().expect("a body");
        assert_eq!(body.text(), " x = 1   ");
        assert_eq!(body.value(), " x = 1");
        assert!(script.end_token().is_some());
    }

    #[test]
    fn theory_atoms_and_definitions() {
        let rule = rule(":- not &sum(1) { x, -y : p ; {a} } <= 3.");
        let Some(BodyElement::TheoryAtom(atom)) = rule.body().expect("body").elements().next()
        else {
            panic!()
        };
        assert_eq!(atom.negation(), Negation::Default);
        assert_eq!(
            atom.name().map(|n| n.text().to_owned()),
            Some("sum".to_owned())
        );
        assert!(atom.arguments().is_some());
        let elements = atom.elements().expect("elements");
        assert_eq!(elements.elements().count(), 2);
        let first = elements.elements().next().expect("an element");
        assert_eq!(first.opterms().count(), 2);
        assert!(first.condition().is_some());
        let guard = atom.guard().expect("a guard");
        assert_eq!(
            guard.operator_token().map(|t| t.text().to_owned()),
            Some("<=".to_owned())
        );
        assert!(guard.opterm().is_some());
        let Statement::TheoryDefinition(definition) = first_statement(
            "#theory t { x { - : 1, unary; + : 0, binary, left }; &a/0 : x, {<=}, x, any }.",
        ) else {
            panic!()
        };
        assert_eq!(definition.items().count(), 2);
        let Some(TheoryDefItem::Term(term_definition)) = definition.items().next() else {
            panic!()
        };
        let ops: Vec<Option<Associativity>> = term_definition
            .op_definitions()
            .map(|op| op.associativity())
            .collect();
        assert_eq!(ops, [None, Some(Associativity::Left)]);
    }

    #[test]
    fn the_fragment_roots_answer_their_construct_or_none() {
        let source = Source::new(SourceId::new(0), "  ".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(
            crate::parse::parse_statement(&lexer, crate::parse::NestingLimit::DEFAULT)
                .tree()
                .statement()
                .is_none()
        );
        assert!(
            crate::parse::parse_term(&lexer, crate::parse::NestingLimit::DEFAULT)
                .tree()
                .term()
                .is_none()
        );
        let source = Source::new(SourceId::new(0), "f(1) + 2".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(
            crate::parse::parse_term(&lexer, crate::parse::NestingLimit::DEFAULT)
                .tree()
                .term(),
            Some(Term::Binary(_))
        ));
        let source = Source::new(SourceId::new(0), "p :- q.".to_owned()).expect("admits");
        let lexer = crate::lexer::Lexer::new(&source, Dialect::Clingo);
        assert!(matches!(
            crate::parse::parse_statement(&lexer, crate::parse::NestingLimit::DEFAULT)
                .tree()
                .statement(),
            Some(Statement::Rule(_))
        ));
    }

    #[test]
    fn an_atom_definition_reads_its_structural_slots_not_a_count_of_idents() {
        fn atom(text: &str) -> AtomDefinition {
            let Statement::TheoryDefinition(definition) = first_statement(text) else {
                panic!("a #theory definition")
            };
            match definition.items().next() {
                Some(TheoryDefItem::Atom(atom)) => atom,
                other => panic!("an atom-definition, got {other:?}"),
            }
        }
        fn text_of(ident: Option<Ident>) -> Option<String> {
            ident.map(|i| i.text().to_owned())
        }

        // Well-formed with a guard: every slot reads its own token.
        let a = atom("#theory t { &a/0 : ty, {<=}, gt, head }.");
        assert_eq!(text_of(a.name()), Some("a".to_owned()));
        assert_eq!(text_of(a.type_name()), Some("ty".to_owned()));
        assert_eq!(text_of(a.guard_type_name()), Some("gt".to_owned()));
        assert_eq!(text_of(a.occurrence()), Some("head".to_owned()));

        // No guard: the guard's term type is absent; the rest hold.
        let a = atom("#theory t { &a/0 : ty, body }.");
        assert_eq!(text_of(a.type_name()), Some("ty".to_owned()));
        assert!(a.guard_type_name().is_none());
        assert_eq!(text_of(a.occurrence()), Some("body".to_owned()));

        // A dropped term type is absent — not the occurrence miscounted
        // into its slot; the occurrence still reads from the last comma.
        let a = atom("#theory t { &a/0 : , any }.");
        assert!(
            a.type_name().is_none(),
            "a dropped term type is absent, not the occurrence"
        );
        assert_eq!(text_of(a.occurrence()), Some("any".to_owned()));

        // A dropped occurrence is absent — not the term type `.last()` lands on.
        let a = atom("#theory t { &a/0 : ty, }.");
        assert_eq!(text_of(a.type_name()), Some("ty".to_owned()));
        assert!(
            a.occurrence().is_none(),
            "a dropped occurrence is absent, not the term type"
        );

        // A dropped term type with the guard present: the guard's term
        // type still reads from after the brace — a count of idents (only
        // three) would report None for it and read it as the term type.
        let a = atom("#theory t { &a/0 : , {<=}, gt, head }.");
        assert!(a.type_name().is_none());
        assert_eq!(text_of(a.guard_type_name()), Some("gt".to_owned()));
        assert_eq!(text_of(a.occurrence()), Some("head".to_owned()));
    }
}
