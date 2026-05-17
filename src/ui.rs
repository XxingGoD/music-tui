use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, Focus, SearchMode},
    models::format_duration,
};

const CARD_MIN_WIDTH: u16 = 34;
const CARD_HEIGHT: u16 = 9;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.size();
    render_wallpaper(frame, area);

    let shell = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(50)])
        .split(area);

    render_sidebar(frame, shell[0], app);
    render_main(frame, shell[1], app);

    if app.busy {
        render_busy(frame, area);
    }
}

fn render_wallpaper(frame: &mut Frame, area: Rect) {
    let block = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(block, area);

    let art = [
        "      .        *          .        ",
        "   *      MUSIC TOOL TERMINAL   .  ",
        "        .      ▓▓▒▒░░      *       ",
        "   .       neon cards / rust tui   ",
        "        *        .          .      ",
    ];
    let mut lines = Vec::new();
    for _ in 0..area.height.saturating_sub(art.len() as u16 + 2) / 2 {
        lines.push(Line::from(""));
    }
    for line in art {
        lines.push(Line::from(Span::styled(
            line,
            Style::default().fg(Color::Rgb(58, 24, 58)),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let items = [
        ("▌", "Music TUI", "Rust + standalone helper"),
        ("⌂", "首页", "本地曲库"),
        ("⌕", "搜索", "在线下载"),
        ("♫", "歌词", "同步显示"),
        ("⚙", "设置", "config.toml"),
    ];

    let mut lines = Vec::new();
    lines.push(Line::from(""));
    for (idx, (icon, title, desc)) in items.iter().enumerate() {
        let selected = match idx {
            1 => app.focus == Focus::Library,
            2 => app.focus == Focus::Search,
            _ => false,
        };
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else if idx == 0 {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {icon} "), style),
            Span::styled(*title, style),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {desc}"),
            Style::default().fg(Color::Rgb(170, 140, 180)),
        )));
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(Color::Rgb(210, 90, 255))),
        ),
        area,
    );
}

fn render_main(frame: &mut Frame, area: Rect, app: &mut App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    render_topbar(frame, layout[0], app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(layout[1]);

    render_cards(frame, body[0], app);
    render_info_panel(frame, body[1], app);
    render_footer(frame, layout[2], app);
}

fn render_topbar(frame: &mut Frame, area: Rect, app: &App) {
    let mode_style = match app.search_mode {
        SearchMode::Song => Style::default().fg(Color::Yellow),
        SearchMode::Artist => Style::default().fg(Color::Magenta),
    };
    let title = if app.focus == Focus::Library {
        "本地曲库"
    } else {
        "在线搜索"
    };
    let query = if app.query.is_empty() {
        "输入关键词，Enter 搜索"
    } else {
        &app.query
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Left),
        Line::from(vec![
            Span::styled(
                format!("{}搜索: ", app.search_mode.label()),
                mode_style.add_modifier(Modifier::BOLD),
            ),
            Span::styled(query, Style::default().fg(Color::White)),
            Span::styled("  _", Style::default().fg(Color::Cyan)),
        ])
        .alignment(Alignment::Center),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(220, 90, 255)))
                .title(" 首页 "),
        ),
        shrink(area, 1, 0),
    );
}

fn render_cards(frame: &mut Frame, area: Rect, app: &mut App) {
    let area = shrink(area, 1, 0);
    let title = if app.focus == Focus::Library {
        format!(" ACTIVE 本地曲库 · {} 首 ", app.library.len())
    } else {
        format!(" ACTIVE 搜索结果 · {} 条 ", app.search_results.len())
    };
    let active_border = match app.focus {
        Focus::Library => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        Focus::Search => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    };

    frame.render_widget(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(active_border),
        area,
    );

    let inner = shrink(area, 1, 1);
    let columns = (inner.width / CARD_MIN_WIDTH).clamp(1, 3);
    let rows = (inner.height / CARD_HEIGHT).max(1);
    let visible = (columns * rows) as usize;

    let selected = selected_index(app);
    let start = selected.saturating_sub(visible.saturating_sub(1));
    let mut index = start;

    for row in 0..rows {
        let row_area = Rect::new(
            inner.x,
            inner.y + row * CARD_HEIGHT,
            inner.width,
            CARD_HEIGHT.min(inner.height.saturating_sub(row * CARD_HEIGHT)),
        );
        let col_constraints = vec![Constraint::Ratio(1, columns as u32); columns as usize];
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for cell in cells.iter() {
            if index >= item_count(app) {
                break;
            }
            render_card(frame, shrink(*cell, 1, 0), app, index, index == selected);
            index += 1;
        }
    }
}

fn render_card(frame: &mut Frame, area: Rect, app: &App, index: usize, selected: bool) {
    let border = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(185, 145, 220))
    };
    let card_style = if selected {
        Style::default().bg(Color::Rgb(18, 22, 32))
    } else {
        Style::default()
    };

    let (title, subtitle, album, source, duration) = match app.focus {
        Focus::Library => {
            let track = &app.library[index];
            (
                truncate(&track.title, area.width.saturating_sub(4) as usize),
                truncate(&track.artist, area.width.saturating_sub(4) as usize),
                truncate(&track.album, area.width.saturating_sub(4) as usize),
                "local".to_string(),
                format_duration(track.duration),
            )
        }
        Focus::Search => {
            let song = &app.search_results[index];
            (
                truncate(&song.name, area.width.saturating_sub(4) as usize),
                truncate(&song.artist, area.width.saturating_sub(4) as usize),
                truncate(&song.album, area.width.saturating_sub(4) as usize),
                song.source.clone(),
                format_duration(song.duration),
            )
        }
    };

    let cover = cover_art(&source);
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        cover,
        Style::default()
            .fg(Color::Rgb(255, 105, 240))
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        subtitle,
        Style::default().fg(Color::Rgb(235, 225, 245)),
    )));
    lines.push(Line::from(Span::styled(
        if album.is_empty() {
            "Unknown Album".to_string()
        } else {
            album
        },
        Style::default().fg(Color::Rgb(170, 140, 180)),
    )));
    lines.push(Line::from(vec![
        Span::styled(source, Style::default().fg(Color::Rgb(210, 150, 255))),
        Span::raw(" · "),
        Span::styled(
            duration,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border)
                    .title(if selected { " ▶ " } else { "   " }),
            )
            .style(card_style)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_info_panel(frame: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(8)])
        .split(shrink(area, 0, 0));

    render_now_card(frame, shrink(layout[0], 0, 0), app);
    render_lyrics_card(frame, shrink(layout[1], 0, 1), app);
}

fn render_now_card(frame: &mut Frame, area: Rect, app: &App) {
    let playing = app.player.current().unwrap_or("-");
    let lines = vec![
        Line::from(Span::styled(
            "◷",
            Style::default()
                .fg(Color::Rgb(220, 140, 210))
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            "正在播放",
            Style::default().fg(Color::Rgb(220, 140, 210)),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            truncate(playing, area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!(
                "曲库: {}  搜索: {}",
                app.library.len(),
                app.search_results.len()
            ),
            Style::default().fg(Color::Rgb(170, 140, 180)),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" 状态 ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(185, 145, 220))),
        ),
        area,
    );
}

fn render_lyrics_card(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.active_lyric_index();
    let mut lines = Vec::new();

    if app.lyrics.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "播放本地歌曲后显示歌词",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "下载时会保存同名 .lrc",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let active = active.unwrap_or(0);
        let visible = area.height.saturating_sub(2).max(1) as usize;
        let half = visible / 2;
        let start = active.saturating_sub(half);
        let end = (start + visible).min(app.lyrics.len());

        for (idx, lyric) in app.lyrics[start..end].iter().enumerate() {
            let absolute_idx = start + idx;
            let style = if absolute_idx == active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if absolute_idx < active {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(lyric.text.as_str(), style)));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" 歌词 | {} ", truncate(&app.lyric_source, 24)))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = "[Tab] 切换区域  [←→↑↓/hjkl] 当前区域导航  [Enter] 搜索/播放  [d] 下载  [a] 作者模式  [q] 退出";
    let lines = vec![
        Line::from(Span::styled(
            truncate(&app.status, area.width.saturating_sub(2) as usize),
            Style::default().fg(Color::Green),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            help,
            Style::default().fg(Color::Rgb(220, 180, 255)),
        ))
        .alignment(Alignment::Center),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_busy(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(42, 5, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new("任务执行中...\n搜索/下载由内置 helper 处理")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Working ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Magenta)),
            ),
        popup,
    );
}

fn selected_index(app: &App) -> usize {
    match app.focus {
        Focus::Library => app.library_state.selected().unwrap_or(0),
        Focus::Search => app.search_state.selected().unwrap_or(0),
    }
}

fn item_count(app: &App) -> usize {
    match app.focus {
        Focus::Library => app.library.len(),
        Focus::Search => app.search_results.len(),
    }
}

fn cover_art(source: &str) -> &'static str {
    match source {
        "netease" => "  Netease  ",
        "qq" => "  QQ Music ",
        "kugou" => "  Kugou   ",
        "kuwo" => "  Kuwo    ",
        "migu" => "  Migu    ",
        "soda" => "  Soda    ",
        "local" => "  Local   ",
        _ => "  Music   ",
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx + 1 >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

fn shrink(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(x),
        y: area.y.saturating_add(y),
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
