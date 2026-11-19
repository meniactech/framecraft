// SPDX-License-Identifier: AGPL-3.0-or-later
//! The template export of every reference graphic matches FRAME's
//! `template-export.json`.

mod common;

use framecraft_core::export::template::{ExportOptions, build};
use framecraft_core::model::Graphic;
use framecraft_core::normalize::normalize;
use framecraft_core::profile::Profile;

#[test]
fn template_export_matches_frame() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let graphics = common::graphics(&expected);
    let profile = Profile::frame();
    let mut failures = Vec::new();
    for (name, dir) in &graphics {
        let want = common::read_json(&dir.join("template-export.json"));
        let graphic =
            Graphic::from_json(&std::fs::read_to_string(dir.join("graphic.json")).unwrap())
                .unwrap();
        let opts = ExportOptions {
            // The id is a slug of the graphic name, which the reference
            // folder does not keep; take it from FRAME's output.
            template_id: want["templateId"].as_str().unwrap().to_string(),
            palette_colors: Vec::new(),
        };
        let got =
            serde_json::to_value(build(&normalize(&graphic, &profile), &opts, &profile)).unwrap();
        let diffs = common::json_diffs(&want, &got);
        if !diffs.is_empty() {
            let shown: Vec<&String> = diffs.iter().take(25).collect();
            let more = if diffs.len() > 25 {
                format!("\n  … {} more", diffs.len() - 25)
            } else {
                String::new()
            };
            failures.push(format!(
                "{name}: {} differences\n  {}{more}",
                diffs.len(),
                shown
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ));
        }
    }
    println!("{} reference graphics checked", graphics.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
