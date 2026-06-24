use std::collections::HashSet;

use crate::model::{Diff, DiffFile, FileStatus, Hunk, Line, LineKind};
use crate::tree::{FlatEntry, FileTree};

pub fn make_line(kind: LineKind, content: &str, old_num: Option<usize>, new_num: Option<usize>) -> Line {
    Line {
        kind,
        content: content.to_string(),
        old_num,
        new_num,
    }
}

pub fn make_hunk(header: &str, lines: Vec<Line>) -> Hunk {
    Hunk {
        header: header.to_string(),
        lines,
    }
}

pub fn make_file(path: &str, hunks: Vec<Hunk>) -> DiffFile {
    let added = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| matches!(l.kind, LineKind::Added))
        .count();
    let removed = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| matches!(l.kind, LineKind::Removed))
        .count();

    DiffFile {
        path: path.to_string(),
        status: FileStatus::Modified,
        hunks,
        added,
        removed,
        viewed: false,
        is_binary: false,
    }
}

pub fn make_diff(files: Vec<DiffFile>) -> Diff {
    Diff { files }
}

/// Build a simple Diff with N files, each having one hunk with a context line.
pub fn make_simple_diff(n: usize) -> Diff {
    let files = (0..n)
        .map(|i| {
            make_file(
                &format!("src/file{}.rs", i),
                vec![make_hunk(
                    &format!("@@ -1,1 +1,1 @@ fn file{}()", i),
                    vec![make_line(LineKind::Context, "// content", Some(1), Some(1))],
                )],
            )
        })
        .collect();
    make_diff(files)
}

/// Flatten a diff's files into FlatEntries using FileTree (no collapsed folders).
pub fn flat_entries_for(diff: &Diff) -> Vec<FlatEntry> {
    let tree = FileTree::from_files(&diff.files);
    tree.flatten(&HashSet::new())
}
