// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Script tab and script editor (FRAME's Js tab and Script editor):
//! `onLoad`, functions called from Script cues, and globals
//! (`window.__ionframeVars`), shipped as `template.js`. Every change is one
//! undoable [`SetScript`].

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::model::{TemplateFunction, TemplateGlobal};
use framecraft_core::script::{
    ON_LOAD, SetScript, new_function, new_global, next_name, sanitize_name, starter_functions,
};

fn current(ui: Ui) -> (Vec<TemplateFunction>, Vec<TemplateGlobal>) {
    ui.editor.read().as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |e| {
            let d = e.doc();
            (
                framecraft_core::script::ensure_on_load(&d.template_functions),
                d.template_globals.clone(),
            )
        },
    )
}

fn write(ui: Ui, functions: Vec<TemplateFunction>, globals: Vec<TemplateGlobal>) {
    ui.execute(&SetScript { functions, globals });
}

/// Properties → Script: a summary and the editor button.
#[component]
pub fn ScriptTab(ui: Ui) -> Element {
    let (fns, globals) = current(ui);
    let on_load = fns.first().is_some_and(|f| !f.body.trim().is_empty());
    let cues = fns
        .iter()
        .filter(|f| sanitize_name(&f.name) != ON_LOAD)
        .count();
    let mut open = ui.script_editor;
    rsx! {
        h3 { "Script" }
        p { class: "dim",
            "Edit onLoad, cue functions and globals in a large editor. They ship with exports as template.js."
        }
        div { class: "script-summary",
            span { class: "dim", "onLoad" }
            span { if on_load { "Set" } else { "Empty" } }
            span { class: "dim", "Cue functions" }
            span { "{cues}" }
            span { class: "dim", "Globals" }
            span { "{globals.len()}" }
        }
        button { class: "active", onclick: move |_| open.set(true), "Open script editor" }
    }
}

#[component]
pub fn ScriptEditor(ui: Ui) -> Element {
    let mut open = ui.script_editor;
    let mut selected = use_signal(|| ON_LOAD.to_string());
    if !open() {
        return rsx! {};
    }
    let (fns, globals) = current(ui);
    // Selection by id; fall back to onLoad.
    let sel_id = fns
        .iter()
        .find(|f| f.id == selected())
        .or_else(|| fns.first())
        .map(|f| f.id.clone())
        .unwrap_or_default();
    let sel = fns.iter().find(|f| f.id == sel_id).cloned();
    let sel_is_load = sel
        .as_ref()
        .is_some_and(|f| sanitize_name(&f.name) == ON_LOAD);
    let cue_count = fns
        .iter()
        .filter(|f| sanitize_name(&f.name) != ON_LOAD)
        .count();
    let (g_fns, g_globals) = (fns.clone(), globals.clone());

    let add_function = {
        let (fns, globals) = (fns.clone(), globals.clone());
        move |_| {
            let taken: Vec<String> = fns.iter().map(|f| f.name.clone()).collect();
            let f = new_function(&next_name("fn", &taken), None);
            selected.set(f.id.clone());
            let mut next = fns.clone();
            next.push(f);
            write(ui, next, globals.clone());
        }
    };
    let add_starter = {
        let globals = globals.clone();
        move |_| {
            let s = starter_functions();
            if let Some(f) = s.iter().find(|f| f.name != ON_LOAD) {
                selected.set(f.id.clone());
            }
            write(ui, s, globals.clone());
        }
    };
    let add_global = {
        let (fns, globals) = (fns.clone(), globals.clone());
        move |_| {
            let taken: Vec<String> = globals.iter().map(|g| g.name.clone()).collect();
            let mut next = globals.clone();
            next.push(new_global(&taken));
            write(ui, fns.clone(), next);
        }
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| open.set(false),
            div {
                class: "modal script-dialog",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        span { class: "panel-title", "Script editor" }
                        p { class: "dim",
                            "onLoad runs when the template loads. Other functions are called from Script cues. Use window.__ionframeVars for globals."
                        }
                    }
                    button { onclick: move |_| open.set(false), "Close" }
                }
                div { class: "script-body",
                    // Functions
                    div { class: "script-col",
                        div { class: "lib-section-head",
                            span { class: "panel-title", "Functions" }
                            div {
                                if cue_count == 0 {
                                    button { title: "Add starter functions", onclick: add_starter, "Starter" }
                                }
                                button { title: "Add function", onclick: add_function, "+" }
                            }
                        }
                        div { class: "script-fns",
                            for f in fns.iter().cloned() {
                                {
                                    let id = f.id.clone();
                                    let is_load = sanitize_name(&f.name) == ON_LOAD;
                                    rsx! {
                                        button {
                                            class: if f.id == sel_id { "script-fn active" } else { "script-fn" },
                                            onclick: move |_| selected.set(id.clone()),
                                            span { if f.name.is_empty() { "…" } else { "{f.name}" } }
                                            if is_load {
                                                span { class: "badge", "load" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // The selected function
                    div { class: "script-col grow",
                        if let Some(f) = sel {
                            {
                                let (fa, ga) = (g_fns.clone(), g_globals.clone());
                                let (fb, gb) = (g_fns.clone(), g_globals.clone());
                                let (fc, gc) = (g_fns.clone(), g_globals.clone());
                                let (ia, ib, ic) = (f.id.clone(), f.id.clone(), f.id.clone());
                                rsx! {
                                    div { class: "script-name",
                                        label {
                                            span { class: "dim", "Name" }
                                            input {
                                                r#type: "text",
                                                value: "{f.name}",
                                                spellcheck: "false",
                                                disabled: sel_is_load,
                                                onchange: move |evt| {
                                                    let name = sanitize_name(&evt.value());
                                                    if name.is_empty() || name == ON_LOAD {
                                                        return;
                                                    }
                                                    let next = fa
                                                        .iter()
                                                        .cloned()
                                                        .map(|mut x| {
                                                            if x.id == ia {
                                                                x.name = name.clone();
                                                            }
                                                            x
                                                        })
                                                        .collect();
                                                    write(ui, next, ga.clone());
                                                },
                                            }
                                        }
                                        if !sel_is_load {
                                            button {
                                                onclick: move |_| {
                                                    let next = fc.iter().filter(|x| x.id != ic).cloned().collect();
                                                    selected.set(ON_LOAD.to_string());
                                                    write(ui, next, gc.clone());
                                                },
                                                "Delete"
                                            }
                                        }
                                    }
                                    code { class: "script-sig", "{f.name}(ctx) {{" }
                                    textarea {
                                        class: "script-code",
                                        spellcheck: "false",
                                        autocomplete: "off",
                                        value: "{f.body}",
                                        // Written on blur: one undo step per edit.
                                        onchange: move |evt| {
                                            let body = evt.value();
                                            let next = fb
                                                .iter()
                                                .cloned()
                                                .map(|mut x| {
                                                    if x.id == ib {
                                                        x.body = body.clone();
                                                    }
                                                    x
                                                })
                                                .collect();
                                            write(ui, next, gb.clone());
                                        },
                                    }
                                    code { class: "script-sig", "}}" }
                                }
                            }
                        } else {
                            p { class: "dim", "Select or add a function." }
                        }
                    }
                    // Globals
                    div { class: "script-col",
                        div { class: "lib-section-head",
                            span { class: "panel-title", "Globals" }
                            button { title: "Add global", onclick: add_global, "+" }
                        }
                        code { class: "dim", "window.__ionframeVars.name" }
                        if globals.is_empty() {
                            p { class: "dim", "No globals yet." }
                        }
                        for (gi , g) in globals.iter().cloned().enumerate() {
                            {
                                let edit = {
                                    let (fns, globals) = (g_fns.clone(), g_globals.clone());
                                    move |f: Box<dyn FnOnce(&mut TemplateGlobal)>| {
                                        let mut next = globals.clone();
                                        if let Some(x) = next.get_mut(gi) {
                                            f(x);
                                        }
                                        write(ui, fns.clone(), next);
                                    }
                                };
                                let (e1, e2, e3) = (edit.clone(), edit.clone(), edit.clone());
                                let remove = {
                                    let (fns, globals) = (g_fns.clone(), g_globals.clone());
                                    move |_| {
                                        let mut next = globals.clone();
                                        next.remove(gi);
                                        write(ui, fns.clone(), next);
                                    }
                                };
                                let is_true = matches!(g.value.trim().to_lowercase().as_str(), "true" | "1" | "yes");
                                rsx! {
                                    div { class: "script-global",
                                        div { class: "script-name",
                                            input {
                                                r#type: "text",
                                                value: "{g.name}",
                                                spellcheck: "false",
                                                onchange: move |evt| {
                                                    let v = evt.value();
                                                    e1(Box::new(move |x| {
                                                        let n = sanitize_name(&v);
                                                        if !n.is_empty() {
                                                            x.name = n;
                                                        }
                                                    }))
                                                },
                                            }
                                            button { class: "key-button", title: "Remove", onclick: remove, "×" }
                                        }
                                        div { class: "script-name",
                                            select {
                                                onchange: move |evt| {
                                                    let v = evt.value();
                                                    e2(Box::new(move |x| x.value_kind = v))
                                                },
                                                for k in ["number", "string", "boolean"] {
                                                    option { value: k, selected: g.value_kind == k, "{k}" }
                                                }
                                            }
                                            if g.value_kind == "boolean" {
                                                select {
                                                    onchange: move |evt| {
                                                        let v = evt.value();
                                                        e3(Box::new(move |x| x.value = v))
                                                    },
                                                    option { value: "false", selected: !is_true, "false" }
                                                    option { value: "true", selected: is_true, "true" }
                                                }
                                            } else {
                                                input {
                                                    r#type: "text",
                                                    value: "{g.value}",
                                                    spellcheck: "false",
                                                    onchange: move |evt| {
                                                        let v = evt.value();
                                                        e3(Box::new(move |x| x.value = v))
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
}
