import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export interface UpdateState {
  status: UpdateStatus;
  version?: string;
  body?: string;
  progress: number;
  error?: string;
}

const INITIAL_STATE: UpdateState = { status: "idle", progress: 0 };

export function useAppUpdater() {
  const [state, setState] = useState<UpdateState>(INITIAL_STATE);
  const pendingUpdate = useRef<Update | null>(null);

  const checkForUpdate = useCallback(async (silent: boolean) => {
    setState({ status: "checking", progress: 0 });
    try {
      const update = await check();
      if (!update) {
        pendingUpdate.current = null;
        if (silent) {
          setState(INITIAL_STATE);
          return;
        }
        setState({ status: "up-to-date", progress: 0 });
        return;
      }
      pendingUpdate.current = update;
      setState({
        status: "available",
        version: update.version,
        body: update.body,
        progress: 0,
      });
    } catch (e) {
      if (silent) {
        setState(INITIAL_STATE);
        return;
      }
      setState({ status: "error", progress: 0, error: String(e) });
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    const update = pendingUpdate.current;
    if (!update) return;

    let total = 0;
    let downloaded = 0;
    setState((prev) => ({ ...prev, status: "downloading", progress: 0 }));
    try {
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            downloaded = 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setState((prev) => ({
              ...prev,
              status: "downloading",
              progress: total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : prev.progress,
            }));
            break;
          case "Finished":
            setState((prev) => ({ ...prev, status: "ready", progress: 100 }));
            break;
        }
      });
    } catch (e) {
      setState({ status: "error", progress: 0, error: String(e) });
    }
  }, []);

  const restartNow = useCallback(async () => {
    await relaunch();
  }, []);

  const dismiss = useCallback(() => {
    pendingUpdate.current = null;
    setState(INITIAL_STATE);
  }, []);

  useEffect(() => {
    if (state.status !== "up-to-date") return;
    const timer = setTimeout(() => setState(INITIAL_STATE), 3000);
    return () => clearTimeout(timer);
  }, [state.status]);

  return { state, checkForUpdate, downloadAndInstall, restartNow, dismiss };
}
