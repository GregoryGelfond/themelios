//! The syntax tier's seed of the *diagnostics-quality* witness
//! (docs/specification.md §3): characteristic malformed programs fed to
//! the parser; every diagnostic a typed value with a stable identity
//! and a precise span, rendered through the base tier's human view —
//! the renderings themselves are held to their reviewed goldens by
//! `tests/golden.rs`.

use themelios_syntax::base::diagnostic::ToDiagnostic;
use themelios_syntax::base::source::{Source, SourceSet};
use themelios_syntax::base::view::human;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn main() {
    let programs = [
        "p(X) :- q(X) r(X).\n",
        "p(a, b,).\n",
        "#heuristic a : b.\n",
        "p(\"a\\qb\"). q.\n",
        ":- #count { X : p(X) .\n",
    ];
    let mut catalog = SourceSet::new();
    for text in programs {
        let file = catalog
            .add("input.lp".to_owned(), text.to_owned())
            .expect("admits");
        let source = Source::new(file, text.to_owned()).expect("admits");
        let parse = parse(&source, Dialect::Clingo);
        assert!(parse.has_errors());
        for diagnostic in parse.diagnostics() {
            assert_eq!(diagnostic.id().namespace(), "syntax");
            // A precise span (docs/design/syntax.md §7): located by
            // construction within the source, zero-width only where it marks
            // the end of the input — the exact locus an "unexpected end of
            // input" points at.
            let span = diagnostic.primary().span;
            assert!(
                span.end().get() as usize <= text.len(),
                "the span lies within the source"
            );
            assert!(
                !span.is_empty() || span.start().get() as usize == text.len(),
                "a precise span"
            );
            println!("{}", human(&diagnostic.to_diagnostic(), &catalog));
        }
    }
}
