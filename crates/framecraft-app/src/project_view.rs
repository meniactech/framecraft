// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Project view (FRAME's dashboard for one project): the project's
//! graphics, and the Media Manager with the project's images, Lottie files
//! and videos, and the user's palettes. It covers the editor while open;
//! the editor stays loaded underneath.

use crate::preview::library_url;
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::media::{accept, of_category};

/// Media Manager tabs, in FRAME's order (Plates and Fonts come later).
const TABS: &[(&str, &str)] = &[
    ("images", "Images"),
    ("fonts", "Fonts"),
    ("palettes", "Palettes"),
    ("components", "Components"),
    ("lottie", "Lottie"),
    ("videos", "Videos (exp.)"),
];

#[component]
pub fn ProjectView(ui: Ui) -> Element {
    let mut open = ui.project_view;
    let mut tab = use_signal(|| "palettes");
    // Opening the view refreshes the open graphic's thumbnail.
    use_effect(move || {
        if open() {
            spawn(async move { ui.capture_thumbnail().await });
        }
    });
    if !open() {
        return rsx! {};
    }
    let (name, graphics, current) = match ui.editor.read().as_ref() {
        Some(e) => (
            e.project.name().to_string(),
            e.project
                .media
                .pack
                .graphics
                .iter()
                .map(|g| {
                    let thumb = g.thumbnail.as_ref().map(|t| {
                        format!("data:image/jpeg;base64,{}", crate::export::encode_base64(t))
                    });
                    (g.name.clone(), thumb)
                })
                .collect::<Vec<_>>(),
            Some(e.graphic),
        ),
        None => (String::new(), Vec::new(), None),
    };
    let has = !graphics.is_empty();
    let count = graphics.len();
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
                UpdateToggle {}
                button { class: "active", title: "Back to the editor (Tab)", onclick: move |_| open.set(false), "Back to editor" }
            }
            div { class: "project-body",
                aside { class: "project-graphics",
                    div { class: "lib-section-head",
                        h3 { "Graphics" }
                        if has {
                            button { title: "A new empty graphic", onclick: move |_| ui.add_graphic(), "+ New" }
                        }
                    }
                    if has {
                        div { class: "row-buttons wrap",
                            button {
                                title: "Import a .framepack.zip (graphic + images + fonts) into this project",
                                onclick: move |_| ui.import_graphic_pack(),
                                "Import Pack…"
                            }
                            button {
                                title: "All graphics as one OGraf suite ZIP (default)",
                                onclick: move |_| crate::export::export_suite(ui, true),
                                "OGraf Suite…"
                            }
                            button {
                                title: "All graphics as one SPX suite ZIP",
                                onclick: move |_| crate::export::export_suite(ui, false),
                                "SPX Suite…"
                            }
                        }
                    }
                    if graphics.is_empty() {
                        p { class: "dim", "Open a project to see its graphics." }
                    }
                    for (i , (g , thumb)) in graphics.into_iter().enumerate() {
                        GraphicCard {
                            ui,
                            index: i,
                            name: g,
                            thumb,
                            current: current == Some(i),
                            last: i + 1 == count,
                            only: count == 1,
                        }
                    }
                }
                main { class: "media-manager",
                    div { class: "lib-head",
                        h3 { "Media Manager" }
                        span { class: "dim", "The project's images, Lottie and video files; your palettes and components" }
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
                        } else if current_tab == "components" {
                            crate::components::ComponentsTab { ui }
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
    // Fonts show a sample in their own face (FRAME's font tiles).
    let faces: Vec<(String, String)> = if category == "fonts" {
        ed.project
            .media
            .fonts
            .iter()
            .map(|f| {
                (
                    framecraft_core::media::font_family(f),
                    library_url(&f.source),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let face_css: String = faces
        .iter()
        .map(|(family, url)| {
            format!(
                "@font-face {{ font-family: 'fc-pv-{}'; src: url('{url}'); }}\n",
                family.replace('\'', "")
            )
        })
        .collect();
    // (url, file name, sample font family for font files)
    let files: Vec<(String, String, String)> = of_category(&ed.project.media.pack, category)
        .into_iter()
        .map(|e| {
            let name = e
                .original_name
                .clone()
                .unwrap_or_else(|| e.url.rsplit('/').next().unwrap_or("").to_string());
            let lib = library_url(&e.url);
            let face = faces
                .iter()
                .find(|(_, u)| *u == lib)
                .map(|(f, _)| format!("fc-pv-{}", f.replace('\'', "")))
                .unwrap_or_default();
            (e.url.clone(), name, face)
        })
        .collect();
    drop(editor);
    rsx! {
        if !face_css.is_empty() {
            style { "{face_css}" }
        }
        section { class: "lib-section",
            div { class: "lib-section-head",
                p { class: "dim",
                    if category == "fonts" {
                        "{files.len()} font(s). Use one by typing or picking its family name in a text layer's Font field; exports include the fonts that layers use."
                    } else {
                        "{files.len()} file(s). Saving the project keeps them in its media folder."
                    }
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
                for (url , name , face) in files {
                    div { class: "media-tile", title: "{name}",
                        div { class: "media-thumb",
                            if category == "images" {
                                img { src: "{library_url(&url)}", loading: "lazy", draggable: "false" }
                            } else if category == "fonts" {
                                span {
                                    class: "font-sample",
                                    style: "font-family: '{face}', sans-serif;",
                                    "Aa 123"
                                }
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

/// One graphic of the project (FRAME's project page card): open, rename,
/// clone, move, export as a graphic pack, delete.
#[component]
fn GraphicCard(
    ui: Ui,
    index: usize,
    name: String,
    thumb: Option<String>,
    current: bool,
    last: bool,
    only: bool,
) -> Element {
    let mut open = ui.project_view;
    let mut renaming = use_signal(|| false);
    let mut confirm = use_signal(|| false);
    let i = index;
    rsx! {
        article { class: if current { "graphic-card active" } else { "graphic-card" },
            button {
                class: "graphic-thumb",
                title: "Open in editor",
                onclick: move |_| {
                    ui.select_graphic(i);
                    open.set(false);
                },
                if let Some(src) = thumb {
                    img { src: "{src}", alt: "", draggable: "false" }
                } else {
                    span { class: "dim", "No thumbnail" }
                }
            }
            if renaming() {
                input {
                    class: "graphic-rename",
                    r#type: "text",
                    value: "{name}",
                    autofocus: true,
                    onchange: move |evt| {
                        ui.rename_graphic(i, &evt.value());
                        renaming.set(false);
                    },
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            renaming.set(false);
                        }
                    },
                }
            } else {
                span {
                    class: "graphic-name",
                    title: "Double-click to rename",
                    ondoubleclick: move |_| renaming.set(true),
                    "{name}"
                    if current {
                        span { class: "badge", "Open" }
                    }
                }
            }
            div { class: "graphic-actions",
                button { class: "key-button", title: "Rename", onclick: move |_| renaming.set(true), "✎" }
                button { class: "key-button", title: "Clone", onclick: move |_| ui.clone_graphic(i), "⧉" }
                button {
                    class: "key-button",
                    title: "Move up",
                    disabled: i == 0,
                    onclick: move |_| ui.move_graphic(i, i - 1),
                    "▲"
                }
                button {
                    class: "key-button",
                    title: "Move down",
                    disabled: last,
                    onclick: move |_| ui.move_graphic(i, i + 1),
                    "▼"
                }
                button {
                    class: "key-button",
                    title: "Export this graphic as a FRAME graphic pack (.framepack.zip)",
                    onclick: move |_| ui.export_graphic_pack(i),
                    "⇩"
                }
                if confirm() {
                    button {
                        class: "danger",
                        title: "Delete this graphic for good (not undoable)",
                        onclick: move |_| {
                            confirm.set(false);
                            ui.delete_graphic(i);
                        },
                        "Delete?"
                    }
                    button { onclick: move |_| confirm.set(false), "Keep" }
                } else {
                    button {
                        class: "key-button",
                        title: if only { "A project keeps at least one graphic" } else { "Delete" },
                        disabled: only,
                        onclick: move |_| confirm.set(true),
                        "✕"
                    }
                }
            }
        }
    }
}

/// "Check for updates": FrameCraft asks GitHub for a newer release at
/// start (at most once a day).
#[component]
fn UpdateToggle() -> Element {
    let mut on = use_signal(crate::updates::enabled);
    rsx! {
        label {
            class: "update-toggle dim",
            title: "At start (at most once a day) FrameCraft asks GitHub whether a newer release exists, and says so in the status bar. Nothing about you or your projects is sent.",
            input {
                r#type: "checkbox",
                checked: on(),
                onchange: move |_| {
                    let next = !on();
                    on.set(next);
                    crate::updates::set_enabled(next);
                },
            }
            "Check for updates"
        }
    }
}
