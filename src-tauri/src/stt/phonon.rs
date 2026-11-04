use super::{AudioSource, SpeechConfig, SpeechEngine, TranscriptEvent};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

pub struct PhononEngine;

#[async_trait]
impl SpeechEngine for PhononEngine {
    fn id(&self) -> &'static str {
        "phonon"
    }

    async fn run_source(
        &self,
        source: AudioSource,
        config: SpeechConfig,
        mut audio: mpsc::Receiver<Vec<f32>>,
        events: mpsc::Sender<TranscriptEvent>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<(), String> {
        let model = sanitize_model(if config.model.trim().is_empty() {
            "phonon-2.5-en"
        } else {
            config.model.trim()
        });
        let port = match source {
            AudioSource::Me => 8011,
            AudioSource::Them => 8012,
        };
        let who = match source {
            AudioSource::Me => "you",
            AudioSource::Them => "the other person",
        };
        let _ = events
            .send(TranscriptEvent::Status {
                message: format!("Loading {model} for {who}…"),
            })
            .await;

        let mut child = Command::new("fermion")
            .args([
                "serve",
                &model,
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "fermion is not installed. Run: pip install fermion-research".to_string()
                } else {
                    format!("Could not start fermion: {error}")
                }
            })?;

        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("[shiro] phonon:{port}: {line}");
                }
            });
        }

        wait_until_ready(&mut child, port, &model, &mut stop).await?;
        if *stop.borrow() {
            return Ok(());
        }

        let url = format!("ws://127.0.0.1:{port}/v1/audio/stream");
        let request = url
            .into_client_request()
            .map_err(|e| format!("Phonon URL failed: {e}"))?;
        let (mut socket, _) = connect_async(request)
            .await
            .map_err(|e| format!("Phonon connection on port {port} failed: {e}"))?;
        let _ = socket
            .send(Message::Text(
                r#"{"sample_rate":16000,"format":"pcm_f32le"}"#.into(),
            ))
            .await
            .map_err(|e| format!("Phonon rejected the audio format: {e}"))?;
        eprintln!("[shiro] Phonon stream open for {source:?} on {port} ({model})");
        let _ = events
            .send(TranscriptEvent::Status {
                message: "Listening".into(),
            })
            .await;

        loop {
            tokio::select! {
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        break;
                    }
                }
                chunk = audio.recv() => {
                    match chunk {
                        Some(samples) => {
                            let bytes = pcm_f32le(&samples);
                            if socket.send(Message::Binary(bytes.into())).await.is_err() {
                                return Err(format!("Phonon connection on port {port} closed"));
                            }
                        }
                        None => break,
                    }
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            handle_message(&text, source, &events).await?;
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(error)) => {
                            return Err(format!("Phonon stream on port {port} failed: {error}"));
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        let _ = socket.send(Message::Text(r#"{"type":"end"}"#.into())).await;
        let _ = socket.close(None).await;
        Ok(())
    }
}

async fn wait_until_ready(
    child: &mut tokio::process::Child,
    port: u16,
    model: &str,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| format!("HTTP client failed: {e}"))?;
    let health = format!("http://127.0.0.1:{port}/health");
    let started = tokio::time::Instant::now();

    loop {
        if *stop.borrow() {
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(180) {
            return Err(
                "Phonon did not become ready within 3 minutes. The first run downloads the model."
                    .into(),
            );
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Phonon exited while loading {model} ({status}). On a Mac, also install the MLX runtime: pip install mlx mlx-audio mlx-lm soundfile scipy zstandard"
            ));
        }
        if let Ok(response) = client.get(&health).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return Ok(());
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
}

fn sanitize_model(model: &str) -> String {
    let cleaned: String = model
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'))
        .collect();
    if cleaned.is_empty() {
        "phonon-2.5-en".to_string()
    } else {
        cleaned
    }
}

fn pcm_f32le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

async fn handle_message(
    text: &str,
    source: AudioSource,
    events: &mpsc::Sender<TranscriptEvent>,
) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let body = value.get("text").and_then(|item| item.as_str()).unwrap_or("").trim();
    match value.get("type").and_then(|kind| kind.as_str()).unwrap_or("") {
        "partial" if !body.is_empty() => {
            let _ = events
                .send(TranscriptEvent::Partial {
                    source,
                    text: body.to_string(),
                })
                .await;
        }
        "final" if !body.is_empty() => {
            let _ = events
                .send(TranscriptEvent::Final {
                    source,
                    text: body.to_string(),
                })
                .await;
        }
        "error" => {
            let message = value
                .get("message")
                .and_then(|item| item.as_str())
                .unwrap_or(text);
            return Err(format!("Phonon error: {message}"));
        }
        _ => {}
    }
    Ok(())
}
