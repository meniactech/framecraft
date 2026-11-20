// SPDX-License-Identifier: AGPL-3.0-or-later
// FrameCraft editor script: preview only, never part of an export.
//
// Applies the editor's live updates to the running page without reloading.
// The playout runtime reads window.__ionframeSpx on every frame, so updating
// that data in place and repainting is enough for poses; layer markup is
// swapped node by node, keeping nested layers.
//
// Messages (window.postMessage, type 'framecraft-editor'):
//   { action: 'pose', tracks }
//   { action: 'layers', nodes: [{ domId, html }], tracks, textLayers,
//     imageLayers, visibilityLayers, hidden: [{ id, value }] }
//   { action: 'select', ids }   track these layers' geometry
//   { action: 'hit', x, y, seq } which layers are under a scene point
// Edits are answered with { action: 'applied', ok, seq }, hits with
// { action: 'hits', ids, seq }. While layers are selected, their corners
// in scene coordinates are posted as { action: 'geometry', items } whenever
// they change (every frame at most).
(function () {
  'use strict';

  var LAYER_ID = /^(shp|txt|img|isl|cht|qr|prt|ltn|pat|lot|vid|emp)_/;

  function data() {
    return window.__ionframeSpx || {};
  }

  // Replace each track's fields in place: the runtime keeps references to
  // the track objects, so their identity must not change.
  function patchTracks(tracks) {
    var list = data().animationTracks;
    if (!Array.isArray(list) || !Array.isArray(tracks)) return true;
    var ok = true;
    for (var i = 0; i < tracks.length; i++) {
      var next = tracks[i];
      var old = null;
      for (var j = 0; j < list.length; j++) {
        if (list[j].id === next.id) {
          old = list[j];
          break;
        }
      }
      if (old) Object.assign(old, next);
      else ok = false;
    }
    return ok;
  }

  function replaceArray(key, values) {
    var target = data()[key];
    if (!Array.isArray(target) || !Array.isArray(values)) return;
    target.splice.apply(target, [0, target.length].concat(values));
  }

  // Swap one layer's own element; its nested layers move into the new one.
  function swapNode(domId, html) {
    var old = document.getElementById(domId);
    if (!old) return false;
    var tpl = document.createElement('template');
    tpl.innerHTML = String(html || '').trim();
    var node = tpl.content.firstElementChild;
    if (!node) return false;
    var kids = [];
    for (var i = 0; i < old.children.length; i++) {
      if (LAYER_ID.test(old.children[i].id)) kids.push(old.children[i]);
    }
    for (var k = 0; k < kids.length; k++) node.appendChild(kids[k]);
    // The new markup carries the edited style; the repaint that follows sets
    // the animated values (position, transform, opacity) again.
    old.replaceWith(node);
    return true;
  }

  function setHidden(fields) {
    if (!Array.isArray(fields)) return;
    var box = document.getElementById('hiddenSpxData');
    if (!box) return;
    for (var i = 0; i < fields.length; i++) {
      var f = fields[i];
      var el = document.getElementById(f.id);
      if (el && box.contains(el)) el.textContent = f.value == null ? '' : String(f.value);
    }
  }

  // Repaint the current frame. (Not runTemplateUpdate: it also seeks to 0.)
  // A swapped text layer already carries its text in data-ion-full-text,
  // which is what the runtime paints from.
  function repaint() {
    if (typeof window.__ionframeRefreshFrame === 'function') window.__ionframeRefreshFrame();
  }

  var PREFIXES = ['shp', 'txt', 'img', 'isl', 'cht', 'qr', 'prt', 'ltn', 'pat', 'lot', 'vid', 'emp'];

  function layerElement(id) {
    for (var i = 0; i < PREFIXES.length; i++) {
      var el = document.getElementById(PREFIXES[i] + '_' + id);
      if (el) return el;
    }
    return null;
  }

  function num(v) {
    var n = parseFloat(v);
    return isFinite(n) ? n : 0;
  }

  // A point in an element's border box, mapped through every CSS transform
  // up to #gfx (scene coordinates).
  function toScene(el, x, y) {
    var px = x;
    var py = y;
    var node = el;
    while (node && node.id !== 'gfx') {
      var cs = getComputedStyle(node);
      if (cs.transform && cs.transform !== 'none') {
        var m = new DOMMatrix(cs.transform);
        var o = cs.transformOrigin.split(' ');
        var ox = num(o[0]);
        var oy = num(o[1]);
        var dx = px - ox;
        var dy = py - oy;
        px = m.a * dx + m.c * dy + m.e + ox;
        py = m.b * dx + m.d * dy + m.f + oy;
      }
      var parent = node.parentElement;
      var pcs = parent ? getComputedStyle(parent) : null;
      px += num(cs.left) + (pcs ? num(pcs.borderLeftWidth) : 0);
      py += num(cs.top) + (pcs ? num(pcs.borderTopWidth) : 0);
      node = parent;
    }
    return [px, py];
  }

  function corners(el) {
    var cs = getComputedStyle(el);
    var w = num(cs.width) || el.offsetWidth;
    var h = num(cs.height) || el.offsetHeight;
    return [toScene(el, 0, 0), toScene(el, w, 0), toScene(el, w, h), toScene(el, 0, h)];
  }

  var selection = [];
  var lastGeometry = '';

  function measureLoop() {
    requestAnimationFrame(measureLoop);
    if (!selection.length || !window.parent) return;
    var items = [];
    for (var i = 0; i < selection.length; i++) {
      var el = layerElement(selection[i]);
      if (el) items.push({ id: selection[i], corners: corners(el) });
    }
    var json = JSON.stringify(items);
    if (json === lastGeometry) return;
    lastGeometry = json;
    window.parent.postMessage({ type: 'framecraft-editor', action: 'geometry', items: items }, '*');
  }
  requestAnimationFrame(measureLoop);

  // Every layer can be picked in the editor, even ones that ignore the
  // pointer in playout (paths, SVG shapes). Preview only.
  var pickable = document.createElement('style');
  pickable.textContent = '#gfx, #gfx * { pointer-events: auto !important; }';
  (document.head || document.documentElement).appendChild(pickable);

  // Layers under a scene point, topmost first. Invisible path guides and
  // layers hidden by live data are skipped.
  function hits(x, y) {
    var out = [];
    var stack = document.elementsFromPoint(x, y);
    for (var i = 0; i < stack.length; i++) {
      var node = stack[i];
      while (node && node.id !== 'gfx' && !LAYER_ID.test(node.id || '')) node = node.parentElement;
      if (!node || node.id === 'gfx') continue;
      if (node.hasAttribute('data-ion-path-guide') || node.hasAttribute('data-ion-live-hidden')) continue;
      var id = node.id.slice(node.id.indexOf('_') + 1);
      if (out.indexOf(id) < 0) out.push(id);
    }
    return out;
  }

  window.addEventListener('message', function (event) {
    var m = event.data;
    if (!m || m.type !== 'framecraft-editor') return;
    if (m.action === 'select') {
      selection = Array.isArray(m.ids) ? m.ids.slice() : [];
      lastGeometry = '';
      if (!selection.length && window.parent) {
        window.parent.postMessage({ type: 'framecraft-editor', action: 'geometry', items: [] }, '*');
      }
      return;
    }
    if (m.action === 'snapshot') {
      // Export → PNG Frame: the scene as drawn now, at its own size.
      var src = event.source;
      var reply = function (r) {
        r.type = 'framecraft-editor';
        r.action = 'snapshot';
        r.seq = m.seq;
        if (src) src.postMessage(r, '*');
      };
      if (!window.htmlToImage) {
        reply({ error: 'The snapshot library did not load' });
        return;
      }
      var root = document.getElementById('gfx') || document.body;
      // WebKit paints images that html-to-image makes from canvases
      // (particles, charts, lightning, patterns) before they are decoded,
      // so they came out empty. Stand an already decoded <img> of each
      // canvas in its place for the capture, then put the canvases back.
      var swaps = [];
      var canvases = root.querySelectorAll('canvas');
      // What each canvas holds, for FRAMECRAFT_DEBUG (drawn? readable?).
      var diag = [];
      for (var di = 0; di < canvases.length; di++) {
        var dc = canvases[di];
        var info = { id: dc.id, w: dc.width, h: dc.height, shown: dc.getClientRects().length > 0 };
        try {
          var dctx = dc.getContext('2d');
          var px = dctx ? dctx.getImageData(0, 0, dc.width, dc.height).data : null;
          var lit = 0;
          if (px) for (var pi = 3; pi < px.length; pi += 4) if (px[pi]) lit++;
          info.litPixels = lit;
        } catch (err) { info.error = String(err && err.message || err); }
        diag.push(info);
      }
      var decoding = [];
      for (var ci = 0; ci < canvases.length; ci++) {
        var cv = canvases[ci];
        var url;
        try { url = cv.toDataURL('image/png'); } catch (err) { continue; }
        if (!url || url === 'data:,') continue;
        var img = document.createElement('img');
        img.src = url;
        img.className = cv.className;
        img.style.cssText = cv.style.cssText;
        var cs = window.getComputedStyle(cv);
        img.style.width = cs.width;
        img.style.height = cs.height;
        img.style.display = cs.display === 'none' ? 'none' : (cs.display === 'inline' ? 'inline-block' : cs.display);
        decoding.push(img.decode ? img.decode().catch(function () {}) : Promise.resolve());
        swaps.push([cv, img, cv.style.display]);
      }
      var restore = function () {
        for (var si = 0; si < swaps.length; si++) {
          var sw = swaps[si];
          if (sw[1].parentNode) sw[1].parentNode.removeChild(sw[1]);
          sw[0].style.display = sw[2];
        }
      };
      var capture = function () {
        for (var si = 0; si < swaps.length; si++) {
          var sw = swaps[si];
          sw[0].parentNode.insertBefore(sw[1], sw[0]);
          sw[0].style.display = 'none';
        }
        // A thumbnail (`thumb: [w, h]`) is a small JPEG on black; else a full PNG.
        return m.thumb
        ? window.htmlToImage.toJpeg(root, {
            width: m.width,
            height: m.height,
            canvasWidth: m.thumb[0],
            canvasHeight: m.thumb[1],
            pixelRatio: 1,
            quality: 0.8,
            backgroundColor: '#000000',
            cacheBust: false
          })
        : window.htmlToImage.toPng(root, { width: m.width, height: m.height, pixelRatio: 1, cacheBust: false });
      };
      Promise.all(decoding)
        .then(capture)
        .then(function (url) { restore(); reply({ dataUrl: url, canvases: diag }); })
        .catch(function (err) { restore(); reply({ error: String((err && err.message) || err) }); });
      return;
    }
    if (m.action === 'hit') {
      if (event.source) event.source.postMessage({ type: 'framecraft-editor', action: 'hits', ids: hits(m.x, m.y), seq: m.seq }, '*');
      return;
    }
    var ok = true;
    if (m.action === 'pose') {
      ok = patchTracks(m.tracks);
      repaint();
    } else if (m.action === 'layers') {
      var nodes = Array.isArray(m.nodes) ? m.nodes : [];
      for (var i = 0; i < nodes.length; i++) ok = swapNode(nodes[i].domId, nodes[i].html) && ok;
      ok = patchTracks(m.tracks) && ok;
      replaceArray('textLayers', m.textLayers);
      replaceArray('imageLayers', m.imageLayers);
      replaceArray('visibilityLayers', m.visibilityLayers);
      setHidden(m.hidden);
      repaint();
    } else {
      return;
    }
    if (event.source && typeof event.source.postMessage === 'function') {
      event.source.postMessage({ type: 'framecraft-editor', action: 'applied', ok: ok, seq: m.seq }, '*');
    }
  });
})();
