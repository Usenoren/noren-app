use std::path::Path;

const MAX_TEXT_LENGTH: usize = 50_000;
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

#[tauri::command]
pub fn read_file_as_text(path: String) -> Result<String, String> {
    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err(format!("File not found: {}", path));
    }

    let metadata = std::fs::metadata(file_path)
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large ({:.1} MB). Maximum size is 10 MB.",
            metadata.len() as f64 / (1024.0 * 1024.0)
        ));
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        "txt" | "md" | "csv" | "json" | "xml" | "html" | "htm" | "log" | "yaml" | "yml"
        | "toml" | "ini" | "cfg" | "conf" | "rst" | "tex" => {
            std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read file: {}", e))?
        }
        "pdf" => extract_pdf_text(file_path)?,
        _ => {
            return Err(format!(
                "Unsupported file type: .{}. Supported: txt, md, csv, json, xml, html, pdf, yaml, toml",
                ext
            ));
        }
    };

    // Truncate to prevent massive context blowup
    if text.len() > MAX_TEXT_LENGTH {
        Ok(format!(
            "{}...\n\n[Truncated — showing first {} of {} characters]",
            &text[..MAX_TEXT_LENGTH],
            MAX_TEXT_LENGTH,
            text.len()
        ))
    } else {
        Ok(text)
    }
}

fn extract_pdf_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read PDF: {}", e))?;
    pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("Failed to extract text from PDF: {}", e))
}
