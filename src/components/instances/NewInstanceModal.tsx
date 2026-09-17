import { useState } from "react";
import { createInstance } from "@/lib/tauri";
import type { AppError, Instance, Loader } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Button } from "../shared/Primitives";

const LOADER_OPTIONS: { value: Loader; label: string }[] = [
  { value: "fabric", label: "Fabric" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
  { value: "quilt", label: "Quilt" },
  { value: "minecraft", label: "Minecraft (vanilla / resource packs only)" },
];

export function NewInstanceModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (instance: Instance) => void;
}) {
  const [name, setName] = useState("");
  const [minecraftVersion, setMinecraftVersion] = useState("");
  const [loader, setLoader] = useState<Loader>("fabric");
  const [loaderVersion, setLoaderVersion] = useState("");
  const [javaVersion, setJavaVersion] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const canSubmit = name.trim().length > 0 && minecraftVersion.trim().length > 0 && !busy;

  async function handleSubmit() {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      const instance = await createInstance({
        name,
        minecraftVersion,
        loader,
        loaderVersion: loaderVersion.trim() || null,
        javaMajorVersion: javaVersion.trim() ? Number(javaVersion.trim()) : null,
      });
      onCreated(instance);
    } catch (err) {
      setError(isAppError(err) ? err : { kind: "internal", message: String(err) });
    } finally {
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
      onClick={onClose}
    >
      <div
        className="panel"
        style={{ width: 400, boxShadow: "var(--shadow-panel)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 style={{ fontSize: 16 }}>New Instance</h3>
        <p className="page-subtitle" style={{ marginTop: 4 }}>
          Creates a local instance folder Blocklight can install content into. This doesn't
          download Minecraft or the loader itself yet.
        </p>

        <div style={{ marginTop: 14, display: "flex", flexDirection: "column", gap: 12 }}>
          <div>
            <label className="field-label" htmlFor="instance-name">
              Name
            </label>
            <input
              id="instance-name"
              className="text-input"
              placeholder="Modded Survival"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
            />
          </div>

          <div style={{ display: "flex", gap: 10 }}>
            <div style={{ flex: 1 }}>
              <label className="field-label" htmlFor="instance-mc-version">
                Minecraft version
              </label>
              <input
                id="instance-mc-version"
                className="text-input"
                placeholder="1.21.1"
                value={minecraftVersion}
                onChange={(e) => setMinecraftVersion(e.target.value)}
              />
            </div>
            <div style={{ flex: 1 }}>
              <label className="field-label" htmlFor="instance-loader">
                Loader
              </label>
              <select
                id="instance-loader"
                className="text-input"
                value={loader}
                onChange={(e) => setLoader(e.target.value as Loader)}
              >
                {LOADER_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div style={{ display: "flex", gap: 10 }}>
            <div style={{ flex: 1 }}>
              <label className="field-label" htmlFor="instance-loader-version">
                Loader version <span style={{ opacity: 0.6 }}>(optional)</span>
              </label>
              <input
                id="instance-loader-version"
                className="text-input"
                placeholder="0.16.9"
                value={loaderVersion}
                onChange={(e) => setLoaderVersion(e.target.value)}
              />
            </div>
            <div style={{ flex: 1 }}>
              <label className="field-label" htmlFor="instance-java">
                Java version <span style={{ opacity: 0.6 }}>(optional)</span>
              </label>
              <input
                id="instance-java"
                className="text-input"
                placeholder="21"
                inputMode="numeric"
                value={javaVersion}
                onChange={(e) => setJavaVersion(e.target.value.replace(/[^0-9]/g, ""))}
              />
            </div>
          </div>
        </div>

        {error && (
          <p className="state-body" style={{ color: "var(--danger)", marginTop: 12 }}>
            {error.message}
          </p>
        )}

        <div style={{ display: "flex", gap: 8, marginTop: 16 }}>
          <Button variant="primary" onClick={handleSubmit} disabled={!canSubmit}>
            {busy ? "Creating…" : "Create Instance"}
          </Button>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        </div>
      </div>
    </div>
  );
}
