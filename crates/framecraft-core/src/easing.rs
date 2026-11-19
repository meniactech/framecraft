// SPDX-License-Identifier: AGPL-3.0-or-later
//! Keyframe easing curves. Ids match FRAME and the exported runtime.

use std::f64::consts::PI;

/// Every easing FrameCraft knows, in FRAME's menu order.
pub const EASING_IDS: [&str; 31] = [
    "linear",
    "ease-in-sine",
    "ease-out-sine",
    "ease-in-out-sine",
    "ease-in-quad",
    "ease-out-quad",
    "ease-in-out-quad",
    "ease-in-cubic",
    "ease-out-cubic",
    "ease-in-out-cubic",
    "ease-in-quart",
    "ease-out-quart",
    "ease-in-out-quart",
    "ease-in-quint",
    "ease-out-quint",
    "ease-in-out-quint",
    "ease-in-expo",
    "ease-out-expo",
    "ease-in-out-expo",
    "ease-in-circ",
    "ease-out-circ",
    "ease-in-out-circ",
    "ease-in-back",
    "ease-out-back",
    "ease-in-out-back",
    "ease-in-elastic",
    "ease-out-elastic",
    "ease-in-out-elastic",
    "ease-in-bounce",
    "ease-out-bounce",
    "ease-in-out-bounce",
];

/// A known easing id for any stored value: known ids pass, three legacy
/// camelCase names are mapped, anything else is `linear`.
pub fn normalize(value: Option<&str>) -> &'static str {
    let Some(v) = value.filter(|v| !v.is_empty()) else {
        return "linear";
    };
    if let Some(id) = EASING_IDS.iter().find(|id| **id == v) {
        return id;
    }
    match v {
        "easeInQuad" => "ease-in-quad",
        "easeOutQuad" => "ease-out-quad",
        "easeInOutQuad" => "ease-in-out-quad",
        _ => "linear",
    }
}

/// Eased progress for `t` in 0–1, exactly as exported graphics play it.
///
/// The playout runtime eases with anime.js v4 (`eases.outQuad`, …; Back and
/// Elastic with their default settings), so these are anime.js's
/// definitions: one "in" curve per family, with "out" and "in-out" derived
/// from it the way anime.js derives them. Unknown ids behave as linear.
pub fn apply(id: &str, t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    let Some(rest) = id.strip_prefix("ease-") else {
        return x;
    };
    let (mode, family) = if let Some(f) = rest.strip_prefix("in-out-") {
        (Mode::InOut, f)
    } else if let Some(f) = rest.strip_prefix("out-") {
        (Mode::Out, f)
    } else if let Some(f) = rest.strip_prefix("in-") {
        (Mode::In, f)
    } else {
        return x;
    };
    let curve: fn(f64) -> f64 = match family {
        "quad" => |t| t.powf(2.0),
        "cubic" => |t| t.powf(3.0),
        "quart" => |t| t.powf(4.0),
        "quint" => |t| t.powf(5.0),
        "sine" => |t| 1.0 - (t * PI / 2.0).cos(),
        "circ" => |t| 1.0 - (1.0 - t * t).sqrt(),
        "expo" => |t| {
            if t == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * t - 10.0)
            }
        },
        "bounce" => bounce_in,
        "back" => |t| {
            let o = 1.7;
            (o + 1.0) * t * t * t - o * t * t
        },
        "elastic" => elastic_in,
        _ => return x,
    };
    match mode {
        Mode::In => curve(x),
        Mode::Out => 1.0 - curve(1.0 - x),
        Mode::InOut => {
            if x < 0.5 {
                curve(2.0 * x) / 2.0
            } else {
                1.0 - curve(-2.0 * x + 2.0) / 2.0
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Mode {
    In,
    Out,
    InOut,
}

/// anime.js `Bounce`.
fn bounce_in(t: f64) -> f64 {
    let mut s = 4i32;
    let mut e;
    loop {
        s -= 1;
        e = 2f64.powi(s);
        if t >= (e - 1.0) / 11.0 {
            break;
        }
    }
    1.0 / 4f64.powi(3 - s) - 7.5625 * ((3.0 * e - 2.0) / 22.0 - t).powf(2.0)
}

/// anime.js `Elastic` with its defaults (amplitude 1, period 0.3).
fn elastic_in(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let (amplitude, period) = (1.0f64, 0.3f64);
    let r = period / (2.0 * PI) * (1.0 / amplitude).asin();
    let n = 2.0 * PI / period;
    -amplitude * 2f64.powf(-10.0 * (1.0 - t)) * ((1.0 - t - r) * n).sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ids() {
        assert_eq!(normalize(None), "linear");
        assert_eq!(normalize(Some("")), "linear");
        assert_eq!(normalize(Some("ease-out-back")), "ease-out-back");
        assert_eq!(normalize(Some("easeInOutQuad")), "ease-in-out-quad");
        assert_eq!(normalize(Some("bogus")), "linear");
    }

    /// Every curve against anime.js itself (201 samples each, made by
    /// running the runtime's easing function in Node).
    #[test]
    fn matches_anime_js() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/eases.json")).unwrap();
        let samples = data["samples"].as_object().unwrap();
        assert_eq!(samples.len(), EASING_IDS.len());
        for id in EASING_IDS {
            for (i, want) in samples[id].as_array().unwrap().iter().enumerate() {
                let want = want.as_f64().unwrap();
                let got = apply(id, i as f64 / 200.0);
                assert!(
                    (got - want).abs() < 1e-12,
                    "{id} at t={}: {got} vs anime.js {want}",
                    i as f64 / 200.0
                );
            }
        }
    }

    #[test]
    fn curves_start_at_0_and_end_at_1() {
        for id in EASING_IDS {
            assert!(apply(id, 0.0).abs() < 1e-9, "{id} at 0");
            assert!((apply(id, 1.0) - 1.0).abs() < 1e-9, "{id} at 1");
        }
        assert_eq!(apply("linear", 0.25), 0.25);
        assert_eq!(apply("ease-in-quad", 0.5), 0.25);
    }
}
