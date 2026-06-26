import { invoke, Channel } from "@tauri-apps/api/core";
import { useState, useCallback } from "react";

interface StreamChunk {
  text: string;
  done: boolean;
}

export function useCaptureQuery() {
  const [response, setResponse] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const query = useCallback(async (prompt: string) => {
    setLoading(true);
    setResponse("");
    setError(null);

    const channel = new Channel<StreamChunk>();
    channel.onmessage = (chunk) => {
      if (!chunk.done) {
        setResponse((prev) => prev + chunk.text);
      } else {
        setLoading(false);
      }
    };

    try {
      await invoke("capture_and_query", { prompt, channel });
    } catch (e) {
      setError(String(e));
      setLoading(false);
    }
  }, []);

  const reset = useCallback(() => {
    setResponse("");
    setError(null);
  }, []);

  return { response, loading, error, query, reset };
}
