import { useState } from "react";
import { CallMode, CallSession, AnswerStyle, CALL_LANGUAGES, loadCallSession, saveCallSession } from "../callSession";
import { readUploadedDocument } from "../readDocument";

const fieldClass =
  "rounded-lg border border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500";

export default function CallSessionForm() {
  const [session, setSession] = useState<CallSession>(() => loadCallSession());

  const [fileError, setFileError] = useState("");

  const update = (patch: Partial<CallSession>) => {
    const next = { ...session, ...patch };
    setSession(next);
    saveCallSession(next);
  };

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    setFileError("");
    try {
      const text = await readUploadedDocument(file);
      const document = session.document.trim() ? `${session.document.trim()}\n\n${text}` : text;
      update({ document });
    } catch (error) {
      setFileError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <section className="shrink-0 flex flex-col gap-3 rounded-xl border border-zinc-200 dark:border-zinc-700 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Call session</h2>
          <p className="text-xs text-zinc-400">The overlay uses this on the next question and when you write notes.</p>
        </div>
        <input
          value={session.title}
          onChange={(e) => update({ title: e.target.value })}
          placeholder="Session title"
          className={`${fieldClass} w-48`}
        />
      </div>

      <div className="flex flex-wrap gap-2">
        <Toggle
          value={session.mode}
          options={[
            ["meeting", "Meeting"],
            ["interview", "Interview"],
          ]}
          onChange={(mode) => update({ mode: mode as CallMode })}
        />
        <Toggle
          value={session.style}
          options={[
            ["spoken", "Spoken"],
            ["script", "Script"],
            ["coding", "Coding"],
          ]}
          onChange={(style) => update({ style: style as AnswerStyle })}
        />
        <select
          value={session.language}
          onChange={(e) => update({ language: e.target.value })}
          className={fieldClass}
        >
          {CALL_LANGUAGES.map(([id, label]) => (
            <option key={id} value={id}>{label}</option>
          ))}
        </select>
      </div>

      <textarea
        value={session.instructions}
        onChange={(e) => update({ instructions: e.target.value })}
        rows={2}
        placeholder="Instructions for this call. Example: answer as a backend engineer, keep it concrete."
        className={fieldClass}
      />
      {session.mode === "interview" && (
        <textarea
          value={session.resume}
          onChange={(e) => update({ resume: e.target.value })}
          rows={3}
          placeholder="Paste a resume. Interview mode sends it with each answer."
          className={fieldClass}
        />
      )}
      <label className="text-xs text-zinc-500">
        Add a text or PDF file
        <input
          type="file"
          accept=".txt,.md,.pdf,text/plain,application/pdf"
          onChange={(e) => {
            void onFile(e.target.files?.[0]);
            e.target.value = "";
          }}
          className="mt-1 block w-full text-xs"
        />
      </label>
      {fileError && <p className="text-xs text-red-500">{fileError}</p>}
      <textarea
        value={session.document}
        onChange={(e) => update({ document: e.target.value })}
        rows={3}
        placeholder="Reference notes, a job description, or a playbook."
        className={fieldClass}
      />
    </section>
  );
}

function Toggle({
  value,
  options,
  onChange,
}: {
  value: string;
  options: [string, string][];
  onChange: (value: string) => void;
}) {
  return (
    <div className="flex rounded-lg border border-zinc-200 dark:border-zinc-700 overflow-hidden text-xs">
      {options.map(([id, label]) => (
        <button
          key={id}
          onClick={() => onChange(id)}
          className={`px-3 py-1.5 ${
            value === id
              ? "bg-blue-600 text-white"
              : "text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
