//! TEMP lag trace: finds what stalls the cursor around track changes.
//! Lines go to %TEMP%\silence-lag-trace.log through a writer thread, so no
//! caller (the mouse hook included) ever touches the file. Remove when done.

use once_cell::sync::Lazy;
use std::{io::Write, sync::Mutex, time::Instant};

static START: Lazy<Instant> = Lazy::new(Instant::now);
static SINK: Lazy<Mutex<Option<flume::Sender<String>>>> = Lazy::new(|| {
    let (sender, receiver) = flume::unbounded::<String>();
    let path = std::env::temp_dir().join("silence-lag-trace.log");
    std::thread::Builder::new()
        .name("silence-lag-trace".into())
        .spawn(move || {
            let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            else {
                return;
            };
            let _ = writeln!(file, "---- start pid {}", std::process::id());
            while let Ok(line) = receiver.recv() {
                let _ = writeln!(file, "{line}");
                let _ = file.flush();
            }
        })
        .ok();
    Mutex::new(Some(sender))
});

pub fn log(line: impl FnOnce() -> String) {
    let at = START.elapsed().as_secs_f64();
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("?").to_string();
    if let Ok(sink) = SINK.try_lock()
        && let Some(sink) = sink.as_ref()
    {
        let _ = sink.send(format!("{at:10.3} [{name}] {}", line()));
    }
}

/// Logs `what` when it took longer than `threshold_ms`.
pub struct Span {
    what: &'static str,
    detail: String,
    started: Instant,
    threshold_ms: f64,
}

impl Span {
    pub fn new(what: &'static str, threshold_ms: f64) -> Self {
        Self::with(what, String::new(), threshold_ms)
    }

    pub fn with(what: &'static str, detail: String, threshold_ms: f64) -> Self {
        Lazy::force(&START);
        Self {
            what,
            detail,
            started: Instant::now(),
            threshold_ms,
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        if ms >= self.threshold_ms {
            let what = self.what;
            let detail = std::mem::take(&mut self.detail);
            log(move || format!("{what} {detail} took {ms:.1}ms"));
        }
    }
}
