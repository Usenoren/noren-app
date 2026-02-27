use noren_engine::extraction::client::{ExtractionClient, StubExtractionClient};

#[tauri::command]
pub async fn run_extraction(
    samples: String,
    format: String,
) -> Result<String, String> {
    let client = StubExtractionClient;
    client
        .extract(&samples, &format)
        .await
        .map(|_| "Extraction complete".to_string())
        .map_err(|e| e.to_string())
}
