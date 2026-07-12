# mpv-ne

A clean, minimal video player built on **libmpv** + **iced** (Rust),
with a northern lights colour theme. Deep darks and aurora accents,
inspired by [PotPlayer](https://potplayer.tv/).

![screenshot placeholder](assets/MPV_NE_logo_hires.png)

---

## Features

- Hardware-accelerated video via libmpv (H.264, H.265/HEVC, AV1, VP9, …)
- Custom dark UI - no OS chrome required
- **Focus mode** - hide all chrome with `H`, leaving only the video. Controls and top bar fade in on hover
- **Picture-in-picture mode** - shrink to a small always-on-top corner window
- Seekbar thumbnail scrub preview (generated in background, no ffmpeg required)
- **Live/growing file support** - designed for active recordings still being written to disk; `End` key jumps to the live edge with automatic catch-up, and playback resumes automatically as new content is buffered. Works with any format mpv can stream (MKV, TS, fragmented MP4, …)
- **Network streams** - paste a URL (direct link, RTSP/HLS, or a YouTube/Twitch/etc. link) via Open URL; yt-dlp is fetched automatically the first time it's needed
- Playlist with sort, shuffle, save/load (.m3u); file browser (with back/forward history) & recent files panels, including recently opened streams
- Right-click context menu on the video, with a matching floating main menu
- Video equaliser (brightness, contrast, saturation, hue, gamma)
- Audio normalizer, per-file remembered volume/audio-track/subtitle-track
- Subtitle search via OpenSubtitles.com
- A-B loop, chapter navigation, named bookmarks
- Stats overlay (bitrate, fps, dropped frames, buffer)
- Snap-to-screen-edge window behaviour; remembered window position, validated against connected monitors
- Resume playback from last position
- Private/no-trace mode - suppresses all of the above history for the session
- And more - see the Settings panel

---

## Platform support

| Platform | Status |
|----------|--------|
| Windows 10/11 | ✅ Tested |
| Linux | 🔧 Untested - code is structured for it, needs testing |
| macOS | 🔧 Untested - code is structured for it, needs testing |

Windows-specific features (snap-to-edge, window subclassing) are `#[cfg(target_os = "windows")]` guarded and degrade gracefully on other platforms.

## Building from source

### Prerequisites

| Tool | Where to get |
|------|-------------|
| Rust (stable) | https://rustup.rs |
| mpv import library (`mpv.lib`) | See below |
| libmpv DLL (`libmpv-2.dll`) | See below |

### libmpv

`mpv-lib/` contains the Windows import library stub (`mpv.lib`, 14 KB) - no mpv
source code, just symbol names. This is the same file mpv distributes in their
own dev packages for linking purposes.

You only need the **runtime DLL** (`libmpv-2.dll`). The build script copies it
automatically if found at one of the common locations, or set `MPV_DLL_DIR`:

```powershell
$env:MPV_DLL_DIR = "C:\path\to\folder\containing\libmpv-2.dll"
cargo build --release
```

**Windows** - get `libmpv-2.dll` from:
- **mpv.net** - https://github.com/mpvnet-player/mpv.net/releases
- **shinchiro's builds** - https://github.com/shinchiro/mpv-winbuild-cmake/releases  
  (`mpv-dev-x86_64-*.7z` → extract `libmpv-2.dll`)

**Linux** - install via package manager:
```sh
sudo apt install libmpv-dev   # Debian/Ubuntu
sudo pacman -S mpv            # Arch
```

**macOS** - install via Homebrew:
```sh
brew install mpv
```

### Build & run

```powershell
cargo run --release
```

---

## Project structure

```
src/
  app.rs          - application state & message handling
  player.rs       - libmpv wrapper
  ui/             - iced UI modules (controls, panels, …)
  thumbnail.rs    - seekbar thumbnail generation via libmpv
  opensubs.rs     - OpenSubtitles.com API client
  resume.rs       - resume position & metadata cache
  settings.rs     - persistent settings (TOML)
assets/           - icons, logo
mpv-lib/          - mpv.lib import library (not included, see above)
```

---

## Roadmap

- Settings panel full customisability (choose what appears, reorder sections)
- Colour themes based on Nordic seasons
- Visual effects and animations
- Customisable button layout
- Mini player mode + audio spectrum visualizer (built, not yet merged)
- Pan and scan with mouse drag
- Jump to next / previous subtitle
- Secondary subtitle track
- Speed step customisation
- Richer playlist formats (import .pls/.m3u/etc., not just export)
- File associations (register as default player)
- Frame-by-frame stepping
- Configurable seek step size
- In-app keyboard shortcut remapping
- Minimize to system tray
- Linux and macOS support (untested)

## Version

0.3.5 - see [CHANGELOG](CHANGELOG.md)

## Licence

MIT
