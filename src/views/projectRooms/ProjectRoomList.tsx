import React from "react";
import { Beaker, MessageSquareText, Radio, SquareCheckBig } from "lucide-react";
import { ProjectRoomSummary } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  summaries: ProjectRoomSummary[];
  selectedId: string | null;
  onSelect: (projectId: string) => void;
}

const sessionDot = (status: string) => {
  if (status === "ready" || status === "executing") return "bg-emerald-400";
  if (status === "starting") return "bg-amber-400";
  return "bg-zinc-600";
};

export const ProjectRoomList: React.FC<Props> = ({
  summaries,
  selectedId,
  onSelect,
}) => {
  const { t } = useTranslation();

  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-2 lg:sticky lg:top-4 lg:block lg:space-y-2">
      <div className="col-span-full mb-1 flex items-center justify-between px-1 lg:mb-3">
        <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-zinc-600">
          {t("project_rooms.project_list")}
        </span>
        <span className="rounded-md border border-zinc-800 bg-zinc-900/70 px-1.5 py-0.5 text-[9px] font-mono text-zinc-600">
          {summaries.length}
        </span>
      </div>

      {summaries.map((item) => {
        const selected = selectedId === item.id;
        return (
          <button
            type="button"
            key={item.id}
            onClick={() => onSelect(item.id)}
            className={`group relative w-full overflow-hidden rounded-xl border p-3.5 text-left transition-all ${
              selected
                ? "border-zinc-600 bg-zinc-900 shadow-[0_10px_32px_rgba(0,0,0,0.18)]"
                : "border-zinc-800/80 bg-dark-card hover:border-zinc-700 hover:bg-zinc-900/60"
            }`}
          >
            {selected && (
              <span className="absolute inset-y-3 left-0 w-0.5 rounded-r bg-emerald-400" />
            )}

            <div className="flex items-center justify-between gap-3">
              <span
                className={`truncate text-[13px] font-semibold ${
                  selected ? "text-zinc-50" : "text-zinc-200"
                }`}
              >
                {item.name}
              </span>
              <span className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-zinc-800 bg-black/20 px-2 py-1 text-[9px] font-medium text-zinc-500">
                <span
                  className={`h-1.5 w-1.5 rounded-full ${sessionDot(
                    item.session_status
                  )}`}
                />
                {item.session_status}
              </span>
            </div>

            <div
              className="mt-2 truncate text-[9px] font-mono text-zinc-600"
              title={item.local_root}
            >
              {item.local_root}
            </div>

            <div className="mt-3 grid grid-cols-3 gap-1.5">
              <div className="rounded-lg border border-zinc-800/70 bg-zinc-950/35 px-2 py-1.5">
                <div className="flex items-center gap-1 text-zinc-600">
                  <SquareCheckBig className="h-3 w-3" />
                  <span className="text-[8px] uppercase tracking-wide">Task</span>
                </div>
                <div className="mt-0.5 text-[11px] font-semibold tabular-nums text-zinc-300">
                  {item.active_tasks}
                </div>
              </div>
              <div className="rounded-lg border border-zinc-800/70 bg-zinc-950/35 px-2 py-1.5">
                <div className="flex items-center gap-1 text-zinc-600">
                  <Beaker className="h-3 w-3" />
                  <span className="text-[8px] uppercase tracking-wide">Exp</span>
                </div>
                <div className="mt-0.5 text-[11px] font-semibold tabular-nums text-zinc-300">
                  {item.experiments}
                </div>
              </div>
              <div className="rounded-lg border border-zinc-800/70 bg-zinc-950/35 px-2 py-1.5">
                <div className="flex items-center gap-1 text-zinc-600">
                  <MessageSquareText className="h-3 w-3" />
                  <span className="text-[8px] uppercase tracking-wide">Msg</span>
                </div>
                <div className="mt-0.5 text-[11px] font-semibold tabular-nums text-zinc-300">
                  {item.discussion_messages}
                </div>
              </div>
            </div>

            {item.binding_count > 0 && (
              <div className="mt-2.5 flex items-center gap-1.5 text-[9px] text-emerald-500/70">
                <Radio className="h-3 w-3" />
                {item.binding_count} web binding
              </div>
            )}
          </button>
        );
      })}
    </div>
  );
};
