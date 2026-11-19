// SPDX-License-Identifier: AGPL-3.0-or-later
//! The exported HTML page: layer markup, the data object the runtime reads
//! (`window.__ionframeSpx`) and the SPX or banner page around them.
//! A port of FRAME's `spxDraft.ts`.
//!
//! Fixed script blocks and page skeletons live in `html/*.in`. They were
//! taken from FRAME's source by evaluating its template literals, so they
//! match FRAME's output exactly; `@NAME@` markers are filled here.

use super::css::*;
use super::manifest::{ograf_step_count, pause_times_ms};
use super::template::{Element, TemplateExport, board_panel_url};
use crate::js::{self, number_to_string as n};
use crate::model::{Modifier, PolygonPoint, ShapeType};
use crate::profile::Profile;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const SPX_PAGE: &str = include_str!("html/spx_page.html.in");
const BANNER_PAGE: &str = include_str!("html/banner_page.html.in");
const CHART_SCRIPT: &str = include_str!("html/chart.html.in");
const QR_SCRIPT: &str = include_str!("html/qr.html.in");
const LOTTIE_SCRIPT: &str = include_str!("html/lottie.html.in");
const VIDEO_SCRIPT: &str = include_str!("html/video.html.in");
const MODULE_INIT_SCRIPT: &str = include_str!("html/module_init.html.in");
const BOARD_SCRIPT: &str = include_str!("html/ionboard_hook.html.in");
const ANIME_EASE_KEYS: &str = include_str!("html/anime_ease_keys.json");

/// `js/spx_interface.js`: the SPX bridge (`update`, `play`, `stop`, `next`).
pub const SPX_INTERFACE_JS: &str = include_str!("html/spx_interface.js");

/// Which page to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    /// SPX template: waits for play, carries the SPX template definition.
    Spx,
    /// Self-contained banner: plays and loops on its own.
    Banner,
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// DOM id prefix of an element type (`shp_<id>`, `txt_<id>`, …).
pub fn dom_prefix(kind: &str) -> &'static str {
    match kind {
        "shape" => "shp",
        "text" => "txt",
        "chart" => "cht",
        "qr" => "qr",
        "particles" => "prt",
        "lightning" => "ltn",
        "pattern" => "pat",
        "lottie" => "lot",
        "video" => "vid",
        "empty" => "emp",
        "imageSlot" => "isl",
        _ => "img",
    }
}

/// Position, size, opacity, blend and transform: shared by every layer.
fn frame_css(el: &Element) -> String {
    let l = &el.layout;
    format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;opacity:{};{}transform-origin:{}% {}%;transform:rotate({}deg) scale({}, {});",
        n(l.x),
        n(l.y),
        n(l.width),
        n(l.height),
        n(l.opacity),
        blend_mode_css(Some(&el.style.blend_mode)),
        n(l.pivot_x),
        n(l.pivot_y),
        n(l.rotation),
        n(l.scale_x),
        n(l.scale_y)
    )
}

fn border_css(el: &Element) -> String {
    let s = &el.style;
    if any_border(s.border_width, &s.border_width_mode, Some(s.border_widths)) {
        format!(
            "border-style:solid;border-width:{};border-color:{}",
            border_width_css(s.border_width, &s.border_width_mode, Some(s.border_widths)),
            s.border_color
        )
    } else {
        "border:none".into()
    }
}

fn radius_css(r: &[f64; 4]) -> String {
    format!("{}px {}px {}px {}px", n(r[0]), n(r[1]), n(r[2]), n(r[3]))
}

fn center(el: &Element) -> (f64, f64) {
    (el.style.gradient_center.x, el.style.gradient_center.y)
}

fn element_fill_ramp(el: &Element) -> Vec<crate::model::RampColor> {
    let s = &el.style;
    fill_ramp(
        s.gradient_ramp.as_deref(),
        s.gradient_start.as_deref(),
        s.gradient_end.as_deref(),
    )
}

fn background_css(el: &Element) -> String {
    let s = &el.style;
    match s.background_type.as_deref() {
        Some("gradient") => {
            let css = gradient_css(
                &element_fill_ramp(el),
                &s.gradient_type,
                s.gradient_angle.unwrap_or(90.0),
                center(el),
            );
            format!(
                "background-image:{css};background-color:transparent;background-repeat:no-repeat;background-size:auto;background-position:initial;"
            )
        }
        Some("image") => format!(
            "background-image:{};background-color:{};background-size:{};background-position:{};background-repeat:{};",
            background_image_url_css(s.bg_image_url.as_deref()),
            s.bg_color,
            bg_image_size_css(&s.bg_image_fit),
            bg_image_position_css(&s.bg_image_position),
            s.bg_image_repeat
        ),
        _ => format!(
            "background-color:{};background-image:none;background-repeat:no-repeat;background-size:auto;background-position:initial;",
            s.bg_color
        ),
    }
}

fn shape_node(el: &Element, children: &str) -> String {
    let s = &el.style;
    let l = &el.layout;
    let frame = format!("{}box-sizing:border-box;", frame_css(el));
    let points: &[PolygonPoint] = s.polygon_points.as_deref().unwrap_or_default();
    let stroke_w = s.border_width.max(0.0);
    let bc = &s.border_color;
    let view_box = format!("0 0 {} {}", n(l.width.max(1.0)), n(l.height.max(1.0)));

    match el.shape_type.unwrap_or(ShapeType::Box) {
        ShapeType::Path => {
            if s.path_keep_visible != Some(true) {
                // Motion path guide only: invisible in playout.
                return format!(
                    "      <div class=\"shapeItem\" id=\"shp_{}\" style=\"{frame}border:none;box-shadow:none;background:none;opacity:0;pointer-events:none;\" data-ion-path-guide=\"1\">{children}</div>",
                    el.id
                );
            }
            let d = open_path_d(points, Some(s.polygon_curve.as_deref().unwrap_or("bezier")));
            let stroke = if !bc.is_empty() && bc != "transparent" {
                bc.as_str()
            } else {
                "rgba(56, 189, 248, 0.95)"
            };
            let sw = js::or(Some(stroke_w), 2.0).max(1.0);
            format!(
                "      <div class=\"shapeItem\" id=\"shp_{}\" style=\"{frame}border:none;box-shadow:none;background:none;pointer-events:none;\" data-ion-path-graphic=\"1\"><svg width=\"{}\" height=\"{}\" viewBox=\"{view_box}\" preserveAspectRatio=\"none\" style=\"overflow:visible;display:block;\"><path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" vector-effect=\"non-scaling-stroke\"/></svg>{children}</div>",
                el.id,
                n(l.width),
                n(l.height),
                n(sw)
            )
        }
        ShapeType::Polygon => {
            let curve = s.polygon_curve.as_deref();
            let bezier = curve == Some("bezier");
            let d = polygon_path_d(points, curve, l.width, l.height);
            let attr = polygon_points_attr(points, l.width, l.height);
            let kind = s.gradient_type.as_str();
            let angle = s.gradient_angle.unwrap_or(90.0);
            let rad = angle * std::f64::consts::PI / 180.0;
            let (dx, dy) = (rad.sin(), -rad.cos());
            let grad_id = format!("poly_grad_{}", el.id);
            let ramp = element_fill_ramp(el);
            let stops: String = svg_stops(&ramp)
                .into_iter()
                .map(|(offset, color)| {
                    format!("<stop offset=\"{offset}\" stop-color=\"{color}\" />")
                })
                .collect();
            let gradient = s.background_type.as_deref() == Some("gradient");
            let conic = gradient && kind == "conic";
            let fill = if gradient {
                if conic {
                    "transparent".to_string()
                } else {
                    format!("url(#{grad_id})")
                }
            } else {
                s.bg_color.clone()
            };
            let defs = if gradient && !conic {
                if kind == "circle" || kind == "ellipse" {
                    let (units, cx, cy, r) = svg_radial_params(kind, center(el), l.width, l.height);
                    format!(
                        "<defs><radialGradient id=\"{grad_id}\" gradientUnits=\"{units}\" cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\">{stops}</radialGradient></defs>"
                    )
                } else {
                    format!(
                        "<defs><linearGradient id=\"{grad_id}\" x1=\"{}%\" y1=\"{}%\" x2=\"{}%\" y2=\"{}%\">{stops}</linearGradient></defs>",
                        n(50.0 - dx * 50.0),
                        n(50.0 - dy * 50.0),
                        n(50.0 + dx * 50.0),
                        n(50.0 + dy * 50.0)
                    )
                }
            } else {
                String::new()
            };
            let overlay = if conic {
                let css = gradient_css(&ramp, "conic", angle, center(el));
                let clip = if bezier && points.len() >= 3 {
                    path_clip_path(&d)
                } else if points.len() >= 3 {
                    polygon_clip_path(points)
                } else {
                    "none".into()
                };
                format!(
                    "<div style=\"position:absolute;inset:0;pointer-events:none;background-image:{css};background-color:transparent;clip-path:{clip};\" aria-hidden=\"true\"></div>"
                )
            } else {
                String::new()
            };
            let stroke = if stroke_w > 0.0 {
                bc.as_str()
            } else {
                "transparent"
            };
            let body = if bezier {
                format!(
                    "<path d=\"{d}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{}\"></path>",
                    n(stroke_w)
                )
            } else {
                format!(
                    "<polygon points=\"{attr}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{}\"></polygon>",
                    n(stroke_w)
                )
            };
            format!(
                "      <div class=\"shapeItem\" id=\"shp_{}\" style=\"{frame}border:none;box-shadow:none;background:none;\">{overlay}<svg width=\"{}\" height=\"{}\" viewBox=\"{view_box}\" preserveAspectRatio=\"none\" style=\"position:absolute;inset:0;overflow:visible;pointer-events:none;\">{defs}{body}</svg>{children}</div>",
                el.id,
                n(l.width),
                n(l.height)
            )
        }
        shape => {
            let radius = match shape {
                ShapeType::Ellipse | ShapeType::Circle => "50%".to_string(),
                _ => radius_css(&s.border_radius),
            };
            let sh = &s.shadow;
            format!(
                "      <div class=\"shapeItem\" id=\"shp_{}\" style=\"{frame}{}{};border-radius:{radius};box-shadow:{}px {}px {}px {};\">{children}</div>",
                el.id,
                background_css(el),
                border_css(el),
                n(sh.x),
                n(sh.y),
                n(sh.blur),
                sh.color
            )
        }
    }
}

fn text_node(el: &Element, children: &str) -> String {
    let s = &el.style;
    let raw = el
        .text
        .as_ref()
        .map(|t| t.value.clone())
        .or_else(|| el.text_default.clone())
        .unwrap_or_default();
    let text = esc_html(&raw);
    format!(
        "      <div class=\"textItem\" id=\"txt_{}\" data-ion-full-text=\"{}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;font-family:{};font-size:{}px;font-weight:{};color:{};text-align:{};opacity:{};{}transform-origin:{}% {}%;transform:rotate({}deg) scale({}, {});\"><span data-ion-text>{text}</span>{children}</div>",
        el.id,
        text.replace('"', "&quot;"),
        n(el.layout.x),
        n(el.layout.y),
        n(el.layout.width),
        n(el.layout.height),
        s.font_family.as_deref().unwrap_or("Mouse Memoirs"),
        n(s.font_size.unwrap_or(42.0)),
        n(s.font_weight.unwrap_or(400.0)),
        s.text_color.as_deref().unwrap_or("#ffffff"),
        s.text_align.as_deref().unwrap_or("left"),
        n(el.layout.opacity),
        blend_mode_css(Some(&s.blend_mode)),
        n(el.layout.pivot_x),
        n(el.layout.pivot_y),
        n(el.layout.rotation),
        n(el.layout.scale_x),
        n(el.layout.scale_y)
    )
}

fn image_node(el: &Element, children: &str) -> String {
    let media = el.media_url.clone().unwrap_or_default();
    let layout = frame_css(el);
    if el.svg_tint == Some(true) {
        let src = media.replace('"', "&quot;").replace('\'', "\\'");
        let s = &el.style;
        let mask = format!(
            "-webkit-mask-image:url('{src}');mask-image:url('{src}');-webkit-mask-size:100% 100%;mask-size:100% 100%;-webkit-mask-repeat:no-repeat;mask-repeat:no-repeat;-webkit-mask-position:center;mask-position:center;"
        );
        let stroke_w = js::or(Some(s.border_width), 0.0).max(0.0);
        let filter = if stroke_w > 0.0 {
            silhouette_stroke_filter(stroke_w, &s.border_color)
        } else {
            "none".into()
        };
        return format!(
            "      <div class=\"imgItem\" id=\"img_{}\" data-ion-svg-tint=\"1\" style=\"{layout}overflow:visible;\"><div style=\"position:absolute;inset:0;filter:{filter};\"><div data-ion-svg-tint-fill style=\"position:absolute;inset:0;{}{mask}\"></div></div>{children}</div>",
            el.id,
            background_css(el)
        );
    }
    if children.is_empty() {
        return format!(
            "      <img class=\"imgItem\" id=\"img_{}\" src=\"{media}\" style=\"{layout}\" />",
            el.id
        );
    }
    format!(
        "      <div class=\"imgItem\" id=\"img_{}\" style=\"{layout}overflow:hidden;\"><img src=\"{media}\" style=\"position:absolute;inset:0;width:100%;height:100%;object-fit:contain;border:0;\" />{children}</div>",
        el.id
    )
}

fn image_slot_node(el: &Element, children: &str) -> String {
    let s = &el.style;
    let fit = if el.image_fit.as_deref() == Some("contain") {
        "contain"
    } else {
        "cover"
    };
    format!(
        "      <div class=\"imgSlotItem\" id=\"isl_{}\" style=\"{}box-sizing:border-box;overflow:hidden;background-color:{};{};border-radius:{};\"><img data-ion-image-slot alt=\"\" style=\"position:absolute;inset:0;width:100%;height:100%;object-fit:{fit};border:0;display:none;\" />{children}</div>",
        el.id,
        frame_css(el),
        s.bg_color,
        border_css(el),
        radius_css(&s.border_radius)
    )
}

fn chart_node(el: &Element, children: &str) -> String {
    format!(
        "      <div class=\"chartItem\" id=\"cht_{0}\" style=\"{1}box-sizing:border-box;overflow:hidden;{2}{3};border-radius:{4};\"><canvas id=\"cht_canvas_{0}\" style=\"display:block;width:100%;height:100%;\"></canvas>{children}</div>",
        el.id,
        frame_css(el),
        background_css(el),
        border_css(el),
        radius_css(&el.style.border_radius)
    )
}

fn qr_node(el: &Element, children: &str) -> String {
    format!(
        "      <div class=\"qrItem\" id=\"qr_{0}\" style=\"{1}box-sizing:border-box;overflow:hidden;background:transparent;{2};border-radius:{3};\"><canvas id=\"qr_canvas_{0}\" style=\"display:block;margin:0 auto;\"></canvas>{children}</div>",
        el.id,
        frame_css(el),
        border_css(el),
        radius_css(&el.style.border_radius)
    )
}

/// Particles, lightning, patterns and Lottie: a canvas the runtime draws on.
fn canvas_node(el: &Element, class: &str, prefix: &str, children: &str) -> String {
    format!(
        "      <div class=\"{class}\" id=\"{prefix}_{0}\" style=\"{1}box-sizing:border-box;overflow:hidden;background:transparent;\"><canvas id=\"{prefix}_canvas_{0}\" style=\"display:block;width:100%;height:100%;\"></canvas>{children}</div>",
        el.id,
        frame_css(el)
    )
}

fn video_node(el: &Element, children: &str) -> String {
    let src = js::trim(el.media_url.as_deref().unwrap_or("")).replace('"', "&quot;");
    format!(
        "      <div class=\"videoItem\" id=\"vid_{0}\" style=\"{1}box-sizing:border-box;overflow:hidden;background:transparent;\" data-ion-experimental=\"video\"><video id=\"vid_el_{0}\" src=\"{src}\" muted playsinline preload=\"auto\" style=\"display:block;width:100%;height:100%;object-fit:contain;background:transparent;\"></video>{children}</div>",
        el.id,
        frame_css(el)
    )
}

fn empty_node(el: &Element, children: &str) -> String {
    format!(
        "      <div class=\"emptyItem\" id=\"emp_{}\" style=\"{}box-sizing:border-box;overflow:visible;background:transparent;\">{children}</div>",
        el.id,
        frame_css(el)
    )
}

/// Layer markup, nested by parent so a parent's crop and mask also clip its
/// children. Elements whose parent was not exported are left out, as in
/// FRAME.
pub fn layers_html(t: &TemplateExport) -> String {
    let mut children: BTreeMap<Option<&str>, Vec<&Element>> = BTreeMap::new();
    for el in &t.elements {
        children
            .entry(el.parent_id.as_deref())
            .or_default()
            .push(el);
    }
    fn render(el: &Element, children: &BTreeMap<Option<&str>, Vec<&Element>>) -> String {
        let inner: Vec<String> = children
            .get(&Some(el.id.as_str()))
            .into_iter()
            .flatten()
            .map(|c| render(c, children))
            .collect();
        let inner = inner.join("\n");
        match el.kind.as_str() {
            "shape" => shape_node(el, &inner),
            "text" => text_node(el, &inner),
            "image" => image_node(el, &inner),
            "imageSlot" => image_slot_node(el, &inner),
            "chart" => chart_node(el, &inner),
            "qr" => qr_node(el, &inner),
            "particles" => canvas_node(el, "particlesItem", "prt", &inner),
            "lightning" => canvas_node(el, "lightningItem", "ltn", &inner),
            "pattern" => canvas_node(el, "patternItem", "pat", &inner),
            "lottie" => canvas_node(el, "lottieItem", "lot", &inner),
            "video" => video_node(el, &inner),
            "empty" => empty_node(el, &inner),
            _ => String::new(),
        }
    }
    children
        .get(&None)
        .into_iter()
        .flatten()
        .map(|el| render(el, &children))
        .collect::<Vec<_>>()
        .join("\n")
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("export data serializes")
}

/// Per-element animation data for the runtime.
fn animation_track(el: &Element, t: &TemplateExport) -> Value {
    let by_id = |id: &str| t.elements.iter().find(|e| e.id == id);
    let s = &el.style;
    let mut track = Map::new();
    track.insert("id".into(), el.id.clone().into());
    track.insert(
        "parentId".into(),
        el.parent_id.clone().map_or(Value::Null, Value::from),
    );
    track.insert("modifiers".into(), to_value(&el.modifiers));
    track.insert(
        "domId".into(),
        format!("{}_{}", dom_prefix(&el.kind), el.id).into(),
    );

    let mask_src = el.modifiers.iter().find_map(|m| match m {
        Modifier::Mask(m) if m.enabled => m.source_element_id.as_deref().filter(|s| !s.is_empty()),
        _ => None,
    });
    if let Some(src) = mask_src.and_then(by_id) {
        let pts = src.style.polygon_points.as_deref().unwrap_or_default();
        if src.shape_type == Some(ShapeType::Polygon) && pts.len() >= 3 {
            track.insert("maskSourcePolygon".into(), to_value(&pts));
            track.insert(
                "maskSourcePolygonCurve".into(),
                src.style
                    .polygon_curve
                    .as_deref()
                    .unwrap_or("linear")
                    .into(),
            );
        }
    }
    let follow_src = el.modifiers.iter().find_map(|m| match m {
        Modifier::PathFollow(m) if m.enabled && !m.source_element_id.is_empty() => {
            Some(m.source_element_id.as_str())
        }
        _ => None,
    });
    let mut follow_open = false;
    if let Some(src) = follow_src.and_then(by_id) {
        let pts = src.style.polygon_points.as_deref().unwrap_or_default();
        let is_path = src.shape_type == Some(ShapeType::Path);
        if (is_path || src.shape_type == Some(ShapeType::Polygon)) && pts.len() >= 2 {
            track.insert("pathFollowSourceId".into(), src.id.clone().into());
            track.insert("pathFollowSourcePolygon".into(), to_value(&pts));
            let default_curve = if is_path { "bezier" } else { "linear" };
            track.insert(
                "pathFollowSourcePolygonCurve".into(),
                src.style
                    .polygon_curve
                    .as_deref()
                    .unwrap_or(default_curve)
                    .into(),
            );
            follow_open = is_path;
        }
    }
    track.insert("pathFollowSourceIsOpen".into(), follow_open.into());

    let lottie_start = el.lottie_start_ms;
    let video_start = el.video_start_ms;
    let mut base = json!({
        "x": el.layout.x,
        "y": el.layout.y,
        "rotation": el.layout.rotation,
        "scaleX": el.layout.scale_x,
        "scaleY": el.layout.scale_y,
        "pivotX": el.layout.pivot_x,
        "pivotY": el.layout.pivot_y,
        "opacity": el.layout.opacity,
        "borderWidth": js::or(Some(s.border_width), 0.0),
        "borderColor": s.border_color,
        "width": el.layout.width,
        "height": el.layout.height,
        "fontSize": s.font_size.unwrap_or(if el.kind == "chart" { 14.0 } else { 42.0 }),
        "textColor": s.text_color.as_deref().unwrap_or("#ffffff"),
        "bgColor": s.bg_color,
        "backgroundType": s.background_type.as_deref().unwrap_or("solid"),
        "gradientType": s.gradient_type,
        "gradientAngle": s.gradient_angle.unwrap_or(90.0),
        "gradientCenter": { "x": s.gradient_center.x, "y": s.gradient_center.y },
        "gradientStart": s.gradient_start.as_deref().unwrap_or("rgba(99, 102, 241, 1)"),
        "gradientEnd": s.gradient_end.as_deref().unwrap_or("rgba(56, 189, 248, 1)"),
    });
    let obj = base.as_object_mut().expect("object");
    if let Some(r) = &s.gradient_ramp {
        obj.insert("gradientRamp".into(), to_value(r));
    }
    obj.insert(
        "bgImageUrl".into(),
        js::trim(s.bg_image_url.as_deref().unwrap_or("")).into(),
    );
    obj.insert("bgImageFit".into(), s.bg_image_fit.clone().into());
    obj.insert("bgImagePosition".into(), s.bg_image_position.clone().into());
    obj.insert("bgImageRepeat".into(), s.bg_image_repeat.clone().into());
    obj.insert(
        "lottieStartMs".into(),
        js::or(lottie_start, 0.0).max(0.0).into(),
    );
    obj.insert("lottieLoop".into(), (el.lottie_loop == Some(true)).into());
    obj.insert(
        "videoStartMs".into(),
        js::or(video_start, 0.0).max(0.0).into(),
    );
    obj.insert("videoLoop".into(), (el.video_loop == Some(true)).into());
    obj.insert(
        "mediaUrl".into(),
        js::trim(el.media_url.as_deref().unwrap_or("")).into(),
    );
    obj.insert("svgTint".into(), (el.svg_tint == Some(true)).into());
    track.insert("base".into(), base);

    const LANES: [&str; 46] = [
        "x",
        "y",
        "width",
        "height",
        "scaleX",
        "scaleY",
        "fontSize",
        "textColorR",
        "textColorG",
        "textColorB",
        "textColorA",
        "bgColorR",
        "bgColorG",
        "bgColorB",
        "bgColorA",
        "modBlurRadius",
        "modColorBrightness",
        "modColorContrast",
        "modColorSaturation",
        "modColorHueRotate",
        "modShadowX",
        "modShadowY",
        "modShadowBlur",
        "modCropPercent",
        "modTypewriterPercent",
        "modPathProgress",
        "modTextStaggerProgress",
        "modTextScrambleProgress",
        "modTextGlyphColorProgress",
        "modSkewX",
        "modSkewY",
        "textProgress",
        "rotation",
        "opacity",
        "borderWidth",
        "chartReveal",
        "particleEmitting",
        "particleSpawnRate",
        "particleEmitterX",
        "particleEmitterY",
        "lightningEmitting",
        "lightningAx",
        "lightningAy",
        "lightningBx",
        "lightningBy",
        "patternPhase",
    ];
    let mut timeline = Map::new();
    for lane in LANES {
        timeline.insert(
            lane.into(),
            el.timeline.get(lane).map_or(json!([]), to_value),
        );
    }
    track.insert("timeline".into(), Value::Object(timeline));
    Value::Object(track)
}

/// `v` when it is one of `allowed`, otherwise `default`.
fn allowed_or<'a>(v: Option<&'a str>, allowed: &[&str], default: &'a str) -> &'a str {
    v.filter(|v| allowed.contains(v)).unwrap_or(default)
}

fn clean(v: &Option<String>, default: &str) -> String {
    let s = js::trim(v.as_deref().unwrap_or(""));
    if s.is_empty() {
        default.into()
    } else {
        s.into()
    }
}

/// The object the runtime reads as `window.__ionframeSpx`.
pub fn runtime_data(t: &TemplateExport, kind: PageKind, p: &Profile) -> Value {
    let els = &t.elements;
    let of = |k: &'static str| els.iter().filter(move |e| e.kind == k);
    let hook = &t.ionboard_hook;
    let panel = &t.ionboard_panel;
    let hook_on = hook.enabled;
    let panel_on = hook_on && panel.enabled;
    let or_default = |v: &str, d: &str| {
        if v.is_empty() {
            d.to_string()
        } else {
            js::trim(v).to_string()
        }
    };
    let api_base = if hook.api_base.is_empty() {
        p.board_api_base.clone()
    } else {
        hook.api_base.clone()
    };
    let panel_url = match js::trim(&panel.url) {
        "" => board_panel_url(p, &hook.magic_id_default, &hook.project_id_default),
        u => u.to_string(),
    };
    let trimmed_or = |v: &str, d: &str| match js::trim(if v.is_empty() { d } else { v }) {
        "" => d.to_string(),
        s => s.to_string(),
    };

    let cues: Vec<Value> = t
        .cues
        .iter()
        .map(|c| {
            let delay = js::or(c.delay_ms, 0.0).max(0.0);
            let mut v = to_value(c);
            v["delayMs"] = delay.into();
            v["effectiveTimeMs"] = js::round(c.time + delay).max(0.0).into();
            v
        })
        .collect();

    let chart_layers: Vec<Value> = of("chart")
        .map(|el| {
            let s = &el.style;
            let kind = allowed_or(el.chart_kind.as_deref(), &["pie", "line"], "bar");
            let points: Vec<Value> = el
                .chart_points
                .iter()
                .flatten()
                .map(|pt| {
                    let mut v = Map::new();
                    v.insert("label".into(), pt.label.clone().into());
                    v.insert("value".into(), (if pt.value.is_finite() { pt.value } else { 0.0 }).into());
                    if let Some(c) = pt.color.as_deref().map(js::trim).filter(|c| !c.is_empty()) {
                        v.insert("color".into(), c.into());
                    }
                    if let Some(f) = pt.live_variable_id.as_deref().map(js::trim).filter(|f| !f.is_empty()) {
                        v.insert("fieldId".into(), f.into());
                    }
                    Value::Object(v)
                })
                .collect();
            json!({
                "id": el.id,
                "kind": kind,
                "points": points,
                "cutout": js::or(el.chart_cutout, 0.0),
                "barThickness": js::or(el.chart_bar_thickness, 80.0),
                "lineWidth": js::or(el.chart_line_width, 2.0),
                "pointRadius": js::finite_or(el.chart_point_radius, 4.0),
                "tension": js::finite_or(el.chart_line_tension, 0.2),
                "showLegend": el.chart_show_legend.unwrap_or(el.chart_kind.as_deref() == Some("pie")),
                "showValues": el.chart_show_values.unwrap_or(matches!(el.chart_kind.as_deref(), None | Some("" | "bar"))),
                "valueSort": allowed_or(el.chart_value_sort.as_deref(), &["asc", "desc"], "none"),
                "fontFamily": clean(&s.font_family, "Mouse Memoirs"),
                "fontSize": js::finite_or(s.font_size, 14.0),
                "textColor": clean(&s.text_color, "#e2e8f0"),
            })
        })
        .collect();

    let qr_layers: Vec<Value> = of("qr")
        .map(|el| {
            let ec = js::trim(el.qr_error_correction.as_deref().unwrap_or("M")).to_uppercase();
            json!({
                "id": el.id,
                "content": clean(&el.qr_content, &p.default_qr_content),
                "fieldId": js::trim(el.qr_variable_id.as_deref().or(el.field_id.as_deref()).unwrap_or("")),
                "fg": clean(&el.qr_fg_color, "#000000"),
                "bg": clean(&el.qr_bg_color, "#ffffff"),
                "ec": if ["L", "M", "Q", "H"].contains(&ec.as_str()) { ec } else { "M".into() },
                "margin": js::round(js::or(el.qr_margin, 2.0)).clamp(0.0, 8.0),
                "width": el.layout.width,
                "height": el.layout.height,
            })
        })
        .collect();

    let particle_layers: Vec<Value> = of("particles")
        .filter_map(|el| el.particles.as_ref().map(|f| (el, f)))
        .map(|(el, f)| {
            json!({
                "id": el.id,
                "emitting": f.particle_emitting,
                "emitterX": f.particle_emitter_x,
                "emitterY": f.particle_emitter_y,
                "emitterRadius": f.particle_emitter_radius,
                "emitterWidth": js::or(Some(f.particle_emitter_width), 80.0),
                "emitterHeight": js::or(Some(f.particle_emitter_height), 48.0),
                "emitterShape": f.particle_emitter_shape,
                "emitAngleDeg": js::finite_or(Some(f.particle_emit_angle_deg), -90.0),
                "spawnRate": f.particle_spawn_rate,
                "maxCount": js::or(Some(f.particle_max_count), 300.0),
                "lifetimeMs": js::or(Some(f.particle_lifetime_ms), 1800.0),
                "size": js::or(Some(f.particle_size), 8.0),
                "sizeEnd": js::or(Some(f.particle_size_end), 2.0),
                "sizeRamp": f.particle_size_ramp.iter().map(|s| json!({ "offset": js::or(Some(s.offset), 0.0), "value": js::or(Some(s.value), 0.0).clamp(0.0, 1.0) })).collect::<Vec<_>>(),
                "speed": js::or(Some(f.particle_speed), 80.0),
                "spreadDeg": js::or(Some(f.particle_spread_deg), 360.0),
                "gravityX": f.particle_gravity_x,
                "gravityY": f.particle_gravity_y,
                "rotationDeg": f.particle_rotation_deg,
                "rotationRandomDeg": f.particle_rotation_random_deg.max(0.0),
                "spinDeg": f.particle_spin_deg,
                "alignToVelocity": f.particle_align_to_velocity,
                "colorStart": f.particle_color_start,
                "colorEnd": f.particle_color_end,
                "colorRamp": f.particle_color_ramp.iter().map(|s| json!({ "offset": js::or(Some(s.offset), 0.0), "color": s.color })).collect::<Vec<_>>(),
                "colorMode": f.particle_color_mode,
                "paletteColors": f.particle_palette_colors,
                "alphaRamp": f.particle_alpha_ramp.iter().map(|s| json!({ "offset": js::or(Some(s.offset), 0.0), "value": js::or(Some(s.value), 0.0).clamp(0.0, 1.0) })).collect::<Vec<_>>(),
                "spriteUrl": js::trim(f.particle_sprite_url.as_deref().unwrap_or("")),
                "blend": f.particle_blend,
                "attach": f.particle_attach,
                "seed": js::or(Some(f.particle_seed), 1.0),
                "width": el.layout.width,
                "height": el.layout.height,
            })
        })
        .collect();

    let lightning_layers: Vec<Value> = of("lightning")
        .filter_map(|el| el.lightning.as_ref().map(|f| (el, f)))
        .map(|(el, f)| {
            json!({
                "id": el.id,
                "emitting": f.lightning_emitting,
                "ax": js::round(f.lightning_ax), "ay": js::round(f.lightning_ay),
                "bx": js::round(f.lightning_bx), "by": js::round(f.lightning_by),
                "segments": js::or(Some(f.lightning_segments), 14.0),
                "amplitude": f.lightning_amplitude,
                "width": js::or(Some(f.lightning_width), 2.0),
                "glowWidth": f.lightning_glow_width,
                "color": f.lightning_color,
                "glowColor": f.lightning_glow_color,
                "branches": f.lightning_branches,
                "seed": js::or(Some(f.lightning_seed), 1.0),
                "flickerMs": js::or(Some(f.lightning_flicker_ms), 48.0),
                "layoutWidth": el.layout.width,
                "layoutHeight": el.layout.height,
            })
        })
        .collect();

    let pattern_layers: Vec<Value> = of("pattern")
        .filter_map(|el| el.pattern.as_ref().map(|f| (el, f)))
        .map(|(el, f)| {
            json!({
                "id": el.id,
                "kind": f.pattern_kind,
                "colorA": f.pattern_color_a,
                "colorB": f.pattern_color_b,
                "cellSize": f.pattern_cell_size,
                "stripeWidth": f.pattern_stripe_width,
                "stripeGap": f.pattern_stripe_gap,
                "stripeAngleDeg": f.pattern_stripe_angle_deg,
                "dotRadius": f.pattern_dot_radius,
                "lineWidth": f.pattern_line_width,
                "phase": f.pattern_phase,
                "layoutWidth": el.layout.width,
                "layoutHeight": el.layout.height,
            })
        })
        .collect();

    let clip_layer = |el: &Element, start: Option<f64>, looped: Option<bool>| {
        json!({
            "id": el.id,
            "src": js::trim(el.media_url.as_deref().unwrap_or("")),
            "startMs": js::or(start, 0.0).max(0.0),
            "loop": looped == Some(true),
            "width": el.layout.width,
            "height": el.layout.height,
        })
    };

    let mut data = Map::new();
    data.insert("exportStamp".into(), "ionframe-spx-2026-07-23-quiet".into());
    data.insert(
        "sceneDurationMs".into(),
        js::or(Some(t.scene.duration_ms), 1000.0).max(1.0).into(),
    );
    data.insert("pausePoints".into(), to_value(&pause_times_ms(t)));
    data.insert("cues".into(), Value::Array(cues));
    data.insert(
        "animationTracks".into(),
        els.iter().map(|e| animation_track(e, t)).collect(),
    );
    data.insert(
        "ionEasingToAnime".into(),
        serde_json::from_str(ANIME_EASE_KEYS).expect("easing map is JSON"),
    );
    data.insert(
        "textLayers".into(),
        of("text")
            .map(|el| {
                let fallback = el.text.as_ref().map(|t| t.value.clone()).or_else(|| el.text_default.clone()).unwrap_or_default();
                json!({ "id": el.id, "fieldId": js::trim(el.field_id.as_deref().unwrap_or("")), "fallback": fallback })
            })
            .collect(),
    );
    data.insert(
        "imageLayers".into(),
        of("imageSlot")
            .map(|el| {
                json!({
                    "id": el.id,
                    "fieldId": js::trim(el.image_variable_id.as_deref().or(el.field_id.as_deref()).unwrap_or("")),
                    "fallback": js::trim(el.image_default_value.as_deref().unwrap_or("")),
                    "fit": if el.image_fit.as_deref() == Some("contain") { "contain" } else { "cover" },
                })
            })
            .collect(),
    );
    data.insert(
        "visibilityLayers".into(),
        els.iter()
            .filter(|el| !js::trim(el.visibility_variable_id.as_deref().unwrap_or("")).is_empty())
            .map(|el| {
                json!({
                    "id": el.id,
                    "domId": format!("{}_{}", dom_prefix(&el.kind), el.id),
                    "fieldId": js::trim(el.visibility_variable_id.as_deref().unwrap_or("")),
                    "invert": el.visibility_invert == Some(true),
                })
            })
            .collect(),
    );
    data.insert("chartLayers".into(), Value::Array(chart_layers));
    data.insert("qrLayers".into(), Value::Array(qr_layers));
    data.insert("particleLayers".into(), Value::Array(particle_layers));
    data.insert("lightningLayers".into(), Value::Array(lightning_layers));
    data.insert("patternLayers".into(), Value::Array(pattern_layers));
    data.insert(
        "lottieLayers".into(),
        of("lottie")
            .map(|el| clip_layer(el, el.lottie_start_ms, el.lottie_loop))
            .collect(),
    );
    data.insert(
        "videoLayers".into(),
        of("video")
            .map(|el| clip_layer(el, el.video_start_ms, el.video_loop))
            .collect(),
    );
    data.insert(
        "ionboardHook".into(),
        json!({
            "enabled": hook_on,
            "projectIdField": or_default(&hook.project_id_field, "ionboard_project_id"),
            "magicIdField": or_default(&hook.magic_id_field, "ionboard_magic_id"),
            "apiBase": api_base.strip_suffix('/').unwrap_or(&api_base),
            "pollMs": js::or(Some(hook.poll_ms), 500.0).max(200.0),
            "magicIdDefault": hook.magic_id_default,
            "projectIdDefault": hook.project_id_default,
        }),
    );
    data.insert(
        "ionboardPanel".into(),
        json!({
            "enabled": panel_on,
            "field": trimmed_or(&panel.field, "ionboard_panel"),
            "title": trimmed_or(&panel.title, &p.board_panel_title),
            "aspect": trimmed_or(&panel.aspect, "16:9"),
            "url": panel_url,
        }),
    );
    if kind == PageKind::Banner {
        data.insert("autoplay".into(), true.into());
        data.insert("loop".into(), true.into());
    }
    Value::Object(data)
}

/// `window.SPXGCTemplateDefinition`: what SPX shows and stores per item.
pub fn spx_template_definition(t: &TemplateExport, p: &Profile) -> Value {
    let hook = &t.ionboard_hook;
    let panel = &t.ionboard_panel;
    let data = runtime_data(t, PageKind::Spx, p);
    let (hook_on, panel_on) = (hook.enabled, hook.enabled && panel.enabled);
    let mut fields = vec![json!({
        "field": "comment", "ftype": "textfield", "title": "Rundown item comment", "value": "[ Item nickname ]"
    })];
    if panel_on {
        let pd = &data["ionboardPanel"];
        fields.push(json!({
            "field": pd["field"], "ftype": "panel", "title": pd["title"], "aspect": pd["aspect"], "url": pd["url"], "value": ""
        }));
    }
    if hook_on || panel_on {
        let hd = &data["ionboardHook"];
        fields.push(json!({
            "field": hd["magicIdField"], "ftype": "textfield",
            "title": format!("{} Magic ID", p.board_name), "value": hook.magic_id_default
        }));
        fields.push(json!({
            "field": hd["projectIdField"], "ftype": "textfield",
            "title": format!("{} Project ID", p.board_name), "value": hook.project_id_default
        }));
    }
    for f in &t.fields {
        fields.push(if f.kind == "filelist" {
            json!({
                "field": f.id, "ftype": "filelist", "title": f.label,
                "assetfolder": f.assetfolder.as_deref().filter(|s| !s.is_empty()).unwrap_or("./images/"),
                "extension": f.extension.as_deref().filter(|s| !s.is_empty()).unwrap_or("png"),
                "value": f.default
            })
        } else {
            json!({ "field": f.id, "ftype": "textfield", "title": f.label, "value": f.default })
        });
    }
    let d = &t.spx_definition;
    let mut def = json!({
        "description": d.description,
        "playserver": d.playserver,
        "playchannel": d.playchannel,
        "playlayer": d.playlayer,
        "webplayout": d.webplayout,
        "out": d.out,
        "dataformat": d.dataformat,
        "uicolor": d.uicolor,
        "preventToggle": d.prevent_toggle,
        "DataFields": fields,
    });
    let steps = ograf_step_count(t);
    if steps >= 2 {
        def["steps"] = steps.to_string().into();
    }
    def
}

/// Hidden `#hiddenSpxData` divs holding field values (one per field).
fn hidden_data(t: &TemplateExport, data: &Value) -> String {
    let hook = &t.ionboard_hook;
    let mut rows = Vec::new();
    if hook.enabled {
        let hd = &data["ionboardHook"];
        rows.push(format!(
            "        <div id=\"{}\">{}</div>",
            hd["magicIdField"].as_str().unwrap_or_default(),
            esc_html(&hook.magic_id_default)
        ));
        rows.push(format!(
            "        <div id=\"{}\">{}</div>",
            hd["projectIdField"].as_str().unwrap_or_default(),
            esc_html(&hook.project_id_default)
        ));
    }
    for f in &t.fields {
        rows.push(format!(
            "        <div id=\"{}\">{}</div>",
            f.id,
            esc_html(&f.default)
        ));
    }
    rows.join("\n")
}

/// The entry HTML of an SPX / OGraf package, or a banner's `index.html`.
///
/// `t` must already point at package paths (see [`super::assets`]).
/// `font_faces` are `@font-face` rules; `template_js_src` is the script
/// file to load, if the graphic has one.
pub fn page_html(
    t: &TemplateExport,
    font_faces: &[String],
    kind: PageKind,
    template_js_src: Option<&str>,
    p: &Profile,
) -> String {
    let has = |k: &str| t.elements.iter().any(|e| e.kind == k);
    let data = runtime_data(t, kind, p);
    let boot = format!(
        "<script>\n    window.__ionframeSpx = {};\n  </script>",
        js::stringify(&data).replace('<', "\\u003c")
    );
    let template_js = match template_js_src.map(js::trim).filter(|s| !s.is_empty()) {
        Some(src) => format!("<script src=\"{}\"></script>", src.replace('"', "")),
        None => String::new(),
    };
    let overlay: String = [
        ("chart", "\n    .chartItem { position: absolute; }"),
        (
            "qr",
            "\n    .qrItem { position: absolute; overflow: hidden; }",
        ),
        (
            "particles",
            "\n    .particlesItem { position: absolute; overflow: hidden; }",
        ),
        (
            "lightning",
            "\n    .lightningItem { position: absolute; overflow: hidden; }",
        ),
        (
            "pattern",
            "\n    .patternItem { position: absolute; overflow: hidden; }",
        ),
        (
            "lottie",
            "\n    .lottieItem { position: absolute; overflow: hidden; }",
        ),
        (
            "video",
            "\n    .videoItem { position: absolute; overflow: hidden; }",
        ),
    ]
    .iter()
    .filter(|(k, _)| has(k))
    .map(|(_, css)| *css)
    .chain(["\n    .emptyItem { position: absolute; overflow: visible; }"])
    .collect();
    let when = |k: &str, s: &'static str| if has(k) { s } else { "" };
    let definition = js::stringify_pretty(&spx_template_definition(t, p));
    let hidden = hidden_data(t, &data);
    let layers = layers_html(t);
    let (width, height) = (n(t.scene.width), n(t.scene.height));
    let fonts = font_faces.join("\n    ");
    js::fill_markers(
        if kind == PageKind::Spx {
            SPX_PAGE
        } else {
            BANNER_PAGE
        },
        &[
            ("TITLE", &t.template_id),
            ("WIDTH", &width),
            ("HEIGHT", &height),
            ("FONT_FACE_CSS", &fonts),
            ("OVERLAY_CSS", &overlay),
            ("BOOT_SCRIPT", &boot),
            (
                "BOARD_SCRIPT",
                if t.ionboard_hook.enabled {
                    BOARD_SCRIPT
                } else {
                    ""
                },
            ),
            ("TEMPLATE_JS_SCRIPT", &template_js),
            ("CHART_SCRIPT", when("chart", CHART_SCRIPT)),
            ("QR_SCRIPT", when("qr", QR_SCRIPT)),
            ("LOTTIE_SCRIPT", when("lottie", LOTTIE_SCRIPT)),
            ("VIDEO_SCRIPT", when("video", VIDEO_SCRIPT)),
            ("MODULE_INIT_SCRIPT", MODULE_INIT_SCRIPT),
            ("TEMPLATE_DEFINITION", &definition),
            ("LAYERS", &layers),
            ("HIDDEN_DATA", &hidden),
        ],
    )
}
