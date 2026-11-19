// SPDX-License-Identifier: AGPL-3.0-or-later
//! Timeline evaluation matches the playout runtime: every animated lane of
//! every reference graphic, sampled by the runtime's own `interpolateTrack`
//! (FRAME's `reference/generate-poses.mjs`).

mod common;

use framecraft_core::model::Graphic;
use framecraft_core::normalize::normalize;
use framecraft_core::profile::Profile;
use framecraft_core::timeline::lane_value;

#[test]
fn lane_values_match_the_runtime() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let p = Profile::frame();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, dir) in common::graphics(&expected) {
        let poses_file = dir.join("poses.json");
        if !poses_file.is_file() {
            failures.push(format!(
                "{name}: no poses.json (run reference/generate-poses.mjs)"
            ));
            continue;
        }
        let doc = normalize(
            &Graphic::from_json(&std::fs::read_to_string(dir.join("graphic.json")).unwrap())
                .unwrap(),
            &p,
        );
        let poses = common::read_json(&poses_file);
        for (item_id, lanes) in poses.as_object().unwrap() {
            for (lane, rows) in lanes.as_object().unwrap() {
                for row in rows.as_array().unwrap() {
                    let (t, want) = (row[0].as_f64().unwrap(), row[1].as_f64());
                    let got = lane_value(&doc, item_id, lane, t);
                    checked += 1;
                    let ok = match (want, got) {
                        (Some(w), Some(g)) => (w - g).abs() <= 1e-9 * w.abs().max(1.0),
                        (None, None) => true,
                        _ => false,
                    };
                    if !ok && failures.len() < 20 {
                        failures.push(format!("{name}: {item_id} {lane} at {t} ms: runtime {want:?}, FrameCraft {got:?}"));
                    }
                }
            }
        }
    }
    println!("{checked} animated values compared");
    assert!(checked > 0);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
