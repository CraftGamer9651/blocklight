import { useEffect, useRef, useState } from "react";
import {
  isInstanceRunning,
  launchInstance,
  onGameExited,
  onGameLog,
  onPrepareProgress,
  stopInstance,
} from "@/lib/tauri";
import type { AppError, GameExited, PrepareProgress } from "@/lib/types";
import { isAppError } from "@/lib/types";

export type LaunchState =
  | { kind: "idle" }
  | { kind: "preparing"; progress: PrepareProgress | null }
  | { kind: "running" }
  | { kind: "exited"; exitCode: number | null }
  | { kind: "error"; error: AppError };

const MAX_LOG_LINES = 400;

export function useLaunch(instanceId: string | null) {
  const [state, setState] = useState<LaunchState>({ kind: "idle" });
  const [logLines, setLogLines] = useState<string[]>([]);
  const currentInstanceId = useRef(instanceId);
  currentInstanceId.current = instanceId;

  useEffect(() => {
    if (!instanceId) return;
    let cancelled = false;

    isInstanceRunning(instanceId).then((running) => {
      if (!cancelled && running) setState({ kind: "running" });
    });

    const unlistenProgress = onPrepareProgress((progress) => {
      if (progress.instanceId !== currentInstanceId.current) return;
      setState((prev) => (prev.kind === "exited" || prev.kind === "idle" ? prev : { kind: "preparing", progress }));
    });

    const unlistenLog = onGameLog((line) => {
      if (line.instanceId !== currentInstanceId.current) return;
      setState((prev) => (prev.kind === "running" ? prev : { kind: "running" }));
      setLogLines((prev) => {
        const next = [...prev, line.line];
        return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
      });
    });

    const unlistenExit = onGameExited((exit: GameExited) => {
      if (exit.instanceId !== currentInstanceId.current) return;
      setState({ kind: "exited", exitCode: exit.exitCode });
    });

    return () => {
      cancelled = true;
      unlistenProgress.then((f) => f());
      unlistenLog.then((f) => f());
      unlistenExit.then((f) => f());
    };
  }, [instanceId]);

  async function launch() {
    if (!instanceId) return;
    setLogLines([]);
    setState({ kind: "preparing", progress: null });
    try {
      await launchInstance(instanceId);
      setState({ kind: "running" });
    } catch (err) {
      setState({ kind: "error", error: isAppError(err) ? err : { kind: "internal", message: String(err) } });
    }
  }

  async function stop() {
    if (!instanceId) return;
    await stopInstance(instanceId);
  }

  function reset() {
    setState({ kind: "idle" });
    setLogLines([]);
  }

  return { state, logLines, launch, stop, reset };
}
