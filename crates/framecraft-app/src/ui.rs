// SPDX-License-Identifier: AGPL-3.0-or-later
//! The editor's panels: header, viewport, properties and timeline.

use crate::bridge::{self, FRAME_A, FRAME_B};
use crate::state::{Editor, Ui};
use dioxus::prelude::*;
use framecraft_core::commands::{SetKeyframe, SetText, SetValueAt};
use framecraft_core::model::{Item, ItemType};
use framecraft_core::normalize::CLIP_START;
use framecraft_core::timeline::{base_value, lane_value};

/// Timeline scale: pixels per millisecond (10 s = 1000 px). The bridge
/// uses the same numbers to move the playhead during playback.
const PX_PER_MS: f64 = 0.1;
const LABEL_WIDTH: f64 = 200.0;

/// Zooms the viewport so the whole scene fits.
pub async fn fit(mut ui: Ui) {
    let size = document::eval(
        "const v = document.getElementById('fc-viewport'); return v ? [v.clientWidth, v.clientHeight] : [0, 0];",
    )
    .join::<Vec<f64>>()
    .await;
    let (sw, sh) = match ui.editor.read().as_ref() {
        Some(e) => (e.doc().scene.width.max(1.0), e.doc().scene.height.max(1.0)),
        None => return,
    };
    if let Ok(s) = size
        && s.len() == 2
        && s[0] > 0.0
    {
        ui.zoom.set(
            ((s[0] - 32.0) / sw)
                .min((s[1] - 32.0) / sh)
                .clamp(0.05, 2.0),
        );
    }
}

#[component]
pub fn Header(ui: Ui) -> Element {
    let (graphics, current, undo, redo) = match ui.editor.read().as_ref() {
        Some(e) => (
            (0..e.project.docs.len())
                .map(|i| e.project.graphic_name(i).to_string())
                .collect::<Vec<_>>(),
            e.graphic,
            e.history().undo_label().map(String::from),
            e.history().redo_label().map(String::from),
        ),
        None => (Vec::new(), 0, None, None),
    };
    let has_project = !graphics.is_empty();

    let export = move |format: &'static str| {
        let mut status = ui.status;
        let result = ui
            .editor
            .read()
            .as_ref()
            .map(|e| e.project.export(e.graphic, format));
        match result {
            Some(Ok(path)) => status.set(format!("Exported {}", path.display())),
            Some(Err(e)) => status.set(e),
            None => {}
        }
    };

    rsx! {
        div { class: "header",
            span { class: "brand", "FrameCraft" }
            label { class: "button",
                "Open…"
                input {
                    r#type: "file",
                    accept: ".zip",
                    onchange: move |evt| {
                        if let Some(file) = evt.files().first() {
                            ui.open(file.path());
                        }
                    },
                }
            }
            if has_project {
                select {
                    onchange: move |evt| {
                        if let Ok(i) = evt.value().parse::<usize>() {
                            ui.select_graphic(i);
                        }
                    },
                    for (i , name) in graphics.iter().enumerate() {
                        option { value: "{i}", selected: i == current, "{name}" }
                    }
                }
                button {
                    disabled: undo.is_none(),
                    title: undo.as_deref().map(|l| format!("Undo {l} (Ctrl+Z)")).unwrap_or_default(),
                    onclick: move |_| ui.undo(),
                    "↶ Undo"
                }
                button {
                    disabled: redo.is_none(),
                    title: redo.as_deref().map(|l| format!("Redo {l} (Ctrl+Shift+Z)")).unwrap_or_default(),
                    onclick: move |_| ui.redo(),
                    "↷ Redo"
                }
                button { onclick: move |_| export("spx"), "Export SPX" }
                button { onclick: move |_| export("ograf"), "Export OGraf" }
                button { onclick: move |_| export("banner"), "Export banner" }
                button {
                    onclick: move |_| {
                        let mut status = ui.status;
                        let result = ui.editor.read().as_ref().map(|e| e.project.save_copy());
                        match result {
                            Some(Ok(path)) => status.set(format!("Saved {}", path.display())),
                            Some(Err(e)) => status.set(e),
                            None => {}
                        }
                    },
                    "Save copy"
                }
            }
            span { class: "status", "{ui.status}" }
        }
    }
}

#[component]
pub fn Viewport(ui: Ui) -> Element {
    let (w, h, open) = match ui.editor.read().as_ref() {
        Some(e) => (e.doc().scene.width, e.doc().scene.height, true),
        None => (1920.0, 1080.0, false),
    };
    let zoom = (ui.zoom)();
    let frame_style = format!("width:{w}px;height:{h}px;transform:scale({zoom});");
    rsx! {
        div { class: "viewport-panel",
            div { class: "viewport-tools",
                for z in [0.25, 0.5, 0.75, 1.0] {
                    button {
                        class: if (zoom - z).abs() < 1e-6 { "active" } else { "" },
                        onclick: move |_| {
                            let mut zoom = ui.zoom;
                            zoom.set(z);
                        },
                        "{z * 100.0}%"
                    }
                }
                button { onclick: move |_| { spawn(fit(ui)); }, "Fit" }
                span { class: "dim", "{w}×{h}" }
            }
            div { class: "viewport", id: "fc-viewport",
                // Both frames always exist; the bridge sets their src and
                // shows one (class "active"). Dioxus never touches either.
                div {
                    class: "stage",
                    style: if open { "width:{w * zoom}px;height:{h * zoom}px;" } else { "display:none;" },
                    iframe { id: FRAME_A, style: "{frame_style}" }
                    iframe { id: FRAME_B, style: "{frame_style}" }
                }
                if !open {
                    div { class: "empty", "No graphic open" }
                }
            }
        }
    }
}

/// Number fields the properties tab edits; each is also a timeline lane.
const FIELDS: [(&str, &str); 8] = [
    ("x", "X"),
    ("y", "Y"),
    ("width", "Width"),
    ("height", "Height"),
    ("rotation", "Rotation"),
    ("opacity", "Opacity"),
    ("scaleX", "Scale X"),
    ("scaleY", "Scale Y"),
];

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[component]
pub fn Properties(ui: Ui) -> Element {
    let time = (ui.time)();
    let editor = ui.editor.read();
    let Some(ed) = editor.as_ref() else {
        return rsx! { div { class: "properties", h3 { "Transform" } p { class: "dim", "Nothing open" } } };
    };
    let Some(item) = ed
        .selected
        .as_ref()
        .and_then(|id| ed.doc().items.iter().find(|i| &i.id == id))
    else {
        return rsx! { div { class: "properties", h3 { "Transform" } p { class: "dim", "Select a layer in the timeline" } } };
    };
    let id = item.id.clone();
    let doc = ed.doc();
    // Values at the playhead: animated lanes show their animated value.
    let values: Vec<(&str, &str, f64, bool)> = FIELDS
        .iter()
        .map(|(lane, label)| {
            let animated = lane_value(doc, &id, lane, time);
            let v = animated.or_else(|| base_value(item, lane)).unwrap_or(0.0);
            (*lane, *label, round3(v), animated.is_some())
        })
        .collect();
    let text = (item.item_type == Some(ItemType::Text))
        .then(|| item.text_content.clone().unwrap_or_default());
    let name = item.name.clone();
    let kind = format!("{:?}", item.item_type.unwrap_or(ItemType::Shape));
    drop(editor);

    rsx! {
        div { class: "properties",
            h3 { "Transform" }
            p { class: "layer-name", "{name} " span { class: "dim", "{kind}" } }
            for (lane , label , value , is_animated) in values {
                {
                    let id = id.clone();
                    rsx! {
                        label { class: "field",
                            span { "{label}" }
                            input {
                                r#type: "number",
                                step: "any",
                                value: "{value}",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().parse::<f64>() {
                                        ui.execute(&SetValueAt {
                                            item: id.clone(),
                                            lane: lane.to_string(),
                                            time_ms: (ui.time)(),
                                            value: v,
                                            auto_key: (ui.auto_key)(),
                                        });
                                    }
                                },
                            }
                            {
                                let id = id.clone();
                                rsx! {
                                    button {
                                        class: if is_animated { "key-button animated" } else { "key-button" },
                                        title: if is_animated {
                                            "Animated. Click to set a keyframe at the playhead."
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
                                        if is_animated { "◆" } else { "◇" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(text) = text {
                {
                    let id = id.clone();
                    rsx! {
                        h3 { "Text" }
                        textarea {
                            value: "{text}",
                            onchange: move |evt| ui.execute(&SetText { item: id.clone(), text: evt.value() }),
                        }
                    }
                }
            }
        }
    }
}

/// A timeline row: one layer, indented under its parent.
struct Row {
    id: String,
    name: String,
    depth: usize,
    keys: Vec<f64>,
}

fn rows(ed: &Editor) -> Vec<Row> {
    let doc = ed.doc();
    fn visit<'a>(
        items: &'a [Item],
        parent: Option<&str>,
        depth: usize,
        out: &mut Vec<(&'a Item, usize)>,
    ) {
        for item in items
            .iter()
            .filter(|i| i.parent_id.clone().flatten().as_deref() == parent)
        {
            out.push((item, depth));
            if depth < 32 {
                visit(items, Some(&item.id), depth + 1, out);
            }
        }
    }
    let mut ordered = Vec::new();
    visit(&doc.items, None, 0, &mut ordered);
    ordered
        .into_iter()
        .map(|(item, depth)| {
            let mut keys: Vec<f64> = doc
                .timeline
                .item_tracks
                .iter()
                .filter(|t| t.item_id == item.id)
                .flat_map(|t| {
                    t.properties
                        .iter()
                        .filter(|(p, _)| p.as_str() != CLIP_START)
                        .flat_map(|(_, k)| k.iter().map(|k| k.time))
                })
                .collect();
            keys.sort_by(f64::total_cmp);
            keys.dedup();
            Row {
                id: item.id.clone(),
                name: item.name.clone(),
                depth,
                keys,
            }
        })
        .collect()
}

#[component]
pub fn Timeline(ui: Ui) -> Element {
    let duration = ui.duration();
    let time = (ui.time)();
    let playing = (ui.playing)();
    let (rows, selected) = match ui.editor.read().as_ref() {
        Some(e) => (rows(e), e.selected.clone()),
        None => (Vec::new(), None),
    };
    let width = duration * PX_PER_MS;
    let seconds = (duration / 1000.0).ceil() as usize;

    let set_time = move |t: f64| {
        let (mut time, mut playing) = (ui.time, ui.playing);
        if playing() {
            bridge::stop(false);
        }
        playing.set(false);
        let t = t.clamp(0.0, duration);
        time.set(t);
        bridge::seek(t);
    };

    let toggle = move |_| {
        let mut playing = ui.playing;
        if playing() {
            // Pause: the bridge reports the time it stopped at.
            bridge::stop(true);
            return;
        }
        playing.set(true);
        let from = if (ui.time)() >= duration {
            0.0
        } else {
            (ui.time)()
        };
        spawn(async move {
            let stopped_at = bridge::play(from, duration).await;
            let (mut time, mut playing) = (ui.time, ui.playing);
            if let Some(t) = stopped_at {
                time.set(t);
            }
            playing.set(false);
        });
    };

    rsx! {
        div { class: "timeline",
            div { class: "transport",
                button { onclick: move |_| set_time(0.0), "⏮" }
                button { onclick: toggle, if playing { "⏸ Pause" } else { "▶ Play" } }
                button {
                    class: if (ui.auto_key)() { "auto-key on" } else { "auto-key" },
                    title: "Auto-key: every property edit writes a keyframe at the playhead",
                    onclick: move |_| {
                        let mut auto_key = ui.auto_key;
                        auto_key.set(!auto_key());
                    },
                    span { class: "dot" }
                    "Auto-key"
                }
                span { class: "time",
                    span { id: "fc-time", "{time / 1000.0:.2}" }
                    " s / {duration / 1000.0:.2} s"
                }
            }
            div { class: "tl-body",
                div { class: "tl-inner", style: "width:{LABEL_WIDTH + width + 40.0}px;",
                    div { class: "tl-row ruler",
                        div { class: "label" }
                        div {
                            class: "track",
                            style: "width:{width}px;",
                            onmousedown: move |evt| set_time(evt.element_coordinates().x / PX_PER_MS),
                            onmousemove: move |evt| {
                                if evt.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                                    set_time(evt.element_coordinates().x / PX_PER_MS);
                                }
                            },
                            for s in 0..=seconds {
                                div { class: "tick", style: "left:{s as f64 * 1000.0 * PX_PER_MS}px;", "{s}s" }
                            }
                        }
                    }
                    for row in rows {
                        {
                            let id = row.id.clone();
                            let is_selected = selected.as_deref() == Some(row.id.as_str());
                            rsx! {
                                div { class: if is_selected { "tl-row selected" } else { "tl-row" },
                                    div {
                                        class: "label",
                                        style: "padding-left:{8 + row.depth * 14}px;",
                                        onclick: move |_| ui.select_layer(Some(id.clone())),
                                        "{row.name}"
                                    }
                                    div { class: "track", style: "width:{width}px;",
                                        for k in row.keys {
                                            div { class: "key", style: "left:{k * PX_PER_MS}px;", onclick: move |_| set_time(k) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        id: "fc-playhead",
                        class: "playhead",
                        style: "left:{LABEL_WIDTH + time * PX_PER_MS}px;",
                    }
                }
            }
        }
    }
}
