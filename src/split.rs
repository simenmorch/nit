use similar::{DiffTag, TextDiff};

use crate::model::{self, Hunk, LineKind};

/// A segment of a line that is either changed or unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineSpan {
    pub start: usize,
    pub end: usize,
    pub changed: bool,
}

/// One row in the side-by-side view.
#[derive(Debug)]
pub enum SplitRow<'a> {
    HunkHeader(&'a str),
    Context(&'a model::Line),
    Paired {
        left: &'a model::Line,
        right: &'a model::Line,
        left_spans: Vec<InlineSpan>,
        right_spans: Vec<InlineSpan>,
    },
    LeftOnly(&'a model::Line),
    RightOnly(&'a model::Line),
}

/// Build side-by-side rows from hunks using simple 1:1 pairing.
pub fn build_split_rows(hunks: &[Hunk]) -> Vec<SplitRow<'_>> {
    let mut rows = Vec::new();
    for hunk in hunks {
        rows.push(SplitRow::HunkHeader(&hunk.header));
        flush_hunk_lines(&hunk.lines, &mut rows);
    }
    rows
}

/// Walk lines in a hunk, grouping consecutive removed/added runs, and emit rows.
fn flush_hunk_lines<'a>(lines: &'a [model::Line], rows: &mut Vec<SplitRow<'a>>) {
    let mut removed: Vec<&'a model::Line> = Vec::new();
    let mut added: Vec<&'a model::Line> = Vec::new();

    for line in lines {
        match line.kind {
            LineKind::Removed => {
                removed.push(line);
            }
            LineKind::Added => {
                added.push(line);
            }
            LineKind::Context => {
                flush_pair(&mut removed, &mut added, rows);
                rows.push(SplitRow::Context(line));
            }
        }
    }
    flush_pair(&mut removed, &mut added, rows);
}

/// Emit paired/left-only/right-only rows from accumulated removed+added lines, then clear both.
fn flush_pair<'a>(
    removed: &mut Vec<&'a model::Line>,
    added: &mut Vec<&'a model::Line>,
    rows: &mut Vec<SplitRow<'a>>,
) {
    let paired = removed.len().min(added.len());

    for i in 0..paired {
        let (left_spans, right_spans) =
            compute_inline_spans(&removed[i].content, &added[i].content);
        rows.push(SplitRow::Paired {
            left: removed[i],
            right: added[i],
            left_spans,
            right_spans,
        });
    }

    for line in removed.drain(paired..) {
        rows.push(SplitRow::LeftOnly(line));
    }
    for line in added.drain(paired..) {
        rows.push(SplitRow::RightOnly(line));
    }

    removed.clear();
    added.clear();
}

/// Character-level diff between two line contents.
/// Returns inline spans for each side marking changed vs unchanged segments.
pub fn compute_inline_spans(old: &str, new: &str) -> (Vec<InlineSpan>, Vec<InlineSpan>) {
    let diff = TextDiff::from_chars(old, new);
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();
    let mut old_pos: usize = 0;
    let mut new_pos: usize = 0;

    for op in diff.ops() {
        let tag = op.tag();
        let old_range = op.old_range();
        let new_range = op.new_range();

        // Convert char ranges to byte ranges
        let old_start = char_offset_to_byte(old, old_range.start);
        let old_end = char_offset_to_byte(old, old_range.end);
        let new_start = char_offset_to_byte(new, new_range.start);
        let new_end = char_offset_to_byte(new, new_range.end);

        match tag {
            DiffTag::Equal => {
                if old_end > old_start {
                    old_spans.push(InlineSpan { start: old_start, end: old_end, changed: false });
                }
                if new_end > new_start {
                    new_spans.push(InlineSpan { start: new_start, end: new_end, changed: false });
                }
                old_pos = old_end;
                new_pos = new_end;
            }
            DiffTag::Delete => {
                if old_end > old_start {
                    old_spans.push(InlineSpan { start: old_start, end: old_end, changed: true });
                }
                old_pos = old_end;
            }
            DiffTag::Insert => {
                if new_end > new_start {
                    new_spans.push(InlineSpan { start: new_start, end: new_end, changed: true });
                }
                new_pos = new_end;
            }
            DiffTag::Replace => {
                if old_end > old_start {
                    old_spans.push(InlineSpan { start: old_start, end: old_end, changed: true });
                }
                if new_end > new_start {
                    new_spans.push(InlineSpan { start: new_start, end: new_end, changed: true });
                }
                old_pos = old_end;
                new_pos = new_end;
            }
        }
    }

    // Ensure trailing bytes are covered
    if old_pos < old.len() {
        old_spans.push(InlineSpan { start: old_pos, end: old.len(), changed: false });
    }
    if new_pos < new.len() {
        new_spans.push(InlineSpan { start: new_pos, end: new.len(), changed: false });
    }

    (old_spans, new_spans)
}

/// Convert a character index to a byte offset in a string.
fn char_offset_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

/// Count total rows in side-by-side mode for scroll bounds.
pub fn split_row_count(hunks: &[Hunk]) -> usize {
    build_split_rows(hunks).len()
}

/// Compute the starting row index of each hunk in side-by-side mode.
pub fn hunk_start_rows(hunks: &[Hunk]) -> Vec<usize> {
    let rows = build_split_rows(hunks);
    let mut starts = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if matches!(row, SplitRow::HunkHeader(_)) {
            starts.push(i);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hunk, Line, LineKind};

    fn line(kind: LineKind, content: &str, old: Option<usize>, new: Option<usize>) -> Line {
        Line { kind, content: content.to_string(), old_num: old, new_num: new }
    }

    fn hunk(header: &str, lines: Vec<Line>) -> Hunk {
        Hunk { header: header.to_string(), lines }
    }

    #[test]
    fn context_only() {
        let hunks = vec![hunk("@@ -1 +1 @@", vec![
            line(LineKind::Context, "a", Some(1), Some(1)),
            line(LineKind::Context, "b", Some(2), Some(2)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 3); // header + 2 context
        assert!(matches!(rows[0], SplitRow::HunkHeader(_)));
        assert!(matches!(rows[1], SplitRow::Context(_)));
        assert!(matches!(rows[2], SplitRow::Context(_)));
    }

    #[test]
    fn pure_add() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Added, "new1", None, Some(1)),
            line(LineKind::Added, "new2", None, Some(2)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[1], SplitRow::RightOnly(_)));
        assert!(matches!(rows[2], SplitRow::RightOnly(_)));
    }

    #[test]
    fn pure_remove() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Removed, "old1", Some(1), None),
            line(LineKind::Removed, "old2", Some(2), None),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[1], SplitRow::LeftOnly(_)));
        assert!(matches!(rows[2], SplitRow::LeftOnly(_)));
    }

    #[test]
    fn equal_pairing() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Removed, "old1", Some(1), None),
            line(LineKind::Removed, "old2", Some(2), None),
            line(LineKind::Removed, "old3", Some(3), None),
            line(LineKind::Added, "new1", None, Some(1)),
            line(LineKind::Added, "new2", None, Some(2)),
            line(LineKind::Added, "new3", None, Some(3)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 4); // header + 3 paired
        assert!(matches!(rows[1], SplitRow::Paired { .. }));
        assert!(matches!(rows[2], SplitRow::Paired { .. }));
        assert!(matches!(rows[3], SplitRow::Paired { .. }));
    }

    #[test]
    fn unequal_pairing_more_removed() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Removed, "old1", Some(1), None),
            line(LineKind::Removed, "old2", Some(2), None),
            line(LineKind::Removed, "old3", Some(3), None),
            line(LineKind::Removed, "old4", Some(4), None),
            line(LineKind::Removed, "old5", Some(5), None),
            line(LineKind::Added, "new1", None, Some(1)),
            line(LineKind::Added, "new2", None, Some(2)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 6); // header + 2 paired + 3 left-only
        assert!(matches!(rows[1], SplitRow::Paired { .. }));
        assert!(matches!(rows[2], SplitRow::Paired { .. }));
        assert!(matches!(rows[3], SplitRow::LeftOnly(_)));
        assert!(matches!(rows[4], SplitRow::LeftOnly(_)));
        assert!(matches!(rows[5], SplitRow::LeftOnly(_)));
    }

    #[test]
    fn unequal_pairing_more_added() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Removed, "old1", Some(1), None),
            line(LineKind::Added, "new1", None, Some(1)),
            line(LineKind::Added, "new2", None, Some(2)),
            line(LineKind::Added, "new3", None, Some(3)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 4); // header + 1 paired + 2 right-only
        assert!(matches!(rows[1], SplitRow::Paired { .. }));
        assert!(matches!(rows[2], SplitRow::RightOnly(_)));
        assert!(matches!(rows[3], SplitRow::RightOnly(_)));
    }

    #[test]
    fn mixed_context_and_changes() {
        let hunks = vec![hunk("@@", vec![
            line(LineKind::Context, "ctx1", Some(1), Some(1)),
            line(LineKind::Removed, "old", Some(2), None),
            line(LineKind::Added, "new", None, Some(2)),
            line(LineKind::Context, "ctx2", Some(3), Some(3)),
        ])];
        let rows = build_split_rows(&hunks);
        assert_eq!(rows.len(), 4); // header + ctx + paired + ctx
        assert!(matches!(rows[1], SplitRow::Context(_)));
        assert!(matches!(rows[2], SplitRow::Paired { .. }));
        assert!(matches!(rows[3], SplitRow::Context(_)));
    }

    #[test]
    fn inline_spans_simple() {
        let (old_spans, new_spans) = compute_inline_spans("hello world", "hello earth");
        // "hello " is unchanged, "world" vs "earth" differs
        assert!(!old_spans.is_empty());
        assert!(!new_spans.is_empty());

        // The unchanged prefix should be marked as not changed
        assert!(!old_spans[0].changed);
        assert!(!new_spans[0].changed);

        // There should be a changed segment
        assert!(old_spans.iter().any(|s| s.changed));
        assert!(new_spans.iter().any(|s| s.changed));
    }

    #[test]
    fn inline_spans_identical() {
        let (old_spans, new_spans) = compute_inline_spans("same text", "same text");
        assert!(old_spans.iter().all(|s| !s.changed));
        assert!(new_spans.iter().all(|s| !s.changed));
    }

    #[test]
    fn inline_spans_completely_different() {
        let (old_spans, new_spans) = compute_inline_spans("abc", "xyz");
        assert!(old_spans.iter().all(|s| s.changed));
        assert!(new_spans.iter().all(|s| s.changed));
    }

    #[test]
    fn inline_spans_empty_old() {
        let (old_spans, new_spans) = compute_inline_spans("", "added");
        assert!(old_spans.is_empty());
        assert!(!new_spans.is_empty());
        assert!(new_spans[0].changed);
    }

    #[test]
    fn inline_spans_empty_new() {
        let (old_spans, new_spans) = compute_inline_spans("removed", "");
        assert!(!old_spans.is_empty());
        assert!(old_spans[0].changed);
        assert!(new_spans.is_empty());
    }

    #[test]
    fn split_row_count_matches() {
        let hunks = vec![
            hunk("@@", vec![
                line(LineKind::Context, "a", Some(1), Some(1)),
                line(LineKind::Removed, "b", Some(2), None),
                line(LineKind::Added, "c", None, Some(2)),
            ]),
            hunk("@@", vec![
                line(LineKind::Added, "d", None, Some(10)),
            ]),
        ];
        let count = split_row_count(&hunks);
        let rows = build_split_rows(&hunks);
        assert_eq!(count, rows.len());
    }

    #[test]
    fn multi_hunk() {
        let hunks = vec![
            hunk("@@ -1 +1 @@", vec![
                line(LineKind::Context, "a", Some(1), Some(1)),
            ]),
            hunk("@@ -10 +10 @@", vec![
                line(LineKind::Removed, "b", Some(10), None),
            ]),
        ];
        let rows = build_split_rows(&hunks);
        // header + context + header + left-only = 4
        assert_eq!(rows.len(), 4);
        assert!(matches!(rows[0], SplitRow::HunkHeader(_)));
        assert!(matches!(rows[1], SplitRow::Context(_)));
        assert!(matches!(rows[2], SplitRow::HunkHeader(_)));
        assert!(matches!(rows[3], SplitRow::LeftOnly(_)));
    }
}
