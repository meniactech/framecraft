// SPDX-License-Identifier: AGPL-3.0-or-later
//! Direct manipulation in the viewport: selecting, dragging handles and
//! keyboard nudges. Events come from the bridge; the math is in
//! `framecraft_core::manipulate`; every change is a command, and a drag is
//! one undo step.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::commands::{Command, SetNumber, SetValueAt};
use framecraft_core::manipulate::{
    Frame, Handle, Vec2, inside, move_by, move_pivot, parent_frame, resize, rotate,
};
use framecraft_core::selection::{movable, pick};
use framecraft_core::timeline::{Pose, pose_at};
use serde_json::Value;

/// How far (screen px) a handle reacts around its center.
pub const HANDLE_REACH: f64 = 8.0;
/// Distance (screen px) of the rotate handle above the top edge.
pub const ROTATE_OFFSET: f64 = 28.0;

/// Where a selected layer is drawn: its corners in scene coordinates
/// (top-left, top-right, bottom-right, bottom-left), measured in the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Geom {
    pub id: String,
    pub corners: [Vec2; 4],
}

impl Geom {
    /// A point at box fractions `(fx, fy)`.
    pub fn at(&self, f: Vec2) -> Vec2 {
        let [tl, tr, _, bl] = self.corners;
        (
            tl.0 + (tr.0 - tl.0) * f.0 + (bl.0 - tl.0) * f.1,
            tl.1 + (tr.1 - tl.1) * f.0 + (bl.1 - tl.1) * f.1,
        )
    }

    /// The rotate handle: above the top edge's middle, `offset` scene units
    /// out along the layer's own up direction.
    pub fn rotate_handle(&self, offset: f64) -> Vec2 {
        let top = self.at((0.5, 0.0));
        let [tl, _, _, bl] = self.corners;
        let up = (tl.0 - bl.0, tl.1 - bl.1);
        let len = (up.0 * up.0 + up.1 * up.1).sqrt().max(1e-9);
        (top.0 + up.0 / len * offset, top.1 + up.1 / len * offset)
    }
}

fn parse_geometry(items: &Value) -> Vec<Geom> {
    let point = |v: &Value| Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?));
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let c = it["corners"].as_array()?;
            Some(Geom {
                id: it["id"].as_str()?.to_string(),
                corners: [
                    point(c.first()?)?,
                    point(c.get(1)?)?,
                    point(c.get(2)?)?,
                    point(c.get(3)?)?,
                ],
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Move,
    Resize(Handle),
    Rotate,
    Pivot,
}

/// A drag in progress: everything is computed from where it started.
#[derive(Debug, Clone)]
pub struct Drag {
    id: String,
    mode: Mode,
    start: Pose,
    parent: Frame,
    from: Vec2,
}

fn near(a: Vec2, b: Vec2, reach: f64) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) <= reach
}

/// Which control of the selected layer is under `p`, if any.
fn control_at(g: &Geom, pose: &Pose, p: Vec2, zoom: f64, alt: bool) -> Option<Mode> {
    let reach = HANDLE_REACH / zoom;
    if near(g.rotate_handle(ROTATE_OFFSET / zoom), p, reach) {
        return Some(Mode::Rotate);
    }
    if near(g.at((pose.pivot_x / 100.0, pose.pivot_y / 100.0)), p, reach) {
        return Some(Mode::Pivot);
    }
    for h in Handle::ALL {
        if near(g.at(h.at()), p, reach) {
            return Some(Mode::Resize(h));
        }
    }
    // Alt-click inside goes to the layer below instead.
    (!alt && inside(&g.corners, p)).then_some(Mode::Move)
}

fn point(ev: &Value) -> Vec2 {
    (
        ev["x"].as_f64().unwrap_or(0.0),
        ev["y"].as_f64().unwrap_or(0.0),
    )
}

fn flag(ev: &Value, name: &str) -> bool {
    ev[name].as_bool().unwrap_or(false)
}

/// Handles one event from the bridge.
pub fn on_event(ui: Ui, ev: Value) {
    match ev["type"].as_str() {
        Some("geometry") => {
            let mut geometry = ui.geometry;
            geometry.set(parse_geometry(&ev["items"]));
        }
        Some("pointer") => match ev["kind"].as_str() {
            Some("down") => pointer_down(ui, &ev),
            Some("move") => pointer_move(ui, &ev),
            Some("up") => pointer_up(ui),
            _ => {}
        },
        Some("outliner-drop") => crate::outliner::on_drop(ui, &ev),
        Some("key") => key(ui, ev["key"].as_str().unwrap_or(""), flag(&ev, "shift")),
        _ => {}
    }
}

fn start_drag(ui: Ui, id: &str, mode: Mode, from: Vec2) {
    let time = *ui.time.peek();
    let started = ui.editor.peek().as_ref().and_then(|e| {
        let doc = e.doc();
        let item = doc.items.iter().find(|i| i.id == id)?;
        Some((
            pose_at(doc, item, time),
            parent_frame(doc, id, time),
            item.name.clone(),
        ))
    });
    let Some((start, parent, name)) = started else {
        return;
    };
    let verb = match mode {
        Mode::Move => "Move",
        Mode::Resize(_) => "Resize",
        Mode::Rotate => "Rotate",
        Mode::Pivot => "Move pivot of",
    };
    ui.begin_gesture(format!("{verb} {name}"));
    let mut drag = ui.drag;
    drag.set(Some(Drag {
        id: id.to_string(),
        mode,
        start,
        parent,
        from,
    }));
}

fn pointer_down(ui: Ui, ev: &Value) {
    let p = point(ev);
    let alt = flag(ev, "alt");
    let zoom = *ui.zoom.peek();
    let time = *ui.time.peek();

    // The selected layer's own controls come first.
    let current = ui.editor.peek().as_ref().and_then(|e| e.selected.clone());
    if let Some(id) = &current {
        let control = {
            let editor = ui.editor.peek();
            let geometry = ui.geometry.peek();
            editor
                .as_ref()
                .zip(geometry.iter().find(|g| &g.id == id))
                .and_then(|(e, g)| {
                    let doc = e.doc();
                    let item = doc.items.iter().find(|i| &i.id == id)?;
                    if !movable(doc, id) {
                        return None;
                    }
                    control_at(g, &pose_at(doc, item, time), p, zoom, alt)
                })
        };
        if let Some(mode) = control {
            start_drag(ui, id, mode, p);
            return;
        }
    }

    // Otherwise select what is under the pointer and start moving it.
    let hits: Vec<String> = ev["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str().map(String::from))
        .collect();
    let picked = ui
        .editor
        .peek()
        .as_ref()
        .and_then(|e| pick(e.doc(), &hits, current.as_deref(), alt));
    ui.select_layer(picked.clone());
    if let Some(id) = picked {
        let can_move = ui
            .editor
            .peek()
            .as_ref()
            .is_some_and(|e| movable(e.doc(), &id));
        if can_move {
            start_drag(ui, &id, Mode::Move, p);
        }
    }
}

fn set(id: &str, lane: &str, time_ms: f64, value: f64, auto_key: bool) -> Box<dyn Command> {
    Box::new(SetValueAt {
        item: id.into(),
        lane: lane.into(),
        time_ms,
        value,
        auto_key,
    })
}

fn pointer_move(ui: Ui, ev: &Value) {
    let Some(d) = ui.drag.peek().clone() else {
        return;
    };
    let p = point(ev);
    let (shift, alt) = (flag(ev, "shift"), flag(ev, "alt"));
    let (t, auto) = (*ui.time.peek(), *ui.auto_key.peek());
    let id = d.id.as_str();
    let cmds: Vec<Box<dyn Command>> = match d.mode {
        Mode::Move => {
            let (x, y) = move_by(&d.start, &d.parent, d.from, p, shift);
            vec![set(id, "x", t, x, auto), set(id, "y", t, y, auto)]
        }
        Mode::Resize(h) => {
            let (x, y, w, hh) = resize(&d.start, &d.parent, h, d.from, p, shift, alt);
            vec![
                set(id, "x", t, x, auto),
                set(id, "y", t, y, auto),
                set(id, "width", t, w, auto),
                set(id, "height", t, hh, auto),
            ]
        }
        Mode::Rotate => vec![set(
            id,
            "rotation",
            t,
            rotate(&d.start, &d.parent, d.from, p, shift),
            auto,
        )],
        Mode::Pivot => {
            let (x, y, px, py) = move_pivot(&d.start, &d.parent, p);
            vec![
                Box::new(SetNumber {
                    item: id.into(),
                    field: "pivotX".into(),
                    value: px,
                }),
                Box::new(SetNumber {
                    item: id.into(),
                    field: "pivotY".into(),
                    value: py,
                }),
                set(id, "x", t, x, auto),
                set(id, "y", t, y, auto),
            ]
        }
    };
    ui.execute_all(&cmds);
}

fn pointer_up(ui: Ui) {
    let mut drag = ui.drag;
    if drag.peek().is_some() {
        drag.set(None);
        ui.end_gesture();
    }
}

fn key(ui: Ui, key: &str, shift: bool) {
    match key {
        "undo" => ui.undo(),
        "redo" => ui.redo(),
        "Escape" => ui.select_layer(None),
        "Delete" | "Backspace" => ui.delete_selected(),
        "duplicate" => ui.duplicate_selected(),
        "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
            let step = if shift { 10.0 } else { 1.0 };
            let (dx, dy) = match key {
                "ArrowLeft" => (-step, 0.0),
                "ArrowRight" => (step, 0.0),
                "ArrowUp" => (0.0, -step),
                _ => (0.0, step),
            };
            let (t, auto) = (*ui.time.peek(), *ui.auto_key.peek());
            let target = ui.editor.peek().as_ref().and_then(|e| {
                let id = e.selected.clone()?;
                let doc = e.doc();
                let item = doc.items.iter().find(|i| i.id == id)?;
                movable(doc, &id).then(|| (id, pose_at(doc, item, t)))
            });
            if let Some((id, pose)) = target {
                let cmds: Vec<Box<dyn Command>> = vec![
                    set(&id, "x", t, pose.x + dx, auto),
                    set(&id, "y", t, pose.y + dy, auto),
                ];
                ui.begin_gesture("Nudge".into());
                ui.execute_all(&cmds);
                ui.end_gesture();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Geom {
        Geom {
            id: "a".into(),
            corners: [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)],
        }
    }

    fn pose() -> Pose {
        Pose {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            pivot_x: 50.0,
            pivot_y: 50.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn controls_by_priority() {
        let (g, p) = (square(), pose());
        assert_eq!(
            control_at(&g, &p, (50.0, -28.0), 1.0, false),
            Some(Mode::Rotate)
        );
        assert_eq!(
            control_at(&g, &p, (50.0, 25.0), 1.0, false),
            Some(Mode::Pivot)
        );
        assert_eq!(
            control_at(&g, &p, (101.0, 49.0), 1.0, false),
            Some(Mode::Resize(Handle { x: 1, y: 1 }))
        );
        assert_eq!(
            control_at(&g, &p, (20.0, 20.0), 1.0, false),
            Some(Mode::Move)
        );
        assert_eq!(control_at(&g, &p, (20.0, 20.0), 1.0, true), None);
        assert_eq!(control_at(&g, &p, (300.0, 300.0), 1.0, false), None);
        // Handles keep their screen size: at 50% zoom they reach twice as
        // far (16 scene units; this point is 15.6 from the corner).
        assert_eq!(
            control_at(&g, &p, (111.0, 61.0), 0.5, false),
            Some(Mode::Resize(Handle { x: 1, y: 1 }))
        );
    }

    #[test]
    fn geometry_from_the_page() {
        let v = serde_json::json!([{ "id": "a", "corners": [[0, 0], [10, 0], [10, 5], [0, 5]] }, { "id": "bad" }]);
        assert_eq!(
            parse_geometry(&v),
            vec![Geom {
                id: "a".into(),
                corners: [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)]
            }]
        );
    }
}
