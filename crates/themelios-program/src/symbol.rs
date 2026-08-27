//! The ground symbol and the validated names beneath the term algebra
//! (docs/design/program.md §3.1, §3.2, §3.6, §3.7). `Symbol` is the value an
//! answer set contains, an `@`-function exchanges, and a pattern unifies
//! against — owned plain data whose every walk (clone, drop, equality,
//! ordering, hashing, debug, and the `fold` rebuild) is iterative (§13, §14),
//! so a ground value tens of thousands of levels deep is handled without
//! touching the call stack.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::token::{LexMode, TokenSource};
use themelios_syntax::tree::SyntaxKind;

/// Strong (explicit) negation — the `-` of `-p` (§3.1; the precise register:
/// strong, not classical-logic, negation). Distinct in the type from default
/// negation (a body-literal sign, §4) and from the bitwise `~` (a term
/// operator, §3.3): the three are three different things and the API holds them
/// apart (spec §1.4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Sign {
    /// A positive atom, `p`.
    Positive,
    /// A strongly-negated atom, `-p`.
    Negative,
}

/// A ground term: the value an answer set contains, an `@`-function exchanges,
/// and a pattern unifies against (§3.1). `Infimum` and `Supremum` are the least
/// and greatest elements of the term order (grammar §5.1). Owned plain data.
///
/// No `#[derive]`: `Clone`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, and
/// `Debug` are hand-written and iterative (§13, §14), so a deep ground value is
/// cloned, compared, hashed, rendered, and dropped without call-stack recursion.
/// Each matches its derived shape (held by the naive twin, tests/symbol_laws.rs)
/// while its depth is the heap's (the depth gate, §16).
pub enum Symbol {
    /// The least element of the term order.
    Infimum,
    /// A number — `i32`, the engine's own width (§3.1).
    Number(i32),
    /// A string.
    String(String),
    /// A predicate or constant (a constant is the empty-argument case), carrying
    /// its strong sign. `name` is a validated identifier.
    Function {
        /// The functor name.
        name: Name,
        /// The arguments; empty for a constant.
        arguments: Vec<Symbol>,
        /// The strong sign.
        sign: Sign,
    },
    /// The anonymous functor: `(a, b)`, the one-element `(a,)`, the empty `()`.
    Tuple(Vec<Symbol>),
    /// The greatest element of the term order.
    Supremum,
}

/// A validated identifier — a function or predicate name (grammar §4.2). The
/// invariant "a name is a legal identifier" is guarded at construction, so a
/// `Symbol` or a `Term` cannot carry a name the grammar would reject (§3.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Name(String);

/// A validated variable name (grammar §4.2's `VARIABLE`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VarName(String);

/// Text that is not the grammar's `IDENTIFIER` class, carrying the offending
/// text — a value, not a rendered string (spec §1.5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnIdentifier {
    /// The text that is not an identifier.
    pub text: String,
}

/// Text that is not the grammar's `VARIABLE` class.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAVariable {
    /// The text that is not a variable.
    pub text: String,
}

impl Name {
    /// Refuses text that is not the grammar's `IDENTIFIER` class
    /// (`[_']* [a-z] ['A-Za-z0-9_]*`, grammar §4.2), classified by the syntax
    /// tier's one lexer so no second definition of "a name" exists (spec §2
    /// item 3). O(text).
    pub fn new(text: impl Into<String>) -> Result<Name, NotAnIdentifier> {
        let text = text.into();
        if classifies_whole(&text, SyntaxKind::IDENT) {
            Ok(Name(text))
        } else {
            Err(NotAnIdentifier { text })
        }
    }

    /// The identifier text. O(1).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl VarName {
    /// Refuses text that is not the grammar's `VARIABLE` class
    /// (`[_']* [A-Z] ['A-Za-z0-9_]*`, grammar §4.2), classified by the syntax
    /// tier's one lexer. O(text).
    pub fn new(text: impl Into<String>) -> Result<VarName, NotAVariable> {
        let text = text.into();
        if classifies_whole(&text, SyntaxKind::VARIABLE) {
            Ok(VarName(text))
        } else {
            Err(NotAVariable { text })
        }
    }

    /// The variable text. O(1).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether `text` lexes, under the syntax tier's one lexer, to a single token of
/// `want` spanning the whole text — the shared classifier for names (§3.2;
/// grammar §4.2). Name lexing is dialect-neutral, so the dialect is immaterial;
/// a throwaway source carries the classification, since the tier exposes the
/// classifier only through its lexer (never a second definition, spec §2 item
/// 3). The empty text lexes to `EOF`, never `want`, so it is no name.
fn classifies_whole(text: &str, want: SyntaxKind) -> bool {
    let Ok(source) = Source::new(SourceId::new(0), text.to_owned()) else {
        return false;
    };
    let lexer = Lexer::new(&source, Dialect::Clingo);
    match lexer.token_at(ByteOffset::new(0), LexMode::Normal) {
        Ok(token) => token.kind == want && token.text.len() == text.len(),
        Err(_) => false,
    }
}

/// The identity of a predicate atom: its strong sign, its name, and its arity —
/// the key the dependency graph's nodes (analysis.md §4) and the pattern
/// matcher's range (§11.3) are built from (§3.7, §4.8).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Signature {
    /// The strong sign.
    pub sign: Sign,
    /// The predicate name.
    pub name: Name,
    /// The arity.
    pub arity: u32,
}

impl Symbol {
    /// The functor name — `Some` for a function or constant, `None` otherwise. O(1).
    pub fn name(&self) -> Option<&Name> {
        match self {
            Symbol::Function { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The immediate arguments — a function's arguments or a tuple's elements;
    /// the empty slice for an atomic symbol. O(1).
    pub fn arguments(&self) -> &[Symbol] {
        match self {
            Symbol::Function { arguments, .. } => arguments,
            Symbol::Tuple(elements) => elements,
            _ => &[],
        }
    }

    /// The i-th argument, or `None` — total, never a panicking index. O(1).
    pub fn arg(&self, i: usize) -> Option<&Symbol> {
        self.arguments().get(i)
    }

    /// The number of arguments — `0` for an atomic symbol. O(1).
    pub fn arity(&self) -> u32 {
        // A ground term carries no more arguments than a `Vec` holds, itself far
        // under `u32::MAX` on any real machine; the cast cannot truncate one
        // (the workspace `cast_possible_truncation` allowance, argued in place).
        self.arguments().len() as u32
    }

    /// The signature `(sign, name, arity)` — `Some` for a function or constant,
    /// `None` otherwise. O(1) but for the name clone.
    pub fn signature(&self) -> Option<Signature> {
        match self {
            Symbol::Function {
                name,
                arguments,
                sign,
            } => Some(Signature {
                sign: *sign,
                name: name.clone(),
                arity: arguments.len() as u32,
            }),
            _ => None,
        }
    }
}

/// One level of a symbol unrolled, its children a generic `T`, the leaves kept
/// whole (§3.6). At `T = Symbol` it is the owned decomposition (`From`
/// rebuilds); inside `fold` it is what the step sees with children already
/// folded.
pub enum SymbolParts<T> {
    /// The least element.
    Infimum,
    /// A number.
    Number(i32),
    /// A string.
    String(String),
    /// A function or constant.
    Function {
        /// The functor name.
        name: Name,
        /// The folded arguments.
        arguments: Vec<T>,
        /// The strong sign.
        sign: Sign,
    },
    /// A tuple.
    Tuple(Vec<T>),
    /// The greatest element.
    Supremum,
}

impl Symbol {
    /// This symbol decomposed one level, its children owned (§3.6). O(1) plus
    /// the moved children; `From<SymbolParts<Symbol>>` is the inverse.
    pub fn into_parts(mut self) -> SymbolParts<Symbol> {
        // `Symbol` implements `Drop` for its iterative teardown (§13), so its
        // fields cannot be moved out by a consuming pattern — that would drop a
        // partly-moved value, which `forbid(unsafe_code)` gives no way to make
        // sound. Each field is instead taken through `&mut self`, leaving an
        // emptied husk that this method's return then drops finding no children
        // (O(1)); the extracted children are the real ones.
        match &mut self {
            Symbol::Infimum => SymbolParts::Infimum,
            Symbol::Number(n) => SymbolParts::Number(*n),
            Symbol::String(s) => SymbolParts::String(std::mem::take(s)),
            Symbol::Function {
                name,
                arguments,
                sign,
            } => SymbolParts::Function {
                name: std::mem::replace(name, Name(String::new())),
                arguments: std::mem::take(arguments),
                sign: *sign,
            },
            Symbol::Tuple(elements) => SymbolParts::Tuple(std::mem::take(elements)),
            Symbol::Supremum => SymbolParts::Supremum,
        }
    }

    /// The immediate and transitive subsymbols in pre-order — a contract (§3.6).
    /// Iterative; O(nodes) over the walk.
    pub fn subsymbols(&self) -> impl Iterator<Item = &Symbol> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let symbol = stack.pop()?;
            // Push children in reverse so they are yielded left-to-right.
            for child in symbol.arguments().iter().rev() {
                stack.push(child);
            }
            Some(symbol)
        })
    }

    /// Bottom-up rebuild, iterative (§13): each node's children are folded before
    /// it, in document order, `O(nodes)` heap, nothing cloned (§3.6). The one
    /// primitive every rebuild over a symbol is written in.
    pub fn fold<T>(self, mut step: impl FnMut(SymbolParts<T>) -> T) -> T {
        match self.try_fold::<T, std::convert::Infallible>(|parts| Ok(step(parts))) {
            Ok(folded) => folded,
            Err(never) => match never {},
        }
    }

    /// `fold`, short-circuiting on the first `Err` (§3.6). Iterative.
    pub fn try_fold<T, E>(
        self,
        mut step: impl FnMut(SymbolParts<T>) -> Result<T, E>,
    ) -> Result<T, E> {
        // An explicit work list of enter/assemble frames; `done` holds finished
        // `T`s, so recursion depth is the heap's, not the stack's.
        enum Frame {
            Enter(Symbol),
            AssembleFunction {
                name: Name,
                sign: Sign,
                arity: usize,
            },
            AssembleTuple {
                arity: usize,
            },
        }
        let mut work = vec![Frame::Enter(self)];
        let mut done: Vec<T> = Vec::new();
        while let Some(frame) = work.pop() {
            match frame {
                Frame::Enter(symbol) => match symbol.into_parts() {
                    SymbolParts::Infimum => done.push(step(SymbolParts::Infimum)?),
                    SymbolParts::Number(n) => done.push(step(SymbolParts::Number(n))?),
                    SymbolParts::String(s) => done.push(step(SymbolParts::String(s))?),
                    SymbolParts::Supremum => done.push(step(SymbolParts::Supremum)?),
                    SymbolParts::Function {
                        name,
                        arguments,
                        sign,
                    } => {
                        let arity = arguments.len();
                        work.push(Frame::AssembleFunction { name, sign, arity });
                        for argument in arguments.into_iter().rev() {
                            work.push(Frame::Enter(argument));
                        }
                    }
                    SymbolParts::Tuple(elements) => {
                        let arity = elements.len();
                        work.push(Frame::AssembleTuple { arity });
                        for element in elements.into_iter().rev() {
                            work.push(Frame::Enter(element));
                        }
                    }
                },
                Frame::AssembleFunction { name, sign, arity } => {
                    let arguments = done.split_off(done.len() - arity);
                    done.push(step(SymbolParts::Function {
                        name,
                        arguments,
                        sign,
                    })?);
                }
                Frame::AssembleTuple { arity } => {
                    let elements = done.split_off(done.len() - arity);
                    done.push(step(SymbolParts::Tuple(elements))?);
                }
            }
        }
        Ok(done.pop().expect("the root's fold"))
    }
}

impl From<SymbolParts<Symbol>> for Symbol {
    fn from(parts: SymbolParts<Symbol>) -> Symbol {
        match parts {
            SymbolParts::Infimum => Symbol::Infimum,
            SymbolParts::Number(n) => Symbol::Number(n),
            SymbolParts::String(s) => Symbol::String(s),
            SymbolParts::Function {
                name,
                arguments,
                sign,
            } => Symbol::Function {
                name,
                arguments,
                sign,
            },
            SymbolParts::Tuple(elements) => Symbol::Tuple(elements),
            SymbolParts::Supremum => Symbol::Supremum,
        }
    }
}

impl Clone for Symbol {
    fn clone(&self) -> Symbol {
        // Post-order deep copy (§13): visit each node, then rebuild bottom-up
        // from a stack of finished clones.
        enum Step<'a> {
            Enter(&'a Symbol),
            AssembleFunction {
                name: &'a Name,
                sign: Sign,
                arity: usize,
            },
            AssembleTuple {
                arity: usize,
            },
        }
        let mut work = vec![Step::Enter(self)];
        let mut done: Vec<Symbol> = Vec::new();
        while let Some(step) = work.pop() {
            match step {
                Step::Enter(symbol) => match symbol {
                    Symbol::Infimum => done.push(Symbol::Infimum),
                    Symbol::Number(n) => done.push(Symbol::Number(*n)),
                    Symbol::String(s) => done.push(Symbol::String(s.clone())),
                    Symbol::Supremum => done.push(Symbol::Supremum),
                    Symbol::Function {
                        name,
                        arguments,
                        sign,
                    } => {
                        work.push(Step::AssembleFunction {
                            name,
                            sign: *sign,
                            arity: arguments.len(),
                        });
                        for argument in arguments.iter().rev() {
                            work.push(Step::Enter(argument));
                        }
                    }
                    Symbol::Tuple(elements) => {
                        work.push(Step::AssembleTuple {
                            arity: elements.len(),
                        });
                        for element in elements.iter().rev() {
                            work.push(Step::Enter(element));
                        }
                    }
                },
                Step::AssembleFunction { name, sign, arity } => {
                    let arguments = done.split_off(done.len() - arity);
                    done.push(Symbol::Function {
                        name: name.clone(),
                        arguments,
                        sign,
                    });
                }
                Step::AssembleTuple { arity } => {
                    let elements = done.split_off(done.len() - arity);
                    done.push(Symbol::Tuple(elements));
                }
            }
        }
        done.pop().expect("the root's clone")
    }
}

impl Drop for Symbol {
    fn drop(&mut self) {
        // Dismantle iteratively (§13): move every descendant onto a work list
        // and drop them one at a time, so a deep value drops without recursion.
        let mut stack: Vec<Symbol> = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut symbol) = stack.pop() {
            take_children(&mut symbol, &mut stack);
            // `symbol` drops here childless: its own `Drop` finds nothing.
        }
    }
}

/// Moves a symbol's immediate child symbols onto `out`, leaving it childless.
fn take_children(symbol: &mut Symbol, out: &mut Vec<Symbol>) {
    match symbol {
        Symbol::Function { arguments, .. } => out.append(arguments),
        Symbol::Tuple(elements) => out.append(elements),
        _ => {}
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Symbol) -> bool {
        // Iterative structural equality (§13): a work list of pairs, returning on
        // the first mismatch.
        let mut pairs: Vec<(&Symbol, &Symbol)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            match (a, b) {
                (Symbol::Infimum, Symbol::Infimum) | (Symbol::Supremum, Symbol::Supremum) => {}
                (Symbol::Number(x), Symbol::Number(y)) if x == y => {}
                (Symbol::String(x), Symbol::String(y)) if x == y => {}
                (
                    Symbol::Function {
                        name: left_name,
                        arguments: left_args,
                        sign: left_sign,
                    },
                    Symbol::Function {
                        name: right_name,
                        arguments: right_args,
                        sign: right_sign,
                    },
                ) if left_sign == right_sign
                    && left_name == right_name
                    && left_args.len() == right_args.len() =>
                {
                    pairs.extend(left_args.iter().zip(right_args));
                }
                (Symbol::Tuple(x), Symbol::Tuple(y)) if x.len() == y.len() => {
                    pairs.extend(x.iter().zip(y));
                }
                _ => return false,
            }
        }
        true
    }
}
impl Eq for Symbol {}

impl PartialOrd for Symbol {
    fn partial_cmp(&self, other: &Symbol) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Symbol {
    fn cmp(&self, other: &Symbol) -> Ordering {
        // The ground-term order of the literature and the engine (grammar §5.1),
        // iterative and lexicographic: compare the rank band (which crosses the
        // `String` variant, §3.1), then the leaf value or the **function-like
        // head** — a tuple orders as a positive anonymous function (§3.1), so a
        // function and a tuple interleave by (sign, arity, name) with a tuple's
        // name anonymous and never tie — then descend the arguments, returning on
        // the first difference. **Total by construction** (§3.1's precondition):
        // equal only to an identical symbol, so it agrees with `Eq`. Where the
        // anonymous name and the bands fall in the printed order is the authority's,
        // the differential (§16) confirming it without disturbing totality; the
        // naive twin holds the iteration honest.
        let mut pairs: Vec<(&Symbol, &Symbol)> = vec![(self, other)];
        while let Some((a, b)) = pairs.pop() {
            let by_rank = order_rank(a).cmp(&order_rank(b));
            if by_rank != Ordering::Equal {
                return by_rank;
            }
            let here = match (a, b) {
                (Symbol::Number(x), Symbol::Number(y)) => x.cmp(y),
                (Symbol::String(x), Symbol::String(y)) => x.cmp(y),
                // A function and/or a tuple at the same rank: order by the
                // function-like head (a tuple's name is anonymous, `None`), so this
                // one arm serves function/function, tuple/tuple, and the mixed case
                // and no distinct pair falls through to `Equal`; then, when the
                // heads match, descend the arguments (leftmost on top).
                (
                    Symbol::Function { .. } | Symbol::Tuple(_),
                    Symbol::Function { .. } | Symbol::Tuple(_),
                ) => head_key(a).cmp(&head_key(b)).then_with(|| {
                    pairs.extend(a.arguments().iter().zip(b.arguments()).rev());
                    Ordering::Equal
                }),
                // Equal-rank leaves (`Infimum`/`Supremum`) are equal here.
                _ => Ordering::Equal,
            };
            if here != Ordering::Equal {
                return here;
            }
        }
        Ordering::Equal
    }
}

/// The variant's position in the ground-term order (§3.1, grammar §5.1). A
/// nullary function (a constant) and an empty tuple sort before a string, an
/// arity-bearing function or tuple after — the order crosses the `String`
/// variant. The differential (§16) is authoritative on this order.
fn order_rank(symbol: &Symbol) -> u8 {
    match symbol {
        Symbol::Infimum => 0,
        Symbol::Number(_) => 1,
        Symbol::Function { arguments, .. } if arguments.is_empty() => 2,
        Symbol::Tuple(elements) if elements.is_empty() => 2,
        Symbol::String(_) => 3,
        Symbol::Function { .. } | Symbol::Tuple(_) => 4,
        Symbol::Supremum => 5,
    }
}

/// The function-like head that orders a function or a tuple (§3.1): its strong
/// sign, then its arity, then its name. A tuple is a *positive* anonymous function
/// — the authority's reading — so its head sign is `Positive` and its name `None`.
/// `Ord` on `(Sign, usize, Option<&Name>)` sorts a positive head before a negative
/// one, then a smaller arity before a larger, then an anonymous head before any
/// named one — a total key, so a function and a same-arity tuple never compare equal
/// (a tuple's `None` name never ties a function's `Some`). This field order is the
/// authority's printed order, confirmed by the differential (§16); the totality is
/// fixed here.
fn head_key(symbol: &Symbol) -> (Sign, usize, Option<&Name>) {
    let sign = match symbol {
        Symbol::Function { sign, .. } => *sign,
        // A tuple interleaves among the positive functions of its arity, never before
        // every function, so its head sign is `Positive` (§3.1, the authority's reading).
        _ => Sign::Positive,
    };
    (sign, symbol.arguments().len(), symbol.name())
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Iterative pre-order hash (§13): each node's discriminant and leaf value,
        // then a length marker for its children, feed the hasher in document
        // order — the same content projection as `Eq` (§5.2), so equal symbols
        // hash equal (the children follow in the pre-order walk).
        for symbol in self.subsymbols() {
            match symbol {
                Symbol::Infimum => state.write_u8(0),
                Symbol::Number(n) => {
                    state.write_u8(1);
                    n.hash(state);
                }
                Symbol::String(s) => {
                    state.write_u8(2);
                    s.hash(state);
                }
                Symbol::Function {
                    name,
                    arguments,
                    sign,
                } => {
                    state.write_u8(3);
                    sign.hash(state);
                    name.hash(state);
                    state.write_usize(arguments.len());
                }
                Symbol::Tuple(elements) => {
                    state.write_u8(4);
                    state.write_usize(elements.len());
                }
                Symbol::Supremum => state.write_u8(5),
            }
        }
    }
}

impl std::fmt::Debug for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A derived-shaped `Debug`, iterative (§14): rendered from an explicit
        // work list of print actions, so a deep value formats without recursion.
        enum Act<'a> {
            Node(&'a Symbol),
            Str(&'static str),
            Owned(String),
        }
        let mut work = vec![Act::Node(self)];
        while let Some(act) = work.pop() {
            match act {
                Act::Str(s) => f.write_str(s)?,
                Act::Owned(s) => f.write_str(&s)?,
                Act::Node(symbol) => match symbol {
                    Symbol::Infimum => f.write_str("Infimum")?,
                    Symbol::Supremum => f.write_str("Supremum")?,
                    Symbol::Number(n) => write!(f, "Number({n})")?,
                    Symbol::String(s) => write!(f, "String({s:?})")?,
                    Symbol::Function {
                        name,
                        arguments,
                        sign,
                    } => {
                        // Declaration order (§14, derived-shaped): name, arguments,
                        // then sign — `sign` is `Copy`, so it rides the closing action
                        // after the argument list rather than printing before it.
                        write!(f, "Function {{ name: {name:?}, arguments: [")?;
                        work.push(Act::Owned(format!("], sign: {sign:?} }}")));
                        for (i, child) in arguments.iter().enumerate().rev() {
                            work.push(Act::Node(child));
                            if i > 0 {
                                work.push(Act::Str(", "));
                            }
                        }
                    }
                    Symbol::Tuple(elements) => {
                        f.write_str("Tuple([")?;
                        work.push(Act::Str("])"));
                        for (i, child) in elements.iter().enumerate().rev() {
                            work.push(Act::Node(child));
                            if i > 0 {
                                work.push(Act::Str(", "));
                            }
                        }
                    }
                },
            }
        }
        Ok(())
    }
}

/// A Rust value that denotes a ground symbol (§3.4). Not `From`/`Into`: this names a
/// KR relationship — *this value denotes this ground term* — and, being this crate's
/// own trait, a downstream library may implement it for its own types (the orphan
/// rule would block a bare `From<Symbol>`), which lets a mathematics, string, or
/// date/time library of `@`-functions bridge its types.
pub trait ToSymbol {
    /// The ground symbol this value denotes.
    fn to_symbol(&self) -> Symbol;
}

impl ToSymbol for i8 {
    fn to_symbol(&self) -> Symbol {
        Symbol::Number(i32::from(*self))
    }
}
impl ToSymbol for i16 {
    fn to_symbol(&self) -> Symbol {
        Symbol::Number(i32::from(*self))
    }
}
impl ToSymbol for i32 {
    fn to_symbol(&self) -> Symbol {
        Symbol::Number(*self)
    }
}
impl ToSymbol for u8 {
    fn to_symbol(&self) -> Symbol {
        Symbol::Number(i32::from(*self))
    }
}
impl ToSymbol for u16 {
    fn to_symbol(&self) -> Symbol {
        Symbol::Number(i32::from(*self))
    }
}

/// Extract a Rust value from a ground symbol, refusing with the symbol that did not
/// match — a value, not a rendered string (§3.4, spec §1.5).
pub trait FromSymbol: Sized {
    /// Read this value from a ground symbol, or refuse with the offending symbol.
    fn from_symbol(symbol: &Symbol) -> Result<Self, FromSymbolError>;
}

/// The symbol a `FromSymbol` conversion did not match, and the class it expected
/// (§3.4). The offending symbol is carried by value (spec §1.5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FromSymbolError {
    /// The class the conversion expected, in words.
    pub expected: &'static str,
    /// The symbol that did not match.
    pub found: Symbol,
}

/// Read an `i32` from a `Symbol::Number` and narrow it to `T`, refusing the wrong
/// variant or an out-of-range number with the offending symbol (§3.4).
fn from_number<T: TryFrom<i32>>(
    symbol: &Symbol,
    expected: &'static str,
) -> Result<T, FromSymbolError> {
    match symbol {
        Symbol::Number(n) => T::try_from(*n).map_err(|_| FromSymbolError {
            expected,
            found: symbol.clone(),
        }),
        _ => Err(FromSymbolError {
            expected,
            found: symbol.clone(),
        }),
    }
}

impl FromSymbol for i8 {
    fn from_symbol(symbol: &Symbol) -> Result<i8, FromSymbolError> {
        from_number(symbol, "an 8-bit integer")
    }
}
impl FromSymbol for i16 {
    fn from_symbol(symbol: &Symbol) -> Result<i16, FromSymbolError> {
        from_number(symbol, "a 16-bit integer")
    }
}
impl FromSymbol for i32 {
    fn from_symbol(symbol: &Symbol) -> Result<i32, FromSymbolError> {
        from_number(symbol, "an integer")
    }
}
impl FromSymbol for u8 {
    fn from_symbol(symbol: &Symbol) -> Result<u8, FromSymbolError> {
        from_number(symbol, "an 8-bit unsigned integer")
    }
}
impl FromSymbol for u16 {
    fn from_symbol(symbol: &Symbol) -> Result<u16, FromSymbolError> {
        from_number(symbol, "a 16-bit unsigned integer")
    }
}
impl FromSymbol for String {
    fn from_symbol(symbol: &Symbol) -> Result<String, FromSymbolError> {
        match symbol {
            Symbol::String(text) => Ok(text.clone()),
            _ => Err(FromSymbolError {
                expected: "a string",
                found: symbol.clone(),
            }),
        }
    }
}

/// Why a real has no integer symbol (§3.4): it is not finite, or it lies outside the
/// integer range. Carried by the rounding adapters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotAnInteger {
    /// `NaN` or `±∞`.
    NotFinite,
    /// A finite value whose magnitude exceeds the integer range.
    OutOfRange,
}

/// Land a real in the integer domain under the floor rounding (§3.4). `NaN`, `±∞`, and
/// any value outside `Symbol`'s integer range refuse — never a garbage integer. O(1).
pub fn floor(x: f64) -> Result<Symbol, NotAnInteger> {
    round_with(x, f64::floor)
}
/// As `floor`, under the ceiling rounding (§3.4). O(1).
pub fn ceil(x: f64) -> Result<Symbol, NotAnInteger> {
    round_with(x, f64::ceil)
}
/// As `floor`, rounding to the nearest integer, halves away from zero (§3.4). O(1).
pub fn round(x: f64) -> Result<Symbol, NotAnInteger> {
    round_with(x, f64::round)
}
/// As `floor`, truncating toward zero (§3.4). O(1).
pub fn trunc(x: f64) -> Result<Symbol, NotAnInteger> {
    round_with(x, f64::trunc)
}

/// The shared body of the rounding adapters: refuse the non-finite, apply the policy,
/// refuse the out-of-range, else the number (§3.4). The safe replacement for a bare
/// `as` cast, which saturates `NaN`/`±∞` and truncates out-of-range into garbage.
fn round_with(x: f64, policy: fn(f64) -> f64) -> Result<Symbol, NotAnInteger> {
    if !x.is_finite() {
        return Err(NotAnInteger::NotFinite);
    }
    let rounded = policy(x);
    // Both i32 bounds are exact in f64 (below 2^53), so this comparison is exact and
    // the cast below is provably in range — the workspace `cast_possible_truncation`
    // allowance, argued: after the guard the value is a whole number within i32.
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(NotAnInteger::OutOfRange);
    }
    Ok(Symbol::Number(rounded as i32))
}

// §14 / base §8.5 — the std-trait posture on this module's refusals: each states the
// question the caller can fix in `Display`, and composes as `std::error::Error`.

impl std::fmt::Display for NotAnIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is not a legal identifier", self.text)
    }
}
impl std::error::Error for NotAnIdentifier {}

impl std::fmt::Display for NotAVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is not a legal variable", self.text)
    }
}
impl std::error::Error for NotAVariable {}

impl std::fmt::Display for NotAnInteger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAnInteger::NotFinite => f.write_str("not a finite number"),
            NotAnInteger::OutOfRange => f.write_str("outside the integer range"),
        }
    }
}
impl std::error::Error for NotAnInteger {}

impl std::fmt::Display for FromSymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "expected {}, found {:?}", self.expected, self.found)
    }
}
impl std::error::Error for FromSymbolError {}
