import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountState,
  SkinStatus,
  DiscordPresenceStatus,
  ConnectivityState,
  ContentType,
  GameExited,
  GameLogLine,
  Instance,
  InstallHistoryEntry,
  InstallSummary,
  Loader,
  MsaPrompt,
  OfflineProfile,
  OfflineReadiness,
  Platform,
  PrepareProgress,
  Project,
  ResolvedInstall,
  WorldSummary,
} from "./types";

// Every function here corresponds 1:1 to a `#[tauri::command]` in
// src-tauri/src/commands.rs. Components should never call `invoke`
// directly -- going through this module keeps the IPC surface typed and
// in one place if a command's signature ever changes.

export function listInstances(): Promise<Instance[]> {
  return invoke("list_instances");
}

export function getInstance(instanceId: string): Promise<Instance> {
  return invoke("get_instance", { instanceId });
}

export function createInstance(input: {
  name: string;
  minecraftVersion: string;
  loader: Loader;
  loaderVersion?: string | null;
  javaMajorVersion?: number | null;
  memoryMb?: number | null;
  jvmArgs?: string | null;
}): Promise<Instance> {
  return invoke("create_instance", {
    name: input.name,
    minecraftVersion: input.minecraftVersion,
    loader: input.loader,
    loaderVersion: input.loaderVersion ?? null,
    javaMajorVersion: input.javaMajorVersion ?? null,
    memoryMb: input.memoryMb ?? null,
    jvmArgs: input.jvmArgs ?? null,
  });
}

/** Removes the instance and permanently deletes its folder. */
export function removeInstance(instanceId: string): Promise<void> {
  return invoke("remove_instance", { instanceId });
}

/** Opens the instance's folder in the system file manager. */
export function openInstanceFolder(instanceId: string): Promise<void> {
  return invoke("open_instance_folder", { instanceId });
}

/** Saves custom JVM arguments for an instance; blank/null clears them. */
export function setInstanceJvmArgs(
  instanceId: string,
  jvmArgs: string | null
): Promise<Instance> {
  return invoke("set_instance_jvm_args", { instanceId, jvmArgs });
}

export function validateLink(
  url: string
): Promise<{ platform: Platform; contentType: string }> {
  return invoke("validate_link", { url });
}

export function previewLinkInstall(
  url: string,
  instanceId: string
): Promise<ResolvedInstall> {
  return invoke("preview_link_install", { url, instanceId });
}

export function confirmLinkInstall(
  url: string,
  instanceId: string
): Promise<InstallSummary> {
  return invoke("confirm_link_install", { url, instanceId });
}

export function searchContent(
  query: string,
  platform?: Platform
): Promise<Project[]> {
  return invoke("search_content", { query, platform: platform ?? null });
}

export function listTrendingModrinth(
  contentType: ContentType,
  limit = 12
): Promise<Project[]> {
  return invoke("list_trending_modrinth", { contentType, limit });
}

export function listInstallHistory(limit = 50): Promise<InstallHistoryEntry[]> {
  return invoke("list_install_history", { limit });
}

export function clearInstallHistory(): Promise<void> {
  return invoke("clear_install_history");
}

export function getConnectivity(): Promise<ConnectivityState> {
  return invoke("get_connectivity");
}

export function checkOfflineReadiness(instanceId: string): Promise<OfflineReadiness> {
  return invoke("check_offline_readiness", { instanceId });
}

export function listWorlds(instanceId: string): Promise<WorldSummary[]> {
  return invoke("list_worlds", { instanceId });
}

export function backupWorld(instanceId: string, folderName: string): Promise<string> {
  return invoke("backup_world", { instanceId, folderName });
}

export function launchInstance(instanceId: string): Promise<void> {
  return invoke("launch_instance", { instanceId });
}

export function stopInstance(instanceId: string): Promise<void> {
  return invoke("stop_instance", { instanceId });
}

export function isInstanceRunning(instanceId: string): Promise<boolean> {
  return invoke("is_instance_running", { instanceId });
}

export function getAccountState(): Promise<AccountState> {
  return invoke("get_account_state");
}

export function createOfflineProfile(displayName: string): Promise<OfflineProfile> {
  return invoke("create_offline_profile", { displayName });
}

export function listOfflineProfiles(): Promise<OfflineProfile[]> {
  return invoke("list_offline_profiles");
}

export function activateOfflineProfile(profileId: string): Promise<AccountState> {
  return invoke("activate_offline_profile", { profileId });
}

export function deleteOfflineProfile(profileId: string): Promise<AccountState> {
  return invoke("delete_offline_profile", { profileId });
}

export function switchToMicrosoft(): Promise<AccountState> {
  return invoke("switch_to_microsoft");
}

export function getDiscordPresenceStatus(): Promise<DiscordPresenceStatus> {
  return invoke("get_discord_presence_status");
}

export function setDiscordPresenceEnabled(enabled: boolean): Promise<DiscordPresenceStatus> {
  return invoke("set_discord_presence_enabled", { enabled });
}

export function getSkinStatus(): Promise<SkinStatus> {
  return invoke("get_skin_status");
}

/** Downloads the skin behind a SkinMC link (for preview); returns PNG bytes. */
export function fetchSkinFromLink(link: string): Promise<number[]> {
  return invoke("fetch_skin_from_link", { link });
}

/** Validates a skin file (PNG, 64x64 or 64x32) without applying it. */
export function inspectSkinPng(bytes: number[]): Promise<void> {
  return invoke("inspect_skin_png", { bytes });
}

/** Uploads the skin to the active Microsoft account. */
export function applySkin(bytes: number[], variant: "classic" | "slim"): Promise<void> {
  return invoke("apply_skin", { bytes, variant });
}

export function signInWithMicrosoft(): Promise<AccountState> {
  return invoke("sign_in_with_microsoft");
}

export function setCurseForgeApiKey(apiKey: string | null): Promise<void> {
  return invoke("set_curseforge_api_key", { apiKey });
}

export function hasCurseForgeApiKey(): Promise<boolean> {
  return invoke("has_curseforge_api_key");
}

// --- Events -----------------------------------------------------------

export function onConnectivityChanged(
  cb: (state: ConnectivityState) => void
): Promise<UnlistenFn> {
  return listen<ConnectivityState>("connectivity-changed", (event) => cb(event.payload));
}

export function onMsaPrompt(cb: (prompt: MsaPrompt) => void): Promise<UnlistenFn> {
  return listen<MsaPrompt>("msa-prompt", (event) => cb(event.payload));
}

export function onPrepareProgress(cb: (progress: PrepareProgress) => void): Promise<UnlistenFn> {
  return listen<PrepareProgress>("prepare-progress", (event) => cb(event.payload));
}

export function onGameLog(cb: (line: GameLogLine) => void): Promise<UnlistenFn> {
  return listen<GameLogLine>("game-log", (event) => cb(event.payload));
}

export function onGameExited(cb: (exit: GameExited) => void): Promise<UnlistenFn> {
  return listen<GameExited>("game-exited", (event) => cb(event.payload));
}
