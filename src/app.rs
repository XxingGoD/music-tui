use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use ratatui::widgets::ListState;

use crate::{
    config::AppConfig,
    helper::{MusicDl, SearchResult},
    lyrics::load_lyrics,
    models::{DownloadResponse, LocalTrack, LyricLine, RemoteSong},
    player::Player,
    scanner::scan_music_dir,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Library,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Song,
    Artist,
}

impl SearchMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Song => Self::Artist,
            Self::Artist => Self::Song,
        }
    }

    pub fn helper_arg(self) -> &'static str {
        match self {
            Self::Song => "song",
            Self::Artist => "artist",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Song => "歌曲",
            Self::Artist => "作者",
        }
    }
}

#[derive(Debug)]
pub enum WorkerEvent {
    LibraryScanned(Vec<LocalTrack>),
    SearchFinished(Result<SearchResult, String>),
    DownloadFinished(Result<DownloadResponse, String>),
}

pub struct App {
    pub config: AppConfig,
    helper: MusicDl,
    tx: Sender<WorkerEvent>,
    pub rx: Receiver<WorkerEvent>,
    pub focus: Focus,
    pub library: Vec<LocalTrack>,
    pub search_results: Vec<RemoteSong>,
    pub library_state: ListState,
    pub search_state: ListState,
    pub query: String,
    pub search_mode: SearchMode,
    pub status: String,
    pub busy: bool,
    pub player: Player,
    pub lyrics: Vec<LyricLine>,
    pub lyric_source: String,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        let helper = MusicDl::new(&config);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            config,
            helper,
            tx,
            rx,
            focus: Focus::Search,
            library: Vec::new(),
            search_results: Vec::new(),
            library_state: ListState::default(),
            search_state: ListState::default(),
            query: String::new(),
            search_mode: SearchMode::Song,
            status: "输入关键词后按 Enter 搜索；按 a 切换歌曲/作者搜索".to_string(),
            busy: false,
            player: Player::default(),
            lyrics: Vec::new(),
            lyric_source: "未播放".to_string(),
        };
        if !app.helper.is_ready() {
            app.status = format!(
                "未找到 helper: {}。请先在 helper 目录运行 go build -buildvcs=false -o music-dl-helper .",
                app.helper.helper_path().display()
            );
        }
        app.refresh_library();
        app
    }

    pub fn refresh_library(&mut self) {
        self.status = format!("扫描本地曲库: {}", self.config.music_dir.display());
        let tx = self.tx.clone();
        let music_dir = self.config.music_dir.clone();
        thread::spawn(move || {
            let tracks = scan_music_dir(&music_dir);
            let _ = tx.send(WorkerEvent::LibraryScanned(tracks));
        });
    }

    pub fn search(&mut self) {
        let (mode, query) = self.normalized_search();
        if query.is_empty() || self.busy {
            return;
        }

        self.busy = true;
        self.status = format!("{}搜索中: {query}", mode.label());
        self.search_results.clear();
        self.search_state.select(None);

        let tx = self.tx.clone();
        let helper = self.helper.clone();
        let sources = self.config.default_sources.clone();
        thread::spawn(move || {
            let result = helper.search(&query, mode.helper_arg(), &sources);
            let _ = tx.send(WorkerEvent::SearchFinished(result));
        });
    }

    pub fn download_selected(&mut self) {
        if self.busy {
            return;
        }
        if !self.helper.is_ready() {
            self.status = format!(
                "无法下载：未找到 helper {}",
                self.helper.helper_path().display()
            );
            return;
        }
        let Some(song) = self.selected_remote_song().cloned() else {
            self.status = "没有选中的搜索结果".to_string();
            return;
        };

        self.busy = true;
        self.status = format!(
            "下载中: {} - {} -> {}",
            song.name,
            song.artist,
            self.config.music_dir.display()
        );
        let tx = self.tx.clone();
        let helper = self.helper.clone();
        let out_dir = self.config.music_dir.clone();
        let cover = self.config.embed_cover;
        let lyrics = self.config.embed_lyrics;
        thread::spawn(move || {
            let result = helper.download(&song, &out_dir, cover, lyrics);
            let _ = tx.send(WorkerEvent::DownloadFinished(result));
        });
    }

    pub fn play_selected_local(&mut self) {
        let Some(track) = self.selected_local_track() else {
            self.status = "没有选中的本地歌曲".to_string();
            return;
        };
        let path = track.path.clone();
        let title = track.title.clone();
        let artist = track.artist.clone();
        let lyrics = load_lyrics(&path);
        self.lyric_source = if lyrics.is_empty() {
            format!("{title} - {artist}: 未找到歌词")
        } else {
            format!("{title} - {artist}: {} 行歌词", lyrics.len())
        };
        self.lyrics = lyrics;

        match self.player.play(&path) {
            Ok(()) => self.status = format!("正在播放: {title} - {artist}"),
            Err(err) => self.status = err,
        }
    }

    pub fn process_worker_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::LibraryScanned(tracks) => {
                    let count = tracks.len();
                    self.library = tracks;
                    if count > 0 {
                        self.library_state.select(Some(0));
                    } else {
                        self.library_state.select(None);
                    }
                    self.status = format!("本地曲库已加载: {count} 首");
                }
                WorkerEvent::SearchFinished(result) => {
                    self.busy = false;
                    match result {
                        Ok(search) => {
                            let count = search.songs.len();
                            self.search_results = search.songs;
                            if count > 0 {
                                self.search_state.select(Some(0));
                            } else {
                                self.search_state.select(None);
                            }
                            self.status =
                                search_status(self.search_mode.label(), count, &search.warnings);
                        }
                        Err(err) => {
                            self.search_results.clear();
                            self.search_state.select(None);
                            self.status = format!("搜索失败: {err}");
                        }
                    }
                }
                WorkerEvent::DownloadFinished(result) => {
                    self.busy = false;
                    match result {
                        Ok(response) => {
                            let warning = if response.warning.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", response.warning)
                            };
                            let lyric = response
                                .lyric_path
                                .as_ref()
                                .map(|path| format!(" | 歌词: {}", path.display()))
                                .unwrap_or_else(|| " | 歌词: 未保存".to_string());
                            self.status = format!(
                                "下载完成[{}]: {} -> {}{}{}",
                                response.status,
                                response.filename,
                                response.path.display(),
                                lyric,
                                warning
                            );
                            self.refresh_library();
                        }
                        Err(err) => self.status = format!("下载失败: {err}"),
                    }
                }
            }
        }
    }

    pub fn toggle_search_mode(&mut self) {
        self.search_mode = self.search_mode.toggle();
        self.status = format!("搜索模式: {}", self.search_mode.label());
    }

    pub fn active_lyric_index(&self) -> Option<usize> {
        if self.lyrics.is_empty() {
            return None;
        }

        let elapsed_ms = self.player.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        let next_index = self
            .lyrics
            .iter()
            .position(|line| line.timestamp_ms > elapsed_ms)
            .unwrap_or(self.lyrics.len());
        Some(next_index.saturating_sub(1))
    }

    pub fn move_right(&mut self) {
        self.move_selection(1);
    }

    pub fn move_left(&mut self) {
        self.move_selection(-1);
    }

    pub fn move_down(&mut self, columns: usize) {
        self.move_selection(columns.max(1) as isize);
    }

    pub fn move_up(&mut self, columns: usize) {
        self.move_selection(-(columns.max(1) as isize));
    }

    fn move_selection(&mut self, delta: isize) {
        let (state, len) = match self.focus {
            Focus::Library => (&mut self.library_state, self.library.len()),
            Focus::Search => (&mut self.search_state, self.search_results.len()),
        };

        if len == 0 {
            state.select(None);
            return;
        }

        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        state.select(Some(next));
    }

    pub fn selected_local_track(&self) -> Option<&LocalTrack> {
        self.library_state
            .selected()
            .and_then(|index| self.library.get(index))
    }

    pub fn selected_remote_song(&self) -> Option<&RemoteSong> {
        self.search_state
            .selected()
            .and_then(|index| self.search_results.get(index))
    }

    fn normalized_search(&self) -> (SearchMode, String) {
        let query = self.query.trim();
        if let Some(value) = query
            .strip_prefix("@")
            .or_else(|| query.strip_prefix("artist:"))
            .or_else(|| query.strip_prefix("作者:"))
        {
            return (SearchMode::Artist, value.trim().to_string());
        }

        (self.search_mode, query.to_string())
    }
}

fn search_status(label: &str, count: usize, warnings: &[String]) -> String {
    if count == 0 && !warnings.is_empty() {
        return format!("{label}搜索失败: {}", warnings.join(" | "));
    }

    let mut status = format!("{label}搜索完成: {count} 条结果");
    if !warnings.is_empty() {
        let preview = warnings
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ");
        let suffix = if warnings.len() > 2 {
            format!("{preview} 等 {} 个源失败", warnings.len())
        } else {
            preview
        };
        status.push_str(" | 部分源失败: ");
        status.push_str(&suffix);
    }
    status
}
