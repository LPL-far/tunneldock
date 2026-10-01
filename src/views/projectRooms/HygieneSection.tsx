import React from "react";
import { Search } from "lucide-react";
import { HygieneCandidate } from "../../types";
import { useTranslation } from "../../i18n";

interface Props {
  candidates: HygieneCandidate[];
  scanned: boolean;
  busy: boolean;
  onScan: () => void;
}

export const HygieneSection: React.FC<Props> = ({
  candidates,
  scanned,
  busy,
  onScan,
}) => {
  const { t } = useTranslation();

  return (
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
          onClick={onScan}
          className="flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-2 text-xs text-zinc-300 disabled:opacity-40"
        >
          <Search className="w-3.5 h-3.5" />
          {t("project_rooms.scan")}
        </button>
      </div>

      {scanned && candidates.length === 0 && (
        <div className="rounded-lg border border-emerald-900/50 bg-emerald-950/20 px-3 py-3 text-xs text-emerald-400">
          {t("project_rooms.no_hygiene_candidates")}
        </div>
      )}

      {candidates.map((item) => (
        <div
          key={item.path}
          className="rounded-lg border border-zinc-800 bg-zinc-950/60 px-3 py-3"
        >
          <div className="text-[11px] font-mono text-zinc-300">{item.path}</div>
          <div className="mt-1 text-[10px] text-zinc-600">{item.reason}</div>
        </div>
      ))}
    </div>
  );
};
