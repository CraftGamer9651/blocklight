import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountState,
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
}): Promise<Instance> {
  return invoke("create_instance", {
    name: input.name,
    minecraftVersion: input.minecraftVersion,
    loader: input.loader,
    loaderVersion: input.loaderVersion ?? null,
    javaMajorVersion: input.javaMajorVersion ?? null,
  });
}

export function setInstanceJvmArguments(
  instanceId: string,
  args: string[],
): Promise<Instance> {
  return invoke("set_instance_jvm_arguments", { instanceId, args });
}

export function validateLink(
  url: string
): Promise<{ platform: Platform; contentType: string }> {
  return invoke("validate_link", { url });
}
