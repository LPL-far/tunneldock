import React, { useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  Brain,
  CheckCircle2,
  FlaskConical,
  LoaderCircle,
  MessageSquare,
  Network,
  RefreshCw,
  Server,
  ShieldCheck,
  Sparkles,
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
import { ConfigSection } from "./projectRooms/ConfigSection";
import { DiscussionSection } from "./projectRooms/DiscussionSection";
import { ExperimentsSection } from "./projectRooms/ExperimentsSection";
import { HygieneSection } from "./projectRooms/HygieneSection";
import { MemorySection } from "./projectRooms/MemorySection";
import { ProjectRoomHeader } from "./projectRooms/ProjectRoomHeader";
import { ProjectRoomList } from "./projectRooms/ProjectRoomList";
import { TasksSection } from "./projectRooms/TasksSection";

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
  kind: "work",
  thread_id: "",
  auto_dispatch: false,
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

  const sections: Array<{
    id: Section;
    label: string;
    icon: React.ElementType;
    badge?: number;
  }> = [
    { id: "memory", label: t("project_rooms.memory"), icon: Brain },
    {
      id: "tasks",
      label: t("project_rooms.tasks"),
      icon: CheckCircle2,
      badge: room?.tasks.length,
    },
    {
      id: "discussion",
      label: t("project_rooms.discussion"),
      icon: MessageSquare,
      badge: room?.discussion.length,
    },
    {
      id: "experiments",
      label: t("project_rooms.experiments"),
      icon: FlaskConical,
      badge: room?.experiments.length,
    },
    { id: "config", label: t("project_rooms.config"), icon: Server },
    { id: "hygiene", label: t("project_rooms.hygiene"), icon: ShieldCheck },
  ];

  return (
    <div className="w-full space-y-4 p-4 xl:space-y-5 xl:p-5 2xl:p-6">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-zinc-800/80 bg-gradient-to-r from-zinc-900/60 via-dark-card to-zinc-950/40 px-4 py-3.5">
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <div className="rounded-lg border border-emerald-900/50 bg-emerald-950/20 p-2">
              <Network className="h-4 w-4 text-emerald-400" />
            </div>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h2 className="text-[15px] font-semibold tracking-tight text-zinc-100">
                  {t("project_rooms.title")}
                </h2>
                <span className="rounded-md border border-zinc-800 bg-zinc-950/50 px-1.5 py-0.5 text-[9px] font-mono text-zinc-600">
                  {t("project_rooms.rooms_count", { count: summaries.length })}
                </span>
              </div>
              <p className="mt-0.5 max-w-3xl truncate text-[10px] text-zinc-600">
                {t("project_rooms.desc")}
              </p>
            </div>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void refreshTelemetry()}
            className="flex h-8 items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-950/50 px-3 text-[10px] font-medium text-zinc-400 transition hover:border-zinc-700 hover:text-zinc-100 disabled:opacity-40"
            title={t("project_rooms.refresh_capacity")}
          >
            <Sparkles className="h-3.5 w-3.5" />
            {t("project_rooms.refresh_capacity")}
          </button>
          <button
            type="button"
            onClick={() => void refreshCurrent()}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-zinc-800 bg-zinc-950/50 text-zinc-500 transition hover:border-zinc-700 hover:text-zinc-100"
            title={t("common.refresh")}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-start gap-2.5 rounded-xl border border-rose-900/60 bg-rose-950/20 px-3.5 py-3 text-xs text-rose-300">
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-[clamp(230px,19vw,320px)_minmax(0,1fr)] xl:gap-5">
        <ProjectRoomList
          summaries={summaries}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />

        <div className="min-w-0">
          {loading || !room || !memoryDraft || !configDraft ? (
            <div className="min-h-[420px] rounded-xl border border-zinc-800 bg-dark-card flex items-center justify-center text-zinc-500">
              <LoaderCircle className="w-5 h-5 animate-spin" />
            </div>
          ) : (
            <div className="space-y-4">
              <ProjectRoomHeader
                room={room}
                selectedSummary={selectedSummary}
                busy={busy}
                capacities={capacities}
                runtimeMap={runtimeMap}
                projectPrompt={projectPrompt}
                promptCopied={promptCopied}
                onOpenProjectPrompt={() => void openProjectPrompt()}
                onCopyProjectPrompt={() => void copyProjectPrompt()}
              />

              <div className="flex flex-wrap gap-1 rounded-xl border border-zinc-800/80 bg-zinc-950/35 p-1">
                {sections.map((item) => {
                  const Icon = item.icon;
                  const active = section === item.id;
                  return (
                    <button
                      type="button"
                      key={item.id}
                      onClick={() => setSection(item.id)}
                      className={`flex h-8 items-center gap-1.5 rounded-lg px-3 text-[11px] font-medium transition-all ${
                        active
                          ? "bg-zinc-800 text-zinc-100 shadow-sm"
                          : "text-zinc-600 hover:bg-zinc-900/70 hover:text-zinc-300"
                      }`}
                    >
                      <Icon className="h-3.5 w-3.5" />
                      {item.label}
                      {item.badge != null && item.badge > 0 && (
                        <span
                          className={`ml-0.5 rounded-md px-1.5 py-0.5 text-[9px] tabular-nums ${
                            active
                              ? "bg-zinc-700 text-zinc-300"
                              : "bg-zinc-900 text-zinc-600"
                          }`}
                        >
                          {item.badge}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>

              {section === "memory" && (
                <MemorySection
                  draft={memoryDraft}
                  busy={busy}
                  onChange={setMemoryDraft}
                  onSave={() => void saveMemory()}
                />
              )}

              {section === "tasks" && (
                <TasksSection
                  room={room}
                  taskDraft={taskDraft}
                  taskScope={taskScope}
                  busy={busy}
                  dispatchingTaskId={dispatchingTaskId}
                  runtimeMap={runtimeMap}
                  onTaskDraftChange={setTaskDraft}
                  onTaskScopeChange={setTaskScope}
                  onCreate={() => void createTask()}
                  onDispatch={(task) => void dispatchTask(task)}
                />
              )}

              {section === "discussion" && (
                <DiscussionSection
                  messages={room.discussion}
                  author={messageAuthor}
                  draft={messageDraft}
                  busy={busy}
                  onAuthorChange={setMessageAuthor}
                  onDraftChange={setMessageDraft}
                  onSend={() => void appendMessage()}
                />
              )}

              {section === "experiments" && (
                <ExperimentsSection
                  experiments={room.experiments}
                  draft={experimentDraft}
                  artifactDraft={artifactDraft}
                  busy={busy}
                  onDraftChange={setExperimentDraft}
                  onArtifactDraftChange={setArtifactDraft}
                  onCreate={() => void createExperiment()}
                />
              )}

              {section === "config" && (
                <ConfigSection
                  draft={configDraft}
                  selectedSummary={selectedSummary}
                  busy={busy}
                  onChange={setConfigDraft}
                  onSave={() => void saveConfig()}
                  onInitGit={() => void initGit()}
                />
              )}

              {section === "hygiene" && (
                <HygieneSection
                  candidates={hygiene}
                  scanned={hygieneScanned}
                  busy={busy}
                  onScan={() => void scanHygiene()}
                />
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
