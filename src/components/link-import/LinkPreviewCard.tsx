import { openUrl } from "@tauri-apps/plugin-opener";

import { contentTypeLabel, formatFileSize, loaderLabel, platformLabel } from "@/lib/format";
import type { ResolvedInstall } from "@/lib/types";
import { Badge, Button, Spinner } from "../shared/Primitives";
import { DependencyPanel } from "./DependencyPanel";

export function LinkPreviewCard({
  resolved,
  busy,
  onInstall,
  onCancel,
}: {
  resolved: ResolvedInstall;
  busy: boolean;
  onInstall: () => void;
  onCancel: () => void;
}) {
  const { project, compatibility, alreadyInstalled, dependencies } = resolved;
  const version = compatibility.selectedVersion;
  const file = version?.files.find((f) => f.primary) ?? version?.files[0];

  if (!compatibility.compatible) {
    return (
      <div className="panel-notched notched" style={{ marginTop: 14, borderColor: "rgba(226,88,79,0.3)" }}>
        <ProjectHeader project={project} />
        <div style={{ marginTop: 12 }}>
          <div className="state-title" style={{ color: "var(--danger)" }}>
            No compatible version found
          </div>
          <p className="state-body" style={{ marginTop: 4 }}>
            {compatibility.reason}
          </p>
        </div>
        {compatibility.availableAlternatives.length > 0 && (
          <div style={{ marginTop: 10 }}>
            <div className="section-title" style={{ marginBottom: 6 }}>
              Available
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              {compatibility.availableAlternatives.map((alt, i) => (
                <div key={i} className="meta-row">
                  Minecraft {alt.minecraftVersion} <span className="meta-sep">•</span> {loaderLabel(alt.loader)}
                </div>
              ))}
            </div>
          </div>
        )}
        <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
          <Button variant="secondary" size="sm" onClick={() => void openUrl(project.projectUrl)}>
            View on {platformLabel(project.platform)}
          </Button>
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Dismiss
          </Button>
        </div>
      </div>
    );
  }

  const alreadyUpToDate = alreadyInstalled && version && alreadyInstalled.versionId === version.id;
  const isUpdate = alreadyInstalled && version && alreadyInstalled.versionId !== version.id;

  return (
    <div className="panel-notched notched" style={{ marginTop: 14 }}>
      <ProjectHeader project={project} />

      <div className="meta-row" style={{ marginTop: 10 }}>
        <Badge tone="neutral">{contentTypeLabel(project.contentType)}</Badge>
        <Badge tone="neutral">{platformLabel(project.platform)}</Badge>
        {version && <Badge tone="neutral">Minecraft {version.gameVersions[0] ?? "—"}</Badge>}
        {version && version.loaders[0] && <Badge tone="neutral">{loaderLabel(version.loaders[0])}</Badge>}
      </div>

      {version && (
        <div className="meta-row" style={{ marginTop: 8 }}>
          <span className="tabular">Version {version.versionNumber}</span>
          {file && (
            <>
              <span className="meta-sep">•</span>
              <span className="tabular">{formatFileSize(file.sizeBytes)}</span>
            </>
          )}
        </div>
      )}

      <div style={{ marginTop: 10 }}>
        <Badge tone="success" dot>
          Compatible with your instance
        </Badge>
      </div>
      <p className="state-body" style={{ marginTop: 4 }}>
        {compatibility.reason}
      </p>

      {alreadyUpToDate && (
        <div style={{ marginTop: 10 }}>
          <Badge tone="info" dot>
            Already installed — {alreadyInstalled?.versionNumber}
          </Badge>
        </div>
      )}

      {isUpdate && alreadyInstalled && (
        <div className="state-block" style={{ marginTop: 10, padding: 12 }}>
          <div className="state-title" style={{ fontSize: 13.5 }}>
            {project.name} is already installed
          </div>
          <div className="meta-row">
            <span>Installed version: {alreadyInstalled.versionNumber}</span>
            <span className="meta-sep">•</span>
            <span>Available: {version?.versionNumber}</span>
          </div>
        </div>
      )}

      {dependencies.length > 0 && <DependencyPanel dependencies={dependencies} />}

      <div style={{ display: "flex", gap: 8, marginTop: 16 }}>
        {busy ? (
          <Spinner label="Installing…" />
        ) : (
          <>
            <Button variant="primary" onClick={onInstall}>
              {alreadyUpToDate
                ? `Reinstall ${contentTypeLabel(project.contentType)}`
                : isUpdate
                ? "Update"
                : `Install ${contentTypeLabel(project.contentType)}`}
            </Button>
            <Button variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

function ProjectHeader({ project }: { project: ResolvedInstall["project"] }) {
  return (
    <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
      {project.iconUrl ? (
        <img
          src={project.iconUrl}
          alt=""
          width={44}
          height={44}
          style={{ borderRadius: 8, flexShrink: 0, background: "var(--surface-raised)" }}
        />
      ) : (
        <div
          style={{
            width: 44,
            height: 44,
            borderRadius: 8,
            background: "var(--surface-raised)",
            flexShrink: 0,
          }}
        />
      )}
      <div style={{ minWidth: 0 }}>
        <h3 style={{ fontSize: 15.5 }}>{project.name}</h3>
        <p className="page-subtitle" style={{ marginTop: 2 }}>
          by {project.author}
        </p>
      </div>
    </div>
  );
}
