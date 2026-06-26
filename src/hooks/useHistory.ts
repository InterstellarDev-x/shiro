import { useState, useEffect, useCallback } from "react";

export interface HistoryEntry {
  id: string;
  prompt: string;
  response: string;
  timestamp: number;
}

const KEY = "shiro_history";
const MAX = 50;

function load(): HistoryEntry[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]");
  } catch {
    return [];
  }
}

function save(entries: HistoryEntry[]) {
  localStorage.setItem(KEY, JSON.stringify(entries.slice(0, MAX)));
}

export function useHistory() {
  const [history, setHistory] = useState<HistoryEntry[]>(load);

  // Sync across windows via storage event
  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key === KEY) setHistory(load());
    };
    window.addEventListener("storage", handler);
    return () => window.removeEventListener("storage", handler);
  }, []);

  const addEntry = useCallback((prompt: string, response: string) => {
    const entry: HistoryEntry = {
      id: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
      prompt,
      response,
      timestamp: Date.now(),
    };
    setHistory((prev) => {
      const next = [entry, ...prev].slice(0, MAX);
      save(next);
      return next;
    });
    return entry.id;
  }, []);

  const clearHistory = useCallback(() => {
    localStorage.removeItem(KEY);
    setHistory([]);
  }, []);

  return { history, addEntry, clearHistory };
}
