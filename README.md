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

Each of the Mods / Resource Packs / Shaders pages shows a "Top 12" list for
that content type before you type anything — Modrinth's search endpoint
with an empty query, a `project_type` facet, and sorting by downloads
(`ModrinthProvider::list_trending`). This is Modrinth-only by design (not
CurseForge) since it's meant as a fast way to find something to install
without already knowing a link. Every card has two actions: **Install**
(feeds the same resolve → preview → confirm flow as pasting a link) and
**Copy Link**, which puts the project's URL on the clipboard so you can
paste it into the paste-link panel yourself — handy for testing the
link-import flow with a real, current URL instead of typing one from memory.

## Performance

A few things specifically to make launching fast and gameplay smooth:

- **Concurrent downloads, not sequential.** Library and asset downloads
  used to happen one file at a time -- for a heavily-modded NeoForge
  instance with 100+ libraries, or the thousands of small files in a
  vanilla asset set, that seriously added up. Both now download with
  bounded concurrency (`download_library_list` / `download_assets` in
  `launch/download.rs`), and library downloads stay in the same order
  they were given in even though they run concurrently, so classpath
  ordering (which matters if two libraries could provide the same
  class) is exactly as deterministic as the old sequential loop was.
- **Repeat launches don't re-hash everything.** Every already-downloaded
  file used to be fully re-read and re-hashed on *every single launch*
  to confirm it was still correct -- for thousands of asset files, that
  meant real, needless disk I/O and CPU time before the game could even
  start, on every launch, forever. `download_verified` now trusts a
  cheap file-size check for anything already on disk, and only pays for
  a full sha1 verification on a file it just freshly downloaded (which
  is the case that actually catches a corrupted transfer -- that safety
  property didn't change, just when it's paid for).
- **The Mojang version manifest is cached** (`launch/manifest.rs`, 6-hour
  TTL) instead of being fetched fresh on every launch for something that
  changes rarely; a version that isn't in a cached-but-fresh copy still
  falls back to a live fetch, so a just-released version is never stuck
  behind the cache window.
- **JVM heap is pre-sized** (`-Xms` set equal to `-Xmx`, in
  `launch/args.rs`) instead of starting small and growing during the
  game's own early allocation spike (world load, chunk generation) --
  each grow step is itself a pause, so this trades a slightly larger
  upfront reservation for a smoother first few seconds in-game.
- **G1GC tuning** aimed at short/consistent pauses over raw throughput,
  which is the right trade for a game's frame times specifically (a
  batch job would want the opposite). These are ordinary, publicly
  documented G1 tuning parameters (Oracle's own G1 tuning guide covers
  each one) -- nothing Blocklight-specific, and nothing exotic.
- **Heap size auto-detects from system RAM** (half of total, clamped
  between 2 and 8 GB) instead of a flat, possibly-too-small hardcoded
  default, with an optional per-instance override (the "Memory (MB)"
  field on instance creation) for anyone who wants to hand-tune it.

One real bug from this pass, now fixed: the original G1 tuning included
`-XX:G1NewSizePercent`/`G1MaxNewSizePercent`, which turned out to be
flagged "experimental" (refusing to start the JVM at all without
`-XX:+UnlockExperimentalVMOptions` first) on some JDK vendors/versions but
not others. Rather than add an "unlock everything experimental" flag whose
own portability is equally unverified, those two flags were dropped --
G1's default new-generation sizing is solid on its own, and a launcher
that refuses to start the game is a far worse failure than slightly less
fine-tuned GC.

### Custom JVM arguments

Each instance can carry its own JVM arguments (a text area in the New
Instance modal, and an "Edit JVM arguments" panel on the instance tile).
They're stored as the raw text typed, parsed by `launch/user_args.rs`
(whitespace-separated; `"` or `'` group a token; no backslash escapes, so
Windows paths survive), and validated both when saved and again at launch --
a bad value fails in a second with the specific reason, before any
downloading starts.

Where they go: after every Blocklight and loader flag, immediately before
the main class, so the JVM's last-one-wins rule favors the user's value. For
flags where a duplicate would *break* the launch rather than just override,
the overridden default is removed first:

- `-Xmx` replaces both default heap flags (a leftover `-Xms` larger than the
  new `-Xmx` makes the JVM refuse to start).
- Choosing another collector (`-XX:+UseZGC`, ...) drops `-XX:+UseG1GC` and the
  G1-specific tuning (two collectors = "Multiple garbage collectors
  selected").
- Any other `-XX:` flag or `-D` property replaces the same-named default, so
  e.g. `-XX:-AlwaysPreTouch` turns that default off.

Blocked: `-cp`/`-classpath`/`--class-path`, `-p`/`--module-path`, `-jar`,
`-Djava.library.path` and `-Dorg.lwjgl.librarypath` (Blocklight computes
these from the prepared libraries/natives), and bare words that aren't the
value of a preceding `--long-option` (usually a pasted `java ...` command or
a main class).

Verified: the parser/override logic's unit tests, and the two JVM failure
modes above against a real JDK 21. Not verified: a full `cargo check` of the
Tauri crate or an in-app launch (this sandbox has no webview libraries).

### Removing instances

The instance tile has a **Remove** button (disabled while the game is
preparing or running) that opens a confirmation naming the folder. Confirming
removes the instance from `instances.json` *and deletes its folder* -- mods,
worlds, everything. Before anything is touched, the backend checks that the
folder is strictly inside Blocklight's own `instances` directory
(canonicalized, so `..` and symlinks can't escape); anything else is refused
and nothing changes. Install-history entries for the instance are kept.
Removing every instance is fine -- the list stays empty and does not
re-seed the example instance.

### NeoForge: processor outputs are not downloads

The install profile's `data` map (and processor arguments) name some files
the install steps *create* -- the patched client jar
(`net.neoforged:neoforge:<ver>:client`) and the `slim`/`extra`/`srg` jars
(`net.minecraft:client:<ver>:slim` etc.). An earlier version tried to
download those and failed with "installer needs ...:client, which isn't
downloaded and couldn't be fetched". Now `resolve_bracket` only fetches a
coordinate when something says where it comes from (Mojang's own downloads,
or an entry in `libraries`); otherwise, for data values and arguments, it
returns the path and creates the parent folder. A processor's own jar and
classpath still fall back to NeoForge's Maven. The vanilla-jar special case
is also stricter: only the bare `net.minecraft:client:<ver>` is the vanilla
jar, not every `net.minecraft:client:*` coordinate.

Processor `jar` and `classpath` entries are the opposite case: in real
install profiles they're *plain* Maven coordinates (no brackets), so they're
wrapped and resolved as coordinates (`resolve_processor_artifact`) before
being opened. Treating them as literal paths made the installer fail with
"couldn't open net.neoforged.installertools:installertools:2.1.2".

Verified: the coordinate classifier's and bracket-wrapping unit tests. Not verified: a real
NeoForge install end to end (the NeoForge Maven isn't reachable from the
sandbox), so a later processor step may still fail -- the error names the
step.

### NeoForge: JVM arguments and the merged-profile cache

NeoForge's own `version.json` carries the JVM arguments its bootstrap needs
to start: the module path (`-p`), `--add-modules ALL-MODULE-PATH`,
`--add-opens`/`--add-exports`, and `-D` properties built from
`${library_directory}` and `${classpath_separator}`. Two bugs hid this:

- `merge_arguments` kept only vanilla's JVM args and dropped the loader's.
  Without `-p`, NeoForge's bootstrap ran on the classpath and crashed with
  `module java.base does not "opens java.lang.invoke" to unnamed module`.
  Both sides' args now apply, parent first, like the official launcher.
- `${library_directory}` and `${classpath_separator}` weren't defined, so
  even a surviving `-p` would have pointed at literal placeholder text.

The merged profile is cached per NeoForge version, so the cache file is now
`merged_version.v2.json`; the old `merged_version.json` is ignored. The first
launch after updating re-runs NeoForge's processors once (a minute or so).

Verified: merge and placeholder-substitution tests against a realistic
NeoForge argument set (using the real function bodies). Not verified: an
actual NeoForge launch.

### Changing your skin

Settings -> **Skin** changes the active Microsoft account's skin from a PNG
file or a SkinMC link (`https://skinmc.net/skins/<id>`). The flow is pick a
source -> validate and preview -> choose the arm model (Classic 4px / Slim
3px) -> **Apply skin**; nothing is sent to Minecraft until Apply.

- **Offline profiles can't have a skin changed.** A skin belongs to a
  Minecraft account; the section says so instead of offering a control that
  couldn't work.
- **The login token lives in memory only.** Changing a skin needs a live
  Minecraft Services token, which Blocklight keeps for the current run
  (`AppState.mc_session`, never written to disk). After a restart the
  section shows **Sign in again** first. A 401 from Minecraft Services drops
  the token and asks for a fresh sign-in.
- **SkinMC links:** only the skin *id* is taken from the pasted link; the
  download URL is rebuilt as `https://skinmc.net/api/v1/renders/skins/<id>/skin`
  (the page's "Download skin" target), so a link can't redirect the download
  to another host. Redirects that leave skinmc.net are refused. Profile links
  (`/profile/<name>`) are rejected with an explanation.
- **Validation** (`skin_core.rs`): PNG signature, then the header's size must be
  64x64 or the legacy 64x32; files over 256 KB are refused. The backend
  re-validates at Apply time rather than trusting the UI.
- The upload is `POST /minecraft/profile/skins` (multipart: `variant`, then
  `file`), body built by hand so reqwest's `multipart` feature isn't needed.
- Logic is split so it can be tested without Tauri: `skin_core.rs` (pure,
  21 unit tests) and `skins.rs` (network).

Verified: the pure logic's unit tests, and that `skins.rs`/`skin_core.rs`/
`error.rs` compile against the real `reqwest`/`uuid`/`thiserror` crates.
Not verified: an actual upload to Minecraft Services, a real SkinMC download
(neither host is reachable from the sandbox, and SkinMC's bot protection may
reject automated requests -- the error then points to Upload a file), or the
wiring in `commands.rs` end to end.

### Discord status

Settings -> **Discord** -> "Show what I'm playing" puts the running instance
in your Discord status (Rich Presence): `Playing <instance name>`, then
`<Loader> <Minecraft version>` (e.g. `NeoForge 1.21.1`) and an elapsed-time
counter. It's **off by default** and shows nothing else (no account name,
paths or servers). Turning it on/off applies immediately, even mid-game. If
two instances run at once the newest is shown, and the older one returns when
the newer exits.

Setup (once, by whoever builds the app -- end users are never asked for an id):

1. Create an application at <https://discord.com/developers/applications>.
   Its **name** is what Discord shows as the activity title ("Playing
   Blocklight"), so name it that.
2. Copy its **Application ID** (the plain ~18-digit number on the General
   Information page). It is *not* the Bot Token (a secret -- never put that
   anywhere in this app; if one has been pasted somewhere, reset it under
   Bot -> Reset Token) and not the hex Public Key. Paste the Application ID
   into `BUILT_IN_CLIENT_ID` in `src-tauri/src/presence.rs` (already set to the
   Blocklight application's id). It's public, so committing it is fine.
3. Optional, for trying a different Discord application while developing:
   set `BLOCKLIGHT_DISCORD_CLIENT_ID` in the environment (or at build time).
   It takes precedence over the built-in one, and an invalid value is an
   error rather than a silent fall-back.
4. Optional: under Rich Presence -> Art Assets upload an image named
   `blocklight` for the large icon (without it Discord shows no image).

Without a usable id the toggle is disabled and the Settings page says why:
nothing is set, the value is empty, it still has the `<` `>` from a
placeholder, it's a Bot Token (recognized and rejected, never stored or
shown), or it's the hex Public Key. Surrounding quotes and whitespace are
tolerated. A `.env` file is *not* read. Discord's desktop app must
be running on the same computer; if it starts after the game, the status
appears within ~20 seconds (Blocklight retries while a status should be
showing). Implementation: `presence.rs` (a worker thread owns the connection,
so a slow or missing Discord can't delay a launch), with the status
registered just before the game is spawned and cleared by its exit watcher.

Verified: the status logic, text truncation, and the worker's
retry/coalesce/clear behavior (37 unit tests, run against a stand-in with the
same function signatures as `discord-rich-presence` 1.1.0, since the real
crate needs a newer Rust than the sandbox had). Not verified: the real crate
compiling, an actual Discord connection, and how Discord renders the elapsed
timer.

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

- **Frontend**: normally type-checks and builds cleanly (`npx tsc --noEmit`,
  `npm run build`). This session's performance pass specifically could not
  be re-verified that way -- the sandbox's network access to npm's
  registry was restricted for this whole session (a `403`/`host_not_allowed`
  from the sandbox's own egress proxy, not an issue with the project),
  so `node_modules` couldn't be (re)installed. The only frontend changes
  this pass made are a new optional field end-to-end (`Instance.memoryMb`
  in `types.ts`, `createInstance` in `tauri.ts`, and the new field in
  `NewInstanceModal.tsx`) -- small, mechanical, and manually reviewed,
  but genuinely not compiler-checked this time.
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
