// SPDX-License-Identifier: AGPL-3.0-or-later
//! Item modifiers (Properties → Modifiers): masks, filters, text effects and
//! path following. Stored as a list on each item, tagged by `type`.

use super::{Extra, push_extra};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Modifier {
    Mask(MaskModifier),
    Blur(BlurModifier),
    Color(ColorModifier),
    Shadow(ShadowModifier),
    Skew(SkewModifier),
    Crop(CropModifier),
    Typewriter(TypewriterModifier),
    PathFollow(PathFollowModifier),
    TextStagger(TextStaggerModifier),
    TextScramble(TextScrambleModifier),
    TextGlyphColor(TextGlyphColorModifier),
    /// A modifier type this version does not know (including FRAME's legacy
    /// `transform`, which FRAME turns into `skew` on load). Kept as read.
    #[serde(untagged)]
    Other(Value),
}

impl Modifier {
    /// Fields of a known modifier type that this model does not cover.
    pub(crate) fn collect_unknown(&self, path: &str, out: &mut Vec<String>) {
        let extra = match self {
            Modifier::Mask(m) => {
                push_extra(out, &format!("{path}.transform"), &m.transform.extra);
                &m.extra
            }
            Modifier::Blur(m) => &m.extra,
            Modifier::Color(m) => &m.extra,
            Modifier::Shadow(m) => &m.extra,
            Modifier::Skew(m) => &m.extra,
            Modifier::Crop(m) => &m.extra,
            Modifier::Typewriter(m) => &m.extra,
            Modifier::PathFollow(m) => &m.extra,
            Modifier::TextStagger(m) => &m.extra,
            Modifier::TextScramble(m) => &m.extra,
            Modifier::TextGlyphColor(m) => &m.extra,
            Modifier::Other(v) => {
                let kind = v.get("type").and_then(Value::as_str).unwrap_or("?");
                out.push(format!("{path} (unknown modifier type `{kind}`)"));
                return;
            }
        };
        push_extra(out, path, extra);
    }
}

/// Mask the item with a shape, text, image or another layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskModifier {
    pub id: String,
    pub enabled: bool,
    /// `alpha`, `luma` or `invert`.
    pub mode: String,
    /// `rect`, `roundedRect`, `ellipse`, `polygon`, `text` or `image`.
    pub shape: String,
    /// Use another layer as the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_element_id: Option<String>,
    pub feather: f64,
    pub transform: MaskTransform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounded_rect_radius: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon_points: Option<Vec<MaskPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskTransform {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskPoint {
    pub x: f64,
    pub y: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlurModifier {
    pub id: String,
    pub enabled: bool,
    pub radius: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorModifier {
    pub id: String,
    pub enabled: bool,
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub hue_rotate: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowModifier {
    pub id: String,
    pub enabled: bool,
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub color: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkewModifier {
    pub id: String,
    pub enabled: bool,
    pub skew_x: f64,
    pub skew_y: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Wipe the item away in one direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropModifier {
    pub id: String,
    pub enabled: bool,
    /// `up`, `down`, `left` or `right`.
    pub direction: String,
    pub percent: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Reveal text by character count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypewriterModifier {
    pub id: String,
    pub enabled: bool,
    /// 0 = empty, 100 = full text.
    pub percent: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Move the item along a path layer, or run text along it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFollowModifier {
    pub id: String,
    pub enabled: bool,
    pub source_element_id: String,
    /// 0 = path start, 100 = path end.
    pub progress: f64,
    /// `follow` or `textAlong`.
    pub mode: String,
    pub bank: bool,
    pub bank_offset_deg: f64,
    pub bank_flip: bool,
    pub closed: bool,
    pub r#loop: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Letters cascade in or out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStaggerModifier {
    pub id: String,
    pub enabled: bool,
    pub progress: f64,
    /// `in` or `out`.
    pub mode: String,
    /// `fade`, `scale`, `slideUp`, `slideDown`, `slideLeft`, `slideRight`,
    /// `tracking`, `stretch`, `wave` or `nervous`.
    pub style: String,
    pub amount: f64,
    /// `forward`, `reverse` or `center`.
    pub order: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spread: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amplitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Letters show random characters, then settle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextScrambleModifier {
    pub id: String,
    pub enabled: bool,
    pub progress: f64,
    /// `alnum`, `symbols` or `custom`.
    pub charset: String,
    pub custom_charset: String,
    pub order: String,
    pub seed: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Per-letter color wave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextGlyphColorModifier {
    pub id: String,
    pub enabled: bool,
    pub progress: f64,
    pub from_color: String,
    pub to_color: String,
    pub amount: f64,
    pub order: String,
    #[serde(flatten)]
    pub extra: Extra,
}
