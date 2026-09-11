//! Compile-time diagnostics (docs/design/macros.md §5, step 3): the macro
//! reports at the rust-analyzer bar by re-emitting every fragment error as a
//! `compile_error!` at the Rust span the offending token came from (§6).
//!
//! Two error streams meet here, from two parses of one fragment. **Syntax
//! diagnostics** come from the real, splice-bearing parse the codegen walks
//! (§5, step 2). **Lowering diagnostics** come from raising a splice-free view
//! of that fragment ([`MacroSource::splice_free_view`]): the raise runs its
//! ordinary, documented behaviour (program §8) over the ground placeholders,
//! with no reliance on how it treats a splice and tripping no positional check
//! a raised splice would (§5, step 3). Each error's themelios `Location` is
//! under the unresolvable string-input id, so it is never projected; its span's
//! start is re-located through a `MacroSource`'s span map to a
//! `proc_macro2::Span` (§6). The two parses carry distinct byte offsets — a
//! placeholder is shorter than the splice it stands for — so a syntax error
//! re-locates through the real source and a lowering error through the view,
//! each its own map (§5's isomorphism invariant).
// `emit_diagnostics` has no caller outside this module's own tests until the
// entry points wire the pipeline; the not-yet-wired surface would read as dead.
// The allow is removed when the entry points arrive, as on the sibling modules.
#![allow(dead_code)]

use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;
use themelios_program::raise::LowerError;
use themelios_syntax::base::diagnostic::ToDiagnostic;
use themelios_syntax::base::span::Location;
use themelios_syntax::diagnostic::SyntaxError;
use themelios_syntax::tree::TextRange;

use crate::source::MacroSource;

/// Re-emit a fragment's diagnostics as a `compile_error!` stream at the
/// rust-analyzer bar (docs/design/macros.md §5, step 3), or `None` when the
/// fragment is clean. `syntax_errors` come from the real parse of `source`;
/// `lowering_errors` from raising `view`, the splice-free twin of `source`
/// ([`MacroSource::splice_free_view`]). Each error re-locates through its own
/// source's span map — the two carry distinct byte offsets — so a compile
/// error lands on the Rust token it blames, never on the unresolvable
/// themelios `Location` (§6). Emission order is the pipeline's: every syntax
/// error, then every lowering one, as [`RaisedSource`](themelios_program::raise)
/// merges them.
pub(crate) fn emit_diagnostics(
    source: &MacroSource,
    syntax_errors: &[SyntaxError],
    view: &MacroSource,
    lowering_errors: &[LowerError],
) -> Option<TokenStream> {
    if syntax_errors.is_empty() && lowering_errors.is_empty() {
        return None;
    }
    let mut stream = TokenStream::new();
    for error in syntax_errors {
        stream.extend(compile_error(
            rust_span(source, error.primary()),
            &render(error),
        ));
    }
    for error in lowering_errors {
        stream.extend(compile_error(
            rust_span(view, *error.location()),
            &render(error),
        ));
    }
    Some(stream)
}

/// A `compile_error!(message)` at `span` — the rust-analyzer-bar emission of one
/// diagnostic (docs/design/macros.md §5). `quote_spanned!` stamps `span` on the
/// generated tokens, so the compiler blames the Rust token the span came from.
fn compile_error(span: Span, message: &str) -> TokenStream {
    quote_spanned! { span => compile_error!(#message); }
}

/// The `proc_macro2` span the Rust token at `location`'s start came from — the
/// span map re-locates the themelios `Location` (under the unresolvable
/// string-input id) to a real Rust span (§6). `span_of` reads the start and is
/// total; the base `Span`'s `start <= end` invariant makes the range total too.
fn rust_span(source: &MacroSource, location: Location) -> Span {
    let start = location.span.start().get();
    let end = location.span.end().get();
    source.span_of(TextRange::new(start.into(), end.into()))
}

/// The base human message a diagnostic leads with at the rust-analyzer bar
/// (base §6.5): the headline of its normal-form `Diagnostic`, the syntax and
/// lowering tiers rendering through one model (§5). Not the located human
/// render — the Rust span carries the location the themelios `Location` cannot.
fn render(error: &impl ToDiagnostic) -> String {
    error.to_diagnostic().message().to_owned()
}

// The roster's screaming-snake kinds read as the tokens they are; the glob is
// what lets the isomorphism walk name them without a qualifier apiece.
#[cfg(test)]
#[allow(clippy::enum_glob_use)]
mod tests {
    use std::ops::Range;
    use std::str::FromStr;

    use proc_macro2::{TokenStream, TokenTree};
    use themelios_program::raise::{LowerErrorKind, raise_statement};
    use themelios_syntax::tree::SyntaxKind::*;
    use themelios_syntax::tree::{NodeOrToken, SyntaxNode};

    use super::*;
    use crate::engine::parse_statement_fragment;

    /// The macro source `input` assembles to under the dialect (no leading
    /// keyword) — the token source a construction site hands the engine.
    fn source(input: &str) -> MacroSource {
        MacroSource::build(TokenStream::from_str(input).expect("lexes"), None)
            .expect("maps under the dialect")
    }

    /// The lowering errors of `src`'s fragment — raised over its splice-free
    /// view, the compile-time lowering pass (docs/design/macros.md §5, step 3).
    fn lowering(src: &MacroSource) -> Vec<LowerError> {
        raise_statement(&parse_statement_fragment(&src.splice_free_view())).1
    }

    /// Whether the emitted stream carries a token stamped at exactly `range` —
    /// the span `quote_spanned!` set on the `compile_error!` (proc-macro2's
    /// fallback reports a byte range over the parsed text, so this is exact).
    fn diagnostic_span_covers(stream: &TokenStream, range: &Range<usize>) -> bool {
        stream.clone().into_iter().any(|tree| match tree {
            TokenTree::Group(group) => diagnostic_span_covers(&group.stream(), range),
            other => other.span().byte_range() == *range,
        })
    }

    /// The Rust byte range `src`'s span map yields for a fragment's first
    /// syntax error — the span the compile error is expected to carry, read
    /// through the same map, so the assertion tracks the offending token
    /// wherever the parser flags it.
    fn first_syntax_span(src: &MacroSource, parse_errors: &[SyntaxError]) -> Range<usize> {
        rust_span(src, parse_errors[0].primary()).byte_range()
    }

    #[test]
    fn a_syntax_error_maps_to_its_rust_span() {
        // `p(1 1)` — two numerals with no comma between them — is a syntax
        // error the real parse flags; the compile error lands on the token the
        // parser blames, via the span map (docs/design/macros.md §5, step 3).
        let src = source("p(1 1).");
        let real = parse_statement_fragment(&src);
        assert!(real.has_errors(), "{:?}", real.diagnostics());
        let view = src.splice_free_view();
        let stream = emit_diagnostics(&src, real.diagnostics(), &view, &lowering(&src))
            .expect("a syntax error stream");
        let expected = first_syntax_span(&src, real.diagnostics());
        assert!(
            diagnostic_span_covers(&stream, &expected),
            "the compile error carries the offending token's Rust span {expected:?}"
        );
    }

    #[test]
    fn a_lowering_error_maps_to_its_rust_span() {
        // A numeral past `i32` is a genuine `NumberOutOfRange` the raise reports
        // (program §8) — not an unsafe-variable case, which the raise never
        // reports (safety is the analysis tier's). The numeral `9999999999`
        // sits at input bytes 2..12; the compile error lands there.
        let src = source("p(9999999999).");
        let real = parse_statement_fragment(&src);
        assert!(!real.has_errors(), "{:?}", real.diagnostics());
        let view = src.splice_free_view();
        let errors = lowering(&src);
        assert!(!errors.is_empty(), "the numeral overflows i32");
        let stream = emit_diagnostics(&src, real.diagnostics(), &view, &errors)
            .expect("a lowering error stream");
        assert!(
            diagnostic_span_covers(&stream, &(2..12)),
            "the compile error blames the offending numeral, not the themelios location"
        );
    }

    #[test]
    fn a_lowering_error_after_a_splice_maps_through_the_view() {
        // The splice `$x` (two bytes) shrinks to the placeholder `0` (one byte)
        // in the view, so every byte past it sits one earlier in view
        // coordinates than in the source. A lowering error is located in the
        // view's parse; routing it through the source's span map would mispoint
        // by that shift. This pins the two-parse scheme's flagship soundness
        // fix — a lowering error re-locates through the view, never the source
        // (docs/design/macros.md §5, step 3) — which the sole other lowering
        // test cannot, having no splice before its error.
        let src = source("p($x + 9999999999).");
        let real = parse_statement_fragment(&src);
        assert!(!real.has_errors(), "{:?}", real.diagnostics());
        let view = src.splice_free_view();
        let errors = lowering(&src);
        // A single clean `NumberOutOfRange` at the numeral — `$x` is a splice,
        // `0 + 9999999999` overflows `i32` at the numeral alone (program §8).
        assert_eq!(errors.len(), 1, "one lowering error: {errors:?}");
        let kind = errors[0].kind();
        assert!(
            matches!(kind, LowerErrorKind::NumberOutOfRange),
            "the numeral overflows `i32`: {kind:?}"
        );
        let location = *errors[0].location();
        let stream = emit_diagnostics(&src, real.diagnostics(), &view, &errors)
            .expect("a lowering error stream");
        // `9999999999` is at bytes 7..17 of `p($x + 9999999999).` (indexed
        // directly); proc-macro2's fallback reports that range for the literal,
        // ground truth independent of the routing under test. View-routing
        // lands there.
        let numeral = 7..17;
        assert!(
            diagnostic_span_covers(&stream, &numeral),
            "the compile error blames the numeral's Rust token via the view"
        );
        // The witness that this discriminates: source-routing the same view
        // location lands on the `+` (bytes 5..6), not the numeral — so a revert
        // to 3-arg source-routing would move the emitted span off 7..17 and
        // fail the assertion above. The stream carries the numeral's span and
        // never the source-routed one.
        let source_routed = rust_span(&src, location).byte_range();
        assert_ne!(
            source_routed, numeral,
            "source-routing mispoints past the splice"
        );
        assert!(
            !diagnostic_span_covers(&stream, &source_routed),
            "the compile error does not blame the source-routed token"
        );
    }

    #[test]
    fn a_splice_in_a_const_value_does_not_spuriously_error() {
        // The placeholder is the constant `0`, valid where a `#const` value
        // must be a constant term (grammar §5.9) — so the view raises clean,
        // where a raised splice's anonymous-variable placeholder would trip a
        // spurious `NonConstantValue` (docs/design/macros.md §5, step 3).
        let src = source("#const n = $x.");
        let real = parse_statement_fragment(&src);
        assert!(!real.has_errors(), "{:?}", real.diagnostics());
        let view = src.splice_free_view();
        let errors = lowering(&src);
        assert!(
            errors.is_empty(),
            "the constant placeholder is a constant term: {errors:?}"
        );
        assert!(
            emit_diagnostics(&src, real.diagnostics(), &view, &errors).is_none(),
            "a splice in a #const value is not itself an error"
        );
    }

    #[test]
    fn a_clean_fragment_yields_no_diagnostics() {
        let src = source("p(1, a).");
        let real = parse_statement_fragment(&src);
        let view = src.splice_free_view();
        assert!(emit_diagnostics(&src, real.diagnostics(), &view, &lowering(&src)).is_none());
    }

    /// A splice in each structural position the grammar reaches
    /// (docs/design/macros.md §11), each a statement fragment.
    const ISOMORPHISM_CORPUS: [&str; 6] = [
        "p($x).",      // a function argument
        "p(($x, a)).", // a tuple element
        "p(($x; a)).", // a pool alternative
        "p($x..3).",   // an interval bound
        ":- $x < a.",  // a comparison side
        "&a { $x }.",  // a theory-term
    ];

    #[test]
    fn the_two_parses_are_isomorphic_modulo_splice_leaves() {
        // The lemma the two-parse scheme rests on (docs/design/macros.md §5,
        // §11): the splice-bearing parse and the splice-free view's re-parse
        // are node-for-node identical, each `SPLICE_TERM` standing where the
        // view holds the placeholder's `CONSTANT_TERM`. If a later change to
        // the tiling perturbed the structure around a splice, this fails —
        // rather than a macro-site diagnostic silently pointing at a wrong span.
        for fragment in ISOMORPHISM_CORPUS {
            let src = source(fragment);
            let view = src.splice_free_view();
            let real = parse_statement_fragment(&src).syntax();
            let viewed = parse_statement_fragment(&view).syntax();
            assert_eq!(
                splice_terms(&real),
                1,
                "`{fragment}` exercises exactly one splice"
            );
            assert_eq!(
                splice_terms(&viewed),
                0,
                "`{fragment}`'s view is splice-free"
            );
            assert!(
                isomorphic_modulo_splices(&real, &viewed),
                "`{fragment}` re-parses isomorphic to its splice-free view, modulo the splice leaf"
            );
        }
    }

    #[test]
    fn the_isomorphism_check_rejects_divergent_trees() {
        // The guard that gives the corpus check teeth: trees that differ away
        // from any splice — in arity, in a leaf's node kind, and in a leaf
        // token's kind at one slot — are not judged isomorphic. The last
        // (`1 + 2` vs `1 - 2`, identical but for the operator token) is the
        // one the lockstep walk's `(Token, Token)` arm decides.
        for (left, right) in [
            ("p(1).", "p(1, 2)."),
            ("q(X).", "q(1)."),
            ("p(1 + 2).", "p(1 - 2)."),
        ] {
            let one = parse_statement_fragment(&source(left)).syntax();
            let two = parse_statement_fragment(&source(right)).syntax();
            assert!(
                !isomorphic_modulo_splices(&one, &two),
                "`{left}` and `{right}` diverge away from any splice"
            );
        }
    }

    /// The count of `SPLICE_TERM` nodes in a tree — the corpus check reads it
    /// to confirm a fragment exercises a splice and its view erases it.
    fn splice_terms(node: &SyntaxNode) -> usize {
        node.descendants()
            .filter(|node| node.kind() == SPLICE_TERM)
            .count()
    }

    /// Walk two parse trees in lockstep (docs/design/macros.md §5, §11): away
    /// from a splice leaf, every child node and non-trivia token must match
    /// kind for kind; a `SPLICE_TERM` is the one mapped leaf — the
    /// splice-bearing tree holds it where the view holds the placeholder's
    /// `CONSTANT_TERM`, and neither side's leaf interior is compared. Any other
    /// divergence — a differing kind, a differing child count, a node where the
    /// other holds a token — is not isomorphic.
    fn isomorphic_modulo_splices(real: &SyntaxNode, view: &SyntaxNode) -> bool {
        if real.kind() == SPLICE_TERM {
            return view.kind() == CONSTANT_TERM;
        }
        if real.kind() != view.kind() {
            return false;
        }
        let mut real_items = real.children_with_tokens().filter(not_trivia);
        let mut view_items = view.children_with_tokens().filter(not_trivia);
        loop {
            match (real_items.next(), view_items.next()) {
                (Some(NodeOrToken::Node(real)), Some(NodeOrToken::Node(view))) => {
                    if !isomorphic_modulo_splices(&real, &view) {
                        return false;
                    }
                }
                (Some(NodeOrToken::Token(real)), Some(NodeOrToken::Token(view))) => {
                    if real.kind() != view.kind() {
                        return false;
                    }
                }
                (None, None) => return true,
                _ => return false,
            }
        }
    }

    /// Whether a tree element is not trivia — the isomorphism walk skips
    /// whitespace, which a splice takes on both sides yet its placeholder need
    /// not, so the two differ only in trivia there (syntax §4.2).
    fn not_trivia(item: &NodeOrToken<SyntaxNode, themelios_syntax::tree::SyntaxToken>) -> bool {
        !item.kind().is_trivia()
    }
}
