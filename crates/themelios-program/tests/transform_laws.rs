//! The transformation surface (docs/design/program.md §9): the read-only visitor and the
//! `Program -> Program` rewriter, each iterative in depth (§13), the rewrite tracing every
//! replaced node back to its origin (§6, §9.1) and canonicalizing its output (§5.1), and the
//! structural-not-semantic boundary it draws (§9.3).

use std::collections::BTreeSet;

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::{Arguments, Atom, Program, Rule, Statement};
use themelios_program::provenance::{Origin, TransformTag};
use themelios_program::raise::raise;
use themelios_program::symbol::{Name, Sign, Signature};
use themelios_program::term::{Term, UnaryOp, Variable};
use themelios_program::transform::{Rewrite, Visit, rewrite, visit};

// ---- harness ----

/// Raise a program from concrete syntax under the clingo dialect — parsed origins on every
/// node, a rich fixture for the walks.
fn raised(text: &str) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    assert!(
        lowered.diagnostics().is_empty(),
        "the fixture raises cleanly: {:?}",
        lowered.diagnostics(),
    );
    lowered.program().clone()
}

/// A program reaching every statement kind and, within a rule, every head and body shape —
/// so an identity rewrite over it exercises every rebuild arm, and a visit every descent.
fn rich() -> Program {
    raised(
        "diss(X) | dist(X) :- p(X).\n\
         q(X) :- p(X), X < 9, cnd(X) : cndg(X), #count { X : ct(X) } >= 1, \
         3 { su(X); sv(X) : sw(X) } 5, not #sum { X : sm(X) } >= 0.\n\
         1 { cha(X) : chb(X) } 4 :- p(X).\n\
         2 #sum { X : hs(X) } 5 :- p(X).\n\
         th(X) :- &sum(X) { X : tt(X) } >= 0, p(X).\n\
         &sum(X) { X : tt(X) } >= 0 :- p(X).\n\
         tf(X) :- p(X), #true, #false.\n\
         #true :- p(X).\n\
         :- p(X).\n\
         :~ p(X). [X@1, X]\n\
         #minimize { X@1, X : mn(X) }.\n\
         #maximize { X : mx(X) }.\n\
         #show.\n\
         #show p/1.\n\
         #show shterm.\n\
         #show shf(X) : shg(X).\n\
         #project q/2.\n\
         #project pr(X) : ps(X).\n\
         #defined d/1.\n\
         #edge (ea, eb) : ec(X).\n\
         #heuristic he(X) : hc(X). [X@1, true]\n\
         #external ex(X) : ecx(X).\n\
         #const co = 1 + 2.\n\
         #include \"foo.lp\".\n\
         #theory tdef { }.\n",
    )
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

// ---- consumers ----

/// A rewrite that changes nothing — the identity up to the transformation tag (§9.1).
struct Identity;
impl Rewrite for Identity {
    fn tag(&self) -> TransformTag {
        TransformTag::new("identity")
    }
}

/// Rename one predicate everywhere it is *derived or depended on* (§9.1): a `rewrite_atom`
/// override, the framework reaching every atom.
struct Rename {
    from: Name,
    to: Name,
}
impl Rewrite for Rename {
    fn tag(&self) -> TransformTag {
        TransformTag::new("rename")
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

/// Fold a ground arithmetic term to its value (§3.5, §9.1): a `rewrite_term` override,
/// applied bottom-up by the framework's fold.
struct FoldArithmetic;
impl Rewrite for FoldArithmetic {
    fn tag(&self) -> TransformTag {
        TransformTag::new("fold-arithmetic")
    }
    fn rewrite_term(&mut self, term: Term) -> Term {
        if matches!(
            term,
            Term::BinaryOperation { .. } | Term::UnaryOperation { .. } | Term::Absolute(_)
        ) && let Ok(symbol) = term.evaluate()
        {
            return Term::Symbolic(symbol);
        }
        term
    }
}

/// Collect every predicate signature the program mentions — a `visit_atom` override.
#[derive(Default)]
struct Signatures {
    seen: Vec<Signature>,
}
impl Visit for Signatures {
    fn visit_atom(&mut self, atom: &Atom) {
        // One signature per argument-list alternative (§8), mirroring the substrate's own
        // `Atom::signatures`; a `Single` atom yields exactly one.
        for tuple in atom.alternatives() {
            self.seen.push(Signature {
                sign: atom.sign,
                name: atom.name.clone(),
                arity: u32::try_from(tuple.len()).expect("small arity"),
            });
        }
    }
}

/// Count every term node — a `visit_term` override.
#[derive(Default)]
struct TermCount {
    count: usize,
}
impl Visit for TermCount {
    fn visit_term(&mut self, _term: &Term) {
        self.count += 1;
    }
}

// ---- laws ----

#[test]
fn the_identity_rewrite_is_the_identity_up_to_provenance() {
    let program = rich();
    assert_eq!(rewrite(program.clone(), &mut Identity), program);
}

#[test]
fn a_rename_pass_rewrites_every_occurrence_and_stays_canonical() {
    // Rename p -> renamed everywhere; no p remains, and the derived predicate names change.
    let program = raised("q(X) :- p(X), r(X) : p(X).\ndiss :- p(1).\n");
    let renamed = rewrite(
        program,
        &mut Rename {
            from: name("p"),
            to: name("renamed"),
        },
    );
    let mut visitor = Signatures::default();
    visit(&renamed, &mut visitor);
    assert!(
        !visitor
            .seen
            .iter()
            .any(|signature| signature.name == name("p")),
        "no p remains after the rename",
    );
    assert!(
        visitor
            .seen
            .iter()
            .any(|signature| signature.name == name("renamed")),
        "the renamed predicate is present",
    );
}

#[test]
fn a_term_pass_folds_ground_arithmetic_bottom_up() {
    // p(1 + 2). folds to p(3). — the argument term evaluated.
    let program = raised("p(1 + 2).\n");
    let folded = rewrite(program, &mut FoldArithmetic);
    let expected = raised("p(3).\n");
    assert_eq!(folded, expected);
}

#[test]
fn a_rewritten_node_traces_back_to_its_origin() {
    // The rewritten statement carrier carries both the transformation tag and the parsed
    // origin it came from, so blame still reaches source (§6, §9.1, §16).
    let program = raised("p(1).\n");
    let transformed = rewrite(program, &mut Identity);
    let statement = transformed.statements().next().expect("the one statement");
    let origins: Vec<&Origin> = statement.provenance().origins().collect();
    assert!(
        origins
            .iter()
            .any(|origin| matches!(origin, Origin::Transformed(tag) if tag.as_str() == "identity")),
        "the transformation tag is recorded: {origins:?}",
    );
    assert!(
        origins
            .iter()
            .any(|origin| matches!(origin, Origin::Parsed(_))),
        "the parsed origin is preserved: {origins:?}",
    );
}

#[test]
fn visit_sees_every_predicate_signature() {
    // q :- p, r. — the head q and the body p, r, each once (three predicate occurrences).
    let program = raised("q :- p, r.\n");
    let mut visitor = Signatures::default();
    visit(&program, &mut visitor);
    let names: BTreeSet<&str> = visitor
        .seen
        .iter()
        .map(|signature| signature.name.as_str())
        .collect();
    assert_eq!(names, BTreeSet::from(["p", "q", "r"]));
    assert_eq!(
        visitor.seen.len(),
        3,
        "each predicate occurrence exactly once"
    );
}

#[test]
fn visit_sees_every_term() {
    // f(g(X), 1) has four term nodes: f(...), g(...), X, 1.
    let program = raised("p(f(g(X), 1)).\n");
    let mut counter = TermCount::default();
    visit(&program, &mut counter);
    assert_eq!(counter.count, 4);
}

#[test]
fn visit_and_rewrite_do_not_overflow_on_a_deep_term() {
    // A ~200,000-deep term nesting, walked and rewritten stack-safely (§13).
    let mut term = Term::Variable(Variable::Named(
        themelios_program::symbol::VarName::new("X").expect("valid"),
    ));
    for _ in 0..200_000 {
        term = Term::UnaryOperation {
            operator: UnaryOp::Negate,
            argument: Box::new(term),
        };
    }
    let program = single_fact_program(term);
    let mut counter = TermCount::default();
    visit(&program, &mut counter);
    assert!(counter.count >= 200_000);
    // A rewrite over it completes too.
    let _ = rewrite(program, &mut Identity);
}

#[test]
fn visit_descends_every_statement_and_head_shape() {
    // Visiting the rich program with a term counter runs the full read-only descent — every
    // statement kind, every head and body shape, every aggregate and directive.
    let mut counter = TermCount::default();
    visit(&rich(), &mut counter);
    assert!(counter.count > 0, "the rich program has terms to visit");
}

#[test]
fn a_query_is_visited_and_rewritten() {
    use themelios_program::program::Query;
    use themelios_program::provenance::WithProvenance;
    use themelios_program::symbol::VarName;

    let variable = Term::Variable(Variable::Named(VarName::new("X").expect("valid")));
    let query = Statement::Query(Query::new(Atom::new(name("qp"), [variable])));
    let program = Program::of([WithProvenance::constructed(query)]);

    // The visitor reaches the query's atom.
    let mut visitor = Signatures::default();
    visit(&program, &mut visitor);
    assert!(
        visitor
            .seen
            .iter()
            .any(|signature| signature.name == name("qp"))
    );

    // The rewrite reaches it too — rename qp -> qr.
    let renamed = rewrite(
        program,
        &mut Rename {
            from: name("qp"),
            to: name("qr"),
        },
    );
    let mut after = Signatures::default();
    visit(&renamed, &mut after);
    assert!(
        after
            .seen
            .iter()
            .any(|signature| signature.name == name("qr"))
    );
}

#[test]
fn a_visitor_overriding_nothing_descends_by_default() {
    // Every default method runs, the leaf visit_term included, reaching the whole program.
    struct Silent;
    impl Visit for Silent {}
    visit(&rich(), &mut Silent);
}

#[test]
fn rewrite_rebuilds_a_multi_step_comparison() {
    use themelios_program::program::{
        Body, BodyElement, Comparison, DefaultNegation, Literal, LiteralInner, Relation,
    };
    use themelios_program::provenance::WithProvenance;
    use themelios_program::symbol::VarName;

    let x = || Term::Variable(Variable::Named(VarName::new("X").expect("valid")));
    // 1 < X < 9 — a chain of more than one step.
    let comparison =
        Comparison::new(Term::from(1), Relation::Lt, x()).chain(Relation::Lt, Term::from(9));
    let literal = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(comparison)),
    };
    let rule = Rule::new(
        Atom::new(name("mc"), [x()]),
        Body::new([BodyElement::Literal(literal)]),
    );
    let program = Program::of([WithProvenance::constructed(Statement::Rule(rule))]);
    assert_eq!(rewrite(program.clone(), &mut Identity), program);
}

/// A one-fact program `p(<term>).` built through the primitive door — for the depth law,
/// where a term is too deep to spell.
fn single_fact_program(argument: Term) -> Program {
    use themelios_program::provenance::WithProvenance;
    let atom = Atom {
        sign: Sign::Positive,
        name: name("p"),
        arguments: Arguments::Single(vec![argument]),
    };
    let rule = Rule::fact(atom);
    Program::of([WithProvenance::constructed(Statement::Rule(rule))])
}
