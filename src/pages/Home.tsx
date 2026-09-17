import { useEffect, useState } from "react";
import { checkOfflineReadiness } from "@/lib/tauri";
import { loaderLabel } from "@/lib/format";
import type { ConnectivityState, Instance, OfflineReadiness } from "@/lib/types";
import { Badge, Button } from "@/components/shared/Primitives";
import { PasteLinkPanel } from "@/components/link-import/PasteLinkPanel";
import { LinkFlowResult } from "@/components/link-import/LinkFlowResult";
import { InstallHistoryList } from "@/components/history/InstallHistoryList";
import { WorldsList } from "@/components/history/WorldsList";
import { NewInstanceModal } from "@/components/instances/NewInstanceModal";
import { LaunchPanel } from "@/components/instances/LaunchPanel";
import { useLinkInstallFlow } from "@/hooks/useLinkInstallFlow";
import { useLaunch } from "@/hooks/useLaunch";

export function HomePage({
  instances,
  selected,
  selectedId,
  onSelect,
  connectivity,
  onInstanceCreated,
}: {
  instances: Instance[];
  selected: Instance | null;
  selectedId: string | null;
  onSelect: (id: string) => void;
  connectivity: ConnectivityState;
  onInstanceCreated: (instance: Instance) => void;
}) {
  const [installKey, setInstallKey] = useState(0);
  const [creatingInstance, setCreatingInstance] = useState(false);
  const flow = useLinkInstallFlow(selectedId, () => setInstallKey((k) => k + 1));

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Home</h1>
          <p className="page-subtitle">
            {connectivity === "offline"
              ? "Playing locally — internet connection not required."
              : "Your instances, mods, and worlds, all in one place."}
          </p>
        </div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          {instances.length > 1 && selectedId && (
            <select
              className="text-input"
              style={{ width: 200 }}
              value={selectedId}
              onChange={(e) => onSelect(e.target.value)}
            >
              {instances.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name}
                </option>
              ))}
            </select>
          )}
          <Button variant="secondary" size="sm" onClick={() => setCreatingInstance(true)}>
            New Instance
          </Button>
        </div>
      </div>

      {selected && <InstanceTile instance={selected} refreshKey={installKey} />}

      <PasteLinkPanel instanceId={selectedId} flow={flow} />
      <LinkFlowResult flow={flow} />

      {selected && <WorldsList instanceId={selected.id} />}

      <InstallHistoryList refreshKey={installKey} />

      {creatingInstance && (
        <NewInstanceModal
          onClose={() => setCreatingInstance(false)}
          onCreated={(instance) => {
            setCreatingInstance(false);
            onInstanceCreated(instance);
          }}
        />
      )}
    </div>
  );
}

function InstanceTile({ instance, refreshKey }: { instance: Instance; refreshKey: number }) {
  const [readiness, setReadiness] = useState<OfflineReadiness | null>(null);
  const launch = useLaunch(instance.id);
  const busy = launch.state.kind === "preparing" || launch.state.kind === "running";

  useEffect(() => {
    checkOfflineReadiness(instance.id).then(setReadiness).catch(() => {});
  }, [instance.id, instance.installed.length, refreshKey]);

  return (
    <div className="tile notched" style={{ padding: 20 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
        <div>
          <h2 style={{ fontSize: 17 }}>{instance.name}</h2>
          <div className="meta-row" style={{ marginTop: 6 }}>
            <span>Minecraft {instance.minecraftVersion}</span>
            <span className="meta-sep">•</span>
            <span>{loaderLabel(instance.loader)}</span>
            <span className="meta-sep">•</span>
            <span>{instance.installed.length} mods</span>
            {instance.javaMajorVersion && (
              <>
                <span className="meta-sep">•</span>
                <span>Java {instance.javaMajorVersion}</span>
              </>
            )}
          </div>
        </div>
        {!busy && (
          <Button variant="primary" onClick={() => void launch.launch()}>
            Play
          </Button>
        )}
      </div>

      <div style={{ marginTop: 14 }}>
        {readiness === null ? null : readiness.ready ? (
          <Badge tone="success" dot>
            Ready for offline play
          </Badge>
        ) : (
          <Badge tone="warn" dot>
            Offline launch unavailable — missing {readiness.missing.length} file
            {readiness.missing.length === 1 ? "" : "s"}
          </Badge>
        )}
      </div>

      <LaunchPanel
        state={launch.state}
        logLines={launch.logLines}
        onStop={() => void launch.stop()}
        onReset={launch.reset}
      />
    </div>
  );
}
