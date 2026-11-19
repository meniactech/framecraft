// SPDX-License-Identifier: AGPL-3.0-or-later
//! Turns a stored graphic into a complete one: fills defaults, repairs
//! fields from older versions and syncs Lottie / video start keys.
//! A port of FRAME's `parseGraphicData`.

use crate::model::*;
use crate::profile::Profile;
use crate::{js, ramp, script};
use serde_json::Value;
use std::collections::BTreeMap;

/// A graphic ready for editing and export.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub scene: Scene,
    pub items: Vec<Item>,
    pub timeline: Timeline,
    /// Script source shipped with exports (compiled from the script
    /// functions, or the stored classic script).
    pub template_js: String,
    pub template_functions: Vec<TemplateFunction>,
    pub template_globals: Vec<TemplateGlobal>,
}

/// Lane holding the start time of a Lottie or video clip.
pub const CLIP_START: &str = "clipStart";

pub fn normalize(g: &Graphic, profile: &Profile) -> Document {
    let raw_js = g.template_js.clone().unwrap_or_default();
    let template_functions = script::normalize_functions(g.template_functions.as_deref(), &raw_js);
    let template_globals = script::normalize_globals(g.template_globals.as_deref());
    let compiled = script::compile(&template_functions, &template_globals);
    let template_js = if compiled.is_empty() {
        raw_js
    } else {
        compiled
    };

    let items: Vec<Item> = g.items.iter().map(|i| normalize_item(i, profile)).collect();
    let mut timeline = normalize_timeline(g.timeline.as_ref());
    let items = ensure_clip_start_tracks(&mut timeline, items);

    Document {
        scene: normalize_scene(&g.scene, profile),
        items,
        timeline,
        template_js,
        template_functions,
        template_globals,
    }
}

fn normalize_scene(s: &Scene, p: &Profile) -> Scene {
    fn d<T: Clone>(v: &Option<T>, default: T) -> Option<T> {
        Some(v.clone().unwrap_or(default))
    }
    Scene {
        spx_description: d(&s.spx_description, p.default_scene_description.clone()),
        spx_playserver: d(&s.spx_playserver, "OVERLAY".into()),
        spx_playchannel: d(&s.spx_playchannel, 1.0),
        spx_playlayer: d(&s.spx_playlayer, 2.0),
        spx_webplayout: d(&s.spx_webplayout, 2.0),
        spx_out: d(&s.spx_out, SpxOut::Text("manual".into())),
        spx_dataformat: d(&s.spx_dataformat, "json".into()),
        spx_uicolor: d(&s.spx_uicolor, 1.0),
        spx_prevent_toggle: d(&s.spx_prevent_toggle, true),
        ionboard_hook_enabled: d(&s.ionboard_hook_enabled, false),
        ionboard_panel_enabled: d(&s.ionboard_panel_enabled, false),
        ionboard_magic_id: d(&s.ionboard_magic_id, String::new()),
        ionboard_project_id: d(&s.ionboard_project_id, String::new()),
        guide_line_width: d(&s.guide_line_width, 5.0),
        guide_color_safe10: d(&s.guide_color_safe10, "rgba(110, 231, 183, 0.75)".into()),
        guide_color_safe20: d(&s.guide_color_safe20, "rgba(252, 211, 77, 0.75)".into()),
        guide_color_thirds: d(&s.guide_color_thirds, "rgba(103, 232, 249, 0.7)".into()),
        guide_color_center: d(&s.guide_color_center, "rgba(253, 164, 175, 0.8)".into()),
        custom_rulers_h: d(&s.custom_rulers_h, Vec::new()),
        custom_rulers_v: d(&s.custom_rulers_v, Vec::new()),
        custom_ruler_color: d(&s.custom_ruler_color, "rgba(167, 139, 250, 0.75)".into()),
        text_payload: d(&s.text_payload, BTreeMap::new()),
        fake_background_enabled: d(&s.fake_background_enabled, false),
        fake_background_source: d(&s.fake_background_source, "builtin".into()),
        fake_background_builtin_id: d(&s.fake_background_builtin_id, "news-room".into()),
        fake_background_media_url: d(&s.fake_background_media_url, String::new()),
        fake_background_opacity: d(&s.fake_background_opacity, 100.0),
        fake_background_blur: d(&s.fake_background_blur, 0.0),
        ..s.clone()
    }
}

fn one_of(v: &Option<String>, allowed: &[&str], default: &str) -> String {
    match v.as_deref() {
        Some(s) if allowed.contains(&s) => s.to_string(),
        _ => default.to_string(),
    }
}

fn trimmed_or(v: &Option<String>, default: &str) -> String {
    match v.as_deref().map(js::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => default.to_string(),
    }
}

/// JavaScript truthiness of a JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

const BLEND_MODES: [&str; 16] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

pub fn normalize_blend_mode(v: Option<&str>) -> String {
    let s = js::trim(v.unwrap_or("")).to_lowercase();
    if BLEND_MODES.contains(&s.as_str()) {
        s
    } else {
        "normal".into()
    }
}

const BG_POSITIONS: [&str; 8] = [
    "top",
    "bottom",
    "left",
    "right",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
];

/// `/svg/basic/shape-NN.svg`: FRAME's single-color stock shapes.
pub fn is_basic_svg_stock_url(url: Option<&str>) -> bool {
    let u = js::trim(url.unwrap_or("")).to_lowercase();
    let u = u.split(['?', '#']).next().unwrap_or("");
    let Some(rest) = u.rsplit_once("/svg/basic/shape-").map(|(_, r)| r) else {
        return false;
    };
    let Some(num) = rest.strip_suffix(".svg") else {
        return false;
    };
    !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit())
}

fn normalize_item(item: &Item, p: &Profile) -> Item {
    let mut it = item.clone();
    let legacy_nested = it.extra.remove("lockNestedSelection");
    it.locked = Some(item.locked.unwrap_or(false));
    it.lock_children = Some(match item.lock_children {
        Some(v) => v,
        None => truthy(legacy_nested.as_ref()),
    });

    let s = &item.shadow;
    let color: String = s
        .color
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let old_default_shadow = s.x == 0.0
        && s.y == 0.0
        && s.blur == 10.0
        && ["#000000", "#000", "rgb(0,0,0)", "rgba(0,0,0,1)"].contains(&color.as_str());
    if old_default_shadow {
        it.shadow = Shadow {
            color: "rgba(0, 0, 0, 0)".into(),
            x: 0.0,
            y: 0.0,
            blur: 0.0,
            extra: Default::default(),
        };
    }

    it.modifiers = Some(
        item.modifiers
            .iter()
            .flatten()
            .map(upgrade_modifier)
            .collect(),
    );
    it.scale_x = Some(js::finite_or(item.scale_x, 1.0));
    it.scale_y = Some(js::finite_or(item.scale_y, 1.0));
    it.pivot_x = Some(js::finite_or(item.pivot_x, 50.0));
    it.pivot_y = Some(js::finite_or(item.pivot_y, 50.0));

    let start = item
        .gradient_start
        .clone()
        .unwrap_or_else(|| "rgba(99, 102, 241, 1)".into());
    let end = item
        .gradient_end
        .clone()
        .unwrap_or_else(|| "rgba(56, 189, 248, 1)".into());
    it.gradient_ramp = Some(ramp::color_ramp_from_legacy(
        item.gradient_ramp.as_deref(),
        Some(&start),
        Some(&end),
    ));
    it.gradient_start = Some(start);
    it.gradient_end = Some(end);
    it.gradient_type = Some(one_of(
        &item.gradient_type,
        &["circle", "ellipse", "conic"],
        "linear",
    ));
    it.gradient_center = Some(gradient_center(item.gradient_center.as_ref()));
    it.background_type = Some(one_of(
        &item.background_type,
        &["gradient", "image"],
        "solid",
    ));
    it.blend_mode = Some(normalize_blend_mode(item.blend_mode.as_deref()));
    it.bg_image_url = Some(item.bg_image_url.clone().unwrap_or_default());
    it.bg_image_fit = Some(one_of(&item.bg_image_fit, &["contain", "stretch"], "cover"));
    it.bg_image_position = Some(one_of(&item.bg_image_position, &BG_POSITIONS, "center"));
    it.bg_image_repeat = Some(one_of(
        &item.bg_image_repeat,
        &["repeat", "repeat-x", "repeat-y"],
        "no-repeat",
    ));
    it.lottie_start_ms = Some(js::or(item.lottie_start_ms, 0.0).max(0.0));
    it.lottie_loop = Some(item.lottie_loop == Some(true));
    it.video_start_ms = Some(js::or(item.video_start_ms, 0.0).max(0.0));
    it.video_loop = Some(item.video_loop == Some(true));
    it.visibility_variable_id = Some(
        item.visibility_variable_id
            .as_deref()
            .map(js::trim)
            .unwrap_or("")
            .to_string(),
    );
    it.visibility_invert = Some(item.visibility_invert == Some(true));

    match item.item_type {
        Some(ItemType::ImageSlot) => {
            it.image_variable_id = Some(
                item.image_variable_id
                    .as_deref()
                    .map(js::trim)
                    .unwrap_or("")
                    .to_string(),
            );
            it.image_asset_folder = Some(trimmed_or(&item.image_asset_folder, "./images/"));
            it.image_extension = Some(match item.image_extension.as_deref().map(js::trim) {
                Some(s) if !s.is_empty() => s.strip_prefix('.').unwrap_or(s).to_string(),
                _ => "png".into(),
            });
            it.image_default_value = Some(item.image_default_value.clone().unwrap_or_default());
            it.image_fit = Some(one_of(&item.image_fit, &["contain"], "cover"));
        }
        Some(ItemType::Image) => {
            let svg_tint =
                item.svg_tint == Some(true) || is_basic_svg_stock_url(item.media_url.as_deref());
            let bg: String = js::trim(&item.bg_color)
                .to_lowercase()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let transparent = bg.is_empty()
                || [
                    "#00000000",
                    "#0000",
                    "transparent",
                    "rgba(0,0,0,0)",
                    "rgba(0,0,0,0.0)",
                ]
                .contains(&bg.as_str());
            it.svg_tint = Some(svg_tint);
            if svg_tint && transparent {
                it.bg_color = "#ffffffff".into();
            }
        }
        Some(ItemType::Chart) => normalize_chart(item, &mut it),
        Some(ItemType::Qr) => normalize_qr(item, &mut it, p),
        Some(ItemType::Particles) => normalize_particles(item, &mut it),
        Some(ItemType::Lightning) => normalize_lightning(item, &mut it),
        Some(ItemType::Pattern) => normalize_pattern(item, &mut it),
        _ => {}
    }

    let single = js::finite_or(Some(item.border_width), 0.0);
    it.border_width = single;
    it.border_width_mode = Some(one_of(&item.border_width_mode, &["sides"], "single"));
    it.border_widths = Some(match item.border_widths {
        Some(w) => w.map(|v| js::finite_or(Some(v), single)),
        None => [single; 4],
    });
    it
}

/// FRAME's legacy `transform` modifier became `skew`.
fn upgrade_modifier(m: &Modifier) -> Modifier {
    let Modifier::Other(v) = m else {
        return m.clone();
    };
    if v.get("type").and_then(Value::as_str) != Some("transform") {
        return m.clone();
    }
    let mut obj = v.clone();
    let num = |key: &str| {
        v.get(key)
            .and_then(Value::as_f64)
            .filter(|x| x.is_finite())
            .unwrap_or(0.0)
    };
    obj["type"] = "skew".into();
    obj["skewX"] = num("skewX").into();
    obj["skewY"] = num("skewY").into();
    serde_json::from_value(obj).unwrap_or_else(|_| m.clone())
}

fn gradient_center(c: Option<&Point>) -> Point {
    let clamp = |v: Option<f64>| {
        v.filter(|x| x.is_finite())
            .map_or(0.5, |x| x.clamp(0.0, 1.0))
    };
    Point {
        x: clamp(c.map(|c| c.x)),
        y: clamp(c.map(|c| c.y)),
        extra: Default::default(),
    }
}

fn normalize_chart(item: &Item, it: &mut Item) {
    it.chart_kind = Some(one_of(&item.chart_kind, &["pie", "line"], "bar"));
    it.chart_points = Some(match item.chart_points.as_deref() {
        None | Some([]) => [
            ("cp_a", "A", 40.0),
            ("cp_b", "B", 25.0),
            ("cp_c", "C", 55.0),
            ("cp_d", "D", 30.0),
        ]
        .into_iter()
        .map(|(id, label, value)| ChartPoint {
            id: id.into(),
            label: label.into(),
            value,
            color: None,
            live_variable_id: None,
            extra: Default::default(),
        })
        .collect(),
        Some(points) => points
            .iter()
            .enumerate()
            .map(|(i, pt)| ChartPoint {
                id: if pt.id.is_empty() {
                    format!("cp_{i}")
                } else {
                    pt.id.clone()
                },
                label: pt.label.clone(),
                value: if pt.value.is_finite() { pt.value } else { 0.0 },
                color: pt
                    .color
                    .as_deref()
                    .map(js::trim)
                    .filter(|c| !c.is_empty())
                    .map(String::from),
                live_variable_id: pt
                    .live_variable_id
                    .as_deref()
                    .map(js::trim)
                    .filter(|c| !c.is_empty())
                    .map(String::from),
                extra: Default::default(),
            })
            .collect(),
    });
    it.chart_cutout = Some(js::finite_or(item.chart_cutout, 0.0));
    it.chart_bar_thickness = Some(js::finite_or(item.chart_bar_thickness, 80.0));
    it.chart_line_width = Some(js::finite_or(item.chart_line_width, 2.0));
    it.chart_point_radius = Some(js::finite_or(item.chart_point_radius, 4.0));
    it.chart_line_tension = Some(js::finite_or(item.chart_line_tension, 0.2));
    // These two look at the stored kind, before it was normalized above.
    let raw_kind = item.chart_kind.as_deref();
    it.chart_show_legend = Some(item.chart_show_legend.unwrap_or(raw_kind == Some("pie")));
    it.chart_show_values = Some(
        item.chart_show_values
            .unwrap_or(matches!(raw_kind, None | Some("bar"))),
    );
    it.chart_value_sort = Some(one_of(&item.chart_value_sort, &["asc", "desc"], "none"));
}

pub fn normalize_qr_error_correction(v: Option<&str>) -> String {
    let s = js::trim(v.unwrap_or("")).to_uppercase();
    if ["L", "M", "Q", "H"].contains(&s.as_str()) {
        s
    } else {
        "M".into()
    }
}

pub fn normalize_qr_margin(v: Option<f64>) -> f64 {
    match v.map(js::round) {
        Some(n) if n.is_finite() => n.clamp(0.0, 8.0),
        _ => 2.0,
    }
}

fn normalize_qr(item: &Item, it: &mut Item, p: &Profile) {
    it.qr_content = Some(
        item.qr_content
            .clone()
            .unwrap_or_else(|| p.default_qr_content.clone()),
    );
    it.qr_variable_id = Some(
        item.qr_variable_id
            .as_deref()
            .map(js::trim)
            .unwrap_or("")
            .to_string(),
    );
    it.qr_fg_color = Some(trimmed_or(&item.qr_fg_color, "#000000"));
    it.qr_bg_color = Some(trimmed_or(&item.qr_bg_color, "#ffffff"));
    it.qr_error_correction = Some(normalize_qr_error_correction(
        item.qr_error_correction.as_deref(),
    ));
    it.qr_margin = Some(normalize_qr_margin(item.qr_margin));
}

/// FRAME picks a random seed when an item has none. FrameCraft derives one
/// from the item id instead, so the same file always gives the same result.
fn fallback_seed(item_id: &str) -> f64 {
    let mut h: u32 = 0x811c_9dc5;
    for b in item_id.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    f64::from(h.max(1))
}

fn normalize_particles(item: &Item, it: &mut Item) {
    let w = js::or(Some(item.width), 400.0);
    let h = js::or(Some(item.height), 400.0);
    let fin = |v: Option<f64>, d: f64| js::finite_or(v, d);
    let non_empty = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());

    let color_ramp = ramp::color_ramp_from_legacy(
        item.particle_color_ramp.as_deref(),
        Some(
            non_empty(&item.particle_color_start)
                .as_deref()
                .unwrap_or(ramp::DEFAULT_COLOR_START),
        ),
        Some(
            non_empty(&item.particle_color_end)
                .as_deref()
                .unwrap_or(ramp::DEFAULT_COLOR_END),
        ),
    );
    let size_base = fin(item.particle_size, 8.0);
    let size_ramp = ramp::size_scale_ramp_from_legacy(
        item.particle_size_ramp.as_deref(),
        Some(size_base),
        Some(fin(item.particle_size_end, 2.0)),
    );
    let palette_given = item
        .particle_palette_colors
        .as_ref()
        .is_some_and(|c| !c.is_empty());

    it.particle_emitting = Some(item.particle_emitting.unwrap_or(true));
    it.particle_emitter_x = Some(fin(item.particle_emitter_x, w / 2.0));
    it.particle_emitter_y = Some(fin(item.particle_emitter_y, h / 2.0));
    it.particle_emitter_radius = Some(fin(item.particle_emitter_radius, 12.0));
    it.particle_emitter_width = Some(fin(item.particle_emitter_width, 80.0));
    it.particle_emitter_height = Some(fin(item.particle_emitter_height, 48.0));
    it.particle_emitter_shape = Some(one_of(&item.particle_emitter_shape, &["box"], "circle"));
    it.particle_emit_angle_deg = Some(fin(item.particle_emit_angle_deg, -90.0));
    it.particle_spawn_rate = Some(fin(item.particle_spawn_rate, 40.0));
    it.particle_max_count = Some(fin(item.particle_max_count, 300.0));
    it.particle_lifetime_ms = Some(fin(item.particle_lifetime_ms, 1800.0));
    it.particle_size = Some(size_base);
    it.particle_size_end = Some(size_base * size_ramp[size_ramp.len() - 1].value);
    it.particle_size_ramp = Some(size_ramp);
    it.particle_speed = Some(fin(item.particle_speed, 80.0));
    it.particle_spread_deg = Some(fin(item.particle_spread_deg, 360.0));
    it.particle_gravity_x = Some(fin(item.particle_gravity_x, 0.0));
    it.particle_gravity_y = Some(fin(item.particle_gravity_y, 120.0));
    it.particle_rotation_deg = Some(fin(item.particle_rotation_deg, 0.0));
    it.particle_rotation_random_deg = Some(fin(item.particle_rotation_random_deg, 0.0));
    it.particle_spin_deg = Some(fin(item.particle_spin_deg, 0.0));
    it.particle_align_to_velocity = Some(item.particle_align_to_velocity.unwrap_or(false));
    it.particle_color_start = Some(color_ramp[0].color.clone());
    it.particle_color_end = Some(color_ramp[color_ramp.len() - 1].color.clone());
    it.particle_color_ramp = Some(color_ramp);
    it.particle_color_mode = Some(
        if item.particle_color_mode.as_deref() == Some("paletteCycle") && palette_given {
            "paletteCycle"
        } else {
            "ramp"
        }
        .into(),
    );
    it.particle_palette_colors = Some(
        item.particle_palette_colors
            .iter()
            .flatten()
            .filter(|c| !js::trim(c).is_empty())
            .cloned()
            .collect(),
    );
    it.particle_alpha_ramp = Some(ramp::normalize_value_ramp(
        item.particle_alpha_ramp.as_deref(),
        1.0,
        1.0,
    ));
    it.particle_sprite_url = Some(item.particle_sprite_url.clone().unwrap_or_default());
    it.particle_blend = Some(one_of(&item.particle_blend, &["normal"], "additive"));
    it.particle_attach = Some(one_of(&item.particle_attach, &["relative"], "absolute"));
    it.particle_seed = Some(match item.particle_seed {
        Some(s) if s.is_finite() && s != 0.0 => s,
        _ => fallback_seed(&item.id),
    });
}

fn normalize_lightning(item: &Item, it: &mut Item) {
    let w = js::or(Some(item.width), 480.0);
    let h = js::or(Some(item.height), 270.0);
    let pad = 24.0;
    let fin = |v: Option<f64>| v.filter(|x| x.is_finite());
    let coord = |v: Option<f64>, d: f64| fin(v).map_or(d, js::round);
    let clamp_round = |v: Option<f64>, lo: f64, hi: f64, d: f64| {
        fin(v.map(js::round)).map_or(d, |x| x.clamp(lo, hi))
    };

    it.lightning_emitting = Some(item.lightning_emitting.unwrap_or(true));
    it.lightning_ax = Some(coord(item.lightning_ax, pad));
    it.lightning_ay = Some(coord(item.lightning_ay, js::round(h / 2.0)));
    it.lightning_bx = Some(coord(item.lightning_bx, (pad + 8.0).max(w - pad)));
    it.lightning_by = Some(coord(item.lightning_by, js::round(h / 2.0)));
    it.lightning_segments = Some(clamp_round(item.lightning_segments, 2.0, 48.0, 14.0));
    it.lightning_amplitude = Some(fin(item.lightning_amplitude).map_or(18.0, |x| x.max(0.0)));
    it.lightning_width = Some(fin(item.lightning_width).map_or(2.0, |x| x.max(0.5)));
    it.lightning_glow_width = Some(fin(item.lightning_glow_width).map_or(10.0, |x| x.max(0.0)));
    it.lightning_color = Some(trimmed_or(&item.lightning_color, "#e0e7ff"));
    it.lightning_glow_color = Some(trimmed_or(&item.lightning_glow_color, "#6366f1"));
    it.lightning_branches = Some(clamp_round(item.lightning_branches, 0.0, 6.0, 2.0));
    it.lightning_seed = Some(match fin(item.lightning_seed) {
        Some(s) => match js::to_uint32(s.floor()) {
            0.0 => 1.0,
            v => v,
        },
        None => fallback_seed(&item.id),
    });
    it.lightning_flicker_ms = Some(clamp_round(item.lightning_flicker_ms, 8.0, 500.0, 48.0));
}

fn normalize_pattern(item: &Item, it: &mut Item) {
    let fin = |v: Option<f64>| v.filter(|x| x.is_finite());
    let half = |v: f64| (js::round(v * 2.0) / 2.0).max(0.5);
    it.pattern_kind = Some(one_of(
        &item.pattern_kind,
        &["stripes", "dots", "grid", "checker"],
        "checker",
    ));
    it.pattern_color_a = Some(trimmed_or(&item.pattern_color_a, "#1e293b"));
    it.pattern_color_b = Some(trimmed_or(&item.pattern_color_b, "#334155"));
    it.pattern_cell_size =
        Some(fin(item.pattern_cell_size).map_or(32.0, |v| js::round(v).max(2.0)));
    it.pattern_stripe_width =
        Some(fin(item.pattern_stripe_width).map_or(16.0, |v| js::round(v).max(1.0)));
    it.pattern_stripe_gap =
        Some(fin(item.pattern_stripe_gap).map_or(16.0, |v| js::round(v).max(0.0)));
    it.pattern_stripe_angle_deg =
        Some(fin(item.pattern_stripe_angle_deg).map_or(45.0, |v| js::round(v * 10.0) / 10.0));
    it.pattern_dot_radius = Some(fin(item.pattern_dot_radius).map_or(6.0, half));
    it.pattern_line_width = Some(fin(item.pattern_line_width).map_or(2.0, half));
    it.pattern_phase = Some(fin(item.pattern_phase).map_or(0.0, |v| js::round(v * 100.0) / 100.0));
}

fn normalize_timeline(raw: Option<&Timeline>) -> Timeline {
    let Some(raw) = raw else {
        return Timeline {
            duration: 10000.0,
            fps: 30.0,
            snap_enabled: Some(true),
            snap_step: Some(10.0),
            current_time: Some(0.0),
            is_playing: Some(false),
            r#loop: Some(false),
            auto_keyframe: Some(false),
            auto_rewind: Some(true),
            item_tracks: Vec::new(),
            scene_cues: Some(Vec::new()),
            extra: Default::default(),
        };
    };
    let mut cues: Vec<SceneCue> = raw
        .scene_cues
        .iter()
        .flatten()
        .map(|c| SceneCue {
            time: if c.time.is_finite() {
                c.time.max(0.0)
            } else {
                0.0
            },
            delay_ms: c.delay_ms.map(|d| js::or(Some(d), 0.0).max(0.0)),
            ..c.clone()
        })
        .collect();
    cues.sort_by(|a, b| a.time.total_cmp(&b.time));

    let item_tracks = raw
        .item_tracks
        .iter()
        .map(|t| {
            let mut enabled = t.enabled_properties.clone().unwrap_or_default();
            for prop in t.properties.keys() {
                enabled.entry(prop.clone()).or_insert(true);
            }
            ItemTrack {
                enabled_properties: Some(enabled),
                extra: Default::default(),
                ..t.clone()
            }
        })
        .collect();

    Timeline {
        snap_enabled: Some(raw.snap_enabled.unwrap_or(true)),
        snap_step: Some(raw.snap_step.unwrap_or(10.0)),
        current_time: Some(raw.current_time.unwrap_or(0.0)),
        is_playing: Some(raw.is_playing.unwrap_or(false)),
        r#loop: Some(raw.r#loop.unwrap_or(false)),
        auto_keyframe: Some(raw.auto_keyframe.unwrap_or(false)),
        auto_rewind: Some(raw.auto_rewind.unwrap_or(true)),
        item_tracks,
        scene_cues: Some(cues),
        ..raw.clone()
    }
}

fn uses_clip_start(item: &Item) -> bool {
    matches!(item.item_type, Some(ItemType::Lottie | ItemType::Video))
}

fn clip_start_ms(item: &Item) -> f64 {
    let v = match item.item_type {
        Some(ItemType::Lottie) => item.lottie_start_ms,
        Some(ItemType::Video) => item.video_start_ms,
        _ => return 0.0,
    };
    js::round(js::or(v, 0.0)).max(0.0)
}

/// Exactly one `clipStart` key per Lottie / video item, and the item's start
/// field synced from it. `clipStart` lanes on other items are removed.
fn ensure_clip_start_tracks(timeline: &mut Timeline, items: Vec<Item>) -> Vec<Item> {
    let items: Vec<Item> = items
        .into_iter()
        .map(|mut item| {
            if !uses_clip_start(&item) {
                return item;
            }
            let fallback = clip_start_ms(&item);
            let idx = timeline
                .item_tracks
                .iter()
                .position(|t| t.item_id == item.id);
            let mut existing: Vec<f64> = idx
                .and_then(|i| timeline.item_tracks[i].properties.get(CLIP_START))
                .map(|keys| keys.iter().map(|k| k.time).collect())
                .unwrap_or_default();
            existing.sort_by(f64::total_cmp);
            let start = existing
                .first()
                .map_or(fallback, |t| js::round(js::or(Some(*t), 0.0)).max(0.0));
            let enabled = idx.and_then(|i| {
                timeline.item_tracks[i]
                    .enabled_properties
                    .as_ref()
                    .and_then(|e| e.get(CLIP_START).copied())
            });
            let rewrite =
                existing.len() != 1 || (existing[0] - start).abs() > 0.001 || enabled != Some(true);
            if rewrite {
                let i = idx.unwrap_or_else(|| {
                    timeline.item_tracks.push(ItemTrack {
                        item_id: item.id.clone(),
                        properties: BTreeMap::new(),
                        enabled_properties: Some(BTreeMap::new()),
                        extra: Default::default(),
                    });
                    timeline.item_tracks.len() - 1
                });
                let track = &mut timeline.item_tracks[i];
                track.properties.insert(
                    CLIP_START.into(),
                    vec![Keyframe {
                        time: start,
                        value: 0.0,
                        easing: Some("linear".into()),
                        extra: Default::default(),
                    }],
                );
                track
                    .enabled_properties
                    .get_or_insert_default()
                    .insert(CLIP_START.into(), true);
            }
            if clip_start_ms(&item) != start {
                match item.item_type {
                    Some(ItemType::Lottie) => item.lottie_start_ms = Some(start),
                    _ => item.video_start_ms = Some(start),
                }
            }
            item
        })
        .collect();

    for track in &mut timeline.item_tracks {
        let owner = items.iter().find(|i| i.id == track.item_id);
        if owner.is_some_and(uses_clip_start) {
            continue;
        }
        track.properties.remove(CLIP_START);
        if let Some(e) = track.enabled_properties.as_mut() {
            e.remove(CLIP_START);
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_svg_urls() {
        assert!(is_basic_svg_stock_url(Some("/svg/basic/shape-03.svg")));
        assert!(is_basic_svg_stock_url(Some(
            " /SVG/basic/shape-12.svg?v=2 "
        )));
        assert!(!is_basic_svg_stock_url(Some("/svg/basic/shape-.svg")));
        assert!(!is_basic_svg_stock_url(Some("/uploads/shape-03.svg")));
    }

    #[test]
    fn qr_helpers() {
        assert_eq!(normalize_qr_error_correction(Some(" q ")), "Q");
        assert_eq!(normalize_qr_error_correction(Some("x")), "M");
        assert_eq!(normalize_qr_margin(Some(9.6)), 8.0);
        assert_eq!(normalize_qr_margin(None), 2.0);
    }

    #[test]
    fn blend_modes() {
        assert_eq!(normalize_blend_mode(Some(" Multiply ")), "multiply");
        assert_eq!(normalize_blend_mode(Some("plus")), "normal");
    }
}
