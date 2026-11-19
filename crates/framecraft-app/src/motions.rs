// SPDX-License-Identifier: AGPL-3.0-or-later
//! The motion library dialog (FRAME's Motions / Save motion, opened from
//! the timeline): apply a house or saved motion to the selected layer at
//! the playhead, or save the selected keyframes of one layer as a motion.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::easing::EASING_IDS;
use framecraft_core::motion::{
    ApplyMotion, CATEGORIES, MotionData, MotionEntry, end_time, evaluate_at, from_keyframes,
    house_motions,
};

fn seconds(ms: f64) -> String {
    if (ms % 100.0).abs() < 1e-9 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

fn category_label(id: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|(c, _)| *c == id)
        .map_or("Other", |(_, l)| l)
}

/// Where user motions are saved.
pub fn motions_path() -> Option<std::path::PathBuf> {
    crate::project::data_dir().map(|d| d.join("motions.json"))
}

#[component]
pub fn MotionDialog(ui: Ui) -> Element {
    let mode = (ui.motion_dialog)();
    match mode {
        Some("save") => rsx! {
            SaveMotion { ui }
        },
        Some(_) => rsx! {
            ApplyMotions { ui }
        },
        None => rsx! {},
    }
}

/// The selected keyframes, when they are all on one layer.
fn key_selection(ui: Ui) -> Option<(String, Vec<(String, f64)>)> {
    let keys = ui.keys.read();
    let item = keys.first()?.item.clone();
    if keys.iter().any(|k| k.item != item) {
        return None;
    }
    let refs = keys
        .iter()
        .filter(|k| k.lane != framecraft_core::normalize::CLIP_START)
        .map(|k| (k.lane.clone(), k.time))
        .collect::<Vec<_>>();
    (!refs.is_empty()).then_some((item, refs))
}

/// Whether "Save motion" can be used (FRAME: keys of one object selected).
pub fn can_save(ui: Ui) -> bool {
    key_selection(ui).is_some()
}

#[component]
fn ApplyMotions(ui: Ui) -> Element {
    let mut filter = use_signal(|| "all");
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| None::<String>);
    let mut hover = use_signal(|| None::<String>);
    let mut duration = use_signal(|| 600.0_f64);
    let mut easing = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut close = ui.motion_dialog;

    let user = ui.user_motions.read().motions.clone();
    let all: Vec<MotionEntry> = house_motions().into_iter().chain(user).collect();
    let q = query().trim().to_lowercase();
    let f = filter();
    let shown: Vec<MotionEntry> = all
        .iter()
        .filter(|m| f == "all" || m.data.category == f)
        .filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q))
        .cloned()
        .collect();
    let house: Vec<MotionEntry> = shown.iter().filter(|m| m.is_house()).cloned().collect();
    let mine: Vec<MotionEntry> = shown.iter().filter(|m| !m.is_house()).cloned().collect();
    let pick = all
        .iter()
        .find(|m| Some(&m.key) == selected().as_ref())
        .cloned();
    let hovered = hover()
        .filter(|k| Some(k) != selected().as_ref())
        .and_then(|k| all.iter().find(|m| m.key == k).cloned());
    let preview = hovered.clone().or_else(|| pick.clone());
    let preview_len = match &hovered {
        Some(h) => h.data.duration,
        None => duration().max(1.0),
    };
    let preview_easing = if hovered.is_some() {
        String::new()
    } else {
        easing()
    };
    let target = ui.selected();

    let apply = use_callback(move |entry: MotionEntry| {
        let Some(item) = ui.selected() else {
            error.set("Select a layer in the scene first.".into());
            return;
        };
        let len = duration().round().max(1.0);
        let over = easing();
        let (cmd, end_problem, report) = {
            let editor = ui.editor.peek();
            let Some(ed) = editor.as_ref() else { return };
            let doc = ed.doc();
            let start = framecraft_core::keys::snap_time(doc, *ui.time.peek());
            let cmd = ApplyMotion {
                items: vec![item],
                name: entry.name.clone(),
                data: entry.data.clone(),
                start,
                duration: len,
                easing_override: (!over.is_empty()).then_some(over),
                stagger: 0.0,
            };
            let end = end_time(&entry.data, start, len, 0.0, 1);
            let total = doc.timeline.duration;
            let problem = (end > total).then(|| {
                format!(
                    "The motion would end at {}, after the timeline ends ({}). Move the playhead earlier, shorten the motion or lengthen the timeline.",
                    seconds(end),
                    seconds(total)
                )
            });
            let report = cmd.report(doc);
            (cmd, problem, report)
        };
        if let Some(p) = end_problem {
            error.set(p);
            return;
        }
        if report.applied.is_empty() {
            error.set(
                report
                    .skipped
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "This motion does not apply to the selected layer.".into()),
            );
            return;
        }
        if *ui.playing.peek() {
            ui.toggle_play();
        }
        ui.execute(&cmd);
        let mut keys = ui.keys;
        keys.set(Vec::new());
        let note = if report.skipped.is_empty() {
            String::new()
        } else {
            format!(" Skipped: {}.", report.skipped.join("; "))
        };
        let mut status = ui.status;
        status.set(format!(
            "Motion “{}” applied at the playhead.{note}",
            entry.name
        ));
        close.set(None);
    });

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close.set(None),
            div {
                class: "modal motion-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Motions" }
                        p { class: "dim",
                            "Adds keyframes to the selected layer, starting at the playhead. Values are relative to the layer's current pose. Keys on the same properties inside the motion's time span are replaced (Undo restores them)."
                        }
                    }
                    button { onclick: move |_| close.set(None), "Close" }
                }
                div { class: "motion-filters",
                    div { class: "seg",
                        for (id , label) in std::iter::once(("all", "All")).chain(CATEGORIES.iter().copied()) {
                            button {
                                class: if f == id { "active" } else { "" },
                                onclick: move |_| filter.set(id),
                                "{label}"
                            }
                        }
                    }
                    input {
                        r#type: "search",
                        placeholder: "Search…",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                }
                div { class: "motion-body",
                    div { class: "motion-list",
                        if shown.is_empty() {
                            p { class: "dim", "No motions match." }
                        }
                        for (title , list) in [("House motions", house), ("My motions", mine)] {
                            if !list.is_empty() || (title == "My motions" && q.is_empty()) {
                                h3 { "{title}" }
                                if title == "My motions" && list.is_empty() {
                                    p { class: "dim", "None yet. Select keyframes in the timeline and choose Save motion." }
                                }
                                div { class: "motion-grid",
                                    for m in list {
                                        {
                                            let (k1, k2, k3) = (m.key.clone(), m.key.clone(), m.key.clone());
                                            let (e1, e2) = (m.clone(), m.clone());
                                            let summary = format!("{} · {}", seconds(m.data.duration), m.properties().join(", "));
                                            let is_sel = selected().as_deref() == Some(m.key.as_str());
                                            let user = !m.is_house();
                                            rsx! {
                                                div { class: if is_sel { "motion-card active" } else { "motion-card" },
                                                    button {
                                                        class: "motion-pick",
                                                        onclick: move |_| {
                                                            selected.set(Some(k1.clone()));
                                                            duration.set(e1.data.duration);
                                                            error.set(String::new());
                                                        },
                                                        ondoubleclick: move |_| {
                                                            selected.set(Some(e2.key.clone()));
                                                            duration.set(e2.data.duration);
                                                            apply(e2.clone());
                                                        },
                                                        onmouseenter: move |_| hover.set(Some(k2.clone())),
                                                        onmouseleave: move |_| hover.set(None),
                                                        span { class: "motion-name",
                                                            "{m.name}"
                                                            span { class: "motion-cat", "{category_label(&m.data.category)}" }
                                                        }
                                                        span { class: "dim", "{summary}" }
                                                    }
                                                    if user {
                                                        button {
                                                            class: "motion-delete",
                                                            title: "Delete motion",
                                                            onclick: move |_| {
                                                                let key = k3.clone();
                                                                let mut next = ui.user_motions.peek().clone();
                                                                next.motions.retain(|m| m.key != key);
                                                                ui.set_user_motions(next);
                                                                if selected.peek().as_deref() == Some(key.as_str()) {
                                                                    selected.set(None);
                                                                }
                                                            },
                                                            "Delete"
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
                    aside { class: "motion-side",
                        h3 {
                            "Preview"
                            if let Some(p) = &preview {
                                " · {p.name}"
                            }
                        }
                        MotionPreview {
                            ui,
                            data: preview.map(|p| p.data),
                            duration: preview_len,
                            easing: preview_easing,
                        }
                    }
                }
                div { class: "motion-foot",
                    label {
                        span { class: "dim", "Length (ms)" }
                        input {
                            r#type: "number",
                            min: "1",
                            step: "50",
                            disabled: pick.is_none(),
                            value: "{duration}",
                            onchange: move |evt| {
                                if let Ok(v) = evt.value().parse::<f64>() {
                                    duration.set(v.max(1.0));
                                }
                            },
                        }
                    }
                    label {
                        span { class: "dim", "Easing" }
                        select {
                            disabled: pick.is_none(),
                            onchange: move |evt| easing.set(evt.value()),
                            option { value: "", selected: easing().is_empty(), "Motion's own" }
                            for id in EASING_IDS {
                                option { value: id, selected: easing() == id, "{crate::timeline::easing_label(id)}" }
                            }
                        }
                    }
                    span { class: "error", "{error}" }
                    button {
                        class: "active",
                        disabled: pick.is_none() || target.is_none(),
                        title: if target.is_none() { "Select a layer first" } else { "Apply at the playhead" },
                        onclick: move |_| {
                            if let Some(p) = pick.clone() {
                                apply(p);
                            }
                        },
                        "Apply"
                    }
                }
            }
        }
    }
}

#[component]
fn SaveMotion(ui: Ui) -> Element {
    let mut name = use_signal(String::new);
    let mut category = use_signal(|| "in");
    let mut error = use_signal(String::new);
    let mut close = ui.motion_dialog;
    let draft: Result<MotionData, String> = match key_selection(ui) {
        None => Err("Select keyframes from one layer.".into()),
        Some((item, refs)) => match ui.editor.read().as_ref() {
            Some(ed) => from_keyframes(ed.doc(), &item, &refs, category()),
            None => Err("Nothing open.".into()),
        },
    };
    let ok = draft.is_ok();
    let summary = match &draft {
        Ok(d) => {
            let mut props: Vec<&str> = Vec::new();
            for c in &d.channels {
                if !props.contains(&c.property.as_str()) {
                    props.push(&c.property);
                }
            }
            let n = props.len();
            format!(
                "{n} propert{}: {} · {}",
                if n == 1 { "y" } else { "ies" },
                props.join(", "),
                seconds(d.duration)
            )
        }
        Err(e) => e.clone(),
    };
    let preview = draft.clone().ok();
    let preview_len = preview.as_ref().map_or(600.0, |d| d.duration);
    let save = move |_| {
        let Ok(data) = draft.clone() else { return };
        let n = name().trim().to_string();
        if n.is_empty() {
            error.set("Give the motion a name.".into());
            return;
        }
        let mut next = ui.user_motions.peek().clone();
        next.add(&n, data);
        ui.set_user_motions(next);
        let mut status = ui.status;
        status.set(format!("Saved motion “{n}”."));
        close.set(None);
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close.set(None),
            div {
                class: "modal motion-dialog small",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Save as motion" }
                        p { class: "dim",
                            "Saves the selected keyframes as a reusable motion. Values are stored relative to the layer, so the motion works on any layer."
                        }
                    }
                    button { onclick: move |_| close.set(None), "Close" }
                }
                label { class: "motion-field",
                    span { class: "dim", "Name" }
                    input {
                        r#type: "text",
                        maxlength: "120",
                        placeholder: "e.g. Logo bounce in",
                        value: "{name}",
                        oninput: move |evt| name.set(evt.value()),
                    }
                }
                label { class: "motion-field",
                    span { class: "dim", "Type" }
                    select { onchange: move |evt| {
                            let v = evt.value();
                            if let Some((id, _)) = CATEGORIES.iter().find(|(c, _)| *c == v) {
                                category.set(id);
                            }
                        },
                        for (id , label) in CATEGORIES.iter().copied() {
                            option { value: id, selected: category() == id, "{label}" }
                        }
                    }
                    span { class: "dim small",
                        if category() == "in" {
                            "In: the last key is the resting pose. When applied, the layer ends where it is now."
                        } else {
                            "The first key is the resting pose. When applied, the layer starts where it is now."
                        }
                    }
                }
                p { class: if ok { "dim" } else { "error" }, "{summary}" }
                if ok {
                    MotionPreview { ui, data: preview, duration: preview_len, easing: String::new() }
                }
                span { class: "error", "{error}" }
                div { class: "motion-foot",
                    span {}
                    button { onclick: move |_| close.set(None), "Cancel" }
                    button {
                        class: "active",
                        disabled: !ok || name().trim().is_empty(),
                        onclick: save,
                        "Save motion"
                    }
                }
            }
        }
    }
}

/// A small looping preview of a motion on a sample box in a scaled-down
/// scene (FRAME's MotionPreview).
#[component]
fn MotionPreview(ui: Ui, data: Option<MotionData>, duration: f64, easing: String) -> Element {
    const PRE_ROLL: f64 = 350.0;
    const HOLD: f64 = 750.0;
    let mut now = use_signal(|| 0.0_f64);
    use_future(move || async move {
        let start = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(33)).await;
            now.set(start.elapsed().as_secs_f64() * 1000.0);
        }
    });
    let (sw, sh) = ui.editor.read().as_ref().map_or((1920.0, 1080.0), |e| {
        (e.doc().scene.width.max(1.0), e.doc().scene.height.max(1.0))
    });
    let Some(data) = data else {
        return rsx! {
            div { class: "motion-stage empty", "Select a motion to preview it" }
        };
    };
    let len = duration.max(1.0);
    let cycle = PRE_ROLL + len + HOLD;
    let t = ((now() % cycle) - PRE_ROLL).clamp(0.0, len);
    let (bw, bh) = ((sw * 0.3).round(), (sh * 0.16).round());
    let rest = move |p: &str| -> Option<f64> {
        Some(match p {
            "x" => (sw - bw) / 2.0,
            "y" => (sh - bh) / 2.0,
            "width" => bw,
            "height" => bh,
            "fontSize" => (bh * 0.45).round(),
            "modColorBrightness" | "modColorContrast" | "modColorSaturation" => 100.0,
            _ => return None,
        })
    };
    let over = (!easing.is_empty()).then_some(easing.as_str());
    let values = evaluate_at(&data, t, len, over, bw, bh, &rest);
    let has = |p: &str| values.iter().any(|(k, _)| k == p);
    let v = |p: &str| {
        values
            .iter()
            .find(|(k, _)| k == p)
            .map(|(_, v)| *v)
            .or_else(|| rest(p))
            .unwrap_or(if matches!(p, "scaleX" | "scaleY" | "opacity") {
                1.0
            } else {
                0.0
            })
    };
    let mut filters = Vec::new();
    if has("modBlurRadius") {
        filters.push(format!("blur({}px)", v("modBlurRadius").max(0.0)));
    }
    for (lane, f, unit) in [
        ("modColorBrightness", "brightness", "%"),
        ("modColorContrast", "contrast", "%"),
        ("modColorSaturation", "saturate", "%"),
        ("modColorHueRotate", "hue-rotate", "deg"),
    ] {
        if has(lane) {
            filters.push(format!("{f}({}{unit})", v(lane)));
        }
    }
    if has("modShadowX") || has("modShadowY") || has("modShadowBlur") {
        filters.push(format!(
            "drop-shadow({}px {}px {}px rgba(0,0,0,0.5))",
            v("modShadowX"),
            v("modShadowY"),
            v("modShadowBlur").max(0.0)
        ));
    }
    let clip = has("modCropPercent").then(|| {
        let pct = v("modCropPercent").clamp(0.0, 100.0);
        let dir = data
            .channels
            .iter()
            .find(|c| c.property == "modCropPercent")
            .and_then(|c| {
                c.modifier_init
                    .as_ref()?
                    .get("direction")?
                    .as_str()
                    .map(String::from)
            })
            .unwrap_or_else(|| "left".into());
        match dir.as_str() {
            "right" => format!("inset(0% {pct}% 0% 0%)"),
            "up" => format!("inset({pct}% 0% 0% 0%)"),
            "down" => format!("inset(0% 0% {pct}% 0%)"),
            _ => format!("inset(0% 0% 0% {pct}%)"),
        }
    });
    let text = if has("modTypewriterPercent") {
        let n = ((v("modTypewriterPercent").clamp(0.0, 100.0) / 100.0) * 6.0).round() as usize;
        "Motion".chars().take(n).collect()
    } else {
        "Motion".to_string()
    };
    let style = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;opacity:{};transform:rotate({}deg) skew({}deg, {}deg) scale({}, {});font-size:{}px;{}{}",
        v("x"),
        v("y"),
        v("width").max(0.0),
        v("height").max(0.0),
        v("opacity").clamp(0.0, 1.0),
        v("rotation"),
        v("modSkewX"),
        v("modSkewY"),
        v("scaleX"),
        v("scaleY"),
        v("fontSize").max(1.0),
        if filters.is_empty() {
            String::new()
        } else {
            format!("filter:{};", filters.join(" "))
        },
        clip.map_or(String::new(), |c| format!("clip-path:{c};")),
    );
    // The stage is 300 px wide; the scene is drawn scaled into it.
    let k = 300.0 / sw;
    let progress = (t / len * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "motion-stage", style: "width:300px;height:{sh * k}px;",
            div {
                class: "motion-scene",
                style: "width:{sw}px;height:{sh}px;transform:scale({k});",
                div { class: "motion-box", style: "{style}", "{text}" }
            }
        }
        div { class: "motion-progress",
            div { style: "width:{progress}%;" }
        }
    }
}
