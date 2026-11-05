import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { onMsaPrompt, signInWithMicrosoft } from "@/lib/tauri";
import type { AppError, MsaPrompt } from "@/lib/types";
import { isAppError } from "@/lib/types";
import { Button, Spinner } from "../shared/Primitives";
import { ErrorState } from "../shared/ErrorState";

export function MicrosoftSignInModal({
  onClose,
  onSignedIn,
}: {
  onClose: () => void;
  onSignedIn: () => void;
}) {
  const [prompt, setPrompt] = useState<MsaPrompt | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    const unlistenPromise = onMsaPrompt(setPrompt);

    signInWithMicrosoft()
      .then(() => onSignedIn())
      .catch((err) => setError(isAppError(err) ? err : { kind: "internal", message: String(err) }));

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
        style={{ width: 360, boxShadow: "var(--shadow-panel)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 style={{ fontSize: 16 }}>Sign in with Microsoft</h3>

        {error ? (
          <div style={{ marginTop: 12 }}>
            <ErrorState error={error} />
            <div style={{ marginTop: 10 }}>
              <Button size="sm" onClick={onClose}>
                Close
              </Button>
            </div>
          </div>
        ) : !prompt ? (
          <div style={{ marginTop: 14 }}>
            <Spinner label="Requesting a sign-in code…" />
          </div>
        ) : (
          <div style={{ marginTop: 12 }}>
            <p className="state-body">Go to the page below and enter this code:</p>
            <div
              className="tabular"
              style={{
                marginTop: 10,
                textAlign: "center",
                fontSize: 24,
                fontWeight: 700,
                letterSpacing: "0.08em",
                padding: "12px 0",
                background: "var(--surface-raised)",
                borderRadius: 8,
              }}
            >
              {prompt.userCode}
            </div>
            <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
              <Button variant="primary" size="sm" onClick={() => void openUrl(prompt.verificationUri)}>
                Open {prompt.verificationUri.replace(/^https?:\/\//, "")}
              </Button>
              <Button variant="ghost" size="sm" onClick={onClose}>
                Cancel
              </Button>
            </div>
            <div style={{ marginTop: 14 }}>
              <Spinner label="Waiting for you to finish in the browser…" />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
