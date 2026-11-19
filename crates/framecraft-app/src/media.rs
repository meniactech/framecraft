// SPDX-License-Identifier: AGPL-3.0-or-later
//! The media library dialog (FRAME's Media / Lottie / Video Library): the
//! project's files of one kind, an Import button that copies files from
//! disk into the project, and for Lottie the built-in Noto emoji stock.

use crate::preview::library_url;
use crate::project::stock_lottie;
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::media::{accept, of_category};

#[derive(Clone, PartialEq)]
struct Tile {
    url: String,
    name: String,
    /// What to show: an image URL (thumbnail), or none (name only).
    thumb: Option<String>,
}

#[component]
pub fn MediaDialog(ui: Ui) -> Element {
    let mut query = use_signal(String::new);
    // Lottie: the project's files or the stock emoji.
    let mut stock = use_signal(|| false);
    let stock_list = use_hook(stock_lottie);
    let Some(pick) = ui.media.read().clone() else {
        return rsx! {};
    };
    let category = pick.category;
    let title = match category {
        "lottie" => "Lottie Library",
        "videos" => "Video Library",
        _ => "Media Library",
    };
    let q = query().trim().to_lowercase();
    let showing_stock = category == "lottie" && stock();
    let tiles: Vec<Tile> = if showing_stock {
        stock_list
            .iter()
            .map(|s| Tile {
                url: s.url.clone(),
                name: s.name.clone(),
                thumb: s.thumb.as_deref().map(library_url),
            })
            .collect()
    } else {
        let editor = ui.editor.read();
        let Some(ed) = editor.as_ref() else {
            return rsx! {};
        };
        of_category(&ed.project.media.pack, category)
            .into_iter()
            .map(|e| Tile {
                url: e.url.clone(),
                name: e
                    .original_name
                    .clone()
                    .unwrap_or_else(|| e.url.rsplit('/').next().unwrap_or("").to_string()),
                thumb: (category == "images").then(|| library_url(&e.url)),
            })
            .collect()
    };
    let total = tiles.len();
    let tiles: Vec<Tile> = tiles
        .into_iter()
        .filter(|t| q.is_empty() || t.name.to_lowercase().contains(&q))
        .collect();
    let mut close = ui.media;

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| close.set(None),
            div {
                class: "modal media-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    span { class: "panel-title", "{title}" }
                    if category == "lottie" {
                        div { class: "seg",
                            button {
                                class: if !showing_stock { "active" } else { "" },
                                onclick: move |_| stock.set(false),
                                "Project"
                            }
                            button {
                                class: if showing_stock { "active" } else { "" },
                                onclick: move |_| stock.set(true),
                                "Noto emoji"
                            }
                        }
                    }
                    input {
                        r#type: "search",
                        placeholder: "Search",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                    if !showing_stock {
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
                    button { onclick: move |_| close.set(None), "Close" }
                }
                p { class: "dim count",
                    if showing_stock {
                        "Google Noto Animated Emoji. {tiles.len()} of {total}."
                    } else if total == 0 {
                        "No files in this project yet. Import some from disk."
                    } else {
                        "{tiles.len()} of {total} in this project."
                    }
                }
                div { class: "media-grid",
                    for t in tiles {
                        {
                            let (url, name) = (t.url.clone(), t.name.clone());
                            rsx! {
                                button {
                                    class: "media-tile",
                                    title: "{t.name}",
                                    onclick: move |_| ui.pick_media(url.clone(), name.clone()),
                                    div { class: "media-thumb",
                                        match (&t.thumb, category) {
                                            (Some(src), _) => rsx! {
                                                img { src: "{src}", loading: "lazy", draggable: "false" }
                                            },
                                            (None, "videos") => rsx! {
                                                span { class: "media-kind", "▶" }
                                            },
                                            (None, _) => rsx! {
                                                span { class: "media-kind", "{{ }}" }
                                            },
                                        }
                                    }
                                    span { class: "media-name", "{t.name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
