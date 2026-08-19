//! The wrappers over the roster's node kinds and their accessors
//! (docs/design/syntax.md §8.1–§8.2): one per kind, accessors mirroring
//! the production's slots in the production's order — a single child
//! `Option`, a repetition `AstChildren`, a token slot `Option<SyntaxToken>`
//! named for the token, a valued token a typed wrapper.

use crate::tree::{Asp, AstChildren, AstNode, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::tokens::{AstToken, Ident, NumberLit, ScriptBody, StringLit, Variable};
use super::{
    AggregateElement, BodyElement, Constant, DisjunctionElement, HasDocs, HasGuards, Head,
    LiteralInner, SetElement, Term, TheoryDefItem, TheoryOpTermItem, TheoryTerm, ast_node, child,
    children, negation_of, token, tokens,
};

/// A literal's or a signed element's default-negation prefix (grammar
/// §5.2, §5.6): none, `not`, or `not not`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Negation {
    /// No prefix.
    None,
    /// `not`.
    Default,
    /// `not not`.
    DoubleDefault,
}

/// Grammar §5.2's `relation`; the spelling stays on the token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Relation {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `=` or `==`
    Eq,
    /// `!=` or `<>`
    Neq,
}

impl Relation {
    fn of(kind: SyntaxKind) -> Option<Relation> {
        Some(match kind {
            SyntaxKind::LT => Relation::Lt,
            SyntaxKind::LE => Relation::Le,
            SyntaxKind::GT => Relation::Gt,
            SyntaxKind::GE => Relation::Ge,
            SyntaxKind::EQ => Relation::Eq,
            SyntaxKind::NEQ => Relation::Neq,
            _ => return None,
        })
    }
}

/// Grammar §5.1's precedence levels, loosest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Precedence {
    /// `..`
    Interval,
    /// `^`
    BitXor,
    /// `?`
    BitOr,
    /// `&`
    BitAnd,
    /// `+` `-`
    Additive,
    /// `*` `/` `\`
    Multiplicative,
    /// `**`
    Exponentiation,
}

impl Precedence {
    fn of(kind: SyntaxKind) -> Option<Precedence> {
        Some(match kind {
            SyntaxKind::DOTDOT => Precedence::Interval,
            SyntaxKind::CARET => Precedence::BitXor,
            SyntaxKind::QUESTION => Precedence::BitOr,
            SyntaxKind::AMPERSAND => Precedence::BitAnd,
            SyntaxKind::PLUS | SyntaxKind::MINUS => Precedence::Additive,
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::BACKSLASH => {
                Precedence::Multiplicative
            }
            SyntaxKind::STAR_STAR => Precedence::Exponentiation,
            _ => return None,
        })
    }
}

/// The direction a chain folds in: left at every level but
/// exponentiation, right for `**` (grammar §5.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Associativity {
    /// Left.
    Left,
    /// Right.
    Right,
}

/// Grammar §5.3's aggregate functions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateFunction {
    /// `#count`
    Count,
    /// `#sum`
    Sum,
    /// `#sum+`
    SumPlus,
    /// `#min`
    Min,
    /// `#max`
    Max,
}

/// `#const`'s policy word (grammar §5.9).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ConstPolicy {
    /// `[default]`
    Default,
    /// `[override]`
    Override,
}

const OPERATOR_KINDS: [SyntaxKind; 10] = [
    SyntaxKind::DOTDOT,
    SyntaxKind::CARET,
    SyntaxKind::QUESTION,
    SyntaxKind::AMPERSAND,
    SyntaxKind::PLUS,
    SyntaxKind::MINUS,
    SyntaxKind::STAR,
    SyntaxKind::SLASH,
    SyntaxKind::BACKSLASH,
    SyntaxKind::STAR_STAR,
];
const UNARY_KINDS: [SyntaxKind; 2] = [SyntaxKind::MINUS, SyntaxKind::TILDE];
const RELATION_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::LT,
    SyntaxKind::LE,
    SyntaxKind::GT,
    SyntaxKind::GE,
    SyntaxKind::EQ,
    SyntaxKind::NEQ,
];
const SEPARATOR_KINDS: [SyntaxKind; 3] =
    [SyntaxKind::COMMA, SyntaxKind::SEMICOLON, SyntaxKind::PIPE];
const THEORY_OPERATOR_KINDS: [SyntaxKind; 2] = [SyntaxKind::THEORY_OP, SyntaxKind::KW_NOT];

/// The weight, the priority, and the tuple of a weighted term list —
/// `term [ "@" term ] [ "," terms ]` — read from `node`'s children up
/// to a colon or the closing bracket.
struct Weighted {
    weight: Option<Term>,
    priority: Option<Term>,
    tuple: Vec<Term>,
}

fn weighted(node: &SyntaxNode) -> Weighted {
    let mut weighted = Weighted {
        weight: None,
        priority: None,
        tuple: Vec::new(),
    };
    let mut after_at = false;
    let mut after_comma = false;
    for element in node.children_with_tokens() {
        match element {
            SyntaxElement::Token(token) => match token.kind() {
                SyntaxKind::AT => after_at = true,
                SyntaxKind::COMMA => after_comma = true,
                SyntaxKind::COLON | SyntaxKind::R_BRACKET => break,
                _ => {}
            },
            SyntaxElement::Node(child) => {
                let Some(term) = Term::cast(child) else {
                    continue;
                };
                if after_comma {
                    weighted.tuple.push(term);
                } else if after_at {
                    weighted.priority = Some(term);
                } else if weighted.weight.is_none() {
                    weighted.weight = Some(term);
                }
            }
        }
    }
    weighted
}

// ---- statements ---------------------------------------------------------

ast_node! {
    /// Grammar §5.7's `rule`, all five forms.
    Rule => RULE
}

impl HasDocs for Rule {}

impl Rule {
    /// None for a constraint.
    pub fn head(&self) -> Option<Head> {
        child(&self.0)
    }

    /// The `:-`, when the rule has one.
    pub fn neck_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::NECK)
    }

    /// None for a fact; `Some` of an empty body for `h :- .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.7's `weak-constraint`.
    WeakConstraint => WEAK_CONSTRAINT
}

impl HasDocs for WeakConstraint {}

impl WeakConstraint {
    /// The `:~`.
    pub fn weak_neck_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::WEAK_NECK)
    }

    /// The body; None for `:~ .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The bracketed annotation after the dot.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The weight — reads into the annotation.
    pub fn weight(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).weight)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        self.annotation()
            .and_then(|a| weighted(a.syntax()).priority)
    }

    /// The optional term tuple after the weight and priority, in order.
    pub fn tuple(&self) -> impl Iterator<Item = Term> {
        self.annotation()
            .map(|a| weighted(a.syntax()).tuple)
            .unwrap_or_default()
            .into_iter()
    }
}

ast_node! {
    /// Grammar §5.7's `optimize-statement`; the keyword token says which.
    OptimizeStatement => OPTIMIZE_STATEMENT
}

impl HasDocs for OptimizeStatement {}

impl OptimizeStatement {
    /// `#minimize` or `#maximize`, either spelling.
    pub fn keyword_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::KW_MINIMIZE, SyntaxKind::KW_MAXIMIZE]).next()
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<OptimizeElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.7's `optimize-element`.
    OptimizeElement => OPTIMIZE_ELEMENT
}

impl OptimizeElement {
    /// The weight.
    pub fn weight(&self) -> Option<Term> {
        weighted(&self.0).weight
    }

    /// The `@`.
    pub fn at_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::AT)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        weighted(&self.0).priority
    }

    /// The optional term tuple after the weight and priority, in order.
    pub fn tuple(&self) -> impl Iterator<Item = Term> {
        weighted(&self.0).tuple.into_iter()
    }

    /// The `:` before the condition.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.9's `show-statement`, all four forms; the children say
    /// which.
    ShowStatement => SHOW_STATEMENT
}

impl HasDocs for ShowStatement {}

impl ShowStatement {
    /// The `#show`.
    pub fn show_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::KW_SHOW)
    }

    /// The signature, in the signature form.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The term, in the term forms.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }

    /// The `:` before the body.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body, in the conditioned term form.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `signature`: `[-] IDENTIFIER / NUMBER`.
    Signature => SIGNATURE
}

impl Signature {
    /// The `-`, when the signature is classically negated.
    pub fn classical_negation_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MINUS)
    }

    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `/`.
    pub fn slash_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SLASH)
    }

    /// The arity.
    pub fn arity(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }
}

ast_node! {
    /// Grammar §5.9's `project-statement`: the signature form or the atom
    /// form with its conditional dot.
    ProjectStatement => PROJECT_STATEMENT
}

impl HasDocs for ProjectStatement {}

impl ProjectStatement {
    /// The signature, in the signature form.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The atom, in the atom form.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot; empty for `: .`.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `defined-statement`.
    DefinedStatement => DEFINED_STATEMENT
}

impl HasDocs for DefinedStatement {}

impl DefinedStatement {
    /// The signature.
    pub fn signature(&self) -> Option<Signature> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `edge-statement`.
    EdgeStatement => EDGE_STATEMENT
}

impl HasDocs for EdgeStatement {}

impl EdgeStatement {
    /// The `(`.
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_PAREN)
    }

    /// The edges, in order.
    pub fn edges(&self) -> AstChildren<Edge> {
        children(&self.0)
    }

    /// The `)`.
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_PAREN)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// One `term "," term` pair of grammar §5.9's `edges`.
    Edge => EDGE
}

impl Edge {
    /// The first term.
    pub fn from(&self) -> Option<Term> {
        children::<Term>(&self.0).next()
    }

    /// The `,`.
    pub fn comma_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COMMA)
    }

    /// The second term.
    pub fn to(&self) -> Option<Term> {
        children::<Term>(&self.0).nth(1)
    }
}

ast_node! {
    /// Grammar §5.9's `heuristic-statement`.
    HeuristicStatement => HEURISTIC_STATEMENT
}

impl HasDocs for HeuristicStatement {}

impl HeuristicStatement {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The bracketed annotation, mandatory for this family.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The weight — reads into the annotation.
    pub fn weight(&self) -> Option<Term> {
        self.annotation().and_then(|a| weighted(a.syntax()).weight)
    }

    /// The `@`-priority, if any.
    pub fn priority(&self) -> Option<Term> {
        self.annotation()
            .and_then(|a| weighted(a.syntax()).priority)
    }

    /// The modifier — the term after the comma.
    pub fn modifier(&self) -> Option<Term> {
        self.annotation()
            .and_then(|a| weighted(a.syntax()).tuple.into_iter().next())
    }
}

ast_node! {
    /// Grammar §5.9's `external-statement`.
    ExternalStatement => EXTERNAL_STATEMENT
}

impl HasDocs for ExternalStatement {}

impl ExternalStatement {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `:` of the conditional dot.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The body of the conditional dot.
    pub fn body(&self) -> Option<Body> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The optional bracketed annotation.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// The value inside the annotation, if any.
    pub fn value(&self) -> Option<Term> {
        self.annotation().and_then(|a| child(a.syntax()))
    }
}

ast_node! {
    /// Grammar §5.9's `const-statement`; its term under the constant
    /// restriction.
    ConstStatement => CONST_STATEMENT
}

impl HasDocs for ConstStatement {}

impl ConstStatement {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `=`.
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::EQ)
    }

    /// The value.
    pub fn value(&self) -> Option<Term> {
        child(&self.0)
    }

    /// The dot before the annotation.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }

    /// The optional bracketed annotation.
    pub fn annotation(&self) -> Option<Annotation> {
        child(&self.0)
    }

    /// `[default]` or `[override]` (grammar §5.9), read by spelling.
    pub fn policy(&self) -> Option<ConstPolicy> {
        let annotation = self.annotation()?;
        let word = tokens(annotation.syntax(), &[SyntaxKind::IDENT]).next()?;
        match word.text() {
            "default" => Some(ConstPolicy::Default),
            "override" => Some(ConstPolicy::Override),
            _ => None,
        }
    }
}

ast_node! {
    /// A `#script` statement, parsed because the shared syntax has it
    /// (grammar §4.8, §5.9) and carried as opaque text: this crate never
    /// runs, parses, or privileges an embedded script — themelios's own
    /// extension language is Rust (spec §9.6), and an embedded script is
    /// the clingo-world compatibility path, executed only by a backend
    /// that declares the capability, never by themelios.
    ScriptStatement => SCRIPT_STATEMENT
}

impl HasDocs for ScriptStatement {}

impl ScriptStatement {
    /// The named language, as written — an identifier the grammar does
    /// not restrict; what a backend accepts is admission, above.
    pub fn language(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The `SCRIPT_BODY` token: the raw region, exact span — what a tool
    /// that handles the region hands to that language's own tooling.
    /// None when the region is empty (`#end` directly after the
    /// parenthesis) or missing under recovery; `end_token` tells the two
    /// apart.
    pub fn body(&self) -> Option<ScriptBody> {
        tokens(&self.0, &[SyntaxKind::SCRIPT_BODY]).find_map(ScriptBody::cast)
    }

    /// The `#end`.
    pub fn end_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::KW_END)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `include-statement`.
    IncludeStatement => INCLUDE_STATEMENT
}

impl HasDocs for IncludeStatement {}

impl IncludeStatement {
    /// The path, in the string form.
    pub fn path(&self) -> Option<StringLit> {
        tokens(&self.0, &[SyntaxKind::STRING]).find_map(StringLit::cast)
    }

    /// The library name, in the angle form.
    pub fn library(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `program-statement`.
    ProgramStatement => PROGRAM_STATEMENT
}

impl HasDocs for ProgramStatement {}

impl ProgramStatement {
    /// The part's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The parameter list, if any.
    pub fn parameters(&self) -> Option<Parameters> {
        child(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// `"(" [ id-list ] ")"` of a program statement (grammar §5.9).
    Parameters => PARAMETERS
}

impl Parameters {
    /// The parameter names, in order.
    pub fn names(&self) -> impl Iterator<Item = Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).filter_map(Ident::cast)
    }
}

ast_node! {
    /// Grammar §5.9's `theory-definition`.
    TheoryDefinition => THEORY_DEFINITION
}

impl HasDocs for TheoryDefinition {}

impl TheoryDefinition {
    /// The theory's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The items, term and atom definitions interleaved as written.
    pub fn items(&self) -> AstChildren<TheoryDefItem> {
        children(&self.0)
    }

    /// The terminating dot.
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::DOT)
    }
}

ast_node! {
    /// Grammar §5.9's `term-definition`.
    TermDefinition => TERM_DEFINITION
}

impl TermDefinition {
    /// The term type's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The operator definitions, in order.
    pub fn op_definitions(&self) -> AstChildren<OpDefinition> {
        children(&self.0)
    }
}

ast_node! {
    /// Grammar §5.9's `op-definition`.
    OpDefinition => OP_DEFINITION
}

impl OpDefinition {
    /// The operator token — a `THEORY_OP` or `not`.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS).next()
    }

    /// The priority.
    pub fn priority(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }

    /// The word `unary` or `binary`.
    pub fn arity_token(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The word `left` or `right`, for a binary operator.
    pub fn associativity_token(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT])
            .filter_map(Ident::cast)
            .nth(1)
    }

    /// The associativity read from its word; None for a unary operator
    /// or a missing word.
    pub fn associativity(&self) -> Option<Associativity> {
        match self.associativity_token()?.text() {
            "left" => Some(Associativity::Left),
            "right" => Some(Associativity::Right),
            _ => None,
        }
    }
}

ast_node! {
    /// Grammar §5.9's `atom-definition`.
    AtomDefinition => ATOM_DEFINITION
}

impl AtomDefinition {
    /// The atom's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arity.
    pub fn arity(&self) -> Option<NumberLit> {
        tokens(&self.0, &[SyntaxKind::NUMBER]).find_map(NumberLit::cast)
    }

    /// The term type after the colon.
    pub fn type_name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT])
            .filter_map(Ident::cast)
            .nth(1)
    }

    /// The guard operators between the braces, in order.
    pub fn guard_operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS)
    }

    /// The guard's term type, when the guard part is present.
    pub fn guard_type_name(&self) -> Option<Ident> {
        let words: Vec<Ident> = tokens(&self.0, &[SyntaxKind::IDENT])
            .filter_map(Ident::cast)
            .collect();
        if words.len() >= 4 {
            words.get(2).cloned()
        } else {
            None
        }
    }

    /// The occurrence word: `head`, `body`, `any`, or `directive`.
    pub fn occurrence(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT])
            .filter_map(Ident::cast)
            .last()
    }
}

ast_node! {
    /// Grammar §6.1's `query` (ASP-Core-2 dialect).
    Query => QUERY
}

impl HasDocs for Query {}

impl Query {
    /// The atom.
    pub fn atom(&self) -> Option<Atom> {
        child(&self.0)
    }

    /// The `?`.
    pub fn question_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::QUESTION)
    }
}

ast_node! {
    /// The bracketed annotation after the dot of the four families
    /// (grammar §5.11); its meaning is read by the statement's accessors.
    Annotation => ANNOTATION
}

impl Annotation {
    /// The `[`.
    pub fn l_bracket_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACKET)
    }

    /// The terms inside, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The `]`.
    pub fn r_bracket_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACKET)
    }
}

// ---- rule interiors -----------------------------------------------------

ast_node! {
    /// Grammar §5.6's `body-list`; also the empty body of `h :- .` and `: .`.
    Body => BODY
}

impl Body {
    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<BodyElement> {
        children(&self.0)
    }

    /// The `,` and `;` between elements, in order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::COMMA, SyntaxKind::SEMICOLON])
    }
}

ast_node! {
    /// Grammar §5.2's `literal`: negation tokens and one of `#true`,
    /// `#false`, an atom, a comparison.
    Literal => LITERAL
}

impl Literal {
    /// The default-negation prefix.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The inner form; None under recovery.
    pub fn inner(&self) -> Option<LiteralInner> {
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_TRUE => {
                    return Some(LiteralInner::True(token));
                }
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::KW_FALSE => {
                    return Some(LiteralInner::False(token));
                }
                SyntaxElement::Node(node) => {
                    if let Some(atom) = Atom::cast(node.clone()) {
                        return Some(LiteralInner::Atom(atom));
                    }
                    if let Some(comparison) = Comparison::cast(node) {
                        return Some(LiteralInner::Comparison(comparison));
                    }
                }
                SyntaxElement::Token(_) => {}
            }
        }
        None
    }
}

ast_node! {
    /// Grammar §5.2's `atom`.
    Atom => ATOM
}

impl Atom {
    /// The `-` of classical negation.
    pub fn classical_negation_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MINUS)
    }

    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments, if any.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.2's `comparison`, the whole chain.
    Comparison => COMPARISON
}

impl Comparison {
    /// The first term.
    pub fn first(&self) -> Option<Term> {
        children::<Term>(&self.0).next()
    }

    /// The chain after the first term: each step a relation and its
    /// right term (grammar §5.2's guard sequence); the term is None
    /// under recovery.
    pub fn steps(&self) -> impl Iterator<Item = (Relation, Option<Term>)> {
        let mut steps: Vec<(Relation, Option<Term>)> = Vec::new();
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) => {
                    if let Some(relation) = Relation::of(token.kind()) {
                        steps.push((relation, None));
                    }
                }
                SyntaxElement::Node(node) => {
                    if let (Some(term), Some(last)) = (Term::cast(node), steps.last_mut())
                        && last.1.is_none()
                    {
                        last.1 = Some(term);
                    }
                }
            }
        }
        steps.into_iter()
    }
}

ast_node! {
    /// Grammar §5.4's `conditional-literal`, and every `literal ":" [condition]`
    /// shape.
    ConditionalLiteral => CONDITIONAL_LITERAL
}

impl ConditionalLiteral {
    /// The literal.
    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `condition`; present and empty when the colon is.
    Condition => CONDITION
}

impl Condition {
    /// The literals, in order.
    pub fn literals(&self) -> AstChildren<Literal> {
        children(&self.0)
    }
}

ast_node! {
    /// Grammar §5.5's `disjunction`; separators as tokens.
    Disjunction => DISJUNCTION
}

impl Disjunction {
    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<DisjunctionElement> {
        children(&self.0)
    }

    /// The `;`, `|`, and `,` between elements, in order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &SEPARATOR_KINDS)
    }
}

ast_node! {
    /// Grammar §5.3's `function-aggregate` with its guards as `GUARD`
    /// children, and in body position its leading negation tokens.
    FunctionAggregate => FUNCTION_AGGREGATE
}

impl HasGuards for FunctionAggregate {}

impl FunctionAggregate {
    /// The leading `not` tokens in body position (grammar §5.6);
    /// `Negation::None` in head position, where none can stand.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The function.
    pub fn function(&self) -> Option<AggregateFunction> {
        let keyword = tokens(
            &self.0,
            &[
                SyntaxKind::KW_COUNT,
                SyntaxKind::KW_SUM,
                SyntaxKind::KW_SUM_PLUS,
                SyntaxKind::KW_MIN,
                SyntaxKind::KW_MAX,
            ],
        )
        .next()?;
        Some(match keyword.kind() {
            SyntaxKind::KW_COUNT => AggregateFunction::Count,
            SyntaxKind::KW_SUM => AggregateFunction::Sum,
            SyntaxKind::KW_SUM_PLUS => AggregateFunction::SumPlus,
            SyntaxKind::KW_MIN => AggregateFunction::Min,
            _ => AggregateFunction::Max,
        })
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order — body-shaped or head-shaped, as the
    /// parser built them.
    pub fn elements(&self) -> AstChildren<AggregateElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.3's `set-aggregate` with its guards, and in body
    /// position its leading negation tokens.
    SetAggregate => SET_AGGREGATE
}

impl HasGuards for SetAggregate {}

impl SetAggregate {
    /// The leading `not` tokens in body position; `Negation::None` in
    /// head position.
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// Elements are literals or conditional literals (grammar §5.3).
    pub fn elements(&self) -> impl Iterator<Item = SetElement> {
        children::<SetElement>(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.3's `lguard` / `rguard`.
    Guard => GUARD
}

impl Guard {
    /// None means the grammar's default relation for its side (grammar
    /// §5.3): stated as absence, because that is what the author wrote.
    pub fn relation(&self) -> Option<Relation> {
        tokens(&self.0, &RELATION_KINDS)
            .next()
            .and_then(|t| Relation::of(t.kind()))
    }

    /// The guard's term.
    pub fn term(&self) -> Option<Term> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `fn-element` in body position.
    BodyAggregateElement => BODY_AGGREGATE_ELEMENT
}

impl BodyAggregateElement {
    /// The term tuple, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.3's `fn-element` in head position.
    HeadAggregateElement => HEAD_AGGREGATE_ELEMENT
}

impl HeadAggregateElement {
    /// The term tuple, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The first `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The literal after the first colon.
    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }

    /// The `:` before the condition.
    pub fn second_colon_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &[SyntaxKind::COLON]).nth(1)
    }

    /// The condition, present and empty when its colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

// ---- theory atoms -------------------------------------------------------

ast_node! {
    /// Grammar §5.8's `theory-atom`, and in body position its leading
    /// negation tokens.
    TheoryAtom => THEORY_ATOM
}

impl TheoryAtom {
    /// The negation, in body position only (grammar §5.6).
    pub fn negation(&self) -> Negation {
        negation_of(&self.0)
    }

    /// The name after `&`.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The name's arguments, if any.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }

    /// The elements between the braces, if the atom has them.
    pub fn elements(&self) -> Option<TheoryElements> {
        child(&self.0)
    }

    /// The guard after the elements, if any.
    pub fn guard(&self) -> Option<TheoryGuard> {
        child(&self.0)
    }
}

ast_node! {
    /// `"{" [ theory-elements ] "}"` (grammar §5.8).
    TheoryElements => THEORY_ELEMENTS
}

impl TheoryElements {
    /// The `{`.
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_BRACE)
    }

    /// The elements, in order.
    pub fn elements(&self) -> AstChildren<TheoryElement> {
        children(&self.0)
    }

    /// The `}`.
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_BRACE)
    }
}

ast_node! {
    /// Grammar §5.8's `theory-element`.
    TheoryElement => THEORY_ELEMENT
}

impl TheoryElement {
    /// The opterms before the colon, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }

    /// The `:`.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON)
    }

    /// The condition, present and empty when the colon is.
    pub fn condition(&self) -> Option<Condition> {
        child(&self.0)
    }
}

ast_node! {
    /// Grammar §5.8's `theory-opterm`, flat.
    TheoryOpTerm => THEORY_OPTERM
}

impl TheoryOpTerm {
    /// Operators and terms in the flat sequence grammar §5.8 admits;
    /// regrouping under a `#theory` definition is admission, above.
    pub fn items(&self) -> impl Iterator<Item = TheoryOpTermItem> {
        self.0
            .children_with_tokens()
            .filter_map(|element| match element {
                SyntaxElement::Token(token) if THEORY_OPERATOR_KINDS.contains(&token.kind()) => {
                    Some(TheoryOpTermItem::Op(token))
                }
                SyntaxElement::Token(_) => None,
                SyntaxElement::Node(node) => TheoryTerm::cast(node).map(TheoryOpTermItem::Term),
            })
    }
}

ast_node! {
    /// `theory-op theory-opterm` after the elements (grammar §5.8).
    TheoryGuard => THEORY_GUARD
}

impl TheoryGuard {
    /// The guard's operator.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0, &THEORY_OPERATOR_KINDS).next()
    }

    /// The guard's opterm.
    pub fn opterm(&self) -> Option<TheoryOpTerm> {
        child(&self.0)
    }
}

ast_node! {
    /// `"{" [ theory-opterms ] "}"` (grammar §5.8).
    TheorySet => THEORY_SET
}

impl TheorySet {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

ast_node! {
    /// `"[" [ theory-opterms ] "]"` (grammar §5.8).
    TheoryList => THEORY_LIST
}

impl TheoryList {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

ast_node! {
    /// The parenthesized theory-term forms (grammar §5.8): `()`, `(a)`,
    /// `(a,)`, `(a, b)`.
    TheoryTuple => THEORY_TUPLE
}

impl TheoryTuple {
    /// The opterms, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }

    /// The trailing comma of `(a,)`.
    pub fn trailing_comma_token(&self) -> Option<SyntaxToken> {
        let mut last_comma = None;
        let mut term_after = false;
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::COMMA => {
                    last_comma = Some(token);
                    term_after = false;
                }
                SyntaxElement::Node(_) => term_after = true,
                SyntaxElement::Token(_) => {}
            }
        }
        if term_after { None } else { last_comma }
    }
}

ast_node! {
    /// `IDENTIFIER "(" [ theory-opterms ] ")"` (grammar §5.8).
    TheoryFunction => THEORY_FUNCTION
}

impl TheoryFunction {
    /// The function's name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments, in order.
    pub fn opterms(&self) -> AstChildren<TheoryOpTerm> {
        children(&self.0)
    }
}

// ---- terms --------------------------------------------------------------

ast_node! {
    /// One precedence level's chain, flat (docs/design/syntax.md §6.2):
    /// `1 + 2 - 3` is one node of three operands and two operators; a
    /// tighter level is an operand.
    BinaryTerm => BINARY_TERM
}

impl BinaryTerm {
    /// The operands in source order — at least two when well formed;
    /// under recovery one may be missing.
    pub fn operands(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The operator tokens in source order — one fewer than the operands.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &OPERATOR_KINDS)
    }

    /// The chain's level, read from its first operator (grammar §5.1).
    pub fn level(&self) -> Option<Precedence> {
        self.operators()
            .next()
            .and_then(|t| Precedence::of(t.kind()))
    }

    /// Left at every level but exponentiation, right for `**` — the
    /// grammar's fact, carried here so no consumer re-derives it.
    pub fn associativity(&self) -> Option<Associativity> {
        self.level().map(|level| match level {
            Precedence::Exponentiation => Associativity::Right,
            _ => Associativity::Left,
        })
    }
}

ast_node! {
    /// A run of prefix operators and its one operand, flat: `- - x` is
    /// one node; unary binds tighter than every binary level (grammar §5.1).
    UnaryTerm => UNARY_TERM
}

impl UnaryTerm {
    /// The prefix operators, outermost first.
    pub fn operators(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, &UNARY_KINDS)
    }

    /// The operand.
    pub fn operand(&self) -> Option<Term> {
        child(&self.0)
    }
}

ast_node! {
    /// `( … )`: tuples separated by `;` (grammar §5.1).
    Pool => POOL
}

impl Pool {
    /// The tuples, in order.
    pub fn tuples(&self) -> AstChildren<Tuple> {
        children(&self.0)
    }

    /// `(a)` — exactly one tuple of one term with no trailing comma — is
    /// the term `a` parenthesized (grammar §5.1); this names it.
    pub fn parenthesized(&self) -> Option<Term> {
        let mut tuples = self.tuples();
        let tuple = tuples.next()?;
        if tuples.next().is_some() || tuple.trailing_comma_token().is_some() {
            return None;
        }
        let mut terms = tuple.terms();
        let term = terms.next()?;
        if terms.next().is_some() {
            None
        } else {
            Some(term)
        }
    }
}

ast_node! {
    /// Grammar §5.1's `tuple`, and each `[ terms ]` alternative of
    /// `arguments`.
    Tuple => TUPLE
}

impl Tuple {
    /// The terms, in order.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }

    /// The trailing comma, when the tuple has one.
    pub fn trailing_comma_token(&self) -> Option<SyntaxToken> {
        let mut last_comma = None;
        let mut term_after = false;
        for element in self.0.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::COMMA => {
                    last_comma = Some(token);
                    term_after = false;
                }
                SyntaxElement::Node(_) => term_after = true,
                SyntaxElement::Token(_) => {}
            }
        }
        if term_after { None } else { last_comma }
    }
}

ast_node! {
    /// `"(" arguments ")"` of a function, an atom, or an external call.
    Arguments => ARGUMENTS
}

impl Arguments {
    /// The `(`.
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::L_PAREN)
    }

    /// The pooled alternatives, in order — one for `f(a,b)`, several for
    /// `f(a;b)`, an empty one for `f()`.
    pub fn alternatives(&self) -> AstChildren<Tuple> {
        children(&self.0)
    }

    /// The `)`.
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::R_PAREN)
    }
}

ast_node! {
    /// `IDENTIFIER "(" arguments ")"` (grammar §5.1).
    FunctionTerm => FUNCTION_TERM
}

impl FunctionTerm {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// `@name` and `@name(args)` — the syntax themelios's Rust
    /// `@`-functions answer to (spec §9.6); the `@` and the name are
    /// separate tokens (grammar §5.1).
    ExternalTerm => EXTERNAL_TERM
}

impl ExternalTerm {
    /// The name.
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0, &[SyntaxKind::IDENT]).find_map(Ident::cast)
    }

    /// The arguments; None for the bare `@name`.
    pub fn arguments(&self) -> Option<Arguments> {
        child(&self.0)
    }
}

ast_node! {
    /// `"|" abs-arguments "|"` (grammar §5.1).
    AbsTerm => ABS_TERM
}

impl AbsTerm {
    /// The pooled arguments, in order — one for `|X|`, several for `|X;Y|`.
    pub fn terms(&self) -> AstChildren<Term> {
        children(&self.0)
    }
}

ast_node! {
    /// `IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"` as a term.
    ConstantTerm => CONSTANT_TERM
}

impl ConstantTerm {
    /// The constant.
    pub fn constant(&self) -> Option<Constant> {
        let token = self
            .0
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|t| !t.kind().is_trivia())?;
        Some(match token.kind() {
            SyntaxKind::IDENT => Constant::Symbol(Ident::cast(token)?),
            SyntaxKind::NUMBER => Constant::Number(NumberLit::cast(token)?),
            SyntaxKind::STRING => Constant::String(StringLit::cast(token)?),
            SyntaxKind::KW_INF => Constant::Infimum(token),
            SyntaxKind::KW_SUP => Constant::Supremum(token),
            _ => return None,
        })
    }
}

ast_node! {
    /// `VARIABLE | ANONYMOUS` as a term.
    VariableTerm => VARIABLE_TERM
}

impl VariableTerm {
    /// The variable, or the anonymous variable.
    pub fn variable(&self) -> Option<Variable> {
        tokens(&self.0, &[SyntaxKind::VARIABLE, SyntaxKind::ANONYMOUS]).find_map(Variable::cast)
    }
}

ast_node! {
    /// A splice in term or theory-term position (grammar §9).
    SpliceTerm => SPLICE_TERM
}

impl SpliceTerm {
    /// The splice token.
    pub fn splice_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SPLICE)
    }
}

ast_node! {
    /// Skipped or refused input, byte-preserved (docs/design/syntax.md
    /// §6.6, §6.7).
    Error => ERROR
}
