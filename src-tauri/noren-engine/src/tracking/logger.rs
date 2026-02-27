use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A single edit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditEntry {
    pub ts: String,
    pub ctx: String,
    pub orig: String,
    pub edit: String,
    pub app: String,
}

/// Append-only JSONL edit logger
pub struct EditLogger {
    log_dir: PathBuf,
}

impl EditLogger {
    pub fn new(base_dir: &Path) -> Self {
        let log_dir = base_dir.join("edit-log");
        let _ = fs::create_dir_all(&log_dir);
        Self { log_dir }
    }

    /// Log an edit entry to today's JSONL file
    pub fn log(&self, entry: &EditEntry) -> Result<(), std::io::Error> {
        let date = &entry.ts[..10]; // YYYY-MM-DD
        let path = self.log_dir.join(format!("{}.jsonl", date));

        let line = serde_json::to_string(entry)? + "\n";

        // Atomic-ish append: open with append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    /// Read all entries from the log directory
    pub fn read_all(&self) -> Vec<EditEntry> {
        let mut entries = Vec::new();

        let files = match fs::read_dir(&self.log_dir) {
            Ok(f) => f,
            Err(_) => return entries,
        };

        for file in files.flatten() {
            let path = file.path();
            if path.extension().map_or(false, |ext| ext == "jsonl") {
                if let Ok(content) = fs::read_to_string(&path) {
                    for line in content.lines() {
                        if let Ok(entry) = serde_json::from_str::<EditEntry>(line) {
                            entries.push(entry);
                        }
                    }
                }
            }
        }

        entries.sort_by(|a, b| a.ts.cmp(&b.ts));
        entries
    }

    /// Read entries from the last N days
    pub fn read_recent(&self, days: u32) -> Vec<EditEntry> {
        let cutoff = chrono_minus_days(days);
        self.read_all()
            .into_iter()
            .filter(|e| e.ts >= cutoff)
            .collect()
    }
}

/// Simple date math without chrono dependency
fn chrono_minus_days(days: u32) -> String {
    // Get current time and subtract days in seconds
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let target = now - (days as u64 * 86400);

    // Convert back to date string (basic implementation)
    let days_since_epoch = target / 86400;
    let mut y = 1970i64;
    let mut remaining = days_since_epoch as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for (i, &d) in month_days.iter().enumerate() {
        if remaining < d as i64 {
            m = i + 1;
            break;
        }
        remaining -= d as i64;
    }

    format!("{:04}-{:02}-{:02}", y, m, remaining + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
