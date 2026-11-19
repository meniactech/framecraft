// SPDX-License-Identifier: AGPL-3.0-or-later
//! The modifier stack (Properties → Modifiers), as FRAME's: which modifiers
//! a layer can have, their defaults and fields, and the commands that add,
//! remove and reorder them. Field edits go through [`SetField`] with
//! [`FieldTarget::Modifier`], or [`SetValueAt`] for animated fields.
//!
//! [`SetField`]: crate::commands::SetField
//! [`SetValueAt`]: crate::commands::SetValueAt

use crate::commands::{Change, Command, CommandError, FieldTarget, Update, item_mut, item_name};
use crate::model::Modifier;
use crate::model::{
    BlurModifier, ColorModifier, CropModifier, MaskModifier, MaskTransform, PathFollowModifier,
    ShadowModifier, SkewModifier, TextGlyphColorModifier, TextScrambleModifier,
    TextStaggerModifier, TypewriterModifier,
};
use crate::model::{Item, ItemType, ShapeType};
use crate::normalize::Document;
use crate::schema::{Kind, Property, Scope, Tab};
use serde_json::Value;

/// Modifier types in FRAME's menu order: `(type, label)`.
pub const KINDS: &[(&str, &str)] = &[
    ("mask", "Mask"),
    ("blur", "Blur"),
    ("color", "Color"),
    ("shadow", "Shadow"),
    ("skew", "Skew"),
    ("crop", "Crop"),
    ("typewriter", "Typewriter"),
    ("textStagger", "Text Stagger"),
    ("textScramble", "Text Scramble"),
    ("textGlyphColor", "Glyph Color"),
    ("pathFollow", "Path Follow"),
];

const TEXT_ONLY: [&str; 4] = [
    "typewriter",
    "textStagger",
    "textScramble",
    "textGlyphColor",
];

pub fn label(kind: &str) -> &str {
    KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(kind, |(_, l)| l)
}

/// The modifier's `type`.
pub fn kind(m: &Modifier) -> &str {
    match m {
        Modifier::Mask(_) => "mask",
        Modifier::Blur(_) => "blur",
        Modifier::Color(_) => "color",
        Modifier::Shadow(_) => "shadow",
        Modifier::Skew(_) => "skew",
        Modifier::Crop(_) => "crop",
        Modifier::Typewriter(_) => "typewriter",
        Modifier::PathFollow(_) => "pathFollow",
        Modifier::TextStagger(_) => "textStagger",
        Modifier::TextScramble(_) => "textScramble",
        Modifier::TextGlyphColor(_) => "textGlyphColor",
        Modifier::Other(v) => v.get("type").and_then(Value::as_str).unwrap_or("?"),
    }
}

pub fn id(m: &Modifier) -> &str {
    match m {
        Modifier::Mask(m) => &m.id,
        Modifier::Blur(m) => &m.id,
        Modifier::Color(m) => &m.id,
        Modifier::Shadow(m) => &m.id,
        Modifier::Skew(m) => &m.id,
        Modifier::Crop(m) => &m.id,
        Modifier::Typewriter(m) => &m.id,
        Modifier::PathFollow(m) => &m.id,
        Modifier::TextStagger(m) => &m.id,
        Modifier::TextScramble(m) => &m.id,
        Modifier::TextGlyphColor(m) => &m.id,
        Modifier::Other(v) => v.get("id").and_then(Value::as_str).unwrap_or(""),
    }
}

pub fn enabled(m: &Modifier) -> bool {
    match m {
        Modifier::Mask(m) => m.enabled,
        Modifier::Blur(m) => m.enabled,
        Modifier::Color(m) => m.enabled,
        Modifier::Shadow(m) => m.enabled,
        Modifier::Skew(m) => m.enabled,
        Modifier::Crop(m) => m.enabled,
        Modifier::Typewriter(m) => m.enabled,
        Modifier::PathFollow(m) => m.enabled,
        Modifier::TextStagger(m) => m.enabled,
        Modifier::TextScramble(m) => m.enabled,
        Modifier::TextGlyphColor(m) => m.enabled,
        Modifier::Other(v) => v.get("enabled").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// FRAME's info text for a modifier.
pub fn help(kind: &str, item: &Item) -> &'static str {
    match kind {
        "typewriter" => {
            "0% = no characters, 100% = full text. Animate this channel for a type-on reveal."
        }
        "textStagger" => {
            "Animate Progress. Amount = cascade stagger. Spread / Amplitude / Speed tune tracking, stretch, wave, and nervous."
        }
        "textScramble" => {
            "Decode letters from scramble junk. Seek-stable (seeded). Animate Progress 0→100."
        }
        "textGlyphColor" => {
            "Per-letter color wave from → to. Animate Progress; Amount controls sequencing."
        }
        "pathFollow" if is_text(item) => {
            "Letters follow the path (SVG textPath). Animate Progress so text enters and exits along the stroke — use an open Path, not a filled polygon."
        }
        "pathFollow" => {
            "Pivot rides the path. Enable Bank to rotate to the tangent. Prefer an open Path for clean motion."
        }
        "crop" => "0% = no crop, 100% = fully cropped. Animate Percent.",
        "mask" => {
            "Clip to another layer, or draw a region in this layer's own box. Feather softens the edge."
        }
        _ => "",
    }
}

fn is_text(item: &Item) -> bool {
    item.item_type == Some(ItemType::Text)
}

/// Modifier types offered for a layer (FRAME's `modifierOptionsForItem`).
pub fn kinds_for(item: &Item) -> Vec<&'static str> {
    KINDS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| match item.item_type {
            Some(ItemType::Text) => true,
            Some(ItemType::Empty) => {
                matches!(
                    *k,
                    "pathFollow" | "skew" | "blur" | "color" | "shadow" | "crop"
                )
            }
            _ => !TEXT_ONLY.contains(k),
        })
        .collect()
}

/// Timeline lanes that animate a modifier type.
pub fn lanes(kind: &str) -> &'static [&'static str] {
    match kind {
        "blur" => &["modBlurRadius"],
        "color" => &[
            "modColorBrightness",
            "modColorContrast",
            "modColorSaturation",
            "modColorHueRotate",
        ],
        "shadow" => &["modShadowX", "modShadowY", "modShadowBlur"],
        "crop" => &["modCropPercent"],
        "typewriter" => &["modTypewriterPercent"],
        "pathFollow" => &["modPathProgress"],
        "textStagger" => &["modTextStaggerProgress"],
        "textScramble" => &["modTextScrambleProgress"],
        "textGlyphColor" => &["modTextGlyphColorProgress"],
        "skew" => &["modSkewX", "modSkewY"],
        _ => &[],
    }
}

/// Path and polygon layers a path follow can ride (FRAME: at least 2 points).
pub fn path_sources<'a>(doc: &'a Document, item: &str) -> Vec<&'a Item> {
    doc.items
        .iter()
        .filter(|i| {
            i.id != item
                && i.item_type == Some(ItemType::Shape)
                && matches!(i.shape_type, Some(ShapeType::Path | ShapeType::Polygon))
                && i.polygon_points.as_ref().is_some_and(|p| p.len() >= 2)
        })
        .collect()
}

/// A new modifier with FRAME's defaults. Path follow on text runs the
/// letters along the path; it picks the first path (then polygon) layer.
pub fn new_modifier(doc: &Document, item: &Item, kind: &str) -> Option<Modifier> {
    let id = crate::layers::new_id();
    let extra = Default::default;
    Some(match kind {
        "mask" => Modifier::Mask(Box::new(MaskModifier {
            id,
            enabled: true,
            mode: "alpha".into(),
            shape: "rect".into(),
            source_element_id: Some(String::new()),
            feather: 0.0,
            transform: MaskTransform {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 120.0,
                rotation: 0.0,
                extra: extra(),
            },
            rounded_rect_radius: None,
            polygon_points: None,
            text_content: None,
            text_font_family: None,
            text_font_size: None,
            image_url: None,
            extra: extra(),
        })),
        "blur" => Modifier::Blur(BlurModifier {
            id,
            enabled: true,
            radius: 10.0,
            extra: extra(),
        }),
        "color" => Modifier::Color(ColorModifier {
            id,
            enabled: true,
            brightness: 100.0,
            contrast: 100.0,
            saturation: 100.0,
            hue_rotate: 0.0,
            extra: extra(),
        }),
        "shadow" => Modifier::Shadow(ShadowModifier {
            id,
            enabled: true,
            x: 8.0,
            y: 8.0,
            blur: 12.0,
            color: "rgba(0, 0, 0, 0.5)".into(),
            extra: extra(),
        }),
        "skew" => Modifier::Skew(SkewModifier {
            id,
            enabled: true,
            skew_x: 0.0,
            skew_y: 0.0,
            extra: extra(),
        }),
        "crop" => Modifier::Crop(CropModifier {
            id,
            enabled: true,
            direction: "left".into(),
            percent: 0.0,
            extra: extra(),
        }),
        "typewriter" => Modifier::Typewriter(TypewriterModifier {
            id,
            enabled: true,
            percent: 100.0,
            extra: extra(),
        }),
        "pathFollow" => {
            let sources = path_sources(doc, &item.id);
            let source = sources
                .iter()
                .find(|i| i.shape_type == Some(ShapeType::Path))
                .or_else(|| sources.first())
                .map(|i| i.id.clone())
                .unwrap_or_default();
            Modifier::PathFollow(PathFollowModifier {
                id,
                enabled: true,
                source_element_id: source,
                progress: 0.0,
                mode: if is_text(item) { "textAlong" } else { "follow" }.into(),
                bank: !is_text(item),
                bank_offset_deg: 0.0,
                bank_flip: false,
                closed: false,
                r#loop: false,
                extra: extra(),
            })
        }
        "textStagger" => Modifier::TextStagger(TextStaggerModifier {
            id,
            enabled: true,
            progress: 100.0,
            mode: "in".into(),
            style: "fade".into(),
            amount: 70.0,
            order: "forward".into(),
            spread: Some(55.0),
            amplitude: Some(55.0),
            speed: Some(45.0),
            extra: extra(),
        }),
        "textScramble" => Modifier::TextScramble(TextScrambleModifier {
            id,
            enabled: true,
            progress: 100.0,
            charset: "alnum".into(),
            custom_charset: String::new(),
            order: "forward".into(),
            seed: 1.0,
            extra: extra(),
        }),
        "textGlyphColor" => Modifier::TextGlyphColor(TextGlyphColorModifier {
            id,
            enabled: true,
            progress: 100.0,
            from_color: "#94a3b8".into(),
            to_color: "#f8fafc".into(),
            amount: 70.0,
            order: "forward".into(),
            extra: extra(),
        }),
        _ => return None,
    })
}

/// Sets a modifier lane's own value (the first modifier of that type, as
/// the runtime reads it). `Ok(false)` when `lane` is not a modifier lane.
pub(crate) fn set_lane(item: &mut Item, lane: &str, v: f64) -> Result<bool, CommandError> {
    let Some(kind) = KINDS
        .iter()
        .map(|(k, _)| *k)
        .find(|k| lanes(k).contains(&lane))
    else {
        return Ok(false);
    };
    let m = item
        .modifiers
        .iter_mut()
        .flatten()
        .find(|m| self::kind(m) == kind)
        .ok_or_else(|| CommandError::InvalidValue(format!("{lane}: no {kind} modifier")))?;
    match (m, lane) {
        (Modifier::Blur(b), _) => b.radius = v.max(0.0),
        (Modifier::Color(c), "modColorBrightness") => c.brightness = v,
        (Modifier::Color(c), "modColorContrast") => c.contrast = v,
        (Modifier::Color(c), "modColorSaturation") => c.saturation = v,
        (Modifier::Color(c), _) => c.hue_rotate = v,
        (Modifier::Shadow(s), "modShadowX") => s.x = v,
        (Modifier::Shadow(s), "modShadowY") => s.y = v,
        (Modifier::Shadow(s), _) => s.blur = v.max(0.0),
        (Modifier::Crop(c), _) => c.percent = v.clamp(0.0, 100.0),
        (Modifier::Typewriter(t), _) => t.percent = v.clamp(0.0, 100.0),
        (Modifier::PathFollow(p), _) => p.progress = v,
        (Modifier::TextStagger(t), _) => t.progress = v.clamp(0.0, 100.0),
        (Modifier::TextScramble(t), _) => t.progress = v.clamp(0.0, 100.0),
        (Modifier::TextGlyphColor(t), _) => t.progress = v.clamp(0.0, 100.0),
        (Modifier::Skew(s), "modSkewX") => s.skew_x = v,
        (Modifier::Skew(s), _) => s.skew_y = v,
        _ => unreachable!("lane belongs to the modifier's type"),
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Fields

macro_rules! f {
    ($kind:literal, $path:literal, $label:literal, $field:expr, $lane:expr) => {
        Property {
            tab: Tab::Modifiers,
            group: $kind,
            path: $path,
            label: $label,
            kind: $field,
            scope: Scope::Modifier,
            lane: $lane,
            update: Update::Pose,
        }
    };
}

const fn num(min: f64, max: f64, step: f64) -> Kind {
    Kind::Number { min, max, step }
}
const ANY: f64 = 1e9;
const PCT: Kind = num(0.0, 100.0, 0.1);

const MASK_MODES: &[(&str, &str)] = &[("alpha", "Alpha"), ("luma", "Luma"), ("invert", "Invert")];
const MASK_SHAPES: &[(&str, &str)] = &[
    ("rect", "Rect"),
    ("roundedRect", "Rounded Rect"),
    ("ellipse", "Ellipse"),
    ("polygon", "Polygon"),
    ("text", "Text"),
    ("image", "Image"),
];
const DIRECTIONS: &[(&str, &str)] = &[
    ("up", "Up"),
    ("down", "Down"),
    ("left", "Left"),
    ("right", "Right"),
];
const ORDERS: &[(&str, &str)] = &[
    ("forward", "Left → right"),
    ("reverse", "Right → left"),
    ("center", "Center out"),
];
const STAGGER_MODES: &[(&str, &str)] = &[("in", "In (appear)"), ("out", "Out (disappear)")];
const STAGGER_STYLES: &[(&str, &str)] = &[
    ("fade", "Fade"),
    ("scale", "Scale"),
    ("slideUp", "Slide up"),
    ("slideDown", "Slide down"),
    ("slideLeft", "Slide left"),
    ("slideRight", "Slide right"),
    ("tracking", "Tracking (spread)"),
    ("stretch", "Stretch (spread + scaleX)"),
    ("wave", "Wave (sin/cos)"),
    ("nervous", "Nervous (jitter)"),
];
const CHARSETS: &[(&str, &str)] = &[
    ("alnum", "Letters + digits"),
    ("symbols", "Symbols"),
    ("custom", "Custom"),
];
const PATH_MODES: &[(&str, &str)] = &[("follow", "Object along path")];

/// Every modifier field, grouped by modifier type (`group`), in FRAME's
/// order. Paths are inside the modifier.
pub const FIELDS: &[Property] = &[
    f!("mask", "sourceElementId", "Clip to", Kind::MaskSource, None),
    f!("mask", "feather", "Feather", num(0.0, 500.0, 1.0), None),
    f!("mask", "mode", "Mask mode", Kind::Choice(MASK_MODES), None),
    f!(
        "mask",
        "shape",
        "Mask shape",
        Kind::Choice(MASK_SHAPES),
        None
    ),
    f!("mask", "transform.x", "X", num(-ANY, ANY, 1.0), None),
    f!("mask", "transform.y", "Y", num(-ANY, ANY, 1.0), None),
    f!("mask", "transform.width", "Width", num(1.0, ANY, 1.0), None),
    f!(
        "mask",
        "transform.height",
        "Height",
        num(1.0, ANY, 1.0),
        None
    ),
    f!(
        "mask",
        "transform.rotation",
        "Rotation",
        num(-ANY, ANY, 1.0),
        None
    ),
    f!("mask", "textContent", "Mask text", Kind::Text, None),
    f!(
        "mask",
        "imageUrl",
        "Mask image",
        Kind::Media("images"),
        None
    ),
    f!(
        "mask",
        "polygonPoints",
        "Polygon points",
        Kind::MaskPoints,
        None
    ),
    f!(
        "blur",
        "radius",
        "Blur radius",
        num(0.0, 500.0, 1.0),
        Some("modBlurRadius")
    ),
    f!(
        "color",
        "brightness",
        "Brightness",
        num(0.0, ANY, 1.0),
        Some("modColorBrightness")
    ),
    f!(
        "color",
        "contrast",
        "Contrast",
        num(0.0, ANY, 1.0),
        Some("modColorContrast")
    ),
    f!(
        "color",
        "saturation",
        "Saturation",
        num(0.0, ANY, 1.0),
        Some("modColorSaturation")
    ),
    f!(
        "color",
        "hueRotate",
        "Hue rotate",
        num(-ANY, ANY, 1.0),
        Some("modColorHueRotate")
    ),
    f!(
        "shadow",
        "x",
        "Shadow X",
        num(-ANY, ANY, 1.0),
        Some("modShadowX")
    ),
    f!(
        "shadow",
        "y",
        "Shadow Y",
        num(-ANY, ANY, 1.0),
        Some("modShadowY")
    ),
    f!(
        "shadow",
        "blur",
        "Shadow blur",
        num(0.0, ANY, 1.0),
        Some("modShadowBlur")
    ),
    f!("shadow", "color", "Shadow color", Kind::Color, None),
    f!(
        "skew",
        "skewX",
        "Skew X",
        num(-ANY, ANY, 1.0),
        Some("modSkewX")
    ),
    f!(
        "skew",
        "skewY",
        "Skew Y",
        num(-ANY, ANY, 1.0),
        Some("modSkewY")
    ),
    f!(
        "crop",
        "direction",
        "Direction",
        Kind::Choice(DIRECTIONS),
        None
    ),
    f!("crop", "percent", "Crop %", PCT, Some("modCropPercent")),
    f!(
        "typewriter",
        "percent",
        "Type %",
        PCT,
        Some("modTypewriterPercent")
    ),
    f!(
        "textStagger",
        "progress",
        "Progress",
        PCT,
        Some("modTextStaggerProgress")
    ),
    f!(
        "textStagger",
        "mode",
        "Mode",
        Kind::Choice(STAGGER_MODES),
        None
    ),
    f!(
        "textStagger",
        "style",
        "Style",
        Kind::Choice(STAGGER_STYLES),
        None
    ),
    f!("textStagger", "order", "Order", Kind::Choice(ORDERS), None),
    f!("textStagger", "amount", "Amount (stagger)", PCT, None),
    f!("textStagger", "spread", "Spread", PCT, None),
    f!("textStagger", "amplitude", "Amplitude", PCT, None),
    f!("textStagger", "speed", "Speed", PCT, None),
    f!(
        "textScramble",
        "progress",
        "Progress",
        PCT,
        Some("modTextScrambleProgress")
    ),
    f!(
        "textScramble",
        "charset",
        "Charset",
        Kind::Choice(CHARSETS),
        None
    ),
    f!(
        "textScramble",
        "customCharset",
        "Custom charset",
        Kind::Text,
        None
    ),
    f!("textScramble", "order", "Order", Kind::Choice(ORDERS), None),
    f!("textScramble", "seed", "Seed", Kind::Seed, None),
    f!(
        "textGlyphColor",
        "progress",
        "Progress",
        PCT,
        Some("modTextGlyphColorProgress")
    ),
    f!(
        "textGlyphColor",
        "fromColor",
        "From color",
        Kind::Color,
        None
    ),
    f!("textGlyphColor", "toColor", "To color", Kind::Color, None),
    f!(
        "textGlyphColor",
        "order",
        "Order",
        Kind::Choice(ORDERS),
        None
    ),
    f!("textGlyphColor", "amount", "Amount", PCT, None),
    f!(
        "pathFollow",
        "sourceElementId",
        "Path source",
        Kind::PathSource,
        None
    ),
    f!("pathFollow", "mode", "Mode", Kind::Choice(PATH_MODES), None),
    f!(
        "pathFollow",
        "progress",
        "Progress",
        num(-ANY, ANY, 0.1),
        Some("modPathProgress")
    ),
    f!("pathFollow", "bank", "Bank (tangent)", Kind::Toggle, None),
    f!("pathFollow", "bankFlip", "Flip bank", Kind::Toggle, None),
    f!("pathFollow", "closed", "Closed path", Kind::Toggle, None),
    f!("pathFollow", "loop", "Loop progress", Kind::Toggle, None),
    f!(
        "pathFollow",
        "bankOffsetDeg",
        "Bank offset (°)",
        num(-ANY, ANY, 1.0),
        None
    ),
];

/// What an unset optional modifier number means (FRAME's defaults).
pub fn field_default(kind: &str, path: &str) -> Option<f64> {
    match (kind, path) {
        ("textStagger", "spread" | "amplitude") => Some(55.0),
        ("textStagger", "speed") => Some(45.0),
        _ => None,
    }
}

/// The fields shown for one modifier of `item` (some depend on its
/// settings, as in FRAME).
pub fn fields(doc: &Document, item: &Item, m: &Modifier) -> Vec<&'static Property> {
    let kind = kind(m);
    let json = serde_json::to_value(m).unwrap_or_default();
    let s = |k: &str| {
        json.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    FIELDS
        .iter()
        .filter(|p| p.group == kind)
        .filter(|p| match (kind, p.path) {
            ("mask", "sourceElementId" | "feather") => true,
            ("mask", path) => {
                // The custom region only matters without a source layer.
                s("sourceElementId").trim().is_empty()
                    && match path {
                        "textContent" => s("shape") == "text",
                        "imageUrl" => s("shape") == "image",
                        "polygonPoints" => s("shape") == "polygon",
                        _ => true,
                    }
            }
            ("textStagger", "spread") => matches!(s("style").as_str(), "tracking" | "stretch"),
            ("textStagger", "amplitude") => {
                matches!(s("style").as_str(), "stretch" | "wave" | "nervous")
            }
            ("textStagger", "speed") => matches!(s("style").as_str(), "wave" | "nervous"),
            ("textScramble", "customCharset") => s("charset") == "custom",
            ("pathFollow", "mode" | "bank" | "bankFlip" | "bankOffsetDeg") => !is_text(item),
            ("pathFollow", "closed") => {
                // Open paths stay open (FRAME disables the box for them).
                let source = s("sourceElementId");
                !is_text(item)
                    && !doc
                        .items
                        .iter()
                        .any(|i| i.id == source && i.shape_type == Some(ShapeType::Path))
            }
            _ => true,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Commands

fn modifiers_mut<'a>(
    doc: &'a mut Document,
    item: &str,
) -> Result<&'a mut Vec<Modifier>, CommandError> {
    Ok(item_mut(doc, item)?.modifiers.get_or_insert_with(Vec::new))
}

fn position(list: &[Modifier], modifier: &str) -> Result<usize, CommandError> {
    list.iter()
        .position(|m| id(m) == modifier)
        .ok_or_else(|| CommandError::InvalidValue(format!("no modifier {modifier}")))
}

/// Adds a modifier at the end of the stack. A layer has at most one of each
/// type, and text effects only go on text layers (as in FRAME).
pub struct AddModifier {
    pub item: String,
    pub modifier: Modifier,
}

impl Command for AddModifier {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let kind = kind(&self.modifier);
        let item = item_mut(doc, &self.item)?;
        if !kinds_for(item).contains(&kind) {
            return Err(CommandError::InvalidValue(format!(
                "{} cannot go on this layer",
                label(kind)
            )));
        }
        if item
            .modifiers
            .iter()
            .flatten()
            .any(|m| self::kind(m) == kind)
        {
            return Err(CommandError::InvalidValue(format!(
                "this layer already has a {} modifier",
                label(kind)
            )));
        }
        item.modifiers
            .get_or_insert_with(Vec::new)
            .push(self.modifier.clone());
        Ok(Change::Layer(vec![self.item.clone()]))
    }
    fn label(&self, doc: &Document) -> String {
        format!(
            "Add {} to {}",
            label(kind(&self.modifier)),
            item_name(doc, &self.item)
        )
    }
}

/// Removes a modifier and the keyframes of its lanes (as FRAME does).
pub struct RemoveModifier {
    pub item: String,
    pub modifier: String,
}

impl Command for RemoveModifier {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let list = modifiers_mut(doc, &self.item)?;
        let at = position(list, &self.modifier)?;
        let removed = list.remove(at);
        if list.is_empty() {
            item_mut(doc, &self.item)?.modifiers = None;
        }
        let kind = kind(&removed).to_string();
        if let Some(track) = doc
            .timeline
            .item_tracks
            .iter_mut()
            .find(|t| t.item_id == self.item)
        {
            for lane in lanes(&kind) {
                track.properties.remove(*lane);
                if let Some(on) = track.enabled_properties.as_mut()
                    && on.contains_key(*lane)
                {
                    on.insert((*lane).to_string(), false);
                }
            }
        }
        Ok(Change::Layer(vec![self.item.clone()]))
    }
    fn label(&self, doc: &Document) -> String {
        let kind = doc
            .items
            .iter()
            .find(|i| i.id == self.item)
            .and_then(|i| {
                i.modifiers
                    .iter()
                    .flatten()
                    .find(|m| id(m) == self.modifier)
            })
            .map_or("modifier", |m| label(kind(m)));
        format!("Remove {kind} from {}", item_name(doc, &self.item))
    }
}

/// Moves a modifier to another place in the stack (they apply in order).
pub struct MoveModifier {
    pub item: String,
    pub modifier: String,
    pub to: usize,
}

impl Command for MoveModifier {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let list = modifiers_mut(doc, &self.item)?;
        let from = position(list, &self.modifier)?;
        if self.to >= list.len() {
            return Err(CommandError::InvalidValue("no such place".into()));
        }
        let m = list.remove(from);
        list.insert(self.to, m);
        Ok(Change::Layer(vec![self.item.clone()]))
    }
    fn label(&self, doc: &Document) -> String {
        format!("Reorder modifiers of {}", item_name(doc, &self.item))
    }
}

/// Switches a modifier on or off.
pub fn set_enabled(item: &str, modifier: &str, on: bool) -> crate::commands::SetField {
    crate::commands::SetField {
        target: FieldTarget::Modifier {
            item: item.into(),
            modifier: modifier.into(),
        },
        path: "enabled".into(),
        value: Value::Bool(on),
        update: Update::Layer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::History;
    use crate::layers::{NewLayer, new_layer};
    use crate::normalize::normalize;
    use crate::profile::Profile;

    fn doc() -> Document {
        let g = serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut d = normalize(&g, &Profile::framecraft());
        let mut text = new_layer(NewLayer::Text, None);
        text.id = "t".into();
        let mut rect = new_layer(NewLayer::Box, None);
        rect.id = "b".into();
        d.items.push(text);
        d.items.push(rect);
        d
    }

    fn add(
        d: &mut Document,
        h: &mut History,
        item: &str,
        kind: &str,
    ) -> Result<String, CommandError> {
        let it = d.items.iter().find(|i| i.id == item).unwrap().clone();
        let m = new_modifier(d, &it, kind).unwrap();
        let mid = id(&m).to_string();
        h.execute(
            d,
            &AddModifier {
                item: item.into(),
                modifier: m,
            },
        )?;
        Ok(mid)
    }

    #[test]
    fn every_kind_has_defaults_and_its_fields_write_back() {
        let mut d = doc();
        let mut h = History::default();
        for (kind, _) in KINDS {
            let target = if kinds_for(&d.items[1]).contains(kind) {
                "b"
            } else {
                "t"
            };
            let mid = add(&mut d, &mut h, target, kind).unwrap();
            let item = d.items.iter().find(|i| i.id == target).unwrap().clone();
            let m = item
                .modifiers
                .iter()
                .flatten()
                .find(|m| id(m) == mid)
                .unwrap()
                .clone();
            let json = serde_json::to_value(&m).unwrap();
            assert!(FIELDS.iter().any(|p| p.group == *kind), "{kind}");
            for p in FIELDS.iter().filter(|p| p.group == *kind) {
                let mut v = crate::schema::lookup(&json, p.path);
                if v.is_null() {
                    v = match p.kind {
                        Kind::MaskPoints => {
                            serde_json::json!([{"x":0,"y":0},{"x":10,"y":0},{"x":0,"y":10}])
                        }
                        _ => Value::from("x"),
                    };
                }
                let cmd = p
                    .modifier_command(target, &mid, v, 0.0, false)
                    .unwrap_or_else(|| panic!("{kind}.{}", p.path));
                h.execute(&mut d, cmd.as_ref())
                    .unwrap_or_else(|e| panic!("{kind}.{}: {e}", p.path));
            }
        }
        // Text effects stay off non-text layers; one of each type.
        assert!(add(&mut d, &mut h, "b", "typewriter").is_err());
        assert!(add(&mut d, &mut h, "b", "blur").is_err());
    }

    #[test]
    fn lanes_set_the_modifier_and_removing_drops_its_keys() {
        let mut d = doc();
        let mut h = History::default();
        let mid = add(&mut d, &mut h, "b", "blur").unwrap();
        let radius = FIELDS
            .iter()
            .find(|p| p.lane == Some("modBlurRadius"))
            .unwrap();
        // Not animated, Auto-key off: the modifier's own value changes.
        let cmd = radius
            .modifier_command("b", &mid, Value::from("4"), 0.0, false)
            .unwrap();
        h.execute(&mut d, cmd.as_ref()).unwrap();
        assert_eq!(
            crate::keys::lane_base(&d.items[1], "modBlurRadius"),
            Some(4.0)
        );
        // Auto-key on: a keyframe.
        let cmd = radius
            .modifier_command("b", &mid, Value::from(9), 500.0, true)
            .unwrap();
        h.execute(&mut d, cmd.as_ref()).unwrap();
        let keys = |d: &Document| {
            d.timeline
                .item_tracks
                .iter()
                .find(|t| t.item_id == "b")
                .and_then(|t| t.properties.get("modBlurRadius"))
                .map_or(0, Vec::len)
        };
        assert_eq!(keys(&d), 1);
        h.execute(
            &mut d,
            &RemoveModifier {
                item: "b".into(),
                modifier: mid.clone(),
            },
        )
        .unwrap();
        assert!(d.items[1].modifiers.is_none());
        assert_eq!(keys(&d), 0);
        h.undo(&mut d).unwrap();
        assert_eq!(keys(&d), 1);
        assert_eq!(d.items[1].modifiers.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn reorder_and_switch_off() {
        let mut d = doc();
        let mut h = History::default();
        let a = add(&mut d, &mut h, "b", "blur").unwrap();
        let b = add(&mut d, &mut h, "b", "shadow").unwrap();
        h.execute(
            &mut d,
            &MoveModifier {
                item: "b".into(),
                modifier: b.clone(),
                to: 0,
            },
        )
        .unwrap();
        let order: Vec<&str> = d.items[1].modifiers.iter().flatten().map(id).collect();
        assert_eq!(order, [b.as_str(), a.as_str()]);
        h.execute(&mut d, &set_enabled("b", &a, false)).unwrap();
        assert!(!enabled(&d.items[1].modifiers.as_ref().unwrap()[1]));
    }

    #[test]
    fn fields_follow_settings() {
        let mut d = doc();
        let mut h = History::default();
        add(&mut d, &mut h, "t", "textStagger").unwrap();
        add(&mut d, &mut h, "b", "mask").unwrap();
        let paths = |d: &Document, i: usize| -> Vec<&'static str> {
            let item = &d.items[i];
            fields(d, item, &item.modifiers.as_ref().unwrap()[0])
                .iter()
                .map(|p| p.path)
                .collect()
        };
        // Fade: no spread / amplitude / speed. Rect mask: no text or points.
        assert!(!paths(&d, 0).contains(&"spread"));
        let mask = paths(&d, 1);
        assert!(mask.contains(&"transform.width") && !mask.contains(&"textContent"));
    }
}
