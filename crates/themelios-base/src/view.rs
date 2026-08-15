//! Views: pure derivations over `(&Diagnostic, &impl Sources)`
//! (docs/design/base.md §7). There is deliberately no view trait —
//! the open extension point for a new view is the model being public
//! plain data; anyone writes a function over it. The polymorphism a
//! view does need is over its *environment*, and that is the
//! `Sources` trait.

use crate::diagnostic::{Diagnostic, Label};
use crate::line::{ColumnEncoding, LineCol, LineIndex, PositionRefusal};
use crate::source::{SourceId, Sources};

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
}
