//! "Playing <instance>" in the user's Discord status (Rich Presence).
//!
//! Discord's desktop app exposes a local socket; this talks to it through
//! the `discord-rich-presence` crate. It never touches the network
//! itself, and it does nothing unless (a) this build has a Discord
//! application id and (b) the user has switched the feature on in
//! Settings. What gets shown is deliberately minimal: the instance name,
//! its loader and Minecraft version, and an elapsed-time counter. No
//! account name, no file paths, no server addresses.
//!
//! Structure, so each part is testable on its own:
//! - `PresenceState`: pure bookkeeping of which games are running and
//!   what (if anything) the status should currently say.
//! - `Backend`: the thing that talks to Discord, behind a trait so tests
//!   can use a fake. `DiscordBackend` is the real one.
//! - A worker thread owning the backend. Callers just send it "this is
//!   what the status should be now" and return immediately, so a slow or
//!   missing Discord can never stall a game launch.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

/// Discord rejects `details`/`state` strings longer than this.
const MAX_FIELD_CHARS: usize = 128;

/// If Discord wasn't reachable (not running yet, restarting), try again
/// this often for as long as the status should be showing.
const RETRY_INTERVAL: Duration = Duration::from_secs(20);

/// Name of the large image asset the Discord application is expected to
/// have uploaded (Developer Portal -> Rich Presence -> Art Assets). If it
/// isn't there Discord simply shows no image.
const LARGE_IMAGE_KEY: &str = "blocklight";

const CLIENT_ID_VAR: &str = "BLOCKLIGHT_DISCORD_CLIENT_ID";

/// Checks a candidate Discord application id. A Discord application id is
/// a numeric "snowflake" (17-20 digits today). On success returns the id
/// cleaned up (whitespace and one pair of surrounding quotes removed, as
/// left behind by `.env`-style quoting); on failure returns a sentence
/// explaining what's wrong, written to be shown to the user. `subject`
/// names the value in that sentence ("BLOCKLIGHT_DISCORD_CLIENT_ID" or
/// "That value"). The value itself is never echoed back, only described.
pub fn check_client_id(raw: &str, subject: &str) -> Result<String, String> {
    let mut id = raw.trim();
    for quote in ['"', '\''] {
        if id.len() >= 2 && id.starts_with(quote) && id.ends_with(quote) {
            id = id[1..id.len() - 1].trim();
            break;
        }
    }

    if id.is_empty() {
        return Err(format!("{subject} is empty."));
    }
    if id.contains('<') || id.contains('>') {
        return Err(format!(
            "{subject} still has the angle brackets from the placeholder — use just the number, \
             with no < or >."
        ));
    }
    if id.chars().all(|c| c.is_ascii_digit()) {
        return if (15..=25).contains(&id.len()) {
            Ok(id.to_string())
        } else {
            Err(format!(
                "{subject} has {} digits, but a Discord Application ID has about 17–20. Re-copy it \
                 from the General Information page of your application.",
                id.len()
            ))
        };
    }
    if looks_like_bot_token(id) {
        return Err(format!(
            "{subject} looks like a Bot Token. That's a secret login for your bot, not the \
             Application ID, and it shouldn't be pasted here. Use the plain number (about 18 \
             digits) from the General Information page instead. If the token has been pasted \
             anywhere else, reset it under Bot -> Reset Token."
        ));
    }
    if id.len() >= 32 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{subject} looks like a long hexadecimal string — that's probably the Public Key. The \
             Application ID is the plain number (about 18 digits) shown above it on the General \
             Information page."
        ));
    }
    Err(format!(
        "{subject} contains characters other than digits. A Discord Application ID is a number of \
         about 18 digits."
    ))
}

/// Discord bot tokens are three dot-separated base64url segments (the
/// first encodes the bot's user id), e.g. `MTIz....GhXyZ1....xxxxxxxx`.
/// Recognized only so the user can be told, plainly, that they pasted the
/// wrong -- and secret -- thing; the value is never stored or echoed.
fn looks_like_bot_token(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts[0].len() >= 18
        && parts[1].len() >= 5
        && parts[2].len() >= 20
        && parts.iter().all(|p| {
            p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// The Discord application id baked into the source. Fill this in once
/// with your application's **Application ID** (the plain ~18-digit number
/// on the General Information page -- public, safe to commit; never the
/// Bot Token or Public Key) and every user of the build gets the Discord
/// status with nothing to configure. Left empty, the id must come from
/// the environment instead (see `resolve_client_id`).
const BUILT_IN_CLIENT_ID: &str = "1557244646155878420";

/// Chooses the Discord application id, in order of precedence:
///
/// 1. `runtime` -- `BLOCKLIGHT_DISCORD_CLIENT_ID` in the environment the
///    app was started from (handy for trying a different application
///    while developing);
/// 2. `build_time` -- the same variable baked in when the app was built;
/// 3. `built_in` -- `BUILT_IN_CLIENT_ID`.
///
/// The first one that's *present* is used, and if it's invalid that's an
/// error rather than a silent fall-through to the next, so a typo can't
/// quietly leave the feature running on a different application.
pub fn pick_client_id(
    runtime: Option<&str>,
    build_time: Option<&str>,
    built_in: &str,
) -> Result<String, String> {
    if let Some(raw) = runtime {
        return check_client_id(raw, CLIENT_ID_VAR);
    }
    if let Some(raw) = build_time {
        return check_client_id(raw, CLIENT_ID_VAR);
    }
    if !built_in.trim().is_empty() {
        return check_client_id(built_in, "The built-in Application ID");
    }
    Err(format!(
        "This build has no Discord Application ID. Set {CLIENT_ID_VAR} when starting the app, or \
         fill in BUILT_IN_CLIENT_ID in presence.rs."
    ))
}

pub fn resolve_client_id() -> Result<String, String> {
    pick_client_id(
        std::env::var(CLIENT_ID_VAR).ok().as_deref(),
        option_env!("BLOCKLIGHT_DISCORD_CLIENT_ID"),
        BUILT_IN_CLIENT_ID,
    )
}

/// One running game, as far as the status is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceSession {
    pub instance_id: String,
    pub instance_name: String,
    /// e.g. "NeoForge 1.21.1".
    pub detail: String,
    /// When the game started, in Unix milliseconds (Discord's unit).
    pub started_at_ms: i64,
}

/// What the Discord status should say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityData {
    pub details: String,
    pub state: String,
    pub started_at_ms: i64,
}

/// Cuts `text` to at most `max` characters (not bytes), ending with an
/// ellipsis when something was removed.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

pub fn build_activity(session: &PresenceSession) -> ActivityData {
    let name = session.instance_name.trim();
    let details = if name.is_empty() {
        "Playing Minecraft".to_string()
    } else {
        // "Playing " is 8 characters; leave room for it.
        format!("Playing {}", truncate_chars(name, MAX_FIELD_CHARS - 8))
    };
    ActivityData {
        details,
        state: truncate_chars(session.detail.trim(), MAX_FIELD_CHARS),
        started_at_ms: session.started_at_ms,
    }
}

/// Pure bookkeeping: which games are running, and whether the user has
/// the feature switched on. The status shows the most recently started
/// game that's still running.
#[derive(Debug, Default)]
pub struct PresenceState {
    enabled: bool,
    sessions: Vec<PresenceSession>,
}

impl PresenceState {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn start(&mut self, session: PresenceSession) {
        // Starting an instance that's somehow already listed replaces it
        // rather than showing it twice.
        self.sessions.retain(|s| s.instance_id != session.instance_id);
        self.sessions.push(session);
    }

    pub fn end(&mut self, instance_id: &str) {
        self.sessions.retain(|s| s.instance_id != instance_id);
    }

    /// What the status should say right now; `None` means "show nothing".
    pub fn desired(&self) -> Option<ActivityData> {
        if !self.enabled {
            return None;
        }
        self.sessions.last().map(build_activity)
    }
}

/// The thing that actually talks to Discord.
pub trait Backend: Send + 'static {
    fn show(&mut self, activity: &ActivityData) -> Result<(), String>;
    /// Clears the status and releases the connection. Never fails: if
    /// Discord is already gone there's nothing left to clear.
    fn hide(&mut self);
}

/// Owns the loop that applies the latest desired status to a backend.
///
/// - Only the newest request matters: a burst (game starts, exits, toggle)
///   collapses into the final state.
/// - A failed `show` (Discord not running yet) is retried every
///   `retry_interval` until it works or the desired status changes.
/// - When the channel closes (the app is shutting down) the status is
///   cleared.
fn run_worker<B: Backend>(
    rx: Receiver<Option<ActivityData>>,
    mut backend: B,
    retry_interval: Duration,
) {
    let mut desired: Option<ActivityData> = None;
    let mut shown: Option<ActivityData> = None;

    loop {
        let out_of_sync = desired != shown;
        let message = if out_of_sync && desired.is_some() {
            match rx.recv_timeout(retry_interval) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            }
        };

        if let Some(mut latest) = message {
            while let Ok(newer) = rx.try_recv() {
                latest = newer;
            }
            desired = latest;
        }

        if desired != shown {
            match &desired {
                Some(activity) => match backend.show(activity) {
                    Ok(()) => shown = desired.clone(),
                    Err(e) => eprintln!("[blocklight] Discord status unavailable: {e}"),
                },
                None => {
                    backend.hide();
                    shown = None;
                }
            }
        }
    }

    backend.hide();
}

/// The real backend, using the local Discord client.
pub struct DiscordBackend {
    client_id: String,
    client: Option<DiscordIpcClient>,
}

impl DiscordBackend {
    pub fn new(client_id: String) -> Self {
        DiscordBackend { client_id, client: None }
    }

    fn connect(&self) -> Result<DiscordIpcClient, String> {
        let mut client = DiscordIpcClient::new(&self.client_id);
        client
            .connect()
            .map_err(|e| format!("couldn't connect to Discord (is the desktop app running?): {e}"))?;
        Ok(client)
    }

    fn send(client: &mut DiscordIpcClient, data: &ActivityData) -> Result<(), String> {
        let payload = activity::Activity::new()
            .details(data.details.as_str())
            .state(data.state.as_str())
            .timestamps(activity::Timestamps::new().start(data.started_at_ms))
            .assets(
                activity::Assets::new()
                    .large_image(LARGE_IMAGE_KEY)
                    .large_text("Blocklight"),
            );
        client.set_activity(payload).map_err(|e| e.to_string())
    }
}

impl Backend for DiscordBackend {
    fn show(&mut self, data: &ActivityData) -> Result<(), String> {
        if let Some(client) = self.client.as_mut() {
            if Self::send(client, data).is_ok() {
                return Ok(());
            }
            // The connection went stale (Discord restarted, say): drop it
            // and fall through to a fresh connection.
            if let Some(mut stale) = self.client.take() {
                let _ = stale.close();
            }
        }
        let mut client = self.connect()?;
        match Self::send(&mut client, data) {
            Ok(()) => {
                self.client = Some(client);
                Ok(())
            }
            Err(e) => {
                let _ = client.close();
                Err(e)
            }
        }
    }

    fn hide(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
    }
}

/// Shared handle held in `AppState`. All methods return immediately.
pub struct PresenceManager {
    state: Mutex<PresenceState>,
    /// `None` when there's no usable application id: the feature is then
    /// inert and the Settings page says why.
    tx: Option<Sender<Option<ActivityData>>>,
    /// Why there's no usable id, for display. `None` when configured.
    config_problem: Option<String>,
}

impl PresenceManager {
    /// Starts the worker thread when `client_id` is `Ok`; an `Err` is kept
    /// as the explanation shown in Settings.
    pub fn new(client_id: Result<String, String>) -> Self {
        let (tx, config_problem) = match client_id {
            Ok(id) => {
                let (tx, rx) = mpsc::channel();
                std::thread::Builder::new()
                    .name("discord-presence".into())
                    .spawn(move || run_worker(rx, DiscordBackend::new(id), RETRY_INTERVAL))
                    .expect("failed to start the Discord presence thread");
                (Some(tx), None)
            }
            Err(problem) => (None, Some(problem)),
        };
        PresenceManager { state: Mutex::new(PresenceState::default()), tx, config_problem }
    }

    pub fn is_configured(&self) -> bool {
        self.tx.is_some()
    }

    /// Why the feature can't run, if it can't.
    pub fn config_problem(&self) -> Option<String> {
        self.config_problem.clone()
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap();
        state.set_enabled(enabled);
        self.push(&state);
    }

    pub fn game_started(&self, instance_id: String, instance_name: String, detail: String) {
        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let mut state = self.state.lock().unwrap();
        state.start(PresenceSession { instance_id, instance_name, detail, started_at_ms });
        self.push(&state);
    }

    pub fn game_exited(&self, instance_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.end(instance_id);
        self.push(&state);
    }

    fn push(&self, state: &PresenceState) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(state.desired());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    fn session(id: &str, name: &str, detail: &str, at: i64) -> PresenceSession {
        PresenceSession {
            instance_id: id.into(),
            instance_name: name.into(),
            detail: detail.into(),
            started_at_ms: at,
        }
    }

    // ---- client id ----

    const ENV: &str = "BLOCKLIGHT_DISCORD_CLIENT_ID";

    #[test]
    fn accepts_a_normal_application_id() {
        assert_eq!(check_client_id("1234567890123456789", ENV).as_deref(), Ok("1234567890123456789"));
        assert_eq!(check_client_id("123456789012345678", ENV).as_deref(), Ok("123456789012345678"));
    }

    #[test]
    fn tolerates_whitespace_and_surrounding_quotes() {
        let id = "1234567890123456789";
        assert_eq!(check_client_id(&format!("  {id}\n"), ENV).as_deref(), Ok(id));
        assert_eq!(check_client_id(&format!("\"{id}\""), ENV).as_deref(), Ok(id));
        assert_eq!(check_client_id(&format!("'{id}'"), ENV).as_deref(), Ok(id));
        assert_eq!(check_client_id(&format!(" \" {id} \" "), ENV).as_deref(), Ok(id));
    }

    #[test]
    fn empty_value_is_explained() {
        assert!(check_client_id("", ENV).unwrap_err().contains("empty"));
        assert!(check_client_id("   ", ENV).unwrap_err().contains("empty"));
        assert!(check_client_id("\"\"", ENV).unwrap_err().contains("empty"));
    }

    #[test]
    fn leftover_placeholder_brackets_are_called_out() {
        let err = check_client_id("<1234567890123456789>", ENV).unwrap_err();
        assert!(err.contains("angle brackets"), "{err}");
    }

    #[test]
    fn wrong_length_digits_report_the_length_not_the_value() {
        let err = check_client_id("12345", ENV).unwrap_err();
        assert!(err.contains("5 digits"), "{err}");
        assert!(!err.contains("12345"), "must not echo the value: {err}");
        assert!(check_client_id("1234567890123456789012345678", ENV).unwrap_err().contains("28 digits"));
    }

    #[test]
    fn a_bot_token_is_recognised_and_never_echoed() {
        // Built from fragments so this file never contains something
        // that looks like a real credential.
        let token = format!("{}.{}.{}", "MTExMjIyMzMzNDQ0NTU1NjY2", "GhXyZ1", "abcdefghijklmnopqrstuvwxyz0-_");
        let err = check_client_id(&token, ENV).unwrap_err();
        assert!(err.contains("Bot Token"), "{err}");
        assert!(err.contains("Reset Token"), "{err}");
        assert!(!err.contains("GhXyZ1") && !err.contains("MTExMj"), "must not echo the value: {err}");
    }

    #[test]
    fn dotted_non_tokens_are_not_mistaken_for_bot_tokens() {
        assert!(!looks_like_bot_token("my.app.id"));
        assert!(!looks_like_bot_token("1234567890123456789"));
        assert!(!looks_like_bot_token("a.b"));
        let err = check_client_id("v1.2.3", ENV).unwrap_err();
        assert!(err.contains("characters other than digits"), "{err}");
    }

    #[test]
    fn a_hex_public_key_is_recognised() {
        let key = "a".repeat(64);
        let err = check_client_id(&key, ENV).unwrap_err();
        assert!(err.contains("Public Key"), "{err}");
        assert!(!err.contains(&key), "must not echo the value");
    }

    #[test]
    fn other_junk_gets_a_generic_explanation() {
        assert!(check_client_id("my-app-id", ENV).unwrap_err().contains("characters other than digits"));
        assert!(check_client_id("123 456 789 012 345 678", ENV).unwrap_err().contains("other than digits"));
    }

    #[test]
    fn messages_name_the_subject_they_were_given() {
        let err = check_client_id("12345", "That value").unwrap_err();
        assert!(err.starts_with("That value has 5 digits"), "{err}");
        assert!(!err.contains("BLOCKLIGHT_DISCORD_CLIENT_ID"), "{err}");
    }

    const ID_A: &str = "1234567890123456789";
    const ID_B: &str = "987654321098765432";
    const ID_C: &str = "111111111111111111";

    #[test]
    fn runtime_id_beats_build_time_and_built_in() {
        assert_eq!(pick_client_id(Some(ID_A), Some(ID_B), ID_C).as_deref(), Ok(ID_A));
    }

    #[test]
    fn build_time_id_beats_built_in() {
        assert_eq!(pick_client_id(None, Some(ID_B), ID_C).as_deref(), Ok(ID_B));
    }

    #[test]
    fn built_in_id_is_the_fallback() {
        assert_eq!(pick_client_id(None, None, ID_C).as_deref(), Ok(ID_C));
        assert_eq!(pick_client_id(None, None, "  \"111111111111111111\" ").as_deref(), Ok(ID_C));
    }

    #[test]
    fn nothing_set_says_how_to_set_it() {
        let err = pick_client_id(None, None, "").unwrap_err();
        assert!(err.contains("BLOCKLIGHT_DISCORD_CLIENT_ID"), "{err}");
        assert!(err.contains("BUILT_IN_CLIENT_ID"), "{err}");
        assert!(pick_client_id(None, None, "   ").is_err(), "whitespace counts as empty");
    }

    #[test]
    fn an_invalid_runtime_id_is_an_error_not_a_silent_fallback() {
        let err = pick_client_id(Some("12345"), Some(ID_B), ID_C).unwrap_err();
        assert!(err.starts_with("BLOCKLIGHT_DISCORD_CLIENT_ID has 5 digits"), "{err}");
    }

    #[test]
    fn an_invalid_built_in_id_is_reported_as_such() {
        let err = pick_client_id(None, None, "12345").unwrap_err();
        assert!(err.starts_with("The built-in Application ID"), "{err}");
    }

    #[test]
    fn a_manager_without_an_id_is_inert_and_says_why() {
        let m = PresenceManager::new(Err("nope".to_string()));
        assert!(!m.is_configured());
        assert_eq!(m.config_problem().as_deref(), Some("nope"));
        // None of these may panic or block with no worker present.
        m.set_enabled(true);
        m.game_started("1".into(), "A".into(), "x".into());
        m.game_exited("1");
    }

    #[test]
    fn the_built_in_id_is_valid_when_set() {
        // Catches a typo'd or wrongly-pasted constant at test time rather
        // than as a disabled toggle in the running app.
        if !BUILT_IN_CLIENT_ID.trim().is_empty() {
            assert!(check_client_id(BUILT_IN_CLIENT_ID, "built-in").is_ok());
        }
    }

    #[test]
    fn the_built_in_id_is_used_when_nothing_overrides_it() {
        assert_eq!(
            pick_client_id(None, None, BUILT_IN_CLIENT_ID).as_deref(),
            Ok(BUILT_IN_CLIENT_ID)
        );
    }

    #[test]
    fn a_manager_with_an_id_is_configured() {
        let m = PresenceManager::new(Ok(ID_A.to_string()));
        assert!(m.is_configured());
        assert_eq!(m.config_problem(), None);
    }

    // ---- activity text ----

    #[test]
    fn activity_names_the_instance_and_its_loader() {
        let a = build_activity(&session("1", "Survival", "NeoForge 1.21.1", 5));
        assert_eq!(a.details, "Playing Survival");
        assert_eq!(a.state, "NeoForge 1.21.1");
        assert_eq!(a.started_at_ms, 5);
    }

    #[test]
    fn long_names_are_cut_to_discords_limit_by_characters() {
        let name = "é".repeat(300); // 2 bytes each: a byte-based cut would panic or overshoot
        let a = build_activity(&session("1", &name, "Fabric 1.20.1", 0));
        assert!(a.details.chars().count() <= MAX_FIELD_CHARS, "{}", a.details.chars().count());
        assert!(a.details.starts_with("Playing é"));
        assert!(a.details.ends_with('…'));
    }

    #[test]
    fn short_names_are_not_padded_or_cut() {
        let a = build_activity(&session("1", "A", "Forge 1.12.2", 0));
        assert_eq!(a.details, "Playing A");
    }

    #[test]
    fn blank_names_fall_back_to_plain_minecraft() {
        let a = build_activity(&session("1", "   ", "Minecraft 1.21.1", 0));
        assert_eq!(a.details, "Playing Minecraft");
    }

    // ---- state ----

    #[test]
    fn nothing_shows_while_disabled() {
        let mut s = PresenceState::default();
        s.start(session("1", "A", "x", 1));
        assert_eq!(s.desired(), None);
    }

    #[test]
    fn enabling_while_a_game_runs_shows_it() {
        let mut s = PresenceState::default();
        s.start(session("1", "Survival", "x", 1));
        s.set_enabled(true);
        assert_eq!(s.desired().unwrap().details, "Playing Survival");
    }

    #[test]
    fn disabling_hides_even_while_a_game_runs() {
        let mut s = PresenceState::default();
        s.set_enabled(true);
        s.start(session("1", "A", "x", 1));
        s.set_enabled(false);
        assert_eq!(s.desired(), None);
    }

    #[test]
    fn exit_clears_the_status() {
        let mut s = PresenceState::default();
        s.set_enabled(true);
        s.start(session("1", "A", "x", 1));
        s.end("1");
        assert_eq!(s.desired(), None);
    }

    #[test]
    fn newest_game_wins_and_the_previous_one_returns_when_it_exits() {
        let mut s = PresenceState::default();
        s.set_enabled(true);
        s.start(session("1", "First", "x", 1));
        s.start(session("2", "Second", "x", 2));
        assert_eq!(s.desired().unwrap().details, "Playing Second");
        s.end("2");
        assert_eq!(s.desired().unwrap().details, "Playing First");
        s.end("1");
        assert_eq!(s.desired(), None);
    }

    #[test]
    fn restarting_the_same_instance_does_not_duplicate_it() {
        let mut s = PresenceState::default();
        s.set_enabled(true);
        s.start(session("1", "A", "x", 1));
        s.start(session("1", "A", "x", 9));
        assert_eq!(s.desired().unwrap().started_at_ms, 9);
        s.end("1");
        assert_eq!(s.desired(), None, "one end() must clear it");
    }

    #[test]
    fn ending_an_unknown_instance_is_harmless() {
        let mut s = PresenceState::default();
        s.set_enabled(true);
        s.start(session("1", "A", "x", 1));
        s.end("nope");
        assert!(s.desired().is_some());
    }

    // ---- worker ----

    #[derive(Default)]
    struct Log {
        shown: Vec<String>,
        hides: usize,
        fail_next: usize,
        attempts: usize,
    }

    struct FakeBackend(Arc<StdMutex<Log>>);

    impl Backend for FakeBackend {
        fn show(&mut self, a: &ActivityData) -> Result<(), String> {
            let mut log = self.0.lock().unwrap();
            log.attempts += 1;
            if log.fail_next > 0 {
                log.fail_next -= 1;
                return Err("discord not running".into());
            }
            log.shown.push(a.details.clone());
            Ok(())
        }
        fn hide(&mut self) {
            self.0.lock().unwrap().hides += 1;
        }
    }

    fn act(details: &str) -> ActivityData {
        ActivityData { details: details.into(), state: "s".into(), started_at_ms: 0 }
    }

    fn start_worker(
        retry: Duration,
    ) -> (Sender<Option<ActivityData>>, Arc<StdMutex<Log>>, std::thread::JoinHandle<()>) {
        let log = Arc::new(StdMutex::new(Log::default()));
        let (tx, rx) = mpsc::channel();
        let backend = FakeBackend(log.clone());
        let handle = std::thread::spawn(move || run_worker(rx, backend, retry));
        (tx, log, handle)
    }

    #[test]
    fn worker_shows_then_clears() {
        let (tx, log, handle) = start_worker(Duration::from_secs(60));
        tx.send(Some(act("Playing A"))).unwrap();
        tx.send(None).unwrap();
        drop(tx);
        handle.join().unwrap();
        let log = log.lock().unwrap();
        assert!(log.hides >= 1, "must clear at the end");
        assert!(log.shown.len() <= 1);
    }

    #[test]
    fn worker_clears_when_the_app_shuts_down_with_a_status_showing() {
        let (tx, log, handle) = start_worker(Duration::from_secs(60));
        tx.send(Some(act("Playing A"))).unwrap();
        // Give the worker time to apply it before the channel closes.
        std::thread::sleep(Duration::from_millis(100));
        drop(tx);
        handle.join().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.shown, vec!["Playing A"]);
        assert!(log.hides >= 1);
    }

    #[test]
    fn worker_does_not_resend_an_unchanged_status() {
        let (tx, log, handle) = start_worker(Duration::from_secs(60));
        tx.send(Some(act("Playing A"))).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        tx.send(Some(act("Playing A"))).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        drop(tx);
        handle.join().unwrap();
        assert_eq!(log.lock().unwrap().shown.len(), 1);
    }

    #[test]
    fn worker_retries_until_discord_appears() {
        let (tx, log, handle) = start_worker(Duration::from_millis(30));
        log.lock().unwrap().fail_next = 3;
        tx.send(Some(act("Playing A"))).unwrap();
        // 3 failures at 30 ms spacing, then success.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if !log.lock().unwrap().shown.is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "never retried successfully");
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(tx);
        handle.join().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.shown, vec!["Playing A"]);
        assert_eq!(log.attempts, 4);
    }

    #[test]
    fn worker_stops_retrying_once_the_status_is_cleared() {
        let (tx, log, handle) = start_worker(Duration::from_millis(30));
        log.lock().unwrap().fail_next = 1000;
        tx.send(Some(act("Playing A"))).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        tx.send(None).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let attempts_after_clear = log.lock().unwrap().attempts;
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(log.lock().unwrap().attempts, attempts_after_clear, "kept retrying a cleared status");
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn a_burst_collapses_to_the_final_state() {
        let (tx, log, handle) = start_worker(Duration::from_secs(60));
        // Queue several updates before the worker can run.
        for i in 0..20 {
            tx.send(Some(act(&format!("Playing {i}")))).unwrap();
        }
        tx.send(Some(act("Playing last"))).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        drop(tx);
        handle.join().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.shown.last().map(String::as_str), Some("Playing last"));
    }
}
