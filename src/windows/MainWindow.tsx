import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { register, unregisterAll } from "@tauri-apps/plugin-global-shortcut";
import { getWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useCaptureQuery } from "../hooks/useCaptureQuery";

const HOTKEY = "CommandOrControl+Shift+S";

type Provider = "anthropic" | "openai";

const PROVIDER_LABELS: Record<Provider, string> = {
  anthropic: "Anthropic (Claude)",
  openai: "OpenAI (GPT-4o)",
};

const DEFAULT_MODELS: Record<Provider, string> = {
  anthropic: "claude-sonnet-4-6",
  openai: "gpt-4o",
};

export default function MainWindow() {
  const [provider, setProvider] = useState<Provider>(
    () => (localStorage.getItem("provider") as Provider) ?? "anthropic"
  );
  const [apiKey, setApiKey] = useState(
    () => localStorage.getItem(`api_key_${provider}`) ?? ""
  );
  const [baseUrl, setBaseUrl] = useState(
    () => localStorage.getItem("openai_base_url") ?? ""
  );
  const [saved, setSaved] = useState(false);
  const { response, loading, error, query, reset } = useCaptureQuery();
  const [prompt, setPrompt] = useState("");

  // Keep apiKey field in sync when provider changes
  useEffect(() => {
    setApiKey(localStorage.getItem(`api_key_${provider}`) ?? "");
  }, [provider]);

  // Register global hotkey to show overlay
  useEffect(() => {
    register(HOTKEY, async () => {
      const overlay = await getWebviewWindow("overlay");
      if (overlay) {
        await overlay.show();
        await overlay.setFocus();
      }
    }).catch(console.error);

    return () => {
      unregisterAll().catch(console.error);
    };
  }, []);

  const saveProvider = async () => {
    localStorage.setItem("provider", provider);
    localStorage.setItem(`api_key_${provider}`, apiKey);
    if (provider === "openai") {
      localStorage.setItem("openai_base_url", baseUrl);
    }

    await invoke("set_provider", {
      provider,
      apiKey,
      model: DEFAULT_MODELS[provider],
      baseUrl: provider === "openai" ? baseUrl : null,
    }).catch(console.error);

    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const handleQuery = async () => {
    if (!prompt.trim() || loading) return;
    await query(prompt.trim());
  };

  return (
    <div className="flex flex-col h-screen p-5 gap-4 max-w-2xl mx-auto">
      <h1 className="text-xl font-bold text-zinc-900 dark:text-zinc-100">
        Shiro
      </h1>
      <p className="text-sm text-zinc-500 dark:text-zinc-400">
        Press{" "}
        <kbd className="rounded bg-zinc-100 dark:bg-zinc-800 px-1.5 py-0.5 text-xs font-mono border border-zinc-300 dark:border-zinc-600">
          {HOTKEY}
        </kbd>{" "}
        anywhere to open the overlay and ask about your screen.
      </p>

      {/* Provider selector */}
      <div className="flex flex-col gap-2">
        <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">
          Provider
        </label>
        <div className="flex gap-2">
          {(["anthropic", "openai"] as Provider[]).map((p) => (
            <button
              key={p}
              onClick={() => setProvider(p)}
              className={`flex-1 rounded-lg border text-sm py-2 font-medium transition-colors ${
                provider === p
                  ? "border-blue-600 bg-blue-50 dark:bg-blue-950 text-blue-700 dark:text-blue-300"
                  : "border-zinc-200 dark:border-zinc-700 text-zinc-600 dark:text-zinc-400 hover:bg-zinc-50 dark:hover:bg-zinc-800"
              }`}
            >
              {PROVIDER_LABELS[p]}
            </button>
          ))}
        </div>
      </div>

      {/* API Key + optional base URL */}
      <div className="flex flex-col gap-2">
        <div className="flex gap-2 items-center">
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={`${PROVIDER_LABELS[provider]} API key`}
            className="flex-1 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          <button
            onClick={saveProvider}
            className="rounded-lg bg-zinc-800 dark:bg-zinc-200 text-white dark:text-zinc-900 text-sm font-medium px-4 py-2 hover:opacity-80 transition-opacity"
          >
            {saved ? "Saved ✓" : "Save"}
          </button>
        </div>
        {provider === "openai" && (
          <input
            type="text"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="Base URL (optional) — e.g. https://api.openai.com/v1"
            className="rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        )}
      </div>

      <hr className="border-zinc-200 dark:border-zinc-700" />

      {/* Manual query */}
      <div className="flex gap-2">
        <input
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleQuery()}
          placeholder="Ask about your current screen…"
          disabled={loading}
          className="flex-1 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        />
        <button
          onClick={handleQuery}
          disabled={loading || !prompt.trim()}
          className="rounded-lg bg-blue-600 hover:bg-blue-700 disabled:bg-blue-300 dark:disabled:bg-blue-900 text-white text-sm font-medium px-4 py-2 transition-colors"
        >
          {loading ? "…" : "Ask"}
        </button>
        {response && (
          <button
            onClick={reset}
            className="rounded-lg border border-zinc-200 dark:border-zinc-700 text-sm px-3 py-2 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
          >
            Clear
          </button>
        )}
      </div>

      {/* Response */}
      {(response || error) && (
        <div className="flex-1 overflow-y-auto rounded-lg bg-zinc-50 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 p-4 text-sm text-zinc-800 dark:text-zinc-200 whitespace-pre-wrap">
          {error ? <span className="text-red-500">{error}</span> : response}
        </div>
      )}
    </div>
  );
}
