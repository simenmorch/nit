use std::collections::HashSet;

use crate::git;
use crate::model;
use crate::tree::{FlatEntry, FlatEntryKind};

pub const PR_FILTER_OPTIONS: &[&str] = &["open", "draft", "merged", "closed", "mine"];

pub struct PrFilter {
    pub enabled: HashSet<String>,
    pub modal_open: bool,
    pub modal_selected: usize,
    pub github_user: Option<String>,
}

impl Default for PrFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl PrFilter {
    pub fn new() -> Self {
        let mut enabled = HashSet::new();
        enabled.insert("open".to_string());
        Self {
            enabled,
            modal_open: false,
            modal_selected: 0,
            github_user: None,
        }
    }

    pub fn toggle(&mut self, state: &str) {
        if self.enabled.contains(state) {
            self.enabled.remove(state);
        } else {
            self.enabled.insert(state.to_string());
        }
    }

    pub fn matches(&self, pr: &model::PrInfo) -> bool {
        let has_state_filters = self.enabled.iter().any(|s| s != "mine");
        let state_match = !has_state_filters || self.enabled.contains(pr.state.as_str());
        let mine_filter = if self.enabled.contains("mine") {
            self.github_user.as_ref().is_some_and(|user| pr.author == *user)
        } else {
            true
        };
        state_match && mine_filter
    }
}

pub enum Focus {
    Sidebar,
    Diff,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Diff,
    Commits,
    PRs,
}

#[derive(Clone)]
pub enum ReviewMode {
    WorkingTree,
    Commit { short_oid: String, message: String, return_tab: Tab },
    PullRequest { number: u64, title: String, return_tab: Tab },
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum DiffViewMode {
    #[default]
    Unified,
    SideBySide,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum CommitField {
    #[default]
    Summary,
    Description,
}

#[derive(Clone)]
pub enum PendingAction {
    GitPull,
    GitPush,
}

pub struct ConfirmModal {
    pub open: bool,
    pub message: String,
    pub action: PendingAction,
}

pub struct BranchModal {
    pub open: bool,
    pub selected: usize,
    pub branches: Vec<git::BranchInfo>,
    pub filter: String,
}

impl Default for BranchModal {
    fn default() -> Self {
        Self::new()
    }
}

impl BranchModal {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            branches: Vec::new(),
            filter: String::new(),
        }
    }

    pub fn filtered(&self) -> Vec<&git::BranchInfo> {
        if self.filter.is_empty() {
            self.branches.iter().collect()
        } else {
            let query = self.filter.to_lowercase();
            self.branches
                .iter()
                .filter(|b| b.name.to_lowercase().contains(&query))
                .collect()
        }
    }

    pub fn current_name(&self) -> &str {
        self.branches.iter()
            .find(|b| b.is_head)
            .map(|b| b.name.as_str())
            .unwrap_or("")
    }
}

pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub match_lines: HashSet<usize>,
    pub current: usize,
}

#[derive(Clone)]
pub struct SearchMatch {
    pub line_index: usize,
}

pub struct App {
    pub focus: Focus,
    pub show_sidebar: bool,
    pub view_mode: DiffViewMode,
    pub selected: usize,
    pub selected_file: usize,
    pub scroll: usize,
    pub sidebar_scroll: usize,
    pub collapsed: HashSet<String>,
    pub g_pressed: bool,
    pub searching: bool,
    pub search_input: String,
    pub search: Option<SearchState>,
    pub committing: bool,
    pub commit_summary: String,
    pub commit_description: Vec<String>,
    pub commit_focus: CommitField,
    pub commit_cursor: usize,
    pub active_tab: Tab,
    pub commits: Vec<model::CommitInfo>,
    pub commit_selected: usize,
    pub commit_scroll: usize,
    pub prs: Vec<model::PrInfo>,
    pub pr_selected: usize,
    pub pr_scroll: usize,
    pub pr_filter: PrFilter,
    pub review_mode: ReviewMode,
    pub branch_modal: BranchModal,
    pub status_message: Option<String>,
    pub show_help: bool,
    pub loading_message: Option<String>,
    pub confirm: Option<ConfirmModal>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            focus: Focus::Sidebar,
            show_sidebar: true,
            view_mode: DiffViewMode::default(),
            selected: 0,
            selected_file: 0,
            scroll: 0,
            sidebar_scroll: 0,
            collapsed: HashSet::new(),
            g_pressed: false,
            searching: false,
            search_input: String::new(),
            search: None,
            committing: false,
            commit_summary: String::new(),
            commit_description: vec![String::new()],
            commit_focus: CommitField::default(),
            commit_cursor: 0,
            active_tab: Tab::default(),
            commits: Vec::new(),
            commit_selected: 0,
            commit_scroll: 0,
            prs: Vec::new(),
            pr_selected: 0,
            pr_scroll: 0,
            pr_filter: PrFilter::new(),
            review_mode: ReviewMode::WorkingTree,
            branch_modal: BranchModal::new(),
            status_message: None,
            show_help: false,
            loading_message: None,
            confirm: None,
        }
    }

    pub fn filtered_prs(&self) -> Vec<&model::PrInfo> {
        self.prs.iter().filter(|pr| self.pr_filter.matches(pr)).collect()
    }

    pub fn filtered_pr_count(&self) -> usize {
        self.prs.iter().filter(|pr| self.pr_filter.matches(pr)).count()
    }

    pub fn update_selected_file(&mut self, visible: &[FlatEntry]) {
        if let Some(entry) = visible.get(self.selected)
            && let FlatEntryKind::File { file_index, .. } = &entry.kind
        {
            self.selected_file = *file_index;
            self.scroll = 0;
        }
    }

    pub fn select_next_entry(&mut self, visible: &[FlatEntry]) {
        if !visible.is_empty() && self.selected < visible.len() - 1 {
            self.selected += 1;
            self.update_selected_file(visible);
        }
    }

    pub fn select_prev_entry(&mut self, visible: &[FlatEntry]) {
        if self.selected > 0 {
            self.selected -= 1;
            self.update_selected_file(visible);
        }
    }

    pub fn select_next_file(&mut self, visible: &[FlatEntry]) {
        for (i, entry) in visible.iter().enumerate().skip(self.selected + 1) {
            if let FlatEntryKind::File { file_index, .. } = &entry.kind {
                self.selected = i;
                self.selected_file = *file_index;
                self.scroll = 0;
                return;
            }
        }
    }

    pub fn select_prev_file(&mut self, visible: &[FlatEntry]) {
        for i in (0..self.selected).rev() {
            if let FlatEntryKind::File { file_index, .. } = &visible[i].kind {
                self.selected = i;
                self.selected_file = *file_index;
                self.scroll = 0;
                return;
            }
        }
    }

    pub fn activate_entry(&mut self, visible: &[FlatEntry]) {
        if let Some(entry) = visible.get(self.selected) {
            match &entry.kind {
                FlatEntryKind::File { .. } => self.focus_diff(),
                FlatEntryKind::Folder { path, .. } => {
                    if self.collapsed.contains(path) {
                        self.collapsed.remove(path);
                    } else {
                        self.collapsed.insert(path.clone());
                    }
                }
            }
        }
    }

    /// `l` key: if on a file, focus diff. If on a folder, jump the cursor
    /// to the file currently shown in the diff panel and focus it.
    pub fn open_file(&mut self, visible: &[FlatEntry]) {
        if let Some(entry) = visible.get(self.selected)
            && matches!(entry.kind, FlatEntryKind::Folder { .. })
        {
            // Jump cursor to the entry matching selected_file
            for (i, e) in visible.iter().enumerate() {
                if let FlatEntryKind::File { file_index, .. } = &e.kind
                    && *file_index == self.selected_file
                {
                    self.selected = i;
                    break;
                }
            }
        }
        self.focus_diff();
    }

    pub fn toggle_fold(&mut self, visible: &[FlatEntry]) {
        if let Some(entry) = visible.get(self.selected)
            && let FlatEntryKind::Folder { path, .. } = &entry.kind
        {
            if self.collapsed.contains(path) {
                self.collapsed.remove(path);
            } else {
                self.collapsed.insert(path.clone());
            }
        }
    }

    /// Toggle viewed state for the selected entry. Returns indices of files
    /// that were newly marked as viewed (for staging purposes).
    pub fn toggle_viewed_entry(
        &mut self,
        diff: &mut crate::model::Diff,
        visible: &[FlatEntry],
    ) -> Vec<usize> {
        let mut newly_viewed = Vec::new();
        if let Some(entry) = visible.get(self.selected) {
            match &entry.kind {
                FlatEntryKind::File { file_index, .. } => {
                    let was_viewed = diff.files[*file_index].viewed;
                    diff.files[*file_index].viewed = !was_viewed;
                    if !was_viewed {
                        newly_viewed.push(*file_index);
                    }
                }
                FlatEntryKind::Folder { path, .. } => {
                    let prefix = format!("{}/", path);
                    let all_viewed = diff
                        .files
                        .iter()
                        .filter(|f| f.path.starts_with(&prefix))
                        .all(|f| f.viewed);
                    let new_viewed = !all_viewed;
                    for (i, file) in diff.files.iter_mut().enumerate() {
                        if file.path.starts_with(&prefix) {
                            file.viewed = new_viewed;
                            if new_viewed {
                                newly_viewed.push(i);
                            }
                        }
                    }
                }
            }
        }
        newly_viewed
    }

    /// Mark the current file as viewed and advance to the next file.
    /// Returns the index of the file that was marked as viewed.
    pub fn mark_viewed_and_next(
        &mut self,
        diff: &mut crate::model::Diff,
        visible: &[FlatEntry],
    ) -> usize {
        let marked = self.selected_file;
        diff.files[marked].viewed = true;

        // Find the next file entry after the current sidebar selection
        for (i, entry) in visible.iter().enumerate().skip(self.selected + 1) {
            if let FlatEntryKind::File { file_index, .. } = &entry.kind {
                self.selected = i;
                self.selected_file = *file_index;
                self.scroll = 0;
                return marked;
            }
        }
        marked
    }

    pub fn scroll_down(&mut self, content_height: usize, viewport_height: usize) {
        let max_scroll = content_height.saturating_sub(viewport_height);
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    pub fn scroll_down_half_page(&mut self, content_height: usize, viewport_height: usize) {
        let half = viewport_height / 2;
        let max_scroll = content_height.saturating_sub(viewport_height);
        self.scroll = (self.scroll + half).min(max_scroll);
    }

    pub fn scroll_up_half_page(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        self.scroll = self.scroll.saturating_sub(half);
    }

    pub fn focus_diff(&mut self) {
        self.focus = Focus::Diff;
    }

    pub fn focus_sidebar(&mut self) {
        self.show_sidebar = true;
        self.focus = Focus::Sidebar;
    }

    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        if !self.show_sidebar {
            self.focus = Focus::Diff;
        }
    }

    pub fn toggle_view_mode(
        &mut self,
        diff: &model::Diff,
        cache: &crate::cache::DiffCache,
    ) {
        let old_mode = self.view_mode;
        self.view_mode = match old_mode {
            DiffViewMode::Unified => DiffViewMode::SideBySide,
            DiffViewMode::SideBySide => DiffViewMode::Unified,
        };

        // Map scroll position from old view to new view
        if let Some(file) = diff.files.get(self.selected_file) {
            let unified_starts = unified_hunk_starts(&file.hunks);
            let split_starts = cache.hunk_start_rows(self.selected_file);

            let (from_starts, to_starts) = match old_mode {
                DiffViewMode::Unified => (unified_starts.as_slice(), split_starts),
                DiffViewMode::SideBySide => (split_starts, unified_starts.as_slice()),
            };

            self.scroll = map_scroll(self.scroll, from_starts, to_starts);
        }

        match self.view_mode {
            DiffViewMode::SideBySide => {
                self.show_sidebar = false;
                self.focus = Focus::Diff;
            }
            DiffViewMode::Unified => {
                self.show_sidebar = true;
            }
        }
    }

    pub fn jump_to_top(&mut self, visible: &[FlatEntry]) {
        match self.focus {
            Focus::Sidebar => {
                self.selected = 0;
                self.sidebar_scroll = 0;
                self.update_selected_file(visible);
            }
            Focus::Diff => {
                self.scroll = 0;
            }
        }
    }

    pub fn jump_to_bottom(&mut self, visible: &[FlatEntry], content_height: usize, viewport_height: usize) {
        match self.focus {
            Focus::Sidebar => {
                if !visible.is_empty() {
                    self.selected = visible.len() - 1;
                    self.update_selected_file(visible);
                }
            }
            Focus::Diff => {
                self.scroll = content_height.saturating_sub(viewport_height);
            }
        }
    }

    pub fn ensure_sidebar_visible(&mut self, sidebar_height: usize) {
        let usable = sidebar_height.saturating_sub(2);
        if usable == 0 {
            return;
        }
        if self.selected < self.sidebar_scroll {
            self.sidebar_scroll = self.selected;
        } else if self.selected >= self.sidebar_scroll + usable {
            self.sidebar_scroll = self.selected - usable + 1;
        }
    }

    pub fn start_search(&mut self) {
        self.searching = true;
        self.search_input.clear();
        self.focus = Focus::Diff;
    }

    pub fn cancel_search(&mut self) {
        self.searching = false;
        self.search_input.clear();
    }

    pub fn start_commit(&mut self) {
        self.committing = true;
        self.commit_summary.clear();
        self.commit_description = vec![String::new()];
        self.commit_focus = CommitField::Summary;
        self.commit_cursor = 0;
    }

    pub fn cancel_commit(&mut self) {
        self.committing = false;
    }

    /// Build the full commit message from summary + description.
    pub fn commit_message(&self) -> String {
        let desc = self.commit_description.join("\n").trim().to_string();
        if desc.is_empty() {
            self.commit_summary.clone()
        } else {
            format!("{}\n\n{}", self.commit_summary, desc)
        }
    }

    pub fn clear_search(&mut self) {
        self.searching = false;
        self.search_input.clear();
        self.search = None;
    }

    pub fn submit_search(&mut self, diff: &crate::model::Diff) {
        self.searching = false;
        let query = self.search_input.clone();
        if query.is_empty() {
            self.search = None;
            return;
        }

        let file = match diff.files.get(self.selected_file) {
            Some(f) => f,
            None => {
                self.search = None;
                return;
            }
        };

        let query_lower = query.to_lowercase();
        let matches = match self.view_mode {
            DiffViewMode::Unified => {
                let mut matches = Vec::new();
                let mut line_index: usize = 0;
                for hunk in &file.hunks {
                    if hunk.header.to_lowercase().contains(&query_lower) {
                        matches.push(SearchMatch { line_index });
                    }
                    line_index += 1;
                    for line in &hunk.lines {
                        if line.content.to_lowercase().contains(&query_lower) {
                            matches.push(SearchMatch { line_index });
                        }
                        line_index += 1;
                    }
                }
                matches
            }
            DiffViewMode::SideBySide => {
                use crate::split::{self, SplitRow};
                let rows = split::build_split_rows(&file.hunks);
                let mut matches = Vec::new();
                for (i, row) in rows.iter().enumerate() {
                    let hit = match row {
                        SplitRow::HunkHeader(h) => h.to_lowercase().contains(&query_lower),
                        SplitRow::Context(line) => line.content.to_lowercase().contains(&query_lower),
                        SplitRow::Paired { left, right, .. } => {
                            left.content.to_lowercase().contains(&query_lower)
                                || right.content.to_lowercase().contains(&query_lower)
                        }
                        SplitRow::LeftOnly(line) => line.content.to_lowercase().contains(&query_lower),
                        SplitRow::RightOnly(line) => line.content.to_lowercase().contains(&query_lower),
                    };
                    if hit {
                        matches.push(SearchMatch { line_index: i });
                    }
                }
                matches
            }
        };

        if let Some(m) = matches.first() {
            self.scroll = m.line_index;
        }

        let match_lines: HashSet<usize> = matches.iter().map(|m| m.line_index).collect();
        self.search = Some(SearchState {
            query,
            matches,
            match_lines,
            current: 0,
        });
    }

    pub fn next_match(&mut self, viewport_height: usize) {
        if let Some(ref mut search) = self.search {
            if search.matches.is_empty() {
                return;
            }
            search.current = (search.current + 1) % search.matches.len();
            let line = search.matches[search.current].line_index;
            if line < self.scroll || line >= self.scroll + viewport_height {
                self.scroll = line;
            }
        }
    }

    pub fn prev_match(&mut self, viewport_height: usize) {
        if let Some(ref mut search) = self.search {
            if search.matches.is_empty() {
                return;
            }
            if search.current == 0 {
                search.current = search.matches.len() - 1;
            } else {
                search.current -= 1;
            }
            let line = search.matches[search.current].line_index;
            if line < self.scroll || line >= self.scroll + viewport_height {
                self.scroll = line;
            }
        }
    }

    pub fn next_hunk(&mut self, diff: &crate::model::Diff, cache: &crate::cache::DiffCache) {
        let Some(file) = diff.files.get(self.selected_file) else {
            return;
        };
        let hunk_starts = self.hunk_starts(&file.hunks, cache);
        for &start in &hunk_starts {
            if start > self.scroll {
                self.scroll = start;
                return;
            }
        }
    }

    pub fn prev_hunk(&mut self, diff: &crate::model::Diff, cache: &crate::cache::DiffCache) {
        let Some(file) = diff.files.get(self.selected_file) else {
            return;
        };
        let hunk_starts = self.hunk_starts(&file.hunks, cache);
        for &start in hunk_starts.iter().rev() {
            if start < self.scroll {
                self.scroll = start;
                return;
            }
        }
    }

    pub fn switch_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
        self.g_pressed = false;
    }

    pub fn select_next_commit(&mut self) {
        if !self.commits.is_empty() && self.commit_selected < self.commits.len() - 1 {
            self.commit_selected += 1;
        }
    }

    pub fn select_prev_commit(&mut self) {
        if self.commit_selected > 0 {
            self.commit_selected -= 1;
        }
    }

    pub fn ensure_commit_visible(&mut self, height: usize) {
        let usable = height.saturating_sub(2);
        if usable == 0 {
            return;
        }
        if self.commit_selected < self.commit_scroll {
            self.commit_scroll = self.commit_selected;
        } else if self.commit_selected >= self.commit_scroll + usable {
            self.commit_scroll = self.commit_selected - usable + 1;
        }
    }

    pub fn scroll_commits_down_half_page(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        if !self.commits.is_empty() {
            self.commit_selected =
                (self.commit_selected + half).min(self.commits.len() - 1);
        }
    }

    pub fn scroll_commits_up_half_page(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        self.commit_selected = self.commit_selected.saturating_sub(half);
    }

    pub fn jump_to_top_commits(&mut self) {
        self.commit_selected = 0;
        self.commit_scroll = 0;
    }

    pub fn jump_to_bottom_commits(&mut self) {
        if !self.commits.is_empty() {
            self.commit_selected = self.commits.len() - 1;
        }
    }

    pub fn select_next_pr(&mut self) {
        let count = self.filtered_pr_count();
        if count > 0 && self.pr_selected < count - 1 {
            self.pr_selected += 1;
        }
    }

    pub fn select_prev_pr(&mut self) {
        if self.pr_selected > 0 {
            self.pr_selected -= 1;
        }
    }

    pub fn ensure_pr_visible(&mut self, height: usize) {
        let usable = height.saturating_sub(2);
        if usable == 0 {
            return;
        }
        if self.pr_selected < self.pr_scroll {
            self.pr_scroll = self.pr_selected;
        } else if self.pr_selected >= self.pr_scroll + usable {
            self.pr_scroll = self.pr_selected - usable + 1;
        }
    }

    pub fn scroll_prs_down_half_page(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        let count = self.filtered_pr_count();
        if count > 0 {
            self.pr_selected = (self.pr_selected + half).min(count - 1);
        }
    }

    pub fn scroll_prs_up_half_page(&mut self, viewport_height: usize) {
        let half = viewport_height / 2;
        self.pr_selected = self.pr_selected.saturating_sub(half);
    }

    pub fn jump_to_top_prs(&mut self) {
        self.pr_selected = 0;
        self.pr_scroll = 0;
    }

    pub fn jump_to_bottom_prs(&mut self) {
        let count = self.filtered_pr_count();
        if count > 0 {
            self.pr_selected = count - 1;
        }
    }

    pub fn clamp_pr_selection(&mut self) {
        let count = self.filtered_pr_count();
        if count == 0 {
            self.pr_selected = 0;
            self.pr_scroll = 0;
        } else if self.pr_selected >= count {
            self.pr_selected = count - 1;
        }
    }

    pub fn reset_diff_state(&mut self) {
        self.focus = Focus::Sidebar;
        self.show_sidebar = true;
        self.view_mode = DiffViewMode::default();
        self.selected = 0;
        self.selected_file = 0;
        self.scroll = 0;
        self.sidebar_scroll = 0;
        self.collapsed.clear();
        self.g_pressed = false;
        self.searching = false;
        self.search_input.clear();
        self.search = None;
    }

    fn hunk_starts(&self, hunks: &[crate::model::Hunk], cache: &crate::cache::DiffCache) -> Vec<usize> {
        match self.view_mode {
            DiffViewMode::Unified => unified_hunk_starts(hunks),
            DiffViewMode::SideBySide => cache.hunk_start_rows(self.selected_file).to_vec(),
        }
    }
}

fn unified_hunk_starts(hunks: &[crate::model::Hunk]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut idx: usize = 0;
    for hunk in hunks {
        starts.push(idx);
        idx += 1 + hunk.lines.len();
    }
    starts
}

/// Map a scroll position from one view's coordinate space to another,
/// using each view's hunk start positions. Preserves proportional offset
/// within the containing hunk.
fn map_scroll(scroll: usize, from_starts: &[usize], to_starts: &[usize]) -> usize {
    if from_starts.is_empty() || to_starts.is_empty() {
        return 0;
    }

    // Find which hunk contains the scroll position
    let hunk_idx = match from_starts.binary_search(&scroll) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };

    let from_hunk_start = from_starts[hunk_idx];
    let from_hunk_end = from_starts
        .get(hunk_idx + 1)
        .copied()
        .unwrap_or(from_hunk_start + 1);
    let from_hunk_len = from_hunk_end - from_hunk_start;

    let to_hunk_start = to_starts.get(hunk_idx).copied().unwrap_or(0);
    let to_hunk_end = to_starts
        .get(hunk_idx + 1)
        .copied()
        .unwrap_or(to_hunk_start + 1);
    let to_hunk_len = to_hunk_end - to_hunk_start;

    let offset_in_hunk = scroll - from_hunk_start;
    let mapped_offset = if from_hunk_len > 0 {
        offset_in_hunk * to_hunk_len / from_hunk_len
    } else {
        0
    };

    to_hunk_start + mapped_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LineKind;
    use crate::test_helpers::*;

    // ── Navigation ──

    #[test]
    fn new_app_defaults() {
        let app = App::new();
        assert!(matches!(app.focus, Focus::Sidebar));
        assert!(app.show_sidebar);
        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn select_next_entry_advances() {
        let diff = make_simple_diff(3);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.select_next_entry(&visible);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn select_next_entry_stops_at_end() {
        let diff = make_diff(vec![make_file("a.rs", vec![]), make_file("b.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.select_next_entry(&visible);
        app.select_next_entry(&visible);
        app.select_next_entry(&visible); // should not exceed len-1
        assert_eq!(app.selected, visible.len() - 1);
    }

    #[test]
    fn select_prev_entry_stops_at_zero() {
        let diff = make_simple_diff(3);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.select_prev_entry(&visible);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn select_next_updates_selected_file() {
        // Two root-level files → flat entries are files directly
        let diff = make_diff(vec![
            make_file("a.rs", vec![make_hunk("@@ -1 +1 @@", vec![])]),
            make_file("b.rs", vec![make_hunk("@@ -1 +1 @@", vec![])]),
        ]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        // first entry is a.rs (file_index=0 due to alphabetical sort, but a < b so index 0)
        app.select_next_entry(&visible);
        assert_eq!(app.selected_file, 1); // b.rs
    }

    #[test]
    fn activate_file_focuses_diff() {
        let diff = make_diff(vec![make_file("a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.activate_entry(&visible);
        assert!(matches!(app.focus, Focus::Diff));
    }

    #[test]
    fn activate_folder_toggles_collapse() {
        let diff = make_diff(vec![make_file("src/a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        // First entry is the "src" folder
        app.activate_entry(&visible);
        assert!(app.collapsed.contains("src"));
        // Recompute visible with collapsed state and activate again
        let visible2 = crate::tree::FileTree::from_files(&diff.files).flatten(&app.collapsed);
        app.activate_entry(&visible2);
        assert!(!app.collapsed.contains("src"));
    }

    #[test]
    fn toggle_fold_on_folder() {
        let diff = make_diff(vec![make_file("src/a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.toggle_fold(&visible); // on folder
        assert!(app.collapsed.contains("src"));
    }

    #[test]
    fn toggle_fold_on_file_is_noop() {
        let diff = make_diff(vec![make_file("a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.toggle_fold(&visible); // on file — no-op
        assert!(app.collapsed.is_empty());
    }

    #[test]
    fn open_file_on_file_focuses_diff() {
        let diff = make_diff(vec![make_file("a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.open_file(&visible);
        assert!(matches!(app.focus, Focus::Diff));
    }

    // ── Scrolling ──

    #[test]
    fn scroll_down_increments() {
        let mut app = App::new();
        app.scroll_down(100, 20);
        assert_eq!(app.scroll, 1);
    }

    #[test]
    fn scroll_down_clamps_at_bottom() {
        let mut app = App::new();
        for _ in 0..200 {
            app.scroll_down(50, 20);
        }
        assert_eq!(app.scroll, 30); // 50 - 20
    }

    #[test]
    fn scroll_up_decrements() {
        let mut app = App::new();
        app.scroll = 5;
        app.scroll_up();
        assert_eq!(app.scroll, 4);
    }

    #[test]
    fn scroll_up_clamps_at_zero() {
        let mut app = App::new();
        app.scroll_up();
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn scroll_down_half_page() {
        let mut app = App::new();
        app.scroll_down_half_page(100, 20);
        assert_eq!(app.scroll, 10); // 20 / 2
    }

    #[test]
    fn scroll_up_half_page() {
        let mut app = App::new();
        app.scroll = 15;
        app.scroll_up_half_page(20);
        assert_eq!(app.scroll, 5); // 15 - 10
    }

    #[test]
    fn scroll_up_half_page_clamps() {
        let mut app = App::new();
        app.scroll = 3;
        app.scroll_up_half_page(20);
        assert_eq!(app.scroll, 0);
    }

    // ── Focus ──

    #[test]
    fn focus_diff_changes_focus() {
        let mut app = App::new();
        app.focus_diff();
        assert!(matches!(app.focus, Focus::Diff));
    }

    #[test]
    fn focus_sidebar_changes_focus_and_shows() {
        let mut app = App::new();
        app.show_sidebar = false;
        app.focus = Focus::Diff;
        app.focus_sidebar();
        assert!(matches!(app.focus, Focus::Sidebar));
        assert!(app.show_sidebar);
    }

    #[test]
    fn toggle_sidebar_hides_and_focuses_diff() {
        let mut app = App::new();
        app.toggle_sidebar();
        assert!(!app.show_sidebar);
        assert!(matches!(app.focus, Focus::Diff));
    }

    #[test]
    fn toggle_sidebar_shows_again() {
        let mut app = App::new();
        app.toggle_sidebar(); // hide
        app.toggle_sidebar(); // show
        assert!(app.show_sidebar);
    }

    // ── Jump ──

    #[test]
    fn jump_to_top_sidebar() {
        let diff = make_simple_diff(5);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.selected = 3;
        app.sidebar_scroll = 2;
        app.jump_to_top(&visible);
        assert_eq!(app.selected, 0);
        assert_eq!(app.sidebar_scroll, 0);
    }

    #[test]
    fn jump_to_top_diff() {
        let diff = make_simple_diff(1);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.focus = Focus::Diff;
        app.scroll = 50;
        app.jump_to_top(&visible);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn jump_to_bottom_sidebar() {
        let diff = make_diff(vec![
            make_file("a.rs", vec![]),
            make_file("b.rs", vec![]),
            make_file("c.rs", vec![]),
        ]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.jump_to_bottom(&visible, 100, 20);
        assert_eq!(app.selected, visible.len() - 1);
    }

    #[test]
    fn jump_to_bottom_diff() {
        let diff = make_simple_diff(1);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.focus = Focus::Diff;
        app.jump_to_bottom(&visible, 100, 20);
        assert_eq!(app.scroll, 80); // 100 - 20
    }

    // ── Ensure sidebar visible ──

    #[test]
    fn ensure_sidebar_visible_scrolls_down() {
        let mut app = App::new();
        app.selected = 25;
        app.sidebar_scroll = 0;
        app.ensure_sidebar_visible(20);
        assert!(app.sidebar_scroll > 0);
    }

    #[test]
    fn ensure_sidebar_visible_scrolls_up() {
        let mut app = App::new();
        app.selected = 2;
        app.sidebar_scroll = 10;
        app.ensure_sidebar_visible(20);
        assert_eq!(app.sidebar_scroll, 2);
    }

    // ── Search ──

    #[test]
    fn start_search_sets_mode() {
        let mut app = App::new();
        app.start_search();
        assert!(app.searching);
        assert!(app.search_input.is_empty());
        assert!(matches!(app.focus, Focus::Diff));
    }

    #[test]
    fn cancel_search_exits() {
        let mut app = App::new();
        app.start_search();
        app.search_input.push_str("query");
        app.cancel_search();
        assert!(!app.searching);
        assert!(app.search_input.is_empty());
    }

    #[test]
    fn clear_search_resets_all() {
        let mut app = App::new();
        app.searching = true;
        app.search_input = "test".to_string();
        app.search = Some(crate::app::SearchState {
            query: "test".to_string(),
            matches: vec![],
            match_lines: HashSet::new(),
            current: 0,
        });
        app.clear_search();
        assert!(!app.searching);
        assert!(app.search_input.is_empty());
        assert!(app.search.is_none());
    }

    #[test]
    fn submit_search_finds_matches() {
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![make_hunk(
                "@@ -1 +1 @@",
                vec![
                    make_line(LineKind::Context, "hello world", Some(1), Some(1)),
                    make_line(LineKind::Added, "hello again", None, Some(2)),
                ],
            )],
        )]);
        let mut app = App::new();
        app.search_input = "hello".to_string();
        app.submit_search(&diff);
        assert!(!app.searching);
        let search = app.search.as_ref().unwrap();
        assert_eq!(search.matches.len(), 2);
        assert_eq!(search.query, "hello");
    }

    #[test]
    fn submit_empty_search_clears() {
        let diff = make_simple_diff(1);
        let mut app = App::new();
        app.search_input.clear();
        app.submit_search(&diff);
        assert!(app.search.is_none());
    }

    #[test]
    fn next_match_advances_and_wraps() {
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![make_hunk(
                "@@ -1 +1 @@",
                vec![
                    make_line(LineKind::Context, "match", Some(1), Some(1)),
                    make_line(LineKind::Context, "match", Some(2), Some(2)),
                ],
            )],
        )]);
        let mut app = App::new();
        app.search_input = "match".to_string();
        app.submit_search(&diff);
        assert_eq!(app.search.as_ref().unwrap().current, 0);
        app.next_match(100);
        assert_eq!(app.search.as_ref().unwrap().current, 1);
        app.next_match(100);
        assert_eq!(app.search.as_ref().unwrap().current, 0); // wraps
    }

    #[test]
    fn prev_match_wraps() {
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![make_hunk(
                "@@ -1 +1 @@",
                vec![
                    make_line(LineKind::Context, "match", Some(1), Some(1)),
                    make_line(LineKind::Context, "match", Some(2), Some(2)),
                ],
            )],
        )]);
        let mut app = App::new();
        app.search_input = "match".to_string();
        app.submit_search(&diff);
        app.prev_match(100);
        assert_eq!(app.search.as_ref().unwrap().current, 1); // wraps to last
    }

    // ── Viewed ──

    #[test]
    fn toggle_viewed_marks_file() {
        let mut diff = make_diff(vec![make_file("a.rs", vec![])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        assert!(!diff.files[0].viewed);
        app.toggle_viewed_entry(&mut diff, &visible);
        assert!(diff.files[0].viewed);
        app.toggle_viewed_entry(&mut diff, &visible);
        assert!(!diff.files[0].viewed);
    }

    #[test]
    fn toggle_viewed_folder_marks_all_children() {
        let mut diff = make_diff(vec![
            make_file("src/a.rs", vec![]),
            make_file("src/b.rs", vec![]),
        ]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        // First entry is the "src" folder
        app.toggle_viewed_entry(&mut diff, &visible);
        assert!(diff.files[0].viewed);
        assert!(diff.files[1].viewed);
    }

    #[test]
    fn mark_viewed_and_next_advances() {
        let mut diff = make_diff(vec![
            make_file("a.rs", vec![make_hunk("@@", vec![])]),
            make_file("b.rs", vec![make_hunk("@@", vec![])]),
        ]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.mark_viewed_and_next(&mut diff, &visible);
        assert!(diff.files[0].viewed);
        assert_eq!(app.selected_file, 1);
    }

    // ── Hunk navigation ──

    #[test]
    fn next_hunk_jumps_forward() {
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![
                make_hunk(
                    "@@ -1 +1 @@",
                    vec![
                        make_line(LineKind::Context, "a", Some(1), Some(1)),
                        make_line(LineKind::Context, "b", Some(2), Some(2)),
                    ],
                ),
                make_hunk(
                    "@@ -10 +10 @@",
                    vec![make_line(LineKind::Context, "c", Some(10), Some(10))],
                ),
            ],
        )]);
        let mut app = App::new();
        let cache = crate::cache::DiffCache::new(&diff);
        app.next_hunk(&diff, &cache);
        // First hunk is at line_index 0, has 1 header + 2 lines = 3 total
        // Second hunk starts at line_index 3
        assert_eq!(app.scroll, 3);
    }

    // ── Tabs ──

    #[test]
    fn switch_tab_changes_active() {
        let mut app = App::new();
        assert_eq!(app.active_tab, Tab::Diff);
        app.switch_tab(Tab::Commits);
        assert_eq!(app.active_tab, Tab::Commits);
        app.switch_tab(Tab::Diff);
        assert_eq!(app.active_tab, Tab::Diff);
    }

    // ── Commit navigation ──

    fn sample_commits(n: usize) -> Vec<crate::model::CommitInfo> {
        (0..n)
            .map(|i| crate::model::CommitInfo {
                oid: format!("{:040x}", i),
                short_oid: format!("{:07x}", i),
                message: format!("commit {}", i),
                author: "Test".to_string(),
                date: "just now".to_string(),
            })
            .collect()
    }

    #[test]
    fn select_next_commit_advances() {
        let mut app = App::new();
        app.commits = sample_commits(5);
        app.select_next_commit();
        assert_eq!(app.commit_selected, 1);
    }

    #[test]
    fn select_next_commit_stops_at_end() {
        let mut app = App::new();
        app.commits = sample_commits(3);
        app.commit_selected = 2;
        app.select_next_commit();
        assert_eq!(app.commit_selected, 2);
    }

    #[test]
    fn select_prev_commit_decrements() {
        let mut app = App::new();
        app.commits = sample_commits(5);
        app.commit_selected = 3;
        app.select_prev_commit();
        assert_eq!(app.commit_selected, 2);
    }

    #[test]
    fn select_prev_commit_stops_at_zero() {
        let mut app = App::new();
        app.commits = sample_commits(5);
        app.select_prev_commit();
        assert_eq!(app.commit_selected, 0);
    }

    #[test]
    fn jump_to_top_commits_resets() {
        let mut app = App::new();
        app.commits = sample_commits(10);
        app.commit_selected = 7;
        app.commit_scroll = 5;
        app.jump_to_top_commits();
        assert_eq!(app.commit_selected, 0);
        assert_eq!(app.commit_scroll, 0);
    }

    #[test]
    fn jump_to_bottom_commits_goes_to_last() {
        let mut app = App::new();
        app.commits = sample_commits(10);
        app.jump_to_bottom_commits();
        assert_eq!(app.commit_selected, 9);
    }

    #[test]
    fn ensure_commit_visible_scrolls_down() {
        let mut app = App::new();
        app.commit_selected = 25;
        app.commit_scroll = 0;
        app.ensure_commit_visible(20);
        assert!(app.commit_scroll > 0);
    }

    #[test]
    fn ensure_commit_visible_scrolls_up() {
        let mut app = App::new();
        app.commit_selected = 2;
        app.commit_scroll = 10;
        app.ensure_commit_visible(20);
        assert_eq!(app.commit_scroll, 2);
    }

    // ── Reset diff state ──

    #[test]
    fn reset_diff_state_restores_defaults() {
        let mut app = App::new();
        app.focus = Focus::Diff;
        app.show_sidebar = false;
        app.selected = 5;
        app.selected_file = 3;
        app.scroll = 42;
        app.sidebar_scroll = 10;
        app.collapsed.insert("src".to_string());
        app.searching = true;
        app.search_input = "query".to_string();
        app.search = Some(SearchState {
            query: "query".to_string(),
            matches: vec![],
            match_lines: HashSet::new(),
            current: 0,
        });

        app.reset_diff_state();

        assert!(matches!(app.focus, Focus::Sidebar));
        assert!(app.show_sidebar);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_file, 0);
        assert_eq!(app.scroll, 0);
        assert_eq!(app.sidebar_scroll, 0);
        assert!(app.collapsed.is_empty());
        assert!(!app.searching);
        assert!(app.search_input.is_empty());
        assert!(app.search.is_none());
    }

    // ── PR filter ──

    #[test]
    fn pr_filter_default_has_open() {
        let f = PrFilter::new();
        assert!(f.enabled.contains("open"));
        assert_eq!(f.enabled.len(), 1);
    }

    #[test]
    fn pr_filter_toggle_on_off() {
        let mut f = PrFilter::new();
        f.toggle("closed");
        assert!(f.enabled.contains("closed"));
        f.toggle("closed");
        assert!(!f.enabled.contains("closed"));
    }

    #[test]
    fn pr_filter_matches_state() {
        let f = PrFilter::new(); // only "open"
        assert!(f.matches(&make_pr(1, "open", "alice")));
        assert!(!f.matches(&make_pr(2, "closed", "alice")));
    }

    #[test]
    fn pr_filter_matches_multiple_states() {
        let mut f = PrFilter::new();
        f.toggle("merged");
        assert!(f.matches(&make_pr(1, "open", "a")));
        assert!(f.matches(&make_pr(2, "merged", "a")));
        assert!(!f.matches(&make_pr(3, "closed", "a")));
    }

    #[test]
    fn pr_filter_mine_with_state() {
        let mut f = PrFilter::new(); // "open"
        f.toggle("mine");
        f.github_user = Some("alice".to_string());
        assert!(f.matches(&make_pr(1, "open", "alice")));
        assert!(!f.matches(&make_pr(2, "open", "bob")));
        assert!(!f.matches(&make_pr(3, "closed", "alice")));
    }

    #[test]
    fn pr_filter_mine_alone_shows_all_states() {
        let mut f = PrFilter::new();
        f.toggle("open"); // remove default "open"
        f.toggle("mine");
        f.github_user = Some("alice".to_string());
        // With only "mine" and no state filters, all of alice's PRs should match
        assert!(f.matches(&make_pr(1, "open", "alice")));
        assert!(f.matches(&make_pr(2, "closed", "alice")));
        assert!(!f.matches(&make_pr(3, "open", "bob")));
    }

    #[test]
    fn pr_filter_mine_without_github_user() {
        let mut f = PrFilter::new();
        f.toggle("mine");
        // github_user is None — "mine" filter rejects everything
        assert!(!f.matches(&make_pr(1, "open", "alice")));
    }

    #[test]
    fn filtered_prs_respects_filter() {
        let mut app = App::new();
        app.prs = mixed_prs();
        // Default filter is "open" — should match PRs #1, #5
        let filtered = app.filtered_prs();
        let numbers: Vec<u64> = filtered.iter().map(|p| p.number).collect();
        assert_eq!(numbers, vec![1, 5]);
    }

    #[test]
    fn filtered_prs_mine_filter() {
        let mut app = App::new();
        app.prs = mixed_prs();
        app.pr_filter.toggle("mine");
        app.pr_filter.github_user = Some("alice".to_string());
        // "open" + "mine" — only alice's open PRs
        let filtered = app.filtered_prs();
        let numbers: Vec<u64> = filtered.iter().map(|p| p.number).collect();
        assert_eq!(numbers, vec![1]);
    }

    #[test]
    fn clamp_pr_selection_on_empty() {
        let mut app = App::new();
        app.prs = Vec::new();
        app.pr_selected = 5;
        app.pr_scroll = 3;
        app.clamp_pr_selection();
        assert_eq!(app.pr_selected, 0);
        assert_eq!(app.pr_scroll, 0);
    }

    #[test]
    fn clamp_pr_selection_past_end() {
        let mut app = App::new();
        app.prs = sample_prs(3);
        app.pr_selected = 5;
        app.clamp_pr_selection();
        assert_eq!(app.pr_selected, 2);
    }

    #[test]
    fn clamp_pr_selection_within_range() {
        let mut app = App::new();
        app.prs = sample_prs(5);
        app.pr_selected = 2;
        app.clamp_pr_selection();
        assert_eq!(app.pr_selected, 2);
    }

    // ── PR navigation ──

    fn sample_prs(n: usize) -> Vec<crate::model::PrInfo> {
        (0..n)
            .map(|i| crate::model::PrInfo {
                number: i as u64 + 1,
                title: format!("PR {}", i),
                author: "Test".to_string(),
                state: "open".to_string(),
                updated_at: "just now".to_string(),
            })
            .collect()
    }

    fn make_pr(number: u64, state: &str, author: &str) -> crate::model::PrInfo {
        crate::model::PrInfo {
            number,
            title: format!("PR #{}", number),
            author: author.to_string(),
            state: state.to_string(),
            updated_at: "just now".to_string(),
        }
    }

    fn mixed_prs() -> Vec<crate::model::PrInfo> {
        vec![
            make_pr(1, "open", "alice"),
            make_pr(2, "closed", "bob"),
            make_pr(3, "draft", "alice"),
            make_pr(4, "merged", "carol"),
            make_pr(5, "open", "bob"),
        ]
    }

    #[test]
    fn select_next_pr_advances() {
        let mut app = App::new();
        app.prs = sample_prs(5);
        app.select_next_pr();
        assert_eq!(app.pr_selected, 1);
    }

    #[test]
    fn select_next_pr_stops_at_end() {
        let mut app = App::new();
        app.prs = sample_prs(3);
        app.pr_selected = 2;
        app.select_next_pr();
        assert_eq!(app.pr_selected, 2);
    }

    #[test]
    fn select_prev_pr_decrements() {
        let mut app = App::new();
        app.prs = sample_prs(5);
        app.pr_selected = 3;
        app.select_prev_pr();
        assert_eq!(app.pr_selected, 2);
    }

    #[test]
    fn select_prev_pr_stops_at_zero() {
        let mut app = App::new();
        app.prs = sample_prs(5);
        app.select_prev_pr();
        assert_eq!(app.pr_selected, 0);
    }

    #[test]
    fn jump_to_top_prs_resets() {
        let mut app = App::new();
        app.prs = sample_prs(10);
        app.pr_selected = 7;
        app.pr_scroll = 5;
        app.jump_to_top_prs();
        assert_eq!(app.pr_selected, 0);
        assert_eq!(app.pr_scroll, 0);
    }

    #[test]
    fn jump_to_bottom_prs_goes_to_last() {
        let mut app = App::new();
        app.prs = sample_prs(10);
        app.jump_to_bottom_prs();
        assert_eq!(app.pr_selected, 9);
    }

    #[test]
    fn ensure_pr_visible_scrolls_down() {
        let mut app = App::new();
        app.pr_selected = 25;
        app.pr_scroll = 0;
        app.ensure_pr_visible(20);
        assert!(app.pr_scroll > 0);
    }

    #[test]
    fn ensure_pr_visible_scrolls_up() {
        let mut app = App::new();
        app.pr_selected = 2;
        app.pr_scroll = 10;
        app.ensure_pr_visible(20);
        assert_eq!(app.pr_scroll, 2);
    }

    #[test]
    fn prev_hunk_jumps_back() {
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![
                make_hunk(
                    "@@ -1 +1 @@",
                    vec![
                        make_line(LineKind::Context, "a", Some(1), Some(1)),
                        make_line(LineKind::Context, "b", Some(2), Some(2)),
                    ],
                ),
                make_hunk(
                    "@@ -10 +10 @@",
                    vec![make_line(LineKind::Context, "c", Some(10), Some(10))],
                ),
            ],
        )]);
        let mut app = App::new();
        let cache = crate::cache::DiffCache::new(&diff);
        app.scroll = 3; // at second hunk
        app.prev_hunk(&diff, &cache);
        assert_eq!(app.scroll, 0); // back to first
    }

    // ── map_scroll ──

    #[test]
    fn map_scroll_at_hunk_boundary() {
        let from = &[0, 10, 20];
        let to = &[0, 5, 15];
        assert_eq!(map_scroll(0, from, to), 0);
        assert_eq!(map_scroll(10, from, to), 5);
        assert_eq!(map_scroll(20, from, to), 15);
    }

    #[test]
    fn map_scroll_proportional_within_hunk() {
        // Hunk 0: from rows 0..10, to rows 0..5
        let from = &[0, 10];
        let to = &[0, 5];
        assert_eq!(map_scroll(5, from, to), 2); // 5/10 * 5 = 2
    }

    #[test]
    fn map_scroll_empty_starts() {
        assert_eq!(map_scroll(5, &[], &[0, 10]), 0);
        assert_eq!(map_scroll(5, &[0, 10], &[]), 0);
    }

    #[test]
    fn map_scroll_single_hunk() {
        let from = &[0];
        let to = &[0];
        assert_eq!(map_scroll(0, from, to), 0);
    }
}
