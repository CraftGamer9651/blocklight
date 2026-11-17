import { useEffect, useState } from "react";
import { installPerformancePack, performancePackInfo } from "@/lib/tauri";
import type { PerformancePackEntry, PerformancePackResult, Instance } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Badge, Button } from "../shared/Primitives";

/**
 * Collapsible one-click installer for well-known performance mods
 * (Sodium, Lithium, FerriteCore and friends). Blocklight doesn't ship its
 * own game client; these mods are what actually make Minecraft faster,
 * and the backend installs them through the normal Modrinth pipeline.
 * Vanilla instances get an explanation instead, since nothing can be
 * injected without a mod loader.
 */
export function PerformancePackPanel({
  instance,
  disabled,
  onInstalled,
}: {
  instance: Instance;
  disabled: boolean;
  onInstalled: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<PerformancePackEntry[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PerformancePackResult | null>(null);

  const vanilla = instance.loader === "minecraft";

  // Load the checklist up front so the header badge is accurate before
  // the panel is opened, and reload when the instance changes or an
  // install lands (instance.installed changes length).
  useEffect(() => {
    if (vanilla) return;
    let cancelled = false;
    performancePackInfo(instance.id)
      .then((list) => {
        if (!cancelled) setEntries(list);
      })
      .catch((err) => {
        if (!cancelled) setError(isAppError(err) ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [vanilla, instance.id, instance.installed.length]);

  useEffect(() => {
    setResult(null);
    setError(null);
    setEntries(null);
  }, [instance.id]);

  async function install() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await installPerformancePack(instance.id);
      setResult(res);
      onInstalled();
    } catch (err) {
      setError(isAppError(err) ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const missing = entries?.filter((e) => !e.alreadyInstalled).length ?? 0;

  const total = entries?.length ?? 0;
  const allInstalled = entries !== null && total > 0 && missing === 0;

  return (
    <div
      style={{
        marginTop: 14,
        padding: 14,
        borderRadius: 10,
        background: "var(--ember-wash)",
        border: "1px solid var(--ember)",
      }}
    >
      <div
        style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 12 }}
      >
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <strong style={{ fontSize: 14 }}>⚡ Performance Pack</strong>
            {!vanilla && entries && (
              <Badge tone={allInstalled ? "success" : "warn"}>
                {allInstalled ? "All installed" : `${total - missing} of ${total} installed`}
              </Badge>
            )}
          </div>
          <p className="page-subtitle" style={{ marginTop: 4 }}>
            {vanilla
              ? "Higher FPS and smoother gameplay with a mod loader."
              : allInstalled
                ? "This instance is already optimized."
                : "Get higher FPS and smoother gameplay in one click."}
          </p>
        </div>
        <Button
          variant={open || allInstalled || vanilla ? "secondary" : "primary"}
          size="sm"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
        >
          {open ? "Hide" : vanilla ? "Learn more" : allInstalled ? "View" : "Boost FPS"}
        </Button>
      </div>

      {open && (
        <div style={{ marginTop: 10 }}>
          {vanilla ? (
            <p className="page-subtitle">
              Performance mods need a mod loader. Create a Fabric or NeoForge instance for this
              Minecraft version and the pack can be installed there.
            </p>
          ) : (
            <>
              <p className="page-subtitle">
                Installs well-known optimization mods from Modrinth
                {instance.loader === "quilt" ? " (Fabric builds, which Quilt can load)" : ""}. Any
                without a build for Minecraft {instance.minecraftVersion} yet are skipped.
              </p>

              {entries && (
                <ul style={{ listStyle: "none", padding: 0, margin: "10px 0 0" }}>
                  {entries.map((e) => (
                    <li
                      key={e.slug}
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        gap: 12,
                        padding: "6px 0",
                      }}
                    >
                      <span>
                        <strong>{e.label}</strong>
                        <span className="page-subtitle"> — {e.purpose}</span>
                      </span>
                      {e.alreadyInstalled && <Badge tone="success">Installed</Badge>}
                    </li>
                  ))}
                </ul>
              )}

              <div style={{ marginTop: 10 }}>
                <Button
                  variant="primary"
                  size="sm"
                  disabled={disabled || busy || !entries || missing === 0}
                  onClick={() => void install()}
                >
                  {busy
                    ? "Installing…"
                    : entries && missing === 0
                      ? "Everything installed"
                      : "Install Performance Pack"}
                </Button>
              </div>

              {disabled && (
                <p className="page-subtitle" style={{ marginTop: 6 }}>
                  Stop the game before installing mods.
                </p>
              )}
            </>
          )}

          {error && (
            <p className="state-body" style={{ color: "var(--danger)", marginTop: 8 }}>
              {error}
            </p>
          )}

          {result && (
            <div style={{ marginTop: 8 }}>
              {result.installed.length > 0 && (
                <p className="state-body">Installed: {result.installed.join(", ")}.</p>
              )}
              {result.skipped
                .filter((s) => s.reason !== "Already installed")
                .map((s) => (
                  <p key={s.label} className="page-subtitle">
                    {s.label}: {s.reason}
                  </p>
                ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
