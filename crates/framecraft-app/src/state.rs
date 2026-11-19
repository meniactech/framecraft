// SPDX-License-Identifier: AGPL-3.0-or-later
//! Editor state and the one path every edit takes:
//! command → document → undo history → preview update.

use crate::bridge;
use crate::interact::{Drag, Geom};
use crate::preview::STORE;
use crate::project::Project;
use crate::timeline::TlView;
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
    pub selected: Option<String>,
    /// One undo history per graphic.
    histories: Vec<History>,
    /// What the loaded preview was built from.
    preview: Option<PreviewState>,
}

impl Editor {
    pub fn doc(&self) -> &Document {
        &self.project.docs[self.graphic]
    }

    pub fn history(&self) -> &History {
        &self.histories[self.graphic]
    }
}

/// Shared UI state, handed to components as one copyable value.
#[derive(Clone, Copy, PartialEq)]
pub struct Ui {
    pub editor: Signal<Option<Editor>>,
    pub time: Signal<f64>,
    pub playing: Signal<bool>,
    pub zoom: Signal<f64>,
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
    pub tl_zoom: Signal<f64>,
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
    /// A project folder whose autosave copy waits for Restore / Discard.
    pub recovery: Signal<Option<PathBuf>>,
    /// The user's components (data folder).
    pub user_components: Signal<framecraft_core::components::UserComponents>,
    /// Make Component waiting for its name.
    pub pending_component: Signal<Option<crate::components::PendingComponent>>,
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
            auto_key: use_signal(|| false),
            geometry: use_signal(Vec::new),
            drag: use_signal(|| None),
            status: use_signal(|| "Open a .frameproject.zip or .framepack.zip".to_string()),
            tab: use_signal(|| Tab::Transform),
            keys: use_signal(Vec::new),
            cue: use_signal(|| None),
            tl_view: use_signal(|| TlView::Keys),
            tl_zoom: use_signal(|| 0.1),
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
            dirty: use_signal(|| false),
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
    pub fn new_project(self) {
        self.install(Project::new_empty("Untitled"), "New project");
        let mut dirty = self.dirty;
        dirty.set(true);
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
                .set_library(files, crate::project::builtin_dir())
        });
        self.editor.set(Some(Editor {
            project,
            graphic: 0,
            selected: None,
            histories,
            preview: None,
        }));
        self.dirty.set(false);
        self.time.set(0.0);
        self.playing.set(false);
        self.rebuild();
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            crate::ui::fit(self).await;
        });
        if debug() {
            spawn(async {
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
            Some(dir) => self.save_into(dir),
            None => self.save_as(),
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
        spawn(async move {
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
        spawn(async move {
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
    pub fn open_dialog(self) {
        spawn(async move {
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

    pub fn select_graphic(mut self, index: usize) {
        if let Some(e) = self.editor.write().as_mut()
            && index < e.project.docs.len()
        {
            e.graphic = index;
            e.selected = None;
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

    pub fn select_layer(mut self, id: Option<String>) {
        let changed = self
            .editor
            .peek()
            .as_ref()
            .is_some_and(|e| e.selected != id);
        if let Some(e) = self.editor.write().as_mut() {
            e.selected = id.clone();
        }
        if changed {
            self.geometry.set(Vec::new());
            self.keys.set(Vec::new());
            self.cue.set(None);
        }
        bridge::select(&id.into_iter().collect::<Vec<_>>());
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

    /// Duplicates the selected layer (with its children) and selects the copy.
    pub fn duplicate_selected(self) {
        let Some(id) = self.selected() else { return };
        let cmd = match self.editor.peek().as_ref() {
            Some(e) => DuplicateItems::new(e.doc(), vec![id]),
            None => return,
        };
        self.execute(&cmd);
        let copy = self
            .editor
            .peek()
            .as_ref()
            .and_then(|e| cmd.copy_ids(e.doc()).into_iter().next());
        if copy.is_some() {
            self.select_layer(copy);
        }
    }

    pub fn delete_selected(self) {
        if let Some(id) = self.selected() {
            self.select_layer(None);
            self.execute(&DeleteItems { items: vec![id] });
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
        spawn(async move {
            let stopped_at = bridge::play(from, duration).await;
            let (mut time, mut playing) = (self.time, self.playing);
            if let Some(t) = stopped_at {
                time.set(t);
            }
            playing.set(false);
        });
    }
}
