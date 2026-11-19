// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serves the current preview package to the webview from memory.
//!
//! The viewport is an iframe on `/preview/<revision>/<entry>.html`: the
//! real exported page with its runtime, so the editor shows exactly what
//! SPX, OGraf hosts and Air will play. Each rebuild gets a new revision,
//! which also keeps the webview from reusing cached files.

use framecraft_core::export::package::Package;
use std::cell::RefCell;
use std::rc::Rc;

/// URL prefix the asset handler is registered under.
pub const PREFIX: &str = "preview";

#[derive(Default)]
pub struct PreviewFiles {
    revision: u64,
    package: Package,
    entry: String,
}

pub type SharedPreview = Rc<RefCell<PreviewFiles>>;

impl PreviewFiles {
    /// Replaces the package; returns the iframe URL of the new revision.
    pub fn publish(&mut self, package: Package) -> String {
        self.revision += 1;
        self.entry = package
            .files
            .iter()
            .map(|(p, _)| p.as_str())
            .find(|p| p.ends_with(".html") && !p.contains('/'))
            .unwrap_or("index.html")
            .to_string();
        self.package = package;
        format!(
            "/{PREFIX}/{}/{}",
            self.revision,
            percent_encode_path(&self.entry)
        )
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The file for a request path, if it belongs to the current revision.
    pub fn lookup(&self, request_path: &str) -> Option<(Vec<u8>, &'static str)> {
        let rest = request_path
            .trim_start_matches('/')
            .strip_prefix(PREFIX)?
            .strip_prefix('/')?;
        let (rev, file) = rest.split_once('/')?;
        if rev.parse::<u64>().ok()? != self.revision {
            return None;
        }
        let file = percent_decode(file);
        let bytes = self.package.get(&file)?;
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
    fn serves_only_the_current_revision() {
        let mut files = PreviewFiles::default();
        let mut pkg = Package::default();
        pkg.files.push(("js/a.js".into(), b"x".to_vec()));
        pkg.files.push(("Name Tag.html".into(), b"<html>".to_vec()));
        let url = files.publish(pkg);
        assert_eq!(url, "/preview/1/Name%20Tag.html");
        assert_eq!(
            files.lookup("/preview/1/Name%20Tag.html"),
            Some((b"<html>".to_vec(), "text/html; charset=utf-8"))
        );
        assert!(files.lookup("/preview/1/js/a.js").is_some());
        assert!(files.lookup("/preview/0/js/a.js").is_none());
        assert!(files.lookup("/other/1/js/a.js").is_none());
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%C3%A4%"), "a bä%");
    }
}
