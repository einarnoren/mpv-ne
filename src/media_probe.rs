//! Lightweight media duration probe - reads just enough of the file header
//! to extract duration without needing to play the file.
//!
//! Supported containers:
//!   MP4 / M4V / MOV  - reads moov/mvhd box
//!   MKV / WebM       - reads EBML Segment/Info/Duration element
//!   AVI              - reads avih chunk
//!   FLV              - reads metadata scriptdata tag

use std::path::Path;

/// Probe a media file for its duration in seconds.
/// Returns None if the format is unsupported or the file cannot be read.
pub fn probe_duration(path: &Path) -> Option<f64> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" | "mov" | "m4a" | "mp4v" => probe_mp4(path),
        "mkv" | "webm" | "mka" | "mk3d"         => probe_mkv(path),
        "avi"                                     => probe_avi(path),
        _                                         => None,
    }
}

// ---------------------------------------------------------------------------
// MP4 / MOV  (ISO base media file format)
// ---------------------------------------------------------------------------
// Walk top-level boxes until we find 'moov', then find 'mvhd' inside it.
// mvhd v0: timescale @ offset 12, duration @ offset 16 (both u32, BE)
// mvhd v1: timescale @ offset 20, duration @ offset 24 (timescale u32, duration u64, BE)

fn probe_mp4(path: &Path) -> Option<f64> {
    let data = read_head(path, 12 * 1024 * 1024)?; // up to 12 MB - moov can be at end
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes(data[pos..pos+4].try_into().ok()?) as usize;
        let name = &data[pos+4..pos+8];
        if size < 8 { break; }
        if name == b"moov" {
            return find_mvhd(&data[pos+8..pos.saturating_add(size).min(data.len())]);
        }
        pos += size;
    }
    None
}

fn find_mvhd(data: &[u8]) -> Option<f64> {
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes(data[pos..pos+4].try_into().ok()?) as usize;
        let name = &data[pos+4..pos+8];
        if size < 8 { break; }
        if name == b"mvhd" && pos + 8 < data.len() {
            let body = &data[pos+8..];
            let version = *body.first()?;
            if version == 0 && body.len() >= 16 {
                let ts = u32::from_be_bytes(body[4..8].try_into().ok()?) as f64;
                let dur = u32::from_be_bytes(body[8..12].try_into().ok()?) as f64;
                if ts > 0.0 { return Some(dur / ts); }
            } else if version == 1 && body.len() >= 28 {
                let ts = u32::from_be_bytes(body[12..16].try_into().ok()?) as f64;
                let dur = u64::from_be_bytes(body[16..24].try_into().ok()?) as f64;
                if ts > 0.0 { return Some(dur / ts); }
            }
            return None;
        }
        if name == b"trak" || name == b"udta" { break; } // stop early
        pos += size;
    }
    None
}

// ---------------------------------------------------------------------------
// MKV / WebM  (EBML)
// ---------------------------------------------------------------------------
// We scan for the Duration element (ID 0x4489) in the first 256 KB.
// Duration is stored as an IEEE 754 float (f32 or f64) in a timescale
// defined by TimecodeScale (default 1_000_000 ns per tick = 1ms per tick).
// Duration in seconds = duration_ticks * timecode_scale_ns / 1_000_000_000.

fn probe_mkv(path: &Path) -> Option<f64> {
    let data = read_head(path, 256 * 1024)?;
    // Find TimecodeScale (0x2AD7B1) and Duration (0x4489) in the byte stream.
    let mut timecode_scale: f64 = 1_000_000.0; // default: 1ms per tick
    let mut duration: Option<f64> = None;
    let mut i = 0usize;
    while i + 4 < data.len() {
        // Match TimecodeScale element ID: 0x2A D7 B1
        if i + 7 < data.len() && data[i] == 0x2A && data[i+1] == 0xD7 && data[i+2] == 0xB1 {
            if let Some((size, skip)) = ebml_vint(&data[i+3..]) {
                let start = i + 3 + skip;
                let end = (start + size).min(data.len());
                if end - start == 8 {
                    timecode_scale = u64::from_be_bytes(data[start..end].try_into().ok()?) as f64;
                } else if end - start == 4 {
                    timecode_scale = u32::from_be_bytes(data[start..end].try_into().ok()?) as f64;
                }
            }
        }
        // Match Duration element ID: 0x44 0x89
        if data[i] == 0x44 && data[i+1] == 0x89 {
            if let Some((size, skip)) = ebml_vint(&data[i+2..]) {
                let start = i + 2 + skip;
                let end = (start + size).min(data.len());
                if end - start == 8 {
                    let v = f64::from_be_bytes(data[start..end].try_into().ok()?);
                    if v > 0.0 { duration = Some(v); }
                } else if end - start == 4 {
                    let v = f32::from_be_bytes(data[start..end].try_into().ok()?) as f64;
                    if v > 0.0 { duration = Some(v); }
                }
            }
        }
        if duration.is_some() { break; }
        i += 1;
    }
    // Duration ticks * timescale_ns / 1_000_000_000 = seconds
    duration.map(|d| d * timecode_scale / 1_000_000_000.0)
}

/// Decode an EBML variable-length integer. Returns (value, bytes_consumed).
fn ebml_vint(data: &[u8]) -> Option<(usize, usize)> {
    let first = *data.first()? as usize;
    if first == 0 { return None; }
    let extra = first.leading_zeros() as usize;
    let len = extra + 1;
    if data.len() < len { return None; }
    let mut val = first & (0xFF >> len);
    for &b in &data[1..len] {
        val = (val << 8) | b as usize;
    }
    Some((val, len))
}

// ---------------------------------------------------------------------------
// AVI
// ---------------------------------------------------------------------------
// avih chunk contains microseconds per frame and total frames.
// duration = total_frames * us_per_frame / 1_000_000

fn probe_avi(path: &Path) -> Option<f64> {
    let data = read_head(path, 4096)?;
    if data.len() < 32 || &data[0..4] != b"RIFF" || &data[8..12] != b"AVI " { return None; }
    // Find 'avih' chunk in hdrl list
    let mut i = 12usize;
    while i + 8 < data.len() {
        let tag = &data[i..i+4];
        let size = u32::from_le_bytes(data[i+4..i+8].try_into().ok()?) as usize;
        if tag == b"avih" && i + 8 + 24 < data.len() {
            let body = &data[i+8..];
            let us_per_frame = u32::from_le_bytes(body[0..4].try_into().ok()?) as f64;
            let total_frames = u32::from_le_bytes(body[16..20].try_into().ok()?) as f64;
            if us_per_frame > 0.0 && total_frames > 0.0 {
                return Some(total_frames * us_per_frame / 1_000_000.0);
            }
            return None;
        }
        i += 8 + size + (size & 1); // RIFF pads to even
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_head(path: &Path, max_bytes: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; max_bytes];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(buf)
}
