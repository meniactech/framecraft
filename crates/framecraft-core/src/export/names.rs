// SPDX-License-Identifier: AGPL-3.0-or-later
//! File names and identifiers inside export packages.

use crate::js;

/// A file name base: path characters removed, spaces collapsed, at most
/// 120 characters (UTF-16 units, as in FRAME).
pub fn file_base_name(name: &str, fallback: &str) -> String {
    let cleaned: String = js::trim(name)
        .chars()
        .filter(|c| !r#"\/:*?"<>|"#.contains(*c))
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let base = if cleaned.is_empty() {
        fallback
    } else {
        &cleaned
    };
    js::slice_utf16(base, 120).to_string()
}

/// `My Graphic.html`
pub fn html_entry_name(graphic_name: &str) -> String {
    format!("{}.html", file_base_name(graphic_name, "template"))
}

/// Template id: the file base name in lower case with `_` for spaces.
pub fn template_slug(name: &str, fallback: &str) -> String {
    let base = file_base_name(name, fallback).to_lowercase();
    base.split_whitespace().collect::<Vec<_>>().join("_")
}

/// Lower case `[a-z0-9]` runs joined by `-`.
fn kebab(s: &str) -> String {
    let lower = js::trim(s).to_lowercase();
    let mut out = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Custom element safe id: starts with a letter and contains a hyphen.
/// SPX registers `{id}-v{version}` as the element name.
pub fn ograf_element_id(name: &str, fallback: &str) -> String {
    let mut s = kebab(name);
    if s.is_empty() {
        s = fallback.to_string();
    }
    if !s.starts_with(|c: char| c.is_ascii_lowercase()) {
        s = format!("g-{s}");
    }
    if !s.contains('-') {
        s.push_str("-graphic");
    }
    js::slice_utf16(&s, 80).to_string()
}

/// A version usable inside an element name (`0.9.0` → `0-9-0`).
pub fn ograf_element_version(version: &str, fallback: &str) -> String {
    let s = kebab(version);
    js::slice_utf16(if s.is_empty() { fallback } else { &s }, 40).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_names() {
        assert_eq!(
            file_base_name("  Pies / and: charts?  ", "x"),
            "Pies and charts"
        );
        assert_eq!(file_base_name(" ** ", "template"), "template");
        assert_eq!(html_entry_name("TEAMS A"), "TEAMS A.html");
        assert_eq!(template_slug("Name  Tag", "t"), "name_tag");
    }

    #[test]
    fn element_ids() {
        assert_eq!(ograf_element_id("pies_and_charts", "f"), "pies-and-charts");
        assert_eq!(ograf_element_id("teams", "f"), "teams-graphic");
        assert_eq!(ograf_element_id("2026 final", "f"), "g-2026-final");
        assert_eq!(
            ograf_element_id("___", "ionframe-template"),
            "ionframe-template"
        );
        assert_eq!(ograf_element_version("0.9.0", "0-9-0"), "0-9-0");
    }
}
