// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package metadata matches FRAME byte for byte: the Air render manifest
//! and export info of all three formats, the OGraf manifest and the OGraf
//! component.

mod common;

use framecraft_core::export::manifest::{
    AirTemplate, ExportInfoKind, PackageFormat, air_render_manifest, export_info,
    ograf_component_source, ograf_manifest, ograf_step_count,
};
use framecraft_core::export::names::file_base_name;
use framecraft_core::export::template::{ExportOptions, build};
use framecraft_core::js::stringify_pretty;
use framecraft_core::model::Graphic;
use framecraft_core::normalize::normalize;
use framecraft_core::profile::Profile;
use std::path::Path;

/// The reference generator's fixed clock.
const EXPORTED_AT: &str = "2026-01-01T12:00:00.000Z";

/// First line where two texts differ, for a readable failure.
fn first_difference(want: &str, got: &str) -> Option<String> {
    if want == got {
        return None;
    }
    for (i, (w, g)) in want.lines().zip(got.lines()).enumerate() {
        if w != g {
            return Some(format!("line {}:\n      want: {w}\n      got:  {g}", i + 1));
        }
    }
    Some(format!(
        "{} lines vs {} lines (trailing newline: {} vs {})",
        want.lines().count(),
        got.lines().count(),
        want.ends_with('\n'),
        got.ends_with('\n')
    ))
}

/// The entry HTML file at the root of a package.
fn entry_html(dir: &Path) -> String {
    let files = common::read_json(&dir.join("files.json"));
    let names: Vec<&String> = files
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.ends_with(".html") && !k.contains('/'))
        .collect();
    assert_eq!(names.len(), 1, "one root .html in {}", dir.display());
    names[0].clone()
}

#[test]
fn metadata_matches_frame_byte_for_byte() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let graphics = common::graphics(&expected);
    let p = Profile::frame();
    let mut failures = Vec::new();
    let mut checked = 0;

    for (name, dir) in &graphics {
        let want_export = common::read_json(&dir.join("template-export.json"));
        let graphic =
            Graphic::from_json(&std::fs::read_to_string(dir.join("graphic.json")).unwrap())
                .unwrap();
        let opts = ExportOptions {
            template_id: want_export["templateId"].as_str().unwrap().to_string(),
            palette_colors: Vec::new(),
        };
        let t = build(&normalize(&graphic, &p), &opts, &p);

        let html = entry_html(&dir.join("spx"));
        let base = file_base_name(
            if t.spx_definition.description.is_empty() {
                &t.template_id
            } else {
                &t.spx_definition.description
            },
            if t.template_id.is_empty() {
                "template"
            } else {
                &t.template_id
            },
        );
        let (ograf_json, ograf_mjs) = (format!("{base}.ograf.json"), format!("{base}.mjs"));
        let air = |format, main: Option<&str>, entry: &str| {
            let tpl = AirTemplate {
                payload: &t,
                entry_html: entry,
                name: None,
                ograf_main: main,
            };
            stringify_pretty(&air_render_manifest(format, &[tpl], None, EXPORTED_AT, &p))
        };

        let produced: Vec<(String, String)> = vec![
            (
                "spx/meta/air-render.json".into(),
                air(PackageFormat::SpxDraft, None, &html),
            ),
            (
                "spx/meta/export-info.json".into(),
                stringify_pretty(&export_info(
                    &t,
                    ExportInfoKind::Spx,
                    &html,
                    EXPORTED_AT,
                    &p,
                )),
            ),
            (
                "ograf/meta/air-render.json".into(),
                air(PackageFormat::Ograf, Some(&ograf_mjs), &html),
            ),
            (
                "ograf/meta/export-info.json".into(),
                stringify_pretty(&export_info(
                    &t,
                    ExportInfoKind::Ograf {
                        manifest: &ograf_json,
                        main: &ograf_mjs,
                    },
                    &html,
                    EXPORTED_AT,
                    &p,
                )),
            ),
            (
                format!("ograf/{ograf_json}"),
                stringify_pretty(&ograf_manifest(&t, &ograf_mjs, &html, None, &p)),
            ),
            (
                format!("ograf/{ograf_mjs}"),
                ograf_component_source(&html, ograf_step_count(&t)),
            ),
            (
                "banner/meta/air-render.json".into(),
                air(PackageFormat::HtmlBanner, None, &html),
            ),
            (
                "banner/meta/export-info.json".into(),
                stringify_pretty(&export_info(
                    &t,
                    ExportInfoKind::Banner,
                    &html,
                    EXPORTED_AT,
                    &p,
                )),
            ),
        ];

        for (file, got) in produced {
            let path = dir.join(&file);
            let Ok(want) = std::fs::read_to_string(&path) else {
                failures.push(format!(
                    "{name}/{file}: FRAME has no such file (name mismatch?)"
                ));
                continue;
            };
            checked += 1;
            if let Some(diff) = first_difference(&want, &got) {
                failures.push(format!("{name}/{file}: {diff}"));
            }
        }
    }

    println!(
        "{checked} files compared across {} graphics",
        graphics.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
