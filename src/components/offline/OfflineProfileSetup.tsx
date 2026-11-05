import { useState } from "react";
import { createOfflineProfile } from "@/lib/tauri";
import type { OfflineProfile } from "@/lib/types";
import { Badge, Button } from "../shared/Primitives";

export function OfflineProfileSetup({
  onCreated,
}: {
  onCreated: (profile: OfflineProfile) => void;
}) {
  const [name, setName] = useState("BlocklightUser");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleContinue() {
    setBusy(true);
    setError(null);
    try {
      const profile = await createOfflineProfile(name);
      onCreated(profile);
    } catch (err) {
      setError(err instanceof Object && "message" in err ? String((err as { message: unknown }).message) : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel-notched notched" style={{ maxWidth: 360 }}>
      <h3 style={{ fontSize: 15.5 }}>Offline Profile</h3>
      <div style={{ marginTop: 14 }}>
        <label className="field-label" htmlFor="player-name">
          Player Name
        </label>
        <input
          id="player-name"
          className="text-input"
          value={name}
          maxLength={24}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
        <Badge tone="success" dot>
          Local profile
        </Badge>
        <Badge tone="success" dot>
          No internet required
        </Badge>
      </div>
      {error && (
        <p className="state-body" style={{ color: "var(--danger)", marginTop: 10 }}>
          {error}
        </p>
      )}
      <div style={{ marginTop: 16 }}>
        <Button variant="primary" block onClick={handleContinue} disabled={busy || !name.trim()}>
          {busy ? "Creating…" : "Continue"}
        </Button>
      </div>
    </div>
  );
}
