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
    pub line_index: usize, // index into the flat list of rendered diff lines
}

pub struct App {
    pub focus: Focus,
    pub show_sidebar: bool,
    pub selected: usize,
    pub scroll: usize,
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
            scroll: 0,
            g_pressed: false,
            searching: false,
            search_input: String::new(),
            search: None,
        }
    }

    pub fn select_next(&mut self, file_count: usize) {
        if file_count > 0 && self.selected < file_count - 1 {
            self.selected += 1;
            self.scroll = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.scroll = 0;
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

    pub fn jump_to_top(&mut self) {
        match self.focus {
            Focus::Sidebar => {
                self.selected = 0;
                self.scroll = 0;
            }
            Focus::Diff => {
                self.scroll = 0;
            }
        }
    }

    pub fn jump_to_bottom(&mut self, file_count: usize, content_height: usize) {
        match self.focus {
            Focus::Sidebar => {
                if file_count > 0 {
                    self.selected = file_count - 1;
                    self.scroll = 0;
                }
            }
            Focus::Diff => {
                self.scroll = content_height;
            }
        }
    }

    pub fn toggle_viewed(&mut self, diff: &mut crate::model::Diff) {
        if let Some(file) = diff.files.get_mut(self.selected) {
            file.viewed = !file.viewed;
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

        let file = match diff.files.get(self.selected) {
            Some(f) => f,
            None => { self.search = None; return; }
        };

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();
        let mut line_index: usize = 0;

        for hunk in &file.hunks {
            // hunk header line
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

        let current = 0;

        // Jump to first match
        if let Some(m) = matches.first() {
            self.scroll = m.line_index;
        }

        self.search = Some(SearchState { query, matches, current });
    }

    pub fn next_match(&mut self, viewport_height: usize) {
        if let Some(ref mut search) = self.search {
            if search.matches.is_empty() {
                return;
            }
            search.current = (search.current + 1) % search.matches.len();
            let line = search.matches[search.current].line_index;
            // Scroll so the match is visible
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
}
