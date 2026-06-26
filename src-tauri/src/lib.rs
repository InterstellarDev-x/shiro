mod capture;
mod llm;

use llm::{AnthropicProvider, OpenAiProvider, StreamChunk, VisionLlm};
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub llm: Arc<dyn VisionLlm>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn capture_and_query(
    prompt: String,
    channel: tauri::ipc::Channel<StreamChunk>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let png_bytes = tokio::task::spawn_blocking(capture::capture_primary_screen)
        .await
        .map_err(|e| format!("Capture task panicked: {e}"))??;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

    let llm = {
        let state = state.lock().await;
        state.llm.clone()
    };

    let llm_task = tokio::spawn(async move { llm.query(&png_bytes, &prompt, tx).await });

    while let Some(chunk) = rx.recv().await {
        channel
            .send(chunk)
            .map_err(|e| format!("Channel send failed: {e}"))?;
    }

    llm_task
        .await
        .map_err(|e| format!("LLM task panicked: {e}"))??;

    Ok(())
}

/// Switch the active provider at runtime. provider: "anthropic" | "openai"
#[tauri::command]
async fn set_provider(
    provider: String,
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let new_llm: Arc<dyn VisionLlm> = match provider.as_str() {
        "anthropic" => {
            let mut p = AnthropicProvider::new(api_key);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Arc::new(p)
        }
        "openai" => {
            let mut p = OpenAiProvider::new(api_key);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            if let Some(url) = base_url {
                if !url.is_empty() {
                    p = p.with_base_url(url);
                }
            }
            Arc::new(p)
        }
        other => return Err(format!("Unknown provider: {other}")),
    };

    state.lock().await.llm = new_llm;
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Default to Anthropic if ANTHROPIC_API_KEY is set, otherwise OpenAI
    let llm: Arc<dyn VisionLlm> = if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        Arc::new(AnthropicProvider::new(key))
    } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        let mut p = OpenAiProvider::new(key);
        if let Ok(url) = std::env::var("OPENAI_BASE_URL") {
            if !url.is_empty() {
                p = p.with_base_url(url);
            }
        }
        Arc::new(p)
    } else {
        // No key yet — default to Anthropic, user will set via set_provider
        Arc::new(AnthropicProvider::new(""))
    };

    let state = Mutex::new(AppState { llm });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            capture_and_query,
            set_provider
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
