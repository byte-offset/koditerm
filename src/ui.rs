use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, InputMode, LibraryItem, PlaybackBackend, RepeatMode, SearchMode, SearchScope};
use crate::config::Theme;
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
        draw_help(f, &app.theme, area);
    }
    if app.show_track_info {
        draw_track_info(f, app, area);
    }
}

fn draw_now_playing(f: &mut Frame, app: &App, area: Rect) {
    let status = &app.status;

    let t = &app.theme;
    let backend_span = if app.backend == PlaybackBackend::Local {
        Span::styled(" LOCAL ", Style::default().fg(Color::Black).bg(Color::Yellow))
    } else {
        Span::styled(" REMOTE ", Style::default().fg(t.dim))
    };
    let repeat_style = Style::default().fg(t.dim);
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
            Span::styled(" Now Playing ", Style::default().fg(t.accent)),
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
                Span::styled(song.artist.join(", "), Style::default().fg(t.highlight)),
                Span::raw("  —  "),
                Span::styled(song.album.clone(), Style::default().fg(t.dim)),
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
                .gauge_style(Style::default().fg(t.highlight).bg(t.dim))
                .ratio(ratio);
            f.render_widget(gauge, bar_chunks[0]);
            f.render_widget(
                Paragraph::new(time_str).alignment(Alignment::Right),
                bar_chunks[1],
            );
        } else {
            f.render_widget(
                Paragraph::new("Local mode. ENTER to play, Alt+ENTER to queue.")
                    .style(Style::default().fg(t.dim)),
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
            Span::styled(item.artist.join(", "), Style::default().fg(t.highlight)),
            Span::raw("  —  "),
            Span::styled(&item.album, Style::default().fg(t.dim)),
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
            .gauge_style(Style::default().fg(t.accent).bg(t.dim))
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
            Paragraph::new(msg).style(Style::default().fg(t.dim)),
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
    let t = &app.theme;
    let scope_labels: [(SearchScope, String); 4] = [
        (SearchScope::All,     format!("All [F1]")),
        (SearchScope::Artists, format!("{} Artists [F2]", t.tag_artist_label)),
        (SearchScope::Albums,  format!("{} Albums [F3]",  t.tag_album_label)),
        (SearchScope::Songs,   format!("{} Songs [F4]",   t.tag_song_label)),
    ];
    let title_spans: Vec<Span> = scope_labels
        .iter()
        .flat_map(|(scope, label)| {
            let active = *scope == app.search_scope;
            let style = if active {
                Style::default().fg(Color::Black).bg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.dim)
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
            Paragraph::new(msg).style(Style::default().fg(t.dim)),
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
                Style::default().fg(t.dim).bg(t.selected_bg)
            } else {
                Style::default().fg(t.dim)
            };
            let num_span = Span::styled(format!("{:>3} ", dist), num_style);

            let tag_style = |bg: Option<Color>| match bg {
                Some(bg) => Style::default().fg(Color::Black).bg(bg),
                None if selected => Style::default().fg(t.selected_fg).bg(t.selected_bg),
                None => Style::default(),
            };
            let type_tag = match item {
                LibraryItem::Artist(_) => Span::styled(t.tag_artist_label.clone(), tag_style(t.tag_artist)),
                LibraryItem::Album(_)  => Span::styled(t.tag_album_label.clone(),  tag_style(t.tag_album)),
                LibraryItem::Song(_)   => Span::styled(t.tag_song_label.clone(),   tag_style(t.tag_song)),
            };

            let label = Span::styled(
                format!(" {} ", item.display_label()),
                if selected {
                    Style::default().fg(t.selected_fg).bg(t.selected_bg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text)
                },
            );

            let sub = item.subtitle();
            let sub_span = Span::styled(
                format!(" {sub}"),
                if selected {
                    Style::default().fg(t.dim).bg(t.selected_bg)
                } else {
                    Style::default().fg(t.dim)
                },
            );

            // Duration for songs
            let dur_span = if let LibraryItem::Song(s) = item {
                if let Some(d) = s.duration {
                    Span::styled(
                        format!(" {} ", format_duration(d)),
                        if selected {
                            Style::default().fg(t.dim).bg(t.selected_bg)
                        } else {
                            Style::default().fg(t.dim)
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

    let t = &app.theme;
    let title = if queue.is_empty() {
        Span::styled(" Queue ", Style::default().fg(t.dim))
    } else {
        let pos = if has_current { current_pos + 1 } else { 0 };
        Span::styled(
            format!(" Queue {}/{} ", pos, queue.len()),
            Style::default().fg(t.highlight),
        )
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if queue.is_empty() {
        f.render_widget(
            Paragraph::new("Empty\n\nAdd songs\nwith Alt+Enter")
                .style(Style::default().fg(t.dim))
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
    let num_style = Style::default().fg(t.dim);

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
                ("▶ ", Style::default().fg(t.accent).add_modifier(Modifier::BOLD))
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
                " ESC: cancel  ENTER: play  Alt+ENTER: queue  TAB: fuzzy/exact  F1-F4: scope ".to_string(),
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
                " /: search  j/k: nav  gg/G: top/bot  ENTER: play  Alt+ENTER: queue  SPACE: pause  n/p: skip  r: repeat  i: info  +/-: vol  q: quit ".to_string(),
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

    let t = &app.theme;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, Style::default().fg(t.accent)));

    let input_style = if matches!(app.input_mode, InputMode::Search) {
        Style::default().fg(t.highlight)
    } else {
        Style::default().fg(Color::White)
    };

    f.render_widget(
        Paragraph::new(content).block(input_block).style(input_style),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(t.dim))
            .block(Block::default().borders(Borders::ALL)),
        chunks[1],
    );
}

fn draw_help(f: &mut Frame, theme: &Theme, area: Rect) {
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
                ("ENTER",      "Play (clears queue)"),
                ("Alt+ENTER",  "Add to queue"),
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
                ("i",        "Track info popup"),
                ("?",        "Toggle this help"),
                ("q",        "Quit"),
            ],
        ),
    ];

    let key_style  = Style::default().fg(theme.accent).add_modifier(Modifier::BOLD);
    let val_style  = Style::default().fg(Color::White);
    let head_style = Style::default().fg(theme.highlight).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

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
            .title(Span::styled(" Help  (? to close) ", Style::default().fg(theme.accent))),
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

fn draw_track_info(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let label_style = Style::default().fg(t.dim);
    let value_style = Style::default().fg(Color::White);
    let title_style = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = vec![Line::from("")];

    match app.backend {
        PlaybackBackend::Local => {
            if let Some(song) = &app.local_current_song {
                lines.push(Line::from(Span::styled(song.label.clone(), title_style)));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Artist       ", label_style),
                    Span::styled(song.artist.join(", "), value_style),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Album        ", label_style),
                    Span::styled(song.album.clone(), value_style),
                ]));
                if let Some(t) = song.track {
                    lines.push(Line::from(vec![
                        Span::styled("Track        ", label_style),
                        Span::styled(t.to_string(), value_style),
                    ]));
                }
                if let Some(d) = song.duration {
                    lines.push(Line::from(vec![
                        Span::styled("Duration     ", label_style),
                        Span::styled(format_duration(d), value_style),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "Nothing playing.",
                    Style::default().fg(t.dim),
                )));
            }
        }
        PlaybackBackend::Remote => {
            if let Some(item) = &app.status.current_item {
                lines.push(Line::from(Span::styled(item.title.clone(), title_style)));
                lines.push(Line::from(""));

                let mut row = |label: &'static str, val: String| {
                    if !val.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{label:<13}"), label_style),
                            Span::styled(val, value_style),
                        ]));
                    }
                };

                row("Artist", item.artist.join(", "));
                row("Album", item.album.clone());
                if item.albumartist != item.artist && !item.albumartist.is_empty() {
                    row("Album Artist", item.albumartist.join(", "));
                }
                row("Genre", item.genre.join(", "));

                let mut meta = Vec::new();
                if let Some(y) = item.year { meta.push(y.to_string()); }
                if let Some(t) = item.track {
                    if let Some(d) = item.disc {
                        meta.push(format!("Track {t} / Disc {d}"));
                    } else {
                        meta.push(format!("Track {t}"));
                    }
                }
                if !meta.is_empty() {
                    row("", meta.join("   "));
                }

                row("Duration", format_duration(item.duration));

                if item.rating > 0.0 {
                    row("Rating", format!("{:.1}", item.rating));
                }
                if item.playcount > 0 {
                    row("Play count", item.playcount.to_string());
                }
                if !item.comment.is_empty() {
                    row("Comment", item.comment.clone());
                }
                if !item.file.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("File         ", label_style),
                        Span::styled(item.file.clone(), Style::default().fg(t.dim)),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "Nothing playing.",
                    Style::default().fg(t.dim),
                )));
            }
        }
    }

    lines.push(Line::from(""));

    let popup_w = (area.width * 3 / 4).max(60).min(area.width);
    let popup_h = (lines.len() as u16 + 2).min(area.height);
    let popup_area = center_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(" Track Info  (any key to close) ", Style::default().fg(t.accent))),
            )
            .wrap(ratatui::widgets::Wrap { trim: false }),
        popup_area,
    );
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
