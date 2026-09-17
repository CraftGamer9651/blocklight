use serde::Serialize;

/// A single application-wide error type.
///
/// Every variant maps to one of the error states described in the product
/// spec ("Unsupported link", "Invalid link", "Project unavailable", ...).
/// `AppError` is serialized to the frontend as `{ kind, message }` so the UI
/// can render the right empty/error state without string-matching.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unsupported link")]
    UnsupportedLink,

    #[error("invalid link: {0}")]
    InvalidLink(String),

    #[error("project unavailable")]
    ProjectUnavailable,

    #[error("no compatible version")]
    NoCompatibleVersion,

    #[error("network failure: {0}")]
    NetworkFailure(String),

    #[error("you're offline")]
    Offline,

    #[error("file integrity check failed for {file}")]
    IntegrityCheckFailed { file: String },

    #[error("refused to write outside instance directory: {0}")]
    PathTraversal(String),

    #[error("curseforge api key is not configured")]
    MissingCurseForgeKey,

    #[error("this Microsoft account doesn't own Minecraft")]
    MinecraftNotOwned,

    #[error("database error: {0}")]
    Database(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Stable machine-readable discriminant, mirrored on the frontend in
/// `src/lib/types.ts` (`AppErrorKind`) so components can switch on it.
impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::UnsupportedLink => "unsupported_link",
            AppError::InvalidLink(_) => "invalid_link",
            AppError::ProjectUnavailable => "project_unavailable",
            AppError::NoCompatibleVersion => "no_compatible_version",
            AppError::NetworkFailure(_) => "network_failure",
            AppError::Offline => "offline",
            AppError::IntegrityCheckFailed { .. } => "integrity_check_failed",
            AppError::PathTraversal(_) => "path_traversal",
            AppError::MissingCurseForgeKey => "missing_curseforge_key",
            AppError::MinecraftNotOwned => "minecraft_not_owned",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::Internal(_) => "internal",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SerializableAppError {
    pub kind: &'static str,
    pub message: String,
}

impl From<AppError> for SerializableAppError {
    fn from(err: AppError) -> Self {
        SerializableAppError {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

// Tauri commands return `Result<T, AppError>`; Tauri serializes the `Err`
// variant with `Serialize`, so we implement it in terms of the stable
// kind/message shape rather than deriving it directly on the enum.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializableAppError::from_ref(self).serialize(serializer)
    }
}

impl SerializableAppError {
    fn from_ref(err: &AppError) -> Self {
        SerializableAppError {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::NetworkFailure(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<url::ParseError> for AppError {
    fn from(e: url::ParseError) -> Self {
        AppError::InvalidLink(e.to_string())
    }
}
