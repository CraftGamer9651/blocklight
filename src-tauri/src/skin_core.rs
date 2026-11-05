//! The pure (no network, no Tauri) half of skin changing: validating a
//! skin file, turning a SkinMC link into its texture URL, building the
//! upload request body, and classifying Minecraft Services' failures.
//! Kept free of `reqwest`/`AppError` so every rule here is unit-tested in
//! isolation; `skins.rs` does the actual I/O around it.

use std::time::{Duration, Instant};

/// Skins are tiny (a 64x64 PNG is a few KB). This only exists to stop a
/// wrong file -- or a hostile server -- from making us buffer something
/// huge, so it is deliberately generous.
pub const MAX_SKIN_BYTES: usize = 256 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Arm model. `Classic` is the 4px-arm "Steve" model, `Slim` the 3px-arm
/// "Alex" model. The two use the same texture layout; Minecraft Services
/// just needs to be told which one the skin was drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinVariant {
    Classic,
    Slim,
}

impl SkinVariant {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "classic" => Some(SkinVariant::Classic),
            "slim" => Some(SkinVariant::Slim),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SkinVariant::Classic => "classic",
            SkinVariant::Slim => "slim",
        }
    }
}

/// Checks that `bytes` is a PNG of a size Minecraft accepts for a skin:
/// 64x64, or the legacy 64x32 layout. Only the header is inspected -- the
/// image data is never decoded -- and Minecraft Services does the final
/// validation of the pixels. Returns the dimensions, or a message that is
/// safe to show the user as-is.
pub fn inspect_png(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(format!(
            "that file is {} KB, far too large for a skin (they're a few KB)",
            bytes.len() / 1024
        ));
    }
    // Signature (8) + IHDR chunk header (length 4 + type 4) + width/height (8).
    if bytes.len() < 24 || bytes[..8] != PNG_SIGNATURE {
        return Err("that isn't a PNG image — skins must be .png files".to_string());
    }
    if &bytes[12..16] != b"IHDR" {
        return Err("that PNG looks corrupted".to_string());
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    match (width, height) {
        (64, 64) | (64, 32) => Ok((width, height)),
        _ => Err(format!(
            "skins must be 64×64 pixels (or the older 64×32); this image is {width}×{height}"
        )),
    }
}

/// Extracts the skin id from a SkinMC link. Accepts the skin page
/// (`https://skinmc.net/skins/<id>`, optionally `www.`, a locale prefix
/// like `/en/`, or a trailing slug) and SkinMC's own texture/render URL
/// (`.../api/v1/renders/skins/<id>/skin`). Returns the id in canonical
/// lowercase form, or a message that is safe to show the user.
///
/// Only the *id* is taken from the link; the URL that gets downloaded is
/// rebuilt from it (`skinmc_texture_url`), so a pasted link can never
/// redirect the download to another host.
pub fn parse_skinmc_link(input: &str) -> Result<String, String> {
    let not_skinmc = || {
        "that doesn't look like a SkinMC skin link (expected something like \
         https://skinmc.net/skins/…)"
            .to_string()
    };

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("paste a SkinMC skin link first".to_string());
    }
    let url = url::Url::parse(trimmed).map_err(|_| not_skinmc())?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(not_skinmc());
    }
    match url.host_str().map(|h| h.to_ascii_lowercase()).as_deref() {
        Some("skinmc.net") | Some("www.skinmc.net") => {}
        _ => return Err(not_skinmc()),
    }

    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    // `/skins/<id>[/slug]`, optionally behind a locale segment (`/en/`,
    // `/pt-br/`), or `/api/v1/renders/skins/<id>/...`.
    let id_segment = match segments.as_slice() {
        ["skins", id, ..] => Some(*id),
        [locale, "skins", id, ..] if is_locale(locale) => Some(*id),
        ["api", "v1", "renders", "skins", id, ..] => Some(*id),
        _ => None,
    };

    let id_segment = id_segment.ok_or_else(|| {
        // Profile pages (/profile/<name>) are the one near-miss worth
        // explaining, since people paste their own SkinMC profile.
        if segments.first() == Some(&"profile") {
            "that's a profile link — open one of its skins and copy that page's link instead"
                .to_string()
        } else {
            not_skinmc()
        }
    })?;

    uuid::Uuid::parse_str(id_segment)
        .map(|u| u.hyphenated().to_string())
        .map_err(|_| not_skinmc())
}

fn is_locale(segment: &str) -> bool {
    let len = segment.len();
    (2..=5).contains(&len)
        && segment.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        && segment != "api"
}

/// SkinMC's "Download skin" endpoint for a skin id: the raw 64x64 texture.
pub fn skinmc_texture_url(id: &str) -> String {
    format!("https://skinmc.net/api/v1/renders/skins/{id}/skin")
}

/// Hosts a SkinMC download is allowed to end up on after redirects.
pub fn is_skinmc_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "skinmc.net" || h.ends_with(".skinmc.net")
}

/// Builds the `multipart/form-data` body for
/// `POST /minecraft/profile/skins`: a `variant` field followed by the
/// `file` field. Written by hand so the app doesn't need reqwest's
/// `multipart` feature (and its extra dependencies) for one request.
pub fn multipart_body(boundary: &str, variant: SkinVariant, png: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(png.len() + 400);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{}\r\n",
            variant.as_str()
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// How a failed skin upload should be reported.
#[derive(Debug, PartialEq, Eq)]
pub enum UploadFailure {
    /// The token was rejected: the user needs to sign in again.
    SignInRequired,
    /// Anything else, as a message that is safe to show as-is.
    Message(String),
}

/// Maps an error response from Minecraft Services to something
/// actionable. `body` is the raw response text; Mojang's error JSON
/// carries a human-readable `errorMessage`, used when present.
pub fn classify_upload_failure(status: u16, body: &str) -> UploadFailure {
    let api_message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("errorMessage")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .filter(|m| !m.trim().is_empty());

    match status {
        401 => UploadFailure::SignInRequired,
        429 => UploadFailure::Message(
            "Minecraft Services is rate-limiting skin changes — wait a minute and try again"
                .to_string(),
        ),
        400 => UploadFailure::Message(match api_message {
            Some(m) => format!("Minecraft Services rejected that skin: {m}"),
            None => "Minecraft Services rejected that skin file".to_string(),
        }),
        403 => UploadFailure::Message(match api_message {
            Some(m) => format!("Minecraft Services refused the change: {m}"),
            None => "Minecraft Services refused the change".to_string(),
        }),
        _ => UploadFailure::Message(format!(
            "Minecraft Services returned an unexpected error ({status})"
        )),
    }
}

/// A Minecraft Services access token, held in memory only. Blocklight
/// deliberately never writes this to disk (see the note in
/// `launch_instance`), so it lives exactly as long as the app run or its
/// own expiry, whichever comes first.
pub struct McSession {
    access_token: String,
    expires_at: Instant,
}

impl McSession {
    /// `expires_in_secs` is what Minecraft Services reported; a minute is
    /// shaved off so a token isn't used in its last seconds.
    pub fn new(access_token: String, expires_in_secs: u64) -> Self {
        Self::with_clock(access_token, expires_in_secs, Instant::now())
    }

    fn with_clock(access_token: String, expires_in_secs: u64, now: Instant) -> Self {
        McSession {
            access_token,
            expires_at: now + Duration::from_secs(expires_in_secs.saturating_sub(60)),
        }
    }

    /// The token, if it hasn't expired.
    pub fn token(&self) -> Option<&str> {
        self.token_at(Instant::now())
    }

    fn token_at(&self, now: Instant) -> Option<&str> {
        (now < self.expires_at).then_some(self.access_token.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "4d242e62-18a0-46cb-b6dd-6c334371aac2";

    /// A header-only PNG: signature, IHDR chunk header, dimensions. Real
    /// files have much more after it, but `inspect_png` reads only this.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = PNG_SIGNATURE.to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    // ---- inspect_png ----

    #[test]
    fn accepts_modern_and_legacy_skin_sizes() {
        assert_eq!(inspect_png(&png_header(64, 64)), Ok((64, 64)));
        assert_eq!(inspect_png(&png_header(64, 32)), Ok((64, 32)));
    }

    #[test]
    fn rejects_other_sizes_and_says_which() {
        let err = inspect_png(&png_header(128, 128)).unwrap_err();
        assert!(err.contains("128×128"), "{err}");
        assert!(inspect_png(&png_header(32, 32)).is_err());
        assert!(inspect_png(&png_header(64, 48)).is_err());
    }

    #[test]
    fn rejects_non_png_data() {
        assert!(inspect_png(b"GIF89a....................").unwrap_err().contains("PNG"));
        assert!(inspect_png(&[]).is_err());
        assert!(inspect_png(&PNG_SIGNATURE).is_err(), "truncated after signature");
    }

    #[test]
    fn rejects_png_without_ihdr_first() {
        let mut v = png_header(64, 64);
        v[12..16].copy_from_slice(b"IDAT");
        assert!(inspect_png(&v).unwrap_err().contains("corrupted"));
    }

    #[test]
    fn rejects_oversized_files() {
        let mut v = png_header(64, 64);
        v.resize(MAX_SKIN_BYTES + 1, 0);
        assert!(inspect_png(&v).unwrap_err().contains("too large"));
    }

    // ---- parse_skinmc_link ----

    #[test]
    fn parses_skin_page_links() {
        for url in [
            format!("https://skinmc.net/skins/{ID}"),
            format!("https://www.skinmc.net/skins/{ID}"),
            format!("https://skinmc.net/skins/{ID}/"),
            format!("https://skinmc.net/skins/{ID}/some-slug"),
            format!("  https://skinmc.net/skins/{ID}  "),
            format!("http://skinmc.net/skins/{ID}"),
            format!("https://SkinMC.net/skins/{ID}?utm_source=x#top"),
            format!("https://skinmc.net/en/skins/{ID}"),
            format!("https://skinmc.net/pt-br/skins/{ID}"),
        ] {
            assert_eq!(parse_skinmc_link(&url).as_deref(), Ok(ID), "{url}");
        }
    }

    #[test]
    fn parses_the_texture_download_url_too() {
        let url = format!("https://skinmc.net/api/v1/renders/skins/{ID}/skin");
        assert_eq!(parse_skinmc_link(&url).as_deref(), Ok(ID));
    }

    #[test]
    fn normalizes_the_id_to_lowercase() {
        let url = format!("https://skinmc.net/skins/{}", ID.to_uppercase());
        assert_eq!(parse_skinmc_link(&url).as_deref(), Ok(ID));
    }

    #[test]
    fn rejects_other_hosts_including_lookalikes() {
        for url in [
            format!("https://example.com/skins/{ID}"),
            format!("https://skinmc.net.evil.example/skins/{ID}"),
            format!("https://evilskinmc.net/skins/{ID}"),
            format!("https://namemc.com/skins/{ID}"),
            format!("ftp://skinmc.net/skins/{ID}"),
        ] {
            assert!(parse_skinmc_link(&url).is_err(), "{url}");
        }
    }

    #[test]
    fn rejects_pages_that_are_not_a_skin() {
        assert!(parse_skinmc_link("https://skinmc.net/skins").is_err());
        assert!(parse_skinmc_link("https://skinmc.net/skins/tagged/steve").is_err());
        assert!(parse_skinmc_link("https://skinmc.net/skins/not-a-uuid").is_err());
        assert!(parse_skinmc_link("https://skinmc.net/capes/editor").is_err());
        assert!(parse_skinmc_link("not a url").is_err());
        assert!(parse_skinmc_link("").unwrap_err().contains("paste"));
    }

    #[test]
    fn explains_profile_links() {
        let err = parse_skinmc_link("https://skinmc.net/profile/Steve.1").unwrap_err();
        assert!(err.contains("profile"), "{err}");
    }

    #[test]
    fn texture_url_is_rebuilt_from_the_id_only() {
        assert_eq!(
            skinmc_texture_url(ID),
            format!("https://skinmc.net/api/v1/renders/skins/{ID}/skin")
        );
    }

    #[test]
    fn redirect_host_check() {
        assert!(is_skinmc_host("skinmc.net"));
        assert!(is_skinmc_host("cdn.skinmc.net"));
        assert!(!is_skinmc_host("skinmc.net.evil.example"));
        assert!(!is_skinmc_host("notskinmc.net"));
    }

    // ---- variant ----

    #[test]
    fn variant_round_trips_and_rejects_unknowns() {
        assert_eq!(SkinVariant::parse("classic"), Some(SkinVariant::Classic));
        assert_eq!(SkinVariant::parse(" SLIM "), Some(SkinVariant::Slim));
        assert_eq!(SkinVariant::parse("alex"), None);
        assert_eq!(SkinVariant::Slim.as_str(), "slim");
    }

    // ---- multipart ----

    #[test]
    fn multipart_body_has_variant_then_file_and_preserves_bytes() {
        // Bytes that would break a naive string-based body: a PNG
        // signature (high bit set) and CR/LF/boundary-looking runs.
        let mut png = png_header(64, 64);
        png.extend_from_slice(b"\r\n--B\r\n\xff\xfe\x00");
        let body = multipart_body("B", SkinVariant::Slim, &png);

        let text = String::from_utf8_lossy(&body);
        let variant_at = text.find("name=\"variant\"").unwrap();
        let file_at = text.find("name=\"file\"").unwrap();
        assert!(variant_at < file_at, "variant must come before file");
        assert!(text.contains("\r\n\r\nslim\r\n"));
        assert!(text.contains("filename=\"skin.png\""));
        assert!(text.contains("Content-Type: image/png"));

        let ends_with_close = body.ends_with(b"\r\n--B--\r\n");
        assert!(ends_with_close);
        // The PNG bytes appear verbatim, contiguous, right before the closer.
        let tail_start = body.len() - b"\r\n--B--\r\n".len() - png.len();
        assert_eq!(&body[tail_start..tail_start + png.len()], png.as_slice());
    }

    // ---- upload failure classification ----

    #[test]
    fn unauthorized_means_sign_in_again() {
        assert_eq!(classify_upload_failure(401, ""), UploadFailure::SignInRequired);
        assert_eq!(
            classify_upload_failure(401, r#"{"error":"UNAUTHORIZED"}"#),
            UploadFailure::SignInRequired
        );
    }

    #[test]
    fn bad_request_surfaces_minecrafts_own_message() {
        let body = r#"{"path":"/minecraft/profile/skins","errorType":"BAD_REQUEST","errorMessage":"Invalid skin: wrong dimensions"}"#;
        match classify_upload_failure(400, body) {
            UploadFailure::Message(m) => assert!(m.contains("wrong dimensions"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_request_without_json_still_reads_sensibly() {
        match classify_upload_failure(400, "<html>nope</html>") {
            UploadFailure::Message(m) => assert!(m.contains("rejected"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rate_limit_and_unexpected_statuses_are_distinct_messages() {
        match classify_upload_failure(429, "") {
            UploadFailure::Message(m) => assert!(m.contains("rate-limiting")),
            other => panic!("{other:?}"),
        }
        match classify_upload_failure(503, "") {
            UploadFailure::Message(m) => assert!(m.contains("503")),
            other => panic!("{other:?}"),
        }
    }

    // ---- session expiry ----

    #[test]
    fn session_token_is_valid_until_a_minute_before_expiry() {
        let t0 = Instant::now();
        let s = McSession::with_clock("tok".into(), 3600, t0);
        assert_eq!(s.token_at(t0), Some("tok"));
        assert_eq!(s.token_at(t0 + Duration::from_secs(3539)), Some("tok"));
        assert_eq!(s.token_at(t0 + Duration::from_secs(3541)), None);
    }

    #[test]
    fn session_with_tiny_ttl_is_immediately_unusable() {
        let t0 = Instant::now();
        let s = McSession::with_clock("tok".into(), 30, t0);
        assert_eq!(s.token_at(t0), None);
    }
}
