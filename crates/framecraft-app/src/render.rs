// SPDX-License-Identifier: AGPL-3.0-or-later
//! Render to video (Properties → Export → Video): the open graphic, frame
//! by frame at its size and frame rate, as a PNG sequence or — with FFmpeg —
//! ProRes 4444 / WebM VP9 (both with alpha) or MP4 H.264. This is MENiAC
//! Air's offline render done in the editor: the preview is seeked
//! (`goToTime`) and captured for each frame. The whole timeline renders
//! through pause cues, holds at them, or splits there into segments plus
//! stills (MENiAC Air's pause modes).
//!
//! FFmpeg is found on PATH, or set with `FRAMECRAFT_FFMPEG`.

use crate::state::Ui;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
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

/// How pause cues are rendered (MENiAC Air's pause modes).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pauses {
    /// One file, straight through.
    Through,
    /// One file; each pause's frame is held for a while.
    Hold,
    /// A motion clip per segment, a still at each pause, and a sidecar.
    Segments,
}

/// Pause cue times (time + delay) inside the timeline, as in Air's
/// manifest (`pause_times_ms`).
fn pause_times(doc: &framecraft_core::normalize::Document) -> Vec<f64> {
    let duration = doc.timeline.duration.max(1.0);
    let mut out: Vec<f64> = doc
        .timeline
        .scene_cues
        .iter()
        .flatten()
        .filter(|c| c.kind == framecraft_core::model::CueKind::Pause)
        .map(|c| (c.time + c.delay_ms.unwrap_or(0.0).max(0.0)).round())
        .filter(|t| *t > 0.0 && *t < duration)
        .collect();
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// Where frames go: numbered PNGs in a folder, or FFmpeg.
enum Sink {
    Pngs {
        dir: PathBuf,
        base: String,
        next: usize,
    },
    Video(Child),
}

impl Sink {
    fn open(format: &str, fps: f64, path: &Path, base: &str) -> Result<Self, String> {
        if format == "png" {
            // Some file pickers create the chosen file; the sequence is a folder.
            if path.is_file() && std::fs::metadata(path).is_ok_and(|m| m.len() == 0) {
                let _ = std::fs::remove_file(path);
            }
            std::fs::create_dir_all(path)
                .map_err(|e| format!("Could not create {}: {e}", path.display()))?;
            return Ok(Sink::Pngs {
                dir: path.to_path_buf(),
                base: base.to_string(),
                next: 0,
            });
        }
        let ff = ffmpeg().ok_or("FFmpeg not found (install it, or set FRAMECRAFT_FFMPEG)")?;
        encoder(&ff, format, fps, &path.to_path_buf())
            .map(Sink::Video)
            .map_err(|e| format!("Could not start FFmpeg: {e}"))
    }

    fn push(&mut self, png: &[u8]) -> Result<(), String> {
        match self {
            Sink::Pngs { dir, base, next } => {
                let file = dir.join(format!("{base}_{:05}.png", *next));
                *next += 1;
                std::fs::write(&file, png).map_err(|e| e.to_string())
            }
            Sink::Video(child) => child
                .stdin
                .as_mut()
                .ok_or("FFmpeg input closed")?
                .write_all(png)
                .map_err(|e| e.to_string()),
        }
    }

    fn finish(self) -> Result<(), String> {
        let Sink::Video(mut child) = self else {
            return Ok(());
        };
        drop(child.stdin.take()); // end of input: FFmpeg finishes the file
        let o = child
            .wait_with_output()
            .map_err(|e| format!("FFmpeg: {e}"))?;
        if o.status.success() {
            Ok(())
        } else {
            let msg = String::from_utf8_lossy(&o.stderr);
            Err(format!(
                "FFmpeg failed: {}",
                msg.lines().last().unwrap_or("unknown error")
            ))
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
type Native = Option<crate::capture::Capture>;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
type Native = ();

/// Linux: native capture in a hidden view (canvases, images, video
/// included); elsewhere the in-page capture of the viewport.
async fn open_native(w: f64, h: f64) -> Native {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        match crate::bridge::active_url().await {
            Some(url) => crate::capture::Capture::open(&url, w, h).await.ok(),
            None => None,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (w, h);
    }
}

/// The graphic at `t` as a PNG.
async fn grab(native: &Native, w: f64, h: f64, t: f64) -> Result<Vec<u8>, String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Some(c) = native {
        return c.frame(t).await;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = native;
    crate::bridge::seek(t);
    let url = crate::bridge::snapshot(w, h).await?;
    url.split_once(',')
        .and_then(|(_, b)| crate::export::decode_base64(b))
        .ok_or_else(|| "unreadable image data".to_string())
}

/// Frame numbers in `[a, b)` ms at `fps`.
fn frames_in(a: f64, b: f64, fps: f64) -> std::ops::Range<usize> {
    let at = |ms: f64| (ms * fps / 1000.0 - 1e-6).ceil().max(0.0) as usize;
    at(a)..at(b)
}

/// Counts frames done; stops when Cancel was pressed.
struct Run {
    progress: Signal<Option<Progress>>,
    done: usize,
    total: usize,
}

impl Run {
    fn tick(&mut self) -> Result<(), String> {
        let cancel = self.progress.peek().is_some_and(|p| p.cancel);
        if cancel {
            return Err("Render cancelled".into());
        }
        self.done += 1;
        self.progress.set(Some(Progress {
            done: self.done,
            total: self.total,
            cancel: false,
        }));
        Ok(())
    }
}

/// Everything a render needs.
struct Job {
    format: &'static str,
    ext: &'static str,
    mode: Pauses,
    hold_ms: f64,
    w: f64,
    h: f64,
    fps: f64,
    duration: f64,
    pauses: Vec<f64>,
    base: String,
}

/// Renders `job` to `out` (a file, or a folder for PNG sequences).
async fn render(job: &Job, out: &Path, native: &Native, run: &mut Run) -> Result<String, String> {
    let fps = job.fps;
    let mut edges = vec![0.0];
    edges.extend(job.pauses.iter().copied());
    edges.push(job.duration);
    let segments: Vec<(f64, f64, bool)> = edges
        .windows(2)
        .map(|w| (w[0], w[1], w[1] < job.duration))
        .collect();
    let hold_frames = ((job.hold_ms / 1000.0) * fps).round().max(1.0) as usize;
    let frame_t = |i: usize| (i as f64 * 1000.0 / fps).min(job.duration);
    match job.mode {
        Pauses::Through | Pauses::Hold => {
            run.total = frames_in(0.0, job.duration, fps).len()
                + if job.mode == Pauses::Hold {
                    job.pauses.len() * hold_frames
                } else {
                    0
                };
            let mut sink = Sink::open(job.format, fps, out, &job.base)?;
            for (a, b, pause) in &segments {
                for i in frames_in(*a, *b, fps) {
                    sink.push(&grab(native, job.w, job.h, frame_t(i)).await?)?;
                    run.tick()?;
                }
                if job.mode == Pauses::Hold && *pause {
                    let still = grab(native, job.w, job.h, *b).await?;
                    for _ in 0..hold_frames {
                        sink.push(&still)?;
                        run.tick()?;
                    }
                }
            }
            sink.finish()?;
            Ok(format!("Rendered {} frames to {}", run.done, out.display()))
        }
        Pauses::Segments => {
            // A video's segments go next to the chosen file; a PNG
            // sequence's into the chosen (new) folder.
            let (dir, stem) = if job.format == "png" {
                if out.is_file() && std::fs::metadata(out).is_ok_and(|m| m.len() == 0) {
                    let _ = std::fs::remove_file(out);
                }
                std::fs::create_dir_all(out)
                    .map_err(|e| format!("Could not create {}: {e}", out.display()))?;
                (out.to_path_buf(), job.base.clone())
            } else {
                let stem = out
                    .file_stem()
                    .map_or(job.base.clone(), |s| s.to_string_lossy().into_owned());
                (
                    out.parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
                    stem,
                )
            };
            run.total = frames_in(0.0, job.duration, fps).len() + job.pauses.len();
            let mut items: Vec<Value> = Vec::new();
            for (n, (a, b, pause)) in segments.iter().enumerate() {
                let n = n + 1;
                let name = if job.format == "png" {
                    format!("{n:02}")
                } else {
                    format!("{stem}_{n:02}.{}", job.ext)
                };
                let mut sink = Sink::open(job.format, fps, &dir.join(&name), &stem)?;
                for i in frames_in(*a, *b, fps) {
                    sink.push(&grab(native, job.w, job.h, frame_t(i)).await?)?;
                    run.tick()?;
                }
                sink.finish()?;
                items.push(json!({ "kind": "motion", "file": name, "startMs": a, "endMs": b }));
                if *pause {
                    let still = format!("{stem}_{n:02}_hold.png");
                    let png = grab(native, job.w, job.h, *b).await?;
                    std::fs::write(dir.join(&still), png).map_err(|e| e.to_string())?;
                    run.tick()?;
                    items.push(json!({ "kind": "hold", "file": still, "atMs": b }));
                }
            }
            // Air's output sidecar: the motion and hold files in order.
            let sidecar = dir.join(format!("{stem}_render.json"));
            let doc = json!({
                "generator": "FrameCraft",
                "version": env!("CARGO_PKG_VERSION"),
                "width": job.w,
                "height": job.h,
                "fps": fps,
                "durationMs": job.duration,
                "items": items,
            });
            std::fs::write(
                &sidecar,
                serde_json::to_vec_pretty(&doc).unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(format!(
                "Rendered {} segment(s) and {} still(s) to {}",
                segments.len(),
                job.pauses.len(),
                dir.display()
            ))
        }
    }
}

/// Asks where to write, then renders.
fn start(ui: Ui, format: &'static str, mode: Pauses, hold_ms: f64) {
    let Some((name, w, h, fps, duration, pauses, folder)) = ui.editor.peek().as_ref().map(|e| {
        let d = e.doc();
        (
            e.project.graphic_name(e.graphic).to_string(),
            d.scene.width,
            d.scene.height,
            d.timeline.fps.max(1.0),
            d.timeline.duration.max(1.0),
            pause_times(d),
            e.project.folder(),
        )
    }) else {
        return;
    };
    let ext = FORMATS.iter().find(|f| f.0 == format).map_or("", |f| f.2);
    let job = Job {
        format,
        ext,
        mode: if pauses.is_empty() {
            Pauses::Through
        } else {
            mode
        },
        hold_ms,
        w,
        h,
        fps,
        duration,
        pauses,
        base: framecraft_core::export::names::file_base_name(&name, "render"),
    };
    spawn_forever(async move {
        let mut status = ui.status;
        let base = &job.base;
        let picked = if ext.is_empty() {
            rfd::AsyncFileDialog::new()
                .set_title("Render PNG sequence: name of the new folder")
                .set_directory(&folder)
                .set_file_name(format!("{base}-frames"))
                .save_file()
                .await
        } else {
            rfd::AsyncFileDialog::new()
                .set_title(if job.mode == Pauses::Segments {
                    "Render video segments: base name (segments, stills and a sidecar go next to it)"
                } else {
                    "Render video"
                })
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
        let resume_at = *ui.time.peek();
        let mut progress = ui.render;
        progress.set(Some(Progress::default()));
        let native = open_native(w, h).await;
        let mut run = Run {
            progress,
            done: 0,
            total: 0,
        };
        let result = render(&job, &out, &native, &mut run).await;
        drop(native);
        progress.set(None);
        crate::bridge::seek(resume_at);
        status.set(match result {
            Ok(msg) => msg,
            Err(e) => e,
        });
    });
}

/// Properties → Export → Video.
#[component]
pub fn RenderSection(ui: Ui) -> Element {
    let mut format = use_signal(|| "png");
    let mut mode = use_signal(|| Pauses::Segments);
    let mut hold_s = use_signal(|| 4.0_f64);
    let has_ffmpeg = use_hook(|| ffmpeg().is_some());
    let info = ui.editor.read().as_ref().map(|e| {
        let d = e.doc();
        (
            d.scene.width,
            d.timeline.fps,
            d.scene.height,
            d.timeline.duration,
            pause_times(d).len(),
        )
    });
    let Some((w, fps, h, duration, pauses)) = info else {
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
                    "The whole timeline at {w}×{h}, {fps} fps: {frames} frames."
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
                if pauses > 0 {
                    // MENiAC Air's pause modes.
                    select {
                        disabled: progress.is_some(),
                        title: "How the {pauses} pause cue(s) are rendered",
                        onchange: move |evt| {
                            mode.set(match evt.value().as_str() {
                                "through" => Pauses::Through,
                                "hold" => Pauses::Hold,
                                _ => Pauses::Segments,
                            });
                        },
                        option { value: "segments", selected: mode() == Pauses::Segments,
                            "Pauses: segments + stills ({pauses + 1} clips, {pauses} stills)"
                        }
                        option { value: "hold", selected: mode() == Pauses::Hold, "Pauses: hold in one file" }
                        option { value: "through", selected: mode() == Pauses::Through, "Pauses: straight through" }
                    }
                    if mode() == Pauses::Hold {
                        label { class: "dim",
                            "Hold (s) "
                            input {
                                r#type: "number",
                                min: "0.04",
                                step: "0.5",
                                value: "{hold_s}",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().trim().parse::<f64>() {
                                        hold_s.set(v.max(0.04));
                                    }
                                },
                            }
                        }
                    }
                    if mode() == Pauses::Segments {
                        p { class: "dim small",
                            "A clip up to each pause, a still at each pause (for holding in an editor) and a *_render.json listing them in order — as MENiAC Air renders."
                        }
                    }
                }
                if !has_ffmpeg {
                    p { class: "dim small", "Video formats need FFmpeg: install it, or set FRAMECRAFT_FFMPEG to its path." }
                }
            }
            if let Some(p) = progress {
                div { class: "render-progress",
                    progress { max: "{p.total.max(1)}", value: "{p.done}" }
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
                button {
                    class: "active",
                    onclick: move |_| start(ui, format(), mode(), hold_s() * 1000.0),
                    "Render…"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::frames_in;

    #[test]
    fn segments_share_no_frames() {
        // 25 fps, pause at 1 s: 0..25 then 25..50, nothing lost or doubled.
        assert_eq!(frames_in(0.0, 1000.0, 25.0), 0..25);
        assert_eq!(frames_in(1000.0, 2000.0, 25.0), 25..50);
        // A pause between frames (1010 ms) starts the next segment after it.
        assert_eq!(frames_in(0.0, 1010.0, 25.0), 0..26);
        assert_eq!(frames_in(1010.0, 2000.0, 25.0), 26..50);
        // The whole timeline matches the straight-through count.
        assert_eq!(frames_in(0.0, 4000.0, 30.0).len(), 120);
    }
}
