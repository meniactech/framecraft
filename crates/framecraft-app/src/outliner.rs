// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Outliner: the graphic's layers as a tree. The top of the list is
//! drawn on top. Click selects, double-click renames, the eye and lock
//! switch visibility and locking, and dragging a row moves the layer
//! (above, below, or into another layer).

use crate::state::{PickTarget, Ui};
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

fn parent_of(item: &Item) -> Option<String> {
    item.parent_id.clone().flatten()
}

fn icon(item: &Item) -> &'static str {
    match item.item_type {
        Some(ItemType::Text) => "T",
        Some(ItemType::Empty) => "◌",
        Some(ItemType::ImageSlot) => "▣",
        Some(ItemType::Chart) => "▤",
        Some(ItemType::Qr) => "▦",
        Some(ItemType::Particles) => "✦",
        Some(ItemType::Lightning) => "ϟ",
        Some(ItemType::Pattern) => "▩",
        Some(ItemType::Lottie) => "◍",
        Some(ItemType::Video) => "▶",
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
    let mut menu = use_signal(|| false);
    // Right-click menu: where, and for which layer.
    let mut context = use_signal(|| None::<(f64, f64, String)>);

    let (rows, selected, selection) = match ui.editor.read().as_ref() {
        Some(e) => (
            rows(e.doc(), &collapsed.read()),
            e.selected.clone(),
            e.selection(),
        ),
        None => (Vec::new(), None, Vec::new()),
    };
    let order: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let open = ui.editor.read().is_some();

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
                                    if kind == NewLayer::ImageSlot {
                                        button {
                                            title: "Insert a copy of a saved component",
                                            onclick: move |_| {
                                                menu.set(false);
                                                let mut open = ui.component_library;
                                                open.set(true);
                                            },
                                            "Insert Component…"
                                        }
                                        button {
                                            title: "FRAME's built-in SVG shapes, as a polygon or a tintable SVG",
                                            onclick: move |_| {
                                                menu.set(false);
                                                ui.open_media("shapes", PickTarget::NewLayer);
                                            },
                                            "SVG Shape…"
                                        }
                                    }
                                    if kind == NewLayer::ImageSlot {
                                        button {
                                            title: "An image from the project's media",
                                            onclick: move |_| {
                                                menu.set(false);
                                                ui.open_media("images", PickTarget::NewLayer);
                                            },
                                            "Image…"
                                        }
                                    }
                                    button {
                                        onclick: move |_| {
                                            menu.set(false);
                                            match kind {
                                                NewLayer::Lottie => ui.open_media("lottie", PickTarget::NewLayer),
                                                NewLayer::Video => ui.open_media("videos", PickTarget::NewLayer),
                                                _ => ui.add_layer(kind),
                                            }
                                        },
                                        match kind {
                                            NewLayer::Lottie | NewLayer::Video => format!("{}…", kind.label()),
                                            _ => kind.label().to_string(),
                                        }
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
            if let Some((x, y, id)) = context() {
                // A full-window catcher closes the menu on any click.
                div {
                    class: "context-catcher",
                    onclick: move |_| context.set(None),
                    oncontextmenu: move |evt| {
                        evt.prevent_default();
                        context.set(None);
                    },
                }
                div { class: "menu context-menu", style: "left:{x}px;top:{y}px;",
                    {
                        let a = id.clone();
                        let selection = ui.selection();
                        let many = if selection.len() > 1 && selection.contains(&id) {
                            selection.len()
                        } else {
                            1
                        };
                        rsx! {
                            button {
                                title: if many > 1 { "Save the selected layers (under a new Empty) as one reusable component" } else { "Save this layer with everything under it as a reusable component" },
                                onclick: move |_| {
                                    context.set(None);
                                    crate::components::make_component(ui, &a);
                                },
                                if many > 1 { "Make Component of {many} layers…" } else { "Make Component…" }
                            }
                            button {
                                onclick: move |_| {
                                    context.set(None);
                                    ui.duplicate_selected();
                                },
                                "Duplicate"
                            }
                            button {
                                onclick: move |_| {
                                    context.set(None);
                                    ui.delete_selected();
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
            div { class: "layer-list",
                if rows.is_empty() {
                    p { class: "dim pad", if open { "No layers. Add one with + Add." } else { "Nothing open" } }
                }
                for row in rows {
                    {
                        let is_selected = selection.contains(&row.id);
                        let order = order.clone();
                        let class = format!(
                            "layer-row{}{}",
                            if is_selected { " selected" } else { "" },
                            if row.visible { "" } else { " hidden" },
                        );
                        let is_renaming = renaming.read().as_deref() == Some(row.id.as_str());
                        let Row { id, name, icon, depth, has_children, collapsed: is_collapsed, visible, locked, lock_children } = row;
                        let (id1, id3, id4, id5, id6, id7, id8) = (
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                            id.clone(),
                        );
                        let id_ctx = id.clone();
                        rsx! {
                            // Dragging is done by the bridge (`data-id`); see `on_drop`.
                            div {
                                key: "{id}",
                                class: "{class}",
                                "data-id": "{id}",
                                style: "padding-left:{4 + depth * 14}px;",
                                // Ctrl+click adds or removes; Shift+click
                                // selects the rows from the primary layer.
                                onclick: move |evt| {
                                    let m = evt.modifiers();
                                    if m.ctrl() || m.meta() {
                                        ui.toggle_layer(&id1);
                                    } else if m.shift() {
                                        let anchor = ui.selected();
                                        let at = |id: &str| order.iter().position(|x| x == id);
                                        match anchor.as_deref().and_then(at).zip(at(&id1)) {
                                            Some((a, b)) => {
                                                let (lo, hi) = (a.min(b), a.max(b));
                                                let range: Vec<String> = order[lo..=hi].to_vec();
                                                ui.set_selection(Some(id1.clone()), range);
                                            }
                                            None => ui.select_layer(Some(id1.clone())),
                                        }
                                    } else {
                                        ui.select_layer(Some(id1.clone()));
                                    }
                                },
                                oncontextmenu: move |evt| {
                                    evt.prevent_default();
                                    ui.select_layer(Some(id_ctx.clone()));
                                    let p = evt.client_coordinates();
                                    context.set(Some((p.x, p.y, id_ctx.clone())));
                                },
                                if has_children {
                                    span {
                                        class: "caret",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
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
                                        onclick: move |evt| evt.stop_propagation(),
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
                                        onclick: move |evt| {
                                            evt.stop_propagation();
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
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        ui.execute(&SetLocked { item: id7.clone(), locked: !locked });
                                    },
                                    if locked { "🔒" } else { "🔓" }
                                }
                                button {
                                    class: if visible { "row-toggle" } else { "row-toggle on" },
                                    title: if visible { "Hide" } else { "Show" },
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        ui.execute(&SetVisible { item: id8.clone(), visible: !visible });
                                    },
                                    if visible { "👁" } else { "–" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A row dropped on another row (`{type: "outliner-drop", id, target,
/// zone: "above"|"into"|"below"}` from the bridge): moves the layer.
pub fn on_drop(ui: Ui, ev: &serde_json::Value) {
    let (Some(id), Some(target)) = (ev["id"].as_str(), ev["target"].as_str()) else {
        return;
    };
    let zone = match ev["zone"].as_str() {
        Some("above") => Zone::Above,
        Some("below") => Zone::Below,
        Some("into") => Zone::Into,
        _ => return,
    };
    let time = *ui.time.peek();
    let cmd = ui
        .editor
        .peek()
        .as_ref()
        .and_then(|e| drop_move(e.doc(), id, target, zone, time));
    if let Some(cmd) = cmd {
        ui.execute(&cmd);
        ui.select_layer(Some(id.to_string()));
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
