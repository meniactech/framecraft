// SPDX-License-Identifier: AGPL-3.0-or-later
//! Render to video (Properties → Export → Video): the open graphic, frame
//! by frame at its size and frame rate, as a PNG sequence or — with FFmpeg —
//! ProRes 4444 / WebM VP9 (both with alpha) or MP4 H.264. This is MENiAC
//! Air's offline render done in the editor: the preview is seeked
//! (`goToTime`) and captured for each frame. The whole timeline renders
//! straight through pause cues.
//!
//! FFmpeg is found on PATH, or set with `FRAMECRAFT_FFMPEG`.

use crate::state::Ui;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Output formats: `(id, label, file extension, needs FFmpeg, alpha)`.
const FORMATS: &[(&str, &str, &str, bool, bool)] = &[
    ("png", "PNG sequence (alpha)", "", false, true),
    ("prores", "ProRes 4444 .mov (alpha)", "mov", true, true),
    ("webm", "WebM VP9 (alpha)", "webm", true, true),
    ("mp4", "MP4 H.264 (no alpha)", "mp4", true, false),
];

/// Progress of a running render: frames done, total, cancel asked.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub cancel: bool,
}

/// FFmpeg to use: `FRAMECRAFT_FFMPEG`, else `ffmpeg` on PATH.
fn ffmpeg() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FRAMECRAFT_FFMPEG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let exe = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(exe))
            .find(|p| p.is_file())
    })
}

/// FFmpeg reading PNG frames on stdin and writing `out` in `format`.
fn encoder(ffmpeg: &PathBuf, format: &str, fps: f64, out: &PathBuf) -> std::io::Result<Child> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "image2pipe",
    ])
    .args(["-framerate", &fps.to_string(), "-c:v", "png", "-i", "-"]);
    match format {
        "prores" => cmd.args([
            "-c:v",
            "prores_ks",
            "-profile:v",
            "4444",
            "-pix_fmt",
            "yuva444p10le",
            "-vendor",
            "apl0",
        ]),
        "webm" => cmd.args([
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuva420p",
            "-b:v",
            "0",
            "-crf",
            "28",
            "-auto-alt-ref",
            "0",
        ]),
        // No alpha: transparent areas become black.
        _ => cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"]),
    };
    cmd.arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
}

/// Asks where to write, then renders every frame.
fn start(ui: Ui, format: &'static str) {
    let Some((name, w, h, fps, duration, folder)) = ui.editor.peek().as_ref().map(|e| {
        let d = e.doc();
        (
            e.project.graphic_name(e.graphic).to_string(),
            d.scene.width,
            d.scene.height,
            d.timeline.fps.max(1.0),
            d.timeline.duration.max(1.0),
            e.project.folder(),
        )
    }) else {
        return;
    };
    let ext = FORMATS.iter().find(|f| f.0 == format).map_or("", |f| f.2);
    let base = framecraft_core::export::names::file_base_name(&name, "render");
    spawn_forever(async move {
        let mut status = ui.status;
        // Where: a file for video, a new folder for a PNG sequence.
        let picked = if ext.is_empty() {
            rfd::AsyncFileDialog::new()
                .set_title("Render PNG sequence: name of the new folder")
                .set_directory(&folder)
                .set_file_name(format!("{base}-frames"))
                .save_file()
                .await
        } else {
            rfd::AsyncFileDialog::new()
                .set_title("Render video")
                .set_directory(&folder)
                .set_file_name(format!("{base}.{ext}"))
                .add_filter(ext.to_uppercase(), &[ext])
                .save_file()
                .await
        };
        let Some(handle) = picked else {
            status.set("Render cancelled".into());
            return;
        };
        let out = handle.path().to_path_buf();
        let total = ((duration / 1000.0) * fps).round().max(1.0) as usize;
        let mut encoder_proc = None;
        if ext.is_empty() {
            // Some file pickers create the chosen file; the sequence is a folder.
            if out.is_file() && std::fs::metadata(&out).is_ok_and(|m| m.len() == 0) {
                let _ = std::fs::remove_file(&out);
            }
            if let Err(e) = std::fs::create_dir_all(&out) {
                status.set(format!("Could not create {}: {e}", out.display()));
                return;
            }
        } else {
            let Some(ff) = ffmpeg() else {
                status.set("FFmpeg not found (install it, or set FRAMECRAFT_FFMPEG)".into());
                return;
            };
            match encoder(&ff, format, fps, &out) {
                Ok(child) => encoder_proc = Some(child),
                Err(e) => {
                    status.set(format!("Could not start FFmpeg: {e}"));
                    return;
                }
            }
        }
        let resume_at = *ui.time.peek();
        let mut progress = ui.render;
        progress.set(Some(Progress {
            done: 0,
            total,
            cancel: false,
        }));
        let mut error = None;
        for i in 0..total {
            if progress.peek().is_some_and(|p| p.cancel) {
                error = Some("Render cancelled".to_string());
                break;
            }
            let t = (i as f64 * 1000.0 / fps).min(duration);
            crate::bridge::seek(t);
            let png = match crate::bridge::snapshot(w, h).await {
                Ok(url) => url
                    .split_once(',')
                    .and_then(|(_, b)| crate::export::decode_base64(b)),
                Err(e) => {
                    error = Some(format!("Frame {i}: {e}"));
                    break;
                }
            };
            let Some(png) = png else {
                error = Some(format!("Frame {i}: unreadable image data"));
                break;
            };
            let written = match encoder_proc.as_mut().and_then(|c| c.stdin.as_mut()) {
                Some(stdin) => stdin.write_all(&png).map_err(|e| e.to_string()),
                None => std::fs::write(out.join(format!("{base}_{i:05}.png")), &png)
                    .map_err(|e| e.to_string()),
            };
            if let Err(e) = written {
                error = Some(format!("Frame {i}: {e}"));
                break;
            }
            progress.set(Some(Progress {
                done: i + 1,
                total,
                cancel: false,
            }));
        }
        if let Some(mut child) = encoder_proc {
            drop(child.stdin.take()); // end of input: FFmpeg finishes the file
            match child.wait_with_output() {
                Ok(o) if !o.status.success() && error.is_none() => {
                    let msg = String::from_utf8_lossy(&o.stderr);
                    error = Some(format!(
                        "FFmpeg failed: {}",
                        msg.lines().last().unwrap_or("unknown error")
                    ));
                }
                Err(e) if error.is_none() => error = Some(format!("FFmpeg: {e}")),
                _ => {}
            }
        }
        progress.set(None);
        crate::bridge::seek(resume_at);
        match error {
            Some(e) => status.set(e),
            None => status.set(format!("Rendered {total} frames to {}", out.display())),
        }
    });
}

/// Properties → Export → Video.
#[component]
pub fn RenderSection(ui: Ui) -> Element {
    let mut format = use_signal(|| "png");
    let has_ffmpeg = use_hook(|| ffmpeg().is_some());
    let info = ui.editor.read().as_ref().map(|e| {
        let d = e.doc();
        (
            d.scene.width,
            d.scene.height,
            d.timeline.fps,
            d.timeline.duration,
        )
    });
    let Some((w, h, fps, duration)) = info else {
        return rsx! {};
    };
    let frames = ((duration / 1000.0) * fps.max(1.0)).round();
    let progress = (ui.render)();
    rsx! {
        h3 { "Video" }
        div { class: "export-row",
            div {
                span { class: "export-title", "Render" }
                p { class: "dim",
                    "The whole timeline at {w}×{h}, {fps} fps: {frames} frames. Pause cues are rendered through."
                }
                select {
                    disabled: progress.is_some(),
                    onchange: move |evt| {
                        if let Some(f) = FORMATS.iter().find(|f| f.0 == evt.value()) {
                            format.set(f.0);
                        }
                    },
                    for (id , label , _ext , needs , _alpha) in FORMATS.iter().copied() {
                        option {
                            value: "{id}",
                            disabled: needs && !has_ffmpeg,
                            selected: id == format(),
                            if needs && !has_ffmpeg { "{label} — needs FFmpeg" } else { "{label}" }
                        }
                    }
                }
                if !has_ffmpeg {
                    p { class: "dim small", "Video formats need FFmpeg: install it, or set FRAMECRAFT_FFMPEG to its path." }
                }
            }
            if let Some(p) = progress {
                div { class: "render-progress",
                    progress { max: "{p.total}", value: "{p.done}" }
                    span { class: "dim", "{p.done} / {p.total}" }
                    button {
                        onclick: move |_| {
                            let mut r = ui.render;
                            if let Some(mut p) = r() {
                                p.cancel = true;
                                r.set(Some(p));
                            }
                        },
                        "Cancel"
                    }
                }
            } else {
                button { class: "active", onclick: move |_| start(ui, format()), "Render…" }
            }
        }
    }
}
