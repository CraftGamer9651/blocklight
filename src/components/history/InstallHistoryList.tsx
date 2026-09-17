import { useEffect, useState } from "react";
import { clearInstallHistory, listInstallHistory } from "@/lib/tauri";
import { contentTypeLabel, formatRelativeTime, platformLabel } from "@/lib/format";
import type { InstallHistoryEntry } from "@/lib/types";
import { Button } from "../shared/Primitives";

export function InstallHistoryList({ refreshKey }: { refreshKey?: number }) {
  const [entries, setEntries] = useState<InstallHistoryEntry[] | null>(null);

  useEffect(() => {
    listInstallHistory(20).then(setEntries).catch(() => setEntries([]));
  }, [refreshKey]);

  async function handleClear() {
    await clearInstallHistory();
    setEntries([]);
  }

  if (entries === null) return null;

  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div className="section-title">Recently Installed</div>
        {entries.length > 0 && (
          <Button size="sm" variant="ghost" onClick={handleClear}>
            Clear history
          </Button>
        )}
      </div>
      {entries.length === 0 ? (
        <p className="page-subtitle" style={{ marginTop: 8 }}>
          Nothing installed yet — paste a link above to get started.
        </p>
      ) : (
        <div className="list" style={{ marginTop: 4 }}>
          {entries.map((entry) => (
            <div className="list-row" key={entry.id}>
              <div className="list-row-main">
                <span className="list-row-title">{entry.projectName}</span>
                <span className="list-row-meta">
                  {platformLabel(entry.platform)} · {contentTypeLabel(entry.contentType)}
                </span>
              </div>
              <span className="list-row-meta">{formatRelativeTime(entry.installedAt)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
