//! The witness seeds this tier owns, consolidated (docs/specification.md §3; program §16). The
//! three scenarios below are the named witnesses of the roster this tier can discharge today,
//! gathered here as the checked laws that run on every change; `examples/{first_solve,
//! round_trip,transformation}.rs` are the same seeds as runnable narratives. A witness carries a
//! **name**, not an ordinal (spec §3), so it is cited by name throughout.
//!
//! - ***first-solve*** (construction half): one program built two ways is one value.
//! - ***round-trip***: render, parse, and raise return the same program up to provenance.
//! - ***transformation***: a `Program → Program` rewrite carries provenance to the origins.

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::construct::not;
use themelios_program::program::{
    Arguments, Atom, Body, BodyElement, DefaultNegation, Head, IntoHead, Literal, LiteralInner,
    Program, Rule, Statement,
};
use themelios_program::provenance::{Origin, TransformTag, WithProvenance};
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, Variable};
use themelios_program::transform::{Rewrite, rewrite};

// =====================================================================================
// first-solve (the construction half, spec §3 item 1, front-matter flag 2)
// =====================================================================================

/// ***first-solve*** — the construction half. One small reachability program, built through the
/// spelled-out declarative surface a logician writes and through the primitive constructors a
/// generator or a language model targets, is **one and the same value**: both doors validate
/// through the one ingest authority (program §6.3, §7.3), so "written as ASP" and "assembled
/// node by node" meet at a single `Program`.
///
/// This is the *construction* half of the seed. Its two remaining halves are **named here, not
/// silently dropped** — they cannot be asserted yet:
///  - the **macro-versus-constructor** half — that a program built through the `atom!`/`rule!`/…
///    macros equals these — arrives with `themelios-macros` (a later tier, spec §8);
///  - the **ground/solve/read-back** half — grounding the program, solving it, and reading the
///    answer sets back as owned typed values — arrives at the solve tier (spec §11 item 8, §3
///    item 1).
///
/// Together the three will hold "written as ASP", "declared in Rust", and "assembled node by
/// node" to be one program, then run it. This tier owns and checks the first.
#[test]
fn first_solve_the_two_construction_doors_reach_one_value() {
    let declarative = through_the_surface();
    let primitive = through_the_primitives();
    assert_eq!(
        declarative, primitive,
        "the declarative surface and the primitive constructors build one program",
    );
    assert_eq!(
        declarative.base().statements().count(),
        5,
        "the reachability program is five statements",
    );
}

/// The program as a logician declares it — the shape of each expression mirrors the shape of the
/// rule (`edge(1, 2).`, `reach(X, Y) :- edge(X, Y).`, …, `:- not edge(1, 2).`).
fn through_the_surface() -> Program {
    let edge = Rule::fact(Atom::new(name("edge"), [Term::from(1), Term::from(2)]));

    let reach_from_edge = Atom::new(name("reach"), [tvar("X"), tvar("Y")])
        .into_head()
        .when(Atom::new(name("edge"), [tvar("X"), tvar("Y")]));

    let reach_transitively = Atom::new(name("reach"), [tvar("X"), tvar("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [tvar("X"), tvar("Y")]),
            Atom::new(name("edge"), [tvar("Y"), tvar("Z")]),
        ]);

    let step = Atom::new(name("step"), [tvar("X"), tvar("X") + 1])
        .into_head()
        .when(Atom::new(name("edge"), [tvar("X"), tvar("Y")]));

    let edge_must_hold =
        Rule::constraint(not(Atom::new(name("edge"), [Term::from(1), Term::from(2)])));

    program_of([
        edge,
        reach_from_edge,
        reach_transitively,
        step,
        edge_must_hold,
    ])
}

/// The same program as a generator assembles it — explicit heads, bodies, and literals over the
/// typed algebra. It reaches the identical value.
fn through_the_primitives() -> Program {
    let edge = Rule::new(head("edge", vec![num(1), num(2)]), Body::empty());

    let reach_from_edge = Rule::new(
        head("reach", vec![tvar("X"), tvar("Y")]),
        Body::new([body_atom("edge", vec![tvar("X"), tvar("Y")])]),
    );

    let reach_transitively = Rule::new(
        head("reach", vec![tvar("X"), tvar("Z")]),
        Body::new([
            body_atom("reach", vec![tvar("X"), tvar("Y")]),
            body_atom("edge", vec![tvar("Y"), tvar("Z")]),
        ]),
    );

    let step = Rule::new(
        head("step", vec![tvar("X"), add(tvar("X"), num(1))]),
        Body::new([body_atom("edge", vec![tvar("X"), tvar("Y")])]),
    );

    let edge_must_hold = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Literal(Literal {
            negation: DefaultNegation::Not,
            inner: LiteralInner::Atom(WithProvenance::constructed(raw_atom(
                "edge",
                vec![num(1), num(2)],
            ))),
        })]),
    );

    program_of([
        edge,
        reach_from_edge,
        reach_transitively,
        step,
        edge_must_hold,
    ])
}

// =====================================================================================
// round-trip (spec §3 item 10)
// =====================================================================================

/// ***round-trip*** — for a program this renderer covers, `raise(parse(render(P, d), d)) == P`
/// up to provenance (program §10, spec §7.6): render, parse, and raise return the same program.
/// The canonical form makes it hold by construction — binary operators and intervals render
/// fully parenthesized, so the tree's grouping is carried in the text with no precedence to
/// re-derive on the way back.
///
/// **The two named exceptions of §10** ride outside this witness, stated not discovered: the
/// authority's own unparse is non-injective on the pair of forms an *empty aggregate* can take
/// (`#count {}` with one empty element versus none), and the *theory* carve-out of §5 makes the
/// reparse up-to-grounding for theory-bearing programs. The program below carries arithmetic, a
/// comparison, and an interval — the round-trippable core — and **neither** exception, so the
/// identity is exact. The authority half of the law (the rendered text parsed by the pinned
/// engine) is the clingo differential (`tests/differential.rs`); this is the estate-parser
/// fixpoint half.
#[test]
fn round_trip_render_parse_raise_returns_the_same_program() {
    let text = "reachable(a).\n\
                reachable(Y) :- reachable(X), edge(X, Y).\n\
                step(N + 1) :- step(N), N < 10.\n\
                cell(1 .. 3).\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    let program = raise(&parse(&source, Dialect::Clingo)).program().clone();

    let rendered = render(&program, Dialect::Clingo).expect("the program renders");

    let reparsed_source = Source::new(SourceId::new(0), rendered).expect("the rendering admits");
    let reparsed = raise(&parse(&reparsed_source, Dialect::Clingo))
        .program()
        .clone();

    assert_eq!(
        program, reparsed,
        "render, parse, and raise return the same program up to provenance",
    );
}

// =====================================================================================
// transformation (spec §3 item 9)
// =====================================================================================

/// ***transformation*** — a `Program → Program` rewrite whose provenance reaches the origins.
/// A rename of one predicate everywhere it occurs is shown to (a) reach every occurrence and
/// (b) carry provenance through: every rewritten node records the transformation that produced
/// it *and* keeps the source span it was parsed from, so **a diagnostic on a rewritten rule
/// still points at the text it came from** (program §9.3). This is the structural half of the
/// promise; a rename's effect on answer sets is the modeler's to know, not this tier's to
/// certify.
#[test]
fn transformation_carries_provenance_from_the_rewrite_to_the_source() {
    let text = "reach(X, Y) :- edge(X, Y).\nreach(X, Z) :- reach(X, Y), edge(Y, Z).\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    let program = raise(&parse(&source, Dialect::Clingo)).program().clone();

    let mut rename = Rename {
        from: name("edge"),
        to: name("link"),
        tag: TransformTag::new("rename-edge-to-link"),
    };
    let transformed = rewrite(program, &mut rename);

    // The rename reached every occurrence: no `edge` remains, and `link` is present.
    assert!(
        !mentions(&transformed, "edge"),
        "the rename reaches every occurrence of edge",
    );
    assert!(
        mentions(&transformed, "link"),
        "the renamed predicate link is present",
    );

    // Every rewritten statement records the transformation *and* keeps the parsed origin, so
    // blame still reaches the source (§9.3) — the diagnostic-points-at-source guarantee.
    let mut traced = 0;
    for statement in transformed.statements() {
        let origins: Vec<&Origin> = statement.provenance().origins().collect();
        let has_tag = origins.iter().any(|origin| {
            matches!(origin, Origin::Transformed(tag) if tag.as_str() == "rename-edge-to-link")
        });
        let reaches_source = origins
            .iter()
            .any(|origin| matches!(origin, Origin::Parsed(_)));
        assert!(
            has_tag && reaches_source,
            "a rewritten rule records the transform and still traces to its source span",
        );
        traced += 1;
    }
    assert_eq!(
        traced, 2,
        "both reachability rules were rewritten and traced"
    );
}

/// Rename one predicate everywhere — a `rewrite_atom` override, the framework reaching every
/// atom and carrying provenance through (§9.1).
struct Rename {
    from: Name,
    to: Name,
    tag: TransformTag,
}

impl Rewrite for Rename {
    fn tag(&self) -> TransformTag {
        self.tag.clone()
    }
    fn rewrite_atom(&mut self, atom: Atom) -> Atom {
        if atom.name == self.from {
            Atom {
                sign: atom.sign,
                name: self.to.clone(),
                arguments: atom.arguments,
            }
        } else {
            atom
        }
    }
}

/// Whether a program derives or depends on a predicate — enough to see the rename took.
fn mentions(program: &Program, predicate: &str) -> bool {
    let target = name(predicate);
    program.statements().any(|statement| {
        let Statement::Rule(rule) = statement.get() else {
            return false;
        };
        let mut names = Vec::new();
        if let Head::Literal(literal) = rule.head().get() {
            push_atom_name(literal, &mut names);
        }
        for element in rule.body().get().elements() {
            if let BodyElement::Literal(literal) = element.get() {
                push_atom_name(literal, &mut names);
            }
        }
        names.into_iter().any(|found| found == &target)
    })
}

fn push_atom_name<'a>(literal: &'a Literal, names: &mut Vec<&'a Name>) {
    if let LiteralInner::Atom(atom) = &literal.inner {
        names.push(&atom.get().name);
    }
}

// =====================================================================================
// construction helpers (the estate's vocabulary, shared by first-solve)
// =====================================================================================

fn program_of(rules: [Rule; 5]) -> Program {
    Program::of(rules.map(|rule| WithProvenance::constructed(Statement::Rule(rule))))
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn tvar(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable"),
    ))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn add(left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn raw_atom(predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    }
}

fn head(predicate: &str, arguments: Vec<Term>) -> Head {
    Head::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(raw_atom(predicate, arguments))),
    })
}

fn body_atom(predicate: &str, arguments: Vec<Term>) -> BodyElement {
    BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(raw_atom(predicate, arguments))),
    })
}
