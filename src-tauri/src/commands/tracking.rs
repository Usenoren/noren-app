use noren_engine::{EditEntry, EditLogger, EditStats};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn log_edit(
    state: State<'_, AppState>,
    context: String,
    original: String,
    edited: String,
    app_name: String,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let base_dir = config
        .profile_dir
        .parent()
        .unwrap_or(&config.profile_dir);

    let logger = EditLogger::new(base_dir);

    let now = now_iso();
    let entry = EditEntry {
        ts: now,
        ctx: context,
        orig: original,
        edit: edited,
        app: app_name,
    };

    logger.log(&entry).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_edit_stats(state: State<'_, AppState>) -> Result<EditStats, String> {
    let config = state.config.lock().unwrap();
    let base_dir = config
        .profile_dir
        .parent()
        .unwrap_or(&config.profile_dir);

    let logger = EditLogger::new(base_dir);
    let entries = logger.read_all();
    Ok(EditStats::from_entries(&entries))
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let mut y = 1970i64;
    let mut remaining = days_since_epoch as i64;

    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut mo = 0;
    for (i, &d) in month_days.iter().enumerate() {
        if remaining < d as i64 {
            mo = i + 1;
            break;
        }
        remaining -= d as i64;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        mo,
        remaining + 1,
        h,
        m,
        s
    )
}
