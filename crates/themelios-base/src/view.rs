//! Views: pure derivations over `(&Diagnostic, &impl Sources)`
//! (docs/design/base.md §7) — plain functions over the public model,
//! whose one polymorphism is over the environment, the `Sources` trait.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use crate::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
use crate::line::{ColumnEncoding, LineCol, LineIndex, PositionRefusal};
use crate::source::{Resolution, SourceFacet, SourceId, Sources, Verdict};
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
    Human {
        diagnostic,
        sources,
    }
    .to_string()
}

/// The human rendering as a `Display` value: `human` is its
/// `to_string`. Every row is written through the formatter with `?` —
/// the standard library's own way to build text, the sink's
/// fallibility honest in every signature and the infallible `String`
/// case discharged by std, not here. Private: `Display` on the public
/// surface is reserved for the refusals and the two identity
/// renderings (base.md §8.5).
struct Human<'a, S: Sources> {
    diagnostic: &'a Diagnostic,
    sources: &'a S,
}

/// The unit the human view lays columns out in: code points, so a
/// caret sits under the character it names (base.md §7.1). Named once
/// — `place_labels` positions in it and `line_extent` counts in it.
const HUMAN_COLUMNS: ColumnEncoding = ColumnEncoding::CodePoints;

/// A label's role in its diagnostic (base.md §6.4): the required
/// primary, or one of the secondary set — the distinction the underline
/// marker and the header anchor read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Primary,
    Secondary,
}

impl<S: Sources> fmt::Display for Human<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diagnostic = self.diagnostic;
        writeln!(
            f,
            "{}[{}]: {}",
            diagnostic.severity(),
            diagnostic.id(),
            diagnostic.message()
        )?;
        // One pass groups the labels by source — O(labels · log sources),
        // within the stated O(rendered size) (base.md §7.1, §9) — with
        // the primary inserted at its position in its own block, marked.
        // The primary's source renders first, then every other touched
        // source in identity order.
        let mut blocks: BTreeMap<SourceId, Vec<(&Label, Role)>> = BTreeMap::new();
        for label in diagnostic.secondary() {
            blocks
                .entry(label.location.source)
                .or_default()
                .push((label, Role::Secondary));
        }
        let primary = diagnostic.primary();
        let mut own_block = blocks.remove(&primary.location.source).unwrap_or_default();
        let at = own_block.partition_point(|(label, _)| *label < primary);
        own_block.insert(at, (primary, Role::Primary));
        render_block(f, primary.location.source, &own_block, self.sources)?;
        for (&source, labels) in &blocks {
            render_block(f, source, labels, self.sources)?;
        }
        for note in diagnostic.notes() {
            writeln!(f, "  = note: {note}")?;
        }
        for help in diagnostic.helps() {
            writeln!(f, "  = help: {help}")?;
        }
        Ok(())
    }
}

/// One label the block can render: both span ends positioned.
struct Fitting<'a> {
    label: &'a Label,
    role: Role,
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
    f: &mut fmt::Formatter<'_>,
    source: SourceId,
    labels: &[(&Label, Role)],
    sources: &impl Sources,
) -> fmt::Result {
    let resolution = Resolution::of(sources, source);
    if resolution.is_unknown() {
        return writeln!(f, " --> <source {}: unresolved>", source.get());
    }
    // A partial resolution is a completeness breach (base.md §3.4); the
    // placeholder names the missing facet, and every coherent part
    // still renders: a missing name still gets its snippet.
    let display = match resolution.name {
        Some(name) => name.to_owned(),
        None => format!(
            "<source {}: missing {}>",
            source.get(),
            SourceFacet::Name.word()
        ),
    };
    let Some((text, index)) = resolution.text.zip(resolution.index) else {
        writeln!(f, " --> {display}")?;
        // The first facet lacking past the name, in the law's order,
        // names the breach in one gutter row.
        if let Some(facet) = resolution
            .missing()
            .find(|facet| *facet != SourceFacet::Name)
        {
            writeln!(f, "  | <source {}: missing {}>", source.get(), facet.word())?;
        }
        return Ok(());
    };

    let (fitting, misfits) = place_labels(index, text, labels);

    // Header coordinates: the primary if it fits, else the first
    // fitting label, else none.
    let anchor = fitting
        .iter()
        .find(|fit| fit.role == Role::Primary)
        .or_else(|| fitting.first());
    match anchor {
        Some(fit) => writeln!(
            f,
            " --> {display}:{}:{}",
            fit.start.line + 1,
            fit.start.col + 1
        )?,
        None => writeln!(f, " --> {display}")?,
    }

    // Only labeled lines render; `..` marks an elision. One pass
    // maps each rendered line to the labels touching it, in position
    // order, so the walk below is flat — O(rendered rows), the
    // stated cost (base.md §7.1, §9) — instead of scanning every
    // label per line.
    let mut rows: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
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
    writeln!(f, "{pad} |")?;

    let mut previous: Option<u32> = None;
    for (&line, touching) in &rows {
        if let Some(previous) = previous
            && line != previous + 1
        {
            writeln!(f, "{pad}..")?;
        }
        previous = Some(line);
        render_line(f, index, text, line, width)?;
        for &fit_index in touching {
            render_underline(f, &fitting[fit_index], index, text, line, &pad)?;
        }
    }
    for (label, refusal) in misfits {
        let span = label.location.span;
        // Worded here rather than through the refusal's Display: the
        // placeholder's words are the golden corpus's, held apart from
        // refusal Display texts, which are presentation and may move
        // (base.md §8.5).
        let reason = match refusal {
            PositionRefusal::OutOfBounds(oob) => {
                format!("the text ends at byte {}", oob.max.get())
            }
            PositionRefusal::NotCharBoundary(ncb) => {
                format!("byte {} splits a character", ncb.offset.get())
            }
        };
        writeln!(
            f,
            "{pad} | <label {}..{} does not fit source {}: {reason}>",
            span.start().get(),
            span.end().get(),
            source.get()
        )?;
    }
    Ok(())
}

/// Positions both ends of every label against the resolved index, in
/// code points; a label whose end refuses positioning is a misfit,
/// reported rather than dropped.
fn place_labels<'a>(
    index: &LineIndex,
    text: &str,
    labels: &[(&'a Label, Role)],
) -> (Vec<Fitting<'a>>, Vec<Misfit<'a>>) {
    let mut fitting = Vec::new();
    let mut misfits = Vec::new();
    for &(label, role) in labels {
        let span = label.location.span;
        match (
            index.position(span.start(), HUMAN_COLUMNS),
            index.position(span.end(), HUMAN_COLUMNS),
        ) {
            (Ok(start), Ok(mut end)) => {
                if end.line > start.line && end.col == 0 {
                    end.line -= 1;
                    end.col = line_extent(index, text, end.line);
                }
                fitting.push(Fitting {
                    label,
                    role,
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
fn render_line(
    f: &mut fmt::Formatter<'_>,
    index: &LineIndex,
    text: &str,
    line: u32,
    width: usize,
) -> fmt::Result {
    let content = index
        .line_span(line)
        .ok()
        .and_then(|span| text.get(span.start().get() as usize..span.end().get() as usize))
        .unwrap_or("<line does not fit the resolved text>");
    let number = line + 1;
    writeln!(f, "{number:>width$} | {content}")
}

/// One label's underline on one line it touches: `^` for the primary,
/// `-` for a secondary, covering the label's overlap with the line's
/// content — clamped, with a minimum width of one on the label's anchor
/// line and on its last touched line, so an empty last line still
/// carries the row the message follows: nothing is silently dropped
/// (base.md §7.1).
fn render_underline(
    f: &mut fmt::Formatter<'_>,
    fit: &Fitting<'_>,
    index: &LineIndex,
    text: &str,
    line: u32,
    pad: &str,
) -> fmt::Result {
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
    let anchored = line == fit.start.line || line == fit.end.line;
    let width_cols = to.saturating_sub(from);
    if width_cols == 0 && !anchored {
        return Ok(());
    }
    let marker = match fit.role {
        Role::Primary => '^',
        Role::Secondary => '-',
    };
    let underline = marker.to_string().repeat(width_cols.max(1) as usize);
    write!(f, "{pad} | {}{underline}", " ".repeat(from as usize))?;
    if line == fit.end.line
        && let Some(message) = &fit.label.message
    {
        write!(f, " {message}")?;
    }
    writeln!(f)
}

/// A line's content extent in `HUMAN_COLUMNS` — `chars()` counts code
/// points — taken from the rendered content rather than the index, so
/// an underline never outruns what the row shows; fallibly: zero when
/// the index and text disagree (the coherence trust boundary).
fn line_extent(index: &LineIndex, text: &str, line: u32) -> u32 {
    index
        .line_span(line)
        .ok()
        .and_then(|span| text.get(span.start().get() as usize..span.end().get() as usize))
        // The cast cannot truncate: the content is a slice of one line
        // of an admitted text, at most Source::MAX_LEN bytes, and a
        // character count never exceeds a byte count.
        .map_or(0, |content| content.chars().count() as u32)
}

/// The editor-protocol payload as a typed value. Serialization is the
/// consumer's step: this crate ships shapes, not bytes (base.md §7.2).
///
/// A typed intermediate shape at the last typed point before bytes —
/// bytes here would mean a serializer the zero-dependency closure
/// forecloses. What keeps this from becoming a second model: exactly
/// one pure function produces it, nothing in this crate holds it, it
/// mirrors the protocol's categories, and nothing maintains it — every
/// instance is a fresh derivation. base.md §7.2 carries the argument.
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
                write!(
                    f,
                    "source {} resolved partially: missing {}",
                    id.get(),
                    missing.word()
                )
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
    match Resolution::of(sources, id).verdict() {
        Verdict::Unknown => Err(EditorRefusal::UnknownSource { id }),
        Verdict::Partial(missing) => Err(EditorRefusal::IncompleteSource { id, missing }),
        Verdict::Complete(index) => Ok(index),
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
    use std::cmp::Ordering;

    use super::*;
    use crate::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
    use crate::line::{ColumnEncoding, LineCol, PositionRefusal};
    use crate::source::{NotCharBoundary, SourceFacet, SourceId, SourceSet};
    use crate::span::{ByteOffset, Location, Span};

    const UNEXPECTED: DiagnosticId = DiagnosticId::new("syntax", "unexpected-token");
    const OTHER: DiagnosticId = DiagnosticId::new("program", "unknown-name");

    // Fixtures, in the order the tests below use them.

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

    // The human view (base.md §7.1).

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

    #[test]
    fn equal_secondary_renders_after_the_primary() {
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
    fn missing_name_placeholder_lands_in_the_header() {
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
    fn empty_span_at_line_start_stays_on_its_line() {
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

    #[test]
    fn a_label_starting_at_a_line_end_marks_that_line() {
        // The label begins at the end of line 1 (covering its
        // terminator) and ends inside line 2: the anchor line still
        // shows a minimum-width marker where the label begins.
        let rendered = rendering_of("ab\ncd", label_in_first_source(2, 4), &[]);
        let expected = "\
error[syntax::unexpected-token]: m
 --> demo.lp:1:3
  |
1 | ab
  |   ^
2 | cd
  | ^
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn a_message_lands_on_an_empty_last_line() {
        // The label covers line 1 and the empty line 2's terminator, so
        // line 2 is its last touched line: the message lands there, on a
        // minimum-width row, rather than being dropped.
        let mut catalog = SourceSet::new();
        catalog
            .add("demo.lp".to_owned(), "a\n\nb".to_owned())
            .expect("small text admits");
        let diagnostic = Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "m".to_owned(),
            Label {
                location: Location {
                    source: SourceId::new(0),
                    span: Span::new(ByteOffset::new(0), ByteOffset::new(3))
                        .expect("ordered endpoints"),
                },
                message: Some("here".to_owned()),
            },
        )
        .expect("non-empty headline");
        let expected = "\
error[syntax::unexpected-token]: m
 --> demo.lp:1:1
  |
1 | a
  | ^
2 | 
  | ^ here
";
        assert_eq!(human(&diagnostic, &catalog), expected);
    }

    // The editor view (base.md §7.2).

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

    // The canonical order (base.md §7.4).

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
}
