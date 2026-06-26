use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc;

/// A single streamed chunk from the LLM.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamChunk {
    pub text: String,
    pub done: bool,
}

#[async_trait]
pub trait VisionLlm: Send + Sync {
    async fn query(
        &self,
        png_bytes: &[u8],
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Shared SSE helpers
// ---------------------------------------------------------------------------

async fn send_chunk(tx: &mpsc::Sender<StreamChunk>, text: &str) {
    let _ = tx
        .send(StreamChunk {
            text: text.to_string(),
            done: false,
        })
        .await;
}

async fn send_done(tx: &mpsc::Sender<StreamChunk>) {
    let _ = tx
        .send(StreamChunk {
            text: String::new(),
            done: true,
        })
        .await;
}

// ---------------------------------------------------------------------------
// Anthropic Claude
// ---------------------------------------------------------------------------

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "claude-sonnet-4-6".to_string(),
            client: Client::new(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[async_trait]
impl VisionLlm for AnthropicProvider {
    async fn query(
        &self,
        png_bytes: &[u8],
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), String> {
        let b64 = STANDARD.encode(png_bytes);

        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "stream": true,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": b64
                        }
                    },
                    { "type": "text", "text": prompt }
                ]
            }]
        });

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Claude API error {status}: {text}"));
        }

        let body = response.text().await.map_err(|e| format!("Read error: {e}"))?;

        for line in body.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    // Claude SSE: {"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}
                    if let Some(text) = val
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        send_chunk(&tx, text).await;
                    }
                }
            }
        }

        send_done(&tx).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// OpenAI GPT-4o / GPT-4-vision
// ---------------------------------------------------------------------------

pub struct OpenAiProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            client: Client::new(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the base URL — supports Azure OpenAI, Ollama, LM Studio, etc.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        // Trim trailing slash so we can always append /chat/completions cleanly
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl VisionLlm for OpenAiProvider {
    async fn query(
        &self,
        png_bytes: &[u8],
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), String> {
        let b64 = STANDARD.encode(png_bytes);
        // OpenAI expects a data URL: "data:<media_type>;base64,<data>"
        let data_url = format!("data:image/png;base64,{b64}");

        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "stream": true,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": { "url": data_url, "detail": "auto" }
                    },
                    { "type": "text", "text": prompt }
                ]
            }]
        });

        let endpoint = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error {status}: {text}"));
        }

        let body = response.text().await.map_err(|e| format!("Read error: {e}"))?;

        for line in body.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    // OpenAI SSE: {"choices":[{"delta":{"content":"..."}}]}
                    if let Some(text) = val
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|t| t.as_str())
                    {
                        send_chunk(&tx, text).await;
                    }
                }
            }
        }

        send_done(&tx).await;
        Ok(())
    }
}
