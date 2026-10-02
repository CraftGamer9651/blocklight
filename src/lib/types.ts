// Mirrors src-tauri/src/models.rs. Field names use camelCase because the
// Rust structs are annotated with #[serde(rename_all = "camelCase")].

export type Platform = "modrinth" | "curseforge";

export type ContentType = "mod" | "resource_pack" | "shader_pack" | "modpack";

export type Loader = "fabric" | "forge" | "neoforge" | "quilt" | "minecraft";

export type DependencyKind = "required" | "optional" | "incompatible" | "embedded";

export type ActiveAccountKind = "microsoft" | "offline";

export type ConnectivityState = "online" | "offline";

export interface InstalledContent {
  platform: Platform;
  projectId: string;
  projectSlug: string;
  contentType: ContentType;
  versionId: string;
  versionNumber: string;
  fileName: string;
}

export interface Instance {
  id: string;
  name: string;
  minecraftVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  javaMajorVersion: number | null;
  instanceDir: string;
  installed: InstalledContent[];
  customJvmArguments: string[];
}
