// SPDX-License-Identifier: AGPL-3.0-or-later
//! CSS and SVG fragments for exported layers: fills, gradients, borders,
//! polygon paths and clip paths. Numbers are printed the JavaScript way.

use crate::js::{self, number_to_string as n};
use crate::model::{PolygonPoint, RampColor};
use crate::normalize::normalize_blend_mode;
use crate::ramp::{color_ramp_from_legacy, normalize_color_ramp};

/// `mix-blend-mode:…;`, or nothing for `normal`.
pub fn blend_mode_css(mode: Option<&str>) -> String {
    let m = normalize_blend_mode(mode);
    if m == "normal" {
        String::new()
    } else {
        format!("mix-blend-mode:{m};")
    }
}

/// Border widths in CSS order (top, right, bottom, left), never negative.
pub fn border_widths(width: f64, mode: &str, widths: Option<[f64; 4]>) -> [f64; 4] {
    let fin = |v: f64, d: f64| if v.is_finite() { v.max(0.0) } else { d };
    let single = fin(width, 0.0);
    match (mode, widths) {
        ("sides", Some(w)) => w.map(|v| fin(v, single)),
        _ => [single; 4],
    }
}

/// CSS `border-width` value.
pub fn border_width_css(width: f64, mode: &str, widths: Option<[f64; 4]>) -> String {
    let [t, r, b, l] = border_widths(width, mode, widths);
    if mode == "sides" {
        format!("{}px {}px {}px {}px", n(t), n(r), n(b), n(l))
    } else {
        format!("{}px", n(t))
    }
}

pub fn any_border(width: f64, mode: &str, widths: Option<[f64; 4]>) -> bool {
    border_widths(width, mode, widths).iter().any(|w| *w > 0.0)
}

/// `0.123` → `12.3%` (one decimal).
fn pct(v: f64) -> String {
    format!("{}%", n(js::round(v * 1000.0) / 10.0))
}

fn clamp_center(c: (f64, f64)) -> (f64, f64) {
    let f = |v: f64| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.5
        }
    };
    (f(c.0), f(c.1))
}

fn ramp_css_parts(stops: &[RampColor]) -> String {
    normalize_color_ramp(Some(stops))
        .iter()
        .map(|s| format!("{} {}", s.color, pct(s.offset)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// CSS gradient for a fill: `linear`, `circle`, `ellipse` or `conic`.
pub fn gradient_css(stops: &[RampColor], kind: &str, angle_deg: f64, center: (f64, f64)) -> String {
    let angle = if angle_deg.is_finite() {
        angle_deg
    } else {
        90.0
    };
    let (cx, cy) = clamp_center(center);
    let parts = ramp_css_parts(stops);
    let at = format!("{} {}", pct(cx), pct(cy));
    match kind {
        "circle" => format!("radial-gradient(circle at {at}, {parts})"),
        "ellipse" => format!("radial-gradient(ellipse at {at}, {parts})"),
        "conic" => format!("conic-gradient(from {}deg at {at}, {parts})", n(angle)),
        _ => format!("linear-gradient({}deg, {parts})", n(angle)),
    }
}

/// SVG `<stop>` offsets and colors for a fill ramp.
pub fn svg_stops(stops: &[RampColor]) -> Vec<(String, String)> {
    normalize_color_ramp(Some(stops))
        .into_iter()
        .map(|s| (pct(s.offset), s.color))
        .collect()
}

/// SVG radial gradient attributes: `(gradientUnits, cx, cy, r)`.
pub fn svg_radial_params(
    kind: &str,
    center: (f64, f64),
    width: f64,
    height: f64,
) -> (&'static str, String, String, String) {
    let (cx, cy) = clamp_center(center);
    if kind == "ellipse" {
        return ("objectBoundingBox", n(cx), n(cy), "0.7071".into());
    }
    let (w, h) = (width.max(1.0), height.max(1.0));
    let (x, y) = (cx * w, cy * h);
    let r = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
        .iter()
        .fold(0.0f64, |r, (px, py)| r.max((px - x).hypot(py - y)));
    let round3 = |v: f64| n(js::round(v * 1000.0) / 1000.0);
    ("userSpaceOnUse", round3(x), round3(y), round3(r))
}

/// The fill ramp of a style, from its ramp or legacy start / end colors.
pub fn fill_ramp(
    ramp: Option<&[RampColor]>,
    start: Option<&str>,
    end: Option<&str>,
) -> Vec<RampColor> {
    color_ramp_from_legacy(
        ramp,
        Some(start.unwrap_or("rgba(99, 102, 241, 1)")),
        Some(end.unwrap_or("rgba(56, 189, 248, 1)")),
    )
}

pub fn bg_image_size_css(fit: &str) -> &'static str {
    match fit {
        "contain" => "contain",
        "stretch" => "100% 100%",
        _ => "cover",
    }
}

pub fn bg_image_position_css(position: &str) -> &'static str {
    match position {
        "top" => "center top",
        "bottom" => "center bottom",
        "left" => "left center",
        "right" => "right center",
        "top-left" => "left top",
        "top-right" => "right top",
        "bottom-left" => "left bottom",
        "bottom-right" => "right bottom",
        _ => "center center",
    }
}

/// `url('…')` for a background image, or `none`. Relative paths get a
/// leading `/`, as in FRAME.
pub fn background_image_url_css(url: Option<&str>) -> String {
    let raw = js::trim(url.unwrap_or(""));
    if raw.is_empty() {
        return "none".into();
    }
    let resolved = if raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("data:")
        || raw.starts_with('/')
    {
        raw.to_string()
    } else {
        format!("/{raw}")
    };
    format!(
        "url('{}')",
        resolved.replace('\\', "\\\\").replace('\'', "\\'")
    )
}

/// Eight hard drop shadows around a tinted SVG, faking an outline.
pub fn silhouette_stroke_filter(width_px: f64, color: &str) -> String {
    let w = js::or(Some(width_px), 0.0).clamp(0.0, 48.0);
    if w <= 0.0 {
        return "none".into();
    }
    let c = js::trim(color);
    let c = if c.is_empty() { "#000000" } else { c };
    (0..8)
        .map(|i| {
            let a = f64::from(i) / 8.0 * std::f64::consts::PI * 2.0;
            format!(
                "drop-shadow({}px {}px 0px {c})",
                js::to_fixed(a.cos() * w, 2),
                js::to_fixed(a.sin() * w, 2)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_bezier(p: &PolygonPoint) -> bool {
    p.curve.as_deref() == Some("bezier")
}

fn cubic_to(a: &PolygonPoint, b: &PolygonPoint) -> String {
    let (c1x, c1y) = if is_bezier(a) {
        (a.out_x.unwrap_or(a.x), a.out_y.unwrap_or(a.y))
    } else {
        (a.x, a.y)
    };
    let (c2x, c2y) = if is_bezier(b) {
        (b.in_x.unwrap_or(b.x), b.in_y.unwrap_or(b.y))
    } else {
        (b.x, b.y)
    };
    format!(
        " C {} {}, {} {}, {} {}",
        n(c1x),
        n(c1y),
        n(c2x),
        n(c2y),
        n(b.x),
        n(b.y)
    )
}

fn linear_points(pts: &[PolygonPoint]) -> String {
    pts.iter()
        .map(|p| format!("{} {}", n(p.x), n(p.y)))
        .collect::<Vec<_>>()
        .join(" L ")
}

/// SVG path for an open path shape.
pub fn open_path_d(pts: &[PolygonPoint], mode: Option<&str>) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    if mode != Some("bezier") {
        return format!("M {}", linear_points(pts));
    }
    let mut d = format!("M {} {}", n(pts[0].x), n(pts[0].y));
    for pair in pts.windows(2) {
        d.push_str(&cubic_to(&pair[0], &pair[1]));
    }
    d
}

/// SVG path for a closed polygon; a triangle when it has fewer than 3 points.
pub fn polygon_path_d(pts: &[PolygonPoint], mode: Option<&str>, width: f64, height: f64) -> String {
    if pts.len() < 3 {
        return format!(
            "M 0 0 L {} 0 L {} {} Z",
            n(width),
            n(width / 2.0),
            n(height)
        );
    }
    if mode != Some("bezier") {
        return format!("M {} Z", linear_points(pts));
    }
    let mut d = format!("M {} {}", n(pts[0].x), n(pts[0].y));
    for (i, a) in pts.iter().enumerate() {
        d.push_str(&cubic_to(a, &pts[(i + 1) % pts.len()]));
    }
    format!("{d} Z")
}

/// SVG `points` attribute for a polygon.
pub fn polygon_points_attr(pts: &[PolygonPoint], width: f64, height: f64) -> String {
    if pts.len() < 3 {
        return format!("0,0 {},0 {},{}", n(width), n(width / 2.0), n(height));
    }
    pts.iter()
        .map(|p| format!("{},{}", n(p.x), n(p.y)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// CSS `polygon(…)` clip path (coordinates rounded to 0.001).
pub fn polygon_clip_path(pts: &[PolygonPoint]) -> String {
    if pts.len() < 3 {
        return String::new();
    }
    let f = |v: f64| n(js::round(v * 1000.0) / 1000.0);
    format!(
        "polygon({})",
        pts.iter()
            .map(|p| format!("{}px {}px", f(p.x), f(p.y)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// CSS `path('…')` clip path.
pub fn path_clip_path(d: &str) -> String {
    if d.is_empty() {
        String::new()
    } else {
        format!("path('{d}')")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ramp::color;

    fn pt(x: f64, y: f64) -> PolygonPoint {
        PolygonPoint {
            x,
            y,
            curve: None,
            in_x: None,
            in_y: None,
            out_x: None,
            out_y: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn borders() {
        assert_eq!(border_width_css(2.0, "single", None), "2px");
        assert_eq!(
            border_width_css(2.0, "sides", Some([1.0, -1.0, f64::NAN, 4.5])),
            "1px 0px 2px 4.5px"
        );
        assert!(!any_border(0.0, "single", None));
    }

    #[test]
    fn gradients() {
        let stops = [color(0.0, "red"), color(1.0, "blue")];
        assert_eq!(
            gradient_css(&stops, "linear", 45.0, (0.5, 0.5)),
            "linear-gradient(45deg, red 0%, blue 100%)"
        );
        assert_eq!(
            gradient_css(&stops, "circle", 0.0, (0.25, 2.0)),
            "radial-gradient(circle at 25% 100%, red 0%, blue 100%)"
        );
        assert_eq!(
            gradient_css(&stops, "conic", 90.0, (0.5, 0.5)),
            "conic-gradient(from 90deg at 50% 50%, red 0%, blue 100%)"
        );
        assert_eq!(
            svg_radial_params("circle", (0.5, 0.5), 200.0, 100.0),
            (
                "userSpaceOnUse",
                "100".into(),
                "50".into(),
                "111.803".into()
            )
        );
        assert_eq!(svg_radial_params("ellipse", (0.2, 0.5), 1.0, 1.0).1, "0.2");
    }

    #[test]
    fn background_images() {
        assert_eq!(
            background_image_url_css(Some("assets/a b's.png")),
            "url('/assets/a b\\'s.png')"
        );
        assert_eq!(background_image_url_css(Some("  ")), "none");
        assert_eq!(bg_image_position_css("top-left"), "left top");
    }

    #[test]
    fn silhouette() {
        assert_eq!(silhouette_stroke_filter(0.0, "red"), "none");
        let f = silhouette_stroke_filter(2.0, "");
        assert!(f.starts_with(
            "drop-shadow(2.00px 0.00px 0px #000000) drop-shadow(1.41px 1.41px 0px #000000)"
        ));
        assert_eq!(f.matches("drop-shadow").count(), 8);
    }

    #[test]
    fn paths() {
        let tri = [pt(0.0, 0.0), pt(10.0, 0.0), pt(5.5, 8.0)];
        assert_eq!(
            polygon_path_d(&tri, None, 1.0, 1.0),
            "M 0 0 L 10 0 L 5.5 8 Z"
        );
        assert_eq!(polygon_points_attr(&tri, 1.0, 1.0), "0,0 10,0 5.5,8");
        assert_eq!(polygon_path_d(&[], None, 10.0, 7.0), "M 0 0 L 10 0 L 5 7 Z");
        assert_eq!(
            open_path_d(&tri[..2], Some("bezier")),
            "M 0 0 C 0 0, 10 0, 10 0"
        );
        assert_eq!(
            polygon_clip_path(&tri),
            "polygon(0px 0px,10px 0px,5.5px 8px)"
        );
    }
}
