// SPDX-License-Identifier: AGPL-3.0-or-later
//! The SPX, OGraf and banner HTML of every reference graphic matches FRAME.
//!
//! Every line is compared byte for byte, except the line holding
//! `window.__ionframeSpx`: FRAME copies some input objects into it with
//! their original key order, so that object is compared as JSON values.

mod common;

use framecraft_core::export::assets::{AssetMap, font_faces};
use framecraft_core::export::html::{PageKind, page_html};
use framecraft_core::export::template::{ExportOptions, TemplateExport, build};
use framecraft_core::model::Graphic;
use framecraft_core::normalize::normalize;
use framecraft_core::profile::Profile;
use std::path::Path;

fn entry_html(dir: &Path) -> String {
    let files = common::read_json(&dir.join("files.json"));
    files
        .as_object()
        .unwrap()
        .keys()
        .find(|k| k.ends_with(".html") && !k.contains('/'))
        .unwrap()
        .clone()
}

fn template_js_src(t: &TemplateExport) -> Option<&'static str> {
    t.template_js
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|_| "js/template.js")
}

#[test]
fn html_matches_frame() {
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

        let spx = AssetMap::spx().remap(&t);
        let (spx_fonts, _) = font_faces(&spx, &[], false, &|_| true);
        let spx_html = page_html(&spx, &spx_fonts, PageKind::Spx, template_js_src(&spx), &p);
        let banner = AssetMap::banner().remap(&t);
        let (banner_fonts, _) = font_faces(&banner, &[], true, &|_| true);
        let banner_html = page_html(
            &banner,
            &banner_fonts,
            PageKind::Banner,
            template_js_src(&banner),
            &p,
        );

        let entry = entry_html(&dir.join("spx"));
        for (file, got) in [
            (format!("spx/{entry}"), &spx_html),
            (format!("ograf/{entry}"), &spx_html),
            (format!("banner/{entry}"), &banner_html),
        ] {
            let want = std::fs::read_to_string(dir.join(&file)).unwrap();
            checked += 1;
            let diffs = common::html_diffs(&want, got);
            if !diffs.is_empty() {
                failures.push(format!("{name}/{file}:\n  {}", diffs.join("\n  ")));
            }
        }
    }
    println!(
        "{checked} HTML files compared across {} graphics",
        graphics.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
