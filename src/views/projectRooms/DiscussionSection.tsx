import React from "react";
import { ProjectDiscussionMessage } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  messages: ProjectDiscussionMessage[];
  author: string;
  draft: string;
  busy: boolean;
  onAuthorChange: (author: string) => void;
  onDraftChange: (draft: string) => void;
  onSend: () => void;
}

export const DiscussionSection: React.FC<Props> = ({
  messages,
  author,
  draft,
  busy,
  onAuthorChange,
  onDraftChange,
  onSend,
}) => {
  const { t } = useTranslation();

  return (
    <div className="space-y-3">
      <div className="rounded-xl border border-zinc-800 bg-dark-card p-4">
        <div className="flex gap-2">
          <select
            value={author}
            onChange={(event) => onAuthorChange(event.target.value)}
            className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          >
            <option value="user">User</option>
            <option value="chatgpt">ChatGPT</option>
            <option value="codex">Codex</option>
            <option value="gemini">Gemini</option>
          </select>
          <input
            value={draft}
            onChange={(event) => onDraftChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                onSend();
              }
            }}
            placeholder={t("project_rooms.discussion_placeholder")}
            className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          />
          <button
            type="button"
            disabled={busy || !draft.trim()}
            onClick={onSend}
            className="rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-xs text-zinc-200 disabled:opacity-40"
          >
            {t("project_rooms.send")}
          </button>
        </div>
      </div>

      {messages.map((message) => (
        <div
          key={message.id}
          className="rounded-xl border border-zinc-800 bg-dark-card p-4"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-[11px] font-semibold text-zinc-300">
              {message.author} → {message.recipients.join(", ")}
            </span>
            <span className="text-[10px] font-mono text-zinc-600">
              {message.created_at}
            </span>
          </div>
          <p className="mt-2 whitespace-pre-wrap text-xs leading-relaxed text-zinc-400">
            {message.message}
          </p>
        </div>
      ))}
    </div>
  );
};
