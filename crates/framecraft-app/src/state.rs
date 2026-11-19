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
        }
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

    pub fn open(mut self, path: PathBuf) {
        match Project::open(&path) {
            Ok(mut project) => {
                project.palette_colors = self.palettes.peek().active_css();
                let warnings = project.warnings.len();
                self.set_status(format!(
                    "Opened {} ({} graphics){}",
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
            Err(e) => self.set_status(e),
        }
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

    fn selected(&self) -> Option<String> {
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
        if problems.is_empty() {
            self.set_status(format!(
                "Imported {added} file{}. Saving keeps the files that layers use.",
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
