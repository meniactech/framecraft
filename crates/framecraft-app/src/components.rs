// SPDX-License-Identifier: AGPL-3.0-or-later
//! Components (FRAME's component library): Outliner → right-click → Make
//! Component saves the layer with everything under it; Add → Insert
//! Component… puts a copy into the graphic; the Project view lists them.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::components::{
    ComponentData, InsertComponent, StoredComponent, UserComponents, rename_root,
};

/// Where the user's components are saved.
pub fn components_path() -> Option<std::path::PathBuf> {
    crate::project::data_dir().map(|d| d.join("components.json"))
}

/// The pending Make Component (asks for its name).
#[derive(Clone, PartialEq)]
pub struct PendingComponent {
    pub name: String,
    pub data: ComponentData,
}

/// Starts Make Component for a layer: the name dialog opens.
pub fn make_component(ui: Ui, id: &str) {
    let made = ui
        .editor
        .peek()
        .as_ref()
        .and_then(|e| framecraft_core::components::from_selection(e.doc(), &[id.to_string()]));
    let mut pending = ui.pending_component;
    pending.set(made.map(|(name, data)| PendingComponent { name, data }));
}

fn save_components(ui: Ui, next: UserComponents) {
    if let Some(path) = components_path()
        && let Err(e) = next.save(&path)
    {
        let mut status = ui.status;
        status.set(format!("Could not save components: {e}"));
    }
    let mut lib = ui.user_components;
    lib.set(next);
}

/// Asks for the new component's name, then saves it.
#[component]
pub fn ComponentNameDialog(ui: Ui) -> Element {
    let mut pending = ui.pending_component;
    let mut name = use_signal(String::new);
    // A new Make Component starts from its suggested name.
    let suggested = pending.read().as_ref().map(|p| p.name.clone());
    use_effect(use_reactive!(|suggested| {
        if let Some(n) = suggested {
            name.set(n);
        }
    }));
    let save = use_callback(move |entered: String| {
        let Some(p) = pending.peek().clone() else {
            return;
        };
        let n = if entered.trim().is_empty() {
            p.name.clone()
        } else {
            entered.trim().to_string()
        };
        let mut data = p.data;
        rename_root(&mut data, &n);
        let mut next = ui.user_components.peek().clone();
        next.add(&n, data);
        save_components(ui, next);
        let mut status = ui.status;
        status.set(format!(
            "Saved component “{n}”. Insert it with + Add → Insert Component…"
        ));
        pending.set(None);
    });
    let Some(count) = pending.read().as_ref().map(|p| p.data.items.len()) else {
        return rsx! {};
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| pending.set(None),
            div {
                class: "modal motion-dialog small",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Make Component" }
                        p { class: "dim",
                            "Saves the layer with everything under it and its keyframes ({count} layers) to your components, for any graphic."
                        }
                    }
                }
                label { class: "motion-field",
                    span { class: "dim", "Component name" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |evt| name.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter {
                                save(name());
                            }
                        },
                    }
                }
                div { class: "motion-foot",
                    span {}
                    button { onclick: move |_| pending.set(None), "Cancel" }
                    button { class: "active", onclick: move |_| save(name()), "Save component" }
                }
            }
        }
    }
}

/// The saved components, searchable; `on_insert` makes the rows insert.
#[component]
fn ComponentList(ui: Ui, on_insert: Option<EventHandler<StoredComponent>>) -> Element {
    let mut query = use_signal(String::new);
    let q = query().trim().to_lowercase();
    let all = ui.user_components.read().components.clone();
    let total = all.len();
    let list: Vec<StoredComponent> = all
        .into_iter()
        .filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q))
        .collect();
    rsx! {
        div { class: "motion-filters",
            input {
                r#type: "search",
                placeholder: "Search components",
                value: "{query}",
                oninput: move |evt| query.set(evt.value()),
            }
        }
        if total == 0 {
            p { class: "dim", "No components yet. In the Outliner, right-click a layer → Make Component." }
        } else if list.is_empty() {
            p { class: "dim", "No components match." }
        }
        div { class: "motion-grid",
            for c in list {
                {
                    let (insert, del) = (c.clone(), c.id.clone());
                    let count = c.item_count();
                    rsx! {
                        div { class: "motion-card",
                            button {
                                class: "motion-pick",
                                disabled: on_insert.is_none(),
                                title: if on_insert.is_some() { "Insert a copy into this graphic" } else { "" },
                                onclick: move |_| {
                                    if let Some(h) = on_insert {
                                        h(insert.clone());
                                    }
                                },
                                span { class: "motion-name", "{c.name}" }
                                span { class: "dim", "{count} layer(s)" }
                            }
                            button {
                                class: "motion-delete",
                                title: "Delete component",
                                onclick: move |_| {
                                    let mut next = ui.user_components.peek().clone();
                                    next.components.retain(|x| x.id != del);
                                    save_components(ui, next);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Add → Insert Component…
#[component]
pub fn ComponentLibraryDialog(ui: Ui) -> Element {
    let mut open = ui.component_library;
    if !open() {
        return rsx! {};
    }
    let insert = move |c: StoredComponent| {
        open.set(false);
        let cmd = InsertComponent::new(&c.name, &c.data);
        let root = cmd.roots.first().cloned();
        ui.execute(&cmd);
        if root.is_some() {
            ui.select_layer(root);
        }
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| open.set(false),
            div {
                class: "modal motion-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Components" }
                        p { class: "dim",
                            "Insert a saved layer tree into this graphic (a copy). Create one in the Outliner: right-click → Make Component."
                        }
                    }
                    button { onclick: move |_| open.set(false), "Close" }
                }
                div { class: "motion-list",
                    ComponentList { ui, on_insert: insert }
                }
            }
        }
    }
}

/// The Project view's Components tab.
#[component]
pub fn ComponentsTab(ui: Ui) -> Element {
    rsx! {
        section { class: "lib-section",
            p { class: "dim",
                "Your components, for every project. Make one in the Outliner (right-click → Make Component); insert with + Add → Insert Component…."
            }
            ComponentList { ui, on_insert: None }
        }
    }
}
