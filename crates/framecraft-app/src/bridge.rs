// SPDX-License-Identifier: AGPL-3.0-or-later
//! The JavaScript side of the viewport, in the app's page.
//!
//! The preview runs on its own origin (`framecraft://`), so the app talks to
//! it only by `postMessage`: the runtime's own commands (`goToTime`, `ping`)
//! and the editor script's (`framecraft-editor`).
//!
//! **Two frames.** A rebuilt preview loads into the hidden frame. The bridge
//! seeks it to the playhead until its runtime answers, replays edits made
//! meanwhile, and only then swaps it in, so the viewport never shows a blank
//! or half-loaded page.
//!
//! **Playback** runs here at display rate; Rust hears only the final time.

use dioxus::prelude::*;
use serde_json::Value;

/// Iframe ids; the bridge decides which one is shown.
pub const FRAME_A: &str = "fc-preview-a";
pub const FRAME_B: &str = "fc-preview-b";

const BRIDGE_JS: &str = r#"
if (!window.__fc) {
  const fc = (window.__fc = {
    time: 0, playing: false, raf: null, onStop: null,
    labelWidth: 200, pxPerMs: 0.1,
    active: null, pending: null, retry: null, queue: [], seq: 0,
    loads: 0, swaps: 0, applied: 0, failed: 0,
  });
  const frames = () => ['fc-preview-a', 'fc-preview-b'].map((id) => document.getElementById(id)).filter(Boolean);
  fc.postTo = (frame, msg) => {
    if (frame && frame.contentWindow) frame.contentWindow.postMessage(msg, '*');
  };
  fc.post = (msg) => fc.postTo(fc.active, msg);
  fc.seek = (t) => {
    fc.time = t;
    fc.post({ type: 'ionframe-spx', action: 'goToTime', timestamp: t });
  };
  // Live edits for the editor script. While a rebuild loads they are also
  // kept, and replayed to the new page before it is shown.
  fc.edit = (msg) => {
    msg.seq = ++fc.seq;
    fc.post(msg);
    if (fc.pending) fc.queue.push(msg);
  };
  // Loads a rebuilt preview into the hidden frame.
  fc.load = (url) => {
    const all = frames();
    const next = all.find((f) => f !== fc.active) || all[0];
    if (!next) return;
    fc.pending = next;
    fc.queue = [];
    next.src = url;
  };
  fc.swap = () => {
    const next = fc.pending;
    if (!next) return;
    fc.pending = null;
    if (fc.retry) { clearInterval(fc.retry); fc.retry = null; }
    for (const m of fc.queue) fc.postTo(next, m);
    fc.queue = [];
    fc.postTo(next, { type: 'ionframe-spx', action: 'goToTime', timestamp: fc.time });
    next.classList.add('active');
    const old = fc.active;
    fc.active = next;
    fc.swaps++;
    if (old && old !== next) {
      old.classList.remove('active');
      // Free the old page once the new one is on screen.
      setTimeout(() => { if (old !== fc.active && old !== fc.pending) old.src = 'about:blank'; }, 100);
    }
  };
  window.addEventListener('message', (e) => {
    const m = e.data;
    if (!m) return;
    if (m.type === 'ionframe-spx' && (m.action === 'pong' || m.action === 'goToTimeDone')) {
      if (fc.pending && e.source === fc.pending.contentWindow) fc.swap();
    } else if (m.type === 'framecraft-editor' && m.action === 'applied') {
      if (m.ok) fc.applied++; else fc.failed++;
    }
  });
  // Iframe load events do not bubble; listen in the capture phase. Seek the
  // new page until its runtime (which starts asynchronously) answers.
  document.addEventListener('load', (e) => {
    const f = e.target;
    if (!f || f !== fc.pending) return;
    fc.loads++;
    if (fc.retry) clearInterval(fc.retry);
    let n = 0;
    fc.retry = setInterval(() => {
      fc.postTo(f, { type: 'ionframe-spx', action: 'goToTime', timestamp: fc.time });
      fc.postTo(f, { type: 'ionframe-spx', action: 'ping' });
      if (++n >= 80) fc.swap(); // runtime never answered: show it anyway
    }, 50);
  }, true);

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
}
"#;

/// Installs the bridge (once per page; later calls do nothing).
pub fn install() {
    let _ = document::eval(BRIDGE_JS);
}

/// Loads a rebuilt preview behind the visible one and swaps when ready.
pub fn load(url: &str) {
    let url = serde_json::to_string(url).expect("strings serialize");
    let _ = document::eval(&format!("window.__fc && window.__fc.load({url});"));
}

/// Sends a live edit to the editor script.
pub fn edit(msg: &Value) {
    let _ = document::eval(&format!("window.__fc && window.__fc.edit({msg});"));
}

pub fn seek(time_ms: f64) {
    let _ = document::eval(&format!("window.__fc && window.__fc.seek({time_ms});"));
}

/// Stops playback; with `report`, the play call's callback gets the time.
pub fn stop(report: bool) {
    let _ = document::eval(&format!("window.__fc && window.__fc.stop({report});"));
}

/// Plays from `from_ms` and resolves with the time playback stopped at
/// (`None` when a scrub stopped it).
pub async fn play(from_ms: f64, duration_ms: f64) -> Option<f64> {
    let mut eval = document::eval(&format!(
        "await new Promise((done) => window.__fc.play({from_ms}, {duration_ms}, (t) => {{ dioxus.send(t); done(); }}));"
    ));
    eval.recv::<Option<f64>>().await.ok().flatten()
}

/// The bridge's counters, for `FRAMECRAFT_DEBUG`.
pub async fn stats() -> Option<Value> {
    document::eval(
        "const f = window.__fc; return f ? { loads: f.loads, swaps: f.swaps, applied: f.applied, failed: f.failed, active: f.active && f.active.id, pending: !!f.pending } : null;",
    )
    .join::<Value>()
    .await
    .ok()
}

/// Delivers Undo / Redo keys pressed outside text fields.
pub async fn shortcuts(mut on_key: impl FnMut(&str)) {
    let mut eval = document::eval(
        r#"
document.addEventListener('keydown', (e) => {
  const t = e.target;
  const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
  if (typing || !(e.ctrlKey || e.metaKey)) return;
  const k = e.key.toLowerCase();
  if (k === 'z' && !e.shiftKey) { e.preventDefault(); dioxus.send('undo'); }
  else if ((k === 'z' && e.shiftKey) || k === 'y') { e.preventDefault(); dioxus.send('redo'); }
});
await new Promise(() => {});
"#,
    );
    while let Ok(key) = eval.recv::<String>().await {
        on_key(&key);
    }
}
