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
    /// The project's media by URL, for thumbnails in the media library
    /// (`framecraft://preview/library/<url>`).
    library: std::collections::HashMap<String, Rc<Vec<u8>>>,
    /// Built-in files (stock Lottie and the like), read from disk on
    /// request from the first of these folders that has them.
    builtin: Vec<std::path::PathBuf>,
}

/// The address the editor's own page loads a media URL from (thumbnails).
/// Absolute and `data:` URLs stay as they are.
pub fn library_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") {
        return url.to_string();
    }
    let path = url.split(['?', '#']).next().unwrap_or("");
    format!(
        "{SCHEME}://{HOST}/library/{}",
        percent_encode_path(path.trim_start_matches('/'))
    )
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

    /// Replaces the media the library serves.
    pub fn set_library(
        &mut self,
        media: impl IntoIterator<Item = (String, Vec<u8>)>,
        builtin: Vec<std::path::PathBuf>,
    ) {
        self.library = media
            .into_iter()
            .map(|(url, bytes)| (library_key(&url), Rc::new(bytes)))
            .collect();
        self.builtin = builtin;
    }

    /// Adds one file to the media the library serves.
    pub fn add_to_library(&mut self, url: &str, bytes: Vec<u8>) {
        self.library.insert(library_key(url), Rc::new(bytes));
    }

    fn library_file(&self, rel: &str) -> Option<(Vec<u8>, &'static str)> {
        let rel = percent_decode(rel);
        if let Some(bytes) = self.library.get(&library_key(&rel)) {
            return Some((bytes.to_vec(), mime_type(&rel)));
        }
        if rel.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        // Before any project is open, the app's own icons still come from
        // the built-in folders.
        let fallback;
        let dirs = if self.builtin.is_empty() {
            fallback = crate::project::builtin_dirs();
            &fallback
        } else {
            &self.builtin
        };
        let bytes = dirs
            .iter()
            .find_map(|d| std::fs::read(d.join(&rel)).ok())?;
        Some((bytes, mime_type(&rel)))
    }

    #[cfg(test)]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The file for a request path (`/<revision>/<file>`), if that revision
    /// is still kept.
    pub fn lookup(&self, request_path: &str) -> Option<(Vec<u8>, &'static str)> {
        if let Some(rel) = request_path.strip_prefix("/library/") {
            return self.library_file(rel);
        }
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

/// URLs compare without the leading `/` and query.
fn library_key(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string()
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
    fn serves_library_media() {
        let mut files = PreviewFiles::default();
        files.set_library([("/media/images/a b.png".to_string(), vec![7])], Vec::new());
        let url = library_url("/media/images/a b.png?v=1");
        assert_eq!(url, "framecraft://preview/library/media/images/a%20b.png");
        let path = url.trim_start_matches("framecraft://preview");
        assert_eq!(files.lookup(path), Some((vec![7], "image/png")));
        assert_eq!(files.lookup("/library/../etc/passwd"), None);
    }

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
