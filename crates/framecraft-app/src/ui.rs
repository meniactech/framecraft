// SPDX-License-Identifier: AGPL-3.0-or-later
//! The editor's header and viewport (the Outliner, properties and timeline
//! have their own modules).

use crate::bridge::{FRAME_A, FRAME_B};
use crate::interact::{Geom, ROTATE_OFFSET};
use crate::state::Ui;
use dioxus::prelude::*;
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
    let dirty = (ui.dirty)();

    rsx! {
        div { class: "header",
            span { class: "brand", "FrameCraft" }
            button { title: "A new project with one empty graphic", onclick: move |_| ui.new_project(), "New" }
            button {
                title: "Open a FrameCraft project (its project.json) or a FRAME pack (.frameproject.zip / .framepack.zip)",
                onclick: move |_| ui.open_dialog(),
                "Open…"
            }
            if has_project {
                button {
                    class: if dirty { "save dirty" } else { "save" },
                    title: "Save the project folder (Ctrl+S)",
                    onclick: move |_| ui.save(),
                    if dirty { "Save •" } else { "Save" }
                }
                button { title: "Save as a new FrameCraft project folder", onclick: move |_| ui.save_as(), "Save as…" }
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
                button {
                    title: "Export OGraf, SPX, banner or PNG (Properties → Export)",
                    onclick: move |_| {
                        let mut tab = ui.tab;
                        tab.set(framecraft_core::schema::Tab::Export);
                    },
                    "Export…"
                }
                button {
                    title: "Write the project as a FRAME project pack (.frameproject.zip), to open in FRAME",
                    onclick: move |_| ui.export_pack(),
                    "Export FRAME pack…"
                }
            }
            button {
                title: "The project's graphics, media and your palettes",
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
                    // `display` in both: Dioxus updates style properties one by
                    // one, so a value missing from the new style stays (an
                    // empty start's `display:none` hid the viewport after New).
                    style: if open { "display:block;width:{w * zoom}px;height:{h * zoom}px;" } else { "display:none;width:0;height:0;" },
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
                }
                if !open {
                    div { class: "empty", "No graphic open" }
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
                if let Some((outline, handles, pivot, top, rotate, can_move, marks)) = drawn {
                    polygon { class: if can_move { "sel-outline" } else { "sel-outline locked" }, points: "{outline}" }
                    if can_move {
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
