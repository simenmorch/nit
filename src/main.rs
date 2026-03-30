mod app;
mod git;
mod github;
mod model;
mod provider;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode};
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

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut diff, &label, &ss, theme);
    ratatui::restore();

    result
}

fn run(
    terminal: &mut DefaultTerminal,
    diff: &mut model::Diff,
    branch: &str,
    ss: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) -> Result<()> {
    let mut app = app::App::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, &app, diff, branch, ss, theme))?;

        let viewport_height = terminal.size()?.height as usize;
        let content_height = diff
            .files
            .get(app.selected)
            .map(|f| ui::diff_line_count(f))
            .unwrap_or(0);

        if let Event::Key(key) = event::read()? {
            // Handle gg (two-key combo)
            if app.g_pressed {
                app.g_pressed = false;
                if key.code == KeyCode::Char('g') {
                    app.jump_to_top();
                    continue;
                }
            }

            // Global keys (work in any focus)
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Tab => { app.toggle_sidebar(); continue; }
                KeyCode::Char('v') => { app.toggle_viewed(diff); continue; }
                KeyCode::Char('g') => { app.g_pressed = true; continue; }
                KeyCode::Char('G') => {
                    app.jump_to_bottom(diff.files.len(), content_height);
                    continue;
                }
                _ => {}
            }

            // Focus-specific keys
            match app.focus {
                app::Focus::Sidebar => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => app.select_next(diff.files.len()),
                    KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                    KeyCode::Enter | KeyCode::Char('l') => app.focus_diff(),
                    _ => {}
                },
                app::Focus::Diff => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.scroll_down(content_height, viewport_height);
                    }
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                    KeyCode::Char('h') | KeyCode::Esc => app.focus_sidebar(),
                    _ => {}
                },
            }
        }

    }

    Ok(())
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
