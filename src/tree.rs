use std::collections::{BTreeMap, HashSet};

use crate::model::DiffFile;

/// A hierarchical view of the diff's file paths.
///
/// Folders and files are kept in separate maps so that a path segment can be
/// both — e.g. a commit that deletes the file `foo` and adds `foo/bar.rs`.
/// With a single map keyed by name, one of the two would silently overwrite or
/// be swallowed by the other, hiding files that the status bar still counts.
#[derive(Default)]
pub struct FileTree {
    folders: BTreeMap<String, FileTree>,
    files: BTreeMap<String, usize>,
}

pub struct FlatEntry {
    pub depth: usize,
    pub kind: FlatEntryKind,
    /// For each ancestor depth (0..depth), true if that ancestor has more
    /// siblings after this branch — meaning a vertical `│` should be drawn
    /// in that column. Length == depth.
    pub ancestor_has_next: Vec<bool>,
    /// True if this entry is the last child at its own depth.
    pub is_last_sibling: bool,
}

pub enum FlatEntryKind {
    Folder {
        path: String,
        name: String,
        expanded: bool,
    },
    File {
        file_index: usize,
        name: String,
    },
}

impl FileTree {
    pub fn from_files(files: &[DiffFile]) -> Self {
        let mut root = FileTree::default();
        for (i, file) in files.iter().enumerate() {
            let parts: Vec<&str> = file.path.split('/').collect();
            root.insert_parts(&parts, i);
        }
        root
    }

    fn insert_parts(&mut self, parts: &[&str], file_index: usize) {
        if parts.len() == 1 {
            self.files.insert(parts[0].to_string(), file_index);
        } else {
            self.folders
                .entry(parts[0].to_string())
                .or_default()
                .insert_parts(&parts[1..], file_index);
        }
    }

    pub fn flatten(&self, collapsed: &HashSet<String>) -> Vec<FlatEntry> {
        self.flatten_inner(collapsed, "", 0, &[])
    }

    fn flatten_inner(
        &self,
        collapsed: &HashSet<String>,
        prefix: &str,
        depth: usize,
        ancestor_has_next: &[bool],
    ) -> Vec<FlatEntry> {
        let mut result = Vec::new();

        // Folders first, then files (BTreeMap keeps alphabetical order within each group).
        // Total siblings emitted at this depth determines is_last_sibling for each.
        let folder_count = self.folders.len();
        let total = folder_count + self.files.len();

        for (i, (name, subtree)) in self.folders.iter().enumerate() {
            // Collapse single-folder chains: walk down while the current folder
            // contains exactly one entry and that entry is itself a folder.
            let mut merged_segments: Vec<String> = vec![name.clone()];
            let mut current = subtree;
            while current.files.is_empty() && current.folders.len() == 1 {
                let (only_name, only_subtree) = current.folders.iter().next().unwrap();
                merged_segments.push(only_name.clone());
                current = only_subtree;
            }

            let display_name = merged_segments.join("/");
            let full_path = if prefix.is_empty() {
                display_name.clone()
            } else {
                format!("{}/{}", prefix, display_name)
            };
            let expanded = !collapsed.contains(&full_path);
            let is_last_sibling = i + 1 == total;
            result.push(FlatEntry {
                depth,
                kind: FlatEntryKind::Folder {
                    path: full_path.clone(),
                    name: display_name,
                    expanded,
                },
                ancestor_has_next: ancestor_has_next.to_vec(),
                is_last_sibling,
            });
            if expanded {
                let mut child_ancestors = ancestor_has_next.to_vec();
                child_ancestors.push(!is_last_sibling);
                result.extend(current.flatten_inner(
                    collapsed,
                    &full_path,
                    depth + 1,
                    &child_ancestors,
                ));
            }
        }

        for (j, (name, file_index)) in self.files.iter().enumerate() {
            let i = folder_count + j;
            let is_last_sibling = i + 1 == total;
            result.push(FlatEntry {
                depth,
                kind: FlatEntryKind::File {
                    file_index: *file_index,
                    name: name.clone(),
                },
                ancestor_has_next: ancestor_has_next.to_vec(),
                is_last_sibling,
            });
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    fn entry_name(entry: &FlatEntry) -> &str {
        match &entry.kind {
            FlatEntryKind::Folder { name, .. } => name,
            FlatEntryKind::File { name, .. } => name,
        }
    }

    fn is_folder(entry: &FlatEntry) -> bool {
        matches!(entry.kind, FlatEntryKind::Folder { .. })
    }

    /// Every file in the diff must be reachable in the flattened tree,
    /// otherwise the sidebar silently hides changes the status bar still counts.
    fn assert_all_files_reachable(diff: &crate::model::Diff) {
        let flat = flat_entries_for(diff);
        let mut found: Vec<usize> = flat
            .iter()
            .filter_map(|e| match &e.kind {
                FlatEntryKind::File { file_index, .. } => Some(*file_index),
                _ => None,
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        let expected: Vec<usize> = (0..diff.files.len()).collect();
        assert_eq!(
            found,
            expected,
            "some files are missing from the tree: {:?}",
            diff.files
                .iter()
                .enumerate()
                .filter(|(i, _)| !found.contains(i))
                .map(|(_, f)| &f.path)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn file_then_folder_with_same_name_keeps_both() {
        // A commit that deletes file `foo` and adds `foo/bar.rs`.
        let diff = make_diff(vec![
            make_file("foo", vec![]),
            make_file("foo/bar.rs", vec![]),
        ]);
        assert_all_files_reachable(&diff);
    }

    #[test]
    fn folder_then_file_with_same_name_keeps_both() {
        // Same collision, opposite insertion order.
        let diff = make_diff(vec![
            make_file("foo/bar.rs", vec![]),
            make_file("foo", vec![]),
        ]);
        assert_all_files_reachable(&diff);
    }

    #[test]
    fn deep_collision_keeps_all_files() {
        let diff = make_diff(vec![
            make_file("a/b", vec![]),
            make_file("a/b/c.rs", vec![]),
            make_file("a/b/d.rs", vec![]),
        ]);
        assert_all_files_reachable(&diff);
    }

    #[test]
    fn single_file_no_nesting() {
        let diff = make_diff(vec![make_file("foo.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        assert_eq!(flat.len(), 1);
        assert_eq!(entry_name(&flat[0]), "foo.rs");
        assert!(!is_folder(&flat[0]));
        assert_eq!(flat[0].depth, 0);
    }

    #[test]
    fn nested_file_creates_folder() {
        let diff = make_diff(vec![make_file("src/main.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        assert_eq!(flat.len(), 2);
        assert!(is_folder(&flat[0]));
        assert_eq!(entry_name(&flat[0]), "src");
        assert_eq!(flat[0].depth, 0);
        assert_eq!(entry_name(&flat[1]), "main.rs");
        assert_eq!(flat[1].depth, 1);
    }

    #[test]
    fn shared_prefix_single_folder() {
        let diff = make_diff(vec![
            make_file("src/a.rs", vec![]),
            make_file("src/b.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // src/ folder, then a.rs, b.rs
        assert_eq!(flat.len(), 3);
        assert!(is_folder(&flat[0]));
        assert_eq!(entry_name(&flat[1]), "a.rs");
        assert_eq!(entry_name(&flat[2]), "b.rs");
    }

    #[test]
    fn deeply_nested() {
        let diff = make_diff(vec![make_file("a/b/c/d.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        // Chain collapses: "a/b/c" folder at depth 0 + "d.rs" file at depth 1
        assert_eq!(flat.len(), 2);
        assert_eq!(entry_name(&flat[0]), "a/b/c");
        assert_eq!(flat[0].depth, 0);
        assert_eq!(entry_name(&flat[1]), "d.rs");
        assert_eq!(flat[1].depth, 1);
    }

    #[test]
    fn collapse_hides_children() {
        let diff = make_diff(vec![
            make_file("src/a.rs", vec![]),
            make_file("src/b.rs", vec![]),
        ]);
        let tree = FileTree::from_files(&diff.files);
        let mut collapsed = HashSet::new();
        collapsed.insert("src".to_string());
        let flat = tree.flatten(&collapsed);
        // Only the collapsed folder entry
        assert_eq!(flat.len(), 1);
        assert!(is_folder(&flat[0]));
        if let FlatEntryKind::Folder { expanded, .. } = &flat[0].kind {
            assert!(!expanded);
        }
    }

    #[test]
    fn collapse_inner_folder_keeps_outer() {
        let diff = make_diff(vec![
            make_file("src/sub/a.rs", vec![]),
            make_file("src/top.rs", vec![]),
        ]);
        let tree = FileTree::from_files(&diff.files);
        let mut collapsed = HashSet::new();
        collapsed.insert("src/sub".to_string());
        let flat = tree.flatten(&collapsed);
        // src (expanded) -> sub (collapsed) -> top.rs
        assert_eq!(flat.len(), 3);
        assert_eq!(entry_name(&flat[0]), "src");
        assert_eq!(entry_name(&flat[1]), "sub"); // collapsed, no children
        assert_eq!(entry_name(&flat[2]), "top.rs");
    }

    #[test]
    fn empty_diff_empty_flat() {
        let diff = make_diff(vec![]);
        let flat = flat_entries_for(&diff);
        assert!(flat.is_empty());
    }

    #[test]
    fn folders_before_files() {
        let diff = make_diff(vec![
            make_file("zebra.rs", vec![]),
            make_file("alpha/inner.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // Folder "alpha" should come before file "zebra.rs"
        assert!(is_folder(&flat[0]));
        assert_eq!(entry_name(&flat[0]), "alpha");
    }

    #[test]
    fn file_indices_are_correct() {
        let diff = make_diff(vec![make_file("b.rs", vec![]), make_file("a.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        // BTreeMap sorts alphabetically: a.rs (file_index=1), b.rs (file_index=0)
        if let FlatEntryKind::File { file_index, .. } = &flat[0].kind {
            assert_eq!(*file_index, 1); // a.rs was index 1 in the original vec
        }
        if let FlatEntryKind::File { file_index, .. } = &flat[1].kind {
            assert_eq!(*file_index, 0); // b.rs was index 0
        }
    }

    // ── Single-child folder-chain collapsing ──

    #[test]
    fn single_folder_child_chain_is_merged() {
        let diff = make_diff(vec![make_file("a/b/c/leaf.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        // 2 entries: "a/b/c" folder at depth 0, "leaf.rs" file at depth 1
        assert_eq!(flat.len(), 2);
        assert!(is_folder(&flat[0]));
        assert_eq!(entry_name(&flat[0]), "a/b/c");
        assert_eq!(flat[0].depth, 0);
        assert_eq!(entry_name(&flat[1]), "leaf.rs");
        assert_eq!(flat[1].depth, 1);
    }

    #[test]
    fn chain_breaks_when_folder_has_multiple_children() {
        // src/ has two children (foo/ and bar.rs) so src/ does NOT merge with foo.
        let diff = make_diff(vec![
            make_file("src/foo/leaf.rs", vec![]),
            make_file("src/bar.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // src/ folder, then foo/ folder (single file child → not merged), then leaf.rs, then bar.rs
        assert_eq!(flat.len(), 4);
        assert_eq!(entry_name(&flat[0]), "src");
        assert_eq!(entry_name(&flat[1]), "foo");
        assert_eq!(entry_name(&flat[2]), "leaf.rs");
        assert_eq!(entry_name(&flat[3]), "bar.rs");
    }

    #[test]
    fn collapsed_chain_path_is_full_for_collapse_toggle() {
        // Folder path stored in FlatEntryKind::Folder must be the merged path
        // so that toggle_collapse / collapsed HashSet keying still works.
        let diff = make_diff(vec![
            make_file("a/b/x.rs", vec![]),
            make_file("a/b/y.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // a/b folder (merged), then x.rs, then y.rs
        assert_eq!(flat.len(), 3);
        if let FlatEntryKind::Folder { path, name, .. } = &flat[0].kind {
            assert_eq!(path, "a/b");
            assert_eq!(name, "a/b");
        } else {
            panic!("expected folder");
        }
    }

    // ── Tree connector sibling info ──

    #[test]
    fn last_sibling_flag_correct_for_root() {
        let diff = make_diff(vec![make_file("a.rs", vec![]), make_file("b.rs", vec![])]);
        let flat = flat_entries_for(&diff);
        assert!(!flat[0].is_last_sibling); // a.rs
        assert!(flat[1].is_last_sibling); // b.rs
    }

    #[test]
    fn ancestor_has_next_drawn_for_non_last_parents() {
        // Two folders at root, each with one file:
        //   src/foo.rs   (src is not last → for foo.rs ancestor_has_next = [true])
        //   tests/bar.rs (tests is last → for bar.rs ancestor_has_next = [false])
        let diff = make_diff(vec![
            make_file("src/foo.rs", vec![]),
            make_file("tests/bar.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // [src, foo.rs, tests, bar.rs]
        assert_eq!(flat[1].ancestor_has_next, vec![true]); // src has tests after it
        assert_eq!(flat[3].ancestor_has_next, vec![false]); // tests is last
    }
}
