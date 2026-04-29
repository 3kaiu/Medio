use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref OP_LOG: Mutex<Option<OpLog>> = Mutex::new(None);
}

struct OpLog {
    path: PathBuf,
    enabled: bool,
}

impl OpLog {
    fn log(&self, action: &str) {
        if !self.enabled {
            return;
        }
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{}] {}\n", timestamp, action);
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// Initialize the operation log. Call once at startup.
pub fn init(enabled: bool) {
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("medio");
    let _ = fs::create_dir_all(&log_dir);
    let path = log_dir.join("operations.log");

    let op_log = OpLog { path, enabled };
    *OP_LOG.lock().unwrap() = Some(op_log);
}

/// Log a file operation action.
pub fn log(action: &str) {
    if let Ok(guard) = OP_LOG.lock()
        && let Some(ref op_log) = *guard
    {
        op_log.log(action);
    }
}
