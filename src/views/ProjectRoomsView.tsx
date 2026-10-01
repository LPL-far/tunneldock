import React, { useEffect, useMemo, useState } from "react";
import {
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
