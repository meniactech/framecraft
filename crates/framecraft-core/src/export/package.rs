// SPDX-License-Identifier: AGPL-3.0-or-later
//! Complete export packages: SPX, OGraf and HTML banner ZIPs.
//!
//! A [`Package`] is the list of files in the order FRAME wrote them;
//! [`Package::to_zip`] turns it into ZIP bytes. Media and fonts come from a
//! [`MediaSource`], so the exporter does not care where files live.

use super::assets::{AssetCopy, AssetMap, UserFont, font_faces};
use super::html::{PageKind, SPX_INTERFACE_JS, page_html};
use super::manifest::{
    AIR_MANIFEST_PATH, AirTemplate, ExportInfoKind, PackageFormat, air_render_manifest,
    export_info, ograf_component_source, ograf_manifest, ograf_step_count,
};
use super::names::{file_base_name, html_entry_name, template_slug};
use super::template::{ExportOptions, TemplateExport, build};
use crate::js::stringify_pretty;
use crate::normalize::Document;
use crate::profile::Profile;
use std::io::Write;

/// Reads media, sprites and fonts by the URL a graphic uses
/// (`/uploads/…`, `/particles/…`, `/fonts/…`).
pub trait MediaSource {
    /// The file's bytes, or `None` when it cannot be found.
    fn read(&self, url: &str) -> Option<Vec<u8>>;
}

impl<F: Fn(&str) -> Option<Vec<u8>>> MediaSource for F {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        self(url)
    }
}

/// The playout runtime and libraries copied into every package.
const RUNTIME: &[u8] = include_bytes!("../../runtime/ionframe-spx-runtime.mjs");
const ANIME: &[u8] = include_bytes!("../../runtime/lib/anime.esm.min.js");
const CHART: &[u8] = include_bytes!("../../runtime/lib/chart.umd.min.js");
const QRCODE: &[u8] = include_bytes!("../../runtime/lib/qrcode.min.js");

/// Package path of the template script, when the graphic has one.
pub const TEMPLATE_JS_PATH: &str = "js/template.js";

/// What to export.
pub struct ExportRequest<'a> {
    pub doc: &'a Document,
    /// Graphic name: sets the template id and the HTML file name.
    pub graphic_name: &'a str,
    /// Active palette colors, for chart points without a color.
    pub palette_colors: &'a [String],
    /// Fonts the user added.
    pub user_fonts: &'a [UserFont],
    /// ISO time written into the metadata.
    pub exported_at: &'a str,
}

/// Files of one export, in writing order.
#[derive(Debug, Clone, Default)]
pub struct Package {
    pub files: Vec<(String, Vec<u8>)>,
    /// Media or fonts the source could not provide. FRAME left them out
    /// silently; FrameCraft lists them so the UI can warn.
    pub missing: Vec<String>,
}

impl Package {
    fn add(&mut self, path: &str, bytes: impl Into<Vec<u8>>) {
        let bytes = bytes.into();
        // A later file with the same path replaces the earlier one (as a ZIP
        // library does), keeping the first position.
        match self.files.iter_mut().find(|(p, _)| p == path) {
            Some(entry) => entry.1 = bytes,
            None => self.files.push((path.to_string(), bytes)),
        }
    }

    fn copy(&mut self, copies: &[AssetCopy], media: &dyn MediaSource) {
        for c in copies {
            match media.read(&c.source) {
                Some(bytes) => self.add(&c.target, bytes),
                None => {
                    if !self.missing.contains(&c.source) {
                        self.missing.push(c.source.clone());
                    }
                }
            }
        }
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.as_slice())
    }

    /// ZIP bytes. Timestamps are fixed, so the same input gives the same file.
    pub fn to_zip(&self) -> std::io::Result<Vec<u8>> {
        use zip::write::SimpleFileOptions;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in &self.files {
            w.start_file(path.as_str(), opts)
                .map_err(std::io::Error::other)?;
            w.write_all(bytes)?;
        }
        Ok(w.finish().map_err(std::io::Error::other)?.into_inner())
    }
}

/// Writes the graphic's script to `path` (trimmed, ending in a newline) and
/// returns the path, or `None` when there is no script.
fn template_js_file(t: &TemplateExport, pkg: &mut Package, path: &str) -> Option<String> {
    let src = t.template_js.as_deref()?.trim();
    if src.is_empty() {
        return None;
    }
    let text = if src.ends_with('\n') {
        src.to_string()
    } else {
        format!("{src}\n")
    };
    pkg.add(path, text);
    Some(path.to_string())
}

/// `js/template-<id>.js` for one graphic of a suite.
pub fn suite_template_js_path(graphic_id: &str) -> String {
    let id = if graphic_id.is_empty() {
        "graphic"
    } else {
        graphic_id
    };
    let safe: String = id
        .chars()
        .flat_map(|c| {
            let keep = c.is_ascii_alphanumeric() || c == '_' || c == '-';
            // JavaScript replaces per UTF-16 unit.
            std::iter::repeat_n(
                if keep { c } else { '_' },
                if keep { 1 } else { c.len_utf16() },
            )
        })
        .take(64)
        .collect();
    format!(
        "js/template-{}.js",
        if safe.is_empty() { "graphic" } else { &safe }
    )
}

pub(crate) fn export_for(req: &ExportRequest, fallback_id: &str, p: &Profile) -> TemplateExport {
    let opts = ExportOptions {
        template_id: template_slug(req.graphic_name, fallback_id),
        palette_colors: req.palette_colors.to_vec(),
    };
    build(req.doc, &opts, p)
}

fn shared_runtime(pkg: &mut Package, with_spx_interface: bool) {
    pkg.add("js/lib/anime.esm.min.js", ANIME);
    pkg.add("js/lib/chart.umd.min.js", CHART);
    pkg.add("js/lib/qrcode.min.js", QRCODE);
    pkg.add("js/ionframe-spx-runtime.mjs", RUNTIME);
    if with_spx_interface {
        pkg.add("js/spx_interface.js", SPX_INTERFACE_JS);
    }
}

/// HTML, script, assets and fonts common to SPX and OGraf packages.
/// Returns the remapped export and the entry file name.
fn spx_core(
    req: &ExportRequest,
    media: &dyn MediaSource,
    p: &Profile,
    pkg: &mut Package,
) -> (TemplateExport, String) {
    let html = html_entry_name(req.graphic_name);
    shared_runtime(pkg, true);
    let mut assets = AssetMap::spx();
    let t = assets.remap(&export_for(req, "ionframe_template", p));
    pkg.copy(assets.copies(), media);
    let (fonts, font_copies) =
        font_faces(&t, req.user_fonts, false, &|src| media.read(src).is_some());
    pkg.copy(&font_copies, media);
    let js = template_js_file(&t, pkg, TEMPLATE_JS_PATH);
    pkg.add(
        &html,
        page_html(&t, &fonts, PageKind::Spx, js.as_deref(), p),
    );
    (t, html)
}

/// An SPX template ZIP.
pub fn spx_package(req: &ExportRequest, media: &dyn MediaSource, p: &Profile) -> Package {
    let mut pkg = Package::default();
    let (t, html) = spx_core(req, media, p, &mut pkg);
    pkg.add(
        "meta/export-info.json",
        stringify_pretty(&export_info(
            &t,
            ExportInfoKind::Spx,
            &html,
            req.exported_at,
            p,
        )),
    );
    let air = [AirTemplate {
        payload: &t,
        entry_html: &html,
        name: None,
        ograf_main: None,
    }];
    pkg.add(
        AIR_MANIFEST_PATH,
        stringify_pretty(&air_render_manifest(
            PackageFormat::SpxDraft,
            &air,
            None,
            req.exported_at,
            p,
        )),
    );
    pkg
}

/// An OGraf package ZIP: the SPX HTML plus an OGraf manifest and component.
pub fn ograf_package(req: &ExportRequest, media: &dyn MediaSource, p: &Profile) -> Package {
    let mut pkg = Package::default();
    let (t, html) = spx_core(req, media, p, &mut pkg);
    let desc = &t.spx_definition.description;
    let base = file_base_name(
        if desc.is_empty() {
            &t.template_id
        } else {
            desc
        },
        if t.template_id.is_empty() {
            "template"
        } else {
            &t.template_id
        },
    );
    let (main, manifest) = (format!("{base}.mjs"), format!("{base}.ograf.json"));
    pkg.add(&main, ograf_component_source(&html, ograf_step_count(&t)));
    pkg.add(
        &manifest,
        stringify_pretty(&ograf_manifest(&t, &main, &html, None, p)),
    );
    pkg.add(
        "meta/export-info.json",
        stringify_pretty(&export_info(
            &t,
            ExportInfoKind::Ograf {
                manifest: &manifest,
                main: &main,
            },
            &html,
            req.exported_at,
            p,
        )),
    );
    let air = [AirTemplate {
        payload: &t,
        entry_html: &html,
        name: None,
        ograf_main: Some(&main),
    }];
    pkg.add(
        AIR_MANIFEST_PATH,
        stringify_pretty(&air_render_manifest(
            PackageFormat::Ograf,
            &air,
            None,
            req.exported_at,
            p,
        )),
    );
    pkg
}

/// A self-contained HTML banner ZIP (plays and loops on its own).
pub fn banner_package(req: &ExportRequest, media: &dyn MediaSource, p: &Profile) -> Package {
    let mut pkg = Package::default();
    let html = html_entry_name(req.graphic_name);
    let mut assets = AssetMap::banner();
    let t = assets.remap(&export_for(req, "ionframe_banner", p));
    pkg.copy(assets.copies(), media);
    let (fonts, font_copies) =
        font_faces(&t, req.user_fonts, true, &|src| media.read(src).is_some());
    pkg.copy(&font_copies, media);
    shared_runtime(&mut pkg, false);
    let js = template_js_file(&t, &mut pkg, TEMPLATE_JS_PATH);
    pkg.add(
        &html,
        page_html(&t, &fonts, PageKind::Banner, js.as_deref(), p),
    );
    pkg.add(
        "meta/export-info.json",
        stringify_pretty(&export_info(
            &t,
            ExportInfoKind::Banner,
            &html,
            req.exported_at,
            p,
        )),
    );
    let air = [AirTemplate {
        payload: &t,
        entry_html: &html,
        name: None,
        ograf_main: None,
    }];
    pkg.add(
        AIR_MANIFEST_PATH,
        stringify_pretty(&air_render_manifest(
            PackageFormat::HtmlBanner,
            &air,
            None,
            req.exported_at,
            p,
        )),
    );
    pkg
}

/// One graphic of a whole-project export.
pub struct SuiteGraphic<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub doc: &'a Document,
}

/// What to export as one suite package.
pub struct SuiteRequest<'a> {
    pub project_name: &'a str,
    pub graphics: &'a [SuiteGraphic<'a>],
    pub palette_colors: &'a [String],
    pub user_fonts: &'a [UserFont],
    pub exported_at: &'a str,
}

/// `name`, then `name_2`, `name_3`, … (case-insensitive).
fn unique_name(base: &str, used: &mut Vec<String>, ext: &str) -> String {
    let mut candidate = format!("{base}{ext}");
    let mut i = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = format!("{base}_{i}{ext}");
        i += 1;
    }
    used.push(candidate.to_lowercase());
    candidate
}

/// All graphics of a project in one SPX ZIP (`ograf: false`) or one OGraf
/// ZIP (`ograf: true`), sharing `js/`, `assets/` and `fonts/`.
pub fn suite_package(
    req: &SuiteRequest,
    ograf: bool,
    media: &dyn MediaSource,
    p: &Profile,
) -> Package {
    let mut pkg = Package::default();
    shared_runtime(&mut pkg, true);
    let mut assets = AssetMap::spx();
    let mut used = Vec::new();
    let mut entries = Vec::new();
    let mut exports = Vec::new();

    for g in req.graphics {
        let single = ExportRequest {
            doc: g.doc,
            graphic_name: g.name,
            palette_colors: req.palette_colors,
            user_fonts: req.user_fonts,
            exported_at: req.exported_at,
        };
        let base = file_base_name(g.name, "template");
        let (html, base) = if ograf {
            let b = unique_name(&base, &mut used, "");
            (format!("{b}.html"), b)
        } else {
            (unique_name(&base, &mut used, ".html"), base)
        };
        let before = assets.copies().len();
        let mut t = assets.remap(&export_for(&single, "ionframe_template", p));
        pkg.copy(&assets.copies()[before..], media);
        if ograf {
            let fallback = if t.template_id.is_empty() {
                "template".to_string()
            } else {
                t.template_id.clone()
            };
            t.template_id = template_slug(&base, &fallback);
        }
        let (fonts, font_copies) =
            font_faces(&t, req.user_fonts, false, &|src| media.read(src).is_some());
        pkg.copy(&font_copies, media);
        let js = template_js_file(&t, &mut pkg, &suite_template_js_path(g.id));
        pkg.add(
            &html,
            page_html(&t, &fonts, PageKind::Spx, js.as_deref(), p),
        );

        let mut entry = serde_json::Map::new();
        entry.insert("graphicId".into(), g.id.into());
        entry.insert("graphicName".into(), g.name.into());
        entry.insert("htmlFileName".into(), html.clone().into());
        let mut main = None;
        if ograf {
            let (m, manifest) = (format!("{base}.mjs"), format!("{base}.ograf.json"));
            pkg.add(&m, ograf_component_source(&html, ograf_step_count(&t)));
            pkg.add(
                &manifest,
                stringify_pretty(&ograf_manifest(&t, &m, &html, Some(g.name), p)),
            );
            entry.insert("ografManifest".into(), manifest.into());
            entry.insert("ografMain".into(), m.clone().into());
            main = Some(m);
        }
        entry.insert("templateId".into(), t.template_id.clone().into());
        entry.insert("width".into(), t.scene.width.into());
        entry.insert("height".into(), t.scene.height.into());
        entries.push(serde_json::Value::Object(entry));
        exports.push((t, html, main, g.name));
    }

    let info = serde_json::json!({
        "generator": p.generator,
        "profile": if ograf { "ograf-suite-v1" } else { "spx-draft-suite-esm" },
        "projectName": req.project_name,
        "templateCount": entries.len(),
        "exportedAt": req.exported_at,
        "templates": entries,
        "airRenderManifest": AIR_MANIFEST_PATH,
    });
    pkg.add("meta/export-info.json", stringify_pretty(&info));
    let air: Vec<AirTemplate> = exports
        .iter()
        .map(|(t, html, main, name)| AirTemplate {
            payload: t,
            entry_html: html,
            name: Some(name),
            ograf_main: main.as_deref(),
        })
        .collect();
    let format = if ograf {
        PackageFormat::OgrafSuite
    } else {
        PackageFormat::SpxDraftSuite
    };
    pkg.add(
        AIR_MANIFEST_PATH,
        stringify_pretty(&air_render_manifest(
            format,
            &air,
            Some(req.project_name),
            req.exported_at,
            p,
        )),
    );
    pkg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_file_replaces_earlier_in_place() {
        let mut p = Package::default();
        p.add("a", "1");
        p.add("b", "2");
        p.add("a", "3");
        assert_eq!(
            p.files,
            vec![
                ("a".to_string(), b"3".to_vec()),
                ("b".to_string(), b"2".to_vec())
            ]
        );
    }

    #[test]
    fn suite_names() {
        assert_eq!(
            suite_template_js_path("gfx_1.2 x"),
            "js/template-gfx_1_2_x.js"
        );
        assert_eq!(suite_template_js_path(""), "js/template-graphic.js");
        let mut used = Vec::new();
        assert_eq!(unique_name("Untitled", &mut used, ".html"), "Untitled.html");
        assert_eq!(
            unique_name("untitled", &mut used, ".html"),
            "untitled_2.html"
        );
        assert_eq!(
            unique_name("Untitled", &mut used, ".html"),
            "Untitled_3.html"
        );
    }

    #[test]
    fn zip_is_deterministic_and_readable() {
        let mut p = Package::default();
        p.add("x.txt", "hello");
        p.add("js/y.js", "world");
        let a = p.to_zip().unwrap();
        assert_eq!(a, p.to_zip().unwrap());
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(a)).unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("js/y.js").unwrap(), &mut s).unwrap();
        assert_eq!(s, "world");
    }
}
