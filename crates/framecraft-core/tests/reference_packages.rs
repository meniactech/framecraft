// SPDX-License-Identifier: AGPL-3.0-or-later
//! End to end: export every graphic of the reference project packs as SPX,
//! OGraf and HTML banner packages, and compare every file (path, size and
//! SHA-256) with FRAME's packages.
//!
//! Media comes from the project pack itself; built-in files (fonts,
//! particle sprites, stock Lottie) from FRAME's `static/` folder.

mod common;

use framecraft_core::export::package::{
    ExportRequest, MediaSource, Package, banner_package, ograf_package, spx_package,
};
use framecraft_core::model::Graphic;
use framecraft_core::normalize::normalize;
use framecraft_core::profile::Profile;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const EXPORTED_AT: &str = "2026-01-01T12:00:00.000Z";

/// Media from a project pack, then FRAME's static folder.
struct PackMedia {
    pack: BTreeMap<String, Vec<u8>>,
    static_dir: PathBuf,
}

impl MediaSource for PackMedia {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        let clean = url.split(['?', '#']).next().unwrap_or("");
        if let Some(bytes) = self.pack.get(clean) {
            return Some(bytes.clone());
        }
        let rel = clean.strip_prefix('/')?;
        if rel.split('/').any(|part| part == "..") {
            return None;
        }
        std::fs::read(self.static_dir.join(rel)).ok()
    }
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut out).unwrap();
    out
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Differences between a package and FRAME's package in `dir`: hashes for
/// every file, and for HTML pages the line comparison of `common`.
fn compare(pkg: &Package, dir: &Path) -> Vec<String> {
    let want = common::read_json(&dir.join("files.json"));
    let want = want.as_object().unwrap();
    let mut out = Vec::new();
    for (path, bytes) in &pkg.files {
        match want.get(path) {
            None => out.push(format!("unexpected file {path}")),
            Some(_) if path.ends_with(".html") => {
                let want_html = std::fs::read_to_string(dir.join(path)).unwrap();
                let diffs = common::html_diffs(&want_html, std::str::from_utf8(bytes).unwrap());
                out.extend(diffs.into_iter().map(|d| format!("{path}: {d}")));
            }
            Some(w) if w["sha256"] != sha256(bytes) => out.push(format!(
                "{path}: content differs (size {} vs {})",
                bytes.len(),
                w["size"]
            )),
            Some(_) => {}
        }
    }
    for path in want.keys().filter(|k| pkg.get(k).is_none()) {
        out.push(format!("missing file {path}"));
    }
    if !pkg.missing.is_empty() {
        out.push(format!("media not found: {}", pkg.missing.join(", ")));
    }
    out
}

#[test]
fn packages_match_frame() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let reference = expected.parent().unwrap();
    let static_dir = reference.parent().unwrap().join("static");
    let p = Profile::frame();
    let mut failures = Vec::new();
    let mut compared = 0;

    let mut packs: Vec<PathBuf> = std::fs::read_dir(reference.join("projects"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".frameproject.zip"))
        .collect();
    packs.sort();
    assert!(!packs.is_empty(), "no project packs");

    for pack_path in packs {
        let pack_name = pack_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".frameproject.zip", "");
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&pack_path).unwrap()).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&read_entry(&mut zip, "manifest.json")).unwrap();
        let mut media = PackMedia {
            pack: BTreeMap::new(),
            static_dir: static_dir.clone(),
        };
        for m in manifest["media"].as_array().into_iter().flatten() {
            let bytes = read_entry(&mut zip, m["path"].as_str().unwrap());
            media
                .pack
                .insert(m["url"].as_str().unwrap().to_string(), bytes);
        }

        for g in manifest["graphics"].as_array().unwrap() {
            let path = g["path"].as_str().unwrap();
            let key = Path::new(path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let name = g["name"].as_str().unwrap();
            let text = String::from_utf8(read_entry(&mut zip, path)).unwrap();
            let doc = normalize(&Graphic::from_json(&text).unwrap(), &p);
            let req = ExportRequest {
                doc: &doc,
                graphic_name: name,
                palette_colors: &[],
                user_fonts: &[],
                exported_at: EXPORTED_AT,
            };
            let dir = expected.join(&pack_name).join(&key);
            for (format, pkg) in [
                ("spx", spx_package(&req, &media, &p)),
                ("ograf", ograf_package(&req, &media, &p)),
                ("banner", banner_package(&req, &media, &p)),
            ] {
                compared += pkg.files.len();
                let diffs = compare(&pkg, &dir.join(format));
                if !diffs.is_empty() {
                    failures.push(format!(
                        "{pack_name}/{key}/{format}:\n  {}",
                        diffs.join("\n  ")
                    ));
                }
                // The ZIP itself must be readable and hold the same files.
                let bytes = pkg.to_zip().unwrap();
                let z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
                assert_eq!(
                    z.len(),
                    pkg.files.len(),
                    "{pack_name}/{key}/{format}: ZIP entry count"
                );
            }
        }
    }
    println!("{compared} package files compared");
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
