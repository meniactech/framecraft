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
    ImageSlot,
    BarChart,
    PieChart,
    LineChart,
    Qr,
    Particles,
    Lightning,
    Pattern,
    Lottie,
    Video,
}

impl NewLayer {
    pub const ALL: [NewLayer; 16] = [
        NewLayer::Empty,
        NewLayer::Text,
        NewLayer::Box,
        NewLayer::Ellipse,
        NewLayer::Circle,
        NewLayer::Stadium,
        NewLayer::ImageSlot,
        NewLayer::BarChart,
        NewLayer::PieChart,
        NewLayer::LineChart,
        NewLayer::Qr,
        NewLayer::Particles,
        NewLayer::Lightning,
        NewLayer::Pattern,
        NewLayer::Lottie,
        NewLayer::Video,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NewLayer::Empty => "Empty",
            NewLayer::Text => "Text",
            NewLayer::Box => "Box",
            NewLayer::Ellipse => "Ellipse",
            NewLayer::Circle => "Circle",
            NewLayer::Stadium => "Stadium",
            NewLayer::ImageSlot => "Image Slot",
            NewLayer::BarChart => "Bar Chart",
            NewLayer::PieChart => "Pie Chart",
            NewLayer::LineChart => "Line Chart",
            NewLayer::Qr => "QR Code",
            NewLayer::Particles => "Particles",
            NewLayer::Lightning => "Lightning",
            NewLayer::Pattern => "Pattern",
            NewLayer::Lottie => "Lottie",
            NewLayer::Video => "Video (experimental)",
        }
    }
}

/// A random seed for particles and lightning (1 to 2³² − 1, as FRAME).
pub fn random_seed() -> u32 {
    u32::from_str_radix(&new_id()[..8], 16).unwrap_or(1).max(1)
}

/// A chart data point id, like FRAME's (`cp_` and 8 characters).
pub fn new_chart_point_id() -> String {
    format!("cp_{}", &new_id().replace('-', "")[..8])
}

/// FRAME's chart series colors, used for points without their own.
pub const DEFAULT_CHART_COLORS: [&str; 8] = [
    "#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#a855f7", "#06b6d4", "#f97316", "#84cc16",
];

fn default_chart_points() -> Value {
    let p = |label: &str, value: f64| json!({ "id": new_chart_point_id(), "label": label, "value": value });
    json!([p("A", 40.0), p("B", 25.0), p("C", 55.0), p("D", 30.0)])
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
        NewLayer::ImageSlot => set(
            &mut v,
            json!({ "itemType": "imageSlot", "name": "Image Slot", "imageVariableId": "f_image",
                "imageAssetFolder": "./images/", "imageExtension": "png", "imageDefaultValue": "",
                "imageFit": "cover", "width": 320, "height": 180, "bg_color": "#00000000",
                "borderWidth": 0, "borderWidths": [0, 0, 0, 0] }),
        ),
        NewLayer::BarChart | NewLayer::PieChart | NewLayer::LineChart => {
            let (kind, name) = match kind {
                NewLayer::PieChart => ("pie", "Pie Chart"),
                NewLayer::LineChart => ("line", "Line Chart"),
                _ => ("bar", "Bar Chart"),
            };
            set(
                &mut v,
                json!({ "itemType": "chart", "chartKind": kind, "name": name, "chartPoints": default_chart_points(),
                    "width": 480, "height": 320, "bg_color": "#00000000", "borderWidth": 0,
                    "borderWidths": [0, 0, 0, 0], "borderRadius": [8, 8, 8, 8],
                    "fontFamily": "Mouse Memoirs", "fontSize": 14, "textColor": "#e2e8f0" }),
            );
        }
        NewLayer::Qr => set(
            &mut v,
            json!({ "itemType": "qr", "name": "QR Code", "qrContent": "https://meniac.tech", "qrVariableId": "",
                "qrFgColor": "#000000", "qrBgColor": "#ffffff", "qrErrorCorrection": "M", "qrMargin": 2,
                "width": 240, "height": 240, "bg_color": "#00000000", "borderWidth": 0,
                "borderWidths": [0, 0, 0, 0] }),
        ),
        NewLayer::Particles => set(
            &mut v,
            json!({ "itemType": "particles", "name": "Particles", "width": 400, "height": 400,
                "bg_color": "#00000000", "borderWidth": 0, "borderWidths": [0, 0, 0, 0],
                "particleEmitting": true, "particleEmitterX": 200, "particleEmitterY": 200,
                "particleEmitterRadius": 12, "particleEmitterWidth": 80, "particleEmitterHeight": 48,
                "particleEmitterShape": "circle", "particleEmitAngleDeg": -90, "particleSpawnRate": 40,
                "particleMaxCount": 300, "particleLifetimeMs": 1800, "particleSize": 8, "particleSizeEnd": 2,
                "particleSizeRamp": [{ "offset": 0, "value": 1 }, { "offset": 1, "value": 0.25 }],
                "particleSpeed": 80, "particleSpreadDeg": 360, "particleGravityX": 0, "particleGravityY": 120,
                "particleRotationDeg": 0, "particleRotationRandomDeg": 0, "particleSpinDeg": 0,
                "particleAlignToVelocity": false,
                "particleColorStart": "rgba(255, 255, 255, 1)", "particleColorEnd": "rgba(255, 255, 255, 0)",
                "particleColorRamp": [
                    { "offset": 0, "color": "rgba(255, 255, 255, 1)" },
                    { "offset": 1, "color": "rgba(255, 255, 255, 0)" }
                ],
                "particleColorMode": "ramp", "particlePaletteColors": [],
                "particleAlphaRamp": [{ "offset": 0, "value": 1 }, { "offset": 1, "value": 1 }],
                "particleSpriteUrl": "", "particleBlend": "additive", "particleAttach": "absolute",
                "particleSeed": random_seed() }),
        ),
        NewLayer::Lightning => set(
            &mut v,
            json!({ "itemType": "lightning", "name": "Lightning", "width": 480, "height": 270,
                "bg_color": "#00000000", "borderWidth": 0, "borderWidths": [0, 0, 0, 0],
                "lightningEmitting": true, "lightningAx": 24, "lightningAy": 135, "lightningBx": 456,
                "lightningBy": 135, "lightningSegments": 14, "lightningAmplitude": 18, "lightningWidth": 2,
                "lightningGlowWidth": 10, "lightningColor": "#e0e7ff", "lightningGlowColor": "#6366f1",
                "lightningBranches": 2, "lightningSeed": random_seed(), "lightningFlickerMs": 48 }),
        ),
        NewLayer::Pattern => set(
            &mut v,
            json!({ "itemType": "pattern", "name": "Pattern", "width": 480, "height": 270,
                "bg_color": "#00000000", "borderWidth": 0, "borderWidths": [0, 0, 0, 0],
                "patternKind": "checker", "patternColorA": "#1e293b", "patternColorB": "#334155",
                "patternCellSize": 32, "patternStripeWidth": 16, "patternStripeGap": 16,
                "patternStripeAngleDeg": 45, "patternDotRadius": 6, "patternLineWidth": 2, "patternPhase": 0 }),
        ),
        NewLayer::Lottie => set(
            &mut v,
            json!({ "itemType": "lottie", "name": "Lottie", "width": 400, "height": 400,
                "bg_color": "#00000000", "borderWidth": 0, "borderWidths": [0, 0, 0, 0],
                "mediaUrl": "", "lottieStartMs": 0, "lottieLoop": false }),
        ),
        NewLayer::Video => set(
            &mut v,
            json!({ "itemType": "video", "name": "Video (experimental)", "width": 480, "height": 270,
                "bg_color": "#00000000", "borderWidth": 0, "borderWidths": [0, 0, 0, 0],
                "mediaUrl": "", "videoStartMs": 0, "videoLoop": false }),
        ),
    }
    v["id"] = new_id().into();
    v["parentId"] = parent.map_or(Value::Null, Value::from);
    serde_json::from_value(v).expect("layer defaults match the model")
}

/// A layer showing a media file (FRAME's Add → Image, Lottie and Video
/// from the library): an image at its pixel size, or a Lottie / video clip
/// named after the file.
pub fn new_media_layer(
    category: &str,
    url: &str,
    name: &str,
    size: Option<(f64, f64)>,
    parent: Option<&str>,
) -> Item {
    let kind = match category {
        "lottie" => NewLayer::Lottie,
        "videos" => NewLayer::Video,
        _ => {
            let mut v = base();
            let (w, h) = size.unwrap_or((256.0, 256.0));
            for (k, val) in json!({ "itemType": "image", "mediaUrl": url,
                "name": if name.is_empty() { "Image Item" } else { name },
                "width": w, "height": h, "bg_color": "#00000000" })
            .as_object()
            .expect("object")
            {
                v[k] = val.clone();
            }
            v["id"] = new_id().into();
            v["parentId"] = parent.map_or(Value::Null, Value::from);
            return serde_json::from_value(v).expect("layer defaults match the model");
        }
    };
    let mut item = new_layer(kind, parent);
    item.media_url = Some(url.to_string());
    if !name.is_empty() {
        item.name = if kind == NewLayer::Video {
            format!("{name} (experimental)")
        } else {
            name.to_string()
        };
    }
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_layers() {
        let img = new_media_layer(
            "images",
            "/media/images/a.png",
            "a.png",
            Some((640.0, 360.0)),
            None,
        );
        assert_eq!(img.item_type, Some(crate::model::ItemType::Image));
        assert_eq!((img.width, img.height), (640.0, 360.0));
        let mut unknown = Vec::new();
        img.collect_unknown("", &mut unknown);
        assert!(unknown.is_empty(), "{unknown:?}");
        let clip = new_media_layer("videos", "/media/videos/b.mp4", "b.mp4", None, None);
        assert_eq!(clip.name, "b.mp4 (experimental)");
        assert_eq!(clip.media_url.as_deref(), Some("/media/videos/b.mp4"));
    }

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
