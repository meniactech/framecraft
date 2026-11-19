// SPDX-License-Identifier: AGPL-3.0-or-later
//! CSS colors as FRAME stores them (`#rgb`, `#rrggbb`, `#rrggbbaa`,
//! `rgb(…)`, `rgba(…)`), for color fields.

/// Red, green, blue (0–255) and alpha (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// Parses a color; `None` for anything else (names like `red` included).
pub fn parse(text: &str) -> Option<Rgba> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok().map(f64::from);
        let short = |i: usize| digit(i, 1).map(|v| v * 17.0);
        return match hex.len() {
            3 => Some(Rgba {
                r: short(0)?,
                g: short(1)?,
                b: short(2)?,
                a: 1.0,
            }),
            4 => Some(Rgba {
                r: short(0)?,
                g: short(1)?,
                b: short(2)?,
                a: short(3)? / 255.0,
            }),
            6 => Some(Rgba {
                r: digit(0, 2)?,
                g: digit(2, 2)?,
                b: digit(4, 2)?,
                a: 1.0,
            }),
            8 => Some(Rgba {
                r: digit(0, 2)?,
                g: digit(2, 2)?,
                b: digit(4, 2)?,
                a: digit(6, 2)? / 255.0,
            }),
            _ => None,
        };
    }
    let lower = t.to_ascii_lowercase();
    let inner = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<f64> = inner
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    match parts.as_slice() {
        [r, g, b] => Some(Rgba {
            r: *r,
            g: *g,
            b: *b,
            a: 1.0,
        }),
        [r, g, b, a] => Some(Rgba {
            r: *r,
            g: *g,
            b: *b,
            a: *a,
        }),
        _ => None,
    }
}

impl Rgba {
    /// `#rrggbb`, for a color picker (alpha dropped).
    pub fn hex(&self) -> String {
        let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }

    /// `#rrggbb` when opaque, otherwise `rgba(r, g, b, a)` (FRAME's forms).
    pub fn css(&self) -> String {
        if self.a >= 0.999 {
            return self.hex();
        }
        let c = |v: f64| v.round().clamp(0.0, 255.0);
        let a = (self.a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
        format!(
            "rgba({}, {}, {}, {})",
            c(self.r),
            c(self.g),
            c(self.b),
            crate::js::number_to_string(a)
        )
    }
}

/// A new color from a picker's `#rrggbb`, keeping the alpha of `current`.
pub fn with_rgb(current: &str, hex: &str) -> Option<String> {
    let rgb = parse(hex)?;
    let a = parse(current).map_or(1.0, |c| c.a);
    Some(Rgba { a, ..rgb }.css())
}

/// The same color with a new alpha (0–1).
pub fn with_alpha(current: &str, a: f64) -> Option<String> {
    let c = parse(current)?;
    Some(
        Rgba {
            a: a.clamp(0.0, 1.0),
            ..c
        }
        .css(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_colors() {
        assert_eq!(
            parse("#64748b"),
            Some(Rgba {
                r: 100.0,
                g: 116.0,
                b: 139.0,
                a: 1.0
            })
        );
        assert_eq!(parse("#fff").unwrap().hex(), "#ffffff");
        assert_eq!(parse("#00000000").unwrap().a, 0.0);
        assert_eq!(
            parse("rgba(99, 102, 241, 0.5)"),
            Some(Rgba {
                r: 99.0,
                g: 102.0,
                b: 241.0,
                a: 0.5
            })
        );
        assert_eq!(parse("rgb(1,2,3)").unwrap().a, 1.0);
        assert_eq!(parse("red"), None);
        assert_eq!(parse("#12"), None);
    }

    #[test]
    fn picker_edits_keep_alpha() {
        assert_eq!(
            with_rgb("rgba(0, 0, 0, 0.25)", "#ff8000").as_deref(),
            Some("rgba(255, 128, 0, 0.25)")
        );
        assert_eq!(with_rgb("#000000", "#ff8000").as_deref(), Some("#ff8000"));
        assert_eq!(
            with_alpha("#ff8000", 0.5).as_deref(),
            Some("rgba(255, 128, 0, 0.5)")
        );
        assert_eq!(
            with_alpha("rgba(255, 128, 0, 0.5)", 1.0).as_deref(),
            Some("#ff8000")
        );
    }
}
