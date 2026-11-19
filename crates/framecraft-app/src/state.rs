// SPDX-License-Identifier: AGPL-3.0-or-later
//! Editor state and the one path every edit takes:
//! command → document → undo history → preview update.

use crate::bridge;
use crate::interact::{Drag, Geom};
use crate::preview::STORE;
use crate::project::Project;
use dioxus::prelude::*;
use framecraft_core::commands::{Change, Command, History};
use framecraft_core::normalize::Document;
use framecraft_core::preview::{PreviewState, PreviewUpdate};
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
            Ok(project) => {
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
}
