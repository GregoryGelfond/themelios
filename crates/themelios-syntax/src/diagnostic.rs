//! The tier's typed diagnostics (docs/design/syntax.md §7): a fully
//! typed value — matchable, exhaustive, carrying the expected set as a
//! real type — lowering into base's normal form for rendering and
//! transport. Identities and severities are Appendix B's; message texts
//! are presentation, held by the golden corpus.

use std::collections::BTreeSet;
use std::fmt;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity, ToDiagnostic};
use themelios_base::span::{ByteOffset, Location};

use crate::dialect::Dialect;
use crate::tree::SyntaxKind;

/// One syntax diagnostic: what happened, where, and what would settle
/// it. Located by construction — the primary span is required.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SyntaxError {
    kind: SyntaxErrorKind,
    primary: Location,
    related: BTreeSet<Related>,
}

impl SyntaxError {
    /// A diagnostic of `kind` at `primary`, with no related loci yet.
    pub(crate) fn new(kind: SyntaxErrorKind, primary: Location) -> SyntaxError {
        SyntaxError {
            kind,
            primary,
            related: BTreeSet::new(),
        }
    }

    /// The diagnostic with a related locus added; a locus already
    /// present stays once — set semantics.
    #[must_use]
    pub(crate) fn with_related(mut self, related: Related) -> SyntaxError {
        self.related.insert(related);
        self
    }

    /// What happened, typed. Total, O(1).
    pub fn kind(&self) -> &SyntaxErrorKind {
        &self.kind
    }

    /// The stable identity, derived from the kind (Appendix B). Total, O(1).
    pub fn id(&self) -> DiagnosticId {
        self.kind.id()
    }

    /// The severity, derived from the kind (Appendix B). Total, O(1).
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// The primary location. Total, O(1).
    pub fn primary(&self) -> Location {
        self.primary
    }

    /// The related loci, a set. Total, O(1).
    pub fn related(&self) -> &BTreeSet<Related> {
        &self.related
    }
}

/// A secondary locus, typed: what the location is, so that its text is
/// derived at lowering like every other text on the diagnostic and a
/// wording change is never a parser change. Closed; a locus is admitted
/// here when a golden shows a reader needs it, as a `Hint` is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Related {
    /// What the location is.
    pub locus: RelatedLocus,
    /// Where.
    pub location: Location,
}

/// The kinds of related locus.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RelatedLocus {
    /// "the statement began here"
    StatementBegan,
    /// "to close this `{`" — the opener a missing closer answers to.
    ToClose(SyntaxKind),
    /// "the literal, whole" — the string a bad escape sits in.
    LiteralExtent,
}

/// The closed roster of what can go wrong, each with its typed payload.
/// Declared in the order a parse meets them: lexical, then structural,
/// then the restrictions, then the warnings.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SyntaxErrorKind {
    /// Characters that begin no token, one run (syntax.md §4.5).
    UnexpectedCharacters,
    /// A `#`-word that spells no keyword (grammar §4.5).
    UnknownHashWord,
    /// A string literal the dialect's rule refuses (grammar §4.4, §6.2).
    MalformedString {
        /// What broke the literal.
        defect: StringDefect,
    },
    /// `%*` never closed (grammar §4.1, §6.3).
    UnterminatedBlockComment,
    /// A `#script` region with no `#end` (grammar §4.8).
    UnterminatedScript,
    /// `_` inside a theory expression, where none is admitted (grammar §4.7).
    AnonymousInTheoryExpression,
    /// A token the grammar does not admit here.
    UnexpectedToken {
        /// What the parser would have accepted.
        expected: ExpectedSet,
        /// The kind it met.
        found: SyntaxKind,
        /// A characteristic mistake recognized here, if any.
        hint: Option<Hint>,
    },
    /// The input ended where more was expected.
    UnexpectedEndOfInput {
        /// What the parser would have accepted.
        expected: ExpectedSet,
        /// A characteristic mistake recognized here, if any.
        hint: Option<Hint>,
    },
    /// A bracket that would open a frame past `MAX_NESTING_DEPTH`
    /// (syntax.md §6.6).
    NestingTooDeep {
        /// The bound that was reached.
        depth: u32,
    },
    /// The input is aspif, not a program (grammar §4.9).
    AspifInput,
    /// The token source breached a law the parser can witness (syntax.md §4.3).
    TokenSourceBreach {
        /// Which breach.
        breach: SourceBreach,
    },
    /// A term form the position's restriction forbids (syntax.md §6.2).
    FormNotAllowedHere {
        /// The form written.
        form: RestrictedForm,
        /// The restriction in force.
        context: Restriction,
    },
    /// A doc comment that documents nothing (grammar §4.1, §5.11) — a
    /// warning; the input stays a member.
    MisplacedDocComment {
        /// Why it documents nothing.
        reason: MisplacedDoc,
    },
}

/// What broke a string literal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StringDefect {
    /// A raw line break inside the literal (the clingo rule).
    RawLineBreak,
    /// A backslash before a character the rule does not admit; the
    /// character.
    InvalidEscape(char),
    /// End of input before the closing quote.
    Unterminated,
}

/// The term forms a restriction can forbid (syntax.md §6.2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RestrictedForm {
    /// A variable.
    Variable,
    /// The anonymous variable.
    AnonymousVariable,
    /// A pool.
    Pool,
    /// An interval.
    Interval,
    /// An `@`-call.
    ExternalCall,
    /// An absolute value over a pooled argument.
    PooledAbsoluteValue,
}

/// The restriction contexts (grammar §5.9, §5.10).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Restriction {
    /// `#const`'s constant term.
    ConstantTerm,
    /// The term-value sublanguage.
    TermValue,
}

/// Why a doc comment documents nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MisplacedDoc {
    /// The run is followed by no statement.
    NoStatementFollows,
    /// The line stands inside a statement.
    InsideStatement,
}

/// The two breaches of the token-source laws the parser can witness in
/// one pass, both at an offset it reached by tiling: `Tiling` — an
/// `EOF` before the text's end, or a token running past it, the kind
/// and length saying which; `Refusal` — the door refused where it owed
/// a token. The slice law is trusted and determinism unobservable in
/// one pass, so neither appears here — the checker's
/// `TokenSourceLawViolation` is the wider type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SourceBreach {
    /// Tiling broke.
    Tiling {
        /// Where.
        at: ByteOffset,
        /// The token answered there.
        token: SyntaxKind,
        /// Its length.
        len: u32,
    },
    /// The door refused a position it owed a token.
    Refusal {
        /// Where.
        at: ByteOffset,
    },
}

/// The characteristic mistakes the parser recognizes at an unexpected
/// token — each a shape the grammar or the corpus names, each carrying
/// one help text at lowering. Closed; a hint is admitted here when a
/// golden case shows a reader needs it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Hint {
    /// `f(a,)` — no trailing comma in an argument list (grammar §5.1).
    TrailingCommaInArguments,
    /// A `?` ending the input under the clingo dialect — the ASP-Core-2
    /// query mark (grammar §6.1).
    QueryMarkNeedsAspCore2,
    /// Two numerals adjacent — a leading zero (grammar §4.3).
    LeadingZeroNumeral,
    /// `p(X) : | q(X)` — the empty-conditioned element before `|`
    /// (grammar §5.5); write `;`.
    EmptyConditionBeforePipe,
    /// `#heuristic … .` without its bracket (grammar §5.9).
    HeuristicNeedsAnnotation,
}

/// What the parser would have accepted at a point: tokens by kind,
/// identifiers by spelling where the grammar wants a word, and grammar
/// classes where listing tokens would mislead. A set — order carries no
/// meaning, duplicates are defects, and rendering derives its order
/// (kinds, then words, then classes).
pub type ExpectedSet = BTreeSet<Expected>;

/// One expectation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Expected {
    /// A token by kind.
    Token(SyntaxKind),
    /// An identifier by spelling.
    Word(GrammarWord),
    /// A grammar class.
    Class(SyntaxClass),
}

/// The words the grammar wants by spelling where it has no token for
/// them (grammar §5.9): the ten identifiers matched by spelling in
/// `#const` annotations and `#theory` definitions. Closed, so an
/// expected set is matchable and a golden can enumerate it; `Display`
/// is the spelling.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GrammarWord {
    /// `default`
    Default,
    /// `override`
    Override,
    /// `unary`
    Unary,
    /// `binary`
    Binary,
    /// `left`
    Left,
    /// `right`
    Right,
    /// `head`
    Head,
    /// `body`
    Body,
    /// `any`
    Any,
    /// `directive`
    Directive,
}

impl GrammarWord {
    /// The spelling the grammar matches.
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            GrammarWord::Default => "default",
            GrammarWord::Override => "override",
            GrammarWord::Unary => "unary",
            GrammarWord::Binary => "binary",
            GrammarWord::Left => "left",
            GrammarWord::Right => "right",
            GrammarWord::Head => "head",
            GrammarWord::Body => "body",
            GrammarWord::Any => "any",
            GrammarWord::Directive => "directive",
        }
    }
}

impl fmt::Display for GrammarWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spelling())
    }
}

/// The grammar's classes a consumer or a message names as one thing.
/// Closed; each is a nonterminal or a family of the grammar of record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SyntaxClass {
    /// A statement.
    Statement,
    /// A head.
    Head,
    /// A body element.
    BodyElement,
    /// A literal.
    Literal,
    /// An atom.
    Atom,
    /// A term.
    Term,
    /// A theory term.
    TheoryTerm,
    /// A theory operator.
    TheoryOperator,
    /// An aggregate guard.
    Guard,
    /// A signature `name/arity`.
    Signature,
    /// A condition.
    Condition,
    /// A bracketed annotation.
    Annotation,
    /// End of input.
    EndOfInput,
}

const NAMESPACE: &str = "syntax";

impl SyntaxErrorKind {
    /// The identity (Appendix B).
    fn id(&self) -> DiagnosticId {
        let name = match self {
            SyntaxErrorKind::UnexpectedCharacters => "unexpected-characters",
            SyntaxErrorKind::UnknownHashWord => "unknown-hash-word",
            SyntaxErrorKind::MalformedString { .. } => "malformed-string",
            SyntaxErrorKind::UnterminatedBlockComment => "unterminated-block-comment",
            SyntaxErrorKind::UnterminatedScript => "unterminated-script",
            SyntaxErrorKind::AnonymousInTheoryExpression => "anonymous-in-theory-expression",
            SyntaxErrorKind::UnexpectedToken { .. } => "unexpected-token",
            SyntaxErrorKind::UnexpectedEndOfInput { .. } => "unexpected-end-of-input",
            SyntaxErrorKind::NestingTooDeep { .. } => "nesting-too-deep",
            SyntaxErrorKind::AspifInput => "aspif-input",
            SyntaxErrorKind::TokenSourceBreach { .. } => "token-source-breach",
            SyntaxErrorKind::FormNotAllowedHere { .. } => "form-not-allowed-here",
            SyntaxErrorKind::MisplacedDocComment { .. } => "misplaced-doc-comment",
        };
        DiagnosticId::new(NAMESPACE, name)
    }

    /// The severity (Appendix B): every kind an error but the doc warning.
    fn severity(&self) -> Severity {
        match self {
            SyntaxErrorKind::MisplacedDocComment { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// Whether this kind is one of the incompleteness errors
    /// (docs/design/syntax.md §6.5): end of input where more was
    /// expected, an unterminated block comment, an unterminated script
    /// region, or — under the ASP-Core-2 dialect only, where a string
    /// may span lines — an unterminated string.
    pub(crate) fn is_incompleteness(&self, dialect: Dialect) -> bool {
        match self {
            SyntaxErrorKind::UnexpectedEndOfInput { .. }
            | SyntaxErrorKind::UnterminatedBlockComment
            | SyntaxErrorKind::UnterminatedScript => true,
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::Unterminated,
            } => dialect == Dialect::AspCore2,
            _ => false,
        }
    }

    /// The headline, derived from the kind and its payload.
    fn headline(&self) -> String {
        match self {
            SyntaxErrorKind::UnexpectedCharacters => "unexpected characters".to_owned(),
            SyntaxErrorKind::UnknownHashWord => "unknown `#`-word".to_owned(),
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::RawLineBreak,
            } => "string literal broken by a line break".to_owned(),
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::InvalidEscape('\n'),
            } => "string literal with a backslash at the end of its line".to_owned(),
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::InvalidEscape(c),
            } => {
                format!("invalid escape `\\{}` in string literal", c.escape_debug())
            }
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::Unterminated,
            } => "unterminated string literal".to_owned(),
            SyntaxErrorKind::UnterminatedBlockComment => "unterminated block comment".to_owned(),
            SyntaxErrorKind::UnterminatedScript => "unterminated `#script` region".to_owned(),
            SyntaxErrorKind::AnonymousInTheoryExpression => {
                "anonymous variable inside a theory expression".to_owned()
            }
            SyntaxErrorKind::UnexpectedToken {
                expected, found, ..
            } => {
                format!(
                    "expected {}, found {}",
                    render_expected(expected),
                    describe(*found)
                )
            }
            SyntaxErrorKind::UnexpectedEndOfInput { expected, .. } => {
                format!("expected {}, found end of input", render_expected(expected))
            }
            SyntaxErrorKind::NestingTooDeep { depth } => {
                format!("brackets nested deeper than {depth} levels")
            }
            SyntaxErrorKind::AspifInput => "aspif input, not a program".to_owned(),
            SyntaxErrorKind::TokenSourceBreach {
                breach: SourceBreach::Tiling { .. },
            } => "the token source breached its tiling law".to_owned(),
            SyntaxErrorKind::TokenSourceBreach {
                breach: SourceBreach::Refusal { .. },
            } => "the token source refused a position it owes a token".to_owned(),
            SyntaxErrorKind::FormNotAllowedHere { form, context } => {
                format!(
                    "{} is not allowed in {}",
                    describe_form(*form),
                    describe_restriction(*context)
                )
            }
            SyntaxErrorKind::MisplacedDocComment {
                reason: MisplacedDoc::NoStatementFollows,
            } => "doc comment followed by no statement".to_owned(),
            SyntaxErrorKind::MisplacedDocComment {
                reason: MisplacedDoc::InsideStatement,
            } => "doc comment inside a statement".to_owned(),
        }
    }

    /// The primary label's text.
    fn primary_text(&self) -> Option<String> {
        Some(
            match self {
                SyntaxErrorKind::UnexpectedCharacters => "no token begins here",
                SyntaxErrorKind::UnknownHashWord => "not a keyword of the language",
                SyntaxErrorKind::MalformedString { defect: StringDefect::RawLineBreak } => {
                    "the literal ends at the line break without its closing quote"
                }
                SyntaxErrorKind::MalformedString { defect: StringDefect::InvalidEscape(_) } => {
                    "not one of the escapes `\\\"`, `\\\\`, `\\n`"
                }
                SyntaxErrorKind::MalformedString { defect: StringDefect::Unterminated }
                | SyntaxErrorKind::UnterminatedBlockComment => "opened here and never closed",
                SyntaxErrorKind::UnterminatedScript => "the region begins here and no `#end` follows",
                SyntaxErrorKind::AnonymousInTheoryExpression => "`_` is not admitted here",
                SyntaxErrorKind::UnexpectedToken { .. } => return None,
                SyntaxErrorKind::UnexpectedEndOfInput { .. } => "the input ends here",
                SyntaxErrorKind::NestingTooDeep { .. } => {
                    "this bracket would open one level too many; the rest of the statement is carried unparsed"
                }
                SyntaxErrorKind::AspifInput => "the aspif header",
                SyntaxErrorKind::TokenSourceBreach { .. } => "here",
                SyntaxErrorKind::FormNotAllowedHere { .. } => "not allowed here",
                SyntaxErrorKind::MisplacedDocComment { .. } => "a plain comment here",
            }
            .to_owned(),
        )
    }

    /// The notes the kind derives.
    fn notes(&self) -> Vec<String> {
        match self {
            SyntaxErrorKind::UnknownHashWord => vec![
                "`#`-words are recognized whole: a keyword extended by name characters is one unknown word"
                    .to_owned(),
            ],
            SyntaxErrorKind::UnterminatedBlockComment => vec![
                "under the clingo dialect a `%` inside a block comment silences the rest of its line, closers included"
                    .to_owned(),
            ],
            SyntaxErrorKind::AspifInput => vec![
                "the input is in the intermediate format a solver reads; the syntax tier does not parse it"
                    .to_owned(),
            ],
            SyntaxErrorKind::MisplacedDocComment { .. } => vec![
                "a `%!` line documents the statement that follows it; the program is unchanged".to_owned(),
            ],
            _ => Vec::new(),
        }
    }

    /// The helps the kind derives: one per hint.
    fn helps(&self) -> Vec<String> {
        let hint = match self {
            SyntaxErrorKind::UnexpectedToken { hint, .. }
            | SyntaxErrorKind::UnexpectedEndOfInput { hint, .. } => *hint,
            _ => None,
        };
        hint.map(help_text).into_iter().collect()
    }
}

/// The help a hint carries.
fn help_text(hint: Hint) -> String {
    match hint {
        Hint::TrailingCommaInArguments => {
            "remove the trailing comma: an argument list takes no trailing comma".to_owned()
        }
        Hint::QueryMarkNeedsAspCore2 => {
            "a final `?` is the ASP-Core-2 query mark; parse under the ASP-Core-2 dialect to read it as a query"
                .to_owned()
        }
        Hint::LeadingZeroNumeral => {
            "decimal numerals take no leading zero: `007` is three numerals".to_owned()
        }
        Hint::EmptyConditionBeforePipe => {
            "write `;` here: an empty condition directly before `|` does not parse".to_owned()
        }
        Hint::HeuristicNeedsAnnotation => {
            "`#heuristic` takes its bracket after the dot: `[weight@priority, modifier]`".to_owned()
        }
    }
}

/// The text a related locus carries.
fn related_text(locus: RelatedLocus) -> String {
    match locus {
        RelatedLocus::StatementBegan => "the statement began here".to_owned(),
        RelatedLocus::ToClose(opener) => format!("to close this {}", describe(opener)),
        RelatedLocus::LiteralExtent => "the literal, whole".to_owned(),
    }
}

/// An expected set in words: kinds, then words, then classes — the
/// set's own order — joined as a list.
fn render_expected(expected: &ExpectedSet) -> String {
    let items: Vec<String> = expected
        .iter()
        .map(|item| match item {
            Expected::Token(kind) => describe(*kind),
            Expected::Word(word) => format!("`{word}`"),
            Expected::Class(class) => describe_class(*class).to_owned(),
        })
        .collect();
    match items.as_slice() {
        [] => "nothing".to_owned(),
        [one] => one.clone(),
        [first, second] => format!("{first} or {second}"),
        [init @ .., last] => format!("{}, or {last}", init.join(", ")),
    }
}

/// A token kind in words: its spelling in backticks where it has one,
/// its class otherwise.
fn describe(kind: SyntaxKind) -> String {
    let spelling = match kind {
        SyntaxKind::WHITESPACE => return "whitespace".to_owned(),
        SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT | SyntaxKind::SHEBANG_COMMENT => {
            return "a comment".to_owned();
        }
        SyntaxKind::DOC_COMMENT => return "a doc comment".to_owned(),
        SyntaxKind::IDENT => return "an identifier".to_owned(),
        SyntaxKind::VARIABLE => return "a variable".to_owned(),
        SyntaxKind::ANONYMOUS => "_",
        SyntaxKind::NUMBER => return "a number".to_owned(),
        SyntaxKind::STRING => return "a string".to_owned(),
        SyntaxKind::KW_CONST => "#const",
        SyntaxKind::KW_COUNT => "#count",
        SyntaxKind::KW_DEFINED => "#defined",
        SyntaxKind::KW_EDGE => "#edge",
        SyntaxKind::KW_EXTERNAL => "#external",
        SyntaxKind::KW_FALSE => "#false",
        SyntaxKind::KW_HEURISTIC => "#heuristic",
        SyntaxKind::KW_INCLUDE => "#include",
        SyntaxKind::KW_INF => "#inf",
        SyntaxKind::KW_MAX => "#max",
        SyntaxKind::KW_MAXIMIZE => "#maximize",
        SyntaxKind::KW_MIN => "#min",
        SyntaxKind::KW_MINIMIZE => "#minimize",
        SyntaxKind::KW_PROGRAM => "#program",
        SyntaxKind::KW_PROJECT => "#project",
        SyntaxKind::KW_SCRIPT => "#script",
        SyntaxKind::KW_SHOW => "#show",
        SyntaxKind::KW_SUM => "#sum",
        SyntaxKind::KW_SUM_PLUS => "#sum+",
        SyntaxKind::KW_SUP => "#sup",
        SyntaxKind::KW_THEORY => "#theory",
        SyntaxKind::KW_TRUE => "#true",
        SyntaxKind::KW_NOT => "not",
        SyntaxKind::KW_END => "#end",
        SyntaxKind::DOT => ".",
        SyntaxKind::DOTDOT => "..",
        SyntaxKind::COMMA => ",",
        SyntaxKind::SEMICOLON => ";",
        SyntaxKind::COLON => ":",
        SyntaxKind::NECK => ":-",
        SyntaxKind::WEAK_NECK => ":~",
        SyntaxKind::PIPE => "|",
        SyntaxKind::L_PAREN => "(",
        SyntaxKind::R_PAREN => ")",
        SyntaxKind::L_BRACKET => "[",
        SyntaxKind::R_BRACKET => "]",
        SyntaxKind::L_BRACE => "{",
        SyntaxKind::R_BRACE => "}",
        SyntaxKind::PLUS => "+",
        SyntaxKind::MINUS => "-",
        SyntaxKind::STAR => "*",
        SyntaxKind::STAR_STAR => "**",
        SyntaxKind::SLASH => "/",
        SyntaxKind::BACKSLASH => "\\",
        SyntaxKind::CARET => "^",
        SyntaxKind::AMPERSAND => "&",
        SyntaxKind::TILDE => "~",
        SyntaxKind::QUESTION => "?",
        SyntaxKind::AT => "@",
        SyntaxKind::EQ => "=",
        SyntaxKind::NEQ => "!=",
        SyntaxKind::LT => "<",
        SyntaxKind::LE => "<=",
        SyntaxKind::GT => ">",
        SyntaxKind::GE => ">=",
        SyntaxKind::THEORY_OP => return "a theory operator".to_owned(),
        SyntaxKind::SCRIPT_BODY => return "a script body".to_owned(),
        SyntaxKind::SPLICE => return "a splice".to_owned(),
        SyntaxKind::ERROR => return "unrecognized input".to_owned(),
        SyntaxKind::EOF => return "end of input".to_owned(),
        node => return format!("{node}"),
    };
    format!("`{spelling}`")
}

fn describe_class(class: SyntaxClass) -> &'static str {
    match class {
        SyntaxClass::Statement => "a statement",
        SyntaxClass::Head => "a head",
        SyntaxClass::BodyElement => "a body element",
        SyntaxClass::Literal => "a literal",
        SyntaxClass::Atom => "an atom",
        SyntaxClass::Term => "a term",
        SyntaxClass::TheoryTerm => "a theory term",
        SyntaxClass::TheoryOperator => "a theory operator",
        SyntaxClass::Guard => "a guard",
        SyntaxClass::Signature => "a signature",
        SyntaxClass::Condition => "a condition",
        SyntaxClass::Annotation => "an annotation",
        SyntaxClass::EndOfInput => "end of input",
    }
}

fn describe_form(form: RestrictedForm) -> &'static str {
    match form {
        RestrictedForm::Variable => "a variable",
        RestrictedForm::AnonymousVariable => "the anonymous variable",
        RestrictedForm::Pool => "a pool",
        RestrictedForm::Interval => "an interval",
        RestrictedForm::ExternalCall => "an `@`-call",
        RestrictedForm::PooledAbsoluteValue => "an absolute value over a pooled argument",
    }
}

fn describe_restriction(context: Restriction) -> &'static str {
    match context {
        Restriction::ConstantTerm => "a `#const` term",
        Restriction::TermValue => "a term value",
    }
}

impl ToDiagnostic for SyntaxError {
    /// The base normal form: identity, severity, the headline, the
    /// primary label, the related loci as secondary labels, and the
    /// notes and helps the kind derives. O(payload).
    fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            self.kind.id(),
            self.kind.severity(),
            self.kind.headline(),
            Label {
                location: self.primary,
                message: self.kind.primary_text(),
            },
        )
        .expect("every headline is non-empty by construction");
        for related in &self.related {
            diagnostic = diagnostic.with_secondary(Label {
                location: related.location,
                message: Some(related_text(related.locus)),
            });
        }
        for note in self.kind.notes() {
            diagnostic = diagnostic.with_note(note);
        }
        for help in self.kind.helps() {
            diagnostic = diagnostic.with_help(help);
        }
        diagnostic
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::fs;
    use std::path::PathBuf;

    use themelios_base::source::{SourceId, SourceSet};
    use themelios_base::span::Span;
    use themelios_base::view::human;

    use super::*;

    fn at(start: u32, end: u32) -> Location {
        Location {
            source: SourceId::new(0),
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end)).expect("ordered"),
        }
    }

    fn expected(items: &[Expected]) -> ExpectedSet {
        items.iter().copied().collect()
    }

    /// One representative of every kind, in the roster's order.
    fn representatives() -> Vec<SyntaxErrorKind> {
        vec![
            SyntaxErrorKind::UnexpectedCharacters,
            SyntaxErrorKind::UnknownHashWord,
            SyntaxErrorKind::MalformedString {
                defect: StringDefect::InvalidEscape('q'),
            },
            SyntaxErrorKind::UnterminatedBlockComment,
            SyntaxErrorKind::UnterminatedScript,
            SyntaxErrorKind::AnonymousInTheoryExpression,
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[Expected::Token(SyntaxKind::DOT)]),
                found: SyntaxKind::COMMA,
                hint: None,
            },
            SyntaxErrorKind::UnexpectedEndOfInput {
                expected: expected(&[Expected::Class(SyntaxClass::Term)]),
                hint: None,
            },
            SyntaxErrorKind::NestingTooDeep { depth: 3 },
            SyntaxErrorKind::AspifInput,
            SyntaxErrorKind::TokenSourceBreach {
                breach: SourceBreach::Refusal {
                    at: ByteOffset::new(2),
                },
            },
            SyntaxErrorKind::FormNotAllowedHere {
                form: RestrictedForm::Pool,
                context: Restriction::ConstantTerm,
            },
            SyntaxErrorKind::MisplacedDocComment {
                reason: MisplacedDoc::NoStatementFollows,
            },
        ]
    }

    #[test]
    fn the_identity_table_matches_its_snapshot() {
        let mut table = String::new();
        for kind in representatives() {
            let error = SyntaxError::new(kind, at(0, 1));
            writeln!(table, "{} {}", error.id(), error.severity()).expect("write to a String");
        }
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/identity-table.txt");
        if std::env::var_os("GOLDEN_BLESS").is_some() {
            fs::write(&path, &table).expect("golden file writes");
            return;
        }
        let shipped = fs::read_to_string(&path).expect("the identity table is shipped");
        assert_eq!(
            table, shipped,
            "docs/design/syntax.md Appendix B: the identity table changed"
        );
    }

    #[test]
    fn identities_are_in_the_syntax_namespace_and_only_the_doc_warning_warns() {
        for kind in representatives() {
            let error = SyntaxError::new(kind.clone(), at(0, 1));
            assert_eq!(error.id().namespace(), "syntax");
            let expected = if matches!(kind, SyntaxErrorKind::MisplacedDocComment { .. }) {
                Severity::Warning
            } else {
                Severity::Error
            };
            assert_eq!(error.severity(), expected, "{kind:?}");
        }
    }

    #[test]
    fn lowering_carries_identity_severity_primary_and_related_loci() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedEndOfInput {
                expected: expected(&[Expected::Token(SyntaxKind::R_BRACE)]),
                hint: None,
            },
            at(10, 10),
        )
        .with_related(Related {
            locus: RelatedLocus::ToClose(SyntaxKind::L_BRACE),
            location: at(4, 5),
        });
        let lowered = error.to_diagnostic();
        assert_eq!(lowered.id().to_string(), "syntax::unexpected-end-of-input");
        assert_eq!(lowered.severity(), Severity::Error);
        assert_eq!(lowered.primary().location, at(10, 10));
        assert_eq!(lowered.message(), "expected `}`, found end of input");
        let secondary: Vec<_> = lowered.secondary().iter().collect();
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0].location, at(4, 5));
        assert_eq!(secondary[0].message.as_deref(), Some("to close this `{`"));
    }

    #[test]
    fn expected_sets_render_kinds_then_words_then_classes() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[
                    Expected::Class(SyntaxClass::Term),
                    Expected::Word(GrammarWord::Default),
                    Expected::Token(SyntaxKind::DOT),
                    Expected::Token(SyntaxKind::COMMA),
                ]),
                found: SyntaxKind::R_PAREN,
                hint: None,
            },
            at(3, 4),
        );
        assert_eq!(
            error.to_diagnostic().message(),
            "expected `.`, `,`, `default`, or a term, found `)`"
        );
    }

    #[test]
    fn a_hint_lowers_to_a_help() {
        let error = SyntaxError::new(
            SyntaxErrorKind::UnexpectedToken {
                expected: expected(&[Expected::Class(SyntaxClass::Term)]),
                found: SyntaxKind::R_PAREN,
                hint: Some(Hint::TrailingCommaInArguments),
            },
            at(3, 4),
        );
        let lowered = error.to_diagnostic();
        assert_eq!(lowered.helps().len(), 1);
        assert!(lowered.helps()[0].contains("trailing comma"));
    }

    #[test]
    fn a_kind_never_lowers_to_an_empty_headline() {
        let mut catalog = SourceSet::new();
        let file = catalog
            .add("x.lp".to_owned(), "p(\"a\\qb\").\n".to_owned())
            .expect("admits");
        for kind in representatives() {
            let error = SyntaxError::new(
                kind,
                Location {
                    source: file,
                    span: Span::new(ByteOffset::new(2), ByteOffset::new(3)).expect("ordered"),
                },
            );
            let lowered = error.to_diagnostic();
            assert!(!lowered.message().is_empty());
            assert!(human(&lowered, &catalog).starts_with(&format!("{}[", lowered.severity())));
        }
    }

    #[test]
    fn grammar_words_display_their_spellings() {
        assert_eq!(GrammarWord::Override.to_string(), "override");
        assert_eq!(GrammarWord::Directive.to_string(), "directive");
    }
}
