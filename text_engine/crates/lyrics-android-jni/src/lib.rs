#[macro_use]
extern crate log;

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "android")]
mod android_gpu;
mod jvm;
mod native;

const MAX_DIAGNOSTIC_LINES: usize = 1_024;
const MAX_DIAGNOSTIC_MESSAGE_CHARS: usize = 2_048;
static DIAGNOSTIC_LINES: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn record_diagnostic(level: &str, target: &str, message: &str) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let message = if message.chars().count() > MAX_DIAGNOSTIC_MESSAGE_CHARS {
        let mut truncated = message
            .chars()
            .take(MAX_DIAGNOSTIC_MESSAGE_CHARS)
            .collect::<String>();
        truncated.push_str("…[truncated]");
        truncated
    } else {
        message.to_owned()
    };
    let mut lines = DIAGNOSTIC_LINES
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_DIAGNOSTIC_LINES)))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if lines.len() == MAX_DIAGNOSTIC_LINES {
        lines.pop_front();
    }
    lines.push_back(format!("{timestamp_ms} [{level}] [{target}] {message}"));
}

fn diagnostics_snapshot() -> String {
    DIAGNOSTIC_LINES
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_DIAGNOSTIC_LINES)))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(target_os = "android")]
struct AndroidDiagnosticLogger;

#[cfg(target_os = "android")]
impl log::Log for AndroidDiagnosticLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        record_diagnostic(record.level().as_str(), record.target(), &message);
        android_log_write(record.level(), &message);
    }

    fn flush(&self) {}
}

#[cfg(target_os = "android")]
static ANDROID_LOGGER: AndroidDiagnosticLogger = AndroidDiagnosticLogger;

#[cfg(target_os = "android")]
fn android_log_write(level: log::Level, message: &str) {
    use std::os::raw::{c_char, c_int};
    unsafe extern "C" {
        fn __android_log_print(
            priority: c_int,
            tag: *const c_char,
            format: *const c_char,
            ...
        ) -> c_int;
    }
    let priority = match level {
        log::Level::Error => 6,
        log::Level::Warn => 5,
        log::Level::Info => 4,
        log::Level::Debug => 3,
        log::Level::Trace => 2,
    };
    let Ok(message) = std::ffi::CString::new(message) else {
        return;
    };
    unsafe {
        __android_log_print(
            priority,
            c"FontTower".as_ptr(),
            c"%s".as_ptr(),
            message.as_ptr(),
        );
    }
}

#[cfg(target_os = "android")]
pub fn init_logging() {
    if log::set_logger(&ANDROID_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[cfg(not(target_os = "android"))]
pub fn init_logging() {}
