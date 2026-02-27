use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::logger::EditEntry;

/// A word replacement pattern (original -> edited)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopReplacement {
    pub original: String,
    pub replacement: String,
    pub count: u32,
}

/// Aggregate edit statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditStats {
    pub total_edits: u32,
    pub edits_last_7_days: u32,
    pub edits_last_30_days: u32,
    pub top_replacements: Vec<TopReplacement>,
    pub avg_edit_distance: f64,
    pub most_active_context: String,
}

impl EditStats {
    /// Compute stats from a set of edit entries
    pub fn from_entries(entries: &[EditEntry]) -> Self {
        let total_edits = entries.len() as u32;

        // Count by recency
        let _now = now_iso();
        let day_7 = days_ago_iso(7);
        let day_30 = days_ago_iso(30);

        let edits_last_7_days = entries.iter().filter(|e| e.ts >= day_7).count() as u32;
        let edits_last_30_days = entries.iter().filter(|e| e.ts >= day_30).count() as u32;

        // Word-level diff for replacements
        let mut replacement_counts: HashMap<(String, String), u32> = HashMap::new();
        let mut total_distance = 0usize;

        for entry in entries {
            let orig_words: Vec<&str> = entry.orig.split_whitespace().collect();
            let edit_words: Vec<&str> = entry.edit.split_whitespace().collect();

            // Simple word-level diff
            let dist = word_edit_distance(&orig_words, &edit_words);
            total_distance += dist;

            // Track word replacements (simple: compare at same positions)
            let min_len = orig_words.len().min(edit_words.len());
            for i in 0..min_len {
                if orig_words[i] != edit_words[i] {
                    let key = (
                        orig_words[i].to_lowercase(),
                        edit_words[i].to_lowercase(),
                    );
                    *replacement_counts.entry(key).or_insert(0) += 1;
                }
            }
        }

        // Top replacements
        let mut replacements: Vec<TopReplacement> = replacement_counts
            .into_iter()
            .map(|((orig, repl), count)| TopReplacement {
                original: orig,
                replacement: repl,
                count,
            })
            .collect();
        replacements.sort_by(|a, b| b.count.cmp(&a.count));
        replacements.truncate(10);

        // Most active context
        let mut context_counts: HashMap<&str, u32> = HashMap::new();
        for entry in entries {
            *context_counts.entry(&entry.ctx).or_insert(0) += 1;
        }
        let most_active_context = context_counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(ctx, _)| ctx.to_string())
            .unwrap_or_default();

        let avg_edit_distance = if entries.is_empty() {
            0.0
        } else {
            total_distance as f64 / entries.len() as f64
        };

        Self {
            total_edits,
            edits_last_7_days,
            edits_last_30_days,
            top_replacements: replacements,
            avg_edit_distance,
            most_active_context,
        }
    }
}

/// Simple word-level edit distance (Levenshtein on word arrays)
fn word_edit_distance(a: &[&str], b: &[&str]) -> usize {
    let m = a.len();
    let n = b.len();

    // Shortcut for very different lengths
    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for j in 0..=n {
        prev[j] = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    epoch_to_date(secs)
}

fn days_ago_iso(days: u32) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    epoch_to_date(secs - (days as u64 * 86400))
}

fn epoch_to_date(secs: u64) -> String {
    let days_since_epoch = secs / 86400;
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
