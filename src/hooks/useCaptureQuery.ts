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

  const query = useCallback(async (prompt: string, opts?: { cached?: boolean; text?: boolean; screen?: boolean; task?: "answer" | "notes" | "mock" }): Promise<string> => {
    setLoading(true);
    setResponse("");
    setError(null);
    fullResponse.current = "";

    let buffer = "";
    let rafId: number | null = null;
    let lastPaint = 0;

    const paint = () => {
      if (!buffer) return;
      const text = buffer;
      buffer = "";
      fullResponse.current += text;
      setResponse((prev) => prev + text);
      lastPaint = performance.now();
    };

    const flush = () => {
      rafId = null;
      if (performance.now() - lastPaint < 80) {
        rafId = requestAnimationFrame(flush);
        return;
      }
      paint();
    };

    const channel = new Channel<StreamChunk>();
    channel.onmessage = (chunk) => {
      if (!chunk.done) {
        buffer += chunk.text;
        if (rafId === null) rafId = requestAnimationFrame(flush);
      } else {
        if (rafId !== null) cancelAnimationFrame(rafId);
        paint();
        setLoading(false);
      }
    };

    try {
      const cmd = opts?.cached ? "query_cached" : opts?.text ? "answer_call" : "capture_and_query";
      const notice = await invoke<string | null>(
        cmd,
        opts?.text
          ? { transcript: prompt, includeScreen: Boolean(opts.screen) && opts.task !== "notes", task: opts.task ?? "answer", channel }
          : { prompt, channel },
      );
      if (notice) setError(notice);
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
