import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText } from "@tauri-apps/plugin-clipboard-manager";

import type { ContentType } from "@/lib/types";
import type { useLinkInstallFlow } from "@/hooks/useLinkInstallFlow";
import { Button } from "../shared/Primitives";

const GENERIC_COPY = {
  heading: "Paste a Modrinth or CurseForge link",
  helper: "Blocklight will figure out what it is and whether it fits this instance.",
  placeholder: "https://modrinth.com/mod/... or curseforge.com/minecraft/mc-mods/...",
};

const CONTENT_TYPE_COPY: Record<
  ContentType,
  { heading: string; helper: string; placeholder: string }
> = {
  mod: {
    heading: "Install Mod",
    helper: "Search for a mod or paste a Modrinth/CurseForge link.",
    placeholder: "https://modrinth.com/mod/...",
  },
  resource_pack: {
    heading: "Add Resource Pack",
    helper: "Search or paste a supported Modrinth/CurseForge link.",
    placeholder: "https://modrinth.com/resourcepack/...",
  },
  shader_pack: {
    heading: "Add Shader Pack",
    helper: "Search or paste a supported Modrinth/CurseForge link.",
    placeholder: "https://modrinth.com/shader/...",
  },
  modpack: {
    heading: "Install Modpack",
    helper: "Paste a Modrinth or CurseForge modpack link.",
    placeholder: "https://modrinth.com/modpack/...",
  },
};

export function PasteLinkPanel({
  instanceId,
  contentType,
  flow,
}: {
  instanceId: string | null;
  contentType?: ContentType;
  flow: ReturnType<typeof useLinkInstallFlow>;
}) {
  const [url, setUrl] = useState("");
  const [dragActive, setDragActive] = useState(false);
  const copy = contentType ? CONTENT_TYPE_COPY[contentType] : GENERIC_COPY;
  const busy = flow.stage.kind === "resolving" || flow.stage.kind === "installing";

  // Drag & drop: Tauri delivers OS drag-drop as a window-level event.
  // Browsers vary in whether a dragged address-bar link arrives as a
  // droppable file path or plain text, so this listens for both and
  // treats anything that parses as an http(s) URL the same as a paste.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== "drop") {
          setDragActive(event.payload.type === "over");
          return;
        }
        setDragActive(false);
        const dropped = event.payload.paths.find((p) => /^https?:\/\//i.test(p));
        if (dropped) {
          setUrl(dropped);
          void flow.resolve(dropped);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  async function handlePasteAndInstall() {
    try {
      const clipboardText = (await readText()) ?? "";
      if (!clipboardText.trim()) return;
      setUrl(clipboardText);
      await flow.resolve(clipboardText);
    } catch {
      // Clipboard access denied or empty -- silently no-op rather than
      // surfacing a scary error for what's an optional convenience action.
    }
  }

  return (
    <div
      className="panel"
      style={dragActive ? { borderColor: "var(--ember)", background: "var(--ember-wash)" } : undefined}
    >
      <div style={{ marginBottom: 4 }}>
        <h3 style={{ fontSize: 15 }}>{copy.heading}</h3>
        <p className="page-subtitle" style={{ marginTop: 2 }}>
          {dragActive ? "Drop the link here" : copy.helper}
        </p>
      </div>
      <div className="input-row">
        <input
          className="text-input"
          placeholder={copy.placeholder}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void flow.resolve(url);
          }}
        />
        <Button
          variant="primary"
          onClick={() => void flow.resolve(url)}
          disabled={!url.trim() || !instanceId || busy}
        >
          Install
        </Button>
        <Button variant="secondary" onClick={handlePasteAndInstall} disabled={!instanceId || busy}>
          Paste &amp; Install
        </Button>
      </div>
    </div>
  );
}
