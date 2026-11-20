// SPDX-License-Identifier: AGPL-3.0-or-later
//! The editor's header and viewport (the Outliner, properties and timeline
//! have their own modules).

use crate::bridge::{FRAME_A, FRAME_B};
use crate::interact::{Geom, ROTATE_OFFSET};
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::js::number_to_string;
use framecraft_core::manipulate::Handle;
use framecraft_core::selection::movable;
use framecraft_core::timeline::pose_at;

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
            ((s[0] - 160.0) / sw)
                .min((s[1] - 160.0) / sh)
                .clamp(0.05, 2.0),
        );
        ui.pan.set((0.0, 0.0));
    }
}

#[component]
pub fn Header(ui: Ui) -> Element {
    let (graphics, current) = match ui.editor.read().as_ref() {
        Some(e) => (
            (0..e.project.docs.len())
                .map(|i| e.project.graphic_name(i).to_string())
                .collect::<Vec<_>>(),
            e.graphic,
        ),
        None => (Vec::new(), 0),
    };
    let has_project = !graphics.is_empty();
    let dirty = (ui.dirty)();

    rsx! {
        div { class: "header",
            span { class: "brand", "FrameCraft" }
            crate::menubar::MenuBar { ui }
            div { class: "spacer" }
            if has_project && dirty {
                button {
                    class: "unsaved",
                    title: "Unsaved changes: click to save (Ctrl+S)",
                    onclick: move |_| ui.save(),
                    "● Unsaved"
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
            }
            button {
                title: "The project's graphics, media and your palettes (Tab)",
                onclick: move |_| {
                    let mut open = ui.project_view;
                    open.set(true);
                },
                "Project"
            }
        }
    }
}

#[component]
pub fn Viewport(ui: Ui) -> Element {
    let (w, h, open, plate) = match ui.editor.read().as_ref() {
        Some(e) => {
            let s = &e.doc().scene;
            let plate = crate::properties::plate_css(s).map(|bg| {
                (
                    bg,
                    s.fake_background_opacity.unwrap_or(100.0).clamp(0.0, 100.0) / 100.0,
                    s.fake_background_blur.unwrap_or(0.0).clamp(0.0, 20.0),
                )
            });
            (s.width, s.height, true, plate)
        }
        None => (1920.0, 1080.0, false, None),
    };
    let zoom = (ui.zoom)();
    let (px, py) = (ui.pan)();
    let has_selection = ui
        .editor
        .read()
        .as_ref()
        .is_some_and(|e| e.selected.is_some());
    let frame_style = format!("width:{w}px;height:{h}px;transform:scale({zoom});");
    let px_rulers = open && (ui.guides)().pixel_rulers;
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
                div { class: "spacer" }
                button {
                    class: if px_rulers { "active" } else { "" },
                    title: "Show / hide the pixel rulers along the viewport edges (Ctrl+R)",
                    onclick: move |_| crate::menubar::toggle_guide(ui, "pixel_rulers"),
                    "Px rulers"
                }
            }
            div { class: if px_rulers { "viewport-area with-px-rulers" } else { "viewport-area" },
            div { class: "viewport", id: "fc-viewport",
                // Both frames always exist; the bridge sets their src and
                // shows one (class "active"). Dioxus never touches either.
                div {
                    class: "stage",
                    // `display` in both: Dioxus updates style properties one by
                    // one, so a value missing from the new style stays (an
                    // empty start's `display:none` hid the viewport after New).
                    style: if open { "display:block;width:{w * zoom}px;height:{h * zoom}px;transform:translate({px}px,{py}px);--z:{zoom};" } else { "display:none;width:0;height:0;transform:none;--z:1;" },
                    // Editor-only plate behind the graphic (Scene → Plates).
                    if let Some((bg, opacity, blur)) = plate {
                        div {
                            class: "plate",
                            style: "background:{bg};opacity:{opacity};filter:blur({blur * zoom}px);",
                        }
                    }
                    iframe { id: FRAME_A, style: "{frame_style}" }
                    iframe { id: FRAME_B, style: "{frame_style}" }
                    Overlay { ui, width: w, height: h }
                    RulerHandles { ui }
                }
                if !open {
                    div { class: "empty", "No graphic open" }
                }
            }
            // Scene-pixel scales on the top and left edges (the bridge draws
            // them and turns a drag from one into a new custom ruler).
            if px_rulers {
                div { class: "px-rulers", "data-w": "{w}", "data-h": "{h}",
                    canvas {
                        class: "px-ruler top",
                        title: "Scene pixels. Drag down into the scene to add a horizontal ruler",
                    }
                    canvas {
                        class: "px-ruler left",
                        title: "Scene pixels. Drag right into the scene to add a vertical ruler",
                    }
                    div { class: "px-corner", title: "Pixel rulers (Ctrl+R hides them)", "px" }
                }
            }
            // FRAME's places: guide toggles down the right edge, align
            // groups (horizontal, vertical) at the bottom left.
            GuideBar { ui }
            div { class: "align-tools", role: "toolbar",
                for row in framecraft_core::manipulate::ALIGN_MODES.chunks(3) {
                    div { class: "align-group", role: "group",
                        for (mode , title , icon) in row.iter().copied() {
                            button {
                                class: "align-btn",
                                title: "{title} (to the scene, or the parent layer)",
                                disabled: !has_selection,
                                onclick: move |_| ui.align(mode),
                                img {
                                    src: crate::preview::library_url(&format!("/icons/align/{icon}.svg")),
                                    alt: "",
                                    draggable: "false",
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

/// How far (screen px) the overlay reaches past the graphic; matches
/// `.overlay` in style.css.
const OVERLAY_MARGIN: f64 = 64.0;

/// Selection outline and handles over the preview. It also receives all
/// pointer input on the stage (the bridge listens on `#fc-overlay`).
#[component]
fn Overlay(ui: Ui, width: f64, height: f64) -> Element {
    let zoom = (ui.zoom)().max(0.01);
    let time = (ui.time)();
    let geometry = ui.geometry.read();
    let editor = ui.editor.read();
    // What to draw for the selection, in scene coordinates.
    // Outlines of the other selected layers (multi-select).
    let others: Vec<String> = editor
        .as_ref()
        .map(|e| {
            e.also
                .iter()
                .filter_map(|id| geometry.iter().find(|g| &g.id == id))
                .map(|g| {
                    g.corners
                        .iter()
                        .map(|(x, y)| format!("{x},{y}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
        .unwrap_or_default();
    // FRAME: a box around the whole multi-selection.
    let union = editor.as_ref().and_then(|e| {
        if e.also.is_empty() {
            return None;
        }
        let boxes: Vec<(f64, f64, f64, f64)> = e
            .selection()
            .iter()
            .filter_map(|id| geometry.iter().find(|g| &g.id == id))
            .filter_map(|g| {
                let xs = g.corners.iter().map(|c| c.0);
                let ys = g.corners.iter().map(|c| c.1);
                let (x0, x1) = (
                    xs.clone().fold(f64::INFINITY, f64::min),
                    xs.fold(f64::NEG_INFINITY, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::INFINITY, f64::min),
                    ys.fold(f64::NEG_INFINITY, f64::max),
                );
                (x0.is_finite() && y0.is_finite()).then_some((x0, y0, x1 - x0, y1 - y0))
            })
            .collect();
        framecraft_core::manipulate::bounds_of(&boxes)
    });
    let single = editor.as_ref().is_some_and(|e| e.also.is_empty());
    let drawn = editor.as_ref().and_then(|e| {
        let id = e.selected.as_ref()?;
        let g: &Geom = geometry.iter().find(|g| &g.id == id)?;
        let doc = e.doc();
        let item = doc.items.iter().find(|i| &i.id == id)?;
        let pose = pose_at(doc, item, time);
        let outline = g
            .corners
            .iter()
            .map(|(x, y)| format!("{x},{y}"))
            .collect::<Vec<_>>()
            .join(" ");
        let handles: Vec<(f64, f64)> = Handle::ALL.iter().map(|h| g.at(h.at())).collect();
        let pivot = g.at((pose.pivot_x / 100.0, pose.pivot_y / 100.0));
        let top = g.at((0.5, 0.0));
        let rotate = g.rotate_handle(ROTATE_OFFSET / zoom);
        let marks = crate::interact::point_marks(g, item, &pose);
        Some((
            outline,
            handles,
            pivot,
            top,
            rotate,
            movable(doc, id),
            marks,
        ))
    });
    drop(geometry);
    drop(editor);
    let hs = 8.0 / zoom;
    rsx! {
        div {
            id: "fc-overlay",
            class: "overlay",
            "data-w": "{width}",
            "data-h": "{height}",
            "data-margin": "{OVERLAY_MARGIN}",
            svg {
                view_box: "0 0 {width} {height}",
                width: "{width * zoom}",
                height: "{height * zoom}",
                // Other selected layers: a dark halo under a bright dashed
                // line, readable on light and dark graphics.
                Guides { ui, width, height }
                for outline in others {
                    polygon { class: "sel-other-halo", points: "{outline}" }
                    polygon { class: "sel-other", points: "{outline}" }
                }
                if let Some((x, y, w, h)) = union {
                    rect { class: "sel-union-halo", x: "{x}", y: "{y}", width: "{w}", height: "{h}" }
                    rect { class: "sel-union", x: "{x}", y: "{y}", width: "{w}", height: "{h}" }
                }
                if let Some((outline, handles, pivot, top, rotate, can_move, marks)) = drawn {
                    polygon { class: if can_move { "sel-outline" } else { "sel-outline locked" }, points: "{outline}" }
                    if can_move && single {
                        line { class: "sel-stem", x1: "{top.0}", y1: "{top.1}", x2: "{rotate.0}", y2: "{rotate.1}" }
                        circle { class: "sel-rotate", cx: "{rotate.0}", cy: "{rotate.1}", r: "{hs * 0.6}" }
                        for (x , y) in handles {
                            rect {
                                class: "sel-handle",
                                x: "{x - hs / 2.0}",
                                y: "{y - hs / 2.0}",
                                width: "{hs}",
                                height: "{hs}",
                            }
                        }
                        circle { class: "sel-pivot", cx: "{pivot.0}", cy: "{pivot.1}", r: "{hs * 0.55}" }
                        line { class: "sel-pivot", x1: "{pivot.0 - hs}", y1: "{pivot.1}", x2: "{pivot.0 + hs}", y2: "{pivot.1}" }
                        line { class: "sel-pivot", x1: "{pivot.0}", y1: "{pivot.1 - hs}", x2: "{pivot.0}", y2: "{pivot.1 + hs}" }
                        // Polygon / path points and their bezier handles.
                        for (at , handles) in marks {
                            if let Some((hin, hout)) = handles {
                                line { class: "poly-arm", x1: "{hin.0}", y1: "{hin.1}", x2: "{at.0}", y2: "{at.1}" }
                                line { class: "poly-arm", x1: "{at.0}", y1: "{at.1}", x2: "{hout.0}", y2: "{hout.1}" }
                                circle { class: "poly-handle", cx: "{hin.0}", cy: "{hin.1}", r: "{hs * 0.4}" }
                                circle { class: "poly-handle", cx: "{hout.0}", cy: "{hout.1}", r: "{hs * 0.4}" }
                            }
                            circle { class: "poly-point", cx: "{at.0}", cy: "{at.1}", r: "{hs * 0.55}" }
                        }
                    }
                }
            }
        }
    }
}

/// The status bar at the bottom (FRAME's): what the pointer is over (its
/// tooltip), else the last message, else Ready; then the selected layer,
/// layer count, resolution, frame rate and version.
#[component]
pub fn StatusBar(ui: Ui) -> Element {
    let hint = (ui.hint)();
    let status = (ui.status)();
    let (text, class) = if !hint.is_empty() {
        (hint, "status-text hover")
    } else if !status.is_empty() {
        (status, "status-text")
    } else {
        ("Ready".to_string(), "status-text idle")
    };
    let info = ui.editor.read().as_ref().map(|e| {
        let d = e.doc();
        let selected = e
            .selected
            .as_ref()
            .and_then(|id| d.items.iter().find(|i| &i.id == id))
            .map_or_else(
                || "Scene".to_string(),
                |i| {
                    if i.name.trim().is_empty() {
                        "—".to_string()
                    } else {
                        i.name.trim().to_string()
                    }
                },
            );
        (
            selected,
            d.items.len(),
            format!("{}×{}", d.scene.width.round(), d.scene.height.round()),
            format!("{} fps", d.timeline.fps.round()),
        )
    });
    rsx! {
        footer { class: "status-bar",
            span { class: "{class}", "{text}" }
            if let Some(u) = (ui.update)() {
                button {
                    class: "update-note",
                    title: "Open the FrameCraft {u.version} release page in your browser",
                    onclick: move |_| crate::updates::open(&u.url),
                    "Update {u.version} available"
                }
            }
            span { class: "status-info",
                if let Some((selected, count, res, fps)) = info {
                    span { class: "status-sel", title: "Selected layer", "{selected}" }
                    span { class: "sep", "|" }
                    span { title: "Layer count", "{count}" }
                    span { class: "sep", "|" }
                    span { title: "Scene resolution", "{res}" }
                    span { class: "sep", "|" }
                    span { title: "Timeline frame rate", "{fps}" }
                    span { class: "sep", "|" }
                }
                span { title: "FrameCraft version", "v{env!(\"CARGO_PKG_VERSION\")}" }
            }
        }
    }
}

/// Wheel zoom (by `factor`) around a point of the viewport (`x`, `y` from
/// its top-left; `w`, `h` its size): the scene point under it stays put.
pub fn zoom_at(mut ui: Ui, factor: f64, x: f64, y: f64, w: f64, h: f64) {
    let z = *ui.zoom.peek();
    let next = (z * factor).clamp(0.05, 8.0);
    if (next - z).abs() < 1e-9 {
        return;
    }
    let (px, py) = *ui.pan.peek();
    // The stage is centred in the viewport, then moved by the pan.
    let (cx, cy) = (x - w / 2.0, y - h / 2.0);
    let k = next / z;
    ui.zoom.set(next);
    ui.pan.set((cx - (cx - px) * k, cy - (cy - py) * k));
}

/// Middle-button drag: moves the view by screen pixels.
pub fn pan_by(mut ui: Ui, dx: f64, dy: f64) {
    let (px, py) = *ui.pan.peek();
    ui.pan.set((px + dx, py + dy));
}

#[cfg(test)]
mod tests {
    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        // Stage centred, pan (10, 0), zoom 0.5; cursor 100 px right of centre.
        let (z, pan, c) = (0.5_f64, (10.0_f64, 0.0_f64), (100.0_f64, 0.0_f64));
        let scene_x = (c.0 - pan.0) / z; // 180 scene px right of the stage centre
        let k = 2.0;
        let next_pan = c.0 - (c.0 - pan.0) * k;
        assert_eq!(scene_x * (z * k) + next_pan, c.0);
    }
}

/// A guide toggle: label, tooltip, on, CSS key, and how it flips.
type GuideButton = (
    &'static str,
    &'static str,
    bool,
    &'static str,
    fn(&mut framecraft_core::manipulate::GuideToggles),
);

/// FRAME's guide toggles: safe areas, centre cross, thirds, rulers, snap.
#[component]
fn GuideBar(ui: Ui) -> Element {
    let g = (ui.guides)();
    let toggle = move |f: fn(&mut framecraft_core::manipulate::GuideToggles)| {
        let mut guides = ui.guides;
        let mut next = *guides.peek();
        f(&mut next);
        guides.set(next);
        crate::layout::save_guides(&next);
    };
    let buttons: [GuideButton; 6] = [
        (
            "Safe 10%",
            "Toggle 10% safe area",
            g.safe10,
            "safe10",
            |g| g.safe10 = !g.safe10,
        ),
        (
            "Safe 20%",
            "Toggle 20% safe area",
            g.safe20,
            "safe20",
            |g| g.safe20 = !g.safe20,
        ),
        ("Center +", "Toggle center point", g.center, "center", |g| {
            g.center = !g.center
        }),
        ("Thirds", "Toggle rule of thirds", g.thirds, "thirds", |g| {
            g.thirds = !g.thirds
        }),
        ("Rulers", "Toggle custom rulers", g.rulers, "rulers", |g| {
            g.rulers = !g.rulers
        }),
        (
            "Snap",
            "Moving layers snap to the shown guides",
            g.snap,
            "snap",
            |g| g.snap = !g.snap,
        ),
    ];
    rsx! {
        div { class: "guide-tools",
            for (label , title , on , key , f) in buttons {
                button {
                    class: if on { "guide-btn on {key}" } else { "guide-btn" },
                    title: "{title}",
                    onclick: move |_| toggle(f),
                    "{label}"
                }
            }
        }
    }
}

/// The shown guides over the scene, in scene units (FRAME's colours and
/// line width from the scene settings).
#[component]
fn Guides(ui: Ui, width: f64, height: f64) -> Element {
    let g = (ui.guides)();
    let Some((lw, c10, c20, cth, cc, cr, rh, rv)) = ui.editor.read().as_ref().map(|e| {
        let s = &e.doc().scene;
        let col = |v: &Option<String>, d: &str| {
            v.clone()
                .filter(|x| !x.trim().is_empty())
                .unwrap_or_else(|| d.to_string())
        };
        (
            s.guide_line_width.unwrap_or(5.0).clamp(1.0, 20.0),
            col(&s.guide_color_safe10, "rgba(110, 231, 183, 0.75)"),
            col(&s.guide_color_safe20, "rgba(252, 211, 77, 0.75)"),
            col(&s.guide_color_thirds, "rgba(103, 232, 249, 0.7)"),
            col(&s.guide_color_center, "rgba(253, 164, 175, 0.8)"),
            col(&s.custom_ruler_color, "rgba(167, 139, 250, 0.75)"),
            s.custom_rulers_h.clone().unwrap_or_default(),
            s.custom_rulers_v.clone().unwrap_or_default(),
        )
    }) else {
        return rsx! {};
    };
    let (w, h) = (width, height);
    let plus = (lw * 6.0).max(24.0);
    rsx! {
        g { class: "guides", stroke_width: "{lw}",
            if g.thirds {
                for f in [1.0 / 3.0, 2.0 / 3.0] {
                    line { x1: "{w * f}", y1: "0", x2: "{w * f}", y2: "{h}", stroke: "{cth}" }
                    line { x1: "0", y1: "{h * f}", x2: "{w}", y2: "{h * f}", stroke: "{cth}" }
                }
            }
            if g.safe10 {
                rect { x: "{w * 0.1}", y: "{h * 0.1}", width: "{w * 0.8}", height: "{h * 0.8}", fill: "none", stroke: "{c10}" }
            }
            if g.safe20 {
                rect { x: "{w * 0.2}", y: "{h * 0.2}", width: "{w * 0.6}", height: "{h * 0.6}", fill: "none", stroke: "{c20}" }
            }
            if g.center {
                line { x1: "{w / 2.0 - plus / 2.0}", y1: "{h / 2.0}", x2: "{w / 2.0 + plus / 2.0}", y2: "{h / 2.0}", stroke: "{cc}" }
                line { x1: "{w / 2.0}", y1: "{h / 2.0 - plus / 2.0}", x2: "{w / 2.0}", y2: "{h / 2.0 + plus / 2.0}", stroke: "{cc}" }
            }
            if g.rulers {
                for p in rv {
                    line { x1: "{w * p / 100.0}", y1: "0", x2: "{w * p / 100.0}", y2: "{h}", stroke: "{cr}" }
                }
                for p in rh {
                    line { x1: "0", y1: "{h * p / 100.0}", x2: "{w}", y2: "{h * p / 100.0}", stroke: "{cr}" }
                }
            }
        }
    }
}

/// FRAME's ruler boxes: one above the scene for each vertical ruler and
/// left of it for each horizontal one; drag to move the ruler (the bridge
/// does the drag, with FRAME's snapping when Snap is on).
#[component]
fn RulerHandles(ui: Ui) -> Element {
    let g = (ui.guides)();
    let Some((sw, sh, rv, rh)) = ui.editor.read().as_ref().map(|e| {
        let s = &e.doc().scene;
        (
            s.width,
            s.height,
            s.custom_rulers_v.clone().unwrap_or_default(),
            s.custom_rulers_h.clone().unwrap_or_default(),
        )
    }) else {
        return rsx! {};
    };
    if !g.rulers {
        return rsx! {};
    }
    // What a dragged ruler snaps to (percent), besides the other rulers.
    let mut targets_v: Vec<f64> = Vec::new();
    if g.center {
        targets_v.push(50.0);
    }
    if g.thirds {
        targets_v.extend([33.3333, 66.6667]);
    }
    if g.safe10 {
        targets_v.extend([10.0, 90.0]);
    }
    if g.safe20 {
        targets_v.extend([20.0, 80.0]);
    }
    let targets = serde_json::to_string(&targets_v).unwrap_or_default();
    rsx! {
        div {
            class: "ruler-handles",
            "data-w": "{sw}",
            "data-h": "{sh}",
            "data-snap": if g.snap { "1" } else { "0" },
            "data-guides": "{targets}",
            "data-rulers-v": "{serde_json::to_string(&rv).unwrap_or_default()}",
            "data-rulers-h": "{serde_json::to_string(&rh).unwrap_or_default()}",
            for (i , p) in rv.iter().copied().enumerate() {
                div {
                    class: "ruler-handle v",
                    "data-axis": "v",
                    "data-index": "{i}",
                    style: "left:{p}%;",
                    title: "Drag to move this ruler ({number_to_string(p)} %)",
                }
            }
            for (i , p) in rh.iter().copied().enumerate() {
                div {
                    class: "ruler-handle h",
                    "data-axis": "h",
                    "data-index": "{i}",
                    style: "top:{p}%;",
                    title: "Drag to move this ruler ({number_to_string(p)} %)",
                }
            }
        }
    }
}
