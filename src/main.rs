mod app;
mod config;
mod helper;
mod lyrics;
mod models;
mod player;
mod scanner;
mod ui;

use std::{io, time::Duration};

use app::{App, Focus};
use config::AppConfig;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("download-test") {
        return run_download_test();
    }

    let config = AppConfig::load();
    config.ensure_dirs();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut app = App::new(config);
    let result = run_app(&mut terminal, &mut app);

    app.player.stop();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_download_test() -> Result<(), Box<dyn std::error::Error>> {
    use helper::MusicDl;
    use models::RemoteSong;
    use std::{collections::HashMap, path::Path};

    let config = AppConfig::load();
    let helper = MusicDl::new(&config);
    let song = RemoteSong {
        id: "3378803529".to_string(),
        name: "test".to_string(),
        artist: "FiveY".to_string(),
        album: "test".to_string(),
        duration: 170,
        source: "netease".to_string(),
        ext: String::new(),
        cover: String::new(),
        extra: HashMap::from([("song_id".to_string(), "3378803529".to_string())]),
        is_vip: false,
    };

    let result = helper.download(
        &song,
        Path::new("/tmp/music-tui-download-test"),
        false,
        false,
    )?;
    println!("downloaded: {}", result.path.display());
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        app.process_worker_events();
        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Tab => {
                    app.focus = if app.focus == Focus::Library {
                        Focus::Search
                    } else {
                        Focus::Library
                    };
                }
                KeyCode::Left | KeyCode::Char('h') => app.move_left(),
                KeyCode::Right | KeyCode::Char('l') => app.move_right(),
                KeyCode::Up | KeyCode::Char('k') => app.move_up(card_columns()?),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(card_columns()?),
                KeyCode::Enter => {
                    if app.focus == Focus::Library {
                        app.play_selected_local();
                    } else {
                        app.search();
                    }
                }
                KeyCode::Char('d') => app.download_selected(),
                KeyCode::Char('a') => app.toggle_search_mode(),
                KeyCode::Char('p') => app.play_selected_local(),
                KeyCode::Char('s') => {
                    app.player.stop();
                    app.status = "已停止播放".to_string();
                }
                KeyCode::Char('r') => app.refresh_library(),
                KeyCode::Backspace => {
                    if app.focus == Focus::Search {
                        app.query.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if app.focus == Focus::Search {
                        app.query.push(c);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn card_columns() -> Result<usize, Box<dyn std::error::Error>> {
    let (width, _) = crossterm::terminal::size()?;
    let main_width = width.saturating_sub(24);
    let cards_width = main_width.saturating_mul(72) / 100;
    Ok(((cards_width.saturating_sub(4)) / 34).clamp(1, 3) as usize)
}
