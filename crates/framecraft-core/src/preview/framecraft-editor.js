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
// Each is answered with { type: 'framecraft-editor', action: 'applied', ok, seq }.
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

  window.addEventListener('message', function (event) {
    var m = event.data;
    if (!m || m.type !== 'framecraft-editor') return;
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
