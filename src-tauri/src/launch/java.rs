use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

use crate::error::AppError;

pub struct DetectedJava {
    pub executable: PathBuf,
    pub major_version: u32,
}

/// Finds a Java executable and reports its major version. Tries
/// `JAVA_HOME` first, then whatever `java` resolves to on `PATH`. Does
/// **not** download a JRE -- that's a real feature real launchers have
/// (Mojang publishes one per platform), but it's a separate, sizeable
/// chunk of work on top of everything else here, so for now Blocklight
/// asks the person to have a suitable Java installed already and says so
/// plainly rather than failing with a confusing error.
pub async fn find_java() -> Result<DetectedJava, AppError> {
    let mut candidates = Vec::new();
    if let Some(path) = java_home_candidate() {
        candidates.push(path);
    }
    candidates.push(PathBuf::from("java"));

    for exe in candidates {
        if let Some(major) = probe_version(&exe).await {
            return Ok(DetectedJava {
                executable: exe,
                major_version: major,
            });
        }
    }

    Err(AppError::Internal(
        "Blocklight couldn't find a Java installation. Install a Java runtime (adoptium.net has \
         free builds for every major version) and make sure `java` is on your PATH, or set \
         JAVA_HOME."
            .into(),
    ))
}

/// Confirms `java` meets an instance's minimum Java version. Newer is
/// accepted (the JVM is backwards compatible for what a Minecraft client
/// needs); older is refused with a clear, specific error rather than a
/// cryptic JVM crash at launch.
pub async fn require_java(required_major: Option<u32>) -> Result<DetectedJava, AppError> {
    let java = find_java().await?;
    if let Some(required) = required_major {
        if java.major_version < required {
            return Err(AppError::Internal(format!(
                "This instance needs Java {required}+, but Blocklight found Java {} on your \
                 system. Install a newer Java runtime (adoptium.net) and try again.",
                java.major_version
            )));
        }
    }
    Ok(java)
}

fn java_home_candidate() -> Option<PathBuf> {
    let home = std::env::var_os("JAVA_HOME")?;
    let mut path = PathBuf::from(home).join("bin").join("java");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    Some(path)
}

async fn probe_version(exe: &PathBuf) -> Option<u32> {
    let output = Command::new(exe)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    // `java -version` prints to stderr, e.g.:
    //   openjdk version "21.0.1" 2023-10-17
    //   java version "1.8.0_392"
    let text = String::from_utf8_lossy(&output.stderr);
    let first_quote = text.find('"')?;
    let rest = &text[first_quote + 1..];
    let end_quote = rest.find('"')?;
    let version_str = &rest[..end_quote];

    parse_major_version(version_str)
}

fn parse_major_version(version_str: &str) -> Option<u32> {
    let mut parts = version_str.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        // Legacy scheme: "1.8.0_392" -> Java 8.
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_version() {
        assert_eq!(parse_major_version("21.0.1"), Some(21));
        assert_eq!(parse_major_version("17"), Some(17));
    }

    #[test]
    fn parses_legacy_version() {
        assert_eq!(parse_major_version("1.8.0_392"), Some(8));
    }
}
