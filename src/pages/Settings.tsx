import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  activateOfflineProfile,
  deleteOfflineProfile,
  hasCurseForgeApiKey,
  listOfflineProfiles,
  setCurseForgeApiKey,
  switchToMicrosoft,
} from "@/lib/tauri";
import type { AccountState, OfflineProfile } from "@/lib/types";
import { Badge, Button } from "@/components/shared/Primitives";
import { OfflineProfileSetup } from "@/components/offline/OfflineProfileSetup";
import { MicrosoftSignInModal } from "@/components/offline/MicrosoftSignInModal";

export function SettingsPage({
  account,
  onAccountChange,
}: {
  account: AccountState | null;
  onAccountChange: () => void;
}) {
  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Settings</h1>
          <p className="page-subtitle">Accounts, content providers, and about Blocklight.</p>
        </div>
      </div>

      <AccountSection account={account} onAccountChange={onAccountChange} />
      <CurseForgeSection />
      <AboutSection />
    </div>
  );
}

function AccountSection({
  account,
  onAccountChange,
}: {
  account: AccountState | null;
  onAccountChange: () => void;
}) {
  const [profiles, setProfiles] = useState<OfflineProfile[]>([]);
  const [creating, setCreating] = useState(false);
  const [signingIn, setSigningIn] = useState(false);

  useEffect(() => {
    listOfflineProfiles().then(setProfiles).catch(() => {});
  }, [account]);

  return (
    <section className="panel">
      <div className="section-title">Account</div>

      <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 12 }}>
        <div className="list-row">
          <div className="list-row-main">
            <span className="list-row-title">Microsoft Account</span>
            <span className="list-row-meta">
              {account?.microsoftSignedIn
                ? account.microsoftGamertag ?? "Signed in"
                : "Sign in for online Minecraft features and account functionality."}
            </span>
          </div>
          {account?.activeKind === "microsoft" ? (
            <Badge tone="info" dot>
              Active
            </Badge>
          ) : account?.microsoftSignedIn ? (
            <Button size="sm" onClick={() => switchToMicrosoft().then(onAccountChange)}>
              Switch
            </Button>
          ) : (
            <Button size="sm" variant="primary" onClick={() => setSigningIn(true)}>
              Sign in
            </Button>
          )}
        </div>

        {profiles.map((profile) => (
          <OfflineProfileRow
            key={profile.id}
            profile={profile}
            isActive={account?.offlineProfile?.id === profile.id && account.activeKind === "offline"}
            onSwitch={() => void activateOfflineProfile(profile.id).then(onAccountChange)}
            onRemove={() => void deleteOfflineProfile(profile.id).then(onAccountChange)}
          />
        ))}
      </div>

      {creating ? (
        <div style={{ marginTop: 14 }}>
          <OfflineProfileSetup
            onCreated={() => {
              setCreating(false);
              onAccountChange();
            }}
          />
          <Button size="sm" variant="ghost" onClick={() => setCreating(false)} style={{ marginTop: 8 }}>
            Cancel
          </Button>
        </div>
      ) : (
        <Button size="sm" variant="secondary" onClick={() => setCreating(true)} style={{ marginTop: 12 }}>
          New offline profile
        </Button>
      )}

      {signingIn && (
        <MicrosoftSignInModal
          onClose={() => setSigningIn(false)}
          onSignedIn={() => {
            setSigningIn(false);
            onAccountChange();
          }}
        />
      )}
    </section>
  );
}

function OfflineProfileRow({
  profile,
  isActive,
  onSwitch,
  onRemove,
}: {
  profile: OfflineProfile;
  isActive: boolean;
  onSwitch: () => void;
  onRemove: () => void;
}) {
  const [confirming, setConfirming] = useState(false);

  return (
    <div className="list-row">
      <div className="list-row-main">
        <span className="list-row-title">{profile.displayName}</span>
        <span className="list-row-meta">Offline profile · Local</span>
      </div>
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        {isActive ? (
          <Badge tone="success" dot>
            Active
          </Badge>
        ) : (
          <Button size="sm" onClick={onSwitch}>
            Switch
          </Button>
        )}
        {confirming ? (
          <>
            <Button size="sm" variant="danger" onClick={onRemove}>
              Confirm
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </>
        ) : (
          <Button size="sm" variant="ghost" onClick={() => setConfirming(true)}>
            Remove
          </Button>
        )}
      </div>
    </div>
  );
}

function CurseForgeSection() {
  const [hasKey, setHasKey] = useState(false);
  const [input, setInput] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    hasCurseForgeApiKey().then(setHasKey).catch(() => {});
  }, []);

  async function save() {
    await setCurseForgeApiKey(input.trim() || null);
    setHasKey(input.trim().length > 0);
    setSaved(true);
    setInput("");
    setTimeout(() => setSaved(false), 2000);
  }

  return (
    <section className="panel">
      <div className="section-title">Content Providers</div>
      <p className="state-body" style={{ marginTop: 8 }}>
        Modrinth links and search work out of the box. CurseForge requires a personal API key
        from{" "}
        <button
          className="btn btn-ghost btn-sm"
          style={{ display: "inline-flex", height: "auto", padding: 0, color: "var(--signal-blue)" }}
          onClick={() => void openUrl("https://console.curseforge.com/")}
        >
          console.curseforge.com
        </button>
        .
      </p>
      <div className="meta-row" style={{ marginTop: 10 }}>
        {hasKey ? (
          <Badge tone="success" dot>
            CurseForge connected
          </Badge>
        ) : (
          <Badge tone="neutral">Not connected</Badge>
        )}
      </div>
      <div className="input-row" style={{ marginTop: 10 }}>
        <input
          className="text-input"
          type="password"
          placeholder="CurseForge API key"
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <Button size="sm" onClick={save} disabled={!input.trim()}>
          {saved ? "Saved" : "Save"}
        </Button>
      </div>
    </section>
  );
}

function AboutSection() {
  return (
    <section className="panel">
      <div className="section-title">About</div>
      <p className="state-body" style={{ marginTop: 8 }}>
        Blocklight is a launcher for managing modded Minecraft instances — install from Modrinth
        or CurseForge with a link, and keep playing when you're offline.
      </p>
      <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
        <Button size="sm" variant="secondary" onClick={() => void openUrl("https://github.com/CraftGamer9651/blocklight")}>
          GitHub
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void openUrl("https://craftgamer9651.github.io/")}>
          Website
        </Button>
      </div>
    </section>
  );
}
