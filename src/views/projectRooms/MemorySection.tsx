import React from "react";
import { Save } from "lucide-react";
import { ProjectMemory } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  draft: ProjectMemory;
  busy: boolean;
  onChange: (memory: ProjectMemory) => void;
  onSave: () => void;
}

export const MemorySection: React.FC<Props> = ({
  draft,
  busy,
  onChange,
  onSave,
}) => {
  const { t } = useTranslation();

  return (
    <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-semibold text-zinc-200">
            {t("project_rooms.memory_title")}
          </h4>
          <p className="mt-1 text-[11px] text-zinc-500">
            {t("project_rooms.memory_desc")}
          </p>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={onSave}
          className="flex items-center gap-1.5 rounded-lg border border-emerald-800/60 bg-emerald-950/30 px-3 py-2 text-xs text-emerald-300 disabled:opacity-50"
        >
          <Save className="w-3.5 h-3.5" />
          {t("common.save")}
        </button>
      </div>

      {(
        [
          ["project_state", t("project_rooms.project_state"), 14],
          ["session_handoff", t("project_rooms.session_handoff"), 8],
          ["decisions", t("project_rooms.decisions"), 10],
          ["experiments", t("project_rooms.memory_experiments"), 10],
        ] as Array<[keyof ProjectMemory, string, number]>
      ).map(([key, label, rows]) => (
        <label key={key} className="block space-y-1.5">
          <span className="text-[11px] font-medium text-zinc-400">{label}</span>
          <textarea
            value={String(draft[key] ?? "")}
            onChange={(event) =>
              onChange({
                ...draft,
                [key]: event.target.value,
              })
            }
            rows={rows}
            className="w-full resize-y rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono leading-relaxed text-zinc-200 outline-none focus:border-zinc-600"
          />
        </label>
      ))}

      <details className="rounded-lg border border-zinc-800 bg-zinc-950/60">
        <summary className="cursor-pointer px-3 py-2 text-[11px] text-zinc-500">
          {t("project_rooms.memory_protocol")}
        </summary>
        <pre className="border-t border-zinc-800 px-3 py-3 whitespace-pre-wrap text-[10px] leading-relaxed text-zinc-500">
          {draft.memory_protocol}
        </pre>
      </details>
    </div>
  );
};
