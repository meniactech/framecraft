// SPDX-License-Identifier: AGPL-3.0-or-later
//! Components, as FRAME's `componentLibrary.ts`: the selected layers with
//! everything under them and their keyframes, saved as a reusable piece
//! and inserted into any graphic as a copy (new ids, no link back). FRAME's
//! JSON format (`{version, items, itemTracks}`) is kept.

use crate::commands::{Change, Command, CommandError, subtree};
use crate::model::{Item, ItemTrack, Modifier};
use crate::normalize::Document;
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

/// FRAME: inserted components land 40 px right and down of where they were.
const INSERT_OFFSET: f64 = 40.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentData {
    pub version: u32,
    pub items: Vec<Item>,
    #[serde(default)]
    pub item_tracks: Vec<ItemTrack>,
}

/// Selected layers that are not inside another selected layer.
fn roots(doc: &Document, selected: &[String]) -> Vec<String> {
    selected
        .iter()
        .filter(|id| doc.items.iter().any(|i| &i.id == *id))
        .filter(|id| {
            !selected
                .iter()
                .any(|other| other != *id && subtree(doc, other).contains(id))
        })
        .cloned()
        .collect()
}

/// The selection (with everything under it) as a component, and its
/// suggested name (FRAME's `serializeSelectionAsComponent`). Roots move so
/// their top-left is at 0,0; several roots go under a new Empty.
pub fn from_selection(doc: &Document, selected: &[String]) -> Option<(String, ComponentData)> {
    let roots = roots(doc, selected);
    if roots.is_empty() {
        return None;
    }
    let included: Vec<String> = roots.iter().flat_map(|r| subtree(doc, r)).collect();
    let mut items: Vec<Item> = doc
        .items
        .iter()
        .filter(|i| included.contains(&i.id))
        .cloned()
        .collect();
    for item in &mut items {
        let parent_inside = item
            .parent_id
            .clone()
            .flatten()
            .is_some_and(|p| included.contains(&p));
        if roots.contains(&item.id) || !parent_inside {
            item.parent_id = Some(None);
        }
    }
    let root_items = || items.iter().filter(|i| roots.contains(&i.id));
    let min_x = root_items().map(|i| i.x).fold(f64::INFINITY, f64::min);
    let min_y = root_items().map(|i| i.y).fold(f64::INFINITY, f64::min);
    let (min_x, min_y) = (
        if min_x.is_finite() { min_x } else { 0.0 },
        if min_y.is_finite() { min_y } else { 0.0 },
    );
    for item in items.iter_mut().filter(|i| roots.contains(&i.id)) {
        item.x -= min_x;
        item.y -= min_y;
    }
    let name = match roots.as_slice() {
        [one] => items
            .iter()
            .find(|i| &i.id == one)
            .map(|i| i.name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or("Component")
            .to_string(),
        many => format!("Component ({} items)", many.len()),
    };
    if roots.len() > 1 {
        let mut empty = crate::layers::new_layer(crate::layers::NewLayer::Empty, None);
        empty.name = name.clone();
        empty.x = 0.0;
        empty.y = 0.0;
        for item in items.iter_mut().filter(|i| roots.contains(&i.id)) {
            item.parent_id = Some(Some(empty.id.clone()));
        }
        items.insert(0, empty);
    }
    let item_tracks = doc
        .timeline
        .item_tracks
        .iter()
        .filter(|t| included.contains(&t.item_id))
        .cloned()
        .collect();
    Some((
        name,
        ComponentData {
            version: VERSION,
            items,
            item_tracks,
        },
    ))
}

/// Gives the component's top layer `name` (FRAME does this on save when
/// there is one root).
pub fn rename_root(data: &mut ComponentData, name: &str) {
    let ids: Vec<String> = data.items.iter().map(|i| i.id.clone()).collect();
    let mut roots = data.items.iter_mut().filter(|i| {
        i.parent_id
            .clone()
            .flatten()
            .is_none_or(|p| !ids.contains(&p))
    });
    if let (Some(root), None) = (roots.next(), roots.next()) {
        root.name = name.to_string();
    }
}

/// Inserts a component as a copy (FRAME's `insertComponentIntoScene`): new
/// ids for layers, modifiers and chart points; masks and path follows
/// inside it point at the copies; the top layers 40 px right and down.
/// Everything is made in [`InsertComponent::new`], so redo repeats it.
pub struct InsertComponent {
    pub name: String,
    items: Vec<Item>,
    tracks: Vec<ItemTrack>,
    /// The inserted top layers (to select them).
    pub roots: Vec<String>,
}

impl InsertComponent {
    pub fn new(name: &str, data: &ComponentData) -> Self {
        let new_id = crate::layers::new_id;
        let map: Vec<(String, String)> = data
            .items
            .iter()
            .map(|i| (i.id.clone(), new_id()))
            .collect();
        let lookup = |id: &str| map.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone());
        let mut roots = Vec::new();
        let items = data
            .items
            .iter()
            .map(|item| {
                let mut c = item.clone();
                c.id = lookup(&item.id).expect("mapped");
                match item.parent_id.clone().flatten().and_then(|p| lookup(&p)) {
                    Some(p) => c.parent_id = Some(Some(p)),
                    None => {
                        c.parent_id = Some(None);
                        c.x += INSERT_OFFSET;
                        c.y += INSERT_OFFSET;
                        roots.push(c.id.clone());
                    }
                }
                for m in c.modifiers.iter_mut().flatten() {
                    match m {
                        Modifier::Mask(mask) => {
                            mask.id = new_id();
                            if let Some(n) = mask.source_element_id.as_deref().and_then(lookup) {
                                mask.source_element_id = Some(n);
                            }
                        }
                        Modifier::PathFollow(pf) => {
                            pf.id = new_id();
                            if let Some(n) = lookup(&pf.source_element_id) {
                                pf.source_element_id = n;
                            }
                        }
                        Modifier::Blur(x) => x.id = new_id(),
                        Modifier::Color(x) => x.id = new_id(),
                        Modifier::Shadow(x) => x.id = new_id(),
                        Modifier::Skew(x) => x.id = new_id(),
                        Modifier::Crop(x) => x.id = new_id(),
                        Modifier::Typewriter(x) => x.id = new_id(),
                        Modifier::TextStagger(x) => x.id = new_id(),
                        Modifier::TextScramble(x) => x.id = new_id(),
                        Modifier::TextGlyphColor(x) => x.id = new_id(),
                        Modifier::Other(_) => {}
                    }
                }
                for p in c.chart_points.iter_mut().flatten() {
                    p.id = crate::layers::new_chart_point_id();
                }
                c
            })
            .collect();
        let tracks = data
            .item_tracks
            .iter()
            .filter_map(|t| {
                lookup(&t.item_id).map(|n| ItemTrack {
                    item_id: n,
                    ..t.clone()
                })
            })
            .collect();
        Self {
            name: name.to_string(),
            items,
            tracks,
            roots,
        }
    }
}

impl Command for InsertComponent {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        if self.items.is_empty() {
            return Err(CommandError::InvalidValue("Nothing to insert".into()));
        }
        doc.items.extend(self.items.iter().cloned());
        doc.timeline.item_tracks.extend(self.tracks.iter().cloned());
        Ok(Change::Rebuild)
    }
    fn label(&self, _doc: &Document) -> String {
        format!("Insert {}", self.name)
    }
}

/// A saved component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredComponent {
    pub id: String,
    pub name: String,
    pub data: ComponentData,
}

impl StoredComponent {
    pub fn item_count(&self) -> usize {
        self.data.items.len()
    }
}

/// The user's components (FrameCraft's data folder).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserComponents {
    pub components: Vec<StoredComponent>,
}

impl UserComponents {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("serializes"))?;
        std::fs::rename(tmp, path)
    }

    /// Adds a component; returns its id.
    pub fn add(&mut self, name: &str, data: ComponentData) -> String {
        let id = crate::layers::new_id();
        self.components.push(StoredComponent {
            id: id.clone(),
            name: name.to_string(),
            data,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{History, SetKeyframe};
    use crate::layers::{NewLayer, new_layer};

    fn doc() -> Document {
        let g = serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut d = crate::normalize::normalize(&g, &crate::profile::Profile::framecraft());
        let mut group = new_layer(NewLayer::Empty, None);
        group.id = "g".into();
        group.name = "Lower third".into();
        group.x = 300.0;
        group.y = 200.0;
        let mut bar = new_layer(NewLayer::Box, Some("g"));
        bar.id = "bar".into();
        let mut text = new_layer(NewLayer::Text, Some("g"));
        text.id = "txt".into();
        // The text is masked by the bar.
        text.modifiers = Some(vec![
            serde_json::from_value(serde_json::json!({
                "id": "m", "type": "mask", "enabled": true, "mode": "alpha", "shape": "rect",
                "sourceElementId": "bar", "feather": 0,
                "transform": { "x": 0, "y": 0, "width": 10, "height": 10, "rotation": 0 }
            }))
            .unwrap(),
        ]);
        let mut other = new_layer(NewLayer::Circle, None);
        other.id = "c".into();
        other.x = 500.0;
        other.y = 100.0;
        d.items.extend([group, bar, text, other]);
        let mut h = History::default();
        h.execute(
            &mut d,
            &SetKeyframe {
                item: "bar".into(),
                lane: "x".into(),
                time_ms: 0.0,
                value: 5.0,
                easing: None,
            },
        )
        .unwrap();
        d
    }

    #[test]
    fn one_root_keeps_its_tree_and_keys() {
        let d = doc();
        let (name, data) = from_selection(&d, &["g".into(), "txt".into()]).unwrap();
        assert_eq!(name, "Lower third");
        assert_eq!(data.items.len(), 3);
        let root = &data.items[0];
        assert_eq!(
            (root.x, root.y, root.parent_id.clone()),
            (0.0, 0.0, Some(None))
        );
        assert_eq!(data.item_tracks.len(), 1);
        // Through JSON, as saved (FRAME's field names).
        let json = serde_json::to_value(&data).unwrap();
        assert!(json.get("itemTracks").is_some());
        let back: ComponentData = serde_json::from_value(json).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn several_roots_go_under_an_empty() {
        let d = doc();
        let (name, data) = from_selection(&d, &["g".into(), "c".into()]).unwrap();
        assert_eq!(name, "Component (2 items)");
        assert_eq!(data.items[0].name, name);
        let empty = data.items[0].id.clone();
        let tops: Vec<(f64, f64)> = data
            .items
            .iter()
            .filter(|i| i.parent_id.clone().flatten().as_deref() == Some(&empty))
            .map(|i| (i.x, i.y))
            .collect();
        // Normalized to the roots' top-left (300, 100).
        assert_eq!(tops, [(0.0, 100.0), (200.0, 0.0)]);
    }

    #[test]
    fn insert_is_a_fresh_copy() {
        let mut d = doc();
        let (_, data) = from_selection(&d, &["g".into()]).unwrap();
        let cmd = InsertComponent::new("Lower third", &data);
        let mut h = History::default();
        h.execute(&mut d, &cmd).unwrap();
        assert_eq!(d.items.len(), 7);
        let root = d.items.iter().find(|i| i.id == cmd.roots[0]).unwrap();
        assert_eq!((root.x, root.y), (40.0, 40.0));
        let copies: Vec<&Item> = d.items[4..].iter().collect();
        assert!(
            copies
                .iter()
                .all(|i| !["g", "bar", "txt"].contains(&i.id.as_str()))
        );
        // The copied mask points at the copied bar, the copied key track too.
        let bar_copy = copies
            .iter()
            .find(|i| i.parent_id.is_some() && i.item_type == d.items[1].item_type)
            .unwrap();
        let mask_src = copies
            .iter()
            .find_map(|i| match i.modifiers.as_ref()?.first()? {
                Modifier::Mask(m) => m.source_element_id.clone(),
                _ => None,
            })
            .unwrap();
        assert_eq!(mask_src, bar_copy.id);
        assert!(
            d.timeline
                .item_tracks
                .iter()
                .any(|t| t.item_id == bar_copy.id)
        );
        // Undo and redo make the same ids.
        h.undo(&mut d).unwrap();
        assert_eq!(d.items.len(), 4);
        h.redo(&mut d).unwrap();
        assert!(d.items.iter().any(|i| i.id == cmd.roots[0]));
    }

    #[test]
    fn library_saves_and_loads() {
        let d = doc();
        let (name, mut data) = from_selection(&d, &["g".into()]).unwrap();
        rename_root(&mut data, "LT");
        assert_eq!(data.items[0].name, "LT");
        let mut lib = UserComponents::default();
        lib.add(&name, data);
        let path =
            std::env::temp_dir().join(format!("fc-components-{}.json", crate::layers::new_id()));
        lib.save(&path).unwrap();
        let back = UserComponents::load(&path);
        assert_eq!(back, lib);
        assert_eq!(back.components[0].item_count(), 3);
        let _ = std::fs::remove_file(path);
    }
}
