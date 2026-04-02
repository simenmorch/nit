mod app;
mod config;
mod git;
mod github;
mod model;
mod provider;
mod tree;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use provider::ReviewProvider;

#[derive(Parser)]
#[command(name = "nit", about = "Terminal code review tool")]
struct Cli {
    /// Commit, range (abc..def), PR (#42), or nothing for uncommitted changes
    rev: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo = git::open_repo()?;

    let (mut diff, label) = match cli.rev.as_deref() {
        None => {
            let diff = git::get_uncommitted_diff(&repo)?;
            let branch = git::branch_name(&repo);
            (diff, branch)
        }
        Some(rev) if rev.contains("..") => {
            let parts: Vec<&str> = rev.splitn(2, "..").collect();
            let diff = git::get_range_diff(&repo, parts[0], parts[1])?;
            (diff, rev.to_string())
        }
        Some(rev) if rev.starts_with('#') || rev.contains('#') => {
            let (owner, repo_name, pr_number) = parse_pr_ref(rev, &repo)?;
            let provider = github::GitHubProvider::new(owner.clone(), repo_name.clone())?;
            let diff = provider.fetch_diff(&pr_number)?;
            let label = format!("{}#{}", if owner.is_empty() { String::new() } else { format!("{}/{} ", owner, repo_name) }, pr_number);
            (diff, label)
        }
        Some(rev) => {
            let diff = git::get_commit_diff(&repo, rev)?;
            (diff, rev.to_string())
        }
    };

    let cfg = config::load()?;

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let theme = if let Some(ref path) = cfg.theme.syntax_file {
        ThemeSet::get_theme(path)
            .with_context(|| format!("failed to load theme from {}", path))?
    } else {
        ts.themes
            .get(&cfg.theme.syntax)
            .cloned()
            .with_context(|| format!("unknown theme '{}'. Available: {}", cfg.theme.syntax,
                ts.themes.keys().cloned().collect::<Vec<_>>().join(", ")))?
    };

    let tree = tree::FileTree::from_files(&diff.files);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut diff, &tree, &label, &ss, &theme, &cfg.colors);
    ratatui::restore();

    result
}

enum KeyAction {
    Continue,
    Quit,
}

fn handle_key(
    key: KeyEvent,
    app: &mut app::App,
    diff: &mut model::Diff,
    visible: &[tree::FlatEntry],
    content_height: usize,
    viewport_height: usize,
) -> KeyAction {
    // Search input mode — capture keystrokes for the query
    if app.searching {
        match key.code {
            KeyCode::Enter => app.submit_search(diff),
            KeyCode::Esc => app.cancel_search(),
            KeyCode::Backspace => { app.search_input.pop(); }
            KeyCode::Char(c) => app.search_input.push(c),
            _ => {}
        }
        return KeyAction::Continue;
    }

    // Handle gg (two-key combo)
    if app.g_pressed {
        app.g_pressed = false;
        if key.code == KeyCode::Char('g') {
            app.jump_to_top(visible);
            return KeyAction::Continue;
        }
    }

    // Ctrl-modified keys (check before plain char matches)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => {
                app.scroll_down_half_page(content_height, viewport_height);
                return KeyAction::Continue;
            }
            KeyCode::Char('u') => {
                app.scroll_up_half_page(viewport_height);
                return KeyAction::Continue;
            }
            _ => {}
        }
    }

    // Global keys (work in any focus)
    match key.code {
        KeyCode::Char('q') => return KeyAction::Quit,
        KeyCode::Tab => { app.toggle_sidebar(); return KeyAction::Continue; }
        KeyCode::Char('v') => { app.toggle_viewed_entry(diff, visible); return KeyAction::Continue; }
        KeyCode::Char('g') => { app.g_pressed = true; return KeyAction::Continue; }
        KeyCode::Char('G') => {
            app.jump_to_bottom(visible, content_height, viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::Char('/') => { app.start_search(); return KeyAction::Continue; }
        KeyCode::Char('n') => { app.next_match(viewport_height); return KeyAction::Continue; }
        KeyCode::Char('N') => { app.prev_match(viewport_height); return KeyAction::Continue; }
        _ => {}
    }

    // Focus-specific keys
    match app.focus {
        app::Focus::Sidebar => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next_entry(visible),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev_entry(visible),
            KeyCode::Enter => app.activate_entry(visible),
            KeyCode::Char('l') => app.open_file(visible),
            KeyCode::Char(' ') => app.toggle_fold(visible),
            KeyCode::Esc => app.clear_search(),
            _ => {}
        },
        app::Focus::Diff => match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.scroll_down(content_height, viewport_height);
            }
            KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
            KeyCode::Char('h') => app.focus_sidebar(),
            KeyCode::Esc => app.clear_search(),
            _ => {}
        },
    }

    KeyAction::Continue
}

fn run(
    terminal: &mut DefaultTerminal,
    diff: &mut model::Diff,
    tree: &tree::FileTree,
    branch: &str,
    ss: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    colors: &config::ColorsConfig,
) -> Result<()> {
    let mut app = app::App::new();

    loop {
        let visible = tree.flatten(&app.collapsed);

        // Clamp selection if entries were hidden by collapsing
        if !visible.is_empty() && app.selected >= visible.len() {
            app.selected = visible.len() - 1;
            app.update_selected_file(&visible);
        }

        let sidebar_height = (terminal.size()?.height as usize).saturating_sub(1);
        app.ensure_sidebar_visible(sidebar_height);

        terminal.draw(|frame| ui::draw(frame, &app, diff, &visible, branch, ss, theme, colors))?;

        let viewport_height = terminal.size()?.height as usize;

        // Process events — drain pending queue before redrawing
        let mut visible = visible;
        loop {
            let content_height = diff
                .files
                .get(app.selected_file)
                .map(ui::diff_line_count)
                .unwrap_or(0);

            if let Event::Key(key) = event::read()?
                && matches!(
                    handle_key(key, &mut app, diff, &visible, content_height, viewport_height),
                    KeyAction::Quit
                )
            {
                return Ok(());
            }

            // Recompute visible tree in case a fold was toggled
            visible = tree.flatten(&app.collapsed);
            if !visible.is_empty() && app.selected >= visible.len() {
                app.selected = visible.len() - 1;
                app.update_selected_file(&visible);
            }

            // If no more events are queued, break to redraw
            if !event::poll(std::time::Duration::ZERO)? {
                break;
            }
        }
    }
}

/// Parse a PR reference into (owner, repo, pr_number).
/// Supports: "#42", "owner/repo#42"
fn parse_pr_ref(rev: &str, repo: &git2::Repository) -> Result<(String, String, String)> {
    if let Some(rest) = rev.strip_prefix('#') {
        // #42 — infer owner/repo from git remote
        let (owner, repo_name) = git::owner_repo_from_remote(repo)?;
        Ok((owner, repo_name, rest.to_string()))
    } else if rev.contains('#') {
        // owner/repo#42
        let parts: Vec<&str> = rev.splitn(2, '#').collect();
        let slug = parts[0];
        let pr_number = parts[1];
        let slug_parts: Vec<&str> = slug.splitn(2, '/').collect();
        if slug_parts.len() != 2 {
            anyhow::bail!("invalid PR reference '{}' — expected owner/repo#number", rev);
        }
        Ok((slug_parts[0].to_string(), slug_parts[1].to_string(), pr_number.to_string()))
    } else {
        anyhow::bail!("invalid PR reference '{}'", rev);
    }
}
