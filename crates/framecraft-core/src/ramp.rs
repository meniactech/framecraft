// SPDX-License-Identifier: AGPL-3.0-or-later
//! 0→1 ramps: gradient fills, particle color, size and alpha over life.

use crate::model::{RampColor, RampValue};

pub const DEFAULT_COLOR_START: &str = "rgba(255, 255, 255, 1)";
pub const DEFAULT_COLOR_END: &str = "rgba(255, 255, 255, 0)";

fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn color(offset: f64, color: &str) -> RampColor {
    RampColor {
        offset,
        color: color.to_string(),
        extra: Default::default(),
    }
}

pub fn value(offset: f64, value: f64) -> RampValue {
    RampValue {
        offset,
        value,
        extra: Default::default(),
    }
}

fn default_color_ramp(start: &str, end: &str) -> Vec<RampColor> {
    vec![color(0.0, start), color(1.0, end)]
}

/// Sorts, clamps, merges equal offsets (last wins) and pins the ends at 0
/// and 1. Always returns at least two stops.
pub fn normalize_color_ramp(stops: Option<&[RampColor]>) -> Vec<RampColor> {
    let fallback = || default_color_ramp(DEFAULT_COLOR_START, DEFAULT_COLOR_END);
    let Some(stops) = stops.filter(|s| !s.is_empty()) else {
        return fallback();
    };
    let mut cleaned: Vec<RampColor> = stops
        .iter()
        .map(|s| {
            let c = s.color.trim();
            color(
                clamp01(s.offset),
                if c.is_empty() { DEFAULT_COLOR_START } else { c },
            )
        })
        .collect();
    cleaned.sort_by(|a, b| a.offset.total_cmp(&b.offset));

    let mut out: Vec<RampColor> = Vec::new();
    for stop in cleaned {
        match out.last_mut() {
            Some(prev) if (prev.offset - stop.offset).abs() < 1e-6 => prev.color = stop.color,
            _ => out.push(stop),
        }
    }
    pin_ends(
        &mut out,
        |s| s.offset,
        |s, o| s.offset = o,
        |s, o| color(o, &s.color),
    );
    if out.len() < 2 {
        return default_color_ramp(&out[0].color, DEFAULT_COLOR_END);
    }
    out
}

/// A normalized color ramp from a stored ramp (two or more stops), or from
/// legacy start and end colors.
pub fn color_ramp_from_legacy(
    ramp: Option<&[RampColor]>,
    start: Option<&str>,
    end: Option<&str>,
) -> Vec<RampColor> {
    if let Some(r) = ramp.filter(|r| r.len() >= 2) {
        return normalize_color_ramp(Some(r));
    }
    let start = start
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_COLOR_START);
    let end = end.filter(|s| !s.is_empty()).unwrap_or(DEFAULT_COLOR_END);
    normalize_color_ramp(Some(&default_color_ramp(start, end)))
}

/// As [`normalize_color_ramp`], for number ramps.
pub fn normalize_value_ramp(
    stops: Option<&[RampValue]>,
    fallback_start: f64,
    fallback_end: f64,
) -> Vec<RampValue> {
    let Some(stops) = stops.filter(|s| !s.is_empty()) else {
        return vec![value(0.0, fallback_start), value(1.0, fallback_end)];
    };
    let mut cleaned: Vec<RampValue> = stops
        .iter()
        .map(|s| {
            value(
                clamp01(s.offset),
                if s.value.is_finite() {
                    s.value
                } else {
                    fallback_start
                },
            )
        })
        .collect();
    cleaned.sort_by(|a, b| a.offset.total_cmp(&b.offset));

    let mut out: Vec<RampValue> = Vec::new();
    for stop in cleaned {
        match out.last_mut() {
            Some(prev) if (prev.offset - stop.offset).abs() < 1e-6 => prev.value = stop.value,
            _ => out.push(stop),
        }
    }
    pin_ends(
        &mut out,
        |s| s.offset,
        |s, o| s.offset = o,
        |s, o| value(o, s.value),
    );
    if out.len() < 2 {
        return vec![value(0.0, out[0].value), value(1.0, fallback_end)];
    }
    out
}

/// Particle size over life as 0–1 multipliers of the base size. Ramps from
/// older builds stored absolute pixels; those are divided back down.
pub fn size_scale_ramp_from_legacy(
    ramp: Option<&[RampValue]>,
    base_size: Option<f64>,
    size_end: Option<f64>,
) -> Vec<RampValue> {
    let base = base_size.filter(|v| v.is_finite()).unwrap_or(8.0).max(0.01);
    let end_abs = size_end.filter(|v| v.is_finite()).unwrap_or(base * 0.25);

    if let Some(r) = ramp.filter(|r| r.len() >= 2) {
        let peak = r
            .iter()
            .map(|s| s.value)
            .filter(|v| v.is_finite())
            .fold(0.0, f64::max);
        let divisor = if peak > 1.0001 { base.max(peak) } else { 1.0 };
        let scaled: Vec<RampValue> = r
            .iter()
            .map(|s| value(s.offset, clamp01(s.value / divisor)))
            .collect();
        return normalize_value_ramp(Some(&scaled), 0.0, 1.0);
    }
    normalize_value_ramp(
        Some(&[value(0.0, 1.0), value(1.0, clamp01(end_abs / base))]),
        0.0,
        1.0,
    )
}

/// Puts the first stop at 0 and the last at 1, adding a stop when needed.
fn pin_ends<T>(
    stops: &mut Vec<T>,
    offset: impl Fn(&T) -> f64,
    set: impl Fn(&mut T, f64),
    copy_at: impl Fn(&T, f64) -> T,
) {
    if offset(&stops[0]) > 0.0 {
        let first = copy_at(&stops[0], 0.0);
        stops.insert(0, first);
    } else {
        set(&mut stops[0], 0.0);
    }
    let last = stops.len() - 1;
    if offset(&stops[last]) < 1.0 {
        let end = copy_at(&stops[last], 1.0);
        stops.push(end);
    } else {
        set(&mut stops[last], 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_ramp_pins_sorts_and_merges() {
        let r = normalize_color_ramp(Some(&[color(0.8, "b"), color(0.2, "a"), color(0.2, "c")]));
        let got: Vec<(f64, &str)> = r.iter().map(|s| (s.offset, s.color.as_str())).collect();
        // 0.2 is the first stop, so a copy of it is added at 0.
        assert_eq!(got, vec![(0.0, "c"), (0.2, "c"), (0.8, "b"), (1.0, "b")]);
    }

    #[test]
    fn legacy_colors_make_a_two_stop_ramp() {
        let r = color_ramp_from_legacy(None, Some("red"), None);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].color, "red");
        assert_eq!(r[1].color, DEFAULT_COLOR_END);
    }

    #[test]
    fn single_value_stop_gets_fallback_end() {
        let r = normalize_value_ramp(Some(&[value(0.0, 3.0)]), 1.0, 9.0);
        // A single stop at 0 is pinned to 1 as well, so two stops result.
        assert_eq!(r.len(), 2);
        assert_eq!((r[1].offset, r[1].value), (1.0, 3.0));
    }

    #[test]
    fn absolute_size_ramp_is_scaled() {
        let r = size_scale_ramp_from_legacy(
            Some(&[value(0.0, 16.0), value(1.0, 4.0)]),
            Some(8.0),
            None,
        );
        assert_eq!(r[0].value, 1.0);
        assert_eq!(r[1].value, 0.25);
        let d = size_scale_ramp_from_legacy(None, Some(8.0), Some(2.0));
        assert_eq!((d[0].value, d[1].value), (1.0, 0.25));
    }
}
