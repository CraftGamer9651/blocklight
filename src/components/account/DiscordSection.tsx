import { useEffect, useState } from "react";

import { getDiscordPresenceStatus, setDiscordPresenceEnabled } from "@/lib/tauri";
import type { DiscordPresenceStatus } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Badge, Button } from "@/components/shared/Primitives";

/**
 * Opt-in toggle for showing the running instance in the user's Discord
 * status. Off by default because it makes what you play visible to your
 * Discord friends. The Discord application it uses is built into the app
 * (see `presence.rs`), so there's nothing for the user to configure.
 */
export function DiscordSection() {
  const [status, setStatus] = useState<DiscordPresenceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getDiscordPresenceStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  async function toggle() {
    if (!status) return;
    setBusy(true);
    setError(null);
    try {
      setStatus(await setDiscordPresenceEnabled(!status.enabled));
    } catch (err) {
      setError(isAppError(err) ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel">
      <div className="section-title">Discord</div>

      <div className="list-row" style={{ marginTop: 12 }}>
        <div className="list-row-main">
          <span className="list-row-title">Show what I'm playing</span>
          <span className="list-row-meta">
            While a game is running, your Discord status shows the instance name, its loader and
            Minecraft version, and how long you've been playing. Nothing else — no account name,
            files, or servers. Needs the Discord desktop app running on this computer.
          </span>
        </div>
        {status === null ? null : (
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            {status.enabled && status.configured && (
              <Badge tone="success" dot>
                On
              </Badge>
            )}
            <Button
              size="sm"
              variant={status.enabled ? "secondary" : "primary"}
              disabled={busy || !status.configured}
              onClick={() => void toggle()}
            >
              {status.enabled ? "Turn off" : "Turn on"}
            </Button>
          </div>
        )}
      </div>

      {status && !status.configured && (
        <p className="state-body" style={{ color: "var(--danger)", marginTop: 8 }}>
          {status.configProblem ?? "Discord status isn't available in this build."}
        </p>
      )}

      {error && (
        <p className="state-body" style={{ color: "var(--danger)", marginTop: 8 }}>
          {error}
        </p>
      )}
    </section>
  );
}
