//! The tree's vocabulary and its rowan realization (docs/design/syntax.md
//! §4.1, §5.2–§5.4): the kind roster, the language marker, the cursor
//! aliases, the coordinate seam between rowan's `TextSize`/`TextRange`
//! and base's `ByteOffset`/`Span`, and the role of a token.

use std::fmt;

use themelios_base::span::{ByteOffset, Span};

pub use rowan::ast::{AstChildren, AstNode, AstPtr};
pub use rowan::{
    Direction, GreenNode, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset, WalkEvent,
};

/// Declares the roster: one enum, tokens first and nodes after, each in
/// the grammar of record's order (docs/design/syntax.md Appendix A),
/// with `ALL` in declaration order so a raw kind maps back by index.
macro_rules! syntax_kinds {
    (
        tokens { $( $(#[$token_meta:meta])* $token:ident, )* }
        nodes { $( $(#[$node_meta:meta])* $node:ident, )* }
    ) => {
        /// Every token and node kind of the tree. Tokens first, then
        /// nodes; within each, the grammar of record's order (Appendix A
        /// of docs/design/syntax.md is the complete roster with the
        /// production each kind realizes). `ERROR` is one kind naming
        /// both the lexical error token and the recovery node — the tree
        /// says which it is where it stands. `Debug` and `Display` are
        /// the SCREAMING_SNAKE name, the spelling dumps and goldens use.
        // The variants are the roster's own SCREAMING_SNAKE names — the
        // rowan idiom, and the spelling the goldens read — so the
        // camel-case convention is set aside here by name.
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $( $(#[$token_meta])* $token, )*
            $( $(#[$node_meta])* $node, )*
        }

        impl SyntaxKind {
            /// Every kind, in declaration order: `ALL[k as usize] == k`.
            pub const ALL: &'static [SyntaxKind] = &[
                $( SyntaxKind::$token, )*
                $( SyntaxKind::$node, )*
            ];
        }
    };
}

syntax_kinds! {
    tokens {
        /// Grammar §4.1 `WHITESPACE`; one token per run.
        WHITESPACE,
        /// Grammar §4.1 `LINE-COMMENT`.
        LINE_COMMENT,
        /// Grammar §4.1 `BLOCK-COMMENT`, nesting per dialect (grammar §6.3).
        BLOCK_COMMENT,
        /// Grammar §4.1 `SHEBANG-COMMENT`.
        SHEBANG_COMMENT,
        /// Grammar §4.1 `DOC-COMMENT` — significant in docs position,
        /// trivia elsewhere; `role` answers which (syntax.md §5.4).
        DOC_COMMENT,
        /// Grammar §4.2 `IDENTIFIER`.
        IDENT,
        /// Grammar §4.2 `VARIABLE`.
        VARIABLE,
        /// Grammar §4.2 `ANONYMOUS`, the lone `_`.
        ANONYMOUS,
        /// Grammar §4.3 `NUMBER`, all four radices; the text is preserved.
        NUMBER,
        /// Grammar §4.4 `STRING` under the dialect's rule (grammar §6.2).
        STRING,
        /// `#const` (grammar §4.5).
        KW_CONST,
        /// `#count`.
        KW_COUNT,
        /// `#defined`.
        KW_DEFINED,
        /// `#edge`.
        KW_EDGE,
        /// `#external`.
        KW_EXTERNAL,
        /// `#false`.
        KW_FALSE,
        /// `#heuristic`.
        KW_HEURISTIC,
        /// `#include`.
        KW_INCLUDE,
        /// `#inf` and `#infimum` — synonyms share the kind and keep their text.
        KW_INF,
        /// `#max`.
        KW_MAX,
        /// `#maximize` and `#maximise`.
        KW_MAXIMIZE,
        /// `#min`.
        KW_MIN,
        /// `#minimize` and `#minimise`.
        KW_MINIMIZE,
        /// `#program`.
        KW_PROGRAM,
        /// `#project`.
        KW_PROJECT,
        /// `#script`.
        KW_SCRIPT,
        /// `#show`.
        KW_SHOW,
        /// `#sum`.
        KW_SUM,
        /// `#sum+`.
        KW_SUM_PLUS,
        /// `#sup` and `#supremum`.
        KW_SUP,
        /// `#theory`.
        KW_THEORY,
        /// `#true`.
        KW_TRUE,
        /// `not` — the one reserved word; also the theory operator
        /// spelled `not` (grammar §4.5, §4.7).
        KW_NOT,
        /// `#end`, the script terminator only (grammar §4.8).
        KW_END,
        /// `.`
        DOT,
        /// `..`
        DOTDOT,
        /// `,`
        COMMA,
        /// `;`
        SEMICOLON,
        /// `:`
        COLON,
        /// `:-`
        NECK,
        /// `:~`
        WEAK_NECK,
        /// `|`
        PIPE,
        /// `(`
        L_PAREN,
        /// `)`
        R_PAREN,
        /// `[`
        L_BRACKET,
        /// `]`
        R_BRACKET,
        /// `{`
        L_BRACE,
        /// `}`
        R_BRACE,
        /// `+`
        PLUS,
        /// `-`
        MINUS,
        /// `*`
        STAR,
        /// `**`
        STAR_STAR,
        /// `/`
        SLASH,
        /// `\`
        BACKSLASH,
        /// `^`
        CARET,
        /// `&`
        AMPERSAND,
        /// `~`
        TILDE,
        /// `?`
        QUESTION,
        /// `@`
        AT,
        /// `=` and `==` — synonyms share the kind (grammar §4.6).
        EQ,
        /// `!=` and `<>`.
        NEQ,
        /// `<`
        LT,
        /// `<=`
        LE,
        /// `>`
        GT,
        /// `>=`
        GE,
        /// Grammar §4.7 `THEORY-OP`, under theory mode.
        THEORY_OP,
        /// Grammar §4.8 `SCRIPT-BODY`, under script mode.
        SCRIPT_BODY,
        /// The macro dialect's `splice` marker and operand (grammar §9);
        /// never from text.
        SPLICE,
        /// A lexical error token (syntax.md §4.5), or the recovery node
        /// holding skipped or refused input byte for byte (syntax.md
        /// §6.6, §6.7).
        ERROR,
        /// End of input: returned by a source, never in a tree.
        EOF,
    }
    nodes {
        /// Grammar §5.11 `program`; the program entry's root.
        PROGRAM,
        /// The statement entry's root (syntax.md §6.1).
        STATEMENT_FRAGMENT,
        /// The term and term-value entries' root (syntax.md §6.1).
        TERM_FRAGMENT,
        /// Grammar §5.7 `rule`, all five forms; a constraint has no head child.
        RULE,
        /// Grammar §5.7 `weak-constraint`.
        WEAK_CONSTRAINT,
        /// Grammar §5.7 `optimize-statement`; the keyword token says which.
        OPTIMIZE_STATEMENT,
        /// Grammar §5.7 `optimize-element`.
        OPTIMIZE_ELEMENT,
        /// Grammar §5.9 `show-statement`, all four forms; children say which.
        SHOW_STATEMENT,
        /// Grammar §5.9 `signature`.
        SIGNATURE,
        /// Grammar §5.9 `project-statement`.
        PROJECT_STATEMENT,
        /// Grammar §5.9 `defined-statement`.
        DEFINED_STATEMENT,
        /// Grammar §5.9 `edge-statement`.
        EDGE_STATEMENT,
        /// One `term "," term` pair of grammar §5.9 `edges`.
        EDGE,
        /// Grammar §5.9 `heuristic-statement`.
        HEURISTIC_STATEMENT,
        /// Grammar §5.9 `external-statement`.
        EXTERNAL_STATEMENT,
        /// Grammar §5.9 `const-statement`; its term under the constant
        /// restriction (syntax.md §6.2).
        CONST_STATEMENT,
        /// Grammar §5.9 `script-statement`.
        SCRIPT_STATEMENT,
        /// Grammar §5.9 `include-statement`.
        INCLUDE_STATEMENT,
        /// Grammar §5.9 `program-statement`.
        PROGRAM_STATEMENT,
        /// `"(" [ id-list ] ")"` of a program statement (grammar §5.9).
        PARAMETERS,
        /// Grammar §5.9 `theory-definition`.
        THEORY_DEFINITION,
        /// Grammar §5.9 `term-definition`.
        TERM_DEFINITION,
        /// Grammar §5.9 `op-definition`.
        OP_DEFINITION,
        /// Grammar §5.9 `atom-definition`.
        ATOM_DEFINITION,
        /// Grammar §6.1 `query` (ASP-Core-2 dialect).
        QUERY,
        /// The bracketed annotation after the dot of the four families
        /// (grammar §5.11).
        ANNOTATION,
        /// Grammar §5.6 `body-list`; also the empty body of `h :- .` and `: .`.
        BODY,
        /// Grammar §5.2 `literal`: negation tokens and one of `#true`,
        /// `#false`, `ATOM`, `COMPARISON`.
        LITERAL,
        /// Grammar §5.2 `atom`.
        ATOM,
        /// Grammar §5.2 `comparison`, the whole chain.
        COMPARISON,
        /// Grammar §5.4 `conditional-literal`, and every
        /// `literal ":" [condition]` shape: set-aggregate elements,
        /// disjunction elements with a condition.
        CONDITIONAL_LITERAL,
        /// Grammar §5.3 `condition`; present and empty when the colon is.
        CONDITION,
        /// Grammar §5.5 `disjunction`; separators as tokens.
        DISJUNCTION,
        /// Grammar §5.3 `function-aggregate` with its guards as `GUARD`
        /// children, and in body position its leading negation tokens.
        FUNCTION_AGGREGATE,
        /// Grammar §5.3 `set-aggregate` with its guards, and in body
        /// position its leading negation tokens.
        SET_AGGREGATE,
        /// Grammar §5.3 `lguard` / `rguard`.
        GUARD,
        /// Grammar §5.3 `fn-element` in body position.
        BODY_AGGREGATE_ELEMENT,
        /// Grammar §5.3 `fn-element` in head position.
        HEAD_AGGREGATE_ELEMENT,
        /// Grammar §5.8 `theory-atom`, and in body position its leading
        /// negation tokens.
        THEORY_ATOM,
        /// `"{" [ theory-elements ] "}"` (grammar §5.8).
        THEORY_ELEMENTS,
        /// Grammar §5.8 `theory-element`.
        THEORY_ELEMENT,
        /// Grammar §5.8 `theory-opterm`, flat.
        THEORY_OPTERM,
        /// `theory-op theory-opterm` after the elements (grammar §5.8).
        THEORY_GUARD,
        /// `"{" [ theory-opterms ] "}"` (grammar §5.8).
        THEORY_SET,
        /// `"[" [ theory-opterms ] "]"` (grammar §5.8).
        THEORY_LIST,
        /// The parenthesized theory-term forms (grammar §5.8).
        THEORY_TUPLE,
        /// `IDENTIFIER "(" [ theory-opterms ] ")"` (grammar §5.8).
        THEORY_FUNCTION,
        /// One precedence level's maximal chain of `term BINOP term`,
        /// flat: operands interleaved with operator tokens (syntax.md §6.2).
        BINARY_TERM,
        /// A maximal run of `UNOP` and its one operand, flat (syntax.md §6.2).
        UNARY_TERM,
        /// `"(" pool ")"` (grammar §5.1).
        POOL,
        /// Grammar §5.1 `tuple`, and each `[ terms ]` alternative of `arguments`.
        TUPLE,
        /// `"(" arguments ")"` of a function, an atom, or an external call.
        ARGUMENTS,
        /// `IDENTIFIER "(" arguments ")"` (grammar §5.1).
        FUNCTION_TERM,
        /// `"@" IDENTIFIER [ "(" arguments ")" ]` (grammar §5.1).
        EXTERNAL_TERM,
        /// `"|" abs-arguments "|"` (grammar §5.1).
        ABS_TERM,
        /// `IDENTIFIER | NUMBER | STRING | "#inf" | "#sup"` as a term.
        CONSTANT_TERM,
        /// `VARIABLE | ANONYMOUS` as a term.
        VARIABLE_TERM,
        /// A splice in term or theory-term position (grammar §9).
        SPLICE_TERM,
    }
}

impl SyntaxKind {
    /// The kinds that are trivia wherever they stand: `WHITESPACE`,
    /// `LINE_COMMENT`, `BLOCK_COMMENT`, `SHEBANG_COMMENT`. `DOC_COMMENT`
    /// is not among them — its status is positional, and `role` is the
    /// predicate that answers it for a token. Total, O(1).
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE
                | SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::SHEBANG_COMMENT
        )
    }

    /// The comment forms: `LINE_COMMENT`, `BLOCK_COMMENT`,
    /// `SHEBANG_COMMENT`, `DOC_COMMENT`. Total, O(1).
    pub const fn is_comment(self) -> bool {
        matches!(
            self,
            SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::SHEBANG_COMMENT
                | SyntaxKind::DOC_COMMENT
        )
    }

    /// The keyword tokens: the `#`-keywords of grammar §4.5, `not`, and
    /// the script terminator `#end`. Total, O(1).
    pub const fn is_keyword(self) -> bool {
        (self as u16) >= (SyntaxKind::KW_CONST as u16)
            && (self as u16) <= (SyntaxKind::KW_END as u16)
    }

    /// A kind a token may carry: everything declared before the first
    /// node kind, `ERROR` and `EOF` included. Total, O(1).
    pub const fn is_token(self) -> bool {
        (self as u16) <= (SyntaxKind::EOF as u16)
    }

    /// A kind a node may carry: `PROGRAM` and every kind after it, and
    /// `ERROR`. Total, O(1).
    pub const fn is_node(self) -> bool {
        (self as u16) >= (SyntaxKind::PROGRAM as u16) || matches!(self, SyntaxKind::ERROR)
    }

    /// The statement kinds — grammar §5.11's `statement` alternatives
    /// and the query — the kinds whose leading `DOC_COMMENT` run is
    /// documentation (docs/design/syntax.md §5.4).
    pub(crate) const fn is_statement(self) -> bool {
        matches!(
            self,
            SyntaxKind::RULE
                | SyntaxKind::WEAK_CONSTRAINT
                | SyntaxKind::OPTIMIZE_STATEMENT
                | SyntaxKind::SHOW_STATEMENT
                | SyntaxKind::PROJECT_STATEMENT
                | SyntaxKind::DEFINED_STATEMENT
                | SyntaxKind::EDGE_STATEMENT
                | SyntaxKind::HEURISTIC_STATEMENT
                | SyntaxKind::EXTERNAL_STATEMENT
                | SyntaxKind::CONST_STATEMENT
                | SyntaxKind::SCRIPT_STATEMENT
                | SyntaxKind::INCLUDE_STATEMENT
                | SyntaxKind::PROGRAM_STATEMENT
                | SyntaxKind::THEORY_DEFINITION
                | SyntaxKind::QUERY
        )
    }
}

impl fmt::Display for SyntaxKind {
    /// The SCREAMING_SNAKE name, as `Debug` renders it — stable, being
    /// what dumps and goldens read (docs/design/syntax.md §12.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The rowan language marker: maps `SyntaxKind` to and from rowan's raw
/// kind. Uninhabited — a type-level tag, never a value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Asp {}

impl rowan::Language for Asp {
    type Kind = SyntaxKind;

    /// The kind a raw kind names. A raw kind this crate never produced —
    /// a green tree of another language wrapped under `Asp`, outside
    /// every contract here — names nothing of the language and reads as
    /// `ERROR`, the kind for what was not understood; the door is total.
    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::ALL
            .get(usize::from(raw.0))
            .copied()
            .unwrap_or(SyntaxKind::ERROR)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// A node cursor over the tree — a view, `!Send` (docs/design/syntax.md §5.1).
pub type SyntaxNode = rowan::SyntaxNode<Asp>;
/// A token cursor over the tree — a view.
pub type SyntaxToken = rowan::SyntaxToken<Asp>;
/// A node or a token cursor.
pub type SyntaxElement = rowan::SyntaxElement<Asp>;
/// A node's child nodes, in order.
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<Asp>;
/// A node's child nodes and tokens, in order.
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<Asp>;
/// An iterative preorder over nodes.
pub type Preorder = rowan::api::Preorder<Asp>;
/// An iterative preorder over nodes and tokens.
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<Asp>;
/// Positional identity by kind and range, resolvable against a root.
pub type SyntaxNodePtr = rowan::ast::SyntaxNodePtr<Asp>;

/// The span of a rowan range: total, since a `TextRange`'s start never
/// exceeds its end.
pub fn span_of(range: TextRange) -> Span {
    Span::new(offset_of(range.start()), offset_of(range.end()))
        .expect("a TextRange's start never exceeds its end")
}

/// The rowan range of a span: total.
pub fn range_of(span: Span) -> TextRange {
    TextRange::new(size_of(span.start()), size_of(span.end()))
}

/// The base offset of a rowan size: total.
pub fn offset_of(size: TextSize) -> ByteOffset {
    ByteOffset::new(u32::from(size))
}

/// The rowan size of a base offset: total.
pub fn size_of(offset: ByteOffset) -> TextSize {
    TextSize::new(offset.get())
}

/// What a token is, where it stands (docs/design/syntax.md §5.4).
/// `Documentation`: a `DOC_COMMENT` in docs position — a leading child
/// of a statement node with only trivia and other `DOC_COMMENT` tokens
/// before it. `Trivia`: whitespace, the plain comment forms wherever
/// they stand, and a `DOC_COMMENT` anywhere else. `Significant`: every
/// other token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenRole {
    /// A statement's documentation.
    Documentation,
    /// Whitespace, or a comment that is not documentation.
    Trivia,
    /// Every other token.
    Significant,
}

/// The role of `token` where it stands. Total; O(preceding siblings of
/// the token).
pub fn role(token: &SyntaxToken) -> TokenRole {
    match token.kind() {
        SyntaxKind::DOC_COMMENT if in_docs_position(token) => TokenRole::Documentation,
        SyntaxKind::DOC_COMMENT => TokenRole::Trivia,
        kind if kind.is_trivia() => TokenRole::Trivia,
        _ => TokenRole::Significant,
    }
}

/// A leading child of a statement node with only trivia and doc-comment
/// tokens before it.
fn in_docs_position(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if !parent.kind().is_statement() {
        return false;
    }
    let mut earlier = token.prev_sibling_or_token();
    while let Some(element) = earlier {
        match &element {
            NodeOrToken::Node(_) => return false,
            NodeOrToken::Token(before) => {
                if !(before.kind().is_trivia() || before.kind() == SyntaxKind::DOC_COMMENT) {
                    return false;
                }
            }
        }
        earlier = element.prev_sibling_or_token();
    }
    true
}

#[cfg(test)]
mod tests {
    use rowan::{GreenNodeBuilder, Language};
    use themelios_base::span::{ByteOffset, Span};

    use super::*;

    /// A tree built by hand: `PROGRAM > RULE > [DOC_COMMENT, WHITESPACE,
    /// IDENT, DOT]`, then a stray `DOC_COMMENT` after the rule.
    fn documented_fact() -> SyntaxNode {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! a fact");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! stray");
        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    fn tokens(root: &SyntaxNode) -> Vec<SyntaxToken> {
        root.descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .collect()
    }

    #[test]
    fn the_roster_is_declared_in_order_tokens_first() {
        for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(
                *kind as usize, index,
                "{kind:?} sits at its declaration index"
            );
        }
        let first_node = SyntaxKind::ALL
            .iter()
            .position(|kind| kind.is_node() && *kind != SyntaxKind::ERROR)
            .expect("nodes exist");
        assert_eq!(SyntaxKind::ALL[first_node], SyntaxKind::PROGRAM);
        assert!(
            SyntaxKind::ALL[..first_node]
                .iter()
                .all(|kind| kind.is_token())
        );
        assert!(
            SyntaxKind::ALL[first_node..]
                .iter()
                .all(|kind| kind.is_node())
        );
        assert_eq!(SyntaxKind::ALL[first_node - 1], SyntaxKind::EOF);
    }

    #[test]
    fn error_is_one_kind_for_the_token_and_the_node() {
        assert!(SyntaxKind::ERROR.is_token());
        assert!(SyntaxKind::ERROR.is_node());
        assert!(!SyntaxKind::EOF.is_node());
        assert!(!SyntaxKind::PROGRAM.is_token());
    }

    #[test]
    fn the_predicates_answer_by_kind() {
        assert!(SyntaxKind::WHITESPACE.is_trivia());
        assert!(SyntaxKind::LINE_COMMENT.is_trivia());
        assert!(SyntaxKind::BLOCK_COMMENT.is_trivia());
        assert!(SyntaxKind::SHEBANG_COMMENT.is_trivia());
        assert!(!SyntaxKind::DOC_COMMENT.is_trivia());
        assert!(SyntaxKind::DOC_COMMENT.is_comment());
        assert!(!SyntaxKind::WHITESPACE.is_comment());
        assert!(SyntaxKind::KW_CONST.is_keyword());
        assert!(SyntaxKind::KW_NOT.is_keyword());
        assert!(SyntaxKind::KW_END.is_keyword());
        assert!(!SyntaxKind::IDENT.is_keyword());
        assert!(SyntaxKind::RULE.is_statement());
        assert!(SyntaxKind::QUERY.is_statement());
        assert!(!SyntaxKind::BODY.is_statement());
    }

    #[test]
    fn the_language_round_trips_every_kind() {
        for kind in SyntaxKind::ALL {
            assert_eq!(Asp::kind_from_raw(Asp::kind_to_raw(*kind)), *kind);
        }
        let beyond = rowan::SyntaxKind(SyntaxKind::ALL.len() as u16);
        assert_eq!(Asp::kind_from_raw(beyond), SyntaxKind::ERROR);
    }

    #[test]
    fn display_is_the_screaming_snake_name() {
        assert_eq!(SyntaxKind::L_PAREN.to_string(), "L_PAREN");
        assert_eq!(
            SyntaxKind::THEORY_OPTERM.to_string(),
            format!("{:?}", SyntaxKind::THEORY_OPTERM)
        );
    }

    #[test]
    fn the_coordinate_seam_converts_both_ways() {
        let span = Span::new(ByteOffset::new(3), ByteOffset::new(9)).expect("ordered");
        let range = range_of(span);
        assert_eq!(u32::from(range.start()), 3);
        assert_eq!(u32::from(range.end()), 9);
        assert_eq!(span_of(range), span);
        assert_eq!(offset_of(size_of(ByteOffset::new(42))), ByteOffset::new(42));
        assert_eq!(size_of(offset_of(TextSize::new(7))), TextSize::new(7));
    }

    #[test]
    fn a_leading_doc_comment_of_a_statement_is_documentation() {
        let root = documented_fact();
        let tokens = tokens(&root);
        assert_eq!(role(&tokens[0]), TokenRole::Documentation);
        assert_eq!(role(&tokens[1]), TokenRole::Trivia);
        assert_eq!(role(&tokens[2]), TokenRole::Significant);
        assert_eq!(role(&tokens[3]), TokenRole::Significant);
    }

    #[test]
    fn a_doc_comment_outside_docs_position_is_trivia() {
        let root = documented_fact();
        let tokens = tokens(&root);
        assert_eq!(tokens[5].kind(), SyntaxKind::DOC_COMMENT);
        assert_eq!(role(&tokens[5]), TokenRole::Trivia);
    }

    #[test]
    fn a_doc_comment_after_a_significant_token_is_trivia() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! inside");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());
        let tokens = tokens(&root);
        assert_eq!(role(&tokens[2]), TokenRole::Trivia);
    }
}
