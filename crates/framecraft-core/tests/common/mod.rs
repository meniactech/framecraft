// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers for tests that compare against FRAME's reference set.
//!
//! The reference set is not part of this repository. Point
//! `FRAMECRAFT_REFERENCE` at a folder made by FRAME's
//! `reference/generate.mjs`; by default `../FRAME/reference` next to this
//! repository is used. Without it the tests are skipped.
#![allow(dead_code)]

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
