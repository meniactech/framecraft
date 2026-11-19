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

mod bridge;
mod components;
mod export;
mod interact;
mod media;
mod motions;
mod outliner;
mod palettes;
mod preview;
mod project;
mod project_view;
mod properties;
mod script_editor;
mod state;
mod timeline;
mod ui;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, wry::http::Response};
use dioxus::prelude::*;
use outliner::Outliner;
use preview::{HOST, SCHEME, STORE};
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
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_custom_head(format!("<style>{STYLE}</style>"))
        .with_custom_protocol(SCHEME, |_, request| {
            let found = (request.uri().host() == Some(HOST))
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

    // A pack given on the command line opens at start.
    use_hook(move || {
        if let Some(path) = std::env::args_os().nth(1) {
            ui.open(PathBuf::from(path));
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
            div { class: "main",
                Viewport { ui }
                div { class: "side",
                    Outliner { ui }
                    Properties { ui }
                }
            }
            Timeline { ui }
            media::MediaDialog { ui }
            project_view::ProjectView { ui }
            motions::MotionDialog { ui }
            components::ComponentNameDialog { ui }
            components::ComponentLibraryDialog { ui }
            RecoveryPrompt { ui }
            script_editor::ScriptEditor { ui }
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
