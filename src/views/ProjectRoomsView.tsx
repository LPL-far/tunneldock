import React, { useEffect, useMemo, useState } from "react";
import {
  Brain,
  CheckCircle2,
  Copy,
  FlaskConical,
  GitBranch,
  LoaderCircle,
  MessageSquare,
  Network,
  RefreshCw,
  Save,
  Search,
  Server,
  ShieldCheck,
  Sparkles,
  Wrench,
} from "lucide-react";
import {
  AgentCapacity,
  AgentRuntimeInfo,
  HygieneCandidate,
  ProjectDiscussionMessage,
  ProjectExperiment,
  ProjectMemory,
  ProjectRoomConfig,
  ProjectRoomSnapshot,
  ProjectRoomSummary,
  ProjectTask,
} from "../types";
import {
  appendProjectMessage,
  dispatchProjectTask,
  generateProjectRoomPrompt,
  getProjectRoom,
  initializeProjectGit,
  listAgentRuntimes,
  listProjectRooms,
  refreshAgentCapacities,
  refreshProjectRuns,
  scanProjectHygiene,
  updateProjectConfig,
  updateProjectMemory,
  upsertProjectExperiment,
  upsertProjectTask,
} from "../api";
import { useTranslation } from "../i18n";

type Section =
  | "memory"
  | "tasks"
  | "discussion"
  | "experiments"
  | "config"
  | "hygiene";

const emptyTask = (): ProjectTask => ({
  id: "",
  title: "",
  goal: "",
  owner: "codex",
  reviewers: ["chatgpt"],
  status: "backlog",
  write_scope: [],
  summary: "",
  created_at: "",
  updated_at: "",
});

const emptyExperiment = (): ProjectExperiment => ({
  id: "",
  hypothesis: "",
  code_revision: "",
  command: "",
  config: "",
  dataset: "",
  metrics: "",
  result: "",
  analysis: "",
  artifacts: [],
  status: "planned",
  created_at: "",
  updated_at: "",
});

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

export const ProjectRoomsView: React.FC = () => {
  const { t, locale } = useTranslation();
  const [summaries, setSummaries] = useState<ProjectRoomSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [room, setRoom] = useState<ProjectRoomSnapshot | null>(null);
  const [section, setSection] = useState<Section>("memory");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [memoryDraft, setMemoryDraft] = useState<ProjectMemory | null>(null);
  const [configDraft, setConfigDraft] = useState<ProjectRoomConfig | null>(null);
  const [taskDraft, setTaskDraft] = useState<ProjectTask>(emptyTask);
  const [taskScope, setTaskScope] = useState("");
  const [messageDraft, setMessageDraft] = useState("");
  const [messageAuthor, setMessageAuthor] = useState("user");
  const [experimentDraft, setExperimentDraft] =
    useState<ProjectExperiment>(emptyExperiment);
  const [artifactDraft, setArtifactDraft] = useState("");
  const [hygiene, setHygiene] = useState<HygieneCandidate[]>([]);
  const [hygieneScanned, setHygieneScanned] = useState(false);
  const [projectPrompt, setProjectPrompt] = useState<string | null>(null);
  const [promptCopied, setPromptCopied] = useState(false);
  const [runtimes, setRuntimes] = useState<AgentRuntimeInfo[]>([]);
  const [dispatchingTaskId, setDispatchingTaskId] = useState<string | null>(null);

  const loadSummaries = async (preferId?: string | null) => {
    const list = await listProjectRooms();
    setSummaries(list);
    const next =
      preferId && list.some((item) => item.id === preferId)
        ? preferId
        : selectedId && list.some((item) => item.id === selectedId)
        ? selectedId
        : list[0]?.id ?? null;
    setSelectedId(next);
    return next;
  };

  const loadRoom = async (projectId: string) => {
    setLoading(true);
    setError(null);
    try {
      const snapshot = await getProjectRoom(projectId);
      setRoom(snapshot);
      setMemoryDraft(snapshot.memory);
      setConfigDraft(snapshot.config);
      setHygiene([]);
      setHygieneScanned(false);
      setProjectPrompt(null);
      setPromptCopied(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void (async () => {
      try {
        const [detectedRuntimes] = await Promise.all([
          listAgentRuntimes(),
          refreshAgentCapacities(),
        ]);
        setRuntimes(detectedRuntimes);
        const first = await loadSummaries(null);
        if (first) {
          await loadRoom(first);
        }
      } catch (err) {
        setError(String(err));
      }
    })();
    // Initial project registry bootstrap only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (selectedId) {
      void loadRoom(selectedId);
    }
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId || !room?.runs.some((run) =>
      ["running", "interactive"].includes(run.status)
    )) {
      return;
    }

    const timer = window.setInterval(() => {
      void refreshProjectRuns(selectedId)
        .then((snapshot) => {
          setRoom(snapshot);
          setMemoryDraft(snapshot.memory);
          setConfigDraft(snapshot.config);
          void loadSummaries(selectedId);
        })
        .catch((err) => setError(String(err)));
    }, 4000);

    return () => window.clearInterval(timer);
  }, [selectedId, room?.runs]);

  const selectedSummary = useMemo(
    () => summaries.find((item) => item.id === selectedId) ?? null,
    [summaries, selectedId]
  );

  const capacities = useMemo(() => {
    const map = new Map<string, AgentCapacity>();
    room?.capacities.forEach((capacity) => map.set(capacity.agent_id, capacity));
    return map;
  }, [room]);

  const runtimeMap = useMemo(() => {
    const map = new Map<string, AgentRuntimeInfo>();
    runtimes.forEach((runtime) => map.set(runtime.agent_id, runtime));
    return map;
  }, [runtimes]);

  const refreshCurrent = async () => {
    try {
      const id = await loadSummaries(selectedId);
      if (id) await loadRoom(id);
    } catch (err) {
      setError(String(err));
    }
  };

  const refreshTelemetry = async () => {
    setBusy(true);
    setError(null);
    try {
      const [detectedRuntimes] = await Promise.all([
        listAgentRuntimes(),
        refreshAgentCapacities(),
      ]);
      setRuntimes(detectedRuntimes);
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const dispatchTask = async (task: ProjectTask) => {
    if (!room || !["codex", "gemini"].includes(task.owner)) return;
    setDispatchingTaskId(task.id);
    setError(null);
    try {
      await dispatchProjectTask(room.config.id, task.id, task.owner);
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setDispatchingTaskId(null);
    }
  };

  const saveMemory = async () => {
    if (!room || !memoryDraft) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await updateProjectMemory(room.config.id, memoryDraft);
      setMemoryDraft(saved);
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const saveConfig = async () => {
    if (!configDraft) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await updateProjectConfig(configDraft);
      setRoom(saved);
      setConfigDraft(saved.config);
      await loadSummaries(saved.config.id);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const createTask = async () => {
    if (!room || !taskDraft.title.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const reviewers =
        taskDraft.owner === "gemini"
          ? ["codex", "chatgpt"]
          : taskDraft.owner === "codex"
          ? ["chatgpt"]
          : ["chatgpt"];
      await upsertProjectTask(room.config.id, {
        ...taskDraft,
        reviewers,
        write_scope: taskScope
          .split(",")
          .map((item) => item.trim())
          .filter(Boolean),
      });
      setTaskDraft(emptyTask());
      setTaskScope("");
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const appendMessage = async () => {
    if (!room || !messageDraft.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const message: ProjectDiscussionMessage = {
        id: "",
        thread_id: "general",
        author: messageAuthor,
        recipients: ["all"],
        message: messageDraft.trim(),
        created_at: "",
      };
      await appendProjectMessage(room.config.id, message);
      setMessageDraft("");
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const createExperiment = async () => {
    if (!room || !experimentDraft.hypothesis.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await upsertProjectExperiment(room.config.id, {
        ...experimentDraft,
        artifacts: artifactDraft
          .split(",")
          .map((item) => item.trim())
          .filter(Boolean),
      });
      setExperimentDraft(emptyExperiment());
      setArtifactDraft("");
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const openProjectPrompt = async () => {
    if (!room) return;
    setBusy(true);
    setError(null);
    try {
      const prompt = await generateProjectRoomPrompt(room.config.id, locale);
      setProjectPrompt(prompt);
      setPromptCopied(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const copyProjectPrompt = async () => {
    if (!projectPrompt) return;
    await navigator.clipboard.writeText(projectPrompt);
    setPromptCopied(true);
    setTimeout(() => setPromptCopied(false), 1800);
  };

  const initGit = async () => {
    if (!room) return;
    setBusy(true);
    setError(null);
    try {
      await initializeProjectGit(room.config.id);
      await refreshCurrent();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const scanHygiene = async () => {
    if (!room) return;
    setBusy(true);
    setError(null);
    try {
      const result = await scanProjectHygiene(room.config.id);
      setHygiene(result);
      setHygieneScanned(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const sections: Array<{ id: Section; label: string; icon: React.ElementType }> = [
    { id: "memory", label: t("project_rooms.memory"), icon: Brain },
    { id: "tasks", label: t("project_rooms.tasks"), icon: CheckCircle2 },
    { id: "discussion", label: t("project_rooms.discussion"), icon: MessageSquare },
    { id: "experiments", label: t("project_rooms.experiments"), icon: FlaskConical },
    { id: "config", label: t("project_rooms.config"), icon: Server },
    { id: "hygiene", label: t("project_rooms.hygiene"), icon: ShieldCheck },
  ];

  return (
    <div className="p-5 space-y-4 max-w-[1500px] mx-auto">
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <Network className="w-5 h-5 text-emerald-400" />
            <h2 className="text-lg font-semibold text-zinc-100">
              {t("project_rooms.title")}
            </h2>
          </div>
          <p className="mt-1 text-xs text-zinc-500 max-w-3xl">
            {t("project_rooms.desc")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void refreshTelemetry()}
            className="flex items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 text-[11px] text-zinc-400 hover:text-zinc-100 hover:border-zinc-700 disabled:opacity-40"
            title={t("project_rooms.refresh_capacity")}
          >
            <Sparkles className="w-3.5 h-3.5" />
            {t("project_rooms.refresh_capacity")}
          </button>
          <button
            type="button"
            onClick={() => void refreshCurrent()}
            className="p-2 rounded-lg border border-zinc-800 bg-zinc-900 text-zinc-400 hover:text-zinc-100 hover:border-zinc-700"
            title={t("common.refresh")}
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {error && (
        <div className="rounded-lg border border-rose-800/60 bg-rose-950/40 px-4 py-3 text-xs text-rose-300">
          {error}
        </div>
      )}

      <div className="grid grid-cols-[280px_minmax(0,1fr)] gap-4 items-start">
        <div className="space-y-2">
          {summaries.map((item) => (
            <button
              type="button"
              key={item.id}
              onClick={() => setSelectedId(item.id)}
              className={`w-full text-left rounded-xl border p-4 transition-colors ${
                selectedId === item.id
                  ? "border-emerald-700/70 bg-emerald-950/15"
                  : "border-zinc-800 bg-dark-card hover:border-zinc-700"
              }`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm font-semibold text-zinc-100">
                  {item.name}
                </span>
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

        <div className="min-w-0">
          {loading || !room || !memoryDraft || !configDraft ? (
            <div className="min-h-[420px] rounded-xl border border-zinc-800 bg-dark-card flex items-center justify-center text-zinc-500">
              <LoaderCircle className="w-5 h-5 animate-spin" />
            </div>
          ) : (
            <div className="space-y-4">
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
                        Remote:{" "}
                        {room.config.remote.root || t("project_rooms.not_configured")}
                      </div>
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-2 text-[10px]">
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void openProjectPrompt()}
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
                        onClick={() => void copyProjectPrompt()}
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
                              runtime?.installed
                                ? "text-emerald-500/80"
                                : "text-zinc-600"
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

              <div className="flex flex-wrap gap-2">
                {sections.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      type="button"
                      key={item.id}
                      onClick={() => setSection(item.id)}
                      className={`flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs transition-colors ${
                        section === item.id
                          ? "border-zinc-600 bg-zinc-800 text-zinc-100"
                          : "border-zinc-800 bg-zinc-900/70 text-zinc-500 hover:text-zinc-300"
                      }`}
                    >
                      <Icon className="w-3.5 h-3.5" />
                      {item.label}
                    </button>
                  );
                })}
              </div>

              {section === "memory" && (
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
                      onClick={() => void saveMemory()}
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
                      <span className="text-[11px] font-medium text-zinc-400">
                        {label}
                      </span>
                      <textarea
                        value={String(memoryDraft[key] ?? "")}
                        onChange={(event) =>
                          setMemoryDraft({
                            ...memoryDraft,
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
                      {memoryDraft.memory_protocol}
                    </pre>
                  </details>
                </div>
              )}

              {section === "tasks" && (
                <div className="space-y-3">
                  <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-3">
                    <h4 className="text-sm font-semibold text-zinc-200">
                      {t("project_rooms.new_task")}
                    </h4>
                    <input
                      value={taskDraft.title}
                      onChange={(event) =>
                        setTaskDraft({ ...taskDraft, title: event.target.value })
                      }
                      placeholder={t("project_rooms.task_title")}
                      className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200 outline-none"
                    />
                    <textarea
                      value={taskDraft.goal}
                      onChange={(event) =>
                        setTaskDraft({ ...taskDraft, goal: event.target.value })
                      }
                      placeholder={t("project_rooms.task_goal")}
                      rows={3}
                      className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200 outline-none"
                    />
                    <div className="grid grid-cols-2 gap-3">
                      <select
                        value={taskDraft.owner}
                        onChange={(event) =>
                          setTaskDraft({ ...taskDraft, owner: event.target.value })
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
                        onChange={(event) => setTaskScope(event.target.value)}
                        placeholder="write scope: model/**, visualization/**"
                        className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                      />
                    </div>
                    <button
                      type="button"
                      disabled={busy || !taskDraft.title.trim()}
                      onClick={() => void createTask()}
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
                            onClick={() => void dispatchTask(task)}
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
                              <span className="font-mono text-zinc-500">
                                {run.id}
                              </span>
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
              )}

              {section === "discussion" && (
                <div className="space-y-3">
                  <div className="rounded-xl border border-zinc-800 bg-dark-card p-4">
                    <div className="flex gap-2">
                      <select
                        value={messageAuthor}
                        onChange={(event) => setMessageAuthor(event.target.value)}
                        className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                      >
                        <option value="user">User</option>
                        <option value="chatgpt">ChatGPT</option>
                        <option value="codex">Codex</option>
                        <option value="gemini">Gemini</option>
                      </select>
                      <input
                        value={messageDraft}
                        onChange={(event) => setMessageDraft(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" && !event.shiftKey) {
                            event.preventDefault();
                            void appendMessage();
                          }
                        }}
                        placeholder={t("project_rooms.discussion_placeholder")}
                        className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                      />
                      <button
                        type="button"
                        disabled={busy || !messageDraft.trim()}
                        onClick={() => void appendMessage()}
                        className="rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-xs text-zinc-200 disabled:opacity-40"
                      >
                        {t("project_rooms.send")}
                      </button>
                    </div>
                  </div>
                  {room.discussion.map((message) => (
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
              )}

              {section === "experiments" && (
                <div className="space-y-3">
                  <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-3">
                    <h4 className="text-sm font-semibold text-zinc-200">
                      {t("project_rooms.new_experiment")}
                    </h4>
                    <textarea
                      value={experimentDraft.hypothesis}
                      onChange={(event) =>
                        setExperimentDraft({
                          ...experimentDraft,
                          hypothesis: event.target.value,
                        })
                      }
                      rows={3}
                      placeholder={t("project_rooms.hypothesis")}
                      className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                    />
                    <div className="grid grid-cols-2 gap-3">
                      <input
                        value={experimentDraft.dataset}
                        onChange={(event) =>
                          setExperimentDraft({
                            ...experimentDraft,
                            dataset: event.target.value,
                          })
                        }
                        placeholder="dataset"
                        className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                      />
                      <input
                        value={artifactDraft}
                        onChange={(event) => setArtifactDraft(event.target.value)}
                        placeholder="artifacts: outputs/a.png, logs/run.json"
                        className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
                      />
                    </div>
                    <input
                      value={experimentDraft.command}
                      onChange={(event) =>
                        setExperimentDraft({
                          ...experimentDraft,
                          command: event.target.value,
                        })
                      }
                      placeholder="command"
                      className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
                    />
                    <button
                      type="button"
                      disabled={busy || !experimentDraft.hypothesis.trim()}
                      onClick={() => void createExperiment()}
                      className="rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-xs text-zinc-200 disabled:opacity-40"
                    >
                      {t("project_rooms.record_experiment")}
                    </button>
                  </div>
                  {room.experiments.map((experiment) => (
                    <div
                      key={experiment.id}
                      className="rounded-xl border border-zinc-800 bg-dark-card p-4"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-xs font-semibold text-zinc-200">
                          {experiment.id}
                        </span>
                        <span className="text-[10px] text-zinc-500">
                          {experiment.status}
                        </span>
                      </div>
                      <p className="mt-2 text-[11px] leading-relaxed text-zinc-400">
                        {experiment.hypothesis}
                      </p>
                      {experiment.result && (
                        <div className="mt-2 text-[11px] text-emerald-400/80">
                          {experiment.result}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}

              {section === "config" && (
                <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-4">
                  <div className="flex items-center justify-between">
                    <h4 className="text-sm font-semibold text-zinc-200">
                      {t("project_rooms.project_config")}
                    </h4>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void saveConfig()}
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
                        value={String(configDraft[key] ?? "")}
                        onChange={(event) =>
                          setConfigDraft({
                            ...configDraft,
                            [key]: event.target.value,
                          })
                        }
                        className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
                      />
                    </label>
                  ))}
                  <div className="grid grid-cols-2 gap-3">
                    <input
                      value={configDraft.remote.host}
                      onChange={(event) =>
                        setConfigDraft({
                          ...configDraft,
                          remote: {
                            ...configDraft.remote,
                            host: event.target.value,
                          },
                        })
                      }
                      placeholder="remote host"
                      className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
                    />
                    <input
                      value={configDraft.remote.root}
                      onChange={(event) =>
                        setConfigDraft({
                          ...configDraft,
                          remote: {
                            ...configDraft.remote,
                            root: event.target.value,
                          },
                        })
                      }
                      placeholder="remote root"
                      className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
                    />
                    <input
                      value={configDraft.remote.environment}
                      onChange={(event) =>
                        setConfigDraft({
                          ...configDraft,
                          remote: {
                            ...configDraft.remote,
                            environment: event.target.value,
                          },
                        })
                      }
                      placeholder="conda / runtime environment"
                      className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
                    />
                    <input
                      value={configDraft.remote.notes}
                      onChange={(event) =>
                        setConfigDraft({
                          ...configDraft,
                          remote: {
                            ...configDraft.remote,
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
                      checked={configDraft.keep_session_alive}
                      onChange={(event) =>
                        setConfigDraft({
                          ...configDraft,
                          keep_session_alive: event.target.checked,
                        })
                      }
                      className="h-4 w-4 accent-emerald-500"
                    />
                  </label>
                  <div className="border-t border-zinc-800 pt-4">
                    <button
                      type="button"
                      onClick={() => void initGit()}
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
              )}

              {section === "hygiene" && (
                <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-4">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h4 className="text-sm font-semibold text-zinc-200">
                        {t("project_rooms.hygiene_title")}
                      </h4>
                      <p className="mt-1 text-[11px] text-zinc-500">
                        {t("project_rooms.hygiene_desc")}
                      </p>
                    </div>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void scanHygiene()}
                      className="flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-2 text-xs text-zinc-300 disabled:opacity-40"
                    >
                      <Search className="w-3.5 h-3.5" />
                      {t("project_rooms.scan")}
                    </button>
                  </div>
                  {hygieneScanned && hygiene.length === 0 && (
                    <div className="rounded-lg border border-emerald-900/50 bg-emerald-950/20 px-3 py-3 text-xs text-emerald-400">
                      {t("project_rooms.no_hygiene_candidates")}
                    </div>
                  )}
                  {hygiene.map((item) => (
                    <div
                      key={item.path}
                      className="rounded-lg border border-zinc-800 bg-zinc-950/60 px-3 py-3"
                    >
                      <div className="text-[11px] font-mono text-zinc-300">
                        {item.path}
                      </div>
                      <div className="mt-1 text-[10px] text-zinc-600">
                        {item.reason}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
