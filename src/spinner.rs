//! Animated "thinking" spinner shown on stderr while a provider call is in
//! flight. Stays out of stdout so `$(plz …)` capture remains clean.
//!
//! `Spinner::start` spawns a background thread that redraws the spinner
//! frame in place every ~80ms; `Spinner::stop` signals the thread to exit
//! and clears the line. If stderr is not a TTY (CI logs, piped output) the
//! spinner falls back to a single static line so progress is still visible
//! without leaving cursor-control escape codes in a log file.

use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const FRAMES: &[&str] = &[
    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
];
const FRAME_MS: u64 = 80;

pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Start an animated spinner labelled `<label>:` on stderr. The label is
    /// usually the model ID being queried (e.g. `claude-haiku-4-5`).
    pub fn start(label: impl Into<String>) -> Self {
        let label = label.into();
        let stop = Arc::new(AtomicBool::new(false));

        if !io::stderr().is_terminal() {
            // No TTY — emit a single static line and skip the redraw loop so
            // log files don't get filled with `\r` and ANSI clears.
            eprintln!("\x1b[2m· {label}…\x1b[0m");
            return Self {
                stop,
                handle: None,
            };
        }

        let stop_c = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut i: usize = 0;
            while !stop_c.load(Ordering::Relaxed) {
                let frame = FRAMES[i % FRAMES.len()];
                let mut e = io::stderr().lock();
                let _ = write!(e, "\r\x1b[2K\x1b[2m{frame} {label}:\x1b[0m");
                let _ = e.flush();
                drop(e);
                thread::sleep(Duration::from_millis(FRAME_MS));
                i = i.wrapping_add(1);
            }
            let mut e = io::stderr().lock();
            let _ = write!(e, "\r\x1b[2K");
            let _ = e.flush();
        });

        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// Stop the spinner thread and clear the line. Safe to call on a TTY-less
    /// spinner — it's a no-op there.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        // If `stop()` wasn't called (e.g. early `?` propagation), make sure
        // the thread exits and the spinner line is cleared before the
        // process unwinds further.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
