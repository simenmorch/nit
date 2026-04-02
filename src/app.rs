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
        if let Some(entry) = visible.get(self.selected) {
            if matches!(entry.kind, FlatEntryKind::Folder { .. }) {
                // Jump cursor to the entry matching selected_file
                for (i, e) in visible.iter().enumerate() {
                    if let FlatEntryKind::File { file_index, .. } = &e.kind {
                        if *file_index == self.selected_file {
                            self.selected = i;
                            break;
                        }
                    }
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
