use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, InputMode, LibraryItem, PlaybackBackend, RepeatMode, SearchMode, SearchScope};
use crate::kodi::format_duration;

pub fn draw(f: &mut Frame, app: &mut App) {
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

    if app.show_help {
        draw_help(f, area);
    }
}

fn draw_now_playing(f: &mut Frame, app: &App, area: Rect) {
    let status = &app.status;

    let backend_span = if app.backend == PlaybackBackend::Local {
        Span::styled(" LOCAL ", Style::default().fg(Color::Black).bg(Color::Yellow))
    } else {
        Span::styled(" REMOTE ", Style::default().fg(Color::DarkGray))
    };
    let repeat_style = Style::default().fg(Color::Gray);
    let repeat_span = if app.backend == PlaybackBackend::Local {
        match app.repeat_mode {
            RepeatMode::Off => Span::raw(""),
            RepeatMode::Track => Span::styled(" ↻1", repeat_style),
            RepeatMode::Queue => Span::styled(" ↻", repeat_style),
        }
    } else {
        match app.status.repeat.as_str() {
            "one" => Span::styled(" ↻1", repeat_style),
            "all" => Span::styled(" ↻", repeat_style),
            _ => Span::raw(""),
        }
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(" Now Playing ", Style::default().fg(Color::Cyan)),
            backend_span,
            repeat_span,
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.backend == PlaybackBackend::Local {
        if let Some(song) = &app.local_current_song {
            let play_icon = if app.local_paused { "⏸" } else { "▶" };
            let vol_icon = if app.local_volume == 0 { "🔇" } else { "🔊" };
            let title_line = Line::from(vec![
                Span::raw(format!("{play_icon} ")),
                Span::styled(
                    song.label.clone(),
                    Style::default().add_modifier(Modifier::BOLD).fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled(song.artist.join(", "), Style::default().fg(Color::Yellow)),
                Span::raw("  —  "),
                Span::styled(song.album.clone(), Style::default().fg(Color::DarkGray)),
            ]);

            let pos = app.local_position;
            let dur = song.duration.unwrap_or(0);
            let ratio = if dur > 0 { (pos as f64 / dur as f64).clamp(0.0, 1.0) } else { 0.0 };
            let time_str = format!(
                "{} / {}   {vol_icon}{:3}%",
                format_duration(pos),
                format_duration(dur),
                app.local_volume,
            );

            let info_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1)])
                .split(inner);

            f.render_widget(Paragraph::new(title_line), info_chunks[0]);

            let bar_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(22)])
                .split(info_chunks[1]);

            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                .ratio(ratio);
            f.render_widget(gauge, bar_chunks[0]);
            f.render_widget(
                Paragraph::new(time_str).alignment(Alignment::Right),
                bar_chunks[1],
            );
        } else {
            f.render_widget(
                Paragraph::new("Local mode. Press Enter to play, 'a' to queue.")
                    .style(Style::default().fg(Color::DarkGray)),
                inner,
            );
        }
        return;
    }

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

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(34)])
        .split(area);

    draw_library(f, app, panes[0]);
    draw_queue(f, app, panes[1]);
}

fn draw_library(f: &mut Frame, app: &mut App, area: Rect) {
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
    app.visible_rows = visible_rows;

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
            let dist = (global_idx as i64 - app.selected as i64).unsigned_abs() as usize;

            let num_style = if selected {
                Style::default().fg(Color::DarkGray).bg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let num_span = Span::styled(format!("{:>3} ", dist), num_style);

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

            ListItem::new(Line::from(vec![num_span, type_tag, label, sub_span, dur_span]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn draw_queue(f: &mut Frame, app: &App, area: Rect) {
    let (queue, current_pos, has_current) = if app.backend == PlaybackBackend::Local {
        (&app.local_queue, app.local_queue_pos, app.local_current_song.is_some())
    } else {
        (&app.remote_queue, app.status.playlist_pos, app.status.player_id.is_some())
    };

    let title = if queue.is_empty() {
        Span::styled(" Queue ", Style::default().fg(Color::DarkGray))
    } else {
        let pos = if has_current { current_pos + 1 } else { 0 };
        Span::styled(
            format!(" Queue {}/{} ", pos, queue.len()),
            Style::default().fg(Color::Yellow),
        )
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if queue.is_empty() {
        f.render_widget(
            Paragraph::new("Empty\n\nAdd songs\nwith 'a'")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let visible = inner.height as usize;
    let offset = current_pos
        .saturating_sub(visible / 2)
        .min(queue.len().saturating_sub(visible));

    // prefix: "  3 " (4 chars) + "▶ " or "  " (2 chars) = 6 chars total
    let max_label = inner.width.saturating_sub(6) as usize;
    let num_style = Style::default().fg(Color::DarkGray);

    let items: Vec<ListItem> = queue
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, song)| {
            let playing = has_current && i == current_pos;
            let dist = (i as i64 - current_pos as i64).unsigned_abs() as usize;
            let num_str = if has_current {
                format!("{:>3} ", dist)
            } else {
                format!("{:>3} ", i + 1)
            };
            let (indicator, song_style) = if playing {
                ("▶ ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                ("  ", Style::default().fg(Color::White))
            };
            let label = if song.label.len() > max_label {
                format!("{}…", &song.label[..max_label.saturating_sub(1)])
            } else {
                song.label.clone()
            };
            ListItem::new(Line::from(vec![
                Span::styled(num_str, num_style),
                Span::styled(indicator, song_style),
                Span::styled(label, song_style),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
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
            let mode = match app.search_mode {
                SearchMode::Exact => "exact",
                SearchMode::Fuzzy => "fuzzy",
            };
            (
                format!(" Search ({scope}, {mode}) "),
                format!("{}_", app.search_query),
                " ESC cancel  Enter play  Tab toggle fuzzy/exact  F1-F4 scope ".to_string(),
            )
        }
        InputMode::Normal => {
            let base = app
                .status_message
                .clone()
                .unwrap_or_else(|| format!("{} items", app.filtered_items.len()));
            let msg = if app.pending_count.is_empty() {
                base
            } else {
                format!("[{}] {}", app.pending_count, base)
            };
            (
                " koditerm ".to_string(),
                msg,
                " /search  j/k nav  gg/G top/bot  Enter play  Space pause  n/p skip  r repeat  +/- vol  q quit ".to_string(),
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

fn draw_help(f: &mut Frame, area: Rect) {
    type Section = (&'static str, &'static [(&'static str, &'static str)]);

    const LEFT: &[Section] = &[
        (
            "Navigation",
            &[
                ("j / ↓",    "Move down"),
                ("k / ↑",    "Move up"),
                ("Ctrl+d",   "Half page down"),
                ("Ctrl+u",   "Half page up"),
                ("PgDn",     "Page down"),
                ("PgUp",     "Page up"),
                ("gg",       "Jump to top"),
                ("G",        "Jump to bottom"),
            ],
        ),
        (
            "Search & Scope",
            &[
                ("/",        "New search"),
                ("Tab",      "Toggle fuzzy/exact"),
                ("Esc",      "Cancel search"),
                ("F1",       "Scope: All"),
                ("F2",       "Scope: Artists"),
                ("F3",       "Scope: Albums"),
                ("F4",       "Scope: Songs"),
                ("Ctrl+n",   "Next scope"),
                ("Ctrl+p",   "Previous scope"),
            ],
        ),
    ];

    const RIGHT: &[Section] = &[
        (
            "Playback",
            &[
                ("Enter",    "Play (clears queue)"),
                ("a",        "Add to queue"),
                ("F5",       "Queue (from search)"),
                ("Space",    "Pause / resume"),
                ("s",        "Stop"),
                ("n",        "Next track"),
                ("p",        "Previous track"),
                ("r",        "Cycle repeat: off / ↻1 track / ↻ queue"),
                ("+  /  =",  "Volume up 5%"),
                ("-",        "Volume down 5%"),
                ("Alt+=",    "Volume up 1%"),
                ("Alt+-",    "Volume down 1%"),
            ],
        ),
        (
            "General",
            &[
                ("L",        "Toggle local/remote"),
                ("?",        "Toggle this help"),
                ("q",        "Quit"),
            ],
        ),
    ];

    let key_style  = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let val_style  = Style::default().fg(Color::White);
    let head_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    let build_lines = |sections: &[Section]| -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = vec![Line::from("")];
        for (heading, bindings) in sections {
            lines.push(Line::from(Span::styled(format!("  {heading}"), head_style)));
            for (key, desc) in *bindings {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {key:<10}", key = key), key_style),
                    Span::styled(format!(" {desc}"), val_style),
                ]));
            }
            lines.push(Line::from(""));
        }
        lines
    };

    let left_lines  = build_lines(LEFT);
    let right_lines = build_lines(RIGHT);

    // col_width = 2 indent + 10 key + 1 space + 19 desc + 1 padding = 33
    let col_w: u16 = 33;
    let popup_w = col_w * 2 + 3; // 3 = left border + divider + right border
    let popup_h = left_lines.len().max(right_lines.len()) as u16 + 2;

    let popup_area = center_rect(popup_w, popup_h, area);
    f.render_widget(Clear, popup_area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" Help  (? to close) ", Style::default().fg(Color::Cyan))),
        popup_area,
    );

    let inner = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(col_w), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    f.render_widget(Paragraph::new(left_lines), cols[0]);
    // Vertical divider
    f.render_widget(
        Block::default().borders(Borders::LEFT),
        Rect { x: cols[1].x, y: cols[1].y, width: 1, height: cols[1].height },
    );
    f.render_widget(Paragraph::new(right_lines), cols[2]);
}

fn center_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
