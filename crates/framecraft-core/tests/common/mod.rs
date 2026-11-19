// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers for tests that compare against FRAME's reference set.
//!
//! The reference set is not part of this repository. Point
//! `FRAMECRAFT_REFERENCE` at a folder made by FRAME's
//! `reference/generate.mjs`; by default `../FRAME/reference` next to this
//! repository is used. Without it the tests are skipped.
#![allow(dead_code)]

use framecraft_core::export::package::MediaSource;
use framecraft_core::pack::ProjectPack;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `<reference>/expected`, or `None` (with a note) when it is missing.
pub fn expected_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("FRAMECRAFT_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/reference"))
        .join("expected");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipped: no reference set (set FRAMECRAFT_REFERENCE)");
        None
    }
}

/// `(name, folder)` for every graphic: `expected/<project>/<graphic>/`.
pub fn graphics(expected: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for project in std::fs::read_dir(expected).unwrap().flatten() {
        if !project.path().is_dir() {
            continue;
        }
        for graphic in std::fs::read_dir(project.path()).unwrap().flatten() {
            let dir = graphic.path();
            if dir.join("graphic.json").is_file() {
                let name = dir.strip_prefix(expected).unwrap().display().to_string();
                out.push((name, dir));
            }
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no graphics under {}", expected.display());
    out
}

pub fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Differences between two JSON values. Numbers compare by value (`5` and
/// `5.0` are equal); object key order does not matter.
pub fn json_diffs(want: &Value, got: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff(want, got, "$", &mut out);
    out
}

fn diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.as_f64() != y.as_f64() {
                out.push(format!("{path}: want {x}, got {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => diff(v, w, &format!("{path}.{k}"), out),
                    None => out.push(format!("{path}.{k}: missing (want {})", short(v))),
                }
            }
            for (k, v) in y.iter().filter(|(k, _)| !x.contains_key(*k)) {
                out.push(format!("{path}.{k}: unexpected {}", short(v)));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: want {} entries, got {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                diff(v, w, &format!("{path}[{i}]"), out);
            }
        }
        _ if a != b => out.push(format!("{path}: want {}, got {}", short(a), short(b))),
        _ => {}
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 80 {
        format!(
            "{}…",
            &s[..s.char_indices().nth(77).map_or(s.len(), |(i, _)| i)]
        )
    } else {
        s
    }
}

/// The line holding `window.__ionframeSpx`.
const DATA_PREFIX: &str = "    window.__ionframeSpx = ";

/// Differences between two exported HTML pages. Lines must match byte for
/// byte, except the runtime data line, which FRAME fills from input
/// objects in their original key order: that one is compared as JSON.
pub fn html_diffs(want: &str, got: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (w, g): (Vec<&str>, Vec<&str>) = (want.split('\n').collect(), got.split('\n').collect());
    if w.len() != g.len() {
        out.push(format!("{} lines, want {}", g.len(), w.len()));
    }
    for (i, (wl, gl)) in w.iter().zip(&g).enumerate() {
        if let (Some(wj), Some(gj)) = (wl.strip_prefix(DATA_PREFIX), gl.strip_prefix(DATA_PREFIX)) {
            let parse = |s: &str| {
                serde_json::from_str::<serde_json::Value>(s.trim_end_matches(';')).unwrap()
            };
            let diffs = json_diffs(&parse(wj), &parse(gj));
            out.extend(
                diffs
                    .into_iter()
                    .take(15)
                    .map(|d| format!("line {} (runtime data) {d}", i + 1)),
            );
        } else if wl != gl {
            let at = wl
                .bytes()
                .zip(gl.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(wl.len().min(gl.len()));
            let from = at.saturating_sub(60);
            let cut = |s: &str| s.get(from..(at + 80).min(s.len())).unwrap_or(s).to_string();
            out.push(format!(
                "line {}:\n      want: …{}\n      got:  …{}",
                i + 1,
                cut(wl),
                cut(gl)
            ));
        }
        if out.len() > 15 {
            break;
        }
    }
    out
}

/// Media from a project pack (read by FrameCraft), then FRAME's static folder.
pub struct PackMedia {
    pub pack: ProjectPack,
    pub static_dir: PathBuf,
}

impl MediaSource for PackMedia {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = self.pack.read(url) {
            return Some(bytes);
        }
        let clean = url.split(['?', '#']).next().unwrap_or("");
        let rel = clean.strip_prefix('/')?;
        if rel.split('/').any(|part| part == "..") {
            return None;
        }
        std::fs::read(self.static_dir.join(rel)).ok()
    }
}
