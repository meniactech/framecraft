// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project media: importing files into a project pack, what kind of media
//! a file is, and the pixel size of images (for new image layers).

use crate::pack::{PackMediaEntry, PackedMedia, ProjectPack, normalize_pack_url};

/// Media library categories, as FRAME's: `(category, label)`.
pub const CATEGORIES: &[(&str, &str)] = &[
    ("images", "Images"),
    ("videos", "Videos"),
    ("lottie", "Lottie"),
];

/// The category of a file by its name, or `None` when FrameCraft does not
/// import that kind of file.
pub fn category_for(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" => "images",
        "mp4" | "webm" | "mov" | "m4v" => "videos",
        "json" | "lottie" => "lottie",
        _ => return None,
    })
}

/// File extensions a category's Import button offers.
pub fn accept(category: &str) -> &'static str {
    match category {
        "videos" => ".mp4,.webm,.mov,.m4v",
        "lottie" => ".json",
        _ => ".png,.jpg,.jpeg,.gif,.webp,.svg,.avif",
    }
}

fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A file name safe in URLs and ZIP paths: letters, digits, `.`, `_`, `-`.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut collapsed = String::with_capacity(safe.len());
    for c in safe.chars() {
        if !(c == '-' && collapsed.ends_with('-')) {
            collapsed.push(c);
        }
    }
    let safe = collapsed.trim_matches(['-', '.']).to_string();
    if safe.is_empty() { "file".into() } else { safe }
}

/// Adds a file to the project's media and returns its entry. The same
/// bytes imported again give the existing entry. The URL is
/// `/media/<category>/<hash>-<name>`.
pub fn import(
    pack: &mut ProjectPack,
    name: &str,
    bytes: Vec<u8>,
) -> Result<PackMediaEntry, String> {
    let category =
        category_for(name).ok_or_else(|| format!("{name}: not a supported media file"))?;
    if let Some(m) = pack
        .media
        .iter()
        .find(|m| m.entry.category == category && m.bytes == bytes)
    {
        return Ok(m.entry.clone());
    }
    let file = format!("{:08x}-{}", fnv64(&bytes) as u32, safe_name(name));
    let entry = PackMediaEntry {
        url: format!("/media/{category}/{file}"),
        path: format!("media/{category}/{file}"),
        category: category.into(),
        original_name: Some(name.rsplit(['/', '\\']).next().unwrap_or(name).to_string()),
        font_family: None,
    };
    pack.media.push(PackedMedia {
        entry: entry.clone(),
        bytes,
    });
    Ok(entry)
}

/// The project's media of one category, in import order.
pub fn of_category<'a>(pack: &'a ProjectPack, category: &str) -> Vec<&'a PackMediaEntry> {
    pack.media
        .iter()
        .map(|m| &m.entry)
        .filter(|e| e.category == category)
        .collect()
}

/// A shown name for a media URL: the original file name when known.
pub fn display_name(pack: &ProjectPack, url: &str) -> String {
    let wanted = normalize_pack_url(url);
    pack.media
        .iter()
        .find(|m| normalize_pack_url(&m.entry.url) == wanted)
        .and_then(|m| m.entry.original_name.clone())
        .unwrap_or_else(|| url.rsplit('/').next().unwrap_or(url).to_string())
}

/// Pixel size of a PNG, JPEG, GIF, WebP or SVG image.
pub fn image_size(bytes: &[u8]) -> Option<(f64, f64)> {
    let be16 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?])))
    };
    let le16 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?])))
    };
    let be32 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
    };
    let le24 = |b: &[u8], i: usize| -> Option<u32> {
        Some(
            u32::from(*b.get(i)?)
                | u32::from(*b.get(i + 1)?) << 8
                | u32::from(*b.get(i + 2)?) << 16,
        )
    };
    let size = |w: u32, h: u32| (w > 0 && h > 0).then_some((f64::from(w), f64::from(h)));
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return size(be32(bytes, 16)?, be32(bytes, 20)?);
    }
    if bytes.starts_with(b"GIF8") {
        return size(le16(bytes, 6)?, le16(bytes, 8)?);
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        // Walk the segments to the first start-of-frame.
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                return size(be16(bytes, i + 7)?, be16(bytes, i + 5)?);
            }
            if marker == 0xff || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
                i += if marker == 0xff { 1 } else { 2 };
                continue;
            }
            i += 2 + be16(bytes, i + 2)? as usize;
        }
        return None;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8 " => size(le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff),
            b"VP8L" => {
                let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
                size((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
            }
            b"VP8X" => size(le24(bytes, 24)? + 1, le24(bytes, 27)? + 1),
            _ => None,
        };
    }
    svg_size(std::str::from_utf8(bytes).ok()?)
}

/// `width` / `height` of the root `<svg>`, or its `viewBox` size.
fn svg_size(text: &str) -> Option<(f64, f64)> {
    let start = text.find("<svg")?;
    let tag = &text[start..start + text[start..].find('>')?];
    let attr = |name: &str| -> Option<&str> {
        let mut rest = tag;
        loop {
            let at = rest.find(name)?;
            let before = rest[..at].chars().last();
            rest = &rest[at + name.len()..];
            if before.is_some_and(char::is_whitespace) {
                let r = rest.trim_start().strip_prefix('=')?.trim_start();
                let q = r.chars().next()?;
                let r = &r[1..];
                return Some(&r[..r.find(q)?]);
            }
        }
    };
    let px = |v: &str| -> Option<f64> {
        let v = v.trim().trim_end_matches("px");
        v.parse::<f64>().ok().filter(|n| *n > 0.0)
    };
    if let (Some(w), Some(h)) = (attr("width").and_then(px), attr("height").and_then(px)) {
        return Some((w, h));
    }
    let vb: Vec<f64> = attr("viewBox")?
        .split([' ', ','])
        .filter_map(|s| s.parse().ok())
        .collect();
    (vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0).then(|| (vb[2], vb[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> ProjectPack {
        ProjectPack {
            name: "P".into(),
            exported_at: String::new(),
            graphics: Vec::new(),
            media: Vec::new(),
            thumbnail: None,
        }
    }

    #[test]
    fn import_names_and_dedupes() {
        let mut p = pack();
        let a = import(&mut p, "/home/me/My Logo (1).PNG", vec![1, 2, 3]).unwrap();
        assert!(a.url.starts_with("/media/images/") && a.url.ends_with("-My-Logo-1-.PNG"));
        assert_eq!(a.original_name.as_deref(), Some("My Logo (1).PNG"));
        let again = import(&mut p, "copy.png", vec![1, 2, 3]).unwrap();
        assert_eq!(again.url, a.url);
        assert_eq!(p.media.len(), 1);
        assert!(import(&mut p, "notes.txt", vec![1]).is_err());
        assert_eq!(category_for("clip.webm"), Some("videos"));
        assert_eq!(of_category(&p, "images").len(), 1);
        use crate::export::package::MediaSource;
        assert_eq!(p.read(&a.url), Some(vec![1, 2, 3]));
    }

    #[test]
    fn image_sizes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(640u32.to_be_bytes());
        png.extend(360u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((640.0, 360.0)));
        let gif = b"GIF89a\x40\x01\xf0\x00";
        assert_eq!(image_size(gif), Some((320.0, 240.0)));
        // SOI, an APP0 segment, then SOF0 with height 100, width 200.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00,
            0x64, 0x00, 0xc8, 0x03,
        ];
        assert_eq!(image_size(&jpeg), Some((200.0, 100.0)));
        assert_eq!(
            image_size(br#"<?xml?><svg xmlns="x" width="48px" height='24'>"#),
            Some((48.0, 24.0))
        );
        assert_eq!(
            image_size(br#"<svg stroke-width="2" viewBox="0 0 100 50">"#),
            Some((100.0, 50.0))
        );
        assert_eq!(image_size(b"nope"), None);
    }
}
