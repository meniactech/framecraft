// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which layer a click in the viewport selects.

use crate::normalize::Document;

/// The layer a click selects, from the layers under the pointer (topmost
/// first, as the page reports them).
///
/// - A layer inside a parent with "lock children" selects that parent
///   (the outermost one, when they nest), as in FRAME.
/// - With `cycle` (Alt-click), the layer below the current selection is
///   picked, wrapping around, so stacked layers can be reached.
/// - Nothing under the pointer selects nothing.
pub fn pick(doc: &Document, hits: &[String], current: Option<&str>, cycle: bool) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    for id in hits {
        let resolved = locked_group(doc, id).unwrap_or_else(|| id.clone());
        if !candidates.contains(&resolved) && doc.items.iter().any(|i| i.id == resolved) {
            candidates.push(resolved);
        }
    }
    let first = candidates.first()?.clone();
    if !cycle {
        return Some(first);
    }
    match current.and_then(|c| candidates.iter().position(|id| id == c)) {
        Some(i) => Some(candidates[(i + 1) % candidates.len()].clone()),
        None => Some(first),
    }
}

/// The outermost ancestor of `id` that locks its children.
fn locked_group(doc: &Document, id: &str) -> Option<String> {
    let mut found = None;
    let mut parent = doc
        .items
        .iter()
        .find(|i| i.id == id)?
        .parent_id
        .clone()
        .flatten();
    let mut guard = 0;
    while let Some(pid) = parent {
        let Some(p) = doc.items.iter().find(|i| i.id == pid) else {
            break;
        };
        if p.lock_children == Some(true) {
            found = Some(p.id.clone());
        }
        parent = p.parent_id.clone().flatten();
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    found
}

/// Whether a layer may be moved in the viewport (locked layers can be
/// selected but not moved).
pub fn movable(doc: &Document, id: &str) -> bool {
    doc.items
        .iter()
        .find(|i| i.id == id)
        .is_some_and(|i| i.locked != Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Graphic;
    use crate::normalize::normalize;
    use crate::profile::Profile;
    use serde_json::json;

    fn doc() -> Document {
        let item = |id: &str, parent: Option<&str>, lock_children: bool, locked: bool| {
            json!({
                "id": id, "name": id, "parentId": parent, "itemType": "shape", "lockChildren": lock_children, "locked": locked,
                "x": 0, "y": 0, "width": 10, "height": 10, "rotation": 0, "opacity": 1, "bg_color": "#fff",
                "borderWidth": 0, "borderColor": "#000", "borderRadius": [0, 0, 0, 0],
                "shadow": { "color": "#000", "x": 0, "y": 0, "blur": 0 }
            })
        };
        let g: Graphic = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": [
                item("group", None, true, false),
                item("inner", Some("group"), true, false),
                item("leaf", Some("inner"), false, false),
                item("free", None, false, true),
                item("under", None, false, false)
            ]
        }))
        .unwrap();
        normalize(&g, &Profile::framecraft())
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn locked_children_select_the_outermost_group() {
        let d = doc();
        assert_eq!(
            pick(&d, &ids(&["leaf", "under"]), None, false).as_deref(),
            Some("group")
        );
    }

    #[test]
    fn alt_click_cycles_down_the_stack() {
        let d = doc();
        let hits = ids(&["free", "leaf", "under"]);
        assert_eq!(pick(&d, &hits, None, true).as_deref(), Some("free"));
        assert_eq!(
            pick(&d, &hits, Some("free"), true).as_deref(),
            Some("group")
        );
        assert_eq!(
            pick(&d, &hits, Some("group"), true).as_deref(),
            Some("under")
        );
        assert_eq!(
            pick(&d, &hits, Some("under"), true).as_deref(),
            Some("free")
        );
    }

    #[test]
    fn empty_clicks_and_locks() {
        let d = doc();
        assert_eq!(pick(&d, &[], Some("free"), false), None);
        assert_eq!(pick(&d, &ids(&["gone"]), None, false), None);
        assert!(!movable(&d, "free"));
        assert!(movable(&d, "under"));
    }
}
