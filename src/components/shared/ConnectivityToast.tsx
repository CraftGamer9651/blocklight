import { useEffect, useRef, useState } from "react";
import { onConnectivityChanged } from "@/lib/tauri";
import { Button } from "../shared/Primitives";

export function ConnectivityToast() {
  const [toast, setToast] = useState<"lost" | "restored" | null>(null);
  const wasOnline = useRef(true);
  const timeoutRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    const unlistenPromise = onConnectivityChanged((state) => {
      if (state === "offline" && wasOnline.current) {
        wasOnline.current = false;
        setToast("lost");
        window.clearTimeout(timeoutRef.current);
      } else if (state === "online" && !wasOnline.current) {
        wasOnline.current = true;
        setToast("restored");
        window.clearTimeout(timeoutRef.current);
        timeoutRef.current = window.setTimeout(() => setToast(null), 6000);
      }
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
      window.clearTimeout(timeoutRef.current);
    };
  }, []);

  if (!toast) return null;

  return (
    <div
      style={{
        position: "fixed",
        bottom: 20,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 60,
      }}
    >
      <div
        className="panel"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "10px 14px",
          boxShadow: "var(--shadow-panel)",
          borderColor: toast === "lost" ? "rgba(143,209,79,0.3)" : "rgba(79,156,240,0.3)",
        }}
      >
        <span
          className="status-dot"
          style={{ color: toast === "lost" ? "var(--lime)" : "var(--signal-blue)" }}
        />
        <div>
          <div style={{ fontSize: 13, fontWeight: 600 }}>
            {toast === "lost" ? "Connection lost" : "You're back online"}
          </div>
          <div style={{ fontSize: 12, color: "var(--ink-dim)" }}>
            {toast === "lost"
              ? "Blocklight switched to Offline Mode. Your installed instances remain available."
              : "Online services are available again."}
          </div>
        </div>
        <Button size="sm" variant="ghost" onClick={() => setToast(null)}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}
