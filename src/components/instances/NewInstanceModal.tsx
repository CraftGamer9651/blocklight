import { useEffect, useMemo, useState } from "react";
import { createInstance, listLoaderVersions, listMinecraftVersions, recommendedJava } from "@/lib/tauri";
import type { AppError, Instance, Loader, LoaderVersionOption, McVersion } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Button } from "../shared/Primitives";
import { JVM_ARGS_TEXTAREA_STYLE } from "./JvmArgsEditor";

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
  // "auto" = use Mojang's recommended Java for the chosen Minecraft version.
  const [javaChoice, setJavaChoice] = useState("auto");
  const [mcVersions, setMcVersions] = useState<McVersion[] | null>(null);
  const [mcListFailed, setMcListFailed] = useState(false);
  const [showSnapshots, setShowSnapshots] = useState(false);
  // null while loading; [] when none are published or the lookup failed.
  const [loaderVersions, setLoaderVersions] = useState<LoaderVersionOption[] | null>([]);
  const [recommendedJavaMajor, setRecommendedJavaMajor] = useState<number | null>(null);
  const [memoryMb, setMemoryMb] = useState("");
  const [jvmArgs, setJvmArgs] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  // Minecraft versions, once, on open.
  useEffect(() => {
    let cancelled = false;
    listMinecraftVersions()
      .then((list) => {
        if (!cancelled) setMcVersions(list);
      })
      .catch(() => {
        if (!cancelled) setMcListFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const visibleVersions = useMemo(
    () => (mcVersions ?? []).filter((v) => showSnapshots || v.kind === "release"),
    [mcVersions, showSnapshots]
  );

  // Pick the newest release by default, and recover if the selected
  // version disappears (snapshots toggled off).
  useEffect(() => {
    if (!mcVersions) return;
    if (visibleVersions.some((v) => v.id === minecraftVersion)) return;
    const newest = mcVersions.find((v) => v.kind === "release") ?? visibleVersions[0];
    setMinecraftVersion(newest ? newest.id : "");
  }, [mcVersions, visibleVersions, minecraftVersion]);

  // Loader builds for the chosen loader + Minecraft version.
  useEffect(() => {
    setLoaderVersion("");
    if (loader === "minecraft" || !minecraftVersion.trim()) {
      setLoaderVersions([]);
      return;
    }
    let cancelled = false;
    setLoaderVersions(null);
    const timer = setTimeout(() => {
      listLoaderVersions(loader, minecraftVersion.trim())
        .then((list) => {
          if (!cancelled) setLoaderVersions(list);
        })
        .catch(() => {
          if (!cancelled) setLoaderVersions([]);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [loader, minecraftVersion]);

  // Mojang's recommended Java for the chosen Minecraft version.
  useEffect(() => {
    setRecommendedJavaMajor(null);
    setJavaChoice("auto");
    if (!minecraftVersion.trim()) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      recommendedJava(minecraftVersion.trim())
        .then((major) => {
          if (!cancelled) setRecommendedJavaMajor(major);
        })
        .catch(() => {});
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [minecraftVersion]);

  const javaOptions = useMemo(() => {
    const majors = new Set([8, 17, 21, 25]);
    if (recommendedJavaMajor) majors.add(recommendedJavaMajor);
    return [...majors].sort((a, b) => a - b);
  }, [recommendedJavaMajor]);

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
        javaMajorVersion: javaChoice === "auto" ? recommendedJavaMajor : Number(javaChoice),
        memoryMb: memoryMb.trim() ? Number(memoryMb.trim()) : null,
        jvmArgs: jvmArgs.trim() || null,
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
              {mcListFailed ? (
                <input
                  id="instance-mc-version"
                  className="text-input"
                  placeholder="1.21.1"
                  value={minecraftVersion}
                  onChange={(e) => setMinecraftVersion(e.target.value)}
                />
              ) : (
                <select
                  id="instance-mc-version"
                  className="text-input"
                  value={minecraftVersion}
                  disabled={!mcVersions}
                  onChange={(e) => setMinecraftVersion(e.target.value)}
                >
                  {!mcVersions && <option value="">Loading…</option>}
                  {visibleVersions.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.id}
                      {v.kind === "snapshot" ? " (snapshot)" : ""}
                    </option>
                  ))}
                </select>
              )}
              {mcListFailed ? (
                <p className="page-subtitle" style={{ marginTop: 4 }}>
                  Couldn't load the version list. Type a version instead.
                </p>
              ) : (
                <label
                  className="page-subtitle"
                  style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 4 }}
                >
                  <input
                    type="checkbox"
                    checked={showSnapshots}
                    onChange={(e) => setShowSnapshots(e.target.checked)}
                  />
                  Show snapshots
                </label>
              )}
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
                Loader version
              </label>
              <select
                id="instance-loader-version"
                className="text-input"
                value={loaderVersion}
                disabled={loader === "minecraft" || loaderVersions === null}
                onChange={(e) => setLoaderVersion(e.target.value)}
              >
                {loader === "minecraft" ? (
                  <option value="">None needed</option>
                ) : loaderVersions === null ? (
                  <option value="">Loading…</option>
                ) : (
                  <>
                    <option value="">Latest (recommended)</option>
                    {loaderVersions.map((v) => (
                      <option key={v.version} value={v.version}>
                        {v.version}
                        {v.stable ? "" : " (beta)"}
                      </option>
                    ))}
                  </>
                )}
              </select>
              {loader !== "minecraft" && loaderVersions !== null && loaderVersions.length === 0 && (
                <p className="page-subtitle" style={{ marginTop: 4 }}>
                  No builds listed for this Minecraft version. The latest is picked at launch.
                </p>
              )}
            </div>
            <div style={{ flex: 1 }}>
              <label className="field-label" htmlFor="instance-java">
                Java version
              </label>
              <select
                id="instance-java"
                className="text-input"
                value={javaChoice}
                onChange={(e) => setJavaChoice(e.target.value)}
              >
                <option value="auto">
                  Recommended{recommendedJavaMajor ? ` (Java ${recommendedJavaMajor})` : ""}
                </option>
                {javaOptions.map((major) => (
                  <option key={major} value={String(major)}>
                    Java {major}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div>
            <label className="field-label" htmlFor="instance-memory">
              Memory (MB) <span style={{ opacity: 0.6 }}>(optional — auto-detected if left blank)</span>
            </label>
            <input
              id="instance-memory"
              className="text-input"
              placeholder="Auto"
              inputMode="numeric"
              value={memoryMb}
              onChange={(e) => setMemoryMb(e.target.value.replace(/[^0-9]/g, ""))}
            />
          </div>

          <div>
            <label className="field-label" htmlFor="instance-jvm-args">
              JVM arguments <span style={{ opacity: 0.6 }}>(optional)</span>
            </label>
            <textarea
              id="instance-jvm-args"
              className="text-input"
              style={JVM_ARGS_TEXTAREA_STYLE}
              placeholder="-XX:+UseZGC -Dexample=value"
              rows={2}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              value={jvmArgs}
              onChange={(e) => setJvmArgs(e.target.value)}
            />
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
