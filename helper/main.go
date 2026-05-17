package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/guohuiyuan/go-music-dl/core"
	"github.com/guohuiyuan/music-lib/model"
)

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

func main() {
	if len(os.Args) < 2 {
		fail(errors.New("usage: music-dl-helper <search|download|lyric|sources> [flags]"))
	}

	core.CM.Load()

	switch os.Args[1] {
	case "search":
		searchCmd(os.Args[2:])
	case "download":
		downloadCmd(os.Args[2:])
	case "lyric":
		lyricCmd(os.Args[2:])
	case "sources":
		sourcesCmd()
	default:
		fail(fmt.Errorf("unknown command: %s", os.Args[1]))
	}
}

func searchCmd(args []string) {
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
		sources = core.GetDefaultSourceNames()
	}

	var songs []model.Song
	var mu sync.Mutex

	if strings.HasPrefix(query, "http://") || strings.HasPrefix(query, "https://") {
		source := core.DetectSource(query)
		if source == "" {
			fail(errors.New("unsupported link source"))
		}
		if parseFn := core.GetParseFunc(source); parseFn != nil {
			if song, err := parseFn(query); err == nil && song != nil {
				song.Source = source
				songs = append(songs, *song)
			}
		}
		if len(songs) == 0 {
			if parsePlaylistFn := core.GetParsePlaylistFunc(source); parsePlaylistFn != nil {
				if _, parsedSongs, err := parsePlaylistFn(query); err == nil {
					for i := range parsedSongs {
						if parsedSongs[i].Source == "" {
							parsedSongs[i].Source = source
						}
					}
					songs = append(songs, parsedSongs...)
				}
			}
		}
		if len(songs) == 0 {
			if parseAlbumFn := core.GetParseAlbumFunc(source); parseAlbumFn != nil {
				if _, parsedSongs, err := parseAlbumFn(query); err == nil {
					for i := range parsedSongs {
						if parsedSongs[i].Source == "" {
							parsedSongs[i].Source = source
						}
					}
					songs = append(songs, parsedSongs...)
				}
			}
		}
	} else {
		var wg sync.WaitGroup
		for _, source := range sources {
			searchFn := core.GetSearchFunc(source)
			if searchFn == nil {
				continue
			}
			wg.Add(1)
			go func(src string, fn core.SearchFunc) {
				defer wg.Done()
				result, err := fn(query)
				if err != nil {
					return
				}
				for i := range result {
					result[i].Source = src
				}
				mu.Lock()
				songs = append(songs, result...)
				mu.Unlock()
			}(source, searchFn)
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

func downloadCmd(args []string) {
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

	result, err := core.SaveSongToFile(song, strings.TrimSpace(*outDir), *withCover, *withLyrics)
	if err != nil {
		fail(err)
	}

	lyricPath, lyricWarning := saveLyricFile(song, result.SavedPath, *withLyrics)
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

func lyricCmd(args []string) {
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
	lyricFn := core.GetLyricFunc(song.Source)
	if lyricFn == nil {
		fail(errors.New("lyric unsupported for source"))
	}
	lyric, err := lyricFn(song)
	if err != nil {
		fail(err)
	}
	writeJSON(map[string]string{"lyric": lyric})
}

func saveLyricFile(song *model.Song, audioPath string, enabled bool) (string, string) {
	if !enabled {
		return "", ""
	}
	if strings.TrimSpace(audioPath) == "" {
		return "", "lyric skipped: empty saved path"
	}

	lyricFn := core.GetLyricFunc(song.Source)
	if lyricFn == nil {
		return "", "lyric unsupported for source"
	}

	lyric, err := lyricFn(song)
	if err != nil {
		return "", "lyric fetch failed: " + err.Error()
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
	sources := core.GetAllSourceNames()
	items := make([]sourceDTO, 0, len(sources))
	for _, source := range sources {
		items = append(items, sourceDTO{ID: source, Name: core.GetSourceDescription(source)})
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
