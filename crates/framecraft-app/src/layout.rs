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
