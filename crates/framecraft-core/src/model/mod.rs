// SPDX-License-Identifier: AGPL-3.0-or-later
//! The graphic document, as FRAME stores it.
//!
//! A graphic is one JSON object with a scene, a flat list of items (layers,
//! nested through `parentId`), a timeline of keyframes and optional template
//! scripts. These types mirror that format field for field, so FrameCraft
//! reads FRAME graphics and writes files FRAME can read.
//!
//! Conventions:
//! - Field names match the JSON (camelCase, plus `bg_color`).
//! - All numbers are `f64`, as in JavaScript.
//! - Optional JSON fields are `Option` and are left out when `None`.
//! - Every struct keeps fields it does not know in `extra`, so loading and
//!   saving never drops data written by a newer version.
//!
//! This is the stored form. Defaults and legacy fixes that FRAME applies on
//! load (`parseGraphicData`) are a separate step.

mod item;
mod modifier;
mod scene;
mod timeline;

pub use item::*;
pub use modifier::*;
pub use scene::*;
pub use timeline::*;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// Fields a struct does not model, kept as they were read.
pub type Extra = Map<String, Value>;

/// One graphic: what FRAME saves per graphic and puts in a project pack
/// as `graphics/NN-name.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graphic {
    pub scene: Scene,
    pub items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Timeline>,
    /// Classic script source (older graphics; newer ones use `template_functions`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_js: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_functions: Option<Vec<TemplateFunction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_globals: Option<Vec<TemplateGlobal>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Graphic {
    /// Parses a graphic from its JSON text.
    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        serde_json::from_str(text)
    }

    /// Writes the graphic as pretty-printed JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Paths of every field kept in an `extra` map, i.e. fields this model
    /// does not know. Empty when the model covers the whole file.
    pub fn unknown_fields(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_extra(&mut out, "", &self.extra);
        push_extra(&mut out, "scene", &self.scene.extra);
        for (i, item) in self.items.iter().enumerate() {
            item.collect_unknown(&format!("items[{i}]"), &mut out);
        }
        if let Some(t) = &self.timeline {
            t.collect_unknown("timeline", &mut out);
        }
        for (i, f) in self.template_functions.iter().flatten().enumerate() {
            push_extra(&mut out, &format!("templateFunctions[{i}]"), &f.extra);
        }
        for (i, g) in self.template_globals.iter().flatten().enumerate() {
            push_extra(&mut out, &format!("templateGlobals[{i}]"), &g.extra);
        }
        out
    }
}

/// A named script function (Properties → Js).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateFunction {
    pub id: String,
    pub name: String,
    /// Body inside `name(ctx) { … }`.
    pub body: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A script global, compiled to `window.__ionframeVars`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateGlobal {
    pub id: String,
    pub name: String,
    /// `string`, `number` or `boolean`.
    pub value_kind: String,
    /// Raw editor value, parsed by `value_kind` when compiled.
    pub value: String,
    #[serde(flatten)]
    pub extra: Extra,
}

pub(crate) fn push_extra(out: &mut Vec<String>, path: &str, extra: &Extra) {
    for key in extra.keys() {
        out.push(if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        });
    }
}

/// For fields that can be missing, `null` or a value, and must keep that
/// difference when saved again. Use with `#[serde(default)]`.
pub(crate) fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graphic(item_extra: Value) -> Value {
        let mut item = json!({
            "id": "a", "name": "A", "x": 0, "y": 0, "width": 10, "height": 10,
            "rotation": 0, "opacity": 1, "bg_color": "#fff", "borderWidth": 0,
            "borderColor": "#000", "borderRadius": [0, 0, 0, 0],
            "shadow": { "color": "#000", "x": 0, "y": 0, "blur": 0 }
        });
        for (k, v) in item_extra.as_object().unwrap() {
            item[k] = v.clone();
        }
        json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": [item]
        })
    }

    fn round_trip(v: &Value) -> (Graphic, Value) {
        let g: Graphic = serde_json::from_value(v.clone()).unwrap();
        let back = serde_json::to_value(&g).unwrap();
        (g, back)
    }

    #[test]
    fn unknown_fields_are_kept_and_reported() {
        let (g, back) = round_trip(&graphic(json!({ "futureField": { "a": 1 } })));
        assert_eq!(g.unknown_fields(), vec!["items[0].futureField"]);
        assert_eq!(back["items"][0]["futureField"], json!({ "a": 1 }));
    }

    #[test]
    fn parent_id_null_and_missing_stay_apart() {
        let (_, back) = round_trip(&graphic(json!({ "parentId": null })));
        assert_eq!(back["items"][0].get("parentId"), Some(&Value::Null));
        let (_, back) = round_trip(&graphic(json!({})));
        assert_eq!(back["items"][0].get("parentId"), None);
        let (g, _) = round_trip(&graphic(json!({ "parentId": "group" })));
        assert_eq!(g.items[0].parent_id, Some(Some("group".into())));
    }

    #[test]
    fn unknown_modifier_type_is_kept() {
        let m = json!({ "id": "m1", "type": "transform", "enabled": true, "skewX": 5 });
        let (g, back) = round_trip(&graphic(json!({ "modifiers": [m.clone()] })));
        assert!(matches!(
            g.items[0].modifiers.as_ref().unwrap()[0],
            Modifier::Other(_)
        ));
        assert_eq!(
            g.unknown_fields(),
            vec!["items[0].modifiers[0] (unknown modifier type `transform`)"]
        );
        assert_eq!(back["items"][0]["modifiers"][0], m);
    }

    #[test]
    fn known_modifier_is_typed() {
        let m = json!({ "id": "b", "type": "blur", "enabled": true, "radius": 4 });
        let (g, _) = round_trip(&graphic(json!({ "modifiers": [m] })));
        match &g.items[0].modifiers.as_ref().unwrap()[0] {
            Modifier::Blur(b) => assert_eq!(b.radius, 4.0),
            other => panic!("expected blur, got {other:?}"),
        }
        assert!(g.unknown_fields().is_empty());
    }

    #[test]
    fn spx_out_number_or_manual() {
        let mut v = graphic(json!({}));
        v["scene"]["spxOut"] = json!("manual");
        let (g, _) = round_trip(&v);
        assert_eq!(g.scene.spx_out, Some(SpxOut::Text("manual".into())));
        v["scene"]["spxOut"] = json!(5000);
        let (g, _) = round_trip(&v);
        assert_eq!(g.scene.spx_out, Some(SpxOut::Millis(5000.0)));
    }
}
