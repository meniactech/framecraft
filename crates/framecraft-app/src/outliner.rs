// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Outliner: the graphic's layers as a tree. The top of the list is
//! drawn on top. Click selects, double-click renames, the eye and lock
//! switch visibility and locking, and dragging a row moves the layer
//! (above, below, or into another layer).

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::commands::{
    FieldTarget, MoveItem, Rename, SetField, SetLocked, SetVisible, Update,
};
use framecraft_core::layers::NewLayer;
use framecraft_core::model::{Item, ItemType};
use framecraft_core::normalize::Document;

#[derive(Clone, PartialEq)]
struct Row {
    id: String,
    name: String,
    icon: &'static str,
    depth: usize,
    has_children: bool,
    collapsed: bool,
    visible: bool,
    locked: bool,
    lock_children: bool,
}

/// Where a dragged row lands, relative to the row under the mouse.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Zone {
    Above,
    Into,
    Below,
}

#[derive(Clone, PartialEq)]
struct Dragging {
    id: String,
    /// The mouse has left the row it started on (a real drag, not a click).
    moved: bool,
    target: Option<(String, Zone)>,
}

fn parent_of(item: &Item) -> Option<String> {
    item.parent_id.clone().flatten()
}

fn icon(item: &Item) -> &'static str {
    match item.item_type {
        Some(ItemType::Text) => "T",
        Some(ItemType::Empty) => "◌",
        Some(ItemType::ImageSlot) => "▣",
        Some(ItemType::Chart) => "▤",
        Some(ItemType::Shape) | None => "▭",
        _ => "•",
    }
}

/// Layers in list order: children under their parent, topmost first.
fn rows(doc: &Document, collapsed: &[String]) -> Vec<Row> {
    fn visit(
        doc: &Document,
        parent: Option<&str>,
        depth: usize,
        collapsed: &[String],
        out: &mut Vec<Row>,
    ) {
        let children: Vec<&Item> = doc
            .items
            .iter()
            .filter(|i| parent_of(i).as_deref() == parent)
            .collect();
        for item in children.into_iter().rev() {
            let has_children = doc
                .items
                .iter()
                .any(|i| parent_of(i).as_deref() == Some(item.id.as_str()));
            let is_collapsed = collapsed.contains(&item.id);
            out.push(Row {
                id: item.id.clone(),
                name: item.name.clone(),
                icon: icon(item),
                depth,
                has_children,
                collapsed: is_collapsed,
                visible: item.visible != Some(false),
                locked: item.locked == Some(true),
                lock_children: item.lock_children == Some(true),
            });
            if has_children && !is_collapsed && depth < 32 {
                visit(doc, Some(&item.id), depth + 1, collapsed, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(doc, None, 0, collapsed, &mut out);
    out
}

/// Whether `id` is `ancestor` or nested under it.
fn within(doc: &Document, id: &str, ancestor: &str) -> bool {
    let mut current = Some(id.to_string());
    for _ in 0..64 {
        match current {
            Some(c) if c == ancestor => return true,
            Some(c) => current = doc.items.iter().find(|i| i.id == c).and_then(parent_of),
            None => return false,
        }
    }
    false
}

/// The move a drop makes, or `None` when it would go nowhere.
fn drop_move(doc: &Document, mover: &str, target: &str, zone: Zone, time: f64) -> Option<MoveItem> {
    if within(doc, target, mover) {
        return None;
    }
    let target_item = doc.items.iter().find(|i| i.id == target)?;
    let (parent, index) = match zone {
        // On top of the target's children.
        Zone::Into => (Some(target.to_string()), usize::MAX),
        Zone::Above | Zone::Below => {
            let parent = parent_of(target_item);
            let pos = doc
                .items
                .iter()
                .filter(|i| parent_of(i) == parent && i.id != mover)
                .position(|i| i.id == target)?;
            // The list shows the topmost first: above means drawn later.
            (parent, if zone == Zone::Above { pos + 1 } else { pos })
        }
    };
    Some(MoveItem {
        item: mover.to_string(),
        parent,
        index,
        keep_place: Some(time),
    })
}

#[component]
pub fn Outliner(ui: Ui) -> Element {
    let mut collapsed = use_signal(Vec::<String>::new);
    let mut renaming = use_signal(|| None::<String>);
    let mut dragging = use_signal(|| None::<Dragging>);
    let mut menu = use_signal(|| false);

    let (rows, selected) = match ui.editor.read().as_ref() {
        Some(e) => (rows(e.doc(), &collapsed.read()), e.selected.clone()),
        None => (Vec::new(), None),
    };
    let open = ui.editor.read().is_some();
    let drag_now = dragging.read().clone();

    let finish_drag = move |_| {
        let Some(d) = dragging.take() else { return };
        let Some((target, zone)) = d.target.filter(|_| d.moved) else {
            return;
        };
        let time = *ui.time.peek();
        let cmd = ui
            .editor
            .peek()
            .as_ref()
            .and_then(|e| drop_move(e.doc(), &d.id, &target, zone, time));
        if let Some(cmd) = cmd {
            ui.execute(&cmd);
        }
    };

    rsx! {
        div { class: "outliner",
            div { class: "panel-head",
                span { class: "panel-title", "Layers" }
                if open {
                    div { class: "add-wrap",
                        button {
                            title: "Add a layer",
                            onclick: move |_| menu.set(!menu()),
                            "+ Add"
                        }
                        if menu() {
                            div { class: "menu", onmouseleave: move |_| menu.set(false),
                                for kind in NewLayer::ALL {
                                    button {
                                        onclick: move |_| {
                                            menu.set(false);
                                            ui.add_layer(kind);
                                        },
                                        "{kind.label()}"
                                    }
                                }
                            }
                        }
                    }
                    button {
                        disabled: selected.is_none(),
                        title: "Duplicate (Ctrl+D)",
                        onclick: move |_| ui.duplicate_selected(),
                        "Duplicate"
                    }
                    button {
                        disabled: selected.is_none(),
                        title: "Delete (Delete)",
                        onclick: move |_| ui.delete_selected(),
                        "Delete"
                    }
                }
            }
            div {
                class: if drag_now.as_ref().is_some_and(|d| d.moved) { "layer-list dragging" } else { "layer-list" },
                onmouseup: finish_drag,
                onmouseleave: move |_| dragging.set(None),
                if rows.is_empty() {
                    p { class: "dim pad", if open { "No layers. Add one with + Add." } else { "Nothing open" } }
                }
                for row in rows {
                    {
                        let is_selected = selected.as_deref() == Some(row.id.as_str());
                        let target = drag_now
                            .as_ref()
                            .filter(|d| d.moved && d.id != row.id)
                            .and_then(|d| d.target.clone())
                            .filter(|(t, _)| *t == row.id)
                            .map(|(_, z)| z);
                        let class = format!(
                            "layer-row{}{}{}{}",
                            if is_selected { " selected" } else { "" },
                            if row.visible { "" } else { " hidden" },
                            match target {
                                Some(Zone::Above) => " drop-above",
                                Some(Zone::Into) => " drop-into",
                                Some(Zone::Below) => " drop-below",
                                None => "",
                            },
                            if drag_now.as_ref().is_some_and(|d| d.moved && d.id == row.id) { " drag-source" } else { "" },
                        );
                        let show_zones = drag_now.as_ref().is_some_and(|d| d.moved && d.id != row.id);
                        let is_renaming = renaming.read().as_deref() == Some(row.id.as_str());
                        let Row { id, name, icon, depth, has_children, collapsed: is_collapsed, visible, locked, lock_children } = row;
                        let (id1, id2, id3, id4, id5, id6, id7, id8) = (
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                        );
                        rsx! {
                            div {
                                key: "{id}",
                                class: "{class}",
                                style: "padding-left:{4 + depth * 14}px;",
                                onmousedown: move |evt| {
                                    if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                                        return;
                                    }
                                    ui.select_layer(Some(id1.clone()));
                                    dragging.set(Some(Dragging { id: id1.clone(), moved: false, target: None }));
                                },
                                onmouseleave: move |_| {
                                    let mut d = dragging.write();
                                    if let Some(d) = d.as_mut().filter(|d| d.id == id2) {
                                        d.moved = true;
                                    }
                                },
                                if has_children {
                                    span {
                                        class: "caret",
                                        onmousedown: move |evt| evt.stop_propagation(),
                                        onclick: move |_| {
                                            let mut c = collapsed.write();
                                            if let Some(i) = c.iter().position(|x| *x == id3) {
                                                c.remove(i);
                                            } else {
                                                c.push(id3.clone());
                                            }
                                        },
                                        if is_collapsed { "▸" } else { "▾" }
                                    }
                                } else {
                                    span { class: "caret" }
                                }
                                span { class: "layer-icon", "{icon}" }
                                if is_renaming {
                                    input {
                                        class: "rename",
                                        value: "{name}",
                                        onmousedown: move |evt| evt.stop_propagation(),
                                        onmounted: move |evt| async move {
                                            let _ = evt.set_focus(true).await;
                                        },
                                        onchange: move |evt| {
                                            let name = evt.value().trim().to_string();
                                            if !name.is_empty() {
                                                ui.execute(&Rename { item: id4.clone(), name });
                                            }
                                            renaming.set(None);
                                        },
                                        onkeydown: move |evt| {
                                            if evt.key() == Key::Escape {
                                                renaming.set(None);
                                            }
                                        },
                                        onblur: move |_| renaming.set(None),
                                    }
                                } else {
                                    span {
                                        class: "layer-name",
                                        ondoubleclick: move |_| renaming.set(Some(id5.clone())),
                                        "{name}"
                                    }
                                }
                                if has_children {
                                    button {
                                        class: if lock_children { "row-toggle on" } else { "row-toggle" },
                                        title: "Lock children: clicking a child in the viewport selects this layer",
                                        onmousedown: move |evt| evt.stop_propagation(),
                                        onclick: move |_| {
                                            ui.execute(&SetField {
                                                target: FieldTarget::Item(id6.clone()),
                                                path: "lockChildren".into(),
                                                value: (!lock_children).into(),
                                                update: Update::None,
                                            });
                                        },
                                        "⛓"
                                    }
                                }
                                button {
                                    class: if locked { "row-toggle on" } else { "row-toggle" },
                                    title: if locked { "Unlock" } else { "Lock (locked layers are click-through in the viewport)" },
                                    onmousedown: move |evt| evt.stop_propagation(),
                                    onclick: move |_| ui.execute(&SetLocked { item: id7.clone(), locked: !locked }),
                                    if locked { "🔒" } else { "🔓" }
                                }
                                button {
                                    class: if visible { "row-toggle" } else { "row-toggle on" },
                                    title: if visible { "Hide" } else { "Show" },
                                    onmousedown: move |evt| evt.stop_propagation(),
                                    onclick: move |_| ui.execute(&SetVisible { item: id8.clone(), visible: !visible }),
                                    if visible { "👁" } else { "–" }
                                }
                                if show_zones {
                                    {
                                        let (a, b, c) = (id.clone(), id.clone(), id.clone());
                                        rsx! {
                                            div { class: "drop-zones",
                                                div {
                                                    class: "zone",
                                                    onmouseenter: move |_| set_target(dragging, &a, Zone::Above),
                                                }
                                                div {
                                                    class: "zone",
                                                    onmouseenter: move |_| set_target(dragging, &b, Zone::Into),
                                                }
                                                div {
                                                    class: "zone",
                                                    onmouseenter: move |_| set_target(dragging, &c, Zone::Below),
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
    }
}

fn set_target(mut dragging: Signal<Option<Dragging>>, id: &str, zone: Zone) {
    if let Some(d) = dragging.write().as_mut() {
        d.target = Some((id.to_string(), zone));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use framecraft_core::commands::{Command, History};
    use framecraft_core::layers::new_layer;
    use framecraft_core::model::Graphic;
    use framecraft_core::normalize::normalize;
    use framecraft_core::profile::Profile;
    use serde_json::json;

    fn doc() -> Document {
        let g: Graphic = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 100, "height": 100 },
            "items": []
        }))
        .unwrap();
        let mut d = normalize(&g, &Profile::framecraft());
        for (id, parent) in [("a", None), ("b", None), ("c", Some("b")), ("d", None)] {
            let mut item = new_layer(NewLayer::Box, parent);
            item.id = id.into();
            d.items.push(item);
        }
        d
    }

    fn order(d: &Document) -> Vec<String> {
        rows(d, &[])
            .into_iter()
            .map(|r| format!("{}{}", " ".repeat(r.depth), r.id))
            .collect()
    }

    fn drop(d: &mut Document, mover: &str, target: &str, zone: Zone) {
        let cmd = drop_move(d, mover, target, zone, 0.0).expect("a move");
        History::default().execute(d, &cmd as &dyn Command).unwrap();
    }

    #[test]
    fn list_shows_topmost_first() {
        assert_eq!(order(&doc()), ["d", "b", " c", "a"]);
    }

    #[test]
    fn drops_land_where_the_row_shows() {
        let mut d = doc();
        drop(&mut d, "a", "d", Zone::Above);
        assert_eq!(order(&d), ["a", "d", "b", " c"]);
        drop(&mut d, "a", "b", Zone::Below);
        assert_eq!(order(&d), ["d", "b", " c", "a"]);
        drop(&mut d, "d", "c", Zone::Into);
        assert_eq!(order(&d), ["b", " c", "  d", "a"]);
        drop(&mut d, "a", "c", Zone::Above);
        assert_eq!(order(&d), ["b", " a", " c", "  d",]);
    }

    #[test]
    fn no_drop_into_itself() {
        let d = doc();
        assert!(drop_move(&d, "b", "c", Zone::Into, 0.0).is_none());
        assert!(drop_move(&d, "b", "b", Zone::Above, 0.0).is_none());
    }
}
