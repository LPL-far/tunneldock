import React from "react";
import { Save } from "lucide-react";
import { MemoryHealth, ProjectMemory } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  draft: ProjectMemory;
  health: MemoryHealth;
  busy: boolean;
  onChange: (memory: ProjectMemory) => void;
  onSave: () => void;
}

export const MemorySection: React.FC<Props> = ({
  draft,
  health,
  busy,
  onChange,
  onSave,
}) => {
  const { t } = useTranslation();

  const editors: Array<[keyof ProjectMemory, string, number]> = [
    ["project_state", t("project_rooms.project_state"), 12],
    ["session_handoff", t("project_rooms.session_handoff"), 8],
    ["model_design", t("project_rooms.model_design"), 11],
    ["data_catalog", t("project_rooms.data_catalog"), 11],
    ["experiments", t("project_rooms.memory_experiments"), 10],
    ["results", t("project_rooms.results"), 10],
    ["references", t("project_rooms.references"), 9],
    ["documents", t("project_rooms.documents"), 9],
    ["decisions", t("project_rooms.decisions"), 8],
  ];

  return (
    <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-semibold text-zinc-200">
            {t("project_rooms.memory_title")}
          </h4>
          <p className="mt-1 max-w-3xl text-[11px] text-zinc-500">
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

      <div className="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <div className="text-[11px] font-medium text-zinc-300">
              {t("project_rooms.memory_governor")}
            </div>
            <div className="mt-1 text-[10px] text-zinc-600">
              {t("project_rooms.memory_current")}: {(health.total_current_bytes / 1024).toFixed(1)} KB · {t("project_rooms.memory_archive")}: {(health.total_archive_bytes / 1024).toFixed(1)} KB · {t("project_rooms.memory_events")}: {health.ledger_events}
            </div>
          </div>
          <span className={`rounded-md px-2 py-1 text-[10px] ${health.requires_compaction ? "bg-amber-950/60 text-amber-300" : health.near_budget ? "bg-yellow-950/50 text-yellow-300" : "bg-emerald-950/40 text-emerald-400"}`}>
            {health.requires_compaction
              ? t("project_rooms.memory_compaction_required")
              : health.near_budget
              ? t("project_rooms.memory_near_budget")
              : t("project_rooms.memory_healthy")}
          </span>
        </div>
        <div className="mt-3 grid gap-1.5 sm:grid-cols-2 xl:grid-cols-3">
          {health.files.map((file) => (
            <div key={file.file} className="rounded-md border border-zinc-900 bg-black/20 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2 text-[9px]">
                <span className="truncate font-mono text-zinc-500" title={file.file}>{file.file}</span>
                <span className={file.status === "over_budget" ? "text-amber-300" : file.status === "near_budget" ? "text-yellow-300" : "text-zinc-600"}>
                  {Math.round(file.utilization * 100)}%
                </span>
              </div>
              <div className="mt-1 h-1 overflow-hidden rounded bg-zinc-900">
                <div
                  className="h-full bg-zinc-600"
                  style={{ width: `${Math.min(100, file.utilization * 100)}%` }}
                />
              </div>
            </div>
          ))}
        </div>
      </div>

      {editors.map(([key, label, rows]) => (
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
          {t("project_rooms.memory_index")}
        </summary>
        <pre className="border-t border-zinc-800 px-3 py-3 whitespace-pre-wrap text-[10px] leading-relaxed text-zinc-500">
          {draft.memory_index}
        </pre>
      </details>

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
