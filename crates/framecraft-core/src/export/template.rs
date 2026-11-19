// SPDX-License-Identifier: AGPL-3.0-or-later
//! The template export: one graphic as the data the SPX, OGraf and banner
//! writers render. A port of FRAME's `buildTemplateExport`.

use crate::model::*;
use crate::normalize::{CLIP_START, Document, is_basic_svg_stock_url};
use crate::profile::Profile;
use crate::{easing, js};
use serde::Serialize;
use std::collections::BTreeMap;

/// Chart colors used when a point has none and no palette is active.
pub const DEFAULT_CHART_COLORS: [&str; 8] = [
    "#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#a855f7", "#06b6d4", "#f97316", "#84cc16",
];

/// Inputs besides the document.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Template id (a slug of the graphic name).
    pub template_id: String,
    /// Colors of the active palette, for chart points without a color.
    pub palette_colors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateExport {
    pub template_id: String,
    pub version: String,
    pub spx_definition: SpxDefinition,
    pub ionboard_hook: BoardHook,
    pub ionboard_panel: BoardPanel,
    pub scene: ExportScene,
    pub fields: Vec<Field>,
    pub elements: Vec<Element>,
    pub cues: Vec<SceneCue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_js: Option<String>,
}

/// SPX template definition values (all strings, as SPX expects).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpxDefinition {
    pub description: String,
    pub playserver: String,
    pub playchannel: String,
    pub playlayer: String,
    pub webplayout: String,
    pub prevent_toggle: bool,
    pub out: String,
    pub dataformat: String,
    pub uicolor: String,
}

/// BOARD live-data polling.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardHook {
    pub enabled: bool,
    pub project_id_field: String,
    pub magic_id_field: String,
    pub api_base: String,
    pub poll_ms: f64,
    pub magic_id_default: String,
    pub project_id_default: String,
}

/// BOARD control panel shown inside SPX.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPanel {
    pub enabled: bool,
    pub field: String,
    pub title: String,
    pub aspect: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportScene {
    pub width: f64,
    pub height: f64,
    pub duration_ms: f64,
    pub fps: f64,
}

/// A field the operator fills in (SPX / OGraf).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: String,
    /// `text` or `filelist`.
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    pub default: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assetfolder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

/// One exported layer. Type-specific fields are present only for that type.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub modifiers: Vec<Modifier>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape_type: Option<ShapeType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_points: Option<Vec<ChartPoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_cutout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_bar_thickness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_line_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_point_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_line_tension: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_show_legend: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_show_values: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_value_sort: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_variable_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_fg_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_bg_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_error_correction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_margin: Option<f64>,

    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub particles: Option<ParticleFields>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub lightning: Option<LightningFields>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<PatternFields>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub lottie_start_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lottie_loop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_start_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_loop: Option<bool>,

    /// Filled from a field at playout.
    pub dynamic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility_variable_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility_invert: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_tint: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_variable_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_asset_folder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_fit: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ElementText>,
    pub style: Style,
    pub layout: Layout,
    /// Animated property → keyframes (enabled lanes only, easing resolved).
    pub timeline: BTreeMap<String, Vec<ExportKeyframe>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementText {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_id: Option<String>,
    pub dynamic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportKeyframe {
    pub time: f64,
    pub value: f64,
    pub easing: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub pivot_x: f64,
    pub pivot_y: f64,
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub bg_color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_type: Option<String>,
    pub blend_mode: String,
    pub gradient_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_angle: Option<f64>,
    pub gradient_center: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_ramp: Option<Vec<RampColor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_image_url: Option<String>,
    pub bg_image_fit: String,
    pub bg_image_position: String,
    pub bg_image_repeat: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon_points: Option<Vec<PolygonPoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon_curve: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_keep_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    pub border_width: f64,
    pub border_width_mode: String,
    pub border_widths: [f64; 4],
    pub border_color: String,
    pub border_radius: [f64; 4],
    pub shadow: Shadow,
}

impl Default for Point {
    fn default() -> Self {
        Point {
            x: 0.5,
            y: 0.5,
            extra: Default::default(),
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow {
            color: String::new(),
            x: 0.0,
            y: 0.0,
            blur: 0.0,
            extra: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleFields {
    pub particle_emitting: bool,
    pub particle_emitter_x: f64,
    pub particle_emitter_y: f64,
    pub particle_emitter_radius: f64,
    pub particle_emitter_width: f64,
    pub particle_emitter_height: f64,
    pub particle_emitter_shape: String,
    pub particle_emit_angle_deg: f64,
    pub particle_spawn_rate: f64,
    pub particle_max_count: f64,
    pub particle_lifetime_ms: f64,
    pub particle_size: f64,
    pub particle_size_end: f64,
    pub particle_size_ramp: Vec<RampValue>,
    pub particle_speed: f64,
    pub particle_spread_deg: f64,
    pub particle_gravity_x: f64,
    pub particle_gravity_y: f64,
    pub particle_rotation_deg: f64,
    pub particle_rotation_random_deg: f64,
    pub particle_spin_deg: f64,
    pub particle_align_to_velocity: bool,
    pub particle_color_start: String,
    pub particle_color_end: String,
    pub particle_color_ramp: Vec<RampColor>,
    pub particle_color_mode: String,
    pub particle_palette_colors: Vec<String>,
    pub particle_alpha_ramp: Vec<RampValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub particle_sprite_url: Option<String>,
    pub particle_blend: String,
    pub particle_attach: String,
    pub particle_seed: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightningFields {
    pub lightning_emitting: bool,
    pub lightning_ax: f64,
    pub lightning_ay: f64,
    pub lightning_bx: f64,
    pub lightning_by: f64,
    pub lightning_segments: f64,
    pub lightning_amplitude: f64,
    pub lightning_width: f64,
    pub lightning_glow_width: f64,
    pub lightning_color: String,
    pub lightning_glow_color: String,
    pub lightning_branches: f64,
    pub lightning_seed: f64,
    pub lightning_flicker_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternFields {
    pub pattern_kind: String,
    pub pattern_color_a: String,
    pub pattern_color_b: String,
    pub pattern_cell_size: f64,
    pub pattern_stripe_width: f64,
    pub pattern_stripe_gap: f64,
    pub pattern_stripe_angle_deg: f64,
    pub pattern_dot_radius: f64,
    pub pattern_line_width: f64,
    pub pattern_phase: f64,
}

/// `my_field-id` → `My Field Id`.
fn title_case(input: &str) -> String {
    let spaced: String = input
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let mut out = String::new();
    let mut prev_word = false;
    for c in words.join(" ").chars() {
        let word = c.is_ascii_alphanumeric() || c == '_';
        out.push(if word && !prev_word {
            c.to_ascii_uppercase()
        } else {
            c
        });
        prev_word = word;
    }
    out
}

fn trimmed(v: &Option<String>) -> String {
    v.as_deref().map(js::trim).unwrap_or("").to_string()
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// `./images/` style folder with a trailing slash.
fn image_asset_folder(v: &Option<String>) -> String {
    let s = trimmed(v);
    if s.is_empty() {
        "./images/".into()
    } else if s.ends_with('/') {
        s
    } else {
        format!("{s}/")
    }
}

fn image_extension(v: &Option<String>) -> String {
    let s = trimmed(v);
    let s = s.strip_prefix('.').unwrap_or(&s);
    if s.is_empty() { "png".into() } else { s.into() }
}

/// SPX / OGraf field id: letters, digits and `_`.
fn sanitize_field_id(v: &Option<String>) -> String {
    trimmed(v)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn one_of(v: &Option<String>, allowed: &[&str], default: &str) -> String {
    match v.as_deref() {
        Some(s) if allowed.contains(&s) => s.into(),
        _ => default.into(),
    }
}

/// BOARD page for the SPX panel: the project's play page when known,
/// otherwise the gate page.
pub fn board_panel_url(p: &Profile, magic_id: &str, project_id: &str) -> String {
    let magic = js::trim(magic_id);
    let project = js::trim(project_id);
    let base = if project.is_empty() {
        p.board_panel_gate_url.clone()
    } else {
        format!("{}{}", p.board_play_base, js::encode_uri_component(project))
    };
    if magic.is_empty() {
        base
    } else {
        format!("{base}?magic={}", js::form_urlencode(magic))
    }
}

fn is_enabled(track: &ItemTrack, prop: &str) -> bool {
    match track.enabled_properties.as_ref().and_then(|e| e.get(prop)) {
        Some(flag) => *flag,
        None => track.properties.get(prop).is_some_and(|k| !k.is_empty()),
    }
}

fn element_timeline(item_id: &str, tl: &Timeline) -> BTreeMap<String, Vec<ExportKeyframe>> {
    let Some(track) = tl.item_tracks.iter().find(|t| t.item_id == item_id) else {
        return BTreeMap::new();
    };
    track
        .properties
        .iter()
        .filter(|(prop, keys)| {
            is_enabled(track, prop) && prop.as_str() != CLIP_START && !keys.is_empty()
        })
        .map(|(prop, keys)| {
            let keys = keys
                .iter()
                .map(|k| ExportKeyframe {
                    time: k.time,
                    value: k.value,
                    easing: easing::normalize(k.easing.as_deref()).into(),
                })
                .collect();
            (prop.clone(), keys)
        })
        .collect()
}

fn clip_start_ms(item_id: &str, tl: &Timeline, fallback: f64) -> f64 {
    let first = tl
        .item_tracks
        .iter()
        .find(|t| t.item_id == item_id)
        .and_then(|t| t.properties.get(CLIP_START))
        .and_then(|k| k.first());
    match first {
        Some(k) => js::round(js::or(Some(k.time), 0.0)).max(0.0),
        None => js::round(fallback).max(0.0),
    }
}

/// The element type FRAME exports for an item. Items saved before types
/// existed count as text when they have text, image when they have media.
fn export_kind(item: &Item) -> &'static str {
    let legacy_text = item.item_type.is_none()
        && (!trimmed(&item.text_variable_id).is_empty() || !trimmed(&item.text_content).is_empty());
    match item.item_type {
        _ if legacy_text => "text",
        Some(ItemType::Text) => "text",
        Some(ItemType::Chart) => "chart",
        Some(ItemType::Qr) => "qr",
        Some(ItemType::Particles) => "particles",
        Some(ItemType::Lightning) => "lightning",
        Some(ItemType::Pattern) => "pattern",
        Some(ItemType::Lottie) => "lottie",
        Some(ItemType::Video) => "video",
        Some(ItemType::Empty) => "empty",
        Some(ItemType::ImageSlot) => "imageSlot",
        Some(ItemType::Image) => "image",
        None if item.media_url.is_some() => "image",
        _ => "shape",
    }
}

pub fn build(doc: &Document, opts: &ExportOptions, p: &Profile) -> TemplateExport {
    let tl = &doc.timeline;
    let template_id = opts.template_id.clone();

    // Visible items, plus mask and path-follow sources they use (those are
    // needed even when hidden).
    let mut export_ids: Vec<&str> = Vec::new();
    for item in doc.items.iter().filter(|i| i.visible != Some(false)) {
        export_ids.push(&item.id);
        for m in item.modifiers.iter().flatten() {
            let source = match m {
                Modifier::PathFollow(m) if m.enabled => Some(m.source_element_id.as_str()),
                Modifier::Mask(m) if m.enabled => m.source_element_id.as_deref(),
                _ => None,
            };
            if let Some(src) = source.map(js::trim).filter(|s| !s.is_empty()) {
                export_ids.push(src);
            }
        }
    }

    let mut fields: Vec<Field> = Vec::new();
    let mut elements = Vec::new();
    for item in doc
        .items
        .iter()
        .filter(|i| export_ids.contains(&i.id.as_str()))
    {
        elements.push(build_element(item, tl, opts, p, &mut fields));
    }

    let s = &doc.scene;
    let prevent_toggle = s.spx_prevent_toggle != Some(false);
    let mut playlayer = js::or(s.spx_playlayer.or(Some(2.0)), 2.0).max(1.0);
    let mut webplayout = js::or(s.spx_webplayout.or(Some(2.0)), 2.0).max(1.0);
    // SPX bug: with preventToggle on, Stop on layer 1 misses the instance.
    if prevent_toggle && (playlayer == 1.0 || webplayout == 1.0) {
        playlayer = 2.0;
        webplayout = 2.0;
    }
    let out = match &s.spx_out {
        Some(SpxOut::Text(t)) if t == "manual" => "manual".to_string(),
        Some(SpxOut::Text(t)) => {
            js::number_to_string(js::or(Some(js::str_to_number(t)), 0.0).max(0.0))
        }
        Some(SpxOut::Millis(m)) => js::number_to_string(js::or(Some(*m), 0.0).max(0.0)),
        None => "0".into(),
    };
    let magic = trimmed(&s.ionboard_magic_id);
    let project = trimmed(&s.ionboard_project_id);
    let hook = s.ionboard_hook_enabled == Some(true);

    TemplateExport {
        template_id: template_id.clone(),
        version: p.version.clone(),
        spx_definition: SpxDefinition {
            description: s
                .spx_description
                .clone()
                .unwrap_or_else(|| p.fallback_spx_description.replace("{id}", &template_id)),
            playserver: s.spx_playserver.clone().unwrap_or_else(|| "OVERLAY".into()),
            playchannel: js::number_to_string(
                js::or(s.spx_playchannel.or(Some(1.0)), 1.0).max(1.0),
            ),
            playlayer: js::number_to_string(playlayer),
            webplayout: js::number_to_string(webplayout),
            prevent_toggle,
            out,
            dataformat: s.spx_dataformat.clone().unwrap_or_else(|| "json".into()),
            uicolor: js::number_to_string(js::clamp(
                js::or(s.spx_uicolor.or(Some(1.0)), 1.0),
                0.0,
                7.0,
            )),
        },
        ionboard_hook: BoardHook {
            enabled: hook,
            project_id_field: "ionboard_project_id".into(),
            magic_id_field: "ionboard_magic_id".into(),
            api_base: p.board_api_base.clone(),
            poll_ms: 500.0,
            magic_id_default: magic.clone(),
            project_id_default: project.clone(),
        },
        ionboard_panel: BoardPanel {
            enabled: hook && s.ionboard_panel_enabled == Some(true),
            field: "ionboard_panel".into(),
            title: p.board_panel_title.clone(),
            aspect: "16:9".into(),
            url: board_panel_url(p, &magic, &project),
        },
        scene: ExportScene {
            width: s.width,
            height: s.height,
            duration_ms: tl.duration,
            fps: tl.fps,
        },
        fields,
        elements,
        cues: tl.scene_cues.clone().unwrap_or_default(),
        template_js: (!js::trim(&doc.template_js).is_empty()).then(|| doc.template_js.clone()),
    }
}

fn build_element(
    item: &Item,
    tl: &Timeline,
    opts: &ExportOptions,
    p: &Profile,
    fields: &mut Vec<Field>,
) -> Element {
    let kind = export_kind(item);
    let is_text = kind == "text";
    let text_field = if is_text {
        trimmed(&item.text_variable_id)
    } else {
        String::new()
    };
    let qr_field = if kind == "qr" {
        trimmed(&item.qr_variable_id)
    } else {
        String::new()
    };
    let image_field = if kind == "imageSlot" {
        sanitize_field_id(&item.image_variable_id)
    } else {
        String::new()
    };
    let dynamic = (is_text && !text_field.is_empty()) || (kind == "qr" && !qr_field.is_empty());
    let qr_content = item
        .qr_content
        .clone()
        .unwrap_or_else(|| p.default_qr_content.clone());

    let mut add_field = |f: Field| {
        if !fields.iter().any(|e| e.id == f.id) {
            fields.push(f);
        }
    };
    if is_text && !text_field.is_empty() {
        add_field(Field {
            id: text_field.clone(),
            kind: "text".into(),
            label: title_case(&text_field),
            default: item.text_content.clone().unwrap_or_default(),
            required: false,
            assetfolder: None,
            extension: None,
        });
    }
    if kind == "qr" && !qr_field.is_empty() {
        add_field(Field {
            id: qr_field.clone(),
            kind: "text".into(),
            label: title_case(&qr_field),
            default: qr_content.clone(),
            required: false,
            assetfolder: None,
            extension: None,
        });
    }
    if kind == "imageSlot" && !image_field.is_empty() {
        let folder = image_asset_folder(&item.image_asset_folder);
        let ext = image_extension(&item.image_extension);
        match fields.iter_mut().find(|f| f.id == image_field) {
            None => fields.push(Field {
                id: image_field.clone(),
                kind: "filelist".into(),
                label: title_case(&image_field),
                default: non_empty(trimmed(&item.image_default_value))
                    .unwrap_or_else(|| format!("{folder}default.{ext}")),
                required: false,
                assetfolder: Some(folder),
                extension: Some(ext),
            }),
            // The same id was registered as text first: a file list wins.
            Some(f) if f.kind == "text" => {
                f.kind = "filelist".into();
                f.assetfolder = Some(folder);
                f.extension = Some(ext);
            }
            Some(_) => {}
        }
    }

    let mut el = Element {
        id: item.id.clone(),
        parent_id: item.parent_id.clone().flatten(),
        modifiers: item.modifiers.clone().unwrap_or_default(),
        kind: kind.into(),
        shape_type: (kind == "shape").then(|| item.shape_type.unwrap_or(ShapeType::Box)),
        dynamic: dynamic || (kind == "imageSlot" && !image_field.is_empty()),
        field_id: [
            (is_text, &text_field),
            (kind == "qr", &qr_field),
            (kind == "imageSlot", &image_field),
        ]
        .into_iter()
        .find(|(on, id)| *on && !id.is_empty())
        .map(|(_, id)| id.clone()),
        text_default: is_text.then(|| item.text_content.clone().unwrap_or_default()),
        media_url: item.media_url.clone(),
        text: is_text.then(|| ElementText {
            value: item.text_content.clone().unwrap_or_default(),
            variable_id: dynamic.then(|| text_field.clone()),
            dynamic,
        }),
        style: style(item),
        layout: Layout {
            x: js::finite_or(Some(item.x), 0.0),
            y: js::finite_or(Some(item.y), 0.0),
            width: js::finite_or(Some(item.width), 0.0),
            height: js::finite_or(Some(item.height), 0.0),
            rotation: js::finite_or(Some(item.rotation), 0.0),
            scale_x: js::finite_or(item.scale_x, 1.0),
            scale_y: js::finite_or(item.scale_y, 1.0),
            pivot_x: js::finite_or(item.pivot_x, 50.0),
            pivot_y: js::finite_or(item.pivot_y, 50.0),
            opacity: js::finite_or(Some(item.opacity), 1.0),
        },
        timeline: element_timeline(&item.id, tl),
        ..Default::default()
    };

    let visibility = trimmed(&item.visibility_variable_id);
    if !visibility.is_empty() {
        el.visibility_variable_id = Some(visibility);
        el.visibility_invert = Some(item.visibility_invert == Some(true));
    }
    if kind == "image"
        && (item.svg_tint == Some(true) || is_basic_svg_stock_url(item.media_url.as_deref()))
    {
        el.svg_tint = Some(true);
    }

    match kind {
        "chart" => chart_fields(item, opts, &mut el),
        "qr" => {
            el.qr_content = Some(if qr_content.is_empty() {
                p.default_qr_content.clone()
            } else {
                qr_content
            });
            el.qr_variable_id = non_empty(qr_field);
            el.qr_fg_color =
                Some(non_empty(trimmed(&item.qr_fg_color)).unwrap_or_else(|| "#000000".into()));
            el.qr_bg_color =
                Some(non_empty(trimmed(&item.qr_bg_color)).unwrap_or_else(|| "#ffffff".into()));
            el.qr_error_correction = Some(crate::normalize::normalize_qr_error_correction(
                item.qr_error_correction.as_deref(),
            ));
            el.qr_margin = Some(crate::normalize::normalize_qr_margin(item.qr_margin));
        }
        "particles" => el.particles = Some(particle_fields(item)),
        "lightning" => el.lightning = Some(lightning_fields(item)),
        "pattern" => el.pattern = Some(pattern_fields(item)),
        "lottie" => {
            el.lottie_start_ms = Some(clip_start_ms(
                &item.id,
                tl,
                js::or(item.lottie_start_ms, 0.0),
            ));
            el.lottie_loop = Some(item.lottie_loop == Some(true));
        }
        "video" => {
            el.video_start_ms = Some(clip_start_ms(
                &item.id,
                tl,
                js::or(item.video_start_ms, 0.0),
            ));
            el.video_loop = Some(item.video_loop == Some(true));
        }
        "imageSlot" => {
            el.image_variable_id = non_empty(image_field);
            el.image_asset_folder = Some(image_asset_folder(&item.image_asset_folder));
            el.image_extension = Some(image_extension(&item.image_extension));
            el.image_default_value = non_empty(trimmed(&item.image_default_value));
            el.image_fit = Some(one_of(&item.image_fit, &["contain"], "cover"));
        }
        _ => {}
    }
    el
}

fn chart_fields(item: &Item, opts: &ExportOptions, el: &mut Element) {
    let kind = one_of(&item.chart_kind, &["pie", "line"], "bar");
    let points = item.chart_points.clone().unwrap_or_default();
    let palette: Vec<&str> = if opts.palette_colors.is_empty() {
        DEFAULT_CHART_COLORS.to_vec()
    } else {
        opts.palette_colors.iter().map(String::as_str).collect()
    };
    el.chart_points = Some(
        points
            .iter()
            .enumerate()
            .map(|(i, pt)| ChartPoint {
                id: pt.id.clone(),
                label: pt.label.clone(),
                value: if pt.value.is_finite() { pt.value } else { 0.0 },
                color: Some(
                    non_empty(trimmed(&pt.color))
                        .unwrap_or_else(|| palette[i % palette.len()].to_string()),
                ),
                live_variable_id: non_empty(trimmed(&pt.live_variable_id)),
                extra: Default::default(),
            })
            .collect(),
    );
    let clamp_round = |v: Option<f64>, d: f64, lo: f64, hi: f64| {
        v.filter(|x| x.is_finite())
            .map_or(d, |x| js::clamp(js::round(x), lo, hi))
    };
    el.chart_cutout = Some(clamp_round(item.chart_cutout, 0.0, 0.0, 80.0));
    el.chart_bar_thickness = Some(clamp_round(item.chart_bar_thickness, 80.0, 10.0, 100.0));
    el.chart_line_width = Some(clamp_round(item.chart_line_width, 2.0, 1.0, 16.0));
    el.chart_point_radius = Some(clamp_round(item.chart_point_radius, 4.0, 0.0, 20.0));
    el.chart_line_tension = Some(
        item.chart_line_tension
            .filter(|x| x.is_finite())
            .map_or(0.2, |x| js::clamp(js::round(x * 100.0) / 100.0, 0.0, 1.0)),
    );
    el.chart_show_legend = Some(kind == "pie" && item.chart_show_legend.unwrap_or(true));
    el.chart_show_values = Some(kind == "bar" && item.chart_show_values.unwrap_or(true));
    el.chart_value_sort = Some(one_of(&item.chart_value_sort, &["asc", "desc"], "none"));
    el.chart_kind = Some(kind);
}

fn particle_fields(item: &Item) -> ParticleFields {
    let or = js::or;
    let size = or(item.particle_size, 8.0);
    let size_end = or(item.particle_size_end, 2.0);
    let clamp01 = |v: f64| js::clamp(v, 0.0, 1.0);
    let color_start = item
        .particle_color_start
        .clone()
        .unwrap_or_else(|| "rgba(255, 255, 255, 1)".into());
    let color_end = item
        .particle_color_end
        .clone()
        .unwrap_or_else(|| "rgba(255, 255, 255, 0)".into());
    ParticleFields {
        particle_emitting: item.particle_emitting != Some(false),
        particle_emitter_x: or(item.particle_emitter_x, 0.0),
        particle_emitter_y: or(item.particle_emitter_y, 0.0),
        particle_emitter_radius: or(item.particle_emitter_radius, 0.0),
        particle_emitter_width: or(item.particle_emitter_width, 80.0),
        particle_emitter_height: or(item.particle_emitter_height, 48.0),
        particle_emitter_shape: one_of(&item.particle_emitter_shape, &["box"], "circle"),
        particle_emit_angle_deg: js::finite_or(item.particle_emit_angle_deg, -90.0),
        particle_spawn_rate: or(item.particle_spawn_rate, 0.0),
        particle_max_count: or(item.particle_max_count, 300.0),
        particle_lifetime_ms: or(item.particle_lifetime_ms, 1800.0),
        particle_size: size,
        particle_size_end: size_end,
        particle_size_ramp: match &item.particle_size_ramp {
            Some(r) => r
                .iter()
                .map(|s| {
                    crate::ramp::value(or(Some(s.offset), 0.0), clamp01(or(Some(s.value), 0.0)))
                })
                .collect(),
            None => vec![
                crate::ramp::value(0.0, 1.0),
                crate::ramp::value(1.0, clamp01(size_end / size.max(0.01))),
            ],
        },
        particle_speed: or(item.particle_speed, 80.0),
        particle_spread_deg: or(item.particle_spread_deg, 360.0),
        particle_gravity_x: or(item.particle_gravity_x, 0.0),
        particle_gravity_y: or(item.particle_gravity_y, 0.0),
        particle_rotation_deg: or(item.particle_rotation_deg, 0.0),
        particle_rotation_random_deg: or(item.particle_rotation_random_deg, 0.0).max(0.0),
        particle_spin_deg: or(item.particle_spin_deg, 0.0),
        particle_align_to_velocity: item.particle_align_to_velocity == Some(true),
        particle_color_ramp: match &item.particle_color_ramp {
            Some(r) => r
                .iter()
                .map(|s| crate::ramp::color(or(Some(s.offset), 0.0), &s.color))
                .collect(),
            None => vec![
                crate::ramp::color(0.0, &color_start),
                crate::ramp::color(1.0, &color_end),
            ],
        },
        particle_color_start: color_start,
        particle_color_end: color_end,
        particle_color_mode: one_of(&item.particle_color_mode, &["paletteCycle"], "ramp"),
        particle_palette_colors: item
            .particle_palette_colors
            .iter()
            .flatten()
            .filter(|c| !js::trim(c).is_empty())
            .cloned()
            .collect(),
        particle_alpha_ramp: match &item.particle_alpha_ramp {
            Some(r) => r
                .iter()
                .map(|s| {
                    crate::ramp::value(or(Some(s.offset), 0.0), clamp01(or(Some(s.value), 0.0)))
                })
                .collect(),
            None => vec![crate::ramp::value(0.0, 1.0), crate::ramp::value(1.0, 0.0)],
        },
        particle_sprite_url: non_empty(trimmed(&item.particle_sprite_url)),
        particle_blend: one_of(&item.particle_blend, &["normal"], "additive"),
        particle_attach: one_of(&item.particle_attach, &["relative"], "absolute"),
        particle_seed: or(item.particle_seed, 1.0),
    }
}

fn lightning_fields(item: &Item) -> LightningFields {
    let or = js::or;
    LightningFields {
        lightning_emitting: item.lightning_emitting != Some(false),
        lightning_ax: js::round(or(item.lightning_ax, 0.0)),
        lightning_ay: js::round(or(item.lightning_ay, 0.0)),
        lightning_bx: js::round(or(item.lightning_bx, 0.0)),
        lightning_by: js::round(or(item.lightning_by, 0.0)),
        lightning_segments: or(item.lightning_segments, 14.0),
        lightning_amplitude: or(item.lightning_amplitude, 0.0),
        lightning_width: or(item.lightning_width, 2.0),
        lightning_glow_width: or(item.lightning_glow_width, 0.0),
        lightning_color: non_empty(trimmed(&item.lightning_color))
            .unwrap_or_else(|| "#e0e7ff".into()),
        lightning_glow_color: non_empty(trimmed(&item.lightning_glow_color))
            .unwrap_or_else(|| "#6366f1".into()),
        lightning_branches: or(item.lightning_branches, 0.0),
        lightning_seed: or(item.lightning_seed, 1.0),
        lightning_flicker_ms: or(item.lightning_flicker_ms, 48.0),
    }
}

fn pattern_fields(item: &Item) -> PatternFields {
    let or = js::or;
    let fin = |v: Option<f64>| v.filter(|x| x.is_finite());
    PatternFields {
        pattern_kind: one_of(&item.pattern_kind, &["stripes", "dots", "grid"], "checker"),
        pattern_color_a: non_empty(trimmed(&item.pattern_color_a))
            .unwrap_or_else(|| "#1e293b".into()),
        pattern_color_b: non_empty(trimmed(&item.pattern_color_b))
            .unwrap_or_else(|| "#334155".into()),
        pattern_cell_size: js::round(or(item.pattern_cell_size, 32.0)).max(2.0),
        pattern_stripe_width: js::round(or(item.pattern_stripe_width, 16.0)).max(1.0),
        pattern_stripe_gap: js::round(or(item.pattern_stripe_gap, 16.0)).max(0.0),
        pattern_stripe_angle_deg: fin(item.pattern_stripe_angle_deg)
            .map_or(45.0, |v| js::round(v * 10.0) / 10.0),
        pattern_dot_radius: or(item.pattern_dot_radius, 6.0).max(0.5),
        pattern_line_width: or(item.pattern_line_width, 2.0).max(0.5),
        pattern_phase: fin(item.pattern_phase).map_or(0.0, |v| js::round(v * 100.0) / 100.0),
    }
}

fn style(item: &Item) -> Style {
    let gc = item.gradient_center.as_ref();
    let center = |v: Option<f64>| {
        v.filter(|x| x.is_finite())
            .map_or(0.5, |x| x.clamp(0.0, 1.0))
    };
    let sides = item.border_width_mode.as_deref() == Some("sides");
    Style {
        bg_color: item.bg_color.clone(),
        background_type: item.background_type.clone(),
        blend_mode: item.blend_mode.clone().unwrap_or_else(|| "normal".into()),
        gradient_type: one_of(
            &item.gradient_type,
            &["circle", "ellipse", "conic"],
            "linear",
        ),
        gradient_angle: item.gradient_angle,
        gradient_center: Point {
            x: center(gc.map(|c| c.x)),
            y: center(gc.map(|c| c.y)),
            extra: Default::default(),
        },
        gradient_start: item.gradient_start.clone(),
        gradient_end: item.gradient_end.clone(),
        gradient_ramp: item.gradient_ramp.as_ref().map(|r| {
            r.iter()
                .map(|s| crate::ramp::color(s.offset, &s.color))
                .collect()
        }),
        bg_image_url: non_empty(trimmed(&item.bg_image_url)),
        bg_image_fit: one_of(&item.bg_image_fit, &["contain", "stretch"], "cover"),
        bg_image_position: one_of(
            &item.bg_image_position,
            &[
                "top",
                "bottom",
                "left",
                "right",
                "top-left",
                "top-right",
                "bottom-left",
                "bottom-right",
            ],
            "center",
        ),
        bg_image_repeat: one_of(
            &item.bg_image_repeat,
            &["repeat", "repeat-x", "repeat-y"],
            "no-repeat",
        ),
        polygon_points: item.polygon_points.clone(),
        polygon_curve: item.polygon_curve.clone(),
        path_keep_visible: (item.path_keep_visible == Some(true)).then_some(true),
        font_family: item.font_family.clone(),
        font_size: item.font_size,
        font_weight: item.font_weight,
        text_color: item.text_color.clone(),
        text_align: item.text_align.clone(),
        border_width: item.border_width,
        border_width_mode: if sides { "sides" } else { "single" }.into(),
        border_widths: match (sides, item.border_widths) {
            (true, Some(w)) => w.map(|v| js::or(Some(v), 0.0)),
            _ => [item.border_width; 4],
        },
        border_color: item.border_color.clone(),
        border_radius: item.border_radius,
        shadow: item.shadow.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_cases_field_ids() {
        assert_eq!(title_case("home_team-name"), "Home Team Name");
        assert_eq!(title_case("  score__1 "), "Score 1");
        // JavaScript treats `ä` as a non-word character, so `b` starts a word.
        assert_eq!(title_case("äbc"), "äBc");
    }

    #[test]
    fn board_urls() {
        let p = Profile::frame();
        assert_eq!(board_panel_url(&p, "", ""), "https://board.meniac.tech/spx");
        assert_eq!(
            board_panel_url(&p, "mb_1 2", ""),
            "https://board.meniac.tech/spx?magic=mb_1+2"
        );
        assert_eq!(
            board_panel_url(&p, "m", "my proj"),
            "https://board.meniac.tech/play/my%20proj?magic=m"
        );
    }

    #[test]
    fn image_slot_helpers() {
        assert_eq!(image_asset_folder(&Some("media".into())), "media/");
        assert_eq!(image_asset_folder(&None), "./images/");
        assert_eq!(image_extension(&Some(".jpg".into())), "jpg");
        assert_eq!(
            sanitize_field_id(&Some(" team-logo 1".into())),
            "team_logo_1"
        );
    }
}
