import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useCaptureQuery } from "../hooks/useCaptureQuery";
import { useHistory, HistoryEntry } from "../hooks/useHistory";
import { useTemplates } from "../hooks/useTemplates";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

const DEFAULT_HOTKEY = "Ctrl+Shift+0";
const HOTKEY_KEY = "shiro_hotkey";

type Provider = "anthropic" | "openai";

const PROVIDER_LABELS: Record<Provider, string> = {
  anthropic: "Anthropic (Claude)",
  openai: "OpenAI (GPT-4o)",
};

const DEFAULT_MODELS: Record<Provider, string> = {
  anthropic: "claude-haiku-4-5-20251001",
  openai: "gpt-4o",
};

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function SettingsPanel({ onClose, onHotkeyChange }: { onClose: () => void; onHotkeyChange: (hk: string) => void }) {
  const { templates, addTemplate, removeTemplate, resetToDefaults } = useTemplates();
  const [newLabel, setNewLabel] = useState("");
  const [newPrompt, setNewPrompt] = useState("");
  const [hotkey, setHotkey] = useState(() => localStorage.getItem(HOTKEY_KEY) ?? DEFAULT_HOTKEY);
  const [hotkeyError, setHotkeyError] = useState("");
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  const toggleAutostart = async () => {
    try {
      await invoke("set_autostart", { enabled: !autostart });
      setAutostart(!autostart);
    } catch (e) {
      console.error(e);
    }
  };

  const saveHotkey = async () => {
    setHotkeyError("");
    try {
      await invoke("set_hotkey", { shortcutStr: hotkey });
      localStorage.setItem(HOTKEY_KEY, hotkey);
      onHotkeyChange(hotkey);
    } catch (e) {
      setHotkeyError(String(e));
    }
  };

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
  const [debugImg, setDebugImg] = useState<string | null>(null);

  useEffect(() => {
    setApiKey(localStorage.getItem(`api_key_${provider}`) ?? "");
  }, [provider]);

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

  const handleDebugCapture = async () => {
    const dataUrl = await invoke<string>("debug_capture").catch((e) => { console.error(e); return null; });
    setDebugImg(dataUrl);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-700 shadow-xl p-6 flex flex-col gap-5">
        <div className="flex items-center justify-between">
          <h2 className="text-base font-semibold text-zinc-900 dark:text-zinc-100">Settings</h2>
          <button onClick={onClose} className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 text-lg leading-none">✕</button>
        </div>

        <div className="flex flex-col gap-2">
          <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">Provider</label>
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

        <div className="flex flex-col gap-2">
          <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">API Key</label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={`${PROVIDER_LABELS[provider]} API key`}
            className="rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          {provider === "openai" && (
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="Base URL (optional)"
              className="rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          )}
        </div>

        <div className="flex flex-col gap-2">
          <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">Debug</label>
          <div className="flex gap-2 items-center">
            <button
              onClick={handleDebugCapture}
              className="rounded-lg border border-zinc-200 dark:border-zinc-700 text-xs px-3 py-1.5 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
            >
              Preview capture
            </button>
            {debugImg && (
              <button onClick={() => setDebugImg(null)} className="text-xs text-zinc-400 hover:text-zinc-600">hide</button>
            )}
          </div>
          {debugImg && (
            <img src={debugImg} alt="Captured screen" className="rounded-lg border border-zinc-200 dark:border-zinc-700 w-full object-contain max-h-40" />
          )}
        </div>

        {/* Template management */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">Quick Templates</label>
            <button onClick={resetToDefaults} className="text-xs text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300">Reset defaults</button>
          </div>
          <div className="flex flex-col gap-1 max-h-32 overflow-y-auto">
            {templates.map((t) => (
              <div key={t.id} className="flex items-center gap-2 rounded-lg bg-zinc-50 dark:bg-zinc-800 px-2 py-1.5">
                <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300 w-20 shrink-0">{t.label}</span>
                <span className="flex-1 text-xs text-zinc-400 truncate">{t.prompt}</span>
                <button onClick={() => removeTemplate(t.id)} className="text-zinc-300 hover:text-red-500 text-sm leading-none shrink-0">×</button>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <input
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              placeholder="Label"
              className="w-20 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
            <input
              value={newPrompt}
              onChange={(e) => setNewPrompt(e.target.value)}
              placeholder="Prompt text…"
              className="flex-1 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-2 py-1.5 text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
            <button
              onClick={() => {
                if (!newLabel.trim() || !newPrompt.trim()) return;
                addTemplate(newLabel.trim(), newPrompt.trim());
                setNewLabel("");
                setNewPrompt("");
              }}
              className="rounded-lg bg-blue-600 hover:bg-blue-700 text-white text-xs px-3 py-1.5 transition-colors"
            >
              Add
            </button>
          </div>
        </div>

        {/* Hotkey */}
        <div className="flex flex-col gap-2">
          <label className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">Hotkey</label>
          <div className="flex gap-2">
            <input
              value={hotkey}
              onChange={(e) => setHotkey(e.target.value)}
              placeholder="e.g. Ctrl+Shift+0"
              className="flex-1 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <button
              onClick={saveHotkey}
              className="rounded-lg bg-zinc-700 hover:bg-zinc-800 text-white text-sm px-3 py-2 transition-colors"
            >
              Apply
            </button>
          </div>
          {hotkeyError && <p className="text-xs text-red-500">{hotkeyError}</p>}
          <p className="text-xs text-zinc-400">Format: Ctrl+Shift+0 — modifiers: Ctrl Shift Alt Meta. Keys: A–Z, 0–9, F1–F12, Space, Tab.</p>
        </div>

        {/* Launch at startup */}
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">Launch at startup</p>
            <p className="text-xs text-zinc-400">Start Shiro automatically when you log in</p>
          </div>
          <button
            onClick={toggleAutostart}
            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
              autostart ? "bg-blue-600" : "bg-zinc-300 dark:bg-zinc-600"
            }`}
          >
            <span className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              autostart ? "translate-x-6" : "translate-x-1"
            }`} />
          </button>
        </div>

        <button
          onClick={saveProvider}
          className="rounded-lg bg-zinc-800 dark:bg-zinc-200 text-white dark:text-zinc-900 text-sm font-medium py-2 hover:opacity-80 transition-opacity"
        >
          {saved ? "Saved ✓" : "Save"}
        </button>
      </div>
    </div>
  );
}

function HistoryDetail({ entry, onBack }: { entry: HistoryEntry; onBack: () => void }) {
  const [copied, setCopied] = useState(false);

  const copy = () => {
    navigator.clipboard.writeText(entry.response);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex flex-col h-full gap-3">
      <div className="flex items-center gap-2">
        <button
          onClick={onBack}
          className="text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200 text-sm"
        >
          ← Back
        </button>
        <span className="flex-1 text-xs text-zinc-400">{timeAgo(entry.timestamp)}</span>
        <button
          onClick={copy}
          className="text-xs rounded border border-zinc-200 dark:border-zinc-700 px-2 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
        >
          {copied ? "Copied ✓" : "Copy"}
        </button>
      </div>
      <div className="rounded-lg bg-zinc-100 dark:bg-zinc-800 px-3 py-2 text-sm text-zinc-700 dark:text-zinc-300">
        {entry.prompt}
      </div>
      <div className="flex-1 overflow-y-auto rounded-lg bg-zinc-50 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 p-4 text-sm text-zinc-800 dark:text-zinc-200">
        <ReactMarkdown
          components={{
            code({ className, children, ...props }) {
              const match = /language-(\w+)/.exec(className ?? "");
              return match ? (
                <SyntaxHighlighter style={oneDark} language={match[1]} PreTag="div" className="rounded-md text-xs my-2">
                  {String(children).replace(/\n$/, "")}
                </SyntaxHighlighter>
              ) : (
                <code className="bg-zinc-200 dark:bg-zinc-700 rounded px-1 py-0.5 text-xs font-mono" {...props}>{children}</code>
              );
            },
            p({ children }) { return <p className="mb-2 last:mb-0 leading-relaxed">{children}</p>; },
            ul({ children }) { return <ul className="list-disc list-inside mb-2 space-y-1">{children}</ul>; },
            ol({ children }) { return <ol className="list-decimal list-inside mb-2 space-y-1">{children}</ol>; },
            h1({ children }) { return <h1 className="text-base font-bold mb-2">{children}</h1>; },
            h2({ children }) { return <h2 className="text-sm font-bold mb-1">{children}</h2>; },
            strong({ children }) { return <strong className="font-semibold">{children}</strong>; },
          }}
        >
          {entry.response}
        </ReactMarkdown>
      </div>
    </div>
  );
}

export default function MainWindow() {
  const [showSettings, setShowSettings] = useState(false);
  const [selectedEntry, setSelectedEntry] = useState<HistoryEntry | null>(null);
  const [hotkey, setHotkey] = useState(() => localStorage.getItem(HOTKEY_KEY) ?? DEFAULT_HOTKEY);
  const { response, loading, error, query, reset } = useCaptureQuery();
  const { history, addEntry, clearHistory } = useHistory();
  const [prompt, setPrompt] = useState("");

  const handleQuery = async () => {
    if (!prompt.trim() || loading) return;
    const userPrompt = prompt.trim();
    setPrompt("");
    const finalResponse = await query(userPrompt);
    if (finalResponse) addEntry(userPrompt, finalResponse);
  };

  return (
    <div className="flex h-screen overflow-hidden">
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} onHotkeyChange={setHotkey} />}

      {/* History sidebar */}
      <aside className="w-56 shrink-0 flex flex-col border-r border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-950">
        <div className="flex items-center justify-between px-3 py-3 border-b border-zinc-200 dark:border-zinc-700">
          <span className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 uppercase tracking-wide">History</span>
          {history.length > 0 && (
            <button
              onClick={clearHistory}
              className="text-xs text-zinc-400 hover:text-red-500 transition-colors"
            >
              Clear
            </button>
          )}
        </div>
        <div className="flex-1 overflow-y-auto">
          {history.length === 0 ? (
            <p className="text-xs text-zinc-400 px-3 py-4 text-center">No captures yet</p>
          ) : (
            history.map((entry) => (
              <button
                key={entry.id}
                onClick={() => setSelectedEntry(entry)}
                className={`w-full text-left px-3 py-2.5 border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors ${
                  selectedEntry?.id === entry.id ? "bg-blue-50 dark:bg-blue-950 border-l-2 border-l-blue-500" : ""
                }`}
              >
                <p className="text-xs font-medium text-zinc-700 dark:text-zinc-300 truncate">{entry.prompt}</p>
                <p className="text-[10px] text-zinc-400 mt-0.5">{timeAgo(entry.timestamp)}</p>
              </button>
            ))
          )}
        </div>
      </aside>

      {/* Main content */}
      <div className="flex flex-col flex-1 min-w-0 p-5 gap-4">
        {/* Header */}
        <div className="flex items-center justify-between shrink-0">
          <h1 className="text-xl font-bold text-zinc-900 dark:text-zinc-100">Shiro</h1>
          <button
            onClick={() => setShowSettings(true)}
            className="rounded-lg border border-zinc-200 dark:border-zinc-700 px-3 py-1.5 text-sm text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
          >
            Settings
          </button>
        </div>

        <p className="text-sm text-zinc-500 dark:text-zinc-400 shrink-0">
          Press{" "}
          <kbd className="rounded bg-zinc-100 dark:bg-zinc-800 px-1.5 py-0.5 text-xs font-mono border border-zinc-300 dark:border-zinc-600">
            {hotkey}
          </kbd>{" "}
          anywhere to open the overlay and ask about your screen.
        </p>

        <hr className="border-zinc-200 dark:border-zinc-700 shrink-0" />

        {/* History detail or current query */}
        {selectedEntry ? (
          <HistoryDetail entry={selectedEntry} onBack={() => setSelectedEntry(null)} />
        ) : (
          <>
            <div className="flex gap-2 shrink-0">
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

            {(response || error) && (
              <div className="flex-1 overflow-y-auto rounded-lg bg-zinc-50 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 p-4 text-sm text-zinc-800 dark:text-zinc-200">
                {error ? (
                  <span className="text-red-500">{error}</span>
                ) : (
                  <ReactMarkdown
                    components={{
                      code({ className, children, ...props }) {
                        const match = /language-(\w+)/.exec(className ?? "");
                        return match ? (
                          <SyntaxHighlighter style={oneDark} language={match[1]} PreTag="div" className="rounded-md text-xs my-2">
                            {String(children).replace(/\n$/, "")}
                          </SyntaxHighlighter>
                        ) : (
                          <code className="bg-zinc-200 dark:bg-zinc-700 rounded px-1 py-0.5 text-xs font-mono" {...props}>{children}</code>
                        );
                      },
                      p({ children }) { return <p className="mb-2 last:mb-0 leading-relaxed">{children}</p>; },
                      ul({ children }) { return <ul className="list-disc list-inside mb-2 space-y-1">{children}</ul>; },
                      ol({ children }) { return <ol className="list-decimal list-inside mb-2 space-y-1">{children}</ol>; },
                      h1({ children }) { return <h1 className="text-base font-bold mb-2">{children}</h1>; },
                      h2({ children }) { return <h2 className="text-sm font-bold mb-1">{children}</h2>; },
                      strong({ children }) { return <strong className="font-semibold">{children}</strong>; },
                    }}
                  >
                    {response}
                  </ReactMarkdown>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
