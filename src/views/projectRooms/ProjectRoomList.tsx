import React from "react";
import { ProjectRoomSummary } from "../../types";

interface Props {
  summaries: ProjectRoomSummary[];
  selectedId: string | null;
  onSelect: (projectId: string) => void;
}

export const ProjectRoomList: React.FC<Props> = ({
  summaries,
  selectedId,
  onSelect,
}) => (
  <div className="space-y-2">
    {summaries.map((item) => (
      <button
        type="button"
        key={item.id}
        onClick={() => onSelect(item.id)}
        className={`w-full text-left rounded-xl border p-4 transition-colors ${
          selectedId === item.id
            ? "border-emerald-700/70 bg-emerald-950/15"
            : "border-zinc-800 bg-dark-card hover:border-zinc-700"
        }`}
      >
        <div className="flex items-center justify-between gap-2">
          <span className="text-sm font-semibold text-zinc-100">{item.name}</span>
          <span
            className={`w-2 h-2 rounded-full ${
              item.session_status === "ready" ||
              item.session_status === "executing"
                ? "bg-emerald-500"
                : item.session_status === "starting"
                ? "bg-amber-500"
                : "bg-zinc-600"
            }`}
            title={`Pi Session: ${item.session_status}`}
          />
        </div>
        <div className="mt-2 text-[10px] font-mono text-zinc-500 truncate">
          {item.local_root}
        </div>
        <div className="mt-2 text-[10px] font-mono text-zinc-600">
          Pi: {item.session_status}
          {item.binding_count > 0 ? ` · ${item.binding_count} bound` : ""}
        </div>
        <div className="mt-2 grid grid-cols-3 gap-1 text-[10px] text-zinc-500">
          <span>{item.active_tasks} task</span>
          <span>{item.experiments} exp</span>
          <span>{item.discussion_messages} msg</span>
        </div>
      </button>
    ))}
  </div>
);
