// SPDX-License-Identifier: AGPL-3.0-or-later
//! The menu bar: File | Edit | Layer | View | Timeline | Help. Drawn by the
//! app itself (same look on every system); each item names its shortcut.
//! Open a menu with a click, then hovering the other titles switches
//! between menus; a click outside or Escape closes it.

use crate::state::{PickTarget, Ui};
use dioxus::prelude::*;
use framecraft_core::layers::NewLayer;
use framecraft_core::schema::Tab;

/// What a menu item does.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    ExportPack,
    ProjectView,
    Undo,
    Redo,
    Duplicate,
    Delete,
    SelectNone,
    Add(NewLayer),
    AddImage,
    AddSvg,
    AddMedia(&'static str),
    InsertComponent,
    MakeComponent,
    Zoom(f64),
    Fit,
    Guide(&'static str),
    Play,
    ToStart,
    AutoKey,
    Motions,
    TimelineFit,
    Shortcuts,
    Recent(usize),
    ClearRecent,
    About,
    Link(&'static str),
}

/// A menu item: label, shortcut, enabled, check mark (`None` = no box).
#[derive(Clone, PartialEq)]
enum Item {
    Do(String, &'static str, Act, bool, Option<bool>),
    Sep,
}

fn item(label: impl Into<String>, keys: &'static str, act: Act, on: bool) -> Item {
    Item::Do(label.into(), keys, act, on, None)
}

fn check(label: &'static str, keys: &'static str, act: Act, on: bool, checked: bool) -> Item {
    Item::Do(label.into(), keys, act, on, Some(checked))
}

const REPO: &str = "https://github.com/meniactech/framecraft";

/// The menus as they are now (enabled items, check marks, undo labels).
fn menus(ui: Ui) -> Vec<(&'static str, Vec<Item>)> {
    let (open, selected, undo, redo) = match ui.editor.read().as_ref() {
        Some(e) => (
            true,
            e.selected.clone(),
            e.history().undo_label().map(String::from),
            e.history().redo_label().map(String::from),
        ),
        None => (false, None, None, None),
    };
    let has_sel = selected.is_some();
    let g = (ui.guides)();
    let zoom = (ui.zoom)();
    let mut layers: Vec<Item> = Vec::new();
    for kind in NewLayer::ALL {
        if kind == NewLayer::ImageSlot {
            layers.push(item("Image…", "", Act::AddImage, open));
            layers.push(item("SVG Shape…", "", Act::AddSvg, open));
        }
        let act = match kind {
            NewLayer::Lottie => Act::AddMedia("lottie"),
            NewLayer::Video => Act::AddMedia("videos"),
            k => Act::Add(k),
        };
        let label = match kind {
            NewLayer::Lottie | NewLayer::Video => format!("{}…", kind.label()),
            _ => kind.label().to_string(),
        };
        layers.push(item(label, "", act, open));
    }
    layers.push(Item::Sep);
    layers.push(item("Insert Component…", "", Act::InsertComponent, open));
    layers.push(item("Make Component…", "", Act::MakeComponent, has_sel));

    let zooms = [0.25, 0.5, 0.75, 1.0].map(|z| {
        check(
            match z {
                0.25 => "25 %",
                0.5 => "50 %",
                0.75 => "75 %",
                _ => "100 %",
            },
            "",
            Act::Zoom(z),
            open,
            (zoom - z).abs() < 1e-6,
        )
    });
    let mut view = zooms.to_vec();
    view.extend([
        item("Fit", "", Act::Fit, open),
        Item::Sep,
        check("Safe 10 %", "", Act::Guide("safe10"), true, g.safe10),
        check("Safe 20 %", "", Act::Guide("safe20"), true, g.safe20),
        check("Center +", "", Act::Guide("center"), true, g.center),
        check("Thirds", "", Act::Guide("thirds"), true, g.thirds),
        check("Rulers", "", Act::Guide("rulers"), true, g.rulers),
        check("Snap to guides", "", Act::Guide("snap"), true, g.snap),
        Item::Sep,
        item("Project view", "Tab", Act::ProjectView, open),
    ]);

    vec![
        (
            "File",
            vec![
                item("New…", "Ctrl+N", Act::New, true),
                item("Open…", "Ctrl+O", Act::Open, true),
            ]
            .into_iter()
            .chain(recent_items(ui))
            .chain([
                Item::Sep,
                item("Save", "Ctrl+S", Act::Save, open),
                item("Save as…", "Ctrl+Shift+S", Act::SaveAs, open),
                Item::Sep,
                item("Export…", "", Act::Export, open),
                item("Export FRAME pack…", "", Act::ExportPack, open),
                Item::Sep,
                item("Project view", "Tab", Act::ProjectView, open),
            ])
            .collect(),
        ),
        (
            "Edit",
            vec![
                item(
                    undo.map_or("Undo".into(), |l| format!("Undo {l}")),
                    "Ctrl+Z",
                    Act::Undo,
                    open,
                ),
                item(
                    redo.map_or("Redo".into(), |l| format!("Redo {l}")),
                    "Ctrl+Shift+Z",
                    Act::Redo,
                    open,
                ),
                Item::Sep,
                item("Duplicate", "Ctrl+D", Act::Duplicate, has_sel),
                item("Delete", "Del", Act::Delete, has_sel),
                item("Select none", "Esc", Act::SelectNone, has_sel),
            ],
        ),
        ("Layer", layers),
        ("View", view),
        (
            "Timeline",
            vec![
                item(
                    if (ui.playing)() { "Pause" } else { "Play" },
                    "Space",
                    Act::Play,
                    open,
                ),
                item("To start", "", Act::ToStart, open),
                Item::Sep,
                check("Auto-key", "", Act::AutoKey, open, (ui.auto_key)()),
                item("Motions…", "", Act::Motions, open),
                Item::Sep,
                item("Fit timeline", "", Act::TimelineFit, open),
            ],
        ),
        (
            "Help",
            vec![
                item("Keyboard shortcuts", "", Act::Shortcuts, true),
                item("About FrameCraft", "", Act::About, true),
                Item::Sep,
                item("FrameCraft on GitHub", "", Act::Link(REPO), true),
                item(
                    "Releases and downloads",
                    "",
                    Act::Link("https://github.com/meniactech/framecraft/releases"),
                    true,
                ),
            ],
        ),
    ]
}

/// File menu: the recent projects under Open…, and Clear.
fn recent_items(ui: Ui) -> Vec<Item> {
    let list = (ui.recent)();
    if list.is_empty() {
        return Vec::new();
    }
    let mut items = vec![Item::Sep];
    for (i, p) in list.iter().enumerate() {
        items.push(item(crate::recent::label(p), "", Act::Recent(i), true));
    }
    items.push(item("Clear recent", "", Act::ClearRecent, true));
    items
}

fn run(ui: Ui, act: Act) {
    match act {
        Act::Recent(i) => {
            let path = ui.recent.peek().get(i).cloned();
            if let Some(p) = path {
                ui.open_recent(p);
            }
        }
        Act::ClearRecent => crate::recent::clear(ui),
        Act::New => ui.new_project(),
        Act::Open => ui.open_dialog(),
        Act::Save => ui.save(),
        Act::SaveAs => ui.save_as(),
        Act::Export => {
            let mut tab = ui.tab;
            tab.set(Tab::Export);
        }
        Act::ExportPack => ui.export_pack(),
        Act::ProjectView => {
            let mut open = ui.project_view;
            open.set(true);
        }
        Act::Undo => ui.undo(),
        Act::Redo => ui.redo(),
        Act::Duplicate => ui.duplicate_selected(),
        Act::Delete => ui.delete_selected(),
        Act::SelectNone => ui.select_layer(None),
        Act::Add(kind) => ui.add_layer(kind),
        Act::AddImage => ui.open_media("images", PickTarget::NewLayer),
        Act::AddSvg => ui.open_media("shapes", PickTarget::NewLayer),
        Act::AddMedia(cat) => ui.open_media(cat, PickTarget::NewLayer),
        Act::InsertComponent => {
            let mut open = ui.component_library;
            open.set(true);
        }
        Act::MakeComponent => {
            let id = ui.editor.peek().as_ref().and_then(|e| e.selected.clone());
            if let Some(id) = id {
                crate::components::make_component(ui, &id);
            }
        }
        Act::Zoom(z) => {
            let mut zoom = ui.zoom;
            zoom.set(z);
        }
        Act::Fit => {
            spawn(crate::ui::fit(ui));
        }
        Act::Guide(which) => {
            let mut guides = ui.guides;
            let mut g = *guides.peek();
            match which {
                "safe10" => g.safe10 = !g.safe10,
                "safe20" => g.safe20 = !g.safe20,
                "center" => g.center = !g.center,
                "thirds" => g.thirds = !g.thirds,
                "rulers" => g.rulers = !g.rulers,
                _ => g.snap = !g.snap,
            }
            guides.set(g);
            crate::layout::save_guides(&g);
        }
        Act::Play => ui.toggle_play(),
        Act::ToStart => ui.set_time(0.0),
        Act::AutoKey => {
            let mut auto = ui.auto_key;
            let next = !*auto.peek();
            auto.set(next);
        }
        Act::Motions => {
            let mut open = ui.motion_dialog;
            open.set(Some("apply"));
        }
        Act::TimelineFit => {
            let mut z = ui.tl_zoom;
            z.set(1.0);
        }
        Act::Shortcuts => {
            let mut help = ui.help_dialog;
            help.set(Some("keys"));
        }
        Act::About => {
            let mut help = ui.help_dialog;
            help.set(Some("about"));
        }
        Act::Link(url) => crate::updates::open(url),
    }
}

#[component]
pub fn MenuBar(ui: Ui) -> Element {
    let mut open = use_signal(|| None::<usize>);
    let all = menus(ui);
    rsx! {
        nav { class: "menubar", role: "menubar",
            for (i , (title , items)) in all.into_iter().enumerate() {
                div { class: "menubar-menu",
                    button {
                        class: if open() == Some(i) { "menubar-title open" } else { "menubar-title" },
                        onclick: move |_| open.set(if open() == Some(i) { None } else { Some(i) }),
                        onmouseenter: move |_| {
                            if open().is_some() {
                                open.set(Some(i));
                            }
                        },
                        "{title}"
                    }
                    if open() == Some(i) {
                        div { class: "menu menubar-drop", role: "menu",
                            for it in items {
                                match it {
                                    Item::Sep => rsx! {
                                        div { class: "menu-sep" }
                                    },
                                    Item::Do(label, keys, act, on, checked) => rsx! {
                                        button {
                                            class: "menu-item",
                                            role: "menuitem",
                                            disabled: !on,
                                            onclick: move |_| {
                                                open.set(None);
                                                run(ui, act);
                                            },
                                            span { class: "menu-check",
                                                if checked == Some(true) { "✓" }
                                            }
                                            span { class: "menu-label", "{label}" }
                                            span { class: "menu-keys", "{keys}" }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
            if open().is_some() {
                div {
                    class: "menubar-catcher",
                    onclick: move |_| open.set(None),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            open.set(None);
                        }
                    },
                }
            }
        }
    }
}

/// Help → Keyboard shortcuts / About FrameCraft.
#[component]
pub fn HelpDialog(ui: Ui) -> Element {
    let mut which = ui.help_dialog;
    let Some(page) = which() else {
        return rsx! {};
    };
    let keys: &[(&str, &str)] = &[
        ("Ctrl+N / Ctrl+O", "New project / Open"),
        ("Ctrl+S / Ctrl+Shift+S", "Save / Save as"),
        ("Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y)", "Undo / Redo"),
        ("Ctrl+D", "Duplicate the selected layer(s)"),
        ("Ctrl+C / Ctrl+V", "Copy / paste selected keyframes"),
        ("Delete", "Delete the selected layer(s), keys or cue"),
        ("Esc", "Select none"),
        ("Arrows (Shift ×10)", "Nudge the selected layer(s) by 1 px"),
        ("Ctrl+click", "Add to or remove from the selection"),
        ("Space", "Play / pause"),
        (", and .", "One frame back / forward"),
        ("Ctrl+, and Ctrl+.", "Previous / next keyframe"),
        ("K", "Add or remove keys at the playhead"),
        ("Tab", "Project view ↔ editor"),
        ("Wheel / middle drag", "Viewport zoom at the pointer / pan"),
        (
            "Ctrl+wheel / Shift+wheel",
            "Timeline zoom at the pointer / pan",
        ),
        (
            "Drag a field's label",
            "Change a number (Shift ×10, Alt ×0.1)",
        ),
        ("Enter / Esc in a field", "Commit / leave the field"),
    ];
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| which.set(None),
            div { class: "modal motion-dialog small help-dialog", onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    span { class: "panel-title",
                        if page == "keys" { "Keyboard shortcuts" } else { "About FrameCraft" }
                    }
                    button { class: "key-button", title: "Close", onclick: move |_| which.set(None), "✕" }
                }
                if page == "keys" {
                    table { class: "keys-table",
                        for (k , what) in keys.iter() {
                            tr {
                                td { kbd { "{k}" } }
                                td { "{what}" }
                            }
                        }
                    }
                } else {
                    p { strong { "FrameCraft {env!(\"CARGO_PKG_VERSION\")}" } }
                    p { "Editor for animated broadcast and web graphics: SPX templates, OGraf packages and HTML banners. Open-source successor of MENiAC FRAME." }
                    p { class: "dim",
                        "The editor is free software under the GNU AGPL 3.0 or later; the runtime inside exported graphics is MIT licensed. © Oy MENiAC Ltd."
                    }
                    div { class: "row-buttons",
                        button { onclick: move |_| crate::updates::open(REPO), "Source code on GitHub" }
                        button {
                            onclick: move |_| crate::updates::open("https://github.com/meniactech/framecraft/releases"),
                            "Releases"
                        }
                    }
                }
            }
        }
    }
}
