# Blocklight

A desktop Minecraft launcher (Tauri + React/TypeScript) implementing two features
plus actual game launching:

1. **Modrinth & CurseForge Link Import** — paste, drag-drop, or clipboard-paste a
   project link and Blocklight resolves the platform/content type/version,
   checks it against your instance, resolves dependencies, and installs it.
2. **Offline / Local Profile Mode** — a real offline mode: automatic
   connectivity detection, an offline-only local profile, and a launcher UI
   that keeps working (managing mods, resource packs, shaders, worlds) without
   a network connection or a signed-in Microsoft account.
3. **Launching** — downloads the Minecraft client, libraries, natives, and
   assets (or reuses what's already there); resolves a Fabric/Quilt loader
   profile, or runs NeoForge's real installer/processor chain, when the
   instance has one; finds a compatible local Java install; builds the
   JVM/game arguments; and spawns and streams the game's log back into the
   UI. See [Launching](#launching) below for what this does and doesn't cover.

This repo is a full working scaffold, not a mockup — the URL parsing,
compatibility matching, dependency resolution, download/verification
pipeline, SQLite-backed history/profiles, the Modrinth/CurseForge API
clients, and the launch pipeline are real implementations. A few things are
intentionally out of scope; see [What's not included](#whats-not-included)
below.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ and npm
- [Rust](https://www.rust-lang.org/tools/install) (stable, 1.77+) and Cargo
- The [Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS
  (on Linux: `webkit2gtk-4.1`, `gtk3`, `libsoup-3.0`, and friends — see the link above for
  the exact package list per distro)

## Getting started

```bash
npm install
npm run tauri dev
```

This starts the Vite dev server and launches the Tauri window. On first run
you'll see the **First-Run Account Choice** screen (Microsoft Account vs.
Offline / Local Mode) described in the offline-mode spec.

To build a distributable:

```bash
npm run tauri build
```

## Configuration

Blocklight reads two optional environment variables at startup (set them
before running `npm run tauri dev` / the built binary):

| Variable | Purpose |
| --- | --- |
| `BLOCKLIGHT_CURSEFORGE_API_KEY` | A CurseForge API key from [console.curseforge.com](https://console.curseforge.com/), so CurseForge links/search work. Can also be set later from **Settings → Content Providers** without restarting. |
| `BLOCKLIGHT_MSA_CLIENT_ID` | An Azure AD application id for Microsoft sign-in. See below — without this, Microsoft sign-in returns a clear "not configured" error rather than pretending to work. |

Modrinth requires no key or configuration.

### Microsoft Sign-In setup

Blocklight implements the real Microsoft device-code flow, followed by the
standard Xbox Live → XSTS → Minecraft Services token exchange and an actual
Minecraft-ownership entitlement check (`src-tauri/src/msa.rs`). It never
fakes, bypasses, or skips that verification — an account without a
Minecraft entitlement is refused.

What it deliberately does **not** do is ship a working client id out of the
box. Microsoft's terms don't allow distributing a shared client id for
unofficial launchers, so — the same as Prism Launcher, ATLauncher, and other
open-source launchers — each build needs its own:

1. Register an app at the [Azure Portal](https://portal.azure.com/) →
   *App registrations* → *New registration*.
   - Supported account types: **Personal Microsoft accounts only**.
   - No redirect URI is needed for the device-code flow used here.
2. Under *Authentication*, enable **"Allow public client flows"**.
3. Copy the **Application (client) ID** and set it as `BLOCKLIGHT_MSA_CLIENT_ID`.

Without this variable set, the sign-in button surfaces a plain error
explaining what's missing instead of silently failing or faking success.

Offline Mode itself never touches Microsoft/Xbox/Minecraft authentication —
the Offline Profile is a local, clearly-labeled display name Blocklight uses
for its own UI, not a substitute for a licensed account.

## Project layout

```
src-tauri/src/
  models.rs           shared data types (Project, ProjectVersion, Instance, ...)
  error.rs             AppError -> the exact error states from the spec
  url_resolver.rs       parses/validates pasted Modrinth & CurseForge URLs
  providers/            ContentProvider trait + Modrinth & CurseForge clients
  compatibility.rs      matches instance (MC version + loader) against versions
  dependencies.rs        resolves a version's required/optional dependencies
  downloader.rs          HTTPS-only, host-allowlisted, hash-verified downloads;
                          safe zip extraction with path-traversal checks
  instances.rs            local instance store, offline readiness, worlds, backups
  network_monitor.rs       low-frequency connectivity polling -> "offline" state
  profiles.rs               offline profiles + active-account switching
  msa.rs                     Microsoft device-code / Xbox / Minecraft Services auth
  history.rs                  SQLite-backed "Recently Installed" list
  launch/                       version manifest, Fabric/Quilt profile merging,
                                 NeoForge's installer/processor pipeline
                                 (launch/neoforge.rs), library/asset downloads,
                                 Java detection, argument building, and process
                                 spawning -- see "Launching" below
  commands.rs                    Tauri command handlers wiring all of the above together

src/
  lib/tauri.ts           typed wrappers around every Tauri command
  lib/types.ts             TypeScript mirrors of the Rust DTOs
  hooks/useLinkInstallFlow  the resolve -> preview -> install state machine,
                             shared by the paste panel, drag-drop, clipboard
                             button, and search results
  hooks/useLaunch            preparing/running/exited state + live log lines
                              for one instance's launch
  components/link-import/   paste panel, link preview card, dependency panel
  components/offline/       offline status card, first-run choice, MSA sign-in modal
  components/instances/     new-instance form, launch progress/log/stop panel
  pages/                     Home, Mods/Resource Packs/Shaders, Settings
```

## Browsing Modrinth

Each of the Mods / Resource Packs / Shaders pages shows a "Top 10" list for
that content type before you type anything — Modrinth's search endpoint
with an empty query, a `project_type` facet, and sorting by downloads
(`ModrinthProvider::list_trending`). This is Modrinth-only by design (not
CurseForge) since it's meant as a fast way to find something to install
without already knowing a link. Every card has two actions: **Install**
(feeds the same resolve → preview → confirm flow as pasting a link) and
**Copy Link**, which puts the project's URL on the clipboard so you can
paste it into the paste-link panel yourself — handy for testing the
link-import flow with a real, current URL instead of typing one from memory.

## Launching

`launch_instance` does the real thing: it resolves the version JSON (vanilla
directly, a Fabric/Quilt profile merged with vanilla via their meta APIs, or
NeoForge's real installer/processor pipeline -- see below), downloads
whatever's missing (client jar, libraries, natives, asset index and objects
— already-present files are skipped after a hash check, which is what lets
a fully-prepared instance launch with zero network access), finds a Java
install that satisfies the instance's required version, builds the full
JVM/game argument list, and spawns the process. Stdout/stderr stream back as
`game-log` events; `game-exited` fires on termination; `stop_instance` kills
it by PID.

### NeoForge

`launch/neoforge.rs` downloads NeoForge's real installer jar, extracts its
`install_profile.json` and `version.json`, downloads the libraries it lists,
and **actually runs its processor chain** (each processor is a real Java
program NeoForge ships, invoked the same way its own installer would invoke
it) to produce the patched client -- there's no shortcut version of this;
that chain is what turns a vanilla client into a moddable NeoForge one. The
result is cached per Minecraft+NeoForge version pair, so this only runs once.

This is scoped to NeoForge specifically, not Forge in general, and that's a
deliberate simplification rather than an oversight: NeoForge only exists
from Minecraft 1.20.1 onward and has only ever used this modern,
processor-based installer format. Older Forge (pre-1.13 especially) also has
to support a much older single-step binary-patching format that this module
doesn't attempt, which is why Forge itself still returns a clear "not
implemented" error while NeoForge instances can launch.

If a processor references a library that the bulk download pass didn't
already fetch (an OS `rules` gate excluding it, or a coordinate that simply
isn't in `install_profile.json`'s own list), Blocklight fetches it on demand
at the point it's needed rather than failing outright, using that library's
own declared URL when known and NeoForge's Maven as a fallback. If that
*also* fails, the error names the exact coordinate and URL involved.

A few coordinates the installer's data map references aren't real Maven
artifacts at all -- `net.minecraft:client`/`:server`, with or without a
`:mappings` classifier, mean "the vanilla client/server jar" and "Mojang's
official obfuscation mappings" respectively. Mojang only publishes those via
the vanilla version JSON's own `downloads` block, never through a Maven
repository, so `special_case_url` in `neoforge.rs` resolves those four
specifically from `downloads.client` / `downloads.server` /
`downloads.client_mappings` / `downloads.server_mappings` instead of trying
(and failing) to fetch them as if they were normal library coordinates.

**This is, by a wide margin, the least-tested code in this project.** It was
written from documented knowledge of NeoForge's installer format, not
verified against a live install, since this sandbox can't run a JVM or a
real installer. If it breaks, the error names exactly which processor step
failed and includes that processor's own output -- please paste that back
rather than just "it didn't work," since it pinpoints the exact step to fix.

One known rough edge: if you leave an instance's NeoForge version
unspecified (letting Blocklight auto-resolve "latest" each time), the
natives-cache key for the *final* merged profile doesn't yet track exactly
which resolved version was used, so if NeoForge ships a new build between
two launches, cached natives from the older build could be reused instead
of being refreshed. Pinning an explicit NeoForge version on the instance
(the "Loader version" field when creating it) sidesteps this entirely and
is the more reliable way to run the same setup repeatedly anyway.

### What launching doesn't do

- **Plain Forge instances can't launch** (NeoForge can -- see above).
- **No JRE auto-download.** Mojang publishes a Java runtime per platform the
  same way official/third-party launchers fetch one automatically; Blocklight
  instead detects an existing local Java install (`JAVA_HOME`, then `PATH`)
  and gives a clear error naming the required version if nothing suitable is
  found, rather than silently failing to launch.
- **Signed-in Microsoft accounts can't launch yet** — only offline profiles
  can. A real online launch needs the live Minecraft Services access token
  from the sign-in exchange (`msa.rs`), which Blocklight deliberately never
  persists anywhere (see `profiles.rs`); wiring a launch through to use it
  before it's discarded, plus handling refresh, is a real but separate
  follow-up. Offline-profile launches are fully implemented, including
  vanilla's own offline-mode UUID formula for player identity.

## What's not included

Both product specs assume Blocklight already has a broader instance-manager.
Launching (above) covers the biggest piece of that gap; what's still not
here:

- `InstanceStore` starts with one seeded example instance and a "New
  Instance" form for adding more, rather than a full Minecraft-installation
  browser/importer.
- Modpack *links* resolve, show compatibility, and can be searched like
  everything else, but a full modpack install (parsing a `.mrpack` index,
  downloading every listed file, extracting `overrides/`) isn't wired up —
  `downloader::safe_extract_zip` is written and ready for it, just not yet
  called from a command.
- Everything downstream of "here's an instance with a Minecraft version and
  loader" — link resolution, compatibility matching, dependency resolution,
  secure downloads, offline detection, offline readiness checks, worlds,
  backups, history, account/profile switching, and launching itself — is
  fully implemented against that instance.

## Verification notes

- **Frontend**: type-checks cleanly (`npx tsc --noEmit`) and builds cleanly
  (`npm run build`).
- **Backend, instance creation and the original link-import/offline-mode
  code**: compiles cleanly with `cargo check` (confirmed against a real
  Tauri 2.11.5 toolchain). Two real bugs surfaced that way and are fixed: a
  missing `use tauri::Manager;` for `app.manage()`, and an
  `RwLockReadGuard` held across an `.await` in three async commands (fixed
  by moving the CurseForge API key's mutability into its own interior lock
  instead of wrapping the whole provider registry).
- **The launch pipeline (`src-tauri/src/launch/`) is new and has not been
  compiled** — this sandbox's own Rust install is too old for Tauri v2's
  dependency tree (a transitive dependency needs `edition2024`) and rustup
  wasn't reachable to get a newer one. It was written and reviewed by hand
  as carefully as the rest of this project, and that review did catch and
  fix a few real issues along the way (a `&str`/`&String` type mismatch
  inside an array literal passed to `Command::args`, a missing tokio `sync`
  feature flag needed for `Semaphore`, an ownership-check gap in the
  Microsoft sign-in flow) — but unlike everything else in this repo, it
  hasn't had a real compiler's confirmation yet. Run `cargo check` after
  cloning; if anything doesn't compile, treat it the same way the two bugs
  above were handled — a small, local fix rather than a structural problem.
- **`launch/neoforge.rs` is the newest, least-verified piece of that
  already-unverified pipeline.** Beyond not having compiled, its actual
  installer/processor logic has never run against a live NeoForge install
  (this sandbox can't run a JVM). It's a good-faith implementation of a
  publicly documented format, not a tested one -- expect it to need at
  least one round of fixes against real output, and see the "Launching"
  section above for the one architectural rough edge already known about
  (natives caching when a NeoForge version is left unpinned).
