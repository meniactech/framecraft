// SPDX-License-Identifier: AGPL-3.0-or-later
//! Product-specific values written into documents and exports.
//!
//! FRAME baked its name, version and BOARD addresses into every export.
//! FrameCraft keeps them in one place: [`Profile::framecraft`] for normal
//! use, [`Profile::frame`] to reproduce FRAME's exports exactly (tests).

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// `version` in template exports.
    pub version: String,
    /// Default SPX description for a new or older scene.
    pub default_scene_description: String,
    /// SPX description when the scene has none: `{id}` is the template id.
    pub fallback_spx_description: String,
    /// Default QR code content.
    pub default_qr_content: String,
    /// BOARD live-data API base.
    pub board_api_base: String,
    /// BOARD panel page used when no project is set.
    pub board_panel_gate_url: String,
    /// BOARD play page base; the project id is appended.
    pub board_play_base: String,
    /// Title of the BOARD panel field in SPX.
    pub board_panel_title: String,
}

impl Profile {
    /// FrameCraft's own values.
    pub fn framecraft() -> Self {
        Self {
            version: crate::VERSION.to_string(),
            default_scene_description: "FrameCraft template".into(),
            fallback_spx_description: "FrameCraft draft export ({id})".into(),
            ..Self::frame()
        }
    }

    /// FRAME 0.9.0's values, for comparing against FRAME's exports.
    pub fn frame() -> Self {
        Self {
            version: "0.9.0".into(),
            default_scene_description: "MENiAC FRAME template".into(),
            fallback_spx_description: "MENiAC FRAME draft export ({id})".into(),
            default_qr_content: "https://meniac.tech".into(),
            board_api_base: "https://board.meniac.tech".into(),
            board_panel_gate_url: "https://board.meniac.tech/spx".into(),
            board_play_base: "https://board.meniac.tech/play/".into(),
            board_panel_title: "MENiAC BOARD".into(),
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::framecraft()
    }
}
