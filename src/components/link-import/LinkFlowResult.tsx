import type { useLinkInstallFlow } from "@/hooks/useLinkInstallFlow";
import { Button, Spinner } from "../shared/Primitives";
import { ErrorState } from "../shared/ErrorState";
import { LinkPreviewCard } from "./LinkPreviewCard";

export function LinkFlowResult({ flow }: { flow: ReturnType<typeof useLinkInstallFlow> }) {
  const { stage } = flow;

  if (stage.kind === "idle") return null;

  if (stage.kind === "resolving") {
    return (
      <div className="panel">
        <Spinner label="Looking up this link…" />
      </div>
    );
  }

  if (stage.kind === "error") {
    return (
      <div className="panel">
        <ErrorState error={stage.error} onRetry={() => void flow.resolve(stage.url)} />
        <div style={{ marginTop: 10 }}>
          <Button size="sm" variant="ghost" onClick={flow.reset}>
            Dismiss
          </Button>
        </div>
      </div>
    );
  }

  if (stage.kind === "preview") {
    return (
      <LinkPreviewCard
        resolved={stage.resolved}
        busy={false}
        onInstall={() => void flow.install()}
        onCancel={flow.reset}
      />
    );
  }

  if (stage.kind === "installing") {
    return (
      <LinkPreviewCard resolved={stage.resolved} busy onInstall={() => {}} onCancel={() => {}} />
    );
  }

  // installed
  return (
    <div
      className="panel"
      style={{ borderColor: "rgba(143,209,79,0.35)", background: "var(--lime-wash)" }}
    >
      <div className="state-title" style={{ color: "var(--lime)" }}>
        Installed
      </div>
      <div className="state-body">
        {stage.summary.installed.map((entry) => entry.projectName).join(", ")} added to{" "}
        {stage.summary.instance.name}.
      </div>
      <div style={{ marginTop: 10 }}>
        <Button size="sm" onClick={flow.reset}>
          Install another
        </Button>
      </div>
    </div>
  );
}
