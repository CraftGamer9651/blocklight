import { useState } from "react";
import type { AccountState, ConnectivityState } from "@/lib/types";
import { switchToMicrosoft } from "@/lib/tauri";
import { Button } from "../shared/Primitives";
import { MicrosoftSignInModal } from "./MicrosoftSignInModal";

export function OfflineStatusCard({
  connectivity,
  account,
  onAccountChange,
  onOpenSettings,
}: {
  connectivity: ConnectivityState;
  account: AccountState | null;
  onAccountChange: () => void;
  onOpenSettings: () => void;
}) {
  const [signingIn, setSigningIn] = useState(false);

  const usingOffline = !account || account.activeKind === "offline";
  const isOnline = connectivity === "online";

  async function handleSwitchToMicrosoft() {
    if (account?.microsoftSignedIn) {
      await switchToMicrosoft();
      onAccountChange();
      return;
    }
    setSigningIn(true);
  }

  async function handleSignedIn() {
    setSigningIn(false);
    onAccountChange();
  }

  if (usingOffline || !isOnline) {
    return (
      <>
        <div
          className="panel"
          style={{
            padding: "10px 12px",
            borderColor: "rgba(143,209,79,0.25)",
            background: "var(--lime-wash)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--lime)" }}>
            <span className="status-dot" />
            <span style={{ fontSize: 12.5, fontWeight: 600 }}>
              {isOnline ? "Offline profile" : "Offline mode"}
            </span>
          </div>
          <p style={{ fontSize: 12, color: "var(--ink-dim)", marginTop: 4 }}>
            {isOnline
              ? account?.offlineProfile
                ? `Playing as ${account.offlineProfile.displayName}`
                : "No internet connection"
              : "No internet connection · Local features available"}
          </p>
          {isOnline && (
            <div style={{ marginTop: 8 }}>
              <Button size="sm" variant="secondary" block onClick={handleSwitchToMicrosoft}>
                Switch to Microsoft Login
              </Button>
            </div>
          )}
        </div>
        {signingIn && (
          <MicrosoftSignInModal onClose={() => setSigningIn(false)} onSignedIn={handleSignedIn} />
        )}
      </>
    );
  }

  return (
    <div
      className="panel"
      style={{
        padding: "10px 12px",
        borderColor: "rgba(79,156,240,0.25)",
        background: "var(--signal-blue-wash)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--signal-blue)" }}>
        <span className="status-dot" />
        <span style={{ fontSize: 12.5, fontWeight: 600 }}>Online</span>
      </div>
      <p style={{ fontSize: 12, color: "var(--ink-dim)", marginTop: 4 }}>
        {account?.microsoftGamertag
          ? `${account.microsoftGamertag} connected`
          : "Microsoft account connected"}
      </p>
      <div style={{ marginTop: 8 }}>
        <Button size="sm" variant="secondary" block onClick={onOpenSettings}>
          Account Settings
        </Button>
      </div>
    </div>
  );
}
