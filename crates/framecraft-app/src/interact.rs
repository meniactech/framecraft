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

    /// A layer-local point (px in a `w` × `h` box) in scene coordinates.
    pub fn to_scene(&self, local: Vec2, w: f64, h: f64) -> Vec2 {
        self.at((local.0 / w.max(1e-9), local.1 / h.max(1e-9)))
    }

    /// A scene point in layer-local px (the inverse of [`Geom::to_scene`]),
    /// through rotation, scale and parents as drawn.
    pub fn to_local(&self, p: Vec2, w: f64, h: f64) -> Vec2 {
        let [tl, tr, _, bl] = self.corners;
        let (u, v) = ((tr.0 - tl.0, tr.1 - tl.1), (bl.0 - tl.0, bl.1 - tl.1));
        let det = u.0 * v.1 - u.1 * v.0;
        if det.abs() < 1e-12 {
            return (0.0, 0.0);
        }
        let d = (p.0 - tl.0, p.1 - tl.1);
        let fx = (d.0 * v.1 - d.1 * v.0) / det;
        let fy = (u.0 * d.1 - u.1 * d.0) / det;
        (fx * w, fy * h)
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
    /// A polygon / path point (its handles move with it).
    Point(usize),
    /// A point's bezier handle: `true` for the out handle.
    PointHandle(usize, bool),
}

/// A selected polygon or path's points in scene coordinates, for drawing
/// and hits: `(point, Some((in, out)) when it shows bezier handles)`.
pub fn point_marks(
    g: &Geom,
    item: &framecraft_core::model::Item,
    pose: &Pose,
) -> Vec<(Vec2, Option<(Vec2, Vec2)>)> {
    use framecraft_core::model::ShapeType;
    if !matches!(item.shape_type, Some(ShapeType::Polygon | ShapeType::Path)) {
        return Vec::new();
    }
    let bezier = item.polygon_curve.as_deref() == Some("bezier");
    let (w, h) = (pose.width, pose.height);
    item.polygon_points
        .iter()
        .flatten()
        .map(|p| {
            let at = g.to_scene((p.x, p.y), w, h);
            let handles = (bezier && p.curve.as_deref() == Some("bezier")).then(|| {
                (
                    g.to_scene((p.in_x.unwrap_or(p.x), p.in_y.unwrap_or(p.y)), w, h),
                    g.to_scene((p.out_x.unwrap_or(p.x), p.out_y.unwrap_or(p.y)), w, h),
                )
            });
            (at, handles)
        })
        .collect()
}

/// Which polygon point or handle is under `p`. Handles first (they sit
/// near their point), then points.
fn point_at(marks: &[(Vec2, Option<(Vec2, Vec2)>)], p: Vec2, zoom: f64) -> Option<Mode> {
    let reach = HANDLE_REACH / zoom;
    for (i, (_, handles)) in marks.iter().enumerate() {
        if let Some((hin, hout)) = handles {
            if near(*hout, p, reach) {
                return Some(Mode::PointHandle(i, true));
            }
            if near(*hin, p, reach) {
                return Some(Mode::PointHandle(i, false));
            }
        }
    }
    marks
        .iter()
        .position(|(at, _)| near(*at, p, reach))
        .map(Mode::Point)
}

/// A drag in progress: everything is computed from where it started.
#[derive(Debug, Clone)]
pub struct Drag {
    id: String,
    mode: Mode,
    start: Pose,
    parent: Frame,
    from: Vec2,
    /// A polygon / path's points when the drag began (a resize scales them).
    points: Option<Vec<framecraft_core::model::PolygonPoint>>,
    /// Where the layer was drawn when the drag began (point drags).
    geom: Option<Geom>,
    /// Other selected layers moved along (multi-select): id, start pose and
    /// parent frame.
    group: Vec<(String, Pose, Frame)>,
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
        Some("tl") => crate::timeline::on_event(ui, &ev),
        Some("key") => key(ui, ev["key"].as_str().unwrap_or(""), flag(&ev, "shift")),
        Some("ramp") => {
            let mut ramp = ui.ramp_event;
            ramp.set(Some(ev.clone()));
        }
        Some("view") => {
            let n = |k: &str| ev[k].as_f64().unwrap_or(0.0);
            match ev["action"].as_str() {
                Some("zoom") => crate::ui::zoom_at(ui, n("factor"), n("x"), n("y"), n("w"), n("h")),
                Some("pan") => crate::ui::pan_by(ui, n("dx"), n("dy")),
                _ => {}
            }
        }
        Some("log") => {
            if crate::state::debug() {
                eprintln!("{}", ev["text"].as_str().unwrap_or(""));
            }
        }
        Some("hint") => {
            let mut hint = ui.hint;
            let text = ev["text"].as_str().unwrap_or("").to_string();
            if *hint.peek() != text {
                hint.set(text);
            }
        }
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
            item.polygon_points.clone().filter(|p| p.len() >= 2),
        ))
    });
    let Some((start, parent, name, points)) = started else {
        return;
    };
    // A move drags every selected (movable) layer along.
    let group: Vec<(String, Pose, Frame)> = if mode == Mode::Move {
        ui.editor
            .peek()
            .as_ref()
            .map(|e| {
                let doc = e.doc();
                e.selection()
                    .into_iter()
                    .filter(|o| o != id && movable(doc, o))
                    .filter_map(|o| {
                        let item = doc.items.iter().find(|i| i.id == o)?;
                        Some((
                            o.clone(),
                            pose_at(doc, item, time),
                            parent_frame(doc, &o, time),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let name = if group.is_empty() {
        name
    } else {
        format!("{} layers", group.len() + 1)
    };
    let verb = match mode {
        Mode::Move => "Move",
        Mode::Resize(_) => "Resize",
        Mode::Rotate => "Rotate",
        Mode::Pivot => "Move pivot of",
        Mode::Point(_) | Mode::PointHandle(..) => "Edit points of",
    };
    ui.begin_gesture(format!("{verb} {name}"));
    let mut drag = ui.drag;
    drag.set(Some(Drag {
        id: id.to_string(),
        mode,
        start,
        parent,
        from,
        points,
        geom: ui.geometry.peek().iter().find(|g| g.id == id).cloned(),
        group,
    }));
}

fn pointer_down(ui: Ui, ev: &Value) {
    let p = point(ev);
    let alt = flag(ev, "alt");
    let zoom = *ui.zoom.peek();
    let time = *ui.time.peek();

    // The selected layer's own controls come first: polygon points, then
    // the box handles.
    let current = ui.editor.peek().as_ref().and_then(|e| e.selected.clone());
    if let Some(id) = &current
        && point_edit(ui, id, p, ev)
    {
        return;
    }
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
    // Ctrl+click adds to or removes from the selection.
    if flag(ev, "ctrl") {
        if let Some(id) = picked {
            ui.toggle_layer(&id);
        }
        return;
    }
    // Pressing a layer that is already selected keeps the selection (to
    // drag them all); anything else selects just that layer.
    let selection = ui.selection();
    match &picked {
        Some(id) if selection.len() > 1 && selection.contains(id) => {
            let others = selection.into_iter().filter(|x| x != id).collect();
            ui.set_selection(Some(id.clone()), others);
        }
        _ => ui.select_layer(picked.clone()),
    }
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

/// Polygon / path point editing on the selected layer (FRAME's viewport
/// gestures): drag a point or handle, Ctrl+click a point to smooth or
/// sharpen it, Alt+click to remove it, Shift+click near an edge to insert
/// one. Returns whether the click was used.
fn point_edit(ui: Ui, id: &str, p: Vec2, ev: &Value) -> bool {
    use framecraft_core::model::ShapeType;
    use framecraft_core::polygon::{SetPolygon, any_bezier, insert_near, toggle_smooth};
    let zoom = *ui.zoom.peek();
    let time = *ui.time.peek();
    let found = {
        let editor = ui.editor.peek();
        let geometry = ui.geometry.peek();
        editor.as_ref().and_then(|e| {
            let doc = e.doc();
            let item = doc.items.iter().find(|i| i.id == id)?;
            if !movable(doc, id)
                || !matches!(item.shape_type, Some(ShapeType::Polygon | ShapeType::Path))
            {
                return None;
            }
            let g = geometry.iter().find(|g| g.id == id)?;
            let pose = pose_at(doc, item, time);
            let marks = point_marks(g, item, &pose);
            let local = g.to_local(p, pose.width, pose.height);
            let min = if item.shape_type == Some(ShapeType::Path) {
                2
            } else {
                3
            };
            Some((
                point_at(&marks, p, zoom),
                local,
                item.polygon_points.clone()?,
                min,
            ))
        })
    };
    let Some((hit, local, points, min)) = found else {
        return false;
    };
    let edit = |points: Vec<_>| {
        let curve = any_bezier(&points).then(|| "bezier".to_string());
        ui.execute(&SetPolygon {
            item: id.into(),
            points: Some(points),
            curve,
            to_shape: false,
        });
    };
    match hit {
        Some(Mode::Point(i)) if flag(ev, "alt") => {
            if points.len() > min {
                let mut next = points;
                next.remove(i);
                edit(next);
            }
            true
        }
        Some(Mode::Point(i)) if flag(ev, "ctrl") => {
            edit(toggle_smooth(&points, i));
            true
        }
        Some(mode) => {
            start_drag(ui, id, mode, p);
            true
        }
        None if flag(ev, "shift") => {
            // FRAME: reach 28 local px, more when zoomed out.
            match insert_near(&points, local.0, local.1, 28f64.max(28.0 / zoom)) {
                Some(next) => {
                    edit(next);
                    true
                }
                None => false,
            }
        }
        None => false,
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
            let mut cmds = vec![set(id, "x", t, x, auto), set(id, "y", t, y, auto)];
            for (other, start, parent) in &d.group {
                let (ox, oy) = move_by(start, parent, d.from, p, shift);
                cmds.push(set(other, "x", t, ox, auto));
                cmds.push(set(other, "y", t, oy, auto));
            }
            cmds
        }
        Mode::Resize(h) => {
            let (x, y, w, hh) = resize(&d.start, &d.parent, h, d.from, p, shift, alt);
            let mut cmds = vec![
                set(id, "x", t, x, auto),
                set(id, "y", t, y, auto),
                set(id, "width", t, w, auto),
                set(id, "height", t, hh, auto),
            ];
            // Polygons and paths scale their points with the box (FRAME).
            if let Some(points) = &d.points {
                let scaled = framecraft_core::polygon::scale_points(
                    points,
                    d.start.width,
                    d.start.height,
                    w,
                    hh,
                );
                cmds.push(Box::new(framecraft_core::commands::SetField {
                    target: framecraft_core::commands::FieldTarget::Item(id.into()),
                    path: "polygonPoints".into(),
                    value: serde_json::to_value(scaled).unwrap_or_default(),
                    update: framecraft_core::commands::Update::Layer,
                }));
            }
            cmds
        }
        Mode::Point(i) | Mode::PointHandle(i, _) => {
            let (Some(points), Some(g)) = (&d.points, &d.geom) else {
                return;
            };
            let (w, h) = (d.start.width, d.start.height);
            let (a, b) = (g.to_local(d.from, w, h), g.to_local(p, w, h));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let mut next = points.clone();
            let Some(q) = next.get_mut(i) else { return };
            let shift = |v: Option<f64>, by: f64| v.map(|v| v + by);
            match d.mode {
                Mode::PointHandle(_, true) => {
                    q.out_x = Some(q.out_x.unwrap_or(q.x) + dx);
                    q.out_y = Some(q.out_y.unwrap_or(q.y) + dy);
                }
                Mode::PointHandle(_, false) => {
                    q.in_x = Some(q.in_x.unwrap_or(q.x) + dx);
                    q.in_y = Some(q.in_y.unwrap_or(q.y) + dy);
                }
                _ => {
                    q.x += dx;
                    q.y += dy;
                    q.in_x = shift(q.in_x, dx);
                    q.in_y = shift(q.in_y, dy);
                    q.out_x = shift(q.out_x, dx);
                    q.out_y = shift(q.out_y, dy);
                }
            }
            vec![Box::new(framecraft_core::commands::SetField {
                target: framecraft_core::commands::FieldTarget::Item(id.into()),
                path: "polygonPoints".into(),
                value: serde_json::to_value(next).unwrap_or_default(),
                update: framecraft_core::commands::Update::Layer,
            })]
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
    let ended = drag.peek().clone();
    if let Some(d) = ended {
        drag.set(None);
        // After a point drag the box fits the points (FRAME), in the same
        // undo step.
        if matches!(d.mode, Mode::Point(_) | Mode::PointHandle(..)) {
            ui.execute(&framecraft_core::polygon::SetPolygon {
                item: d.id.clone(),
                points: None,
                curve: None,
                to_shape: false,
            });
        }
        ui.end_gesture();
    }
}

fn key(ui: Ui, key: &str, shift: bool) {
    // Selected keyframes and cues take Delete, Escape and arrows first.
    if crate::timeline::key(ui, key, shift) {
        return;
    }
    match key {
        "undo" => ui.undo(),
        "redo" => ui.redo(),
        "Escape" => ui.select_layer(None),
        "Delete" | "Backspace" => ui.delete_selected(),
        "duplicate" => ui.duplicate_selected(),
        "save" => ui.save(),
        "save-as" => ui.save_as(),
        "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
            let step = if shift { 10.0 } else { 1.0 };
            let (dx, dy) = match key {
                "ArrowLeft" => (-step, 0.0),
                "ArrowRight" => (step, 0.0),
                "ArrowUp" => (0.0, -step),
                _ => (0.0, step),
            };
            let (t, auto) = (*ui.time.peek(), *ui.auto_key.peek());
            // Every selected (movable) layer moves.
            let cmds: Vec<Box<dyn Command>> = ui
                .editor
                .peek()
                .as_ref()
                .map(|e| {
                    let doc = e.doc();
                    e.selection()
                        .into_iter()
                        .filter(|id| movable(doc, id))
                        .filter_map(|id| {
                            let item = doc.items.iter().find(|i| i.id == id)?;
                            let pose = pose_at(doc, item, t);
                            Some([
                                set(&id, "x", t, pose.x + dx, auto),
                                set(&id, "y", t, pose.y + dy, auto),
                            ])
                        })
                        .flatten()
                        .collect()
                })
                .unwrap_or_default();
            if !cmds.is_empty() {
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

    #[test]
    fn local_points_round_trip_through_a_rotated_scaled_box() {
        // A 100 × 50 layer drawn rotated 90° and twice as big.
        let g = Geom {
            id: "a".into(),
            corners: [(200.0, 0.0), (200.0, 200.0), (100.0, 200.0), (100.0, 0.0)],
        };
        let s = g.to_scene((25.0, 10.0), 100.0, 50.0);
        assert_eq!(s, (180.0, 50.0));
        let l = g.to_local(s, 100.0, 50.0);
        assert!((l.0 - 25.0).abs() < 1e-9 && (l.1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn handles_win_over_their_point() {
        let marks = vec![
            ((10.0, 10.0), Some(((0.0, 10.0), (30.0, 10.0)))),
            ((50.0, 50.0), None),
        ];
        assert_eq!(
            point_at(&marks, (30.0, 11.0), 1.0),
            Some(Mode::PointHandle(0, true))
        );
        assert_eq!(point_at(&marks, (11.0, 10.0), 1.0), Some(Mode::Point(0)));
        assert_eq!(point_at(&marks, (50.0, 55.0), 1.0), Some(Mode::Point(1)));
        assert_eq!(point_at(&marks, (80.0, 80.0), 1.0), None);
    }

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
