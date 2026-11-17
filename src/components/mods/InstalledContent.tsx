import { useCallback, useEffect, useState } from "react";

import {
  addLocalContent,
  getInstance,
  listInstalledContent,
  openContentFolder,
  removeInstalledContent,
} from "@/lib/tauri";
import { formatFileSize, platformLabel } from "@/lib/format";
import type { AddResult, ContentType, InstalledItem } from "@/lib/types";
import { Badge, Button, Spinner } from "@/components/shared/Primitives";

/** The list scrolls past this many rows instead of pushing the browse
 *  section off-screen. */
const LIST_MAX_HEIGHT = 300;

/** Only worth showing a filter box once the list is long. */
const FILTER_THRESHOLD = 8;

const COPY: Record<
  string,
  { title: string; noun: string; folder: string; addLabel: string; hint: string }
> = {
  mod: {
    title: "Installed mods",
    noun: "mods",
    folder: "mods folder",
    addLabel: "Add .jar files…",
    hint: "Mod not on Modrinth? Add its .jar file here and it's copied into this instance's mods folder.",
  },
  resource_pack: {
    title: "Installed resource packs",
    noun: "resource packs",
    folder: "resource packs folder",
    addLabel: "Add .zip files…",
    hint: "Pack not on Modrinth? Add its .zip file here and it's copied into this instance's resource packs folder.",
  },
  shader_pack: {
    title: "Installed shaders",
    noun: "shader packs",
    folder: "shader packs folder",
    addLabel: "Add .zip files…",
    hint: "Shader pack not on Modrinth? Add its .zip file here and it's copied into this instance's shader packs folder.",
  },
};

/**
 * "Installed ..." for the selected instance on the Mods, Resource Packs
 * and Shaders tabs: everything in the matching folder (including files
 * dropped in by hand), with a Remove button per entry, an "Add" button
 * for content that isn't on Modrinth or CurseForge, and a button that
 * opens the folder.
 *
 * Removing deletes the file for good, so it takes a second click on an
 * inline confirmation first. Added files are copied, never moved --
 * the originals stay where they were.
 */
export function InstalledContent({
  contentType,
  instanceId,
  refreshKey,
}: {
  contentType: ContentType;
  instanceId: string | null;
  /** Change this to force a reload (e.g. after a link install finishes). */
  refreshKey: number;
}) {
  const [instanceName, setInstanceName] = useState<string | null>(null);
  const copy = COPY[contentType];
  const [mods, setMods] = useState<InstalledItem[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [filter, setFilter] = useState("");

  const [confirming, setConfirming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [addResult, setAddResult] = useState<AddResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    if (!instanceId) return;
    try {
      const [list, instance] = await Promise.all([
        listInstalledContent(instanceId, contentType),
        getInstance(instanceId),
      ]);
      setMods(list);
      setInstanceName(instance.name);
      setLoadError(false);
    } catch {
      setLoadError(true);
      setMods((current) => current ?? []);
    }
  }, [instanceId, contentType]);

  // Reload when the instance or tab changes or the parent bumps `refreshKey`.
  // Everything transient is cleared on an instance switch so a pending
  // confirmation or banner never carries over to a different instance.
  useEffect(() => {
    setMods(null);
    setConfirming(null);
    setAddResult(null);
    setError(null);
    setFilter("");
    void reload();
  }, [instanceId, contentType, refreshKey, reload]);

  async function handleRemove(fileName: string) {
    if (!instanceId) return;
    setRemoving(fileName);
    setError(null);
    setAddResult(null);
    try {
      await removeInstalledContent(instanceId, contentType, fileName);
      setConfirming(null);
      await reload();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setRemoving(null);
    }
  }

  async function handleOpenFolder() {
    if (!instanceId) return;
    setError(null);
    try {
      await openContentFolder(instanceId, contentType);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleAdd() {
    if (!instanceId) return;
    setAdding(true);
    setError(null);
    setAddResult(null);
    setConfirming(null);
    try {
      const result = await addLocalContent(instanceId, contentType);
      if (!result.cancelled) setAddResult(result);
      await reload();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setAdding(false);
    }
  }

  if (!instanceId) {
    return (
      <div className="panel">
        <div className="section-title">{copy.title}</div>
        <p className="page-subtitle">Select or create an instance on Home to see its {copy.noun} here.</p>
      </div>
    );
  }

  const needle = filter.trim().toLowerCase();
  const visible =
    mods === null
      ? []
      : needle
        ? mods.filter(
            (m) =>
              m.fileName.toLowerCase().includes(needle) ||
              (m.projectSlug ?? "").toLowerCase().includes(needle)
          )
        : mods;

  return (
    <div className="panel" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 12 }}>
        <div style={{ minWidth: 0 }}>
          <div className="section-title">
            {copy.title}{mods !== null && ` (${mods.length})`}
          </div>
          {instanceName && (
            <div className="list-row-meta" style={{ marginTop: 2 }}>
              in {instanceName}
            </div>
          )}
        </div>
        <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
          <Button size="sm" variant="ghost" onClick={() => void handleOpenFolder()}>
            Open folder
          </Button>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void handleAdd()}
            disabled={adding || removing !== null}
          >
            {adding ? "Adding…" : copy.addLabel}
          </Button>
        </div>
      </div>

      <p className="list-row-meta">{copy.hint}</p>

      {addResult && <AddResultBanner result={addResult} onDismiss={() => setAddResult(null)} />}

      {error && (
        <p className="state-body" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}

      {mods === null ? (
        <Spinner label={`Reading ${copy.folder}…`} />
      ) : loadError && mods.length === 0 ? (
        <p className="page-subtitle">Couldn't read this instance's {copy.folder}.</p>
      ) : mods.length === 0 ? (
        <p className="page-subtitle">No {copy.noun} installed yet.</p>
      ) : (
        <>
          {mods.length > FILTER_THRESHOLD && (
            <input
              className="text-input"
              placeholder={`Filter installed ${copy.noun}…`}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          )}
          <div className="list" style={{ maxHeight: LIST_MAX_HEIGHT, overflowY: "auto" }}>
            {visible.map((mod) => (
              <ModRow
                key={mod.fileName}
                mod={mod}
                confirming={confirming === mod.fileName}
                removing={removing === mod.fileName}
                busy={removing !== null || adding}
                onAskRemove={() => setConfirming(mod.fileName)}
                onCancel={() => setConfirming(null)}
                onConfirm={() => void handleRemove(mod.fileName)}
              />
            ))}
            {visible.length === 0 && (
              <p className="page-subtitle" style={{ padding: "8px 2px" }}>
                No installed {copy.noun} match "{filter}".
              </p>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function ModRow({
  mod,
  confirming,
  removing,
  busy,
  onAskRemove,
  onCancel,
  onConfirm,
}: {
  mod: InstalledItem;
  confirming: boolean;
  removing: boolean;
  busy: boolean;
  onAskRemove: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="list-row">
      <div className="list-row-main">
        <span className="list-row-title" title={mod.fileName}>
          {mod.fileName}
        </span>
        <span className="list-row-meta" style={{ display: "flex", gap: 6, alignItems: "center" }}>
          {mod.platform ? (
            <Badge tone="neutral">{platformLabel(mod.platform)}</Badge>
          ) : (
            <Badge tone="info">Added manually</Badge>
          )}
          {mod.versionNumber && <span>v{mod.versionNumber}</span>}
          {mod.isFolder ? <span>Folder</span> : mod.sizeBytes !== null && <span>{formatFileSize(mod.sizeBytes)}</span>}
        </span>
      </div>

      {confirming ? (
        <div style={{ display: "flex", gap: 6, alignItems: "center", flexShrink: 0 }}>
          <span className="list-row-meta">
            Delete this {mod.isFolder ? "folder" : "file"}?
          </span>
          <Button size="sm" variant="danger" onClick={onConfirm} disabled={removing}>
            {removing ? "Removing…" : "Remove"}
          </Button>
          <Button size="sm" variant="ghost" onClick={onCancel} disabled={removing}>
            Cancel
          </Button>
        </div>
      ) : (
        <Button size="sm" variant="ghost" onClick={onAskRemove} disabled={busy}>
          Remove
        </Button>
      )}
    </div>
  );
}

function AddResultBanner({
  result,
  onDismiss,
}: {
  result: AddResult;
  onDismiss: () => void;
}) {
  const warned = result.added.filter((a) => a.warning);
  return (
    <div
      style={{
        border: "1px solid var(--line-soft)",
        borderRadius: 8,
        padding: "8px 10px",
        display: "flex",
        flexDirection: "column",
        gap: 4,
        fontSize: 12.5,
      }}
    >
      {result.added.length > 0 && (
        <div>
          Added {result.added.length} file{result.added.length === 1 ? "" : "s"}. They load the
          next time you launch.
        </div>
      )}
      {warned.map((a) => (
        <div key={`w:${a.fileName}`} style={{ color: "var(--ember)" }}>
          {a.fileName}: {a.warning}
        </div>
      ))}
      {result.skipped.map((s) => (
        <div key={`s:${s.fileName}`} style={{ color: "var(--danger)" }}>
          Skipped {s.fileName}: {s.reason}
        </div>
      ))}
      <div>
        <Button size="sm" variant="ghost" onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}

function errorMessage(err: unknown): string {
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}
