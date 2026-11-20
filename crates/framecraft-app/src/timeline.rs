// SPDX-License-Identifier: AGPL-3.0-or-later
//! The timeline, as in FRAME:
//!
//! - **Keys** view: the selected layer's animated lanes, one row each; with
//!   no layer selected, the scene's cue rows (Pause, API, Script).
//! - **Dope** view: one row per layer, all its keys, and a bar from its
//!   first to its last key.
//!
//! Keys and cues are selected, box-selected and dragged in the page (see
//! the bridge); Rust hears the result as `{type: "tl", action, …}` events
//! and turns them into commands.

use crate::bridge;
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::commands::{FieldTarget, SetField, SetKeyframe, Update};
use framecraft_core::easing::EASING_IDS;
use framecraft_core::keys::{
    AddCue, CUE_KINDS, KeyRef, PasteKeys, RemoveCue, RemoveKeys, RetimeKeys, SetCueField,
    SetKeysEasing, copy_keys, cue_label, key_at, lane_label, new_cue, same_time, shown_lanes,
    snap_time, value_now,
};
use framecraft_core::model::{CueKind, Item};
use framecraft_core::normalize::{CLIP_START, Document};
use serde_json::{Value, json};

/// Width of the row labels; the bridge draws the playhead from it.
pub const LABEL_WIDTH: f64 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlView {
    Keys,
    Dope,
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// `ease-in-out-back` → `In Out Back`.
pub(crate) fn easing_label(id: &str) -> String {
    if id == "linear" {
        return "Linear".into();
    }
    id.trim_start_matches("ease-")
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parent_of(item: &Item) -> Option<String> {
    item.parent_id.clone().flatten()
}

/// Every key of a layer, all lanes.
fn all_keys(doc: &Document, item: &str) -> Vec<KeyRef> {
    doc.timeline
        .item_tracks
        .iter()
        .filter(|t| t.item_id == item)
        .flat_map(|t| {
            t.properties
                .iter()
                .filter(|(lane, _)| lane.as_str() != CLIP_START)
                .flat_map(|(lane, keys)| keys.iter().map(|k| KeyRef::new(item, lane, k.time)))
        })
        .collect()
}

/// Keys of a layer at one time (a Dope view diamond).
fn keys_at(doc: &Document, item: &str, time: f64) -> Vec<KeyRef> {
    all_keys(doc, item)
        .into_iter()
        .filter(|k| same_time(k.time, time))
        .collect()
}

#[derive(Clone, PartialEq)]
struct LaneKey {
    time: f64,
    selected: bool,
}

#[derive(Clone, PartialEq)]
enum Row {
    Lane {
        item: String,
        lane: &'static str,
        value: Option<f64>,
        keys: Vec<LaneKey>,
        /// Spans where the value changes (between keys of different value),
        /// each with the times of its whole tween (FRAME: dragging a span
        /// slides every key linked by value changes).
        moves: Vec<(f64, f64, String)>,
        key_here: bool,
    },
    Cues {
        kind: CueKind,
        cues: Vec<(String, f64, String, bool)>,
    },
    Layer {
        item: String,
        name: String,
        depth: usize,
        keys: Vec<LaneKey>,
        span: Option<(f64, f64)>,
        selected: bool,
    },
}

fn layer_rows(doc: &Document, selected: Option<&str>, sel: &[KeyRef]) -> Vec<Row> {
    fn visit(doc: &Document, parent: Option<&str>, depth: usize, out: &mut Vec<(String, usize)>) {
        let children: Vec<&Item> = doc
            .items
            .iter()
            .filter(|i| parent_of(i).as_deref() == parent)
            .collect();
        for item in children.into_iter().rev() {
            out.push((item.id.clone(), depth));
            if depth < 32 {
                visit(doc, Some(&item.id), depth + 1, out);
            }
        }
    }
    let mut order = Vec::new();
    visit(doc, None, 0, &mut order);
    order
        .into_iter()
        .filter_map(|(id, depth)| {
            let item = doc.items.iter().find(|i| i.id == id)?;
            let mut times: Vec<f64> = all_keys(doc, &id).into_iter().map(|k| k.time).collect();
            times.sort_by(f64::total_cmp);
            times.dedup_by(|a, b| same_time(*a, *b));
            let span = match (times.first(), times.last()) {
                (Some(a), Some(b)) if b > a => Some((*a, *b)),
                _ => None,
            };
            Some(Row::Layer {
                keys: times
                    .iter()
                    .map(|t| LaneKey {
                        time: *t,
                        selected: sel.iter().any(|k| k.item == id && same_time(k.time, *t)),
                    })
                    .collect(),
                selected: selected == Some(id.as_str()),
                name: item.name.clone(),
                item: id,
                depth,
                span,
            })
        })
        .collect()
}

fn lane_rows(doc: &Document, item: &Item, time: f64, sel: &[KeyRef]) -> Vec<Row> {
    let track = doc
        .timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == item.id);
    let now = snap_time(doc, time);
    shown_lanes(doc, &item.id)
        .into_iter()
        .map(|lane| {
            let keys = track
                .and_then(|t| t.properties.get(lane))
                .cloned()
                .unwrap_or_default();
            let differ = |i: usize| (keys[i].value - keys[i + 1].value).abs() > 1e-9;
            let moves = (0..keys.len().saturating_sub(1))
                .filter(|&i| differ(i))
                .map(|i| {
                    let (mut lo, mut hi) = (i, i + 1);
                    while lo > 0 && differ(lo - 1) {
                        lo -= 1;
                    }
                    while hi + 1 < keys.len() && differ(hi) {
                        hi += 1;
                    }
                    let times = keys[lo..=hi]
                        .iter()
                        .map(|k| k.time.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    (keys[i].time, keys[i + 1].time, times)
                })
                .collect();
            Row::Lane {
                item: item.id.clone(),
                lane,
                value: value_now(doc, item, lane, time).map(round3),
                key_here: keys.iter().any(|k| same_time(k.time, now)),
                keys: keys
                    .iter()
                    .map(|k| LaneKey {
                        time: k.time,
                        selected: sel.iter().any(|s| {
                            s.item == item.id && s.lane == lane && same_time(s.time, k.time)
                        }),
                    })
                    .collect(),
                moves,
            }
        })
        .collect()
}

fn cue_rows(doc: &Document, selected: Option<&str>) -> Vec<Row> {
    CUE_KINDS
        .iter()
        .map(|kind| Row::Cues {
            kind: *kind,
            cues: doc
                .timeline
                .scene_cues
                .iter()
                .flatten()
                .filter(|c| c.kind == *kind)
                .map(|c| {
                    let label = c.label.clone().unwrap_or_default();
                    (c.id.clone(), c.time, label, selected == Some(c.id.as_str()))
                })
                .collect(),
        })
        .collect()
}

#[component]
pub fn Timeline(ui: Ui) -> Element {
    let duration = ui.duration();
    let time = (ui.time)();
    let playing = (ui.playing)();
    let px = (ui.tl_zoom)();
    let view = (ui.tl_view)();
    let sel = ui.keys.read().clone();
    let cue_sel = ui.cue.read().clone();

    // Keep the bridge's playhead scale and snap in step with ours.
    use_effect(move || {
        bridge::set_timeline_scale((ui.tl_zoom)(), LABEL_WIDTH);
    });
    use_effect(move || {
        let step = ui.editor.read().as_ref().map_or(0.0, |e| {
            let tl = &e.doc().timeline;
            if tl.snap_enabled == Some(true) {
                tl.snap_step.unwrap_or(1.0).max(1.0)
            } else {
                0.0
            }
        });
        bridge::set_snap(step);
    });

    let (rows, snap, snap_step, layer_selected) = match ui.editor.read().as_ref() {
        Some(e) => {
            let doc = e.doc();
            let item = e
                .selected
                .as_ref()
                .and_then(|id| doc.items.iter().find(|i| &i.id == id));
            let rows = match (view, item) {
                (TlView::Dope, _) => layer_rows(doc, e.selected.as_deref(), &sel),
                (TlView::Keys, Some(item)) => lane_rows(doc, item, time, &sel),
                (TlView::Keys, None) => cue_rows(doc, cue_sel.as_deref()),
            };
            (
                rows,
                doc.timeline.snap_enabled == Some(true),
                doc.timeline.snap_step.unwrap_or(100.0),
                item.is_some(),
            )
        }
        None => (Vec::new(), false, 100.0, false),
    };
    let width = duration * px;
    // Ruler ticks: every second, or every 5 / 10 when zoomed out.
    let tick_s = if px >= 0.05 {
        1
    } else if px >= 0.02 {
        5
    } else {
        10
    };
    let seconds = (duration / 1000.0).ceil() as usize;
    let empty_hint = match (view, layer_selected) {
        (TlView::Keys, true) => {
            "No animated properties. Click ◇ next to a field in Transform (or press K) to add a keyframe."
        }
        _ => "",
    };

    rsx! {
        div { class: "timeline",
            div { class: "transport",
                button { title: "To start", onclick: move |_| ui.set_time(0.0), "⏮" }
                button { title: "Play / Pause (Space)", onclick: move |_| ui.toggle_play(),
                    if playing { "⏸ Pause" } else { "▶ Play" }
                }
                span { class: "time",
                    span { id: "fc-time", "{time / 1000.0:.2}" }
                    " s / {duration / 1000.0:.2} s"
                }
                button {
                    class: if (ui.auto_key)() { "auto-key on" } else { "auto-key" },
                    title: "Auto-key on: edits write keyframes at the playhead. Off: edits shift existing animations and add no keyframes.",
                    onclick: move |_| {
                        let mut auto_key = ui.auto_key;
                        auto_key.set(!auto_key());
                    },
                    span { class: "dot" }
                    "Auto-key"
                }
                button {
                    title: "Motion library: apply a ready-made or saved motion to the selected layer at the playhead",
                    disabled: ui.editor.read().is_none(),
                    onclick: move |_| {
                        let mut open = ui.motion_dialog;
                        open.set(Some("apply"));
                    },
                    "Motions"
                }
                button {
                    title: "Save the selected keyframes (one layer) as a reusable motion",
                    disabled: !crate::motions::can_save(ui),
                    onclick: move |_| {
                        let mut open = ui.motion_dialog;
                        open.set(Some("save"));
                    },
                    "Save motion"
                }
                button {
                    class: if snap { "active" } else { "" },
                    title: "Snap keyframes and cues to the step",
                    onclick: move |_| {
                        ui.execute(&SetField {
                            target: FieldTarget::Timeline,
                            path: "snapEnabled".into(),
                            value: (!snap).into(),
                            update: Update::None,
                        });
                    },
                    "Snap"
                }
                input {
                    class: "snap-step",
                    r#type: "number",
                    min: "1",
                    step: "10",
                    title: "Snap step (ms)",
                    value: "{snap_step}",
                    onchange: move |evt| {
                        if let Ok(v) = evt.value().trim().parse::<f64>() {
                            ui.execute(&SetField {
                                target: FieldTarget::Timeline,
                                path: "snapStep".into(),
                                value: json!(v.max(1.0).round()),
                                update: Update::None,
                            });
                        }
                    },
                }
                span { class: "dim", "ms" }
                div { class: "spacer" }
                div { class: "seg",
                    button {
                        class: if view == TlView::Keys { "active" } else { "" },
                        title: "The selected layer's lanes (or the scene's cues)",
                        onclick: move |_| {
                            let mut v = ui.tl_view;
                            v.set(TlView::Keys);
                        },
                        "Keys"
                    }
                    button {
                        class: if view == TlView::Dope { "active" } else { "" },
                        title: "All layers",
                        onclick: move |_| {
                            let mut v = ui.tl_view;
                            v.set(TlView::Dope);
                        },
                        "Dope"
                    }
                }
                button { title: "Zoom out (Ctrl+wheel)", onclick: move |_| zoom(ui, 0.8), "−" }
                button { title: "Zoom in (Ctrl+wheel)", onclick: move |_| zoom(ui, 1.25), "+" }
            }
            Inspector { ui }
            div { class: "tl-body",
                div { class: "tl-inner", style: "width:{LABEL_WIDTH + width + 40.0}px;",
                    div { class: "tl-row ruler",
                        div { class: "label" }
                        div {
                            class: "track",
                            style: "width:{width}px;",
                            // Scrubbed by the bridge (pointer capture), see `scrub`.
                            "data-scrub": "1",
                            for s in (0..=seconds).step_by(tick_s) {
                                div { class: "tick", style: "left:{s as f64 * 1000.0 * px}px;", "{s}s" }
                            }
                        }
                    }
                    div { class: "tl-lanes",
                        for row in rows {
                            RowView { ui, row, px, width }
                        }
                        if !empty_hint.is_empty() {
                            div { class: "tl-hint dim", "{empty_hint}" }
                        }
                    }
                    div {
                        id: "fc-playhead",
                        class: "playhead",
                        style: "left:{LABEL_WIDTH + time * px}px;",
                    }
                }
            }
        }
    }
}

#[component]
fn RowView(ui: Ui, row: Row, px: f64, width: f64) -> Element {
    match row {
        Row::Lane {
            item,
            lane,
            value,
            keys,
            moves,
            key_here,
        } => {
            let shown = value
                .map(framecraft_core::js::number_to_string)
                .unwrap_or_default();
            let toggle_item = item.clone();
            rsx! {
                div { class: "tl-row", "data-item": "{item}",
                    div { class: "label lane-label",
                        span { class: "lane-name", "{lane_label(lane)}" }
                        span { class: "lane-value dim", "{shown}" }
                        // The Start key of a clip is move-only.
                        if lane != CLIP_START {
                            button {
                                class: if key_here { "key-button animated" } else { "key-button" },
                                title: if key_here { "Remove the key at the playhead" } else { "Add a key at the playhead" },
                                onclick: move |_| toggle_lane_key(ui, &toggle_item, lane),
                                if key_here { "◆" } else { "◇" }
                            }
                        }
                    }
                    div { class: "track", style: "width:{width}px;",
                        for (a , b , times) in moves {
                            div {
                                class: "tl-move",
                                style: "left:{a * px}px;width:{(b - a) * px}px;",
                                title: "Drag to slide this tween",
                                "data-item": "{item}",
                                "data-lane": "{lane}",
                                "data-start": "{a}",
                                "data-keys": "{times}",
                            }
                        }
                        for k in keys {
                            div {
                                class: if k.selected { "tl-key selected" } else { "tl-key" },
                                style: "left:{k.time * px}px;",
                                "data-item": "{item}",
                                "data-lane": "{lane}",
                                "data-time": "{k.time}",
                            }
                        }
                    }
                }
            }
        }
        Row::Cues { kind, cues } => {
            let class = match kind {
                CueKind::Pause => "tl-cue pause",
                CueKind::ApiCall => "tl-cue api",
                CueKind::Script => "tl-cue script",
            };
            rsx! {
                div { class: "tl-row",
                    div { class: "label lane-label",
                        span { class: "lane-name", "{cue_label(kind)}" }
                        button {
                            class: "key-button",
                            title: "Add a {cue_label(kind)} cue at the playhead",
                            onclick: move |_| add_cue(ui, kind),
                            "+"
                        }
                    }
                    div { class: "track", style: "width:{width}px;",
                        for (id , t , label , selected) in cues {
                            div {
                                class: if selected { "{class} selected" } else { "{class}" },
                                style: "left:{t * px}px;",
                                title: "{label}",
                                "data-cue": "{id}",
                                "data-time": "{t}",
                            }
                        }
                    }
                }
            }
        }
        Row::Layer {
            item,
            name,
            depth,
            keys,
            span,
            selected,
        } => {
            let id = item.clone();
            rsx! {
                div {
                    class: if selected { "tl-row selected" } else { "tl-row" },
                    "data-item": "{item}",
                    div {
                        class: "label",
                        style: "padding-left:{8 + depth * 14}px;",
                        onclick: move |_| ui.select_layer(Some(id.clone())),
                        "{name}"
                    }
                    div { class: "track", style: "width:{width}px;",
                        if let Some((a, b)) = span {
                            div {
                                class: "tl-span",
                                style: "left:{a * px}px;width:{(b - a) * px}px;",
                                title: "Drag to move all of this layer's keys",
                                "data-item": "{item}",
                                "data-start": "{a}",
                            }
                        }
                        for k in keys {
                            div {
                                class: if k.selected { "tl-key selected" } else { "tl-key" },
                                style: "left:{k.time * px}px;",
                                "data-item": "{item}",
                                "data-time": "{k.time}",
                                "data-dope": "1",
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The selected keyframes or cue: time, value, easing.
#[component]
fn Inspector(ui: Ui) -> Element {
    let sel = ui.keys.read().clone();
    let cue_id = ui.cue.read().clone();
    let editor = ui.editor.read();
    let Some(doc) = editor.as_ref().map(|e| e.doc()) else {
        return rsx! {
            div { class: "inspector" }
        };
    };

    if let Some(cue) = cue_id
        .as_ref()
        .and_then(|id| {
            doc.timeline
                .scene_cues
                .iter()
                .flatten()
                .find(|c| &c.id == id)
        })
        .cloned()
    {
        drop(editor);
        let id = cue.id.clone();
        let field = move |path: &'static str| {
            let id = id.clone();
            move |evt: FormEvent| {
                let raw = evt.value();
                let value = match path {
                    "time" => raw.trim().parse::<f64>().map(|s| json!(s * 1000.0)).ok(),
                    "delayMs" => raw.trim().parse::<f64>().map(|v| json!(v)).ok(),
                    _ => Some(json!(raw)),
                };
                if let Some(value) = value {
                    ui.execute(&SetCueField {
                        id: id.clone(),
                        path: path.into(),
                        value,
                    });
                }
            }
        };
        let remove_id = cue.id.clone();
        let t = round3(cue.time / 1000.0);
        let delay = cue.delay_ms.unwrap_or(0.0);
        let label = cue.label.clone().unwrap_or_default();
        return rsx! {
            div { class: "inspector",
                span { class: "insp-title", "{cue_label(cue.kind)} cue" }
                label { "Time (s)"
                    input { r#type: "number", step: "0.01", value: "{t}", onchange: field("time") }
                }
                label { "Delay (ms)"
                    input { r#type: "number", step: "10", value: "{delay}", onchange: field("delayMs") }
                }
                label { "Label"
                    input { r#type: "text", value: "{label}", onchange: field("label") }
                }
                if cue.kind == CueKind::ApiCall {
                    label { "URL"
                        input { class: "wide", r#type: "text", value: "{cue.api_url.clone().unwrap_or_default()}", onchange: field("apiUrl") }
                    }
                    label { "Method"
                        select { onchange: field("apiMethod"),
                            for m in ["POST", "GET"] {
                                option { value: m, selected: cue.api_method.as_deref().unwrap_or("POST") == m, "{m}" }
                            }
                        }
                    }
                    label { "Body"
                        input { class: "wide", r#type: "text", value: "{cue.api_body.clone().unwrap_or_default()}", onchange: field("apiBody") }
                    }
                }
                if cue.kind == CueKind::Script {
                    label { title: "A function from the script editor (Properties → Script)",
                        "Function"
                        input {
                            r#type: "text",
                            list: "fc-cue-fns",
                            value: "{cue.script_fn.clone().unwrap_or_default()}",
                            onchange: field("scriptFn"),
                        }
                        datalist { id: "fc-cue-fns",
                            for name in ui
                                .editor
                                .read()
                                .as_ref()
                                .map(|e| framecraft_core::script::cue_function_names(&e.doc().template_functions))
                                .unwrap_or_default()
                            {
                                option { value: "{name}" }
                            }
                        }
                    }
                }
                button {
                    onclick: move |_| {
                        let mut cue = ui.cue;
                        cue.set(None);
                        ui.execute(&RemoveCue { id: remove_id.clone() });
                    },
                    "Delete"
                }
            }
        };
    }

    let keys: Vec<(KeyRef, f64, String)> = sel
        .iter()
        .filter_map(|r| {
            let k = key_at(doc, r)?;
            Some((
                r.clone(),
                k.value,
                framecraft_core::easing::normalize(k.easing.as_deref()).to_string(),
            ))
        })
        .collect();
    drop(editor);
    if keys.is_empty() {
        // Always there, so the timeline does not jump when keys get selected.
        return rsx! {
            div { class: "inspector",
                span { class: "dim", "Select a key or cue to edit its time, value and easing. Shift+click or drag on empty track space to select more." }
            }
        };
    }
    let easing = if keys.iter().all(|k| k.2 == keys[0].2) {
        keys[0].2.clone()
    } else {
        String::new()
    };
    let refs: Vec<KeyRef> = keys.iter().map(|k| k.0.clone()).collect();
    let (r1, r2) = (refs.clone(), refs.clone());
    let single = (keys.len() == 1).then(|| keys[0].clone());
    // Start keys have no value, easing, or delete.
    let only_starts = keys.iter().all(|k| k.0.lane == CLIP_START);

    rsx! {
        div { class: "inspector",
            if let Some((r, value, _)) = single {
                {
                    let (rt, rv) = (r.clone(), r.clone());
                    let t = round3(r.time / 1000.0);
                    let v = framecraft_core::js::number_to_string(round3(value));
                    rsx! {
                        span { class: "insp-title", "{lane_label(&r.lane)} key" }
                        label { "Time (s)"
                            input {
                                r#type: "number",
                                step: "0.01",
                                value: "{t}",
                                onchange: move |evt| {
                                    if let Ok(s) = evt.value().trim().parse::<f64>() {
                                        retime(ui, vec![rt.clone()], s * 1000.0 - rt.time, false);
                                    }
                                },
                            }
                        }
                        if rv.lane == CLIP_START {
                            span { class: "dim", "Drag the Start key to set when the clip begins." }
                        } else {
                        label { "Value"
                            input {
                                r#type: "number",
                                step: "any",
                                value: "{v}",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().trim().parse::<f64>() {
                                        ui.execute(&SetKeyframe {
                                            item: rv.item.clone(),
                                            lane: rv.lane.clone(),
                                            time_ms: rv.time,
                                            value: v,
                                            easing: None,
                                        });
                                    }
                                },
                            }
                        }
                        }
                    }
                }
            } else {
                span { class: "insp-title", "{keys.len()} keys" }
            }
            if !only_starts {
            EasingPicker { ui, keys: r1.clone(), current: easing.clone() }
            button {
                title: "Delete keys (Delete)",
                onclick: move |_| {
                    let mut keys = ui.keys;
                    keys.set(Vec::new());
                    ui.execute(&RemoveKeys { keys: r2.clone() });
                },
                "Delete"
            }
            }
        }
    }
}

fn zoom(ui: Ui, factor: f64) {
    let mut z = ui.tl_zoom;
    let next = (*z.peek() * factor).clamp(0.005, 2.0);
    z.set(next);
}

/// Moves keys by `delta` ms (clamped; snapped when snapping is on) and
/// selects them at their new times.
fn retime(ui: Ui, keys: Vec<KeyRef>, delta: f64, snap_by_primary: bool) {
    let cmd = {
        let editor = ui.editor.peek();
        let Some(doc) = editor.as_ref().map(|e| e.doc()) else {
            return;
        };
        let snap = doc.timeline.snap_enabled == Some(true);
        let delta = match keys.first() {
            Some(p) if snap && snap_by_primary => snap_time(doc, p.time + delta) - p.time,
            _ => delta.round(),
        };
        let cmd = RetimeKeys { keys, delta, snap };
        let moved = cmd.moved(doc);
        (cmd, moved)
    };
    if cmd.0.delta == 0.0 {
        return;
    }
    ui.execute(&cmd.0);
    let mut sel = ui.keys;
    sel.set(cmd.1);
}

fn add_cue(ui: Ui, kind: CueKind) {
    let cue = new_cue(kind, *ui.time.peek());
    let id = cue.id.clone();
    ui.execute(&AddCue { cue });
    let (mut sel, mut keys) = (ui.cue, ui.keys);
    sel.set(Some(id));
    keys.set(Vec::new());
}

/// The ◇ / ◆ button of a lane: adds a key at the playhead, or removes the
/// key there.
fn toggle_lane_key(ui: Ui, item: &str, lane: &str) {
    toggle_keys(ui, item, &[lane.to_string()]);
}

/// FRAME's K: for each lane, removes the key at the playhead or adds one
/// with the current value. Added keys become the selection.
fn toggle_keys(ui: Ui, item: &str, lanes: &[String]) {
    let time = *ui.time.peek();
    let plan = {
        let editor = ui.editor.peek();
        let Some(doc) = editor.as_ref().map(|e| e.doc()) else {
            return;
        };
        let Some(it) = doc.items.iter().find(|i| i.id == item) else {
            return;
        };
        let t = snap_time(doc, time);
        let mut remove = Vec::new();
        let mut add = Vec::new();
        for lane in lanes.iter().filter(|l| l.as_str() != CLIP_START) {
            let r = KeyRef::new(item, lane, t);
            match key_at(doc, &r) {
                Some(k) => remove.push(KeyRef { time: k.time, ..r }),
                None => {
                    if let Some(v) = value_now(doc, it, lane, time) {
                        add.push((r, v));
                    }
                }
            }
        }
        (remove, add)
    };
    let (remove, add) = plan;
    let mut cmds: Vec<Box<dyn framecraft_core::commands::Command>> = Vec::new();
    if !remove.is_empty() {
        cmds.push(Box::new(RemoveKeys {
            keys: remove.clone(),
        }));
    }
    for (r, v) in &add {
        cmds.push(Box::new(SetKeyframe {
            item: r.item.clone(),
            lane: r.lane.clone(),
            time_ms: r.time,
            value: *v,
            easing: None,
        }));
    }
    if cmds.is_empty() {
        return;
    }
    ui.begin_gesture(if add.is_empty() {
        "Delete keys".into()
    } else {
        "Add keys".into()
    });
    ui.execute_all(&cmds);
    ui.end_gesture();
    let mut sel = ui.keys;
    if add.is_empty() {
        sel.write().retain(|k| !remove.contains(k));
    } else {
        sel.set(add.into_iter().map(|(r, _)| r).collect());
    }
}

fn key_ref(ev: &Value) -> Option<KeyRef> {
    Some(KeyRef::new(
        ev["item"].as_str()?,
        ev["lane"].as_str().unwrap_or(""),
        ev["time"].as_f64()?,
    ))
}

/// The keys an event's diamond stands for: one key, or in the Dope view
/// all of a layer's keys at that time.
fn refs_of(ui: Ui, ev: &Value) -> Vec<KeyRef> {
    let Some(r) = key_ref(ev) else {
        return Vec::new();
    };
    if !r.lane.is_empty() {
        return vec![r];
    }
    ui.editor
        .peek()
        .as_ref()
        .map(|e| keys_at(e.doc(), &r.item, r.time))
        .unwrap_or_default()
}

fn contains(sel: &[KeyRef], r: &KeyRef) -> bool {
    sel.iter()
        .any(|s| s.item == r.item && s.lane == r.lane && same_time(s.time, r.time))
}

/// Timeline events from the bridge.
pub fn on_event(ui: Ui, ev: &Value) {
    let (mut sel, mut cue) = (ui.keys, ui.cue);
    let flag = |name: &str| ev[name].as_bool().unwrap_or(false);
    match ev["action"].as_str() {
        // Press on a key: select it (Shift / Ctrl add or remove).
        Some("key-down") => {
            let refs = refs_of(ui, ev);
            cue.set(None);
            let mut s = sel.peek().clone();
            if flag("shift") || flag("ctrl") {
                if refs.iter().all(|r| contains(&s, r)) {
                    s.retain(|k| !contains(&refs, k));
                } else {
                    let new: Vec<KeyRef> = refs.into_iter().filter(|r| !contains(&s, r)).collect();
                    s.extend(new);
                }
            } else if !refs.iter().all(|r| contains(&s, r)) {
                s = refs;
            }
            sel.set(s);
        }
        // A plain click (no drag) on a key among several selected.
        Some("key-click") => sel.set(refs_of(ui, ev)),
        Some("key-drag") => {
            let primary = ev["primary"].as_f64().unwrap_or(0.0);
            let delta = ev["delta"].as_f64().unwrap_or(0.0);
            let mut keys = sel.peek().clone();
            // The pressed key leads (for snapping).
            if let Some(i) = keys.iter().position(|k| same_time(k.time, primary)) {
                let first = keys.remove(i);
                keys.insert(0, first);
            }
            retime(ui, keys, delta, true);
        }
        // FRAME: a tween bar selects its keys, and dragging slides them.
        Some("tween-down") | Some("tween-drag") => {
            let (Some(item), Some(lane)) = (ev["item"].as_str(), ev["lane"].as_str()) else {
                return;
            };
            let start = ev["primary"].as_f64().unwrap_or(0.0);
            let mut keys: Vec<KeyRef> = ev["times"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .map(|t| KeyRef::new(item, lane, t))
                .collect();
            if ev["action"].as_str() == Some("tween-down") {
                sel.set(keys);
                return;
            }
            if let Some(i) = keys.iter().position(|k| same_time(k.time, start)) {
                let first = keys.remove(i);
                keys.insert(0, first);
            }
            retime(ui, keys, ev["delta"].as_f64().unwrap_or(0.0), true);
        }
        Some("span-drag") => {
            let Some(item) = ev["item"].as_str() else {
                return;
            };
            let start = ev["primary"].as_f64().unwrap_or(0.0);
            let delta = ev["delta"].as_f64().unwrap_or(0.0);
            let mut keys = ui
                .editor
                .peek()
                .as_ref()
                .map(|e| all_keys(e.doc(), item))
                .unwrap_or_default();
            keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            if let Some(i) = keys.iter().position(|k| same_time(k.time, start)) {
                let first = keys.remove(i);
                keys.insert(0, first);
            }
            retime(ui, keys, delta, true);
        }
        Some("box") => {
            let mut found: Vec<KeyRef> = Vec::new();
            for k in ev["keys"].as_array().into_iter().flatten() {
                for r in refs_of(ui, k) {
                    if !contains(&found, &r) {
                        found.push(r);
                    }
                }
            }
            if flag("additive") {
                let mut s = sel.peek().clone();
                let new: Vec<KeyRef> = found.into_iter().filter(|r| !contains(&s, r)).collect();
                s.extend(new);
                sel.set(s);
            } else {
                sel.set(found);
            }
            cue.set(None);
        }
        Some("track-click") => {
            sel.set(Vec::new());
            cue.set(None);
            if let Some(t) = ev["time"].as_f64() {
                ui.set_time(t);
            }
        }
        Some("cue-down") => {
            sel.set(Vec::new());
            cue.set(ev["id"].as_str().map(String::from));
        }
        Some("cue-drag") => {
            let (Some(id), Some(from), Some(delta)) = (
                ev["id"].as_str(),
                ev["primary"].as_f64(),
                ev["delta"].as_f64(),
            ) else {
                return;
            };
            ui.execute(&SetCueField {
                id: id.into(),
                path: "time".into(),
                value: json!((from + delta).max(0.0)),
            });
        }
        // Ruler scrub: the bridge already moved the playhead and the page.
        Some("scrub") => {
            if let Some(t) = ev["time"].as_f64() {
                ui.set_time(t);
            }
        }
        Some("zoom") => zoom(ui, ev["factor"].as_f64().unwrap_or(1.0)),
        _ => {}
    }
}

/// Timeline keys; `true` when handled.
pub fn key(ui: Ui, key: &str, shift: bool) -> bool {
    let selected = ui.editor.peek().as_ref().and_then(|e| e.selected.clone());
    let sel = ui.keys.peek().clone();
    let cue = ui.cue.peek().clone();
    match key {
        "play" => ui.toggle_play(),
        "Delete" | "Backspace" if !sel.is_empty() => {
            let mut s = ui.keys;
            s.set(Vec::new());
            ui.execute(&RemoveKeys { keys: sel });
        }
        "Delete" | "Backspace" if cue.is_some() => {
            let mut c = ui.cue;
            c.set(None);
            ui.execute(&RemoveCue {
                id: cue.unwrap_or_default(),
            });
        }
        "Escape" if !sel.is_empty() || cue.is_some() => {
            let (mut s, mut c) = (ui.keys, ui.cue);
            s.set(Vec::new());
            c.set(None);
        }
        "ArrowLeft" | "ArrowRight" if !sel.is_empty() || cue.is_some() => {
            let dir = if key == "ArrowRight" { 1.0 } else { -1.0 };
            let step = ui.editor.peek().as_ref().map_or(1.0, |e| {
                let tl = &e.doc().timeline;
                if tl.snap_enabled == Some(true) {
                    tl.snap_step.unwrap_or(1.0).max(1.0)
                } else {
                    1.0
                }
            }) * if shift { 10.0 } else { 1.0 };
            if let Some(id) = cue {
                let from = ui.editor.peek().as_ref().and_then(|e| {
                    e.doc()
                        .timeline
                        .scene_cues
                        .iter()
                        .flatten()
                        .find(|c| c.id == id)
                        .map(|c| c.time)
                });
                if let Some(from) = from {
                    ui.execute(&SetCueField {
                        id,
                        path: "time".into(),
                        value: json!((from + dir * step).max(0.0)),
                    });
                }
            } else {
                retime(ui, sel, dir * step, false);
            }
        }
        "key-toggle" => {
            let Some(item) = selected else { return true };
            // Selected keys' lanes, else every shown lane.
            let mut lanes: Vec<String> = Vec::new();
            for k in sel.iter().filter(|k| k.item == item) {
                if !lanes.contains(&k.lane) {
                    lanes.push(k.lane.clone());
                }
            }
            if lanes.is_empty() {
                lanes = ui
                    .editor
                    .peek()
                    .as_ref()
                    .map(|e| {
                        shown_lanes(e.doc(), &item)
                            .into_iter()
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            toggle_keys(ui, &item, &lanes);
        }
        "step-prev" | "step-next" => {
            let fps = ui
                .editor
                .peek()
                .as_ref()
                .map_or(30.0, |e| e.doc().timeline.fps.max(1.0));
            let dir = if key == "step-next" { 1.0 } else { -1.0 };
            let frames = if shift { 10.0 } else { 1.0 };
            ui.set_time(*ui.time.peek() + dir * frames * 1000.0 / fps);
        }
        "jump-prev" | "jump-next" => {
            let Some(item) = selected else { return true };
            let keys = ui
                .editor
                .peek()
                .as_ref()
                .map(|e| {
                    let lanes: Vec<String> = if sel.is_empty() {
                        shown_lanes(e.doc(), &item)
                            .into_iter()
                            .map(String::from)
                            .collect()
                    } else {
                        sel.iter().map(|k| k.lane.clone()).collect()
                    };
                    all_keys(e.doc(), &item)
                        .into_iter()
                        .filter(|k| lanes.contains(&k.lane))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let now = *ui.time.peek();
            let mut times: Vec<f64> = keys.iter().map(|k| k.time).collect();
            times.sort_by(f64::total_cmp);
            let target = if key == "jump-next" {
                times
                    .iter()
                    .copied()
                    .find(|t| *t > now + 0.001)
                    .or(times.last().copied())
            } else {
                times
                    .iter()
                    .rev()
                    .copied()
                    .find(|t| *t < now - 0.001)
                    .or(times.first().copied())
            };
            if let Some(t) = target {
                ui.set_time(t);
                let mut s = ui.keys;
                s.set(keys.into_iter().filter(|k| same_time(k.time, t)).collect());
            }
        }
        "copy" if !sel.is_empty() => {
            let copied = ui
                .editor
                .peek()
                .as_ref()
                .map(|e| copy_keys(e.doc(), &sel))
                .unwrap_or_default();
            let mut c = ui.copied;
            c.set(copied);
        }
        "paste" => {
            let copied = ui.copied.peek().clone();
            let Some(item) = selected.filter(|_| !copied.is_empty()) else {
                return true;
            };
            let cmd = PasteKeys {
                item,
                at: *ui.time.peek(),
                keys: copied,
            };
            let pasted = ui
                .editor
                .peek()
                .as_ref()
                .map(|e| cmd.pasted(e.doc()))
                .unwrap_or_default();
            ui.execute(&cmd);
            let mut s = ui.keys;
            s.set(pasted);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_names() {
        assert_eq!(easing_label("ease-in-out-back"), "In Out Back");
        assert_eq!(easing_label("linear"), "Linear");
    }
}

/// FRAME's easing button: the curve icon of the selected keys' easing; it
/// opens a palette of every easing with its curve.
#[component]
fn EasingPicker(ui: Ui, keys: Vec<KeyRef>, current: String) -> Element {
    let mut open = use_signal(|| false);
    let icon = |id: &str| crate::preview::library_url(&format!("/icons/easing/{id}.svg"));
    let label = if current.is_empty() {
        "Mixed".to_string()
    } else {
        easing_label(&current)
    };
    rsx! {
        div { class: "easing-picker",
            button {
                class: "easing-button",
                title: "Easing of the move arriving at the key",
                onclick: move |_| open.toggle(),
                span { class: "dim", "Easing" }
                if !current.is_empty() {
                    img { src: "{icon(&current)}", alt: "", draggable: "false" }
                }
                span { "{label}" }
            }
            if open() {
                div { class: "modal-backdrop", onclick: move |_| open.set(false),
                    div {
                        class: "modal easing-palette",
                        onclick: move |evt| evt.stop_propagation(),
                        div { class: "modal-head",
                            span { class: "panel-title", "Easing" }
                            button { onclick: move |_| open.set(false), "Close" }
                        }
                        div { class: "easing-grid",
                            for id in EASING_IDS {
                                {
                                    let k = keys.clone();
                                    rsx! {
                                        button {
                                            class: if current == id { "easing-tile active" } else { "easing-tile" },
                                            title: "{easing_label(id)}",
                                            onclick: move |_| {
                                                open.set(false);
                                                ui.execute(&SetKeysEasing { keys: k.clone(), easing: id.to_string() });
                                            },
                                            img { src: "{icon(id)}", alt: "", draggable: "false" }
                                            span { "{easing_label(id)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
