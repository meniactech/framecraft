// SPDX-License-Identifier: AGPL-3.0-or-later
//! Media and fonts a package needs: where each file goes inside the ZIP,
//! and the template export rewritten to point there. Reading the files is
//! up to the caller (see the package writers).

use super::template::TemplateExport;
use crate::js;

/// A file to copy into the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetCopy {
    /// Where the file comes from: a URL path such as `/uploads/…/a.png`.
    pub source: String,
    /// Path inside the package, such as `assets/a.png`.
    pub target: String,
}

/// The last path segment of a URL, without query or fragment.
pub fn basename(url: &str) -> String {
    let clean = url.split(['?', '#']).next().unwrap_or("");
    let name = clean.rsplit('/').next().unwrap_or("");
    if name.is_empty() {
        "asset.bin".into()
    } else {
        name.into()
    }
}

/// Assigns package paths under `assets/`. Shared by all templates of a
/// suite package.
#[derive(Debug, Clone)]
pub struct AssetMap {
    copies: Vec<AssetCopy>,
    unique_names: bool,
}

impl AssetMap {
    /// SPX and OGraf packages: different files with the same name get
    /// `_2`, `_3`, … suffixes.
    pub fn spx() -> Self {
        Self {
            copies: Vec::new(),
            unique_names: true,
        }
    }

    /// HTML banners, as FRAME made them: plain file names, so a later file
    /// with the same name replaces an earlier one.
    pub fn banner() -> Self {
        Self {
            copies: Vec::new(),
            unique_names: false,
        }
    }

    pub fn copies(&self) -> &[AssetCopy] {
        &self.copies
    }

    fn path_for(&mut self, url: &str) -> String {
        if let Some(c) = self.copies.iter().find(|c| c.source == url) {
            return c.target.clone();
        }
        let mut file = basename(url);
        let taken = |name: &str, copies: &[AssetCopy]| {
            copies.iter().any(|c| c.target == format!("assets/{name}"))
        };
        if self.unique_names && taken(&file, &self.copies) {
            let (stem, ext) = match file.rfind('.') {
                Some(dot) if dot > 0 => (file[..dot].to_string(), file[dot..].to_string()),
                _ => (file.clone(), String::new()),
            };
            let mut i = 2;
            while taken(&format!("{stem}_{i}{ext}"), &self.copies) {
                i += 1;
            }
            file = format!("{stem}_{i}{ext}");
        }
        let target = format!("assets/{file}");
        self.copies.push(AssetCopy {
            source: url.to_string(),
            target: target.clone(),
        });
        target
    }

    /// A copy of the export with media, particle sprite and background
    /// image URLs pointing into the package.
    pub fn remap(&mut self, t: &TemplateExport) -> TemplateExport {
        // Same order as FRAME: element media and sprites first, then fills.
        let mut urls: Vec<String> = Vec::new();
        for el in &t.elements {
            let sprite = el
                .particles
                .as_ref()
                .and_then(|p| p.particle_sprite_url.clone());
            for u in [el.media_url.clone(), sprite].into_iter().flatten() {
                if !js::trim(&u).is_empty() && !urls.contains(&u) {
                    urls.push(u);
                }
            }
        }
        for el in &t.elements {
            if let Some(bg) = el
                .style
                .bg_image_url
                .as_deref()
                .map(js::trim)
                .filter(|u| !u.is_empty())
                && !urls.iter().any(|u| u == bg)
            {
                urls.push(bg.to_string());
            }
        }
        for u in &urls {
            self.path_for(u);
        }

        let lookup = |u: &str| {
            self.copies
                .iter()
                .find(|c| c.source == u)
                .map(|c| c.target.clone())
        };
        let mut out = t.clone();
        for el in &mut out.elements {
            if let Some(bg) = el.style.bg_image_url.clone()
                && let Some(p) = lookup(js::trim(&bg))
            {
                el.style.bg_image_url = Some(p);
            }
            if let Some(m) = el.media_url.clone().filter(|m| !m.is_empty())
                && let Some(p) = lookup(&m)
            {
                el.media_url = Some(p);
            }
            if let Some(particles) = el.particles.as_mut()
                && let Some(s) = particles.particle_sprite_url.clone()
                && let Some(p) = lookup(&s)
            {
                particles.particle_sprite_url = Some(p);
            }
        }
        out
    }
}

/// A font the user uploaded.
#[derive(Debug, Clone)]
pub struct UserFont {
    /// Family name; when empty, the file name without extension.
    pub family: String,
    pub original_name: String,
    /// Where the file comes from.
    pub source: String,
}

/// Fonts that ship with FrameCraft: family and source path.
pub const BUILTIN_FONTS: [(&str, &str); 6] = [
    ("Inter", "/fonts/Inter-VariableFont_opsz,wght.ttf"),
    ("Open Sans", "/fonts/OpenSans-VariableFont_wdth,wght.ttf"),
    ("Poppins", "/fonts/Poppins-Regular.ttf"),
    ("Montserrat", "/fonts/Montserrat-VariableFont_wght.ttf"),
    ("Roboto", "/fonts/Roboto-VariableFont_wdth,wght.ttf"),
    ("Mouse Memoirs", "/fonts/MouseMemoirs-Regular.ttf"),
];

/// `@font-face` rules for the fonts the export uses (uploaded fonts first,
/// then built-in ones), and the files to copy into `fonts/`.
///
/// `available` says whether a font file can be read; unreadable fonts are
/// skipped.
///
/// When an uploaded font has a built-in font's family, SPX packages still
/// add the built-in rule (pointing at a file that is not copied); banners
/// skip it. Both as in FRAME.
pub fn font_faces(
    t: &TemplateExport,
    uploaded: &[UserFont],
    banner: bool,
    available: &dyn Fn(&str) -> bool,
) -> (Vec<String>, Vec<AssetCopy>) {
    let used: Vec<String> = t
        .elements
        .iter()
        .filter_map(|e| e.style.font_family.as_deref().map(js::trim))
        .filter(|f| !f.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut rules = Vec::new();
    let mut copies: Vec<AssetCopy> = Vec::new();
    let mut exported: Vec<String> = Vec::new();
    let mut add = |family: &str, source: &str, builtin: bool| {
        let key = family.to_lowercase();
        if !used.contains(&key) || (banner && builtin && exported.contains(&key)) {
            return;
        }
        let target = format!("fonts/{}", basename(source));
        if !exported.contains(&key) {
            // A font that cannot be read gets no rule, as in FRAME.
            if !available(source) {
                return;
            }
            exported.push(key);
            copies.push(AssetCopy {
                source: source.to_string(),
                target: target.clone(),
            });
        }
        rules.push(format!(
            "@font-face {{ font-family: '{}'; src: url('./{target}'); font-weight: 100 900; font-style: normal; font-display: swap; }}",
            family.replace('\'', "\\'")
        ));
    };
    for f in uploaded {
        let family = js::trim(&f.family);
        let family = if family.is_empty() {
            let name = &f.original_name;
            js::trim(
                name.rfind('.')
                    .filter(|d| *d + 1 < name.len())
                    .map_or(name.as_str(), |d| &name[..d]),
            )
            .to_string()
        } else {
            family.to_string()
        };
        add(&family, &f.source, false);
    }
    for (family, source) in BUILTIN_FONTS {
        add(family, source, true);
    }
    (rules, copies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basenames() {
        assert_eq!(basename("/uploads/u/images/a.png?v=1#x"), "a.png");
        assert_eq!(basename("/uploads/"), "asset.bin");
    }

    #[test]
    fn unique_targets() {
        let mut m = AssetMap::spx();
        assert_eq!(m.path_for("/a/logo.png"), "assets/logo.png");
        assert_eq!(m.path_for("/b/logo.png"), "assets/logo_2.png");
        assert_eq!(m.path_for("/c/logo.png"), "assets/logo_3.png");
        assert_eq!(m.path_for("/a/logo.png"), "assets/logo.png");
        assert_eq!(m.path_for("/d/README"), "assets/README");
        let mut b = AssetMap::banner();
        assert_eq!(b.path_for("/a/logo.png"), "assets/logo.png");
        assert_eq!(b.path_for("/b/logo.png"), "assets/logo.png");
    }
}
