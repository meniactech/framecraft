// SPDX-License-Identifier: AGPL-3.0-or-later
//! FrameCraft desktop app (UI test build).
//!
//! Opens a FRAME project pack, shows a graphic in the viewport (the real
//! exported page, seeked by the timeline), edits basic properties of the
//! selected layer, exports SPX / OGraf / banner ZIPs and saves a copy of
//! the project.
//!
//! Run with an optional pack path: `cargo run -p framecraft-app -- my.frameproject.zip`

mod preview;
mod project;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, wry::http::Response};
use dioxus::prelude::*;
use framecraft_core::model::{Item, ItemType};
use framecraft_core::normalize::CLIP_START;
use preview::{HOST, SCHEME, STORE, SharedPreview};
use project::Project;
use std::path::PathBuf;
use std::time::Duration;

const STYLE: &str = include_str!("style.css");
/// Timeline scale: pixels per millisecond (10 s = 1000 px).
const PX_PER_MS: f64 = 0.1;
const LABEL_WIDTH: f64 = 200.0;

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

/// Everything about the open project the UI edits.
struct Editor {
    project: Project,
    graphic: usize,
    selected: Option<String>,
}

impl Editor {
    fn doc(&self) -> &framecraft_core::normalize::Document {
        &self.project.docs[self.graphic]
    }
    fn item_mut(&mut self, id: &str) -> Option<&mut Item> {
        self.project.docs[self.graphic]
            .items
            .iter_mut()
            .find(|i| i.id == id)
    }
}

/// Shared UI state, handed to child components as one value.
#[derive(Clone, Copy, PartialEq)]
struct Ui {
    editor: Signal<Option<Editor>>,
    time: Signal<f64>,
    playing: Signal<bool>,
    zoom: Signal<f64>,
    status: Signal<String>,
    preview_url: Signal<String>,
    /// Revision of the package the viewport shows.
    revision: Signal<u64>,
}

impl Ui {
    /// Rebuilds the preview package and points the viewport at it.
    fn refresh(mut self, store: &SharedPreview) {
        install_bridge();
        let pkg = match self.editor.read().as_ref() {
            Some(ed) => ed.project.preview(ed.graphic),
            None => return,
        };
        let url = store.borrow_mut().publish(pkg);
        let rev = store.borrow().revision();
        self.preview_url.set(url);
        self.revision.set(rev);
        seek(rev, (self.time)());
    }

    fn open(mut self, path: PathBuf, store: &SharedPreview) {
        match Project::open(&path) {
            Ok(project) => {
                let warnings = project.warnings.len();
                self.status.set(format!(
                    "Opened {} ({} graphics){}",
                    project.name(),
                    project.docs.len(),
                    if warnings > 0 {
                        format!(", {warnings} warnings")
                    } else {
                        String::new()
                    }
                ));
                self.editor.set(Some(Editor {
                    project,
                    graphic: 0,
                    selected: None,
                }));
                self.time.set(0.0);
                self.playing.set(false);
                self.refresh(store);
                spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    fit(self).await;
                });
                if debug() {
                    spawn(report_preview_state());
                }
            }
            Err(e) => self.status.set(e),
        }
    }

    fn duration(&self) -> f64 {
        self.editor
            .read()
            .as_ref()
            .map_or(10_000.0, |e| e.doc().timeline.duration.max(1.0))
    }
}

/// Talks to the preview iframe. The preview runs on its own origin
/// (`framecraft://`), so the app cannot call into it directly; it sends the
/// runtime's `postMessage` commands instead. After each load it repeats the
/// seek until the runtime has started (it starts asynchronously).
const BRIDGE_JS: &str = r#"
if (!window.__fc) {
  const fc = (window.__fc = { time: 0, loads: 0, pongs: 0, retry: null, playing: false, raf: null, onStop: null, labelWidth: 200, pxPerMs: 0.1 });
  const frame = () => document.getElementById('fc-preview');
  fc.post = (msg) => {
    const f = frame();
    if (f && f.contentWindow) f.contentWindow.postMessage(msg, '*');
  };
  fc.seek = (t) => {
    fc.time = t;
    fc.post({ type: 'ionframe-spx', action: 'goToTime', timestamp: t });
  };
  // Playback runs here, once per display frame: seek the preview and move
  // the playhead and time label directly. Rust only hears the final time.
  fc.drawPlayhead = (t) => {
    const p = document.getElementById('fc-playhead');
    if (p) p.style.left = (fc.labelWidth + t * fc.pxPerMs) + 'px';
    const l = document.getElementById('fc-time');
    if (l) l.textContent = (t / 1000).toFixed(2);
  };
  fc.play = (from, duration, onStop) => {
    fc.stop(false);
    const start = performance.now();
    fc.playing = true;
    fc.onStop = onStop;
    const tick = (now) => {
      if (!fc.playing) return;
      const t = Math.min(duration, from + (now - start));
      fc.seek(t);
      fc.drawPlayhead(t);
      if (t >= duration) { fc.stop(true); return; }
      fc.raf = requestAnimationFrame(tick);
    };
    fc.raf = requestAnimationFrame(tick);
  };
  // `report`: tell Rust where playback stopped (Pause, end); a scrub sets
  // the time itself and does not report.
  fc.stop = (report) => {
    if (fc.raf) cancelAnimationFrame(fc.raf);
    fc.raf = null;
    const was = fc.playing;
    fc.playing = false;
    const cb = fc.onStop;
    fc.onStop = null;
    if (was && cb) cb(report ? fc.time : null);
  };
  window.addEventListener('message', (e) => {
    const m = e.data;
    if (m && m.type === 'ionframe-spx' && (m.action === 'pong' || m.action === 'goToTimeDone')) {
      fc.pongs++;
      if (fc.retry) { clearInterval(fc.retry); fc.retry = null; }
    }
  });
  // Iframe load events do not bubble; listen in the capture phase.
  document.addEventListener('load', (e) => {
    if (!e.target || e.target.id !== 'fc-preview') return;
    fc.loads++;
    if (fc.retry) clearInterval(fc.retry);
    let n = 0;
    fc.retry = setInterval(() => {
      fc.seek(fc.time);
      fc.post({ type: 'ionframe-spx', action: 'ping' });
      if (++n >= 60) { clearInterval(fc.retry); fc.retry = null; }
    }, 50);
  }, true);
}
"#;

/// Installs [`BRIDGE_JS`] (once per page).
fn install_bridge() {
    let _ = document::eval(BRIDGE_JS);
}

/// Moves the preview to `time_ms`.
fn seek(_rev: u64, time_ms: f64) {
    let _ = document::eval(&format!("window.__fc && window.__fc.seek({time_ms});"));
}

/// `FRAMECRAFT_DEBUG=1` logs preview requests and the preview's state.
fn debug() -> bool {
    std::env::var_os("FRAMECRAFT_DEBUG").is_some()
}

/// Prints the preview bridge's state: loads seen and runtime replies.
async fn report_preview_state() {
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let js = "const f = document.getElementById('fc-preview'); \
              return { src: f && f.src, bridge: window.__fc ? { loads: window.__fc.loads, pongs: window.__fc.pongs, time: window.__fc.time } : null };";
    match document::eval(js).join::<serde_json::Value>().await {
        Ok(v) => eprintln!("[preview state] {v:#}"),
        Err(e) => eprintln!("[preview state] could not inspect: {e:?}"),
    }
}

/// Zooms the viewport so the whole scene fits.
async fn fit(mut ui: Ui) {
    let size = document::eval(
        "const v = document.getElementById('fc-viewport'); return v ? [v.clientWidth, v.clientHeight] : [0, 0];",
    )
    .join::<Vec<f64>>()
    .await;
    let (sw, sh) = match ui.editor.read().as_ref() {
        Some(e) => (e.doc().scene.width.max(1.0), e.doc().scene.height.max(1.0)),
        None => return,
    };
    if let Ok(s) = size
        && s.len() == 2
        && s[0] > 0.0
    {
        ui.zoom.set(
            ((s[0] - 32.0) / sw)
                .min((s[1] - 32.0) / sh)
                .clamp(0.05, 2.0),
        );
    }
}

#[component]
fn App() -> Element {
    let ui = Ui {
        editor: use_signal(|| None),
        time: use_signal(|| 0.0),
        playing: use_signal(|| false),
        zoom: use_signal(|| 0.5),
        status: use_signal(|| "Open a .frameproject.zip or .framepack.zip".to_string()),
        preview_url: use_signal(String::new),
        revision: use_signal(|| 0),
    };
    let store: SharedPreview = STORE.with(Clone::clone);
    use_context_provider(|| store.clone());

    // A pack given on the command line opens at start.
    let start_store = store.clone();
    use_hook(move || {
        if let Some(path) = std::env::args_os().nth(1) {
            ui.open(PathBuf::from(path), &start_store);
        }
    });

    rsx! {
        div { class: "app",
            Header { ui }
            div { class: "main",
                Viewport { ui }
                Properties { ui }
            }
            Timeline { ui }
        }
    }
}

#[component]
fn Header(ui: Ui) -> Element {
    let store = use_context::<SharedPreview>();
    let (graphics, current) = match ui.editor.read().as_ref() {
        Some(e) => (
            (0..e.project.docs.len())
                .map(|i| e.project.graphic_name(i).to_string())
                .collect::<Vec<_>>(),
            e.graphic,
        ),
        None => (Vec::new(), 0),
    };
    let has_project = !graphics.is_empty();
    let open_store = store.clone();
    let select_store = store.clone();

    let export = move |format: &'static str| {
        let mut status = ui.status;
        let result = ui
            .editor
            .read()
            .as_ref()
            .map(|e| e.project.export(e.graphic, format));
        match result {
            Some(Ok(path)) => status.set(format!("Exported {}", path.display())),
            Some(Err(e)) => status.set(e),
            None => {}
        }
    };

    rsx! {
        div { class: "header",
            span { class: "brand", "FrameCraft" }
            label { class: "button",
                "Open…"
                input {
                    r#type: "file",
                    accept: ".zip",
                    onchange: move |evt| {
                        if let Some(file) = evt.files().first() {
                            ui.open(file.path(), &open_store);
                        }
                    },
                }
            }
            if has_project {
                select {
                    onchange: move |evt| {
                        if let Ok(i) = evt.value().parse::<usize>() {
                            let mut editor = ui.editor;
                            if let Some(e) = editor.write().as_mut() {
                                e.graphic = i;
                                e.selected = None;
                            }
                            let (mut time, mut playing) = (ui.time, ui.playing);
                            time.set(0.0);
                            playing.set(false);
                            ui.refresh(&select_store);
                        }
                    },
                    for (i , name) in graphics.iter().enumerate() {
                        option { value: "{i}", selected: i == current, "{name}" }
                    }
                }
                button { onclick: move |_| export("spx"), "Export SPX" }
                button { onclick: move |_| export("ograf"), "Export OGraf" }
                button { onclick: move |_| export("banner"), "Export banner" }
                button {
                    onclick: move |_| {
                        let mut status = ui.status;
                        let result = ui.editor.read().as_ref().map(|e| e.project.save_copy());
                        match result {
                            Some(Ok(path)) => status.set(format!("Saved {}", path.display())),
                            Some(Err(e)) => status.set(e),
                            None => {}
                        }
                    },
                    "Save copy"
                }
            }
            span { class: "status", "{ui.status}" }
        }
    }
}

#[component]
fn Viewport(ui: Ui) -> Element {
    let (w, h) = match ui.editor.read().as_ref() {
        Some(e) => (e.doc().scene.width, e.doc().scene.height),
        None => (1920.0, 1080.0),
    };
    let zoom = (ui.zoom)();
    let url = (ui.preview_url)();
    rsx! {
        div { class: "viewport-panel",
            div { class: "viewport-tools",
                for z in [0.25, 0.5, 0.75, 1.0] {
                    button {
                        class: if (zoom - z).abs() < 1e-6 { "active" } else { "" },
                        onclick: move |_| {
                            let mut zoom = ui.zoom;
                            zoom.set(z);
                        },
                        "{z * 100.0}%"
                    }
                }
                button { onclick: move |_| { spawn(fit(ui)); }, "Fit" }
                span { class: "dim", "{w}×{h}" }
            }
            div { class: "viewport", id: "fc-viewport",
                if !url.is_empty() {
                    div { class: "stage", style: "width:{w * zoom}px;height:{h * zoom}px;",
                        iframe {
                            id: "fc-preview",
                            src: "{url}",
                            style: "width:{w}px;height:{h}px;transform:scale({zoom});",
                        }
                    }
                } else {
                    div { class: "empty", "No graphic open" }
                }
            }
        }
    }
}

/// Number fields the properties tab edits.
const FIELDS: [(&str, &str); 8] = [
    ("x", "X"),
    ("y", "Y"),
    ("width", "Width"),
    ("height", "Height"),
    ("rotation", "Rotation"),
    ("opacity", "Opacity"),
    ("scaleX", "Scale X"),
    ("scaleY", "Scale Y"),
];

fn get_field(item: &Item, key: &str) -> f64 {
    match key {
        "x" => item.x,
        "y" => item.y,
        "width" => item.width,
        "height" => item.height,
        "rotation" => item.rotation,
        "opacity" => item.opacity,
        "scaleX" => item.scale_x.unwrap_or(1.0),
        "scaleY" => item.scale_y.unwrap_or(1.0),
        _ => 0.0,
    }
}

fn set_field(item: &mut Item, key: &str, v: f64) {
    match key {
        "x" => item.x = v,
        "y" => item.y = v,
        "width" => item.width = v.max(0.0),
        "height" => item.height = v.max(0.0),
        "rotation" => item.rotation = v,
        "opacity" => item.opacity = v.clamp(0.0, 1.0),
        "scaleX" => item.scale_x = Some(v),
        "scaleY" => item.scale_y = Some(v),
        _ => {}
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[component]
fn Properties(ui: Ui) -> Element {
    let store = use_context::<SharedPreview>();
    let editor = ui.editor.read();
    let Some(ed) = editor.as_ref() else {
        return rsx! { div { class: "properties", h3 { "Transform" } p { class: "dim", "Nothing open" } } };
    };
    let Some(item) = ed
        .selected
        .as_ref()
        .and_then(|id| ed.doc().items.iter().find(|i| &i.id == id))
    else {
        return rsx! { div { class: "properties", h3 { "Transform" } p { class: "dim", "Select a layer in the timeline" } } };
    };
    let id = item.id.clone();
    let animated: Vec<String> = ed
        .doc()
        .timeline
        .item_tracks
        .iter()
        .filter(|t| t.item_id == id)
        .flat_map(|t| {
            t.properties
                .iter()
                .filter(|(_, k)| !k.is_empty())
                .map(|(p, _)| p.clone())
        })
        .collect();
    let text = (item.item_type == Some(ItemType::Text))
        .then(|| item.text_content.clone().unwrap_or_default());
    let name = item.name.clone();
    let kind = format!("{:?}", item.item_type.unwrap_or(ItemType::Shape));
    let values: Vec<(&str, &str, f64, bool)> = FIELDS
        .iter()
        .map(|(key, label)| {
            (
                *key,
                *label,
                round3(get_field(item, key)),
                animated.iter().any(|a| a == key),
            )
        })
        .collect();
    drop(editor);

    rsx! {
        div { class: "properties",
            h3 { "Transform" }
            p { class: "layer-name", "{name} " span { class: "dim", "{kind}" } }
            for (key , label , value , is_animated) in values {
                {
                    let id = id.clone();
                    let store = store.clone();
                    rsx! {
                        label { class: "field",
                            span { "{label}" }
                            input {
                                r#type: "number",
                                step: "any",
                                value: "{value}",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().parse::<f64>() {
                                        let mut editor = ui.editor;
                                        if let Some(item) = editor.write().as_mut().and_then(|e| e.item_mut(&id)) {
                                            set_field(item, key, v);
                                        }
                                        ui.refresh(&store);
                                    }
                                },
                            }
                            if is_animated {
                                span { class: "badge", title: "This property has keyframes; they override this value while animating.", "◆" }
                            }
                        }
                    }
                }
            }
            if let Some(text) = text {
                {
                    let id = id.clone();
                    let store = store.clone();
                    rsx! {
                        h3 { "Text" }
                        textarea {
                            value: "{text}",
                            onchange: move |evt| {
                                let mut editor = ui.editor;
                                if let Some(item) = editor.write().as_mut().and_then(|e| e.item_mut(&id)) {
                                    item.text_content = Some(evt.value());
                                }
                                ui.refresh(&store);
                            },
                        }
                    }
                }
            }
        }
    }
}

/// A timeline row: one layer, indented under its parent.
struct Row {
    id: String,
    name: String,
    depth: usize,
    keys: Vec<f64>,
}

fn rows(ed: &Editor) -> Vec<Row> {
    let doc = ed.doc();
    let depth_of = |item: &Item| {
        let mut depth = 0;
        let mut parent = item.parent_id.clone().flatten();
        while let Some(pid) = parent {
            depth += 1;
            parent = doc
                .items
                .iter()
                .find(|i| i.id == pid)
                .and_then(|p| p.parent_id.clone().flatten());
            if depth > 32 {
                break;
            }
        }
        depth
    };
    // Parents before children, keeping item order otherwise.
    fn visit<'a>(items: &'a [Item], parent: Option<&str>, out: &mut Vec<&'a Item>) {
        for item in items
            .iter()
            .filter(|i| i.parent_id.clone().flatten().as_deref() == parent)
        {
            out.push(item);
            visit(items, Some(&item.id), out);
        }
    }
    let mut ordered = Vec::new();
    visit(&doc.items, None, &mut ordered);
    ordered
        .into_iter()
        .map(|item| {
            let mut keys: Vec<f64> = doc
                .timeline
                .item_tracks
                .iter()
                .filter(|t| t.item_id == item.id)
                .flat_map(|t| {
                    t.properties
                        .iter()
                        .filter(|(p, _)| p.as_str() != CLIP_START)
                        .flat_map(|(_, k)| k.iter().map(|k| k.time))
                })
                .collect();
            keys.sort_by(f64::total_cmp);
            keys.dedup();
            Row {
                id: item.id.clone(),
                name: item.name.clone(),
                depth: depth_of(item),
                keys,
            }
        })
        .collect()
}

#[component]
fn Timeline(ui: Ui) -> Element {
    let duration = ui.duration();
    let time = (ui.time)();
    let playing = (ui.playing)();
    let (rows, selected) = match ui.editor.read().as_ref() {
        Some(e) => (rows(e), e.selected.clone()),
        None => (Vec::new(), None),
    };
    let width = duration * PX_PER_MS;
    let seconds = (duration / 1000.0).ceil() as usize;

    let set_time = move |t: f64| {
        let (mut time, mut playing) = (ui.time, ui.playing);
        if playing() {
            let _ = document::eval("window.__fc && window.__fc.stop(false);");
        }
        playing.set(false);
        let t = t.clamp(0.0, duration);
        time.set(t);
        seek((ui.revision)(), t);
    };

    let toggle = move |_| {
        let mut playing = ui.playing;
        if playing() {
            // Pause: the bridge reports the time it stopped at.
            let _ = document::eval("window.__fc && window.__fc.stop(true);");
            return;
        }
        playing.set(true);
        let from = if (ui.time)() >= duration {
            0.0
        } else {
            (ui.time)()
        };
        spawn(async move {
            let mut eval = document::eval(&format!(
                "await new Promise((done) => window.__fc.play({from}, {duration}, (t) => {{ dioxus.send(t); done(); }}));"
            ));
            let stopped_at = eval.recv::<Option<f64>>().await;
            let (mut time, mut playing) = (ui.time, ui.playing);
            if let Ok(Some(t)) = stopped_at {
                time.set(t);
            }
            playing.set(false);
        });
    };

    rsx! {
        div { class: "timeline",
            div { class: "transport",
                button { onclick: move |_| set_time(0.0), "⏮" }
                button { onclick: toggle, if playing { "⏸ Pause" } else { "▶ Play" } }
                span { class: "time",
                    span { id: "fc-time", "{time / 1000.0:.2}" }
                    " s / {duration / 1000.0:.2} s"
                }
            }
            div { class: "tl-body",
                div { class: "tl-inner", style: "width:{LABEL_WIDTH + width + 40.0}px;",
                    div { class: "tl-row ruler",
                        div { class: "label" }
                        div {
                            class: "track",
                            style: "width:{width}px;",
                            onmousedown: move |evt| set_time(evt.element_coordinates().x / PX_PER_MS),
                            onmousemove: move |evt| {
                                if evt.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                                    set_time(evt.element_coordinates().x / PX_PER_MS);
                                }
                            },
                            for s in 0..=seconds {
                                div { class: "tick", style: "left:{s as f64 * 1000.0 * PX_PER_MS}px;", "{s}s" }
                            }
                        }
                    }
                    for row in rows {
                        {
                            let id = row.id.clone();
                            let is_selected = selected.as_deref() == Some(row.id.as_str());
                            rsx! {
                                div { class: if is_selected { "tl-row selected" } else { "tl-row" },
                                    div {
                                        class: "label",
                                        style: "padding-left:{8 + row.depth * 14}px;",
                                        onclick: move |_| {
                                            let mut editor = ui.editor;
                                            if let Some(e) = editor.write().as_mut() {
                                                e.selected = Some(id.clone());
                                            }
                                        },
                                        "{row.name}"
                                    }
                                    div { class: "track", style: "width:{width}px;",
                                        for k in row.keys {
                                            div {
                                                class: "key",
                                                style: "left:{k * PX_PER_MS}px;",
                                                onclick: move |_| set_time(k),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        id: "fc-playhead",
                        class: "playhead",
                        style: "left:{LABEL_WIDTH + time * PX_PER_MS}px;",
                    }
                }
            }
        }
    }
}
