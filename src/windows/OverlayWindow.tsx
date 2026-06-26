import { useState, useRef, useEffect, KeyboardEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize, LogicalSize } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import { useCaptureQuery } from "../hooks/useCaptureQuery";
import { useHistory } from "../hooks/useHistory";
import { useTemplates } from "../hooks/useTemplates";

const MODEL_KEY = "shiro_overlay_model";

type ModelTier = "fast" | "smart";

const MODEL_TIERS: Record<string, Record<ModelTier, string>> = {
  anthropic: { fast: "claude-haiku-4-5-20251001", smart: "claude-sonnet-4-6" },
  openai:    { fast: "gpt-4o-mini",               smart: "gpt-4o" },
};
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface Message {
  role: "user" | "assistant";
  text: string;
}

const POS_KEY = "shiro_overlay_pos";
const SIZE_KEY = "shiro_overlay_size";
const OPACITY_KEY = "shiro_overlay_opacity";

export default function OverlayWindow() {
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [opacity, setOpacity] = useState(() => {
    const v = localStorage.getItem(OPACITY_KEY);
    return v ? parseFloat(v) : 0.85;
  });
  const [showOpacity, setShowOpacity] = useState(false);
  const [modelTier, setModelTier] = useState<ModelTier>(
    () => (localStorage.getItem(MODEL_KEY) as ModelTier) ?? "fast"
  );
  const [hasCachedShot, setHasCachedShot] = useState(false);

  const { response, loading, error, query, reset } = useCaptureQuery();
  const { addEntry } = useHistory();
  const { templates } = useTemplates();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const appWindow = getCurrentWindow();

  // Transparent background
  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    const root = document.getElementById("root");
    if (root) root.style.background = "transparent";
  }, []);

  // Restore saved position & size
  useEffect(() => {
    const pos = localStorage.getItem(POS_KEY);
    const size = localStorage.getItem(SIZE_KEY);
    if (pos) {
      try {
        const { x, y } = JSON.parse(pos);
        appWindow.setPosition(new PhysicalPosition(x, y)).catch(() => {});
      } catch {}
    }
    if (size) {
      try {
        const { w, h } = JSON.parse(size);
        appWindow.setSize(new PhysicalSize(w, h)).catch(() => {});
      } catch {}
    }
  }, []);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // Auto-scroll to bottom as response streams
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [response, messages]);

  // Append streaming response as last assistant message
  useEffect(() => {
    if (!loading && !response) return;
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last?.role === "assistant") {
        return [...prev.slice(0, -1), { role: "assistant", text: response }];
      }
      if (response) {
        return [...prev, { role: "assistant", text: response }];
      }
      return prev;
    });
  }, [response, loading]);

  const savePosition = async () => {
    const pos = await appWindow.outerPosition().catch(() => null);
    if (pos) localStorage.setItem(POS_KEY, JSON.stringify({ x: pos.x, y: pos.y }));
    const size = await appWindow.outerSize().catch(() => null);
    if (size) localStorage.setItem(SIZE_KEY, JSON.stringify({ w: size.width, h: size.height }));
  };

  const handleClose = async () => {
    await savePosition();
    reset();
    await appWindow.hide();
  };

  const applyModelTier = async (tier: ModelTier) => {
    const provider = (localStorage.getItem("provider") ?? "openai") as string;
    const apiKey = localStorage.getItem(`api_key_${provider}`) ?? "";
    const baseUrl = localStorage.getItem("openai_base_url") ?? "";
    const model = MODEL_TIERS[provider]?.[tier] ?? MODEL_TIERS["openai"][tier];
    await invoke("set_provider", {
      provider,
      apiKey,
      model,
      baseUrl: provider === "openai" ? baseUrl : null,
    }).catch(console.error);
  };

  const handleTierChange = async (tier: ModelTier) => {
    setModelTier(tier);
    localStorage.setItem(MODEL_KEY, tier);
    await applyModelTier(tier);
  };

  const handleSubmit = async (cached = false) => {
    if (!prompt.trim() || loading) return;
    const userPrompt = prompt.trim();
    setPrompt("");
    setMessages((prev) => [...prev, { role: "user", text: userPrompt }]);
    reset();
    const finalResponse = await query(userPrompt, { cached });
    if (finalResponse) {
      addEntry(userPrompt, finalResponse);
      if (!cached) setHasCachedShot(true);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(false);
    }
  };

  const handleTemplate = async (templatePrompt: string) => {
    if (loading) return;
    setMessages((prev) => [...prev, { role: "user", text: templatePrompt }]);
    reset();
    const finalResponse = await query(templatePrompt, { cached: false });
    if (finalResponse) { addEntry(templatePrompt, finalResponse); setHasCachedShot(true); }
  };

  const handleNewChat = () => {
    reset();
    setMessages([]);
    setPrompt("");
  };

  const onTitleMouseDown = () => {
    appWindow.startDragging().catch(console.error);
  };

  const onOpacityChange = (val: number) => {
    setOpacity(val);
    localStorage.setItem(OPACITY_KEY, String(val));
  };

  return (
    <div
      style={{ opacity }}
      className="relative h-screen w-screen flex flex-col bg-black/70 backdrop-blur-md rounded-xl shadow-2xl border border-white/10 overflow-hidden"
    >
      {/* Title bar — drag handle */}
      <div
        onMouseDown={onTitleMouseDown}
        className="h-9 flex items-center px-3 gap-2 select-none cursor-grab active:cursor-grabbing shrink-0 border-b border-white/10"
      >
        <span className="flex-1 text-xs font-semibold text-white/60 tracking-wide uppercase pointer-events-none">
          Shiro
        </span>

        {/* Model tier toggle */}
        <div
          onMouseDown={(e) => e.stopPropagation()}
          className="flex rounded-md border border-white/20 overflow-hidden text-[10px]"
        >
          {(["fast", "smart"] as ModelTier[]).map((t) => (
            <button
              key={t}
              onClick={() => handleTierChange(t)}
              className={`px-2 py-0.5 cursor-pointer transition-colors ${
                modelTier === t
                  ? "bg-blue-600 text-white"
                  : "text-white/40 hover:text-white/70"
              }`}
            >
              {t === "fast" ? "⚡ Fast" : "🧠 Smart"}
            </button>
          ))}
        </div>

        {/* Opacity toggle */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => setShowOpacity((v) => !v)}
          title="Opacity"
          className="text-white/40 hover:text-white/80 text-xs px-1 cursor-pointer"
        >
          ◑
        </button>

        {/* New chat */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleNewChat}
          title="New chat"
          className="text-white/40 hover:text-white/80 text-xs px-1 cursor-pointer"
        >
          ＋
        </button>

        {/* Close */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
          title="Close"
          className="text-white/40 hover:text-white text-lg leading-none px-1 cursor-pointer"
        >
          ×
        </button>
      </div>

      {/* Opacity slider */}
      {showOpacity && (
        <div className="flex items-center gap-2 px-3 py-2 bg-white/5 border-b border-white/10 shrink-0">
          <span className="text-white/50 text-xs">Opacity</span>
          <input
            type="range"
            min={0.2}
            max={1}
            step={0.05}
            value={opacity}
            onChange={(e) => onOpacityChange(parseFloat(e.target.value))}
            className="flex-1 h-1 accent-blue-400"
          />
          <span className="text-white/50 text-xs w-8 text-right">{Math.round(opacity * 100)}%</span>
        </div>
      )}

      {/* Conversation messages */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto px-3 py-2 flex flex-col gap-3 min-h-0"
      >
        {messages.length === 0 && !loading && (
          <p className="text-white/30 text-xs text-center mt-8">
            Ask anything about your screen
          </p>
        )}

        {messages.map((msg, i) => (
          <div
            key={i}
            className={`flex flex-col gap-0.5 ${msg.role === "user" ? "items-end" : "items-start"}`}
          >
            <span className="text-white/30 text-[10px] uppercase tracking-wide">
              {msg.role === "user" ? "You" : "Shiro"}
            </span>
            {msg.role === "user" ? (
              <div className="bg-blue-600/80 rounded-lg px-3 py-2 text-sm text-white max-w-[90%]">
                {msg.text}
              </div>
            ) : (
              <div className="bg-white/10 rounded-lg px-3 py-2 text-sm text-white/90 max-w-[90%]">
                <ReactMarkdown
                  components={{
                    code({ className, children, ...props }) {
                      const match = /language-(\w+)/.exec(className ?? "");
                      return match ? (
                        <SyntaxHighlighter style={oneDark} language={match[1]} PreTag="div" className="rounded text-xs my-1">
                          {String(children).replace(/\n$/, "")}
                        </SyntaxHighlighter>
                      ) : (
                        <code className="bg-white/20 rounded px-1 text-xs font-mono" {...props}>{children}</code>
                      );
                    },
                    p({ children }) { return <p className="mb-1.5 last:mb-0 leading-relaxed">{children}</p>; },
                    ul({ children }) { return <ul className="list-disc list-inside mb-1.5 space-y-0.5">{children}</ul>; },
                    ol({ children }) { return <ol className="list-decimal list-inside mb-1.5 space-y-0.5">{children}</ol>; },
                  }}
                >
                  {msg.text}
                </ReactMarkdown>
              </div>
            )}
          </div>
        ))}

        {loading && messages[messages.length - 1]?.role === "user" && (
          <div className="flex flex-col gap-0.5 items-start">
            <span className="text-white/30 text-[10px] uppercase tracking-wide">Shiro</span>
            <div className="bg-white/10 rounded-lg px-3 py-2 text-sm text-white/50 animate-pulse">
              Capturing & thinking…
            </div>
          </div>
        )}

        {error && (
          <div className="bg-red-500/20 rounded-lg px-3 py-2 text-sm text-red-300">{error}</div>
        )}
      </div>

      {/* Template pills */}
      <div className="flex gap-1.5 px-3 py-1.5 overflow-x-auto shrink-0 border-t border-white/10 scrollbar-none">
        {templates.map((t) => (
          <button
            key={t.id}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => handleTemplate(t.prompt)}
            disabled={loading}
            className="shrink-0 rounded-full border border-white/20 bg-white/10 hover:bg-white/20 px-3 py-1 text-xs text-white/70 hover:text-white transition-colors disabled:opacity-40 whitespace-nowrap"
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Input bar */}
      <div className="flex gap-2 px-3 pb-1 pt-2 shrink-0">
        <textarea
          ref={textareaRef}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask about your screen… (Enter to send)"
          disabled={loading}
          rows={2}
          className="flex-1 resize-none rounded-lg border border-white/20 bg-white/10 px-3 py-2 text-sm text-white placeholder:text-white/40 focus:outline-none focus:ring-2 focus:ring-blue-400 disabled:opacity-50"
        />
        <div className="flex flex-col gap-1 self-end">
          <button
            onClick={() => handleSubmit(false)}
            disabled={loading || !prompt.trim()}
            title="Capture screen & ask"
            className="rounded-lg bg-blue-600 hover:bg-blue-700 disabled:bg-blue-300/30 text-white text-xs font-medium px-3 py-1.5 transition-colors"
          >
            {loading ? "…" : "📷 Ask"}
          </button>
          <button
            onClick={() => handleSubmit(true)}
            disabled={loading || !prompt.trim() || !hasCachedShot}
            title={hasCachedShot ? "Ask using last screenshot (no new capture)" : "Ask once first to cache a screenshot"}
            className="rounded-lg bg-white/10 hover:bg-white/20 disabled:opacity-30 text-white text-xs font-medium px-3 py-1.5 transition-colors border border-white/20"
          >
            ↩ Re-ask
          </button>
        </div>
      </div>
      {/* Resize grip — bottom-right corner */}
      <div
        onMouseDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          const startX = e.screenX;
          const startY = e.screenY;
          const startW = window.innerWidth;
          const startH = window.innerHeight;
          const onMove = (ev: MouseEvent) => {
            const newW = Math.max(280, startW + (ev.screenX - startX));
            const newH = Math.max(300, startH + (ev.screenY - startY));
            appWindow.setSize(new LogicalSize(newW, newH)).catch(() => {});
          };
          const onUp = () => {
            window.removeEventListener("mousemove", onMove);
            window.removeEventListener("mouseup", onUp);
          };
          window.addEventListener("mousemove", onMove);
          window.addEventListener("mouseup", onUp);
        }}
        className="absolute bottom-0 right-0 w-5 h-5 cursor-se-resize flex items-center justify-center"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" className="text-white/30">
          <path d="M9 1L1 9M9 5L5 9M9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
        </svg>
      </div>
    </div>
  );
}
