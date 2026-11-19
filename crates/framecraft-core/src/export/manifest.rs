// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package metadata: `meta/air-render.json` (MENiAC Air offline render),
//! the OGraf manifest and component, and `meta/export-info.json`.
//!
//! Key order and number format match FRAME's output exactly; write these
//! with [`crate::js::stringify_pretty`].

use super::names::{ograf_element_id, ograf_element_version};
use super::template::TemplateExport;
use crate::js;
use crate::model::CueKind;
use crate::profile::Profile;
use serde_json::{Map, Value, json};

/// Air render manifest schema id.
pub const AIR_SCHEMA: &str = "ionframe-air-render-manifest/v1";
pub const OGRAF_SCHEMA_URL: &str =
    "https://ograf.ebu.io/v1/specification/json-schemas/graphics/schema.json";
/// Where packages keep the Air manifest.
pub const AIR_MANIFEST_PATH: &str = "meta/air-render.json";

/// Package kind recorded in the Air manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    SpxDraft,
    SpxDraftSuite,
    HtmlBanner,
    Ograf,
    OgrafSuite,
}

impl PackageFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpxDraft => "spx-draft",
            Self::SpxDraftSuite => "spx-draft-suite",
            Self::HtmlBanner => "html-banner",
            Self::Ograf => "ograf",
            Self::OgrafSuite => "ograf-suite",
        }
    }
}

fn duration_ms(t: &TemplateExport) -> f64 {
    js::round(js::or(Some(t.scene.duration_ms), 1000.0)).max(1.0)
}

fn fps(t: &TemplateExport) -> f64 {
    js::round(js::or(Some(t.scene.fps), 25.0)).max(1.0)
}

/// Pause cue times (with delay) strictly inside the scene, sorted, unique.
pub fn pause_times_ms(t: &TemplateExport) -> Vec<f64> {
    let duration = duration_ms(t);
    let mut times: Vec<f64> = Vec::new();
    for cue in t.cues.iter().filter(|c| c.kind == CueKind::Pause) {
        let delay = js::or(cue.delay_ms, 0.0).max(0.0);
        let at = js::round(cue.time + delay).max(0.0);
        if at > 0.0 && at < duration && !times.contains(&at) {
            times.push(at);
        }
    }
    times.sort_by(f64::total_cmp);
    times
}

/// OGraf steps: 0 for an auto-out graphic without pauses, 1 for play and
/// stop, one more per pause.
pub fn ograf_step_count(t: &TemplateExport) -> usize {
    let pauses = pause_times_ms(t).len();
    let out = js::trim(&t.spx_definition.out).to_lowercase();
    match pauses {
        0 if out == "none" || out == "0" => 0,
        0 => 1,
        n => n + 1,
    }
}

/// `{id}-v{version}`, the custom element name SPX and Air register.
pub fn ograf_custom_element_tag(t: &TemplateExport) -> String {
    format!(
        "{}-v{}",
        ograf_element_id(&t.template_id, "ionframe-template"),
        ograf_element_version(&t.version, "0-9-0")
    )
}

fn cue_kind(k: CueKind) -> &'static str {
    match k {
        CueKind::Pause => "pause",
        CueKind::ApiCall => "api_call",
        CueKind::Script => "script",
    }
}

/// Motion ranges from 0 through each pause to the end.
fn air_segments(duration: f64, pauses: &[f64]) -> Vec<Value> {
    let mut edges = vec![0.0];
    edges.extend(pauses.iter().copied().filter(|t| *t > 0.0 && *t < duration));
    edges.push(duration);
    edges.dedup();
    edges
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            json!({
                "index": i,
                "startMs": w[0],
                "endMs": w[1],
                "endKind": if w[1] >= duration { "end" } else { "pause" },
            })
        })
        .collect()
}

/// One template in an Air render manifest.
pub struct AirTemplate<'a> {
    pub payload: &'a TemplateExport,
    pub entry_html: &'a str,
    /// Display name in Air's template picker; defaults to the description.
    pub name: Option<&'a str>,
    /// OGraf component file, for OGraf packages.
    pub ograf_main: Option<&'a str>,
}

fn air_template(a: &AirTemplate, p: &Profile) -> Value {
    let t = a.payload;
    let duration = duration_ms(t);
    let pauses = pause_times_ms(t);

    let cues: Vec<Value> = t
        .cues
        .iter()
        .map(|cue| {
            let delay = js::or(cue.delay_ms, 0.0).max(0.0);
            let time = js::round(js::or(Some(cue.time), 0.0)).max(0.0);
            let mut c = json!({
                "id": cue.id,
                "type": cue_kind(cue.kind),
                "timeMs": time,
                "delayMs": delay,
                "effectiveTimeMs": time + delay,
            });
            if let Some(label) = cue.label.as_deref().filter(|l| !l.is_empty()) {
                c["label"] = label.into();
            }
            if let Some(f) = cue
                .script_fn
                .as_deref()
                .map(js::trim)
                .filter(|f| !f.is_empty())
            {
                c["scriptFn"] = f.into();
            }
            c
        })
        .collect();

    let mut fields: Vec<Value> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut push = |id: &str, label: &str, default: &str| {
        if !id.is_empty() && !seen.iter().any(|s| s == id) {
            seen.push(id.to_string());
            fields.push(json!({ "id": id, "label": label, "type": "text", "default": default }));
        }
    };
    let hook = &t.ionboard_hook;
    if hook.enabled {
        // Default first, then trim: a blank-but-set id is skipped, as in FRAME.
        let or_default = |v: &str, d: &'static str| {
            if v.is_empty() {
                d.to_string()
            } else {
                js::trim(v).to_string()
            }
        };
        let magic = or_default(&hook.magic_id_field, "ionboard_magic_id");
        let project = or_default(&hook.project_id_field, "ionboard_project_id");
        push(
            &magic,
            &format!("{} Magic ID", p.board_name),
            &hook.magic_id_default,
        );
        push(
            &project,
            &format!("{} Project ID", p.board_name),
            &hook.project_id_default,
        );
    }
    for f in &t.fields {
        push(
            &f.id,
            if f.label.is_empty() { &f.id } else { &f.label },
            &f.default,
        );
    }

    let mut runtime = json!({
        "api": "ionframe-spx",
        "seekGlobal": "goToTime",
        "postMessageType": "ionframe-spx",
        "bridgeGlobal": "__ionframeAir",
    });
    if let Some(main) = a.ograf_main.filter(|m| !m.is_empty()) {
        runtime["ografMain"] = main.into();
        runtime["ografElementId"] = ograf_custom_element_tag(t).into();
        runtime["supportsNonRealTime"] = true.into();
    }
    let mut hints =
        json!({ "pauseAsStill": true, "stillAt": "segmentEnd", "motionBetweenPauses": true });
    if t.elements.iter().any(|e| e.kind == "video") {
        hints["experimentalVideo"] = true.into();
    }
    if t.elements.iter().any(|e| e.kind == "lottie") {
        hints["hasLottie"] = true.into();
    }

    let name = js::trim(a.name.unwrap_or(&t.spx_definition.description));
    json!({
        "id": t.template_id,
        "name": if name.is_empty() { t.template_id.as_str() } else { name },
        "entryHtml": a.entry_html,
        "width": js::round(js::or(Some(t.scene.width), 1920.0)).max(1.0),
        "height": js::round(js::or(Some(t.scene.height), 1080.0)).max(1.0),
        "fps": fps(t),
        "durationMs": duration,
        "alpha": true,
        "runtime": runtime,
        "fields": fields,
        "cues": cues,
        "pausePointsMs": pauses,
        "segments": air_segments(duration, &pauses),
        "renderHints": hints,
    })
}

/// `meta/air-render.json`.
pub fn air_render_manifest(
    format: PackageFormat,
    templates: &[AirTemplate],
    project_name: Option<&str>,
    exported_at: &str,
    p: &Profile,
) -> Value {
    let mut m = Map::new();
    m.insert("$schema".into(), AIR_SCHEMA.into());
    m.insert("version".into(), 1.into());
    m.insert("generator".into(), p.generator.clone().into());
    m.insert("packageFormat".into(), format.as_str().into());
    m.insert("exportedAt".into(), exported_at.into());
    if let Some(name) = project_name.filter(|n| !n.is_empty()) {
        m.insert("projectName".into(), name.into());
    }
    m.insert(
        "templates".into(),
        templates.iter().map(|a| air_template(a, p)).collect(),
    );
    Value::Object(m)
}

/// OGraf schema properties: a rundown nickname, BOARD ids when the hook is
/// on, then one property per field.
fn ograf_properties(t: &TemplateExport, p: &Profile) -> Value {
    let mut props = Map::new();
    props.insert(
        "comment".into(),
        json!({ "type": "string", "title": "Nickname of this item on the rundown", "default": "[ Item nickname ]" }),
    );
    let hook = &t.ionboard_hook;
    if hook.enabled {
        let magic = if hook.magic_id_field.is_empty() {
            "ionboard_magic_id"
        } else {
            &hook.magic_id_field
        };
        let project = if hook.project_id_field.is_empty() {
            "ionboard_project_id"
        } else {
            &hook.project_id_field
        };
        props.insert(
            magic.into(),
            json!({ "type": "string", "title": format!("{} Magic ID", p.board_name), "default": hook.magic_id_default }),
        );
        props.insert(
            project.into(),
            json!({ "type": "string", "title": format!("{} Project ID", p.board_name), "default": hook.project_id_default }),
        );
    }
    for f in &t.fields {
        let title = if f.label.is_empty() { &f.id } else { &f.label };
        let v_spx = if f.kind == "filelist" {
            json!({
                "ftype": "filelist",
                "assetfolder": f.assetfolder.as_deref().filter(|s| !s.is_empty()).unwrap_or("./images/"),
                "extension": f.extension.as_deref().filter(|s| !s.is_empty()).unwrap_or("png"),
            })
        } else {
            json!({ "ftype": "textfield" })
        };
        // Re-inserting an existing key keeps its first position, as in JavaScript.
        props.insert(
            f.id.clone(),
            json!({ "type": "string", "title": title, "default": f.default, "v_spx": v_spx }),
        );
    }
    Value::Object(props)
}

/// The `*.ograf.json` manifest.
pub fn ograf_manifest(
    t: &TemplateExport,
    main_file: &str,
    entry_html: &str,
    name: Option<&str>,
    p: &Profile,
) -> Value {
    let d = &t.spx_definition;
    let trimmed_name = js::trim(name.unwrap_or(&d.description));
    let step_count = ograf_step_count(t);
    let version = if t.version.is_empty() {
        &p.version
    } else {
        &t.version
    };

    let prevent_toggle = d.prevent_toggle;
    let (mut playlayer, mut webplayout) = (d.playlayer.clone(), d.webplayout.clone());
    if prevent_toggle && (webplayout == "1" || playlayer == "1") {
        playlayer = "2".into();
        webplayout = "2".into();
    }

    let pauses = pause_times_ms(t);
    let total = duration_ms(t);
    let in_duration = pauses.first().copied().unwrap_or(total);
    let out_duration = pauses.last().map_or(0.0, |last| total - last);

    let mut m = Map::new();
    m.insert("$schema".into(), OGRAF_SCHEMA_URL.into());
    m.insert(
        "id".into(),
        ograf_element_id(&t.template_id, "ionframe-template").into(),
    );
    m.insert(
        "version".into(),
        ograf_element_version(version, "0-9-0").into(),
    );
    m.insert(
        "name".into(),
        (if trimmed_name.is_empty() {
            t.template_id.as_str()
        } else {
            trimmed_name
        })
        .into(),
    );
    m.insert("description".into(), d.description.clone().into());
    m.insert("author".into(), json!({ "name": p.generator }));
    m.insert("main".into(), main_file.into());
    m.insert("supportsRealTime".into(), true.into());
    m.insert("supportsNonRealTime".into(), true.into());
    if step_count != 1 {
        m.insert("stepCount".into(), step_count.into());
    }
    m.insert(
        "schema".into(),
        json!({ "type": "object", "properties": ograf_properties(t, p) }),
    );
    m.insert(
        "v_spx".into(),
        json!({
            "playserver": d.playserver,
            "playchannel": d.playchannel,
            "playlayer": playlayer,
            "webplayout": webplayout,
            "out": d.out,
            "uicolor": d.uicolor,
            "preventToggle": prevent_toggle,
            // SPX's OGraf check: supportsNonRealTime needs these. The runtime
            // plays 0 → first pause on Play and last pause → end on Stop.
            "inDuration": in_duration,
            "outDuration": out_duration,
        }),
    );
    m.insert(
        "v_ionframe".into(),
        json!({
            "templateId": t.template_id,
            "entryHtml": entry_html,
            "durationMs": duration_ms(t),
            "fps": fps(t),
            "airRenderManifest": AIR_MANIFEST_PATH,
        }),
    );
    Value::Object(m)
}

const OGRAF_COMPONENT: &str = include_str!("ograf_component.mjs.in");

/// The OGraf Web Component (`*.mjs`). It loads the entry HTML in an iframe
/// and forwards play, stop, steps and `goToTime` to it.
pub fn ograf_component_source(entry_html: &str, step_count: usize) -> String {
    let entry = if entry_html.is_empty() {
        "template.html"
    } else {
        entry_html
    };
    let entry = entry.strip_prefix('.').unwrap_or(entry);
    let entry = entry.strip_prefix('/').unwrap_or(entry);
    let multi = step_count >= 2;
    let when = |s: &'static str| if multi { s } else { "" };
    let entry_json = serde_json::to_string(entry).expect("strings serialize");
    let play_body = if multi {
        "      if (this.step > 0) {\n        this.iframe.contentWindow.next();\n      } else {\n        this.iframe.contentWindow.play();\n      }\n      this.step++;\n      return { statusCode: 200, currentStep: this.step };"
    } else {
        "      this.iframe.contentWindow.play();\n      return { statusCode: 200 };"
    };
    let custom_body = if multi {
        "    if (this.iframe && this.iframe.contentWindow && params && params.id === \"next\") {\n      this.iframe.contentWindow.next();\n    }"
    } else {
        "    // No custom actions"
    };
    js::fill_markers(
        OGRAF_COMPONENT,
        &[
            ("ENTRY_JSON", &entry_json),
            ("STEP_RESET_4", when("    this.step = 0;\n")),
            ("STEP_RESET_6", when("      this.step = 0;\n")),
            ("CURRENT_STEP", when(", currentStep: this.step")),
            ("STEP_LOG", when(", { step: this.step }")),
            ("PLAY_BODY", play_body),
            ("CUSTOM_BODY", custom_body),
        ],
    )
}

/// What an `export-info.json` describes.
pub enum ExportInfoKind<'a> {
    Spx,
    Ograf { manifest: &'a str, main: &'a str },
    Banner,
}

/// `meta/export-info.json`.
pub fn export_info(
    t: &TemplateExport,
    kind: ExportInfoKind,
    html_file: &str,
    exported_at: &str,
    p: &Profile,
) -> Value {
    let profile = match kind {
        ExportInfoKind::Spx => "spx-draft-esm",
        ExportInfoKind::Ograf { .. } => "ograf-v1",
        ExportInfoKind::Banner => "html-banner",
    };
    let mut m = Map::new();
    m.insert("generator".into(), p.generator.clone().into());
    m.insert("profile".into(), profile.into());
    m.insert("version".into(), t.version.clone().into());
    m.insert("exportedAt".into(), exported_at.into());
    m.insert("templateId".into(), t.template_id.clone().into());
    m.insert("htmlFileName".into(), html_file.into());
    match kind {
        ExportInfoKind::Ograf { manifest, main } => {
            m.insert("ografManifest".into(), manifest.into());
            m.insert("ografMain".into(), main.into());
        }
        ExportInfoKind::Banner => {
            m.insert("width".into(), t.scene.width.into());
            m.insert("height".into(), t.scene.height.into());
        }
        ExportInfoKind::Spx => {}
    }
    m.insert("airRenderManifest".into(), AIR_MANIFEST_PATH.into());
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_split_at_pauses() {
        let s = air_segments(3000.0, &[1000.0, 2000.0]);
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[1],
            json!({ "index": 1, "startMs": 1000.0, "endMs": 2000.0, "endKind": "pause" })
        );
        assert_eq!(s[2]["endKind"], "end");
    }

    #[test]
    fn component_variants() {
        let single = ograf_component_source("./Name Tag.html", 1);
        assert!(single.contains("import.meta.resolve(\"./\" + \"Name Tag.html\")"));
        assert!(single.contains("// No custom actions"));
        assert!(!single.contains("this.step"));
        let multi = ograf_component_source("x.html", 3);
        assert!(multi.contains("this.iframe.contentWindow.next();"));
        assert!(!multi.contains('@'));
    }
}
