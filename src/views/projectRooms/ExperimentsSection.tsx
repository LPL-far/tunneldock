import React from "react";
import { ProjectExperiment } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  experiments: ProjectExperiment[];
  draft: ProjectExperiment;
  artifactDraft: string;
  busy: boolean;
  onDraftChange: (experiment: ProjectExperiment) => void;
  onArtifactDraftChange: (value: string) => void;
  onCreate: () => void;
}

export const ExperimentsSection: React.FC<Props> = ({
  experiments,
  draft,
  artifactDraft,
  busy,
  onDraftChange,
  onArtifactDraftChange,
  onCreate,
}) => {
  const { t } = useTranslation();

  return (
    <div className="space-y-3">
      <div className="rounded-xl border border-zinc-800 bg-dark-card p-5 space-y-3">
        <h4 className="text-sm font-semibold text-zinc-200">
          {t("project_rooms.new_experiment")}
        </h4>
        <textarea
          value={draft.hypothesis}
          onChange={(event) =>
            onDraftChange({
              ...draft,
              hypothesis: event.target.value,
            })
          }
          rows={3}
          placeholder={t("project_rooms.hypothesis")}
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
        />
        <div className="grid grid-cols-2 gap-3">
          <input
            value={draft.dataset}
            onChange={(event) =>
              onDraftChange({
                ...draft,
                dataset: event.target.value,
              })
            }
            placeholder="dataset"
            className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          />
          <input
            value={artifactDraft}
            onChange={(event) => onArtifactDraftChange(event.target.value)}
            placeholder="artifacts: outputs/a.png, logs/run.json"
            className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200"
          />
        </div>
        <input
          value={draft.command}
          onChange={(event) =>
            onDraftChange({
              ...draft,
              command: event.target.value,
            })
          }
          placeholder="command"
          className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs font-mono text-zinc-200"
        />
        <button
          type="button"
          disabled={busy || !draft.hypothesis.trim()}
          onClick={onCreate}
          className="rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-xs text-zinc-200 disabled:opacity-40"
        >
          {t("project_rooms.record_experiment")}
        </button>
      </div>

      {experiments.map((experiment) => (
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
  );
};
