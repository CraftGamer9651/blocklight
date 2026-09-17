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
}

export interface Project {
  platform: Platform;
  id: string;
  slug: string;
  name: string;
  author: string;
  summary: string;
  iconUrl: string | null;
  contentType: ContentType;
  projectUrl: string;
}

export interface FileHashes {
  sha1: string | null;
  sha512: string | null;
}

export interface ProjectFile {
  filename: string;
  downloadUrl: string;
  sizeBytes: number;
  hashes: FileHashes;
  primary: boolean;
}

export interface Dependency {
  projectId: string;
  versionId: string | null;
  projectName: string | null;
  kind: DependencyKind;
}

export interface ProjectVersion {
  id: string;
  projectId: string;
  versionNumber: string;
  gameVersions: string[];
  loaders: string[];
  dependencies: Dependency[];
  files: ProjectFile[];
  datePublished: string;
}

export interface VersionSummary {
  minecraftVersion: string;
  loader: string;
}

export interface CompatibilityResult {
  compatible: boolean;
  selectedVersion: ProjectVersion | null;
  reason: string;
  availableAlternatives: VersionSummary[];
}

export interface DependencyCheck {
  dependency: Dependency;
  resolvedProject: Project | null;
  resolvedVersion: ProjectVersion | null;
  alreadyInstalled: boolean;
  compatible: boolean;
}

export interface ResolvedInstall {
  project: Project;
  instanceId: string;
  compatibility: CompatibilityResult;
  alreadyInstalled: InstalledContent | null;
  dependencies: DependencyCheck[];
}

export interface InstallHistoryEntry {
  id: string;
  projectName: string;
  platform: Platform;
  contentType: ContentType;
  versionNumber: string;
  instanceId: string;
  installedAt: string;
}

export interface InstallSummary {
  installed: InstallHistoryEntry[];
  instance: Instance;
}

export interface OfflineProfile {
  id: string;
  displayName: string;
  createdAt: string;
}

export interface AccountState {
  activeKind: ActiveAccountKind;
  microsoftSignedIn: boolean;
  microsoftGamertag: string | null;
  offlineProfile: OfflineProfile | null;
}

export interface OfflineReadiness {
  ready: boolean;
  missing: string[];
}

export interface WorldSummary {
  name: string;
  folderName: string;
  savePath: string;
  lastPlayed: string | null;
}

export interface MsaPrompt {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
}

export interface PrepareProgress {
  instanceId: string;
  stage: "client" | "libraries" | "assets" | "natives" | "neoforge_libraries" | "neoforge_install";
  completed: number;
  total: number;
  detail: string | null;
}

export interface GameLogLine {
  instanceId: string;
  stream: "stdout" | "stderr";
  line: string;
}

export interface GameExited {
  instanceId: string;
  exitCode: number | null;
}

// Mirrors src-tauri/src/error.rs's SerializableAppError.
export type AppErrorKind =
  | "unsupported_link"
  | "invalid_link"
  | "project_unavailable"
  | "no_compatible_version"
  | "network_failure"
  | "offline"
  | "integrity_check_failed"
  | "path_traversal"
  | "missing_curseforge_key"
  | "minecraft_not_owned"
  | "database"
  | "io"
  | "internal";

export interface AppError {
  kind: AppErrorKind;
  message: string;
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value
  );
}
