# Changelog — MPV-NE Nordic Edition

All notable changes to this project, documented retrospectively from the initial
development session. The project was built from scratch in a single extended
session, so this represents the full feature history.

---

## [0.4.0] — 2026-07-16

The settings rework release - a new standalone Settings window, persisted
settings rebuilt to be extensible, keyboard remapping, an audio equaliser,
playlist URL support, and a pass on UI stutter during playback.

### Settings window
- New standalone **Settings** window (Interface / Keyboard), separate from
  the playback-focused settings panel docked next to the video - matches the
  main window's chrome, has its own taskbar icon, remembers position/size.
- **Keyboard remapping** - click a binding, press the new key; reset one
  action or all of them; conflicts are resolved by freeing the key from
  whichever action previously held it.
- Interface toggles: resume playback, window edge/sibling-window snapping,
  drag-window-from-anywhere, remember window position/size, start pinned,
  OSD notifications, seekbar thumbnail preview, custom title bar (OS vs.
  app-drawn - requires restart), auto-update yt-dlp, hide all windows when
  minimized, pause on focus loss, pause on minimize, auto-load sibling files
  into the playlist, and an opt-in single-instance mode (Windows).
- The docked panel's settings tab is now playback-only and labeled
  accordingly, to stay distinct from the new window.

### Persisted settings
- Rewritten from one flat struct to nested, versioned sections
  (window/playback/audio/subtitles/streaming/interface/keybindings), each
  independently defaulted - a config from an older version, or one missing
  a field a newer version added, loads cleanly instead of erroring. Old
  flat-format configs are migrated automatically on first load.

### Audio
- 10-band graphic equaliser (own frequency set), alongside the existing
  video equaliser.

### Playlist
- Add a URL/stream directly to the playlist; its real title, duration, and
  uploader resolve in the background via yt-dlp and replace the raw URL
  once known.
- Bookmark markers on the seek bar (chapters already had them).
- Fixed: loading a saved `.m3u` playlist silently dropped any URL entries
  (they were being filtered by a local-file-existence check that a URL can
  never pass).
- Fixed: switching to a URL playlist entry sometimes loaded but never
  rendered a frame until pause was toggled twice - mpv's own internal
  `pause` property wasn't being explicitly cleared on file/URL open, only
  our own tracked flag was.

### Single instance (Windows, opt-in)
- When enabled, opening a second file hands it to the already-running
  window (via a named mutex + window message) instead of starting a new
  process, and brings that window to the foreground.

### Performance
- Fixed significant UI stutter during playback, worst with the Settings
  window or main menu open while moving the mouse. Root causes: several
  high-frequency event sources (video frame delivery, mpv's `time-pos`/
  `demuxer-cache-time` property updates, cursor movement) each forced a
  full rebuild of the *entire* UI, panels included, and cursor-move and
  frame-delivery rebuilds weren't rate-matched so they compounded. Fixed by
  throttling all of the above to sane, still-smooth rates, and by
  memoizing the three heaviest panels (playback settings, the new Settings
  window, the main menu) so they only rebuild when something they actually
  display changes rather than on every video frame.

### Other
- Right-click paste in text input fields (Open URL, etc.) - previously only
  Ctrl+V worked.
- Scrolling inside a panel no longer also adjusts volume.

## [0.3.5] — 2026-07-12

### Network Streaming
- **Open URL / stream playback fixed end-to-end** — direct links, RTSP/HLS,
  and YouTube/Twitch/etc. links via yt-dlp (fetched automatically the first
  time it's needed). Fixed three stacked bugs: a dead message handler that
  never fired, an unbounded stream quality default that could select
  unplayable AV1 4K streams, and a missing internal state update that kept
  the video view on the idle placeholder even while frames were decoding.
- **Stream quality selector** (480p through 4K, or Best/uncapped), remembered
  in Settings.
- **LIVE badge** on the transport bar for streams confirmed to be growing.
- Recently opened streams now show up in the Recent panel alongside files.
- mpv's own internal log messages are now forwarded into the app's log
  output, which made the above diagnosable in the first place.

### Settings Panel
- Reorganized into labeled categories (Playback, Audio, Subtitles, Video,
  Playback control, Other); larger section headings; clarified "Audio sync"
  vs "Subtitle sync" wording (previously easy to confuse).
- Added controls that existed elsewhere in the app but were missing from
  Settings: audio/subtitle track pickers, mute, subtitle visibility, frame
  fit cycling, window size presets, audio normalizer, preferred
  audio/subtitle language, and a precise-vs-fast seek toggle.

### Side Panel
- The docked Playlist/Browser/Recent/Settings panel can now be detached into
  its own resizable window, matching the main window's chrome. Its position
  and size are remembered between detach/reattach.
- File browser gained Explorer-style Back/Forward navigation.

### Other
- Fixed icon contrast on activated toggle buttons.
- Main menu no longer overlaps or renders off-screen on smaller displays.

## [0.3.0] — 2026-07-08

### Main Menu
- **Floating main menu** — hamburger button in the top bar and right-click on
  the video both open the same menu (File / Playback / Video / Audio /
  Subtitles / Window / App). Implemented as a genuine separate OS window
  (not an in-window overlay), so it can extend past the main window's edges
  like a native context menu. Required switching the app from iced's
  `application` to `daemon` builder for per-window view content.

### Window Behaviour
- **Window-to-window snap** — dragging one MPV-NE window near another now
  snaps to it, like PotPlayer with multiple instances open: abutting (edge
  touches the other window's opposite edge) and aligning (edge matches the
  other window's same edge), whichever is closer. Corner-to-corner catches
  too. Siblings are identified by window class name, no process queries.
- Fixed a jitter bug in the snap engine: once an edge snapped, its target was
  recomputed fresh every frame, so near a corner (where an abut and an align
  candidate can both be in range at once) the position could silently flip
  between them without a proper release. The target now freezes at the
  moment it snaps.
- Fixed `WindowMoved` saving Windows' off-screen sentinel position
  (~-32000, -32000, reported while minimized) as if it were real, which made
  the window unreachable on the next launch after being minimized.
- **Resize OSD** — shows the rendered video's actual pixel size on resize,
  like other action toasts.

### DPI / HiDPI Correctness
- Window sizing, `settings.toml` persistence, the resize OSD, and the actual
  mpv video render resolution are now all consistently physical-pixel-aware
  via a live-queried OS scale factor (matching the pattern the Fit menu
  already used correctly). Previously the video could silently render at a
  lower resolution than the display on any scaled (>100%) monitor, and saved
  window sizes could drift wrong across sessions.

### Fixes
- Live-edge polling now detects after 3 stalled duration checks that a file
  isn't actually growing/live, and stops poking `play()` every 2s forever.
- Seek-bar hover thumbnail no longer appears across the whole control row —
  only when actually hovering the timeline bar itself.
- Scrollable panels (Settings, Playlist, Recent, Browser, subtitle search)
  no longer snap back to the top mid-scroll — they were missing stable
  widget ids, so periodic background ticks (stats refresh, file-size
  polling) could reset scroll position.
- Screenshot filenames now include the source filename and playback
  timestamp instead of mpv's generic `mpv-shot0001.jpg`.

---

## [0.2.0] — 2026-06-29

### Playback & View
- **Stats overlay** (`S` key) — top-right panel showing resolution, video/audio
  codec and bitrate, container/measured fps, dropped frames, A/V sync, demux
  buffer-ahead, and decode mode. Polled twice a second only while visible.
- **Frame fit cycle** (`Z` key) — cycle Fit (letterbox), Fill (crop to cover),
  and Stretch (distort to fill). Applied in the render shader; resets per file.

### UI
- The panels button now toggles the last-used side panel directly (switch
  between Playlist / Browser / Recent / Settings via the tab bar) instead of
  opening a picker popup.

### Live / Growing File Support
- **Full duration shown on load** — a lightweight container byte-rate probe scans
  cluster boundaries at the front and tail of the file, measures the byte-rate
  across nearly the whole file, and extrapolates the true duration. The seekbar
  reflects the real extent of a long recording immediately, without mpv having to
  index the whole file. Falls back to the header `Duration` when present.
- Probed duration acts as a floor so mpv's slowly-climbing forward-index duration
  can no longer pull the seekbar back down (fixes a flicker on load).
- Faster, more reliable live-edge catch-up via an instant `seek 100` plus a
  duration-driven chase to the true edge.
- Broadened from MKV-only to any format mpv can stream (MKV, TS, fragmented MP4, …).

### Per-File Memory
- Remembers audio track, subtitle track, and volume per file.
- Named bookmarks (timestamps) stored per file.

### Window Behaviour
- Reworked snap-to-edge: easier to pull a window off an edge, with no false
  re-snapping on diagonal drags or when sliding along an edge.

### Fixes
- Fixed EBML variable-length integer decoding (it counted leading zeros on a
  widened integer instead of the byte), which had been corrupting all MKV
  cluster/header parsing.
- Suppressed speed/volume OSD messages on startup.

---

## [0.1.0] — Initial Release

### Core Player
- libmpv software render pipeline via `mpv_render_context` SW API
- Frame-accurate video rendering via custom wgpu shader (RGBA texture, aspect-ratio preserving letterbox)
- Hardware decode support (`auto-copy-safe`) with toggle (`I` key)
- Volume control 0–200% (boost above 100%)
- Mute toggle
- Playback speed adjustment (`[` / `]` / `\` to reset)
- Seek bar with precise `absolute+exact` seeking
- Resume playback from last position (persisted to JSON)
- A–B loop repeat (moved to Settings panel)
- Screenshot capture (moved to Settings panel)
- Deinterlace toggle

### UI / Chrome
- Custom title bar (no OS chrome) with drag, pin, focus, minimize, maximize, close
- Dark nordic aesthetic — `BG_DEEPEST` / `BG_SURFACE` / aurora accent colours
- Aurora gradient seekbar (teal → purple active rail, green volume rail)
- Top bar with app icon, title + `[32/51]` playlist counter, help button, focus button, pin button
- Controls bar with responsive breakpoints (buttons hide as window narrows)
- OSD overlay (top-left, brief messages)
- File info OSD on load (resolution × codec × hwdec)
- Focus / chrome-hidden mode (`H` key) — auto-hiding overlays on hover
- Fullscreen (`F` key) — covers taskbar via `AlwaysOnTop`, auto-hiding chrome
- Window snap-to-screen-edge during drag (Win32 `WM_MOVING` subclass, separate snap-in / snap-out thresholds)
- Window position + size persistence across launches
- Double-click title bar = maximize; double-click video = fullscreen
- Escape exits fullscreen / focus mode only, never enters

### Panels System
- Single `⊞` panels button opening a picker popup (replaces 4 separate buttons)
- **Playlist panel** — folder-based playlist, chapter list, file metadata (size / duration / resolution), sort (A-Z / Z-A / size / date), shuffle, save/load `.m3u`, drag-to-append
- **File Browser panel** — filesystem navigation, drive listing, file metadata
- **Recent Files panel** — last 30 opened files with metadata, clear button, last-played age
- **Settings panel** — speed, loop, deinterlace, resume, HW decode picker, subtitle appearance, aspect ratio presets, video zoom, EQ, rotate/flip, after-playback action, AB repeat, jump-to-time, open URL, screenshot folder
- Panel close button (`|◀` chevron icon) in tab bar
- Panels remember open state; window resizes ±280px on open/close (skips in maximized/fullscreen)

### Playlist & Navigation
- Auto-scan folder on file open (all media siblings become playlist)
- Previous / next file (PageUp / PageDown / media keys)
- Playlist jump, remove, shuffle, sort
- Chapter navigation (Ctrl+Left / Ctrl+Right)
- After-playback: do nothing / next file / loop / close player
- Multi-file drag & drop (video area = replace; panel = append)
- Playlist saves/loads `.m3u` files

### Subtitles & Audio
- Subtitle track picker popup
- Audio track picker popup
- Subtitle delay, font size, vertical position controls
- External subtitle file loader
- **OpenSubtitles.com search** — search by title, download and auto-load `.srt`/`.ass`
- Cycle subtitle tracks (`J` key)
- Cycle audio tracks (`#` key)
- Toggle subtitle visibility (`V` key)

### Video Adjustments
- **Video equaliser** — brightness, contrast, saturation, hue, gamma via `lavfi=[eq=...]` filter (no flicker — atomic `vf set`)
- Aspect ratio presets (Auto / 4:3 / 16:9 / 21:9 / 1:1 / 2.35)
- Video zoom slider with 100% snap
- Rotate ↻↺ 90° / H-flip / V-flip (resets on new file)
- Fit-to-visible size popup

### Seekbar Thumbnails
- Background thumbnail generation via secondary libmpv instance (no ffmpeg required)
- Single-pass keyframe seeking (fast; exact pass dropped — quality unchanged)
- 30 thumbnails per file, single sequential worker (low CPU impact)
- Incremental extension for growing files (`spawn_extend`) — new frames added without clearing existing ones; throttled to one dispatch per step interval
- Popup follows cursor along seekbar with timestamp below thumbnail
- Thumbnail cache keyed by generation ID (no stale frames on file change)

### Live / Growing File Support
- `End` key triggers JumpToLive: rapid seek cascade using demuxer readahead to reach the live edge in ~2 seconds regardless of file length
- Auto-resume when paused at the live edge: periodic 2-second poke resumes playback as soon as new content is buffered (works around mpv `keep-open=yes` demuxer stall)
- `eof-reached` property observer for reliable live-edge detection (more reliable than `MPV_EVENT_END_FILE` with `keep-open=yes`)
- Deferred thumbnail regeneration after each JumpToLive with correct step size for current duration

### Keyboard Shortcuts
- Full binding table (`Space`, `F`, `H`, `M`, `[`, `]`, `\`, `J`, `#`, `V`, `I`, `S`, `?`, Ctrl+G, Ctrl+Left/Right, Ctrl+Scroll)
- **Media keys** — Play/Pause, Stop, Next Track, Previous Track
- `?` / `/` — keyboard shortcut help overlay
- Ctrl+G — jump to time (accepts `h:mm:ss` / `m:ss` / seconds)
- Ctrl+Scroll — seek ±5 seconds

### File Info & Metadata
- File size, duration, resolution shown in all file listings
- Duration probed from file headers (MP4, MKV, AVI) without playing
- Resolution cached from playback, persisted to JSON
- Last-played timestamp shown in recent panel
- Background metadata probing (no stutter during playback)

### Settings Persistence
- Window size, position, volume, resume-enabled, screenshot directory — all persisted to `settings.toml`
- Resume DB stores position, duration, resolution, last-played per file (capped at 2000 entries)
- Recent files list (30 max) persisted separately

### Platform / Build
- Windows 10/11 — fully tested
- Linux / macOS — `#[cfg]`-guarded, structured for future support
- `mpv.lib` import stub included (14 KB, no mpv source)
- `build.rs` auto-copies `libmpv-2.dll` from common install locations
- GitHub Actions release workflow (tag → build → zip with DLL → GitHub Release)
- App icon embedded in exe via `winres`; shown in taskbar, Alt+Tab, Explorer

### Win32 Integrations
- Window subclassing via `SetWindowLongPtrW` (no comctl32 v6 required)
- Snap-to-edge during drag (`WM_MOVING`), DPI-aware, separate attract/release thresholds
- Maximized-state detection via `IsZoomed`
- `AlwaysOnTop` window level in fullscreen (covers taskbar)
- Modal loop pause/resume for menu interactions

---

*Built with [Rust](https://rust-lang.org), [iced 0.14](https://github.com/iced-rs/iced), and [libmpv](https://github.com/mpv-player/mpv).*
