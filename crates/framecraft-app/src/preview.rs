// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serves the current preview package to the webview from memory.
//!
//! The viewport is an iframe on `framecraft://preview/<revision>/<entry>.html`:
//! the real exported page with its runtime, so the editor shows exactly what
//! SPX, OGraf hosts and Air will play. Each rebuild gets a new revision,
//! which also keeps the webview from reusing cached files.
//!
//! The preview has its own URL scheme because Dioxus allows only one
//! navigation on `dioxus://` (its own page) and opens `http(s)://` links
//! in the system browser; both would keep the iframe empty.

use framecraft_core::export::package::Package;
use std::cell::RefCell;
use std::rc::Rc;

/// URL scheme of the preview, registered on the webview at start.
pub const SCHEME: &str = "framecraft";
/// Host part of preview URLs.
pub const HOST: &str = "preview";

/// The latest packages by revision. Two are kept: while a rebuilt preview
/// loads in the hidden frame, the visible one may still request files.
#[derive(Default)]
pub struct PreviewFiles {
    revision: u64,
    packages: Vec<(u64, Package)>,
}

pub type SharedPreview = Rc<RefCell<PreviewFiles>>;

thread_local! {
    /// The preview the protocol handler serves. The UI and the webview's
    /// protocol handler both run on the main thread.
    pub static STORE: SharedPreview = SharedPreview::default();
}

impl PreviewFiles {
    /// Replaces the package; returns the iframe URL of the new revision.
    pub fn publish(&mut self, package: Package) -> String {
        self.revision += 1;
        let entry = package
            .files
            .iter()
            .map(|(p, _)| p.as_str())
            .find(|p| p.ends_with(".html") && !p.contains('/'))
            .unwrap_or("index.html")
            .to_string();
        self.packages.push((self.revision, package));
        if self.packages.len() > 2 {
            self.packages.remove(0);
        }
        format!(
            "{SCHEME}://{HOST}/{}/{}",
            self.revision,
            percent_encode_path(&entry)
        )
    }

    #[cfg(test)]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The file for a request path (`/<revision>/<file>`), if that revision
    /// is still kept.
    pub fn lookup(&self, request_path: &str) -> Option<(Vec<u8>, &'static str)> {
        let (rev, file) = request_path.trim_start_matches('/').split_once('/')?;
        let rev = rev.parse::<u64>().ok()?;
        let (_, package) = self.packages.iter().find(|(r, _)| *r == rev)?;
        let file = percent_decode(file);
        let bytes = package.get(&file)?;
        Some((bytes.to_vec(), mime_type(&file)))
    }
}

pub fn mime_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    }
}

fn percent_encode_path(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_latest_two_revisions() {
        let mut files = PreviewFiles::default();
        let mut pkg = Package::default();
        pkg.files.push(("js/a.js".into(), b"x".to_vec()));
        pkg.files.push(("Name Tag.html".into(), b"<html>".to_vec()));
        let url = files.publish(pkg);
        assert_eq!(url, "framecraft://preview/1/Name%20Tag.html");
        assert_eq!(
            files.lookup("/1/Name%20Tag.html"),
            Some((b"<html>".to_vec(), "text/html; charset=utf-8"))
        );
        assert!(files.lookup("/1/js/a.js").is_some());
        assert!(files.lookup("/0/js/a.js").is_none());
        assert!(files.lookup("/x/1/js/a.js").is_none());
        // The previous revision stays available while the next one loads;
        // the one before that is dropped.
        files.publish(Package::default());
        assert!(files.lookup("/1/js/a.js").is_some());
        files.publish(Package::default());
        assert!(files.lookup("/1/js/a.js").is_none());
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%C3%A4%"), "a bä%");
    }
}
