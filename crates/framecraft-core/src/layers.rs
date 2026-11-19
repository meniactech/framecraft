// SPDX-License-Identifier: AGPL-3.0-or-later
//! New layers, with FRAME's defaults, and fresh ids.

use crate::model::Item;
use serde_json::{Value, json};

/// A random-looking v4-style UUID. Not cryptographic: ids only need to be
/// unique within a project.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mix = |mut h: u64| {
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^ (h >> 31)
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let a = mix(nanos
        ^ u64::from(std::process::id()).rotate_left(32)
        ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let b = mix(a ^ n);
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0x0fff,
        ((b >> 48) & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

/// Layer types the Add menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewLayer {
    Empty,
    Text,
    Box,
    Ellipse,
    Circle,
    Stadium,
}

impl NewLayer {
    pub const ALL: [NewLayer; 6] = [
        NewLayer::Empty,
        NewLayer::Text,
        NewLayer::Box,
        NewLayer::Ellipse,
        NewLayer::Circle,
        NewLayer::Stadium,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NewLayer::Empty => "Empty",
            NewLayer::Text => "Text",
            NewLayer::Box => "Box",
            NewLayer::Ellipse => "Ellipse",
            NewLayer::Circle => "Circle",
            NewLayer::Stadium => "Stadium",
        }
    }
}

/// FRAME's base fields for a new layer.
fn base() -> Value {
    json!({
        "parentId": null, "x": 100, "y": 100, "width": 100, "height": 200, "rotation": 0,
        "scaleX": 1, "scaleY": 1, "pivotX": 50, "pivotY": 50, "opacity": 1, "visible": true,
        "backgroundType": "solid", "gradientType": "linear", "gradientAngle": 90,
        "gradientCenter": { "x": 0.5, "y": 0.5 },
        "gradientStart": "rgba(99, 102, 241, 1)", "gradientEnd": "rgba(56, 189, 248, 1)",
        "gradientRamp": [
            { "offset": 0, "color": "rgba(99, 102, 241, 1)" },
            { "offset": 1, "color": "rgba(56, 189, 248, 1)" }
        ],
        "bgImageUrl": "", "bgImageFit": "cover", "bgImagePosition": "center", "bgImageRepeat": "no-repeat",
        "bg_color": "#64748b", "borderWidth": 1, "borderWidthMode": "single", "borderWidths": [1, 1, 1, 1],
        "borderColor": "#000000", "borderRadius": [0, 0, 0, 0],
        "shadow": { "color": "rgba(0, 0, 0, 0)", "x": 0, "y": 0, "blur": 0 }
    })
}

/// A new layer of `kind` with FRAME's defaults, under `parent`.
pub fn new_layer(kind: NewLayer, parent: Option<&str>) -> Item {
    let mut v = base();
    let set = |v: &mut Value, extra: Value| {
        for (k, val) in extra.as_object().expect("object") {
            v[k] = val.clone();
        }
    };
    let no_border = json!({ "borderWidth": 0, "borderWidths": [0, 0, 0, 0] });
    match kind {
        NewLayer::Empty => {
            set(
                &mut v,
                json!({ "itemType": "empty", "name": "Empty", "width": 48, "height": 48,
                "bg_color": "#00000000", "borderColor": "#00000000", "lockChildren": true }),
            );
            set(&mut v, no_border);
        }
        NewLayer::Text => {
            set(
                &mut v,
                json!({ "itemType": "text", "name": "New Text", "textContent": "New Text",
                "textVariableId": "", "fontFamily": "Mouse Memoirs", "fontSize": 42, "fontWeight": 400,
                "textColor": "#f8fafc", "textAlign": "left", "width": 300, "height": 80, "bg_color": "#00000000" }),
            );
            set(&mut v, no_border);
        }
        NewLayer::Box => set(
            &mut v,
            json!({ "itemType": "shape", "shapeType": "box", "name": "New Shape" }),
        ),
        NewLayer::Ellipse => set(
            &mut v,
            json!({ "itemType": "shape", "shapeType": "ellipse", "name": "New Ellipse", "width": 180, "height": 120 }),
        ),
        NewLayer::Circle => set(
            &mut v,
            json!({ "itemType": "shape", "shapeType": "circle", "name": "New Circle", "width": 140, "height": 140 }),
        ),
        NewLayer::Stadium => set(
            &mut v,
            json!({ "itemType": "shape", "shapeType": "stadium", "name": "New Stadium", "width": 180, "height": 120,
                "borderRadius": [9999, 9999, 9999, 9999] }),
        ),
    }
    v["id"] = new_id().into();
    v["parentId"] = parent.map_or(Value::Null, Value::from);
    serde_json::from_value(v).expect("layer defaults match the model")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_look_like_uuids_and_differ() {
        let a = new_id();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!("89ab".contains(&a[19..20]));
        assert_ne!(a, new_id());
    }

    #[test]
    fn every_new_layer_is_valid() {
        for kind in NewLayer::ALL {
            let item = new_layer(kind, Some("p"));
            assert_eq!(item.parent_id, Some(Some("p".into())));
            let mut unknown = Vec::new();
            item.collect_unknown("", &mut unknown);
            assert!(unknown.is_empty(), "{kind:?}: {unknown:?}");
        }
    }
}
