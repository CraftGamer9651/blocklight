import { useState } from "react";
import { removeInstance } from "@/lib/tauri";
import type { AppError, Instance } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Button } from "../shared/Primitives";

/**
 * Confirmation for the one irreversible action in the app. Removing an
 * instance deletes its whole folder -- mods, worlds, configs -- so the
 * wording says exactly that, shows the path, and the primary button is
 * the red one rather than the default.
 */
export function RemoveInstanceModal({
  instance,
  onClose,
  onRemoved,
}: {
  instance: Instance;
  onClose: () => void;
  onRemoved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  async function handleRemove() {
    setBusy(true);
    setError(null);
    try {
      await removeInstance(instance.id);
      onRemoved();
    } catch (err) {
      setError(isAppError(err) ? err : { kind: "internal", message: String(err) });
      setBusy(false);
    }
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 50,
      }}
      onClick={busy ? undefined : onClose}
    >
      <div
        className="panel"
        style={{ width: 400, boxShadow: "var(--shadow-panel)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 style={{ fontSize: 16 }}>Remove “{instance.name}”?</h3>
        <p className="page-subtitle" style={{ marginTop: 8 }}>
          This permanently deletes the instance's folder from your computer, including its
          mods, worlds, screenshots and settings. This can't be undone.
        </p>
        <p
          className="page-subtitle"
          style={{ marginTop: 8, wordBreak: "break-all", opacity: 0.8 }}
        >
          {instance.instanceDir}
        </p>

        {error && (
          <p className="state-body" style={{ color: "var(--danger)", marginTop: 12 }}>
            {error.message}
          </p>
        )}

        <div style={{ display: "flex", gap: 8, marginTop: 16 }}>
          <Button variant="danger" onClick={() => void handleRemove()} disabled={busy}>
            {busy ? "Removing…" : "Remove and delete files"}
          </Button>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        </div>
      </div>
    </div>
  );
}
