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

    let header_dur = duration.map(|d| d * timecode_scale / 1_000_000_000.0);

    // For a growing/recording file the header usually has NO Duration element,
    // and the clusters can be spaced many MB apart (so blind tail-scanning lands
    // mid-cluster and finds nothing). Instead, measure the byte-rate between two
    // cluster boundaries near the front and extrapolate to the full file size.
    let est_dur = probe_mkv_bitrate(path, timecode_scale);

    match (header_dur, est_dur) {
        (Some(h), Some(t)) => Some(h.max(t)),
        (Some(h), None)    => Some(h),
        (None,    t)       => t,
    }
}

/// Estimate the duration of a growing MKV by measuring its byte-rate.
///
/// Collects (byte_offset, timestamp) pairs at Cluster boundaries from a window
/// at the FRONT of the file and a window at the END. The byte-rate between the
/// earliest and latest cluster (spanning nearly the whole file) is then used to
/// extrapolate the remaining bytes after the last cluster:
///
///   duration ≈ ts_last + (file_len - byte_last) / bytes_per_sec
///
/// Because the end window's last cluster sits within ~window bytes of EOF, the
/// extrapolation is tiny and the result tracks the true duration closely. Works
/// for live recordings whose header carries no Duration element.
fn probe_mkv_bitrate(path: &Path, timecode_scale: f64) -> Option<f64> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let file_len = f.seek(SeekFrom::End(0)).ok()?;

    // Scan a window [start, start+size) for Cluster boundaries, appending
    // (absolute_byte_offset, timestamp_ticks) to `out`.
    fn scan_window(
        f: &mut std::fs::File,
        start: u64,
        size: usize,
        out: &mut Vec<(u64, u64)>,
    ) {
        use std::io::{Read, Seek, SeekFrom};
        if f.seek(SeekFrom::Start(start)).is_err() { return; }
        let mut buf = vec![0u8; size];
        let n = match f.read(&mut buf) { Ok(n) => n, Err(_) => return };
        buf.truncate(n);
        let mut i = 0usize;
        while i + 4 < buf.len() {
            if buf[i] == 0x1F && buf[i+1] == 0x43 && buf[i+2] == 0xB6 && buf[i+3] == 0x75 {
                let mut j = i + 4;
                if let Some((_, skip)) = ebml_vint(&buf[j..]) { j += skip; }
                let limit = (j + 128).min(buf.len());
                while j + 2 < limit {
                    if buf[j] == 0xE7 {
                        if let Some((sz, sk)) = ebml_vint(&buf[j+1..]) {
                            let s = j + 1 + sk;
                            let e = (s + sz).min(buf.len());
                            if sz <= 8 && e > s {
                                let mut val = 0u64;
                                for &b in &buf[s..e] { val = (val << 8) | b as u64; }
                                out.push((start + i as u64, val));
                            }
                        }
                        break;
                    }
                    j += 1;
                }
            }
            i += 1;
        }
    }

    let window = 32 * 1024 * 1024u64; // 32 MB per scan
    let mut clusters: Vec<(u64, u64)> = Vec::new();
    scan_window(&mut f, 0, window as usize, &mut clusters);
    if file_len > window {
        let tail_start = file_len - window;
        scan_window(&mut f, tail_start, window as usize, &mut clusters);
    }

    // Earliest and latest cluster by timestamp across both windows.
    clusters.sort_by_key(|&(_, t)| t);

    if clusters.len() < 2 { return None; }
    let (b_first, t_first) = *clusters.first().unwrap();
    let (b_last,  t_last)  = *clusters.last().unwrap();
    if b_last <= b_first || t_last <= t_first { return None; }

    let ticks_to_secs = |ticks: u64| ticks as f64 * timecode_scale / 1_000_000_000.0;
    let secs_first = ticks_to_secs(t_first);
    let secs_last  = ticks_to_secs(t_last);
    let bytes_per_sec = (b_last - b_first) as f64 / (secs_last - secs_first);
    if bytes_per_sec <= 0.0 { return None; }

    let estimated = secs_last + (file_len - b_last) as f64 / bytes_per_sec;
    Some(estimated)
}

/// Decode an EBML variable-length integer. Returns (value, bytes_consumed).
fn ebml_vint(data: &[u8]) -> Option<(usize, usize)> {
    let first = *data.first()?;
    if first == 0 { return None; }
    // The length is encoded by the number of leading zero bits in the FIRST
    // byte (1..=8). Count on the u8 — casting to usize first would count the
    // extra 56 high zero bits and corrupt every decode.
    let len = first.leading_zeros() as usize + 1;
    if data.len() < len { return None; }
    let mut val = (first & (0xFF >> len)) as usize;
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
