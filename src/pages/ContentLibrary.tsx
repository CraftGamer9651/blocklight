import { useEffect, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import { listTrendingModrinth, searchContent } from "@/lib/tauri";
import { contentTypeLabel, platformLabel } from "@/lib/format";
import type { ContentType, Project } from "@/lib/types";
import { Badge, Button, Spinner } from "@/components/shared/Primitives";
import { PasteLinkPanel } from "@/components/link-import/PasteLinkPanel";
import { LinkFlowResult } from "@/components/link-import/LinkFlowResult";
import { useLinkInstallFlow } from "@/hooks/useLinkInstallFlow";

const PAGE_META: Record<ContentType, { title: string; subtitle: string }> = {
  mod: {
    title: "Mods",
    subtitle: "Browse the top mods on Modrinth, search for more, or paste a link to install directly.",
  },
  resource_pack: {
    title: "Resource Packs",
    subtitle: "Browse the top resource packs on Modrinth, search for more, or paste a link.",
  },
  shader_pack: {
    title: "Shaders",
    subtitle: "Browse the top shaders on Modrinth, search for more, or paste a link.",
  },
  modpack: {
    title: "Modpacks",
    subtitle: "Search or paste a supported Modrinth/CurseForge link.",
  },
};

export function ContentLibraryPage({
  contentType,
  instanceId,
}: {
  contentType: ContentType;
  instanceId: string | null;
}) {
  const meta = PAGE_META[contentType];
  const flow = useLinkInstallFlow(instanceId);

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Project[] | null>(null);
  const [searching, setSearching] = useState(false);

  const [trending, setTrending] = useState<Project[] | null>(null);
  const [trendingLoading, setTrendingLoading] = useState(true);

  // Reload the default "Top 10" browse list whenever the tab (content
  // type) changes. This is what shows before anyone types a search.
  useEffect(() => {
    let cancelled = false;
    setTrending(null);
    setTrendingLoading(true);
    listTrendingModrinth(contentType, 10)
      .then((hits) => {
        if (!cancelled) setTrending(hits);
      })
      .catch(() => {
        if (!cancelled) setTrending([]);
      })
      .finally(() => {
        if (!cancelled) setTrendingLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [contentType]);

  async function runSearch(q: string) {
    if (!q.trim()) {
      setResults(null);
      return;
    }
    setSearching(true);
    try {
      const hits = await searchContent(q);
      setResults(hits.filter((p) => p.contentType === contentType));
    } catch {
      setResults([]);
    } finally {
      setSearching(false);
    }
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1 className="page-title">{meta.title}</h1>
          <p className="page-subtitle">{meta.subtitle}</p>
        </div>
      </div>

      <div className="input-row">
        <input
          className="text-input"
          placeholder={`Search ${meta.title.toLowerCase()}…`}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            if (!e.target.value.trim()) setResults(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") void runSearch(query);
          }}
        />
      </div>

      {searching && <Spinner label="Searching…" />}

      {results !== null ? (
        <>
          {results.length > 0 ? (
            <div className="tile-row">
              {results.map((project) => (
                <ProjectCard
                  key={`${project.platform}:${project.id}`}
                  project={project}
                  onInstall={() => void flow.resolve(project.projectUrl)}
                />
              ))}
            </div>
          ) : (
            !searching && <p className="page-subtitle">No results for "{query}".</p>
          )}
        </>
      ) : (
        <div>
          <div className="section-title" style={{ marginBottom: 8 }}>
            Top {meta.title} on Modrinth
          </div>
          {trendingLoading ? (
            <Spinner label="Loading…" />
          ) : trending && trending.length > 0 ? (
            <div className="tile-row">
              {trending.map((project) => (
                <ProjectCard
                  key={`${project.platform}:${project.id}`}
                  project={project}
                  onInstall={() => void flow.resolve(project.projectUrl)}
                />
              ))}
            </div>
          ) : (
            <p className="page-subtitle">
              Couldn't load trending {meta.title.toLowerCase()} right now — you're probably offline.
            </p>
          )}
        </div>
      )}

      <PasteLinkPanel instanceId={instanceId} contentType={contentType} flow={flow} />
      <LinkFlowResult flow={flow} />
    </div>
  );
}

function ProjectCard({ project, onInstall }: { project: Project; onInstall: () => void }) {
  const [copied, setCopied] = useState(false);

  async function handleCopyLink() {
    try {
      await writeText(project.projectUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard access denied -- not worth an error state for a
      // convenience action.
    }
  }

  return (
    <div className="tile">
      <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
        {project.iconUrl ? (
          <img
            src={project.iconUrl}
            alt=""
            width={36}
            height={36}
            style={{ borderRadius: 7, flexShrink: 0, background: "var(--surface-raised)" }}
          />
        ) : (
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 7,
              background: "var(--surface-raised)",
              flexShrink: 0,
            }}
          />
        )}
        <div style={{ minWidth: 0 }}>
          <div style={{ fontSize: 13.5, fontWeight: 600 }}>{project.name}</div>
          <div className="list-row-meta" style={{ marginTop: 2 }}>
            by {project.author}
          </div>
        </div>
      </div>
      <p
        className="state-body"
        style={{
          fontSize: 12.5,
          marginTop: 8,
          overflow: "hidden",
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
        }}
      >
        {project.summary}
      </p>
      <div className="meta-row" style={{ marginTop: 8 }}>
        <Badge tone="neutral">{platformLabel(project.platform)}</Badge>
        <Badge tone="neutral">{contentTypeLabel(project.contentType)}</Badge>
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
        <Button size="sm" variant="primary" onClick={onInstall}>
          Install
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void handleCopyLink()}>
          {copied ? "Copied!" : "Copy Link"}
        </Button>
      </div>
    </div>
  );
}
