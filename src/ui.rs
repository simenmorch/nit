use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Focus};
use crate::model;

const SIDEBAR_WIDTH: u16 = 40;

pub fn draw(frame: &mut Frame, app: &App, diff: &model::Diff, branch: &str) {
    let area = frame.area();

    if diff.files.is_empty() {
        let message = Paragraph::new("No uncommitted changes.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" nit "));
        frame.render_widget(message, area);
        return;
    }

    // Vertical split: main content + status bar
    let outer = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ]).split(area);

    // Horizontal split: sidebar + diff (or just diff if sidebar hidden)
    if app.show_sidebar {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SIDEBAR_WIDTH),
                Constraint::Min(1),
            ])
            .split(outer[0]);

        draw_sidebar(frame, app, diff, panels[0]);
        draw_file_diff(frame, app, diff, panels[1]);
    } else {
        draw_file_diff(frame, app, diff, outer[0]);
    }

    draw_status_bar(frame, app, diff, branch, outer[1]);
}

fn draw_sidebar(frame: &mut Frame, app: &App, diff: &model::Diff, area: Rect) {
    let is_focused = matches!(app.focus, Focus::Sidebar);

    let lines: Vec<Line> = diff
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let is_selected = i == app.selected;
            let marker = if is_selected { "▸ " } else { "  " };
            let viewed = if file.viewed { "✓ " } else { "  " };
            let stats = format!("+{} -{}", file.added, file.removed);
            let path_style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled(viewed, Style::default().fg(Color::Green)),
                Span::raw(marker),
                Span::styled(&file.path, path_style),
                Span::raw("  "),
                Span::styled(stats, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let file_list = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Files "),
    );

    frame.render_widget(file_list, area);
}

fn draw_file_diff(frame: &mut Frame, app: &App, diff: &model::Diff, area: Rect) {
    let file = &diff.files[app.selected];
    let is_focused = matches!(app.focus, Focus::Diff);

    let mut lines: Vec<Line> = Vec::new();

    for hunk in &file.hunks {
        lines.push(Line::from(Span::styled(
            hunk.header.trim_end().to_string(),
            Style::default().fg(Color::DarkGray),
        )));

        for line in &hunk.lines {
            let (prefix, style) = match line.kind {
                model::LineKind::Added => ("+", Style::default().fg(Color::Green)),
                model::LineKind::Removed => ("-", Style::default().fg(Color::Red)),
                model::LineKind::Context => (" ", Style::default().fg(Color::White)),
            };

            let content = format!("{}{}", prefix, line.content.trim_end());
            lines.push(Line::from(Span::styled(content, style)));
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

    let diff_view = Paragraph::new(lines)
        .scroll((app.scroll as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
        );

    frame.render_widget(diff_view, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, diff: &model::Diff, branch: &str, area: Rect) {
    let viewed_count = diff.files.iter().filter(|f| f.viewed).count();
    let total_added: usize = diff.files.iter().map(|f| f.added).sum();
    let total_removed: usize = diff.files.iter().map(|f| f.removed).sum();

    let hints = match app.focus {
        Focus::Sidebar => "j/k: navigate  l/Enter: diff  Tab: toggle sidebar  v: viewed  q: quit",
        Focus::Diff => "j/k: scroll  h/Esc: sidebar  Tab: toggle sidebar  v: viewed  q: quit",
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
