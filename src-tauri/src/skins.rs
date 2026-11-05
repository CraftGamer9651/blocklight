//! Changing the signed-in Microsoft account's skin: downloading a skin
//! from SkinMC and uploading one to Minecraft Services. All the rules
//! (validation, link parsing, request body, error classification) live in
//! `skin_core.rs`; this file only does the network I/O around them.

use crate::error::AppError;
use crate::skin_core::{
    self, classify_upload_failure, is_skinmc_host, multipart_body, skinmc_texture_url,
    SkinVariant, UploadFailure, MAX_SKIN_BYTES,
};

const SKIN_UPLOAD_URL: &str = "https://api.minecraftservices.com/minecraft/profile/skins";

/// Downloads the raw skin texture for a SkinMC skin id, bounded in size
/// and checked to be a valid skin PNG before it's returned.
pub async fn fetch_skinmc_texture(client: &reqwest::Client, id: &str) -> Result<Vec<u8>, AppError> {
    let url = skinmc_texture_url(id);
    let mut resp = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "image/png,image/*;q=0.8")
        .send()
        .await
        .map_err(|e| AppError::NetworkFailure(format!("couldn't reach SkinMC: {e}")))?;

    // The URL is fixed to skinmc.net, but redirects are followed by
    // default -- make sure the bytes still came from SkinMC.
    match resp.url().host_str() {
        Some(host) if is_skinmc_host(host) => {}
        _ => {
            return Err(AppError::NetworkFailure(
                "SkinMC redirected the download somewhere unexpected, so it was refused".into(),
            ))
        }
    }

    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            404 => AppError::InvalidLink("SkinMC has no skin with that id".into()),
            401 | 403 | 429 | 503 => AppError::NetworkFailure(
                "SkinMC declined the download (it may block automated requests). Download the \
                 skin from its page and use Upload a file instead"
                    .into(),
            ),
            code => AppError::NetworkFailure(format!("SkinMC returned an error ({code})")),
        });
    }

    if resp
        .content_length()
        .is_some_and(|len| len as usize > MAX_SKIN_BYTES)
    {
        return Err(AppError::NetworkFailure(
            "SkinMC sent a file far larger than any skin, so it was refused".into(),
        ));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_SKIN_BYTES {
            return Err(AppError::NetworkFailure(
                "SkinMC sent a file far larger than any skin, so it was refused".into(),
            ));
        }
    }

    skin_core::inspect_png(&bytes).map_err(|reason| {
        AppError::NetworkFailure(format!("SkinMC didn't return a usable skin: {reason}"))
    })?;
    Ok(bytes)
}

/// Uploads `png` as the account's skin. `token` is a Minecraft Services
/// access token for that account. A 401 is reported as
/// `AppError::SignInRequired` so the caller can discard the stale token
/// and the UI can ask the user to sign in again.
pub async fn upload_skin(
    client: &reqwest::Client,
    token: &str,
    png: &[u8],
    variant: SkinVariant,
) -> Result<(), AppError> {
    let boundary = format!("blocklight{}", uuid::Uuid::new_v4().simple());
    let body = multipart_body(&boundary, variant, png);

    let resp = client
        .post(SKIN_UPLOAD_URL)
        .bearer_auth(token)
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::NetworkFailure(format!("couldn't reach Minecraft Services: {e}")))?;

    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let text = resp.text().await.unwrap_or_default();
    Err(match classify_upload_failure(status.as_u16(), &text) {
        UploadFailure::SignInRequired => AppError::SignInRequired,
        UploadFailure::Message(message) => AppError::Internal(message),
    })
}
