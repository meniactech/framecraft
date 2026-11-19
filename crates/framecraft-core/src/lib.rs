// SPDX-License-Identifier: AGPL-3.0-or-later
//! FrameCraft core.
//!
//! Everything that does not need a window lives here: the document model
//! (scenes, items, timelines), timeline evaluation and easing, motions,
//! modifiers, and the exporters that write SPX, OGraf and HTML banner
//! packages. The desktop app (`framecraft-app`) is a UI on top of this crate.
//!
//! Exports are checked against the reference set produced by FRAME, so the
//! files this crate writes stay compatible with SPX, OGraf hosts,
//! MENiAC Air and BOARD.

pub mod color;
pub mod commands;
pub mod easing;
pub mod export;
pub mod js;
pub mod keys;
pub mod layers;
pub mod manipulate;
pub mod media;
pub mod model;
pub mod modifiers;
pub mod motion;
pub mod normalize;
pub mod pack;
pub mod palettes;
pub mod polygon;
pub mod preview;
pub mod profile;
pub mod project;
pub mod ramp;
pub mod schema;
pub mod script;
pub mod selection;
pub mod timeline;

/// Product name used in generated files (`generator` fields and similar).
pub const GENERATOR: &str = "FrameCraft";

/// Crate version, written into export metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
        assert_eq!(GENERATOR, "FrameCraft");
    }
}
