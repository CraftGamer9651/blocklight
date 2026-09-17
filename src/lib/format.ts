import type { ContentType, Platform } from "./types";

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

export function formatRelativeTime(iso: string): string {
  const date = new Date(iso);
  const diffMs = Date.now() - date.getTime();
  const diffSec = Math.round(diffMs / 1000);

  if (diffSec < 60) return "just now";
  const diffMin = Math.round(diffSec / 60);
  if (diffMin < 60) return `${diffMin} minute${diffMin === 1 ? "" : "s"} ago`;
  const diffHour = Math.round(diffMin / 60);
  if (diffHour < 24) return `${diffHour} hour${diffHour === 1 ? "" : "s"} ago`;
  const diffDay = Math.round(diffHour / 24);
  if (diffDay === 1) return "yesterday";
  if (diffDay < 7) return `${diffDay} days ago`;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function contentTypeLabel(type: ContentType): string {
  switch (type) {
    case "mod":
      return "Mod";
    case "resource_pack":
      return "Resource Pack";
    case "shader_pack":
      return "Shader Pack";
    case "modpack":
      return "Modpack";
    default:
      return type;
  }
}

export function platformLabel(platform: Platform): string {
  return platform === "modrinth" ? "Modrinth" : "CurseForge";
}

export function loaderLabel(loader: string): string {
  const map: Record<string, string> = {
    fabric: "Fabric",
    forge: "Forge",
    neoforge: "NeoForge",
    quilt: "Quilt",
    minecraft: "Minecraft",
  };
  return map[loader.toLowerCase()] ?? loader;
}
