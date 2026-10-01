import React, { useMemo } from "react";
import {
  AlertTriangle,
  Brain,
  CheckCircle2,
  Clock3,
  Copy,
  Gauge,
  Sparkles,
  Wrench,
} from "lucide-react";
import {
  AgentCapacity,
  AgentRuntimeInfo,
  ProjectRoomSnapshot,
  ProjectRoomSummary,
} from "../../types";
import { useTranslation } from "../../i18n";

type Translate = ReturnType<typeof useTranslation>["t"];

const capacityLabel = (
  capacity: AgentCapacity | undefined,
  t: Translate
) => {
  if (!capacity) return "—";
  if (capacity.source === "unavailable") return "—";
  if (capacity.remaining_percent == null) {
    return capacity.available
      ? t("project_rooms.quota_ready")
      : t("project_rooms.quota_offline");
  }
  return `${Math.round(capacity.remaining_percent)}%`;
};

const capacityTone = (capacity: AgentCapacity | undefined) => {
  if (!capacity || capacity.remaining_percent == null) {
    return "bg-zinc-700";
  }
  if (capacity.remaining_percent <= 10) return "bg-rose-500";
  if (capacity.remaining_percent <= 25) return "bg-amber-400";
  return "bg-emerald-500";
};

const telemetryBadge = (
  capacity: AgentCapacity | undefined,
  t: Translate
) => {
  if (!capacity) {
    return { label: t("project_rooms.quota_no_data"), className: "text-zinc-600" };
  }
  if (capacity.confidence === "runtime_telemetry") {
    return { label: t("project_rooms.quota_live"), className: "text-emerald-400" };
  }
  if (capacity.confidence === "stale_runtime_telemetry") {
    return {
      label: t("project_rooms.quota_stale_badge"),
      className: "text-amber-400",
    };
  }
  return { label: t("project_rooms.quota_no_data"), className: "text-zinc-600" };
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
  const { t, locale } = useTranslation();

  const quotaAlerts = useMemo(
    () =>
      room.agents.flatMap((agent) => {
        const capacity = capacities.get(agent.agent_id);
        if (!capacity) return [];

        if (
          capacity.remaining_percent != null &&
          capacity.remaining_percent <= 25
        ) {
          return [
            {
              id: `${agent.agent_id}-low`,
              agent: agent.display_name,
              text: t("project_rooms.quota_remaining", {
                percent: Math.round(capacity.remaining_percent),
              }),
              tone:
                capacity.remaining_percent <= 10
                  ? "text-rose-300"
                  : "text-amber-300",
            },
          ];
        }

        if (capacity.confidence === "stale_runtime_telemetry") {
          return [
            {
              id: `${agent.agent_id}-stale`,
              agent: agent.display_name,
              text: t("project_rooms.quota_stale"),
              tone: "text-amber-300",
            },
          ];
        }

        return [];
      }),
    [capacities, room.agents, t]
  );

  const formatTime = (value: string | null | undefined) => {
    if (!value) return null;
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return value;
    return new Intl.DateTimeFormat(locale, {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(parsed);
  };

  return (
    <div className="overflow-hidden rounded-2xl border border-zinc-800/90 bg-dark-card shadow-[0_18px_60px_rgba(0,0,0,0.18)]">
      <div className="border-b border-zinc-800/80 bg-gradient-to-r from-zinc-900/90 via-zinc-900/40 to-emerald-950/10 px-5 py-4">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2.5">
              <h3 className="text-xl font-semibold tracking-tight text-zinc-50">
                {room.config.name}
              </h3>
              <span className="rounded-md border border-zinc-700/80 bg-black/20 px-2 py-1 text-[10px] font-mono text-zinc-500">
                {room.config.id}
              </span>
            </div>

            <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[10px] font-mono text-zinc-500">
              <span className="max-w-[520px] truncate" title={room.config.local_root}>
                LOCAL · {room.config.local_root}
              </span>
              <span
                className="max-w-[520px] truncate"
                title={room.config.remote.root || undefined}
              >
                REMOTE ·{" "}
                {room.config.remote.root || t("project_rooms.not_configured")}
              </span>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-end gap-2 text-[10px]">
            <button
              type="button"
              disabled={busy}
              onClick={onOpenProjectPrompt}
              className="flex h-8 items-center gap-1.5 rounded-lg border border-emerald-800/60 bg-emerald-950/25 px-3 font-medium text-emerald-300 transition hover:border-emerald-700 hover:bg-emerald-950/45 disabled:opacity-40"
            >
              <Copy className="h-3.5 w-3.5" />
              {t("project_rooms.bind_chatgpt")}
            </button>

            <span
              className={`flex h-8 items-center gap-1.5 rounded-lg border px-2.5 ${
                selectedSummary?.session_status === "ready" ||
                selectedSummary?.session_status === "executing"
                  ? "border-emerald-900/70 bg-emerald-950/20 text-emerald-400"
                  : selectedSummary?.session_status === "starting"
                  ? "border-amber-900/70 bg-amber-950/20 text-amber-300"
                  : "border-zinc-800 bg-zinc-950/50 text-zinc-500"
              }`}
              title={selectedSummary?.session_id || undefined}
            >
              <span className="h-1.5 w-1.5 rounded-full bg-current" />
              Pi {selectedSummary?.session_status || "stopped"}
            </span>

            <span
              className={`flex h-8 items-center rounded-lg border px-2.5 ${
                selectedSummary?.remote_configured
                  ? "border-sky-900/70 bg-sky-950/20 text-sky-400"
                  : "border-zinc-800 bg-zinc-950/50 text-zinc-500"
              }`}
            >
              Remote {selectedSummary?.remote_configured ? "ready" : "unset"}
            </span>
          </div>
        </div>

        {quotaAlerts.length > 0 && (
          <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border border-amber-900/40 bg-amber-950/10 px-3.5 py-2.5">
            <div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-amber-300/80">
              <AlertTriangle className="h-3.5 w-3.5" />
              {t("project_rooms.capacity_attention")}
            </div>
            {quotaAlerts.map((alert) => (
              <div
                key={alert.id}
                className={`text-[11px] ${alert.tone}`}
              >
                <span className="font-medium">{alert.agent}</span>
                <span className="text-zinc-600"> · </span>
                {alert.text}
              </div>
            ))}
          </div>
        )}
      </div>

      {projectPrompt && (
        <div className="border-b border-zinc-800/80 bg-zinc-950/45 px-5 py-4">
          <div className="flex items-center justify-between gap-3">
            <div className="text-[11px] font-medium text-zinc-300">
              {t("project_rooms.chatgpt_prompt_title")}
            </div>
            <button
              type="button"
              onClick={onCopyProjectPrompt}
              className="flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-900 px-2.5 py-1.5 text-[10px] text-zinc-300 hover:text-zinc-100"
            >
              <Copy className="h-3 w-3" />
              {promptCopied
                ? t("project_rooms.copied")
                : t("project_rooms.copy_prompt")}
            </button>
          </div>
          <textarea
            readOnly
            value={projectPrompt}
            rows={6}
            className="mt-2.5 w-full resize-y rounded-lg border border-zinc-800 bg-black/25 px-3 py-2.5 text-[10px] font-mono leading-relaxed text-zinc-400 outline-none"
          />
        </div>
      )}

      <div className="grid grid-cols-3 divide-x divide-zinc-800/80">
        {room.agents.map((agent) => {
          const capacity = capacities.get(agent.agent_id);
          const runtime = runtimeMap.get(agent.agent_id);
          const telemetry = telemetryBadge(capacity, t);
          const AgentIcon =
            agent.agent_id === "chatgpt"
              ? Brain
              : agent.agent_id === "codex"
              ? Wrench
              : Sparkles;
          const remaining = capacity?.remaining_percent;

          return (
            <div
              key={agent.agent_id}
              className="min-w-0 bg-zinc-950/20 px-4 py-4 transition-colors hover:bg-zinc-900/30"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex min-w-0 items-start gap-2.5">
                  <div className="mt-0.5 rounded-lg border border-zinc-800 bg-zinc-900/80 p-2">
                    <AgentIcon className="h-4 w-4 text-zinc-300" />
                  </div>
                  <div className="min-w-0">
                    <div className="truncate text-xs font-semibold text-zinc-100">
                      {agent.display_name}
                    </div>
                    <div className="mt-0.5 truncate text-[10px] text-zinc-600">
                      {agent.role}
                    </div>
                  </div>
                </div>
                <div className="shrink-0 text-right">
                  <div className="text-lg font-semibold tabular-nums tracking-tight text-zinc-100">
                    {capacityLabel(capacity, t)}
                  </div>
                  <div
                    className={`mt-0.5 text-[9px] font-semibold tracking-[0.12em] ${telemetry.className}`}
                    title={capacity?.source}
                  >
                    {telemetry.label}
                  </div>
                </div>
              </div>

              <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-zinc-800/80">
                {remaining != null && (
                  <div
                    className={`h-full rounded-full transition-all ${capacityTone(
                      capacity
                    )}`}
                    style={{ width: `${Math.max(2, Math.min(100, remaining))}%` }}
                  />
                )}
              </div>

              <div className="mt-2.5 flex min-h-4 flex-wrap items-center gap-x-3 gap-y-1 text-[9px] font-mono text-zinc-600">
                <span
                  className={
                    runtime?.installed ? "text-emerald-500/80" : "text-zinc-600"
                  }
                >
                  {runtime?.installed ? (
                    <span className="inline-flex items-center gap-1">
                      <CheckCircle2 className="h-3 w-3" />
                      {t("project_rooms.runtime_ready")}
                    </span>
                  ) : (
                    t("project_rooms.runtime_unavailable")
                  )}
                </span>
                {capacity?.reset_at && (
                  <span className="inline-flex items-center gap-1">
                    <Clock3 className="h-3 w-3" />
                    {formatTime(capacity.reset_at)}
                  </span>
                )}
                {capacity?.updated_at && (
                  <span
                    className="inline-flex items-center gap-1"
                    title={capacity.updated_at}
                  >
                    <Gauge className="h-3 w-3" />
                    {formatTime(capacity.updated_at)}
                  </span>
                )}
              </div>

              <div className="mt-2.5 line-clamp-2 text-[10px] leading-relaxed text-zinc-500">
                {agent.review_rule}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
