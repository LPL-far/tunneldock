import React from "react";
import { LoaderCircle, Sparkles, Wrench } from "lucide-react";
import {
  AgentRuntimeInfo,
  ProjectRoomSnapshot,
  ProjectTask,
} from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  room: ProjectRoomSnapshot;
  taskDraft: ProjectTask;
  taskScope: string;
  busy: boolean;
  dispatchingTaskId: string | null;
  runtimeMap: Map<string, AgentRuntimeInfo>;
  onTaskDraftChange: (task: ProjectTask) => void;
  onTaskScopeChange: (scope: string) => void;
  onCreate: () => void;
  onDispatch: (task: ProjectTask) => void;
}

export const TasksSection: React.FC<Props> = ({
  room,
  taskDraft,
  taskScope,
  busy,
  dispatchingTaskId,
  runtimeMap,
  onTaskDraftChange,
  onTaskScopeChange,
  onCreate,
  onDispatch,
}) => {
  const { t } = useTranslation();

  return (
    <div className="space-y-3">
      <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-3">
        <h4 className="text-sm font-semibold text-zinc-200">
          {t("project_rooms.new_task")}
        </h4>
        <input
          value={taskDraft.title}
          onChange={(event) =>
            onTaskDraftChange({ ...taskDraft, title: event.target.value })
          }
          placeholder={t("project_rooms.task_title")}
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200 outline-none"
        />
        <textarea
          value={taskDraft.goal}
          onChange={(event) =>
            onTaskDraftChange({ ...taskDraft, goal: event.target.value })
          }
          placeholder={t("project_rooms.task_goal")}
          rows={3}
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200 outline-none"
        />
        <div className="grid grid-cols-2 gap-3">
          <select
            value={taskDraft.owner}
            onChange={(event) =>
              onTaskDraftChange({ ...taskDraft, owner: event.target.value })
            }
            className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          >
            <option value="codex">Codex</option>
            <option value="gemini">Gemini</option>
            <option value="chatgpt">ChatGPT</option>
            <option value="user">User</option>
          </select>
          <input
            value={taskScope}
            onChange={(event) => onTaskScopeChange(event.target.value)}
            placeholder="write scope: model/**, visualization/**"
            className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          />
        </div>
        <button
          type="button"
          disabled={busy || !taskDraft.title.trim()}
          onClick={onCreate}
          className="rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-xs text-zinc-200 disabled:opacity-40"
        >
          {t("project_rooms.create_task")}
        </button>
      </div>

      {room.tasks.map((task) => (
        <div
          key={task.id}
          className="rounded-xl border border-zinc-800 bg-dark-card p-4"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-zinc-200">
                {task.title}
              </span>
              <span className="rounded bg-zinc-900 px-1.5 py-0.5 text-[10px] text-zinc-500">
                {task.status}
              </span>
              {task.kind === "consultation" && (
                <span className="rounded bg-sky-950/50 px-1.5 py-0.5 text-[10px] text-sky-400">
                  consult
                </span>
              )}
            </div>
            <span className="text-[10px] font-mono text-zinc-500">
              {task.owner} → {task.reviewers.join(", ")}
            </span>
          </div>

          {task.goal && (
            <p className="mt-2 text-[11px] leading-relaxed text-zinc-400">
              {task.goal}
            </p>
          )}
          {task.write_scope.length > 0 && (
            <div className="mt-2 text-[10px] font-mono text-zinc-600">
              write: {task.write_scope.join(", ")}
            </div>
          )}

          {["codex", "gemini"].includes(task.owner) && (
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={
                  dispatchingTaskId === task.id ||
                  task.status === "active" ||
                  !runtimeMap.get(task.owner)?.installed
                }
                onClick={() => onDispatch(task)}
                className="flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-1.5 text-[10px] font-medium text-zinc-300 hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-40"
              >
                {dispatchingTaskId === task.id ? (
                  <LoaderCircle className="w-3 h-3 animate-spin" />
                ) : task.owner === "codex" ? (
                  <Wrench className="w-3 h-3" />
                ) : (
                  <Sparkles className="w-3 h-3" />
                )}
                {task.owner === "codex"
                  ? t("project_rooms.dispatch_codex")
                  : t("project_rooms.dispatch_gemini")}
              </button>
              {!runtimeMap.get(task.owner)?.installed && (
                <span className="text-[10px] text-amber-400/70">
                  {t("project_rooms.worker_not_found")}
                </span>
              )}
            </div>
          )}

          {room.runs
            .filter((run) => run.task_id === task.id)
            .slice(0, 1)
            .map((run) => (
              <div
                key={run.id}
                className="mt-3 rounded-lg border border-zinc-800 bg-zinc-950/70 px-3 py-2 text-[10px]"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-mono text-zinc-500">{run.id}</span>
                  <span
                    className={
                      run.status === "completed"
                        ? "text-emerald-400"
                        : run.status === "failed"
                        ? "text-rose-400"
                        : "text-amber-300"
                    }
                  >
                    {run.agent_id} · {run.status}
                  </span>
                </div>
                <div className="mt-1 truncate font-mono text-zinc-600">
                  handoff: {run.output_path}
                </div>
              </div>
            ))}

          {task.summary && (
            <details className="mt-3 rounded-lg border border-zinc-800 bg-zinc-950/50">
              <summary className="cursor-pointer px-3 py-2 text-[10px] text-zinc-500">
                {t("project_rooms.latest_handoff")}
              </summary>
              <pre className="border-t border-zinc-800 px-3 py-3 whitespace-pre-wrap text-[10px] leading-relaxed text-zinc-400">
                {task.summary}
              </pre>
            </details>
          )}
        </div>
      ))}
    </div>
  );
};
