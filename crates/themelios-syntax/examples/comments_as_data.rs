//! The syntax tier's seed of the *comments-as-data* witness
//! (docs/specification.md §3): a program bearing comments is parsed;
//! each comment and its attachment — trailing, leading, dangling — is
//! retrieved through the public API; the tree's text is the input, byte
//! for byte, so an emit preserves every comment.

use themelios_syntax::attach::{Slot, attachment, attachments, comments};
use themelios_syntax::base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;
use themelios_syntax::tree::{TokenRole, role};

fn main() {
    let text = "% every route is a road or a rail\nroute(X, Y) :- road(X, Y). % roads\nroute(X, Y) :- rail(X, Y). % rails\n\n% unreachable? not from here\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the program admits");
    let parse = parse(&source, Dialect::Clingo);
    assert!(!parse.has_errors());
    assert_eq!(parse.syntax().text(), text, "emit preserves every byte");
    let root = parse.syntax();
    let mut seen = 0;
    for (comment, att) in attachments(&root) {
        seen += 1;
        assert_eq!(role(&comment), TokenRole::Trivia);
        assert_eq!(attachment(&comment).as_ref(), Ok(&att));
        println!(
            "{:?} -> {:?} of {}",
            comment.text(),
            att.slot,
            att.anchor.kind()
        );
    }
    assert_eq!(seen, 4);
    let program = themelios_syntax::tree::SyntaxElement::Node(root.clone());
    assert_eq!(
        comments(&program, Slot::Dangling).count(),
        1,
        "the comment past the blank line dangles in the program"
    );
    let first_rule =
        themelios_syntax::tree::SyntaxElement::Node(root.children().next().expect("a rule"));
    assert_eq!(comments(&first_rule, Slot::Leading).count(), 1);
    assert_eq!(comments(&first_rule, Slot::Trailing).count(), 1);
}
