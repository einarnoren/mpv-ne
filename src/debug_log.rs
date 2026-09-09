//! In-memory ring buffer of recent debug log lines, feeding the in-app Log
//! Viewer window (see `ui::log_viewer`). Always collected - cheap and
//! bounded - so the viewer works without any settings change or restart,
//! unlike the file-based "Debug logging" setting.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

const MAX_LINES: usize = 2000;

static BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn buffer() -> &'static Mutex<VecDeque<String>> {
    BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_LINES)))
}

/// `tracing_subscriber` writer that appends complete lines to the ring
/// buffer. `Write::write` can be called with partial lines (or several
/// lines at once), so incomplete text is held until a newline arrives.
#[derive(Default)]
pub struct RingWriter {
    partial: String,
}

impl std::io::Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.partial.push_str(&String::from_utf8_lossy(buf));
        while let Some(pos) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=pos).collect();
            push_line(line.trim_end().to_string());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn push_line(line: String) {
    let mut buf = buffer().lock().unwrap();
    if buf.len() >= MAX_LINES {
        buf.pop_front();
    }
    buf.push_back(line);
}

/// Snapshot of the currently buffered lines, oldest first.
pub fn snapshot() -> Vec<String> {
    buffer().lock().unwrap().iter().cloned().collect()
}

pub fn clear() {
    buffer().lock().unwrap().clear();
}
