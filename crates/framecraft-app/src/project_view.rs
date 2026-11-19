// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Project view (FRAME's dashboard for one project): the project's
//! graphics, and the Media Manager with the project's images, Lottie files
//! and videos, and the user's palettes. It covers the editor while open;
//! the editor stays loaded underneath.

use crate::preview::library_url;
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::media::{accept, of_category};

/// Media Manager tabs, as FRAME's (Plates, Fonts and Components come later).
const TABS: &[(&str, &str)] = &[
    ("images", "Images"),
    ("lottie", "Lottie"),
    ("videos", "Videos (exp.)"),
    ("palettes", "Palettes"),
];

#[component]
pub fn ProjectView(ui: Ui) -> Element {
    let mut open = ui.project_view;
    let mut tab = use_signal(|| "palettes");
    if !open() {
        return rsx! {};
    }
    let (name, graphics, current) = match ui.editor.read().as_ref() {
        Some(e) => (
            e.project.name().to_string(),
            (0..e.project.docs.len())
                .map(|i| e.project.graphic_name(i).to_string())
                .collect::<Vec<_>>(),
            Some(e.graphic),
        ),
        None => (String::new(), Vec::new(), None),
    };
    let current_tab = tab();
    rsx! {
        div { class: "project-view",
            div { class: "project-head",
                span { class: "brand", "FrameCraft" }
                h2 {
                    if name.is_empty() {
                        "No project open"
                    } else {
                        "{name}"
                    }
                }
                button { class: "active", onclick: move |_| open.set(false), "Back to editor" }
            }
            div { class: "project-body",
                aside { class: "project-graphics",
                    h3 { "Graphics" }
                    if graphics.is_empty() {
                        p { class: "dim", "Open a project to see its graphics." }
                    }
                    for (i , g) in graphics.into_iter().enumerate() {
                        button {
                            class: if current == Some(i) { "graphic-card active" } else { "graphic-card" },
                            title: "Edit this graphic",
                            onclick: move |_| {
                                ui.select_graphic(i);
                                open.set(false);
                            },
                            "{g}"
                        }
                    }
                }
                main { class: "media-manager",
                    div { class: "lib-head",
                        h3 { "Media Manager" }
                        span { class: "dim", "The project's images, Lottie and video files, and your palettes" }
                    }
                    div { class: "seg lib-tabs",
                        for (id , label) in TABS.iter().copied() {
                            button {
                                class: if id == current_tab { "active" } else { "" },
                                onclick: move |_| tab.set(id),
                                "{label}"
                            }
                        }
                    }
                    div { class: "lib-content",
                        if current_tab == "palettes" {
                            crate::palettes::PalettesTab { ui }
                        } else {
                            MediaTab { ui, category: current_tab }
                        }
                    }
                }
            }
        }
    }
}

/// One media category of the project, with Import.
#[component]
fn MediaTab(ui: Ui, category: &'static str) -> Element {
    let editor = ui.editor.read();
    let Some(ed) = editor.as_ref() else {
        return rsx! {
            p { class: "dim", "Open a project to manage its media." }
        };
    };
    let files: Vec<(String, String)> = of_category(&ed.project.media.pack, category)
        .into_iter()
        .map(|e| {
            let name = e
                .original_name
                .clone()
                .unwrap_or_else(|| e.url.rsplit('/').next().unwrap_or("").to_string());
            (e.url.clone(), name)
        })
        .collect();
    drop(editor);
    rsx! {
        section { class: "lib-section",
            div { class: "lib-section-head",
                p { class: "dim",
                    "{files.len()} file(s). Saving the project keeps the files that layers use."
                }
                label {
                    class: "button",
                    title: "Copy files from disk into the project",
                    "Import…"
                    input {
                        r#type: "file",
                        multiple: true,
                        accept: accept(category),
                        onchange: move |evt| {
                            let paths: Vec<_> = evt.files().iter().map(|f| f.path()).collect();
                            ui.import_media(paths);
                        },
                    }
                }
            }
            div { class: "media-grid",
                for (url , name) in files {
                    div { class: "media-tile", title: "{name}",
                        div { class: "media-thumb",
                            if category == "images" {
                                img { src: "{library_url(&url)}", loading: "lazy", draggable: "false" }
                            } else if category == "videos" {
                                span { class: "media-kind", "▶" }
                            } else {
                                span { class: "media-kind", "{{ }}" }
                            }
                        }
                        span { class: "media-name", "{name}" }
                    }
                }
            }
        }
    }
}
