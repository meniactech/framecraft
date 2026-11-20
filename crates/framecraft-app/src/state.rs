// SPDX-License-Identifier: AGPL-3.0-or-later
//! Editor state and the one path every edit takes:
//! command → document → undo history → preview update.

use crate::bridge;
use crate::interact::{Drag, Geom};
use crate::preview::STORE;
use crate::project::Project;
use crate::timeline::TlView;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use framecraft_core::commands::{AddItem, Change, Command, DeleteItems, DuplicateItems, History};
use framecraft_core::keys::{CopiedKey, KeyRef};
use framecraft_core::layers::{NewLayer, SvgMode, new_layer};
use framecraft_core::normalize::Document;
use framecraft_core::preview::{PreviewState, PreviewUpdate};
use framecraft_core::schema::Tab;
use std::path::PathBuf;
use std::time::Duration;

/// The open project and what the UI edits in it.
pub struct Editor {
    pub project: Project,
    pub graphic: usize,
    /// The primary selected layer (properties and timeline show it).
    pub selected: Option<String>,
    /// Other selected layers (multi-select), primary not included.
    pub also: Vec<String>,
    /// One undo history per graphic.
    histories: Vec<History>,
    /// What the loaded preview was built from.
    preview: Option<PreviewState>,
}

impl Editor {
    pub fn doc(&self) -> &Document {
        &self.project.docs[self.graphic]
    }

    /// Every selected layer, the primary first.
    pub fn selection(&self) -> Vec<String> {
        self.selected
            .iter()
            .chain(self.also.iter())
            .cloned()
            .collect()
    }

    pub fn history(&self) -> &History {
        &self.histories[self.graphic]
    }
}

/// What replaces the open project (asked about when it has unsaved changes).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Switch {
    New,
    Open,
}

/// Shared UI state, handed to components as one copyable value.
#[derive(Clone, Copy, PartialEq)]
pub struct Ui {
    pub editor: Signal<Option<Editor>>,
    pub time: Signal<f64>,
    pub playing: Signal<bool>,
    pub zoom: Signal<f64>,
    /// Viewport pan: the stage's offset (screen px) from the centre.
    pub pan: Signal<(f64, f64)>,
    /// Viewport guides shown, and snapping to them.
    pub guides: Signal<framecraft_core::manipulate::GuideToggles>,
    /// Timeline Auto-key: every field edit writes a keyframe at the playhead.
    pub auto_key: Signal<bool>,
    /// Where the selected layers are drawn, as measured in the page.
    pub geometry: Signal<Vec<Geom>>,
    /// The viewport drag in progress (read with `peek`: it changes every
    /// pointer move and nothing renders from it).
    pub drag: Signal<Option<Drag>>,
    pub status: Signal<String>,
    /// The properties tab last picked.
    pub tab: Signal<Tab>,
    /// Selected keyframes (timeline).
    pub keys: Signal<Vec<KeyRef>>,
    /// Selected scene cue (timeline).
    pub cue: Signal<Option<String>>,
    pub tl_view: Signal<TlView>,
    /// Timeline scale, pixels per millisecond.
    /// Timeline zoom: 1 = the whole duration fits (FRAME), up to 64.
    pub tl_zoom: Signal<f64>,
    /// Width (px) the track area has, measured by the bridge.
    pub tl_fit: Signal<f64>,
    /// Keyframes copied with Ctrl+C.
    pub copied: Signal<Vec<CopiedKey>>,
    /// The open media library dialog, if any.
    pub media: Signal<Option<MediaPick>>,
    /// The user's palettes (saved in the data folder).
    pub palettes: Signal<framecraft_core::palettes::UserPalettes>,
    /// The Project view (graphics and Media Manager) covers the editor.
    pub project_view: Signal<bool>,
    /// The motion library: `Some("apply")` or `Some("save")` when open.
    pub motion_dialog: Signal<Option<&'static str>>,
    /// The user's saved motions (data folder).
    pub user_motions: Signal<framecraft_core::motion::UserMotions>,
    /// Edits not saved yet.
    pub dirty: Signal<bool>,
    /// The tooltip of what the pointer is over (status bar).
    pub hint: Signal<String>,
    /// A gradient editor action from the bridge (add / select / move stop).
    pub ramp_event: Signal<Option<serde_json::Value>>,
    /// A project folder whose autosave copy waits for Restore / Discard.
    pub recovery: Signal<Option<PathBuf>>,
    /// The user's components (data folder).
    pub user_components: Signal<framecraft_core::components::UserComponents>,
    /// Make Component waiting for its name.
    pub pending_component: Signal<Option<crate::components::PendingComponent>>,
    /// How times are shown and typed (storage stays ms).
    pub time_format: Signal<framecraft_core::timecode::Format>,
    /// New / Open waiting for an answer about unsaved changes.
    pub switch_prompt: Signal<Option<Switch>>,
    /// Add → Insert Component… is open.
    pub component_library: Signal<bool>,
    /// The script editor is open.
    pub script_editor: Signal<bool>,
}

/// An open media library: what it lists and where the pick goes.
#[derive(Clone, PartialEq)]
pub struct MediaPick {
    /// `images`, `videos`, `lottie`, or `shapes` (the built-in SVG shapes).
    pub category: &'static str,
    pub target: PickTarget,
}

#[derive(Clone, PartialEq)]
pub enum PickTarget {
    /// A new layer showing the file (Add → Image, Lottie, Video).
    NewLayer,
    /// A media field of a layer (or of one of its modifiers).
    Field {
        prop: &'static framecraft_core::schema::Property,
        item: String,
        modifier: Option<String>,
    },
}

/// `FRAMECRAFT_DEBUG=1` logs preview requests and bridge activity.
pub fn debug() -> bool {
    std::env::var_os("FRAMECRAFT_DEBUG").is_some()
}

impl Ui {
    pub fn new() -> Self {
        Self {
            editor: use_signal(|| None),
            time: use_signal(|| 0.0),
            playing: use_signal(|| false),
            zoom: use_signal(|| 0.5),
            pan: use_signal(|| (0.0, 0.0)),
            guides: use_signal(crate::layout::load_guides),
            time_format: use_signal(crate::layout::load_time_format),
            auto_key: use_signal(|| false),
            geometry: use_signal(Vec::new),
            drag: use_signal(|| None),
            status: use_signal(|| {
                "Open a project or a FRAME pack (Open…), or start one (New…)".to_string()
            }),
            tab: use_signal(|| Tab::Transform),
            keys: use_signal(Vec::new),
            cue: use_signal(|| None),
            tl_view: use_signal(|| TlView::Keys),
            tl_zoom: use_signal(|| 1.0),
            tl_fit: use_signal(|| 800.0),
            copied: use_signal(Vec::new),
            media: use_signal(|| None),
            palettes: use_signal(|| {
                crate::project::palettes_path()
                    .map(|p| framecraft_core::palettes::UserPalettes::load(&p))
                    .unwrap_or_default()
            }),
            project_view: use_signal(|| false),
            motion_dialog: use_signal(|| None),
            user_components: use_signal(|| {
                crate::components::components_path()
                    .map(|p| framecraft_core::components::UserComponents::load(&p))
                    .unwrap_or_default()
            }),
            pending_component: use_signal(|| None),
            switch_prompt: use_signal(|| None),
            dirty: use_signal(|| false),
            hint: use_signal(String::new),
            ramp_event: use_signal(|| None),
            recovery: use_signal(|| None),
            component_library: use_signal(|| false),
            script_editor: use_signal(|| false),
            user_motions: use_signal(|| {
                crate::motions::motions_path()
                    .map(|p| framecraft_core::motion::UserMotions::load(&p))
                    .unwrap_or_default()
            }),
        }
    }

    /// Replaces the user's saved motions and saves them.
    pub fn set_user_motions(mut self, next: framecraft_core::motion::UserMotions) {
        if let Some(path) = crate::motions::motions_path()
            && let Err(e) = next.save(&path)
        {
            self.set_status(format!("Could not save motions: {e}"));
        }
        self.user_motions.set(next);
    }

    /// Replaces the user's palettes: saves them, and when the active colors
    /// change, rebuilds the preview (charts use them).
    pub fn set_palettes(mut self, next: framecraft_core::palettes::UserPalettes) {
        let before = self.palettes.peek().active_css();
        if let Some(path) = crate::project::palettes_path()
            && let Err(e) = next.save(&path)
        {
            self.set_status(format!("Could not save palettes: {e}"));
        }
        let colors = next.active_css();
        self.palettes.set(next);
        if colors != before {
            let has = self.editor.peek().is_some();
            if let Some(e) = self.editor.write().as_mut() {
                e.project.palette_colors = colors;
            }
            if has {
                self.rebuild();
            }
        }
    }

    fn set_status(mut self, text: impl Into<String>) {
        self.status.set(text.into());
    }

    pub fn duration(&self) -> f64 {
        self.editor
            .read()
            .as_ref()
            .map_or(10_000.0, |e| e.doc().timeline.duration.max(1.0))
    }

    /// Opens a FRAME pack or a FrameCraft project folder (or its
    /// `project.json`). A folder with an autosave copy asks to restore it.
    pub fn open(mut self, path: PathBuf) {
        // Absolute, so save and export dialogs can start next to it (a
        // relative path from the command line made them start elsewhere).
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        match Project::open_any(&path) {
            Ok(project) => {
                let recovery = project.has_recovery().then(|| project.path.clone());
                self.install(project, "Opened");
                if let Some(dir) = recovery {
                    self.recovery.set(Some(dir));
                }
            }
            Err(e) => self.set_status(e),
        }
    }

    /// A new, unsaved project with one empty graphic.
    /// New: asks where to create the project folder first, then creates
    /// and saves it there, so autosave and recovery work from the start
    /// and imported media go straight into its `media/` folder. The
    /// project is named after the folder.
    /// New, after asking about unsaved changes.
    pub fn new_project(self) {
        self.guard(Switch::New);
    }

    /// Open…, after asking about unsaved changes.
    pub fn open_dialog(self) {
        self.guard(Switch::Open);
    }

    /// Replacing the open project with unsaved changes asks first.
    fn guard(self, s: Switch) {
        if *self.dirty.peek() && self.editor.peek().is_some() {
            let mut prompt = self.switch_prompt;
            prompt.set(Some(s));
        } else {
            self.continue_switch(s);
        }
    }

    /// Goes on with New / Open (the answer was given, or nothing to lose).
    pub fn continue_switch(self, s: Switch) {
        match s {
            Switch::New => self.new_project_now(),
            Switch::Open => self.open_dialog_now(),
        }
    }

    fn new_project_now(self) {
        let start = self
            .editor
            .peek()
            .as_ref()
            .and_then(|e| e.project.folder.clone())
            .and_then(|f| f.parent().map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        spawn_forever(async move {
            let ui = self;
            let picked = rfd::AsyncFileDialog::new()
                .set_title("New FrameCraft project: choose a name and where to create it")
                .set_directory(&start)
                .set_file_name("Untitled.framecraft")
                .save_file()
                .await;
            let Some(h) = picked else {
                ui.set_status("New project cancelled".to_string());
                return;
            };
            let mut dir = h.path().to_path_buf();
            if dir.extension().is_none() {
                dir.set_extension("framecraft");
            }
            let has_files =
                dir.is_dir() && std::fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some());
            if has_files {
                ui.set_status(format!(
                    "{} already exists and has files in it; choose another name",
                    dir.display()
                ));
                return;
            }
            let name = dir
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Untitled")
                .to_string();
            ui.install(Project::new_empty(&name), "New project");
            ui.save_into(dir);
        });
    }

    /// Restores (`true`) or discards the autosave copy found when opening.
    pub fn resolve_recovery(mut self, restore: bool) {
        let Some(dir) = self.recovery.peek().clone() else {
            return;
        };
        self.recovery.set(None);
        if restore {
            match Project::open_folder(&dir, true) {
                Ok(project) => {
                    self.install(project, "Restored unsaved changes of");
                    let mut dirty = self.dirty;
                    dirty.set(true);
                }
                Err(e) => self.set_status(format!("Could not restore: {e}")),
            }
        } else if let Err(e) = framecraft_core::project::Project::discard_recovery(&dir) {
            self.set_status(format!("Could not discard the autosave: {e}"));
        }
    }

    fn install(mut self, mut project: Project, verb: &str) {
        project.palette_colors = self.palettes.peek().active_css();
        let warnings = project.warnings.len();
        self.set_status(format!(
            "{verb} {} ({} graphics){}",
            project.name(),
            project.docs.len(),
            if warnings > 0 {
                format!(", {warnings} warnings")
            } else {
                String::new()
            }
        ));
        let histories = project.docs.iter().map(|_| History::default()).collect();
        let files = project.media_files();
        STORE.with(|s| {
            s.borrow_mut()
                .set_library(files, crate::project::builtin_dirs())
        });
        self.editor.set(Some(Editor {
            project,
            graphic: 0,
            selected: None,
            also: Vec::new(),
            histories,
            preview: None,
        }));
        self.dirty.set(false);
        self.time.set(0.0);
        self.playing.set(false);
        self.rebuild();
        spawn_forever(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            crate::ui::fit(self).await;
        });
        if debug() {
            spawn_forever(async {
                tokio::time::sleep(Duration::from_millis(3000)).await;
                eprintln!("[bridge] {:?}", bridge::stats().await);
            });
        }
    }

    /// Saves to the project folder; a project without one (opened from a
    /// FRAME pack, or new) asks where first.
    pub fn save(self) {
        let folder = self
            .editor
            .peek()
            .as_ref()
            .and_then(|e| e.project.folder.clone());
        match folder {
            Some(_) => {
                spawn_forever(async move {
                    self.save_now().await;
                });
            }
            None => self.save_as(),
        }
    }

    /// Refreshes the open graphic's thumbnail and saves into the project
    /// folder; `true` when saved (a project without a folder is not).
    pub async fn save_now(self) -> bool {
        let Some(dir) = self
            .editor
            .peek()
            .as_ref()
            .and_then(|e| e.project.folder.clone())
        else {
            return false;
        };
        self.capture_thumbnail().await;
        self.save_into(dir);
        !*self.dirty.peek()
    }

    /// Renders the open graphic (at the playhead) as its thumbnail, a
    /// 480 px wide JPEG. Failures are ignored: a thumbnail is a nicety.
    pub async fn capture_thumbnail(mut self) {
        let Some((w, h, index)) = self.editor.peek().as_ref().map(|e| {
            let s = &e.doc().scene;
            (s.width.max(1.0), s.height.max(1.0), e.graphic)
        }) else {
            return;
        };
        let tw = 480u32;
        let th = ((h / w) * f64::from(tw)).round().clamp(1.0, 960.0) as u32;
        let Ok(url) = bridge::thumbnail(w, h, (tw, th)).await else {
            return;
        };
        let Some(bytes) = url
            .split_once(',')
            .and_then(|(_, b)| crate::export::decode_base64(b))
        else {
            return;
        };
        if let Some(e) = self.editor.write().as_mut()
            && let Some(g) = e.project.media.pack.graphics.get_mut(index)
        {
            g.thumbnail = Some(bytes);
        }
    }

    /// Asks for a project folder (`Name.framecraft`) and saves there.
    pub fn save_as(self) {
        let Some((name, start)) = self
            .editor
            .peek()
            .as_ref()
            .map(|e| (e.project.name().to_string(), e.project.folder()))
        else {
            return;
        };
        spawn_forever(async move {
            let file = format!(
                "{}.framecraft",
                framecraft_core::export::names::file_base_name(&name, "Project")
            );
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Save FrameCraft project")
                .set_directory(&start)
                .set_file_name(&file)
                .save_file()
                .await;
            if let Some(h) = picked {
                let mut dir = h.path().to_path_buf();
                if dir.extension().is_none() {
                    dir.set_extension("framecraft");
                }
                self.save_into(dir);
            }
        });
    }

    fn save_into(mut self, dir: PathBuf) {
        // Some file-chooser portals create an empty file at the chosen
        // path; the project is a folder there.
        if dir.is_file() && std::fs::metadata(&dir).is_ok_and(|m| m.len() == 0) {
            let _ = std::fs::remove_file(&dir);
        }
        let result = self
            .editor
            .write()
            .as_mut()
            .map(|e| e.project.save_to(&dir));
        match result {
            Some(Ok(())) => {
                self.dirty.set(false);
                self.set_status(format!("Saved {}", dir.display()));
            }
            Some(Err(e)) => {
                let hint = if e.contains("Permission denied") {
                    " — no permission to write there; choose a folder you own (e.g. in your home folder)"
                } else {
                    ""
                };
                self.set_status(format!("Could not save: {e}{hint}"));
            }
            None => {}
        }
    }

    /// Writes the autosave copy when there are unsaved changes.
    pub fn autosave(self) {
        let mut status = self.status;
        if debug() {
            eprintln!("[autosave] tick, unsaved changes: {}", *self.dirty.peek());
        }
        if !*self.dirty.peek() {
            return;
        }
        let result = self.editor.peek().as_ref().map(|e| {
            if e.project.folder.is_none() {
                return Err("save the project once (Ctrl+S) to turn autosave on".to_string());
            }
            e.project.save_recovery()
        });
        match result {
            Some(Ok(())) => status
                .set("Autosaved (unsaved changes kept in the project's .recovery folder)".into()),
            Some(Err(e)) => status.set(format!("Not autosaved: {e}")),
            None => {}
        }
    }

    /// Asks where to write a FRAME project pack (`.frameproject.zip`).
    pub fn export_pack(self) {
        let Some((name, start, bytes)) = self.editor.peek().as_ref().map(|e| {
            (
                e.project.name().to_string(),
                e.project.folder(),
                e.project.export_pack(),
            )
        }) else {
            return;
        };
        let mut status = self.status;
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                status.set(format!("Could not build the pack: {e}"));
                return;
            }
        };
        spawn_forever(async move {
            let file = format!(
                "{}.frameproject.zip",
                framecraft_core::export::names::file_base_name(&name, "project")
            );
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Export FRAME project pack")
                .set_directory(&start)
                .set_file_name(&file)
                .add_filter("FRAME project pack", &["zip"])
                .save_file()
                .await;
            if let Some(h) = picked {
                let path = h.path().to_path_buf();
                match std::fs::write(&path, &bytes) {
                    Ok(()) => status.set(format!("Exported {}", path.display())),
                    Err(e) => status.set(format!("Could not write {}: {e}", path.display())),
                }
            }
        });
    }

    /// The open dialog: a FrameCraft project (its project.json) or a FRAME pack.
    fn open_dialog_now(self) {
        spawn_forever(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Open project")
                .add_filter("FrameCraft project or FRAME pack", &["json", "zip"])
                .add_filter("FrameCraft project (project.json)", &["json"])
                .add_filter("FRAME pack", &["zip"])
                .pick_file()
                .await;
            if let Some(h) = picked {
                self.open(h.path().to_path_buf());
            }
        });
    }

    /// Graphics of the project (Project view): add, clone, rename, delete,
    /// move and import keep each graphic's undo history with it.
    pub fn add_graphic(mut self) {
        let index = self.editor.write().as_mut().map(|e| {
            let i = e.project.add_graphic();
            e.histories.push(History::default());
            i
        });
        if let Some(i) = index {
            self.dirty.set(true);
            self.select_graphic(i);
        }
    }

    pub fn clone_graphic(mut self, i: usize) {
        let done = self.editor.write().as_mut().and_then(|e| {
            let at = e.project.clone_graphic(i)?;
            e.histories.insert(at, History::default());
            if e.graphic >= at {
                e.graphic += 1;
            }
            Some(())
        });
        if done.is_some() {
            self.dirty.set(true);
        }
    }

    pub fn rename_graphic(mut self, i: usize, name: &str) {
        if let Some(e) = self.editor.write().as_mut() {
            e.project.rename_graphic(i, name);
        }
        self.dirty.set(true);
    }

    pub fn delete_graphic(mut self, i: usize) {
        let current = self.editor.write().as_mut().and_then(|e| {
            if !e.project.delete_graphic(i) {
                return None;
            }
            e.histories.remove(i);
            let was = e.graphic;
            let next = if was == i {
                i.min(e.project.docs.len() - 1)
            } else if was > i {
                was - 1
            } else {
                was
            };
            e.graphic = next;
            Some((was == i, next))
        });
        match current {
            Some((true, next)) => {
                self.dirty.set(true);
                self.select_graphic(next);
            }
            Some(_) => self.dirty.set(true),
            None => self.set_status("A project keeps at least one graphic"),
        }
    }

    pub fn move_graphic(mut self, from: usize, to: usize) {
        let moved = self.editor.write().as_mut().is_some_and(|e| {
            if !e.project.move_graphic(from, to) {
                return false;
            }
            let h = e.histories.remove(from);
            e.histories.insert(to, h);
            // The open graphic stays open wherever it moves.
            e.graphic = follow_move(e.graphic, from, to);
            true
        });
        if moved {
            self.dirty.set(true);
        }
    }

    /// FRAME's Import Pack: a graphic pack (`.framepack.zip`) into this project.
    pub fn import_graphic_pack(self) {
        spawn_forever(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Import graphic pack")
                .add_filter("FRAME graphic pack", &["zip"])
                .pick_file()
                .await;
            let Some(h) = picked else { return };
            let mut ui = self;
            let result = ui.editor.write().as_mut().map(|e| {
                let r = e.project.import_graphic_pack(h.path());
                if r.is_ok() {
                    e.histories.push(History::default());
                }
                r
            });
            match result {
                Some(Ok((i, warnings))) => {
                    let files = ui
                        .editor
                        .peek()
                        .as_ref()
                        .map(|e| e.project.media_files())
                        .unwrap_or_default();
                    STORE.with(|s| {
                        s.borrow_mut()
                            .set_library(files, crate::project::builtin_dirs())
                    });
                    ui.dirty.set(true);
                    ui.select_graphic(i);
                    if !warnings.is_empty() {
                        ui.set_status(format!(
                            "Imported with warnings: {}",
                            warnings
                                .iter()
                                .take(3)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("; ")
                        ));
                    }
                }
                Some(Err(e)) => ui.set_status(e),
                None => {}
            }
        });
    }

    /// FRAME's Export Pack: one graphic as a `.framepack.zip`.
    pub fn export_graphic_pack(self, i: usize) {
        let Some((name, start, bytes)) = self.editor.peek().as_ref().map(|e| {
            (
                e.project.graphic_name(i).to_string(),
                e.project.folder(),
                e.project.export_graphic_pack(i),
            )
        }) else {
            return;
        };
        let mut status = self.status;
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                status.set(format!("Could not build the pack: {e}"));
                return;
            }
        };
        spawn_forever(async move {
            let file = format!(
                "{}.framepack.zip",
                framecraft_core::export::names::file_base_name(&name, "graphic")
            );
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Export graphic pack")
                .set_directory(&start)
                .set_file_name(&file)
                .add_filter("FRAME graphic pack", &["zip"])
                .save_file()
                .await;
            if let Some(h) = picked {
                let path = h.path().to_path_buf();
                match std::fs::write(&path, &bytes) {
                    Ok(()) => status.set(format!("Exported {}", path.display())),
                    Err(e) => status.set(format!("Could not write {}: {e}", path.display())),
                }
            }
        });
    }

    pub fn select_graphic(mut self, index: usize) {
        if let Some(e) = self.editor.write().as_mut()
            && index < e.project.docs.len()
        {
            e.graphic = index;
            e.selected = None;
            e.also.clear();
        }
        self.geometry.set(Vec::new());
        self.keys.set(Vec::new());
        self.cue.set(None);
        bridge::select(&[]);
        bridge::stop(false);
        self.playing.set(false);
        self.time.set(0.0);
        self.rebuild();
    }

    pub fn select_layer(self, id: Option<String>) {
        self.set_selection(id, Vec::new());
    }

    /// Sets the selection: the primary layer and the others.
    pub fn set_selection(mut self, primary: Option<String>, others: Vec<String>) {
        let others: Vec<String> = others
            .into_iter()
            .filter(|o| Some(o) != primary.as_ref())
            .collect();
        let changed = self
            .editor
            .peek()
            .as_ref()
            .is_some_and(|e| e.selected != primary);
        if let Some(e) = self.editor.write().as_mut() {
            e.selected = primary.clone();
            e.also = if primary.is_some() {
                others
            } else {
                Vec::new()
            };
        }
        if changed {
            self.keys.set(Vec::new());
            self.cue.set(None);
        }
        let all = self.selection();
        let mut geometry = self.geometry;
        geometry.set(Vec::new());
        bridge::select(&all);
    }

    /// Ctrl+click: adds a layer to the selection (it becomes primary), or
    /// removes it.
    pub fn toggle_layer(self, id: &str) {
        let current = self.selection();
        if current.iter().any(|x| x == id) {
            let rest: Vec<String> = current.into_iter().filter(|x| x != id).collect();
            let primary = rest.first().cloned();
            self.set_selection(primary, rest);
        } else {
            self.set_selection(Some(id.to_string()), current);
        }
    }

    /// Every selected layer, the primary first.
    pub fn selection(&self) -> Vec<String> {
        self.editor
            .peek()
            .as_ref()
            .map(|e| e.selection())
            .unwrap_or_default()
    }

    /// Aligns the selected layer (FRAME's align tools). One layer aligns to
    /// its parent's box, or to the scene; positions are set at the
    /// playhead (keys with Auto-key), as one undo step.
    pub fn align(self, mode: &str) {
        use framecraft_core::commands::SetValueAt;
        use framecraft_core::manipulate::align_box;
        use framecraft_core::timeline::pose_at;
        let (time, auto_key) = (*self.time.peek(), *self.auto_key.peek());
        if self.selection().len() > 1 {
            self.align_together(mode);
            return;
        }
        let planned = self.editor.peek().as_ref().and_then(|e| {
            let doc = e.doc();
            let id = e.selected.clone()?;
            let item = doc.items.iter().find(|i| i.id == id)?;
            let pose = pose_at(doc, item, time);
            let area = match item
                .parent_id
                .clone()
                .flatten()
                .and_then(|p| doc.items.iter().find(|i| i.id == p))
            {
                // Children are placed in their parent's box.
                Some(parent) => {
                    let pp = pose_at(doc, parent, time);
                    (0.0, 0.0, pp.width, pp.height)
                }
                None => (0.0, 0.0, doc.scene.width, doc.scene.height),
            };
            let (x, y) = align_box(mode, (pose.x, pose.y, pose.width, pose.height), area);
            let mut cmds: Vec<Box<dyn Command>> = Vec::new();
            for (lane, from, to) in [("x", pose.x, x), ("y", pose.y, y)] {
                if (from - to).abs() > 1e-9 {
                    cmds.push(Box::new(SetValueAt {
                        item: id.clone(),
                        lane: lane.into(),
                        time_ms: time,
                        value: to,
                        auto_key,
                    }));
                }
            }
            Some((item.name.clone(), cmds))
        });
        let Some((name, cmds)) = planned else { return };
        if cmds.is_empty() {
            return;
        }
        self.begin_gesture(format!("Align {name}"));
        self.execute_all(&cmds);
        self.end_gesture();
    }

    /// Starts an undo gesture (a drag): commands until
    /// [`Ui::end_gesture`] are one undo step.
    pub fn begin_gesture(mut self, label: String) {
        if let Some(e) = self.editor.write().as_mut() {
            let g = e.graphic;
            e.histories[g].begin_gesture(&e.project.docs[g], label);
        }
    }

    pub fn end_gesture(mut self) {
        if let Some(e) = self.editor.write().as_mut() {
            let g = e.graphic;
            e.histories[g].end_gesture(&e.project.docs[g]);
        }
    }

    /// Runs several commands and updates the preview once.
    pub fn execute_all(mut self, cmds: &[Box<dyn Command>]) {
        let mut change = Change::None;
        let mut error = None;
        if let Some(e) = self.editor.write().as_mut() {
            let g = e.graphic;
            for cmd in cmds {
                match e.histories[g].execute(&mut e.project.docs[g], cmd.as_ref()) {
                    Ok(c) => change = change.merge(c),
                    Err(err) => error = Some(err.to_string()),
                }
            }
        }
        if let Some(err) = error {
            self.set_status(err);
        }
        self.show(change);
    }

    /// Loads a freshly built preview of the current graphic.
    pub fn rebuild(mut self) {
        bridge::install();
        let built = self.editor.write().as_mut().map(|e| {
            let (pkg, state) = e.project.preview(e.graphic);
            e.preview = Some(state);
            pkg
        });
        if let Some(pkg) = built {
            let url = STORE.with(|s| s.borrow_mut().publish(pkg));
            bridge::seek((self.time)());
            bridge::load(&url);
        }
    }

    /// Brings the preview in line after a change.
    fn show(mut self, change: Change) {
        if !*self.dirty.peek() {
            self.dirty.set(true);
        }
        let update = self.editor.write().as_mut().and_then(|e| {
            let prev = e.preview.as_ref()?;
            let (update, state) = e.project.preview_update(e.graphic, prev, &change);
            e.preview = Some(state);
            Some(update)
        });
        match update {
            None | Some(PreviewUpdate::Nothing) => {}
            Some(PreviewUpdate::Message(msg)) => bridge::edit(&msg),
            Some(PreviewUpdate::Rebuild(pkg)) => {
                let url = STORE.with(|s| s.borrow_mut().publish(pkg));
                bridge::load(&url);
            }
        }
        if debug() {
            eprintln!("[edit] {change:?}");
        }
    }

    /// Runs a command on the current graphic as its own undo step (or as
    /// part of an open gesture).
    pub fn execute(mut self, cmd: &dyn Command) {
        let result = self.editor.write().as_mut().map(|e| {
            let g = e.graphic;
            e.histories[g].execute(&mut e.project.docs[g], cmd)
        });
        match result {
            Some(Ok(change)) => self.show(change),
            Some(Err(err)) => self.set_status(err.to_string()),
            None => {}
        }
    }

    pub fn undo(mut self) {
        let change = self.editor.write().as_mut().and_then(|e| {
            let g = e.graphic;
            e.histories[g].undo(&mut e.project.docs[g])
        });
        if let Some(change) = change {
            self.show(change);
        }
    }

    pub fn redo(mut self) {
        let change = self.editor.write().as_mut().and_then(|e| {
            let g = e.graphic;
            e.histories[g].redo(&mut e.project.docs[g])
        });
        if let Some(change) = change {
            self.show(change);
        }
    }

    pub fn selected(&self) -> Option<String> {
        self.editor.peek().as_ref().and_then(|e| e.selected.clone())
    }

    /// Adds a layer on top and selects it.
    /// Opens the media library for a pick.
    pub fn open_media(mut self, category: &'static str, target: PickTarget) {
        if self.editor.peek().is_some() {
            self.media.set(Some(MediaPick { category, target }));
        }
    }

    /// Copies files from disk into the project's media. Returns how many
    /// were added; problems go to the status line.
    pub fn import_media(mut self, paths: Vec<PathBuf>) -> usize {
        let mut added = 0;
        let mut problems = Vec::new();
        if let Some(e) = self.editor.write().as_mut() {
            for path in paths {
                match e.project.import_media(&path) {
                    Ok(entry) => {
                        let bytes = e
                            .project
                            .media_files()
                            .into_iter()
                            .find(|(u, _)| *u == entry.url)
                            .map(|(_, b)| b)
                            .unwrap_or_default();
                        STORE.with(|s| s.borrow_mut().add_to_library(&entry.url, bytes));
                        added += 1;
                    }
                    Err(err) => problems.push(err),
                }
            }
        }
        if added > 0 {
            self.dirty.set(true);
        }
        if problems.is_empty() {
            self.set_status(format!(
                "Imported {added} file{}. Saving keeps them in the project folder.",
                if added == 1 { "" } else { "s" }
            ));
        } else {
            self.set_status(problems.join("; "));
        }
        added
    }

    /// Uses a picked media file (`url`, shown name `name`) for the open
    /// library's target, and closes the library.
    pub fn pick_media(mut self, url: String, name: String, svg_mode: SvgMode) {
        let Some(pick) = self.media.peek().clone() else {
            return;
        };
        self.media.set(None);
        let is_svg = url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .ends_with(".svg");
        match pick.target {
            PickTarget::NewLayer
                if is_svg && (pick.category == "shapes" || svg_mode != SvgMode::Image) =>
            {
                use framecraft_core::export::package::MediaSource;
                let bytes = self
                    .editor
                    .peek()
                    .as_ref()
                    .and_then(|e| e.project.media.read(&url))
                    .unwrap_or_default();
                // FRAME: stock shapes fit 256 px, uploaded SVGs 512 px.
                let max_side = if pick.category == "shapes" {
                    256.0
                } else {
                    512.0
                };
                let item =
                    framecraft_core::layers::new_svg_layer(&url, &name, &bytes, svg_mode, max_side);
                let id = item.id.clone();
                self.execute(&AddItem { item });
                self.select_layer(Some(id));
            }
            PickTarget::NewLayer => {
                let size = (pick.category == "images")
                    .then(|| {
                        use framecraft_core::export::package::MediaSource;
                        self.editor
                            .peek()
                            .as_ref()
                            .and_then(|e| e.project.media.read(&url))
                            .and_then(|b| framecraft_core::media::image_size(&b))
                    })
                    .flatten();
                let item = framecraft_core::layers::new_media_layer(
                    pick.category,
                    &url,
                    &name,
                    size,
                    None,
                );
                let id = item.id.clone();
                self.execute(&AddItem { item });
                self.select_layer(Some(id));
            }
            PickTarget::Field {
                prop,
                item,
                modifier,
            } => {
                let value = serde_json::Value::from(url);
                let (time, auto_key) = (*self.time.peek(), *self.auto_key.peek());
                let cmd = match &modifier {
                    Some(m) => prop.modifier_command(&item, m, value, time, auto_key),
                    None => prop.command(Some(&item), value, time, auto_key),
                };
                if let Some(cmd) = cmd {
                    self.execute(cmd.as_ref());
                }
            }
        }
    }

    /// Turns a tinted SVG image layer into an editable polygon (FRAME's
    /// Convert to polygon), traced at the layer's size.
    pub fn convert_svg_to_polygon(self, id: &str) {
        use framecraft_core::export::package::MediaSource;
        let traced = self.editor.peek().as_ref().and_then(|e| {
            let item = e.doc().items.iter().find(|i| i.id == id)?;
            let bytes = e.project.media.read(item.media_url.as_deref()?)?;
            let text = String::from_utf8(bytes).ok()?;
            framecraft_core::polygon::svg_to_polygon(&text, item.width, item.height)
        });
        match traced {
            Some((points, bezier)) => self.execute(&framecraft_core::polygon::SetPolygon {
                item: id.to_string(),
                points: Some(points),
                curve: Some(if bezier { "bezier" } else { "linear" }.into()),
                to_shape: true,
            }),
            None => self.set_status("This SVG has no path or polygon to trace"),
        }
    }

    pub fn add_layer(self, kind: NewLayer) {
        if self.editor.peek().is_none() {
            return;
        }
        let item = new_layer(kind, None);
        let id = item.id.clone();
        self.execute(&AddItem { item });
        self.select_layer(Some(id));
    }

    /// Several layers aligned to their joint bounds (FRAME's multi-select
    /// align), one undo step.
    fn align_together(self, mode: &str) {
        use framecraft_core::commands::SetValueAt;
        use framecraft_core::manipulate::{align_box, bounds_of};
        use framecraft_core::timeline::pose_at;
        let (time, auto_key) = (*self.time.peek(), *self.auto_key.peek());
        let ids = self.selection();
        let cmds: Vec<Box<dyn Command>> = self
            .editor
            .peek()
            .as_ref()
            .map(|e| {
                let doc = e.doc();
                let boxes: Vec<(String, (f64, f64, f64, f64))> = ids
                    .iter()
                    .filter_map(|id| {
                        let item = doc.items.iter().find(|i| &i.id == id)?;
                        let p = pose_at(doc, item, time);
                        Some((id.clone(), (p.x, p.y, p.width, p.height)))
                    })
                    .collect();
                let Some(area) = bounds_of(&boxes.iter().map(|(_, b)| *b).collect::<Vec<_>>())
                else {
                    return Vec::new();
                };
                let mut cmds: Vec<Box<dyn Command>> = Vec::new();
                for (id, b) in boxes {
                    let (x, y) = align_box(mode, b, area);
                    for (lane, from, to) in [("x", b.0, x), ("y", b.1, y)] {
                        if (from - to).abs() > 1e-9 {
                            cmds.push(Box::new(SetValueAt {
                                item: id.clone(),
                                lane: lane.into(),
                                time_ms: time,
                                value: to,
                                auto_key,
                            }));
                        }
                    }
                }
                cmds
            })
            .unwrap_or_default();
        if cmds.is_empty() {
            return;
        }
        self.begin_gesture(format!("Align {} layers", ids.len()));
        self.execute_all(&cmds);
        self.end_gesture();
    }

    /// Duplicates the selected layers (with their children) and selects
    /// the copies.
    pub fn duplicate_selected(self) {
        let ids = self.selection();
        if ids.is_empty() {
            return;
        }
        let cmd = match self.editor.peek().as_ref() {
            Some(e) => DuplicateItems::new(e.doc(), ids),
            None => return,
        };
        self.execute(&cmd);
        let copies = self
            .editor
            .peek()
            .as_ref()
            .map(|e| cmd.copy_ids(e.doc()))
            .unwrap_or_default();
        if let Some(first) = copies.first().cloned() {
            self.set_selection(Some(first), copies);
        }
    }

    pub fn delete_selected(self) {
        let ids = self.selection();
        if !ids.is_empty() {
            self.select_layer(None);
            self.execute(&DeleteItems { items: ids });
        }
    }

    /// Moves the playhead (stopping playback).
    pub fn set_time(mut self, t: f64) {
        if *self.playing.peek() {
            bridge::stop(false);
        }
        self.playing.set(false);
        let t = t.clamp(0.0, self.duration());
        self.time.set(t);
        bridge::seek(t);
    }

    /// Play / Pause. Playback runs in the page; Rust hears where it stopped.
    pub fn toggle_play(mut self) {
        if *self.playing.peek() {
            // Pause: the bridge reports the time it stopped at.
            bridge::stop(true);
            return;
        }
        let duration = self.duration();
        self.playing.set(true);
        let now = *self.time.peek();
        let from = if now >= duration { 0.0 } else { now };
        spawn_forever(async move {
            let stopped_at = bridge::play(from, duration).await;
            let (mut time, mut playing) = (self.time, self.playing);
            if let Some(t) = stopped_at {
                time.set(t);
            }
            playing.set(false);
        });
    }
}

/// Where the item at `current` ends up when the item at `from` moves to `to`.
fn follow_move(current: usize, from: usize, to: usize) -> usize {
    if current == from {
        to
    } else if from < current && to >= current {
        current - 1
    } else if from > current && to <= current {
        current + 1
    } else {
        current
    }
}

impl Ui {
    /// The open graphic's frame rate (frames and timecode formats).
    pub fn fps(self) -> f64 {
        self.editor
            .peek()
            .as_ref()
            .map_or(25.0, |e| e.doc().timeline.fps)
    }

    /// `ms` in the chosen time format.
    pub fn fmt_time(self, ms: f64) -> String {
        framecraft_core::timecode::format_time(ms, *self.time_format.peek(), self.fps())
    }

    /// Typed time (in the chosen format, or with a unit) as ms.
    pub fn parse_time(self, text: &str) -> Option<f64> {
        framecraft_core::timecode::parse_time(text, *self.time_format.peek(), self.fps())
    }

    /// Sets the time format, remembers it and tells the bridge (the
    /// playhead label is drawn there while playing).
    pub fn set_time_format(self, f: framecraft_core::timecode::Format) {
        let mut tf = self.time_format;
        tf.set(f);
        crate::layout::save_time_format(f);
        crate::bridge::time_format(f.id(), self.fps());
    }
}

#[cfg(test)]
mod tests {
    use super::follow_move;

    #[test]
    fn open_graphic_follows_moves() {
        // [a b c d], c open (2).
        assert_eq!(follow_move(2, 2, 0), 0); // c itself moves to the front
        assert_eq!(follow_move(2, 0, 3), 1); // a moves past c
        assert_eq!(follow_move(2, 3, 0), 3); // d moves before c
        assert_eq!(follow_move(2, 0, 1), 2); // a and b swap, c stays
    }
}
