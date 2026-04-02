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
