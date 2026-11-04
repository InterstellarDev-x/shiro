use super::{AudioSource, SpeechConfig, SpeechEngine, TranscriptEvent};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

pub struct DeepgramEngine;

#[async_trait]
impl SpeechEngine for DeepgramEngine {
    fn id(&self) -> &'static str {
        "deepgram"
    }

    async fn run_source(
        &self,
        source: AudioSource,
        config: SpeechConfig,
        mut audio: mpsc::Receiver<Vec<f32>>,
        events: mpsc::Sender<TranscriptEvent>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<(), String> {
        if config.api_key.trim().is_empty() {
            return Err(
                "Add a Deepgram API key in Settings, or set DEEPGRAM_API_KEY.".into(),
            );
        }
        let model = sanitize_model(if config.model.trim().is_empty() {
            "nova-3"
        } else {
            config.model.trim()
        });

        let language = sanitize_language(&config.language);
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model={model}&language={language}&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&endpointing=400&punctuate=true&smart_format=true&vad_events=true&utterance_end_ms=1200&mip_opt_out=true"
        );
        let mut request = url
            .into_client_request()
            .map_err(|e| format!("Deepgram URL failed: {e}"))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", config.api_key.trim())
                .parse()
                .map_err(|_| "Deepgram API key contains an invalid character")?,
        );

        // Both rustls crypto backends are enabled, so the connection panics
        // unless one is chosen before the socket opens.
        let _ = rustls::crypto::ring::default_provider().install_default();
        eprintln!("[shiro] connecting {source:?} to Deepgram ({model})");
        let (socket, _) = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            connect_async(request),
        )
        .await
        .map_err(|_| "Deepgram took too long to connect. Check the network and the API key.".to_string())?
        .map_err(|e| format!("Deepgram connection failed: {e}"))?;
        eprintln!("[shiro] Deepgram stream open for {source:?} ({model})");
        let (mut write, mut read) = socket.split();

        let mut committed = String::new();
        let mut sent = 0u64;
        let mut announced = false;
        loop {
            tokio::select! {
                biased;
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        break;
                    }
                }
                message = read.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            handle_message(&text, source, &mut committed, &events).await?;
                        }
                        Some(Ok(Message::Binary(bytes))) => {
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                handle_message(text, source, &mut committed, &events).await?;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(error)) => return Err(format!("Deepgram stream failed: {error}")),
                        Some(Ok(_)) => {}
                    }
                }
                chunk = audio.recv() => {
                    match chunk {
                        Some(samples) => {
                            let bytes = linear16(&samples);
                            sent += bytes.len() as u64;
                            if !announced {
                                announced = true;
                                eprintln!("[shiro] sending microphone audio to Deepgram for {source:?}");
                            }
                            if write.send(Message::Binary(bytes.into())).await.is_err() {
                                return Err("Deepgram connection closed while sending audio".into());
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        eprintln!("[shiro] {source:?} sent {sent} bytes to Deepgram");
        let _ = write
            .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
            .await;
        Ok(())
    }
}

fn sanitize_language(language: &str) -> String {
    let cleaned: String = language
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    if cleaned.is_empty() { "en".to_string() } else { cleaned }
}

fn sanitize_model(model: &str) -> String {
    let cleaned: String = model
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect();
    if cleaned.is_empty() {
        "nova-3".to_string()
    } else {
        cleaned
    }
}

fn linear16(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&scaled.to_le_bytes());
    }
    bytes
}

async fn handle_message(
    text: &str,
    source: AudioSource,
    committed: &mut String,
    events: &mpsc::Sender<TranscriptEvent>,
) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    match value.get("type").and_then(|kind| kind.as_str()).unwrap_or("") {
        "SpeechStarted" => {
            let _ = events
                .send(TranscriptEvent::SpeechStarted { source })
                .await;
        }
        "UtteranceEnd" => {
            let finished = std::mem::take(committed);
            let finished = finished.trim();
            if !finished.is_empty() {
                let _ = events
                    .send(TranscriptEvent::Final {
                        source,
                        text: finished.to_string(),
                    })
                    .await;
            }
        }
        "Results" => {
            let transcript = value
                .pointer("/channel/alternatives/0/transcript")
                .and_then(|item| item.as_str())
                .unwrap_or("")
                .trim();
            let is_final = value.get("is_final").and_then(|item| item.as_bool()).unwrap_or(false);
            let speech_final = value
                .get("speech_final")
                .and_then(|item| item.as_bool())
                .unwrap_or(false);

            if speech_final {
                let finished = join_transcript(committed, transcript);
                committed.clear();
                if !finished.is_empty() {
                    let _ = events
                        .send(TranscriptEvent::Final {
                            source,
                            text: finished,
                        })
                        .await;
                }
            } else if is_final && !transcript.is_empty() {
                if !committed.is_empty() {
                    committed.push(' ');
                }
                committed.push_str(transcript);
                let _ = events
                    .send(TranscriptEvent::Partial {
                        source,
                        text: committed.clone(),
                    })
                    .await;
            } else if !transcript.is_empty() {
                let _ = events
                    .send(TranscriptEvent::Partial {
                        source,
                        text: join_transcript(committed, transcript),
                    })
                    .await;
            }
        }
        "Error" => {
            let message = value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(|item| item.as_str())
                .unwrap_or(text);
            return Err(format!("Deepgram error: {message}"));
        }
        _ => {}
    }
    Ok(())
}

fn join_transcript(committed: &str, latest: &str) -> String {
    match (committed.trim().is_empty(), latest.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => committed.trim().to_string(),
        (true, false) => latest.trim().to_string(),
        (false, false) => format!("{} {}", committed.trim(), latest.trim()),
    }
}
