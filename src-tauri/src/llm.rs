use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
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
// Shared helpers
// ---------------------------------------------------------------------------

async fn send_chunk(tx: &mpsc::Sender<StreamChunk>, text: &str) {
    let _ = tx.send(StreamChunk { text: text.to_string(), done: false }).await;
}

async fn send_done(tx: &mpsc::Sender<StreamChunk>) {
    let _ = tx.send(StreamChunk { text: String::new(), done: true }).await;
}

/// Byte-level line splitter. Appends new_bytes to buf, calls on_line for every
/// complete \n-terminated line, and leaves any trailing partial line in buf.
fn split_lines_bytes(buf: &mut Vec<u8>, new_bytes: &[u8], mut on_line: impl FnMut(&str)) {
    buf.extend_from_slice(new_bytes);
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let end = if pos > 0 && buf[pos - 1] == b'\r' { pos - 1 } else { pos };
        let line = String::from_utf8_lossy(&buf[..end]).into_owned();
        buf.drain(..=pos);
        on_line(&line);
    }
}

/// Try to extract choices[0].delta.content from a JSON fragment that is
/// missing its opening {"id":"chatcmpl- prefix (Vocareum-style split SSE).
fn extract_content_from_fragment(fragment: &str) -> Option<String> {
    // Vocareum splits: `data: {"id":"chatcmpl-XXXX` on one line,
    // then `XXXX","object":...,"choices":[{"delta":{"content":"token"}}]}` on next.
    // Prepending the missing prefix reconstructs the full valid JSON.
    let prefixes: &[&str] = &[
        r#"{"id":"chatcmpl-"#,
        r#"{"id":""#,
        r#"{"#,
    ];
    for prefix in prefixes {
        let candidate = format!("{}{}", prefix, fragment);
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&candidate) {
            if let Some(text) = val
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|t| t.as_str())
            {
                return Some(text.to_string());
            }
        }
    }
    None
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
            "max_tokens": 2048,
            "stream": true,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": { "type": "base64", "media_type": "image/png", "data": b64 }
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

        let mut stream = response.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;
            let mut texts: Vec<String> = Vec::new();

            split_lines_bytes(&mut buf, &bytes, |line| {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" { return; }
                    match serde_json::from_str::<serde_json::Value>(data) {
                        Ok(val) => {
                            if let Some(text) = val
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                texts.push(text.to_string());
                            }
                        }
                        Err(e) => eprintln!("[shiro] Anthropic SSE parse error: {e}"),
                    }
                }
            });

            for text in texts {
                send_chunk(&tx, &text).await;
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

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
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
        let data_url = format!("data:image/png;base64,{b64}");

        let body = json!({
            "model": self.model,
            "max_tokens": 2048,
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
        eprintln!("[shiro] OpenAI → {endpoint} model={}", self.model);

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        let status = response.status();
        eprintln!("[shiro] OpenAI status: {status}");
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error {status}: {text}"));
        }

        let mut stream = response.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut first_chunk = true;
        let mut token_count = 0usize;
        let mut from_data_lines = 0usize;
        let mut from_fragments = 0usize;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;

            // Dump first raw chunk so we can see Vocareum's exact format
            if first_chunk {
                first_chunk = false;
                let preview: String = bytes.iter().take(300)
                    .flat_map(|&b| std::ascii::escape_default(b))
                    .map(|b| b as char)
                    .collect();
                eprintln!("[shiro] First chunk ({} bytes): {}", bytes.len(), preview);
            }

            let mut texts: Vec<String> = Vec::new();

            split_lines_bytes(&mut buf, &bytes, |line| {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" { return; }
                    match serde_json::from_str::<serde_json::Value>(data) {
                        Ok(val) => {
                            if let Some(text) = val
                                .get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("delta"))
                                .and_then(|d| d.get("content"))
                                .and_then(|t| t.as_str())
                            {
                                from_data_lines += 1;
                                texts.push(text.to_string());
                            }
                        }
                        Err(e) => eprintln!("[shiro] OpenAI SSE parse error: {e}\n  data: {data}"),
                    }
                } else if !line.is_empty() && !line.starts_with(':') {
                    // Vocareum (and potentially other proxies) send partial JSON lines
                    // missing the {"id":"chatcmpl- prefix. Try to reconstruct and parse.
                    if let Some(content) = extract_content_from_fragment(line) {
                        from_fragments += 1;
                        texts.push(content);
                    }
                }
            });

            token_count += texts.len();
            for text in texts {
                send_chunk(&tx, &text).await;
            }
        }

        eprintln!(
            "[shiro] OpenAI stream done. tokens={token_count} (data_lines={from_data_lines} fragments={from_fragments})"
        );
        send_done(&tx).await;
        Ok(())
    }
}
