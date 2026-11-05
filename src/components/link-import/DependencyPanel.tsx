import type { DependencyCheck } from "@/lib/types";
import { Badge } from "../shared/Primitives";

export function DependencyPanel({ dependencies }: { dependencies: DependencyCheck[] }) {
  const toInstall = dependencies.filter((d) => !d.alreadyInstalled);
  if (toInstall.length === 0) return null;

  const allCompatible = toInstall.every((d) => d.compatible);

  return (
    <div style={{ marginTop: 14 }}>
      <div className="section-title" style={{ marginBottom: 8 }}>
        {toInstall.length} {toInstall.length === 1 ? "dependency" : "dependencies"} required
      </div>
      <div className="list">
        {toInstall.map((dep, i) => {
          const name =
            dep.resolvedProject?.name ?? dep.dependency.projectName ?? "Unknown dependency";
          return (
            <div className="list-row" key={i}>
              <div className="list-row-main">
                <span className="list-row-title">{name}</span>
                {dep.resolvedVersion && (
                  <span className="list-row-meta tabular">{dep.resolvedVersion.versionNumber}</span>
                )}
              </div>
              {dep.compatible ? (
                <Badge tone="success" dot>
                  Compatible
                </Badge>
              ) : (
                <Badge tone="danger" dot>
                  Not compatible
                </Badge>
              )}
            </div>
          );
        })}
      </div>
      {!allCompatible && (
        <p className="state-body" style={{ marginTop: 8, color: "var(--danger)" }}>
          One or more dependencies aren't compatible with this instance, so this can't be installed yet.
        </p>
      )}
    </div>
  );
}
