import React from "react";
import { Brain, Copy, Sparkles, Wrench } from "lucide-react";
import {
  AgentCapacity,
  AgentRuntimeInfo,
  ProjectRoomSnapshot,
  ProjectRoomSummary,
} from "../../types";
import { useTranslation } from "../../i18n";

const capacityLabel = (capacity: AgentCapacity | undefined) => {
  if (!capacity) return "—";
  if (capacity.source === "unavailable") return "quota ?";
  if (capacity.remaining_percent == null) {
    return capacity.available ? "available" : "unavailable";
  }
  const value = `${Math.round(capacity.remaining_percent)}%`;
  return capacity.confidence === "stale_runtime_telemetry"
    ? `${value} · stale`
    : value;
};

interface Props {
  room: ProjectRoomSnapshot;
  selectedSummary: ProjectRoomSummary | null;
  busy: boolean;
  capacities: Map<string, AgentCapacity>;
  runtimeMap: Map<string, AgentRuntimeInfo>;
  projectPrompt: string | null;
  promptCopied: boolean;
  onOpenProjectPrompt: () => void;
  onCopyProjectPrompt: () => void;
}

export const ProjectRoomHeader: React.FC<Props> = ({
  room,
  selectedSummary,
  busy,
  capacities,
  runtimeMap,
  projectPrompt,
  promptCopied,
  onOpenProjectPrompt,
  onCopyProjectPrompt,
}) => {
  const { t } = useTranslation();

  return (
    <div className="rounded-xl border border-zinc-800 bg-dark-card p-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-xl font-semibold text-zinc-100">
              {room.config.name}
            </h3>
            <span className="rounded border border-zinc-700 bg-zinc-900 px-2 py-0.5 text-[10px] font-mono text-zinc-500">
              project://{room.config.id}
            </span>
          </div>
          <div className="mt-2 space-y-1 text-[11px] font-mono text-zinc-500">
            <div>Local: {room.config.local_root}</div>
            <div>
              Remote: {room.config.remote.root || t("project_rooms.not_configured")}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 text-[10px]">
          <button
            type="button"
            disabled={busy}
            onClick={onOpenProjectPrompt}
            className="flex items-center gap-1.5 rounded border border-emerald-800/60 bg-emerald-950/30 px-2.5 py-1 text-emerald-300 hover:bg-emerald-950/50 disabled:opacity-40"
          >
            <Copy className="w-3 h-3" />
            {t("project_rooms.bind_chatgpt")}
          </button>
          <span
            className={`rounded border px-2 py-1 ${
              selectedSummary?.session_status === "ready" ||
              selectedSummary?.session_status === "executing"
                ? "border-emerald-800 bg-emerald-950/30 text-emerald-400"
                : selectedSummary?.session_status === "starting"
                ? "border-amber-800 bg-amber-950/30 text-amber-300"
                : "border-zinc-700 bg-zinc-900 text-zinc-500"
            }`}
            title={selectedSummary?.session_id || undefined}
          >
            Pi {selectedSummary?.session_status || "stopped"}
          </span>
          <span
            className={`rounded border px-2 py-1 ${
              selectedSummary?.git_initialized
                ? "border-emerald-800 bg-emerald-950/30 text-emerald-400"
                : "border-zinc-700 bg-zinc-900 text-zinc-500"
            }`}
          >
            Git {selectedSummary?.git_initialized ? "ready" : "off"}
          </span>
          <span
            className={`rounded border px-2 py-1 ${
              selectedSummary?.remote_configured
                ? "border-sky-800 bg-sky-950/30 text-sky-400"
                : "border-zinc-700 bg-zinc-900 text-zinc-500"
            }`}
          >
            Remote {selectedSummary?.remote_configured ? "ready" : "unset"}
          </span>
        </div>
      </div>

      {projectPrompt && (
        <div className="mt-4 rounded-lg border border-emerald-900/50 bg-zinc-950/80 p-3">
          <div className="flex items-center justify-between gap-3">
            <div className="text-[11px] font-medium text-zinc-300">
              {t("project_rooms.chatgpt_prompt_title")}
            </div>
            <button
              type="button"
              onClick={onCopyProjectPrompt}
              className="flex items-center gap-1.5 rounded border border-zinc-700 bg-zinc-900 px-2.5 py-1 text-[10px] text-zinc-300 hover:text-zinc-100"
            >
              <Copy className="w-3 h-3" />
              {promptCopied
                ? t("project_rooms.copied")
                : t("project_rooms.copy_prompt")}
            </button>
          </div>
          <textarea
            readOnly
            value={projectPrompt}
            rows={8}
            className="mt-2 w-full resize-y rounded border border-zinc-800 bg-black/30 px-3 py-2 text-[10px] font-mono leading-relaxed text-zinc-400 outline-none"
          />
        </div>
      )}

      <div className="mt-5 grid grid-cols-3 gap-3">
        {room.agents.map((agent) => {
          const capacity = capacities.get(agent.agent_id);
          const runtime = runtimeMap.get(agent.agent_id);
          const AgentIcon =
            agent.agent_id === "chatgpt"
              ? Brain
              : agent.agent_id === "codex"
              ? Wrench
              : Sparkles;

          return (
            <div
              key={agent.agent_id}
              className="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3 min-w-0"
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  <AgentIcon className="w-4 h-4 text-zinc-400 shrink-0" />
                  <span className="text-xs font-semibold text-zinc-200 truncate">
                    {agent.display_name}
                  </span>
                </div>
                <span className="text-[10px] font-mono text-zinc-500">
                  {capacityLabel(capacity)}
                </span>
              </div>
              <div className="mt-2 text-[10px] text-zinc-500 leading-relaxed">
                {agent.role}
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-[9px] font-mono text-zinc-600">
                <span
                  className={
                    runtime?.installed ? "text-emerald-500/80" : "text-zinc-600"
                  }
                >
                  {runtime?.installed
                    ? t("project_rooms.runtime_ready")
                    : t("project_rooms.runtime_unavailable")}
                </span>
                {runtime?.version && <span>{runtime.version}</span>}
                {capacity?.source && (
                  <span>
                    {capacity.source} / {capacity.confidence}
                  </span>
                )}
                {capacity?.reset_at && (
                  <span>
                    {t("project_rooms.reset_at")}: {capacity.reset_at}
                  </span>
                )}
                {capacity?.updated_at && (
                  <span>
                    {t("project_rooms.telemetry_updated")}: {capacity.updated_at}
                  </span>
                )}
              </div>
              <div className="mt-2 text-[10px] text-amber-300/70 leading-relaxed">
                {agent.review_rule}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
