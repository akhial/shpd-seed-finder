// SPDX-License-Identifier: GPL-3.0-or-later

//! Startup check for a newer release on GitHub, surfaced as an alert
//! dialog per the GNOME HIG. Silent on any failure: the app must never
//! nag or break because the network or the API is unavailable.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::{gio, glib};
use serde::{Deserialize, Serialize};

use crate::config::APP_ID;

const RELEASES_PAGE: &str = "https://github.com/akhial/shpd-seed-seeker/releases/latest";
const API_URL: &str = "https://api.github.com/repos/akhial/shpd-seed-seeker/releases/latest";
const CHECK_INTERVAL: Duration = Duration::from_hours(24);

struct UpdateInfo {
    version: String,
    url: String,
}

#[derive(Default, Deserialize, Serialize)]
struct UpdateState {
    #[serde(default)]
    skipped_version: Option<String>,
    #[serde(default)]
    last_checked: u64,
}

/// Checks for a newer release in the background and presents an alert
/// dialog over `window` when one is available. The `SEED_SEEKER_FAKE_LATEST`
/// environment variable stands in for the latest release tag, bypassing
/// the network and the daily throttle.
pub fn check_on_startup(window: &adw::ApplicationWindow) {
    let fake = std::env::var("SEED_SEEKER_FAKE_LATEST").ok();
    let mut state = load_state();
    let now = unix_now();
    if fake.is_none() && now.saturating_sub(state.last_checked) < CHECK_INTERVAL.as_secs() {
        return;
    }
    state.last_checked = now;
    save_state(&state);

    let window = window.clone();
    glib::spawn_future_local(async move {
        let latest = match fake {
            Some(tag) => Some((tag, RELEASES_PAGE.to_owned())),
            None => gio::spawn_blocking(fetch_latest).await.ok().flatten(),
        };
        let Some((tag, url)) = latest else { return };
        let Some(update) = newer(&tag, env!("CARGO_PKG_VERSION"), url) else {
            return;
        };
        if load_state().skipped_version.as_deref() == Some(update.version.as_str()) {
            return;
        }
        present_dialog(&window, &update);
    });
}

fn present_dialog(window: &adw::ApplicationWindow, update: &UpdateInfo) {
    let dialog = adw::AlertDialog::new(
        Some("Update available"),
        Some(&format!(
            "Seed Seeker {} is available on GitHub. You have {}.",
            update.version,
            env!("CARGO_PKG_VERSION")
        )),
    );
    dialog.add_responses(&[
        ("skip", "Skip This Version"),
        ("later", "Not Now"),
        ("download", "Download"),
    ]);
    dialog.set_response_appearance("download", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("download"));
    dialog.set_close_response("later");
    let url = update.url.clone();
    let version = update.version.clone();
    let weak_window = window.downgrade();
    dialog.connect_response(None, move |_, response| match response {
        "download" => {
            gtk::UriLauncher::new(&url).launch(
                weak_window.upgrade().as_ref(),
                gio::Cancellable::NONE,
                |_| {},
            );
        }
        "skip" => {
            let mut state = load_state();
            state.skipped_version = Some(version.clone());
            save_state(&state);
        }
        _ => {}
    });
    dialog.present(Some(window));
}

/// Fetches the latest release tag and page from the GitHub API.
fn fetch_latest() -> Option<(String, String)> {
    let response = ureq::get(API_URL)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
        .ok()?;
    let body: serde_json::Value = serde_json::from_str(&response.into_string().ok()?).ok()?;
    let tag = body.get("tag_name")?.as_str()?.to_owned();
    let url = body
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(RELEASES_PAGE)
        .to_owned();
    Some((tag, url))
}

/// Returns the latest release when it is strictly newer than `current`.
fn newer(latest: &str, current: &str, url: String) -> Option<UpdateInfo> {
    let latest_parts = parse(latest)?;
    let current_parts = parse(current)?;
    for index in 0..latest_parts.len().max(current_parts.len()) {
        let left = latest_parts.get(index).copied().unwrap_or(0);
        let right = current_parts.get(index).copied().unwrap_or(0);
        if left != right {
            return (left > right).then(|| UpdateInfo {
                version: display_version(latest).to_owned(),
                url,
            });
        }
    }
    None
}

/// Strips the tag prefix and any pre-release suffix: "v1.2.3-beta" → "1.2.3".
fn display_version(tag: &str) -> &str {
    let bare = tag.trim();
    let bare = bare.strip_prefix(['v', 'V']).unwrap_or(bare);
    bare.split('-').next().unwrap_or(bare)
}

fn parse(version: &str) -> Option<Vec<u64>> {
    let parts: Vec<u64> = display_version(version)
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect();
    let expected = display_version(version).split('.').count();
    (parts.len() == expected && !parts.is_empty()).then_some(parts)
}

fn state_path() -> PathBuf {
    glib::user_config_dir().join(APP_ID).join("update.json")
}

fn load_state() -> UpdateState {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_state(state: &UpdateState) {
    let path = state_path();
    let Ok(contents) = serde_json::to_string_pretty(state) else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = fs::write(path, contents);
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::{display_version, newer, parse};

    fn version(latest: &str, current: &str) -> Option<String> {
        newer(latest, current, String::new()).map(|update| update.version)
    }

    #[test]
    fn newer_version_is_reported() {
        assert_eq!(version("v0.6.0", "0.5.2").as_deref(), Some("0.6.0"));
        assert_eq!(version("v1.0.0", "0.5.2").as_deref(), Some("1.0.0"));
        assert_eq!(version("0.5.10", "0.5.2").as_deref(), Some("0.5.10"));
        assert_eq!(version("v0.6", "0.5.2").as_deref(), Some("0.6"));
    }

    #[test]
    fn same_or_older_version_is_ignored() {
        assert_eq!(version("v0.5.2", "0.5.2"), None);
        assert_eq!(version("v0.5.1", "0.5.2"), None);
        assert_eq!(version("v0.4.9", "0.5.2"), None);
        assert_eq!(version("v0.5", "0.5.0"), None);
    }

    #[test]
    fn suffixes_and_prefixes_are_stripped() {
        assert_eq!(version("v0.6.0-rc1", "0.5.2").as_deref(), Some("0.6.0"));
        assert_eq!(display_version("v1.2.3-beta"), "1.2.3");
        assert_eq!(display_version(" V2.0.0 "), "2.0.0");
    }

    #[test]
    fn garbage_is_ignored() {
        assert_eq!(version("nightly", "0.5.2"), None);
        assert_eq!(version("v9.9.9", "unknown"), None);
        assert_eq!(version("", "0.5.2"), None);
        assert!(parse("1.2.x").is_none());
    }
}
