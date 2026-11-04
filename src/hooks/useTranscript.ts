import { invoke, Channel } from "@tauri-apps/api/core";
import { useCallback, useRef, useState } from "react";
import {
  DEFAULT_DEEPGRAM_MODEL,
  DEFAULT_PHONON_MODEL,
  SPEECH_PROVIDER,
} from "../speech/provider";
import { loadCallSession } from "../callSession";

export type TranscriptSource = "me" | "them";

export interface TranscriptLine {
  id: string;
  source: TranscriptSource;
  text: string;
}

type TranscriptEvent =
  | { kind: "partial"; source: TranscriptSource; text: string }
  | { kind: "final"; source: TranscriptSource; text: string }
  | { kind: "speech_started"; source: TranscriptSource }
  | { kind: "status"; message: string }
  | { kind: "error"; message: string };

export function useTranscript() {
  const [lines, setLines] = useState<TranscriptLine[]>([]);
  const [partials, setPartials] = useState<Partial<Record<TranscriptSource, string>>>({});
  const [listening, setListening] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState<string | null>(null);
  const channelRef = useRef<Channel<TranscriptEvent> | null>(null);
  const startingRef = useRef(false);

  const start = useCallback(async () => {
    if (startingRef.current) return;
    startingRef.current = true;
    setError(null);
    setStatus("Starting…");
    setLines([]);
    setPartials({});
    const channel = new Channel<TranscriptEvent>();
    channel.onmessage = (event) => {
      if (event.kind === "partial") {
        setPartials((prev) => ({ ...prev, [event.source]: event.text }));
      } else if (event.kind === "final") {
        const text = event.text.trim();
        setPartials((prev) => ({ ...prev, [event.source]: "" }));
        if (!text) return;
        setLines((prev) => [
          ...prev,
          {
            id: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
            source: event.source,
            text,
          },
        ]);
      } else if (event.kind === "status") {
        setStatus(event.message);
      } else if (event.kind === "error") {
        setError(event.message);
      }
    };
    channelRef.current = channel;

    const model =
      SPEECH_PROVIDER === "phonon"
        ? localStorage.getItem("phonon_model") || DEFAULT_PHONON_MODEL
        : localStorage.getItem("deepgram_model") || DEFAULT_DEEPGRAM_MODEL;

    try {
      await invoke("start_listening", {
        apiKey: localStorage.getItem("api_key_deepgram") ?? "",
        model,
        language: loadCallSession().language,
        channel,
      });
      setListening(true);
    } catch (e) {
      setStatus("");
      setError(String(e));
      setListening(false);
    } finally {
      startingRef.current = false;
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      await invoke("stop_listening");
    } catch (e) {
      setError(String(e));
    }
    setListening(false);
    setStatus("");
    setPartials({});
  }, []);

  const clear = useCallback(() => {
    setLines([]);
    setPartials({});
    setError(null);
  }, []);

  return { lines, partials, listening, status, error, start, stop, clear };
}
