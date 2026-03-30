pub enum Focus {
    Sidebar,
    Diff,
}

pub struct App {
    pub focus: Focus,
    pub show_sidebar: bool,
    pub selected: usize,
    pub scroll: usize,
    pub g_pressed: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            focus: Focus::Sidebar,
            show_sidebar: true,
            selected: 0,
            scroll: 0,
            g_pressed: false,
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

}
