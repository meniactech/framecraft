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
    // What the loader sees (FRAMECRAFT_DEBUG): frames, sizes, the stage.
    const describe = (all) => {
      const stage = document.querySelector('.stage');
      return 'frames=' + all.length + ' ' + all.map((f) => f.id + ':' + f.clientWidth + 'x' + f.clientHeight + ' rects=' + f.getClientRects().length).join(' ') +
        ' stage=' + (stage ? JSON.stringify(stage.getAttribute('style')) + ' ' + stage.clientWidth + 'x' + stage.clientHeight : 'none');
    };
    let logged = false;
    const tryLoad = () => {
      fc.waiting = null;
      const all = frames();
      const next = all.find((f) => f !== fc.active) || all[0];
      const ready = next && next.getClientRects().length > 0 && next.clientWidth > 0;
      if (!ready && !logged) {
        logged = true;
        fc.emit({ type: 'log', text: '[load] waiting for the viewport: ' + describe(all) });
      }
      // A plain timer, not requestAnimationFrame: animation frames are not
      // delivered while nothing on screen changes (an empty viewport after
      // New), which left the load waiting for good.
      if (!ready && performance.now() - t0 < 10000) {
        fc.waiting = setTimeout(tryLoad, 30);
        return;
      }
      fc.emit({ type: 'log', text: '[load] ' + (ready ? 'loading ' : 'gave up waiting, loading ') + fc.wanted + ' into ' + (next ? next.id : 'nothing') + ' (' + describe(all) + ')' });
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
    } else if (m.type === 'framecraft-editor' && m.action === 'cue-log') {
      fc.emit({ type: 'cue', text: String(m.text || '') });
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
    const tween = key ? null : t.closest('.tl-move');
    const surface = t.closest('.tl-lanes .track');
    if (!key && !cue && !span && !tween && !surface) return;
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
    } else if (tween) {
      // FRAME: the tween's keys get selected and slide together.
      const times = (tween.dataset.keys || '').split(' ').map(parseFloat).filter((n) => !isNaN(n));
      const row = tween.closest('.tl-row');
      const same = (a, b) => Math.abs(a - b) < 1e-6;
      const els = row ? [...row.querySelectorAll('.tl-key')].filter((el) => times.some((t) => same(t, parseFloat(el.dataset.time)))) : [];
      if (row) for (const m of row.querySelectorAll('.tl-move')) if (m.dataset.keys === tween.dataset.keys) els.push(m);
      const info = { item: tween.dataset.item, lane: tween.dataset.lane, times, primary: parseFloat(tween.dataset.start) };
      tlEmit(Object.assign({ action: 'tween-down' }, info));
      Object.assign(d, { kind: 'tween', primary: info.primary, info, els });
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
    } else if (d.kind === 'tween') {
      if (d.moved) tlEmit(Object.assign({ action: 'tween-drag', delta: d.delta }, d.info));
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
  // Timeline zoom and pan: Ctrl+wheel zooms keeping the time under the
  // pointer in place, Shift+wheel pans, the middle button drags.
  document.addEventListener('wheel', (e) => {
    const body = e.target && e.target.closest ? e.target.closest('.tl-body') : null;
    if (!body) return;
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const x = e.clientX - body.getBoundingClientRect().left;
      fc.zoomAnchor = { t: (body.scrollLeft + x - fc.labelWidth) / (fc.pxPerMs || 0.1), x };
      tlEmit({ action: 'zoom', factor: e.deltaY < 0 ? 1.25 : 0.8 });
    } else if (e.shiftKey) {
      e.preventDefault();
      body.scrollLeft += e.deltaY || e.deltaX;
    }
  }, { passive: false });
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 1) return;
    const body = e.target && e.target.closest ? e.target.closest('.tl-body') : null;
    if (!body) return;
    e.preventDefault();
    body.setPointerCapture(e.pointerId);
    const start = { x: e.clientX, y: e.clientY, l: body.scrollLeft, t: body.scrollTop };
    const move = (ev) => {
      body.scrollLeft = start.l - (ev.clientX - start.x);
      body.scrollTop = start.t - (ev.clientY - start.y);
    };
    const up = () => {
      body.removeEventListener('pointermove', move);
      body.removeEventListener('pointerup', up);
      body.removeEventListener('pointercancel', up);
    };
    body.addEventListener('pointermove', move);
    body.addEventListener('pointerup', up);
    body.addEventListener('pointercancel', up);
  });
  // The timeline's width, for the fit (1×) zoom.
  fc.tlWidth = 0;
  setInterval(() => {
    const body = document.querySelector('.tl-body');
    const w = body ? body.clientWidth : 0;
    if (w > 0 && Math.abs(w - fc.tlWidth) > 0.5) { fc.tlWidth = w; tlEmit({ action: 'width', width: w }); }
  }, 250);
  // After a zoom, scroll so the anchored time is under the pointer again.
  fc.onScale = () => {
    const a = fc.zoomAnchor;
    if (!a) return;
    fc.zoomAnchor = null;
    setTimeout(() => {
      const body = document.querySelector('.tl-body');
      if (body) body.scrollLeft = Math.max(0, fc.labelWidth + a.t * fc.pxPerMs - a.x);
    }, 0);
  };

  // Viewport navigation (FRAME's): the wheel zooms around the pointer,
  // the middle button drags the view.
  document.addEventListener('wheel', (e) => {
    const v = e.target && e.target.closest ? e.target.closest('#fc-viewport') : null;
    if (!v) return;
    e.preventDefault();
    const r = v.getBoundingClientRect();
    fc.emit({ type: 'view', action: 'zoom', factor: e.deltaY < 0 ? 1.1 : 1 / 1.1,
      x: e.clientX - r.left, y: e.clientY - r.top, w: r.width, h: r.height });
  }, { passive: false });
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 1) return;
    const v = e.target && e.target.closest ? e.target.closest('#fc-viewport') : null;
    if (!v) return;
    e.preventDefault();
    v.setPointerCapture(e.pointerId);
    fc.pan = { pointer: e.pointerId, x: e.clientX, y: e.clientY, el: v };
    v.classList.add('panning');
  }, true);
  document.addEventListener('pointermove', (e) => {
    const p = fc.pan;
    if (!p || e.pointerId !== p.pointer) return;
    const dx = e.clientX - p.x, dy = e.clientY - p.y;
    p.x = e.clientX; p.y = e.clientY;
    if (dx || dy) fc.emit({ type: 'view', action: 'pan', dx, dy });
  }, true);
  const endPan = (e) => {
    const p = fc.pan;
    if (!p || e.pointerId !== p.pointer) return;
    p.el.classList.remove('panning');
    fc.pan = null;
  };
  document.addEventListener('pointerup', endPan, true);
  document.addEventListener('pointercancel', endPan, true);
  // No middle-click paste or autoscroll in the viewport.
  ['mousedown', 'auxclick'].forEach((t) => document.addEventListener(t, (e) => {
    if (e.button === 1 && e.target && e.target.closest && e.target.closest('#fc-viewport')) e.preventDefault();
  }, true));

  // Gradient editors (.ramp-track[data-ramp]): a click adds a stop, a
  // press selects one, middle stops drag with a live preview; Rust gets
  // add / select / move (on release).
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const track = e.target.closest('.ramp-track[data-ramp]');
    if (!track) return;
    e.preventDefault();
    const id = track.dataset.ramp;
    const rect = track.getBoundingClientRect();
    const off = (x) => Math.min(1, Math.max(0, (x - rect.left) / Math.max(1, rect.width)));
    const dot = e.target.closest('.ramp-dot');
    if (!dot) { fc.emit({ type: 'ramp', action: 'add', id, offset: off(e.clientX) }); return; }
    const index = parseInt(dot.dataset.index, 10);
    fc.emit({ type: 'ramp', action: 'select', id, index });
    const dots = Array.from(track.querySelectorAll('.ramp-dot'));
    if (index <= 0 || index >= dots.length - 1) return;
    const lo = parseFloat(dots[index - 1].dataset.offset) + 0.001;
    const hi = parseFloat(dots[index + 1].dataset.offset) - 0.001;
    const start = parseFloat(dot.dataset.offset);
    let cur = start;
    const fill = track.querySelector('.ramp-fill');
    const paint = () => {
      const parts = dots
        .map((d) => [d === dot ? cur : parseFloat(d.dataset.offset), d.dataset.color])
        .sort((a, b) => a[0] - b[0])
        .map(([o, c]) => c + ' ' + (o * 100).toFixed(2) + '%');
      if (fill) fill.style.background = 'linear-gradient(90deg, ' + parts.join(', ') + ')';
    };
    dot.setPointerCapture(e.pointerId);
    const move = (ev) => {
      cur = Math.min(hi, Math.max(lo, off(ev.clientX)));
      dot.style.left = (cur * 100) + '%';
      paint();
    };
    const up = () => {
      dot.removeEventListener('pointermove', move);
      dot.removeEventListener('pointerup', up);
      dot.removeEventListener('pointercancel', up);
      if (cur !== start) fc.emit({ type: 'ramp', action: 'move', id, index, offset: cur });
    };
    dot.addEventListener('pointermove', move);
    dot.addEventListener('pointerup', up);
    dot.addEventListener('pointercancel', up);
  });

  // Curve editors (.curve-track[data-curve]), values 0–1 over life: a click
  // adds a stop at that point, a press selects one, dragging moves it (the
  // ends only up and down) with a live line; Rust gets add / select / move.
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const track = e.target.closest('.curve-track[data-curve]');
    if (!track) return;
    e.preventDefault();
    const id = track.dataset.curve;
    const rect = track.getBoundingClientRect();
    const offAt = (x) => Math.min(1, Math.max(0, (x - rect.left) / Math.max(1, rect.width)));
    const valAt = (y) => Math.round(Math.min(1, Math.max(0, 1 - (y - rect.top) / Math.max(1, rect.height))) * 100) / 100;
    const dot = e.target.closest('.curve-dot');
    if (!dot) { fc.emit({ type: 'ramp', action: 'add', id, offset: offAt(e.clientX), value: valAt(e.clientY) }); return; }
    const index = parseInt(dot.dataset.index, 10);
    fc.emit({ type: 'ramp', action: 'select', id, index });
    const dots = Array.from(track.querySelectorAll('.curve-dot'));
    const end = index === 0 || index === dots.length - 1;
    const lo = end ? 0 : parseFloat(dots[index - 1].dataset.offset) + 0.001;
    const hi = end ? 1 : parseFloat(dots[index + 1].dataset.offset) - 0.001;
    const o0 = parseFloat(dot.dataset.offset), v0 = parseFloat(dot.dataset.value);
    let o = o0, v = v0;
    const line = track.querySelector('polyline');
    const paint = () => {
      if (!line) return;
      line.setAttribute('points', dots
        .map((d) => d === dot ? [o, v] : [parseFloat(d.dataset.offset), parseFloat(d.dataset.value)])
        .sort((a, b) => a[0] - b[0])
        .map(([x, y]) => (x * 100).toFixed(2) + ',' + (100 - y * 100).toFixed(2))
        .join(' '));
    };
    dot.setPointerCapture(e.pointerId);
    const move = (ev) => {
      if (!end) o = Math.min(hi, Math.max(lo, offAt(ev.clientX)));
      v = valAt(ev.clientY);
      dot.style.left = (o * 100) + '%';
      dot.style.top = (100 - v * 100) + '%';
      paint();
    };
    const up = () => {
      dot.removeEventListener('pointermove', move);
      dot.removeEventListener('pointerup', up);
      dot.removeEventListener('pointercancel', up);
      if (o !== o0 || v !== v0) fc.emit({ type: 'ramp', action: 'move', id, index, offset: o, value: v });
    };
    dot.addEventListener('pointermove', move);
    dot.addEventListener('pointerup', up);
    dot.addEventListener('pointercancel', up);
  });

  // Panel splitters (.split[data-split]): drag sets a CSS variable on the
  // page root live; Rust saves the size on release. Double-click resets.
  const SPLIT_VARS = { side: '--side-w', timeline: '--tl-h', outliner: '--outliner-h' };
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const sp = e.target.closest('.split[data-split]');
    if (!sp) return;
    e.preventDefault();
    const name = sp.dataset.split;
    const v = SPLIT_VARS[name];
    if (!v) return;
    const root = document.documentElement;
    const side = document.querySelector('.side');
    const sideRect = side ? side.getBoundingClientRect() : { top: 0, height: window.innerHeight };
    const size = (ev) => {
      if (name === 'side') return Math.min(window.innerWidth - 420, Math.max(220, window.innerWidth - ev.clientX));
      if (name === 'timeline') return Math.min(window.innerHeight - 260, Math.max(120, window.innerHeight - 24 - ev.clientY));
      return Math.min(sideRect.height - 140, Math.max(80, ev.clientY - sideRect.top));
    };
    let px = null;
    sp.classList.add('dragging');
    sp.setPointerCapture(e.pointerId);
    const move = (ev) => { px = size(ev); root.style.setProperty(v, px + 'px'); };
    const up = () => {
      sp.classList.remove('dragging');
      sp.removeEventListener('pointermove', move);
      sp.removeEventListener('pointerup', up);
      sp.removeEventListener('pointercancel', up);
      if (px !== null) fc.emit({ type: 'layout', split: name, px });
    };
    sp.addEventListener('pointermove', move);
    sp.addEventListener('pointerup', up);
    sp.addEventListener('pointercancel', up);
  });
  document.addEventListener('dblclick', (e) => {
    const sp = e.target && e.target.closest ? e.target.closest('.split[data-split]') : null;
    if (!sp) return;
    const v = SPLIT_VARS[sp.dataset.split];
    if (!v) return;
    document.documentElement.style.removeProperty(v);
    fc.emit({ type: 'layout', split: sp.dataset.split, px: null });
  });

  // Ruler boxes (.ruler-handle): drag a custom ruler. With Snap on (FRAME)
  // it moves in 10 scene px steps and snaps to the other rulers and shown
  // guides within 10 px. Rust gets start / move / end (one undo step).
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const hd = e.target.closest('.ruler-handle');
    if (!hd) return;
    e.preventDefault();
    e.stopPropagation();
    const box = hd.parentElement;
    const stage = box.parentElement;
    const axis = hd.dataset.axis;
    const index = parseInt(hd.dataset.index, 10);
    const span = parseFloat(axis === 'v' ? box.dataset.w : box.dataset.h) || 1;
    const snap = box.dataset.snap === '1';
    const others = JSON.parse((axis === 'v' ? box.dataset.rulersV : box.dataset.rulersH) || '[]').filter((_, i) => i !== index);
    const targets = JSON.parse(box.dataset.guides || '[]').concat(others);
    const step = 10 / span * 100;
    const pct = (ev) => {
      const r = stage.getBoundingClientRect();
      let p = axis === 'v' ? (ev.clientX - r.left) / r.width * 100 : (ev.clientY - r.top) / r.height * 100;
      p = Math.min(100, Math.max(0, p));
      if (snap) {
        p = Math.min(100, Math.max(0, Math.round(p / step) * step));
        let best = null;
        for (const t of targets) { const d = Math.abs(t - p); if (d <= step && (best === null || d < best.d)) best = { t, d }; }
        if (best) p = best.t;
      }
      return Math.round(p * 100) / 100;
    };
    hd.setPointerCapture(e.pointerId);
    fc.emit({ type: 'ruler', action: 'start', axis, index });
    let last = null;
    const move = (ev) => {
      const p = pct(ev);
      if (p === last) return;
      last = p;
      if (axis === 'v') hd.style.left = p + '%'; else hd.style.top = p + '%';
      fc.emit({ type: 'ruler', action: 'move', axis, index, value: p });
    };
    const up = () => {
      hd.removeEventListener('pointermove', move);
      hd.removeEventListener('pointerup', up);
      hd.removeEventListener('pointercancel', up);
      fc.emit({ type: 'ruler', action: 'end', axis, index });
    };
    hd.addEventListener('pointermove', move);
    hd.addEventListener('pointerup', up);
    hd.addEventListener('pointercancel', up);
  }, true);

  // Number pills: drag the label (.scrub) to change the value, 1 step per
  // pixel (Shift ×10, Alt ×0.1). Each new value is a `change` event on the
  // input, so Rust sees it like typing; the label's own pointerdown/up
  // (Dioxus handlers) make the drag one undo step.
  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || !e.target || !e.target.closest) return;
    const lab = e.target.closest('.scrub');
    if (!lab) return;
    const input = lab.parentElement && lab.parentElement.querySelector('input');
    if (!input) return;
    const num = (s) => { const v = parseFloat(s); return Number.isFinite(v) ? v : null; };
    const step = num(lab.dataset.step) || 1;
    const min = num(lab.dataset.min), max = num(lab.dataset.max);
    const start = num(input.value) || 0;
    const decimals = (String(step).split('.')[1] || '').length;
    const x0 = e.clientX;
    let moved = false, last = null;
    lab.setPointerCapture(e.pointerId);
    const move = (ev) => {
      const dx = ev.clientX - x0;
      if (!moved && Math.abs(dx) < 3) return;
      moved = true;
      const k = ev.shiftKey ? 10 : (ev.altKey ? 0.1 : 1);
      let v = start + Math.round(dx) * step * k;
      if (min !== null) v = Math.max(min, v);
      if (max !== null) v = Math.min(max, v);
      v = +v.toFixed(Math.min(6, decimals + (ev.altKey ? 1 : 0)));
      if (v === last) return;
      last = v;
      input.value = String(v);
      input.dispatchEvent(new Event('change', { bubbles: true }));
    };
    const up = () => {
      lab.removeEventListener('pointermove', move);
      lab.removeEventListener('pointerup', up);
      lab.removeEventListener('pointercancel', up);
      // A drag is not a click: do not focus the input afterwards.
      if (moved) lab.addEventListener('click', (ev) => { ev.preventDefault(); ev.stopPropagation(); }, { capture: true, once: true });
    };
    lab.addEventListener('pointermove', move);
    lab.addEventListener('pointerup', up);
    lab.addEventListener('pointercancel', up);
  }, true);

  // Status bar hints: the tooltip (or label) of what the pointer is over.
  // As in FRAME, the hovered element's `title` is parked (moved to
  // data-fc-title) so the native tooltip does not show the same text
  // again; it is put back when the pointer leaves. A title Dioxus set
  // meanwhile wins over the parked one.
  fc.hint = '';
  let parked = null;
  const unpark = () => {
    if (!parked) return;
    const t = parked.dataset.fcTitle;
    if (t != null && !parked.hasAttribute('title')) parked.setAttribute('title', t);
    delete parked.dataset.fcTitle;
    parked = null;
  };
  const labeled = (el) => (!el || !el.closest || el.closest('.status-bar')) ? null : el.closest('[title], [data-fc-title], [aria-label]');
  const hintOf = (t) => t ? (t.getAttribute('title') || t.dataset.fcTitle || t.getAttribute('aria-label') || '').trim() : '';
  document.addEventListener('pointerover', (e) => {
    const t = labeled(e.target);
    const h = hintOf(t);
    if (t !== parked) {
      unpark();
      if (t && h && t.hasAttribute('title')) {
        t.dataset.fcTitle = t.getAttribute('title');
        t.removeAttribute('title');
        parked = t;
      }
    }
    if (h !== fc.hint) { fc.hint = h; fc.emit({ type: 'hint', text: h }); }
  }, true);
  document.addEventListener('pointerleave', () => {
    unpark();
    if (fc.hint) { fc.hint = ''; fc.emit({ type: 'hint', text: '' }); }
  });

  // Editor keys, outside text fields.
  // Fields give the keyboard back (FRAME's shortcuts): Enter commits and
  // leaves a one-line field, Escape leaves any field, and a dropdown lets
  // go once a value is picked.
  document.addEventListener('keydown', (e) => {
    const t = e.target;
    if (!t || (t.tagName !== 'INPUT' && t.tagName !== 'SELECT')) return;
    if (e.key === 'Enter' || e.key === 'Escape') setTimeout(() => { if (document.activeElement === t) t.blur(); }, 0);
  }, true);
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (t && t.tagName === 'SELECT') setTimeout(() => { if (document.activeElement === t) t.blur(); }, 0);
  }, true);

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
    else if (mod && !e.shiftKey && k.toLowerCase() === 'n') key = 'new';
    else if (mod && !e.shiftKey && k.toLowerCase() === 'o') key = 'open';
    else if (mod && k.toLowerCase() === 'c') key = 'copy';
    else if (mod && k.toLowerCase() === 'v') key = 'paste';
    else if (e.code === 'Comma' || e.code === 'Period') key = (mod ? 'jump-' : 'step-') + (e.code === 'Comma' ? 'prev' : 'next');
    else if (!mod && !e.altKey && k === ' ') key = 'play';
    else if (!mod && !e.altKey && k.toLowerCase() === 'k') key = 'key-toggle';
    else if (!mod && !e.altKey && !e.shiftKey && k === 'Tab') key = 'project-view';
    else if (!mod && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Delete', 'Backspace', 'Escape'].includes(k)) key = k;
    if (!key) return;
    e.preventDefault();
    fc.emit({ type: 'key', key, shift: e.shiftKey });
  });

  // The playhead time in the chosen format (framecraft_core::timecode,
  // FRAME's timecode.ts); Rust sets fc.timeFormat and fc.fps.
  fc.timeFormat = fc.timeFormat || 's';
  fc.fps = fc.fps || 25;
  fc.fmtTime = (ms) => {
    const t = Math.max(0, Number.isFinite(ms) ? ms : 0);
    const r = Math.max(1, Math.round(fc.fps || 25));
    const pad = (n, w) => String(Math.max(0, Math.floor(n))).padStart(w || 2, '0');
    const f = fc.timeFormat;
    if (f === 'ms') return String(Math.round(t));
    if (f === 's') {
      const sec = t / 1000;
      return Math.abs(sec - Math.round(sec)) < 0.0005 ? String(Math.round(sec)) : String(Number(sec.toFixed(3)));
    }
    if (f === 'frames') return String(Math.round(t / 1000 * r));
    const total = Math.round(t);
    const h = Math.floor(total / 3600000), m = Math.floor((total % 3600000) / 60000);
    const s = Math.floor((total % 60000) / 1000), mil = total % 1000;
    if (f === 'mm:ss.mmm') return pad(Math.floor(total / 60000)) + ':' + pad(s) + '.' + pad(mil, 3);
    if (f === 'hh:mm:ss.mmm') return pad(h) + ':' + pad(m) + ':' + pad(s) + '.' + pad(mil, 3);
    const frames = Math.floor(total / 1000 * r);
    return pad(Math.floor(frames / r / 3600)) + ':' + pad(Math.floor(frames / r / 60) % 60) + ':' + pad(Math.floor(frames / r) % 60) + ':' + pad(frames % r);
  };
  fc.drawPlayhead = (t) => {
    const p = document.getElementById('fc-playhead');
    if (p) p.style.left = (fc.labelWidth + t * fc.pxPerMs) + 'px';
    // While playing, follow the playhead when it leaves the visible part.
    const body = fc.playing ? document.querySelector('.tl-body') : null;
    if (body) {
      const x = t * fc.pxPerMs;
      const from = body.scrollLeft, to = from + body.clientWidth - fc.labelWidth - 20;
      if (x > to || x < from) body.scrollLeft = Math.max(0, x - 20);
    }
    const l = document.getElementById('fc-time');
    // Not while the user is typing a time there.
    if (l && document.activeElement !== l) l.value = fc.fmtTime(t);
  };
  // API and Script cues the playhead passes (FRAME's editor): an API cue
  // sends its request from here, a Script cue calls the graphic's
  // window.__ionframeScripts[name] in the preview. Each fires once a pass.
  fc.fireCue = (c) => {
    const name = c.label || c.id;
    if (c.kind === 'api') {
      if (!c.url) { fc.emit({ type: 'cue', text: 'API cue "' + name + '" skipped (no URL)' }); return; }
      const method = c.method || 'POST';
      const opts = { method };
      if (method === 'POST') {
        opts.headers = { 'Content-Type': 'application/json' };
        opts.body = (c.body || '').trim() ? c.body : '{}';
      }
      fetch(c.url, opts)
        .then((r) => fc.emit({ type: 'cue', text: 'API cue "' + name + '" → ' + r.status + (r.ok ? ' OK' : '') }))
        .catch((e) => fc.emit({ type: 'cue', text: 'API cue "' + name + '" failed: ' + (e && e.message || e) }));
    } else if (c.kind === 'script') {
      if (!c.fn) { fc.emit({ type: 'cue', text: 'Script cue "' + name + '" skipped (no function name)' }); return; }
      fc.post({ type: 'framecraft-editor', action: 'script-cue', fn: c.fn, cueId: c.id, label: c.label || '' });
    }
  };
  fc.play = (from, duration, onStop, cues) => {
    fc.stop(false);
    const start = performance.now();
    fc.playing = true;
    fc.onStop = onStop;
    const pending = (cues || []).filter((c) => c.at > from).sort((a, b) => a.at - b.at);
    let prev = from;
    const tick = (now) => {
      if (!fc.playing) return;
      const t = Math.min(duration, from + (now - start));
      while (pending.length && pending[0].at <= t && pending[0].at > prev) fc.fireCue(pending.shift());
      prev = t;
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

/// The time format and frame rate for the playhead label (see `fc.fmtTime`).
pub fn time_format(id: &str, fps: f64) {
    let _ = document::eval(&format!(
        "if (window.__fc) {{ window.__fc.timeFormat = {}; window.__fc.fps = {}; }}",
        serde_json::to_string(id).unwrap_or_default(),
        if fps.is_finite() { fps } else { 25.0 }
    ));
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
pub async fn play(from_ms: f64, duration_ms: f64, cues: &Value) -> Option<f64> {
    let mut eval = document::eval(&format!(
        "await new Promise((done) => window.__fc.play({from_ms}, {duration_ms}, (t) => {{ dioxus.send(t); done(); }}, {cues}));"
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
    if crate::state::debug() {
        eprintln!("[snapshot] canvases: {}", reply["canvases"]);
    }
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
        "if (window.__fc) {{ window.__fc.pxPerMs = {px_per_ms}; window.__fc.labelWidth = {label_width}; if (window.__fc.onScale) window.__fc.onScale(); }}"
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

/// The visible preview page's URL (for the native capture).
pub async fn active_url() -> Option<String> {
    document::eval("return window.__fc && window.__fc.active ? window.__fc.active.src : null;")
        .join::<Option<String>>()
        .await
        .ok()
        .flatten()
}
