//! The *transformation* witness (docs/design/program.md §9, §16; spec §3): a small
//! `Program -> Program` rewrite that renames a predicate everywhere it occurs, shown to
//! carry provenance through — every rewritten node records the transformation that produced
//! it *and* keeps the source span it was parsed from, so a diagnostic on a rewritten rule
//! still points at the text it came from. Run it with `cargo run --example transformation`.
//!
//! This is the structural half of the promise: the surface transforms `P` into `Q` carrying
//! provenance (§9.3). It makes no claim about answer sets — that a rename preserves them is
//! the modeler's to know, not this tier's to certify.

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::{Atom, Statement};
use themelios_program::provenance::{Origin, TransformTag};
use themelios_program::raise::raise;
use themelios_program::symbol::Name;
use themelios_program::transform::{Rewrite, rewrite};

fn main() {
    // Raise a two-rule reachability program from concrete syntax, so every node carries the
    // span it was parsed from.
    let text = "reach(X, Y) :- edge(X, Y).\nreach(X, Z) :- reach(X, Y), edge(Y, Z).\n";
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the source admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    let program = lowered.program().clone();

    // Rename edge -> link everywhere it is derived or depended on.
    let mut rename = Rename {
        from: Name::new("edge").expect("a valid identifier"),
        to: Name::new("link").expect("a valid identifier"),
        tag: TransformTag::new("rename-edge-to-link"),
    };
    let transformed = rewrite(program, &mut rename);

    // Every edge is now link; no edge remains.
    let mentions_edge = |program: &themelios_program::program::Program, predicate: &str| {
        let target = Name::new(predicate).expect("a valid identifier");
        program
            .statements()
            .any(|statement| renders(statement.get()).any(|name| name == &target))
    };
    assert!(
        !mentions_edge(&transformed, "edge"),
        "the rename reaches every occurrence of edge",
    );
    assert!(
        mentions_edge(&transformed, "link"),
        "the renamed predicate link is present",
    );

    // The transformation witness: each rewritten statement records the transformation *and*
    // keeps the parsed origin it came from, so blame still reaches the source.
    let mut traced = 0;
    for statement in transformed.statements() {
        let origins: Vec<&Origin> = statement.provenance().origins().collect();
        let has_tag = origins
            .iter()
            .any(|origin| matches!(origin, Origin::Transformed(tag) if tag.as_str() == "rename-edge-to-link"));
        let reaches_source = origins
            .iter()
            .any(|origin| matches!(origin, Origin::Parsed(_)));
        assert!(
            has_tag && reaches_source,
            "a rewritten rule traces back to its source"
        );
        traced += 1;
    }

    println!("renamed edge -> link across {traced} rules; each traces back to its source span.");
}

/// The predicate names a statement derives or depends on — enough to see the rename took.
fn renders(statement: &Statement) -> impl Iterator<Item = &Name> {
    let mut names = Vec::new();
    if let Statement::Rule(rule) = statement {
        collect_head(rule, &mut names);
        collect_body(rule, &mut names);
    }
    names.into_iter()
}

fn collect_head<'a>(rule: &'a themelios_program::program::Rule, names: &mut Vec<&'a Name>) {
    if let themelios_program::program::Head::Literal(literal) = rule.head().get() {
        push_atom_name(literal, names);
    }
}

fn collect_body<'a>(rule: &'a themelios_program::program::Rule, names: &mut Vec<&'a Name>) {
    use themelios_program::program::BodyElement;
    for element in rule.body().get().elements() {
        if let BodyElement::Literal(literal) = element.get() {
            push_atom_name(literal, names);
        }
    }
}

fn push_atom_name<'a>(literal: &'a themelios_program::program::Literal, names: &mut Vec<&'a Name>) {
    use themelios_program::program::LiteralInner;
    if let LiteralInner::Atom(atom) = &literal.inner {
        names.push(&atom.get().name);
    }
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
