//! The syntax half of the *hostile-input* witness
//! (docs/specification.md §3, §12.4): the public surface meets
//! adversarial, malformed, and absurdly deep input the way a service
//! boundary receives it — bytes that are not UTF-8 refused at admission
//! with a typed refusal; a megabyte of what begins no token, one token
//! and one diagnostic; nesting past the constant, one refusal with a
//! locus; every unterminated construct, a typed diagnostic; no panic.

use themelios_syntax::base::source::{FromBytesRefusal, Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{NestingLimit, parse};

fn main() {
    let refused = Source::from_bytes(SourceId::new(0), vec![0xff, 0xfe, b'p', b'.']);
    assert!(matches!(refused, Err(FromBytesRefusal::InvalidUtf8(_))));

    let hostile = "$".repeat(1024 * 1024);
    let source = Source::new(SourceId::new(1), hostile.clone()).expect("admits");
    let big = parse(&source, Dialect::Clingo);
    assert_eq!(big.syntax().text(), hostile.as_str());
    assert_eq!(
        big.diagnostics().len(),
        1,
        "one error token, one diagnostic"
    );

    // Nesting past the crash-averse floor a bare `parse` holds to
    // (`NestingLimit::DEFAULT`, docs/design/syntax.md §6.6): one refusal
    // with a locus, and the rest of the statement carried unparsed — never
    // a crash. The frame count `parse` refuses beyond is the floor's; one
    // more `f(` than that — atop the atom's own bracket — is over it.
    let depth = NestingLimit::DEFAULT.frames() as usize + 1;
    let deep = format!("p({}x{}).", "f(".repeat(depth), ")".repeat(depth));
    let source = Source::new(SourceId::new(2), deep.clone()).expect("admits");
    let nested = parse(&source, Dialect::Clingo);
    assert_eq!(nested.syntax().text(), deep.as_str());
    assert_eq!(nested.diagnostics().len(), 1);
    assert_eq!(nested.diagnostics()[0].id().name(), "nesting-too-deep");

    // Every unterminated bracket, comment, and script region is diagnosed
    // and read as incomplete — the REPL's "read more" (§6.5), not a wrong
    // program.
    for text in ["%* open", "#script (lua) x = 1", "p(X) :- q(X", "&a { x"] {
        let source = Source::new(SourceId::new(3), text.to_owned()).expect("admits");
        let parse = parse(&source, Dialect::Clingo);
        assert!(parse.has_errors());
        assert!(parse.is_incomplete(), "{text:?} is incomplete, not wrong");
    }

    // A string is the one construct whose incompleteness is a dialect fact
    // (§6.5): under ASP-Core-2 a string may span lines, so an unterminated
    // one is incomplete — it may yet close on a later line; under clingo it
    // may not, so the same bytes are a wrong program. Both are diagnosed;
    // neither is a panic.
    let unterminated = "p(\"abc";
    let source = Source::new(SourceId::new(4), unterminated.to_owned()).expect("admits");
    assert!(
        parse(&source, Dialect::AspCore2).is_incomplete(),
        "an ASP-Core-2 string may yet close"
    );
    let clingo = parse(&source, Dialect::Clingo);
    assert!(
        clingo.has_errors() && !clingo.is_incomplete(),
        "a clingo string is wrong, not incomplete"
    );

    println!("every hostile input answered with a typed refusal or a typed diagnostic");
}
