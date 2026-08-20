//! The parse half of the *asp-core-2* witness (docs/specification.md
//! §3): a conformant ASP-Core-2 program — query included — parsed under
//! the declared dialect; the query stands as the program's last
//! statement; the standard's string reading holds.

use themelios_syntax::ast::{Constant, Head, LiteralInner, Statement, Term};
use themelios_syntax::base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn main() {
    let text = "node(1..3). edge(1,2). edge(2,3).\nreach(X) :- node(X), start(X).\nreach(Y) :- reach(X), edge(X,Y).\nlabel(\"a\\b\").\nstart(1).\nreach(3)?\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let parse = parse(&source, Dialect::AspCore2);
    assert!(
        !parse.has_errors(),
        "a conformant program is a member: {:?}",
        parse.diagnostics()
    );
    let statements: Vec<Statement> = parse.tree().statements().collect();
    assert!(
        matches!(statements.last(), Some(Statement::Query(_))),
        "the query holds the last position"
    );
    let Some(Statement::Rule(label)) = statements.get(5) else {
        panic!("the labelled fact")
    };
    let Some(Head::Literal(literal)) = label.head() else {
        panic!("a literal head")
    };
    let Some(LiteralInner::Atom(atom)) = literal.inner() else {
        panic!("an atom")
    };
    let tuple = atom
        .arguments()
        .expect("arguments")
        .alternatives()
        .next()
        .expect("a tuple");
    let Some(Term::Constant(constant)) = tuple.terms().next() else {
        panic!("a constant")
    };
    let Some(Constant::String(string)) = constant.constant() else {
        panic!("a string")
    };
    assert_eq!(
        parse.string_value(&string).expect("valid"),
        "a\\b",
        "the standard's string rule: a backslash is itself"
    );
    println!(
        "parsed {} statements under the ASP-Core-2 dialect, the query last",
        statements.len()
    );
}
