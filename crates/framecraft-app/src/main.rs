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
mod interact;
mod media;
mod motions;
mod outliner;
mod palettes;
mod preview;
mod project;
mod project_view;
mod properties;
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
        }
    }
}
