// SPDX-License-Identifier: AGPL-3.0-or-later
use super::Extra;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The scene: canvas size and background, SPX playout settings, BOARD
/// connection and editor-only guides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// Always `"scene"`.
    pub id: String,
    pub name: String,
    pub background_color: String,
    pub width: f64,
    pub height: f64,

    // SPX template definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_playserver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_playchannel: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_playlayer: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_webplayout: Option<f64>,
    /// A number of milliseconds, or `"manual"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_out: Option<SpxOut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_dataformat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_uicolor: Option<f64>,
    /// When true, play while already on air does not stop. Default true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx_prevent_toggle: Option<bool>,

    // BOARD connection (technical name: IonBoard).
    /// Export adds BOARD Magic ID and Project ID fields and polls live data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ionboard_hook_enabled: Option<bool>,
    /// With the hook on, SPX export also adds a BOARD panel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ionboard_panel_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ionboard_magic_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ionboard_project_id: Option<String>,

    // Editor guides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_line_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_color_safe10: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_color_safe20: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_color_thirds: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_color_center: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_rulers_h: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_rulers_v: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_ruler_color: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_payload: Option<BTreeMap<String, String>>,

    // Editor-only preview background; never exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_enabled: Option<bool>,
    /// `builtin` or `media`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_builtin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_media_url: Option<String>,
    /// 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_opacity: Option<f64>,
    /// CSS blur radius in px, 0–20.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fake_background_blur: Option<f64>,

    #[serde(flatten)]
    pub extra: Extra,
}

/// SPX `out` setting: take the graphic out after a time, or manually.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpxOut {
    Millis(f64),
    /// Normally `"manual"`.
    Text(String),
}
