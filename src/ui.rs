use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, InputMode, LibraryItem, SearchScope};
use crate::kodi::format_duration;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Now playing bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Search / status bar
        ])
        .split(area);

    draw_now_playing(f, app, chunks[0]);
    draw_main(f, app, chunks[1]);
    draw_search_bar(f, app, chunks[2]);
}

fn draw_now_playing(f: &mut Frame, app: &App, area: Rect) {
    let status = &app.status;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Now Playing ", Style::default().fg(Color::Cyan)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(item) = &status.current_item {
        let play_icon = if status.playing { "▶" } else { "⏸" };
        let vol_icon = if status.muted { "🔇" } else { "🔊" };

        let title_line = Line::from(vec![
            Span::raw(format!("{play_icon} ")),
            Span::styled(&item.title, Style::default().add_modifier(Modifier::BOLD).fg(Color::White)),
            Span::raw("  "),
            Span::styled(&item.artist, Style::default().fg(Color::Yellow)),
            Span::raw("  —  "),
            Span::styled(&item.album, Style::default().fg(Color::DarkGray)),
        ]);

        let pos = status.position;
        let dur = status.duration;
        let ratio = if dur > 0 { pos as f64 / dur as f64 } else { 0.0 };
        let time_str = format!(
            "{} / {}   {}{:3}%",
            format_duration(pos),
            format_duration(dur),
            vol_icon,
            status.volume,
        );

        let info_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);

        f.render_widget(Paragraph::new(title_line), info_chunks[0]);

        // Progress bar row
        let bar_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(22)])
            .split(info_chunks[1]);

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .ratio(ratio.clamp(0.0, 1.0));
        f.render_widget(gauge, bar_chunks[0]);
        f.render_widget(
            Paragraph::new(time_str).alignment(Alignment::Right),
            bar_chunks[1],
        );
    } else {
        let msg = if app.loading {
            "Loading library…"
        } else {
            "Nothing playing. Press / to search, Enter to play."
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    }
}

fn draw_main(f: &mut Frame, app: &App, area: Rect) {
    let scope_labels = [
        (SearchScope::All, "All [F1]"),
        (SearchScope::Artists, "Artists [F2]"),
        (SearchScope::Albums, "Albums [F3]"),
        (SearchScope::Songs, "Songs [F4]"),
    ];
    let title_spans: Vec<Span> = scope_labels
        .iter()
        .flat_map(|(scope, label)| {
            let active = *scope == app.search_scope;
            let style = if active {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            vec![Span::styled(format!(" {label} "), style), Span::raw(" ")]
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(title_spans));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_rows = inner.height as usize;

    if app.filtered_items.is_empty() {
        let msg = if app.loading {
            "Loading…"
        } else if app.search_query.is_empty() {
            "No items found in library."
        } else {
            "No matches."
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let end = (app.list_offset + visible_rows).min(app.filtered_items.len());
    let visible = &app.filtered_items[app.list_offset..end];

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let global_idx = i + app.list_offset;
            let selected = global_idx == app.selected;

            let type_tag = match item {
                LibraryItem::Artist(_) => Span::styled(
                    " ART ",
                    Style::default().fg(Color::Black).bg(Color::Magenta),
                ),
                LibraryItem::Album(_) => Span::styled(
                    " ALB ",
                    Style::default().fg(Color::Black).bg(Color::Blue),
                ),
                LibraryItem::Song(_) => Span::styled(
                    " SNG ",
                    Style::default().fg(Color::Black).bg(Color::Green),
                ),
            };

            let label = Span::styled(
                format!(" {} ", item.display_label()),
                if selected {
                    Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            );

            let sub = item.subtitle();
            let sub_span = Span::styled(
                format!(" {sub}"),
                if selected {
                    Style::default().fg(Color::DarkGray).bg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            );

            // Duration for songs
            let dur_span = if let LibraryItem::Song(s) = item {
                if let Some(d) = s.duration {
                    Span::styled(
                        format!(" {} ", format_duration(d)),
                        if selected {
                            Style::default().fg(Color::DarkGray).bg(Color::White)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    )
                } else {
                    Span::raw("")
                }
            } else {
                Span::raw("")
            };

            ListItem::new(Line::from(vec![type_tag, label, sub_span, dur_span]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let (title, content, hint) = match &app.input_mode {
        InputMode::Search => {
            let scope = match app.search_scope {
                SearchScope::All => "all",
                SearchScope::Artists => "artists",
                SearchScope::Albums => "albums",
                SearchScope::Songs => "songs",
            };
            (
                format!(" Search ({scope}) "),
                format!("{}_", app.search_query),
                " ESC cancel  Enter play  F1-F4 scope ".to_string(),
            )
        }
        InputMode::Normal => {
            let msg = app
                .status_message
                .clone()
                .unwrap_or_else(|| format!("{} items", app.filtered_items.len()));
            (
                " koditerm ".to_string(),
                msg,
                " /search  j/k nav  gg/G top/bot  Enter play  Space pause  n/p next/prev  +/- vol  q quit ".to_string(),
            )
        }
        InputMode::Command => (
            " Command ".to_string(),
            app.search_query.clone(),
            String::new(),
        ),
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(hint.len() as u16 + 2)])
        .split(area);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, Style::default().fg(Color::Cyan)));

    let input_style = if matches!(app.input_mode, InputMode::Search) {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    f.render_widget(
        Paragraph::new(content).block(input_block).style(input_style),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL)),
        chunks[1],
    );
}
