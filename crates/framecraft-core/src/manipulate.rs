// SPDX-License-Identifier: AGPL-3.0-or-later
//! Geometry of direct manipulation: move, resize, rotate and pivot drags.
//!
//! A layer is drawn like CSS draws it: its box sits at `(x, y)` in its
//! parent's coordinates, and is rotated and scaled around its pivot
//! (`transform-origin: pivotX% pivotY%; transform: rotate(r) scale(sx, sy)`).
//! A point `p` of the box lands in the parent at
//!
//! ```text
//! q(p) = (x, y) + c + M (p - c),   c = (w · pivotX/100, h · pivotY/100),   M = R(r) · S(sx, sy)
//! ```
//!
//! The parent's own placement in the scene is a [`Frame`], measured from
//! the rendered page. Every drag computes absolute values from where it
//! started, so nothing accumulates rounding.

use crate::timeline::Pose;

pub type Vec2 = (f64, f64);

fn add(a: Vec2, b: Vec2) -> Vec2 {
    (a.0 + b.0, a.1 + b.1)
}
fn sub(a: Vec2, b: Vec2) -> Vec2 {
    (a.0 - b.0, a.1 - b.1)
}

/// A parent's coordinate frame in the scene: where its local origin and
/// unit vectors land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: Vec2,
    pub ux: Vec2,
    pub uy: Vec2,
}

impl Frame {
    /// The scene itself.
    pub const SCENE: Frame = Frame {
        origin: (0.0, 0.0),
        ux: (1.0, 0.0),
        uy: (0.0, 1.0),
    };

    fn det(&self) -> f64 {
        self.ux.0 * self.uy.1 - self.uy.0 * self.ux.1
    }

    /// A scene direction in parent units (no origin).
    pub fn vector_to_local(&self, v: Vec2) -> Vec2 {
        let d = self.det();
        if d.abs() < 1e-12 {
            return v;
        }
        (
            (v.0 * self.uy.1 - v.1 * self.uy.0) / d,
            (self.ux.0 * v.1 - self.ux.1 * v.0) / d,
        )
    }

    /// A scene point in parent coordinates.
    pub fn to_local(&self, p: Vec2) -> Vec2 {
        self.vector_to_local(sub(p, self.origin))
    }

    pub fn to_scene(&self, p: Vec2) -> Vec2 {
        (
            self.origin.0 + self.ux.0 * p.0 + self.uy.0 * p.1,
            self.origin.1 + self.ux.1 * p.0 + self.uy.1 * p.1,
        )
    }
}

/// `M = R(r) · S(sx, sy)` as `[a, b, c, d]` (x' = a·x + c·y, y' = b·x + d·y).
fn matrix(pose: &Pose) -> [f64; 4] {
    let (s, c) = pose.rotation.to_radians().sin_cos();
    [
        c * pose.scale_x,
        s * pose.scale_x,
        -s * pose.scale_y,
        c * pose.scale_y,
    ]
}

fn apply(m: [f64; 4], v: Vec2) -> Vec2 {
    (m[0] * v.0 + m[2] * v.1, m[1] * v.0 + m[3] * v.1)
}

fn invert(m: [f64; 4], v: Vec2) -> Vec2 {
    let d = m[0] * m[3] - m[2] * m[1];
    if d.abs() < 1e-12 {
        return v;
    }
    ((m[3] * v.0 - m[2] * v.1) / d, (m[0] * v.1 - m[1] * v.0) / d)
}

fn pivot(pose: &Pose) -> Vec2 {
    (
        pose.width * pose.pivot_x / 100.0,
        pose.height * pose.pivot_y / 100.0,
    )
}

/// Where a point of the layer's box lands in its parent.
pub fn box_to_parent(pose: &Pose, p: Vec2) -> Vec2 {
    let c = pivot(pose);
    add(add((pose.x, pose.y), c), apply(matrix(pose), sub(p, c)))
}

/// A parent point in the layer's own (untransformed) box coordinates.
pub fn parent_to_box(pose: &Pose, q: Vec2) -> Vec2 {
    let c = pivot(pose);
    add(c, invert(matrix(pose), sub(sub(q, (pose.x, pose.y)), c)))
}

/// The layer's corners in the scene: top-left, top-right, bottom-right,
/// bottom-left.
pub fn corners(pose: &Pose, parent: &Frame) -> [Vec2; 4] {
    let (w, h) = (pose.width, pose.height);
    [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|p| parent.to_scene(box_to_parent(pose, p)))
}

/// Moves by the mouse travel since the drag started. With `constrain`
/// (Shift) only along the axis the mouse moved most.
pub fn move_by(start: &Pose, parent: &Frame, from: Vec2, to: Vec2, constrain: bool) -> Vec2 {
    let mut d = sub(to, from);
    if constrain {
        if d.0.abs() >= d.1.abs() {
            d.1 = 0.0;
        } else {
            d.0 = 0.0;
        }
    }
    let local = parent.vector_to_local(d);
    (start.x + local.0, start.y + local.1)
}

/// Which edges a resize handle moves: -1 left / top, 0 none, 1 right / bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    pub x: i8,
    pub y: i8,
}

impl Handle {
    /// The eight handles, clockwise from the top-left corner.
    pub const ALL: [Handle; 8] = [
        Handle { x: -1, y: -1 },
        Handle { x: 0, y: -1 },
        Handle { x: 1, y: -1 },
        Handle { x: 1, y: 0 },
        Handle { x: 1, y: 1 },
        Handle { x: 0, y: 1 },
        Handle { x: -1, y: 1 },
        Handle { x: -1, y: 0 },
    ];

    /// Position in the box, as fractions of width and height.
    pub fn at(&self) -> Vec2 {
        (
            (f64::from(self.x) + 1.0) / 2.0,
            (f64::from(self.y) + 1.0) / 2.0,
        )
    }
}

/// New `(x, y, width, height)` for a resize drag. The opposite edge stays
/// where it is on screen (or the center, with `from_center`); `keep_ratio`
/// keeps the start proportions.
pub fn resize(
    start: &Pose,
    parent: &Frame,
    handle: Handle,
    from: Vec2,
    to: Vec2,
    keep_ratio: bool,
    from_center: bool,
) -> (f64, f64, f64, f64) {
    let (w, h) = (start.width, start.height);
    // Mouse travel in the box's own units.
    let p0 = parent_to_box(start, parent.to_local(from));
    let p1 = parent_to_box(start, parent.to_local(to));
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    let grow = if from_center { 2.0 } else { 1.0 };
    let mut nw = w + f64::from(handle.x) * dx * grow;
    let mut nh = h + f64::from(handle.y) * dy * grow;
    if keep_ratio && w > 0.0 && h > 0.0 {
        let (fw, fh) = (nw / w, nh / h);
        let f = match (handle.x, handle.y) {
            (0, _) => fh,
            (_, 0) => fw,
            _ if (fw - 1.0).abs() >= (fh - 1.0).abs() => fw,
            _ => fh,
        };
        nw = w * f;
        nh = h * f;
    }
    let (nw, nh) = (nw.max(1.0), nh.max(1.0));

    // The point that must not move: the opposite edge or corner, or the
    // center. Its coordinates in the old and the new box:
    let anchor = |size: f64, new_size: f64, side: i8| -> (f64, f64) {
        if from_center {
            (size / 2.0, new_size / 2.0)
        } else {
            match side {
                1 => (0.0, 0.0),        // right / bottom handle: left / top edge stays
                -1 => (size, new_size), // left / top handle: right / bottom edge stays
                _ => (size / 2.0, new_size / 2.0),
            }
        }
    };
    let (ax, nax) = anchor(w, nw, handle.x);
    let (ay, nay) = anchor(h, nh, handle.y);
    let fixed = box_to_parent(start, (ax, ay));
    let next = Pose {
        width: nw,
        height: nh,
        ..*start
    };
    let c = pivot(&next);
    let (x, y) = sub(sub(fixed, c), apply(matrix(&next), sub((nax, nay), c)));
    (x, y, nw, nh)
}

/// The pivot in the scene.
pub fn pivot_in_scene(pose: &Pose, parent: &Frame) -> Vec2 {
    parent.to_scene(box_to_parent(pose, pivot(pose)))
}

/// New rotation for a rotate drag around the pivot. With `snap` (Shift)
/// the result is rounded to 15°.
pub fn rotate(start: &Pose, parent: &Frame, from: Vec2, to: Vec2, snap: bool) -> f64 {
    let c = pivot_in_scene(start, parent);
    let a0 = (from.1 - c.1).atan2(from.0 - c.0);
    let a1 = (to.1 - c.1).atan2(to.0 - c.0);
    let r = start.rotation + (a1 - a0).to_degrees();
    if snap { (r / 15.0).round() * 15.0 } else { r }
}

/// New `(x, y, pivotX, pivotY)` for a pivot drag: the pivot follows the
/// mouse and the layer stays where it is on screen.
pub fn move_pivot(start: &Pose, parent: &Frame, to: Vec2) -> (f64, f64, f64, f64) {
    let p = parent_to_box(start, parent.to_local(to));
    let (w, h) = (start.width.max(1e-9), start.height.max(1e-9));
    let (px, py) = (p.0 / w * 100.0, p.1 / h * 100.0);
    let next = Pose {
        pivot_x: px,
        pivot_y: py,
        ..*start
    };
    // Keep the top-left corner in place.
    let fixed = box_to_parent(start, (0.0, 0.0));
    let c = pivot(&next);
    let (x, y) = sub(sub(fixed, c), apply(matrix(&next), (-c.0, -c.1)));
    (x, y, px, py)
}

/// Whether a scene point is inside a convex quad.
pub fn inside(quad: &[Vec2; 4], p: Vec2) -> bool {
    let mut sign = 0.0f64;
    for i in 0..4 {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if cross.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(rotation: f64, sx: f64, sy: f64) -> Pose {
        Pose {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
            rotation,
            scale_x: sx,
            scale_y: sy,
            pivot_x: 50.0,
            pivot_y: 50.0,
            opacity: 1.0,
        }
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn box_mapping_round_trips() {
        let p = pose(33.0, 1.5, 0.75);
        for pt in [(0.0, 0.0), (200.0, 0.0), (12.5, 77.0)] {
            assert!(close(parent_to_box(&p, box_to_parent(&p, pt)), pt));
        }
        // Unrotated, unscaled: the box sits at (x, y).
        let flat = pose(0.0, 1.0, 1.0);
        assert!(close(box_to_parent(&flat, (0.0, 0.0)), (100.0, 50.0)));
        // 90° about the center: top-left lands at center + (h/2, -w/2).
        let turned = pose(90.0, 1.0, 1.0);
        assert!(close(box_to_parent(&turned, (0.0, 0.0)), (250.0, 0.0)));
    }

    #[test]
    fn move_follows_the_parent_frame() {
        let p = pose(0.0, 1.0, 1.0);
        assert_eq!(
            move_by(&p, &Frame::SCENE, (0.0, 0.0), (10.0, 4.0), false),
            (110.0, 54.0)
        );
        assert_eq!(
            move_by(&p, &Frame::SCENE, (0.0, 0.0), (10.0, 4.0), true),
            (110.0, 50.0)
        );
        // A parent scaled by 2: the layer moves half as far in its parent's units.
        let parent = Frame {
            origin: (0.0, 0.0),
            ux: (2.0, 0.0),
            uy: (0.0, 2.0),
        };
        assert_eq!(
            move_by(&p, &parent, (0.0, 0.0), (10.0, 4.0), false),
            (105.0, 52.0)
        );
    }

    #[test]
    fn resize_keeps_the_opposite_edge_on_screen() {
        for (rot, sx, sy) in [
            (0.0, 1.0, 1.0),
            (30.0, 1.0, 1.0),
            (90.0, 2.0, 0.5),
            (-140.0, 1.3, 1.3),
        ] {
            let p = pose(rot, sx, sy);
            for handle in Handle::ALL {
                let (hx, hy) = handle.at();
                let grab = Frame::SCENE.to_scene(box_to_parent(&p, (hx * p.width, hy * p.height)));
                let drop = (grab.0 + 17.0, grab.1 - 9.0);
                let (x, y, w, h) = resize(&p, &Frame::SCENE, handle, grab, drop, false, false);
                let next = Pose {
                    x,
                    y,
                    width: w,
                    height: h,
                    ..p
                };
                // The opposite point, in old and new box fractions.
                let opp = (1.0 - hx, 1.0 - hy);
                let before = box_to_parent(&p, (opp.0 * p.width, opp.1 * p.height));
                let after = box_to_parent(&next, (opp.0 * w, opp.1 * h));
                assert!(
                    close(before, after),
                    "rot {rot} handle {handle:?}: {before:?} -> {after:?}"
                );
            }
        }
    }

    #[test]
    fn resize_from_center_and_with_ratio() {
        let p = pose(0.0, 1.0, 1.0);
        let right = Handle { x: 1, y: 0 };
        let (x, _, w, h) = resize(
            &p,
            &Frame::SCENE,
            right,
            (300.0, 100.0),
            (320.0, 100.0),
            false,
            true,
        );
        assert_eq!((x, w, h), (80.0, 240.0, 100.0));
        let corner = Handle { x: 1, y: 1 };
        let (_, _, w, h) = resize(
            &p,
            &Frame::SCENE,
            corner,
            (300.0, 150.0),
            (400.0, 160.0),
            true,
            false,
        );
        assert_eq!((w, h), (300.0, 150.0));
        // Never smaller than one unit.
        let (_, _, w, _) = resize(
            &p,
            &Frame::SCENE,
            right,
            (300.0, 100.0),
            (-500.0, 100.0),
            false,
            false,
        );
        assert_eq!(w, 1.0);
    }

    #[test]
    fn rotate_around_the_pivot() {
        let p = pose(0.0, 1.0, 1.0);
        let c = pivot_in_scene(&p, &Frame::SCENE);
        let r = rotate(
            &p,
            &Frame::SCENE,
            (c.0 + 10.0, c.1),
            (c.0, c.1 + 10.0),
            false,
        );
        assert!((r - 90.0).abs() < 1e-9);
        let r = rotate(
            &p,
            &Frame::SCENE,
            (c.0 + 10.0, c.1),
            (c.0 + 10.0, c.1 + 2.0),
            true,
        );
        assert_eq!(r, 15.0);
    }

    #[test]
    fn pivot_moves_without_moving_the_layer() {
        let p = pose(37.0, 1.4, 0.8);
        let before = corners(&p, &Frame::SCENE);
        let (x, y, px, py) = move_pivot(&p, &Frame::SCENE, before[0]);
        assert!((px).abs() < 1e-6 && (py).abs() < 1e-6);
        let after = corners(
            &Pose {
                x,
                y,
                pivot_x: px,
                pivot_y: py,
                ..p
            },
            &Frame::SCENE,
        );
        for (a, b) in before.iter().zip(&after) {
            assert!(close(*a, *b));
        }
    }

    #[test]
    fn inside_a_rotated_quad() {
        let q = corners(&pose(45.0, 1.0, 1.0), &Frame::SCENE);
        assert!(inside(&q, (200.0, 100.0)));
        assert!(!inside(&q, (100.0, 50.0)));
    }
}
