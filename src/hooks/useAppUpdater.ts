import { useCallback, useEffect, useRef, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { invoke } from "@tauri-apps/api/core";
import { APP_VERSION } from "../version";

export type UpdateStage =
  | "idle"
  | "checking"
  | "available"
  | "up_to_date"
  | "downloading"
  | "installing"
  | "installed"
  | "error";

export interface AppUpdateState {
  stage: UpdateStage;
  currentVersion: string;
  latestVersion: string | null;
  releaseDate: string | null;
  releaseNotes: string;
  downloadedBytes: number;
  totalBytes: number | null;
  progressPercent: number;
  errorMessage: string | null;
  lastCheckedAt: number | null;
}

const initialState: AppUpdateState = {
  stage: "idle",
  currentVersion: APP_VERSION,
  latestVersion: null,
  releaseDate: null,
  releaseNotes: "",
  downloadedBytes: 0,
  totalBytes: null,
  progressPercent: 0,
  errorMessage: null,
  lastCheckedAt: null,
};

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function useAppUpdater() {
  const [state, setState] = useState<AppUpdateState>(initialState);
  const [dialogOpen, setDialogOpen] = useState(false);
  const updateRef = useRef<Update | null>(null);
  const busyRef = useRef(false);

  const releaseUpdateResource = useCallback(async () => {
    const previous = updateRef.current;
    updateRef.current = null;
    if (previous) {
      try {
        await previous.close();
      } catch {
        // The native resource may already have been released during install/exit.
      }
    }
  }, []);

  const checkForUpdates = useCallback(
    async (manual = true) => {
      if (busyRef.current) {
        if (manual) setDialogOpen(true);
        return;
      }

      if (!isTauriRuntime()) {
        if (manual) {
          setState((prev) => ({
            ...prev,
            stage: "error",
            errorMessage: "更新检查仅在 TunnelDock 桌面应用中可用。",
          }));
          setDialogOpen(true);
        }
        return;
      }

      if (import.meta.env.DEV) {
        if (manual) {
          setState((prev) => ({
            ...prev,
            stage: "up_to_date",
            errorMessage: null,
            lastCheckedAt: Date.now(),
          }));
          setDialogOpen(true);
        }
        return;
      }

      busyRef.current = true;
      if (manual) setDialogOpen(true);
      setState((prev) => ({
        ...prev,
        stage: "checking",
        errorMessage: null,
      }));

      try {
        await releaseUpdateResource();
        const update = await check({ timeout: 15_000 });
        const checkedAt = Date.now();

        if (!update) {
          setState((prev) => ({
            ...prev,
            stage: manual ? "up_to_date" : "idle",
            latestVersion: null,
            releaseDate: null,
            releaseNotes: "",
            errorMessage: null,
            lastCheckedAt: checkedAt,
          }));
          return;
        }

        updateRef.current = update;
        setState((prev) => ({
          ...prev,
          stage: "available",
          currentVersion: update.currentVersion || APP_VERSION,
          latestVersion: update.version,
          releaseDate: update.date ?? null,
          releaseNotes: update.body?.trim() || "本次 Release 未提供更新日志。",
          downloadedBytes: 0,
          totalBytes: null,
          progressPercent: 0,
          errorMessage: null,
          lastCheckedAt: checkedAt,
        }));
        // A newly discovered release should be visible without requiring the user
        // to hunt through Settings. The dialog can still be dismissed and reopened
        // from the persistent header badge.
        setDialogOpen(true);
      } catch (error) {
        if (manual) {
          setState((prev) => ({
            ...prev,
            stage: "error",
            errorMessage: `检查 GitHub Release 失败：${errorText(error)}`,
            lastCheckedAt: Date.now(),
          }));
          setDialogOpen(true);
        } else {
          console.warn("Background update check failed:", error);
          setState((prev) => ({
            ...prev,
            stage: "idle",
            errorMessage: null,
            lastCheckedAt: Date.now(),
          }));
        }
      } finally {
        busyRef.current = false;
      }
    },
    [releaseUpdateResource]
  );

  const downloadAndInstall = useCallback(async () => {
    if (busyRef.current) return;
    const update = updateRef.current;
    if (!update) {
      await checkForUpdates(true);
      return;
    }

    busyRef.current = true;
    setDialogOpen(true);
    let downloaded = 0;
    let total: number | null = null;
    let guardianSuspended = false;

    setState((prev) => ({
      ...prev,
      stage: "downloading",
      downloadedBytes: 0,
      totalBytes: null,
      progressPercent: 0,
      errorMessage: null,
    }));

    try {
      await update.download((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
          setState((prev) => ({
            ...prev,
            stage: "downloading",
            totalBytes: total,
          }));
          return;
        }

        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          const progress = total && total > 0
            ? Math.min(100, Math.round((downloaded / total) * 100))
            : 0;
          setState((prev) => ({
            ...prev,
            stage: "downloading",
            downloadedBytes: downloaded,
            totalBytes: total,
            progressPercent: progress,
          }));
          return;
        }

        if (event.event === "Finished") {
          setState((prev) => ({
            ...prev,
            downloadedBytes: total ?? downloaded,
            totalBytes: total,
            progressPercent: 100,
          }));
        }
      }, { timeout: 120_000 });

      setState((prev) => ({
        ...prev,
        stage: "installing",
        progressPercent: 100,
      }));

      // Windows launches the installer and exits the application here. On macOS
      // and Linux the package is installed in-place and becomes active on the next
      // launch, so the UI transitions to `installed` if this call returns.
      // The installer may terminate the app without running the UI exit callback.
      await invoke("set_desktop_update_guard", { suspended: true });
      guardianSuspended = true;
      await update.install({ restartAfterInstall: true });

      setState((prev) => ({
        ...prev,
        stage: "installed",
        progressPercent: 100,
      }));
    } catch (error) {
      setState((prev) => ({
        ...prev,
        stage: "error",
        errorMessage: `更新失败：${errorText(error)}`,
      }));
    } finally {
      // Failed/cancelled installs must not silently leave recovery disabled.
      if (guardianSuspended) {
        try {
          await invoke("set_desktop_update_guard", { suspended: false });
        } catch (error) {
          setState((prev) => ({ ...prev, stage: "error", errorMessage: `${prev.errorMessage ?? ""} 恢复主程序守护失败：${errorText(error)}` }));
        }
      }
      busyRef.current = false;
    }
  }, [checkForUpdates]);

  const openDialog = useCallback(() => setDialogOpen(true), []);
  const closeDialog = useCallback(() => setDialogOpen(false), []);

  useEffect(() => {
    // Do not delay first paint. Check quietly after the local application and
    // tunnel status have had time to initialize.
    const timer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, 1800);

    return () => window.clearTimeout(timer);
  }, [checkForUpdates]);

  return {
    state,
    dialogOpen,
    openDialog,
    closeDialog,
    checkForUpdates,
    downloadAndInstall,
  };
}
