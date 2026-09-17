use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::ConnectivityState;

/// Shared, cheaply-readable connectivity flag. Tauri commands read this
/// instead of making a network call, so "is a link install possible right
/// now" is instant and never blocks the UI thread.
pub struct NetworkMonitor {
    online: Arc<AtomicBool>,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        // Optimistic default; the first poll (a few seconds after launch)
        // corrects this if the host actually has no connection.
        NetworkMonitor {
            online: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> ConnectivityState {
        if self.is_online() {
            ConnectivityState::Online
        } else {
            ConnectivityState::Offline
        }
    }

    /// Spawns a low-frequency background poll. Deliberately infrequent and
    /// with a short per-request timeout: the spec calls for detecting lost
    /// connectivity, not hammering the network in the background.
    pub fn start(&self, app: AppHandle) {
        let online = self.online.clone();
        tauri::async_runtime::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .build()
                .expect("reqwest client");

            loop {
                let reachable = probe(&client).await;
                let was_online = online.swap(reachable, Ordering::Relaxed);

                if was_online != reachable {
                    let state = if reachable {
                        ConnectivityState::Online
                    } else {
                        ConnectivityState::Offline
                    };
                    let _ = app.emit("connectivity-changed", state);
                }

                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        });
    }
}

async fn probe(client: &reqwest::Client) -> bool {
    // A tiny HEAD request against Modrinth's API root doubles as "is the
    // internet up" and "are our providers reachable". Any successful
    // response (even a 4xx) means the network path is up.
    client
        .head("https://api.modrinth.com/v2")
        .send()
        .await
        .is_ok()
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}
