// SPDX-License-Identifier: AGPL-3.0-or-later
//! FrameCraft projects on disk: a project folder.
//!
//! ```text
//! Show.framecraft/
//!   project.json            name, graphic order and names, media list
//!   graphics/<id>.json      one graphic each (FRAME's stored form)
//!   media/<category>/<file> images, fonts, videos, animations
//!   .recovery/              autosave copies of project.json and graphics
//! ```
//!
//! Graphic files are named by id, so renaming a graphic touches only
//! `project.json`. Media entries keep the URL graphics use for them (as in
//! FRAME packs), so graphics need no rewriting. Every file is written
//! atomically (temporary file, then rename).

use crate::export::assets::UserFont;
use crate::export::package::{MediaSource, Package};
use crate::model::Graphic;
use crate::normalize::{Document, normalize};
use crate::pack::{PackMediaEntry, PackSource, ProjectPack, write_project_pack};
use crate::profile::Profile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROJECT_FORMAT: &str = "framecraft-project";
pub const PROJECT_VERSION: u32 = 1;
pub const PROJECT_FILE: &str = "project.json";
pub const RECOVERY_DIR: &str = ".recovery";

/// `project.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u32,
    name: String,
    graphics: Vec<GraphicEntry>,
    #[serde(default)]
    media: Vec<PackMediaEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphicEntry {
    id: String,
    name: String,
    /// Path inside the project folder.
    file: String,
}

/// One graphic of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectGraphic {
    pub id: String,
    pub name: String,
    pub doc: Document,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub dir: PathBuf,
    pub name: String,
    pub graphics: Vec<ProjectGraphic>,
    /// Media in `media/`, by the URL graphics use for them.
    pub media: Vec<PackMediaEntry>,
}

#[derive(Debug)]
pub enum ProjectError {
    Io(PathBuf, std::io::Error),
    NotAProject(PathBuf),
    BadFile(PathBuf, String),
    NewerVersion(u32),
    AlreadyExists(PathBuf),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Io(p, e) => write!(f, "{}: {e}", p.display()),
            Self::NotAProject(p) => write!(
                f,
                "{} is not a FrameCraft project (no {PROJECT_FILE})",
                p.display()
            ),
            Self::BadFile(p, e) => write!(f, "{}: {e}", p.display()),
            Self::NewerVersion(v) => write!(f, "made by a newer FrameCraft (project version {v})"),
            Self::AlreadyExists(p) => write!(f, "{} already exists and is not empty", p.display()),
        }
    }
}

impl std::error::Error for ProjectError {}

fn io<T>(path: &Path, r: std::io::Result<T>) -> Result<T, ProjectError> {
    r.map_err(|e| ProjectError::Io(path.to_path_buf(), e))
}

/// Writes `bytes` to `path` through a temporary file, so a crash never
/// leaves a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        io(parent, std::fs::create_dir_all(parent))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    io(&tmp, std::fs::write(&tmp, bytes))?;
    io(path, std::fs::rename(&tmp, path))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ProjectError> {
    let text = io(path, std::fs::read_to_string(path))?;
    serde_json::from_str(&text)
        .map_err(|e| ProjectError::BadFile(path.to_path_buf(), e.to_string()))
}

/// File name for a graphic id: letters, digits, `_` and `-`.
fn graphic_file(id: &str) -> String {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "graphics/{}.json",
        if safe.is_empty() { "graphic" } else { &safe }
    )
}

/// A new id for a graphic (`gfx_` and 16 hex digits).
pub fn new_graphic_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut h = nanos
        ^ COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(std::process::id());
    // SplitMix64 finalizer, so ids made close together look unrelated.
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!("gfx_{:016x}", h ^ (h >> 31))
}

fn graphic_json(doc: &Document) -> Vec<u8> {
    crate::js::stringify_pretty(
        &serde_json::to_value(doc.to_graphic()).expect("graphics serialize"),
    )
    .into_bytes()
}

impl Project {
    fn manifest(&self) -> Manifest {
        Manifest {
            format: PROJECT_FORMAT.into(),
            version: PROJECT_VERSION,
            name: self.name.clone(),
            graphics: self
                .graphics
                .iter()
                .map(|g| GraphicEntry {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    file: graphic_file(&g.id),
                })
                .collect(),
            media: self.media.clone(),
        }
    }

    fn check_empty(dir: &Path) -> Result<(), ProjectError> {
        if dir.exists() && io(dir, std::fs::read_dir(dir))?.next().is_some() {
            return Err(ProjectError::AlreadyExists(dir.to_path_buf()));
        }
        Ok(())
    }

    /// A new, empty project in `dir` (which must not exist or be empty).
    pub fn create(dir: &Path, name: &str) -> Result<Self, ProjectError> {
        Self::check_empty(dir)?;
        let p = Self {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            graphics: Vec::new(),
            media: Vec::new(),
        };
        p.save()?;
        Ok(p)
    }

    /// A project folder made from a FRAME project pack: graphics loaded,
    /// media copied into `media/`.
    pub fn from_pack(
        pack: &ProjectPack,
        dir: &Path,
        profile: &Profile,
    ) -> Result<Self, ProjectError> {
        Self::check_empty(dir)?;
        let mut media = Vec::new();
        for m in &pack.media {
            let rel = m.entry.path.trim_start_matches('/');
            if rel.split('/').any(|part| part == ".." || part.is_empty()) {
                continue;
            }
            write_atomic(&dir.join(rel), &m.bytes)?;
            media.push(PackMediaEntry {
                path: rel.to_string(),
                ..m.entry.clone()
            });
        }
        let mut used = Vec::new();
        let graphics = pack
            .graphics
            .iter()
            .map(|g| {
                // Ids must be unique: they name the files.
                let mut id = if g.id.is_empty() {
                    new_graphic_id()
                } else {
                    g.id.clone()
                };
                while used.contains(&graphic_file(&id)) {
                    id = new_graphic_id();
                }
                used.push(graphic_file(&id));
                ProjectGraphic {
                    id,
                    name: g.name.clone(),
                    doc: normalize(&g.graphic, profile),
                }
            })
            .collect();
        let p = Self {
            dir: dir.to_path_buf(),
            name: pack.name.clone(),
            graphics,
            media,
        };
        p.save()?;
        Ok(p)
    }

    fn load(dir: &Path, graphics_from: &Path, profile: &Profile) -> Result<Self, ProjectError> {
        let manifest_path = graphics_from.join(PROJECT_FILE);
        if !manifest_path.is_file() {
            return Err(ProjectError::NotAProject(dir.to_path_buf()));
        }
        let m: Manifest = read_json(&manifest_path)?;
        if m.format != PROJECT_FORMAT {
            return Err(ProjectError::NotAProject(dir.to_path_buf()));
        }
        if m.version > PROJECT_VERSION {
            return Err(ProjectError::NewerVersion(m.version));
        }
        let mut graphics = Vec::new();
        for g in m.graphics {
            let graphic: Graphic = read_json(&graphics_from.join(&g.file))?;
            graphics.push(ProjectGraphic {
                id: g.id,
                name: g.name,
                doc: normalize(&graphic, profile),
            });
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            name: m.name,
            graphics,
            media: m.media,
        })
    }

    /// Opens a project folder.
    pub fn open(dir: &Path, profile: &Profile) -> Result<Self, ProjectError> {
        Self::load(dir, dir, profile)
    }

    /// Saves the project and clears any recovery copy. Graphic files no
    /// longer in the project are removed.
    pub fn save(&self) -> Result<(), ProjectError> {
        self.write_to(&self.dir)?;
        let graphics_dir = self.dir.join("graphics");
        if graphics_dir.is_dir() {
            let keep: Vec<String> = self.graphics.iter().map(|g| graphic_file(&g.id)).collect();
            for entry in io(&graphics_dir, std::fs::read_dir(&graphics_dir))?.flatten() {
                let rel = format!("graphics/{}", entry.file_name().to_string_lossy());
                if rel.ends_with(".json") && !keep.contains(&rel) {
                    io(&entry.path(), std::fs::remove_file(entry.path()))?;
                }
            }
        }
        Self::discard_recovery(&self.dir)
    }

    fn write_to(&self, base: &Path) -> Result<(), ProjectError> {
        for g in &self.graphics {
            write_atomic(&base.join(graphic_file(&g.id)), &graphic_json(&g.doc))?;
        }
        let manifest = serde_json::to_value(self.manifest()).expect("manifest serializes");
        write_atomic(
            &base.join(PROJECT_FILE),
            crate::js::stringify_pretty(&manifest).as_bytes(),
        )
    }

    /// Writes an autosave copy of the project file and graphics into
    /// `.recovery/` (media are not copied; they do not change by editing).
    pub fn save_recovery(&self) -> Result<(), ProjectError> {
        self.write_to(&self.dir.join(RECOVERY_DIR))
    }

    pub fn has_recovery(dir: &Path) -> bool {
        dir.join(RECOVERY_DIR).join(PROJECT_FILE).is_file()
    }

    /// Opens the autosave copy instead of the saved project.
    pub fn open_recovery(dir: &Path, profile: &Profile) -> Result<Self, ProjectError> {
        Self::load(dir, &dir.join(RECOVERY_DIR), profile)
    }

    pub fn discard_recovery(dir: &Path) -> Result<(), ProjectError> {
        let r = dir.join(RECOVERY_DIR);
        if r.exists() {
            io(&r, std::fs::remove_dir_all(&r))
        } else {
            Ok(())
        }
    }

    /// The project as a FRAME project pack (`.frameproject.zip`).
    pub fn to_pack(
        &self,
        builtin: &dyn MediaSource,
        fonts: &[UserFont],
        exported_at: &str,
    ) -> Package {
        let media = ChainedMedia {
            first: self,
            second: builtin,
        };
        let sources: Vec<PackSource> = self
            .graphics
            .iter()
            .map(|g| PackSource {
                id: &g.id,
                name: &g.name,
                doc: &g.doc,
                thumbnail: None,
            })
            .collect();
        write_project_pack(&self.name, &sources, &media, fonts, None, exported_at)
    }
}

/// FRAME's empty graphic (`createDefaultSceneState`): a 1920 × 1080
/// "Main Scene" with a transparent background and no layers.
pub fn new_graphic(profile: &Profile) -> Document {
    let g: Graphic = serde_json::from_value(serde_json::json!({
        "scene": {
            "id": "scene", "name": "Main Scene", "backgroundColor": "rgba(255, 255, 255, 0)",
            "width": 1920, "height": 1080,
            "spxDescription": "MENiAC FRAME template", "spxPlayserver": "OVERLAY",
            "spxPlaychannel": 1, "spxPlaylayer": 2, "spxWebplayout": 2, "spxOut": "manual",
            "spxDataformat": "json", "spxUicolor": 1, "spxPreventToggle": true,
            "ionboardHookEnabled": false, "ionboardPanelEnabled": false,
            "ionboardMagicId": "", "ionboardProjectId": "",
            "fakeBackgroundEnabled": false, "fakeBackgroundSource": "builtin",
            "fakeBackgroundBuiltinId": "news-room", "fakeBackgroundMediaUrl": "",
            "fakeBackgroundOpacity": 100, "fakeBackgroundBlur": 0
        },
        "items": []
    }))
    .expect("default scene matches the model");
    normalize(&g, profile)
}

/// Saves a project held in memory into the folder `dir`: graphics, the
/// project file and every media file (written when missing or changed).
/// `dir` must be new, empty or already a FrameCraft project.
pub fn save_folder(
    dir: &Path,
    name: &str,
    graphics: Vec<ProjectGraphic>,
    media: &[crate::pack::PackedMedia],
) -> Result<Project, ProjectError> {
    if !dir.join(PROJECT_FILE).is_file() {
        Project::check_empty(dir)?;
    }
    let mut entries = Vec::new();
    for m in media {
        let rel = m.entry.path.trim_start_matches('/');
        if rel.is_empty() || rel.split('/').any(|part| part == ".." || part.is_empty()) {
            continue;
        }
        let target = dir.join(rel);
        let same = std::fs::read(&target).is_ok_and(|b| b == m.bytes);
        if !same {
            write_atomic(&target, &m.bytes)?;
        }
        entries.push(PackMediaEntry {
            path: rel.to_string(),
            ..m.entry.clone()
        });
    }
    let p = Project {
        dir: dir.to_path_buf(),
        name: name.to_string(),
        graphics,
        media: entries,
    };
    p.save()?;
    Ok(p)
}

impl Project {
    /// The project's media files with their bytes (missing files skipped).
    pub fn media_files(&self) -> Vec<crate::pack::PackedMedia> {
        self.media
            .iter()
            .filter(|e| !e.path.split('/').any(|part| part == ".."))
            .filter_map(|e| {
                Some(crate::pack::PackedMedia {
                    entry: e.clone(),
                    bytes: std::fs::read(self.dir.join(&e.path)).ok()?,
                })
            })
            .collect()
    }
}

impl MediaSource for Project {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        let wanted = crate::pack::normalize_pack_url(url);
        let entry = self
            .media
            .iter()
            .find(|m| crate::pack::normalize_pack_url(&m.url) == wanted)?;
        if entry.path.split('/').any(|part| part == "..") {
            return None;
        }
        std::fs::read(self.dir.join(&entry.path)).ok()
    }
}

/// Reads from `first`, then `second`.
pub struct ChainedMedia<'a> {
    pub first: &'a dyn MediaSource,
    pub second: &'a dyn MediaSource,
}

impl MediaSource for ChainedMedia<'_> {
    fn read(&self, url: &str) -> Option<Vec<u8>> {
        self.first.read(url).or_else(|| self.second.read(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the system temp dir, removed on drop.
    pub(crate) struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "framecraft-{tag}-{}-{}",
                std::process::id(),
                new_graphic_id()
            ));
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn graphic() -> Graphic {
        serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": [{
                "id": "a", "name": "A", "itemType": "image", "mediaUrl": "/uploads/u/images/logo.png",
                "x": 0, "y": 0, "width": 10, "height": 10, "rotation": 0, "opacity": 1, "bg_color": "#fff",
                "borderWidth": 0, "borderColor": "#000", "borderRadius": [0, 0, 0, 0],
                "shadow": { "color": "#000", "x": 0, "y": 0, "blur": 0 }
            }]
        }))
        .unwrap()
    }

    #[test]
    fn ids_are_unique_and_safe_file_names() {
        let a = new_graphic_id();
        assert_ne!(a, new_graphic_id());
        assert_eq!(graphic_file("gfx_1.2/x"), "graphics/gfx_1_2_x.json");
    }

    #[test]
    fn save_open_recover_and_remove() {
        let tmp = TempDir::new("project");
        let p = Profile::framecraft();
        let mut project = Project::create(&tmp.0, "Show").unwrap();
        assert!(matches!(
            Project::create(&tmp.0, "Again"),
            Err(ProjectError::AlreadyExists(_))
        ));

        project.graphics.push(ProjectGraphic {
            id: "g1".into(),
            name: "Lower third".into(),
            doc: normalize(&graphic(), &p),
        });
        project.graphics.push(ProjectGraphic {
            id: "g2".into(),
            name: "Bug".into(),
            doc: normalize(&graphic(), &p),
        });
        project.media.push(PackMediaEntry {
            url: "/uploads/u/images/logo.png".into(),
            path: "media/images/logo.png".into(),
            category: "images".into(),
            original_name: Some("logo.png".into()),
            font_family: None,
        });
        write_atomic(&tmp.0.join("media/images/logo.png"), b"png").unwrap();
        project.save().unwrap();
        assert_eq!(Project::open(&tmp.0, &p).unwrap(), project);
        assert_eq!(
            project.read("/uploads/u/images/logo.png").as_deref(),
            Some(&b"png"[..])
        );

        // Autosave copy: the saved project is untouched until the next save.
        let mut edited = project.clone();
        edited.graphics[0].name = "Renamed".into();
        edited.save_recovery().unwrap();
        assert!(Project::has_recovery(&tmp.0));
        assert_eq!(
            Project::open(&tmp.0, &p).unwrap().graphics[0].name,
            "Lower third"
        );
        assert_eq!(
            Project::open_recovery(&tmp.0, &p).unwrap().graphics[0].name,
            "Renamed"
        );

        // Saving clears the recovery copy and removes dropped graphic files.
        edited.graphics.remove(1);
        edited.save().unwrap();
        assert!(!Project::has_recovery(&tmp.0));
        assert!(!tmp.0.join("graphics/g2.json").exists());
        assert_eq!(Project::open(&tmp.0, &p).unwrap(), edited);
    }

    #[test]
    fn rejects_other_folders() {
        let tmp = TempDir::new("not-a-project");
        std::fs::create_dir_all(&tmp.0).unwrap();
        assert!(matches!(
            Project::open(&tmp.0, &Profile::framecraft()),
            Err(ProjectError::NotAProject(_))
        ));
        write_atomic(
            &tmp.0.join(PROJECT_FILE),
            br#"{"format":"framecraft-project","version":99,"name":"x","graphics":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            Project::open(&tmp.0, &Profile::framecraft()),
            Err(ProjectError::NewerVersion(99))
        ));
    }

    #[test]
    fn save_folder_writes_media_and_reopens() {
        let tmp = TempDir::new("savefolder");
        let dir = tmp.0.join("Show.framecraft");
        let doc = new_graphic(&Profile::framecraft());
        assert_eq!(
            (doc.scene.width, doc.scene.name.as_str()),
            (1920.0, "Main Scene")
        );
        let media = vec![crate::pack::PackedMedia {
            entry: PackMediaEntry {
                url: "/media/images/a.png".into(),
                path: "media/images/a.png".into(),
                category: "images".into(),
                original_name: Some("a.png".into()),
                font_family: None,
            },
            bytes: vec![1, 2, 3],
        }];
        let g = vec![ProjectGraphic {
            id: "gfx_1".into(),
            name: "Main".into(),
            doc,
        }];
        save_folder(&dir, "Show", g.clone(), &media).unwrap();
        // Saving again over the same project is fine.
        save_folder(&dir, "Show", g, &media).unwrap();
        let back = Project::open(&dir, &Profile::framecraft()).unwrap();
        assert_eq!(back.name, "Show");
        assert_eq!(back.media_files()[0].bytes, vec![1, 2, 3]);
        // A folder with other things in it is refused.
        let other = tmp.0.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("x.txt"), "x").unwrap();
        assert!(save_folder(&other, "X", Vec::new(), &[]).is_err());
    }
}
