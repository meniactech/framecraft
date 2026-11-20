// SPDX-License-Identifier: AGPL-3.0-or-later
//! Panel sizes: the side column's width, the timeline's height and the
//! outliner's height. The bridge drags the splitters (CSS variables on the
//! page root); the sizes are saved in the data folder and restored at start.

use dioxus::prelude::*;
use serde_json::{Map, Value};

/// CSS variables a splitter sets, by splitter name.
pub const VARS: [(&str, &str); 3] = [
    ("side", "--side-w"),
    ("timeline", "--tl-h"),
    ("outliner", "--outliner-h"),
];

fn path() -> Option<std::path::PathBuf> {
    crate::project::data_dir().map(|d| d.join("layout.json"))
}

/// Saved sizes (px) by splitter name.
pub fn load() -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(path()?).ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

/// Sets the saved sizes on the page.
pub fn apply(layout: &Map<String, Value>) {
    let mut js = String::from("const r = document.documentElement.style;");
    for (name, var) in VARS {
        if let Some(px) = layout.get(name).and_then(Value::as_f64) {
            js.push_str(&format!("r.setProperty('{var}', '{px}px');"));
        }
    }
    let _ = document::eval(&js);
}

/// A splitter was released (`px`) or reset (`None`): saves the sizes.
pub fn on_event(ev: &Value) {
    let Some(name) = ev["split"].as_str() else {
        return;
    };
    if !VARS.iter().any(|(n, _)| *n == name) {
        return;
    }
    let mut layout = load().unwrap_or_default();
    match ev["px"].as_f64() {
        Some(px) => {
            layout.insert(name.to_string(), Value::from(px.round()));
        }
        None => {
            layout.remove(name);
        }
    }
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            p,
            serde_json::to_vec_pretty(&Value::Object(layout)).unwrap_or_default(),
        );
    }
}

/// The guide toggles from the last session.
pub fn load_guides() -> framecraft_core::manipulate::GuideToggles {
    let mut g = framecraft_core::manipulate::GuideToggles::default();
    if let Some(saved) = load().and_then(|l| l.get("guides").cloned()) {
        let b = |k: &str, d: bool| saved.get(k).and_then(Value::as_bool).unwrap_or(d);
        g = framecraft_core::manipulate::GuideToggles {
            safe10: b("safe10", g.safe10),
            safe20: b("safe20", g.safe20),
            center: b("center", g.center),
            thirds: b("thirds", g.thirds),
            rulers: b("rulers", g.rulers),
            snap: b("snap", g.snap),
        };
    }
    g
}

/// Saves the guide toggles with the panel sizes.
pub fn save_guides(g: &framecraft_core::manipulate::GuideToggles) {
    let mut layout = load().unwrap_or_default();
    layout.insert(
        "guides".into(),
        serde_json::json!({
            "safe10": g.safe10, "safe20": g.safe20, "center": g.center,
            "thirds": g.thirds, "rulers": g.rulers, "snap": g.snap,
        }),
    );
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            p,
            serde_json::to_vec_pretty(&Value::Object(layout)).unwrap_or_default(),
        );
    }
}

/// The time display format (FRAME's: shared by the timeline and Scene).
pub fn load_time_format() -> framecraft_core::timecode::Format {
    load()
        .and_then(|l| {
            l.get("timeFormat")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .map_or_else(Default::default, |id| {
            framecraft_core::timecode::Format::from_id(&id)
        })
}

/// Saves the time display format with the panel sizes.
pub fn save_time_format(f: framecraft_core::timecode::Format) {
    let mut layout = load().unwrap_or_default();
    layout.insert("timeFormat".into(), Value::from(f.id()));
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            p,
            serde_json::to_vec_pretty(&Value::Object(layout)).unwrap_or_default(),
        );
    }
}

/// One saved setting (data folder's layout.json).
pub fn setting(key: &str) -> Option<Value> {
    load().and_then(|l| l.get(key).cloned())
}

/// Saves one setting with the others.
pub fn set_setting(key: &str, value: Value) {
    let mut layout = load().unwrap_or_default();
    layout.insert(key.into(), value);
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            p,
            serde_json::to_vec_pretty(&Value::Object(layout)).unwrap_or_default(),
        );
    }
}
