// SPDX-License-Identifier: AGPL-3.0-or-later
//! The editor's live preview: the SPX package plus `js/framecraft-editor.js`,
//! and the cheapest correct update after each command.
//!
//! | `Change` | Update |
//! |---|---|
//! | `None` | nothing |
//! | `Pose` | patch every layer's animation track in place, repaint |
//! | `Layer(ids)` | swap those layers' markup (keeping their children), patch tracks and text / image / visibility data, repaint |
//! | `Rebuild` | a new package |
//!
//! A `Layer` change falls back to a rebuild when it needs something the
//! loaded page does not have: a new media file or font, added, removed or
//! moved layers, or a layer the runtime only sets up at start (charts, QR,
//! particles, lightning, patterns, Lottie, video, image slots).
//!
//! The editor script is preview-only. Exports never contain it.

use crate::commands::Change;
use crate::export::assets::{AssetCopy, AssetMap, font_faces};
use crate::export::html::{PageKind, dom_prefix, element_html, runtime_data};
use crate::export::package::{ExportRequest, MediaSource, Package, export_for, spx_package};
use crate::export::template::TemplateExport;
use crate::profile::Profile;
use serde_json::{Value, json};

/// Package path of the editor script.
pub const EDITOR_SCRIPT_PATH: &str = "js/framecraft-editor.js";
const EDITOR_SCRIPT: &str = include_str!("preview/framecraft-editor.js");
/// html-to-image 1.11.13 (MIT, see `preview/html-to-image.LICENSE`): renders
/// the page to a PNG for Export → PNG Frame. Preview only.
pub const SNAPSHOT_SCRIPT_PATH: &str = "js/html-to-image.js";
const SNAPSHOT_SCRIPT: &[u8] = include_bytes!("preview/html-to-image.js");

/// Layer types the runtime sets up once at start (image slots are filled
/// only by the runtime's field update, which also seeks to 0).
const SET_UP_AT_START: [&str; 8] = [
    "chart",
    "qr",
    "particles",
    "lightning",
    "pattern",
    "lottie",
    "video",
    "imageSlot",
];

/// What the loaded preview was built from.
#[derive(Debug, Clone)]
pub struct PreviewState {
    export: TemplateExport,
    assets: Vec<AssetCopy>,
    fonts: Vec<String>,
}

/// What to do to the loaded preview.
#[derive(Debug, Clone)]
pub enum PreviewUpdate {
    Nothing,
    /// Post this to the editor script (`window.postMessage`).
    Message(Value),
    /// Load this package instead.
    Rebuild(Package),
}

/// The export as the preview page sees it (package paths for media).
fn prepare(req: &ExportRequest, media: &dyn MediaSource, p: &Profile) -> PreviewState {
    let mut assets = AssetMap::spx();
    let export = assets.remap(&export_for(req, "ionframe_template", p));
    let (fonts, _) = font_faces(&export, req.user_fonts, false, &|src| {
        media.read(src).is_some()
    });
    PreviewState {
        export,
        assets: assets.copies().to_vec(),
        fonts,
    }
}

/// Builds the preview package: the SPX package with the editor script.
pub fn build(req: &ExportRequest, media: &dyn MediaSource, p: &Profile) -> (Package, PreviewState) {
    let mut pkg = spx_package(req, media, p);
    let entry = pkg
        .files
        .iter()
        .position(|(path, _)| path.ends_with(".html") && !path.contains('/'));
    if let Some(i) = entry {
        let html = String::from_utf8_lossy(&pkg.files[i].1).into_owned();
        let tag = format!(
            "<script src=\"{SNAPSHOT_SCRIPT_PATH}\"></script>\n  <script src=\"{EDITOR_SCRIPT_PATH}\"></script>\n"
        );
        let html = match html.find("</head>") {
            Some(at) => format!("{}  {tag}{}", &html[..at], &html[at..]),
            None => html,
        };
        pkg.files[i].1 = html.into_bytes();
    }
    pkg.files.push((
        EDITOR_SCRIPT_PATH.to_string(),
        EDITOR_SCRIPT.as_bytes().to_vec(),
    ));
    pkg.files
        .push((SNAPSHOT_SCRIPT_PATH.to_string(), SNAPSHOT_SCRIPT.to_vec()));
    (pkg, prepare(req, media, p))
}

/// Layer ids, types and parents in drawing order: when this differs, the
/// page's structure no longer matches.
fn structure(t: &TemplateExport) -> Vec<(&str, &str, Option<&str>)> {
    t.elements
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str(), e.parent_id.as_deref()))
        .collect()
}

/// The update for `change`, made after the document was edited, and the
/// state the preview will then be in.
pub fn update(
    prev: &PreviewState,
    change: &Change,
    req: &ExportRequest,
    media: &dyn MediaSource,
    p: &Profile,
) -> (PreviewUpdate, PreviewState) {
    if *change == Change::None {
        return (PreviewUpdate::Nothing, prev.clone());
    }
    let next = prepare(req, media, p);
    let same_shape = structure(&prev.export) == structure(&next.export)
        && prev.assets == next.assets
        && prev.fonts == next.fonts;
    let rebuild = || {
        let (pkg, state) = build(req, media, p);
        (PreviewUpdate::Rebuild(pkg), state)
    };
    if !same_shape {
        return rebuild();
    }
    let data = runtime_data(&next.export, PageKind::Spx, p);
    match change {
        Change::None => unreachable!("handled above"),
        Change::Rebuild => rebuild(),
        Change::Pose => {
            let msg = json!({ "type": "framecraft-editor", "action": "pose", "tracks": data["animationTracks"] });
            (PreviewUpdate::Message(msg), next)
        }
        Change::Layer(ids) => {
            let mut nodes = Vec::new();
            for id in ids {
                let Some(el) = next.export.elements.iter().find(|e| &e.id == id) else {
                    // Hidden or gone: only a rebuild shows that.
                    return rebuild();
                };
                if SET_UP_AT_START.contains(&el.kind.as_str()) {
                    return rebuild();
                }
                let html = element_html(&next.export, id).unwrap_or_default();
                nodes.push(
                    json!({ "domId": format!("{}_{}", dom_prefix(&el.kind), el.id), "html": html }),
                );
            }
            let hidden: Vec<Value> = next
                .export
                .fields
                .iter()
                .map(|f| json!({ "id": f.id, "value": f.default }))
                .collect();
            let msg = json!({
                "type": "framecraft-editor",
                "action": "layers",
                "nodes": nodes,
                "tracks": data["animationTracks"],
                "textLayers": data["textLayers"],
                "imageLayers": data["imageLayers"],
                "visibilityLayers": data["visibilityLayers"],
                "hidden": hidden,
            });
            (PreviewUpdate::Message(msg), next)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, SetNumber, SetText, SetVisible};
    use crate::model::Graphic;
    use crate::normalize::{Document, normalize};

    fn doc() -> Document {
        let item = |id: &str, kind: &str| {
            json!({
                "id": id, "name": id, "itemType": kind, "textContent": "Hello", "textVariableId": if id == "t" { "name" } else { "" },
                "x": 0, "y": 0, "width": 10, "height": 10, "rotation": 0, "opacity": 1, "bg_color": "#fff",
                "borderWidth": 0, "borderColor": "#000", "borderRadius": [0, 0, 0, 0],
                "shadow": { "color": "#000", "x": 0, "y": 0, "blur": 0 }
            })
        };
        let g: Graphic = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": [item("s", "shape"), item("t", "text"), item("q", "qr")]
        }))
        .unwrap();
        normalize(&g, &Profile::framecraft())
    }

    fn no_media(_: &str) -> Option<Vec<u8>> {
        None
    }

    fn req(d: &Document) -> ExportRequest<'_> {
        ExportRequest {
            doc: d,
            graphic_name: "G",
            palette_colors: &[],
            user_fonts: &[],
            exported_at: "",
        }
    }

    fn run(cmd: &dyn Command) -> PreviewUpdate {
        let p = Profile::framecraft();
        let mut d = doc();
        let (_, state) = build(&req(&d), &no_media, &p);
        let change = cmd.apply(&mut d).unwrap();
        update(&state, &change, &req(&d), &no_media, &p).0
    }

    #[test]
    fn every_new_layer_exports() {
        use crate::layers::{NewLayer, new_layer};
        let mut d = doc();
        d.items.clear();
        for kind in NewLayer::ALL {
            d.items.push(new_layer(kind, None));
        }
        let (pkg, _) = build(&req(&d), &no_media, &Profile::framecraft());
        let html = String::from_utf8(pkg.get("G.html").unwrap().to_vec()).unwrap();
        for item in &d.items {
            assert!(
                html.contains(&item.id),
                "{} missing from the page",
                item.name
            );
        }
    }

    #[test]
    fn preview_carries_the_editor_script() {
        let (pkg, _) = build(&req(&doc()), &no_media, &Profile::framecraft());
        let html = String::from_utf8(pkg.get("G.html").unwrap().to_vec()).unwrap();
        assert!(html.contains("<script src=\"js/framecraft-editor.js\"></script>\n</head>"));
        assert!(pkg.get(EDITOR_SCRIPT_PATH).is_some());
        // The PNG snapshot library is in the preview, never in exports.
        assert!(html.contains("<script src=\"js/html-to-image.js\"></script>"));
        assert!(pkg.get(SNAPSHOT_SCRIPT_PATH).is_some());
        let export =
            crate::export::package::spx_package(&req(&doc()), &no_media, &Profile::framecraft());
        assert!(
            export.get(SNAPSHOT_SCRIPT_PATH).is_none() && export.get(EDITOR_SCRIPT_PATH).is_none()
        );
    }

    #[test]
    fn pose_changes_patch_tracks() {
        let PreviewUpdate::Message(m) = run(&SetNumber {
            item: "s".into(),
            field: "x".into(),
            value: 42.0,
        }) else {
            panic!("expected a message")
        };
        assert_eq!(m["action"], "pose");
        assert_eq!(m["tracks"][0]["base"]["x"], 42.0);
    }

    #[test]
    fn text_changes_swap_the_layer_and_its_field() {
        let PreviewUpdate::Message(m) = run(&SetText {
            item: "t".into(),
            text: "Bye <b>".into(),
        }) else {
            panic!("expected a message")
        };
        assert_eq!(m["action"], "layers");
        assert_eq!(m["nodes"][0]["domId"], "txt_t");
        assert!(
            m["nodes"][0]["html"]
                .as_str()
                .unwrap()
                .contains("Bye &lt;b&gt;")
        );
        assert_eq!(m["hidden"][0], json!({ "id": "name", "value": "Bye <b>" }));
        assert_eq!(m["textLayers"][0]["fallback"], "Bye <b>");
    }

    #[test]
    fn structure_and_start_up_layers_rebuild() {
        assert!(matches!(
            run(&SetVisible {
                item: "s".into(),
                visible: false
            }),
            PreviewUpdate::Rebuild(_)
        ));
        // A QR code is drawn once at start: a layer swap would not redraw it.
        assert!(matches!(
            run(&SetText {
                item: "q".into(),
                text: "x".into()
            }),
            PreviewUpdate::Rebuild(_)
        ));
    }
}
