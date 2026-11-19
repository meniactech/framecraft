// SPDX-License-Identifier: AGPL-3.0-or-later
//! The properties panel: tabs and fields drawn from the core's property
//! schema (`framecraft_core::schema`). Every edit is one command.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::color;
use framecraft_core::commands::SetKeyframe;
use framecraft_core::js::number_to_string;
use framecraft_core::schema::{self, Kind, Property, Scope, Tab};
use framecraft_core::timeline::lane_value;
use serde_json::{Value, json};

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// What an unset number means (FRAME's defaults).
fn number_default(path: &str) -> Option<f64> {
    match path {
        "scaleX" | "scaleY" | "opacity" => Some(1.0),
        "pivotX" | "pivotY" => Some(50.0),
        p if p.starts_with("shadow.") || p.starts_with("borderRadius.") => Some(0.0),
        "borderWidth" | "gradientAngle" => Some(0.0),
        "chartReveal" => Some(1.0),
        "chartCutout" => Some(0.0),
        "chartBarThickness" => Some(80.0),
        "chartLineWidth" => Some(2.0),
        "chartPointRadius" => Some(4.0),
        "chartLineTension" => Some(0.2),
        "qrMargin" => Some(2.0),
        _ => None,
    }
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map(number_to_string).unwrap_or_default(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Fill fields that only matter for one fill type.
fn shown(p: &Property, item: &Value) -> bool {
    let fill = item["backgroundType"].as_str().unwrap_or("solid");
    match p.path {
        "bg_color" => fill == "solid",
        p if p.starts_with("gradient") => fill == "gradient",
        p if p.starts_with("bgImage") => fill == "image",
        _ => true,
    }
}

#[derive(Clone, PartialEq)]
struct Field {
    prop: &'static Property,
    value: Value,
    /// Animated at the playhead (the value shown is the animated one).
    animated: bool,
}

#[component]
pub fn Properties(ui: Ui) -> Element {
    let time = (ui.time)();
    let editor = ui.editor.read();
    let Some(ed) = editor.as_ref() else {
        return rsx! {
            div { class: "properties",
                p { class: "dim", "Nothing open" }
            }
        };
    };
    let doc = ed.doc();
    let item = ed
        .selected
        .as_ref()
        .and_then(|id| doc.items.iter().find(|i| &i.id == id));
    let mut tabs = vec![Tab::Scene];
    if let Some(i) = item {
        tabs.extend(Tab::for_item(i));
    }
    let wanted = (ui.tab)();
    let tab = if tabs.contains(&wanted) {
        wanted
    } else if item.is_some() {
        Tab::Transform
    } else {
        Tab::Scene
    };
    let item_json = item.map_or(Value::Null, |i| serde_json::to_value(i).unwrap_or_default());
    let scene_json = serde_json::to_value(&doc.scene).unwrap_or_default();
    let timeline_json = serde_json::to_value(&doc.timeline).unwrap_or_default();
    let fields: Vec<Field> = Property::of(tab)
        .filter(|p| p.applies(if tab == Tab::Scene { None } else { item }))
        .filter(|p| p.scope != Scope::Item || shown(p, &item_json))
        .map(|p| {
            let root = match p.scope {
                Scope::Item => &item_json,
                Scope::Scene => &scene_json,
                Scope::Timeline => &timeline_json,
            };
            let mut value = schema::lookup(root, p.path);
            let mut animated = false;
            if let (Some(lane), Some(i)) = (p.lane, item)
                && let Some(v) = lane_value(doc, &i.id, lane, time)
            {
                value = Value::from(round3(v));
                animated = true;
            }
            Field {
                prop: p,
                value,
                animated,
            }
        })
        .collect();
    let id = item.map(|i| i.id.clone());
    let heading = match item {
        Some(i) if tab != Tab::Scene => i.name.clone(),
        _ => doc.scene.name.clone(),
    };
    drop(editor);

    rsx! {
        div { class: "properties",
            div { class: "tabs",
                for t in tabs {
                    button {
                        class: if t == tab { "tab active" } else { "tab" },
                        onclick: move |_| {
                            let mut current = ui.tab;
                            current.set(t);
                        },
                        "{t.label()}"
                    }
                }
            }
            p { class: "layer-name", "{heading}" }
            if tab != Tab::Scene && id.is_none() {
                p { class: "dim", "Select a layer" }
            }
            for (i , f) in fields.iter().enumerate() {
                if i == 0 || fields[i - 1].prop.group != f.prop.group {
                    h3 { "{f.prop.group}" }
                }
                FieldRow { ui, field: f.clone(), item: id.clone() }
            }
            datalist { id: "fc-fonts",
                for family in schema::builtin_font_families() {
                    option { value: "{family}" }
                }
            }
        }
    }
}

#[component]
fn FieldRow(ui: Ui, field: Field, item: Option<String>) -> Element {
    let prop = field.prop;
    let target = item.clone();
    let set = use_callback(move |v: Value| {
        let cmd = prop.command(target.as_deref(), v, *ui.time.peek(), *ui.auto_key.peek());
        match cmd {
            Some(cmd) => ui.execute(cmd.as_ref()),
            None => {
                let mut status = ui.status;
                status.set(format!("{}: not a valid value", prop.label));
            }
        }
    });
    let text = as_text(&field.value);

    let input = match prop.kind {
        Kind::Number { step, .. } => {
            let shown = match field.value.as_f64().or_else(|| number_default(prop.path)) {
                Some(v) => number_to_string(round3(v)),
                None => String::new(),
            };
            rsx! {
                input {
                    r#type: "number",
                    step: "{step}",
                    value: "{shown}",
                    onchange: move |evt| set(Value::String(evt.value())),
                }
            }
        }
        Kind::Color => {
            let hex = color::parse(&text).map_or("#000000".to_string(), |c| c.hex());
            let current = text.clone();
            rsx! {
                div { class: "color-field",
                    input {
                        r#type: "color",
                        value: "{hex}",
                        onchange: move |evt| {
                            if let Some(c) = color::with_rgb(&current, &evt.value()) {
                                set(Value::from(c));
                            }
                        },
                    }
                    input {
                        r#type: "text",
                        value: "{text}",
                        onchange: move |evt| set(Value::from(evt.value().trim())),
                    }
                }
            }
        }
        Kind::Choice(options) => {
            let mut options: Vec<(String, String)> = options
                .iter()
                .map(|(v, l)| (v.to_string(), l.to_string()))
                .collect();
            if !text.is_empty() && !options.iter().any(|(v, _)| *v == text) {
                options.push((text.clone(), text.clone()));
            }
            rsx! {
                select { onchange: move |evt| set(Value::from(evt.value())),
                    for (v , l) in options {
                        option { value: "{v}", selected: v == text, "{l}" }
                    }
                }
            }
        }
        Kind::Toggle => {
            // Unset chart toggles are on in FRAME.
            // Animated toggles read as numbers (on from 0.5); unset chart and
            // emitter toggles are on in FRAME.
            let on = field
                .value
                .as_bool()
                .or_else(|| field.value.as_f64().map(|v| v >= 0.5))
                .unwrap_or(matches!(
                    prop.path,
                    "chartShowLegend"
                        | "chartShowValues"
                        | "particleEmitting"
                        | "lightningEmitting"
                ));
            rsx! {
                input {
                    r#type: "checkbox",
                    checked: on,
                    onchange: move |_| set(Value::from(!on)),
                }
            }
        }
        Kind::TextArea => rsx! {
            textarea { value: "{text}", onchange: move |evt| set(Value::from(evt.value())) }
        },
        Kind::Font => rsx! {
            input {
                r#type: "text",
                list: "fc-fonts",
                value: "{text}",
                onchange: move |evt| set(Value::from(evt.value().trim())),
            }
        },
        Kind::Text | Kind::TextOrNumber => rsx! {
            input { r#type: "text", value: "{text}", onchange: move |evt| set(Value::from(evt.value())) }
        },
        Kind::Ramp => rsx! {
            Ramp { value: field.value.clone(), set }
        },
        Kind::ChartPoints => rsx! {
            ChartPoints { ui, value: field.value.clone(), set }
        },
        Kind::ValueRamp => rsx! {
            ValueRamp { value: field.value.clone(), set }
        },
        Kind::ColorList => rsx! {
            ColorList { value: field.value.clone(), set }
        },
        Kind::Seed => {
            let seed = text.clone();
            rsx! {
                div { class: "seed-field",
                    input {
                        r#type: "number",
                        step: "1",
                        value: "{seed}",
                        onchange: move |evt| set(Value::String(evt.value())),
                    }
                    button {
                        title: "A new random seed",
                        onclick: move |_| set(Value::from(framecraft_core::layers::random_seed())),
                        "Randomize"
                    }
                }
            }
        }
    };

    let key_button = match (prop.lane, item) {
        (Some(lane), Some(id)) => {
            let animated = field.animated;
            let value = field
                .value
                .as_f64()
                .or_else(|| field.value.as_bool().map(|b| f64::from(u8::from(b))))
                .or_else(|| number_default(prop.path))
                .unwrap_or(0.0);
            rsx! {
                button {
                    class: if animated { "key-button animated" } else { "key-button" },
                    title: if animated {
                        "Animated. Click to set a keyframe at the playhead. With Auto-key off, editing shifts the whole animation."
                    } else {
                        "Click to animate: sets the first keyframe at the playhead."
                    },
                    onclick: move |_| {
                        ui.execute(&SetKeyframe {
                            item: id.clone(),
                            lane: lane.to_string(),
                            time_ms: (ui.time)(),
                            value,
                            easing: None,
                        });
                    },
                    if animated { "◆" } else { "◇" }
                }
            }
        }
        _ => rsx! {
            span {}
        },
    };

    let wide = matches!(
        prop.kind,
        Kind::Ramp | Kind::TextArea | Kind::ChartPoints | Kind::ValueRamp | Kind::ColorList
    );
    if wide {
        // A div, not a label: these hold several inputs and buttons.
        return rsx! {
            div { class: "field wide",
                span { class: "field-label", "{prop.label}" }
                {input}
            }
        };
    }
    rsx! {
        label { class: "field",
            span { class: "field-label", "{prop.label}" }
            {input}
            {key_button}
        }
    }
}

/// Gradient stops: position (%) and color per stop.
#[component]
fn Ramp(value: Value, set: Callback<Value>) -> Element {
    let stops: Vec<(f64, String)> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        s["offset"].as_f64().unwrap_or(0.0),
                        s["color"].as_str().unwrap_or("#000000").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let write = move |stops: Vec<(f64, String)>| {
        let mut stops = stops;
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        set(Value::Array(
            stops
                .into_iter()
                .map(|(o, c)| json!({ "offset": o, "color": c }))
                .collect(),
        ));
    };
    let count = stops.len();
    rsx! {
        div { class: "ramp",
            for (i , (offset , c)) in stops.iter().cloned().enumerate() {
                {
                    let (s1, s2, s3) = (stops.clone(), stops.clone(), stops.clone());
                    let hex = color::parse(&c).map_or("#000000".to_string(), |c| c.hex());
                    let pct = number_to_string(round3(offset * 100.0));
                    rsx! {
                        div { class: "stop",
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{pct}",
                                title: "Position %",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().trim().parse::<f64>() {
                                        let mut s = s1.clone();
                                        s[i].0 = (v / 100.0).clamp(0.0, 1.0);
                                        write(s);
                                    }
                                },
                            }
                            input {
                                r#type: "color",
                                value: "{hex}",
                                onchange: move |evt| {
                                    let mut s = s2.clone();
                                    if let Some(c) = color::with_rgb(&s[i].1, &evt.value()) {
                                        s[i].1 = c;
                                        write(s);
                                    }
                                },
                            }
                            button {
                                class: "key-button",
                                disabled: count <= 2,
                                title: "Remove stop",
                                onclick: move |_| {
                                    let mut s = s3.clone();
                                    s.remove(i);
                                    write(s);
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            {
                let s = stops.clone();
                rsx! {
                    button {
                        onclick: move |_| {
                            let mut s = s.clone();
                            let color = s.last().map_or("#ffffff".to_string(), |l| l.1.clone());
                            s.push((1.0, color));
                            write(s);
                        },
                        "+ Stop"
                    }
                }
            }
        }
    }
}

/// Chart data points: label, value and color per point (and its BOARD
/// variable when the scene takes live data).
#[component]
fn ChartPoints(ui: Ui, value: Value, set: Callback<Value>) -> Element {
    use framecraft_core::layers::{DEFAULT_CHART_COLORS, new_chart_point_id};
    let points: Vec<Value> = value.as_array().cloned().unwrap_or_default();
    let board = ui
        .editor
        .read()
        .as_ref()
        .is_some_and(|e| e.doc().scene.ionboard_hook_enabled == Some(true));
    let count = points.len();
    rsx! {
        div { class: "points",
            for (i , p) in points.iter().cloned().enumerate() {
                {
                    let (a, b, c, d, e) = (points.clone(), points.clone(), points.clone(), points.clone(), points.clone());
                    let fallback = DEFAULT_CHART_COLORS[i % DEFAULT_CHART_COLORS.len()];
                    let color = p["color"].as_str().unwrap_or(fallback).to_string();
                    let hex = color::parse(&color).map_or("#000000".to_string(), |c| c.hex());
                    let label = p["label"].as_str().unwrap_or("").to_string();
                    let val = as_text(&p["value"]);
                    let live = p["liveVariableId"].as_str().unwrap_or("").to_string();
                    let edit = move |mut list: Vec<Value>, key: &str, v: Value| {
                        if v.is_null() || v.as_str() == Some("") {
                            if let Some(o) = list[i].as_object_mut() {
                                o.remove(key);
                            }
                        } else {
                            list[i][key] = v;
                        }
                        set(Value::Array(list));
                    };
                    rsx! {
                        div { class: if board { "point live" } else { "point" },
                            input {
                                r#type: "text",
                                value: "{label}",
                                title: "Label",
                                onchange: move |evt| edit(a.clone(), "label", Value::from(evt.value())),
                            }
                            input {
                                r#type: "number",
                                step: "any",
                                value: "{val}",
                                title: "Value",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().trim().parse::<f64>() {
                                        edit(b.clone(), "value", Value::from(v));
                                    }
                                },
                            }
                            input {
                                r#type: "color",
                                value: "{hex}",
                                title: "Color",
                                onchange: move |evt| {
                                    let current = c[i]["color"].as_str().unwrap_or(fallback).to_string();
                                    if let Some(v) = color::with_rgb(&current, &evt.value()) {
                                        edit(c.clone(), "color", Value::from(v));
                                    }
                                },
                            }
                            if board {
                                input {
                                    r#type: "text",
                                    value: "{live}",
                                    placeholder: "BOARD variable",
                                    title: "BOARD variable that sets this value live",
                                    onchange: move |evt| edit(d.clone(), "liveVariableId", Value::from(evt.value().trim())),
                                }
                            }
                            button {
                                class: "key-button",
                                disabled: count <= 1,
                                title: "Remove point",
                                onclick: move |_| {
                                    let mut list = e.clone();
                                    list.remove(i);
                                    set(Value::Array(list));
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            {
                let list = points.clone();
                rsx! {
                    button {
                        onclick: move |_| {
                            let mut list = list.clone();
                            let n = list.len();
                            let label = char::from(b'A' + (n % 26) as u8).to_string();
                            list.push(json!({ "id": new_chart_point_id(), "label": label, "value": 0 }));
                            set(Value::Array(list));
                        },
                        "+ Point"
                    }
                }
            }
        }
    }
}

/// Values (0–1) along a particle's life: position (%) and value per stop.
#[component]
fn ValueRamp(value: Value, set: Callback<Value>) -> Element {
    let stops: Vec<(f64, f64)> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        s["offset"].as_f64().unwrap_or(0.0),
                        s["value"].as_f64().unwrap_or(0.0),
                    )
                })
                .collect()
        })
        .unwrap_or_else(|| vec![(0.0, 1.0), (1.0, 1.0)]);
    let write = move |mut stops: Vec<(f64, f64)>| {
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        set(Value::Array(
            stops
                .into_iter()
                .map(|(o, v)| json!({ "offset": o, "value": v }))
                .collect(),
        ));
    };
    let count = stops.len();
    rsx! {
        div { class: "ramp",
            for (i , (offset , v)) in stops.iter().copied().enumerate() {
                {
                    let (s1, s2, s3) = (stops.clone(), stops.clone(), stops.clone());
                    let pct = number_to_string(round3(offset * 100.0));
                    let val = number_to_string(round3(v));
                    rsx! {
                        div { class: "stop",
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{pct}",
                                title: "Position % of the particle's life",
                                onchange: move |evt| {
                                    if let Ok(p) = evt.value().trim().parse::<f64>() {
                                        let mut s = s1.clone();
                                        s[i].0 = (p / 100.0).clamp(0.0, 1.0);
                                        write(s);
                                    }
                                },
                            }
                            input {
                                r#type: "number",
                                step: "0.05",
                                value: "{val}",
                                title: "Value (0–1)",
                                onchange: move |evt| {
                                    if let Ok(x) = evt.value().trim().parse::<f64>() {
                                        let mut s = s2.clone();
                                        s[i].1 = x.clamp(0.0, 1.0);
                                        write(s);
                                    }
                                },
                            }
                            button {
                                class: "key-button",
                                disabled: count <= 2,
                                title: "Remove stop",
                                onclick: move |_| {
                                    let mut s = s3.clone();
                                    s.remove(i);
                                    write(s);
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            {
                let s = stops.clone();
                rsx! {
                    button {
                        onclick: move |_| {
                            let mut s = s.clone();
                            let last = s.last().map_or(1.0, |l| l.1);
                            s.push((1.0, last));
                            write(s);
                        },
                        "+ Stop"
                    }
                }
            }
        }
    }
}

/// A list of colors (particle palette).
#[component]
fn ColorList(value: Value, set: Callback<Value>) -> Element {
    let colors: Vec<String> = value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        div { class: "color-list",
            for (i , c) in colors.iter().cloned().enumerate() {
                {
                    let (l1, l2) = (colors.clone(), colors.clone());
                    let hex = color::parse(&c).map_or("#000000".to_string(), |x| x.hex());
                    rsx! {
                        div { class: "swatch",
                            input {
                                r#type: "color",
                                value: "{hex}",
                                onchange: move |evt| {
                                    let mut l = l1.clone();
                                    if let Some(v) = color::with_rgb(&l[i], &evt.value()) {
                                        l[i] = v;
                                        set(json!(l));
                                    }
                                },
                            }
                            button {
                                class: "key-button",
                                title: "Remove color",
                                onclick: move |_| {
                                    let mut l = l2.clone();
                                    l.remove(i);
                                    set(json!(l));
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            {
                let l = colors.clone();
                rsx! {
                    button {
                        onclick: move |_| {
                            let mut l = l.clone();
                            l.push(l.last().cloned().unwrap_or_else(|| "#ffffff".into()));
                            set(json!(l));
                        },
                        "+ Color"
                    }
                }
            }
        }
    }
}
