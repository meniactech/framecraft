// SPDX-License-Identifier: AGPL-3.0-or-later
//! An open project: the pack it came from, its graphics loaded for editing,
//! and the media they use.

use framecraft_core::commands::Change;
use framecraft_core::export::package::{
    ExportRequest, MediaSource, Package, banner_package, ograf_package, spx_package,
};
use framecraft_core::normalize::{Document, normalize};
use framecraft_core::pack::{Pack, PackSource, ProjectPack, read_pack, write_project_pack};
use framecraft_core::preview::{self, PreviewState, PreviewUpdate};
use framecraft_core::profile::Profile;
use std::path::{Path, PathBuf};

/// Media from the open pack, then FrameCraft's built-in files (fonts,
/// particle sprites, stock animations).
pub struct ProjectMedia {
    pub pack: ProjectPack,
    pub builtin_dir: Option<PathBuf>,
}

impl MediaSource for ProjectMedia {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = self.pack.read(url) {
            return Some(bytes);
        }
        let rel = url.split(['?', '#']).next()?.strip_prefix('/')?;
        if rel.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(self.builtin_dir.as_ref()?.join(rel)).ok()
    }
}

pub struct Project {
    pub path: PathBuf,
    pub media: ProjectMedia,
    /// One per graphic in the pack, in pack order.
    pub docs: Vec<Document>,
    /// Problems found while opening (missing files and the like).
    pub warnings: Vec<String>,
}

/// Where built-in files live. Until FrameCraft ships its own, it uses
/// `FRAMECRAFT_BUILTIN_DIR` or a FRAME checkout next to this repository.
pub fn builtin_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FRAMECRAFT_BUILTIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/static");
    dev.is_dir().then_some(dev)
}

impl Project {
    pub fn open(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let (pack, warnings) = read_pack(&bytes).map_err(|e| e.to_string())?;
        let pack = match pack {
            Pack::Project(p) => p,
            Pack::Graphic(g) => ProjectPack {
                name: g.graphic.name.clone(),
                exported_at: g.exported_at.clone(),
                graphics: vec![g.graphic.clone()],
                media: g.media.clone(),
                thumbnail: None,
            },
        };
        let profile = Profile::framecraft();
        let docs = pack
            .graphics
            .iter()
            .map(|g| normalize(&g.graphic, &profile))
            .collect();
        Ok(Self {
            path: path.to_path_buf(),
            media: ProjectMedia {
                pack,
                builtin_dir: builtin_dir(),
            },
            docs,
            warnings,
        })
    }

    pub fn name(&self) -> &str {
        &self.media.pack.name
    }

    pub fn graphic_name(&self, index: usize) -> &str {
        &self.media.pack.graphics[index].name
    }

    fn request<'a>(&'a self, index: usize, exported_at: &'a str) -> ExportRequest<'a> {
        ExportRequest {
            doc: &self.docs[index],
            graphic_name: self.graphic_name(index),
            palette_colors: &[],
            user_fonts: &[],
            exported_at,
        }
    }

    /// The preview package the viewport shows (SPX package plus the
    /// editor script) and the state it was built from.
    pub fn preview(&self, index: usize) -> (Package, PreviewState) {
        preview::build(
            &self.request(index, ""),
            &self.media,
            &Profile::framecraft(),
        )
    }

    /// The update that brings a loaded preview in line after an edit.
    pub fn preview_update(
        &self,
        index: usize,
        prev: &PreviewState,
        change: &Change,
    ) -> (PreviewUpdate, PreviewState) {
        preview::update(
            prev,
            change,
            &self.request(index, ""),
            &self.media,
            &Profile::framecraft(),
        )
    }

    /// Writes one export ZIP next to the project file and returns its path.
    pub fn export(&self, index: usize, format: &str) -> Result<PathBuf, String> {
        let now = now_iso();
        let req = self.request(index, &now);
        let p = Profile::framecraft();
        let pkg = match format {
            "ograf" => ograf_package(&req, &self.media, &p),
            "banner" => banner_package(&req, &self.media, &p),
            _ => spx_package(&req, &self.media, &p),
        };
        let file = format!("{}-{format}.zip", self.graphic_name(index));
        self.write_next_to_project(&file, &pkg)
    }

    /// Saves the project, with edits, as a new pack next to the original.
    pub fn save_copy(&self) -> Result<PathBuf, String> {
        let sources: Vec<PackSource> = self
            .media
            .pack
            .graphics
            .iter()
            .zip(&self.docs)
            .map(|(g, doc)| PackSource {
                id: &g.id,
                name: &g.name,
                doc,
                thumbnail: g.thumbnail.as_deref(),
            })
            .collect();
        let pkg = write_project_pack(self.name(), &sources, &self.media, &[], None, &now_iso());
        let stem = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        let stem = stem
            .trim_end_matches(".zip")
            .trim_end_matches(".frameproject")
            .trim_end_matches(".framepack");
        self.write_next_to_project(&format!("{stem}-edited.frameproject.zip"), &pkg)
    }

    fn write_next_to_project(&self, file: &str, pkg: &Package) -> Result<PathBuf, String> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        let target = dir.join(file.replace(['/', '\\'], "_"));
        let bytes = pkg.to_zip().map_err(|e| e.to_string())?;
        std::fs::write(&target, bytes)
            .map_err(|e| format!("Could not write {}: {e}", target.display()))?;
        Ok(target)
    }
}

/// Current UTC time as `2026-01-01T12:00:00.000Z`.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opens FRAME's reference packs (when present next to this repository)
    /// and builds a preview of every graphic.
    #[test]
    fn opens_reference_packs_and_builds_previews() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/reference/projects");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("skipped: no reference packs");
            return;
        };
        let mut opened = 0;
        for path in entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with(".zip"))
        {
            let project = Project::open(&path).unwrap();
            assert!(project.warnings.is_empty(), "{:?}", project.warnings);
            for i in 0..project.docs.len() {
                let (pkg, _) = project.preview(i);
                assert!(
                    pkg.missing.is_empty(),
                    "{}: missing {:?}",
                    project.graphic_name(i),
                    pkg.missing
                );
                let mut files = crate::preview::PreviewFiles::default();
                let url = files.publish(pkg);
                let path = url.strip_prefix("framecraft://preview").unwrap();
                assert!(files.lookup(path).is_some(), "{url} not served");
                assert!(
                    files
                        .lookup(&format!(
                            "/{}/js/ionframe-spx-runtime.mjs",
                            files.revision()
                        ))
                        .is_some()
                );
            }
            opened += 1;
        }
        assert!(opened > 0);
    }

    #[test]
    fn iso_time_shape() {
        let t = now_iso();
        assert_eq!(t.len(), 24);
        assert!(t.ends_with(".000Z") && t.as_bytes()[10] == b'T');
        assert!(t.as_str() > "2026-01-01");
    }
}
