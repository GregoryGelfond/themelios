//! The tree's vocabulary and its rowan realization (docs/design/syntax.md
//! §4.1, §5.2–§5.4): the kind roster and its bracket-pair table, the
//! language marker, the cursor aliases, the coordinate seam between
//! rowan's `TextSize`/`TextRange` and base's `ByteOffset`/`Span`, and the
//! role of a token.

use std::fmt;

use themelios_base::source::{SliceRefusal, Source};
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

/// The closer that matches `open` — the bracket-pair table of the nesting
/// brackets: `L_PAREN` → `R_PAREN`, `L_BRACKET` → `R_BRACKET`, `L_BRACE` →
/// `R_BRACE`; `None` for every other kind, a closer among them. The
/// absolute-value delimiter is not in the table: `|…|` is one `PIPE` kind
/// on both sides, and a `PIPE` is also a disjunction's separator, so
/// whether a `PIPE` opens, closes, or separates is contextual — a fact of
/// where the token stands (attachment reads it off the parent,
/// docs/design/syntax.md §9.2), never of its kind — and an entry here would
/// invite a consumer to take a disjunction's `|` for a bracket. Total, O(1).
pub const fn closer_of(open: SyntaxKind) -> Option<SyntaxKind> {
    match open {
        SyntaxKind::L_PAREN => Some(SyntaxKind::R_PAREN),
        SyntaxKind::L_BRACKET => Some(SyntaxKind::R_BRACKET),
        SyntaxKind::L_BRACE => Some(SyntaxKind::R_BRACE),
        _ => None,
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

/// The text of `source` that `range` spans — `source.slice(span_of(range))`
/// in one call, so a node's, token's, or element's `text_range()` reads its
/// original substring straight off the source. Refuses (`SliceRefusal`)
/// only a range past the end of `source` or off a character boundary —
/// never a range drawn from a tree parsed over this same `source`; O(1)
/// beyond base's own char-boundary check, introducing no walk (base §3.2).
pub fn source_text(source: &Source, range: TextRange) -> Result<&str, SliceRefusal> {
    source.slice(span_of(range))
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

/// The role of `token` where it stands — `role_of`, the one definition,
/// fed the two positional facts read off the token's parent. Total;
/// O(preceding siblings of the token). `roles_of` reads a whole node's
/// roles in one pass over its children.
pub fn role(token: &SyntaxToken) -> TokenRole {
    // Only a DOC_COMMENT's role depends on position; every other kind is
    // O(1) by kind, and the two facts go unread.
    if token.kind() != SyntaxKind::DOC_COMMENT {
        return role_of(token.kind(), false, false);
    }
    let Some(parent) = token.parent() else {
        return role_of(token.kind(), false, false);
    };
    if !parent.kind().is_statement() {
        // Not a statement's child: no doc comment here is documentation,
        // whatever stands before it, so there is no prefix to scan.
        return role_of(token.kind(), false, false);
    }
    // Leading: every element before `token` keeps the prefix — a forward
    // scan of the preceding siblings.
    let leading = parent
        .children_with_tokens()
        .take_while(|element| element.as_token() != Some(token))
        .all(|element| keeps_leading(&element));
    role_of(token.kind(), true, leading)
}

/// The role of a token of `kind` standing where `is_statement` says its
/// parent is a statement and `leading` says every element before it is a
/// trivia-kind token or a `DOC_COMMENT`. The single definition of docs
/// position (docs/design/syntax.md §5.4), read forward — by `role` and
/// `roles_of` here, and by the crate's other one-pass walks over a node's
/// children, which carry the two facts along rather than re-read them.
pub(crate) fn role_of(kind: SyntaxKind, is_statement: bool, leading: bool) -> TokenRole {
    match kind {
        SyntaxKind::DOC_COMMENT if is_statement && leading => TokenRole::Documentation,
        SyntaxKind::DOC_COMMENT => TokenRole::Trivia,
        kind if kind.is_trivia() => TokenRole::Trivia,
        _ => TokenRole::Significant,
    }
}

/// Whether `element` keeps a node's leading trivia/doc prefix intact: a
/// trivia-kind token or a `DOC_COMMENT`. A significant token or any child
/// node ends the prefix.
pub(crate) fn keeps_leading(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Token(token) => {
            token.kind().is_trivia() || token.kind() == SyntaxKind::DOC_COMMENT
        }
        NodeOrToken::Node(_) => false,
    }
}

/// The roles of `node`'s token children, in order, computed in one
/// forward pass — so a consumer reads a node's roles without the
/// per-token scan of the preceding siblings `role` makes. Nodes carry no
/// role (they are not yielded) but end the leading prefix. Total; O(node's
/// children). Reached as `tree::roles_of`.
pub fn roles_of(node: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, TokenRole)> + '_ {
    let is_statement = node.kind().is_statement();
    let mut leading = true;
    node.children_with_tokens().filter_map(move |element| {
        let yielded = match &element {
            NodeOrToken::Token(token) => {
                Some((token.clone(), role_of(token.kind(), is_statement, leading)))
            }
            NodeOrToken::Node(_) => None,
        };
        if !keeps_leading(&element) {
            leading = false;
        }
        yielded
    })
}

/// The tree's shape as one line — `(KIND child …)` for nodes, the text
/// for tokens that are not trivia — for the parser's own tests, which
/// read shapes rather than dumps.
#[cfg(test)]
pub(crate) fn sexpr(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push('(');
                out.push_str(&node.kind().to_string());
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                if !token.kind().is_trivia() {
                    out.push(' ');
                    out.push_str(token.text());
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => out.push(')'),
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out
}

/// The trees docs position turns on, shared by the test modules that hold
/// a reading of roles equal to another — `role` to the backward reading
/// here, and the one-pass walks of `attach` and `ast` to the per-token
/// reading — so every such law is held over one corpus.
#[cfg(test)]
pub(crate) mod role_shapes {
    use rowan::{GreenNodeBuilder, Language};
    use themelios_base::source::{Source, SourceId};

    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;

    /// The source of `text`, which every test text admits.
    pub(crate) fn admitted(text: &str) -> Source {
        Source::new(SourceId::new(7), text.to_owned()).expect("test text admits")
    }

    /// A tree built by hand: `PROGRAM > RULE > [DOC_COMMENT, WHITESPACE,
    /// IDENT, DOT]`, then a stray `DOC_COMMENT` after the rule.
    pub(crate) fn documented_fact() -> SyntaxNode {
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

    /// A tree built by hand around one rule whose children hold every
    /// docs-position shape at once: a three-line doc block with a plain
    /// comment inside it, a `DOC_COMMENT` after the head, a `DOC_COMMENT`
    /// leading a nested `BODY`, and a `DOC_COMMENT` after that child node.
    pub(crate) fn doc_block_rule() -> SyntaxNode {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! one");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! two");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::LINE_COMMENT), "% plain");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOC_COMMENT), "%! three");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! after the head",
        );
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::NECK), ":-");
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.start_node(Asp::kind_to_raw(SyntaxKind::BODY));
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! leading a body",
        );
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "q");
        builder.finish_node();
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! after a child node",
        );
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    /// Texts whose trees hold every shape docs position turns on: doc
    /// blocks and multi-line `%!` runs, with plain comments and blank lines
    /// inside them; a shebang before the docs; a doc line no statement
    /// follows; a doc line after a statement's significant token, after a
    /// child node, and inside nested nodes; a doc run before each statement
    /// family; empty bodies and empty statements; recovery; the marker's
    /// exactness; and the empty program.
    pub(crate) const ROLE_SHAPES: &[&str] = &[
        "%! doc\np.\n",
        "%! one\n%! two\n%! three\np(X) :- q(X).\n",
        "%! one\n% plain\n\n%! two\n%* block *%\n%! three\np.\n",
        "#! shebang\n%! d\np.\n",
        "%! d\n",
        "%! one\n%! two\n%! three\n",
        "p.\n%! x\n",
        "p. %! trailing\nq.\n",
        "p :- %! x\nq.\n",
        ":- %! x\nq.\n",
        "#show %! x\np/1.\n",
        "p(1) %! x\n.\n",
        "p(%! a\n X, %! b\n Y) :- q(X; %! c\n Y).\n",
        "&a { %! x\nx }.\n",
        "%! q\np(1)?\n",
        "%! d\n#show p/1.\n",
        "%! d\n#const n = 3.\n",
        "%! d\n#program base.\n",
        "%! d\n#theory t { }.\n",
        "%! d\n:~ p. [1@1]\n",
        "%! d\n#minimize { 1 : p }.\n",
        "%! d\n#external p.\n",
        "%! d\n#include \"f.lp\".\n",
        "%! d\n#script (python) x #end.\n",
        "%! d\n#edge (a, b) : p.\n",
        "%! d\n#heuristic p. [1, sign]\n",
        "%! d\n#project p/1.\n",
        "%! d\n#defined p/1.\n",
        "%! d\np :- .\n",
        "%! d\n:- .\n",
        ".\n",
        "%! d\n.\n",
        "%! d\n#foo.\np.\n",
        "%! d\n) p.\n",
        "%!x\np.\n% !y\nq.\n",
        "",
    ];

    /// The trees the equivalences are held over: every shape under both
    /// dialects, and the two hand-built trees.
    pub(crate) fn role_corpus() -> Vec<SyntaxNode> {
        let mut trees = vec![documented_fact(), doc_block_rule()];
        for text in ROLE_SHAPES {
            for dialect in [Dialect::Clingo, Dialect::AspCore2] {
                trees.push(parse(&admitted(text), dialect).syntax());
            }
        }
        trees
    }
}

#[cfg(test)]
mod tests {
    use rowan::{GreenNodeBuilder, Language};
    use themelios_base::source::SliceRefusal;
    use themelios_base::span::{ByteOffset, Span};

    use super::role_shapes::{admitted, doc_block_rule, documented_fact, role_corpus};
    use super::*;
    use crate::dialect::Dialect;
    use crate::parse::parse;

    fn tokens(root: &SyntaxNode) -> Vec<SyntaxToken> {
        root.descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .collect()
    }

    /// The backward reading of docs position — a walk over the preceding
    /// siblings from the token — kept here as the oracle the forward
    /// reading `role` makes is held equal to, token for token.
    fn role_backward(token: &SyntaxToken) -> TokenRole {
        match token.kind() {
            SyntaxKind::DOC_COMMENT if in_docs_position_backward(token) => TokenRole::Documentation,
            SyntaxKind::DOC_COMMENT => TokenRole::Trivia,
            kind if kind.is_trivia() => TokenRole::Trivia,
            _ => TokenRole::Significant,
        }
    }

    /// A leading child of a statement node with only trivia and doc-comment
    /// tokens before it, read backward from the token.
    fn in_docs_position_backward(token: &SyntaxToken) -> bool {
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
    fn closer_of_pairs_each_opener_with_its_closer() {
        assert_eq!(closer_of(SyntaxKind::L_PAREN), Some(SyntaxKind::R_PAREN));
        assert_eq!(
            closer_of(SyntaxKind::L_BRACKET),
            Some(SyntaxKind::R_BRACKET)
        );
        assert_eq!(closer_of(SyntaxKind::L_BRACE), Some(SyntaxKind::R_BRACE));
    }

    #[test]
    fn only_the_three_openers_have_a_closer() {
        // A closer is not an opener, nor is any non-bracket kind — and the
        // sweep holds it over the whole roster, not two samples.
        assert_eq!(closer_of(SyntaxKind::R_PAREN), None);
        assert_eq!(closer_of(SyntaxKind::IDENT), None);
        let openers = [
            SyntaxKind::L_PAREN,
            SyntaxKind::L_BRACKET,
            SyntaxKind::L_BRACE,
        ];
        for kind in SyntaxKind::ALL {
            assert_eq!(closer_of(*kind).is_some(), openers.contains(kind), "{kind}");
        }
    }

    #[test]
    fn closer_of_excludes_the_pipe() {
        // `|` closes an absolute value and separates a disjunction under
        // one kind, so at the kind level it is no opener: the documented
        // exclusion.
        assert_eq!(closer_of(SyntaxKind::PIPE), None);
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
    fn source_text_reads_the_substring_a_range_spans() {
        let source = admitted("p(1, 2).");
        let root = parse(&source, Dialect::Clingo).syntax();
        let arguments = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::ARGUMENTS)
            .expect("the atom's arguments");
        assert_eq!(source_text(&source, arguments.text_range()), Ok("(1, 2)"));
        let second_number = tokens(&root)
            .into_iter()
            .filter(|token| token.kind() == SyntaxKind::NUMBER)
            .nth(1)
            .expect("the second number");
        assert_eq!(source_text(&source, second_number.text_range()), Ok("2"));
    }

    #[test]
    fn source_text_refuses_a_range_past_the_end() {
        let source = admitted("p(1, 2).");
        let past_the_end = TextRange::new(TextSize::new(0), TextSize::new(9));
        assert_eq!(
            source_text(&source, past_the_end),
            Err(SliceRefusal::OutOfBounds {
                end: ByteOffset::new(9),
                max: ByteOffset::new(8),
            })
        );
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

    #[test]
    fn role_of_decides_by_kind_statement_and_leading() {
        // Only a DOC_COMMENT reads the two positional facts, and it is
        // documentation under both together; every other kind answers by
        // kind alone.
        for (is_statement, leading) in [(true, true), (true, false), (false, true), (false, false)]
        {
            let of_a_doc_comment = if is_statement && leading {
                TokenRole::Documentation
            } else {
                TokenRole::Trivia
            };
            assert_eq!(
                role_of(SyntaxKind::DOC_COMMENT, is_statement, leading),
                of_a_doc_comment
            );
            for trivia in [
                SyntaxKind::WHITESPACE,
                SyntaxKind::LINE_COMMENT,
                SyntaxKind::BLOCK_COMMENT,
                SyntaxKind::SHEBANG_COMMENT,
            ] {
                assert_eq!(role_of(trivia, is_statement, leading), TokenRole::Trivia);
            }
            for significant in [
                SyntaxKind::IDENT,
                SyntaxKind::DOT,
                SyntaxKind::KW_NOT,
                SyntaxKind::ERROR,
            ] {
                assert_eq!(
                    role_of(significant, is_statement, leading),
                    TokenRole::Significant
                );
            }
        }
    }

    #[test]
    fn roles_of_reads_a_doc_block_in_one_pass() {
        use TokenRole::{Documentation, Significant, Trivia};
        let root = doc_block_rule();
        let rule = root.first_child().expect("the rule");
        let read: Vec<(String, TokenRole)> = roles_of(&rule)
            .map(|(token, role)| (token.text().to_owned(), role))
            .collect();
        let expected: Vec<(String, TokenRole)> = [
            ("%! one", Documentation),
            ("\n", Trivia),
            ("%! two", Documentation),
            ("\n", Trivia),
            ("% plain", Trivia),
            ("\n", Trivia),
            ("%! three", Documentation),
            ("\n", Trivia),
            ("p", Significant),
            (" ", Trivia),
            ("%! after the head", Trivia),
            ("\n", Trivia),
            (":-", Significant),
            (" ", Trivia),
            (" ", Trivia),
            ("%! after a child node", Trivia),
            ("\n", Trivia),
            (".", Significant),
        ]
        .into_iter()
        .map(|(text, role)| (text.to_owned(), role))
        .collect();
        assert_eq!(read, expected);
    }

    #[test]
    fn roles_of_reads_a_doc_leading_a_body_as_trivia() {
        // A `BODY` is no statement, so the doc line leading it is trivia
        // even with nothing before it.
        let root = doc_block_rule();
        let body = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::BODY)
            .expect("the body");
        let read: Vec<TokenRole> = roles_of(&body).map(|(_, role)| role).collect();
        assert_eq!(
            read,
            [TokenRole::Trivia, TokenRole::Trivia, TokenRole::Significant]
        );
    }

    #[test]
    fn roles_of_reads_a_doc_after_a_child_node_as_trivia() {
        // A child node ends the prefix by itself — no significant token
        // need precede the doc line.
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(Asp::kind_to_raw(SyntaxKind::PROGRAM));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::RULE));
        builder.start_node(Asp::kind_to_raw(SyntaxKind::ATOM));
        builder.token(Asp::kind_to_raw(SyntaxKind::IDENT), "p");
        builder.finish_node();
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), " ");
        builder.token(
            Asp::kind_to_raw(SyntaxKind::DOC_COMMENT),
            "%! after the head",
        );
        builder.token(Asp::kind_to_raw(SyntaxKind::WHITESPACE), "\n");
        builder.token(Asp::kind_to_raw(SyntaxKind::DOT), ".");
        builder.finish_node();
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());
        let rule = root.first_child().expect("the rule");
        let read: Vec<TokenRole> = roles_of(&rule).map(|(_, role)| role).collect();
        assert_eq!(
            read,
            [
                TokenRole::Trivia,
                TokenRole::Trivia,
                TokenRole::Trivia,
                TokenRole::Significant
            ]
        );
    }

    #[test]
    fn role_agrees_with_the_backward_reading() {
        // The law, and a witness that the corpus exhibits each outcome the
        // definition can reach for a DOC_COMMENT — so the agreement is not
        // vacuous: documentation; trivia after a significant token or a
        // child node of a statement; trivia under a node that is no
        // statement.
        let mut documented = 0usize;
        let mut after_significant = 0usize;
        let mut after_a_node = 0usize;
        let mut outside_a_statement = 0usize;
        for root in role_corpus() {
            for token in tokens(&root) {
                assert_eq!(
                    role(&token),
                    role_backward(&token),
                    "{:?} {:?}",
                    token.kind(),
                    token.text()
                );
                if token.kind() != SyntaxKind::DOC_COMMENT {
                    continue;
                }
                let parent = token.parent().expect("a token has a parent");
                let before_it: Vec<SyntaxElement> = parent
                    .children_with_tokens()
                    .take_while(|element| element.as_token() != Some(&token))
                    .collect();
                match role(&token) {
                    TokenRole::Documentation => documented += 1,
                    _ if !parent.kind().is_statement() => outside_a_statement += 1,
                    _ if before_it.iter().any(|element| element.as_node().is_some()) => {
                        after_a_node += 1;
                    }
                    _ => after_significant += 1,
                }
            }
        }
        assert!(documented > 0 && after_significant > 0);
        assert!(after_a_node > 0 && outside_a_statement > 0);
    }

    #[test]
    fn roles_of_agrees_with_role_on_every_node() {
        for root in role_corpus() {
            for node in root.descendants() {
                let expected: Vec<(SyntaxToken, TokenRole)> = node
                    .children_with_tokens()
                    .filter_map(SyntaxElement::into_token)
                    .map(|token| {
                        let read = role(&token);
                        (token, read)
                    })
                    .collect();
                let read: Vec<(SyntaxToken, TokenRole)> = roles_of(&node).collect();
                assert_eq!(read, expected, "{}", node.kind());
            }
        }
    }
}
