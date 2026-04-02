use std::collections::HashSet;

use crate::tree::{FlatEntry, FlatEntryKind};

pub enum Focus {
    Sidebar,
    Diff,
}

pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current: usize,
}

#[derive(Clone)]
pub struct SearchMatch {
    pub line_index: usize,
}

pub struct App {
    pub focus: Focus,
    pub show_sidebar: bool,
    pub selected: usize,
    pub selected_file: usize,
    pub scroll: usize,
    pub sidebar_scroll: usize,
    pub collapsed: HashSet<String>,
    pub g_pressed: bool,
    pub searching: bool,
    pub search_input: String,
    pub search: Option<SearchState>,
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
            selected: 0,
            selected_file: 0,
            scroll: 0,
            sidebar_scroll: 0,
            collapsed: HashSet::new(),
            g_pressed: false,
            searching: false,
            search_input: String::new(),
            search: None,
        }
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

    pub fn toggle_viewed_entry(
        &mut self,
        diff: &mut crate::model::Diff,
        visible: &[FlatEntry],
    ) {
        if let Some(entry) = visible.get(self.selected) {
            match &entry.kind {
                FlatEntryKind::File { file_index, .. } => {
                    diff.files[*file_index].viewed = !diff.files[*file_index].viewed;
                }
                FlatEntryKind::Folder { path, .. } => {
                    let prefix = format!("{}/", path);
                    let all_viewed = diff
                        .files
                        .iter()
                        .filter(|f| f.path.starts_with(&prefix))
                        .all(|f| f.viewed);
                    let new_viewed = !all_viewed;
                    for file in diff.files.iter_mut() {
                        if file.path.starts_with(&prefix) {
                            file.viewed = new_viewed;
                        }
                    }
                }
            }
        }
    }

    pub fn mark_viewed_and_next(
        &mut self,
        diff: &mut crate::model::Diff,
        visible: &[FlatEntry],
    ) {
        diff.files[self.selected_file].viewed = true;

        // Find the next file entry after the current sidebar selection
        for (i, entry) in visible.iter().enumerate().skip(self.selected + 1) {
            if let FlatEntryKind::File { file_index, .. } = &entry.kind {
                self.selected = i;
                self.selected_file = *file_index;
                self.scroll = 0;
                return;
            }
        }
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

        if let Some(m) = matches.first() {
            self.scroll = m.line_index;
        }

        self.search = Some(SearchState {
            query,
            matches,
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

    pub fn next_hunk(&mut self, diff: &crate::model::Diff) {
        let Some(file) = diff.files.get(self.selected_file) else {
            return;
        };
        let mut line_index: usize = 0;
        for hunk in &file.hunks {
            if line_index > self.scroll {
                self.scroll = line_index;
                return;
            }
            line_index += 1 + hunk.lines.len();
        }
    }

    pub fn prev_hunk(&mut self, diff: &crate::model::Diff) {
        let Some(file) = diff.files.get(self.selected_file) else {
            return;
        };
        let mut hunk_starts = Vec::new();
        let mut line_index: usize = 0;
        for hunk in &file.hunks {
            hunk_starts.push(line_index);
            line_index += 1 + hunk.lines.len();
        }
        for &start in hunk_starts.iter().rev() {
            if start < self.scroll {
                self.scroll = start;
                return;
            }
        }
    }
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
        app.next_hunk(&diff);
        // First hunk is at line_index 0, has 1 header + 2 lines = 3 total
        // Second hunk starts at line_index 3
        assert_eq!(app.scroll, 3);
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
        app.scroll = 3; // at second hunk
        app.prev_hunk(&diff);
        assert_eq!(app.scroll, 0); // back to first
    }
}
