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
    active: null, pending: null, retry: null, queue: [], seq: 0, wanted: null, waiting: null,
    loads: 0, swaps: 0, applied: 0, failed: 0,
    selection: [], send: null, outbox: [], drag: null, down: null,
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
  // Loads a rebuilt preview into the hidden frame. At start-up the frames
  // may not exist yet, or sit in a stage that is not laid out
  // (`display: none`); a page started there measures everything as zero,
  // so wait until the frame has a layout box.
  fc.load = (url) => {
    fc.wanted = url;
    if (fc.waiting) return;
    const t0 = performance.now();
    const tryLoad = () => {
      fc.waiting = null;
      const all = frames();
      const next = all.find((f) => f !== fc.active) || all[0];
      const ready = next && next.getClientRects().length > 0 && next.clientWidth > 0;
      if (!ready && performance.now() - t0 < 10000) {
        fc.waiting = requestAnimationFrame(tryLoad);
        return;
      }
      if (!next) return;
      fc.pending = next;
      fc.queue = [];
      next.src = fc.wanted;
    };
    tryLoad();
  };
  fc.swap = () => {
    const next = fc.pending;
    if (!next) return;
    fc.pending = null;
    if (fc.retry) { clearInterval(fc.retry); fc.retry = null; }
    for (const m of fc.queue) fc.postTo(next, m);
    fc.queue = [];
    fc.postTo(next, { type: 'ionframe-spx', action: 'goToTime', timestamp: fc.time });
    fc.postTo(next, { type: 'framecraft-editor', action: 'select', ids: fc.selection });
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
    } else if (m.type === 'framecraft-editor' && m.action === 'geometry') {
      if (fc.active && e.source === fc.active.contentWindow) fc.emit({ type: 'geometry', items: m.items || [] });
    } else if (m.type === 'framecraft-editor' && m.action === 'hits') {
      if (fc.down && m.seq === fc.down.seq) fc.releaseDown(m.ids || []);
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

  // Events for Rust (see `events`): queued until the channel is open.
  fc.emit = (m) => {
    if (fc.send) fc.send(m);
    else fc.outbox.push(m);
  };
  fc.select = (ids) => {
    fc.selection = ids;
    fc.post({ type: 'framecraft-editor', action: 'select', ids });
  };

  // Pointer input on the viewport overlay, in scene coordinates. A press
  // first asks the page what is under it; moves arriving meanwhile wait,
  // so Rust always sees down, moves, up in order.
  // The overlay reaches `data-margin` px past the graphic on every side.
  const scenePoint = (ov, e) => {
    const r = ov.getBoundingClientRect();
    const m = parseFloat(ov.dataset.margin) || 0;
    const w = parseFloat(ov.dataset.w) || r.width - 2 * m;
    const s = (r.width - 2 * m) / w || 1;
    return { x: (e.clientX - r.left - m) / s, y: (e.clientY - r.top - m) / s };
  };
  const pointerEvent = (kind, ov, e) => Object.assign(
    { type: 'pointer', kind, shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey || e.metaKey },
    scenePoint(ov, e)
  );
  fc.releaseDown = (hits) => {
    const d = fc.down;
    if (!d) return;
    fc.down = null;
    d.event.hits = hits;
    fc.emit(d.event);
    for (const m of d.queue) fc.emit(m);
  };
  document.addEventListener('pointerdown', (e) => {
    const ov = e.target && e.target.closest ? e.target.closest('#fc-overlay') : null;
    if (!ov || e.button !== 0) return;
    e.preventDefault();
    ov.setPointerCapture(e.pointerId);
    const ev = pointerEvent('down', ov, e);
    const seq = ++fc.seq;
    fc.drag = { ov, pointer: e.pointerId };
    fc.down = { seq, event: ev, queue: [] };
    fc.post({ type: 'framecraft-editor', action: 'hit', x: ev.x, y: ev.y, seq });
    // No page to ask (or it does not answer): go on without hits.
    setTimeout(() => { if (fc.down && fc.down.seq === seq) fc.releaseDown([]); }, 250);
  });
  const onMove = (kind) => (e) => {
    const d = fc.drag;
    if (!d || e.pointerId !== d.pointer) return;
    const ev = pointerEvent(kind, d.ov, e);
    if (kind !== 'move') fc.drag = null;
    if (fc.down) fc.down.queue.push(ev);
    else fc.emit(ev);
  };
  document.addEventListener('pointermove', onMove('move'));
  document.addEventListener('pointerup', onMove('up'));
  document.addEventListener('pointercancel', onMove('up'));

  // Outliner rows (`.layer-row[data-id]`) are dragged here, in the page:
  // Rust only hears the drop. Over a row, the top 30% drops above it, the
  // bottom 30% below it, and the middle inside it.
  const clearDrop = () => {
    for (const el of document.querySelectorAll('.drop-above, .drop-into, .drop-below, .drag-source')) {
      el.classList.remove('drop-above', 'drop-into', 'drop-below', 'drag-source');
    }
  };
  document.addEventListener('pointerdown', (e) => {
    const row = e.target && e.target.closest ? e.target.closest('.layer-row[data-id]') : null;
    if (!row || e.button !== 0 || e.target.closest('button, input, .caret')) return;
    e.preventDefault(); // no text selection; click and dblclick still fire
    fc.rowDrag = { id: row.dataset.id, row, x: e.clientX, y: e.clientY, pointer: e.pointerId, moved: false, target: null, zone: null };
  });
  document.addEventListener('pointermove', (e) => {
    const d = fc.rowDrag;
    if (!d || e.pointerId !== d.pointer) return;
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) return;
    if (!d.moved) { d.moved = true; d.row.classList.add('drag-source'); document.body.classList.add('row-dragging'); }
    for (const el of document.querySelectorAll('.drop-above, .drop-into, .drop-below')) {
      el.classList.remove('drop-above', 'drop-into', 'drop-below');
    }
    d.target = null;
    const hit = document.elementFromPoint(e.clientX, e.clientY);
    const over = hit && hit.closest ? hit.closest('.layer-row[data-id]') : null;
    if (!over || over.dataset.id === d.id) return;
    const r = over.getBoundingClientRect();
    const f = (e.clientY - r.top) / r.height;
    d.zone = f < 0.3 ? 'above' : f > 0.7 ? 'below' : 'into';
    d.target = over.dataset.id;
    over.classList.add('drop-' + d.zone);
  });
  const endRowDrag = (drop) => (e) => {
    const d = fc.rowDrag;
    if (!d || e.pointerId !== d.pointer) return;
    fc.rowDrag = null;
    clearDrop();
    document.body.classList.remove('row-dragging');
    if (!d.moved) return;
    // A drag is not a click: swallow the click that follows.
    const swallow = (c) => { c.stopPropagation(); c.preventDefault(); };
    window.addEventListener('click', swallow, true);
    setTimeout(() => window.removeEventListener('click', swallow, true), 0);
    if (drop && d.target) fc.emit({ type: 'outliner-drop', id: d.id, target: d.target, zone: d.zone });
  };
  document.addEventListener('pointerup', endRowDrag(true));
  document.addEventListener('pointercancel', endRowDrag(false));

  // Timeline: keys (`.tl-key[data-item][data-lane?][data-time]`), cues
  // (`.tl-cue[data-cue]`) and layer bars (`.tl-span`) are dragged here, and
  // empty track space box-selects. Rust hears `{type: 'tl', action, …}`.
  fc.snapStep = 0;
  const tlEmit = (m) => fc.emit(Object.assign({ type: 'tl' }, m));
  const keyData = (el) => ({ item: el.dataset.item, lane: el.dataset.lane || null, time: parseFloat(el.dataset.time) });
  fc.tlDelta = (primary, raw) => {
    const s = fc.snapStep;
    return s > 0 ? Math.round((primary + raw) / s) * s - primary : Math.round(raw);
  };
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const t = e.target;
    const key = t.closest('.tl-key');
    const cue = t.closest('.tl-cue');
    const span = t.closest('.tl-span');
    const surface = t.closest('.tl-lanes .track');
    if (!key && !cue && !span && !surface) return;
    e.preventDefault();
    const d = { x: e.clientX, y: e.clientY, pointer: e.pointerId, moved: false, delta: 0, els: [] };
    if (key) {
      const k = keyData(key);
      const extra = e.shiftKey || e.ctrlKey || e.metaKey;
      tlEmit(Object.assign({ action: 'key-down', shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey }, k));
      Object.assign(d, { kind: 'key', primary: k.time, extra, k, el: key });
    } else if (cue) {
      tlEmit({ action: 'cue-down', id: cue.dataset.cue });
      Object.assign(d, { kind: 'cue', id: cue.dataset.cue, primary: parseFloat(cue.dataset.time), els: [cue] });
    } else if (span) {
      const row = span.closest('.tl-row');
      Object.assign(d, { kind: 'span', item: span.dataset.item, primary: parseFloat(span.dataset.start),
        els: row ? [...row.querySelectorAll('.tl-key, .tl-span')] : [span] });
    } else {
      Object.assign(d, { kind: 'box', root: t.closest('.tl-inner'), track: surface, additive: e.shiftKey || e.ctrlKey || e.metaKey, box: null });
    }
    fc.tlDrag = d;
  });
  document.addEventListener('pointermove', (e) => {
    const d = fc.tlDrag;
    if (!d || e.pointerId !== d.pointer) return;
    const dx = e.clientX - d.x, dy = e.clientY - d.y;
    if (!d.moved && Math.hypot(dx, dy) < 4) return;
    if (!d.moved) {
      d.moved = true;
      if (d.kind === 'key') {
        d.els = [...document.querySelectorAll('.tl-key.selected')];
        if (!d.els.includes(d.el)) d.els = [d.el];
      }
    }
    if (d.kind === 'box') {
      if (!d.root) return;
      if (!d.box) { d.box = document.createElement('div'); d.box.className = 'tl-box'; d.root.appendChild(d.box); }
      const r = d.root.getBoundingClientRect();
      const x1 = Math.min(d.x, e.clientX), x2 = Math.max(d.x, e.clientX);
      const y1 = Math.min(d.y, e.clientY), y2 = Math.max(d.y, e.clientY);
      Object.assign(d.box.style, { left: (x1 - r.left) + 'px', top: (y1 - r.top) + 'px', width: (x2 - x1) + 'px', height: (y2 - y1) + 'px' });
      d.rect = { x1, x2, y1, y2 };
      return;
    }
    d.delta = fc.tlDelta(d.primary, dx / (fc.pxPerMs || 0.1));
    for (const el of d.els) el.style.translate = (d.delta * fc.pxPerMs) + 'px 0';
  });
  const endTl = (commit) => (e) => {
    const d = fc.tlDrag;
    if (!d || e.pointerId !== d.pointer) return;
    fc.tlDrag = null;
    // Rust redraws the moved items; clear the drag offset after that.
    setTimeout(() => { for (const el of d.els) el.style.translate = ''; }, 120);
    if (d.box) d.box.remove();
    if (!commit) return;
    if (d.kind === 'key') {
      if (d.moved) tlEmit({ action: 'key-drag', delta: d.delta, primary: d.primary });
      else if (!d.extra) tlEmit(Object.assign({ action: 'key-click' }, d.k));
    } else if (d.kind === 'cue') {
      if (d.moved) tlEmit({ action: 'cue-drag', id: d.id, delta: d.delta, primary: d.primary });
    } else if (d.kind === 'span') {
      if (d.moved) tlEmit({ action: 'span-drag', item: d.item, delta: d.delta, primary: d.primary });
    } else if (d.kind === 'box') {
      if (d.moved && d.rect && d.root) {
        const keys = [];
        for (const el of d.root.querySelectorAll('.tl-key')) {
          const r = el.getBoundingClientRect();
          if (r.right >= d.rect.x1 && r.left <= d.rect.x2 && r.bottom >= d.rect.y1 && r.top <= d.rect.y2) keys.push(keyData(el));
        }
        tlEmit({ action: 'box', keys, additive: d.additive });
      } else if (d.track) {
        const r = d.track.getBoundingClientRect();
        tlEmit({ action: 'track-click', time: Math.max(0, (e.clientX - r.left) / (fc.pxPerMs || 0.1)) });
      }
    }
  };
  document.addEventListener('pointerup', endTl(true));
  document.addEventListener('pointercancel', endTl(false));
  // Ruler scrub, with pointer capture so it follows the mouse anywhere.
  // The page seeks at once; Rust hears the time once per frame.
  document.addEventListener('pointerdown', (e) => {
    const ruler = e.button === 0 && e.target && e.target.closest ? e.target.closest('[data-scrub]') : null;
    if (!ruler) return;
    e.preventDefault();
    ruler.setPointerCapture(e.pointerId);
    if (fc.playing) fc.stop(false);
    const s = { el: ruler, pointer: e.pointerId, time: 0, sent: true, raf: null };
    fc.scrub = s;
    const at = (x) => {
      const r = ruler.getBoundingClientRect();
      const max = r.width / (fc.pxPerMs || 0.1);
      return Math.min(max, Math.max(0, (x - r.left) / (fc.pxPerMs || 0.1)));
    };
    s.move = (x) => {
      s.time = at(x);
      fc.seek(s.time);
      fc.drawPlayhead(s.time);
      s.sent = false;
      if (!s.raf) s.raf = requestAnimationFrame(() => { s.raf = null; if (!s.sent) { s.sent = true; tlEmit({ action: 'scrub', time: s.time }); } });
    };
    s.move(e.clientX);
  });
  document.addEventListener('pointermove', (e) => {
    const s = fc.scrub;
    if (s && e.pointerId === s.pointer) s.move(e.clientX);
  });
  const endScrub = (e) => {
    const s = fc.scrub;
    if (!s || e.pointerId !== s.pointer) return;
    fc.scrub = null;
    if (s.raf) cancelAnimationFrame(s.raf);
    tlEmit({ action: 'scrub', time: s.time });
  };
  document.addEventListener('pointerup', endScrub);
  document.addEventListener('pointercancel', endScrub);
  document.addEventListener('wheel', (e) => {
    if (!(e.ctrlKey || e.metaKey) || !e.target || !e.target.closest || !e.target.closest('.tl-body')) return;
    e.preventDefault();
    tlEmit({ action: 'zoom', factor: e.deltaY < 0 ? 1.25 : 0.8 });
  }, { passive: false });

  // Status bar hints: the tooltip (or label) of what the pointer is over.
  fc.hint = '';
  const hintOf = (el) => {
    if (!el || !el.closest || el.closest('.status-bar')) return '';
    const t = el.closest('[title], [aria-label]');
    if (!t) return '';
    return (t.getAttribute('title') || t.getAttribute('aria-label') || '').trim();
  };
  document.addEventListener('pointerover', (e) => {
    const h = hintOf(e.target);
    if (h !== fc.hint) { fc.hint = h; fc.emit({ type: 'hint', text: h }); }
  }, true);
  document.addEventListener('pointerleave', () => {
    if (fc.hint) { fc.hint = ''; fc.emit({ type: 'hint', text: '' }); }
  });

  // Editor keys, outside text fields.
  document.addEventListener('keydown', (e) => {
    const t = e.target;
    // Ctrl+S saves from anywhere; a field being edited commits first (blur).
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
      setTimeout(() => fc.emit({ type: 'key', key: e.shiftKey ? 'save-as' : 'save', shift: e.shiftKey }), 0);
      return;
    }
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
    const k = e.key;
    const mod = e.ctrlKey || e.metaKey;
    let key = null;
    if (mod && k.toLowerCase() === 'z') key = e.shiftKey ? 'redo' : 'undo';
    else if (mod && k.toLowerCase() === 'y') key = 'redo';
    else if (mod && k.toLowerCase() === 'd') key = 'duplicate';
    else if (mod && k.toLowerCase() === 'c') key = 'copy';
    else if (mod && k.toLowerCase() === 'v') key = 'paste';
    else if (e.code === 'Comma' || e.code === 'Period') key = (mod ? 'jump-' : 'step-') + (e.code === 'Comma' ? 'prev' : 'next');
    else if (!mod && !e.altKey && k === ' ') key = 'play';
    else if (!mod && !e.altKey && k.toLowerCase() === 'k') key = 'key-toggle';
    else if (!mod && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Delete', 'Backspace', 'Escape'].includes(k)) key = k;
    if (!key) return;
    e.preventDefault();
    fc.emit({ type: 'key', key, shift: e.shiftKey });
  });

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

/// The visible preview as a PNG data URL, rendered at the scene's size by
/// the page's editor script (Export → PNG Frame).
pub async fn snapshot(width: f64, height: f64) -> Result<String, String> {
    snapshot_sized(width, height, None).await
}

/// A small JPEG of the visible preview (`thumb` = its pixel size), as a
/// data URL: graphic thumbnails.
pub async fn thumbnail(width: f64, height: f64, thumb: (u32, u32)) -> Result<String, String> {
    snapshot_sized(width, height, Some(thumb)).await
}

async fn snapshot_sized(
    width: f64,
    height: f64,
    thumb: Option<(u32, u32)>,
) -> Result<String, String> {
    let thumb = thumb.map_or("null".to_string(), |(w, h)| format!("[{w}, {h}]"));
    let mut eval = document::eval(&format!(
        r#"
        const f = window.__fc;
        if (!f || !f.active) {{ dioxus.send({{ error: 'No preview' }}); return; }}
        const seq = 'snap' + Date.now() + Math.random();
        const got = await new Promise((done) => {{
          const on = (e) => {{
            const m = e.data;
            if (m && m.type === 'framecraft-editor' && m.action === 'snapshot' && m.seq === seq) {{
              window.removeEventListener('message', on);
              done(m);
            }}
          }};
          window.addEventListener('message', on);
          f.postTo(f.active, {{ type: 'framecraft-editor', action: 'snapshot', width: {width}, height: {height}, thumb: {thumb}, seq }});
          setTimeout(() => {{ window.removeEventListener('message', on); done({{ error: 'Timed out' }}); }}, 30000);
        }});
        dioxus.send(got);
        "#
    ));
    let reply: Value = eval.recv().await.map_err(|e| format!("{e:?}"))?;
    match reply["dataUrl"].as_str() {
        Some(url) => Ok(url.to_string()),
        None => Err(reply["error"]
            .as_str()
            .unwrap_or("Snapshot failed")
            .to_string()),
    }
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

/// Timeline scale (pixels per ms) and label width, for the playhead and
/// key drags.
pub fn set_timeline_scale(px_per_ms: f64, label_width: f64) {
    let _ = document::eval(&format!(
        "if (window.__fc) {{ window.__fc.pxPerMs = {px_per_ms}; window.__fc.labelWidth = {label_width}; }}"
    ));
}

/// Snap step for key drags (0 = off).
pub fn set_snap(step_ms: f64) {
    let _ = document::eval(&format!(
        "if (window.__fc) window.__fc.snapStep = {step_ms};"
    ));
}

/// Tells the editor script which layers to track (it then reports their
/// geometry).
pub fn select(ids: &[String]) {
    let ids = serde_json::to_string(ids).expect("ids serialize");
    let _ = document::eval(&format!("window.__fc && window.__fc.select({ids});"));
}

/// Delivers events from the page to `on_event`, for as long as the page
/// lives: `{type: "pointer", kind: "down"|"move"|"up", x, y, shift, alt,
/// ctrl, hits?}`, `{type: "geometry", items: [{id, corners}]}` and
/// `{type: "key", key, shift}` (`key`: `undo`, `redo`, `duplicate` or a
/// DOM key name).
pub async fn events(mut on_event: impl FnMut(Value)) {
    install();
    let mut eval = document::eval(
        r#"
const fc = window.__fc;
fc.send = (m) => dioxus.send(m);
const waiting = fc.outbox;
fc.outbox = [];
for (const m of waiting) fc.send(m);
await new Promise(() => {});
"#,
    );
    while let Ok(event) = eval.recv::<Value>().await {
        on_event(event);
    }
}
