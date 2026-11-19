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

const DATA_PREFIX: &str = "    window.__ionframeSpx = ";

fn compare(want: &str, got: &str) -> Vec<String> {
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
            let diffs = common::json_diffs(&parse(wj), &parse(gj));
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
        let (spx_fonts, _) = font_faces(&spx, &[], false);
        let spx_html = page_html(&spx, &spx_fonts, PageKind::Spx, template_js_src(&spx), &p);
        let banner = AssetMap::banner().remap(&t);
        let (banner_fonts, _) = font_faces(&banner, &[], true);
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
            ("banner/index.html".to_string(), &banner_html),
        ] {
            let want = std::fs::read_to_string(dir.join(&file)).unwrap();
            checked += 1;
            let diffs = compare(&want, got);
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
