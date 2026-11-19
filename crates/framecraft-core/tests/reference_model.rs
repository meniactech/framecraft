// SPDX-License-Identifier: AGPL-3.0-or-later
//! Loads every reference graphic exported from FRAME and checks that the
//! document model reads it completely and writes it back unchanged.
//!
//! The reference set is not part of this repository. Point
//! `FRAMECRAFT_REFERENCE` at a folder made by FRAME's
//! `reference/generate.mjs`; by default `../FRAME/reference` next to this
//! repository is used. Without it the test is skipped.

use framecraft_core::model::Graphic;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn reference_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("FRAMECRAFT_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/reference"));
    dir.join("expected").is_dir().then_some(dir)
}

/// Every `graphic.json` under `expected/<project>/<graphic>/`, sorted.
fn reference_graphics(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for project in std::fs::read_dir(dir.join("expected")).unwrap().flatten() {
        if !project.path().is_dir() {
            continue;
        }
        for graphic in std::fs::read_dir(project.path()).unwrap().flatten() {
            let file = graphic.path().join("graphic.json");
            if file.is_file() {
                out.push(file);
            }
        }
    }
    out.sort();
    out
}

/// JSON equality where numbers compare by value: JavaScript writes `5`
/// where Rust's `f64` writes `5.0`, and both mean the same number.
fn same_json(a: &Value, b: &Value, path: &str, diffs: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.as_f64() != y.as_f64() {
                diffs.push(format!("{path}: {x} != {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => same_json(v, w, &format!("{path}.{k}"), diffs),
                    None => diffs.push(format!("{path}.{k}: missing after save")),
                }
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                diffs.push(format!("{path}.{k}: added by save"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                diffs.push(format!("{path}: length {} != {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                same_json(v, w, &format!("{path}[{i}]"), diffs);
            }
        }
        _ => {
            if a != b {
                diffs.push(format!("{path}: {a} != {b}"));
            }
        }
    }
}

#[test]
fn reference_graphics_round_trip() {
    let Some(dir) = reference_dir() else {
        eprintln!("skipped: no reference set (set FRAMECRAFT_REFERENCE)");
        return;
    };
    let files = reference_graphics(&dir);
    assert!(
        !files.is_empty(),
        "no graphic.json files under {}",
        dir.display()
    );

    let mut failures = Vec::new();
    for file in &files {
        let name = file
            .parent()
            .unwrap()
            .strip_prefix(dir.join("expected"))
            .unwrap()
            .display()
            .to_string();
        let text = std::fs::read_to_string(file).unwrap();
        let original: Value = serde_json::from_str(&text).unwrap();

        let graphic = match Graphic::from_json(&text) {
            Ok(g) => g,
            Err(e) => {
                failures.push(format!("{name}: does not load: {e}"));
                continue;
            }
        };

        let unknown = graphic.unknown_fields();
        if !unknown.is_empty() {
            failures.push(format!(
                "{name}: fields the model does not cover: {}",
                unknown.join(", ")
            ));
        }

        let saved: Value = serde_json::from_str(&graphic.to_json().unwrap()).unwrap();
        let mut diffs = Vec::new();
        same_json(&original, &saved, "$", &mut diffs);
        if !diffs.is_empty() {
            failures.push(format!(
                "{name}: changed by load and save:\n  {}",
                diffs.join("\n  ")
            ));
        }
    }

    println!("{} reference graphics checked", files.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
