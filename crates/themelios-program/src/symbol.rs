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

use crate::render::Unspellable;

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
/// while its depth is the heap's (the depth proof, §16).
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

impl Signature {
    /// The identity of the given sign, name, and arity — a flat value with no term to
    /// canonicalize, so the constructor is the struct literal, given so that every family
    /// a program carries has its `new` (§7.1). O(1).
    pub fn new(sign: Sign, name: Name, arity: u32) -> Signature {
        Signature { sign, name, arity }
    }
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

// ---- value spelling: the value's own concrete syntax, through render's one printer (§10) ----

impl Symbol {
    /// Spell this symbol to concrete syntax under a dialect (§10): the value's own text, the
    /// way [`render`](crate::render::render) writes it inside a program, through that one
    /// printer (§10) — no second speller, so a lone value and a rendered program cannot
    /// drift. Total but for the one [`Unspellable`] refusal: a string value the dialect
    /// cannot spell (grammar §4.4/§6.2/§9). A `Symbolic` [`Term`](crate::term::Term) spells
    /// identically to the symbol it holds. `O(output)`.
    pub fn spell(&self, dialect: Dialect) -> Result<String, Unspellable> {
        crate::render::spell_symbol(self, dialect)
    }
}

impl Symbol {
    /// A constant `c` — the empty-argument function (§3.1), its own named
    /// constructor so a simple thing stays simple (§7.1). Total (§7.2): the name
    /// is an already validated identifier. Canonical by construction (§5.1) — a
    /// symbol has no canonicalization pass — so this is the positive `Function`
    /// over no arguments built directly: exactly [`function`](Symbol::function)
    /// over no arguments under `Sign::Positive`. O(1).
    pub fn constant(name: Name) -> Symbol {
        Symbol::Function {
            name,
            arguments: Vec::new(),
            sign: Sign::Positive,
        }
    }

    /// A function or predicate `f(s, …)` under its strong sign — `-f(s, …)`
    /// under `Sign::Negative` (§3.1, §7.1). Total (§7.2): the name is an already
    /// validated identifier and the arguments are ground symbols. Canonical by
    /// construction (§5.1): the variant is built directly, its arguments
    /// collected as given and never re-walked. O(arguments).
    pub fn function(name: Name, arguments: impl IntoIterator<Item = Symbol>, sign: Sign) -> Symbol {
        Symbol::Function {
            name,
            arguments: arguments.into_iter().collect(),
            sign,
        }
    }

    /// A tuple `(s, …)` — the anonymous functor over its elements, the
    /// one-element `(s,)` and the empty `()` included (§3.1, §7.1). Total (§7.2).
    /// Canonical by construction (§5.1): the variant built directly, its elements
    /// collected as [`function`](Symbol::function) collects its arguments.
    /// O(elements).
    pub fn tuple(elements: impl IntoIterator<Item = Symbol>) -> Symbol {
        Symbol::Tuple(elements.into_iter().collect())
    }

    /// A number — `i32`, the engine's own width (§3.1, §7.1). Total; a leaf,
    /// canonical by construction (§5.1). O(1).
    pub fn number(value: i32) -> Symbol {
        Symbol::Number(value)
    }

    /// A string (§3.1, §7.1). Total; a leaf, canonical by construction (§5.1).
    /// O(1) given an owned `String`, O(text) to own a borrowed one.
    pub fn string(text: impl Into<String>) -> Symbol {
        Symbol::String(text.into())
    }
}

// ---- Coercion widens the one obvious spelling; it never adds a second (§7.1) ----

impl From<i32> for Symbol {
    /// A number is a symbol (§3.1, §3.4): `i32`, the engine's own width, lifted
    /// to the `Number` leaf — [`Symbol::number`], and the twin of `From<i32>` for
    /// `Term`. The other integers reach a symbol through their `ToSymbol` (§3.4),
    /// the door lossless-inward: the engine width and narrower; a wider one has no
    /// silent door — the caller narrows it checked and states the intent (§3.4).
    /// This widens the one obvious spelling, it does not add a second.
    fn from(value: i32) -> Symbol {
        Symbol::number(value)
    }
}

impl From<&str> for Symbol {
    /// A string literal is a symbol (§3.1, §3.4): the text owned into the
    /// `String` leaf — [`Symbol::string`]. This widens the one obvious spelling,
    /// it does not add a second.
    fn from(text: &str) -> Symbol {
        Symbol::string(text)
    }
}

impl From<String> for Symbol {
    /// An owned string is a symbol (§3.1, §3.4): the `&str` coercion's O(1)
    /// twin, the text moved into the `String` leaf without a copy —
    /// [`Symbol::string`], and the twin of `From<String>` for `Term`, so the two
    /// types take the same scalars.
    fn from(text: String) -> Symbol {
        Symbol::string(text)
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

/// A Rust value that denotes a ground symbol (§3.4). The conversion surface is a
/// pair of bespoke traits — `ToSymbol` and `FromSymbol` (below) — rather than
/// `From`/`Into`/`TryFrom`, because the trait shape is what the job needs: the
/// conversion is a denotation read by reference (`&self` here, `&Symbol` on the
/// way back), where `From`/`Into` consume by value; that by-ref shape is what
/// admits `impl ToSymbol for str`, since the unsized `str` cannot be the by-value
/// `T` of `From<T>`; `FromSymbol` refuses with one fixed `FromSymbolError`, where
/// each `TryFrom` impl would declare its own associated `Error`; a trait this
/// crate owns leaves room for blanket impls that `std`'s `From`/`Into` blankets
/// would otherwise collide with; and a later interner or context can be threaded
/// through its methods. The relation it names is a KR one — *this value denotes
/// this ground term*.
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
impl ToSymbol for str {
    fn to_symbol(&self) -> Symbol {
        Symbol::String(self.to_owned())
    }
}
impl ToSymbol for String {
    fn to_symbol(&self) -> Symbol {
        self.as_str().to_symbol()
    }
}

/// Extract a Rust value from a ground symbol, refusing with the symbol that did not
/// match — a value, not a rendered string (§3.4, spec §1.5).
pub trait FromSymbol: Sized {
    /// Read this value from a ground symbol, or refuse with the offending symbol.
    fn from_symbol(symbol: &Symbol) -> Result<Self, FromSymbolError>;
}

/// One step of a decode locus (§3.4): a position of an enclosing symbol a
/// `FromSymbol` conversion had descended into when it refused. Positional today —
/// `Argument(i)` is the `i`-th argument of a function or tuple. `#[non_exhaustive]`
/// leaves room for the deferred field/kind taxonomy without breaking a downstream
/// `match`. Derives `Serialize`/`Deserialize` under the `serde` feature, so the
/// locus crosses a service boundary as structured data.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Segment {
    /// The `i`-th positional argument of the enclosing symbol.
    Argument(usize),
}

/// The symbol a `FromSymbol` conversion did not match, the class it expected, and the
/// locus of the offending subsymbol (§3.4). The offending symbol is carried by value
/// (spec §1.5). `#[non_exhaustive]`: the shape grows — a richer locus, an added field —
/// without breaking a downstream, which reads its fields and builds it through the
/// factories below, never a struct literal.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FromSymbolError {
    /// The class the conversion expected, in words.
    pub expected: &'static str,
    /// The symbol that did not match.
    pub found: Symbol,
    /// The locus: the positional path from the root symbol to the offending
    /// subsymbol, outer→inner. Empty when the root itself did not match.
    pub path: Vec<Segment>,
}

impl FromSymbolError {
    /// A root mismatch: the `expected` class was not the `found` symbol, with an empty
    /// locus path (§3.4). The factory `#[non_exhaustive]` routes construction through,
    /// so the shape can grow without touching a call site. O(1).
    pub fn mismatch(expected: &'static str, found: Symbol) -> FromSymbolError {
        FromSymbolError {
            expected,
            found,
            path: Vec::new(),
        }
    }

    /// Situate this refusal within the `index`-th argument of an enclosing symbol,
    /// prepending `Argument(index)` so the locus reads outer→inner to the offending
    /// subsymbol (§3.4): a compound decoder that recurses into an argument and catches a
    /// child refusal calls this to record the descent. `O(path length)` — a decode nests
    /// only as deep as the term it reads, and the shipped decoders are flat.
    #[must_use]
    pub fn within_argument(mut self, index: usize) -> FromSymbolError {
        self.path.insert(0, Segment::Argument(index));
        self
    }
}

/// Read an `i32` from a `Symbol::Number` and narrow it to `T`, refusing the wrong
/// variant or an out-of-range number with the offending symbol (§3.4).
fn from_number<T: TryFrom<i32>>(
    symbol: &Symbol,
    expected: &'static str,
) -> Result<T, FromSymbolError> {
    match symbol {
        Symbol::Number(n) => {
            T::try_from(*n).map_err(|_| FromSymbolError::mismatch(expected, symbol.clone()))
        }
        _ => Err(FromSymbolError::mismatch(expected, symbol.clone())),
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
            _ => Err(FromSymbolError::mismatch("a string", symbol.clone())),
        }
    }
}
impl FromSymbol for Name {
    fn from_symbol(symbol: &Symbol) -> Result<Name, FromSymbolError> {
        // The inverse of `Symbol::constant` (§3.4): a positive nullary function is a
        // constant, and its name is the decode. Anything else refuses — a function with
        // arguments is not a bare constant, and a negated nullary would drop its strong
        // sign, the "repair" spec §5.2 forbids.
        match symbol {
            Symbol::Function {
                name,
                arguments,
                sign: Sign::Positive,
            } if arguments.is_empty() => Ok(name.clone()),
            _ => Err(FromSymbolError::mismatch("a constant", symbol.clone())),
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

#[cfg(test)]
mod tests {
    use super::{FromSymbol, FromSymbolError, Name, Segment, Sign, Signature, Symbol, ToSymbol};

    fn name(text: &str) -> Name {
        Name::new(text).expect("a valid identifier")
    }

    // ---- The builders (§7.1): total, canonical by construction (§5.1) ----

    #[test]
    fn constant_is_the_positive_nullary_function() {
        // A constant is the empty-argument function (§3.1), built directly: a symbol has
        // no canonicalization pass to collapse a raw spelling (§5.1).
        assert_eq!(
            Symbol::constant(name("c")),
            Symbol::Function {
                name: name("c"),
                arguments: Vec::new(),
                sign: Sign::Positive,
            }
        );
    }

    #[test]
    fn constant_equals_function_over_no_arguments() {
        // The constant/empty-function identity (§3.1): one value, two spellings.
        assert_eq!(
            Symbol::constant(name("c")),
            Symbol::function(name("c"), [], Sign::Positive)
        );
    }

    #[test]
    fn function_carries_its_name_arguments_and_sign() {
        assert_eq!(
            Symbol::function(name("f"), [Symbol::number(1)], Sign::Negative),
            Symbol::Function {
                name: name("f"),
                arguments: vec![Symbol::Number(1)],
                sign: Sign::Negative,
            }
        );
    }

    #[test]
    fn tuple_holds_its_elements_in_order() {
        assert_eq!(
            Symbol::tuple([Symbol::number(1), Symbol::number(2)]),
            Symbol::Tuple(vec![Symbol::Number(1), Symbol::Number(2)])
        );
    }

    #[test]
    fn number_is_the_number_leaf() {
        assert_eq!(Symbol::number(7), Symbol::Number(7));
    }

    #[test]
    fn string_is_the_string_leaf() {
        assert_eq!(Symbol::string("a"), Symbol::String("a".to_owned()));
    }

    // ---- The scalar coercions (§7.1): the one obvious spelling, widened ----

    #[test]
    fn an_i32_coerces_to_its_number() {
        assert_eq!(Symbol::from(1), Symbol::Number(1));
        assert_eq!(Symbol::from(1), Symbol::number(1));
    }

    #[test]
    fn a_str_coerces_to_its_string() {
        assert_eq!(Symbol::from("a"), Symbol::String("a".to_owned()));
        assert_eq!(Symbol::from("a"), Symbol::string("a"));
    }

    #[test]
    fn an_owned_string_coerces_to_its_string() {
        // The owned and the borrowed string coercions are two doors to one value (§7.1).
        assert_eq!(
            Symbol::from(String::from("a")),
            Symbol::String("a".to_owned())
        );
        assert_eq!(Symbol::from(String::from("a")), Symbol::from("a"));
        assert_eq!(Symbol::from(String::from("a")), Symbol::string("a"));
    }

    #[test]
    fn the_coercions_agree_with_to_symbol() {
        // The encode coercion and the denotation trait (§3.4) are two doors to one value.
        assert_eq!(Symbol::from(1), 1_i32.to_symbol());
        assert_eq!(Symbol::from("a"), "a".to_symbol());
        assert_eq!(
            Symbol::from(String::from("a")),
            String::from("a").to_symbol()
        );
    }

    // ---- The signature (§3.7): the identity from its parts (§7.1) ----

    #[test]
    fn signature_new_builds_the_identity_from_its_parts() {
        // A flat identity with no term to canonicalize: the constructor is the struct literal,
        // given so that every family a program carries has its `new` (§7.1).
        assert_eq!(
            Signature::new(Sign::Positive, name("p"), 2),
            Signature {
                sign: Sign::Positive,
                name: name("p"),
                arity: 2,
            }
        );
    }

    // ---- The conversion refusal and its locus (§3.4): the evolvable error ----

    /// A test-only recursive decoder standing in for a future compound `FromSymbol`
    /// (the shipped decoders are flat): every leaf must be a number, and the first
    /// leaf that is not refuses through the root factory, each enclosing argument
    /// prepending its position, so the locus reads outer→inner to the offending
    /// subsymbol (§3.4). Exercises `mismatch` and `within_argument` together.
    fn require_all_numbers(symbol: &Symbol) -> Result<(), FromSymbolError> {
        match symbol {
            Symbol::Number(_) => Ok(()),
            Symbol::Function { arguments, .. } => {
                for (i, argument) in arguments.iter().enumerate() {
                    require_all_numbers(argument).map_err(|error| error.within_argument(i))?;
                }
                Ok(())
            }
            _ => Err(FromSymbolError::mismatch("a number", symbol.clone())),
        }
    }

    #[test]
    fn a_root_mismatch_carries_an_empty_locus_path() {
        // A refusal at the root: the whole symbol did not match, so the locus is empty (§3.4).
        let found = Symbol::string("x");
        let error = FromSymbolError::mismatch("an integer", found.clone());
        assert_eq!(error.expected, "an integer");
        assert_eq!(error.found, found);
        assert!(error.path.is_empty());
    }

    #[test]
    fn a_flat_decoder_refuses_at_the_root() {
        // The shipped scalar decoders never recurse, so their refusal is a root one (§3.4).
        let text = Symbol::string("x");
        let error = i32::from_symbol(&text).expect_err("a string is not a number");
        assert_eq!(error.found, text);
        assert!(error.path.is_empty());
    }

    #[test]
    fn a_nested_decode_prepends_each_argument_position_outer_to_inner() {
        // f(0, g(1, 2, "boom")): the refusal is the string at g's argument 2, reached through
        // f's argument 1 — so the locus reads [Argument(1), Argument(2)], outer→inner (§3.4).
        let boom = Symbol::string("boom");
        let inner = Symbol::function(
            name("g"),
            [Symbol::number(1), Symbol::number(2), boom.clone()],
            Sign::Positive,
        );
        let outer = Symbol::function(name("f"), [Symbol::number(0), inner], Sign::Positive);
        let error = require_all_numbers(&outer).expect_err("the string leaf refuses");
        assert_eq!(error.found, boom);
        assert_eq!(error.expected, "a number");
        assert_eq!(error.path, vec![Segment::Argument(1), Segment::Argument(2)]);
    }

    #[test]
    fn a_constant_decodes_to_its_name() {
        // `FromSymbol for Name` inverts `Symbol::constant` (§3.4): the positive nullary
        // function decodes back to the name it was built from.
        let constant = Symbol::constant(name("c"));
        assert_eq!(Name::from_symbol(&constant), Ok(name("c")));
    }

    #[test]
    fn a_non_constant_refuses_the_name_decode_carrying_the_symbol() {
        // A number is not a constant — refuse at the root, carrying the offending symbol (§3.4).
        let number = Symbol::number(7);
        let error = Name::from_symbol(&number).expect_err("a number is not a constant");
        assert_eq!(error.found, number);
        assert!(error.path.is_empty());

        // A function with arguments is not a bare constant.
        let applied = Symbol::function(name("f"), [Symbol::number(1)], Sign::Positive);
        assert!(Name::from_symbol(&applied).is_err());

        // A negated nullary would drop its strong sign — refuse over repair (§3.4, spec §5.2).
        let negated = Symbol::function(name("c"), [], Sign::Negative);
        assert!(Name::from_symbol(&negated).is_err());
    }

    #[test]
    fn the_refusal_displays_its_expectation_and_the_found_symbol() {
        // `#[non_exhaustive]` leaves the in-crate `Display`/`Error` impls and construction
        // through the factory working (§3.4): the message states the class and the symbol.
        let error = FromSymbolError::mismatch("an integer", Symbol::string("x"));
        let shown = error.to_string();
        assert!(
            shown.contains("an integer"),
            "names the expected class: {shown}"
        );
        assert!(shown.contains('x'), "shows the found symbol: {shown}");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_locus_path_survives_a_serde_json_round_trip() {
        // The locus is structured data a downstream can carry across a service boundary (§3.4):
        // the path and its segments serialize and deserialize back to the same value.
        let path = vec![Segment::Argument(1), Segment::Argument(2)];
        let json = serde_json::to_string(&path).expect("the path serializes");
        let restored: Vec<Segment> = serde_json::from_str(&json).expect("the path deserializes");
        assert_eq!(restored, path);
    }
}
