// SPDX-License-Identifier: AGPL-3.0-or-later
//! Palettes and stock gradients (FRAME's stock palette / gradient pickers
//! and palette library): pickers opened from color and gradient fields, and
//! the Palettes dialog that manages the user's palettes.

use crate::project::{stock_gradients, stock_palettes};
use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::model::RampColor;
use framecraft_core::palettes::{MAX_COLORS, MAX_PALETTES, Palette, PaletteColor, swatch_css};

/// CSS for a gradient preview strip.
fn strip(stops: &[RampColor]) -> String {
    let parts: Vec<String> = stops
        .iter()
        .map(|s| format!("{} {}%", s.color, (s.offset * 1000.0).round() / 10.0))
        .collect();
    format!("background: linear-gradient(90deg, {});", parts.join(", "))
}

/// Built-in stock palettes. `on_color` picks one color; `on_palette`,
/// when given, also offers taking the whole palette.
#[component]
pub fn StockPaletteDialog(
    on_close: EventHandler<()>,
    on_color: Option<EventHandler<PaletteColor>>,
    on_palette: Option<EventHandler<Palette>>,
) -> Element {
    let mut query = use_signal(String::new);
    let q = query().trim().to_lowercase();
    let palettes: Vec<Palette> = stock_palettes()
        .iter()
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.colors.iter().any(|c| {
                    c.hex.to_lowercase().contains(&q)
                        || c.label.as_deref().unwrap_or("").to_lowercase().contains(&q)
                })
        })
        .cloned()
        .collect();
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close(()),
            div {
                class: "modal pick-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    span { class: "panel-title", "Stock palettes" }
                    input {
                        r#type: "search",
                        placeholder: "Search",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                    button { onclick: move |_| on_close(()), "Close" }
                }
                if palettes.is_empty() {
                    p { class: "dim", "No stock palettes found (they come from FRAME's static files)." }
                }
                div { class: "pick-list",
                    for p in palettes {
                        div { class: "pick-row",
                            div { class: "pick-name",
                                span { "{p.name}" }
                                if let Some(take) = on_palette {
                                    {
                                        let whole = p.clone();
                                        rsx! {
                                            button { onclick: move |_| take(whole.clone()), "Add to my palettes" }
                                        }
                                    }
                                }
                            }
                            div { class: "swatches",
                                for c in p.colors.clone() {
                                    {
                                        let css = swatch_css(&c.hex, c.alpha);
                                        let tip = [c.label.clone().unwrap_or_default(), c.hex.clone()]
                                            .iter()
                                            .filter(|s| !s.is_empty())
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join(" · ");
                                        rsx! {
                                            button {
                                                class: "swatch-btn",
                                                title: "{tip}",
                                                disabled: on_color.is_none(),
                                                style: "--sw: {css};",
                                                onclick: move |_| {
                                                    if let Some(pick) = on_color {
                                                        pick(c.clone());
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Built-in stock gradients; picking one replaces the ramp's stops.
#[component]
pub fn StockGradientDialog(
    on_close: EventHandler<()>,
    on_pick: EventHandler<Vec<RampColor>>,
) -> Element {
    let mut query = use_signal(String::new);
    let q = query().trim().to_lowercase();
    let list: Vec<_> = stock_gradients()
        .iter()
        .filter(|g| q.is_empty() || g.name.to_lowercase().contains(&q))
        .cloned()
        .collect();
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close(()),
            div {
                class: "modal pick-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    span { class: "panel-title", "Stock gradients" }
                    input {
                        r#type: "search",
                        placeholder: "Search",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                    button { onclick: move |_| on_close(()), "Close" }
                }
                if list.is_empty() {
                    p { class: "dim", "No stock gradients found (they come from FRAME's static files)." }
                }
                div { class: "gradient-grid",
                    for g in list {
                        {
                            let style = strip(&g.stops);
                            let stops = g.stops.clone();
                            rsx! {
                                button { class: "gradient-tile", onclick: move |_| on_pick(stops.clone()),
                                    div { class: "gradient-strip", style: "{style}" }
                                    span { "{g.name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The Palettes tab of the Project view (FRAME's dashboard → Palettes):
/// your palettes (set active, rename, delete, edit colors) and the stock
/// palettes (Use copies one into your palettes and makes it active).
/// Saved to the data folder on every change.
#[component]
pub fn PalettesTab(ui: Ui) -> Element {
    let all = ui.palettes.read().clone();
    let active = all.active().map(|p| p.id.clone());
    let count = all.palettes.len();
    let full = count >= MAX_PALETTES;
    let edit = move |f: Box<dyn FnOnce(&mut framecraft_core::palettes::UserPalettes)>| {
        let mut next = ui.palettes.peek().clone();
        f(&mut next);
        ui.set_palettes(next);
    };
    rsx! {
        section { class: "lib-section",
            div { class: "lib-section-head",
                h3 { "Your palettes" }
                button {
                    disabled: full,
                    title: "A new empty palette",
                    onclick: move |_| {
                        edit(
                            Box::new(|u| {
                                u.add_copy(
                                    &Palette {
                                        id: String::new(),
                                        name: "Palette".into(),
                                        colors: Vec::new(),
                                    },
                                );
                            }),
                        )
                    },
                    "+ New palette"
                }
            }
            p { class: "dim",
                "The active palette shows in the editor's color pickers and colors charts. Up to {MAX_PALETTES} palettes of {MAX_COLORS} colors."
            }
            if count == 0 {
                p { class: "dim", "No palettes yet. Create one, or use a stock palette below." }
            }
            div { class: "palette-grid",
                for p in all.palettes.clone() {
                    {
                        let id = p.id.clone();
                        let is_active = active.as_deref() == Some(id.as_str());
                        let (i1, i2, i3, i4) = (id.clone(), id.clone(), id.clone(), id.clone());
                        let n = p.colors.len();
                        rsx! {
                            article { class: if is_active { "palette-card active" } else { "palette-card" },
                                div { class: "pick-name",
                                    input {
                                        r#type: "text",
                                        value: "{p.name}",
                                        onchange: move |evt| {
                                            let (id, name) = (i2.clone(), evt.value());
                                            edit(
                                                Box::new(move |u| {
                                                    if let Some(p) = u.palettes.iter_mut().find(|p| p.id == id) {
                                                        p.name = name.trim().to_string();
                                                    }
                                                }),
                                            )
                                        },
                                    }
                                    if is_active {
                                        span { class: "badge", title: "Used in the editor color pickers", "Active" }
                                    } else {
                                        button {
                                            title: "Use this palette in the editor",
                                            onclick: move |_| {
                                                let id = i1.clone();
                                                edit(Box::new(move |u| u.active = Some(id)))
                                            },
                                            "Set active"
                                        }
                                    }
                                    button {
                                        class: "key-button",
                                        title: "Delete palette",
                                        onclick: move |_| {
                                            let id = i3.clone();
                                            edit(Box::new(move |u| u.palettes.retain(|p| p.id != id)))
                                        },
                                        "✕"
                                    }
                                }
                                div { class: "swatches",
                                    for (ci , c) in p.colors.iter().cloned().enumerate() {
                                        {
                                            let (ia, ib) = (id.clone(), id.clone());
                                            let tip = c.label.clone().unwrap_or_else(|| c.hex.clone());
                                            rsx! {
                                                span { class: "swatch-edit", title: "{tip}",
                                                    crate::properties::ColorButton {
                                                        ui,
                                                        value: swatch_css(&c.hex, c.alpha),
                                                        title: "{tip}",
                                                        on_change: move |css: String| {
                                                            // Palette colors keep hex and alpha (0–100) apart.
                                                            let Some(rgba) = framecraft_core::color::parse(&css) else {
                                                                return;
                                                            };
                                                            let id = ia.clone();
                                                            edit(
                                                                Box::new(move |u| {
                                                                    if let Some(p) = u.palettes.iter_mut().find(|p| p.id == id) {
                                                                        p.colors[ci].hex = rgba.hex().to_uppercase();
                                                                        p.colors[ci].alpha = (rgba.a * 100.0).round().clamp(0.0, 100.0);
                                                                    }
                                                                }),
                                                            )
                                                        },
                                                    }
                                                    button {
                                                        class: "swatch-x",
                                                        title: "Remove color",
                                                        onclick: move |_| {
                                                            let id = ib.clone();
                                                            edit(
                                                                Box::new(move |u| {
                                                                    if let Some(p) = u.palettes.iter_mut().find(|p| p.id == id) {
                                                                        p.colors.remove(ci);
                                                                    }
                                                                }),
                                                            )
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if n < MAX_COLORS {
                                        button {
                                            class: "swatch-add",
                                            title: "Add color",
                                            onclick: move |_| {
                                                let id = i4.clone();
                                                edit(
                                                    Box::new(move |u| {
                                                        if let Some(p) = u.palettes.iter_mut().find(|p| p.id == id) {
                                                            p.colors
                                                                .push(PaletteColor {
                                                                    hex: "#FFFFFF".into(),
                                                                    alpha: 100.0,
                                                                    label: None,
                                                                });
                                                        }
                                                    }),
                                                )
                                            },
                                            "+"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        section { class: "lib-section",
            div { class: "lib-section-head",
                div {
                    h3 { "Stock palettes" }
                    p { class: "dim",
                        "Built-in presets. Use copies one into your palettes and sets it active (counts toward the {MAX_PALETTES} palette limit)."
                    }
                }
                if full {
                    p { class: "dim", "Limit reached — delete a palette to use stock." }
                }
            }
            div { class: "palette-grid",
                for stock in stock_palettes().iter().cloned() {
                    {
                        let take = stock.clone();
                        rsx! {
                            article { class: "palette-card",
                                div { class: "pick-name",
                                    span { title: "{stock.name}", "{stock.name}" }
                                    button {
                                        disabled: full,
                                        title: "Copy into your palettes and set active",
                                        onclick: move |_| {
                                            let p = take.clone();
                                            edit(
                                                Box::new(move |u| {
                                                    u.add_copy(&p);
                                                }),
                                            )
                                        },
                                        "Use"
                                    }
                                }
                                div { class: "stock-strip",
                                    for c in stock.colors.iter().take(MAX_COLORS) {
                                        div {
                                            style: "--sw: {swatch_css(&c.hex, c.alpha)};",
                                            title: "{c.label.clone().unwrap_or_else(|| c.hex.clone())}",
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
