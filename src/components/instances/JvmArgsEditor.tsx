import { useEffect, useState, type CSSProperties } from "react";
import { setInstanceJvmArgs } from "@/lib/tauri";
import type { AppError, Instance } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Button } from "../shared/Primitives";

/** Shared by the editor and the New Instance modal. `.text-input` is a
 *  fixed-height single-line style, so a textarea needs these overrides. */
export const JVM_ARGS_TEXTAREA_STYLE: CSSProperties = {
  height: "auto",
  minHeight: 56,
  padding: "8px 12px",
  resize: "vertical",
  lineHeight: 1.4,
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace',
  fontSize: 12.5,
};

/**
 * Collapsible per-instance editor for custom JVM arguments. Saves go
 * through the backend, which validates (quotes, blocked flags, stray
 * words) and returns the specific reason on failure. Changes apply on
 * the next launch, so editing is disabled while the game is running.
 */
export function JvmArgsEditor({
  instance,
  disabled,
  onSaved,
}: {
  instance: Instance;
  disabled: boolean;
  onSaved: () => void;
}) {
  const saved = instance.jvmArgs ?? "";
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(saved);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  // Re-sync when the selected instance changes or a save lands.
  useEffect(() => {
    setDraft(saved);
    setError(null);
  }, [instance.id, saved]);

  const dirty = draft.trim() !== saved.trim();

  async function save(next: string) {
    setBusy(true);
    setError(null);
    try {
      await setInstanceJvmArgs(instance.id, next.trim() || null);
      onSaved();
    } catch (err) {
      setError(isAppError(err) ? err : { kind: "internal", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ marginTop: 14 }}>
      <Button variant="ghost" size="sm" onClick={() => setOpen((o) => !o)}>
        {open ? "Hide" : "Edit"} JVM arguments
        {!open && saved.trim() ? " (custom)" : ""}
      </Button>

      {open && (
        <div style={{ marginTop: 8 }}>
          <label className="field-label" htmlFor={`jvm-args-${instance.id}`}>
            Custom JVM arguments
          </label>
          <textarea
            id={`jvm-args-${instance.id}`}
            className="text-input"
            style={JVM_ARGS_TEXTAREA_STYLE}
            placeholder="-XX:+UseZGC -Dexample=value"
            rows={3}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            value={draft}
            disabled={disabled || busy}
            onChange={(e) => setDraft(e.target.value)}
          />
          <p className="page-subtitle" style={{ marginTop: 6 }}>
            Added after Blocklight's defaults, so these win where they overlap (for example,
            setting <code>-Xmx</code> replaces the memory setting, and picking another garbage
            collector replaces G1). Applied on the next launch. Classpath, module path and
            native-library flags are managed by Blocklight and can't be overridden.
          </p>

          {error && (
            <p className="state-body" style={{ color: "var(--danger)", marginTop: 8 }}>
              {error.message}
            </p>
          )}

          <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
            <Button
              variant="primary"
              size="sm"
              disabled={disabled || busy || !dirty}
              onClick={() => void save(draft)}
            >
              {busy ? "Saving…" : "Save"}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={disabled || busy || (!saved.trim() && !draft.trim())}
              onClick={() => {
                setDraft("");
                void save("");
              }}
            >
              Clear
            </Button>
          </div>
          {disabled && (
            <p className="page-subtitle" style={{ marginTop: 6 }}>
              Stop the game to change its arguments.
            </p>
          )}
        </div>
      )}
    </div>
  );
}
