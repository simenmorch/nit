use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
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
            Tab::PRs => {
                draw_pr_list(frame, app, outer[1], colors);
            }
        }
    }

    if app.committing {
        draw_commit_modal(frame, app, area, colors);
    }

    if app.branch_modal.open {
        draw_branch_modal(frame, app, area, colors);
    }

    if app.show_help {
        draw_help_modal(frame, area, colors);
    }

    if let Some(ref confirm) = app.confirm {
        draw_confirm_modal(frame, area, &confirm.message, colors);
    }

    if let Some(ref msg) = app.loading_message {
        draw_loading_overlay(frame, outer[1], msg, colors);
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
                    let (status_char, status_color) = match &file.status {
                        model::FileStatus::Added => ("A", colors.fg_added),
                        model::FileStatus::Modified => ("M", colors.fg_accent),
                        model::FileStatus::Deleted => ("D", colors.fg_removed),
                        model::FileStatus::Renamed { .. } => ("R", colors.fg_info),
                    };
                    let name_style = if is_selected {
                        Style::default().fg(colors.fg_selected)
                    } else {
                        Style::default().fg(colors.fg)
                    };
                    let file_indent = "  ".repeat(entry.depth.saturating_sub(1));
                    let status_badge = format!("{} ", status_char);
                    Line::from(vec![
                        Span::styled(viewed, Style::default().fg(colors.fg_added)),
                        Span::raw(marker),
                        Span::raw(file_indent),
                        Span::styled(status_badge, Style::default().fg(status_color)),
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

    // Get search match info from pre-computed state
    let search_info = app.search.as_ref().map(|s| {
        let current_line = s.matches.get(s.current).map(|m| m.line_index);
        (&s.match_lines, current_line)
    });

    let syntax = Path::new(&file.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let vis_start = app.scroll;
    // Build extra lines to compensate for wrapping pushing content down
    let vis_end = vis_start + inner_height * 2;
    let inner_width = area.width.saturating_sub(2) as usize;
    let wrap_indent_style = Style::default().fg(colors.fg_muted);

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
            let gutter = "         "; // 4 + 1 + 4 = 9 chars blank gutter
            let mut hunk_header_spans = vec![
                Span::styled(gutter, Style::default().fg(colors.fg_muted)),
                Span::styled(
                    hunk.header.trim_end().to_string(),
                    Style::default().fg(colors.fg_muted),
                ),
            ];

            if let Some((all, current)) = search_info
                && all.contains(&line_index)
            {
                let is_current = current == Some(line_index);
                let bg = if is_current { colors.bg_search_current } else { colors.bg_search_match };
                hunk_header_spans = highlight_search_in_spans(hunk_header_spans, app, bg);
            }

            lines.extend(wrap_line_spans(hunk_header_spans, inner_width, 10, wrap_indent_style));
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

                let old_str = match line.old_num {
                    Some(n) => format!("{:>4}", n),
                    None => "    ".to_string(),
                };
                let new_str = match line.new_num {
                    Some(n) => format!("{:>4}", n),
                    None => "    ".to_string(),
                };
                let gutter = format!("{}{} ", old_str, new_str);
                let mut spans = vec![
                    Span::styled(gutter, Style::default().fg(colors.fg_muted)),
                    Span::styled(prefix, prefix_style),
                ];

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
                if let Some((all, current)) = search_info
                    && all.contains(&line_index)
                {
                    let is_current = current == Some(line_index);
                    let bg = if is_current { colors.bg_search_current } else { colors.bg_search_match };
                    spans = highlight_search_in_spans(spans, app, bg);
                }

                lines.extend(wrap_line_spans(spans, inner_width, 10, wrap_indent_style));
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

    let diff_view = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
        );

    frame.render_widget(diff_view, area);

    let content_height = diff_line_count(file, DiffViewMode::Unified);
    render_scrollbar(frame, area, app.scroll, content_height, inner_height, colors);
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

/// Wrap a line's spans to fit within `max_width` columns.
/// Continuation lines are indented with `indent` spaces using `indent_style`.
fn wrap_line_spans(
    spans: Vec<Span<'_>>,
    max_width: usize,
    indent: usize,
    indent_style: Style,
) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![Line::from(
            spans
                .into_iter()
                .map(|s| Span::styled(s.content.to_string(), s.style))
                .collect::<Vec<_>>(),
        )];
    }

    let styled_chars: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();

    let total = styled_chars.len();
    if total <= max_width {
        return vec![Line::from(
            spans
                .into_iter()
                .map(|s| Span::styled(s.content.to_string(), s.style))
                .collect::<Vec<_>>(),
        )];
    }

    let mut result: Vec<Line<'static>> = Vec::new();
    let mut pos = 0;
    let mut is_first = true;

    while pos < total {
        let content_width = if is_first {
            max_width
        } else {
            max_width.saturating_sub(indent)
        };
        if content_width == 0 {
            break;
        }

        let hard_end = (pos + content_width).min(total);
        // Prefer breaking at a word boundary (last space within the limit)
        let end = if hard_end < total {
            let search_start = pos + indent; // don't break inside the gutter region on first line
            let last_space = styled_chars[search_start.min(hard_end)..hard_end]
                .iter()
                .rposition(|&(c, _)| c == ' ')
                .map(|i| search_start.min(hard_end) + i + 1);
            last_space.unwrap_or(hard_end)
        } else {
            hard_end
        };
        let mut line_spans: Vec<Span<'static>> = Vec::new();

        if !is_first {
            line_spans.push(Span::styled(" ".repeat(indent), indent_style));
        }

        // Coalesce adjacent chars with the same style into spans
        let mut text = String::new();
        let mut style = styled_chars[pos].1;
        for &(c, s) in &styled_chars[pos..end] {
            if s == style {
                text.push(c);
            } else {
                line_spans.push(Span::styled(text, style));
                text = String::new();
                style = s;
                text.push(c);
            }
        }
        if !text.is_empty() {
            line_spans.push(Span::styled(text, style));
        }

        result.push(Line::from(line_spans));
        pos = end;
        is_first = false;
    }

    result
}

fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let diff_label = match &app.view_context {
        ViewContext::Default => "Diff".to_string(),
        ViewContext::Commit { short_oid, .. } => format!("Diff ({})", short_oid),
        ViewContext::PullRequest { number, .. } => format!("Diff (#{})", number),
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
    let prs_style = if app.active_tab == Tab::PRs {
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
        Span::styled(" │ ", sep_style),
        Span::styled("3 ", key_style),
        Span::styled("PRs", prs_style),
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

fn draw_pr_list(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let filtered: Vec<&crate::model::PrInfo> = app.filtered_prs();

    // Build active filter label for the title
    let active_filters: Vec<&str> = crate::app::PR_FILTER_OPTIONS
        .iter()
        .filter(|s| app.pr_filter.enabled.contains(**s))
        .copied()
        .collect();
    let filter_label = if active_filters.is_empty() {
        " Pull Requests (no filter) ".to_string()
    } else {
        format!(" Pull Requests ({}) ", active_filters.join(", "))
    };

    if filtered.is_empty() {
        let msg = if app.prs.is_empty() {
            "No pull requests found."
        } else {
            "No pull requests match the current filter."
        };
        let message = Paragraph::new(msg)
            .style(Style::default().fg(colors.fg_muted))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(colors.border_focused))
                    .title(filter_label),
            );
        frame.render_widget(message, area);

        if app.pr_filter.modal_open {
            draw_pr_filter_modal(frame, app, area, colors);
        }
        return;
    }

    let inner_width = area.width.saturating_sub(2) as usize;

    let lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let is_selected = i == app.pr_selected;
            let marker = if is_selected { "▸ " } else { "  " };

            let number_str = format!("#{}", pr.number);
            let state_badge = &pr.state;

            let meta = format!("{}  {}  {}", pr.author, state_badge, pr.updated_at);
            let meta_width = meta.len();
            let prefix_width = 2 + number_str.len() + 2;
            let title_budget = inner_width
                .saturating_sub(prefix_width)
                .saturating_sub(meta_width + 2);

            let title_chars = pr.title.chars().count();
            let title: String = if title_chars > title_budget && title_budget > 3 {
                let truncated: String = pr.title.chars().take(title_budget - 3).collect();
                format!("{}...", truncated)
            } else {
                pr.title.clone()
            };

            let padding = title_budget.saturating_sub(title.chars().count());

            let title_style = if is_selected {
                Style::default().fg(colors.fg_selected)
            } else {
                Style::default().fg(colors.fg)
            };

            let state_color = match pr.state.as_str() {
                "open" => colors.fg_added,
                "draft" => colors.fg_muted,
                _ => colors.fg_muted,
            };

            Line::from(vec![
                Span::styled(marker, title_style),
                Span::styled(number_str, Style::default().fg(colors.fg_accent)),
                Span::raw("  "),
                Span::styled(title, title_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(&pr.author, Style::default().fg(colors.fg_info)),
                Span::raw("  "),
                Span::styled(state_badge, Style::default().fg(state_color)),
                Span::raw("  "),
                Span::styled(&pr.updated_at, Style::default().fg(colors.fg_muted)),
            ])
        })
        .collect();

    let pr_list = Paragraph::new(lines)
        .scroll((app.pr_scroll as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border_focused))
                .title(filter_label),
        );

    frame.render_widget(pr_list, area);

    if app.pr_filter.modal_open {
        draw_pr_filter_modal(frame, app, area, colors);
    }
}

fn draw_pr_filter_modal(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let options = crate::app::PR_FILTER_OPTIONS;
    let modal_height = options.len() as u16 + 2; // borders
    let modal_width: u16 = 22;

    // Position in top-right of the PR list area
    let x = area.x + area.width.saturating_sub(modal_width + 2);
    let y = area.y + 1;

    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear background
    let clear = Block::default().style(Style::default().bg(colors.bg.unwrap_or(Color::Black)));
    frame.render_widget(clear, modal_area);

    let lines: Vec<Line> = options
        .iter()
        .enumerate()
        .map(|(i, &opt)| {
            let is_selected = i == app.pr_filter.modal_selected;
            let is_enabled = app.pr_filter.enabled.contains(opt);

            let check = if is_enabled { "[x] " } else { "[ ] " };
            let marker = if is_selected { "▸ " } else { "  " };

            let style = if is_selected {
                Style::default().fg(colors.fg_selected).add_modifier(Modifier::BOLD)
            } else if is_enabled {
                Style::default().fg(colors.fg)
            } else {
                Style::default().fg(colors.fg_muted)
            };

            Line::from(vec![
                Span::styled(marker, style),
                Span::styled(check, style),
                Span::styled(opt, style),
            ])
        })
        .collect();

    let modal = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border_focused))
            .title(" Filter ")
            .style(Style::default().bg(colors.bg.unwrap_or(Color::Black))),
    );

    frame.render_widget(modal, modal_area);
}

fn draw_branch_modal(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    let filtered = app.branch_modal.filtered();
    if app.branch_modal.branches.is_empty() {
        return;
    }

    let has_filter = app.branch_modal.branches.len() > 10;

    let modal_width = (area.width / 2).max(40);
    let modal_height = (area.height / 2).max(10);
    let extra_lines: u16 = if has_filter { 3 } else { 2 }; // borders (+ filter line)
    let visible_rows = modal_height.saturating_sub(extra_lines) as usize;

    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);
    let inner_width = modal_width.saturating_sub(2) as usize; // inside borders

    let bg = colors.bg.unwrap_or(Color::Black);
    let highlight_bg = colors.border_unfocused;

    // Scroll so the selected item is visible
    let scroll_offset = if app.branch_modal.selected >= visible_rows {
        app.branch_modal.selected - visible_rows + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();

    // Filter input line (only when > 10 branches)
    if has_filter {
        lines.push(Line::from(vec![
            Span::styled("/ ", Style::default().fg(colors.fg_accent)),
            Span::styled(&app.branch_modal.filter, Style::default().fg(colors.fg)),
            Span::styled("█", Style::default().fg(colors.fg_muted)),
        ]));
    }

    // Commit info starts just past halfway
    let commit_col = inner_width / 2;

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No matching branches",
            Style::default().fg(colors.fg_muted),
        )));
    } else {
        for (i, branch) in filtered.iter().enumerate().skip(scroll_offset).take(visible_rows) {
            let is_selected = i == app.branch_modal.selected;

            let prefix = if branch.is_head { "* " } else { "  " };

            let name_style = if branch.is_head {
                Style::default().fg(colors.fg_accent)
            } else {
                Style::default().fg(colors.fg)
            };

            // Truncate branch name if it would overflow into the commit column
            let name_budget = commit_col.saturating_sub(2); // prefix
            let name_display: String = if branch.name.len() > name_budget && name_budget > 3 {
                let truncated: String = branch.name.chars().take(name_budget - 3).collect();
                format!("{}...", truncated)
            } else {
                branch.name.clone()
            };
            let name_padding = commit_col.saturating_sub(2 + name_display.len());

            // Truncate message to fit in the remaining space
            let commit_budget = inner_width.saturating_sub(commit_col);
            let oid_and_space = branch.short_oid.len() + 1;
            let msg_budget = commit_budget.saturating_sub(oid_and_space);
            let msg: String = if branch.message.chars().count() > msg_budget && msg_budget > 3 {
                let truncated: String = branch.message.chars().take(msg_budget - 3).collect();
                format!("{}...", truncated)
            } else {
                branch.message.clone()
            };
            let trail = commit_budget.saturating_sub(oid_and_space + msg.len());

            let row_style = if is_selected {
                Style::default().bg(highlight_bg)
            } else {
                Style::default()
            };
            let name_style = if is_selected { name_style.bg(highlight_bg) } else { name_style };
            let oid_style = if is_selected {
                Style::default().fg(colors.fg_accent).bg(highlight_bg)
            } else {
                Style::default().fg(colors.fg_accent)
            };
            let msg_style = if is_selected {
                Style::default().fg(colors.fg).bg(highlight_bg)
            } else {
                Style::default().fg(colors.fg)
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(name_display, name_style),
                Span::styled(" ".repeat(name_padding), row_style),
                Span::styled(&branch.short_oid, oid_style),
                Span::styled(" ", row_style),
                Span::styled(msg, msg_style),
                Span::styled(" ".repeat(trail), row_style),
            ]));
        }
    }

    frame.render_widget(Clear, modal_area);
    let modal = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border_focused))
            .title(" Branches ")
            .style(Style::default().bg(bg)),
    );

    frame.render_widget(modal, modal_area);
}

fn draw_help_modal(frame: &mut Frame, area: Rect, colors: &ColorsConfig) {
    let sections: &[(&str, &[(&str, &str)])] = &[
        ("Global", &[
            ("1 / 2 / 3", "Switch tab (Diff / Commits / PRs)"),
            ("q", "Quit"),
            ("?", "Toggle this help"),
            ("gg / G", "Jump to top / bottom"),
        ]),
        ("Diff — Sidebar", &[
            ("j / k", "Navigate files"),
            ("l / Enter", "Open file / toggle folder"),
            ("Space", "Fold / unfold folder"),
            ("v", "Toggle viewed"),
            ("V", "Mark viewed & next"),
            ("Tab", "Focus diff panel"),
        ]),
        ("Diff — Panel", &[
            ("j / k", "Scroll up / down"),
            ("Ctrl+d / u", "Half-page down / up"),
            ("Ctrl+n / p", "Next / prev hunk"),
            ("] / [", "Next / prev file"),
            ("h", "Focus sidebar"),
        ]),
        ("Diff — Shared", &[
            ("/ → Enter", "Search in file"),
            ("n / N", "Next / prev match"),
            ("s", "Toggle unified / split view"),
            ("Esc", "Clear search or go back"),
        ]),
        ("Local repo", &[
            ("c", "Commit staged changes"),
            ("b", "Switch branch"),
            ("p / P", "Git pull / push"),
        ]),
        ("Commits / PRs", &[
            ("j / k", "Navigate list"),
            ("Ctrl+d / u", "Half-page down / up"),
            ("Enter", "View diff"),
            ("f", "Filter PRs (PRs tab)"),
            ("Esc", "Return to list"),
        ]),
    ];

    let mut content_lines: Vec<Line> = Vec::new();
    for (i, (heading, bindings)) in sections.iter().enumerate() {
        if i > 0 {
            content_lines.push(Line::from(""));
        }
        content_lines.push(Line::from(Span::styled(
            *heading,
            Style::default().fg(colors.fg_accent).add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in *bindings {
            content_lines.push(Line::from(vec![
                Span::styled(format!("  {:18}", key), Style::default().fg(colors.fg_selected)),
                Span::styled(*desc, Style::default().fg(colors.fg)),
            ]));
        }
    }

    let modal_width = 52_u16.min(area.width.saturating_sub(4));
    let modal_height = (content_lines.len() as u16 + 2).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    let bg = colors.bg.unwrap_or(Color::Black);

    frame.render_widget(Clear, modal_area);
    let modal = Paragraph::new(content_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border_focused))
            .title(" Help ")
            .style(Style::default().bg(bg)),
    );

    frame.render_widget(modal, modal_area);
}

fn draw_confirm_modal(frame: &mut Frame, area: Rect, message: &str, colors: &ColorsConfig) {
    let hint = "y: yes  n: no";
    let text_width = message.len().max(hint.len()) as u16 + 6;
    let modal_width = text_width.clamp(24, area.width.saturating_sub(4));
    let modal_height: u16 = 4;

    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    let bg = colors.bg.unwrap_or(Color::Black);

    let lines = vec![
        Line::from(Span::styled(message, Style::default().fg(colors.fg))),
        Line::from(Span::styled(hint, Style::default().fg(colors.fg_muted))),
    ];

    frame.render_widget(Clear, modal_area);
    let widget = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border_focused))
                .style(Style::default().bg(bg)),
        );

    frame.render_widget(widget, modal_area);
}

fn draw_loading_overlay(frame: &mut Frame, area: Rect, message: &str, colors: &ColorsConfig) {
    let text_width = message.len() as u16 + 4; // padding
    let modal_width = text_width.clamp(20, area.width.saturating_sub(4));
    let modal_height: u16 = 3;

    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    let bg = colors.bg.unwrap_or(Color::Black);

    frame.render_widget(Clear, modal_area);
    let widget = Paragraph::new(Line::from(Span::styled(
        message,
        Style::default().fg(colors.fg_muted),
    )))
    .alignment(ratatui::layout::Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border_focused))
            .style(Style::default().bg(bg)),
    );

    frame.render_widget(widget, modal_area);
}

fn draw_commit_modal(frame: &mut Frame, app: &App, area: Rect, colors: &ColorsConfig) {
    use crate::app::CommitField;

    let modal_width = area.width.saturating_sub(4).min(100);
    let summary_height: u16 = 3; // border + 1 line + border
    let desc_height = area.height.saturating_sub(summary_height + 4).clamp(5, 15);
    let modal_height = summary_height + desc_height;

    let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_height)) / 2;

    let summary_area = Rect::new(x, y, modal_width, summary_height);
    let desc_area = Rect::new(x, y + summary_height, modal_width, desc_height);

    let bg = colors.bg.unwrap_or(Color::Black);
    let focused_border = Style::default().fg(colors.border_focused);
    let unfocused_border = Style::default().fg(colors.border_unfocused);

    // Summary field
    let summary_border = if app.commit_focus == CommitField::Summary {
        focused_border
    } else {
        unfocused_border
    };
    let summary_content = if app.commit_focus == CommitField::Summary {
        Line::from(vec![
            Span::styled(&app.commit_summary, Style::default().fg(colors.fg)),
            Span::styled("█", Style::default().fg(colors.fg_muted)),
        ])
    } else {
        Line::from(Span::styled(&app.commit_summary, Style::default().fg(colors.fg)))
    };
    let summary = Paragraph::new(summary_content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(summary_border)
            .title(" Commit summary ")
            .style(Style::default().bg(bg)),
    );

    // Description field
    let desc_border = if app.commit_focus == CommitField::Description {
        focused_border
    } else {
        unfocused_border
    };
    let desc_lines: Vec<Line> = app
        .commit_description
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if app.commit_focus == CommitField::Description && i == app.commit_cursor {
                Line::from(vec![
                    Span::styled(line.as_str(), Style::default().fg(colors.fg)),
                    Span::styled("█", Style::default().fg(colors.fg_muted)),
                ])
            } else {
                Line::from(Span::styled(line.as_str(), Style::default().fg(colors.fg)))
            }
        })
        .collect();
    let desc_title = " Commit description ";
    let desc = Paragraph::new(desc_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(desc_border)
            .title(desc_title)
            .style(Style::default().bg(bg)),
    );

    // Clear the cells behind the modal, then fill with background
    let full_modal_area = Rect::new(x, y, modal_width, modal_height);
    frame.render_widget(Clear, full_modal_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(bg)),
        full_modal_area,
    );

    frame.render_widget(summary, summary_area);
    frame.render_widget(desc, desc_area);
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

    let in_subview = matches!(app.view_context, ViewContext::Commit { .. } | ViewContext::PullRequest { .. });
    let hints = if app.search.is_some() && in_subview {
        "Esc: clear search  ?: help  q: quit"
    } else if in_subview {
        "Esc: back  ?: help  q: quit"
    } else {
        "?: help  q: quit"
    };

    let status_msg_spans: Vec<Span> = if let Some(ref msg) = app.status_message {
        vec![
            Span::raw("  "),
            Span::styled(msg.as_str(), Style::default().fg(colors.fg_accent).add_modifier(ratatui::style::Modifier::BOLD)),
        ]
    } else {
        vec![]
    };

    let mut spans = vec![
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
    ];
    spans.extend(status_msg_spans);
    spans.push(Span::raw("  "));
    spans.push(Span::styled(hints, Style::default().fg(colors.fg_muted)));

    let status = Line::from(spans);

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

    let search_info = app.search.as_ref().map(|s| {
        let current_line = s.matches.get(s.current).map(|m| m.line_index);
        (&s.match_lines, current_line)
    });

    let syntax = Path::new(&file.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let vis_start = app.scroll;
    // Build extra lines to compensate for wrapping pushing content down
    let vis_end = vis_start + inner_height * 2;
    let inner_width = left_area.width.saturating_sub(2) as usize;
    let wrap_indent_style = Style::default().fg(colors.fg_muted);

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

        let search_bg = search_info.as_ref().and_then(|(all, current)| {
            if all.contains(&row_idx) {
                let is_current = *current == Some(row_idx);
                Some(if is_current { colors.bg_search_current } else { colors.bg_search_match })
            } else {
                None
            }
        });

        let gutter_style = Style::default().fg(colors.fg_muted);

        match row {
            SplitRow::HunkHeader(header) => {
                let text = header.trim_end().to_string();
                let mut left_spans = vec![
                    Span::styled("     ", gutter_style),
                    Span::styled(text.clone(), Style::default().fg(colors.fg_muted)),
                ];
                let mut right_spans = vec![
                    Span::styled("     ", gutter_style),
                    Span::styled(text, Style::default().fg(colors.fg_muted)),
                ];
                if let Some(bg) = search_bg {
                    left_spans = highlight_search_in_spans(left_spans, app, bg);
                    right_spans = highlight_search_in_spans(right_spans, app, bg);
                }
                let wrapped_l = wrap_line_spans(left_spans, inner_width, 6, wrap_indent_style);
                let wrapped_r = wrap_line_spans(right_spans, inner_width, 6, wrap_indent_style);
                let max_rows = wrapped_l.len().max(wrapped_r.len());
                left_lines.extend(wrapped_l);
                right_lines.extend(wrapped_r);
                for _ in left_lines.len()..right_lines.len() { left_lines.push(Line::from(vec![])); }
                for _ in right_lines.len()..left_lines.len() { right_lines.push(Line::from(vec![])); }
                let _ = max_rows;
            }
            SplitRow::Context(line) => {
                let content = line.content.trim_end();
                let l_gutter = format_split_gutter(line.old_num);
                let r_gutter = format_split_gutter(line.new_num);
                let mut l_spans = build_syntax_spans(content, &mut left_hl, ss, None, " ", colors);
                let mut r_spans = build_syntax_spans(content, &mut right_hl, ss, None, " ", colors);
                l_spans.insert(0, Span::styled(l_gutter, gutter_style));
                r_spans.insert(0, Span::styled(r_gutter, gutter_style));
                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                let wrapped_l = wrap_line_spans(l_spans, inner_width, 6, wrap_indent_style);
                let wrapped_r = wrap_line_spans(r_spans, inner_width, 6, wrap_indent_style);
                left_lines.extend(wrapped_l);
                right_lines.extend(wrapped_r);
                for _ in left_lines.len()..right_lines.len() { left_lines.push(Line::from(vec![])); }
                for _ in right_lines.len()..left_lines.len() { right_lines.push(Line::from(vec![])); }
            }
            SplitRow::Paired { left, right, left_spans: l_inline, right_spans: r_inline } => {
                let l_content = left.content.trim_end();
                let r_content = right.content.trim_end();
                let l_gutter = format_split_gutter(left.old_num);
                let r_gutter = format_split_gutter(right.new_num);

                let mut l_spans = build_syntax_spans(l_content, &mut left_hl, ss, Some(colors.bg_removed), "-", colors);
                let mut r_spans = build_syntax_spans(r_content, &mut right_hl, ss, Some(colors.bg_added), "+", colors);

                l_spans = apply_inline_highlight(l_spans, l_inline, colors.bg_inline_removed);
                r_spans = apply_inline_highlight(r_spans, r_inline, colors.bg_inline_added);

                l_spans.insert(0, Span::styled(l_gutter, gutter_style));
                r_spans.insert(0, Span::styled(r_gutter, gutter_style));

                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                let wrapped_l = wrap_line_spans(l_spans, inner_width, 6, wrap_indent_style);
                let wrapped_r = wrap_line_spans(r_spans, inner_width, 6, wrap_indent_style);
                left_lines.extend(wrapped_l);
                right_lines.extend(wrapped_r);
                for _ in left_lines.len()..right_lines.len() { left_lines.push(Line::from(vec![])); }
                for _ in right_lines.len()..left_lines.len() { right_lines.push(Line::from(vec![])); }
            }
            SplitRow::LeftOnly(line) => {
                let content = line.content.trim_end();
                let l_gutter = format_split_gutter(line.old_num);
                let mut l_spans = build_syntax_spans(content, &mut left_hl, ss, Some(colors.bg_removed), "-", colors);
                l_spans.insert(0, Span::styled(l_gutter, gutter_style));
                if let Some(bg) = search_bg {
                    l_spans = highlight_search_in_spans(l_spans, app, bg);
                }
                let wrapped = wrap_line_spans(l_spans, inner_width, 6, wrap_indent_style);
                let n = wrapped.len();
                left_lines.extend(wrapped);
                for _ in 0..n {
                    right_lines.push(Line::from(vec![]).style(Style::default().bg(colors.bg_split_empty)));
                }
            }
            SplitRow::RightOnly(line) => {
                let content = line.content.trim_end();
                let r_gutter = format_split_gutter(line.new_num);
                let mut r_spans = build_syntax_spans(content, &mut right_hl, ss, Some(colors.bg_added), "+", colors);
                r_spans.insert(0, Span::styled(r_gutter, gutter_style));
                if let Some(bg) = search_bg {
                    r_spans = highlight_search_in_spans(r_spans, app, bg);
                }
                let wrapped = wrap_line_spans(r_spans, inner_width, 6, wrap_indent_style);
                let n = wrapped.len();
                right_lines.extend(wrapped);
                for _ in 0..n {
                    left_lines.push(Line::from(vec![]).style(Style::default().bg(colors.bg_split_empty)));
                }
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

    let old_path = match &file.status {
        model::FileStatus::Renamed { from } => from.as_str(),
        _ => &file.path,
    };
    let left_title = format!(" old: {} ", old_path);
    let right_title = format!(" new: {}{} ", file.path, viewed_indicator);
    let left_widget = Paragraph::new(left_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(left_title, Style::default().add_modifier(Modifier::BOLD))),
    );

    let right_widget = Paragraph::new(right_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(right_title, Style::default().add_modifier(Modifier::BOLD))),
    );

    frame.render_widget(left_widget, left_area);
    frame.render_widget(right_widget, right_area);

    render_scrollbar(frame, right_area, app.scroll, rows.len(), inner_height, colors);
}

/// Format a single line number for the split-view gutter (4 chars + 1 space).
fn format_split_gutter(num: Option<usize>) -> String {
    match num {
        Some(n) => format!("{:>4} ", n),
        None => "     ".to_string(),
    }
}

/// Render a vertical scrollbar on the right edge of an area.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    position: usize,
    content_length: usize,
    viewport_height: usize,
    colors: &ColorsConfig,
) {
    if content_length <= viewport_height {
        return;
    }
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .track_symbol(Some("│"))
        .thumb_symbol("█")
        .begin_symbol(None)
        .end_symbol(None)
        .track_style(Style::default().fg(colors.border_unfocused))
        .thumb_style(Style::default().fg(colors.fg_muted));
    let mut state = ScrollbarState::new(content_length.saturating_sub(viewport_height))
        .position(position)
        .viewport_content_length(viewport_height);
    frame.render_stateful_widget(scrollbar, area, &mut state);
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
