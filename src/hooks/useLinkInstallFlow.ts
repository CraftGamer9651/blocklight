import { useState } from "react";
import { confirmLinkInstall, previewLinkInstall } from "@/lib/tauri";
import type { AppError, InstallSummary, ResolvedInstall } from "@/lib/types";
import { isAppError } from "@/lib/types";

export type LinkFlowStage =
  | { kind: "idle" }
  | { kind: "resolving"; url: string }
  | { kind: "preview"; url: string; resolved: ResolvedInstall }
  | { kind: "installing"; url: string; resolved: ResolvedInstall }
  | { kind: "installed"; summary: InstallSummary }
  | { kind: "error"; url: string; error: AppError };

/**
 * One state machine shared by every way a person can start an install --
 * pasting a link, dropping one, clipboard paste, or clicking a search
 * result -- so they all resolve into the exact same preview/confirm UI.
 */
export function useLinkInstallFlow(instanceId: string | null, onInstalled?: (s: InstallSummary) => void) {
  const [stage, setStage] = useState<LinkFlowStage>({ kind: "idle" });

  async function resolve(rawUrl: string) {
    const trimmed = rawUrl.trim();
    if (!trimmed || !instanceId) return;
    setStage({ kind: "resolving", url: trimmed });
    try {
      const resolved = await previewLinkInstall(trimmed, instanceId);
      setStage({ kind: "preview", url: trimmed, resolved });
    } catch (err) {
      setStage({ kind: "error", url: trimmed, error: toAppError(err) });
    }
  }

  async function install() {
    if (stage.kind !== "preview" || !instanceId) return;
    const { url, resolved } = stage;
    setStage({ kind: "installing", url, resolved });
    try {
      const summary = await confirmLinkInstall(url, instanceId);
      setStage({ kind: "installed", summary });
      onInstalled?.(summary);
    } catch (err) {
      setStage({ kind: "error", url, error: toAppError(err) });
    }
  }

  function reset() {
    setStage({ kind: "idle" });
  }

  return { stage, resolve, install, reset };
}

function toAppError(err: unknown): AppError {
  if (isAppError(err)) return err;
  return { kind: "internal", message: String(err) };
}
