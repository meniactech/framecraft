// SPDX-License-Identifier: AGPL-3.0-or-later
//! Timeline editing, as in FRAME's timeline: the lanes a layer can animate,
//! snapping, and commands that work on many keyframes or on scene cues.

use crate::commands::{
    Change, Command, CommandError, item_mut, item_name, set_number, track_mut, with_json,
};
use crate::model::{CueKind, Item, Keyframe, Modifier, SceneCue};
use crate::normalize::{CLIP_START, Document};
use crate::timeline::{lane_enabled, lane_value};
use std::collections::BTreeMap;

/// Every lane, in FRAME's order (plus width and height, which the runtime
/// also animates).
pub const LANES: [&str; 46] = [
    "x",
    "y",
    "width",
    "height",
    "scaleX",
    "scaleY",
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
];

/// FRAME's lane names.
pub fn lane_label(lane: &str) -> &str {
    match lane {
        "clipStart" => "Start",
        "x" => "X",
        "y" => "Y",
        "width" => "Width",
        "height" => "Height",
        "scaleX" => "Scale X",
        "scaleY" => "Scale Y",
        "rotation" => "Rotation",
        "opacity" => "Opacity",
        "borderWidth" => "Border Width",
        "chartReveal" => "Chart Reveal",
        "particleEmitting" => "Particle Emit",
        "particleSpawnRate" => "Particle Rate",
        "particleEmitterX" => "Emitter X",
        "particleEmitterY" => "Emitter Y",
        "lightningEmitting" => "Lightning Emit",
        "lightningAx" => "Bolt A X",
        "lightningAy" => "Bolt A Y",
        "lightningBx" => "Bolt B X",
        "lightningBy" => "Bolt B Y",
        "patternPhase" => "Pattern Phase",
        "fontSize" => "Font Size",
        "textColorR" => "Text R",
        "textColorG" => "Text G",
        "textColorB" => "Text B",
        "textColorA" => "Text A",
        "bgColorR" => "Fill R",
        "bgColorG" => "Fill G",
        "bgColorB" => "Fill B",
        "bgColorA" => "Fill A",
        "modBlurRadius" => "Mod Blur Radius",
        "modColorBrightness" => "Mod Color Brightness",
        "modColorContrast" => "Mod Color Contrast",
        "modColorSaturation" => "Mod Color Saturation",
        "modColorHueRotate" => "Mod Color Hue",
        "modShadowX" => "Mod Shadow X",
        "modShadowY" => "Mod Shadow Y",
        "modShadowBlur" => "Mod Shadow Blur",
        "modCropPercent" => "Mod Crop %",
        "modTypewriterPercent" => "Mod Typewriter %",
        "modPathProgress" => "Mod Path Progress",
        "modTextStaggerProgress" => "Mod Text Stagger %",
        "modTextScrambleProgress" => "Mod Text Scramble %",
        "modTextGlyphColorProgress" => "Mod Glyph Color %",
        "modSkewX" => "Mod Skew X",
        "modSkewY" => "Mod Skew Y",
        "textProgress" => "Text Progress",
        other => other,
    }
}

/// The layer's own value of a lane (what a new keyframe starts from), as
/// FRAME's `propertyValueAtItem`. `None` when the layer has nothing to
/// animate there (a modifier lane without that modifier).
pub fn lane_base(item: &Item, lane: &str) -> Option<f64> {
    let first =
        |pick: &dyn Fn(&Modifier) -> Option<f64>| item.modifiers.iter().flatten().find_map(pick);
    let color = |c: Option<&str>, default: &str| {
        crate::color::parse(c.unwrap_or(default)).unwrap_or(crate::color::Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        })
    };
    let text = || color(item.text_color.as_deref(), "#ffffff");
    let fill = || color(Some(item.bg_color.as_str()), "#00000000");
    Some(match lane {
        "x" => item.x,
        "y" => item.y,
        "width" => item.width,
        "height" => item.height,
        "scaleX" => item.scale_x.unwrap_or(1.0),
        "scaleY" => item.scale_y.unwrap_or(1.0),
        "rotation" => item.rotation,
        "opacity" => item.opacity,
        "borderWidth" => item.border_width.max(0.0),
        "chartReveal" => item.chart_reveal.unwrap_or(1.0),
        "particleEmitting" => f64::from(item.particle_emitting != Some(false)),
        "particleSpawnRate" => item.particle_spawn_rate.unwrap_or(40.0),
        "particleEmitterX" => item.particle_emitter_x.unwrap_or(item.width / 2.0),
        "particleEmitterY" => item.particle_emitter_y.unwrap_or(item.height / 2.0),
        "lightningEmitting" => f64::from(item.lightning_emitting != Some(false)),
        "lightningAx" => item.lightning_ax.unwrap_or(24.0),
        "lightningAy" => item.lightning_ay.unwrap_or(item.height / 2.0),
        "lightningBx" => item.lightning_bx.unwrap_or((item.width - 24.0).max(32.0)),
        "lightningBy" => item.lightning_by.unwrap_or(item.height / 2.0),
        "patternPhase" => item.pattern_phase.unwrap_or(0.0),
        "fontSize" => item.font_size.unwrap_or(42.0),
        "textColorR" => text().r,
        "textColorG" => text().g,
        "textColorB" => text().b,
        "textColorA" => text().a,
        "bgColorR" => fill().r,
        "bgColorG" => fill().g,
        "bgColorB" => fill().b,
        "bgColorA" => fill().a,
        "modBlurRadius" => first(&|m| match m {
            Modifier::Blur(b) => Some(b.radius),
            _ => None,
        })?,
        "modColorBrightness" | "modColorContrast" | "modColorSaturation" | "modColorHueRotate" => {
            first(&|m| match m {
                Modifier::Color(c) => Some(match lane {
                    "modColorBrightness" => c.brightness,
                    "modColorContrast" => c.contrast,
                    "modColorSaturation" => c.saturation,
                    _ => c.hue_rotate,
                }),
                _ => None,
            })?
        }
        "modShadowX" | "modShadowY" | "modShadowBlur" => first(&|m| match m {
            Modifier::Shadow(s) => Some(match lane {
                "modShadowX" => s.x,
                "modShadowY" => s.y,
                _ => s.blur,
            }),
            _ => None,
        })?,
        "modCropPercent" => first(&|m| match m {
            Modifier::Crop(c) => Some(c.percent),
            _ => None,
        })?,
        "modTypewriterPercent" => first(&|m| match m {
            Modifier::Typewriter(t) => Some(t.percent),
            _ => None,
        })?,
        "modPathProgress" => first(&|m| match m {
            Modifier::PathFollow(p) => Some(p.progress),
            _ => None,
        })?,
        "modTextStaggerProgress" => first(&|m| match m {
            Modifier::TextStagger(t) => Some(t.progress),
            _ => None,
        })?,
        "modTextScrambleProgress" => first(&|m| match m {
            Modifier::TextScramble(t) => Some(t.progress),
            _ => None,
        })?,
        "modTextGlyphColorProgress" => first(&|m| match m {
            Modifier::TextGlyphColor(t) => Some(t.progress),
            _ => None,
        })?,
        "modSkewX" | "modSkewY" => first(&|m| match m {
            Modifier::Skew(s) => Some(if lane == "modSkewX" {
                s.skew_x
            } else {
                s.skew_y
            }),
            _ => None,
        })?,
        _ => return None,
    })
}

/// A lane's value at `time_ms`: the animation's, or the layer's own.
pub fn value_now(doc: &Document, item: &Item, lane: &str, time_ms: f64) -> Option<f64> {
    lane_value(doc, &item.id, lane, time_ms).or_else(|| lane_base(item, lane))
}

/// Lanes the Keys view shows for a layer: the switched-on ones, in
/// FRAME's order. (The Lottie / video start lane comes with those types.)
pub fn shown_lanes(doc: &Document, item_id: &str) -> Vec<&'static str> {
    let Some(track) = doc
        .timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == item_id)
    else {
        return Vec::new();
    };
    LANES
        .iter()
        .copied()
        .filter(|lane| {
            lane_enabled(track, lane) && track.properties.get(*lane).is_some_and(|k| !k.is_empty())
        })
        .collect()
}

/// Keys this close (ms) are the same key (FRAME's `timesMatch`).
pub const SAME_TIME_MS: f64 = 0.51;

pub fn same_time(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME_TIME_MS
}

/// A time on the timeline: whole milliseconds within the duration.
pub fn normalize_time(doc: &Document, t: f64) -> f64 {
    t.clamp(0.0, doc.timeline.duration.max(0.0)).round()
}

/// A time snapped to the timeline's snap step, when snapping is on.
pub fn snap_time(doc: &Document, t: f64) -> f64 {
    let tl = &doc.timeline;
    if tl.snap_enabled != Some(true) {
        return normalize_time(doc, t);
    }
    let step = tl.snap_step.unwrap_or(1.0).max(1.0);
    normalize_time(doc, (t / step).round() * step)
}

/// One keyframe, by layer, lane and time.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyRef {
    pub item: String,
    pub lane: String,
    pub time: f64,
}

impl KeyRef {
    pub fn new(item: &str, lane: &str, time: f64) -> Self {
        Self {
            item: item.into(),
            lane: lane.into(),
            time,
        }
    }
}

/// The keyframe a reference points at.
pub fn key_at<'a>(doc: &'a Document, r: &KeyRef) -> Option<&'a Keyframe> {
    doc.timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == r.item)?
        .properties
        .get(&r.lane)?
        .iter()
        .find(|k| same_time(k.time, r.time))
}

/// Keys grouped by layer and lane, in a stable order.
fn grouped(keys: &[KeyRef]) -> BTreeMap<(String, String), Vec<f64>> {
    let mut out: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    for k in keys.iter().filter(|k| k.lane != CLIP_START) {
        out.entry((k.item.clone(), k.lane.clone()))
            .or_default()
            .push(k.time);
    }
    out
}

fn check_items(doc: &Document, keys: &[KeyRef]) -> Result<(), CommandError> {
    for k in keys {
        if !doc.items.iter().any(|i| i.id == k.item) {
            return Err(CommandError::NoSuchItem(k.item.clone()));
        }
    }
    Ok(())
}

fn keys_label(doc: &Document, verb: &str, keys: &[KeyRef]) -> String {
    match keys {
        [one] => format!(
            "{verb} {} keyframe of {}",
            lane_label(&one.lane),
            item_name(doc, &one.item)
        ),
        many => format!("{verb} {} keyframes", many.len()),
    }
}

/// Moves keyframes in time together (a timeline drag or arrow keys). The
/// move is clamped so every key stays within the timeline; with `snap`,
/// each new time snaps to the snap step. A moved key replaces a key
/// already at its new time.
pub struct RetimeKeys {
    pub keys: Vec<KeyRef>,
    pub delta: f64,
    pub snap: bool,
}

impl RetimeKeys {
    fn clamped(&self, doc: &Document) -> f64 {
        let times = self.keys.iter().map(|k| k.time);
        let (min, max) = times.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
            (lo.min(t), hi.max(t))
        });
        if !min.is_finite() {
            return 0.0;
        }
        self.delta.max(-min).min(doc.timeline.duration - max)
    }

    fn new_time(&self, doc: &Document, t: f64, delta: f64) -> f64 {
        if self.snap {
            snap_time(doc, t + delta)
        } else {
            normalize_time(doc, t + delta)
        }
    }

    /// The moved keys (the new selection). Call on the document before
    /// applying.
    pub fn moved(&self, doc: &Document) -> Vec<KeyRef> {
        let delta = self.clamped(doc);
        self.keys
            .iter()
            .map(|k| KeyRef {
                time: self.new_time(doc, k.time, delta),
                ..k.clone()
            })
            .collect()
    }
}

impl Command for RetimeKeys {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        check_items(doc, &self.keys)?;
        let delta = self.clamped(doc);
        let new_times: Vec<(String, String, f64, f64)> = self
            .keys
            .iter()
            .filter(|k| k.lane != CLIP_START)
            .map(|k| {
                (
                    k.item.clone(),
                    k.lane.clone(),
                    k.time,
                    self.new_time(doc, k.time, delta),
                )
            })
            .collect();
        for ((item, lane), _) in grouped(&self.keys) {
            let track = track_mut(doc, &item);
            let Some(keys) = track.properties.get_mut(&lane) else {
                continue;
            };
            let (mut moving, keep): (Vec<Keyframe>, Vec<Keyframe>) =
                keys.drain(..).partition(|k| {
                    new_times
                        .iter()
                        .any(|(i, l, from, _)| *i == item && *l == lane && same_time(*from, k.time))
                });
            for k in &mut moving {
                if let Some((.., to)) = new_times
                    .iter()
                    .find(|(i, l, from, _)| *i == item && *l == lane && same_time(*from, k.time))
                {
                    k.time = *to;
                }
            }
            // Moved keys win over keys already at their new time.
            let mut merged: Vec<Keyframe> = keep
                .into_iter()
                .filter(|k| !moving.iter().any(|m| same_time(m.time, k.time)))
                .collect();
            merged.extend(moving);
            merged.sort_by(|a, b| a.time.total_cmp(&b.time));
            *keys = merged;
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        keys_label(doc, "Move", &self.keys)
    }
}

/// Deletes keyframes. A lane that loses its last key is switched off and
/// the layer keeps that key's value, so nothing jumps.
pub struct RemoveKeys {
    pub keys: Vec<KeyRef>,
}

impl Command for RemoveKeys {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        check_items(doc, &self.keys)?;
        for ((item, lane), times) in grouped(&self.keys) {
            let track = track_mut(doc, &item);
            let Some(keys) = track.properties.get_mut(&lane) else {
                continue;
            };
            let last = keys.last().map(|k| k.value);
            keys.retain(|k| !times.iter().any(|t| same_time(*t, k.time)));
            if keys.is_empty() {
                track.properties.remove(&lane);
                if let Some(e) = track.enabled_properties.as_mut() {
                    e.remove(&lane);
                }
                if let Some(v) = last {
                    // Only plain number fields; colors and modifiers keep theirs.
                    let _ = set_number(item_mut(doc, &item)?, &lane, v);
                }
            }
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        keys_label(doc, "Delete", &self.keys)
    }
}

/// Sets the easing of keyframes (of the move arriving at each).
pub struct SetKeysEasing {
    pub keys: Vec<KeyRef>,
    pub easing: String,
}

impl Command for SetKeysEasing {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        check_items(doc, &self.keys)?;
        let easing = crate::easing::normalize(Some(&self.easing)).to_string();
        for ((item, lane), times) in grouped(&self.keys) {
            let track = track_mut(doc, &item);
            for k in track.properties.get_mut(&lane).into_iter().flatten() {
                if times.iter().any(|t| same_time(*t, k.time)) {
                    k.easing = Some(easing.clone());
                }
            }
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        keys_label(doc, "Easing of", &self.keys)
    }
}

/// A copied keyframe: its lane, time after the first copied key, value
/// and easing.
#[derive(Debug, Clone, PartialEq)]
pub struct CopiedKey {
    pub lane: String,
    pub offset: f64,
    pub value: f64,
    pub easing: Option<String>,
}

/// Copies keyframes, with times relative to the earliest one.
pub fn copy_keys(doc: &Document, keys: &[KeyRef]) -> Vec<CopiedKey> {
    let start = keys.iter().map(|k| k.time).fold(f64::INFINITY, f64::min);
    keys.iter()
        .filter(|k| k.lane != CLIP_START)
        .filter_map(|k| {
            let key = key_at(doc, k)?;
            Some(CopiedKey {
                lane: k.lane.clone(),
                offset: k.time - start,
                value: key.value,
                easing: key.easing.clone(),
            })
        })
        .collect()
}

/// Pastes copied keyframes onto a layer, starting at `at` (snapped).
pub struct PasteKeys {
    pub item: String,
    pub at: f64,
    pub keys: Vec<CopiedKey>,
}

impl PasteKeys {
    /// The pasted keys (the new selection).
    pub fn pasted(&self, doc: &Document) -> Vec<KeyRef> {
        let start = snap_time(doc, self.at);
        self.keys
            .iter()
            .map(|c| KeyRef::new(&self.item, &c.lane, snap_time(doc, start + c.offset)))
            .collect()
    }
}

impl Command for PasteKeys {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?;
        let targets = self.pasted(doc);
        let track = track_mut(doc, &self.item);
        for (c, r) in self.keys.iter().zip(targets) {
            let keys = track.properties.entry(c.lane.clone()).or_default();
            keys.retain(|k| !same_time(k.time, r.time));
            keys.push(Keyframe {
                time: r.time,
                value: c.value,
                easing: Some(c.easing.clone().unwrap_or_else(|| "linear".into())),
                extra: Default::default(),
            });
            keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            track
                .enabled_properties
                .get_or_insert_default()
                .insert(c.lane.clone(), true);
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Paste keyframes on {}", item_name(doc, &self.item))
    }
}

/// Cue rows of the timeline, in FRAME's order.
pub const CUE_KINDS: [CueKind; 3] = [CueKind::Pause, CueKind::ApiCall, CueKind::Script];

pub fn cue_label(kind: CueKind) -> &'static str {
    match kind {
        CueKind::Pause => "Pause",
        CueKind::ApiCall => "API",
        CueKind::Script => "Script",
    }
}

/// A new cue with FRAME's defaults.
pub fn new_cue(kind: CueKind, time: f64) -> SceneCue {
    let mut cue = serde_json::json!({
        "id": crate::layers::new_id(), "time": time, "delayMs": 0,
        "type": kind, "label": cue_label(kind)
    });
    match kind {
        CueKind::ApiCall => {
            cue["apiUrl"] = "".into();
            cue["apiMethod"] = "POST".into();
            cue["apiBody"] = "".into();
        }
        CueKind::Script => cue["scriptFn"] = "".into(),
        CueKind::Pause => {}
    }
    serde_json::from_value(cue).expect("cue defaults match the model")
}

fn sort_cues(doc: &mut Document) {
    if let Some(cues) = doc.timeline.scene_cues.as_mut() {
        cues.sort_by(|a, b| a.time.total_cmp(&b.time));
    }
}

/// Adds a cue (its time snapped).
pub struct AddCue {
    pub cue: SceneCue,
}

impl Command for AddCue {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let mut cue = self.cue.clone();
        cue.time = snap_time(doc, cue.time);
        doc.timeline.scene_cues.get_or_insert_default().push(cue);
        sort_cues(doc);
        Ok(Change::Rebuild)
    }
    fn label(&self, _doc: &Document) -> String {
        format!("Add {} cue", cue_label(self.cue.kind))
    }
}

pub struct RemoveCue {
    pub id: String,
}

impl Command for RemoveCue {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let cues = doc.timeline.scene_cues.get_or_insert_default();
        let before = cues.len();
        cues.retain(|c| c.id != self.id);
        if cues.len() == before {
            return Err(CommandError::NoSuchItem(self.id.clone()));
        }
        Ok(Change::Rebuild)
    }
    fn label(&self, _doc: &Document) -> String {
        "Delete cue".into()
    }
}

/// Sets one field of a cue by its JSON name (`time`, `delayMs`, `label`,
/// `apiUrl`, `apiMethod`, `apiBody`, `scriptFn`). Times snap; the delay is
/// never negative.
pub struct SetCueField {
    pub id: String,
    pub path: String,
    pub value: serde_json::Value,
}

impl Command for SetCueField {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let i = doc
            .timeline
            .scene_cues
            .iter()
            .flatten()
            .position(|c| c.id == self.id)
            .ok_or_else(|| CommandError::NoSuchItem(self.id.clone()))?;
        let mut cue = doc.timeline.scene_cues.as_ref().expect("found")[i].clone();
        with_json(&mut cue, &self.path, self.value.clone())?;
        if self.path == "time" {
            cue.time = snap_time(doc, cue.time);
        }
        if let Some(d) = cue.delay_ms.as_mut() {
            *d = d.max(0.0);
        }
        doc.timeline.scene_cues.as_mut().expect("found")[i] = cue;
        sort_cues(doc);
        Ok(Change::Rebuild)
    }
    fn label(&self, _doc: &Document) -> String {
        if self.path == "time" {
            "Move cue".into()
        } else {
            "Edit cue".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::History;
    use crate::layers::{NewLayer, new_layer};
    use crate::model::Graphic;
    use crate::normalize::normalize;
    use crate::profile::Profile;
    use serde_json::json;

    fn doc() -> Document {
        let g: Graphic = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 100, "height": 100 },
            "items": [],
            "timeline": { "duration": 2000, "fps": 25, "itemTracks": [
                { "itemId": "a", "properties": {
                    "x": [ { "time": 0, "value": 0 }, { "time": 500, "value": 50 }, { "time": 1000, "value": 100 } ],
                    "opacity": [ { "time": 1000, "value": 1, "easing": "easeInQuad" } ]
                } }
            ] }
        }))
        .unwrap();
        let mut d = normalize(&g, &Profile::framecraft());
        let mut a = new_layer(NewLayer::Text, None);
        a.id = "a".into();
        d.items.push(a);
        d
    }

    fn times(d: &Document, lane: &str) -> Vec<f64> {
        d.timeline.item_tracks[0]
            .properties
            .get(lane)
            .map_or(Vec::new(), |k| k.iter().map(|k| k.time).collect())
    }

    #[test]
    fn retime_moves_together_clamps_and_replaces() {
        let mut d = doc();
        let mut h = History::default();
        // 1000 can only go 1000 further: the move is clamped for both.
        let cmd = RetimeKeys {
            keys: vec![KeyRef::new("a", "x", 500.0), KeyRef::new("a", "x", 1000.0)],
            delta: 5000.0,
            snap: false,
        };
        assert_eq!(
            cmd.moved(&d).iter().map(|k| k.time).collect::<Vec<_>>(),
            [1500.0, 2000.0]
        );
        h.execute(&mut d, &cmd).unwrap();
        assert_eq!(times(&d, "x"), [0.0, 1500.0, 2000.0]);
        // Onto an existing key: the moved one wins.
        h.execute(
            &mut d,
            &RetimeKeys {
                keys: vec![KeyRef::new("a", "x", 1500.0)],
                delta: -1500.0,
                snap: false,
            },
        )
        .unwrap();
        assert_eq!(times(&d, "x"), [0.0, 2000.0]);
        assert_eq!(d.timeline.item_tracks[0].properties["x"][0].value, 50.0);
    }

    #[test]
    fn retime_snaps_to_the_step() {
        let mut d = doc();
        d.timeline.snap_enabled = Some(true);
        d.timeline.snap_step = Some(100.0);
        let cmd = RetimeKeys {
            keys: vec![KeyRef::new("a", "x", 500.0)],
            delta: 37.0,
            snap: true,
        };
        History::default().execute(&mut d, &cmd).unwrap();
        assert_eq!(times(&d, "x"), [0.0, 500.0, 1000.0]);
        let cmd = RetimeKeys {
            keys: vec![KeyRef::new("a", "x", 500.0)],
            delta: 61.0,
            snap: true,
        };
        History::default().execute(&mut d, &cmd).unwrap();
        assert_eq!(times(&d, "x"), [0.0, 600.0, 1000.0]);
    }

    #[test]
    fn removing_last_key_keeps_the_value() {
        let mut d = doc();
        let mut h = History::default();
        let keys = vec![
            KeyRef::new("a", "x", 0.0),
            KeyRef::new("a", "x", 500.0),
            KeyRef::new("a", "x", 1000.0),
        ];
        h.execute(&mut d, &RemoveKeys { keys }).unwrap();
        assert!(times(&d, "x").is_empty());
        assert_eq!(d.items[0].x, 100.0);
        assert_eq!(shown_lanes(&d, "a"), ["opacity"]);
    }

    #[test]
    fn easing_copy_and_paste() {
        let mut d = doc();
        let mut h = History::default();
        let sel = vec![
            KeyRef::new("a", "x", 500.0),
            KeyRef::new("a", "opacity", 1000.0),
        ];
        h.execute(
            &mut d,
            &SetKeysEasing {
                keys: sel.clone(),
                easing: "ease-out-back".into(),
            },
        )
        .unwrap();
        assert_eq!(
            key_at(&d, &sel[0]).unwrap().easing.as_deref(),
            Some("ease-out-back")
        );
        let copied = copy_keys(&d, &sel);
        assert_eq!(copied[1].offset, 500.0);
        let paste = PasteKeys {
            item: "a".into(),
            at: 1200.0,
            keys: copied,
        };
        assert_eq!(paste.pasted(&d)[1].time, 1700.0);
        h.execute(&mut d, &paste).unwrap();
        assert_eq!(times(&d, "x"), [0.0, 500.0, 1000.0, 1200.0]);
        assert_eq!(times(&d, "opacity"), [1000.0, 1700.0]);
        assert_eq!(
            key_at(&d, &KeyRef::new("a", "opacity", 1700.0))
                .unwrap()
                .easing
                .as_deref(),
            Some("ease-out-back")
        );
    }

    #[test]
    fn base_values_follow_frame() {
        let d = doc();
        let a = &d.items[0];
        assert_eq!(lane_base(a, "fontSize"), Some(42.0));
        // FRAME's default text color for new text is #f8fafc.
        assert_eq!(lane_base(a, "textColorR"), Some(248.0));
        assert_eq!(lane_base(a, "bgColorA"), Some(0.0));
        assert_eq!(lane_base(a, "modBlurRadius"), None);
        assert_eq!(value_now(&d, a, "x", 250.0), Some(25.0));
        assert_eq!(shown_lanes(&d, "a"), ["x", "opacity"]);
    }

    #[test]
    fn cues_add_edit_remove() {
        let mut d = doc();
        let mut h = History::default();
        let cue = new_cue(CueKind::ApiCall, 700.4);
        let id = cue.id.clone();
        h.execute(&mut d, &AddCue { cue }).unwrap();
        h.execute(
            &mut d,
            &AddCue {
                cue: new_cue(CueKind::Pause, 300.0),
            },
        )
        .unwrap();
        let cues = d.timeline.scene_cues.as_ref().unwrap();
        assert_eq!(
            cues.iter().map(|c| c.time).collect::<Vec<_>>(),
            [300.0, 700.0]
        );
        assert_eq!(cues[1].api_method.as_deref(), Some("POST"));
        h.execute(
            &mut d,
            &SetCueField {
                id: id.clone(),
                path: "time".into(),
                value: json!(100),
            },
        )
        .unwrap();
        h.execute(
            &mut d,
            &SetCueField {
                id: id.clone(),
                path: "delayMs".into(),
                value: json!(-5),
            },
        )
        .unwrap();
        let cues = d.timeline.scene_cues.as_ref().unwrap();
        assert_eq!(
            (cues[0].id.as_str(), cues[0].delay_ms),
            (id.as_str(), Some(0.0))
        );
        h.execute(&mut d, &RemoveCue { id }).unwrap();
        assert_eq!(d.timeline.scene_cues.as_ref().unwrap().len(), 1);
    }
}
