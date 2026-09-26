//! Velopack owns package validation/replacement. No custom executable installer.
//! GitHub/TLS is the feed trust boundary; feed hashes are not publisher signatures.
use serde::Serialize;
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;
use velopack::{UpdateCheck, UpdateManager, VelopackApp, sources::GithubSource};

const REPOSITORY: &str = "https://github.com/joswayski/findanything";
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
pub struct UpdateStatus {
    pub state: String,
    pub message: String,
}

fn state() -> &'static Mutex<UpdateStatus> {
    static STATUS: OnceLock<Mutex<UpdateStatus>> = OnceLock::new();
    STATUS.get_or_init(|| {
        Mutex::new(UpdateStatus {
            state: "disabled".into(),
            message: "Updates require an installed native package.".into(),
        })
    })
}

fn set_status(name: &str, message: &str) {
    *state().lock().unwrap() = UpdateStatus {
        state: name.into(),
        message: message.into(),
    };
}

fn manager() -> Result<UpdateManager, String> {
    // The OS/architecture channel is embedded by vpk pack. Never override it
    // with a common channel that could select another platform's package.
    UpdateManager::new(GithubSource::new(REPOSITORY, None, true), None, None)
        .map_err(|e| e.to_string())
}

/// Must run before instance election and UI initialization. A downloaded update
/// is applied automatically on the next launch; active searches are not interrupted.
pub fn initialize() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        VelopackApp::build().run();
    });
}

pub fn start() {
    static START: Once = Once::new();
    START.call_once(|| {
        if manager().is_err() {
            return;
        }
        set_status("idle", "Automatic updates enabled");
        std::thread::spawn(|| {
            loop {
                check();
                std::thread::sleep(CHECK_INTERVAL);
            }
        });
    });
}

pub fn status() -> UpdateStatus {
    state().lock().unwrap().clone()
}

/// Nonblocking and deduplicated across timer and manual checks.
pub fn check() {
    {
        let mut status = state().lock().unwrap();
        if matches!(status.state.as_str(), "checking" | "downloading" | "ready") {
            return;
        }
        status.state = "checking".into();
        status.message = "Checking for updates…".into();
    }
    std::thread::spawn(|| {
        let manager = match manager() {
            Ok(manager) => manager,
            Err(_) => {
                set_status("disabled", "Updates require an installed native package.");
                return;
            }
        };
        let result = (|| -> Result<bool, String> {
            if manager.get_update_pending_restart().is_some() {
                return Ok(true);
            }
            match manager.check_for_updates().map_err(|e| e.to_string())? {
                UpdateCheck::UpdateAvailable(update) => {
                    set_status("downloading", "Downloading update…");
                    manager
                        .download_updates(&update, None)
                        .map_err(|e| e.to_string())?;
                    Ok(true)
                }
                UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(false),
            }
        })();
        match result {
            Ok(true) => set_status("ready", "Update ready — restart to install"),
            Ok(false) => set_status("idle", "Up to date"),
            Err(error) => set_status("error", &format!("Update check failed: {error}")),
        }
    });
}

pub fn apply() -> Result<(), String> {
    if status().state != "ready" {
        return Err("No downloaded update is ready.".into());
    }
    let manager = manager()?;
    let update = manager
        .get_update_pending_restart()
        .ok_or("No downloaded update is ready.")?;
    manager
        .apply_updates_and_restart(&update)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn restart_without_a_download_does_not_exit() {
        assert_eq!(
            super::apply().unwrap_err(),
            "No downloaded update is ready."
        );
    }
}
