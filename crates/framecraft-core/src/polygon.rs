// SPDX-License-Identifier: AGPL-3.0-or-later
//! Polygon and path shapes, as FRAME's `polygonGeometry.ts` and
//! `svgToPolygon.ts`: default shapes, adding and removing points, fitting
//! the layer box to the points, Linear / Bezier handles, and turning an SVG
//! file into polygon points (vertices and bezier handles, not resampled).

use crate::commands::{Change, Command, CommandError, item_mut, item_name};
use crate::model::{ItemType, PolygonPoint, ShapeType};
use crate::normalize::Document;

fn pt(x: f64, y: f64) -> PolygonPoint {
    PolygonPoint {
        x,
        y,
        curve: Some("linear".into()),
        in_x: Some(x),
        in_y: Some(y),
        out_x: Some(x),
        out_y: Some(y),
        extra: Default::default(),
    }
}

fn bez(x: f64, y: f64, in_x: f64, in_y: f64, out_x: f64, out_y: f64) -> PolygonPoint {
    PolygonPoint {
        x,
        y,
        curve: Some("bezier".into()),
        in_x: Some(in_x),
        in_y: Some(in_y),
        out_x: Some(out_x),
        out_y: Some(out_y),
        extra: Default::default(),
    }
}

fn is_bezier(p: &PolygonPoint) -> bool {
    p.curve.as_deref() == Some("bezier")
}

/// FRAME's new polygon: a quad inset 12 % from the bounds.
pub fn default_polygon_points(width: f64, height: f64) -> Vec<PolygonPoint> {
    let (w, h) = (width.max(1.0), height.max(1.0));
    let r = |v: f64| v.round();
    vec![
        pt(r(w * 0.12), r(h * 0.12)),
        pt(r(w * 0.88), r(h * 0.12)),
        pt(r(w * 0.88), r(h * 0.88)),
        pt(r(w * 0.12), r(h * 0.88)),
    ]
}

/// FRAME's new path: a left → right bezier arch.
pub fn default_path_points(width: f64, height: f64) -> Vec<PolygonPoint> {
    let (w, h) = (width.max(1.0), height.max(1.0));
    let r = |v: f64| v.round();
    let (mid, arch) = (r(h * 0.5), r(h * 0.28));
    vec![
        bez(0.0, mid, 0.0, mid, r(w * 0.18), mid),
        bez(r(w * 0.5), arch, r(w * 0.35), arch, r(w * 0.65), arch),
        bez(w, mid, r(w * 0.82), mid, w, mid),
    ]
}

/// A point's default bezier handles from its neighbours (when collapsed).
fn ensure_handles(p: &PolygonPoint, prev: &PolygonPoint, next: &PolygonPoint) -> PolygonPoint {
    let (tx, ty) = ((next.x - prev.x) / 2.0, (next.y - prev.y) / 2.0);
    let mag = tx.hypot(ty);
    let mag = if mag == 0.0 { 1.0 } else { mag };
    let (ux, uy) = (tx / mag, ty / mag);
    let len = (40f64).min(mag * 0.35);
    let len = if len == 0.0 { 24.0 } else { len };
    let collapsed_in = match (p.in_x, p.in_y) {
        (Some(x), Some(y)) => x == p.x && y == p.y,
        _ => true,
    };
    let collapsed_out = match (p.out_x, p.out_y) {
        (Some(x), Some(y)) => x == p.x && y == p.y,
        _ => true,
    };
    let mut q = p.clone();
    q.curve = Some("bezier".into());
    if collapsed_in {
        q.in_x = Some(p.x - ux * len);
        q.in_y = Some(p.y - uy * len);
    }
    if collapsed_out {
        q.out_x = Some(p.x + ux * len);
        q.out_y = Some(p.y + uy * len);
    }
    q
}

/// Switches every point to `linear` (handles collapsed) or `bezier`
/// (handles made where missing), as FRAME's Linear / Bezier choice.
pub fn normalize_handles(points: &[PolygonPoint], mode: &str) -> Vec<PolygonPoint> {
    let n = points.len();
    if mode != "bezier" {
        return points.iter().map(|p| pt(p.x, p.y)).collect();
    }
    (0..n)
        .map(|i| ensure_handles(&points[i], &points[(i + n - 1) % n], &points[(i + 1) % n]))
        .collect()
}

/// Inserts a point halfway along the longest edge (the panel's Add point).
pub fn insert_on_longest_edge(points: &[PolygonPoint]) -> Vec<PolygonPoint> {
    if points.len() < 2 {
        return default_polygon_points(100.0, 100.0);
    }
    let n = points.len();
    let edge = (0..n)
        .max_by(|&a, &b| {
            let len = |i: usize| {
                let (p, q) = (&points[i], &points[(i + 1) % n]);
                (q.x - p.x).hypot(q.y - p.y)
            };
            // The first longest edge, as FRAME's strict `>`.
            len(a).total_cmp(&len(b)).then(b.cmp(&a))
        })
        .unwrap_or(0);
    let (a, b) = (&points[edge], &points[(edge + 1) % n]);
    let mut next = points.to_vec();
    next.insert(edge + 1, pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
    next
}

/// Points scaled from a `from_w` × `from_h` box to `to_w` × `to_h`
/// (FRAME's `scalePolygonPoints`, used while resizing).
pub fn scale_points(
    points: &[PolygonPoint],
    from_w: f64,
    from_h: f64,
    to_w: f64,
    to_h: f64,
) -> Vec<PolygonPoint> {
    let (sx, sy) = (to_w / from_w.max(1e-6), to_h / from_h.max(1e-6));
    map_points(points, |x, y| (x * sx, y * sy))
}

pub fn any_bezier(points: &[PolygonPoint]) -> bool {
    points.iter().any(is_bezier)
}

/// Bounds of points and their handles: `(min_x, min_y, max_x, max_y)`.
fn bounds(points: &[PolygonPoint]) -> Option<(f64, f64, f64, f64)> {
    let mut b: Option<(f64, f64, f64, f64)> = None;
    for p in points {
        for (x, y) in [
            (p.x, p.y),
            (p.in_x.unwrap_or(p.x), p.in_y.unwrap_or(p.y)),
            (p.out_x.unwrap_or(p.x), p.out_y.unwrap_or(p.y)),
        ] {
            b = Some(match b {
                None => (x, y, x, y),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
            });
        }
    }
    b
}

fn map_points(points: &[PolygonPoint], f: impl Fn(f64, f64) -> (f64, f64)) -> Vec<PolygonPoint> {
    points
        .iter()
        .map(|p| {
            let (x, y) = f(p.x, p.y);
            let h = |hx: Option<f64>, hy: Option<f64>| match (hx, hy) {
                (Some(a), Some(b)) => {
                    let (a, b) = f(a, b);
                    (Some(a), Some(b))
                }
                _ => (None, None),
            };
            let (in_x, in_y) = h(p.in_x, p.in_y);
            let (out_x, out_y) = h(p.out_x, p.out_y);
            PolygonPoint {
                x,
                y,
                in_x,
                in_y,
                out_x,
                out_y,
                ..p.clone()
            }
        })
        .collect()
}

/// Fits the layer box to its points (FRAME's `autoFitPolygonItem`): points
/// move so the top-left is 0,0 and the layer moves the other way (along its
/// rotation), so nothing moves on screen. Needs 3 or more points.
fn auto_fit(item: &mut crate::model::Item) {
    let Some(points) = item.polygon_points.as_ref().filter(|p| p.len() >= 3) else {
        return;
    };
    let Some((min_x, min_y, max_x, max_y)) = bounds(points) else {
        return;
    };
    let (w, h) = ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));
    let eps = 0.25;
    if min_x.abs() < eps
        && min_y.abs() < eps
        && (w - item.width).abs() < eps
        && (h - item.height).abs() < eps
    {
        return;
    }
    let moved = map_points(points, |x, y| (x - min_x, y - min_y));
    let rot = item.rotation.to_radians();
    item.x += min_x * rot.cos() - min_y * rot.sin();
    item.y += min_x * rot.sin() + min_y * rot.cos();
    item.width = w;
    item.height = h;
    item.polygon_points = Some(moved);
}

/// Sets a polygon or path layer's points and / or curve mode, then fits
/// the layer box to them. A curve mode given without points re-does the
/// handles (Linear collapses them, Bezier makes missing ones). `to_shape` turns a tinted
/// SVG image into a polygon layer (FRAME's Convert to polygon).
pub struct SetPolygon {
    pub item: String,
    pub points: Option<Vec<PolygonPoint>>,
    pub curve: Option<String>,
    pub to_shape: bool,
}

impl Command for SetPolygon {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let item = item_mut(doc, &self.item)?;
        if self.to_shape {
            item.item_type = Some(ItemType::Shape);
            item.shape_type = Some(ShapeType::Polygon);
            item.media_url = None;
            item.svg_tint = None;
        } else if !matches!(item.shape_type, Some(ShapeType::Polygon | ShapeType::Path)) {
            return Err(CommandError::InvalidValue(
                "not a polygon or path layer".into(),
            ));
        }
        if let Some(p) = &self.points {
            item.polygon_points = Some(p.clone());
        }
        if let Some(mode) = &self.curve {
            item.polygon_curve = Some(mode.clone());
            // A new mode on its own re-does the handles; points given with
            // it (reset, traced SVG) are kept as they are.
            if self.points.is_none()
                && let Some(p) = item.polygon_points.as_ref()
            {
                item.polygon_points = Some(normalize_handles(p, mode));
            }
        }
        auto_fit(item);
        Ok(if self.to_shape {
            Change::Rebuild
        } else {
            Change::Layer(vec![self.item.clone()])
        })
    }
    fn label(&self, doc: &Document) -> String {
        if self.to_shape {
            format!("Convert {} to polygon", item_name(doc, &self.item))
        } else {
            format!("Edit points of {}", item_name(doc, &self.item))
        }
    }
}

// ---------------------------------------------------------------------------
// SVG → polygon

fn near(a: &PolygonPoint, b: &PolygonPoint) -> bool {
    (a.x - b.x).hypot(a.y - b.y) <= 1e-3
}

fn tokenize(d: &str) -> Vec<(char, Vec<f64>)> {
    let mut out: Vec<(char, Vec<f64>)> = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if "MmLlHhVvCcSsQqTtAaZz".contains(c) {
            out.push((c, Vec::new()));
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            // A number: sign, digits, one dot, exponent.
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            let mut dot = false;
            while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !dot)) {
                dot |= b[i] == b'.';
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'-' || b[j] == b'+') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            if let (Ok(v), Some(last)) = (d[start..i].parse::<f64>(), out.last_mut()) {
                last.1.push(v);
            }
            if i == start {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// Path `d` → vertices with in / out handles (FRAME's `pathDToPolygonPoints`).
/// Returns the points and whether any segment was a curve.
pub fn path_d_to_points(d: &str) -> Option<(Vec<PolygonPoint>, bool)> {
    let mut pts: Vec<PolygonPoint> = Vec::new();
    let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    let (mut last_c2, mut last_curve, mut last_q) = ((0.0, 0.0), false, None::<(f64, f64)>);
    let mut has_bezier = false;
    let mut pen = false;

    for (cmd, args) in tokenize(d) {
        let abs = cmd.is_ascii_uppercase();
        let c = cmd.to_ascii_uppercase();
        match c {
            'Z' => {
                if pts.len() >= 2 && near(&pts[pts.len() - 1], &pts[0]) {
                    let last = pts.pop().expect("two points");
                    if is_bezier(&last) {
                        pts[0].curve = Some("bezier".into());
                        pts[0].in_x = last.in_x.or(Some(pts[0].x));
                        pts[0].in_y = last.in_y.or(Some(pts[0].y));
                        has_bezier = true;
                    }
                }
                cx = sx;
                cy = sy;
                last_curve = false;
                last_q = None;
            }
            'M' => {
                for (k, pair) in args.chunks_exact(2).enumerate() {
                    let (ox, oy) = if abs { (0.0, 0.0) } else { (cx, cy) };
                    let (x, y) = (pair[0] + ox, pair[1] + oy);
                    pts.push(pt(x, y));
                    pen = true;
                    if k == 0 {
                        sx = x;
                        sy = y;
                    }
                    cx = x;
                    cy = y;
                }
                last_curve = false;
                last_q = None;
            }
            'L' | 'H' | 'V' | 'A' => {
                let n = match c {
                    'L' => 2,
                    'A' => 7,
                    _ => 1,
                };
                for a in args.chunks_exact(n) {
                    let (ox, oy) = if abs { (0.0, 0.0) } else { (cx, cy) };
                    let (x, y) = match c {
                        'L' => (a[0] + ox, a[1] + oy),
                        'H' => (a[0] + ox, cy),
                        'V' => (cx, a[0] + oy),
                        _ => (a[5] + ox, a[6] + oy),
                    };
                    if !pen {
                        pts.push(pt(cx, cy));
                        pen = true;
                        sx = cx;
                        sy = cy;
                    }
                    pts.push(pt(x, y));
                    cx = x;
                    cy = y;
                    last_curve = false;
                    last_q = None;
                }
            }
            'C' | 'S' | 'Q' | 'T' => {
                let n = match c {
                    'C' => 6,
                    'S' | 'Q' => 4,
                    _ => 2,
                };
                for a in args.chunks_exact(n) {
                    let (ox, oy) = if abs { (0.0, 0.0) } else { (cx, cy) };
                    let (c1, c2, end, q) = match c {
                        'C' => (
                            (a[0] + ox, a[1] + oy),
                            (a[2] + ox, a[3] + oy),
                            (a[4] + ox, a[5] + oy),
                            None,
                        ),
                        'S' => {
                            let c1 = if last_curve {
                                (2.0 * cx - last_c2.0, 2.0 * cy - last_c2.1)
                            } else {
                                (cx, cy)
                            };
                            (c1, (a[0] + ox, a[1] + oy), (a[2] + ox, a[3] + oy), None)
                        }
                        _ => {
                            let (q, end) = if c == 'Q' {
                                ((a[0] + ox, a[1] + oy), (a[2] + ox, a[3] + oy))
                            } else {
                                let q = last_q
                                    .map_or((cx, cy), |(qx, qy)| (2.0 * cx - qx, 2.0 * cy - qy));
                                (q, (a[0] + ox, a[1] + oy))
                            };
                            // Quadratic → cubic.
                            let c1 = (cx + 2.0 / 3.0 * (q.0 - cx), cy + 2.0 / 3.0 * (q.1 - cy));
                            let c2 = (
                                end.0 + 2.0 / 3.0 * (q.0 - end.0),
                                end.1 + 2.0 / 3.0 * (q.1 - end.1),
                            );
                            (c1, c2, end, Some(q))
                        }
                    };
                    has_bezier = true;
                    if !pen {
                        pts.push(pt(cx, cy));
                        pen = true;
                        sx = cx;
                        sy = cy;
                    }
                    let prev = pts.last_mut().expect("pen point");
                    prev.curve = Some("bezier".into());
                    prev.out_x = Some(c1.0);
                    prev.out_y = Some(c1.1);
                    pts.push(bez(end.0, end.1, c2.0, c2.1, end.0, end.1));
                    cx = end.0;
                    cy = end.1;
                    last_c2 = c2;
                    last_curve = true;
                    last_q = q;
                }
            }
            _ => {}
        }
    }
    if pts.len() >= 4 && near(&pts[0], &pts[pts.len() - 1]) {
        let last = pts.pop().expect("points");
        if is_bezier(&last) {
            pts[0].curve = Some("bezier".into());
            pts[0].in_x = last.in_x.or(Some(pts[0].x));
            pts[0].in_y = last.in_y.or(Some(pts[0].y));
            has_bezier = true;
        }
    }
    (pts.len() >= 3).then_some((pts, has_bezier))
}

/// A 2D affine matrix `[a, b, c, d, e, f]` (SVG order).
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn multiply(m: Matrix, n: Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// An SVG `transform` attribute (matrix, translate, scale, rotate, skewX/Y).
fn parse_transform(s: &str) -> Matrix {
    let mut m = IDENTITY;
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let args: Vec<f64> = rest[open + 1..open + close]
            .split([' ', ','])
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        let a = |i: usize, d: f64| args.get(i).copied().unwrap_or(d);
        let t = match name {
            "matrix" if args.len() == 6 => [
                a(0, 1.0),
                a(1, 0.0),
                a(2, 0.0),
                a(3, 1.0),
                a(4, 0.0),
                a(5, 0.0),
            ],
            "translate" => [1.0, 0.0, 0.0, 1.0, a(0, 0.0), a(1, 0.0)],
            "scale" => [a(0, 1.0), 0.0, 0.0, a(1, a(0, 1.0)), 0.0, 0.0],
            "rotate" => {
                let r = a(0, 0.0).to_radians();
                let (cx, cy) = (a(1, 0.0), a(2, 0.0));
                let rot = [r.cos(), r.sin(), -r.sin(), r.cos(), 0.0, 0.0];
                multiply(
                    multiply([1.0, 0.0, 0.0, 1.0, cx, cy], rot),
                    [1.0, 0.0, 0.0, 1.0, -cx, -cy],
                )
            }
            "skewX" => [1.0, 0.0, a(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0],
            "skewY" => [1.0, a(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => IDENTITY,
        };
        m = multiply(m, t);
        rest = &rest[open + close + 1..];
    }
    m
}

/// An attribute of a start tag (`<g transform="…">`).
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().last();
        rest = &rest[at + name.len()..];
        if before.is_some_and(char::is_whitespace) {
            let r = rest.trim_start();
            let Some(r) = r.strip_prefix('=') else {
                continue;
            };
            let r = r.trim_start();
            let q = r.chars().next()?;
            if q != '"' && q != '\'' {
                continue;
            }
            let r = &r[1..];
            return Some(&r[..r.find(q)?]);
        }
    }
}

/// The first `<path>` (else `<polygon>`) of an SVG, with the transform of it
/// and its parent groups: `(tag, matrix)`.
fn first_shape(svg: &str) -> Option<(String, Matrix)> {
    for wanted in ["path", "polygon"] {
        let mut stack: Vec<Matrix> = vec![IDENTITY];
        let mut i = 0;
        while let Some(lt) = svg[i..].find('<') {
            let start = i + lt;
            let end = start + svg[start..].find('>')?;
            let tag = &svg[start..=end];
            i = end + 1;
            if tag.starts_with("<!") || tag.starts_with("<?") {
                continue;
            }
            if tag.starts_with("</") {
                if stack.len() > 1 {
                    stack.pop();
                }
                continue;
            }
            let name: String = tag[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-')
                .collect();
            let own = attr(tag, "transform").map_or(IDENTITY, parse_transform);
            let m = multiply(*stack.last().expect("root"), own);
            if name == wanted {
                return Some((tag.to_string(), m));
            }
            if !tag.ends_with("/>") {
                stack.push(m);
            }
        }
    }
    None
}

/// An SVG file → polygon points fitted to `width` × `height`, and whether
/// they are bezier (FRAME's `svgUrlToPolygonPoints`).
pub fn svg_to_polygon(svg: &str, width: f64, height: f64) -> Option<(Vec<PolygonPoint>, bool)> {
    let (tag, m) = first_shape(svg)?;
    let (points, bezier) = if tag.starts_with("<path") {
        path_d_to_points(attr(&tag, "d")?)?
    } else {
        let nums: Vec<f64> = attr(&tag, "points")?
            .split([' ', ',', '\n', '\t'])
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        let mut pts: Vec<PolygonPoint> = nums.chunks_exact(2).map(|c| pt(c[0], c[1])).collect();
        if pts.len() >= 4 && near(&pts[0], &pts[pts.len() - 1]) {
            pts.pop();
        }
        if pts.len() < 3 {
            return None;
        }
        (pts, false)
    };
    let world = map_points(&points, |x, y| {
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    });
    let (min_x, min_y, max_x, max_y) = bounds(&world)?;
    let (tw, th) = (width.max(1.0), height.max(1.0));
    let (span_x, span_y) = ((max_x - min_x).max(1e-6), (max_y - min_y).max(1e-6));
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    Some((
        map_points(&world, |x, y| {
            (r3((x - min_x) / span_x * tw), r3((y - min_y) / span_y * th))
        }),
        bezier,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_become_vertices_with_handles() {
        let (pts, bezier) = path_d_to_points("M0,0 L10,0 L10,10 Z").unwrap();
        assert!(!bezier);
        assert_eq!(pts.len(), 3);
        let (pts, bezier) = path_d_to_points("M0 0 C 0 -5 10 -5 10 0 L 10 10 Z").unwrap();
        assert!(bezier);
        assert_eq!((pts[0].out_x, pts[0].out_y), (Some(0.0), Some(-5.0)));
        assert_eq!((pts[1].in_x, pts[1].in_y), (Some(10.0), Some(-5.0)));
        // Relative commands and implicit repeats.
        let (pts, _) = path_d_to_points("m5 5 l10 0 0 10 -10 0z").unwrap();
        assert_eq!((pts[2].x, pts[2].y), (15.0, 15.0));
        let (pts, _) = path_d_to_points("M1e1,2.5.5,3 L0 0").unwrap();
        assert_eq!(
            (pts[0].x, pts[0].y, pts[1].x, pts[1].y),
            (10.0, 2.5, 0.5, 3.0)
        );
    }

    #[test]
    fn stock_shape_fits_the_box() {
        let svg = r#"<?xml version="1.0"?><!DOCTYPE svg><svg viewBox="0 0 512 512">
            <g transform="matrix(2,0,0,2,-100,-50)"><path d="M60,30L110,30L85,80Z" style="fill:white;"/></g></svg>"#;
        let (pts, bezier) = svg_to_polygon(svg, 200.0, 100.0).unwrap();
        assert!(!bezier);
        let xs: Vec<f64> = pts.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.y).collect();
        assert_eq!(xs, [0.0, 200.0, 100.0]);
        assert_eq!(ys, [0.0, 0.0, 100.0]);
        // A polygon element, when there is no path.
        let poly = r#"<svg><polygon points="0,0 4,0 4,4 0,4"/></svg>"#;
        assert_eq!(svg_to_polygon(poly, 8.0, 8.0).unwrap().0.len(), 4);
        // Rotation from a group is applied before fitting.
        let rotated =
            r#"<svg><g transform="rotate(90)"><path d="M0 0 L10 0 L10 2 L0 2 Z"/></g></svg>"#;
        let (pts, _) = svg_to_polygon(rotated, 2.0, 10.0).unwrap();
        assert_eq!((pts[1].x, pts[1].y), (2.0, 10.0));
    }

    #[test]
    fn every_stock_shape_converts() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../FRAME/static/svg/basic");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return; // No FRAME checkout next to this repository.
        };
        let mut n = 0;
        for e in entries.flatten() {
            let text = std::fs::read_to_string(e.path()).unwrap();
            if !text.contains("<path") && !text.contains("<polygon") {
                // FRAME inserts these (an ellipse) as a tinted SVG instead.
                assert!(svg_to_polygon(&text, 256.0, 256.0).is_none());
                continue;
            }
            let (pts, _) = svg_to_polygon(&text, 256.0, 256.0)
                .unwrap_or_else(|| panic!("{}", e.path().display()));
            assert!(pts.len() >= 3);
            assert!(
                pts.iter()
                    .all(|p| (0.0..=256.0).contains(&p.x) && (0.0..=256.0).contains(&p.y))
            );
            n += 1;
        }
        assert_eq!(n, 39);
    }

    #[test]
    fn panel_helpers_match_frame() {
        let quad = default_polygon_points(100.0, 50.0);
        assert_eq!(
            (quad[0].x, quad[0].y, quad[2].x, quad[2].y),
            (12.0, 6.0, 88.0, 44.0)
        );
        let added = insert_on_longest_edge(&quad);
        assert_eq!((added.len(), added[1].x, added[1].y), (5, 50.0, 6.0));
        let smooth = normalize_handles(&quad, "bezier");
        assert!(any_bezier(&smooth) && smooth[0].in_x != Some(12.0));
        assert!(!any_bezier(&normalize_handles(&smooth, "linear")));
    }

    #[test]
    fn set_polygon_fits_the_box_without_moving() {
        let mut item = crate::layers::new_layer(crate::layers::NewLayer::Box, None);
        item.id = "p".into();
        item.shape_type = Some(ShapeType::Polygon);
        item.x = 100.0;
        item.y = 100.0;
        let g = serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut doc = crate::normalize::normalize(&g, &crate::profile::Profile::framecraft());
        doc.items.push(item);
        let pts = vec![pt(10.0, 20.0), pt(50.0, 20.0), pt(30.0, 60.0)];
        SetPolygon {
            item: "p".into(),
            points: Some(pts),
            curve: None,
            to_shape: false,
        }
        .apply(&mut doc)
        .unwrap();
        let it = &doc.items[0];
        assert_eq!(
            (it.x, it.y, it.width, it.height),
            (110.0, 120.0, 40.0, 40.0)
        );
        assert_eq!(it.polygon_points.as_ref().unwrap()[0].x, 0.0);
    }
}
