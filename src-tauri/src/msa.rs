//! Real Microsoft account sign-in for Blocklight, using the standard
//! device-code flow followed by the documented Xbox Live -> XSTS ->
//! Minecraft Services token exchange that every legitimate third-party
//! launcher (Prism Launcher, ATLauncher, the Modrinth App, ...) uses.
//!
//! This module deliberately does **not** embed a Microsoft Azure AD
//! application (client) id. Microsoft's terms restrict distributing a
//! shared client id for unofficial Minecraft launchers, so -- exactly
//! like the open-source launchers above -- each build of Blocklight must
//! be configured with a client id registered by whoever ships that
//! build, via `BLOCKLIGHT_MSA_CLIENT_ID` (see README). Without one,
//! `begin_device_code_sign_in` returns `AppError::Internal` explaining
//! why, rather than pretending to sign the user in.
//!
//! Ownership is verified against Minecraft Services' entitlement
//! endpoint; there is no code path here that creates, forges, or skips
//! that check.

use serde::Deserialize;
use std::env;

use crate::error::AppError;

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

fn client_id() -> Result<String, AppError> {
    env::var("BLOCKLIGHT_MSA_CLIENT_ID").map_err(|_| {
        AppError::Internal(
            "Microsoft sign-in isn't configured. Set BLOCKLIGHT_MSA_CLIENT_ID to an Azure AD \
             application id registered for device-code auth (see README: Microsoft Sign-In Setup)."
                .into(),
        )
    })
}

/// Kept internal to the backend: `device_code` is a bearer-style secret
/// for the token-polling step and is never sent to the frontend. The
/// frontend only ever sees `MsaPrompt` (below), delivered as an event.
#[derive(Debug, Clone)]
pub struct DeviceCodePrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    device_code: String,
}

/// What the frontend is allowed to see: enough to show "go to
/// microsoft.com/link and enter this code" and nothing that would let it
/// (or anything reading its state) complete the sign-in on its own.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MsaPrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

impl From<&DeviceCodePrompt> for MsaPrompt {
    fn from(p: &DeviceCodePrompt) -> Self {
        MsaPrompt {
            user_code: p.user_code.clone(),
            verification_uri: p.verification_uri.clone(),
            expires_in: p.expires_in,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

pub async fn begin_device_code_sign_in(
    client: &reqwest::Client,
) -> Result<DeviceCodePrompt, AppError> {
    let client_id = client_id()?;
    let resp = client
        .post(DEVICE_CODE_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?;

    let raw: RawDeviceCodeResponse = resp.json().await?;
    Ok(DeviceCodePrompt {
        user_code: raw.user_code,
        verification_uri: raw.verification_uri,
        expires_in: raw.expires_in,
        interval: raw.interval,
        device_code: raw.device_code,
    })
}

pub struct SignedInProfile {
    pub gamertag: String,
    pub owns_minecraft: bool,
}

#[derive(Debug, Deserialize)]
struct RawTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct RawTokenErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct RawXblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblUserHash>,
}

#[derive(Debug, Deserialize)]
struct XblUserHash {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct RawMcLoginResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct RawEntitlementsResponse {
    items: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawProfileResponse {
    name: String,
}

/// Polls the token endpoint at the interval the device-code response
/// specified, then walks the full Xbox Live -> XSTS -> Minecraft Services
/// chain. Returns as soon as the user completes sign-in in their browser,
/// an error if they decline, or a timeout error once `expires_in` elapses.
pub async fn poll_and_complete(
    client: &reqwest::Client,
    prompt: &DeviceCodePrompt,
) -> Result<SignedInProfile, AppError> {
    let client_id = client_id()?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(prompt.expires_in);
    let interval = std::time::Duration::from_secs(prompt.interval.max(2));

    let ms_access_token = loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::Internal("sign-in timed out".into()));
        }
        tokio::time::sleep(interval).await;

        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", prompt.device_code.as_str()),
            ])
            .send()
            .await?;

        if resp.status().is_success() {
            let token: RawTokenResponse = resp.json().await?;
            break token.access_token;
        }

        let err: RawTokenErrorResponse = resp
            .json()
            .await
            .unwrap_or(RawTokenErrorResponse { error: "unknown_error".into() });
        match err.error.as_str() {
            "authorization_pending" => continue,
            "authorization_declined" => {
                return Err(AppError::Internal("sign-in was declined".into()))
            }
            "expired_token" => return Err(AppError::Internal("sign-in code expired".into())),
            other => return Err(AppError::Internal(format!("sign-in failed: {other}"))),
        }
    };

    // Xbox Live user authentication.
    let xbl: RawXblResponse = client
        .post(XBL_AUTH_URL)
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    let user_hash = xbl
        .display_claims
        .xui
        .first()
        .map(|u| u.uhs.clone())
        .ok_or_else(|| AppError::Internal("Xbox Live response missing user hash".into()))?;

    // XSTS authorization for the Minecraft relying party.
    let xsts: RawXblResponse = client
        .post(XSTS_AUTH_URL)
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    // Minecraft Services sign-in.
    let mc_login: RawMcLoginResponse = client
        .post(MC_LOGIN_URL)
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={user_hash};{}", xsts.token)
        }))
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    // Ownership verification -- this is the check Blocklight must never
    // skip or fake. A user without a real Minecraft entitlement is
    // reported as not owning the game rather than silently let through.
    let entitlements: RawEntitlementsResponse = client
        .get(MC_ENTITLEMENTS_URL)
        .bearer_auth(&mc_login.access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;
    let owns_minecraft = !entitlements.items.is_empty();

    let profile: RawProfileResponse = client
        .get(MC_PROFILE_URL)
        .bearer_auth(&mc_login.access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    Ok(SignedInProfile {
        gamertag: profile.name,
        owns_minecraft,
    })
}
