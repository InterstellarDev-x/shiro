import { useState, useRef, useEffect, useCallback, KeyboardEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize, LogicalSize } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import { useCaptureQuery } from "../hooks/useCaptureQuery";
import { useHistory } from "../hooks/useHistory";
import { useTemplates } from "../hooks/useTemplates";
import { useTranscript, TranscriptSource } from "../hooks/useTranscript";
import { loadCallSession, withSession } from "../callSession";
import { upsertSavedCall } from "../savedCalls";
import MermaidChart from "../components/MermaidChart";

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

function looksLikeQuestion(text: string): boolean {
  const trimmed = text.trim();
  if (trimmed.length < 8) return false;
  if (trimmed.includes("?")) return true;
  return /\b(who|what|when|where|why|how|which|can you|could you|would you|should i|is there|are there|do you|does|did you|will you|tell me|explain)\b/i.test(trimmed);
}

function TranscriptRow({
  source,
  text,
  live,
}: {
  source: TranscriptSource;
  text: string;
  live: boolean;
}) {
  const label = source === "me" ? "Me" : "Them";
  const color = source === "me" ? "text-blue-300" : "text-emerald-300";
  return (
    <div className={`text-sm leading-snug ${live ? "text-white/50 italic" : "text-white/90"}`}>
      <span className={`mr-2 text-[10px] uppercase tracking-wide ${color}`}>{label}</span>
      {text}
    </div>
  );
}

const SCREEN_KEY = "shiro_call_screen";
const QUICK_KEY = "shiro_quick_context";
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
  const [includeScreen, setIncludeScreen] = useState(
    () => localStorage.getItem(SCREEN_KEY) !== "0"
  );
  const [quick, setQuick] = useState(() => localStorage.getItem(QUICK_KEY) === "1");

  const { response, loading, error, query, reset } = useCaptureQuery();
  const { addEntry } = useHistory();
  const { templates } = useTemplates();
  const transcript = useTranscript();
  const answeredIds = useRef<Set<string>>(new Set());
  const answering = useRef(false);
  const [answerTick, setAnswerTick] = useState(0);
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

  // Keep the latest line in view. Ignore the scroll events caused by our own pin,
  // or a diagram resize looks like the user scrolled and the view jumps.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const stick = { current: true };
    let pinning = false;
    const onScroll = () => {
      if (pinning) return;
      stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 64;
    };
    const pin = () => {
      if (!stick.current) return;
      pinning = true;
      el.scrollTop = el.scrollHeight;
      pinning = false;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    const observer = new ResizeObserver(pin);
    const content = el.firstElementChild;
    if (content) observer.observe(content);
    pin();
    return () => {
      el.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, []);

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

  const linesRef = useRef(transcript.lines);
  linesRef.current = transcript.lines;
  const includeScreenRef = useRef(includeScreen);
  includeScreenRef.current = includeScreen;
  const quickRef = useRef(quick);
  quickRef.current = quick;
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const callId = useRef<string | null>(null);
  const [meetingApp, setMeetingApp] = useState<string | null>(null);
  const dismissedMeeting = useRef<string | null>(null);

  const requestAnswer = useCallback(async (source: TranscriptSource, text: string, force = false) => {
    const trimmed = text.trim();
    const key = `${source}:${trimmed}`;
    if (!trimmed || (!force && !looksLikeQuestion(trimmed)) || answeredIds.current.has(key) || answering.current) return;
    if (!force) answeredIds.current.add(key);
    answering.current = true;
    const label = source === "me" ? "Me" : "Them";
    const fast = quickRef.current;
    const transcriptText = fast
      ? `${label}: ${trimmed}`
      : (() => {
          const context = linesRef.current
            .slice(-8)
            .map((item) => `${item.source === "me" ? "Me" : "Them"}: ${item.text}`)
            .join("\n");
          return context.includes(trimmed) ? context : `${context}\n${label}: ${trimmed}`.trim();
        })();
    const payload = fast
      ? `Answer in one short sentence the user can say out loud.${includeScreenRef.current ? " A screenshot of the user's screen is attached. Use what is visible in it." : ""}\n\n${transcriptText}`
      : withSession(transcriptText);
    setMessages((prev) => [...prev, { role: "user", text: `${label}: ${trimmed}` }]);
    reset();
    try {
      const provider = localStorage.getItem("provider") ?? "openai";
      const apiKey = localStorage.getItem(`api_key_${provider}`) ?? "";
      // The chat box already uses the model loaded in the app. Only switch
      // models when Settings has a key, so an empty value does not replace it.
      if (apiKey.trim()) await applyModelTier(modelTier);
      await query(payload, { text: true, screen: includeScreenRef.current, task: "answer" });
    } finally {
      answering.current = false;
      setAnswerTick((tick) => tick + 1);
    }
  }, [modelTier, query, reset]);

  useEffect(() => {
    if (!transcript.listening) return;
    const line = [...transcript.lines].reverse().find((item) => {
      const key = `${item.source}:${item.text.trim()}`;
      return looksLikeQuestion(item.text) && !answeredIds.current.has(key);
    });
    if (line) void requestAnswer(line.source, line.text);
  }, [transcript.lines, transcript.listening, answerTick, requestAnswer]);

  const writeNotes = async () => {
    if (answering.current || loading) return;
    const body = linesRef.current
      .map((item) => `${item.source === "me" ? "Me" : "Them"}: ${item.text}`)
      .join("\n");
    if (!body.trim()) return;
    answering.current = true;
    setMessages((prev) => [...prev, { role: "user", text: "Call notes" }]);
    reset();
    try {
      const provider = localStorage.getItem("provider") ?? "openai";
      const apiKey = localStorage.getItem(`api_key_${provider}`) ?? "";
      if (apiKey.trim()) await applyModelTier(modelTier);
      await query(withSession(body), { text: true, screen: false, task: "notes" });
    } finally {
      answering.current = false;
      saveActiveCall();
    }
  };

  const saveActiveCall = () => {
    if (!callId.current) return;
    const session = loadCallSession();
    const answers: { question: string; answer: string }[] = [];
    const current = messagesRef.current;
    for (let index = 0; index < current.length; index += 1) {
      const message = current[index];
      const next = current[index + 1];
      if (message.role === "user" && next?.role === "assistant" && next.text.trim()) {
        answers.push({ question: message.text, answer: next.text });
      }
    }
    const notes = [...answers].reverse().find((item) => item.question === "Call notes")?.answer ?? "";
    upsertSavedCall({
      id: callId.current,
      title: session.title.trim() || "Untitled call",
      mode: session.mode,
      style: session.style,
      language: session.language,
      startedAt: Number(callId.current.split("-")[0]) || Date.now(),
      updatedAt: Date.now(),
      lines: linesRef.current.map((line) => ({ source: line.source, text: line.text })),
      answers,
      notes,
    });
  };

  const answerNow = () => {
    const partial = transcript.partials.them || transcript.partials.me;
    const line = linesRef.current[linesRef.current.length - 1];
    if (partial?.trim()) {
      void requestAnswer(transcript.partials.them ? "them" : "me", partial, true);
      return;
    }
    if (line) void requestAnswer(line.source, line.text, true);
  };

  const askAboutCall = async () => {
    const question = prompt.trim();
    if (!question || loading || answering.current) return;
    setPrompt("");
    answering.current = true;
    const fast = quickRef.current;
    const body = fast
      ? ""
      : linesRef.current
          .map((item) => `${item.source === "me" ? "Me" : "Them"}: ${item.text}`)
          .join("\n");
    setMessages((prev) => [...prev, { role: "user", text: question }]);
    reset();
    try {
      const payload = fast
        ? `Answer in one short sentence the user can say out loud.\n\nQuestion: ${question}`
        : withSession(`${body}\nQuestion: ${question}`.trim());
      await query(payload, { text: true, screen: false, task: "answer" });
    } finally {
      answering.current = false;
      saveActiveCall();
    }
  };

  const askMock = async () => {
    if (loading || answering.current) return;
    if (!callId.current) callId.current = `${Date.now()}-${crypto.randomUUID()}`;
    answering.current = true;
    const spoken = linesRef.current
      .map((item) => `${item.source === "me" ? "Me" : "Them"}: ${item.text}`)
      .join("\n");
    const asked = messagesRef.current
      .filter((item) => item.role === "assistant")
      .slice(-4)
      .map((item) => item.text)
      .join("\n");
    setMessages((prev) => [...prev, { role: "user", text: "Mock question" }]);
    reset();
    try {
      const question = await query(
        withSession(`${spoken}\n\nQuestions already asked:\n${asked}`.trim()),
        { text: true, screen: false, task: "mock" },
      );
      if (question && "speechSynthesis" in window) {
        window.speechSynthesis.cancel();
        window.speechSynthesis.speak(new SpeechSynthesisUtterance(question));
      }
    } finally {
      answering.current = false;
      saveActiveCall();
    }
  };

  const beginListen = async () => {
    callId.current = `${Date.now()}-${crypto.randomUUID()}`;
    setMeetingApp(null);
    await transcript.start();
  };

  useEffect(() => {
    if (!callId.current || transcript.lines.length === 0) return;
    saveActiveCall();
  }, [transcript.lines, messages]);

  useEffect(() => {
    if (transcript.listening) return;
    let cancelled = false;
    const look = async () => {
      try {
        const name = await invoke<string | null>("detect_meeting_app");
        if (cancelled) return;
        if (!name) {
          dismissedMeeting.current = null;
          setMeetingApp(null);
          return;
        }
        if (dismissedMeeting.current !== name) setMeetingApp(name);
      } catch {
        if (!cancelled) setMeetingApp(null);
      }
    };
    void look();
    const timer = window.setInterval(() => void look(), 8000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [transcript.listening]);

  const sessionTitle = loadCallSession().title;

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

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => (transcript.listening ? transcript.stop() : void beginListen())}
          title={transcript.listening ? "Stop listening" : "Listen to the call"}
          className={`text-[10px] px-2 py-0.5 rounded-md border cursor-pointer ${
            transcript.listening
              ? "border-red-400/60 bg-red-500/30 text-red-100"
              : "border-white/20 text-white/60 hover:text-white"
          }`}
        >
          {transcript.listening ? "■ Stop" : "🎤 Listen"}
        </button>

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            const next = !includeScreen;
            setIncludeScreen(next);
            localStorage.setItem(SCREEN_KEY, next ? "1" : "0");
          }}
          title={includeScreen ? "Call answers include the screen" : "Call answers use the transcript only"}
          className={`text-[10px] px-2 py-0.5 rounded-md border cursor-pointer ${
            includeScreen
              ? "border-emerald-400/60 bg-emerald-500/30 text-emerald-100"
              : "border-white/20 text-white/60 hover:text-white"
          }`}
        >
          {includeScreen ? "🖥 On" : "🖥 Off"}
        </button>

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            const next = !quick;
            setQuick(next);
            localStorage.setItem(QUICK_KEY, next ? "1" : "0");
          }}
          title={quick ? "Fast reply: latest line only, no screen" : "Full reply: recent transcript, session, and screen"}
          className={`text-[10px] px-2 py-0.5 rounded-md border cursor-pointer ${
            quick
              ? "border-amber-400/60 bg-amber-500/30 text-amber-100"
              : "border-white/20 text-white/60 hover:text-white"
          }`}
        >
          {quick ? "Quick" : "Full"}
        </button>

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
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-scroll [scrollbar-gutter:stable]">
        <div className="px-3 py-2 flex flex-col gap-3">
        {meetingApp && !transcript.listening && (
          <div className="flex items-center justify-between gap-2 rounded-lg bg-emerald-500/15 px-3 py-2 text-xs text-emerald-100">
            <span>{meetingApp} is open.</span>
            <span className="flex gap-2">
              <button onClick={() => void beginListen()} className="underline cursor-pointer">Listen</button>
              <button
                onClick={() => {
                  dismissedMeeting.current = meetingApp;
                  setMeetingApp(null);
                }}
                className="text-white/50 cursor-pointer"
              >
                Not now
              </button>
            </span>
          </div>
        )}

        {messages.length === 0 && transcript.lines.length === 0 && !loading && !transcript.listening && (
          <div className="flex flex-col items-center gap-2 mt-8">
            <p className="text-white/30 text-xs text-center">
              Ask anything about your screen, or listen to a call
            </p>
            <button
              onClick={() => void askMock()}
              disabled={loading}
              className="text-[10px] text-white/50 hover:text-white cursor-pointer"
            >
              Start a mock interview
            </button>
          </div>
        )}

        {(transcript.listening || transcript.lines.length > 0 || transcript.error) && (
          <div className="flex flex-col gap-1.5 pb-2 border-b border-white/10">
            <div className="flex items-center justify-between">
              <span className="text-[10px] uppercase tracking-wide text-white/40 truncate">
                {sessionTitle || transcript.status || "Call"}
              </span>
              {(transcript.listening || transcript.lines.length > 0) && (
                <span className="flex items-center gap-2 shrink-0">
                  <button
                    onClick={answerNow}
                    disabled={loading}
                    className="text-[10px] text-white/40 hover:text-white/70 cursor-pointer disabled:opacity-40"
                  >
                    Answer
                  </button>
                  <button
                    onClick={() => void askMock()}
                    disabled={loading}
                    className="text-[10px] text-white/40 hover:text-white/70 cursor-pointer disabled:opacity-40"
                  >
                    Mock
                  </button>
                  <button
                    onClick={() => void writeNotes()}
                    disabled={loading}
                    className="text-[10px] text-white/40 hover:text-white/70 cursor-pointer disabled:opacity-40"
                  >
                    Notes
                  </button>
                  <button
                    onClick={transcript.clear}
                    className="text-[10px] text-white/40 hover:text-white/70 cursor-pointer"
                  >
                    Clear
                  </button>
                </span>
              )}
            </div>
            {transcript.lines.map((line) => (
              <TranscriptRow key={line.id} source={line.source} text={line.text} live={false} />
            ))}
            {(["them", "me"] as TranscriptSource[]).map((source) => {
              const text = transcript.partials[source];
              return text ? (
                <TranscriptRow key={source} source={source} text={text} live />
              ) : null;
            })}
            {transcript.error && (
              <div className="bg-red-500/20 rounded-lg px-3 py-2 text-sm text-red-300">{transcript.error}</div>
            )}
          </div>
        )}

        {messages.map((msg, i) => {
          const streaming = loading && i === messages.length - 1;
          return (
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
                      if (match?.[1] === "mermaid") {
                        return <MermaidChart chart={String(children).replace(/\n$/, "")} live={streaming} />;
                      }
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
          );
        })}

        {loading && messages[messages.length - 1]?.role === "user" && (
          <div className="flex flex-col gap-0.5 items-start">
            <span className="text-white/30 text-[10px] uppercase tracking-wide">Shiro</span>
            <div className="bg-white/10 rounded-lg px-3 py-2 text-sm text-white/50 animate-pulse">
              Thinking…
            </div>
          </div>
        )}

        {error && (
          <div className="bg-red-500/20 rounded-lg px-3 py-2 text-sm text-red-300">{error}</div>
        )}
        </div>
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
          <button
            onClick={() => void askAboutCall()}
            disabled={loading || !prompt.trim()}
            title="Ask using the call transcript, without a new screenshot"
            className="rounded-lg bg-white/10 hover:bg-white/20 disabled:opacity-30 text-white text-xs font-medium px-3 py-1.5 transition-colors border border-white/20"
          >
            Call
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
