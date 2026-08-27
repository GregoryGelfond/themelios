//! The depth proof (docs/design/syntax.md §6.6, §16), at the two named
//! limits. The ceiling tier: on a thread of exactly `REQUIRED_STACK_BYTES`,
//! inputs nested far beyond `NestingLimit::CEILING` in every self-recursive
//! family — and beside them the bracket-free chains that must not deepen
//! the tree — parsed, walked through the typed AST, attached, certified,
//! rendered (Display, which recurses in depth — §5.4 law 1), compared, and
//! dropped: no overflow, the refusal for the brackets and none for the
//! chains, the deepest tree measured against law 3's bound, and the same
//! again on half the stack at the ceiling itself — the headroom. The
//! default tier: the same families and walks at `NestingLimit::DEFAULT` on
//! a modest `NORMAL_STACK_BYTES` thread, the crash-averse floor a naive
//! consumer holds, with margin to spare. What the proof shows: this
//! crate's and rowan's walks complete on the stated stack over the deepest
//! tree the parser builds at each limit. What it cannot: a consumer's own
//! recursion over the typed AST.
//!
//! The measurement (`--ignored measure_the_constant`) finds, on half the
//! stack, the largest frame depth every walk survives over trees of the
//! deepest per-frame shape, built directly and probed one process at a
//! time — an overflow ends a process, not a test.

use std::env;
use std::process::Command;
use std::thread;

use rowan::{GreenNode, GreenToken, Language, NodeOrToken};
use themelios_base::source::{Source, SourceId};
use themelios_syntax::ast::Term;
use themelios_syntax::attach::attachments;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::equiv::{Certificate, equivalent, non_whitespace_tokens};
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{
    FIXED_LAYERS, MAX_TREE_DEPTH, NestingLimit, Parse, REQUIRED_STACK_BYTES, TERM_LAYERS_PER_FRAME,
    parse, parse_program, parse_term, parse_term_value,
};
use themelios_syntax::tree::{Asp, AstNode, SyntaxKind, SyntaxNode, TextSize, WalkEvent};

/// The headroom factor: the proof's claim holds on half the stack at the
/// constant.
const HEADROOM: usize = 2;
/// How far beyond the constant the proof nests.
const BEYOND: u32 = 4;
/// The measured constant is rounded down to a multiple of this.
const GRANULE: u32 = 1_000;

fn depth(root: &SyntaxNode) -> usize {
    let mut current = 0usize;
    let mut deepest = 0usize;
    for event in root.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(_)) => {
                current += 1;
                deepest = deepest.max(current);
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => current -= 1,
            _ => {}
        }
    }
    deepest
}

/// The deepest per-frame shape a term frame takes (docs/design/syntax.md
/// §5.4 law 3; `TERM_LAYERS_PER_FRAME`): a function frame whose operand
/// runs through every precedence level and a unary run into the next
/// frame.
fn maximal_term(frames: usize) -> String {
    format!(
        "{}x{}",
        "f(1..2^3?4&5+6*7**-".repeat(frames),
        ")".repeat(frames)
    )
}

/// One input per family at `frames` frames, and how it is parsed: a
/// program text, or a term / term-value fragment.
fn family_inputs(frames: usize) -> Vec<(&'static str, String, Entry)> {
    let n = frames;
    vec![
        (
            "term: the deepest frame shape",
            maximal_term(n),
            Entry::Term,
        ),
        (
            "term: parentheses",
            format!("{}x{}", "(".repeat(n), ")".repeat(n)),
            Entry::Term,
        ),
        (
            "term: absolute value",
            format!("{}x{}", "|".repeat(n), "|".repeat(n)),
            Entry::Term,
        ),
        (
            "term: pools",
            format!("{}x{}", "(".repeat(n), ";y)".repeat(n)),
            Entry::Term,
        ),
        (
            "term value: function arguments",
            format!("{}x{}", "f(".repeat(n), ")".repeat(n)),
            Entry::TermValue,
        ),
        (
            "constant term: function arguments",
            format!("#const c = {}x{}.", "f(".repeat(n), ")".repeat(n)),
            Entry::Program,
        ),
        (
            "theory term: sets",
            format!("&a {{ {}x{} }}.", "{".repeat(n), "}".repeat(n)),
            Entry::Program,
        ),
        (
            "theory term: lists",
            format!("&a {{ {}x{} }}.", "[".repeat(n), "]".repeat(n)),
            Entry::Program,
        ),
        (
            "theory term: tuples",
            format!("&a {{ {}x{} }}.", "(".repeat(n), ")".repeat(n)),
            Entry::Program,
        ),
        (
            "theory term: functions",
            format!("&a {{ {}x{} }}.", "f(".repeat(n), ")".repeat(n)),
            Entry::Program,
        ),
    ]
}

/// The bracket-free shapes that must not deepen the tree.
fn chain_inputs(length: usize) -> Vec<(&'static str, String)> {
    vec![
        ("additive chain", vec!["1"; length].join("+")),
        ("exponentiation chain", vec!["2"; length].join("**")),
        ("unary run", format!("{}x", "-".repeat(length))),
    ]
}

#[derive(Clone, Copy)]
enum Entry {
    Program,
    Term,
    TermValue,
}

/// Every walk over one parse — the walks the design names — and the
/// tree's depth. Runs on the caller's thread: call it on the proof's.
fn walks<T: AstNode<Language = Asp>>(one: &Parse<T>, two: &Parse<T>) -> usize {
    let root = one.syntax();
    // Display renders the tree to its text and recurses in tree depth
    // (docs/design/syntax.md §5.4 law 1, §14) — the walk a consumer's
    // `to_string()` performs. Linear in the tree, where the alternate
    // `Debug` (`{:#?}`) is iterative but quadratic in depth — a
    // small-input tool, not a walk to run over the deepest tree (§14).
    let rendered = root.to_string();
    assert!(!rendered.is_empty());
    // `token_at_offset` descends to the token at a deep offset — another
    // of rowan's depth-recursions (§14).
    let mid = TextSize::new(u32::from(root.text_range().end()) / 2);
    let _ = root.token_at_offset(mid);
    // Structural equality recurses in tree depth (§14).
    assert_eq!(
        one, two,
        "two parses of one text compare equal (rowan's equality recurses)"
    );
    let terms = root.descendants().filter_map(Term::cast).count();
    let _ = terms;
    let attached = attachments(&root).count();
    let _ = attached;
    let tokens = non_whitespace_tokens(&root).count();
    assert!(tokens > 0);
    assert_eq!(equivalent(one, two, Certificate::LayoutOnly), Ok(()));
    assert_eq!(equivalent(one, two, Certificate::UpToSpelling), Ok(()));
    depth(&root)
}

fn parse_twice(text: &str, entry: Entry, limit: NestingLimit) -> (usize, bool) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    match entry {
        Entry::Program => {
            let one = parse_program(&Lexer::new(&source, Dialect::Clingo), limit);
            let two = parse_program(&Lexer::new(&source, Dialect::Clingo), limit);
            let refused = one
                .diagnostics()
                .iter()
                .any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
        Entry::Term => {
            let one = parse_term(&Lexer::new(&source, Dialect::Clingo), limit);
            let two = parse_term(&Lexer::new(&source, Dialect::Clingo), limit);
            let refused = one
                .diagnostics()
                .iter()
                .any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
        Entry::TermValue => {
            let one = parse_term_value(&Lexer::new(&source, Dialect::Clingo), limit);
            let two = parse_term_value(&Lexer::new(&source, Dialect::Clingo), limit);
            let refused = one
                .diagnostics()
                .iter()
                .any(|d| d.id().name() == "nesting-too-deep");
            (walks(&one, &two), refused)
        }
    }
}

/// The proof's body at `frames` frames and `limit`: every family, then the
/// chains. Returns the deepest tree seen.
fn proof_body(frames: usize, expect_refusal: bool, limit: NestingLimit) -> usize {
    let mut deepest = 0usize;
    for (family, text, entry) in family_inputs(frames) {
        let (tree_depth, refused) = parse_twice(&text, entry, limit);
        assert_eq!(refused, expect_refusal, "{family} at {frames} frames");
        assert!(
            tree_depth <= MAX_TREE_DEPTH as usize,
            "{family}: {tree_depth} exceeds the bound"
        );
        deepest = deepest.max(tree_depth);
    }
    for (chain, text) in chain_inputs(frames) {
        let (tree_depth, refused) = parse_twice(&text, Entry::Term, limit);
        assert!(!refused, "{chain}: a chain is never refused");
        assert!(
            tree_depth <= FIXED_LAYERS as usize,
            "{chain}: a chain does not deepen the tree"
        );
    }
    deepest
}

fn on_stack<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(bytes: usize, body: F) -> R {
    thread::Builder::new()
        .name(format!("depth-proof-{bytes}"))
        .stack_size(bytes)
        .spawn(body)
        .expect("the proof's thread spawns")
        .join()
        .expect("the proof's thread completes")
}

#[test]
fn the_depth_proof() {
    // The ceiling tier, on the pole. Far beyond the ceiling, on the full
    // stack: refused, walked, dropped.
    let ceiling = NestingLimit::CEILING;
    let beyond = ceiling.frames() as usize * BEYOND as usize;
    let deepest_beyond = on_stack(REQUIRED_STACK_BYTES, move || {
        proof_body(beyond, true, ceiling)
    });
    assert!(deepest_beyond <= MAX_TREE_DEPTH as usize);
    // At the ceiling, on half the stack: admitted, walked, dropped — the
    // headroom — and the deepest shape measured against the bound exactly.
    let at = ceiling.frames() as usize;
    let deepest_at = on_stack(REQUIRED_STACK_BYTES / HEADROOM, move || {
        proof_body(at, false, ceiling)
    });
    assert!(deepest_at <= MAX_TREE_DEPTH as usize);
    let expected_deepest = 2 + at * TERM_LAYERS_PER_FRAME as usize;
    assert_eq!(
        deepest_at, expected_deepest,
        "the deepest frame shape realizes TERM_LAYERS_PER_FRAME layers per frame under the term root and its leaf"
    );
}

/// The stack the default tier's guarantee is stated against
/// (docs/design/syntax.md §6.6): two mebibytes — a quarter of the
/// eight-mebibyte main-thread default of the two supported operating
/// systems, where a naive consumer's code runs.
const NORMAL_STACK_BYTES: usize = 2 * 1024 * 1024;

#[test]
fn the_default_tier_is_safe_on_a_normal_stack() {
    // The default tier, on a modest stack. Unlike the ceiling — measured to
    // the edge of the pole — the default sits well below what the modest
    // stack survives, so the full modest stack, not half, is where its
    // margin shows (docs/design/syntax.md §6.6). Every family, nested far
    // beyond the default: refused, the capped tree walked and dropped.
    let default = NestingLimit::DEFAULT;
    let beyond = default.frames() as usize * BEYOND as usize;
    let deepest_beyond = on_stack(NORMAL_STACK_BYTES, move || {
        proof_body(beyond, true, default)
    });
    assert!(deepest_beyond <= MAX_TREE_DEPTH as usize);
    // And at the default: admitted, the deepest tree it builds walked and
    // dropped — on the modest stack, with margin to spare.
    let at = default.frames() as usize;
    let deepest_at = on_stack(NORMAL_STACK_BYTES, move || proof_body(at, false, default));
    assert!(deepest_at <= MAX_TREE_DEPTH as usize);
    // The file door itself — the door a naive consumer calls — refuses the
    // deepest program and holds, on the modest stack.
    let text = format!("p({}x{}).", "f(".repeat(beyond), ")".repeat(beyond));
    on_stack(NORMAL_STACK_BYTES, move || {
        let source = Source::new(SourceId::new(0), text).expect("admits");
        let one = parse(&source, Dialect::Clingo);
        let two = parse(&source, Dialect::Clingo);
        assert!(
            one.diagnostics()
                .iter()
                .any(|d| d.id().name() == "nesting-too-deep"),
            "the file door refuses at the default, not crashes"
        );
        assert!(walks(&one, &two) <= MAX_TREE_DEPTH as usize);
    });
}

// ---- the measurement --------------------------------------------------

/// A tree of the deepest per-frame shape, `frames` deep, built directly:
/// what the parser builds for `maximal_term(frames)`, without the parser's
/// constant in the way. Built bottom-up through `GreenNode::new` with no
/// intern-cache, so O(nodes) — like the parser's own builder
/// (src/parse/builder.rs); rowan's cached `GreenNodeBuilder` is O(depth²)
/// on this deep, narrow shape (every node at most three children, all
/// cached), which would make the measurement quadratic.
fn build_maximal(frames: usize) -> SyntaxNode {
    type Element = NodeOrToken<GreenNode, GreenToken>;
    let raw = Asp::kind_to_raw;
    let tok =
        |k: SyntaxKind, t: &str| -> Element { NodeOrToken::Token(GreenToken::new(raw(k), t)) };
    let node = |k: SyntaxKind, kids: Vec<Element>| -> Element {
        NodeOrToken::Node(GreenNode::new(raw(k), kids))
    };
    let levels = [
        (SyntaxKind::DOTDOT, "1", ".."),
        (SyntaxKind::CARET, "2", "^"),
        (SyntaxKind::QUESTION, "3", "?"),
        (SyntaxKind::AMPERSAND, "4", "&"),
        (SyntaxKind::PLUS, "5", "+"),
        (SyntaxKind::STAR, "6", "*"),
        (SyntaxKind::STAR_STAR, "7", "**"),
    ];
    // Bottom-up: the leaf, then each frame wraps the inner subtree in its
    // unary run, its seven binary levels (innermost first), and its
    // function frame — the same shape `maximal_term` parses to.
    let mut inner = node(SyntaxKind::CONSTANT_TERM, vec![tok(SyntaxKind::IDENT, "x")]);
    for _ in 0..frames {
        inner = node(
            SyntaxKind::UNARY_TERM,
            vec![tok(SyntaxKind::MINUS, "-"), inner],
        );
        for (kind, operand, operator) in levels.iter().rev() {
            inner = node(
                SyntaxKind::BINARY_TERM,
                vec![
                    node(
                        SyntaxKind::CONSTANT_TERM,
                        vec![tok(SyntaxKind::NUMBER, operand)],
                    ),
                    tok(*kind, operator),
                    inner,
                ],
            );
        }
        inner = node(SyntaxKind::TUPLE, vec![inner]);
        inner = node(
            SyntaxKind::ARGUMENTS,
            vec![
                tok(SyntaxKind::L_PAREN, "("),
                inner,
                tok(SyntaxKind::R_PAREN, ")"),
            ],
        );
        inner = node(
            SyntaxKind::FUNCTION_TERM,
            vec![tok(SyntaxKind::IDENT, "f"), inner],
        );
    }
    SyntaxNode::new_root(GreenNode::new(raw(SyntaxKind::TERM_FRAGMENT), vec![inner]))
}

/// The walks over a built tree, on the caller's thread.
fn walks_over_built(frames: usize) {
    let one = build_maximal(frames);
    let two = build_maximal(frames);
    // The depth-recursions the proof's `walks` runs, so this measurement's
    // overflow point predicts the proof's headroom: Display (§5.4 law 1,
    // §14), `token_at_offset`, structural equality, and drop.
    let rendered = one.to_string();
    assert!(!rendered.is_empty());
    let mid = TextSize::new(u32::from(one.text_range().end()) / 2);
    let _ = one.token_at_offset(mid);
    assert!(one.green() == two.green(), "structural equality recurses");
    let _ = one.descendants().filter_map(Term::cast).count();
    let _ = attachments(&one).count();
    let _ = non_whitespace_tokens(&one).count();
    assert!(depth(&one) >= frames * TERM_LAYERS_PER_FRAME as usize);
    drop(one);
    drop(two);
}

const PROBE: &str = "THEMELIOS_DEPTH_PROBE";

/// The probe entry: run by the measurement in a child process, with
/// `THEMELIOS_DEPTH_PROBE=<frames>,<stack bytes>`; a no-op otherwise.
#[test]
fn probe_entry() {
    let Ok(spec) = env::var(PROBE) else { return };
    let (frames, bytes) = spec.split_once(',').expect("frames,bytes");
    let frames: usize = frames.parse().expect("frames");
    let bytes: usize = bytes.parse().expect("bytes");
    on_stack(bytes, move || walks_over_built(frames));
}

/// Whether every walk survives `frames` frames on `bytes` of stack, in a
/// child process.
fn survives(frames: usize, bytes: usize) -> bool {
    Command::new(env::current_exe().expect("this test binary"))
        .args(["--exact", "probe_entry", "--test-threads=1", "--nocapture"])
        .env(PROBE, format!("{frames},{bytes}"))
        .status()
        .expect("the probe runs")
        .success()
}

/// The measurement (docs/design/syntax.md §6.6): on half the required
/// stack, the largest frame depth every walk survives over trees of the
/// deepest per-frame shape, by doubling then bisection; the constant is
/// the largest multiple of the granule not above it.
#[test]
#[ignore = "out of band: cargo test -p themelios-syntax --test depth_proof -- --ignored measure_the_constant --nocapture"]
fn measure_the_constant() {
    let stack = REQUIRED_STACK_BYTES / HEADROOM;
    let mut low = 1usize;
    let mut high = 2usize;
    while survives(high, stack) {
        low = high;
        high *= 2;
        assert!(
            high < 1 << 26,
            "no overflow found: the stack is larger than any tree the granule can name"
        );
    }
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if survives(middle, stack) {
            low = middle;
        } else {
            high = middle;
        }
    }
    let constant = u32::try_from(low).expect("fits") / GRANULE * GRANULE;
    println!(
        "on {stack} bytes every walk survives {low} frames of the deepest shape and fails at {high};"
    );
    println!(
        "NestingLimit::CEILING = {constant} (the largest multiple of {GRANULE} not above {low})"
    );
}

// ---- builder validation (TDD for the shape helpers) -------------------
//
// The measurement builds trees directly with `build_maximal`, bypassing
// the parser and its constant; the proof then trusts the parsed tree's
// depth exactly. These hold both to the ground truth at small frame
// counts — cheap, and permanent regressions — so a builder that drifts
// from what the parser emits is caught here, not silently mismeasured.

#[test]
fn the_builders_realize_the_per_frame_layer_count() {
    for frames in 1..=4usize {
        let expected = 2 + frames * TERM_LAYERS_PER_FRAME as usize;
        let built = build_maximal(frames);
        assert_eq!(
            depth(&built),
            expected,
            "build_maximal({frames}) is the term root, {frames} frames, and the leaf"
        );
        let source = Source::new(SourceId::new(0), maximal_term(frames)).expect("admits");
        let parsed = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert!(!parsed.has_errors(), "maximal_term({frames}) is a member");
        assert_eq!(
            depth(&parsed.syntax()),
            expected,
            "the parser realizes the layer count build_maximal claims"
        );
        assert!(
            parsed.syntax().green() == built.green(),
            "build_maximal reproduces the parser's tree for maximal_term (the measurement's fidelity)"
        );
    }
}

#[test]
fn the_bracket_free_chains_do_not_deepen_the_tree() {
    for (chain, text) in chain_inputs(64) {
        let source = Source::new(SourceId::new(0), text).expect("admits");
        let parsed = parse_term(&Lexer::new(&source, Dialect::Clingo), NestingLimit::DEFAULT);
        assert!(!parsed.has_errors(), "{chain} is a member");
        let refused = parsed
            .diagnostics()
            .iter()
            .any(|d| d.id().name() == "nesting-too-deep");
        assert!(!refused, "{chain} is never refused");
        assert!(
            depth(&parsed.syntax()) <= FIXED_LAYERS as usize,
            "{chain} stays flat"
        );
    }
}
