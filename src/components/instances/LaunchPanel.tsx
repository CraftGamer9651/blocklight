import { useEffect, useRef, useState } from "react";
import type { LaunchState } from "@/hooks/useLaunch";
import { Badge, Button } from "../shared/Primitives";
import { ErrorState } from "../shared/ErrorState";

const STAGE_LABELS: Record<string, string> = {
  client: "Downloading Minecraft",
  libraries: "Downloading libraries",
  assets: "Downloading assets",
  natives: "Extracting natives",
  neoforge_libraries: "Downloading NeoForge libraries",
  neoforge_install: "Running NeoForge installer",
};

export function LaunchPanel({
  state,
  logLines,
  onStop,
  onReset,
}: {
  state: LaunchState;
  logLines: string[];
  onStop: () => void;
  onReset: () => void;
}) {
  const [showLog, setShowLog] = useState(false);

  if (state.kind === "idle") return null;

  if (state.kind === "error") {
    return (
      <div style={{ marginTop: 12 }}>
        <ErrorState error={state.error} />
      </div>
    );
  }

  if (state.kind === "preparing") {
    const progress = state.progress;
    const pct = progress && progress.total > 0 ? Math.round((progress.completed / progress.total) * 100) : 0;
    return (
      <div style={{ marginTop: 12 }}>
        <div className="meta-row" style={{ marginBottom: 6 }}>
          <span>{progress ? STAGE_LABELS[progress.stage] ?? progress.stage : "Preparing…"}</span>
          {progress && (
            <span className="tabular">
              {progress.completed}/{progress.total}
            </span>
          )}
        </div>
        <div style={{ height: 6, borderRadius: 4, background: "var(--surface-raised)", overflow: "hidden" }}>
          <div
            style={{
              height: "100%",
              width: `${pct}%`,
              background: "var(--ember)",
              transition: "width 200ms ease",
            }}
          />
        </div>
        {progress?.detail && (
          <p className="list-row-meta" style={{ marginTop: 6 }}>
            {progress.detail}
          </p>
        )}
      </div>
    );
  }

  if (state.kind === "running") {
    return (
      <div style={{ marginTop: 12 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Badge tone="success" dot>
            Running
          </Badge>
          <Button size="sm" variant="ghost" onClick={() => setShowLog((v) => !v)}>
            {showLog ? "Hide log" : "Show log"}
          </Button>
          <Button size="sm" variant="danger" onClick={onStop}>
            Stop
          </Button>
        </div>
        {showLog && <LogView lines={logLines} />}
      </div>
    );
  }

  // exited
  return (
    <div style={{ marginTop: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Badge tone={state.exitCode === 0 ? "neutral" : "danger"}>
          Exited{state.exitCode !== null ? ` (code ${state.exitCode})` : ""}
        </Badge>
        <Button size="sm" variant="ghost" onClick={() => setShowLog((v) => !v)}>
          {showLog ? "Hide log" : "Show log"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onReset}>
          Dismiss
        </Button>
      </div>
      {showLog && <LogView lines={logLines} />}
    </div>
  );
}

function LogView({ lines }: { lines: string[] }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines.length]);

  return (
    <div
      ref={ref}
      style={{
        marginTop: 10,
        height: 220,
        overflowY: "auto",
        background: "#0e0f12",
        border: "1px solid var(--line-soft)",
        borderRadius: 8,
        padding: 10,
        fontSize: 12,
        lineHeight: 1.5,
        color: "#c7cad1",
        whiteSpace: "pre-wrap",
        wordBreak: "break-all",
      }}
    >
      {lines.length === 0 ? (
        <span style={{ color: "var(--ink-faint)" }}>No output yet…</span>
      ) : (
        lines.map((line, i) => <div key={i}>{line}</div>)
      )}
    </div>
  );
}
