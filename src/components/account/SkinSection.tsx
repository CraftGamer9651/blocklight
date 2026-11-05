import { useEffect, useRef, useState } from "react";

import { applySkin, fetchSkinFromLink, getSkinStatus, inspectSkinPng } from "@/lib/tauri";
import type { AccountState, SkinStatus } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Badge, Button, Spinner } from "@/components/shared/Primitives";
import { MicrosoftSignInModal } from "@/components/offline/MicrosoftSignInModal";

/** Mirrors the backend's limit; checked here only to fail fast on an
 *  obviously wrong file before reading it into memory. */
const MAX_SKIN_BYTES = 256 * 1024;

type Variant = "classic" | "slim";

interface PendingSkin {
  bytes: number[];
  /** `data:` URL for the preview (the app's CSP allows `data:` images). */
  dataUrl: string;
  /** Where it came from, shown under the preview. */
  source: string;
}

/**
 * Change the active Microsoft account's skin from a PNG file or a SkinMC
 * link. The flow is: pick a source -> the file is validated and previewed
 * -> choose the arm model -> Apply. Nothing is sent to Minecraft until
 * the user presses Apply.
 */
export function SkinSection({
  account,
  onAccountChange,
}: {
  account: AccountState | null;
  onAccountChange: () => void;
}) {
  const [status, setStatus] = useState<SkinStatus | null>(null);
  const [link, setLink] = useState("");
  const [pending, setPending] = useState<PendingSkin | null>(null);
  const [variant, setVariant] = useState<Variant>("classic");
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applied, setApplied] = useState(false);
  const [signingIn, setSigningIn] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  function refreshStatus() {
    getSkinStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }

  useEffect(refreshStatus, [account]);

  const canChange = status?.canChange === true;

  async function choosePreview(bytes: number[], source: string) {
    setPending({ bytes, dataUrl: bytesToPngDataUrl(bytes), source });
    setApplied(false);
  }

  async function onFilePicked(file: File | undefined) {
    if (!file) return;
    setError(null);
    setApplied(false);
    if (file.size > MAX_SKIN_BYTES) {
      setError("That file is far too large for a skin — they're only a few KB.");
      return;
    }
    setLoading(true);
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      await inspectSkinPng(bytes);
      await choosePreview(bytes, file.name);
    } catch (err) {
      setPending(null);
      setError(describeError(err));
    } finally {
      setLoading(false);
      // Allow picking the same file again after an error.
      if (fileInput.current) fileInput.current.value = "";
    }
  }

  async function onFetchLink() {
    setError(null);
    setApplied(false);
    setLoading(true);
    try {
      const bytes = await fetchSkinFromLink(link);
      await choosePreview(bytes, "SkinMC");
    } catch (err) {
      setPending(null);
      setError(describeError(err));
    } finally {
      setLoading(false);
    }
  }

  async function onApply() {
    if (!pending) return;
    setError(null);
    setApplying(true);
    try {
      await applySkin(pending.bytes, variant);
      setApplied(true);
      setPending(null);
      setLink("");
    } catch (err) {
      if (isAppError(err) && err.kind === "sign_in_required") {
        // The token was rejected; the backend dropped it. Re-check status
        // so the "Sign in again" prompt appears.
        refreshStatus();
      }
      setError(describeError(err));
    } finally {
      setApplying(false);
    }
  }

  const name =
    account?.activeKind === "microsoft" ? account.microsoftGamertag ?? "your account" : null;

  return (
    <section className="panel">
      <div className="section-title">Skin</div>

      {status === null ? (
        <div style={{ marginTop: 12 }}>
          <Spinner label="Checking account…" />
        </div>
      ) : !canChange ? (
        <div style={{ marginTop: 8 }}>
          <p className="state-body">{status.reason}</p>
          {status.needsSignIn && (
            <Button
              size="sm"
              variant="primary"
              onClick={() => setSigningIn(true)}
              style={{ marginTop: 10 }}
            >
              Sign in again
            </Button>
          )}
        </div>
      ) : (
        <>
          <p className="state-body" style={{ marginTop: 8 }}>
            Change the skin on {name}. Upload a 64×64 PNG, or paste a link to a skin on SkinMC.
          </p>

          <div style={{ marginTop: 12 }}>
            <input
              ref={fileInput}
              type="file"
              accept="image/png"
              style={{ display: "none" }}
              onChange={(e) => void onFilePicked(e.target.files?.[0])}
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={loading || applying}
              onClick={() => fileInput.current?.click()}
            >
              Upload a PNG file…
            </Button>
          </div>

          <div className="input-row" style={{ marginTop: 10 }}>
            <input
              className="text-input"
              placeholder="https://skinmc.net/skins/…"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              value={link}
              disabled={loading || applying}
              onChange={(e) => setLink(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && link.trim()) void onFetchLink();
              }}
            />
            <Button
              size="sm"
              disabled={!link.trim() || loading || applying}
              onClick={() => void onFetchLink()}
            >
              {loading ? "Fetching…" : "Fetch"}
            </Button>
          </div>

          {error && (
            <p className="state-body" style={{ color: "var(--danger)", marginTop: 10 }}>
              {error}
            </p>
          )}

          {applied && (
            <div className="meta-row" style={{ marginTop: 12 }}>
              <Badge tone="success" dot>
                Skin updated
              </Badge>
              <span className="page-subtitle">
                It shows up the next time you join a game, and can take a minute to appear.
              </span>
            </div>
          )}

          {pending && (
            <div style={{ marginTop: 14, display: "flex", gap: 16, alignItems: "flex-start" }}>
              <div>
                <img
                  src={pending.dataUrl}
                  alt="Skin texture preview"
                  style={{
                    width: 128,
                    height: "auto",
                    imageRendering: "pixelated",
                    background: "var(--surface-hover)",
                    borderRadius: 6,
                    display: "block",
                  }}
                />
                <p className="page-subtitle" style={{ marginTop: 6, maxWidth: 128 }}>
                  {pending.source}
                </p>
              </div>

              <div style={{ flex: 1 }}>
                <div className="field-label">Arm model</div>
                <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
                  <Button
                    size="sm"
                    variant={variant === "classic" ? "primary" : "secondary"}
                    onClick={() => setVariant("classic")}
                  >
                    Classic
                  </Button>
                  <Button
                    size="sm"
                    variant={variant === "slim" ? "primary" : "secondary"}
                    onClick={() => setVariant("slim")}
                  >
                    Slim
                  </Button>
                </div>
                <p className="page-subtitle" style={{ marginTop: 6 }}>
                  Classic has 4-pixel arms (Steve); Slim has 3-pixel arms (Alex). Pick the one the
                  skin was drawn for — the wrong one leaves a visible gap or overlap.
                </p>

                <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={applying}
                    onClick={() => void onApply()}
                  >
                    {applying ? "Applying…" : "Apply skin"}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={applying}
                    onClick={() => setPending(null)}
                  >
                    Cancel
                  </Button>
                </div>
              </div>
            </div>
          )}
        </>
      )}

      {signingIn && (
        <MicrosoftSignInModal
          onClose={() => setSigningIn(false)}
          onSignedIn={() => {
            setSigningIn(false);
            onAccountChange();
            refreshStatus();
          }}
        />
      )}
    </section>
  );
}

/** Backend errors carry a `kind: message` prefix from the error enum
 *  ("invalid link: ...", "internal error: ..."); this section's messages
 *  are already complete sentences, so show them without it. */
function describeError(err: unknown): string {
  const raw = isAppError(err) ? err.message : String(err);
  return raw
    .replace(/^(invalid link|internal error|network failure|io error): /i, "")
    .replace(/^./, (c) => c.toUpperCase());
}

/** Builds a `data:` URL for PNG bytes, in chunks so large arrays don't
 *  overflow the argument limit of `String.fromCharCode`. */
function bytesToPngDataUrl(bytes: number[]): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.slice(i, i + chunk));
  }
  return `data:image/png;base64,${btoa(binary)}`;
}
