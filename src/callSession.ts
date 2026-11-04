export type CallMode = "meeting" | "interview";
export type AnswerStyle = "spoken" | "script" | "coding";

export interface CallSession {
  mode: CallMode;
  title: string;
  instructions: string;
  resume: string;
  document: string;
  style: AnswerStyle;
  language: string;
}

export const CALL_LANGUAGES: [string, string][] = [
  ["en", "English"],
  ["hi", "Hindi"],
  ["multi", "Multi"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["ja", "Japanese"],
  ["zh-CN", "Chinese"],
];

const KEY = "shiro_call_session";

const EMPTY: CallSession = {
  mode: "meeting",
  title: "",
  instructions: "",
  resume: "",
  document: "",
  style: "spoken",
  language: "en",
};

function clip(text: string, max: number): string {
  const trimmed = text.trim();
  return trimmed.length <= max ? trimmed : trimmed.slice(0, max);
}

export function loadCallSession(): CallSession {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...EMPTY };
    const parsed = JSON.parse(raw) as Partial<CallSession>;
    return {
      mode: parsed.mode === "interview" ? "interview" : "meeting",
      title: typeof parsed.title === "string" ? parsed.title : "",
      instructions: typeof parsed.instructions === "string" ? parsed.instructions : "",
      resume: typeof parsed.resume === "string" ? parsed.resume : "",
      document: typeof parsed.document === "string" ? parsed.document : "",
      style: parsed.style === "script" || parsed.style === "coding" ? parsed.style : "spoken",
      language: typeof parsed.language === "string" && parsed.language ? parsed.language : "en",
    };
  } catch {
    return { ...EMPTY };
  }
}

export function saveCallSession(session: CallSession) {
  localStorage.setItem(KEY, JSON.stringify(session));
}

/** Text the model should see before the transcript lines. */
export function sessionBrief(session: CallSession = loadCallSession()): string {
  const title = clip(session.title, 200);
  const kind = session.mode === "interview" ? "Interview" : "Meeting";
  const parts = [`Session: ${kind}${title ? ` — ${title}` : ""}`];
  parts.push(
    session.style === "coding"
      ? "Answer style: Give the approach, the time and space complexity, and the edge cases, in words the user can say. Add a short code sketch only when it helps."
      : session.style === "script"
        ? "Answer style: Give a short script the user can say. If the question is a system design, end with a flowchart inside a mermaid code fence. If code is on screen, walk through the approach and its complexity."
        : "Answer style: One or two short sentences the user can say out loud.",
  );
  const instructions = clip(session.instructions, 8000);
  if (instructions) parts.push(`Instructions:\n${instructions}`);
  if (session.mode === "interview") {
    const resume = clip(session.resume, 12000);
    if (resume) parts.push(`Resume:\n${resume}`);
  }
  const document = clip(session.document, 12000);
  if (document) parts.push(`Reference:\n${document}`);
  return parts.join("\n\n");
}

export function withSession(transcript: string): string {
  const brief = sessionBrief();
  const body = transcript.trim();
  return body ? `${brief}\n\nTranscript:\n${body}` : brief;
}
