import type { AppError } from "@/lib/types";
import { Button, StateBlock } from "./Primitives";

/// Renders the spec's "Error Handling" states (Unsupported link, Invalid
/// link, Project unavailable, No compatible version, Download failed,
/// You're offline) from a raw AppError, with the interface's plain,
/// non-apologetic voice.
export function ErrorState({ error, onRetry }: { error: AppError; onRetry?: () => void }) {
  const content = describe(error);
  return (
    <StateBlock
      error
      title={content.title}
      body={content.body}
      action={
        content.showRetry && onRetry ? (
          <Button size="sm" onClick={onRetry}>
            Retry
          </Button>
        ) : undefined
      }
    />
  );
}

function describe(error: AppError): { title: string; body: string; showRetry: boolean } {
  switch (error.kind) {
    case "unsupported_link":
      return {
        title: "Unsupported link",
        body: "Blocklight currently supports Modrinth and CurseForge links.",
        showRetry: false,
      };
    case "invalid_link":
      return {
        title: "Invalid link",
        body: "The pasted URL doesn't appear to be a valid Modrinth or CurseForge project/file link.",
        showRetry: false,
      };
    case "project_unavailable":
      return {
        title: "Project unavailable",
        body: "Blocklight couldn't retrieve this project. It may have been removed, made private, or temporarily unavailable.",
        showRetry: true,
      };
    case "no_compatible_version":
      return {
        title: "No compatible version",
        body: "Blocklight couldn't find a file matching this instance's Minecraft version and mod loader.",
        showRetry: false,
      };
    case "network_failure":
      return {
        title: "Download failed",
        body: "Blocklight couldn't reach the content provider.",
        showRetry: true,
      };
    case "offline":
      return {
        title: "You're offline",
        body: "This feature requires an internet connection. Your installed content and offline instances are still available.",
        showRetry: true,
      };
    case "integrity_check_failed":
      return {
        title: "File didn't verify",
        body: "The downloaded file didn't match what the provider reported. Nothing was installed.",
        showRetry: true,
      };
    case "missing_curseforge_key":
      return {
        title: "CurseForge isn't connected",
        body: "Add a CurseForge API key in Settings to install CurseForge links and search results.",
        showRetry: false,
      };
    case "minecraft_not_owned":
      return {
        title: "This account doesn't own Minecraft",
        body: "Blocklight couldn't verify a Minecraft entitlement on this Microsoft account, so it can't be signed in.",
        showRetry: false,
      };
    default:
      return {
        title: "Something went wrong",
        body: error.message,
        showRetry: true,
      };
  }
}
