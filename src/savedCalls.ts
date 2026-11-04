import type { CallMode, AnswerStyle } from "./callSession";
import type { TranscriptSource } from "./hooks/useTranscript";

export interface SavedAnswer {
  question: string;
  answer: string;
}

export interface SavedCall {
  id: string;
  title: string;
  mode: CallMode;
  style: AnswerStyle;
  language: string;
  startedAt: number;
  updatedAt: number;
  lines: { source: TranscriptSource; text: string }[];
  answers: SavedAnswer[];
  notes: string;
}

const KEY = "shiro_saved_calls";
const MAX_CALLS = 30;

export function listSavedCalls(): SavedCall[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as SavedCall[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function upsertSavedCall(call: SavedCall) {
  const rest = listSavedCalls().filter((item) => item.id !== call.id);
  const next = [call, ...rest].slice(0, MAX_CALLS);
  localStorage.setItem(KEY, JSON.stringify(next));
}
