// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{Extra, push_extra};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Timeline settings, keyframe tracks and scene cues. Times are milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub duration: f64,
    pub fps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_step: Option<f64>,
    /// Playhead when saved (editor state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_playing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<bool>,
    /// Property edits sync keyframes at the playhead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_keyframe: Option<bool>,
    /// Playhead returns to 0 after a non-looping play reaches the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_rewind: Option<bool>,
    #[serde(default)]
    pub item_tracks: Vec<ItemTrack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_cues: Option<Vec<SceneCue>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Timeline {
    pub(crate) fn collect_unknown(&self, path: &str, out: &mut Vec<String>) {
        push_extra(out, path, &self.extra);
        for (i, t) in self.item_tracks.iter().enumerate() {
            let p = format!("{path}.itemTracks[{i}]");
            push_extra(out, &p, &t.extra);
            for (prop, keys) in &t.properties {
                for (k, key) in keys.iter().enumerate() {
                    push_extra(out, &format!("{p}.properties.{prop}[{k}]"), &key.extra);
                }
            }
        }
        for (i, c) in self.scene_cues.iter().flatten().enumerate() {
            push_extra(out, &format!("{path}.sceneCues[{i}]"), &c.extra);
        }
    }
}

/// Keyframes of one item, per animated property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemTrack {
    pub item_id: String,
    /// Property name (`x`, `opacity`, `modBlurRadius`, …) → keyframes.
    pub properties: BTreeMap<String, Vec<Keyframe>>,
    /// Lanes switched on or off in the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_properties: Option<BTreeMap<String, bool>>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One keyframe. `easing` shapes the move arriving at this key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    /// Milliseconds.
    pub time: f64,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A marker on the scene timeline: pause point, API call or script call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneCue {
    pub id: String,
    /// Milliseconds.
    pub time: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<f64>,
    #[serde(rename = "type")]
    pub kind: CueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// `GET` or `POST`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_body: Option<String>,
    /// Name of an exported function in the template script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_fn: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    Pause,
    ApiCall,
    Script,
}
