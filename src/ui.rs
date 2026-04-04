use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::parsing::SyntaxSet;

use crate::app::{App, DiffViewMode, Focus, Tab, ViewContext};
use crate::config::ColorsConfig;
use crate::model;
use crate::split::{self, SplitRow, InlineSpan};
use crate::tree::{FlatEntry, FlatEntryKind};

const SIDEBAR_WIDTH: u16 = 40;

#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    app: &App,
    diff: &model::Diff,
    visible: &[FlatEntry],
    branch: &str,
    ss: &SyntaxSet,
    theme: &Theme,
    colors: &ColorsConfig,
) {
    let area = frame.area();

    if let Some(bg) = colors.bg {
        frame.render_widget(Block::default().style(Style::default().bg(bg)), area);
    }

    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ]).split(area);

    draw_tab_bar(frame, app, outer[0], colors);

    if diff.files.is_empty() && app.active_tab == Tab::Diff {
        let message = Paragraph::new(format!("No changes found in {branch}."))
            .style(Style::default().fg(colors.fg_muted))
            .block(Block::default().borders(Borders::ALL).title(" nit "));
        frame.render_widget(message, outer[1]);
    } else {
        match app.active_tab {
            Tab::Diff => {
                if app.show_sidebar {
                    let panels = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Length(SIDEBAR_WIDTH),
                            Constraint::Min(1),
                        ])
                        .split(outer[1]);

                    draw_sidebar(frame, app, diff, visible, panels[0], colors);
                    draw_file_diff(frame, app, diff, panels[1], ss, theme, colors);
                } else {
                    draw_file_diff(frame, app, diff, outer[1], ss, theme, colors);
                }
            }
            Tab::Commits => {
                draw_commit_list(frame, app, outer[1], colors);
            }
        }
    }

    if app.searching {
        draw_search_input(frame, app, outer[2], colors);
    } else {
        draw_status_bar(frame, app, diff, branch, outer[2], colors);
    }
}

fn is_folder_viewed(diff: &model::Diff, folder_path: &str) -> bool {
    let prefix = format!("{}/", folder_path);
    let files: Vec<_> = diff.files.iter().filter(|f| f.path.starts_with(&prefix)).collect();
    !files.is_empty() && files.iter().all(|f| f.viewed)
}

fn folder_stats(diff: &model::Diff, folder_path: &str) -> (usize, usize) {
    let prefix = format!("{}/", folder_path);
    let added: usize = diff.files.iter().filter(|f| f.path.starts_with(&prefix)).map(|f| f.added).sum();
    let removed: usize = diff.files.iter().filter(|f| f.path.starts_with(&prefix)).map(|f| f.removed).sum();
    (added, removed)
}

fn draw_sidebar(
    frame: &mut Frame,
    app: &App,
    diff: &model::Diff,
    visible: &[FlatEntry],
    area: Rect,
    colors: &ColorsConfig,
) {
    let is_focused = matches!(app.focus, Focus::Sidebar);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == app.selected;
            let marker = if is_selected { "▸ " } else { "  " };
            let indent = "  ".repeat(entry.depth);

            match &entry.kind {
                FlatEntryKind::Folder { path, name, expanded } => {
                    let viewed = if is_folder_viewed(diff, path) { "✓ " } else { "  " };
                    let arrow = if *expanded { "▾ " } else { "▸ " };
                    let (added, removed) = folder_stats(diff, path);
                    let stats = format!("+{} -{}", added, removed);
                    let name_style = if is_selected {
                        Style::default().fg(colors.fg_selected).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.fg_accent)
                    };
                    Line::from(vec![
                        Span::styled(viewed, Style::default().fg(colors.fg_added)),
                        Span::raw(marker),
                        Span::raw(indent),
                        Span::styled(arrow, Style::default().fg(colors.fg_muted)),
                        Span::styled(format!("{}/", name), name_style),
                        Span::raw("  "),
                        Span::styled(stats, Style::default().fg(colors.fg_muted)),
                    ])
                }
                FlatEntryKind::File { file_index, name } => {
                    let file = &diff.files[*file_index];
                    let viewed = if file.viewed { "✓ " } else { "  " };
                    let stats = format!("+{} -{}", file.added, file.removed);
                    let name_style = if is_selected {
                        Style::default().fg(colors.fg_selected)
                    } else {
                        Style::default().fg(colors.fg)
                    };
                    Line::from(vec![
                        Span::styled(viewed, Style::default().fg(colors.fg_added)),
                        Span::raw(marker),
                        Span::raw(indent),
                        Span::raw("  "), // align with folder names (arrow placeholder)
                        Span::styled(name.clone(), name_style),
                        Span::raw("  "),
                        Span::styled(stats, Style::default().fg(colors.fg_muted)),
                    ])
                }
            }
        })
        .collect();

    let border_style = if is_focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let file_list = Paragraph::new(lines)
        .scroll((app.sidebar_scroll as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Files "),
        );

    frame.render_widget(file_list, area);
}

fn draw_file_diff(
    frame: &mut Frame,
    app: &App,
    diff: &model::Diff,
    area: Rect,
    ss: &SyntaxSet,
    theme: &Theme,
    colors: &ColorsConfig,
) {
    if app.view_mode == DiffViewMode::SideBySide {
        draw_file_diff_split(frame, app, diff, area, ss, theme, colors);
        return;
    }

    let Some(file) = diff.files.get(app.selected_file) else {
        return;
    };
    let is_focused = matches!(app.focus, Focus::Diff);
    let inner_height = area.height.saturating_sub(2) as usize; // subtract borders

    // Build the set of match line indices for highlighting
    let match_lines = app.search.as_ref().map(|s| {
        let current_line = s.matches.get(s.current).map(|m| m.line_index);
        let all: std::collections::HashSet<usize> = s.matches.iter().map(|m| m.line_index).collect();
        (all, current_line)
    });

    let syntax = Path::new(&file.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let vis_start = app.scroll;
    let vis_end = vis_start + inner_height;

    let mut lines: Vec<Line> = Vec::with_capacity(inner_height);
    let mut line_index: usize = 0;

    for hunk in &file.hunks {
        let hunk_size = 1 + hunk.lines.len();

        // Skip hunks entirely above the viewport
        if line_index + hunk_size <= vis_start {
            line_index += hunk_size;
            continue;
        }

        // Stop once past the viewport
        if line_index >= vis_end {
            break;
        }

        // Hunk header
        if line_index >= vis_start {
            let mut hunk_header_spans = vec![Span::styled(
                hunk.header.trim_end().to_string(),
                Style::default().fg(colors.fg_muted),
            )];

            if let Some((ref all, current)) = match_lines
                && all.contains(&line_index)
            {
                let is_current = current == Some(line_index);
                let bg = if is_current { colors.bg_search_current } else { colors.bg_search_match };
                hunk_header_spans = highlight_search_in_spans(hunk_header_spans, app, bg);
            }

            lines.push(Line::from(hunk_header_spans));
        }
        line_index += 1;

        let mut highlighter = HighlightLines::new(syntax, theme);

        for line in &hunk.lines {
            if line_index >= vis_end {
                break;
            }

            let content = line.content.trim_end();

            if line_index >= vis_start {
                let diff_bg = match line.kind {
                    model::LineKind::Added => Some(colors.bg_added),
                    model::LineKind::Removed => Some(colors.bg_removed),
                    model::LineKind::Context => None,
                };

                let prefix = match line.kind {
                    model::LineKind::Added => "+",
                    model::LineKind::Removed => "-",
                    model::LineKind::Context => " ",
                };

                let prefix_style = match line.kind {
                    model::LineKind::Added => Style::default().fg(colors.fg_added),
                    model::LineKind::Removed => Style::default().fg(colors.fg_removed),
                    model::LineKind::Context => Style::default().fg(colors.fg),
                };
                let prefix_style = if let Some(bg) = diff_bg {
                    prefix_style.bg(bg)
                } else {
                    prefix_style
                };

                let mut spans = vec![Span::styled(prefix, prefix_style)];

                if let Ok(highlighted) = highlighter.highlight_line(content, ss) {
                    for (style, text) in highlighted {
                        let fg = Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        );
                        let mut s = Style::default().fg(fg);
                        if let Some(bg) = diff_bg {
                            s = s.bg(bg);
                        }
                        spans.push(Span::styled(text.to_string(), s));
                    }
                } else {
                    let mut s = Style::default();
                    if let Some(bg) = diff_bg {
                        s = s.bg(bg);
                    }
                    spans.push(Span::styled(content.to_string(), s));
                }

                // Apply search highlighting on matching lines
                if let Some((ref all, current)) = match_lines
                    && all.contains(&line_index)
                {
                    let is_current = current == Some(line_index);
                    let bg = if is_current { colors.bg_search_current } else { colors.bg_search_match };
                    spans = highlight_search_in_spans(spans, app, bg);
                }

                lines.push(Line::from(spans));
            } else {
                // Pre-viewport: feed highlighter to maintain state within hunk
                let _ = highlighter.highlight_line(content, ss);
            }

            line_index += 1;
        }
    }

    if file.hunks.is_empty() {
        lines.push(Line::from(Span::styled(
            "Binary file changed",
            Style::default().fg(colors.fg_muted),
        )));
    }

    let viewed_indicator = if file.viewed { " ✓" } else { "" };
    let title = format!(" {}{} ", file.path, viewed_indicator);

    let border_style = if is_focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    // No .scroll() needed — we only built visible lines
    let diff_view = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
        );

    frame.render_widget(diff_view, area);
}

/// Highlight occurrences of the search query within a list of spans.
/// Returns owned spans since we split and rebuild the text.
fn highlight_search_in_spans(spans: Vec<Span<'_>>, app: &App, bg: Color) -> Vec<Span<'static>> {
    let query = match &app.search {
        Some(s) => &s.query,
        None => {
            return spans
                .into_iter()
                .map(|s| Span::styled(s.content.to_string(), s.style))
                .collect();
        }
    };

    let query_lower = query.to_lowercase();
    let mut result: Vec<Span<'static>> = Vec::new();

    for span in spans {
        let text = span.content.to_string();
        let style = span.style;

        // Build a mapping from lowercase byte offsets back to original byte offsets.
        // This is needed because to_lowercase() can change byte lengths of characters.
        let mut lower_to_orig: Vec<usize> = Vec::new();
        let mut text_lower = String::new();
        for (orig_byte, ch) in text.char_indices() {
            let lower_ch = ch.to_lowercase();
            for lc in lower_ch {
                let lc_len = lc.len_utf8();
                for _ in 0..lc_len {
                    lower_to_orig.push(orig_byte);
                }
                text_lower.push(lc);
            }
        }
        // Sentinel so we can look up the "end" position (= original string length)
        lower_to_orig.push(text.len());

        let mut last_orig = 0;
        for (lower_start, matched) in text_lower.match_indices(&query_lower) {
            let lower_end = lower_start + matched.len();
            let orig_start = lower_to_orig[lower_start];
            let orig_end = lower_to_orig[lower_end];

            if orig_start > last_orig {
                result.push(Span::styled(text[last_orig..orig_start].to_string(), style));
            }
            result.push(Span::styled(
                text[orig_start..orig_end].to_string(),
                style.bg(bg).add_modifier(Modifier::BOLD),
            ));
            last_orig = orig_end;
        }
        if last_orig < text.len() {
            result.push(Span::styled(text[last_orig..].to_string(), style));
        } else if last_orig == 0 {
            result.push(Span::styled(text, style));
        }
    }

    result
}

fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let diff_label = match &app.view_context {
        ViewContext::Default => "Diff".to_string(),
        ViewContext::Commit { short_oid, .. } => format!("Diff ({})", short_oid),
    };

    let active_style = Style::default()
        .fg(colors.fg_selected)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(colors.fg_muted);
    let sep_style = Style::default().fg(colors.fg_muted);

    let diff_style = if app.active_tab == Tab::Diff {
        active_style
    } else {
        inactive_style
    };
    let commits_style = if app.active_tab == Tab::Commits {
        active_style
    } else {
        inactive_style
    };

    let key_style = Style::default().fg(colors.fg_muted);

    let tabs = Line::from(vec![
        Span::raw(" "),
        Span::styled("1 ", key_style),
        Span::styled(diff_label, diff_style),
        Span::styled(" │ ", sep_style),
        Span::styled("2 ", key_style),
        Span::styled("Commits", commits_style),
    ]);

    frame.render_widget(Paragraph::new(tabs), area);
}

fn draw_commit_list(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    if app.commits.is_empty() {
        let message = Paragraph::new("No commits found.")
            .style(Style::default().fg(colors.fg_muted))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(colors.border_focused))
                    .title(" Commits "),
            );
        frame.render_widget(message, area);
        return;
    }

    let inner_width = area.width.saturating_sub(2) as usize;

    let lines: Vec<Line> = app
        .commits
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let is_selected = i == app.commit_selected;
            let marker = if is_selected { "▸ " } else { "  " };

            // Reserve space: marker(2) + oid(7) + gap(2) + date(~15) + gap(2) + author(~15)
            let meta = format!("{}  {}", commit.author, commit.date);
            let meta_width = meta.len();
            let prefix_width = 2 + 7 + 2; // marker + oid + gap
            let msg_budget = inner_width
                .saturating_sub(prefix_width)
                .saturating_sub(meta_width + 2);

            let msg: String = if commit.message.len() > msg_budget && msg_budget > 3 {
                let truncated: String = commit.message.chars().take(msg_budget - 3).collect();
                format!("{}...", truncated)
            } else {
                commit.message.clone()
            };

            let padding = msg_budget.saturating_sub(msg.len());

            let msg_style = if is_selected {
                Style::default().fg(colors.fg_selected)
            } else {
                Style::default().fg(colors.fg)
            };

            Line::from(vec![
                Span::styled(marker, msg_style),
                Span::styled(&commit.short_oid, Style::default().fg(colors.fg_accent)),
                Span::raw("  "),
                Span::styled(msg, msg_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(&commit.author, Style::default().fg(colors.fg_info)),
                Span::raw("  "),
                Span::styled(&commit.date, Style::default().fg(colors.fg_muted)),
            ])
        })
        .collect();

    let commit_list = Paragraph::new(lines)
        .scroll((app.commit_scroll as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border_focused))
                .title(" Commits "),
        );

    frame.render_widget(commit_list, area);
}

fn draw_search_input(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let input = Line::from(vec![
        Span::styled("/", Style::default().fg(colors.fg_accent)),
        Span::styled(&app.search_input, Style::default().fg(colors.fg)),
        Span::styled("█", Style::default().fg(colors.fg_muted)),
    ]);

    frame.render_widget(Paragraph::new(input), area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, diff: &model::Diff, branch: &str, area: Rect, colors: &ColorsConfig) {
    let viewed_count = diff.files.iter().filter(|f| f.viewed).count();
    let total_added: usize = diff.files.iter().map(|f| f.added).sum();
    let total_removed: usize = diff.files.iter().map(|f| f.removed).sum();

    let search_info = if let Some(ref search) = app.search {
        if search.matches.is_empty() {
            format!("  /{} (no matches)", search.query)
        } else {
            format!("  /{} ({}/{})", search.query, search.current + 1, search.matches.len())
        }
    } else {
        String::new()
    };

    let mode_hint = match app.view_mode {
        crate::app::DiffViewMode::Unified => "s: split",
        crate::app::DiffViewMode::SideBySide => "s: unified",
    };

    let esc_hint = if matches!(app.view_context, ViewContext::Commit { .. }) {
        "  Esc: back"
    } else {
        ""
    };

    let hints = match app.active_tab {
        Tab::Commits => {
            "j/k: navigate  Ctrl+d/u: page  Enter: view diff  gg/G: jump  1: diff  q: quit".to_string()
        }
        Tab::Diff => match app.focus {
            Focus::Sidebar => format!(
                "j/k: navigate  l/Enter: open  Space: fold  v/V: viewed  G/gg: jump  /: search  {mode_hint}  Tab: diff{esc_hint}  2: commits  q: quit"
            ),
            Focus::Diff => format!(
                "j/k: scroll  Ctrl+d/u: page  Ctrl+n/p: hunk  /: search  n/N: match  {mode_hint}  h: sidebar{esc_hint}  2: commits  q: quit"
            ),
        },
    };

    let status = Line::from(vec![
        Span::raw(" "),
        Span::styled(branch, Style::default().fg(colors.fg_info)),
        Span::raw("  "),
        Span::styled(
            format!("{}/{} viewed", viewed_count, diff.files.len()),
            Style::default().fg(colors.fg),
        ),
        Span::raw("  "),
        Span::styled(format!("+{}", total_added), Style::default().fg(colors.fg_added)),
        Span::raw(" "),
        Span::styled(format!("-{}", total_removed), Style::default().fg(colors.fg_removed)),
        Span::styled(search_info, Style::default().fg(colors.fg_accent)),
        Span::raw("  "),
        Span::styled(&hints, Style::default().fg(colors.fg_muted)),
    ]);

    frame.render_widget(Paragraph::new(status), area);
}

fn draw_file_diff_split(
    frame: &mut Frame,
    app: &App,
    diff: &model::Diff,
    area: Rect,
    ss: &SyntaxSet,
    theme: &Theme,
    colors: &ColorsConfig,
) {
    let Some(file) = diff.files.get(app.selected_file) else {
        return;
    };
    let is_focused = matches!(app.focus, Focus::Diff);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left_area = panels[0];
    let right_area = panels[1];
    let inner_height = left_area.height.saturating_sub(2) as usize;

    let rows = split::build_split_rows(&file.hunks);

    let match_lines = app.search.as_ref().map(|s| {
        let current_line = s.matches.get(s.current).map(|m| m.line_index);
        let all: std::collections::HashSet<usize> = s.matches.iter().map(|m| m.line_index).collect();
        (all, current_line)
    });

    let syntax = Path::new(&file.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let vis_start = app.scroll;
    let vis_end = vis_start + inner_height;

    let mut left_lines: Vec<Line> = Vec::with_capacity(inner_height);
    let mut right_lines: Vec<Line> = Vec::with_capacity(inner_height);

    // We need separate highlighters for left and right panels.
    // Reset them at each hunk header.
    let mut left_hl = HighlightLines::new(syntax, theme);
    let mut right_hl = HighlightLines::new(syntax, theme);

    for (row_idx, row) in rows.iter().enumerate() {
        if row_idx >= vis_end {
            break;
        }

        // Reset highlighters on hunk boundaries
        if matches!(row, SplitRow::HunkHeader(_)) {
            left_hl = HighlightLines::new(syntax, theme);
            right_hl = HighlightLines::new(syntax, theme);
        }

        if row_idx < vis_start {
            // Feed highlighters to maintain state even for off-screen rows
            match row {
                SplitRow::Context(line) | SplitRow::Paired { left: line, .. } => {
                    let _ = left_hl.highlight_line(line.content.trim_end(), ss);
                }
                SplitRow::LeftOnly(line) => {
                    let _ = left_hl.highlight_line(line.content.trim_end(), ss);
                }
                _ => {}
            }
            match row {
                SplitRow::Context(line) | SplitRow::Paired { right: line, .. } => {
                    let _ = right_hl.highlight_line(line.content.trim_end(), ss);
                }
                SplitRow::RightOnly(line) => {
                    let _ = right_hl.highlight_line(line.content.trim_end(), ss);
                }
                _ => {}
            }
            continue;
        }

        let search_bg = match_lines.as_ref().and_then(|(all, current)| {
            if all.contains(&row_idx) {
                let is_current = *current == Some(row_idx);
                Some(if is_current { colors.bg_search_current } else { colors.bg_search_match })
            } else {
                None
            }
        });

        match row {
            SplitRow::HunkHeader(header) => {
                let text = header.trim_end().to_string();
                let mut left_spans = vec![Span::styled(text.clone(), Style::default().fg(colors.fg_muted))];
                let mut right_spans = vec![Span::styled(text, Style::default().fg(colors.fg_muted))];
                if let Some(bg) = search_bg {
                    left_spans = highlight_search_in_spans(left_spans, app, bg);
                    right_spans = highlight_search_in_spans(right_spans, app, bg);
                }
                left_lines.push(Line::from(left_spans));
                right_lines.push(Line::from(right_spans));
            }
            SplitRow::Context(line) => {
                let content = line.content.trim_end();
                let mut l_spans = build_syntax_spans(content, &mut left_hl, ss, None, " ", colors);
                let mut r_spans = build_syntax_spans(content, &mut right_hl, ss, None, " ", colors);
                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                left_lines.push(Line::from(l_spans));
                right_lines.push(Line::from(r_spans));
            }
            SplitRow::Paired { left, right, left_spans: l_inline, right_spans: r_inline } => {
                let l_content = left.content.trim_end();
                let r_content = right.content.trim_end();

                let mut l_spans = build_syntax_spans(l_content, &mut left_hl, ss, Some(colors.bg_removed), "-", colors);
                let mut r_spans = build_syntax_spans(r_content, &mut right_hl, ss, Some(colors.bg_added), "+", colors);

                l_spans = apply_inline_highlight(l_spans, l_inline, colors.bg_inline_removed);
                r_spans = apply_inline_highlight(r_spans, r_inline, colors.bg_inline_added);

                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                left_lines.push(Line::from(l_spans));
                right_lines.push(Line::from(r_spans));
            }
            SplitRow::LeftOnly(line) => {
                let content = line.content.trim_end();
                let mut l_spans = build_syntax_spans(content, &mut left_hl, ss, Some(colors.bg_removed), "-", colors);
                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                }
                left_lines.push(Line::from(l_spans));
                right_lines.push(Line::from(vec![]));
            }
            SplitRow::RightOnly(line) => {
                let content = line.content.trim_end();
                let mut r_spans = build_syntax_spans(content, &mut right_hl, ss, Some(colors.bg_added), "+", colors);
                if let Some(bg) = search_bg {
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                left_lines.push(Line::from(vec![]));
                right_lines.push(Line::from(r_spans));
            }
        }
    }

    if file.hunks.is_empty() {
        let msg = Span::styled("Binary file changed", Style::default().fg(colors.fg_muted));
        left_lines.push(Line::from(msg.clone()));
        right_lines.push(Line::from(msg));
    }

    let viewed_indicator = if file.viewed { " ✓" } else { "" };
    let border_style = if is_focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let left_title = format!(" {}{} ", file.path, viewed_indicator);
    let left_widget = Paragraph::new(left_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(left_title, Style::default().add_modifier(Modifier::BOLD))),
    );

    let right_title = format!(" {}{} ", file.path, viewed_indicator);
    let right_widget = Paragraph::new(right_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(right_title, Style::default().add_modifier(Modifier::BOLD))),
    );

    frame.render_widget(left_widget, left_area);
    frame.render_widget(right_widget, right_area);
}

/// Build syntax-highlighted spans for a single line with an optional diff background.
fn build_syntax_spans<'a>(
    content: &str,
    highlighter: &mut HighlightLines,
    ss: &SyntaxSet,
    diff_bg: Option<Color>,
    prefix: &str,
    colors: &ColorsConfig,
) -> Vec<Span<'a>> {
    let prefix_style = match prefix {
        "+" => Style::default().fg(colors.fg_added),
        "-" => Style::default().fg(colors.fg_removed),
        _ => Style::default().fg(colors.fg),
    };
    let prefix_style = if let Some(bg) = diff_bg { prefix_style.bg(bg) } else { prefix_style };

    let mut spans = vec![Span::styled(prefix.to_string(), prefix_style)];

    if let Ok(highlighted) = highlighter.highlight_line(content, ss) {
        for (style, text) in highlighted {
            let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
            let mut s = Style::default().fg(fg);
            if let Some(bg) = diff_bg {
                s = s.bg(bg);
            }
            spans.push(Span::styled(text.to_string(), s));
        }
    } else {
        let mut s = Style::default();
        if let Some(bg) = diff_bg {
            s = s.bg(bg);
        }
        spans.push(Span::styled(content.to_string(), s));
    }

    spans
}

/// Apply inline change highlighting to syntax-highlighted spans.
/// Splits spans at InlineSpan boundaries and applies highlight_bg to changed segments.
/// The prefix span (index 0) is not modified.
fn apply_inline_highlight<'a>(
    spans: Vec<Span<'a>>,
    inline_spans: &[InlineSpan],
    highlight_bg: Color,
) -> Vec<Span<'a>> {
    if inline_spans.is_empty() || inline_spans.iter().all(|s| !s.changed) {
        return spans;
    }

    let mut result: Vec<Span<'a>> = Vec::new();

    // The first span is the prefix (+/-/space) — pass through unchanged
    let mut span_iter = spans.into_iter();
    if let Some(prefix) = span_iter.next() {
        result.push(prefix);
    }

    // Collect remaining spans and track byte positions (relative to content, not prefix)
    let remaining: Vec<Span<'a>> = span_iter.collect();
    let mut content_offset: usize = 0;

    for span in remaining {
        let span_text = span.content.to_string();
        let span_start = content_offset;
        let span_end = span_start + span_text.len();
        let base_style = span.style;

        let mut pos = span_start;
        for inline in inline_spans {
            // Find overlap between this span segment and this inline span
            let overlap_start = pos.max(inline.start);
            let overlap_end = span_end.min(inline.end);

            if overlap_start >= overlap_end {
                continue;
            }

            // Emit any part before this inline span
            if pos < overlap_start {
                let s = &span_text[pos - span_start..overlap_start - span_start];
                result.push(Span::styled(s.to_string(), base_style));
            }

            // Emit the overlapping part with highlight
            let s = &span_text[overlap_start - span_start..overlap_end - span_start];
            let style = if inline.changed {
                base_style.bg(highlight_bg)
            } else {
                base_style
            };
            result.push(Span::styled(s.to_string(), style));

            pos = overlap_end;
        }

        // Emit any remaining part after all inline spans
        if pos < span_end {
            let s = &span_text[pos - span_start..];
            result.push(Span::styled(s.to_string(), base_style));
        }

        content_offset = span_end;
    }

    result
}

/// Count the total number of rendered lines for a file's diff.
pub fn diff_line_count(file: &model::DiffFile, view_mode: DiffViewMode) -> usize {
    match view_mode {
        DiffViewMode::Unified => file.hunks.iter().map(|h| 1 + h.lines.len()).sum(),
        DiffViewMode::SideBySide => split::split_row_count(&file.hunks),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::ColorsConfig;
    use crate::model::LineKind;
    use crate::test_helpers::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use syntect::highlighting::ThemeSet;
    use syntect::parsing::SyntaxSet;

    fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(w, h)).unwrap()
    }

    fn default_theme() -> syntect::highlighting::Theme {
        let ts = ThemeSet::load_defaults();
        ts.themes["base16-ocean.dark"].clone()
    }

    fn render(
        terminal: &mut Terminal<TestBackend>,
        app: &App,
        diff: &model::Diff,
        visible: &[FlatEntry],
    ) {
        let ss = SyntaxSet::load_defaults_newlines();
        let theme = default_theme();
        let colors = ColorsConfig::default();
        terminal
            .draw(|frame| draw(frame, app, diff, visible, "main", &ss, &theme, &colors))
            .unwrap();
    }

    #[test]
    fn render_empty_diff() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_diff(vec![]);
        let visible = flat_entries_for(&diff);
        let app = App::new();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_single_file() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_diff(vec![make_file(
            "src/main.rs",
            vec![make_hunk(
                "@@ -1,3 +1,4 @@",
                vec![
                    make_line(LineKind::Context, "fn main() {", Some(1), Some(1)),
                    make_line(LineKind::Added, "    println!(\"hello\");", None, Some(2)),
                    make_line(LineKind::Context, "}", Some(2), Some(3)),
                ],
            )],
        )]);
        let visible = flat_entries_for(&diff);
        let app = App::new();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_sidebar_focused() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_simple_diff(3);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.focus = Focus::Sidebar;
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_diff_focused() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_simple_diff(3);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.focus = Focus::Diff;
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_sidebar_hidden() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_simple_diff(2);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.show_sidebar = false;
        app.focus = Focus::Diff;
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_with_search_matches() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_diff(vec![make_file(
            "a.rs",
            vec![make_hunk(
                "@@ -1 +1 @@",
                vec![make_line(LineKind::Context, "search target", Some(1), Some(1))],
            )],
        )]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.search_input = "search".to_string();
        app.submit_search(&diff);
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_search_input_mode() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_simple_diff(1);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.searching = true;
        app.search_input = "query".to_string();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_scrolled_near_bottom() {
        let mut terminal = test_terminal(120, 40);
        let lines: Vec<_> = (0..50)
            .map(|i| make_line(LineKind::Context, &format!("line {}", i), Some(i), Some(i)))
            .collect();
        let diff = make_diff(vec![make_file("big.rs", vec![make_hunk("@@ -1 +1 @@", lines)])]);
        let visible = flat_entries_for(&diff);
        let mut app = App::new();
        app.scroll = 30;
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_small_terminal() {
        let mut terminal = test_terminal(20, 5);
        let diff = make_simple_diff(2);
        let visible = flat_entries_for(&diff);
        let app = App::new();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_binary_file() {
        let mut terminal = test_terminal(120, 40);
        let diff = make_diff(vec![make_file("image.png", vec![])]);
        let visible = flat_entries_for(&diff);
        let app = App::new();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn render_long_filename() {
        let mut terminal = test_terminal(120, 40);
        let long_path = format!("src/very/deeply/nested/directory/structure/{}", "a".repeat(60));
        let diff = make_diff(vec![make_file(
            &long_path,
            vec![make_hunk("@@ -1 +1 @@", vec![make_line(LineKind::Context, "x", Some(1), Some(1))])],
        )]);
        let visible = flat_entries_for(&diff);
        let app = App::new();
        render(&mut terminal, &app, &diff, &visible);
    }

    #[test]
    fn diff_line_count_correct() {
        let file = make_file(
            "a.rs",
            vec![
                make_hunk("@@", vec![
                    make_line(LineKind::Context, "a", Some(1), Some(1)),
                    make_line(LineKind::Context, "b", Some(2), Some(2)),
                ]),
                make_hunk("@@", vec![
                    make_line(LineKind::Added, "c", None, Some(3)),
                ]),
            ],
        );
        // 2 hunks: (1 header + 2 lines) + (1 header + 1 line) = 5
        assert_eq!(diff_line_count(&file, DiffViewMode::Unified), 5);
    }
}
