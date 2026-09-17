import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { backupWorld, listWorlds } from "@/lib/tauri";
import { formatRelativeTime } from "@/lib/format";
import type { WorldSummary } from "@/lib/types";
import { Badge, Button } from "../shared/Primitives";

export function WorldsList({ instanceId }: { instanceId: string }) {
  const [worlds, setWorlds] = useState<WorldSummary[] | null>(null);
  const [backingUp, setBackingUp] = useState<string | null>(null);
  const [backedUp, setBackedUp] = useState<string | null>(null);

  useEffect(() => {
    listWorlds(instanceId).then(setWorlds).catch(() => setWorlds([]));
  }, [instanceId]);

  if (worlds === null || worlds.length === 0) return null;

  async function handleBackup(folderName: string) {
    setBackingUp(folderName);
    try {
      await backupWorld(instanceId, folderName);
      setBackedUp(folderName);
      setTimeout(() => setBackedUp(null), 2500);
    } finally {
      setBackingUp(null);
    }
  }

  return (
    <div>
      <div className="section-title">Worlds</div>
      <div className="list" style={{ marginTop: 4 }}>
        {worlds.map((world) => (
          <div className="list-row" key={world.folderName}>
            <div className="list-row-main">
              <span className="list-row-title">{world.name}</span>
              <span className="list-row-meta">
                {world.lastPlayed ? `Last played ${formatRelativeTime(world.lastPlayed)}` : "Never played"}
              </span>
            </div>
            <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
              {backedUp === world.folderName && (
                <Badge tone="success" dot>
                  Backed up
                </Badge>
              )}
              <Button size="sm" variant="ghost" onClick={() => void openPath(world.savePath)}>
                Open Folder
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void handleBackup(world.folderName)}
                disabled={backingUp === world.folderName}
              >
                {backingUp === world.folderName ? "Backing up…" : "Backup"}
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
