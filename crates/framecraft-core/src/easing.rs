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

/// Eased progress for `t` in 0–1. Unknown ids behave as linear.
pub fn apply(id: &str, t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    match id {
        "ease-in-sine" => 1.0 - (x * PI / 2.0).cos(),
        "ease-out-sine" => (x * PI / 2.0).sin(),
        "ease-in-out-sine" => -((PI * x).cos() - 1.0) / 2.0,
        "ease-in-quad" => x * x,
        "ease-out-quad" => 1.0 - (1.0 - x) * (1.0 - x),
        "ease-in-out-quad" => {
            if x < 0.5 {
                2.0 * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(2) / 2.0
            }
        }
        "ease-in-cubic" => x * x * x,
        "ease-out-cubic" => 1.0 - (1.0 - x).powi(3),
        "ease-in-out-cubic" => {
            if x < 0.5 {
                4.0 * x * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
            }
        }
        "ease-in-quart" => x * x * x * x,
        "ease-out-quart" => 1.0 - (1.0 - x).powi(4),
        "ease-in-out-quart" => {
            if x < 0.5 {
                8.0 * x * x * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(4) / 2.0
            }
        }
        "ease-in-quint" => x * x * x * x * x,
        "ease-out-quint" => 1.0 - (1.0 - x).powi(5),
        "ease-in-out-quint" => {
            if x < 0.5 {
                16.0 * x * x * x * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(5) / 2.0
            }
        }
        "ease-in-expo" => {
            if x == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * x - 10.0)
            }
        }
        "ease-out-expo" => {
            if x == 1.0 {
                1.0
            } else {
                1.0 - 2f64.powf(-10.0 * x)
            }
        }
        "ease-in-out-expo" => {
            if x == 0.0 {
                0.0
            } else if x == 1.0 {
                1.0
            } else if x < 0.5 {
                2f64.powf(20.0 * x - 10.0) / 2.0
            } else {
                (2.0 - 2f64.powf(-20.0 * x + 10.0)) / 2.0
            }
        }
        "ease-in-circ" => 1.0 - (1.0 - x * x).sqrt(),
        "ease-out-circ" => (1.0 - (x - 1.0).powi(2)).sqrt(),
        "ease-in-out-circ" => {
            if x < 0.5 {
                (1.0 - (1.0 - (2.0 * x).powi(2)).sqrt()) / 2.0
            } else {
                ((1.0 - (-2.0 * x + 2.0).powi(2)).sqrt() + 1.0) / 2.0
            }
        }
        "ease-in-back" => {
            let (c1, c3) = (1.70158, 2.70158);
            c3 * x * x * x - c1 * x * x
        }
        "ease-out-back" => {
            let (c1, c3) = (1.70158, 2.70158);
            1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
        }
        "ease-in-out-back" => {
            let c2 = 1.70158 * 1.525;
            if x < 0.5 {
                ((2.0 * x).powi(2) * ((c2 + 1.0) * 2.0 * x - c2)) / 2.0
            } else {
                ((2.0 * x - 2.0).powi(2) * ((c2 + 1.0) * (2.0 * x - 2.0) + c2) + 2.0) / 2.0
            }
        }
        "ease-in-elastic" => {
            if x == 0.0 || x == 1.0 {
                return x;
            }
            let c4 = 2.0 * PI / 3.0;
            -(2f64.powf(10.0 * x - 10.0)) * ((x * 10.0 - 10.75) * c4).sin()
        }
        "ease-out-elastic" => {
            if x == 0.0 || x == 1.0 {
                return x;
            }
            let c4 = 2.0 * PI / 3.0;
            2f64.powf(-10.0 * x) * ((x * 10.0 - 0.75) * c4).sin() + 1.0
        }
        "ease-in-out-elastic" => {
            if x == 0.0 || x == 1.0 {
                return x;
            }
            let c5 = 2.0 * PI / 4.5;
            if x < 0.5 {
                -(2f64.powf(20.0 * x - 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0
            } else {
                (2f64.powf(-20.0 * x + 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0 + 1.0
            }
        }
        "ease-in-bounce" => 1.0 - bounce_out(1.0 - x),
        "ease-out-bounce" => bounce_out(x),
        "ease-in-out-bounce" => {
            if x < 0.5 {
                (1.0 - bounce_out(1.0 - 2.0 * x)) / 2.0
            } else {
                (1.0 + bounce_out(2.0 * x - 1.0)) / 2.0
            }
        }
        _ => x,
    }
}

fn bounce_out(x: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if x < 1.0 / d1 {
        n1 * x * x
    } else if x < 2.0 / d1 {
        n1 * (x - 1.5 / d1) * (x - 1.5 / d1) + 0.75
    } else if x < 2.5 / d1 {
        n1 * (x - 2.25 / d1) * (x - 2.25 / d1) + 0.9375
    } else {
        n1 * (x - 2.625 / d1) * (x - 2.625 / d1) + 0.984375
    }
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
