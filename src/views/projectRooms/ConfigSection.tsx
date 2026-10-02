import React from "react";
import { GitBranch, Save } from "lucide-react";
import { ProjectRoomConfig, ProjectRoomSummary } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  draft: ProjectRoomConfig;
  selectedSummary: ProjectRoomSummary | null;
  busy: boolean;
  onChange: (config: ProjectRoomConfig) => void;
  onSave: () => void;
  onInitGit: () => void;
}

export const ConfigSection: React.FC<Props> = ({
  draft,
  selectedSummary,
  busy,
  onChange,
  onSave,
  onInitGit,
}) => {
  const { t } = useTranslation();

  return (
    <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-semibold text-zinc-200">
          {t("project_rooms.project_config")}
        </h4>
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
          ["name", t("project_rooms.project_name")],
          ["local_root", "Local root"],
          ["repo_root", "Repo root"],
        ] as Array<[keyof ProjectRoomConfig, string]>
      ).map(([key, label]) => (
        <label key={key} className="block space-y-1.5">
          <span className="text-[11px] text-zinc-500">{label}</span>
          <input
            value={String(draft[key] ?? "")}
            onChange={(event) =>
              onChange({
                ...draft,
                [key]: event.target.value,
              })
            }
            className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
          />
        </label>
      ))}

      <label className="block space-y-1.5">
        <span className="text-[11px] text-zinc-500">Codex Thread ID</span>
        <input
          value={draft.codex_thread_id ?? ""}
          onChange={(event) =>
            onChange({
              ...draft,
              codex_thread_id: event.target.value.trim() || null,
            })
          }
          placeholder="existing Codex Desktop thread UUID"
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <div className="text-[10px] text-zinc-600">
          Human/canonical Codex conversation. TunnelDock forks it once into a background automation thread so tasks do not depend on which Desktop chat is currently open.
        </div>
      </label>

      <label className="block space-y-1.5">
        <span className="text-[11px] text-zinc-500">Codex Automation Thread ID</span>
        <input
          value={draft.codex_automation_thread_id ?? "Not created yet"}
          readOnly
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950/60 px-3 py-2 text-xs font-mono text-zinc-500"
        />
        <div className="text-[10px] text-zinc-600">
          Managed by TunnelDock. It inherits context from the human thread and is used for background turns.
        </div>
      </label>

      <label className="block space-y-1.5">
        <span className="text-[11px] text-zinc-500">
          Antigravity Cascade ID
        </span>
        <input
          value={draft.antigravity_cascade_id ?? ""}
          onChange={(event) =>
            onChange({
              ...draft,
              antigravity_cascade_id: event.target.value.trim() || null,
            })
          }
          placeholder="auto-discover from Local root when unique"
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <div className="text-[10px] text-zinc-600">
          Leave empty to auto-bind the unique existing Cascade whose workspace matches Local root.
        </div>
      </label>

      <div className="grid grid-cols-2 gap-3">
        <input
          value={draft.remote.host}
          onChange={(event) =>
            onChange({
              ...draft,
              remote: {
                ...draft.remote,
                host: event.target.value,
              },
            })
          }
          placeholder="remote host"
          className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <input
          value={draft.remote.root}
          onChange={(event) =>
            onChange({
              ...draft,
              remote: {
                ...draft.remote,
                root: event.target.value,
              },
            })
          }
          placeholder="remote root"
          className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <input
          value={draft.remote.environment}
          onChange={(event) =>
            onChange({
              ...draft,
              remote: {
                ...draft.remote,
                environment: event.target.value,
              },
            })
          }
          placeholder="conda / runtime environment"
          className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <input
          value={draft.remote.notes}
          onChange={(event) =>
            onChange({
              ...draft,
              remote: {
                ...draft.remote,
                notes: event.target.value,
              },
            })
          }
          placeholder="remote notes"
          className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
        />
      </div>

      <label className="flex items-center justify-between gap-4 rounded-lg border border-zinc-800 bg-zinc-950/60 px-3 py-3">
        <div>
          <div className="text-xs font-medium text-zinc-300">
            {t("project_rooms.keep_session_alive")}
          </div>
          <div className="mt-1 text-[10px] text-zinc-600">
            {t("project_rooms.keep_session_alive_desc")}
          </div>
        </div>
        <input
          type="checkbox"
          checked={draft.keep_session_alive}
          onChange={(event) =>
            onChange({
              ...draft,
              keep_session_alive: event.target.checked,
            })
          }
          className="h-4 w-4 accent-emerald-500"
        />
      </label>

      <div className="border-t border-zinc-800 pt-4">
        <button
          type="button"
          onClick={onInitGit}
          disabled={busy || selectedSummary?.git_initialized}
          className="flex items-center gap-2 rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-2 text-xs text-zinc-300 disabled:opacity-40"
        >
          <GitBranch className="w-3.5 h-3.5" />
          {selectedSummary?.git_initialized
            ? t("project_rooms.git_ready")
            : t("project_rooms.init_git")}
        </button>
      </div>
    </div>
  );
};
