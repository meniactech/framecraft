// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every reference graphic loads completely and saves back unchanged.

mod common;

use framecraft_core::model::Graphic;

#[test]
fn reference_graphics_round_trip() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let graphics = common::graphics(&expected);
    let mut failures = Vec::new();
    for (name, dir) in &graphics {
        let text = std::fs::read_to_string(dir.join("graphic.json")).unwrap();
        let original: serde_json::Value = serde_json::from_str(&text).unwrap();
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
        let saved: serde_json::Value = serde_json::from_str(&graphic.to_json().unwrap()).unwrap();
        let diffs = common::json_diffs(&original, &saved);
        if !diffs.is_empty() {
            failures.push(format!(
                "{name}: changed by load and save:\n  {}",
                diffs.join("\n  ")
            ));
        }
    }
    println!("{} reference graphics checked", graphics.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
