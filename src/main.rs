use std::time::Duration;
//test

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;
use syntect::parsing::SyntaxSet;

use nit::{app, config, git, github, model, provider, tree, ui};
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

    let (diff, label) = match cli.rev.as_deref() {
        None => {
            let mut diff = git::get_uncommitted_diff(&repo)?;
            let staged = git::get_staged_files(&repo).unwrap_or_default();
            for file in &mut diff.files {
                if staged.contains(&file.path) {
                    file.viewed = true;
                }
            }
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
            let label = format!(
                "{}#{}",
                if owner.is_empty() {
                    String::new()
                } else {
                    format!("{}/{} ", owner, repo_name)
                },
                pr_number
            );
            (diff, label)
        }
        Some(rev) => {
            let diff = git::get_commit_diff(&repo, rev)?;
            (diff, rev.to_string())
        }
    };

    let commits = git::get_commit_log(&repo, 500).unwrap_or_default();

    let github_remote = git::owner_repo_from_remote(&repo).ok();
    let (prs, github_user) = match &github_remote {
        Some((owner, repo_name)) => {
            match github::GitHubProvider::new(owner.clone(), repo_name.clone()) {
                Ok(provider) => {
                    let prs = provider.fetch_pr_list(100).unwrap_or_default();
                    let user = provider.fetch_authenticated_user().ok();
                    (prs, user)
                }
                Err(_) => (Vec::new(), None),
            }
        }
        None => (Vec::new(), None),
    };

    let cfg = config::load()?;

    let ss = SyntaxSet::load_defaults_newlines();
    let theme = config::resolve_theme(&cfg.theme)?;

    let tree = tree::FileTree::from_files(&diff.files);

    let mut terminal = ratatui::init();
    let is_live = cli.rev.is_none();
    let result = run(
        &mut terminal,
        diff,
        tree,
        label,
        &repo,
        is_live,
        commits,
        prs,
        github_remote,
        github_user,
        &ss,
        &theme,
        &cfg.colors,
    );
    ratatui::restore();

    result
}

enum KeyAction {
    Continue,
    Quit,
    Commit(String),
    CheckoutBranch(String),
    LoadCommitDiff(String),
    LoadPrDiff(u64),
    ReturnToDefault,
    GitPull,
    GitPush,
}

fn handle_key(
    key: KeyEvent,
    app: &mut app::App,
    diff: &mut model::Diff,
    visible: &[tree::FlatEntry],
    content_height: usize,
    viewport_height: usize,
    repo: &git2::Repository,
) -> KeyAction {
    // Clear status message on any keypress
    app.status_message = None;

    // Search input mode — capture keystrokes for the query
    if app.searching {
        match key.code {
            KeyCode::Enter => app.submit_search(diff),
            KeyCode::Esc => app.cancel_search(),
            KeyCode::Backspace => {
                app.search_input.pop();
            }
            KeyCode::Char(c) => app.search_input.push(c),
            _ => {}
        }
        return KeyAction::Continue;
    }

    // Commit message modal
    if app.committing {
        // Ctrl+Enter submits from either field
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            if !app.commit_summary.is_empty() {
                let msg = app.commit_message();
                app.committing = false;
                return KeyAction::Commit(msg);
            }
            return KeyAction::Continue;
        }

        match app.commit_focus {
            app::CommitField::Summary => match key.code {
                KeyCode::Enter => {
                    let msg = app.commit_message();
                    if !app.commit_summary.is_empty() {
                        app.committing = false;
                        return KeyAction::Commit(msg);
                    }
                }
                KeyCode::Esc => app.cancel_commit(),
                KeyCode::Tab => {
                    app.commit_focus = app::CommitField::Description;
                    app.commit_cursor = 0;
                }
                KeyCode::Backspace => {
                    app.commit_summary.pop();
                }
                KeyCode::Char(c) => app.commit_summary.push(c),
                _ => {}
            },
            app::CommitField::Description => match key.code {
                KeyCode::Enter => {
                    // Insert a new line after the cursor row
                    let row = app.commit_cursor;
                    app.commit_description.insert(row + 1, String::new());
                    app.commit_cursor += 1;
                }
                KeyCode::Esc => app.cancel_commit(),
                KeyCode::Tab => {
                    app.commit_focus = app::CommitField::Summary;
                }
                KeyCode::Backspace => {
                    let row = app.commit_cursor;
                    if app.commit_description[row].is_empty() {
                        if row > 0 {
                            app.commit_description.remove(row);
                            app.commit_cursor -= 1;
                        }
                    } else {
                        app.commit_description[row].pop();
                    }
                }
                KeyCode::Up => {
                    if app.commit_cursor > 0 {
                        app.commit_cursor -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.commit_cursor + 1 < app.commit_description.len() {
                        app.commit_cursor += 1;
                    }
                }
                KeyCode::Char(c) => {
                    app.commit_description[app.commit_cursor].push(c);
                }
                _ => {}
            },
        }
        return KeyAction::Continue;
    }

    // Branch modal intercepts all keys
    if app.branch_modal.open {
        let has_filter = app.branch_modal.branches.len() > 10;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_filter || key.code == KeyCode::Down => {
                let len = app.branch_modal.filtered().len();
                if len > 0 {
                    app.branch_modal.selected = (app.branch_modal.selected + 1) % len;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if !has_filter || key.code == KeyCode::Up => {
                let len = app.branch_modal.filtered().len();
                if len > 0 {
                    if app.branch_modal.selected == 0 {
                        app.branch_modal.selected = len - 1;
                    } else {
                        app.branch_modal.selected -= 1;
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') if !has_filter || key.code == KeyCode::Enter => {
                let filtered = app.branch_modal.filtered();
                if let Some(branch) = filtered.get(app.branch_modal.selected) {
                    let name = branch.name.clone();
                    let is_current = branch.is_head;
                    app.branch_modal.open = false;
                    app.branch_modal.filter.clear();
                    if !is_current {
                        return KeyAction::CheckoutBranch(name);
                    }
                }
            }
            KeyCode::Esc => {
                app.branch_modal.open = false;
                app.branch_modal.filter.clear();
            }
            KeyCode::Backspace if has_filter => {
                app.branch_modal.filter.pop();
                app.branch_modal.selected = 0;
            }
            KeyCode::Char(c) if has_filter => {
                app.branch_modal.filter.push(c);
                app.branch_modal.selected = 0;
            }
            _ => {}
        }
        return KeyAction::Continue;
    }

    // Confirm modal intercepts all keys when open
    if let Some(ref confirm) = app.confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let action = confirm.action.clone();
                app.confirm = None;
                return match action {
                    app::PendingAction::GitPull => KeyAction::GitPull,
                    app::PendingAction::GitPush => KeyAction::GitPush,
                };
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.confirm = None;
            }
            _ => {}
        }
        return KeyAction::Continue;
    }

    // Help modal intercepts all keys when open
    if app.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                app.show_help = false;
            }
            _ => {}
        }
        return KeyAction::Continue;
    }

    // Filter modal intercepts all keys — must be checked before global keys
    if app.pr_filter.modal_open {
        return handle_prs_tab_key(key, app, viewport_height);
    }

    // Handle gg (two-key combo)
    if app.g_pressed {
        app.g_pressed = false;
        if key.code == KeyCode::Char('g') {
            match app.active_tab {
                app::Tab::Diff => app.jump_to_top(visible),
                app::Tab::Commits => app.jump_to_top_commits(),
                app::Tab::PRs => app.jump_to_top_prs(),
            }
            return KeyAction::Continue;
        }
    }

    // Global keys (work regardless of tab)
    match key.code {
        KeyCode::Char('q') => return KeyAction::Quit,
        KeyCode::Char('?') => {
            app.show_help = true;
            return KeyAction::Continue;
        }
        KeyCode::Char('1') => {
            app.switch_tab(app::Tab::Diff);
            return KeyAction::Continue;
        }
        KeyCode::Char('2') => {
            app.switch_tab(app::Tab::Commits);
            return KeyAction::Continue;
        }
        KeyCode::Char('3') => {
            app.switch_tab(app::Tab::PRs);
            return KeyAction::Continue;
        }
        KeyCode::Char('g') => {
            app.g_pressed = true;
            return KeyAction::Continue;
        }
        KeyCode::Char('G') => {
            match app.active_tab {
                app::Tab::Diff => {
                    app.jump_to_bottom(visible, content_height, viewport_height);
                }
                app::Tab::Commits => app.jump_to_bottom_commits(),
                app::Tab::PRs => app.jump_to_bottom_prs(),
            }
            return KeyAction::Continue;
        }
        KeyCode::Char('p') if matches!(app.view_context, app::ViewContext::Default) => {
            app.confirm = Some(app::ConfirmModal {
                open: true,
                message: "Pull from remote?".to_string(),
                action: app::PendingAction::GitPull,
            });
            return KeyAction::Continue;
        }
        KeyCode::Char('P') if matches!(app.view_context, app::ViewContext::Default) => {
            app.confirm = Some(app::ConfirmModal {
                open: true,
                message: "Push to remote?".to_string(),
                action: app::PendingAction::GitPush,
            });
            return KeyAction::Continue;
        }
        _ => {}
    }

    // Tab-specific dispatch
    match app.active_tab {
        app::Tab::Diff => {
            handle_diff_tab_key(key, app, diff, visible, content_height, viewport_height, repo)
        }
        app::Tab::Commits => handle_commits_tab_key(key, app, viewport_height),
        app::Tab::PRs => handle_prs_tab_key(key, app, viewport_height),
    }
}

fn handle_diff_tab_key(
    key: KeyEvent,
    app: &mut app::App,
    diff: &mut model::Diff,
    visible: &[tree::FlatEntry],
    content_height: usize,
    viewport_height: usize,
    repo: &git2::Repository,
) -> KeyAction {
    // Ctrl-modified keys
    match key.code {
        KeyCode::PageDown => {
            app.scroll_down_half_page(content_height, viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::PageUp => {
            app.scroll_up_half_page(viewport_height);
            return KeyAction::Continue;
        }
        _ => {}
    }

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
            KeyCode::Char('n') => {
                app.next_hunk(diff);
                return KeyAction::Continue;
            }
            KeyCode::Char('p') => {
                app.prev_hunk(diff);
                return KeyAction::Continue;
            }
            _ => {}
        }
    }

    // Diff-tab global keys
    match key.code {
        KeyCode::Tab => {
            app.toggle_sidebar();
            return KeyAction::Continue;
        }
        KeyCode::Char('v') => {
            let newly_viewed = app.toggle_viewed_entry(diff, visible);
            if matches!(app.view_context, app::ViewContext::Default) {
                for idx in newly_viewed {
                    let file = &diff.files[idx];
                    let deleted = matches!(file.status, model::FileStatus::Deleted);
                    let _ = git::stage_file(repo, &file.path, deleted);
                }
            }
            return KeyAction::Continue;
        }
        KeyCode::Char('V') => {
            let idx = app.mark_viewed_and_next(diff, visible);
            if matches!(app.view_context, app::ViewContext::Default) {
                let file = &diff.files[idx];
                let deleted = matches!(file.status, model::FileStatus::Deleted);
                let _ = git::stage_file(repo, &file.path, deleted);
            }
            return KeyAction::Continue;
        }
        KeyCode::Char('/') => {
            app.start_search();
            return KeyAction::Continue;
        }
        KeyCode::Char('n') => {
            app.next_match(viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::Char('N') => {
            app.prev_match(viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::Char('s') => {
            app.toggle_view_mode();
            return KeyAction::Continue;
        }
        KeyCode::Char('c') if matches!(app.view_context, app::ViewContext::Default) => {
            app.start_commit();
            return KeyAction::Continue;
        }
        KeyCode::Char('b') if matches!(app.view_context, app::ViewContext::Default) => {
            if let Ok(branches) = git::list_branches_detailed(repo) {
                let selected = branches.iter().position(|b| b.is_head).unwrap_or(0);
                app.branch_modal.branches = branches;
                app.branch_modal.selected = selected;
                app.branch_modal.filter.clear();
                app.branch_modal.open = true;
            }
            return KeyAction::Continue;
        }
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
            KeyCode::Esc => {
                if app.search.is_some() {
                    app.clear_search();
                } else if matches!(app.view_context, app::ViewContext::Commit { .. } | app::ViewContext::PullRequest { .. }) {
                    return KeyAction::ReturnToDefault;
                }
            }
            _ => {}
        },
        app::Focus::Diff => match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.scroll_down(content_height, viewport_height);
            }
            KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
            KeyCode::Char('h') => app.focus_sidebar(),
            KeyCode::Char(']') => app.select_next_file(visible),
            KeyCode::Char('[') => app.select_prev_file(visible),
            KeyCode::Esc => {
                if app.search.is_some() {
                    app.clear_search();
                } else if matches!(app.view_context, app::ViewContext::Commit { .. } | app::ViewContext::PullRequest { .. }) {
                    return KeyAction::ReturnToDefault;
                }
            }
            _ => {}
        },
    }

    KeyAction::Continue
}

fn handle_commits_tab_key(key: KeyEvent, app: &mut app::App, viewport_height: usize) -> KeyAction {
    match key.code {
        KeyCode::PageDown => {
            app.scroll_commits_down_half_page(viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::PageUp => {
            app.scroll_commits_up_half_page(viewport_height);
            return KeyAction::Continue;
        }
        _ => {}
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => {
                app.scroll_commits_down_half_page(viewport_height);
                return KeyAction::Continue;
            }
            KeyCode::Char('u') => {
                app.scroll_commits_up_half_page(viewport_height);
                return KeyAction::Continue;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next_commit(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev_commit(),
        KeyCode::Enter => {
            if let Some(commit) = app.commits.get(app.commit_selected) {
                return KeyAction::LoadCommitDiff(commit.oid.clone());
            }
        }
        _ => {}
    }
    KeyAction::Continue
}

fn handle_prs_tab_key(key: KeyEvent, app: &mut app::App, viewport_height: usize) -> KeyAction {
    // Filter modal intercepts all keys when open
    if app.pr_filter.modal_open {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let len = app::PR_FILTER_OPTIONS.len();
                app.pr_filter.modal_selected = (app.pr_filter.modal_selected + 1) % len;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let len = app::PR_FILTER_OPTIONS.len();
                if app.pr_filter.modal_selected == 0 {
                    app.pr_filter.modal_selected = len - 1;
                } else {
                    app.pr_filter.modal_selected -= 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let state = app::PR_FILTER_OPTIONS[app.pr_filter.modal_selected];
                app.pr_filter.toggle(state);
                app.clamp_pr_selection();
            }
            KeyCode::Esc | KeyCode::Char('f') => {
                app.pr_filter.modal_open = false;
            }
            _ => {}
        }
        return KeyAction::Continue;
    }

    match key.code {
        KeyCode::PageDown => {
            app.scroll_prs_down_half_page(viewport_height);
            return KeyAction::Continue;
        }
        KeyCode::PageUp => {
            app.scroll_prs_up_half_page(viewport_height);
            return KeyAction::Continue;
        }
        _ => {}
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => {
                app.scroll_prs_down_half_page(viewport_height);
                return KeyAction::Continue;
            }
            KeyCode::Char('u') => {
                app.scroll_prs_up_half_page(viewport_height);
                return KeyAction::Continue;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next_pr(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev_pr(),
        KeyCode::Char('f') => {
            app.pr_filter.modal_open = true;
        }
        KeyCode::Enter => {
            let filtered = app.filtered_prs();
            if let Some(pr) = filtered.get(app.pr_selected) {
                return KeyAction::LoadPrDiff(pr.number);
            }
        }
        _ => {}
    }
    KeyAction::Continue
}

#[allow(clippy::too_many_arguments)]
fn run(
    terminal: &mut DefaultTerminal,
    mut diff: model::Diff,
    mut tree: tree::FileTree,
    mut label: String,
    repo: &git2::Repository,
    is_live: bool,
    commits: Vec<model::CommitInfo>,
    prs: Vec<model::PrInfo>,
    github_remote: Option<(String, String)>,
    github_user: Option<String>,
    ss: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    colors: &config::ColorsConfig,
) -> Result<()> {
    let mut app = app::App::new();
    app.commits = commits;
    app.prs = prs;
    app.pr_filter.github_user = github_user;

    let mut original_diff = diff.clone();
    let original_label = label.clone();

    loop {
        let visible = tree.flatten(&app.collapsed);

        // Clamp selection if entries were hidden by collapsing
        if !visible.is_empty() && app.selected >= visible.len() {
            app.selected = visible.len() - 1;
            app.update_selected_file(&visible);
        }

        let sidebar_height = (terminal.size()?.height as usize).saturating_sub(2);
        app.ensure_sidebar_visible(sidebar_height);

        terminal
            .draw(|frame| ui::draw(frame, &app, &diff, &visible, &label, ss, theme, colors))?;

        let viewport_height = terminal.size()?.height as usize;

        // Process events — drain pending queue before redrawing
        let poll_timeout = if is_live {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(86400)
        };

        let mut visible = visible;
        loop {
            let content_height = diff
                .files
                .get(app.selected_file)
                .map(|f| ui::diff_line_count(f, app.view_mode))
                .unwrap_or(0);

            if !event::poll(poll_timeout)? {
                // Timeout — refresh uncommitted diff if in default view
                if is_live
                    && matches!(app.view_context, app::ViewContext::Default)
                    && let Ok(mut new_diff) = git::get_uncommitted_diff(repo)
                    && !diff_content_eq(&diff, &new_diff)
                {
                    transfer_viewed(&diff, &mut new_diff);
                    // Mark staged files as viewed (unless already transferred)
                    if let Ok(staged) = git::get_staged_files(repo) {
                        for file in &mut new_diff.files {
                            if !file.viewed && staged.contains(&file.path) {
                                file.viewed = true;
                            }
                        }
                    }
                    diff = new_diff;
                    tree = tree::FileTree::from_files(&diff.files);
                    original_diff = diff.clone();
                    if !diff.files.is_empty() && app.selected_file >= diff.files.len() {
                        app.selected_file = diff.files.len() - 1;
                    }
                }
                break;
            }

            if let Event::Key(key) = event::read()? {
                match handle_key(
                    key,
                    &mut app,
                    &mut diff,
                    &visible,
                    content_height,
                    viewport_height,
                    repo,
                ) {
                    KeyAction::Quit => return Ok(()),
                    KeyAction::CheckoutBranch(name) => {
                        if git::checkout_branch(repo, &name).is_ok() {
                            label = name;
                            if let Ok(mut new_diff) = git::get_uncommitted_diff(repo) {
                                let staged = git::get_staged_files(repo).unwrap_or_default();
                                for file in &mut new_diff.files {
                                    if staged.contains(&file.path) {
                                        file.viewed = true;
                                    }
                                }
                                diff = new_diff;
                                tree = tree::FileTree::from_files(&diff.files);
                                original_diff = diff.clone();
                                app.reset_diff_state();
                            }
                        }
                        break;
                    }
                    KeyAction::GitPull => {
                        match git::git_pull(repo) {
                            Ok(output) => {
                                let msg = if output.is_empty() || output.contains("Already up to date") {
                                    "Already up to date".to_string()
                                } else {
                                    "Pull complete".to_string()
                                };
                                app.status_message = Some(msg);
                                // Refresh diff after pull
                                if let Ok(mut new_diff) = git::get_uncommitted_diff(repo) {
                                    let staged = git::get_staged_files(repo).unwrap_or_default();
                                    for file in &mut new_diff.files {
                                        if staged.contains(&file.path) {
                                            file.viewed = true;
                                        }
                                    }
                                    diff = new_diff;
                                    tree = tree::FileTree::from_files(&diff.files);
                                    original_diff = diff.clone();
                                }
                                // Refresh commit log
                                if let Ok(log) = git::get_commit_log(repo, 200) {
                                    app.commits = log;
                                }
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Pull failed: {}", e));
                            }
                        }
                        break;
                    }
                    KeyAction::GitPush => {
                        match git::git_push(repo) {
                            Ok(_) => {
                                app.status_message = Some("Push complete".to_string());
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Push failed: {}", e));
                            }
                        }
                        break;
                    }
                    KeyAction::Commit(msg) => {
                        if let Err(e) = git::create_commit(repo, &msg) {
                            app.status_message = Some(format!("Commit failed: {}", e));
                        }
                        // Refresh diff immediately after commit
                        if let Ok(new_diff) = git::get_uncommitted_diff(repo) {
                            diff = new_diff;
                            tree = tree::FileTree::from_files(&diff.files);
                            original_diff = diff.clone();
                            app.reset_diff_state();
                        }
                        break;
                    }
                    KeyAction::LoadCommitDiff(oid) => {
                        let commit_info = app
                            .commits
                            .iter()
                            .find(|c| c.oid == oid);
                        let message = commit_info
                            .map(|c| c.message.clone())
                            .unwrap_or_default();
                        let short_oid = commit_info
                            .map(|c| c.short_oid.clone())
                            .unwrap_or_else(|| oid[..7.min(oid.len())].to_string());

                        app.loading_message = Some(format!("Loading commit {}...", short_oid));
                        let vis = tree.flatten(&app.collapsed);
                        terminal.draw(|frame| ui::draw(frame, &app, &diff, &vis, &label, ss, theme, colors))?;
                        app.loading_message = None;

                        match git::get_commit_diff(repo, &oid) {
                            Ok(new_diff) => {
                                diff = new_diff;
                                tree = tree::FileTree::from_files(&diff.files);
                                label = format!("{} {}", short_oid, message);
                                app.reset_diff_state();
                                app.view_context =
                                    app::ViewContext::Commit { short_oid, message, return_tab: app::Tab::Commits };
                                app.active_tab = app::Tab::Diff;
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Failed to load commit: {}", e));
                            }
                        }
                        break;
                    }
                    KeyAction::LoadPrDiff(number) => {
                        if let Some((ref owner, ref repo_name)) = github_remote {
                            let pr_info = app.prs.iter().find(|p| p.number == number);
                            let title = pr_info
                                .map(|p| p.title.clone())
                                .unwrap_or_default();

                            app.loading_message = Some(format!("Loading PR #{}...", number));
                            let vis = tree.flatten(&app.collapsed);
                            terminal.draw(|frame| ui::draw(frame, &app, &diff, &vis, &label, ss, theme, colors))?;
                            app.loading_message = None;

                            match github::GitHubProvider::new(owner.clone(), repo_name.clone())
                                .and_then(|provider| provider.fetch_diff(&number.to_string()))
                            {
                                Ok(new_diff) => {
                                    diff = new_diff;
                                    tree = tree::FileTree::from_files(&diff.files);
                                    label = format!("#{} {}", number, title);
                                    app.reset_diff_state();
                                    app.view_context = app::ViewContext::PullRequest {
                                        number,
                                        title,
                                        return_tab: app::Tab::PRs,
                                    };
                                    app.active_tab = app::Tab::Diff;
                                }
                                Err(e) => {
                                    app.status_message = Some(format!("Failed to load PR: {}", e));
                                }
                            }
                        }
                        break;
                    }
                    KeyAction::ReturnToDefault => {
                        let return_tab = match &app.view_context {
                            app::ViewContext::Commit { return_tab, .. }
                            | app::ViewContext::PullRequest { return_tab, .. } => *return_tab,
                            _ => app::Tab::Diff,
                        };
                        diff = original_diff.clone();
                        label = original_label.clone();
                        tree = tree::FileTree::from_files(&diff.files);
                        app.reset_diff_state();
                        app.view_context = app::ViewContext::Default;
                        app.active_tab = return_tab;
                        break;
                    }
                    KeyAction::Continue => {}
                }
            }

            // Recompute visible tree in case a fold was toggled
            visible = tree.flatten(&app.collapsed);
            if !visible.is_empty() && app.selected >= visible.len() {
                app.selected = visible.len() - 1;
                app.update_selected_file(&visible);
            }

            // Also keep list scroll in sync
            let area_height = (terminal.size()?.height as usize).saturating_sub(2);
            if app.active_tab == app::Tab::Commits {
                app.ensure_commit_visible(area_height);
            } else if app.active_tab == app::Tab::PRs {
                app.ensure_pr_visible(area_height);
            }

            // If no more events are queued, break to redraw
            if !event::poll(std::time::Duration::ZERO)? {
                break;
            }
        }
    }
}

/// Check whether two diffs have the same content (ignoring UI state like `viewed`).
fn diff_content_eq(a: &model::Diff, b: &model::Diff) -> bool {
    if a.files.len() != b.files.len() {
        return false;
    }
    a.files.iter().zip(b.files.iter()).all(|(fa, fb)| {
        fa.path == fb.path
            && fa.added == fb.added
            && fa.removed == fb.removed
            && fa.hunks.len() == fb.hunks.len()
    })
}

/// Carry over `viewed` flags from the old diff to a new diff.
fn transfer_viewed(old: &model::Diff, new: &mut model::Diff) {
    for file in &mut new.files {
        if let Some(old_file) = old.files.iter().find(|f| f.path == file.path) {
            file.viewed = old_file.viewed;
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
            anyhow::bail!(
                "invalid PR reference '{}' — expected owner/repo#number",
                rev
            );
        }
        Ok((
            slug_parts[0].to_string(),
            slug_parts[1].to_string(),
            pr_number.to_string(),
        ))
    } else {
        anyhow::bail!("invalid PR reference '{}'", rev);
    }
}
