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
