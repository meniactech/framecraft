// SPDX-License-Identifier: AGPL-3.0-or-later
//! FrameCraft desktop app.
//!
//! Opens a FRAME project pack, shows a graphic in the viewport (the real
//! exported page with an editor script, updated live), edits the selected
//! layer through undoable commands, exports SPX / OGraf / banner ZIPs and
//! saves a copy of the project.
//!
//! Run with an optional pack path: `cargo run -p framecraft-app -- my.frameproject.zip`
//! `FRAMECRAFT_DEBUG=1` logs preview requests, edits and bridge activity.

// Windows: a release build is a GUI program (no console window next to it).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod bridge;
mod components;
mod export;
mod interact;
mod layout;
mod media;
mod menubar;
mod motions;
mod outliner;
mod palettes;
mod preview;
mod project;
mod project_view;
mod properties;
mod recent;
mod render;
mod script_editor;
mod state;
mod timeline;
mod ui;
mod updates;

use dioxus::core::spawn_forever;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, wry::http::Response};
use dioxus::prelude::*;
use outliner::Outliner;
use preview::{ORIGIN, SCHEME, STORE};
use properties::Properties;
use state::{Ui, debug};
use std::path::PathBuf;
use timeline::Timeline;
use ui::{Header, Viewport};

const STYLE: &str = include_str!("style.css");

fn main() {
    let window = WindowBuilder::new()
        .with_title("FrameCraft")
        .with_inner_size(LogicalSize::new(1440.0, 900.0));
    let config = Config::new().with_window(window);
    // No menu bar for now, except on macOS: there copy, paste and Quit in
    // text fields come from the app's Edit menu (Dioxus's default one).
    let config = if cfg!(target_os = "macos") {
        config
    } else {
        config.with_menu(None)
    };
    let config = config
        .with_custom_head(format!(
            "<style>{}</style>",
            STYLE.replace("framecraft://preview", ORIGIN)
        ))
        .with_custom_protocol(SCHEME, |_, request| {
            let found = preview::is_preview_host(request.uri().host())
                .then(|| STORE.with(|s| s.borrow().lookup(request.uri().path())))
                .flatten();
            if debug() {
                let status = found.as_ref().map_or("404".to_string(), |(b, m)| {
                    format!("200 {m} {} bytes", b.len())
                });
                eprintln!("[preview] {} -> {status}", request.uri());
            }
            let response = match found {
                Some((bytes, mime)) => Response::builder()
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(bytes.into()),
                None => Response::builder().status(404).body(Vec::new().into()),
            };
            response.expect("valid response")
        });
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

#[component]
fn App() -> Element {
    let ui = Ui::new();

    // Linux: wry registers the preview scheme as secure only; CORS too, so
    // images loaded with crossOrigin (particle sprites) keep canvases
    // readable for renders and snapshots.
    #[cfg(target_os = "linux")]
    use_hook(|| {
        use dioxus::desktop::wry::WebViewExtUnix;
        use webkit2gtk::{SecurityManagerExt, WebContextExt, WebViewExt};
        let webview = dioxus::desktop::window().webview.webview();
        if let Some(sm) = webview.context().and_then(|c| c.security_manager()) {
            sm.register_uri_scheme_as_cors_enabled(SCHEME);
        }
    });

    // A newer FrameCraft on GitHub (at most one request a day).
    use_hook(move || {
        spawn_forever(updates::check(ui));
    });

    // A pack given on the command line opens at start.
    use_hook(move || {
        if let Some(path) = std::env::args_os().nth(1) {
            let mut path = PathBuf::from(path);
            // An AppImage runs from inside itself (see build-appimage.sh);
            // its runtime keeps the folder it was started from in $OWD.
            if path.is_relative()
                && let Some(owd) = std::env::var_os("OWD").filter(|d| !d.is_empty())
            {
                path = PathBuf::from(owd).join(path);
            }
            ui.open(path);
        }
    });

    // Panel sizes from the last session.
    use_hook(|| {
        if let Some(layout) = layout::load() {
            layout::apply(&layout);
        }
    });

    // Pointer, geometry and key events from the page.
    use_future(move || async move {
        bridge::events(move |ev| interact::on_event(ui, ev)).await;
    });

    // Autosave: unsaved changes go to the project folder's `.recovery/`
    // every 30 seconds.
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            ui.autosave();
        }
    });

    // Closing with unsaved changes asks first: the window hides instead of
    // closing and is shown again with the prompt.
    let mut close_prompt = use_signal(|| false);
    // Dioxus hides the window right after this handler sees the close
    // request (app handlers run first), so the window is shown again on the
    // next event the loop delivers.
    let reshow = std::rc::Rc::new(std::cell::Cell::new(false));
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        use dioxus::desktop::tao::event::Event;
        if let Event::WindowEvent {
            event: dioxus::desktop::WindowEvent::CloseRequested,
            ..
        } = event
        {
            let dirty = *ui.dirty.peek();
            if state::debug() {
                eprintln!("[close] requested, unsaved changes: {dirty}");
            }
            // Decided here, just before Dioxus handles the request: close
            // (and exit), or hide and come back with the prompt.
            dioxus::desktop::window().set_close_behavior(if dirty {
                dioxus::desktop::WindowCloseBehaviour::WindowHides
            } else {
                dioxus::desktop::WindowCloseBehaviour::WindowCloses
            });
            if dirty {
                close_prompt.set(true);
                reshow.set(true);
            }
            return;
        }
        if reshow.get() {
            reshow.set(false);
            let w = dioxus::desktop::window();
            w.set_visible(true);
            w.set_focus();
            if state::debug() {
                eprintln!("[close] window shown again with the prompt");
            }
        }
    });

    // Window title: project name, and • for unsaved changes.
    use_effect(move || {
        let name = ui
            .editor
            .read()
            .as_ref()
            .map(|e| e.project.name().to_string());
        let mark = if (ui.dirty)() { "• " } else { "" };
        let title = match name {
            Some(n) => format!("{mark}{n} — FrameCraft"),
            None => "FrameCraft".to_string(),
        };
        dioxus::desktop::window().set_title(&title);
    });

    rsx! {
        div { class: "app",
            Header { ui }
            // Left: the viewport over the timeline (same width). Right: the
            // outliner and properties, down to the status bar.
            div { class: "main",
                div { class: "work",
                    Viewport { ui }
                    div { class: "split split-timeline", "data-split": "timeline", title: "Drag to resize; double-click to reset" }
                    Timeline { ui }
                }
                div { class: "side",
                    div { class: "split split-side", "data-split": "side", title: "Drag to resize; double-click to reset" }
                    div { class: "split split-outliner", "data-split": "outliner", title: "Drag to resize; double-click to reset" }
                    Outliner { ui }
                    Properties { ui }
                }
            }
            ui::StatusBar { ui }
            media::MediaDialog { ui }
            project_view::ProjectView { ui }
            motions::MotionDialog { ui }
            components::ComponentNameDialog { ui }
            components::ComponentLibraryDialog { ui }
            RecoveryPrompt { ui }
            if close_prompt() {
                ClosePrompt { ui, open: close_prompt }
            }
            if let Some(next) = (ui.switch_prompt)() {
                SwitchPrompt { ui, next }
            }
            script_editor::ScriptEditor { ui }
            menubar::HelpDialog { ui }
        }
    }
}

/// Asks whether to restore the autosave copy found when opening a project.
#[component]
fn RecoveryPrompt(ui: Ui) -> Element {
    let Some(dir) = (ui.recovery)() else {
        return rsx! {};
    };
    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal motion-dialog small",
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Unsaved changes found" }
                        p { class: "dim",
                            "FrameCraft autosaved changes to this project that were never saved (the app closed or crashed): {dir.display()}"
                        }
                    }
                }
                div { class: "motion-foot",
                    span {}
                    button {
                        title: "Delete the autosave and keep the saved project",
                        onclick: move |_| ui.resolve_recovery(false),
                        "Discard"
                    }
                    button {
                        class: "active",
                        title: "Open the autosaved version (save to keep it)",
                        onclick: move |_| ui.resolve_recovery(true),
                        "Restore"
                    }
                }
            }
        }
    }
}

/// Closing with unsaved changes: Cancel, Close without saving, or Save and
/// close (a project not saved as a folder yet asks where first and stays
/// open).
#[component]
fn ClosePrompt(ui: Ui, open: Signal<bool>) -> Element {
    let has_folder = ui
        .editor
        .read()
        .as_ref()
        .is_some_and(|e| e.project.folder.is_some());
    let close_now = move || {
        let w = dioxus::desktop::window();
        w.set_close_behavior(dioxus::desktop::WindowCloseBehaviour::WindowCloses);
        w.close();
    };
    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal motion-dialog small",
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Unsaved changes" }
                        p { class: "dim", "This project has changes that are not saved." }
                    }
                }
                div { class: "motion-foot",
                    span {}
                    button { onclick: move |_| open.set(false), "Cancel" }
                    button {
                        title: "Close and lose the unsaved changes (the autosave copy is deleted too)",
                        onclick: move |_| {
                            let folder = ui.editor.peek().as_ref().and_then(|e| e.project.folder.clone());
                            if let Some(dir) = folder {
                                let _ = framecraft_core::project::Project::discard_recovery(&dir);
                            }
                            close_now();
                        },
                        "Close without saving"
                    }
                    button {
                        class: "active",
                        onclick: move |_| {
                            open.set(false);
                            if has_folder {
                                spawn_forever(async move {
                                    if ui.save_now().await {
                                        close_now();
                                    }
                                });
                            } else {
                                ui.save_as();
                            }
                        },
                        if has_folder { "Save and close" } else { "Save as…" }
                    }
                }
            }
        }
    }
}

/// New / Open with unsaved changes: save first, go on without saving, or
/// stay (the close prompt's choices).
#[component]
fn SwitchPrompt(ui: Ui, next: crate::state::Switch) -> Element {
    let mut prompt = ui.switch_prompt;
    let has_folder = ui
        .editor
        .read()
        .as_ref()
        .is_some_and(|e| e.project.folder.is_some());
    let what = match next {
        crate::state::Switch::New => "a new project",
        crate::state::Switch::Open | crate::state::Switch::Path(_) => "another project",
    };
    let next2 = next.clone();
    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal motion-dialog small",
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Unsaved changes" }
                        p { class: "dim", "This project has changes that are not saved. Save them before opening {what}?" }
                    }
                }
                div { class: "motion-foot",
                    span {}
                    button { onclick: move |_| prompt.set(None), "Cancel" }
                    button {
                        title: "Go on and lose the unsaved changes (the autosave copy stays, and is offered when this project is opened again)",
                        onclick: move |_| {
                            prompt.set(None);
                            ui.continue_switch(next.clone());
                        },
                        "Don't save"
                    }
                    button {
                        class: "active",
                        onclick: move |_| {
                            prompt.set(None);
                            if has_folder {
                                let next = next2.clone();
                                spawn_forever(async move {
                                    if ui.save_now().await {
                                        ui.continue_switch(next);
                                    }
                                });
                            } else {
                                ui.save_as();
                            }
                        },
                        if has_folder { "Save" } else { "Save as…" }
                    }
                }
            }
        }
    }
}
