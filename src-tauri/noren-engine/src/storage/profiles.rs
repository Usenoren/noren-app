use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::EngineError;

/// Save a voice profile to disk.
///
/// Creates:
/// - `{profile_dir}/core-identity.md`
/// - `{profile_dir}/contexts/{format}.md` for each context
/// - `{profile_dir}/quality-check-results.md`
pub fn save_profile(
    profile_dir: &Path,
    core_identity: &str,
    contexts: &HashMap<String, String>,
    quality_check: &str,
) -> Result<PathBuf, EngineError> {
    std::fs::create_dir_all(profile_dir)?;
    let contexts_dir = profile_dir.join("contexts");
    std::fs::create_dir_all(&contexts_dir)?;

    std::fs::write(profile_dir.join("core-identity.md"), core_identity)?;

    for (format, content) in contexts {
        std::fs::write(contexts_dir.join(format!("{}.md", format)), content)?;
    }

    std::fs::write(
        profile_dir.join("quality-check-results.md"),
        quality_check,
    )?;

    Ok(profile_dir.to_path_buf())
}

/// Load a voice profile from disk.
///
/// Returns the core identity and a map of format → context content.
pub fn load_profile(
    profile_dir: &Path,
) -> Result<(String, HashMap<String, String>), EngineError> {
    let core_path = profile_dir.join("core-identity.md");
    if !core_path.exists() {
        return Err(EngineError::Profile(
            "No voice profile found. Create one in Profiles or upgrade to Pro for AI extraction.".to_string()
        ));
    }

    let core_identity = std::fs::read_to_string(&core_path)?;
    let mut contexts = HashMap::new();

    let contexts_dir = profile_dir.join("contexts");
    if contexts_dir.exists() {
        for entry in std::fs::read_dir(&contexts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                let format = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let content = std::fs::read_to_string(&path)?;
                contexts.insert(format, content);
            }
        }
    }

    Ok((core_identity, contexts))
}

/// Load calibration data from a profile directory.
/// Returns None if calibration.json doesn't exist or is unparseable.
pub fn load_calibration(profile_dir: &Path) -> Option<crate::types::CalibrationData> {
    let calibration_path = profile_dir.join("calibration.json");
    if !calibration_path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&calibration_path).ok()?;
    serde_json::from_str(&data).ok()
}

/// List available context formats in a profile directory
pub fn list_formats(profile_dir: &Path) -> Vec<String> {
    let contexts_dir = profile_dir.join("contexts");
    if !contexts_dir.exists() {
        return vec![];
    }

    let mut formats = vec![];
    if let Ok(entries) = std::fs::read_dir(&contexts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Some(stem) = path.file_stem() {
                    formats.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }
    formats.sort();
    formats
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path().join("test-profile");

        let mut contexts = HashMap::new();
        contexts.insert("twitter".to_string(), "Twitter context content".to_string());
        contexts.insert("email".to_string(), "Email context content".to_string());

        save_profile(
            &profile_dir,
            "Core identity content",
            &contexts,
            "Quality check: PASS",
        )
        .unwrap();

        let (core, loaded_contexts) = load_profile(&profile_dir).unwrap();
        assert_eq!(core, "Core identity content");
        assert_eq!(loaded_contexts.len(), 2);
        assert_eq!(loaded_contexts["twitter"], "Twitter context content");
        assert_eq!(loaded_contexts["email"], "Email context content");
    }

    #[test]
    fn load_missing_profile_returns_error() {
        let tmp = TempDir::new().unwrap();
        let result = load_profile(&tmp.path().join("nonexistent"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No voice profile found"));
    }

    #[test]
    fn list_formats_returns_sorted() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path().join("profile");

        let mut contexts = HashMap::new();
        contexts.insert("twitter".to_string(), "t".to_string());
        contexts.insert("email".to_string(), "e".to_string());
        contexts.insert("longform".to_string(), "l".to_string());

        save_profile(&profile_dir, "core", &contexts, "qc").unwrap();

        let formats = list_formats(&profile_dir);
        assert_eq!(formats, vec!["email", "longform", "twitter"]);
    }

    #[test]
    fn list_formats_empty_when_no_contexts() {
        let tmp = TempDir::new().unwrap();
        assert!(list_formats(tmp.path()).is_empty());
    }
}
