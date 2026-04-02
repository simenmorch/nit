use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::parsing::SyntaxSet;

use crate::app::{App, Focus};
use crate::model;
use crate::tree::{FlatEntry, FlatEntryKind};

const SIDEBAR_WIDTH: u16 = 40;
const SEARCH_HIGHLIGHT_BG: Color = Color::Rgb(120, 100, 30);
const SEARCH_CURRENT_BG: Color = Color::Rgb(180, 140, 20);

pub fn draw(
    frame: &mut Frame,
    app: &App,
    diff: &model::Diff,
    visible: &[FlatEntry],
    branch: &str,
    ss: &SyntaxSet,
    theme: &Theme,
) {
    let area = frame.area();

    if diff.files.is_empty() {
        let message = Paragraph::new("No uncommitted changes.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" nit "));
        frame.render_widget(message, area);
        return;
    }

    let outer = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ]).split(area);

    if app.show_sidebar {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SIDEBAR_WIDTH),
                Constraint::Min(1),
            ])
            .split(outer[0]);

        draw_sidebar(frame, app, diff, visible, panels[0]);
        draw_file_diff(frame, app, diff, panels[1], ss, theme);
    } else {
        draw_file_diff(frame, app, diff, outer[0], ss, theme);
    }

    if app.searching {
        draw_search_input(frame, app, outer[1]);
    } else {
        draw_status_bar(frame, app, diff, branch, outer[1]);
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
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Yellow)
                    };
                    Line::from(vec![
                        Span::styled(viewed, Style::default().fg(Color::Green)),
                        Span::raw(marker),
                        Span::raw(indent),
                        Span::styled(arrow, Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{}/", name), name_style),
                        Span::raw("  "),
                        Span::styled(stats, Style::default().fg(Color::DarkGray)),
                    ])
                }
                FlatEntryKind::File { file_index, name } => {
                    let file = &diff.files[*file_index];
                    let viewed = if file.viewed { "✓ " } else { "  " };
                    let stats = format!("+{} -{}", file.added, file.removed);
                    let name_style = if is_selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    Line::from(vec![
                        Span::styled(viewed, Style::default().fg(Color::Green)),
                        Span::raw(marker),
                        Span::raw(indent),
                        Span::raw("  "), // align with folder names (arrow placeholder)
                        Span::styled(name.clone(), name_style),
                        Span::raw("  "),
                        Span::styled(stats, Style::default().fg(Color::DarkGray)),
                    ])
                }
            }
        })
        .collect();

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
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
) {
    let file = &diff.files[app.selected_file];
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
                Style::default().fg(Color::DarkGray),
            )];

            if let Some((ref all, current)) = match_lines {
                if all.contains(&line_index) {
                    let is_current = current == Some(line_index);
                    let bg = if is_current { SEARCH_CURRENT_BG } else { SEARCH_HIGHLIGHT_BG };
                    hunk_header_spans = highlight_search_in_spans(hunk_header_spans, app, bg);
                }
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
                    model::LineKind::Added => Some(Color::Rgb(30, 60, 30)),
                    model::LineKind::Removed => Some(Color::Rgb(60, 30, 30)),
                    model::LineKind::Context => None,
                };

                let prefix = match line.kind {
                    model::LineKind::Added => "+",
                    model::LineKind::Removed => "-",
                    model::LineKind::Context => " ",
                };

                let prefix_style = match line.kind {
                    model::LineKind::Added => Style::default().fg(Color::Green),
                    model::LineKind::Removed => Style::default().fg(Color::Red),
                    model::LineKind::Context => Style::default().fg(Color::White),
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
                if let Some((ref all, current)) = match_lines {
                    if all.contains(&line_index) {
                        let is_current = current == Some(line_index);
                        let bg = if is_current { SEARCH_CURRENT_BG } else { SEARCH_HIGHLIGHT_BG };
                        spans = highlight_search_in_spans(spans, app, bg);
                    }
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
            Style::default().fg(Color::DarkGray),
        )));
    }

    let viewed_indicator = if file.viewed { " ✓" } else { "" };
    let title = format!(" {}{} ", file.path, viewed_indicator);

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
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
        let text_lower = text.to_lowercase();

        let mut last = 0;
        for (start, _) in text_lower.match_indices(&query_lower) {
            let end = start + query.len();
            if start > last {
                result.push(Span::styled(text[last..start].to_string(), style));
            }
            result.push(Span::styled(
                text[start..end].to_string(),
                style.bg(bg).add_modifier(Modifier::BOLD),
            ));
            last = end;
        }
        if last < text.len() {
            result.push(Span::styled(text[last..].to_string(), style));
        } else if last == 0 {
            result.push(Span::styled(text, style));
        }
    }

    result
}

fn draw_search_input(frame: &mut Frame, app: &App, area: Rect) {
    let input = Line::from(vec![
        Span::styled("/", Style::default().fg(Color::Yellow)),
        Span::styled(&app.search_input, Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(input), area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, diff: &model::Diff, branch: &str, area: Rect) {
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

    let hints = match app.focus {
        Focus::Sidebar => "j/k: navigate  l/Enter: open  Space: toggle folder  /: search  Tab: sidebar  v: viewed  q: quit",
        Focus::Diff => "j/k: scroll  h: sidebar  /: search  n/N: next/prev  Tab: sidebar  q: quit",
    };

    let status = Line::from(vec![
        Span::raw(" "),
        Span::styled(branch, Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled(
            format!("{}/{} viewed", viewed_count, diff.files.len()),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(format!("+{}", total_added), Style::default().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(format!("-{}", total_removed), Style::default().fg(Color::Red)),
        Span::styled(search_info, Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(status), area);
}

/// Count the total number of rendered lines for a file's diff.
pub fn diff_line_count(file: &model::DiffFile) -> usize {
    file.hunks
        .iter()
        .map(|h| 1 + h.lines.len())
        .sum()
}
