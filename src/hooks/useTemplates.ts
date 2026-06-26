import { useState, useCallback } from "react";

export interface Template {
  id: string;
  label: string;
  prompt: string;
}

const DEFAULT_TEMPLATES: Template[] = [
  { id: "explain", label: "Explain", prompt: "Explain what is on my screen in simple terms." },
  { id: "debug", label: "Debug", prompt: "There is an error or bug on my screen. Help me understand and fix it." },
  { id: "summarize", label: "Summarize", prompt: "Summarize the key points from what is on my screen." },
  { id: "code-review", label: "Review code", prompt: "Review the code on my screen. Point out issues, improvements, and anything suspicious." },
  { id: "write-tests", label: "Write tests", prompt: "Write unit tests for the code on my screen." },
  { id: "translate", label: "Translate", prompt: "Translate all text on my screen to English." },
];

const STORAGE_KEY = "shiro_templates";

function load(): Template[] {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved ? JSON.parse(saved) : DEFAULT_TEMPLATES;
  } catch {
    return DEFAULT_TEMPLATES;
  }
}

export function useTemplates() {
  const [templates, setTemplates] = useState<Template[]>(load);

  const saveTemplates = useCallback((next: Template[]) => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    setTemplates(next);
  }, []);

  const addTemplate = useCallback((label: string, prompt: string) => {
    const t: Template = { id: `custom-${Date.now()}`, label, prompt };
    setTemplates((prev) => {
      const next = [...prev, t];
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const removeTemplate = useCallback((id: string) => {
    setTemplates((prev) => {
      const next = prev.filter((t) => t.id !== id);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const resetToDefaults = useCallback(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(DEFAULT_TEMPLATES));
    setTemplates(DEFAULT_TEMPLATES);
  }, []);

  return { templates, addTemplate, removeTemplate, resetToDefaults, saveTemplates };
}
