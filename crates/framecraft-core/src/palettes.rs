// SPDX-License-Identifier: AGPL-3.0-or-later
//! Color palettes and stock gradients, as FRAME's: the user's palettes
//! (one is active; its colors show under every color field and color
//! charts), and the built-in stock palettes and gradients
//! (`palettes/stock-palettes.json`, `gradients/stock-gradients.json`).

use crate::model::RampColor;
use serde::{Deserialize, Serialize};

/// FRAME's limits.
pub const MAX_PALETTES: usize = 10;
pub const MAX_COLORS: usize = 14;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaletteColor {
    /// `#RRGGBB`.
    pub hex: String,
    /// 0–100.
    #[serde(default = "full")]
    pub alpha: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

fn full() -> f64 {
    100.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub colors: Vec<PaletteColor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockGradient {
    pub id: String,
    pub name: String,
    pub stops: Vec<RampColor>,
}

#[derive(Deserialize)]
struct PaletteFile {
    palettes: Vec<Palette>,
}

#[derive(Deserialize)]
struct GradientFile {
    gradients: Vec<StockGradient>,
}

/// The stock palettes from FRAME's `stock-palettes.json`.
pub fn parse_stock_palettes(json: &str) -> Vec<Palette> {
    serde_json::from_str::<PaletteFile>(json)
        .map(|f| f.palettes)
        .unwrap_or_default()
}

/// The stock gradients from FRAME's `stock-gradients.json`, with their
/// stops normalized (sorted, clamped) as FRAME applies them.
pub fn parse_stock_gradients(json: &str) -> Vec<StockGradient> {
    serde_json::from_str::<GradientFile>(json)
        .map(|f| f.gradients)
        .unwrap_or_default()
        .into_iter()
        .map(|g| StockGradient {
            stops: crate::ramp::normalize_color_ramp(Some(&g.stops)),
            ..g
        })
        .collect()
}

/// `#RRGGBB` (any case, `#RGB` too) → `(r, g, b)`; white when not a hex.
fn rgb(hex: &str) -> (u8, u8, u8) {
    let raw = hex.trim().trim_start_matches('#');
    let h = |s: &str| u8::from_str_radix(s, 16).ok();
    let parsed = match raw.len() {
        3 => {
            let d: Vec<String> = raw.chars().map(|c| format!("{c}{c}")).collect();
            h(&d[0])
                .zip(h(&d[1]))
                .zip(h(&d[2]))
                .map(|((r, g), b)| (r, g, b))
        }
        6 => raw
            .get(0..2)
            .and_then(h)
            .zip(raw.get(2..4).and_then(h))
            .zip(raw.get(4..6).and_then(h))
            .map(|((r, g), b)| (r, g, b)),
        _ => None,
    };
    parsed.unwrap_or((255, 255, 255))
}

/// A palette color as CSS (FRAME's `paletteSwatchToCss`): `#rrggbb` when
/// opaque, else `rgba(…)`.
pub fn swatch_css(hex: &str, alpha: f64) -> String {
    let (r, g, b) = rgb(hex);
    let a = (alpha / 100.0).clamp(0.0, 1.0);
    if a >= 0.999 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        let a = (a * 1000.0).round() / 1000.0;
        format!("rgba({r}, {g}, {b}, {})", crate::js::number_to_string(a))
    }
}

/// The user's palettes (saved in FrameCraft's data folder).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserPalettes {
    pub palettes: Vec<Palette>,
    /// The active palette's id; the first palette when unset or gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

impl UserPalettes {
    pub fn active(&self) -> Option<&Palette> {
        self.active
            .as_ref()
            .and_then(|id| self.palettes.iter().find(|p| &p.id == id))
            .or_else(|| self.palettes.first())
    }

    /// The active palette's colors as CSS (chart colors in exports).
    pub fn active_css(&self) -> Vec<String> {
        self.active().map_or_else(Vec::new, |p| {
            p.colors
                .iter()
                .map(|c| swatch_css(&c.hex, c.alpha))
                .collect()
        })
    }

    /// Adds a copy of `stock` (at most [`MAX_COLORS`] colors) under a
    /// unique name ("Name", "Name (2)", …) and makes it active. `None` when
    /// there are [`MAX_PALETTES`] already.
    pub fn add_copy(&mut self, stock: &Palette) -> Option<String> {
        if self.palettes.len() >= MAX_PALETTES {
            return None;
        }
        let base = if stock.name.trim().is_empty() {
            "Palette"
        } else {
            stock.name.trim()
        };
        let taken = |n: &str| {
            self.palettes
                .iter()
                .any(|p| p.name.trim().eq_ignore_ascii_case(n))
        };
        let mut name = base.to_string();
        let mut k = 2;
        while taken(&name) {
            name = format!("{base} ({k})");
            k += 1;
        }
        let id = crate::layers::new_id();
        self.palettes.push(Palette {
            id: id.clone(),
            name,
            colors: stock
                .colors
                .iter()
                .take(MAX_COLORS)
                .map(|c| PaletteColor {
                    hex: swatch_css(&c.hex, 100.0).to_uppercase(),
                    alpha: c.alpha.clamp(0.0, 100.0).round(),
                    label: c.label.clone().filter(|l| !l.trim().is_empty()),
                })
                .collect(),
        });
        self.active = Some(id.clone());
        Some(id)
    }

    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("serializes"))?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatches_as_frame() {
        assert_eq!(swatch_css("#0EA5E9", 100.0), "#0ea5e9");
        assert_eq!(swatch_css("abc", 50.0), "rgba(170, 187, 204, 0.5)");
        assert_eq!(swatch_css("nope", 100.0), "#ffffff");
    }

    #[test]
    fn stock_files_parse() {
        let p = parse_stock_palettes(
            r##"{"version":1,"palettes":[{"id":"a","name":"A","colors":[{"hex":"#112233","label":"X"}]}]}"##,
        );
        assert_eq!(p[0].colors[0].alpha, 100.0);
        let g = parse_stock_gradients(
            r#"{"version":1,"gradients":[{"id":"g","name":"G","stops":[{"offset":1,"color":"red"},{"offset":0,"color":"blue"}]}]}"#,
        );
        assert_eq!(g[0].stops[0].color, "blue");
        // FRAME's real files, when the checkout is there.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/static");
        if let Ok(t) = std::fs::read_to_string(dir.join("palettes/stock-palettes.json")) {
            assert!(parse_stock_palettes(&t).len() >= 20);
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("gradients/stock-gradients.json")) {
            assert!(parse_stock_gradients(&t).len() >= 40);
        }
    }

    #[test]
    fn user_palettes_copy_with_unique_names_and_limits() {
        let stock = Palette {
            id: "s".into(),
            name: "News".into(),
            colors: (0..20)
                .map(|i| PaletteColor {
                    hex: format!("#0000{i:02x}"),
                    alpha: if i == 0 { 50.0 } else { 100.0 },
                    label: None,
                })
                .collect(),
        };
        let mut u = UserPalettes::default();
        assert!(u.active_css().is_empty());
        let first = u.add_copy(&stock).unwrap();
        u.add_copy(&stock).unwrap();
        assert_eq!(u.palettes[1].name, "News (2)");
        assert_eq!(u.palettes[0].colors.len(), MAX_COLORS);
        assert_eq!(u.active().unwrap().name, "News (2)");
        u.active = Some(first);
        assert_eq!(u.active_css()[0], "rgba(0, 0, 0, 0.5)");
        for _ in 2..MAX_PALETTES {
            u.add_copy(&stock).unwrap();
        }
        assert!(u.add_copy(&stock).is_none());
        // Saved and loaded back.
        let path =
            std::env::temp_dir().join(format!("fc-palettes-{}.json", crate::layers::new_id()));
        u.save(&path).unwrap();
        assert_eq!(UserPalettes::load(&path), u);
        let _ = std::fs::remove_file(path);
    }
}
