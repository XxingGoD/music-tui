# Music TUI

`Music TUI` 是一个独立的终端音乐工具：

- Rust 负责 TUI、卡片式面板、本地曲库扫描、播放控制和歌词展示。
- Go helper 直接调用 `music-lib` 的各音乐源 provider，负责多平台搜索、下载、歌词获取。
- 默认下载目录为 `~/Music`。

## 功能

- 本地曲库扫描：递归扫描音频文件并读取基础元数据。
- 在线搜索：支持歌曲搜索和作者搜索。
- 音乐下载：选中搜索结果后下载到本地曲库目录。
- 歌词下载：下载歌曲时额外保存同名 `.lrc` 文件；播放时自动读取同名歌词。
- 本地播放：通过 `ffplay` 播放本地歌曲。
- 歌词面板：播放本地歌曲时按播放时间高亮当前歌词。
- 卡片式 TUI：侧边栏、顶部搜索栏、主区域卡片网格、右侧状态/歌词栏。

## 项目结构

```text
music-tui/
├── Cargo.toml              # Rust TUI 项目
├── src/                    # TUI、扫描、播放、歌词、helper 调用
├── helper/
│   ├── go.mod              # Go helper 模块
│   └── main.go             # 内置多音乐源搜索/下载/歌词 helper
└── README.md
```

## 依赖

需要安装：

- `rust` / `cargo`
- `go`
- `ffprobe`：扫描本地音频元数据
- `ffplay`：播放本地音频

`ffprobe` 和 `ffplay` 来自 FFmpeg。

Ubuntu/Debian 示例：

```bash
sudo apt install ffmpeg
```

## 构建

先构建 Go helper：

```bash
cd helper
go build -buildvcs=false -o music-dl-helper .
```

再构建或运行 Rust TUI：

```bash
cd ..
cargo build
cargo run
```

## 配置

首次运行会生成配置文件：

```text
~/.config/music-tui/config.toml
```

默认配置含义：

```toml
music_dir = "/home/you/Music"
helper_path = "helper/music-dl-helper"
default_sources = ["netease", "qq", "kugou", "kuwo", "migu", "qianqian", "soda"]
embed_cover = true
embed_lyrics = true

[source_cookies]
# qq = "uin=...; qm_keyst=..."
# soda = "sessionid=..."
```

- `music_dir`：本地曲库和下载目录。
- `helper_path`：Go helper 路径，默认相对 `music-tui` 项目根目录。
- `default_sources`：默认搜索源。
- `embed_cover`：下载时尝试写入封面元数据。
- `embed_lyrics`：下载时尝试写入歌词元数据，同时保存同名 `.lrc`。
- `source_cookies`：需要登录态的音乐源 cookie，按源名填写。

## 使用

启动：

```bash
cargo run
```

搜索歌曲：

1. 按 `Tab` 切到“在线搜索”区域。
2. 输入歌曲关键词。
3. 按 `Enter` 搜索。
4. 用方向键选择卡片。
5. 按 `d` 下载。

搜索作者：

- 按 `a` 切换到作者搜索模式，然后输入作者名并按 `Enter`。
- 或直接输入 `@作者名` / `artist:作者名` 搜索。

播放本地歌曲：

1. 按 `Tab` 切到“本地曲库”区域。
2. 用方向键选择歌曲。
3. 按 `Enter` 或 `p` 播放。
4. 如果存在同名 `.lrc/.txt/.lyric` 或音频内嵌歌词，右侧歌词面板会显示并高亮。

## 按键

- `Tab`：切换“本地曲库 / 在线搜索”主区域。
- `←/→/↑/↓`：只在当前选中的主区域卡片内移动。
- `h/j/k/l`：同方向键，只在当前主区域内移动。
- `Enter`：搜索区域执行搜索；曲库区域播放选中歌曲。
- `d`：下载选中的在线搜索结果。
- `a`：切换歌曲搜索 / 作者搜索。
- `p`：播放本地选中歌曲。
- `s`：停止播放。
- `r`：刷新本地曲库。
- `q`：退出。

## 下载和歌词

下载成功后会生成音频文件：

```text
~/Music/歌名 - 歌手.mp3
```

如果平台返回歌词，会额外生成：

```text
~/Music/歌名 - 歌手.lrc
```

如果某个源没有歌词，TUI 状态栏会显示类似：

```text
lyric fetch failed: lyric is empty or not found
```

这表示歌曲下载成功，但该源没有返回可保存的歌词。

## 故障排查

如果 TUI 显示找不到 helper：

```bash
cd helper
go build -buildvcs=false -o music-dl-helper .
```

如果能搜索但不能播放：

- 确认安装了 `ffplay`。
- 运行 `ffplay -version` 检查。

如果本地曲库没有元数据：

- 确认安装了 `ffprobe`。
- 运行 `ffprobe -version` 检查。

如果下载后看不到歌曲：

- 检查配置里的 `music_dir`。
- 默认下载到 `~/Music`。
- 按 `r` 刷新本地曲库。

如果某些源搜索/下载失败：

- 先确认该源本身是否可用。
- 对需要登录态的源，在 `~/.config/music-tui/config.toml` 的 `[source_cookies]` 中填写 cookie。

## 已知限制

- 卡片封面区域目前使用来源占位，不渲染真实网络封面图。
- 真实图片渲染需要 Kitty/Sixel/iTerm2 等终端图片协议支持，后续可继续接入。
- 搜索和歌词能力取决于各音乐源接口，部分歌曲可能没有可下载歌词。
- 目前没有内置 cookie 管理 UI，需要手动编辑配置文件。
