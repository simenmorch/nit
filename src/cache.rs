use crate::app::DiffViewMode;
use crate::model;
use crate::split;

/// Cached per-file split-view metadata.
pub struct DiffCache {
    /// split_meta[file_index] = precomputed SplitMeta for that file.
    split_meta: Vec<split::SplitMeta>,
}

impl DiffCache {
    pub fn new(diff: &model::Diff) -> Self {
        let split_meta = diff
            .files
            .iter()
            .map(|f| split::compute_split_meta(&f.hunks))
            .collect();
        DiffCache { split_meta }
    }

    pub fn diff_line_count(
        &self,
        file_index: usize,
        diff: &model::Diff,
        view_mode: DiffViewMode,
    ) -> usize {
        match view_mode {
            DiffViewMode::Unified => diff
                .files
                .get(file_index)
                .map(|f| f.hunks.iter().map(|h| 1 + h.lines.len()).sum())
                .unwrap_or(0),
            DiffViewMode::SideBySide => self
                .split_meta
                .get(file_index)
                .map(|m| m.row_count)
                .unwrap_or(0),
        }
    }

    pub fn hunk_start_rows(&self, file_index: usize) -> &[usize] {
        self.split_meta
            .get(file_index)
            .map(|m| m.hunk_starts.as_slice())
            .unwrap_or(&[])
    }
}
