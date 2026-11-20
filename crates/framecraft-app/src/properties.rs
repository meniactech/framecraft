// SPDX-License-Identifier: AGPL-3.0-or-later
//! The properties panel: tabs and fields drawn from the core's property
//! schema (`framecraft_core::schema`). Every edit is one command.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::color;
use framecraft_core::commands::SetKeyframe;
use framecraft_core::js::number_to_string;
use framecraft_core::modifiers;
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
        "guideLineWidth" => Some(5.0),
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
    let gradient = item["gradientType"].as_str().unwrap_or("linear");
    match p.path {
        "bg_color" => fill == "solid",
        // FRAME: the angle for linear and conic, the center for the others.
        "gradientAngle" => fill == "gradient" && matches!(gradient, "linear" | "conic"),
        "gradientCenter" => fill == "gradient" && gradient != "linear",
        p if p.starts_with("gradient") => fill == "gradient",
        p if p.starts_with("bgImage") => fill == "image",
        // The old per-layer shadow: FRAME has only the Shadow modifier now,
        // so the group shows only on layers that still carry one.
        p if p.starts_with("shadow.") => {
            let sh = &item["shadow"];
            ["x", "y", "blur"]
                .iter()
                .any(|k| sh[k].as_f64().is_some_and(|v| v != 0.0))
        }
        _ => true,
    }
}

#[derive(Clone, PartialEq)]
struct Field {
    prop: &'static Property,
    value: Value,
    /// Animated at the playhead (the value shown is the animated one).
    animated: bool,
    /// The modifier this field belongs to (Modifiers tab).
    modifier: Option<String>,
    /// Choices that depend on the document: `(value, label)` (layer pickers).
    options: Vec<(String, String)>,
    /// The whole modifier, for fields that need its other values.
    context: Value,
}

/// One modifier in the stack, ready to draw.
#[derive(Clone, PartialEq)]
struct ModCard {
    id: String,
    title: String,
    help: &'static str,
    enabled: bool,
    fields: Vec<Field>,
}

/// The cards of a layer's modifier stack.
fn modifier_cards(
    doc: &framecraft_core::normalize::Document,
    item: &framecraft_core::model::Item,
    time: f64,
) -> Vec<ModCard> {
    item.modifiers
        .iter()
        .flatten()
        .map(|m| {
            let json = serde_json::to_value(m).unwrap_or_default();
            let kind = modifiers::kind(m);
            let mid = modifiers::id(m).to_string();
            let fields = modifiers::fields(doc, item, m)
                .into_iter()
                .map(|p| {
                    let mut value = schema::lookup(&json, p.path);
                    let mut animated = false;
                    if let Some(lane) = p.lane
                        && let Some(v) = lane_value(doc, &item.id, lane, time)
                    {
                        value = Value::from(round3(v));
                        animated = true;
                    }
                    let current = value.as_str().unwrap_or("").to_string();
                    let mut options: Vec<(String, String)> = match p.kind {
                        Kind::MaskSource => std::iter::once((
                            String::new(),
                            "Custom region on this layer".to_string(),
                        ))
                        .chain(
                            doc.items
                                .iter()
                                .filter(|i| i.id != item.id)
                                .map(|i| (i.id.clone(), i.name.clone())),
                        )
                        .collect(),
                        Kind::PathSource => {
                            std::iter::once((String::new(), "Select path…".to_string()))
                                .chain(modifiers::path_sources(doc, &item.id).into_iter().map(
                                    |i| {
                                        let shape = if i.shape_type
                                            == Some(framecraft_core::model::ShapeType::Path)
                                        {
                                            "path"
                                        } else {
                                            "polygon"
                                        };
                                        (i.id.clone(), format!("{} ({shape})", i.name))
                                    },
                                ))
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    if matches!(p.kind, Kind::MaskSource | Kind::PathSource)
                        && !options.iter().any(|(v, _)| *v == current)
                    {
                        options.push((current, "(missing — pick again)".to_string()));
                    }
                    Field {
                        prop: p,
                        value,
                        animated,
                        modifier: Some(mid.clone()),
                        options,
                        context: json.clone(),
                    }
                })
                .collect();
            ModCard {
                id: mid,
                title: modifiers::label(kind).to_string(),
                help: modifiers::help(kind, item),
                enabled: modifiers::enabled(m),
                fields,
            }
        })
        .collect()
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
    // Several layers selected: FRAME shows a summary instead of one
    // layer's properties.
    let many = ed.selection().len();
    // FRAME: Scene and Script always, then the layer's tabs.
    let mut tabs = vec![Tab::Scene, Tab::Script, Tab::Export];
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
        // FRAME shows these only while the BOARD Hook is on.
        .filter(|p| {
            !schema::needs_board_hook(p.path) || doc.scene.ionboard_hook_enabled == Some(true)
        })
        .map(|p| {
            let root = match p.scope {
                Scope::Item => &item_json,
                Scope::Scene => &scene_json,
                Scope::Timeline => &timeline_json,
                Scope::Modifier => &Value::Null,
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
                modifier: None,
                options: Vec::new(),
                // The center pad previews the layer's own gradient.
                context: if matches!(p.kind, Kind::GradientCenter) {
                    item_json.clone()
                } else {
                    Value::Null
                },
            }
        })
        .collect();
    let layer_tab = !matches!(tab, Tab::Scene | Tab::Script | Tab::Export);
    let multi = many > 1 && layer_tab;
    let fields = if multi { Vec::new() } else { fields };
    let cards = match item {
        Some(i) if tab == Tab::Modifiers && !multi => modifier_cards(doc, i, time),
        _ => Vec::new(),
    };
    let addable: Vec<(&'static str, bool)> = item.map_or_else(Vec::new, |i| {
        modifiers::kinds_for(i)
            .into_iter()
            .map(|k| {
                (
                    k,
                    i.modifiers
                        .iter()
                        .flatten()
                        .any(|m| modifiers::kind(m) == k),
                )
            })
            .collect()
    });
    let id = item.map(|i| i.id.clone());
    let item_ref = item.cloned();
    let board_hook = doc.scene.ionboard_hook_enabled == Some(true);
    // The project's own fonts first in the font list (FRAME: uploaded
    // fonts join the built-in ones).
    let project_fonts: Vec<String> = ed
        .project
        .media
        .fonts
        .iter()
        .map(framecraft_core::media::font_family)
        .collect();
    let heading = match item {
        Some(_) if multi => format!("{many} layers"),
        Some(i) if !matches!(tab, Tab::Scene | Tab::Script | Tab::Export) => i.name.clone(),
        _ => doc.scene.name.clone(),
    };
    drop(editor);

    rsx! {
        div { class: "properties",
            // Icon tabs stacked on the left, as in FRAME (and Blender).
            div { class: "tab-rail", role: "tablist",
                for t in tabs {
                    button {
                        class: if t == tab { "tab active" } else { "tab" },
                        onclick: move |_| {
                            let mut current = ui.tab;
                            current.set(t);
                        },
                        title: "{t.label()}",
                        img {
                            class: "tab-icon",
                            src: crate::preview::library_url(&format!("/icons/properties/{}.svg", t.icon())),
                            alt: "{t.label()}",
                            draggable: "false",
                        }
                    }
                }
            }
            div { class: "prop-body",
            p { class: "layer-name", "{heading}" }
            if tab == Tab::Script {
                crate::script_editor::ScriptTab { ui }
            }
            if tab == Tab::Export {
                crate::export::ExportTab { ui }
            }
            if layer_tab && id.is_none() {
                p { class: "dim", "Select a layer" }
            }
            if multi && tab == Tab::Transform {
                h3 { "Selection summary" }
                p { "{many} items selected." }
                p { class: "dim", "Use the align buttons in the viewport, or select one item for detailed properties." }
            } else if multi {
                p { class: "dim", "Select a single item" }
            }
            for (i , f) in fields.iter().enumerate() {
                if i == 0 || fields[i - 1].prop.group != f.prop.group {
                    h3 {
                        "{f.prop.group}"
                        if let Some(text) = schema::group_help(tab, f.prop.group, item_ref.as_ref(), board_hook) {
                            InfoTip { text }
                        }
                    }
                }
                if tab == Tab::Scene && f.prop.path == "width" {
                    ResolutionPreset { ui }
                }
                if f.prop.path == "borderWidthMode" {
                    if let Some(item) = id.clone() {
                        BorderMode { ui, item }
                    }
                } else {
                    FieldRow { ui, field: f.clone(), item: id.clone() }
                }
            }
            if tab == Tab::Scene && ui.editor.read().is_some() {
                Plates { ui }
            }
            if tab == Tab::Scene && ui.editor.read().is_some() {
                SceneCues { ui }
            }
            if tab == Tab::Modifiers && !multi {
                if let Some(item) = id.clone() {
                    ModifierStack { ui, item, cards, addable }
                }
            }
            datalist { id: "fc-fonts",
                for family in project_fonts {
                    option { value: "{family}", "Project font" }
                }
                for family in schema::builtin_font_families() {
                    option { value: "{family}" }
                }
            }
            }
        }
    }
}

#[component]
fn FieldRow(ui: Ui, field: Field, item: Option<String>) -> Element {
    let prop = field.prop;
    let target = item.clone();
    let modifier = field.modifier.clone();
    let set = use_callback(move |v: Value| {
        let (time, auto_key) = (*ui.time.peek(), *ui.auto_key.peek());
        let cmd = match (&modifier, target.as_deref()) {
            (Some(m), Some(item)) => prop.modifier_command(item, m, v, time, auto_key),
            _ => prop.command(target.as_deref(), v, time, auto_key),
        };
        match cmd {
            Some(cmd) => ui.execute(cmd.as_ref()),
            None => {
                let mut status = ui.status;
                status.set(format!("{}: not a valid value", prop.label));
            }
        }
    });
    let text = as_text(&field.value);
    let board_hook = ui
        .editor
        .read()
        .as_ref()
        .is_some_and(|e| e.doc().scene.ionboard_hook_enabled == Some(true));
    let help = schema::help(prop.path, board_hook).unwrap_or("");
    let mut confirm_remove = use_signal(|| false);
    let default = if field.modifier.is_some() {
        modifiers::field_default(prop.group, prop.path)
    } else {
        number_default(prop.path)
    };

    let input = match prop.kind {
        Kind::Number { step, .. } => {
            let shown = match field.value.as_f64().or(default) {
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
        Kind::Color => rsx! {
            div { class: "color-field",
                ColorButton { ui, value: text.clone(), on_change: move |c: String| set(Value::from(c)) }
                input {
                    r#type: "text",
                    value: "{text}",
                    onchange: move |evt| set(Value::from(evt.value().trim())),
                }
            }
        },
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
            Ramp { ui, path: prop.path, value: field.value.clone(), set }
        },
        Kind::ChartPoints => rsx! {
            ChartPoints { ui, value: field.value.clone(), set }
        },
        Kind::GradientCenter => rsx! {
            GradientCenter { value: field.value.clone(), item: field.context.clone(), set }
        },
        Kind::ValueRamp => rsx! {
            ValueRamp { ui, path: prop.path, value: field.value.clone(), set }
        },
        Kind::ColorList => rsx! {
            ColorList { ui, value: field.value.clone(), set }
        },
        Kind::ClipStart => rsx! {
            input {
                r#type: "number",
                step: "10",
                min: "0",
                value: "{text}",
                title: "Also set by dragging the Start key on the timeline",
                onchange: move |evt| set(Value::String(evt.value())),
            }
        },
        Kind::MaskSource | Kind::PathSource => {
            let options = field.options.clone();
            rsx! {
                select { onchange: move |evt| set(Value::from(evt.value())),
                    for (v , l) in options {
                        option { value: "{v}", selected: v == text, "{l}" }
                    }
                }
            }
        }
        Kind::Media(category) => {
            let target = crate::state::PickTarget::Field {
                prop,
                item: item.clone().unwrap_or_default(),
                modifier: field.modifier.clone(),
            };
            let thumb = (category == "images" && !text.is_empty())
                .then(|| crate::preview::library_url(&text));
            let has = !text.is_empty();
            rsx! {
                div { class: "media-field",
                    if let Some(src) = thumb {
                        img { class: "media-mini", src: "{src}", draggable: "false" }
                    }
                    input {
                        r#type: "text",
                        value: "{text}",
                        placeholder: "None",
                        title: "{text}",
                        onchange: move |evt| set(Value::from(evt.value().trim())),
                    }
                    button {
                        title: "Pick from the project's media, or import a file",
                        disabled: item.is_none(),
                        onclick: move |_| ui.open_media(category, target.clone()),
                        "Choose…"
                    }
                    if has {
                        button {
                            class: "key-button",
                            title: "Clear",
                            onclick: move |_| set(Value::from("")),
                            "×"
                        }
                    }
                }
            }
        }
        Kind::Rulers => rsx! {
            Rulers { value: field.value.clone(), set }
        },
        Kind::PolygonPoints => rsx! {
            PolygonPoints { ui, item: item.clone().unwrap_or_default(), value: field.value.clone(), set }
        },
        Kind::SvgToPolygon => {
            let id = item.clone().unwrap_or_default();
            rsx! {
                button {
                    title: "Trace the SVG into editable points (its first path)",
                    onclick: move |_| ui.convert_svg_to_polygon(&id),
                    "Convert"
                }
            }
        }
        Kind::MaskPoints => rsx! {
            MaskPoints { value: field.value.clone(), area: field.context["transform"].clone(), set }
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
                .or(default)
                .unwrap_or(0.0);
            // The channel's keys, for removing the whole channel.
            let times: Vec<f64> = ui
                .editor
                .read()
                .as_ref()
                .and_then(|e| framecraft_core::timeline::lane_keys(e.doc(), &id, lane))
                .map(|k| k.iter().map(|k| k.time).collect())
                .unwrap_or_default();
            let n = times.len();
            let id2 = id.clone();
            rsx! {
                span { class: "key-tools", onmouseleave: move |_| confirm_remove.set(false),
                    button {
                        class: if animated { "key-button animated" } else { "key-button" },
                        title: if animated {
                            "Animated. Click to set a keyframe at the playhead. With Auto-key off, editing shifts the whole animation."
                        } else {
                            "Click to animate: sets the first keyframe at the playhead."
                        },
                        onclick: move |evt| {
                            evt.prevent_default();
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
                    if animated && n > 0 {
                        if confirm_remove() {
                            button {
                                class: "danger key-remove-confirm",
                                title: "Remove this animation channel: its {n} key(s) go and the value at the playhead stays",
                                onclick: move |evt| {
                                    evt.prevent_default();
                                    confirm_remove.set(false);
                                    let keys = times
                                        .iter()
                                        .map(|t| framecraft_core::keys::KeyRef::new(&id2, lane, *t))
                                        .collect();
                                    ui.execute(&framecraft_core::keys::RemoveKeys { keys });
                                },
                                "Delete {n}?"
                            }
                        } else {
                            button {
                                class: "key-button key-remove",
                                title: "Remove this animation channel ({n} key(s))",
                                onclick: move |evt| {
                                    evt.prevent_default();
                                    confirm_remove.set(true);
                                },
                                "✕"
                            }
                        }
                    }
                }
            }
        }
        _ => rsx! {
            span {}
        },
    };

    let wide = matches!(
        prop.kind,
        Kind::Ramp
            | Kind::TextArea
            | Kind::ChartPoints
            | Kind::ValueRamp
            | Kind::ColorList
            | Kind::MaskPoints
            | Kind::PolygonPoints
            | Kind::Rulers
    );
    if wide {
        // A div, not a label: these hold several inputs and buttons.
        return rsx! {
            div { class: "field wide",
                span { class: "field-label", title: "{help}", "{prop.label}" }
                {input}
            }
        };
    }
    if let Kind::Number { min, max, step } = prop.kind {
        // FRAME's numeric pill (label | value). Dragging the label left or
        // right scrubs the value (Shift ×10, Alt ×0.1) as one undo step:
        // the bridge sends `change` events on the input meanwhile.
        let label = prop.label;
        return rsx! {
            label { class: "field num",
                div { class: "pill",
                    span {
                        class: "field-label scrub",
                        title: if help.is_empty() { "Drag to change, click to type".to_string() } else { format!("{help}\n\nDrag to change, click to type") },
                        "data-step": "{step}",
                        "data-min": "{min}",
                        "data-max": "{max}",
                        onpointerdown: move |_| ui.begin_gesture(format!("Change {label}")),
                        onpointerup: move |_| ui.end_gesture(),
                        onpointercancel: move |_| ui.end_gesture(),
                        "{prop.label}"
                    }
                    {input}
                }
                {key_button}
            }
        };
    }
    if matches!(prop.kind, Kind::Color) {
        // A div, not a label: clicks in the color popover must not be
        // forwarded to the color chip.
        return rsx! {
            div { class: "field",
                span { class: "field-label", title: "{help}", "{prop.label}" }
                {input}
                {key_button}
            }
        };
    }
    rsx! {
        label { class: "field",
            span { class: "field-label", title: "{help}", "{prop.label}" }
            {input}
            {key_button}
        }
    }
}

/// Gradient stops: position (%) and color per stop.
/// A color ramp's color at `offset` (0–1), mixed between its stops.
fn ramp_color_at(stops: &[(f64, String)], offset: f64) -> String {
    let parse = |c: &str| {
        color::parse(c).unwrap_or(color::Rgba {
            r: 255.0,
            g: 255.0,
            b: 255.0,
            a: 1.0,
        })
    };
    for w in stops.windows(2) {
        let ((oa, ca), (ob, cb)) = (&w[0], &w[1]);
        if offset >= *oa && offset <= *ob {
            let t = (offset - oa) / (ob - oa).max(1e-9);
            let (a, b) = (parse(ca), parse(cb));
            let mix = |x: f64, y: f64| x + (y - x) * t;
            return color::Rgba {
                r: mix(a.r, b.r),
                g: mix(a.g, b.g),
                b: mix(a.b, b.b),
                a: mix(a.a, b.a),
            }
            .css();
        }
    }
    stops
        .last()
        .map_or("rgba(255, 255, 255, 1)".to_string(), |s| s.1.clone())
}

/// What a new layer has for this ramp (Reset).
fn ramp_default(path: &str) -> Value {
    use framecraft_core::layers::{NewLayer, new_layer};
    NewLayer::ALL
        .iter()
        .map(|k| serde_json::to_value(new_layer(*k, None)).unwrap_or_default())
        .map(|v| schema::lookup(&v, path))
        .find(|v| v.as_array().is_some_and(|a| a.len() >= 2))
        .unwrap_or_else(|| {
            json!([{ "offset": 0, "color": "rgba(255, 255, 255, 1)" }, { "offset": 1, "color": "rgba(255, 255, 255, 0)" }])
        })
}

/// A color ramp (FRAME's gradient editor): the gradient on a track; click
/// it to add a stop, press a stop to select it, drag middle stops (the
/// ends stay pinned). The bridge does the pointer work and live preview;
/// changes arrive as `ramp` events. Below: the selected stop's color.
#[component]
fn Ramp(ui: Ui, path: &'static str, value: Value, set: Callback<Value>) -> Element {
    let mut stops: Vec<(f64, String)> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        s["offset"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
                        s["color"].as_str().unwrap_or("#000000").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut selected = use_signal(|| 0usize);
    let mut stock = use_signal(|| false);
    // The effect below reads the latest stops through this.
    let latest =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::<(f64, String)>::new())));
    *latest.borrow_mut() = stops.clone();
    let write = move |list: Vec<(f64, String)>| {
        let mut list = list;
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        set(Value::Array(
            list.into_iter()
                .map(|(o, c)| json!({ "offset": o, "color": c }))
                .collect(),
        ));
    };
    {
        let latest = latest.clone();
        use_effect(move || {
            let Some(ev) = (ui.ramp_event)() else { return };
            if ev["id"].as_str() != Some(path) {
                return;
            }
            let mut ramp_event = ui.ramp_event;
            ramp_event.set(None);
            let mut list = latest.borrow().clone();
            let index = ev["index"].as_u64().unwrap_or(0) as usize;
            let offset = ev["offset"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
            match ev["action"].as_str() {
                Some("select") => selected.set(index),
                Some("add") => {
                    let c = ramp_color_at(&list, offset);
                    list.push((offset, c.clone()));
                    list.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let at = list
                        .iter()
                        .position(|s| s.0 == offset && s.1 == c)
                        .unwrap_or(0);
                    selected.set(at);
                    write(list);
                }
                Some("move") if index < list.len() => {
                    list[index].0 = offset;
                    write(list);
                }
                _ => {}
            }
        });
    }
    let count = stops.len();
    let sel = selected().min(count.saturating_sub(1));
    let can_remove = sel > 0 && sel + 1 < count;
    let gradient = stops
        .iter()
        .map(|(o, c)| format!("{c} {}%", number_to_string(round3(o * 100.0))))
        .collect::<Vec<_>>()
        .join(", ");
    let fill = format!("background: linear-gradient(90deg, {gradient});");
    let current = stops.get(sel).cloned().unwrap_or((0.0, "#ffffff".into()));
    let alpha = color::parse(&current.1).map_or(100.0, |c| (c.a * 100.0).round());
    let pct = number_to_string(round3(current.0 * 100.0));
    let (s_color, s_alpha, s_pos, s_text, s_remove) = (
        stops.clone(),
        stops.clone(),
        stops.clone(),
        stops.clone(),
        stops.clone(),
    );
    rsx! {
        if stock() {
            crate::palettes::StockGradientDialog {
                on_close: move |_| stock.set(false),
                on_pick: move |list: Vec<framecraft_core::model::RampColor>| {
                    stock.set(false);
                    selected.set(0);
                    set(serde_json::to_value(list).unwrap_or_default());
                },
            }
        }
        div { class: "ramp",
            div { class: "ramp-actions",
                button {
                    title: "Reset to the default gradient",
                    onclick: move |_| {
                        selected.set(0);
                        set(ramp_default(path));
                    },
                    "Reset"
                }
                button {
                    title: "Select from stock gradient",
                    onclick: move |_| stock.set(true),
                    "Stock…"
                }
                button {
                    title: "Remove the selected stop (not the ends)",
                    disabled: !can_remove,
                    onclick: move |_| {
                        let mut list = s_remove.clone();
                        if sel < list.len() {
                            list.remove(sel);
                        }
                        selected.set(sel.saturating_sub(1));
                        write(list);
                    },
                    "Remove stop"
                }
            }
            div {
                class: "ramp-track",
                "data-ramp": "{path}",
                title: "Click to add a stop; drag stops to move them",
                div { class: "ramp-fill", style: "{fill}" }
                for (i , (o , c)) in stops.iter().cloned().enumerate() {
                    div {
                        class: if i == sel { "ramp-dot selected" } else { "ramp-dot" },
                        "data-index": "{i}",
                        "data-offset": "{o}",
                        "data-color": "{c}",
                        style: "left:{o * 100.0}%;background:{c};",
                        title: "{number_to_string(round3(o * 100.0))}%",
                    }
                }
            }
            div { class: "ramp-stop",
                span { class: "dim", "Stop" }
                ColorButton {
                    ui,
                    value: stops.get(sel).map(|s| s.1.clone()).unwrap_or_default(),
                    title: "Stop color",
                    on_change: move |c: String| {
                        let mut list = s_color.clone();
                        if let Some(s) = list.get_mut(sel) {
                            s.1 = c;
                            write(list);
                        }
                    },
                }
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: "{alpha}",
                    title: "Alpha {alpha}%",
                    onchange: move |evt| {
                        let mut list = s_alpha.clone();
                        if let (Some(s), Ok(a)) = (list.get_mut(sel), evt.value().parse::<f64>())
                            && let Some(c) = color::with_alpha(&s.1, a / 100.0)
                        {
                            s.1 = c;
                            write(list);
                        }
                    },
                }
                input {
                    r#type: "number",
                    step: "1",
                    min: "0",
                    max: "100",
                    value: "{pct}",
                    disabled: sel == 0 || sel + 1 == count,
                    title: "Position %",
                    onchange: move |evt| {
                        let mut list = s_pos.clone();
                        if let (Some(s), Ok(v)) = (list.get_mut(sel), evt.value().trim().parse::<f64>()) {
                            s.0 = (v / 100.0).clamp(0.0, 1.0);
                            write(list);
                        }
                    },
                }
                input {
                    r#type: "text",
                    value: "{current.1}",
                    title: "Stop color (CSS)",
                    onchange: move |evt| {
                        let mut list = s_text.clone();
                        if let Some(s) = list.get_mut(sel) {
                            s.1 = evt.value().trim().to_string();
                            write(list);
                        }
                    },
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
                            ColorButton {
                                ui,
                                value: color.clone(),
                                title: "Color",
                                on_change: move |v: String| edit(c.clone(), "color", Value::from(v)),
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

/// A value over a particle's life (FRAME's curve editor): a box with the
/// curve from left (birth) to right (death), values 0–1. Click to add a
/// stop, press one to select it, drag it (the ends only up and down). The
/// bridge does the pointer work and live preview; changes arrive as
/// `ramp` events. Below: the selected stop's value.
#[component]
fn ValueRamp(ui: Ui, path: &'static str, value: Value, set: Callback<Value>) -> Element {
    let (value_label, axis_y, default) = match path {
        "particleAlphaRamp" => (
            "Alpha",
            "Alpha 0→1",
            json!([{ "offset": 0, "value": 1 }, { "offset": 1, "value": 0 }]),
        ),
        _ => (
            "Scale",
            "Scale 0→1",
            json!([{ "offset": 0, "value": 1 }, { "offset": 1, "value": 0.25 }]),
        ),
    };
    let mut stops: Vec<(f64, f64)> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        s["offset"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
                        s["value"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if stops.len() < 2 {
        stops = default
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                (
                    s["offset"].as_f64().unwrap_or(0.0),
                    s["value"].as_f64().unwrap_or(0.0),
                )
            })
            .collect();
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut selected = use_signal(|| 0usize);
    let latest = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::<(f64, f64)>::new())));
    *latest.borrow_mut() = stops.clone();
    let write = move |list: Vec<(f64, f64)>| {
        let mut list = list;
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        set(Value::Array(
            list.into_iter()
                .map(|(o, v)| json!({ "offset": o, "value": v }))
                .collect(),
        ));
    };
    {
        let latest = latest.clone();
        use_effect(move || {
            let Some(ev) = (ui.ramp_event)() else { return };
            if ev["id"].as_str() != Some(path) {
                return;
            }
            let mut ramp_event = ui.ramp_event;
            ramp_event.set(None);
            let mut list = latest.borrow().clone();
            let index = ev["index"].as_u64().unwrap_or(0) as usize;
            let offset = ev["offset"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
            let v = ev["value"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
            match ev["action"].as_str() {
                Some("select") => selected.set(index),
                Some("add") => {
                    list.push((offset, v));
                    list.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let at = list.iter().position(|s| *s == (offset, v)).unwrap_or(0);
                    selected.set(at);
                    write(list);
                }
                Some("move") if index < list.len() => {
                    list[index] = (offset, v);
                    write(list);
                }
                _ => {}
            }
        });
    }
    let count = stops.len();
    let sel = selected().min(count.saturating_sub(1));
    let can_remove = sel > 0 && sel + 1 < count;
    let points = stops
        .iter()
        .map(|(o, v)| format!("{},{}", round3(o * 100.0), round3(100.0 - v * 100.0)))
        .collect::<Vec<_>>()
        .join(" ");
    let current = stops.get(sel).copied().unwrap_or((0.0, 0.0));
    let shown = number_to_string(round3(current.1));
    let (s_val, s_remove) = (stops.clone(), stops.clone());
    rsx! {
        div { class: "ramp",
            div { class: "ramp-actions",
                button {
                    title: "Reset to the default curve",
                    onclick: move |_| {
                        selected.set(0);
                        set(default.clone());
                    },
                    "Reset"
                }
                button {
                    title: "Remove the selected stop (not the ends)",
                    disabled: !can_remove,
                    onclick: move |_| {
                        let mut list = s_remove.clone();
                        if sel < list.len() {
                            list.remove(sel);
                        }
                        selected.set(sel.saturating_sub(1));
                        write(list);
                    },
                    "Remove stop"
                }
            }
            div { class: "curve-axes",
                span { "Life 0→1" }
                span { "{axis_y}" }
            }
            div {
                class: "curve-track",
                "data-curve": "{path}",
                title: "Click to add a stop; drag stops to change the curve",
                svg {
                    class: "curve-line",
                    view_box: "0 0 100 100",
                    preserve_aspect_ratio: "none",
                    polyline {
                        points: "{points}",
                        fill: "none",
                        stroke: "#d4d4d4",
                        stroke_width: "2",
                        vector_effect: "non-scaling-stroke",
                    }
                }
                for (i , (o , v)) in stops.iter().copied().enumerate() {
                    div {
                        class: match (i == sel, i == 0 || i + 1 == count) {
                            (true, _) => "curve-dot selected",
                            (false, true) => "curve-dot endpoint",
                            _ => "curve-dot",
                        },
                        "data-index": "{i}",
                        "data-offset": "{o}",
                        "data-value": "{v}",
                        style: "left:{o * 100.0}%;top:{100.0 - v * 100.0}%;",
                        title: "{number_to_string(round3(o * 100.0))}% · {number_to_string(round3(v))}",
                    }
                }
            }
            label { class: "curve-value",
                span { class: "dim", "{value_label}" }
                input {
                    r#type: "number",
                    step: "0.01",
                    min: "0",
                    max: "1",
                    value: "{shown}",
                    onchange: move |evt| {
                        let mut list = s_val.clone();
                        if let (Some(s), Ok(v)) = (list.get_mut(sel), evt.value().trim().parse::<f64>()) {
                            s.1 = v.clamp(0.0, 1.0);
                            write(list);
                        }
                    },
                }
            }
        }
    }
}

/// A list of colors (particle palette).
#[component]
fn ColorList(ui: Ui, value: Value, set: Callback<Value>) -> Element {
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
                    rsx! {
                        div { class: "swatch",
                            ColorButton {
                                ui,
                                value: c.clone(),
                                on_change: move |v: String| {
                                    let mut l = l1.clone();
                                    l[i] = v;
                                    set(json!(l));
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

/// The Modifiers tab: an Add menu and one card per modifier, in stack order.
#[component]
fn ModifierStack(
    ui: Ui,
    item: String,
    cards: Vec<ModCard>,
    addable: Vec<(&'static str, bool)>,
) -> Element {
    use framecraft_core::modifiers::{AddModifier, MoveModifier, RemoveModifier, set_enabled};
    let mut menu = use_signal(|| false);
    let count = cards.len();
    let add_item = item.clone();
    let add = use_callback(move |kind: &'static str| {
        let made = ui.editor.read().as_ref().and_then(|ed| {
            let doc = ed.doc();
            let it = doc.items.iter().find(|i| i.id == add_item)?;
            modifiers::new_modifier(doc, it, kind)
        });
        if let Some(modifier) = made {
            ui.execute(&AddModifier {
                item: add_item.clone(),
                modifier,
            });
        }
    });
    rsx! {
        div { class: "mod-head",
            h3 {
                "Modifier stack"
                if let Some(text) = schema::group_help(Tab::Modifiers, "", None, false) {
                    InfoTip { text }
                }
            }
            div { class: "add-wrap",
                button {
                    title: "Add a modifier. Modifiers apply in list order.",
                    onclick: move |_| menu.set(!menu()),
                    "+ Add modifier"
                }
                if menu() {
                    div { class: "menu", onmouseleave: move |_| menu.set(false),
                        for (kind , taken) in addable {
                            button {
                                disabled: taken,
                                title: if taken { "This layer already has one" } else { "" },
                                onclick: move |_| {
                                    menu.set(false);
                                    add(kind);
                                },
                                "{modifiers::label(kind)}"
                            }
                        }
                    }
                }
            }
        }
        if cards.is_empty() {
            p { class: "dim", "No modifiers yet." }
        }
        for (i , card) in cards.into_iter().enumerate() {
            {
                let (a, b, c, d) = (item.clone(), item.clone(), item.clone(), item.clone());
                let (ma, mb, mc, md) = (card.id.clone(), card.id.clone(), card.id.clone(), card.id.clone());
                let on = card.enabled;
                rsx! {
                    div { class: if on { "mod-card" } else { "mod-card off" },
                        div { class: "mod-title",
                            input {
                                r#type: "checkbox",
                                checked: on,
                                title: "Enable or disable this modifier in the viewport and export",
                                onchange: move |_| ui.execute(&set_enabled(&a, &ma, !on)),
                            }
                            span { title: "{card.help}", "{card.title}" }
                            button {
                                class: "key-button",
                                disabled: i == 0,
                                title: "Move up (applies earlier)",
                                onclick: move |_| {
                                    ui.execute(&MoveModifier { item: b.clone(), modifier: mb.clone(), to: i - 1 });
                                },
                                "▲"
                            }
                            button {
                                class: "key-button",
                                disabled: i + 1 == count,
                                title: "Move down (applies later)",
                                onclick: move |_| {
                                    ui.execute(&MoveModifier { item: c.clone(), modifier: mc.clone(), to: i + 1 });
                                },
                                "▼"
                            }
                            button {
                                class: "key-button",
                                title: "Remove this modifier and its keyframes",
                                onclick: move |_| {
                                    ui.execute(&RemoveModifier { item: d.clone(), modifier: md.clone() });
                                },
                                "×"
                            }
                        }
                        if !card.help.is_empty() {
                            p { class: "mod-help", "{card.help}" }
                        }
                        for f in card.fields {
                            FieldRow { ui, field: f, item: Some(item.clone()) }
                        }
                    }
                }
            }
        }
    }
}

/// A mask's polygon points, in the layer's own box. At least 3 points.
#[component]
fn MaskPoints(value: Value, area: Value, set: Callback<Value>) -> Element {
    let points: Vec<Value> = value.as_array().cloned().unwrap_or_default();
    let n = |k: &str, d: f64| area[k].as_f64().unwrap_or(d);
    let (x, y, w, h) = (
        n("x", 0.0),
        n("y", 0.0),
        n("width", 1.0).max(1.0),
        n("height", 1.0).max(1.0),
    );
    let count = points.len();
    let add = points.clone();
    rsx! {
        div { class: "points",
            for (i , p) in points.iter().cloned().enumerate() {
                {
                    let (a, b, c) = (points.clone(), points.clone(), points.clone());
                    let px = as_text(&p["x"]);
                    let py = as_text(&p["y"]);
                    let edit = move |mut list: Vec<Value>, key: &str, v: String| {
                        if let Ok(v) = v.trim().parse::<f64>() {
                            list[i][key] = Value::from(v);
                            set(Value::Array(list));
                        }
                    };
                    rsx! {
                        div { class: "point xy",
                            span { class: "dim", "#{i + 1}" }
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{px}",
                                title: "X",
                                onchange: move |evt| edit(a.clone(), "x", evt.value()),
                            }
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{py}",
                                title: "Y",
                                onchange: move |evt| edit(b.clone(), "y", evt.value()),
                            }
                            button {
                                class: "key-button",
                                disabled: count <= 3,
                                title: "Remove point (a polygon needs at least 3)",
                                onclick: move |_| {
                                    let mut list = c.clone();
                                    list.remove(i);
                                    set(Value::Array(list));
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            if count == 0 {
                p { class: "dim", "No points yet." }
            }
            div { class: "row-buttons",
                button {
                    title: "Four points on the mask's box",
                    onclick: move |_| {
                        set(json!([
                            { "x": x, "y": y },
                            { "x": x + w, "y": y },
                            { "x": x + w, "y": y + h },
                            { "x": x, "y": y + h }
                        ]));
                    },
                    "Init from box"
                }
                button {
                    onclick: move |_| {
                        // As FRAME: halfway between the last and first point.
                        let mut list = add.clone();
                        if list.len() < 2 {
                            list = vec![json!({ "x": x, "y": y }), json!({ "x": x + w, "y": y })];
                        }
                        let (first, last) = (&list[0], &list[list.len() - 1]);
                        let mid = |k: &str| {
                            (first[k].as_f64().unwrap_or(0.0) + last[k].as_f64().unwrap_or(0.0)) / 2.0
                        };
                        let p = json!({ "x": mid("x"), "y": mid("y") });
                        list.push(p);
                        set(Value::Array(list));
                    },
                    "+ Point"
                }
            }
        }
    }
}

/// A polygon or path's points (FRAME's Look → Polygon / Path): reset, add
/// on the longest edge, and per point smooth, x / y, remove, and the bezier
/// handles. Every change fits the layer box to the points.
#[component]
fn PolygonPoints(ui: Ui, item: String, value: Value, set: Callback<Value>) -> Element {
    use framecraft_core::model::{PolygonPoint, ShapeType};
    use framecraft_core::polygon;
    let points: Vec<PolygonPoint> = serde_json::from_value(value.clone()).unwrap_or_default();
    let (path, w, h, curve) = ui
        .editor
        .read()
        .as_ref()
        .and_then(|e| e.doc().items.iter().find(|i| i.id == item).cloned())
        .map_or((false, 100.0, 100.0, "linear".to_string()), |i| {
            (
                i.shape_type == Some(ShapeType::Path),
                i.width,
                i.height,
                i.polygon_curve.clone().unwrap_or_else(|| "linear".into()),
            )
        });
    let bezier_mode = curve == "bezier";
    let min = if path { 2 } else { 3 };
    let count = points.len();
    let send = move |list: Vec<PolygonPoint>| set(serde_json::to_value(list).unwrap_or_default());
    let add_points = points.clone();
    rsx! {
        div { class: "points",
            p { class: "dim hint",
                if path {
                    "Open stroke: visible while editing, hidden during play."
                } else {
                    "The layer box fits the points after each edit."
                }
            }
            for (i , p) in points.iter().cloned().enumerate() {
                {
                    let (a, b, c, d, e) = (points.clone(), points.clone(), points.clone(), points.clone(), points.clone());
                    let smooth = p.curve.as_deref() == Some("bezier");
                    let num = |v: Option<f64>, d: f64| framecraft_core::js::number_to_string(round3(v.unwrap_or(d)));
                    let (px, py) = (num(Some(p.x), 0.0), num(Some(p.y), 0.0));
                    let handles = [
                        ("In X", num(p.in_x, p.x)),
                        ("In Y", num(p.in_y, p.y)),
                        ("Out X", num(p.out_x, p.x)),
                        ("Out Y", num(p.out_y, p.y)),
                    ];
                    let edit = move |mut list: Vec<PolygonPoint>, key: &str, v: String| {
                        let Ok(v) = v.trim().parse::<f64>() else { return };
                        let q = &mut list[i];
                        match key {
                            "x" => q.x = v,
                            "y" => q.y = v,
                            "In X" => q.in_x = Some(v),
                            "In Y" => q.in_y = Some(v),
                            "Out X" => q.out_x = Some(v),
                            _ => q.out_y = Some(v),
                        }
                        send(list);
                    };
                    rsx! {
                        div { class: "point poly",
                            span { class: "dim", "#{i + 1}" }
                            label { class: "smooth", title: "Bezier handles on this point",
                                input {
                                    r#type: "checkbox",
                                    checked: smooth,
                                    onchange: move |_| {
                                        let mut list = a.clone();
                                        let n = list.len();
                                        let q = list[i].clone();
                                        if smooth {
                                            list[i].curve = Some("linear".into());
                                            list[i].in_x = Some(q.x);
                                            list[i].in_y = Some(q.y);
                                            list[i].out_x = Some(q.x);
                                            list[i].out_y = Some(q.y);
                                        } else {
                                            let one = polygon::normalize_handles(
                                                &[list[(i + n - 1) % n].clone(), q, list[(i + 1) % n].clone()],
                                                "bezier",
                                            );
                                            list[i] = one[1].clone();
                                        }
                                        send(list);
                                    },
                                }
                                "Smooth"
                            }
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{px}",
                                title: "X",
                                onchange: move |evt| edit(b.clone(), "x", evt.value()),
                            }
                            input {
                                r#type: "number",
                                step: "1",
                                value: "{py}",
                                title: "Y",
                                onchange: move |evt| edit(c.clone(), "y", evt.value()),
                            }
                            button {
                                class: "key-button",
                                disabled: count <= min,
                                title: "Remove point",
                                onclick: move |_| {
                                    let mut list = d.clone();
                                    list.remove(i);
                                    send(list);
                                },
                                "×"
                            }
                        }
                        if bezier_mode && smooth {
                            div { class: "point handles",
                                for (key , val) in handles {
                                    {
                                        let list = e.clone();
                                        rsx! {
                                            input {
                                                r#type: "number",
                                                step: "1",
                                                value: "{val}",
                                                title: "{key}",
                                                placeholder: "{key}",
                                                onchange: move |evt| edit(list.clone(), key, evt.value()),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "row-buttons",
                button {
                    title: if path { "Reset to a left → right bezier" } else { "Reset to a rectangle inset in the bounds" },
                    onclick: move |_| {
                        let (points, curve) = if path {
                            (polygon::default_path_points(w, h), "bezier".to_string())
                        } else {
                            (polygon::default_polygon_points(w, h), curve.clone())
                        };
                        set(json!({ "points": points, "curve": curve }));
                    },
                    "Reset shape"
                }
                button {
                    title: "Insert a point on the longest edge",
                    onclick: move |_| send(polygon::insert_on_longest_edge(&add_points)),
                    "+ Point"
                }
            }
        }
    }
}

/// FRAME's built-in plates: `(id, label, image, css fallback)`.
pub const PLATES: &[(&str, &str, Option<&str>, &str)] = &[
    (
        "news-room",
        "News Room",
        Some("/backgrounds/news-room.jpg"),
        "linear-gradient(165deg, #1a2744 0%, #16181d 40%, #1e293b 70%, #334155 100%)",
    ),
    (
        "news-room-2",
        "News Room 2",
        Some("/backgrounds/news-room-2.jpg"),
        "linear-gradient(165deg, #1a2744 0%, #16181d 40%, #1e293b 70%, #334155 100%)",
    ),
    (
        "reporter",
        "Reporter",
        Some("/backgrounds/reporter.jpg"),
        "radial-gradient(ellipse 90% 70% at 50% 40%, #2a3348 0%, #12161f 55%, #0a0c10 100%)",
    ),
    (
        "web-cam",
        "Web Cam",
        Some("/backgrounds/web-cam.jpg"),
        "radial-gradient(ellipse 90% 70% at 50% 40%, #2a3348 0%, #12161f 55%, #0a0c10 100%)",
    ),
    (
        "sports",
        "Sports",
        Some("/backgrounds/sports.jpg"),
        "radial-gradient(ellipse 90% 70% at 50% 40%, #1e3a5f 0%, #16181d 55%, #0a0c10 100%)",
    ),
    (
        "studio-dark",
        "Studio Dark",
        None,
        "radial-gradient(ellipse 90% 70% at 50% 40%, #2a3348 0%, #12161f 55%, #0a0c10 100%)",
    ),
    (
        "studio-blue",
        "Studio Blue",
        None,
        "radial-gradient(ellipse 85% 65% at 50% 35%, #3b82c4 0%, #1e3a5f 50%, #16181d 100%)",
    ),
    (
        "greenscreen",
        "Green Screen",
        None,
        "linear-gradient(180deg, #19c45a 0%, #00a83f 45%, #008a34 100%)",
    ),
    (
        "news-desk",
        "News Desk",
        None,
        "linear-gradient(165deg, #1a2744 0%, #16181d 40%, #1e293b 70%, #334155 100%)",
    ),
    (
        "soft-grey",
        "Soft Grey",
        None,
        "linear-gradient(180deg, #e2e8f0 0%, #cbd5e1 50%, #94a3b8 100%)",
    ),
];

/// CSS `background` for the scene's plate, or `None` when plates are off.
pub fn plate_css(scene: &framecraft_core::model::Scene) -> Option<String> {
    if scene.fake_background_enabled != Some(true) {
        return None;
    }
    if scene.fake_background_source.as_deref() == Some("media") {
        let url = scene
            .fake_background_media_url
            .as_deref()
            .unwrap_or("")
            .trim();
        return (!url.is_empty()).then(|| {
            format!(
                "center / cover no-repeat url('{}')",
                crate::preview::library_url(url)
            )
        });
    }
    let id = scene
        .fake_background_builtin_id
        .as_deref()
        .unwrap_or("news-room");
    let (_, _, image, css) = PLATES.iter().find(|p| p.0 == id).unwrap_or(&PLATES[0]);
    Some(match image {
        Some(img) => format!(
            "center / cover no-repeat url('{}'), {css}",
            crate::preview::library_url(img)
        ),
        None => css.to_string(),
    })
}

/// Scene → Plates (FRAME): an editor-only picture behind the graphic, to
/// judge it over a studio shot. Never exported.
#[component]
fn Plates(ui: Ui) -> Element {
    use framecraft_core::commands::{FieldTarget, SetField, Update};
    let scene = match ui.editor.read().as_ref() {
        Some(e) => e.doc().scene.clone(),
        None => return rsx! {},
    };
    let on = scene.fake_background_enabled == Some(true);
    let opacity = scene
        .fake_background_opacity
        .unwrap_or(100.0)
        .clamp(0.0, 100.0);
    let blur = scene.fake_background_blur.unwrap_or(0.0).clamp(0.0, 20.0);
    let media = scene.fake_background_source.as_deref() == Some("media");
    let builtin = scene
        .fake_background_builtin_id
        .clone()
        .unwrap_or_else(|| "news-room".into());
    let chosen = scene.fake_background_media_url.clone().unwrap_or_default();
    let set = move |path: &'static str, value: Value| {
        ui.execute(&SetField {
            target: FieldTarget::Scene,
            path: path.into(),
            value,
            update: Update::None,
        });
    };
    let images: Vec<(String, String)> = ui
        .editor
        .read()
        .as_ref()
        .map(|e| {
            framecraft_core::media::of_category(&e.project.media.pack, "images")
                .into_iter()
                .map(|m| (m.url.clone(), m.original_name.clone().unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        h3 {
            "Plates"
            if let Some(text) = schema::group_help(Tab::Scene, "Plates", None, false) {
                InfoTip { text }
            }
        }
        label { class: "field",
            span { class: "field-label", title: "Editor-only underlay plate behind your graphics. Never exported to OGraf / SPX / HTML.", "Show plate" }
            input {
                r#type: "checkbox",
                checked: on,
                onchange: move |_| set("fakeBackgroundEnabled", Value::from(!on)),
            }
            span {}
        }
        if on {
            label { class: "field",
                span { class: "field-label", "Opacity" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: "{opacity}",
                    onchange: move |evt| {
                        if let Ok(v) = evt.value().parse::<f64>() {
                            set("fakeBackgroundOpacity", Value::from(v));
                        }
                    },
                }
                span { class: "dim", "{opacity}" }
            }
            label { class: "field",
                span { class: "field-label", title: "Soften the plate so graphics stay readable. ~6–12 is typical; 20 is already heavy.", "Blur" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "20",
                    value: "{blur}",
                    onchange: move |evt| {
                        if let Ok(v) = evt.value().parse::<f64>() {
                            set("fakeBackgroundBlur", Value::from(v));
                        }
                    },
                }
                span { class: "dim", "{blur}" }
            }
            div { class: "seg plate-source",
                button {
                    class: if !media { "active" } else { "" },
                    onclick: move |_| set("fakeBackgroundSource", Value::from("builtin")),
                    "Built-in"
                }
                button {
                    class: if media { "active" } else { "" },
                    onclick: move |_| set("fakeBackgroundSource", Value::from("media")),
                    "My plates"
                }
            }
            div { class: "plate-grid",
                if !media {
                    for (id , label , image , css) in PLATES.iter().copied() {
                        button {
                            class: if builtin == id { "plate-tile active" } else { "plate-tile" },
                            title: "{label}",
                            onclick: move |_| {
                                set("fakeBackgroundBuiltinId", Value::from(id));
                            },
                            div {
                                class: "plate-thumb",
                                style: match image {
                                    Some(img) => format!("background: center / cover no-repeat url('{}'), {css};", crate::preview::library_url(img)),
                                    None => format!("background: {css};"),
                                },
                            }
                            span { "{label}" }
                        }
                    }
                } else {
                    if images.is_empty() {
                        p { class: "dim", "No images in this project. Import some in Project → Images." }
                    }
                    for (url , name) in images {
                        {
                            let u = url.clone();
                            rsx! {
                                button {
                                    class: if chosen == url { "plate-tile active" } else { "plate-tile" },
                                    title: "{name}",
                                    onclick: move |_| set("fakeBackgroundMediaUrl", Value::from(u.clone())),
                                    div {
                                        class: "plate-thumb",
                                        style: format!("background: center / cover no-repeat url('{}');", crate::preview::library_url(&url)),
                                    }
                                    span { "{name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Scene → Cues (FRAME's CueEditor): add cues at the playhead, jump between
/// them, and the list. Their fields are edited on the timeline.
#[component]
fn SceneCues(ui: Ui) -> Element {
    use framecraft_core::keys::{AddCue, RemoveCue, cue_label, new_cue};
    use framecraft_core::model::CueKind;
    let (mut cues, tracks) = match ui.editor.read().as_ref() {
        Some(e) => (
            e.doc().timeline.scene_cues.clone().unwrap_or_default(),
            e.doc().timeline.item_tracks.len(),
        ),
        None => return rsx! {},
    };
    cues.sort_by(|a, b| a.time.total_cmp(&b.time));
    let times: Vec<f64> = cues.iter().map(|c| c.time).collect();
    let (prev_times, next_times) = (times.clone(), times.clone());
    let add = move |kind: CueKind| {
        ui.execute(&AddCue {
            cue: new_cue(kind, *ui.time.peek()),
        });
    };
    rsx! {
        h3 {
            "Cues"
            if let Some(text) = schema::group_help(Tab::Scene, "Cues", None, false) {
                InfoTip { text }
            }
        }
        div { class: "cue-buttons",
            button { onclick: move |_| add(CueKind::Pause), "+ Pause" }
            button { onclick: move |_| add(CueKind::ApiCall), "+ API" }
            button { onclick: move |_| add(CueKind::Script), "+ Script" }
            button {
                title: "Jump to previous cue",
                onclick: move |_| {
                    let now = *ui.time.peek();
                    let t = prev_times.iter().rev().find(|t| **t < now - 0.001).or(prev_times.first());
                    if let Some(t) = t {
                        if *ui.playing.peek() {
                            ui.toggle_play();
                        }
                        ui.set_time(*t);
                    }
                },
                "◀"
            }
            button {
                title: "Jump to next cue",
                onclick: move |_| {
                    let now = *ui.time.peek();
                    let t = next_times.iter().find(|t| **t > now + 0.001).or(next_times.last());
                    if let Some(t) = t {
                        if *ui.playing.peek() {
                            ui.toggle_play();
                        }
                        ui.set_time(*t);
                    }
                },
                "▶"
            }
        }
        div { class: "script-summary",
            span { class: "dim", "Scene cues" }
            span { "{cues.len()}" }
            span { class: "dim", "Item tracks" }
            span { "{tracks}" }
        }
        if cues.is_empty() {
            p { class: "dim", "No cues yet." }
        }
        div { class: "cue-list",
            for c in cues {
                {
                    let (id, t) = (c.id.clone(), c.time);
                    let detail = match c.label.clone().filter(|l| !l.is_empty()) {
                        Some(l) => l,
                        None => match c.kind {
                            CueKind::Script => c.script_fn.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "—".into()),
                            CueKind::ApiCall => c.api_url.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "—".into()),
                            CueKind::Pause => String::new(),
                        },
                    };
                    rsx! {
                        div { class: "cue-row",
                            button {
                                class: "cue-seek",
                                title: "Move the playhead here",
                                onclick: move |_| ui.set_time(t),
                                span { class: "cue-kind", "{cue_label(c.kind)}" }
                                span { class: "dim", "{t.round()}ms" }
                                span { class: "cue-detail", "{detail}" }
                            }
                            button {
                                class: "key-button",
                                title: "Remove cue",
                                onclick: move |_| ui.execute(&RemoveCue { id: id.clone() }),
                                "✕"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// FRAME's ⓘ: hover (or click) shows a short explanation. The bubble is
/// placed on the window, so the scrolling panel does not clip it.
#[component]
pub fn InfoTip(text: &'static str) -> Element {
    let mut at = use_signal(|| None::<(f64, f64)>);
    let mut place = move |evt: MouseEvent| {
        let p = evt.client_coordinates();
        at.set(Some((p.x, p.y)));
    };
    rsx! {
        span {
            class: "info-tip",
            aria_label: "More information",
            onmouseenter: place,
            onmouseleave: move |_| at.set(None),
            onclick: move |evt| {
                evt.stop_propagation();
                evt.prevent_default();
                if at().is_some() { at.set(None) } else { place(evt) }
            },
            img {
                src: crate::preview::library_url("/icons/ui/info.svg"),
                alt: "",
                draggable: "false",
            }
        }
        if let Some((x, y)) = at() {
            div {
                class: "info-bubble",
                // Under the pointer, kept inside the window.
                style: "left:min(calc(100vw - 276px), max(8px, {x - 130.0}px));top:{y + 14.0}px;",
                "{text}"
            }
        }
    }
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque (the hex field).
fn hex_with_alpha(c: color::Rgba) -> String {
    let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    let a = (c.a.clamp(0.0, 1.0) * 255.0).round() as u8;
    if a == 255 {
        format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
    } else {
        format!("#{:02x}{:02x}{:02x}{a:02x}", b(c.r), b(c.g), b(c.b))
    }
}

/// The color picker's sliders (FRAME's RGBA picker): R, G, B (0–255) and
/// A (0–100 %) with tinted tracks and numbers, a preview and a hex field
/// (`#rrggbb`, `#rrggbbaa` or any CSS color). Sliders preview while
/// dragging and write one change on release.
#[component]
fn RgbaSliders(value: String, set: Callback<Value>) -> Element {
    let base = color::parse(&value).unwrap_or(color::Rgba {
        r: 255.0,
        g: 255.0,
        b: 255.0,
        a: 1.0,
    });
    let mut live = use_signal(|| None::<color::Rgba>);
    // A new value from outside ends any preview.
    use_effect(use_reactive!(|value| {
        let _ = &value;
        live.set(None);
    }));
    let c = live().unwrap_or(base);
    let (r, g, b) = (c.r.round(), c.g.round(), c.b.round());
    let a_pct = (c.a * 100.0).round();
    let preview = c.css();
    let hex = hex_with_alpha(c);
    let channels: [(&str, &str, f64, f64, String); 4] = [
        (
            "r",
            "R",
            r,
            255.0,
            format!("linear-gradient(90deg, rgb(0,{g},{b}), rgb(255,{g},{b}))"),
        ),
        (
            "g",
            "G",
            g,
            255.0,
            format!("linear-gradient(90deg, rgb({r},0,{b}), rgb({r},255,{b}))"),
        ),
        (
            "b",
            "B",
            b,
            255.0,
            format!("linear-gradient(90deg, rgb({r},{g},0), rgb({r},{g},255))"),
        ),
        (
            "a",
            "A",
            a_pct,
            100.0,
            format!(
                "linear-gradient(90deg, rgba({r},{g},{b},0), rgba({r},{g},{b},1)), repeating-conic-gradient(#888 0% 25%, #ccc 0% 50%) 50% / 8px 8px"
            ),
        ),
    ];
    let with = move |c: color::Rgba, ch: &str, v: f64| -> color::Rgba {
        let mut n = c;
        match ch {
            "r" => n.r = v.clamp(0.0, 255.0),
            "g" => n.g = v.clamp(0.0, 255.0),
            "b" => n.b = v.clamp(0.0, 255.0),
            _ => n.a = (v / 100.0).clamp(0.0, 1.0),
        }
        n
    };
    rsx! {
        div { class: "rgba-picker",
            div { class: "rgba-head",
                div { class: "rgba-preview", style: "--sw: {preview};" }
                input {
                    class: "rgba-hex",
                    r#type: "text",
                    value: "{hex}",
                    spellcheck: "false",
                    title: "Hex (#rrggbb or #rrggbbaa) or any CSS color",
                    onchange: move |evt| {
                        let t = evt.value();
                        let t = t.trim();
                        let t = if !t.starts_with('#') && t.len() >= 6 && t.chars().all(|ch| ch.is_ascii_hexdigit()) {
                            format!("#{t}")
                        } else {
                            t.to_string()
                        };
                        if let Some(c) = color::parse(&t) {
                            set(Value::from(c.css()));
                        }
                    },
                }
            }
            for (ch , label , v , max , track) in channels {
                div { class: "rgba-row",
                    span { class: "dim", "{label}" }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "{max}",
                        step: "1",
                        value: "{v}",
                        style: "--track: {track};",
                        oninput: move |evt| {
                            if let Ok(n) = evt.value().parse::<f64>() {
                                let cur = live.peek().unwrap_or(base);
                                live.set(Some(with(cur, ch, n)));
                            }
                        },
                        onchange: move |evt| {
                            if let Ok(n) = evt.value().parse::<f64>() {
                                let cur = live.peek().unwrap_or(base);
                                set(Value::from(with(cur, ch, n).css()));
                            }
                        },
                    }
                    input {
                        r#type: "number",
                        min: "0",
                        max: "{max}",
                        step: "1",
                        value: "{v}",
                        onchange: move |evt| {
                            if let Ok(n) = evt.value().trim().parse::<f64>() {
                                set(Value::from(with(base, ch, n).css()));
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Custom ruler positions (FRAME's Custom Rulers): percent of the scene,
/// each editable or removable; + Ruler adds one at 50 %, Sort orders them.
#[component]
fn Rulers(value: Value, set: Callback<Value>) -> Element {
    let list: Vec<f64> = value
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let round = |v: f64| (v.clamp(0.0, 100.0) * 100.0).round() / 100.0;
    let write = move |l: Vec<f64>| set(Value::Array(l.into_iter().map(Value::from).collect()));
    let (add, sort) = (list.clone(), list.clone());
    rsx! {
        div { class: "points",
            if list.is_empty() {
                p { class: "dim hint", "No rulers." }
            }
            for (i , v) in list.iter().copied().enumerate() {
                {
                    let (a, b) = (list.clone(), list.clone());
                    rsx! {
                        div { class: "point ruler",
                            span { class: "dim", "{i + 1}" }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "100",
                                step: "0.1",
                                value: "{number_to_string(v)}",
                                onchange: move |evt| {
                                    if let Ok(n) = evt.value().trim().parse::<f64>() {
                                        let mut l = a.clone();
                                        l[i] = round(n);
                                        write(l);
                                    }
                                },
                            }
                            span { class: "dim", "%" }
                            button {
                                class: "key-button",
                                title: "Remove ruler",
                                onclick: move |_| {
                                    let mut l = b.clone();
                                    l.remove(i);
                                    write(l);
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            div { class: "row-buttons",
                button {
                    onclick: move |_| {
                        let mut l = add.clone();
                        l.push(50.0);
                        write(l);
                    },
                    "+ Ruler"
                }
                button {
                    disabled: list.len() < 2,
                    onclick: move |_| {
                        let mut l = sort.clone();
                        l.sort_by(f64::total_cmp);
                        write(l);
                    },
                    "Sort"
                }
            }
        }
    }
}

/// FRAME's color picker on a chip: the active palette, stock palettes and
/// the RGBA sliders in a popover placed where the chip was clicked (on the
/// window, so scrolling panels do not clip it). `on_change` gets a CSS
/// color (`#rrggbb`, or `rgba(…)` with alpha).
#[component]
pub fn ColorButton(
    ui: Ui,
    value: String,
    on_change: Callback<String>,
    #[props(default = "Pick a color".to_string(), into)] title: String,
) -> Element {
    let mut open = use_signal(|| false);
    let mut stock = use_signal(|| false);
    let mut at = use_signal(|| (0.0_f64, 0.0_f64));
    let (palette_name, swatches): (String, Vec<(String, String)>) = ui
        .palettes
        .read()
        .active()
        .map(|p| {
            let list = p
                .colors
                .iter()
                .map(|c| {
                    let tip = [
                        c.label.clone().unwrap_or_default(),
                        p.name.clone(),
                        c.hex.clone(),
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                    (framecraft_core::palettes::swatch_css(&c.hex, c.alpha), tip)
                })
                .collect();
            (p.name.clone(), list)
        })
        .unwrap_or_default();
    let set = use_callback(move |v: Value| {
        if let Some(c) = v.as_str() {
            on_change(c.to_string());
        }
    });
    rsx! {
        button {
            class: "color-chip",
            r#type: "button",
            title: "{title}",
            style: "--sw: {value};",
            onclick: move |evt| {
                evt.prevent_default();
                let p = evt.client_coordinates();
                at.set((p.x, p.y));
                open.toggle();
            },
        }
        if open() {
            // A click outside closes it (FRAME); a drag that leaves the
            // popover keeps going.
            div { class: "popover-catcher", onclick: move |_| open.set(false) }
            div {
                class: "color-popover",
                style: "left:min(calc(100vw - 276px), max(8px, {at().0 - 20.0}px));top:min(calc(100vh - 350px), {at().1 + 16.0}px);",
                // FRAME: the active palette first, then stock.
                if !swatches.is_empty() {
                    div { class: "palette-label", "{palette_name}" }
                    div { class: "swatches",
                        for (css , tip) in swatches {
                            button {
                                class: "swatch-btn",
                                r#type: "button",
                                title: "{tip}",
                                style: "--sw: {css};",
                                onclick: move |_| {
                                    open.set(false);
                                    on_change(css.clone());
                                },
                            }
                        }
                    }
                }
                button {
                    class: "stock-btn",
                    r#type: "button",
                    onclick: move |_| {
                        open.set(false);
                        stock.set(true);
                    },
                    "Select from stock palette…"
                }
                RgbaSliders { value: value.clone(), set }
            }
        }
        if stock() {
            crate::palettes::StockPaletteDialog {
                on_close: move |_| stock.set(false),
                on_color: move |c: framecraft_core::palettes::PaletteColor| {
                    stock.set(false);
                    on_change(framecraft_core::palettes::swatch_css(&c.hex, c.alpha));
                },
                on_palette: None,
            }
        }
    }
}

/// FRAME's resolution presets above Width / Height: picking one sets both
/// (one undo step); "Custom" when the size matches none.
#[component]
fn ResolutionPreset(ui: Ui) -> Element {
    use framecraft_core::commands::{Batch, Command, FieldTarget, SetField, Update};
    use framecraft_core::resolutions::{GROUPS, PRESETS, find};
    let Some((w, h)) = ui
        .editor
        .read()
        .as_ref()
        .map(|e| (e.doc().scene.width, e.doc().scene.height))
    else {
        return rsx! {};
    };
    let current = find(w, h).map_or("custom", |p| p.id);
    rsx! {
        label { class: "field",
            span { class: "field-label", title: "The viewport and exports use these dimensions immediately.", "Preset" }
            select {
                onchange: move |evt| {
                    let Some(p) = PRESETS.iter().find(|p| p.id == evt.value()) else {
                        return;
                    };
                    let set = |path: &str, v: u32| -> Box<dyn Command> {
                        Box::new(SetField {
                            target: FieldTarget::Scene,
                            path: path.into(),
                            value: Value::from(v),
                            update: Update::Rebuild,
                        })
                    };
                    ui.execute(&Batch {
                        label: format!("Resolution {}", p.label),
                        commands: vec![set("width", p.width), set("height", p.height)],
                    });
                },
                for (g , glabel) in GROUPS.iter().copied() {
                    optgroup { label: "{glabel}",
                        for p in PRESETS.iter().filter(|p| p.group == g) {
                            option { value: "{p.id}", selected: p.id == current, "{p.label} ({p.width} × {p.height})" }
                        }
                    }
                }
                option { value: "custom", selected: current == "custom", "Custom" }
            }
            span {}
        }
    }
}

/// FRAME's gradient center control: a chip with the gradient that opens a
/// pad; drag on the pad (or type X / Y in %) to move the center. The drag
/// previews live and writes one change on release.
#[component]
fn GradientCenter(value: Value, item: Value, set: Callback<Value>) -> Element {
    use framecraft_core::model::RampColor;
    const PAD_W: f64 = 220.0;
    const PAD_H: f64 = 124.0;
    let mut open = use_signal(|| false);
    let mut at = use_signal(|| (0.0_f64, 0.0_f64));
    let mut live = use_signal(|| None::<(f64, f64)>);
    let saved = (
        value["x"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0),
        value["y"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0),
    );
    let (cx, cy) = live().unwrap_or(saved);
    let stops: Option<Vec<RampColor>> = serde_json::from_value(item["gradientRamp"].clone()).ok();
    let stops = framecraft_core::ramp::normalize_color_ramp(stops.as_deref());
    let kind = item["gradientType"]
        .as_str()
        .unwrap_or("circle")
        .to_string();
    let angle = item["gradientAngle"].as_f64().unwrap_or(90.0);
    let bg = framecraft_core::export::css::gradient_css(&stops, &kind, angle, (cx, cy));
    let write = move |x: f64, y: f64| {
        let r = |v: f64| (v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
        set(json!({ "x": r(x), "y": r(y) }));
    };
    let pick = move |e: Event<PointerData>| {
        let p = e.element_coordinates();
        ((p.x / PAD_W).clamp(0.0, 1.0), (p.y / PAD_H).clamp(0.0, 1.0))
    };
    rsx! {
        div { class: "gradient-center",
            button {
                class: "gradient-center-chip",
                r#type: "button",
                title: "Set gradient center",
                style: "background-image:{bg};",
                onclick: move |evt| {
                    evt.prevent_default();
                    let p = evt.client_coordinates();
                    at.set((p.x, p.y));
                    open.toggle();
                },
            }
            span { class: "dim", "{(cx * 100.0).round()} %, {(cy * 100.0).round()} %" }
        }
        if open() {
            div { class: "popover-catcher", onclick: move |_| open.set(false) }
            div {
                class: "color-popover gradient-center-pop",
                style: "left:min(calc(100vw - 256px), max(8px, {at().0 - 20.0}px));top:min(calc(100vh - 230px), {at().1 + 16.0}px);",
                div {
                    class: "gradient-pad",
                    style: "width:{PAD_W}px;height:{PAD_H}px;background-image:{bg};",
                    onpointerdown: move |e| {
                        let p = pick(e);
                        live.set(Some(p));
                    },
                    onpointermove: move |e| {
                        if live.peek().is_some() && !e.held_buttons().is_empty() {
                            let p = pick(e);
                            live.set(Some(p));
                        }
                    },
                    onpointerup: move |_| {
                        if let Some((x, y)) = live.take() {
                            write(x, y);
                        }
                    },
                    onpointerleave: move |_| {
                        if let Some((x, y)) = live.take() {
                            write(x, y);
                        }
                    },
                    span { class: "crosshair", style: "left:{cx * 100.0}%;top:{cy * 100.0}%;" }
                }
                div { class: "gradient-readouts",
                    label {
                        "X "
                        input {
                            r#type: "number",
                            min: "0",
                            max: "100",
                            step: "1",
                            value: "{(cx * 100.0).round()}",
                            onchange: move |evt| {
                                if let Ok(v) = evt.value().trim().parse::<f64>() {
                                    write(v / 100.0, cy);
                                }
                            },
                        }
                        " %"
                    }
                    label {
                        "Y "
                        input {
                            r#type: "number",
                            min: "0",
                            max: "100",
                            step: "1",
                            value: "{(cy * 100.0).round()}",
                            onchange: move |evt| {
                                if let Ok(v) = evt.value().trim().parse::<f64>() {
                                    write(cx, v / 100.0);
                                }
                            },
                        }
                        " %"
                    }
                }
            }
        }
    }
}

/// FRAME's Border Mode: Single, or 4-sided (top, right, bottom, left).
/// Switching copies the widths over (single → all four sides; sides → the
/// top one), as one undo step.
#[component]
fn BorderMode(ui: Ui, item: String) -> Element {
    use framecraft_core::commands::{Batch, Command, FieldTarget, SetField, Update};
    let Some((sides, width, widths)) = ui.editor.read().as_ref().and_then(|e| {
        let i = e.doc().items.iter().find(|i| i.id == item)?;
        Some((
            i.border_width_mode.as_deref() == Some("sides"),
            i.border_width,
            i.border_widths,
        ))
    }) else {
        return rsx! {};
    };
    let id = item.clone();
    rsx! {
        label { class: "field",
            span { class: "field-label", title: "Single: one width for every side (animatable). 4-sided: top, right, bottom and left apart.", "Mode" }
            select {
                onchange: move |evt| {
                    let to_sides = evt.value() == "sides";
                    if to_sides == sides {
                        return;
                    }
                    let set = |path: &str, value: Value| -> Box<dyn Command> {
                        Box::new(SetField {
                            target: FieldTarget::Item(id.clone()),
                            path: path.into(),
                            value,
                            update: Update::Layer,
                        })
                    };
                    let commands = if to_sides {
                        vec![
                            set("borderWidths", json!(widths.unwrap_or([width; 4]))),
                            set("borderWidthMode", json!("sides")),
                        ]
                    } else {
                        vec![
                            set("borderWidth", json!(widths.map_or(width, |w| w[0]))),
                            set("borderWidthMode", json!("single")),
                        ]
                    };
                    ui.execute(&Batch {
                        label: if to_sides { "4-sided border".into() } else { "Single border".into() },
                        commands,
                    });
                },
                option { value: "single", selected: !sides, "Single" }
                option { value: "sides", selected: sides, "4-sided" }
            }
            span {}
        }
    }
}
