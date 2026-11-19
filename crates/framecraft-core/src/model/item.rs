// SPDX-License-Identifier: AGPL-3.0-or-later
//! Items: the layers of a scene. One struct holds the fields of every item
//! type, as in FRAME; fields that do not apply to a type are left out.

use super::{Extra, Modifier, double_option, push_extra};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemType {
    Shape,
    Image,
    /// Dynamic image filled at playout (SPX file list or BOARD).
    ImageSlot,
    Text,
    Chart,
    Qr,
    Particles,
    Lightning,
    Pattern,
    Lottie,
    Video,
    /// Group / null object.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeType {
    Box,
    Ellipse,
    Circle,
    Stadium,
    Polygon,
    /// Motion guide path (also bracket connectors when kept visible).
    Path,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    /// Parent layer. Missing and `null` both mean top level; kept apart so
    /// files save back unchanged.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_id: Option<Option<String>>,
    /// Missing in very old graphics (treated by FRAME as a shape or text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<ItemType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_type: Option<ShapeType>,

    // Polygon and path shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon_points: Option<Vec<PolygonPoint>>,
    /// `linear` or `bezier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon_curve: Option<String>,
    /// Path shapes: draw the stroke on play and export (otherwise a guide only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_keep_visible: Option<bool>,

    // Lottie and video clips (start synced from the `clipStart` key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lottie_start_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lottie_loop: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_start_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_loop: Option<bool>,

    // Image slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_variable_id: Option<String>,
    /// SPX file list folder: `./images/` (in the ZIP) or `/media/images/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_asset_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_default_value: Option<String>,
    /// `cover` or `contain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_fit: Option<String>,

    /// Tint a single-color SVG with `bg_color`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg_tint: Option<bool>,

    // Chart (Chart.js).
    /// `bar`, `pie` or `line`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_points: Option<Vec<ChartPoint>>,
    /// Scrub pose 0–1 from the `chartReveal` track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_reveal: Option<f64>,
    /// Pie hole 0–80 %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_cutout: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_bar_thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_line_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_point_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_line_tension: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_show_legend: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_show_values: Option<bool>,
    /// `none`, `asc` or `desc`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_value_sort: Option<String>,

    // QR code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_variable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_fg_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_bg_color: Option<String>,
    /// `L`, `M`, `Q` or `H`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_error_correction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_margin: Option<f64>,

    // Particles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitting: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_height: Option<f64>,
    /// `circle` or `box`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emit_angle_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_spawn_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_max_count: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_lifetime_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_size_end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_size_ramp: Option<Vec<RampValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_spread_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_gravity_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_gravity_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_rotation_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_rotation_random_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_spin_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_align_to_velocity: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_color_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_color_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_color_ramp: Option<Vec<RampColor>>,
    /// `ramp` or `paletteCycle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_color_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_palette_colors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_alpha_ramp: Option<Vec<RampValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_sprite_url: Option<String>,
    /// `normal` or `additive`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_blend: Option<String>,
    /// `absolute` or `relative`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_attach: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_seed: Option<f64>,

    // Lightning bolt from A to B (item-local coordinates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_emitting: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_ax: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_ay: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_bx: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_by: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_segments: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_amplitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_glow_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_glow_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_branches: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_seed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightning_flicker_ms: Option<f64>,

    // Pattern fill.
    /// `checker`, `stripes`, `dots` or `grid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_color_a: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_color_b: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_cell_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_stripe_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_stripe_gap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_stripe_angle_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_dot_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_line_width: Option<f64>,
    /// Animatable offset in pattern periods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_phase: Option<f64>,

    /// Source of image, Lottie, video or SVG content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,

    // Fill.
    /// `solid`, `gradient` or `image`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_type: Option<String>,
    /// `linear`, `circle`, `ellipse` or `conic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_angle: Option<f64>,
    /// Normalized center (0–1) for circle, ellipse and conic gradients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_center: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_ramp: Option<Vec<RampColor>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image_url: Option<String>,
    /// `cover`, `contain` or `stretch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image_fit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image_position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image_repeat: Option<String>,

    /// Editor visibility; hidden items are left out of exports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// BOARD / live key that shows or hides the layer at playout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_variable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_invert: Option<bool>,

    // Text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    /// BOARD / SPX field that fills the text at playout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_variable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// `left`, `center` or `right`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,

    // Transform.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_y: Option<f64>,
    /// Percent of width (default 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot_x: Option<f64>,
    /// Percent of height (default 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot_y: Option<f64>,
    pub opacity: f64,
    #[serde(rename = "bg_color")]
    pub bg_color: String,
    /// CSS `mix-blend-mode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<String>,

    // Border and shadow.
    pub border_width: f64,
    /// `single` or `sides`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width_mode: Option<String>,
    /// Top, right, bottom, left (when `border_width_mode` is `sides`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_widths: Option<[f64; 4]>,
    pub border_color: String,
    /// Top-left, top-right, bottom-right, bottom-left.
    pub border_radius: [f64; 4],
    pub shadow: Shadow,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<Modifier>>,
    /// Cannot be moved in the viewport (still selectable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Viewport clicks on children select this layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_children: Option<bool>,

    #[serde(flatten)]
    pub extra: Extra,
}

impl Item {
    pub(crate) fn collect_unknown(&self, path: &str, out: &mut Vec<String>) {
        push_extra(out, path, &self.extra);
        push_extra(out, &format!("{path}.shadow"), &self.shadow.extra);
        if let Some(c) = &self.gradient_center {
            push_extra(out, &format!("{path}.gradientCenter"), &c.extra);
        }
        let lists: [(&str, Vec<&Extra>); 6] = [
            (
                "polygonPoints",
                self.polygon_points
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
            (
                "chartPoints",
                self.chart_points
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
            (
                "particleSizeRamp",
                self.particle_size_ramp
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
            (
                "particleAlphaRamp",
                self.particle_alpha_ramp
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
            (
                "particleColorRamp",
                self.particle_color_ramp
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
            (
                "gradientRamp",
                self.gradient_ramp
                    .iter()
                    .flatten()
                    .map(|p| &p.extra)
                    .collect(),
            ),
        ];
        for (name, extras) in lists {
            for (i, e) in extras.into_iter().enumerate() {
                push_extra(out, &format!("{path}.{name}[{i}]"), e);
            }
        }
        for (i, m) in self.modifiers.iter().flatten().enumerate() {
            m.collect_unknown(&format!("{path}.modifiers[{i}]"), out);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonPoint {
    pub x: f64,
    pub y: f64,
    /// `linear` or `bezier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_y: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartPoint {
    pub id: String,
    pub label: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// BOARD key that drives this value at playout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_variable_id: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A number along a 0–1 ramp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RampValue {
    pub offset: f64,
    pub value: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A color stop along a 0–1 ramp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RampColor {
    pub offset: f64,
    pub color: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    #[serde(flatten)]
    pub extra: Extra,
}
