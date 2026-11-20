// SPDX-License-Identifier: AGPL-3.0-or-later
//! End to end: export every graphic of the reference project packs as SPX,
//! OGraf and HTML banner packages, and compare every file (path, size and
//! SHA-256) with FRAME's packages.
//!
//! Media comes from the project pack itself; built-in files (fonts,
//! particle sprites, stock Lottie) from FRAME's `static/` folder.

mod common;

use framecraft_core::export::package::{
    ExportRequest, Package, SuiteGraphic, SuiteRequest, banner_package, ograf_package, spx_package,
    suite_package,
};
use framecraft_core::normalize::{Document, normalize};
use framecraft_core::pack::{Pack, read_pack, sanitize_pack_file_name};
use framecraft_core::profile::Profile;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const EXPORTED_AT: &str = "2026-01-01T12:00:00.000Z";

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// FrameCraft's deliberate changes to FRAME's runtime: `(ours, FRAME's)`.
/// The runtime is compared with these undone, so the rest of it still has
/// to match FRAME byte for byte.
const RUNTIME_PATCHES: &[(&str, &str)] = &[(
    "      // Only an animated width is written each frame: without keys the\r\n      // layer's own CSS has it, including a 4-sided border (FrameCraft).\r\n      if (Array.isArray(track.timeline.borderWidth) && track.timeline.borderWidth.length > 0) {\r\n        node.style.borderWidth = borderWidth + 'px';\r\n      }\r\n",
    "      node.style.borderWidth = borderWidth + 'px';\r\n",
)];

/// The runtime as FRAME ships it (FrameCraft's patches undone).
fn unpatched(path: &str, bytes: &[u8]) -> Vec<u8> {
    if !path.ends_with("ionframe-spx-runtime.mjs") {
        return bytes.to_vec();
    }
    let mut text = String::from_utf8(bytes.to_vec()).unwrap();
    for (ours, frame) in RUNTIME_PATCHES {
        assert!(text.contains(ours), "runtime patch not found: {ours}");
        text = text.replacen(ours, frame, 1);
    }
    text.into_bytes()
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
            Some(w) if w["sha256"] != sha256(&unpatched(path, bytes)) => out.push(format!(
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
        let (pack, warnings) = read_pack(&std::fs::read(&pack_path).unwrap()).unwrap();
        assert!(warnings.is_empty(), "{pack_name}: {warnings:?}");
        let Pack::Project(pack) = pack else {
            panic!("{pack_name} is not a project pack")
        };
        let docs: Vec<Document> = pack
            .graphics
            .iter()
            .map(|g| normalize(&g.graphic, &p))
            .collect();
        let keys: Vec<String> = (1..=pack.graphics.len())
            .zip(&pack.graphics)
            .map(|(i, g)| {
                format!(
                    "{i:02}-{}",
                    sanitize_pack_file_name(&g.name, &format!("graphic-{i}"))
                )
            })
            .collect();
        let project_name = pack.name.clone();
        let media = common::PackMedia {
            pack,
            static_dir: static_dir.clone(),
        };

        for ((g, doc), key) in media.pack.graphics.iter().zip(&docs).zip(&keys) {
            let req = ExportRequest {
                doc,
                graphic_name: &g.name,
                palette_colors: &[],
                user_fonts: &[],
                exported_at: EXPORTED_AT,
            };
            let dir = expected.join(&pack_name).join(key);
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
                let z = zip::ZipArchive::new(std::io::Cursor::new(pkg.to_zip().unwrap())).unwrap();
                assert_eq!(
                    z.len(),
                    pkg.files.len(),
                    "{pack_name}/{key}/{format}: ZIP entry count"
                );
            }
        }

        // Whole-project suites.
        let suite: Vec<SuiteGraphic> = media
            .pack
            .graphics
            .iter()
            .zip(&docs)
            .map(|(g, doc)| SuiteGraphic {
                id: &g.id,
                name: &g.name,
                doc,
            })
            .collect();
        let req = SuiteRequest {
            project_name: &project_name,
            graphics: &suite,
            palette_colors: &[],
            user_fonts: &[],
            exported_at: EXPORTED_AT,
        };
        for (dir_name, ograf) in [("_suite-spx", false), ("_suite-ograf", true)] {
            let pkg = suite_package(&req, ograf, &media, &p);
            compared += pkg.files.len();
            let diffs = compare(&pkg, &expected.join(&pack_name).join(dir_name));
            if !diffs.is_empty() {
                failures.push(format!("{pack_name}/{dir_name}:\n  {}", diffs.join("\n  ")));
            }
        }
    }
    println!("{compared} package files compared");
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
