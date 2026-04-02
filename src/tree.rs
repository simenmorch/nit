use std::collections::{BTreeMap, HashSet};

use crate::model::DiffFile;

pub struct FileTree {
    entries: BTreeMap<String, TreeEntry>,
}

enum TreeEntry {
    Folder(FileTree),
    File(usize),
}

pub struct FlatEntry {
    pub depth: usize,
    pub kind: FlatEntryKind,
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
        let mut root = FileTree {
            entries: BTreeMap::new(),
        };
        for (i, file) in files.iter().enumerate() {
            let parts: Vec<&str> = file.path.split('/').collect();
            root.insert_parts(&parts, i);
        }
        root
    }

    fn insert_parts(&mut self, parts: &[&str], file_index: usize) {
        if parts.len() == 1 {
            self.entries
                .insert(parts[0].to_string(), TreeEntry::File(file_index));
        } else {
            let folder_name = parts[0].to_string();
            let entry = self
                .entries
                .entry(folder_name)
                .or_insert_with(|| TreeEntry::Folder(FileTree { entries: BTreeMap::new() }));
            if let TreeEntry::Folder(subtree) = entry {
                subtree.insert_parts(&parts[1..], file_index);
            }
        }
    }

    pub fn flatten(&self, collapsed: &HashSet<String>) -> Vec<FlatEntry> {
        self.flatten_inner(collapsed, "", 0)
    }

    fn flatten_inner(
        &self,
        collapsed: &HashSet<String>,
        prefix: &str,
        depth: usize,
    ) -> Vec<FlatEntry> {
        let mut result = Vec::new();

        // Folders first, then files (BTreeMap keeps alphabetical order within each group)
        let folders: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, v)| matches!(v, TreeEntry::Folder(_)))
            .collect();
        let files: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, v)| matches!(v, TreeEntry::File(_)))
            .collect();

        for (name, entry) in folders {
            if let TreeEntry::Folder(subtree) = entry {
                let full_path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", prefix, name)
                };
                let expanded = !collapsed.contains(&full_path);
                result.push(FlatEntry {
                    depth,
                    kind: FlatEntryKind::Folder {
                        path: full_path.clone(),
                        name: name.clone(),
                        expanded,
                    },
                });
                if expanded {
                    result.extend(subtree.flatten_inner(collapsed, &full_path, depth + 1));
                }
            }
        }

        for (name, entry) in files {
            if let TreeEntry::File(file_index) = entry {
                result.push(FlatEntry {
                    depth,
                    kind: FlatEntryKind::File {
                        file_index: *file_index,
                        name: name.clone(),
                    },
                });
            }
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
        assert_eq!(flat.len(), 4); // a, b, c, d.rs
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].depth, 1);
        assert_eq!(flat[2].depth, 2);
        assert_eq!(flat[3].depth, 3);
        assert_eq!(entry_name(&flat[3]), "d.rs");
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
        let diff = make_diff(vec![
            make_file("b.rs", vec![]),
            make_file("a.rs", vec![]),
        ]);
        let flat = flat_entries_for(&diff);
        // BTreeMap sorts alphabetically: a.rs (file_index=1), b.rs (file_index=0)
        if let FlatEntryKind::File { file_index, .. } = &flat[0].kind {
            assert_eq!(*file_index, 1); // a.rs was index 1 in the original vec
        }
        if let FlatEntryKind::File { file_index, .. } = &flat[1].kind {
            assert_eq!(*file_index, 0); // b.rs was index 0
        }
    }
}
