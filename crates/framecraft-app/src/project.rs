// SPDX-License-Identifier: AGPL-3.0-or-later
//! An open project: the pack it came from, its graphics loaded for editing,
//! and the media they use.

use framecraft_core::commands::Change;
use framecraft_core::export::package::{
    ExportRequest, MediaSource, Package, banner_package, ograf_package, spx_package,
};
use framecraft_core::normalize::{Document, normalize};
use framecraft_core::pack::{
    Pack, PackMediaEntry, PackSource, ProjectPack, read_pack, write_project_pack,
};
use framecraft_core::preview::{self, PreviewState, PreviewUpdate};
use framecraft_core::profile::Profile;
use std::path::{Path, PathBuf};

/// Media from the open pack, then FrameCraft's built-in files (fonts,
/// particle sprites, stock animations).
pub struct ProjectMedia {
    pub pack: ProjectPack,
}

impl MediaSource for ProjectMedia {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = self.pack.read(url) {
            return Some(bytes);
        }
        let rel = url.split(['?', '#']).next()?.strip_prefix('/')?;
        builtin_read(rel)
    }
}

pub struct Project {
    /// Where it was opened from: a FRAME pack or a project folder.
    pub path: PathBuf,
    /// The FrameCraft project folder it saves to (`None` until saved as one).
    pub folder: Option<PathBuf>,
    pub media: ProjectMedia,
    /// One per graphic in the pack, in pack order.
    pub docs: Vec<Document>,
    /// Problems found while opening (missing files and the like).
    pub warnings: Vec<String>,
    /// The active palette's colors (chart colors in previews and exports).
    pub palette_colors: Vec<String>,
}

/// FrameCraft's data folder (user palettes and libraries):
/// `~/.local/share/framecraft` on Linux (or `$XDG_DATA_HOME/framecraft`),
/// `%APPDATA%\FrameCraft` on Windows, `~/Library/Application Support/FrameCraft`
/// on macOS.
pub fn data_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        return env("APPDATA").map(|d| d.join("FrameCraft"));
    }
    if cfg!(target_os = "macos") {
        return env("HOME").map(|h| h.join("Library/Application Support/FrameCraft"));
    }
    env("XDG_DATA_HOME")
        .or_else(|| env("HOME").map(|h| h.join(".local/share")))
        .map(|d| d.join("framecraft"))
}

/// Where the user's palettes are saved.
pub fn palettes_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("palettes.json"))
}

fn builtin_text(rel: &str) -> String {
    builtin_read(rel)
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

/// FRAME's built-in particle sprites (`particles/Particle-{64|128}-NN.png`):
/// `(url, label)`, 64 px first.
pub fn builtin_sprites() -> Vec<(String, String)> {
    let Some(dir) = builtin_dirs()
        .into_iter()
        .find(|d| d.join("particles").is_dir())
    else {
        return Vec::new();
    };
    let mut files: Vec<String> = std::fs::read_dir(dir.join("particles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("Particle-") && n.ends_with(".png"))
        .collect();
    files.sort_by_key(|n| {
        let mut parts = n.trim_end_matches(".png").split('-').skip(1);
        let size: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let index: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (size, index)
    });
    files
        .into_iter()
        .map(|n| {
            let mut parts = n.trim_end_matches(".png").split('-').skip(1);
            let label = format!(
                "{} · {}",
                parts.next().unwrap_or(""),
                parts
                    .next()
                    .and_then(|p| p.parse::<u32>().ok())
                    .unwrap_or(0)
            );
            (format!("/particles/{n}"), label)
        })
        .collect()
}

/// FRAME's stock palettes (read once).
pub fn stock_palettes() -> &'static [framecraft_core::palettes::Palette] {
    static CACHE: std::sync::OnceLock<Vec<framecraft_core::palettes::Palette>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        framecraft_core::palettes::parse_stock_palettes(&builtin_text(
            "palettes/stock-palettes.json",
        ))
    })
}

/// FRAME's stock gradients (read once).
pub fn stock_gradients() -> &'static [framecraft_core::palettes::StockGradient] {
    static CACHE: std::sync::OnceLock<Vec<framecraft_core::palettes::StockGradient>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        framecraft_core::palettes::parse_stock_gradients(&builtin_text(
            "gradients/stock-gradients.json",
        ))
    })
}

/// Folders with built-in files, searched in order: FrameCraft's own
/// `assets/` (next to the program when installed, or in the repository),
/// then `FRAMECRAFT_BUILTIN_DIR`, then a FRAME checkout next to this
/// repository (particle sprites, plates and Noto emoji until FrameCraft
/// ships them).
pub fn builtin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("assets"));
        dirs.push(exe_dir.join("../share/framecraft/assets"));
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    if let Some(dir) = std::env::var_os("FRAMECRAFT_BUILTIN_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../FRAME/static"));
    dirs.retain(|d| d.is_dir());
    dirs
}

/// A built-in file by its path (`fonts/Inter-….ttf`), from the first
/// folder that has it.
pub fn builtin_read(rel: &str) -> Option<Vec<u8>> {
    if rel.split('/').any(|p| p == ".." || p.is_empty()) {
        return None;
    }
    builtin_dirs()
        .iter()
        .find_map(|d| std::fs::read(d.join(rel)).ok())
}

/// A built-in Lottie animation (Google Noto Animated Emoji).
#[derive(Debug, Clone, PartialEq)]
pub struct StockLottie {
    pub name: String,
    pub url: String,
    pub thumb: Option<String>,
}

/// The stock Lottie list (`lottie/noto-emoji/index.json` in the built-in
/// files), as FRAME's Lottie library shows it.
pub fn stock_lottie() -> Vec<StockLottie> {
    let Some(text) =
        builtin_read("lottie/noto-emoji/index.json").and_then(|b| String::from_utf8(b).ok())
    else {
        return Vec::new();
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            let file = r["file"].as_str()?.trim();
            let thumb = r["thumb"].as_str().map(str::trim).filter(|t| !t.is_empty());
            Some(StockLottie {
                name: r["name"].as_str().unwrap_or(file).to_string(),
                url: format!("/lottie/noto-emoji/{file}"),
                thumb: thumb.map(|t| {
                    if t.starts_with('/') {
                        t.to_string()
                    } else {
                        format!("/lottie/noto-emoji/{t}")
                    }
                }),
            })
        })
        .collect()
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
            folder: None,
            media: ProjectMedia { pack },
            docs,
            warnings,
            palette_colors: Vec::new(),
        })
    }

    /// Opens a pack, a project folder, or a folder's `project.json`.
    pub fn open_any(path: &Path) -> Result<Self, String> {
        match project_dir(path) {
            Some(dir) => Self::open_folder(&dir, false),
            None => Self::open(path),
        }
    }

    /// Opens a FrameCraft project folder (or its autosave copy).
    pub fn open_folder(dir: &Path, recovery: bool) -> Result<Self, String> {
        use framecraft_core::project::Project as Folder;
        let profile = Profile::framecraft();
        let p = if recovery {
            Folder::open_recovery(dir, &profile)
        } else {
            Folder::open(dir, &profile)
        }
        .map_err(|e| e.to_string())?;
        // Media always come from the saved folder (the autosave has none).
        let media = Folder::open(dir, &profile)
            .map(|saved| saved.media_files())
            .unwrap_or_else(|_| p.media_files());
        let pack = ProjectPack {
            name: p.name.clone(),
            exported_at: String::new(),
            graphics: p
                .graphics
                .iter()
                .map(|g| framecraft_core::pack::PackedGraphic {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    graphic: g.doc.to_graphic(),
                    thumbnail: std::fs::read(
                        dir.join(THUMBS_DIR)
                            .join(format!("{}.jpg", thumb_name(&g.id))),
                    )
                    .ok(),
                })
                .collect(),
            media,
            thumbnail: None,
        };
        Ok(Self {
            path: dir.to_path_buf(),
            folder: Some(dir.to_path_buf()),
            media: ProjectMedia { pack },
            docs: p.graphics.into_iter().map(|g| g.doc).collect(),
            warnings: Vec::new(),
            palette_colors: Vec::new(),
        })
    }

    /// A new project (not saved yet) with one empty graphic, as FRAME's
    /// "Empty" template.
    pub fn new_empty(name: &str) -> Self {
        let doc = framecraft_core::project::new_graphic(&Profile::framecraft());
        let pack = ProjectPack {
            name: name.to_string(),
            exported_at: String::new(),
            graphics: vec![framecraft_core::pack::PackedGraphic {
                id: framecraft_core::project::new_graphic_id(),
                name: "Main".into(),
                graphic: doc.to_graphic(),
                thumbnail: None,
            }],
            media: Vec::new(),
            thumbnail: None,
        };
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        Self {
            path: home.join(format!("{name}.framecraft")),
            folder: None,
            media: ProjectMedia { pack },
            docs: vec![doc],
            warnings: Vec::new(),
            palette_colors: Vec::new(),
        }
    }

    fn folder_graphics(&self) -> Vec<framecraft_core::project::ProjectGraphic> {
        self.media
            .pack
            .graphics
            .iter()
            .zip(&self.docs)
            .map(|(g, doc)| framecraft_core::project::ProjectGraphic {
                id: g.id.clone(),
                name: g.name.clone(),
                doc: doc.clone(),
            })
            .collect()
    }

    /// Saves as a FrameCraft project folder (`dir`), which becomes where
    /// this project saves. Clears the autosave copy.
    pub fn save_to(&mut self, dir: &Path) -> Result<(), String> {
        framecraft_core::project::save_folder(
            dir,
            self.name(),
            self.folder_graphics(),
            &self.media.pack.media,
        )
        .map_err(|e| e.to_string())?;
        // Thumbnails: thumbs/<graphic id>.jpg (kept out of the core format).
        let thumbs = dir.join(THUMBS_DIR);
        for g in &self.media.pack.graphics {
            if let Some(t) = &g.thumbnail {
                let path = thumbs.join(format!("{}.jpg", thumb_name(&g.id)));
                if std::fs::read(&path).ok().as_deref() != Some(t.as_slice()) {
                    std::fs::create_dir_all(&thumbs)
                        .and_then(|()| std::fs::write(&path, t))
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                }
            }
        }
        self.folder = Some(dir.to_path_buf());
        self.path = dir.to_path_buf();
        Ok(())
    }

    /// Writes the autosave copy (`.recovery/` in the project folder).
    pub fn save_recovery(&self) -> Result<(), String> {
        let Some(dir) = &self.folder else {
            return Ok(());
        };
        framecraft_core::project::Project {
            dir: dir.clone(),
            name: self.name().to_string(),
            graphics: self.folder_graphics(),
            media: Vec::new(),
        }
        .save_recovery()
        .map_err(|e| e.to_string())
    }

    pub fn name(&self) -> &str {
        &self.media.pack.name
    }

    /// Copies a file from disk into the project's media.
    pub fn import_media(&mut self, path: &Path) -> Result<PackMediaEntry, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        framecraft_core::media::import(&mut self.media.pack, name, bytes)
    }

    /// Every media file of the project, by URL (for the library's thumbnails).
    pub fn media_files(&self) -> Vec<(String, Vec<u8>)> {
        self.media
            .pack
            .media
            .iter()
            .map(|m| (m.entry.url.clone(), m.bytes.clone()))
            .collect()
    }

    pub fn graphic_name(&self, index: usize) -> &str {
        &self.media.pack.graphics[index].name
    }

    fn request<'a>(&'a self, index: usize, exported_at: &'a str) -> ExportRequest<'a> {
        ExportRequest {
            doc: &self.docs[index],
            graphic_name: self.graphic_name(index),
            palette_colors: &self.palette_colors,
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

    /// One graphic's export package: `ograf`, `banner` or `spx`.
    pub fn package(&self, index: usize, format: &str) -> Package {
        let now = now_iso();
        let req = self.request(index, &now);
        let p = Profile::framecraft();
        match format {
            "ograf" => ograf_package(&req, &self.media, &p),
            "banner" => banner_package(&req, &self.media, &p),
            _ => spx_package(&req, &self.media, &p),
        }
    }

    /// Every graphic in one package: an OGraf or SPX suite.
    pub fn suite(&self, ograf: bool) -> Package {
        use framecraft_core::export::package::{SuiteGraphic, SuiteRequest, suite_package};
        let now = now_iso();
        let graphics: Vec<SuiteGraphic> = self
            .media
            .pack
            .graphics
            .iter()
            .zip(&self.docs)
            .map(|(g, doc)| SuiteGraphic {
                id: &g.id,
                name: &g.name,
                doc,
            })
            .collect();
        let req = SuiteRequest {
            project_name: self.name(),
            graphics: &graphics,
            palette_colors: &self.palette_colors,
            user_fonts: &[],
            exported_at: &now,
        };
        suite_package(&req, ograf, &self.media, &Profile::framecraft())
    }

    /// The folder the project file is in (where exports start).
    pub fn folder(&self) -> PathBuf {
        self.path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// Whether the project folder has an autosave copy.
    pub fn has_recovery(&self) -> bool {
        self.folder
            .as_deref()
            .is_some_and(framecraft_core::project::Project::has_recovery)
    }

    /// Adds an empty graphic (FRAME's + New Graphic) at the end; returns
    /// its index.
    pub fn add_graphic(&mut self) -> usize {
        let doc = framecraft_core::project::new_graphic(&Profile::framecraft());
        self.media
            .pack
            .graphics
            .push(framecraft_core::pack::PackedGraphic {
                id: framecraft_core::project::new_graphic_id(),
                name: "Untitled Graphic".into(),
                graphic: doc.to_graphic(),
                thumbnail: None,
            });
        self.docs.push(doc);
        self.docs.len() - 1
    }

    /// A copy of graphic `i` after it, named as FRAME does ("… Copy", or
    /// "… copy 2"); returns the copy's index.
    pub fn clone_graphic(&mut self, i: usize) -> Option<usize> {
        let src = self.media.pack.graphics.get(i)?.clone();
        let base = match src.name.trim() {
            "" => "Graphic",
            n => n,
        };
        let name = if base.to_lowercase().ends_with(" copy") {
            format!("{base} 2")
        } else {
            format!("{base} Copy")
        };
        let doc = self.docs[i].clone();
        self.media.pack.graphics.insert(
            i + 1,
            framecraft_core::pack::PackedGraphic {
                id: framecraft_core::project::new_graphic_id(),
                name,
                graphic: doc.to_graphic(),
                thumbnail: src.thumbnail,
            },
        );
        self.docs.insert(i + 1, doc);
        Some(i + 1)
    }

    pub fn rename_graphic(&mut self, i: usize, name: &str) {
        if let Some(g) = self.media.pack.graphics.get_mut(i) {
            let n = name.trim();
            g.name = if n.is_empty() {
                "Untitled Graphic".into()
            } else {
                n.to_string()
            };
        }
    }

    /// Removes graphic `i` (a project keeps at least one).
    pub fn delete_graphic(&mut self, i: usize) -> bool {
        if self.docs.len() <= 1 || i >= self.docs.len() {
            return false;
        }
        self.media.pack.graphics.remove(i);
        self.docs.remove(i);
        true
    }

    /// Moves graphic `from` to place `to`.
    pub fn move_graphic(&mut self, from: usize, to: usize) -> bool {
        if from >= self.docs.len() || to >= self.docs.len() || from == to {
            return false;
        }
        let g = self.media.pack.graphics.remove(from);
        self.media.pack.graphics.insert(to, g);
        let d = self.docs.remove(from);
        self.docs.insert(to, d);
        true
    }

    /// Adds the graphic of a FRAME graphic pack (`.framepack.zip`), with its
    /// media; returns its index and any warnings (FRAME's Import Pack).
    pub fn import_graphic_pack(&mut self, path: &Path) -> Result<(usize, Vec<String>), String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let (pack, warnings) = read_pack(&bytes).map_err(|e| e.to_string())?;
        let Pack::Graphic(g) = pack else {
            return Err("That is a project pack; open it with Open… instead.".into());
        };
        for m in g.media {
            let known = self
                .media
                .pack
                .media
                .iter()
                .any(|x| x.entry.url == m.entry.url);
            if !known {
                self.media.pack.media.push(m);
            }
        }
        let mut graphic = g.graphic;
        graphic.id = framecraft_core::project::new_graphic_id();
        let doc = normalize(&graphic.graphic, &Profile::framecraft());
        self.media.pack.graphics.push(graphic);
        self.docs.push(doc);
        Ok((self.docs.len() - 1, warnings))
    }

    /// Graphic `i` as a FRAME graphic pack (`.framepack.zip` bytes).
    pub fn export_graphic_pack(&self, i: usize) -> Result<Vec<u8>, String> {
        let g = self.media.pack.graphics.get(i).ok_or("No such graphic")?;
        framecraft_core::pack::write_graphic_pack(
            &g.name,
            &self.docs[i],
            &self.media,
            &[],
            g.thumbnail.as_deref(),
            &now_iso(),
        )
        .to_zip()
        .map_err(|e| e.to_string())
    }

    /// The project as a FRAME project pack (`.frameproject.zip` bytes).
    pub fn export_pack(&self) -> Result<Vec<u8>, String> {
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
        pkg.to_zip().map_err(|e| e.to_string())
    }
}

/// Graphic thumbnails in a project folder.
const THUMBS_DIR: &str = "thumbs";

/// A graphic id as a file name (letters, digits, `_`, `-`).
fn thumb_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The project folder `path` points at: the folder itself or its
/// `project.json`; `None` for anything else (a pack).
pub fn project_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return Some(path.to_path_buf());
    }
    (path.file_name()? == framecraft_core::project::PROJECT_FILE)
        .then(|| path.parent().map(Path::to_path_buf))
        .flatten()
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

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn autosave_copy_is_found_and_restored() {
        let base = std::env::temp_dir().join(format!(
            "fc-app-recovery-{}",
            framecraft_core::project::new_graphic_id()
        ));
        let dir = base.join("Show.framecraft");
        let mut p = Project::new_empty("Show");
        p.save_to(&dir).unwrap();
        assert!(!p.has_recovery());
        // An edit after saving, then the autosave.
        p.docs[0].scene.width = 1280.0;
        p.save_recovery().unwrap();
        assert!(p.has_recovery());
        let opened = Project::open_any(&dir.join("project.json")).unwrap();
        assert!(opened.has_recovery());
        assert_eq!(opened.docs[0].scene.width, 1920.0);
        let restored = Project::open_folder(&dir, true).unwrap();
        assert_eq!(restored.docs[0].scene.width, 1280.0);
        // Thumbnails are kept in the folder.
        p.media.pack.graphics[0].thumbnail = Some(vec![0xff, 0xd8, 1]);
        p.save_to(&dir).unwrap();
        let again = Project::open_folder(&dir, false).unwrap();
        assert_eq!(
            again.media.pack.graphics[0].thumbnail.as_deref(),
            Some(&[0xff, 0xd8, 1][..])
        );
        // Saving clears the autosave.
        let mut r = restored;
        r.save_to(&dir).unwrap();
        assert!(!r.has_recovery());
        let _ = std::fs::remove_dir_all(base);
    }
}

#[cfg(test)]
mod new_project_tests {
    use super::*;

    #[test]
    fn new_project_builds_a_preview() {
        let p = Project::new_empty("Untitled");
        let (pkg, _) = p.preview(0);
        let html = pkg
            .files
            .iter()
            .find(|(path, _)| path.ends_with(".html") && !path.contains('/'))
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .expect("an entry page");
        assert!(
            html.contains("framecraft-editor.js"),
            "editor script missing"
        );
        assert!(
            pkg.files
                .iter()
                .any(|(p, _)| p == "js/ionframe-spx-runtime.mjs")
        );
        eprintln!(
            "entry files: {:?}",
            pkg.files
                .iter()
                .map(|(p, _)| p.as_str())
                .filter(|p| p.ends_with(".html"))
                .collect::<Vec<_>>()
        );
    }
}
