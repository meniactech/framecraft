// SPDX-License-Identifier: AGPL-3.0-or-later
//! Timeline evaluation: the value of an animated property at a time, as
//! exported graphics play it.
//!
//! A port of the playout runtime's `interpolateTrack`: keyframes sorted by
//! time, the first value before the first key, the last value after the
//! last key, and between two keys the easing of the key being moved to.
//! Checked against the runtime itself on the reference graphics.

use crate::easing;
use crate::model::{Item, ItemTrack, Keyframe};
use crate::normalize::{CLIP_START, Document};

/// The value of `keys` at `time_ms`, or `None` when there are no keys.
pub fn value_at(keys: &[Keyframe], time_ms: f64) -> Option<f64> {
    if keys.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Keyframe> = keys.iter().collect();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    let (first, last) = (sorted[0], sorted[sorted.len() - 1]);
    if time_ms <= first.time {
        return Some(first.value);
    }
    if time_ms >= last.time {
        return Some(last.value);
    }
    for pair in sorted.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if time_ms >= a.time && time_ms <= b.time {
            let linear = (time_ms - a.time) / (b.time - a.time).max(1.0);
            let eased = easing::apply(easing::normalize(b.easing.as_deref()), linear);
            return Some(a.value + (b.value - a.value) * eased);
        }
    }
    None
}

fn track<'a>(doc: &'a Document, item_id: &str) -> Option<&'a ItemTrack> {
    doc.timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == item_id)
}

/// Whether a lane takes part in playback: switched on (or, without a
/// switch, having keys).
pub fn lane_enabled(track: &ItemTrack, lane: &str) -> bool {
    match track.enabled_properties.as_ref().and_then(|e| e.get(lane)) {
        Some(on) => *on,
        None => track.properties.get(lane).is_some_and(|k| !k.is_empty()),
    }
}

/// The keyframes that animate `lane` of an item, if it is animated.
/// The Lottie / video start lane is never animated.
pub fn lane_keys<'a>(doc: &'a Document, item_id: &str, lane: &str) -> Option<&'a [Keyframe]> {
    if lane == CLIP_START {
        return None;
    }
    let t = track(doc, item_id)?;
    let keys = t.properties.get(lane)?;
    (lane_enabled(t, lane) && !keys.is_empty()).then_some(keys.as_slice())
}

/// The animated value of `lane` at `time_ms`, or `None` when the lane is
/// not animated (then the item's own value applies).
pub fn lane_value(doc: &Document, item_id: &str, lane: &str, time_ms: f64) -> Option<f64> {
    value_at(lane_keys(doc, item_id, lane)?, time_ms)
}

/// Lanes of an item that are animated, sorted by name.
pub fn animated_lanes(doc: &Document, item_id: &str) -> Vec<String> {
    let Some(t) = track(doc, item_id) else {
        return Vec::new();
    };
    t.properties
        .iter()
        .filter(|(lane, keys)| {
            lane.as_str() != CLIP_START && !keys.is_empty() && lane_enabled(t, lane)
        })
        .map(|(lane, _)| lane.clone())
        .collect()
}

/// Position, size, rotation, scale and opacity of a layer at a time, in
/// its parent's coordinates. Pivot is not animatable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
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

/// The item's own (not animated) value of a transform lane.
pub fn base_value(item: &Item, lane: &str) -> Option<f64> {
    Some(match lane {
        "x" => item.x,
        "y" => item.y,
        "width" => item.width,
        "height" => item.height,
        "rotation" => item.rotation,
        "scaleX" => item.scale_x.unwrap_or(1.0),
        "scaleY" => item.scale_y.unwrap_or(1.0),
        "opacity" => item.opacity,
        _ => return None,
    })
}

/// A layer's pose at `time_ms`.
pub fn pose_at(doc: &Document, item: &Item, time_ms: f64) -> Pose {
    let v = |lane: &str| {
        lane_value(doc, &item.id, lane, time_ms)
            .unwrap_or_else(|| base_value(item, lane).unwrap_or(0.0))
    };
    Pose {
        x: v("x"),
        y: v("y"),
        width: v("width"),
        height: v("height"),
        rotation: v("rotation"),
        scale_x: v("scaleX"),
        scale_y: v("scaleY"),
        pivot_x: item.pivot_x.unwrap_or(50.0),
        pivot_y: item.pivot_y.unwrap_or(50.0),
        opacity: v("opacity"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(time: f64, value: f64, easing: Option<&str>) -> Keyframe {
        Keyframe {
            time,
            value,
            easing: easing.map(String::from),
            extra: Default::default(),
        }
    }

    #[test]
    fn holds_outside_the_keys() {
        let keys = [key(1000.0, 10.0, None), key(2000.0, 20.0, None)];
        assert_eq!(value_at(&keys, 0.0), Some(10.0));
        assert_eq!(value_at(&keys, 1000.0), Some(10.0));
        assert_eq!(value_at(&keys, 2500.0), Some(20.0));
        assert_eq!(value_at(&[], 5.0), None);
    }

    #[test]
    fn interpolates_with_the_target_keys_easing() {
        let keys = [
            key(0.0, 0.0, Some("ease-out-quad")),
            key(1000.0, 100.0, Some("ease-in-quad")),
        ];
        // Easing of the key moved to (in-quad at 0.5 = 0.25).
        assert!((value_at(&keys, 500.0).unwrap() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn unsorted_keys_and_legacy_easing() {
        let keys = [key(1000.0, 100.0, Some("easeInQuad")), key(0.0, 0.0, None)];
        assert!((value_at(&keys, 500.0).unwrap() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn same_time_keys_do_not_divide_by_zero() {
        let keys = [
            key(0.0, 0.0, None),
            key(500.0, 10.0, None),
            key(500.0, 20.0, None),
            key(1000.0, 30.0, None),
        ];
        assert_eq!(value_at(&keys, 500.0), Some(10.0));
        assert!(value_at(&keys, 750.0).unwrap().is_finite());
    }
}
