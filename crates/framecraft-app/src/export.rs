// SPDX-License-Identifier: AGPL-3.0-or-later
//! Properties → Export (FRAME's Exp tab): OGraf (the default), SPX, HTML
//! banner and a PNG of the current frame for the open graphic, plus
//! OGraf / SPX suites of the whole project. Missing media are listed before
//! anything is written; files go where the save dialog says.

use crate::state::Ui;
use dioxus::prelude::*;
use framecraft_core::export::names::file_base_name;
use std::path::PathBuf;

/// What to export.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Graphic(&'static str),
    Suite { ograf: bool },
    Png,
}

/// A built export waiting for the user (missing media to confirm).
#[derive(Clone, PartialEq)]
struct Pending {
    what: String,
    file: String,
    bytes: Vec<u8>,
    missing: Vec<String>,
}

pub(crate) fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

/// Standard base64 (for small images shown inline, e.g. thumbnails).
pub fn encode_base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The whole project as an OGraf or SPX suite (Project view, FRAME's
/// project page). Missing media are named in the status line.
pub fn export_suite(ui: Ui, ograf: bool) {
    let Some((project, folder, pkg)) = ui.editor.read().as_ref().map(|e| {
        (
            e.project.name().to_string(),
            e.project.folder(),
            e.project.suite(ograf),
        )
    }) else {
        return;
    };
    let mut status = ui.status;
    let bytes = match pkg.to_zip() {
        Ok(b) => b,
        Err(e) => {
            status.set(format!("Suite export failed: {e}"));
            return;
        }
    };
    let base = file_base_name(&project, "project");
    let file = if ograf {
        format!("{base}-all-ograf.zip")
    } else {
        format!("{base}-all-spx.zip")
    };
    let mut missing = pkg.missing.clone();
    missing.dedup();
    spawn(async move {
        save_as(ui, file, bytes, folder).await;
        if !missing.is_empty() {
            let mut status = ui.status;
            let note = status.peek().clone();
            status.set(format!(
                "{note} — {} media file(s) missing: {}",
                missing.len(),
                missing.join(", ")
            ));
        }
    });
}

/// Asks where to save (starting in the project's folder) and writes.
async fn save_as(mut ui: Ui, file: String, bytes: Vec<u8>, folder: PathBuf) {
    let ext = file.rsplit('.').next().unwrap_or("zip").to_string();
    let picked = rfd::AsyncFileDialog::new()
        .set_title("Export")
        .set_directory(&folder)
        .set_file_name(&file)
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .save_file()
        .await;
    let Some(handle) = picked else {
        ui.status.set("Export cancelled".into());
        return;
    };
    let path = handle.path().to_path_buf();
    match std::fs::write(&path, &bytes) {
        Ok(()) => ui.status.set(format!("Exported {}", path.display())),
        Err(e) => ui
            .status
            .set(format!("Could not write {}: {e}", path.display())),
    }
}

#[component]
pub fn ExportTab(ui: Ui) -> Element {
    let mut pending = use_signal(|| None::<Pending>);
    let mut busy = use_signal(|| false);
    let info = ui.editor.read().as_ref().map(|e| {
        let d = e.doc();
        (
            e.project.name().to_string(),
            e.project.graphic_name(e.graphic).to_string(),
            d.scene.width,
            d.scene.height,
            e.project.docs.len(),
            e.project.folder(),
        )
    });
    let Some((project, graphic, w, h, count, folder)) = info else {
        return rsx! {
            p { class: "dim", "Nothing open" }
        };
    };
    let time = (ui.time)().round();

    let run = {
        let (project, graphic, folder) = (project.clone(), graphic.clone(), folder.clone());
        move |target: Target| {
            let base = file_base_name(&project, "project");
            if target == Target::Png {
                let file = format!("{}.png", file_base_name(&graphic, "frame"));
                let folder = folder.clone();
                busy.set(true);
                spawn(async move {
                    let shot = crate::bridge::snapshot(w, h).await;
                    busy.set(false);
                    let mut status = ui.status;
                    match shot {
                        Ok(url) => match url.split_once(',').and_then(|(_, b)| decode_base64(b)) {
                            Some(bytes) => save_as(ui, file, bytes, folder).await,
                            None => status.set("PNG export: unreadable image data".into()),
                        },
                        Err(e) => status.set(format!("PNG export failed: {e}")),
                    }
                });
                return;
            }
            let (what, file) = match target {
                Target::Graphic("ograf") => ("OGraf", format!("{base}-ograf.zip")),
                Target::Graphic("banner") => ("HTML banner", format!("{base}.zip")),
                Target::Graphic(_) => ("SPX", format!("{base}.zip")),
                Target::Suite { ograf: true } => {
                    ("OGraf (all graphics)", format!("{base}-all-ograf.zip"))
                }
                Target::Suite { ograf: false } => {
                    ("SPX (all graphics)", format!("{base}-all-spx.zip"))
                }
                Target::Png => unreachable!("handled above"),
            };
            let built = ui.editor.read().as_ref().map(|e| match target {
                Target::Graphic(format) => e.project.package(e.graphic, format),
                Target::Suite { ograf } => e.project.suite(ograf),
                Target::Png => unreachable!("handled above"),
            });
            let Some(pkg) = built else { return };
            let bytes = match pkg.to_zip() {
                Ok(b) => b,
                Err(e) => {
                    let mut status = ui.status;
                    status.set(format!("{what} export failed: {e}"));
                    return;
                }
            };
            let mut missing = pkg.missing.clone();
            missing.dedup();
            let p = Pending {
                what: what.into(),
                file,
                bytes,
                missing,
            };
            if p.missing.is_empty() {
                spawn(save_as(ui, p.file, p.bytes, folder.clone()));
            } else {
                pending.set(Some(p));
            }
        }
    };
    let (mut r1, mut r2, mut r3, mut r4, mut r5, mut r6) = (
        run.clone(),
        run.clone(),
        run.clone(),
        run.clone(),
        run.clone(),
        run,
    );
    let folder2 = folder.clone();

    rsx! {
        if let Some(p) = pending() {
            div { class: "export-missing",
                p {
                    strong { "{p.what}: " }
                    "{p.missing.len()} media file(s) are missing and will not be in the package:"
                }
                ul {
                    for m in p.missing.iter() {
                        li { code { "{m}" } }
                    }
                }
                div { class: "row-buttons",
                    button { onclick: move |_| pending.set(None), "Cancel" }
                    button {
                        class: "active",
                        onclick: move |_| {
                            if let Some(p) = pending.take() {
                                spawn(save_as(ui, p.file, p.bytes, folder2.clone()));
                            }
                        },
                        "Export anyway"
                    }
                }
            }
        }
        h3 { "This graphic" }
        div { class: "export-row",
            div {
                span { class: "export-title", "OGraf (EBU)" }
                span { class: "badge", "Default" }
                p { class: "dim", "HTML / runtime plus *.ograf.json and a Web Component for OGraf hosts." }
            }
            button { class: "active", disabled: busy(), onclick: move |_| r1(Target::Graphic("ograf")), "Export OGraf…" }
        }
        div { class: "export-row",
            div {
                span { class: "export-title", "SPX" }
                p { class: "dim", "SPX HTML template ZIP for SuperPlayoutXML. Variable IDs become SPX fields." }
            }
            button { disabled: busy(), onclick: move |_| r2(Target::Graphic("spx")), "Export SPX…" }
        }
        div { class: "export-row",
            div {
                span { class: "export-title", "HTML Banner" }
                p { class: "dim", "Fixed-size animated banner ({w}×{h}). Autoplays on load." }
            }
            button { disabled: busy(), onclick: move |_| r3(Target::Graphic("banner")), "Export banner…" }
        }
        div { class: "export-row",
            div {
                span { class: "export-title", "PNG Frame" }
                p { class: "dim", "The current timeline frame at {w}×{h} (playhead: {time} ms)." }
            }
            button {
                disabled: busy(),
                onclick: move |_| r4(Target::Png),
                if busy() { "Rendering…" } else { "Export PNG…" }
            }
        }
        h3 { "Whole project" }
        p { class: "dim", "All {count} graphic(s) of {project} in one package, sharing scripts, assets and fonts." }
        div { class: "export-row",
            div {
                span { class: "export-title", "OGraf suite" }
            }
            button { disabled: busy(), onclick: move |_| r5(Target::Suite { ograf: true }), "Export all as OGraf…" }
        }
        div { class: "export-row",
            div {
                span { class: "export-title", "SPX suite" }
            }
            button { disabled: busy(), onclick: move |_| r6(Target::Suite { ograf: false }), "Export all as SPX…" }
        }
        p { class: "dim small", "Exports open a save dialog in the project's folder: {folder.display()}" }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_base64;

    #[test]
    fn base64_round_trip() {
        for s in [&b""[..], b"h", b"hi", b"hey", b"hello world"] {
            assert_eq!(super::decode_base64(&super::encode_base64(s)).unwrap(), s);
        }
        assert_eq!(super::encode_base64(b"hi"), "aGk=");
    }

    #[test]
    fn base64() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGk=").unwrap(), b"hi");
        assert_eq!(decode_base64("YWJj").unwrap(), b"abc");
        assert!(decode_base64("a*b").is_none());
    }
}
