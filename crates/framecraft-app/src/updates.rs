// SPDX-License-Identifier: AGPL-3.0-or-later
//! Update check: at start, at most once a day, asks GitHub for the latest
//! FrameCraft release. A newer one shows in the status bar; clicking opens
//! its release page in the browser (nothing installs by itself). The
//! request carries nothing about the user or their projects. Off with the
//! Project view's "Check for updates" (or FRAMECRAFT_NO_UPDATE_CHECK=1).

use crate::state::Ui;
use dioxus::prelude::*;
use serde_json::{Value, json};

const LATEST: &str = "https://api.github.com/repos/meniactech/framecraft/releases/latest";
/// Release pages the notice may open (nothing else is opened).
const RELEASES: &str = "https://github.com/meniactech/framecraft/releases/";
const DAY_SECS: u64 = 24 * 60 * 60;

/// A newer release: version and release page.
#[derive(Clone, PartialEq, Debug)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// `v1.2.3` / `1.2.3` → numbers, for comparing.
fn parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Whether `latest` is a newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    parts(latest) > parts(current)
}

pub fn enabled() -> bool {
    std::env::var_os("FRAMECRAFT_NO_UPDATE_CHECK").is_none()
        && crate::layout::setting("updateCheck")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
}

pub fn set_enabled(on: bool) {
    crate::layout::set_setting("updateCheck", Value::from(on));
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The newer release remembered from the last check, if still newer.
fn remembered() -> Option<Update> {
    let v = crate::layout::setting("updateLatest")?;
    let u = Update {
        version: v["version"].as_str()?.to_string(),
        url: v["url"].as_str()?.to_string(),
    };
    is_newer(&u.version, env!("CARGO_PKG_VERSION")).then_some(u)
}

/// Runs the check (see the module docs) and sets `ui.update`.
pub async fn check(ui: Ui) {
    if !enabled() {
        return;
    }
    let mut note = ui.update;
    if let Some(u) = remembered() {
        note.set(Some(u));
    }
    let last = crate::layout::setting("updateCheckedAt")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if now().saturating_sub(last) < DAY_SECS {
        return;
    }
    // The webview's fetch: GitHub's API allows it (CORS), no HTTP crate.
    let mut eval = document::eval(&format!(
        "try {{ const r = await fetch({LATEST:?}, {{ headers: {{ Accept: 'application/vnd.github+json' }} }}); \
         const j = r.ok ? await r.json() : null; \
         dioxus.send(j && !j.draft && !j.prerelease ? {{ tag: j.tag_name, url: j.html_url }} : null); }} \
         catch (e) {{ dioxus.send(null); }}"
    ));
    let Ok(found) = eval.recv::<Value>().await else {
        return;
    };
    crate::layout::set_setting("updateCheckedAt", Value::from(now()));
    let (Some(tag), Some(url)) = (found["tag"].as_str(), found["url"].as_str()) else {
        return;
    };
    let version = tag.trim_start_matches('v').to_string();
    crate::layout::set_setting("updateLatest", json!({ "version": version, "url": url }));
    if is_newer(&version, env!("CARGO_PKG_VERSION")) && url.starts_with(RELEASES) {
        note.set(Some(Update {
            version,
            url: url.to_string(),
        }));
    }
}

/// FrameCraft's own pages: the only links the app opens.
const REPO_PAGES: &str = "https://github.com/meniactech/framecraft";

/// Opens one of FrameCraft's GitHub pages in the default browser.
pub fn open(url: &str) {
    if !url.starts_with(REPO_PAGES) {
        return;
    }
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("Could not open {url}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versions_compare() {
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.0"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }
}
