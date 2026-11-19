// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authoring packs: `.frameproject.zip` (a whole project) and
//! `.framepack.zip` (one graphic), FRAME's format for moving work between
//! installations. They hold graphic data and the media it uses; they are
//! not playout packages.
//!
//! A pack read here is also a [`MediaSource`]: its graphics can be exported
//! straight from it.

use crate::export::assets::UserFont;
use crate::export::package::{MediaSource, Package};
use crate::js::{self, stringify_pretty};
use crate::model::{Graphic, Modifier};
use crate::normalize::Document;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;

pub const PROJECT_PACK_FORMAT: &str = "meniac-frame-project-pack";
pub const GRAPHIC_PACK_FORMAT: &str = "meniac-frame-graphic-pack";
pub const PACK_VERSION: u32 = 1;

/// A media file listed in a pack manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMediaEntry {
    /// The URL graphics use for this file.
    pub url: String,
    /// Path inside the pack.
    pub path: String,
    /// `images`, `fonts`, `backgrounds`, `videos` or `lottie`.
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackedMedia {
    pub entry: PackMediaEntry,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PackedGraphic {
    /// The graphic's id in the project it came from.
    pub id: String,
    pub name: String,
    pub graphic: Graphic,
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ProjectPack {
    pub name: String,
    pub exported_at: String,
    pub graphics: Vec<PackedGraphic>,
    pub media: Vec<PackedMedia>,
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct GraphicPack {
    pub graphic: PackedGraphic,
    pub exported_at: String,
    pub media: Vec<PackedMedia>,
}

#[derive(Debug, Clone)]
pub enum Pack {
    Project(ProjectPack),
    /// Boxed: it holds a whole graphic inline.
    Graphic(Box<GraphicPack>),
}

/// Why a pack could not be opened at all. Smaller problems (one missing
/// file) are returned as warnings instead.
#[derive(Debug, PartialEq, Eq)]
pub enum PackError {
    NotAZip(String),
    NoManifest,
    BadManifest(String),
    UnsupportedFormat(String),
    NoGraphics,
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NotAZip(e) => write!(f, "not a ZIP file: {e}"),
            Self::NoManifest => write!(f, "invalid pack: missing manifest.json"),
            Self::BadManifest(e) => write!(f, "invalid pack: {e}"),
            Self::UnsupportedFormat(fmt) => write!(f, "unsupported pack format: {fmt}"),
            Self::NoGraphics => write!(f, "the pack holds no graphic that could be read"),
        }
    }
}

impl std::error::Error for PackError {}

fn media_lookup<'a>(media: &'a [PackedMedia], url: &str) -> Option<&'a [u8]> {
    let wanted = normalize_pack_url(url);
    media
        .iter()
        .find(|m| normalize_pack_url(&m.entry.url) == wanted)
        .map(|m| m.bytes.as_slice())
}

impl MediaSource for ProjectPack {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        media_lookup(&self.media, url).map(<[u8]>::to_vec)
    }
}

impl MediaSource for GraphicPack {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        media_lookup(&self.media, url).map(<[u8]>::to_vec)
    }
}

/// `/path` form of a URL, without query. Absolute http(s) and data URLs
/// stay as they are.
pub fn normalize_pack_url(url: &str) -> String {
    let raw = js::trim(url);
    if raw.is_empty() {
        return String::new();
    }
    let base = raw.split('?').next().unwrap_or("");
    if base.starts_with("http://")
        || base.starts_with("https://")
        || base.starts_with("data:")
        || base.starts_with('/')
    {
        base.to_string()
    } else {
        format!("/{base}")
    }
}

/// Files every installation has; packs leave them out.
pub fn is_shared_install_url(url: &str) -> bool {
    ["/fonts/", "/particles/", "/vendor/", "/icons/", "/images/"]
        .iter()
        .any(|p| url.starts_with(p))
}

fn should_pack(url: &str) -> bool {
    !url.is_empty()
        && !url.starts_with("data:")
        && !is_shared_install_url(url)
        && (url.starts_with('/') || url.starts_with("http://") || url.starts_with("https://"))
}

pub fn category_from_url(url: &str) -> &'static str {
    if url.contains("/fonts/") {
        "fonts"
    } else if url.contains("/backgrounds/") {
        "backgrounds"
    } else if url.contains("/videos/") {
        "videos"
    } else if url.contains("/lottie/") {
        "lottie"
    } else {
        "images"
    }
}

fn basename_from_url(url: &str) -> String {
    let path = match url.find("://") {
        Some(i) => url[i + 3..].find('/').map_or("", |j| &url[i + 3 + j..]),
        None => url,
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.split('/')
        .rfind(|s| !s.is_empty())
        .unwrap_or("asset.bin")
        .to_string()
}

/// File name part for pack paths: word characters and `-` only, at most 80.
pub fn sanitize_pack_file_name(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for c in js::trim(name).chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    // Collapse runs of `-` that came from the name itself.
    let mut collapsed = String::new();
    for c in out.chars() {
        if !(c == '-' && collapsed.ends_with('-')) {
            collapsed.push(c);
        }
    }
    let trimmed = collapsed.strip_prefix('-').unwrap_or(&collapsed);
    let trimmed = trimmed.strip_suffix('-').unwrap_or(trimmed);
    let cut = js::slice_utf16(trimmed, 80);
    if cut.is_empty() {
        fallback.to_string()
    } else {
        cut.to_string()
    }
}

fn unique_media_path(category: &str, file: &str, used: &mut Vec<String>) -> String {
    let mut path = format!("media/{category}/{file}");
    if used.contains(&path) {
        let (stem, ext) = match file.rfind('.') {
            Some(d) if d > 0 => (&file[..d], &file[d..]),
            _ => (file, ""),
        };
        let mut i = 2;
        while used.contains(&format!("media/{category}/{stem}_{i}{ext}")) {
            i += 1;
        }
        path = format!("media/{category}/{stem}_{i}{ext}");
    }
    used.push(path.clone());
    path
}

/// Media URLs a graphic uses, in FRAME's order.
fn graphic_urls(g: &Graphic) -> Vec<String> {
    let mut urls = Vec::new();
    let mut add = |u: Option<&str>| {
        let n = normalize_pack_url(u.unwrap_or(""));
        if should_pack(&n) && !urls.contains(&n) {
            urls.push(n);
        }
    };
    add(g.scene.fake_background_media_url.as_deref());
    for item in &g.items {
        add(item.media_url.as_deref());
        add(item.bg_image_url.as_deref());
        add(item.particle_sprite_url.as_deref());
        for m in item.modifiers.iter().flatten() {
            if let Modifier::Mask(mask) = m {
                add(mask.image_url.as_deref());
            }
        }
    }
    urls
}

/// Lower-case font families a graphic uses (text items and text masks).
fn graphic_font_families(g: &Graphic) -> Vec<String> {
    let mut out = Vec::new();
    for item in &g.items {
        let mut add = |f: Option<&str>| {
            let f = js::trim(f.unwrap_or("")).to_lowercase();
            if !f.is_empty() && !out.contains(&f) {
                out.push(f);
            }
        };
        add(item.font_family.as_deref());
        for m in item.modifiers.iter().flatten() {
            if let Modifier::Mask(mask) = m {
                add(mask.text_font_family.as_deref());
            }
        }
    }
    out
}

/// Packs media and user fonts the graphics use. Files the source cannot
/// provide are skipped and listed in `missing` (FRAME skipped them silently).
fn pack_media(
    graphics: &[&Graphic],
    media: &dyn MediaSource,
    fonts: &[UserFont],
    pkg: &mut Package,
) -> Vec<PackMediaEntry> {
    let mut used = Vec::new();
    let mut entries = Vec::new();
    let mut urls: Vec<String> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    for g in graphics {
        for u in graphic_urls(g) {
            if !urls.contains(&u) {
                urls.push(u);
            }
        }
        for f in graphic_font_families(g) {
            if !families.contains(&f) {
                families.push(f);
            }
        }
    }
    for url in &urls {
        let Some(bytes) = media.read(url) else {
            pkg.missing.push(url.clone());
            continue;
        };
        let category = category_from_url(url);
        let name = basename_from_url(url);
        let path = unique_media_path(category, &name, &mut used);
        pkg.files.push((path.clone(), bytes));
        entries.push(PackMediaEntry {
            url: url.clone(),
            path,
            category: category.into(),
            original_name: Some(name),
            font_family: None,
        });
    }
    for font in fonts {
        let raw = if font.family.is_empty() {
            &font.original_name
        } else {
            &font.family
        };
        let family = match raw.rfind('.') {
            Some(d) if d + 1 < raw.len() => &raw[..d],
            _ => raw.as_str(),
        };
        let family = js::trim(family).to_lowercase();
        let url = normalize_pack_url(&font.source);
        if family.is_empty()
            || !families.contains(&family)
            || url.is_empty()
            || entries.iter().any(|e| e.url == url)
        {
            continue;
        }
        let Some(bytes) = media.read(&url) else {
            pkg.missing.push(url);
            continue;
        };
        let name = basename_from_url(&url);
        let path = unique_media_path(
            "fonts",
            if name.is_empty() { &family } else { &name },
            &mut used,
        );
        pkg.files.push((path.clone(), bytes));
        entries.push(PackMediaEntry {
            url,
            path,
            category: "fonts".into(),
            original_name: Some(font.original_name.clone()),
            font_family: Some(if font.family.is_empty() {
                family
            } else {
                font.family.clone()
            }),
        });
    }
    entries
}

fn graphic_json(g: &Graphic) -> String {
    stringify_pretty(&serde_json::to_value(g).expect("graphics serialize"))
}

/// One graphic to put in a project pack.
pub struct PackSource<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// Stored after loading, as FRAME does.
    pub doc: &'a Document,
    pub thumbnail: Option<&'a [u8]>,
}

/// A `.frameproject.zip`.
pub fn write_project_pack(
    project_name: &str,
    graphics: &[PackSource],
    media: &dyn MediaSource,
    fonts: &[UserFont],
    project_thumbnail: Option<&[u8]>,
    exported_at: &str,
) -> Package {
    let mut pkg = Package::default();
    let stored: Vec<Graphic> = graphics.iter().map(|g| g.doc.to_graphic()).collect();
    let mut entries = Vec::new();
    let mut used_paths: Vec<String> = Vec::new();
    for (i, (g, data)) in graphics.iter().zip(&stored).enumerate() {
        let fallback = format!("graphic-{}", i + 1);
        let slug = sanitize_pack_file_name(
            if g.name.is_empty() { &fallback } else { g.name },
            &fallback,
        );
        let mut path = format!("graphics/{:02}-{slug}.json", i + 1);
        if used_paths.contains(&path) {
            path = format!("graphics/{:02}-{slug}-{i}.json", i + 1);
        }
        used_paths.push(path.clone());
        pkg.files
            .push((path.clone(), graphic_json(data).into_bytes()));
        let thumbnail_path = g.thumbnail.map(|bytes| {
            let p = format!("graphics/thumbs/{:02}-{slug}.jpg", i + 1);
            pkg.files.push((p.clone(), bytes.to_vec()));
            p
        });
        let name = js::trim(g.name);
        entries.push(json!({
            "id": g.id,
            "name": if name.is_empty() { "Graphic" } else { name },
            "path": path,
            "thumbnailPath": thumbnail_path,
        }));
    }
    let refs: Vec<&Graphic> = stored.iter().collect();
    let media_entries = pack_media(&refs, media, fonts, &mut pkg);
    if let Some(t) = project_thumbnail {
        pkg.files.push(("thumbnail.jpg".into(), t.to_vec()));
    }
    let name = js::trim(project_name);
    let manifest = json!({
        "format": PROJECT_PACK_FORMAT,
        "version": PACK_VERSION,
        "product": "frame",
        "projectName": if name.is_empty() { "Project" } else { name },
        "exportedAt": exported_at,
        "graphics": entries,
        "media": media_entries,
    });
    pkg.files.push((
        "manifest.json".into(),
        stringify_pretty(&manifest).into_bytes(),
    ));
    pkg
}

/// A `.framepack.zip` holding one graphic.
pub fn write_graphic_pack(
    graphic_name: &str,
    doc: &Document,
    media: &dyn MediaSource,
    fonts: &[UserFont],
    thumbnail: Option<&[u8]>,
    exported_at: &str,
) -> Package {
    let mut pkg = Package::default();
    let data = doc.to_graphic();
    let media_entries = pack_media(&[&data], media, fonts, &mut pkg);
    if let Some(t) = thumbnail {
        pkg.files.push(("thumbnail.jpg".into(), t.to_vec()));
    }
    let name = js::trim(graphic_name);
    let manifest = json!({
        "format": GRAPHIC_PACK_FORMAT,
        "version": PACK_VERSION,
        "product": "frame",
        "graphicName": if name.is_empty() { "Graphic" } else { name },
        "exportedAt": exported_at,
        "media": media_entries,
    });
    pkg.files.push((
        "manifest.json".into(),
        stringify_pretty(&manifest).into_bytes(),
    ));
    pkg.files
        .push(("graphic.json".into(), graphic_json(&data).into_bytes()));
    pkg
}

struct Zip(zip::ZipArchive<std::io::Cursor<Vec<u8>>>);

impl Zip {
    fn file(&mut self, path: &str) -> Option<Vec<u8>> {
        let path = path.trim_start_matches('/');
        let mut f = self.0.by_name(path).ok()?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).ok()?;
        Some(out)
    }
}

fn read_media(zip: &mut Zip, manifest: &Value, warnings: &mut Vec<String>) -> Vec<PackedMedia> {
    let list: Vec<PackMediaEntry> = manifest
        .get("media")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default();
    let mut out = Vec::new();
    for entry in list {
        if entry.path.is_empty() || normalize_pack_url(&entry.url).is_empty() {
            continue;
        }
        match zip.file(&entry.path) {
            Some(bytes) => out.push(PackedMedia { entry, bytes }),
            None => warnings.push(format!("Missing packed file: {}", entry.path)),
        }
    }
    out
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn non_empty_or(s: &str, default: &str) -> String {
    let t = js::trim(s);
    if t.is_empty() {
        default.into()
    } else {
        t.into()
    }
}

/// Opens a pack. Returns it with warnings about files that were missing or
/// unreadable.
pub fn read_pack(bytes: &[u8]) -> Result<(Pack, Vec<String>), PackError> {
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
        .map_err(|e| PackError::NotAZip(e.to_string()))?;
    let mut zip = Zip(archive);
    let manifest_bytes = zip.file("manifest.json").ok_or(PackError::NoManifest)?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| PackError::BadManifest(e.to_string()))?;
    let format = manifest
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| PackError::BadManifest("manifest.json has no format".into()))?;
    let mut warnings = Vec::new();
    let exported_at = str_field(&manifest, "exportedAt");

    match format {
        PROJECT_PACK_FORMAT => {
            let listed = manifest
                .get("graphics")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut graphics = Vec::new();
            for entry in &listed {
                let name = non_empty_or(&str_field(entry, "name"), "Graphic");
                let path = str_field(entry, "path");
                if path.trim_start_matches('/').is_empty() {
                    warnings.push(format!("Skipping graphic with empty path ({name})"));
                    continue;
                }
                let Some(data) = zip.file(&path) else {
                    warnings.push(format!("Missing packed graphic: {path}"));
                    continue;
                };
                let graphic = match std::str::from_utf8(&data)
                    .map_err(|e| e.to_string())
                    .and_then(|t| Graphic::from_json(t).map_err(|e| e.to_string()))
                {
                    Ok(g) => g,
                    Err(e) => {
                        warnings.push(format!("Could not import {path}: {e}"));
                        continue;
                    }
                };
                let thumbnail = entry
                    .get("thumbnailPath")
                    .and_then(Value::as_str)
                    .and_then(|p| zip.file(p));
                graphics.push(PackedGraphic {
                    id: str_field(entry, "id"),
                    name,
                    graphic,
                    thumbnail,
                });
            }
            if graphics.is_empty() {
                return Err(PackError::NoGraphics);
            }
            let media = read_media(&mut zip, &manifest, &mut warnings);
            let pack = ProjectPack {
                name: non_empty_or(&str_field(&manifest, "projectName"), "Imported Project"),
                exported_at,
                graphics,
                media,
                thumbnail: zip.file("thumbnail.jpg"),
            };
            Ok((Pack::Project(pack), warnings))
        }
        GRAPHIC_PACK_FORMAT => {
            let data = zip.file("graphic.json").ok_or(PackError::NoGraphics)?;
            let text =
                std::str::from_utf8(&data).map_err(|e| PackError::BadManifest(e.to_string()))?;
            let graphic = Graphic::from_json(text)
                .map_err(|e| PackError::BadManifest(format!("graphic.json: {e}")))?;
            let media = read_media(&mut zip, &manifest, &mut warnings);
            let pack = GraphicPack {
                graphic: PackedGraphic {
                    id: String::new(),
                    name: non_empty_or(&str_field(&manifest, "graphicName"), "Graphic"),
                    graphic,
                    thumbnail: zip.file("thumbnail.jpg"),
                },
                exported_at,
                media,
            };
            Ok((Pack::Graphic(Box::new(pack)), warnings))
        }
        other => Err(PackError::UnsupportedFormat(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(normalize_pack_url(" uploads/a.png?x=1 "), "/uploads/a.png");
        assert!(is_shared_install_url("/fonts/Inter.ttf"));
        assert!(should_pack("/lottie/noto-emoji/disguise.json"));
        assert!(!should_pack("/particles/p.png"));
        assert_eq!(category_from_url("/uploads/u/lottie/a.json"), "lottie");
        assert_eq!(
            basename_from_url("https://x.org/a/b%20c.png?q"),
            "b%20c.png"
        );
        assert_eq!(basename_from_url("/uploads/"), "uploads");
    }

    #[test]
    fn pack_file_names() {
        assert_eq!(
            sanitize_pack_file_name("Pies and charts", "g"),
            "Pies-and-charts"
        );
        assert_eq!(sanitize_pack_file_name("  --Ä!!b-- ", "g"), "b");
        assert_eq!(sanitize_pack_file_name("***", "graphic-1"), "graphic-1");
    }

    #[test]
    fn rejects_non_packs() {
        assert!(matches!(read_pack(b"hello"), Err(PackError::NotAZip(_))));
        let mut p = Package::default();
        p.files
            .push(("manifest.json".into(), br#"{"format":"other"}"#.to_vec()));
        let zip = p.to_zip().unwrap();
        assert_eq!(
            read_pack(&zip).unwrap_err(),
            PackError::UnsupportedFormat("other".into())
        );
    }
}
