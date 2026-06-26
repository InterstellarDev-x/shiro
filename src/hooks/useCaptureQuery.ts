import { invoke, Channel } from "@tauri-apps/api/core";
import { useState, useCallback, useRef } from "react";

interface StreamChunk {
  text: string;
  done: boolean;
}

export function useCaptureQuery() {
  const [response, setResponse] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fullResponse = useRef("");

  const query = useCallback(async (prompt: string, opts?: { cached?: boolean }): Promise<string> => {
    setLoading(true);
    setResponse("");
    setError(null);
    fullResponse.current = "";

    let buffer = "";
    let rafId: number | null = null;

    const flush = () => {
      if (buffer) {
        const text = buffer;
        buffer = "";
        fullResponse.current += text;
        setResponse((prev) => prev + text);
      }
      rafId = null;
    };

    const channel = new Channel<StreamChunk>();
    channel.onmessage = (chunk) => {
      if (!chunk.done) {
        buffer += chunk.text;
        if (rafId === null) rafId = requestAnimationFrame(flush);
      } else {
        if (rafId !== null) cancelAnimationFrame(rafId);
        flush();
        setLoading(false);
      }
    };

    try {
      const cmd = opts?.cached ? "query_cached" : "capture_and_query";
      await invoke(cmd, { prompt, channel });
    } catch (e) {
      setError(String(e));
      setLoading(false);
    }

    return fullResponse.current;
  }, []);

  const reset = useCallback(() => {
    setResponse("");
    setError(null);
    fullResponse.current = "";
  }, []);

  return { response, loading, error, query, reset };
}
