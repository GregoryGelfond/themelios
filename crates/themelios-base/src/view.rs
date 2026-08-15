//! Views: pure derivations over `(&Diagnostic, &impl Sources)`
//! (docs/design/base.md §7). There is deliberately no view trait —
//! the open extension point for a new view is the model being public
//! plain data; anyone writes a function over it. The polymorphism a
//! view does need is over its *environment*, and that is the
//! `Sources` trait.

use std::cmp::Ordering;
use std::fmt;

use crate::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
use crate::line::{ColumnEncoding, LineCol, LineIndex, PositionRefusal};
use crate::source::{SourceFacet, SourceId, Sources};
use crate::span::Location;

/// The human rendering: total and deterministic, with zero options —
/// one canonical output, which is what a reviewable golden corpus
/// requires; color and width knobs are named view evolution, not v1
/// surface (base.md §7.1).
///
/// Degradation is honest and maximal (base.md §3.3, §7.1): an
/// unresolvable id, a completeness breach, and a label that does not
/// fit its resolved text each render a named inline placeholder while
/// every coherent label still renders. Total — nothing panics,
/// nothing is silently dropped; O(rendered size), flat iteration.
pub fn human(diagnostic: &Diagnostic, sources: &impl Sources) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}[{}]: {}\n",
        diagnostic.severity(),
        diagnostic.id(),
        diagnostic.message()
    ));
    // One pass groups the labels by source — O(labels · log sources),
    // within the stated O(rendered size) (base.md §7.1, §9) — with
    // the primary inserted at its position in its own block, marked.
    // The primary's source renders first, then every other touched
    // source in identity order.
    let mut blocks: std::collections::BTreeMap<SourceId, Vec<(&Label, bool)>> =
        std::collections::BTreeMap::new();
    for label in diagnostic.secondary() {
        blocks
            .entry(label.location.source)
            .or_default()
            .push((label, false));
    }
    let primary = diagnostic.primary();
    let own_block = blocks.entry(primary.location.source).or_default();
    let at = own_block.partition_point(|(label, _)| *label < primary);
    own_block.insert(at, (primary, true));
    if let Some(labels) = blocks.remove(&primary.location.source) {
        render_block(&mut out, primary.location.source, &labels, sources);
    }
    for (&source, labels) in &blocks {
        render_block(&mut out, source, labels, sources);
    }
    for note in diagnostic.notes() {
        out.push_str(&format!("  = note: {note}\n"));
    }
    for help in diagnostic.helps() {
        out.push_str(&format!("  = help: {help}\n"));
    }
    out
}

/// One label the block can render: both span ends positioned.
struct Fitting<'a> {
    label: &'a Label,
    primary: bool,
    start: LineCol,
    /// End position, with an end falling at column 0 of a later line
    /// pulled back so a span covering a terminator underlines its own
    /// line, not the next one's zero columns.
    end: LineCol,
}

/// A label the block cannot place: one span end refused positioning.
type Misfit<'a> = (&'a Label, PositionRefusal);

/// One source's snippet block: the header, then the labeled lines
/// with their underlines, then the labels that did not fit. Every
/// degradation renders a named placeholder (base.md §7.1).
fn render_block(
    out: &mut String,
    source: SourceId,
    labels: &[(&Label, bool)],
    sources: &impl Sources,
) {
    let name = sources.name(source);
    let text = sources.text(source);
    let index = sources.line_index(source);
    if name.is_none() && text.is_none() && index.is_none() {
        out.push_str(&format!(" --> <source {}: unresolved>\n", source.get()));
        return;
    }
    // A partial resolution is a completeness breach (base.md §3.4);
    // the placeholder names the missing facet.
    let display = match name {
        Some(name) => name.to_owned(),
        None => format!("<source {}: missing name>", source.get()),
    };
    let (text, index) = match (text, index) {
        (Some(text), Some(index)) => (text, index),
        (None, _) => {
            out.push_str(&format!(" --> {display}\n"));
            out.push_str(&format!("  | <source {}: missing text>\n", source.get()));
            return;
        }
        (_, None) => {
            out.push_str(&format!(" --> {display}\n"));
            out.push_str(&format!("  | <source {}: missing index>\n", source.get()));
            return;
        }
    };

    let (fitting, misfits) = place_labels(index, text, labels);

    // Header coordinates: the primary if it fits, else the first
    // fitting label, else none.
    let anchor = fitting
        .iter()
        .find(|fit| fit.primary)
        .or_else(|| fitting.first());
    match anchor {
        Some(fit) => out.push_str(&format!(
            " --> {display}:{}:{}\n",
            fit.start.line + 1,
            fit.start.col + 1
        )),
        None => out.push_str(&format!(" --> {display}\n")),
    }

    // Only labeled lines render; `..` marks an elision. One pass
    // maps each rendered line to the labels touching it, in position
    // order, so the walk below is flat — O(rendered rows), the
    // stated cost (base.md §7.1, §9) — instead of scanning every
    // label per line.
    let mut rows: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    for (fit_index, fit) in fitting.iter().enumerate() {
        for line in fit.start.line..=fit.end.line {
            rows.entry(line).or_default().push(fit_index);
        }
    }
    let width = rows
        .keys()
        .next_back()
        .map_or(1, |last| (last + 1).to_string().len());
    let pad = " ".repeat(width);
    out.push_str(&format!("{pad} |\n"));

    let mut previous: Option<u32> = None;
    for (&line, touching) in &rows {
        if let Some(previous) = previous
            && line != previous + 1
        {
            out.push_str(&format!("{pad}..\n"));
        }
        previous = Some(line);
        render_line(out, index, text, line, width);
        for &fit_index in touching {
            render_underline(out, &fitting[fit_index], index, text, line, &pad);
        }
    }
    for (label, refusal) in misfits {
        let span = label.location.span;
        let reason = match refusal {
            PositionRefusal::OutOfBounds(oob) => {
                format!("the text ends at byte {}", oob.max.get())
            }
            PositionRefusal::NotCharBoundary(ncb) => {
                format!("byte {} splits a character", ncb.offset.get())
            }
        };
        out.push_str(&format!(
            "{pad} | <label {}..{} does not fit source {}: {reason}>\n",
            span.start().get(),
            span.end().get(),
            source.get()
        ));
    }
}

/// Positions both ends of every label against the resolved index, in
/// code points; a label whose end refuses positioning is a misfit,
/// reported rather than dropped.
fn place_labels<'a>(
    index: &LineIndex,
    text: &str,
    labels: &[(&'a Label, bool)],
) -> (Vec<Fitting<'a>>, Vec<Misfit<'a>>) {
    let mut fitting = Vec::new();
    let mut misfits = Vec::new();
    for &(label, primary) in labels {
        let span = label.location.span;
        let cp = ColumnEncoding::CodePoints;
        match (
            index.position(span.start(), cp),
            index.position(span.end(), cp),
        ) {
            (Ok(start), Ok(mut end)) => {
                if end.line > start.line && end.col == 0 {
                    end.line -= 1;
                    end.col = line_extent(index, text, end.line);
                }
                fitting.push(Fitting {
                    label,
                    primary,
                    start,
                    end,
                });
            }
            (Err(refusal), _) | (_, Err(refusal)) => {
                misfits.push((label, refusal));
            }
        }
    }
    (fitting, misfits)
}

/// One gutter row with the line's content, fetched fallibly: a
/// coherence-breaching catalog may hand an index that disagrees with
/// its text; trusted never means panic-licensed (base.md §3.4, §7.1).
fn render_line(out: &mut String, index: &LineIndex, text: &str, line: u32, width: usize) {
    let content = index
        .line_span(line)
        .ok()
        .and_then(|span| text.get(span.start().get() as usize..span.end().get() as usize))
        .unwrap_or("<line does not fit the resolved text>");
    let number = line + 1;
    out.push_str(&format!("{number:>width$} | {content}\n"));
}

/// One label's underline on one line it touches: `^` for the primary,
/// `-` for a secondary, covering the label's overlap with the line's
/// content (clamped; minimum width one on the label's anchor line),
/// with the message following on the label's last touched line.
fn render_underline(
    out: &mut String,
    fit: &Fitting<'_>,
    index: &LineIndex,
    text: &str,
    line: u32,
    pad: &str,
) {
    let extent = line_extent(index, text, line);
    let from = if line == fit.start.line {
        fit.start.col
    } else {
        0
    };
    let to = if line == fit.end.line {
        fit.end.col.min(extent)
    } else {
        extent
    };
    let anchor_line = line == fit.start.line;
    let width_cols = to.saturating_sub(from);
    if width_cols == 0 && !anchor_line {
        return;
    }
    let marker = if fit.primary { '^' } else { '-' };
    let underline = marker.to_string().repeat(width_cols.max(1) as usize);
    let mut row = format!("{pad} | {}{underline}", " ".repeat(from as usize));
    if line == fit.end.line
        && let Some(message) = &fit.label.message
    {
        row.push(' ');
        row.push_str(message);
    }
    row.push('\n');
    out.push_str(&row);
}

/// A line's content extent in code points, fallibly: zero when the
/// index and text disagree (the coherence trust boundary).
fn line_extent(index: &LineIndex, text: &str, line: u32) -> u32 {
    index
        .line_span(line)
        .ok()
        .and_then(|span| text.get(span.start().get() as usize..span.end().get() as usize))
        .map_or(0, |content| content.chars().count() as u32)
}

/// The editor-protocol payload as a typed value. Serialization is the
/// consumer's step: this crate ships shapes, not bytes (base.md §7.2).
///
/// A typed intermediate shape, deliberately: the honest alternative —
/// a view straight to protocol bytes — is foreclosed by the
/// zero-dependency constraint, and the dishonest one — consumers
/// walking `Diagnostic` per protocol — re-derives position conversion
/// and label linearization in every host. So the view stops at the
/// last typed point before bytes. What keeps this from becoming a
/// second model: exactly one pure function produces it, nothing in
/// this crate holds it, it mirrors the protocol's categories, and
/// nothing maintains it — every instance is a fresh derivation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorDiagnostic {
    /// The primary label's source.
    pub source: SourceId,
    /// The primary label's range.
    pub range: EditorRange,
    /// The encoding every range in this payload was derived under —
    /// the view records the derivation it ran, so the payload is
    /// self-describing when stored, compared, or forwarded.
    pub encoding: ColumnEncoding,
    /// The model's typed severity; the protocol mapping (`Note` to
    /// the information class) is the JSON layer's documented step.
    pub severity: Severity,
    /// Typed identity; the `namespace::name` string is the consumer's
    /// serialization step, via `Display`.
    pub code: DiagnosticId,
    /// The headline, then notes and helps folded as `note:` and
    /// `help:` lines — the protocol convention.
    pub message: String,
    /// The secondary labels, linearized in position order — a view
    /// linearizes (base.md §8.4).
    pub related: Vec<EditorRelated>,
}

/// A protocol range: two zero-based coordinates in the payload's
/// stated encoding (base.md §7.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EditorRange {
    /// Where the range begins.
    pub start: LineCol,
    /// Where the range ends, exclusive.
    pub end: LineCol,
}

/// One related location in the payload (base.md §7.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorRelated {
    /// The related label's source.
    pub source: SourceId,
    /// The related label's range.
    pub range: EditorRange,
    /// The label's message, or the diagnostic's headline when the
    /// label carries none — the location still ships.
    pub message: String,
}

/// Why `view::editor` refused: each refusal names its locus — the
/// question the caller can fix (base.md §7.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorRefusal {
    /// The catalog resolves nothing for this id.
    UnknownSource {
        /// The unresolved id.
        id: SourceId,
    },
    /// The catalog breached the completeness law: it resolved this id
    /// partially, and this is the facet the view lacked.
    IncompleteSource {
        /// The partially resolved id.
        id: SourceId,
        /// The first missing facet, in name, text, index order.
        missing: SourceFacet,
    },
    /// A position query failed; `at` is the label whose location was
    /// being converted, so the caller need not replay the iteration.
    Position {
        /// The label under conversion.
        at: Location,
        /// What the line index refused.
        refusal: PositionRefusal,
    },
}

impl fmt::Display for EditorRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorRefusal::UnknownSource { id } => {
                write!(f, "source {} resolves nothing in the catalog", id.get())
            }
            EditorRefusal::IncompleteSource { id, missing } => {
                let facet = match missing {
                    SourceFacet::Name => "name",
                    SourceFacet::Text => "text",
                    SourceFacet::Index => "index",
                };
                write!(f, "source {} resolved partially: missing {facet}", id.get())
            }
            EditorRefusal::Position { at, refusal } => write!(
                f,
                "a position query failed at source {} bytes {}..{}: {}",
                at.source.get(),
                at.span.start().get(),
                at.span.end().get(),
                refusal
            ),
        }
    }
}

// The refusal is the condition itself (base.md §3.2): `Position`'s
// Display already states the position refusal it carries, so it
// reports no `source()` and a host's error chain says it once.
impl std::error::Error for EditorRefusal {}

/// The editor view: refuses rather than fabricate — a protocol
/// payload with invented ranges is worse than no payload. Refuses
/// `EditorRefusal`, the primary first, then secondaries in position
/// order; O(labels · log) via the resolved line index (base.md §7.2,
/// §9).
pub fn editor(
    diagnostic: &Diagnostic,
    sources: &impl Sources,
    encoding: ColumnEncoding,
) -> Result<EditorDiagnostic, EditorRefusal> {
    let primary = diagnostic.primary();
    let index = resolve_complete(sources, primary.location.source)?;
    let range = range_of(index, primary.location, encoding)?;
    let mut message = diagnostic.message().to_owned();
    for note in diagnostic.notes() {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    for help in diagnostic.helps() {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }
    let mut related = Vec::new();
    for label in diagnostic.secondary() {
        let index = resolve_complete(sources, label.location.source)?;
        let range = range_of(index, label.location, encoding)?;
        related.push(EditorRelated {
            source: label.location.source,
            range,
            message: label
                .message
                .clone()
                .unwrap_or_else(|| diagnostic.message().to_owned()),
        });
    }
    Ok(EditorDiagnostic {
        source: primary.location.source,
        range,
        encoding,
        severity: diagnostic.severity(),
        code: diagnostic.id(),
        message,
        related,
    })
}

/// Resolution under the completeness law: all three facets or a named
/// refusal.
fn resolve_complete(sources: &impl Sources, id: SourceId) -> Result<&LineIndex, EditorRefusal> {
    let name = sources.name(id);
    let text = sources.text(id);
    let index = sources.line_index(id);
    match (name, text, index) {
        (None, None, None) => Err(EditorRefusal::UnknownSource { id }),
        (Some(_), Some(_), Some(index)) => Ok(index),
        (name, text, _) => {
            let missing = if name.is_none() {
                SourceFacet::Name
            } else if text.is_none() {
                SourceFacet::Text
            } else {
                SourceFacet::Index
            };
            Err(EditorRefusal::IncompleteSource { id, missing })
        }
    }
}

/// Both span ends positioned under the stated encoding, the refusal
/// carrying the label's location as its locus.
fn range_of(
    index: &LineIndex,
    location: Location,
    encoding: ColumnEncoding,
) -> Result<EditorRange, EditorRefusal> {
    let position = |offset| {
        index
            .position(offset, encoding)
            .map_err(|refusal| EditorRefusal::Position {
                at: location,
                refusal,
            })
    };
    Ok(EditorRange {
        start: position(location.span.start())?,
        end: position(location.span.end())?,
    })
}

/// One deterministic order for batches: source, then primary span,
/// then severity (worst first), then identity, then full structural
/// comparison as the final tiebreak (base.md §7.4).
///
/// Defined once because every batch consumer needs one — the golden
/// corpus first among them — and each inventing its own would
/// diverge. A free function deliberately, not an `Ord` impl: an `Ord`
/// impl would claim *the* natural order of diagnostics, and there is
/// none — this is one batch derivation among possible ones, and
/// ordering a batch for consumption is itself a linearization
/// (base.md §8.4). Total; `Equal` exactly on structural equality;
/// O(structure).
pub fn canonical_order(a: &Diagnostic, b: &Diagnostic) -> Ordering {
    (a.primary().location.source)
        .cmp(&b.primary().location.source)
        .then_with(|| a.primary().location.span.cmp(&b.primary().location.span))
        .then_with(|| b.severity().cmp(&a.severity()))
        .then_with(|| a.id().cmp(&b.id()))
        .then_with(|| a.message().cmp(b.message()))
        .then_with(|| a.primary().message.cmp(&b.primary().message))
        .then_with(|| a.secondary().iter().cmp(b.secondary().iter()))
        .then_with(|| a.notes().cmp(b.notes()))
        .then_with(|| a.helps().cmp(b.helps()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
    use crate::source::SourceSet;
    use crate::span::{ByteOffset, Location, Span};

    const UNEXPECTED: DiagnosticId = DiagnosticId::new("syntax", "unexpected-token");

    #[test]
    fn the_single_span_rendering_is_the_committed_layout() {
        let mut catalog = SourceSet::new();
        let file = catalog
            .add("demo.lp".to_owned(), "p(a).\nq(X) :- r(X)\n".to_owned())
            .expect("small text admits");
        // "r(X)" occupies bytes 14..18, line 2 column 9, one-based.
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            Label {
                location: Location {
                    source: file,
                    span: Span::new(ByteOffset::new(14), ByteOffset::new(18))
                        .expect("ordered endpoints"),
                },
                message: Some("the rule body ends here".to_owned()),
            },
        )
        .expect("non-empty headline");

        let rendered = human(&diagnostic, &catalog);
        let expected = "\
error[syntax::unexpected-token]: expected `.` after the rule body
 --> demo.lp:2:9
  |
2 | q(X) :- r(X)
  |         ^^^^ the rule body ends here
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn an_unresolvable_source_renders_a_named_placeholder() {
        let catalog = SourceSet::new();
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Warning,
            "w".to_owned(),
            Label {
                location: Location {
                    source: crate::source::SourceId::new(7),
                    span: Span::empty(ByteOffset::ZERO),
                },
                message: None,
            },
        )
        .expect("non-empty headline");
        let rendered = human(&diagnostic, &catalog);
        let expected = "\
warning[syntax::unexpected-token]: w
 --> <source 7: unresolved>
";
        assert_eq!(rendered, expected);
    }

    use crate::line::{ColumnEncoding, LineCol, PositionRefusal};
    use crate::source::{NotCharBoundary, SourceFacet, SourceId};

    fn editor_fixture() -> (SourceSet, crate::source::SourceId, Diagnostic) {
        let mut catalog = SourceSet::new();
        let file = catalog
            .add("demo.lp".to_owned(), "p(a).\nq(é) :- r(é)\n".to_owned())
            .expect("small text admits");
        // Line 2 starts at byte 6: q=6 (=7 é=8..10 )=10 ␣=11 :=12
        // -=13 ␣=14 r=15 (=16 é=17..19 )=19; "r(é)" is bytes 15..20.
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            Label {
                location: Location {
                    source: file,
                    span: Span::new(ByteOffset::new(15), ByteOffset::new(20))
                        .expect("ordered endpoints"),
                },
                message: Some("dropped by this projection".to_owned()),
            },
        )
        .expect("non-empty headline")
        .with_secondary(Label {
            location: Location {
                source: file,
                span: Span::new(ByteOffset::new(6), ByteOffset::new(10))
                    .expect("ordered endpoints"),
            },
            message: None,
        })
        .with_note("a note".to_owned())
        .with_help("a help".to_owned());
        (catalog, file, diagnostic)
    }

    #[test]
    fn the_payload_is_typed_self_describing_and_folded() {
        let (catalog, file, diagnostic) = editor_fixture();
        let payload = editor(&diagnostic, &catalog, ColumnEncoding::Utf16Units)
            .expect("a coherent catalog yields a payload");
        assert_eq!(payload.source, file);
        assert_eq!(payload.encoding, ColumnEncoding::Utf16Units);
        assert_eq!(payload.severity, Severity::Error);
        assert_eq!(payload.code, UNEXPECTED);
        // Bytes 15..20 on "q(é) :- r(é)": the é before the span is
        // one UTF-16 unit for two bytes, so columns are 8..12 in
        // UTF-16 units, zero-based.
        assert_eq!(
            payload.range,
            EditorRange {
                start: LineCol { line: 1, col: 8 },
                end: LineCol { line: 1, col: 12 },
            }
        );
        assert_eq!(
            payload.message,
            "expected `.` after the rule body\nnote: a note\nhelp: a help"
        );
        // The message-less secondary ships its location with the
        // headline as its message — nothing dropped silently.
        assert_eq!(payload.related.len(), 1);
        assert_eq!(
            payload.related[0].message,
            "expected `.` after the rule body"
        );
        assert_eq!(
            payload.related[0].range,
            EditorRange {
                start: LineCol { line: 1, col: 0 },
                end: LineCol { line: 1, col: 3 },
            }
        );
    }

    #[test]
    fn the_view_refuses_an_unknown_source() {
        let (_, _, diagnostic) = editor_fixture();
        let empty = SourceSet::new();
        assert_eq!(
            editor(&diagnostic, &empty, ColumnEncoding::Utf16Units),
            Err(EditorRefusal::UnknownSource {
                id: SourceId::new(0)
            })
        );
    }

    #[test]
    fn the_view_refuses_a_completeness_breach() {
        struct NameOnly;
        impl crate::source::Sources for NameOnly {
            fn name(&self, _: SourceId) -> Option<&str> {
                Some("partial.lp")
            }
            fn text(&self, _: SourceId) -> Option<&str> {
                None
            }
            fn line_index(&self, _: SourceId) -> Option<&crate::line::LineIndex> {
                None
            }
        }
        let (_, _, diagnostic) = editor_fixture();
        assert_eq!(
            editor(&diagnostic, &NameOnly, ColumnEncoding::Utf8Bytes),
            Err(EditorRefusal::IncompleteSource {
                id: SourceId::new(0),
                missing: SourceFacet::Text,
            })
        );
    }

    #[test]
    fn the_view_refuses_a_misfit_span_carrying_its_locus() {
        let (catalog, file, _) = editor_fixture();
        // Byte 9 splits the é on line 2 (line starts at 6: q=6, (=7,
        // é=8..10).
        let location = Location {
            source: file,
            span: Span::new(ByteOffset::new(9), ByteOffset::new(10)).expect("ordered endpoints"),
        };
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Warning,
            "w".to_owned(),
            Label {
                location,
                message: None,
            },
        )
        .expect("non-empty headline");
        assert_eq!(
            editor(&diagnostic, &catalog, ColumnEncoding::CodePoints),
            Err(EditorRefusal::Position {
                at: location,
                refusal: PositionRefusal::NotCharBoundary(NotCharBoundary {
                    offset: ByteOffset::new(9)
                }),
            })
        );
    }

    #[test]
    fn editor_refusals_display_the_fixable_question() {
        let refusal = EditorRefusal::IncompleteSource {
            id: SourceId::new(3),
            missing: SourceFacet::Index,
        };
        assert_eq!(
            refusal.to_string(),
            "source 3 resolved partially: missing index"
        );
        let _: &dyn std::error::Error = &refusal;
    }

    use std::cmp::Ordering;

    const OTHER: DiagnosticId = DiagnosticId::new("program", "unknown-name");

    fn diagnostic_at(source: u32, start: u32, severity: Severity, id: DiagnosticId) -> Diagnostic {
        Diagnostic::new(
            id,
            severity,
            "m".to_owned(),
            Label {
                location: Location {
                    source: SourceId::new(source),
                    span: Span::new(ByteOffset::new(start), ByteOffset::new(start + 1))
                        .expect("ordered endpoints"),
                },
                message: None,
            },
        )
        .expect("non-empty headline")
    }

    #[test]
    fn canonical_order_groups_by_source() {
        assert_eq!(
            canonical_order(
                &diagnostic_at(0, 9, Severity::Note, UNEXPECTED),
                &diagnostic_at(1, 0, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
    }

    #[test]
    fn canonical_order_then_primary_span() {
        assert_eq!(
            canonical_order(
                &diagnostic_at(0, 2, Severity::Note, UNEXPECTED),
                &diagnostic_at(0, 5, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
    }

    #[test]
    fn canonical_order_puts_worst_first() {
        assert_eq!(
            canonical_order(
                &diagnostic_at(0, 2, Severity::Error, UNEXPECTED),
                &diagnostic_at(0, 2, Severity::Warning, UNEXPECTED),
            ),
            Ordering::Less
        );
    }

    #[test]
    fn canonical_order_then_identity() {
        assert_eq!(
            canonical_order(
                &diagnostic_at(0, 2, Severity::Error, OTHER),
                &diagnostic_at(0, 2, Severity::Error, UNEXPECTED),
            ),
            Ordering::Less
        );
    }

    #[test]
    fn canonical_order_equal_iff_structural() {
        assert_eq!(
            canonical_order(
                &diagnostic_at(0, 2, Severity::Error, UNEXPECTED),
                &diagnostic_at(0, 2, Severity::Error, UNEXPECTED),
            ),
            Ordering::Equal
        );
        let plain = diagnostic_at(0, 2, Severity::Error, UNEXPECTED);
        let with_note = diagnostic_at(0, 2, Severity::Error, UNEXPECTED).with_note("n".to_owned());
        assert_ne!(canonical_order(&plain, &with_note), Ordering::Equal);
    }

    fn rendering_of(text: &str, primary: Label, secondaries: &[Label]) -> String {
        let mut catalog = SourceSet::new();
        catalog
            .add("demo.lp".to_owned(), text.to_owned())
            .expect("small text admits");
        let mut diagnostic = Diagnostic::new(UNEXPECTED, Severity::Error, "m".to_owned(), primary)
            .expect("non-empty headline");
        for secondary in secondaries {
            diagnostic = diagnostic.with_secondary(secondary.clone());
        }
        human(&diagnostic, &catalog)
    }

    fn label_in_first_source(start: u32, end: u32) -> Label {
        Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(start), ByteOffset::new(end))
                    .expect("ordered endpoints"),
            },
            message: None,
        }
    }

    #[test]
    fn a_secondary_identical_to_the_primary_renders_after_it() {
        // Two labels on one span, one primary and one secondary: the
        // primary's carets come first among equals.
        let rendered = rendering_of(
            "p(a).\n",
            label_in_first_source(0, 4),
            &[label_in_first_source(0, 4)],
        );
        let expected = "\
error[syntax::unexpected-token]: m
 --> demo.lp:1:1
  |
1 | p(a).
  | ^^^^
  | ----
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn a_missing_name_renders_its_placeholder_in_the_header() {
        struct NoName {
            text: String,
            index: crate::line::LineIndex,
        }
        impl crate::source::Sources for NoName {
            fn name(&self, _: SourceId) -> Option<&str> {
                None
            }
            fn text(&self, _: SourceId) -> Option<&str> {
                Some(&self.text)
            }
            fn line_index(&self, _: SourceId) -> Option<&crate::line::LineIndex> {
                Some(&self.index)
            }
        }
        let source = crate::source::Source::new(SourceId::new(0), "p(a).\n".to_owned())
            .expect("small text admits");
        let catalog = NoName {
            text: source.text().to_owned(),
            index: crate::line::LineIndex::of(&source),
        };
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "m".to_owned(),
            label_in_first_source(0, 4),
        )
        .expect("non-empty headline");
        let expected = "\
error[syntax::unexpected-token]: m
 --> <source 0: missing name>:1:1
  |
1 | p(a).
  | ^^^^
";
        assert_eq!(human(&diagnostic, &catalog), expected);
    }

    #[test]
    fn the_gutter_width_follows_the_largest_line_number() {
        // A label on the tenth line: the gutter widens to two columns.
        let rendered = rendering_of(&"a\n".repeat(10), label_in_first_source(18, 19), &[]);
        let expected = "\
error[syntax::unexpected-token]: m
 --> demo.lp:10:1
   |
10 | a
   | ^
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn an_empty_span_at_a_line_start_anchors_on_its_own_line() {
        // Byte 6 is the start of line 2; an empty span there is on
        // line 2, not pulled back onto line 1.
        let rendered = rendering_of("p(a).\nq(X).\n", label_in_first_source(6, 6), &[]);
        let expected = "\
error[syntax::unexpected-token]: m
 --> demo.lp:2:1
  |
2 | q(X).
  | ^
";
        assert_eq!(rendered, expected);
    }
}
