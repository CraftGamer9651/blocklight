import { HashRouter, Navigate, Route, Routes } from "react-router-dom";

import { useAccountState } from "@/hooks/useAccountState";
import { useConnectivity } from "@/hooks/useConnectivity";
import { useInstances } from "@/hooks/useInstances";

import { Sidebar } from "@/components/layout/Sidebar";
import { ConnectivityToast } from "@/components/shared/ConnectivityToast";
import { FirstRunChoice } from "@/components/offline/FirstRunChoice";

import { HomePage } from "@/pages/Home";
import { ContentLibraryPage } from "@/pages/ContentLibrary";
import { SettingsPage } from "@/pages/Settings";

export default function App() {
  const { account, refresh: refreshAccount } = useAccountState();
  const connectivity = useConnectivity();
  const { instances, selected, selectedId, setSelectedId, refresh: refreshInstances } =
    useInstances();

  if (account === null) {
    // Still loading account state from the local database -- avoid a
    // first-run flash while that resolves.
    return <div style={{ height: "100vh", background: "var(--bg)" }} />;
  }

  const needsFirstRun = !account.microsoftSignedIn && !account.offlineProfile;

  if (needsFirstRun) {
    return (
      <div
        style={{
          height: "100vh",
          background: "var(--bg)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: 24,
        }}
      >
        <FirstRunChoice onComplete={refreshAccount} />
      </div>
    );
  }

  return (
    <HashRouter>
      <div className="app-shell">
        <Sidebar connectivity={connectivity} account={account} onAccountChange={refreshAccount} />
        <main className="main">
          <Routes>
            <Route
              path="/"
              element={
                <HomePage
                  instances={instances}
                  selected={selected}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                  connectivity={connectivity}
                  onInstanceCreated={async (instance) => {
                    await refreshInstances();
                    setSelectedId(instance.id);
                  }}
                />
              }
            />
            <Route
              path="/mods"
              element={<ContentLibraryPage contentType="mod" instanceId={selectedId} />}
            />
            <Route
              path="/resource-packs"
              element={<ContentLibraryPage contentType="resource_pack" instanceId={selectedId} />}
            />
            <Route
              path="/shaders"
              element={<ContentLibraryPage contentType="shader_pack" instanceId={selectedId} />}
            />
            <Route
              path="/settings"
              element={<SettingsPage account={account} onAccountChange={refreshAccount} />}
            />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </main>
      </div>
      <ConnectivityToast />
    </HashRouter>
  );
}
