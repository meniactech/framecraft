// SPDX-License-Identifier: AGPL-3.0-or-later
//! The motion library, as FRAME's `motionLibrary.ts` and `houseMotions.ts`:
//! reusable keyframe snippets stored RELATIVE to the target's pose (offset,
//! factor, …), so one "slide in" works on any layer. Applying a motion
//! writes ordinary keyframes (a copy, no link back), so the preview and all
//! exports are unchanged. FRAME's JSON format is kept, so motions move
//! between the two.

use crate::commands::{Change, Command, CommandError, item_name, track_mut};
use crate::model::{Keyframe, Modifier};
use crate::normalize::Document;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const VERSION: u32 = 1;

/// `(id, label)` in FRAME's order.
pub const CATEGORIES: &[(&str, &str)] = &[
    ("in", "In"),
    ("out", "Out"),
    ("emphasis", "Emphasis"),
    ("other", "Other"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionKey {
    /// Milliseconds from the motion's start.
    pub t: f64,
    pub v: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionChannel {
    pub property: String,
    /// `offset` (base + v), `offsetWidth` (base + v × width), `offsetHeight`,
    /// `factor` (base × v) or `absolute` (v).
    pub mode: String,
    pub keys: Vec<MotionKey>,
    /// Modifier settings used when the target lacks the modifier and gets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier_init: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionData {
    pub version: u32,
    pub category: String,
    /// Milliseconds: the last key's time (at least 1).
    pub duration: f64,
    pub channels: Vec<MotionChannel>,
}

/// A motion in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionEntry {
    /// `house:<slug>` or `user:<id>`.
    pub key: String,
    pub name: String,
    pub data: MotionData,
}

impl MotionEntry {
    pub fn is_house(&self) -> bool {
        self.key.starts_with("house:")
    }

    /// The lanes it animates, once each (FRAME's channel summary).
    pub fn properties(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for c in &self.data.channels {
            if !out.contains(&c.property.as_str()) {
                out.push(&c.property);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Property rules

const OFFSET_PROPS: &[&str] = &[
    "x",
    "y",
    "rotation",
    "modSkewX",
    "modSkewY",
    "modShadowX",
    "modShadowY",
    "modColorHueRotate",
    "particleEmitterX",
    "particleEmitterY",
    "lightningAx",
    "lightningAy",
    "lightningBx",
    "lightningBy",
    "patternPhase",
];

const FACTOR_PROPS: &[&str] = &[
    "width",
    "height",
    "scaleX",
    "scaleY",
    "opacity",
    "fontSize",
    "borderWidth",
];

/// Lanes only some layer types have.
fn lane_item_types(lane: &str) -> Option<&'static [&'static str]> {
    Some(match lane {
        "chartReveal" => &["chart"],
        "particleEmitting" | "particleSpawnRate" | "particleEmitterX" | "particleEmitterY" => {
            &["particles"]
        }
        "lightningEmitting" | "lightningAx" | "lightningAy" | "lightningBx" | "lightningBy" => {
            &["lightning"]
        }
        "patternPhase" => &["pattern"],
        "fontSize"
        | "textColorR"
        | "textColorG"
        | "textColorB"
        | "textColorA"
        | "modTypewriterPercent"
        | "modTextStaggerProgress"
        | "modTextScrambleProgress"
        | "modTextGlyphColorProgress" => &["text"],
        _ => return None,
    })
}

/// Modifier lanes: the modifier type, and whether a neutral one may be
/// added when the layer lacks it.
fn modifier_lane(lane: &str) -> Option<(&'static str, bool)> {
    Some(match lane {
        "modBlurRadius" => ("blur", true),
        "modColorBrightness" | "modColorContrast" | "modColorSaturation" | "modColorHueRotate" => {
            ("color", true)
        }
        "modShadowX" | "modShadowY" | "modShadowBlur" => ("shadow", true),
        "modCropPercent" => ("crop", true),
        "modSkewX" | "modSkewY" => ("skew", true),
        "modTypewriterPercent" => ("typewriter", true),
        "modPathProgress" => ("pathFollow", false),
        "modTextStaggerProgress" => ("textStagger", false),
        "modTextScrambleProgress" => ("textScramble", false),
        "modTextGlyphColorProgress" => ("textGlyphColor", false),
        _ => return None,
    })
}

/// Lanes a motion never carries.
const EXCLUDED: &[&str] = &[crate::normalize::CLIP_START];

pub fn default_mode(lane: &str) -> &'static str {
    if OFFSET_PROPS.contains(&lane) {
        "offset"
    } else if FACTOR_PROPS.contains(&lane) {
        "factor"
    } else {
        "absolute"
    }
}

/// A motion read from JSON (a saved or imported one), checked as FRAME's
/// `parseMotionPayload`: bad keys and channels are dropped; `None` when
/// nothing is left.
pub fn parse(raw: &Value) -> Option<MotionData> {
    let raw = match raw {
        Value::String(s) => &serde_json::from_str::<Value>(s).ok()?,
        v => v,
    };
    let modes = [
        "offset",
        "offsetWidth",
        "offsetHeight",
        "factor",
        "absolute",
    ];
    let mut channels = Vec::new();
    for ch in raw.get("channels")?.as_array()? {
        let Some(property) = ch.get("property").and_then(Value::as_str) else {
            continue;
        };
        if EXCLUDED.contains(&property) {
            continue;
        }
        let mode = ch
            .get("mode")
            .and_then(Value::as_str)
            .filter(|m| modes.contains(m))
            .unwrap_or_else(|| default_mode(property));
        let mut keys: Vec<MotionKey> = ch
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|k| {
                let t = k.get("t")?.as_f64()?;
                let v = k.get("v")?.as_f64()?;
                (t >= 0.0 && t.is_finite() && v.is_finite()).then(|| MotionKey {
                    t: t.round(),
                    v,
                    easing: k.get("easing").and_then(Value::as_str).map(String::from),
                })
            })
            .collect();
        if keys.is_empty() {
            continue;
        }
        keys.sort_by(|a, b| a.t.total_cmp(&b.t));
        channels.push(MotionChannel {
            property: property.into(),
            mode: mode.into(),
            keys,
            modifier_init: ch.get("modifierInit").and_then(Value::as_object).cloned(),
        });
    }
    if channels.is_empty() {
        return None;
    }
    let category = raw
        .get("category")
        .and_then(Value::as_str)
        .filter(|c| CATEGORIES.iter().any(|(id, _)| id == c))
        .unwrap_or("other");
    Some(MotionData {
        version: VERSION,
        category: category.into(),
        duration: last_key(&channels),
        channels,
    })
}

fn last_key(channels: &[MotionChannel]) -> f64 {
    channels
        .iter()
        .flat_map(|c| c.keys.iter().map(|k| k.t))
        .fold(1.0, f64::max)
}

// ---------------------------------------------------------------------------
// House motions

fn key(t: f64, v: f64, easing: Option<&str>) -> MotionKey {
    MotionKey {
        t,
        v,
        easing: easing.map(String::from),
    }
}

fn ch(property: &str, mode: &str, keys: Vec<MotionKey>) -> MotionChannel {
    MotionChannel {
        property: property.into(),
        mode: mode.into(),
        keys,
        modifier_init: None,
    }
}

fn house(slug: &str, name: &str, category: &str, channels: Vec<MotionChannel>) -> MotionEntry {
    MotionEntry {
        key: format!("house:{slug}"),
        name: name.into(),
        data: MotionData {
            version: VERSION,
            category: category.into(),
            duration: last_key(&channels),
            channels,
        },
    }
}

/// FRAME's built-in motions, in FRAME's order.
pub fn house_motions() -> Vec<MotionEntry> {
    let fade_in = |ms: f64| {
        ch(
            "opacity",
            "factor",
            vec![key(0.0, 0.0, None), key(ms, 1.0, Some("ease-out-quad"))],
        )
    };
    let fade_out = |from: f64, to: f64| {
        ch(
            "opacity",
            "factor",
            vec![key(from, 1.0, None), key(to, 0.0, Some("ease-in-quad"))],
        )
    };
    let axis_mode = |axis: &str| {
        if axis == "x" {
            "offsetWidth"
        } else {
            "offsetHeight"
        }
    };
    let slide_in = |axis: &str, dir: f64| {
        ch(
            axis,
            axis_mode(axis),
            vec![
                key(0.0, 0.5 * dir, None),
                key(600.0, 0.0, Some("ease-out-cubic")),
            ],
        )
    };
    let slide_out = |axis: &str, dir: f64| {
        ch(
            axis,
            axis_mode(axis),
            vec![
                key(0.0, 0.0, None),
                key(500.0, 0.5 * dir, Some("ease-in-cubic")),
            ],
        )
    };
    let scale_both = |keys: Vec<MotionKey>| {
        vec![
            ch("scaleX", "factor", keys.clone()),
            ch("scaleY", "factor", keys),
        ]
    };
    let with = |mut v: Vec<MotionChannel>, extra: MotionChannel| {
        v.push(extra);
        v
    };
    let crop = |dir: &str, from: f64, to: f64| {
        let mut c = ch(
            "modCropPercent",
            "absolute",
            vec![
                key(0.0, from, None),
                key(600.0, to, Some("ease-in-out-cubic")),
            ],
        );
        c.modifier_init = Some(
            json!({ "direction": dir })
                .as_object()
                .cloned()
                .expect("object"),
        );
        c
    };
    let wobble = |property: &str, keys: &[(f64, f64, Option<&str>)]| {
        ch(
            property,
            "offset",
            keys.iter().map(|(t, v, e)| key(*t, *v, *e)).collect(),
        )
    };
    vec![
        // In
        house("fade-in", "Fade in", "in", vec![fade_in(400.0)]),
        house(
            "slide-in-left",
            "Slide in from left",
            "in",
            vec![slide_in("x", -1.0), fade_in(300.0)],
        ),
        house(
            "slide-in-right",
            "Slide in from right",
            "in",
            vec![slide_in("x", 1.0), fade_in(300.0)],
        ),
        house(
            "slide-in-up",
            "Slide in from below",
            "in",
            vec![slide_in("y", 1.0), fade_in(300.0)],
        ),
        house(
            "slide-in-down",
            "Slide in from above",
            "in",
            vec![slide_in("y", -1.0), fade_in(300.0)],
        ),
        house(
            "pop-in",
            "Pop in",
            "in",
            with(
                scale_both(vec![
                    key(0.0, 0.0, None),
                    key(500.0, 1.0, Some("ease-out-back")),
                ]),
                fade_in(200.0),
            ),
        ),
        house(
            "drop-in",
            "Drop in",
            "in",
            vec![
                ch(
                    "y",
                    "offsetHeight",
                    vec![
                        key(0.0, -0.8, None),
                        key(800.0, 0.0, Some("ease-out-bounce")),
                    ],
                ),
                fade_in(150.0),
            ],
        ),
        house(
            "blur-in",
            "Blur in",
            "in",
            vec![
                ch(
                    "modBlurRadius",
                    "absolute",
                    vec![key(0.0, 20.0, None), key(600.0, 0.0, Some("ease-out-quad"))],
                ),
                fade_in(400.0),
            ],
        ),
        house(
            "wipe-in",
            "Wipe in (left to right)",
            "in",
            vec![crop("right", 100.0, 0.0)],
        ),
        house(
            "type-on",
            "Type on (text)",
            "in",
            vec![ch(
                "modTypewriterPercent",
                "absolute",
                vec![key(0.0, 0.0, None), key(1000.0, 100.0, None)],
            )],
        ),
        // Out
        house("fade-out", "Fade out", "out", vec![fade_out(0.0, 400.0)]),
        house(
            "slide-out-left",
            "Slide out to left",
            "out",
            vec![slide_out("x", -1.0), fade_out(200.0, 500.0)],
        ),
        house(
            "slide-out-right",
            "Slide out to right",
            "out",
            vec![slide_out("x", 1.0), fade_out(200.0, 500.0)],
        ),
        house(
            "slide-out-up",
            "Slide out upwards",
            "out",
            vec![slide_out("y", -1.0), fade_out(200.0, 500.0)],
        ),
        house(
            "slide-out-down",
            "Slide out downwards",
            "out",
            vec![slide_out("y", 1.0), fade_out(200.0, 500.0)],
        ),
        house(
            "pop-out",
            "Pop out",
            "out",
            with(
                scale_both(vec![
                    key(0.0, 1.0, None),
                    key(400.0, 0.0, Some("ease-in-back")),
                ]),
                fade_out(250.0, 400.0),
            ),
        ),
        house(
            "blur-out",
            "Blur out",
            "out",
            vec![
                ch(
                    "modBlurRadius",
                    "absolute",
                    vec![key(0.0, 0.0, None), key(600.0, 20.0, Some("ease-in-quad"))],
                ),
                fade_out(200.0, 600.0),
            ],
        ),
        house(
            "wipe-out",
            "Wipe out (left to right)",
            "out",
            vec![crop("left", 0.0, 100.0)],
        ),
        // Emphasis
        house(
            "pulse",
            "Pulse",
            "emphasis",
            scale_both(vec![
                key(0.0, 1.0, None),
                key(250.0, 1.1, Some("ease-out-quad")),
                key(500.0, 1.0, Some("ease-in-out-quad")),
            ]),
        ),
        house(
            "shake",
            "Shake",
            "emphasis",
            vec![wobble(
                "x",
                &[
                    (0.0, 0.0, None),
                    (60.0, -12.0, Some("ease-out-sine")),
                    (140.0, 12.0, Some("ease-in-out-sine")),
                    (220.0, -8.0, Some("ease-in-out-sine")),
                    (300.0, 8.0, Some("ease-in-out-sine")),
                    (380.0, -4.0, Some("ease-in-out-sine")),
                    (440.0, 0.0, Some("ease-in-sine")),
                ],
            )],
        ),
        house(
            "wiggle",
            "Wiggle",
            "emphasis",
            vec![wobble(
                "rotation",
                &[
                    (0.0, 0.0, None),
                    (100.0, -6.0, Some("ease-out-sine")),
                    (250.0, 6.0, Some("ease-in-out-sine")),
                    (400.0, -3.0, Some("ease-in-out-sine")),
                    (500.0, 0.0, Some("ease-in-sine")),
                ],
            )],
        ),
        house(
            "spin",
            "Spin 360°",
            "emphasis",
            vec![wobble(
                "rotation",
                &[(0.0, 0.0, None), (800.0, 360.0, Some("ease-in-out-cubic"))],
            )],
        ),
        house(
            "flash",
            "Flash",
            "emphasis",
            vec![ch(
                "opacity",
                "factor",
                vec![
                    key(0.0, 1.0, None),
                    key(150.0, 0.2, None),
                    key(300.0, 1.0, None),
                    key(450.0, 0.2, None),
                    key(600.0, 1.0, None),
                ],
            )],
        ),
    ]
}

// ---------------------------------------------------------------------------
// Save: selected keyframes → motion

/// The selected keys of ONE layer as a relative motion (FRAME's
/// `buildMotionFromKeyframes`). The rest pose per lane is the last selected
/// key for `in` motions (the layer lands there), the first otherwise.
pub fn from_keyframes(
    doc: &Document,
    item: &str,
    selection: &[(String, f64)],
    category: &str,
) -> Result<MotionData, String> {
    let track = doc
        .timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == item)
        .ok_or("This layer has no keyframes.")?;
    let mut lanes: Vec<(String, Vec<Keyframe>)> = Vec::new();
    for (lane, time) in selection {
        if EXCLUDED.contains(&lane.as_str()) {
            continue;
        }
        let Some(k) = track
            .properties
            .get(lane)
            .and_then(|ks| ks.iter().find(|k| (k.time - time).abs() <= 0.51))
        else {
            continue;
        };
        match lanes.iter_mut().find(|(l, _)| l == lane) {
            Some((_, list)) if !list.iter().any(|x| x.time == k.time) => list.push(k.clone()),
            Some(_) => {}
            None => lanes.push((lane.clone(), vec![k.clone()])),
        }
    }
    if lanes.is_empty() {
        return Err("Select at least one keyframe (Start keys cannot be saved).".into());
    }
    let start = lanes
        .iter()
        .flat_map(|(_, ks)| ks.iter().map(|k| k.time))
        .fold(f64::INFINITY, f64::min);
    let layer = doc.items.iter().find(|i| i.id == item);
    let mut channels = Vec::new();
    for (lane, mut keys) in lanes {
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        let anchor = if category == "in" {
            &keys[keys.len() - 1]
        } else {
            &keys[0]
        };
        let base = anchor.value;
        let mut mode = default_mode(&lane);
        if mode == "factor" && base.abs() < 1e-9 {
            mode = "absolute";
        }
        let r6 = |v: f64| (v * 1e6).round() / 1e6;
        let motion_keys = keys
            .iter()
            .map(|k| MotionKey {
                t: (k.time - start).round(),
                v: r6(match mode {
                    "offset" => k.value - base,
                    "factor" => k.value / base,
                    _ => k.value,
                }),
                easing: k.easing.clone().filter(|e| e != "linear"),
            })
            .collect();
        // Keep the modifier's static settings (crop direction, shadow color…).
        let modifier_init = modifier_lane(&lane).and_then(|(kind, _)| {
            let m = layer?
                .modifiers
                .iter()
                .flatten()
                .find(|m| crate::modifiers::kind(m) == kind)?;
            let mut o = serde_json::to_value(m).ok()?.as_object()?.clone();
            for k in ["id", "type", "enabled"] {
                o.remove(k);
            }
            Some(o)
        });
        channels.push(MotionChannel {
            property: lane,
            mode: mode.into(),
            keys: motion_keys,
            modifier_init,
        });
    }
    Ok(MotionData {
        version: VERSION,
        category: category.into(),
        duration: last_key(&channels),
        channels,
    })
}

// ---------------------------------------------------------------------------
// Apply and evaluate

fn resolve(mode: &str, base: f64, v: f64, width: f64, height: f64) -> f64 {
    match mode {
        "offset" => base + v,
        "offsetWidth" => base + v * width,
        "offsetHeight" => base + v * height,
        "factor" => base * v,
        _ => v,
    }
}

/// The motion's values at `time_ms` from its start, on a layer of
/// `width` × `height` whose resting values are `rest` (missing: 0, or 1 for
/// factor lanes). Same interpolation as the timeline (FRAME's
/// `evaluateMotionAt`, for previews).
pub fn evaluate_at(
    data: &MotionData,
    time_ms: f64,
    duration: f64,
    easing_override: Option<&str>,
    width: f64,
    height: f64,
    rest: &dyn Fn(&str) -> Option<f64>,
) -> Vec<(String, f64)> {
    let scale = duration.max(1.0) / data.duration.max(1.0);
    data.channels
        .iter()
        .map(|c| {
            let fallback = if FACTOR_PROPS.contains(&c.property.as_str()) {
                1.0
            } else {
                0.0
            };
            let base = rest(&c.property).unwrap_or(fallback);
            let keys: Vec<(f64, f64, &str)> = c
                .keys
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let easing = match (i > 0, easing_override) {
                        (true, Some(e)) => e,
                        _ => k.easing.as_deref().unwrap_or("linear"),
                    };
                    (
                        k.t * scale,
                        resolve(&c.mode, base, k.v, width, height),
                        easing,
                    )
                })
                .collect();
            let last = keys[keys.len() - 1];
            let value = if time_ms >= last.0 {
                last.1
            } else if time_ms <= keys[0].0 {
                keys[0].1
            } else {
                keys.windows(2)
                    .find(|w| time_ms >= w[0].0 && time_ms <= w[1].0)
                    .map_or(last.1, |w| {
                        let lin = (time_ms - w[0].0) / (w[1].0 - w[0].0).max(1.0);
                        let e = crate::easing::apply(crate::easing::normalize(Some(w[1].2)), lin);
                        w[0].1 + (w[1].1 - w[0].1) * e
                    })
            };
            (c.property.clone(), value)
        })
        .collect()
}

/// When the motion (with stagger over `targets` layers) would end.
pub fn end_time(data: &MotionData, start: f64, duration: f64, stagger: f64, targets: usize) -> f64 {
    let _ = data;
    start + (targets.saturating_sub(1) as f64) * stagger.max(0.0) + duration.max(1.0)
}

/// A neutral modifier (no visible change until keyed), with `init`'s
/// settings where they fit (FRAME's `createNeutralModifier`).
fn neutral_modifier(kind: &str, init: Option<&Map<String, Value>>) -> Option<Modifier> {
    let mut base = match kind {
        "blur" => json!({ "type": "blur", "enabled": true, "radius": 0 }),
        "color" => {
            json!({ "type": "color", "enabled": true, "brightness": 100, "contrast": 100, "saturation": 100, "hueRotate": 0 })
        }
        "shadow" => {
            json!({ "type": "shadow", "enabled": true, "x": 0, "y": 0, "blur": 0, "color": "rgba(0, 0, 0, 0.5)" })
        }
        "skew" => json!({ "type": "skew", "enabled": true, "skewX": 0, "skewY": 0 }),
        "crop" => json!({ "type": "crop", "enabled": true, "direction": "left", "percent": 0 }),
        "typewriter" => json!({ "type": "typewriter", "enabled": true, "percent": 100 }),
        _ => return None,
    };
    let obj = base.as_object_mut().expect("object");
    for (k, v) in init.into_iter().flatten() {
        let same_kind = obj.get(k).is_some_and(|cur| {
            std::mem::discriminant(cur) == std::mem::discriminant(v)
                || (cur.is_number() && v.is_number())
        });
        if k != "id" && k != "type" && same_kind {
            obj.insert(k.clone(), v.clone());
        }
    }
    obj.insert("id".into(), crate::layers::new_id().into());
    serde_json::from_value(base).ok()
}

/// Writes a motion's keys onto layers, starting at `start` (FRAME's
/// `applyMotion`). Each layer's resting values are its values at `start`.
/// Keys on the same lanes inside the motion's span are replaced; others
/// are kept. Lanes a layer cannot take are skipped (see
/// [`ApplyMotion::report`]).
pub struct ApplyMotion {
    pub items: Vec<String>,
    pub name: String,
    pub data: MotionData,
    pub start: f64,
    pub duration: f64,
    pub easing_override: Option<String>,
    pub stagger: f64,
}

/// What applying did: layers that got keys, and notes on skipped lanes.
#[derive(Debug, Default, PartialEq)]
pub struct ApplyReport {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub added_modifiers: bool,
}

impl ApplyMotion {
    fn run(&self, doc: &mut Document) -> ApplyReport {
        let mut report = ApplyReport::default();
        let scale = self.duration.max(1.0) / self.data.duration.max(1.0);
        for (index, id) in self.items.iter().enumerate() {
            let Some(pos) = doc.items.iter().position(|i| &i.id == id) else {
                continue;
            };
            let start = (self.start + index as f64 * self.stagger.max(0.0)).round();
            let end = start + (self.data.duration * scale).round();
            let mut wrote = false;
            for c in &self.data.channels {
                let item = &doc.items[pos];
                let name = if item.name.is_empty() {
                    "Layer"
                } else {
                    &item.name
                }
                .to_string();
                if let Some(types) = lane_item_types(&c.property) {
                    let kind = serde_json::to_value(item.item_type)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_else(|| "shape".into());
                    if !types.contains(&kind.as_str()) {
                        report.skipped.push(format!(
                            "{name}: {} only works on {} layers",
                            c.property,
                            types.join(" / ")
                        ));
                        continue;
                    }
                }
                let mut base = crate::keys::value_now(doc, item, &c.property, start);
                if base.is_none() {
                    match modifier_lane(&c.property) {
                        Some((kind, true)) => {
                            let Some(m) = neutral_modifier(kind, c.modifier_init.as_ref()) else {
                                continue;
                            };
                            doc.items[pos]
                                .modifiers
                                .get_or_insert_with(Vec::new)
                                .push(m);
                            report.added_modifiers = true;
                            base = crate::keys::value_now(doc, &doc.items[pos], &c.property, start);
                        }
                        other => {
                            report.skipped.push(format!(
                                "{name}: needs the {} modifier set up first",
                                other.map_or(c.property.as_str(), |(k, _)| k)
                            ));
                            continue;
                        }
                    }
                }
                let Some(base) = base else { continue };
                let (w, h) = (doc.items[pos].width, doc.items[pos].height);
                let added: Vec<Keyframe> = c
                    .keys
                    .iter()
                    .enumerate()
                    .map(|(i, k)| Keyframe {
                        time: (start + k.t * scale).round(),
                        value: (resolve(&c.mode, base, k.v, w, h) * 1e4).round() / 1e4,
                        easing: Some(match (i > 0, &self.easing_override) {
                            (true, Some(e)) => e.clone(),
                            _ => k.easing.clone().unwrap_or_else(|| "linear".into()),
                        }),
                        extra: Default::default(),
                    })
                    .collect();
                let track = track_mut(doc, id);
                let lane = track.properties.entry(c.property.clone()).or_default();
                lane.retain(|k| k.time < start - 0.5 || k.time > end + 0.5);
                for k in added {
                    // Same time after rounding: the last one wins.
                    lane.retain(|x| x.time != k.time);
                    lane.push(k);
                }
                lane.sort_by(|a, b| a.time.total_cmp(&b.time));
                track
                    .enabled_properties
                    .get_or_insert_with(Default::default)
                    .insert(c.property.clone(), true);
                wrote = true;
            }
            if wrote {
                report.applied.push(id.clone());
            }
        }
        report.skipped.dedup();
        report
    }

    /// What applying would do, without changing `doc`.
    pub fn report(&self, doc: &Document) -> ApplyReport {
        self.run(&mut doc.clone())
    }
}

impl Command for ApplyMotion {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let mut scratch = doc.clone();
        let report = self.run(&mut scratch);
        if report.applied.is_empty() {
            return Err(CommandError::InvalidValue(
                report
                    .skipped
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "This motion does not apply to the selected layer.".into()),
            ));
        }
        *doc = scratch;
        Ok(if report.added_modifiers {
            Change::Layer(report.applied)
        } else {
            Change::Pose
        })
    }
    fn label(&self, doc: &Document) -> String {
        match self.items.as_slice() {
            [one] => format!("Apply {} to {}", self.name, item_name(doc, one)),
            _ => format!("Apply {}", self.name),
        }
    }
}

/// The user's saved motions (FrameCraft's data folder), FRAME's format.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserMotions {
    pub motions: Vec<MotionEntry>,
}

impl UserMotions {
    pub fn load(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let raw: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let motions = raw
            .get("motions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(MotionEntry {
                    key: m.get("key")?.as_str()?.to_string(),
                    name: m.get("name")?.as_str()?.to_string(),
                    data: parse(m.get("data")?)?,
                })
            })
            .collect();
        Self { motions }
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("serializes"))?;
        std::fs::rename(tmp, path)
    }

    /// Adds a motion under `name`; returns its key (`user:<id>`).
    pub fn add(&mut self, name: &str, data: MotionData) -> String {
        let key = format!("user:{}", crate::layers::new_id());
        self.motions.push(MotionEntry {
            key: key.clone(),
            name: name.trim().to_string(),
            data,
        });
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::History;
    use crate::layers::{NewLayer, new_layer};

    fn doc() -> Document {
        let g = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut d = crate::normalize::normalize(&g, &crate::profile::Profile::framecraft());
        let mut b = new_layer(NewLayer::Box, None);
        b.id = "b".into();
        b.x = 500.0;
        b.width = 200.0;
        let mut t = new_layer(NewLayer::Text, None);
        t.id = "t".into();
        d.items.push(b);
        d.items.push(t);
        d
    }

    fn house_named(slug: &str) -> MotionEntry {
        house_motions()
            .into_iter()
            .find(|m| m.key == format!("house:{slug}"))
            .unwrap()
    }

    fn keys(d: &Document, item: &str, lane: &str) -> Vec<(f64, f64)> {
        d.timeline
            .item_tracks
            .iter()
            .find(|t| t.item_id == item)
            .and_then(|t| t.properties.get(lane))
            .map(|ks| ks.iter().map(|k| (k.time, k.value)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn house_motions_are_frames() {
        let all = house_motions();
        assert_eq!(all.len(), 23);
        assert_eq!(house_named("drop-in").data.duration, 800.0);
        // Every house motion survives FRAME's own validation.
        for m in &all {
            let back = parse(&serde_json::to_value(&m.data).unwrap()).unwrap();
            assert_eq!(back, m.data, "{}", m.key);
        }
    }

    #[test]
    fn slide_in_lands_where_the_layer_is() {
        let mut d = doc();
        let mut h = History::default();
        let m = house_named("slide-in-left");
        h.execute(
            &mut d,
            &ApplyMotion {
                items: vec!["b".into()],
                name: m.name.clone(),
                data: m.data.clone(),
                start: 1000.0,
                duration: 1200.0,
                easing_override: None,
                stagger: 0.0,
            },
        )
        .unwrap();
        // Half the width (200) to the left, landing at x = 500; stretched ×2.
        assert_eq!(keys(&d, "b", "x"), [(1000.0, 400.0), (2200.0, 500.0)]);
        assert_eq!(keys(&d, "b", "opacity"), [(1000.0, 0.0), (1600.0, 1.0)]);
        h.undo(&mut d).unwrap();
        assert!(keys(&d, "b", "x").is_empty());
    }

    #[test]
    fn modifiers_are_added_and_text_lanes_skipped() {
        let mut d = doc();
        let blur = house_named("blur-in");
        let apply = |items: &[&str], m: &MotionEntry| ApplyMotion {
            items: items.iter().map(|s| s.to_string()).collect(),
            name: m.name.clone(),
            data: m.data.clone(),
            start: 0.0,
            duration: m.data.duration,
            easing_override: Some("linear".into()),
            stagger: 100.0,
        };
        let mut h = History::default();
        h.execute(&mut d, &apply(&["b", "t"], &blur)).unwrap();
        assert!(d.items[0].modifiers.as_ref().unwrap().len() == 1);
        // Staggered: the second layer starts 100 ms later.
        assert_eq!(keys(&d, "t", "modBlurRadius")[0].0, 100.0);
        let wipe = house_named("wipe-in");
        h.execute(&mut d, &apply(&["b"], &wipe)).unwrap();
        let crop = d.items[0]
            .modifiers
            .as_ref()
            .unwrap()
            .iter()
            .find_map(|m| match m {
                Modifier::Crop(c) => Some(c.direction.clone()),
                _ => None,
            });
        assert_eq!(crop.as_deref(), Some("right"));
        // Type on needs a text layer.
        let report = apply(&["b"], &house_named("type-on")).report(&d);
        assert!(report.applied.is_empty() && report.skipped[0].contains("text"));
        assert!(
            h.execute(&mut d, &apply(&["b"], &house_named("type-on")))
                .is_err()
        );
    }

    #[test]
    fn save_and_reapply_round_trip() {
        let mut d = doc();
        let mut h = History::default();
        let m = house_named("pop-in");
        h.execute(
            &mut d,
            &ApplyMotion {
                items: vec!["b".into()],
                name: m.name.clone(),
                data: m.data.clone(),
                start: 0.0,
                duration: 500.0,
                easing_override: None,
                stagger: 0.0,
            },
        )
        .unwrap();
        let selection: Vec<(String, f64)> = ["scaleX", "scaleY", "opacity"]
            .iter()
            .flat_map(|l| {
                keys(&d, "b", l)
                    .into_iter()
                    .map(move |(t, _)| (l.to_string(), t))
            })
            .collect();
        let saved = from_keyframes(&d, "b", &selection, "in").unwrap();
        assert_eq!(saved.duration, 500.0);
        let sx = saved
            .channels
            .iter()
            .find(|c| c.property == "scaleX")
            .unwrap();
        assert_eq!(
            (sx.mode.as_str(), sx.keys[0].v, sx.keys[1].v),
            ("factor", 0.0, 1.0)
        );
        assert_eq!(sx.keys[1].easing.as_deref(), Some("ease-out-back"));
        // Previewed halfway on a 100 × 50 box: scaled between 0 and 1.
        let at = evaluate_at(&saved, 250.0, 500.0, None, 100.0, 50.0, &|_| None);
        let v = at.iter().find(|(p, _)| p == "scaleX").unwrap().1;
        assert!(v > 0.5 && v < 1.2, "{v}");
        // Saved and loaded.
        let mut u = UserMotions::default();
        let key = u.add(" Pop ", saved.clone());
        let path =
            std::env::temp_dir().join(format!("fc-motions-{}.json", crate::layers::new_id()));
        u.save(&path).unwrap();
        let back = UserMotions::load(&path);
        assert_eq!(back.motions[0].key, key);
        assert_eq!(back.motions[0].name, "Pop");
        assert_eq!(back.motions[0].data, saved);
        let _ = std::fs::remove_file(path);
        assert!(from_keyframes(&d, "b", &[], "in").is_err());
    }
}
