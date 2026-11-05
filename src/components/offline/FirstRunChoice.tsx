import { useState } from "react";
import type { OfflineProfile } from "@/lib/types";
import { Button } from "../shared/Primitives";
import { OfflineProfileSetup } from "./OfflineProfileSetup";
import { MicrosoftSignInModal } from "./MicrosoftSignInModal";

export function FirstRunChoice({ onComplete }: { onComplete: () => void }) {
  const [mode, setMode] = useState<"choose" | "offline" | "microsoft">("choose");

  if (mode === "offline") {
    return (
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 16 }}>
        <OfflineProfileSetup onCreated={() => onComplete()} />
        <Button variant="ghost" size="sm" onClick={() => setMode("choose")}>
          Back
        </Button>
      </div>
    );
  }

  return (
    <div style={{ maxWidth: 520, margin: "0 auto", width: "100%" }}>
      <div style={{ textAlign: "center", marginBottom: 28 }}>
        <h1 className="page-title">How would you like to use Blocklight?</h1>
        <p className="page-subtitle" style={{ marginTop: 6 }}>
          You can switch between these any time from the sidebar.
        </p>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        <button
          className="panel-notched notched tile"
          style={{ textAlign: "left", cursor: "pointer" }}
          onClick={() => setMode("microsoft")}
        >
          <div style={{ color: "var(--signal-blue)", fontWeight: 700, fontSize: 15 }}>
            Microsoft Account
          </div>
          <p className="state-body">
            Sign in for online Minecraft features and account functionality.
          </p>
        </button>

        <button
          className="panel-notched notched tile"
          style={{ textAlign: "left", cursor: "pointer" }}
          onClick={() => setMode("offline")}
        >
          <div style={{ color: "var(--lime)", fontWeight: 700, fontSize: 15 }}>
            Offline / Local Mode
          </div>
          <p className="state-body">
            Use Blocklight with locally available Minecraft installations and features that
            support offline operation.
          </p>
        </button>
      </div>

      <p className="state-body" style={{ textAlign: "center", marginTop: 20 }}>
        Offline / Local Mode doesn't bypass Minecraft's own ownership or authentication
        requirements — it's for instances you can already legitimately play offline.
      </p>

      {mode === "microsoft" && (
        <MicrosoftSignInModal onClose={() => setMode("choose")} onSignedIn={onComplete} />
      )}
    </div>
  );
}

// Re-exported for callers that only need the created-profile shape.
export type { OfflineProfile };
