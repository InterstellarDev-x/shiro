import { useState, useRef, useEffect, KeyboardEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCaptureQuery } from "../hooks/useCaptureQuery";

export default function OverlayWindow() {
  const [prompt, setPrompt] = useState("");
  const { response, loading, error, query, reset } = useCaptureQuery();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const appWindow = getCurrentWindow();

  // Focus input when overlay appears
  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  // Close overlay on Escape
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        handleClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const handleClose = async () => {
    reset();
    setPrompt("");
    await appWindow.hide();
  };

  const handleSubmit = async () => {
    if (!prompt.trim() || loading) return;
    await query(prompt.trim());
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    // drag region covers the whole window; input/buttons are excluded
    <div
      data-tauri-drag-region
      className="h-screen w-screen flex flex-col p-3 gap-2 bg-white/90 dark:bg-zinc-900/90 backdrop-blur-md rounded-xl shadow-2xl border border-zinc-200 dark:border-zinc-700"
    >
      {/* Header */}
      <div
        data-tauri-drag-region
        className="flex items-center justify-between select-none"
      >
        <span className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 tracking-wide uppercase">
          Shiro
        </span>
        <button
          onClick={handleClose}
          className="text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200 text-lg leading-none px-1"
        >
          ×
        </button>
      </div>

      {/* Prompt input */}
      <textarea
        ref={textareaRef}
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Ask about your screen… (Enter to send)"
        disabled={loading}
        rows={3}
        className="w-full resize-none rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm text-zinc-900 dark:text-zinc-100 placeholder:text-zinc-400 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
      />

      {/* Send button */}
      <button
        onClick={handleSubmit}
        disabled={loading || !prompt.trim()}
        className="w-full rounded-lg bg-blue-600 hover:bg-blue-700 disabled:bg-blue-300 dark:disabled:bg-blue-900 text-white text-sm font-medium py-1.5 transition-colors"
      >
        {loading ? "Capturing & asking…" : "Capture + Ask"}
      </button>

      {/* Response area */}
      {(response || error) && (
        <div className="flex-1 overflow-y-auto rounded-lg bg-zinc-50 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 p-3 text-sm text-zinc-800 dark:text-zinc-200 whitespace-pre-wrap">
          {error ? (
            <span className="text-red-500">{error}</span>
          ) : (
            response
          )}
        </div>
      )}
    </div>
  );
}
