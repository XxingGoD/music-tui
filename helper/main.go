package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/guohuiyuan/music-lib/apple"
	"github.com/guohuiyuan/music-lib/bilibili"
	"github.com/guohuiyuan/music-lib/fivesing"
	"github.com/guohuiyuan/music-lib/jamendo"
	"github.com/guohuiyuan/music-lib/joox"
	"github.com/guohuiyuan/music-lib/kugou"
	"github.com/guohuiyuan/music-lib/kuwo"
	"github.com/guohuiyuan/music-lib/migu"
	"github.com/guohuiyuan/music-lib/model"
	"github.com/guohuiyuan/music-lib/netease"
	"github.com/guohuiyuan/music-lib/qianqian"
	"github.com/guohuiyuan/music-lib/qq"
	"github.com/guohuiyuan/music-lib/soda"
	"github.com/guohuiyuan/music-lib/utils"
)

const (
	sourceCookiesEnv = "MUSIC_TUI_SOURCE_COOKIES"
	userAgentCommon  = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36"
	userAgentMobile  = "Mozilla/5.0 (iPhone; CPU iPhone OS 9_1 like Mac OS X) AppleWebKit/601.1.46 (KHTML, like Gecko) Version/9.0 Mobile/13B143 Safari/601.1"
)

var (
	errFFmpegNotFound  = errors.New("ffmpeg not found")
	allSources         = []string{"netease", "qq", "kugou", "kuwo", "migu", "fivesing", "jamendo", "joox", "qianqian", "soda", "bilibili", "apple"}
	defaultSources     = []string{"netease", "qq", "kugou", "kuwo", "migu", "qianqian", "soda", "apple"}
	sourceDescriptions = map[string]string{
		"netease":  "网易云音乐",
		"qq":       "QQ音乐",
		"kugou":    "酷狗音乐",
		"kuwo":     "酷我音乐",
		"migu":     "咪咕音乐",
		"fivesing": "5sing",
		"jamendo":  "Jamendo (CC)",
		"joox":     "JOOX",
		"qianqian": "千千音乐",
		"soda":     "汽水音乐",
		"bilibili": "Bilibili",
		"apple":    "Apple Music",
	}
	sourceSpecs = map[string]sourceSpec{
		"netease":  {newClient: func(cookie string) any { return netease.New(cookie) }},
		"qq":       {newClient: func(cookie string) any { return qq.New(cookie) }},
		"kugou":    {newClient: func(cookie string) any { return kugou.New(cookie) }},
		"kuwo":     {newClient: func(cookie string) any { return kuwo.New(cookie) }},
		"migu":     {newClient: func(cookie string) any { return migu.New(cookie) }},
		"fivesing": {newClient: func(cookie string) any { return fivesing.New(cookie) }},
		"jamendo":  {newClient: func(cookie string) any { return jamendo.New(cookie) }},
		"joox":     {newClient: func(cookie string) any { return joox.New(cookie) }},
		"qianqian": {newClient: func(cookie string) any { return qianqian.New(cookie) }},
		"soda":     {newClient: func(cookie string) any { return soda.New(cookie) }},
		"bilibili": {newClient: func(cookie string) any { return bilibili.New(cookie) }},
		"apple":    {newClient: func(cookie string) any { return apple.New(cookie) }},
	}
)

type cookieStore map[string]string

type sourceSpec struct {
	newClient func(cookie string) any
}

type songSearcher interface {
	Search(keyword string) ([]model.Song, error)
}

type songDownloader interface {
	GetDownloadURL(song *model.Song) (string, error)
}

type lyricProvider interface {
	GetLyrics(song *model.Song) (string, error)
}

type songParser interface {
	Parse(link string) (*model.Song, error)
}

type playlistParser interface {
	ParsePlaylist(link string) (*model.Playlist, []model.Song, error)
}

type albumParser interface {
	ParseAlbum(link string) (*model.Playlist, []model.Song, error)
}

type songDTO struct {
	ID       string            `json:"id"`
	Name     string            `json:"name"`
	Artist   string            `json:"artist"`
	Album    string            `json:"album"`
	AlbumID  string            `json:"album_id,omitempty"`
	Duration int               `json:"duration"`
	Size     int64             `json:"size,omitempty"`
	Bitrate  int               `json:"bitrate,omitempty"`
	Source   string            `json:"source"`
	Ext      string            `json:"ext,omitempty"`
	Cover    string            `json:"cover,omitempty"`
	Link     string            `json:"link,omitempty"`
	Extra    map[string]string `json:"extra,omitempty"`
	IsVIP    bool              `json:"is_vip,omitempty"`
}

type searchResponse struct {
	Songs []songDTO `json:"songs"`
}

type downloadResponse struct {
	Status    string `json:"status"`
	Path      string `json:"path"`
	Filename  string `json:"filename"`
	LyricPath string `json:"lyric_path,omitempty"`
	Warning   string `json:"warning,omitempty"`
}

type savedSong struct {
	Ext       string
	Filename  string
	SavedPath string
	Lyric     string
	Warning   string
}

func main() {
	if len(os.Args) < 2 {
		fail(errors.New("usage: music-dl-helper <search|download|lyric|sources> [flags]"))
	}

	cookies := loadCookiesFromEnv()

	switch os.Args[1] {
	case "search":
		searchCmd(os.Args[2:], cookies)
	case "download":
		downloadCmd(os.Args[2:], cookies)
	case "lyric":
		lyricCmd(os.Args[2:], cookies)
	case "sources":
		sourcesCmd()
	default:
		fail(fmt.Errorf("unknown command: %s", os.Args[1]))
	}
}

func searchCmd(args []string, cookies cookieStore) {
	fs := flag.NewFlagSet("search", flag.ExitOnError)
	keyword := fs.String("keyword", "", "keyword or share URL")
	mode := fs.String("mode", "song", "search mode: song or artist")
	sourcesRaw := fs.String("sources", "", "comma separated sources")
	limit := fs.Int("limit", 80, "max results")
	_ = fs.Parse(args)

	query := strings.TrimSpace(*keyword)
	if query == "" {
		fail(errors.New("missing keyword"))
	}

	sources := parseSources(*sourcesRaw)
	if len(sources) == 0 {
		sources = defaultSourceNames()
	}

	var songs []model.Song
	var mu sync.Mutex

	if strings.HasPrefix(query, "http://") || strings.HasPrefix(query, "https://") {
		source := detectSource(query)
		if source == "" {
			fail(errors.New("unsupported link source"))
		}
		if song, err := parseSongLink(source, query, cookies); err == nil && song != nil {
			if song.Source == "" {
				song.Source = source
			}
			songs = append(songs, *song)
		}
		if len(songs) == 0 {
			if parsedSongs, err := parsePlaylistLink(source, query, cookies); err == nil {
				for i := range parsedSongs {
					if parsedSongs[i].Source == "" {
						parsedSongs[i].Source = source
					}
				}
				songs = append(songs, parsedSongs...)
			}
		}
		if len(songs) == 0 {
			if parsedSongs, err := parseAlbumLink(source, query, cookies); err == nil {
				for i := range parsedSongs {
					if parsedSongs[i].Source == "" {
						parsedSongs[i].Source = source
					}
				}
				songs = append(songs, parsedSongs...)
			}
		}
	} else {
		var wg sync.WaitGroup
		for _, source := range sources {
			if _, ok := sourceSpecs[source]; !ok {
				continue
			}
			wg.Add(1)
			go func(src string) {
				defer wg.Done()
				result, err := searchSongs(src, query, cookies)
				if err != nil {
					return
				}
				for i := range result {
					result[i].Source = src
				}
				mu.Lock()
				songs = append(songs, result...)
				mu.Unlock()
			}(source)
		}
		wg.Wait()
	}

	if strings.EqualFold(strings.TrimSpace(*mode), "artist") {
		songs = filterByArtist(songs, query)
	}

	if *limit > 0 && len(songs) > *limit {
		songs = songs[:*limit]
	}

	writeJSON(searchResponse{Songs: mapSongs(songs)})
}

func filterByArtist(songs []model.Song, artist string) []model.Song {
	artist = normalizeText(artist)
	if artist == "" {
		return songs
	}

	filtered := make([]model.Song, 0, len(songs))
	for _, song := range songs {
		if strings.Contains(normalizeText(song.Artist), artist) {
			filtered = append(filtered, song)
		}
	}
	return filtered
}

func normalizeText(value string) string {
	value = strings.TrimSpace(strings.ToLower(value))
	value = strings.ReplaceAll(value, "　", " ")
	fields := strings.Fields(value)
	return strings.Join(fields, " ")
}

func downloadCmd(args []string, cookies cookieStore) {
	fs := flag.NewFlagSet("download", flag.ExitOnError)
	id := fs.String("id", "", "song id")
	source := fs.String("source", "", "song source")
	name := fs.String("name", "Unknown", "song name")
	artist := fs.String("artist", "Unknown", "song artist")
	album := fs.String("album", "", "song album")
	coverURL := fs.String("cover-url", "", "cover URL")
	outDir := fs.String("outdir", "", "output directory")
	withCover := fs.Bool("cover", true, "embed cover when possible")
	withLyrics := fs.Bool("lyrics", true, "embed lyrics when possible")
	extraRaw := fs.String("extra", "", "JSON object with source-specific metadata")
	_ = fs.Parse(args)

	if strings.TrimSpace(*id) == "" || strings.TrimSpace(*source) == "" {
		fail(errors.New("missing id or source"))
	}

	extra := map[string]string(nil)
	if strings.TrimSpace(*extraRaw) != "" {
		if err := json.Unmarshal([]byte(*extraRaw), &extra); err != nil {
			fail(fmt.Errorf("invalid extra JSON: %w", err))
		}
	}

	song := &model.Song{
		ID:     strings.TrimSpace(*id),
		Source: strings.TrimSpace(*source),
		Name:   strings.TrimSpace(*name),
		Artist: strings.TrimSpace(*artist),
		Album:  strings.TrimSpace(*album),
		Cover:  strings.TrimSpace(*coverURL),
		Extra:  extra,
	}

	result, err := saveSongToFile(song, strings.TrimSpace(*outDir), *withCover, *withLyrics, cookies)
	if err != nil {
		fail(err)
	}

	lyricPath, lyricWarning := saveLyricFile(result.Lyric, result.SavedPath, *withLyrics)
	warning := result.Warning
	if lyricWarning != "" {
		if warning != "" {
			warning += "; "
		}
		warning += lyricWarning
	}

	writeJSON(downloadResponse{
		Status:    "ok",
		Path:      result.SavedPath,
		Filename:  result.Filename,
		LyricPath: lyricPath,
		Warning:   warning,
	})
}

func lyricCmd(args []string, cookies cookieStore) {
	fs := flag.NewFlagSet("lyric", flag.ExitOnError)
	id := fs.String("id", "", "song id")
	source := fs.String("source", "", "song source")
	name := fs.String("name", "Unknown", "song name")
	artist := fs.String("artist", "Unknown", "song artist")
	album := fs.String("album", "", "song album")
	extraRaw := fs.String("extra", "", "JSON object with source-specific metadata")
	_ = fs.Parse(args)

	if strings.TrimSpace(*id) == "" || strings.TrimSpace(*source) == "" {
		fail(errors.New("missing id or source"))
	}

	extra := map[string]string(nil)
	if strings.TrimSpace(*extraRaw) != "" {
		if err := json.Unmarshal([]byte(*extraRaw), &extra); err != nil {
			fail(fmt.Errorf("invalid extra JSON: %w", err))
		}
	}

	song := &model.Song{
		ID:     strings.TrimSpace(*id),
		Source: strings.TrimSpace(*source),
		Name:   strings.TrimSpace(*name),
		Artist: strings.TrimSpace(*artist),
		Album:  strings.TrimSpace(*album),
		Extra:  extra,
	}
	lyric, err := fetchLyrics(song, cookies)
	if err != nil {
		fail(err)
	}
	writeJSON(map[string]string{"lyric": lyric})
}

func saveLyricFile(lyric string, audioPath string, enabled bool) (string, string) {
	if !enabled {
		return "", ""
	}
	if strings.TrimSpace(audioPath) == "" {
		return "", "lyric skipped: empty saved path"
	}
	lyric = strings.TrimSpace(lyric)
	if lyric == "" {
		return "", "lyric not found"
	}

	lyricPath := strings.TrimSuffix(audioPath, filepath.Ext(audioPath)) + ".lrc"
	if err := os.WriteFile(lyricPath, []byte(lyric+"\n"), 0644); err != nil {
		return "", "lyric write failed: " + err.Error()
	}
	return lyricPath, ""
}

func sourcesCmd() {
	type sourceDTO struct {
		ID   string `json:"id"`
		Name string `json:"name"`
	}
	sources := allSourceNames()
	items := make([]sourceDTO, 0, len(sources))
	for _, source := range sources {
		items = append(items, sourceDTO{ID: source, Name: sourceDescription(source)})
	}
	writeJSON(map[string]any{"sources": items})
}

func parseSources(raw string) []string {
	var sources []string
	seen := make(map[string]bool)
	for _, source := range strings.Split(raw, ",") {
		source = strings.TrimSpace(source)
		if source == "" || seen[source] {
			continue
		}
		seen[source] = true
		sources = append(sources, source)
	}
	return sources
}

func loadCookiesFromEnv() cookieStore {
	raw := strings.TrimSpace(os.Getenv(sourceCookiesEnv))
	if raw == "" {
		return cookieStore{}
	}

	parsed := map[string]string{}
	if err := json.Unmarshal([]byte(raw), &parsed); err != nil {
		return cookieStore{}
	}

	cookies := cookieStore{}
	for source, cookie := range parsed {
		source = strings.TrimSpace(source)
		cookie = strings.TrimSpace(cookie)
		if source == "" || cookie == "" {
			continue
		}
		cookies[source] = cookie
	}
	return cookies
}

func (c cookieStore) get(source string) string {
	if c == nil {
		return ""
	}
	return strings.TrimSpace(c[strings.TrimSpace(source)])
}

func allSourceNames() []string {
	return append([]string(nil), allSources...)
}

func defaultSourceNames() []string {
	return append([]string(nil), defaultSources...)
}

func sourceDescription(source string) string {
	if desc, ok := sourceDescriptions[source]; ok {
		return desc
	}
	return "未知音乐源"
}

func detectSource(link string) string {
	switch {
	case strings.Contains(link, "163.com"):
		return "netease"
	case strings.Contains(link, "qq.com"):
		return "qq"
	case strings.Contains(link, "5sing"):
		return "fivesing"
	case strings.Contains(link, "kugou.com"):
		return "kugou"
	case strings.Contains(link, "kuwo.cn"):
		return "kuwo"
	case strings.Contains(link, "migu.cn"):
		return "migu"
	case strings.Contains(link, "joox.com"):
		return "joox"
	case strings.Contains(link, "bilibili.com"), strings.Contains(link, "b23.tv"):
		return "bilibili"
	case strings.Contains(link, "douyin.com"), strings.Contains(link, "qishui"):
		return "soda"
	case strings.Contains(link, "91q.com"):
		return "qianqian"
	case strings.Contains(link, "jamendo.com"):
		return "jamendo"
	case strings.Contains(link, "music.apple.com"), strings.Contains(link, "itunes.apple.com"):
		return "apple"
	default:
		return ""
	}
}

func newSourceClient(source string, cookies cookieStore) (any, error) {
	spec, ok := sourceSpecs[source]
	if !ok {
		return nil, fmt.Errorf("unsupported source: %s", source)
	}
	return spec.newClient(cookies.get(source)), nil
}

func searchSongs(source string, keyword string, cookies cookieStore) ([]model.Song, error) {
	client, err := newSourceClient(source, cookies)
	if err != nil {
		return nil, err
	}
	searcher, ok := client.(songSearcher)
	if !ok {
		return nil, fmt.Errorf("search unsupported for source: %s", source)
	}
	return searcher.Search(keyword)
}

func parseSongLink(source string, link string, cookies cookieStore) (*model.Song, error) {
	client, err := newSourceClient(source, cookies)
	if err != nil {
		return nil, err
	}
	parser, ok := client.(songParser)
	if !ok {
		return nil, fmt.Errorf("link parsing unsupported for source: %s", source)
	}
	return parser.Parse(link)
}

func parsePlaylistLink(source string, link string, cookies cookieStore) ([]model.Song, error) {
	client, err := newSourceClient(source, cookies)
	if err != nil {
		return nil, err
	}
	parser, ok := client.(playlistParser)
	if !ok {
		return nil, fmt.Errorf("playlist parsing unsupported for source: %s", source)
	}
	_, songs, err := parser.ParsePlaylist(link)
	return songs, err
}

func parseAlbumLink(source string, link string, cookies cookieStore) ([]model.Song, error) {
	client, err := newSourceClient(source, cookies)
	if err != nil {
		return nil, err
	}
	parser, ok := client.(albumParser)
	if !ok {
		return nil, fmt.Errorf("album parsing unsupported for source: %s", source)
	}
	_, songs, err := parser.ParseAlbum(link)
	return songs, err
}

func fetchDownloadURL(song *model.Song, cookies cookieStore) (string, error) {
	client, err := newSourceClient(song.Source, cookies)
	if err != nil {
		return "", err
	}
	downloader, ok := client.(songDownloader)
	if !ok {
		return "", fmt.Errorf("download unsupported for source: %s", song.Source)
	}
	return downloader.GetDownloadURL(song)
}

func fetchLyrics(song *model.Song, cookies cookieStore) (string, error) {
	client, err := newSourceClient(song.Source, cookies)
	if err != nil {
		return "", err
	}
	provider, ok := client.(lyricProvider)
	if !ok {
		return "", fmt.Errorf("lyric unsupported for source: %s", song.Source)
	}
	return provider.GetLyrics(song)
}

func saveSongToFile(song *model.Song, outDir string, withCover bool, withLyrics bool, cookies cookieStore) (*savedSong, error) {
	normalized := normalizeSong(song)
	audioData, ext, err := fetchSongAudio(normalized, cookies)
	if err != nil {
		return nil, err
	}

	lyric := ""
	if withLyrics {
		lyric, _ = fetchLyrics(normalized, cookies)
		lyric = strings.TrimSpace(lyric)
	}

	var coverData []byte
	var coverMime string
	if withCover && strings.TrimSpace(normalized.Cover) != "" {
		coverData, coverMime, _ = fetchBytesWithMime(normalized.Cover, normalized.Source, cookies.get(normalized.Source))
	}

	finalData := audioData
	warning := ""
	if shouldEmbedMetadata(ext, normalized, lyric, coverData) {
		embeddedData, embedErr := embedSongMetadata(audioData, ext, normalized, lyric, coverData, coverMime)
		switch {
		case embedErr == nil:
			finalData = embeddedData
		case errors.Is(embedErr, errFFmpegNotFound):
			warning = "ffmpeg not found, metadata embedding skipped"
		default:
			warning = "metadata embedding failed, using original audio"
		}
	}

	if ext == "" {
		ext = detectAudioExt(finalData)
	}
	if ext == "" {
		ext = "mp3"
	}

	targetDir := strings.TrimSpace(outDir)
	if targetDir == "" {
		targetDir = "."
	}
	targetDir = filepath.Clean(targetDir)
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		return nil, err
	}

	filename := buildDownloadFilename(normalized, ext)
	savedPath := filepath.Join(targetDir, filename)
	if err := os.WriteFile(savedPath, finalData, 0o644); err != nil {
		return nil, err
	}

	return &savedSong{
		Ext:       ext,
		Filename:  filename,
		SavedPath: savedPath,
		Lyric:     lyric,
		Warning:   warning,
	}, nil
}

func normalizeSong(song *model.Song) *model.Song {
	if song == nil {
		return &model.Song{Name: "Unknown", Artist: "Unknown"}
	}
	normalized := *song
	normalized.ID = strings.TrimSpace(normalized.ID)
	normalized.Source = strings.TrimSpace(normalized.Source)
	normalized.Name = strings.TrimSpace(normalized.Name)
	normalized.Artist = strings.TrimSpace(normalized.Artist)
	normalized.Album = strings.TrimSpace(normalized.Album)
	normalized.Cover = strings.TrimSpace(normalized.Cover)
	if normalized.Name == "" {
		normalized.Name = "Unknown"
	}
	if normalized.Artist == "" {
		normalized.Artist = "Unknown"
	}
	return &normalized
}

func fetchSongAudio(song *model.Song, cookies cookieStore) ([]byte, string, error) {
	if song == nil {
		return nil, "", errors.New("song is nil")
	}
	if song.ID == "" || song.Source == "" {
		return nil, "", errors.New("missing song id or source")
	}

	cookie := cookies.get(song.Source)
	if song.Source == "soda" {
		info, err := soda.New(cookie).GetDownloadInfo(song)
		if err != nil {
			return nil, "", err
		}
		encryptedData, _, err := fetchBytesWithMime(info.URL, song.Source, cookie)
		if err != nil {
			return nil, "", err
		}
		finalData, err := soda.DecryptAudio(encryptedData, info.PlayAuth)
		if err != nil {
			return nil, "", err
		}
		ext := strings.TrimSpace(strings.TrimPrefix(info.Format, "."))
		if ext == "" {
			ext = detectAudioExt(finalData)
		}
		return finalData, ext, nil
	}

	urlStr, err := fetchDownloadURL(song, cookies)
	if err != nil {
		return nil, "", err
	}
	if strings.TrimSpace(urlStr) == "" {
		return nil, "", errors.New("empty download url")
	}

	data, contentType, err := fetchBytesWithMime(urlStr, song.Source, cookie)
	if err != nil {
		return nil, "", err
	}

	ext := detectAudioExtByContentType(contentType)
	if ext == "" {
		ext = strings.TrimSpace(strings.TrimPrefix(song.Ext, "."))
	}
	if ext == "" {
		ext = detectAudioExt(data)
	}
	return data, ext, nil
}

func shouldEmbedMetadata(ext string, song *model.Song, lyric string, coverData []byte) bool {
	switch strings.ToLower(strings.TrimSpace(ext)) {
	case "mp3", "flac", "m4a", "wma":
	default:
		return false
	}
	album := ""
	if song != nil {
		album = strings.TrimSpace(song.Album)
	}
	return album != "" || strings.TrimSpace(lyric) != "" || len(coverData) > 0
}

func buildDownloadFilename(song *model.Song, ext string) string {
	name := "Unknown"
	artist := "Unknown"
	if song != nil {
		if strings.TrimSpace(song.Name) != "" {
			name = strings.TrimSpace(song.Name)
		}
		if strings.TrimSpace(song.Artist) != "" {
			artist = strings.TrimSpace(song.Artist)
		}
	}
	ext = strings.TrimSpace(strings.TrimPrefix(ext, "."))
	filename := fmt.Sprintf("%s - %s", name, artist)
	if ext != "" {
		filename += "." + ext
	}
	return utils.SanitizeFilename(filename)
}

func fetchBytesWithMime(urlStr string, source string, cookie string) ([]byte, string, error) {
	req, err := http.NewRequest(http.MethodGet, urlStr, nil)
	if err != nil {
		return nil, "", err
	}
	applySourceHeaders(req, source, cookie)

	client := &http.Client{Timeout: 2 * time.Minute}
	resp, err := client.Do(req)
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, "", fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, "", err
	}

	contentType := strings.TrimSpace(resp.Header.Get("Content-Type"))
	if contentType == "" && len(data) > 0 {
		contentType = http.DetectContentType(data)
	}
	if idx := strings.Index(contentType, ";"); idx >= 0 {
		contentType = strings.TrimSpace(contentType[:idx])
	}
	return data, contentType, nil
}

func applySourceHeaders(req *http.Request, source string, cookie string) {
	req.Header.Set("User-Agent", userAgentCommon)
	switch source {
	case "bilibili":
		req.Header.Set("Referer", "https://www.bilibili.com/")
	case "netease":
		req.Header.Set("Referer", "http://music.163.com/")
	case "migu":
		req.Header.Set("User-Agent", userAgentMobile)
		req.Header.Set("Referer", "http://music.migu.cn/")
	case "qq":
		req.Header.Set("Referer", "http://y.qq.com")
	}
	if strings.TrimSpace(cookie) != "" {
		req.Header.Set("Cookie", cookie)
	}
	utils.WithRandomIPHeader()(req)
}

func embedSongMetadata(audioData []byte, ext string, song *model.Song, lyric string, coverData []byte, coverMime string) ([]byte, error) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		return nil, errFFmpegNotFound
	}

	inFile, err := os.CreateTemp("", "music-tui-in-*."+ext)
	if err != nil {
		return nil, err
	}
	inPath := inFile.Name()
	defer os.Remove(inPath)
	if _, err := inFile.Write(audioData); err != nil {
		inFile.Close()
		return nil, err
	}
	if err := inFile.Close(); err != nil {
		return nil, err
	}

	outFile, err := os.CreateTemp("", "music-tui-out-*."+ext)
	if err != nil {
		return nil, err
	}
	outPath := outFile.Name()
	if err := outFile.Close(); err != nil {
		return nil, err
	}
	defer os.Remove(outPath)

	args := []string{"-y", "-hide_banner", "-loglevel", "error", "-i", inPath}
	hasCover := len(coverData) > 0
	coverPath := ""
	if hasCover {
		coverExt := ".jpg"
		if strings.Contains(strings.ToLower(coverMime), "png") {
			coverExt = ".png"
		}
		coverFile, err := os.CreateTemp("", "music-tui-cover-*"+coverExt)
		if err != nil {
			return nil, err
		}
		coverPath = coverFile.Name()
		defer os.Remove(coverPath)
		if _, err := coverFile.Write(coverData); err != nil {
			coverFile.Close()
			return nil, err
		}
		if err := coverFile.Close(); err != nil {
			return nil, err
		}
		args = append(args, "-i", coverPath)
	}

	if hasCover {
		args = append(args, "-map", "0:a:0", "-map", "1:v:0", "-c:a", "copy", "-c:v", "copy", "-disposition:v:0", "attached_pic", "-metadata:s:v:0", "title=Album cover", "-metadata:s:v:0", "comment=Cover (front)")
	} else {
		args = append(args, "-map", "0", "-c", "copy")
	}

	if song != nil {
		if song.Name != "" {
			args = append(args, "-metadata", "title="+song.Name)
		}
		if song.Artist != "" {
			args = append(args, "-metadata", "artist="+song.Artist)
		}
		if song.Album != "" {
			args = append(args, "-metadata", "album="+song.Album)
		}
	}
	if strings.TrimSpace(lyric) != "" {
		args = append(args, "-metadata", "lyrics="+lyric)
	}
	if ext == "mp3" {
		args = append(args, "-id3v2_version", "3", "-write_id3v1", "1")
	}
	args = append(args, outPath)

	cmd := exec.Command("ffmpeg", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("ffmpeg metadata embed failed: %v, output: %s", err, strings.TrimSpace(string(output)))
	}

	finalData, err := os.ReadFile(filepath.Clean(outPath))
	if err != nil {
		return nil, err
	}
	if len(finalData) == 0 {
		return nil, errors.New("embedded output is empty")
	}
	return finalData, nil
}

func detectAudioExt(data []byte) string {
	if ext := detectAudioExtBySignature(data); ext != "" {
		return ext
	}
	return "mp3"
}

func detectAudioExtBySignature(data []byte) string {
	switch {
	case len(data) >= 16 && bytes.Equal(data[:16], []byte{0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C}):
		return "wma"
	case len(data) >= 4 && bytes.Equal(data[:4], []byte{'f', 'L', 'a', 'C'}):
		return "flac"
	case len(data) >= 3 && bytes.Equal(data[:3], []byte{'I', 'D', '3'}):
		return "mp3"
	case len(data) >= 2 && data[0] == 0xFF && (data[1]&0xE0) == 0xE0:
		return "mp3"
	case len(data) >= 4 && bytes.Equal(data[:4], []byte{'O', 'g', 'g', 'S'}):
		return "ogg"
	case len(data) >= 12 && bytes.Equal(data[4:8], []byte{'f', 't', 'y', 'p'}):
		return "m4a"
	default:
		return ""
	}
}

func detectAudioExtByContentType(contentType string) string {
	switch strings.ToLower(strings.TrimSpace(contentType)) {
	case "audio/flac", "audio/x-flac":
		return "flac"
	case "audio/x-ms-wma", "audio/wma", "video/x-ms-asf", "application/vnd.ms-asf":
		return "wma"
	case "audio/mpeg", "audio/mp3", "audio/x-mp3":
		return "mp3"
	case "audio/ogg", "application/ogg":
		return "ogg"
	case "audio/mp4", "audio/x-m4a", "audio/aac", "audio/aacp":
		return "m4a"
	default:
		return ""
	}
}

func mapSongs(songs []model.Song) []songDTO {
	result := make([]songDTO, 0, len(songs))
	for _, song := range songs {
		result = append(result, songDTO{
			ID:       song.ID,
			Name:     song.Name,
			Artist:   song.Artist,
			Album:    song.Album,
			AlbumID:  song.AlbumID,
			Duration: song.Duration,
			Size:     song.Size,
			Bitrate:  song.Bitrate,
			Source:   song.Source,
			Ext:      song.Ext,
			Cover:    song.Cover,
			Link:     song.Link,
			Extra:    song.Extra,
			IsVIP:    song.IsVIP,
		})
	}
	return result
}

func writeJSON(v any) {
	enc := json.NewEncoder(os.Stdout)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		fail(err)
	}
}

func fail(err error) {
	_ = json.NewEncoder(os.Stderr).Encode(map[string]string{"error": err.Error()})
	os.Exit(1)
}
