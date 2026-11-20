/**
 * IonFrame SPX draft — timeline / seek / postMessage (keep template.html minimal).
 * Bundled in export ZIP as `js/ionframe-spx-runtime.mjs` (same as static copy).
 */
import { animate, eases } from './lib/anime.esm.min.js';

function e(id) {
  return document.getElementById(id);
}

function htmlDecode(input) {
  const txt = document.createElement('textarea');
  txt.innerHTML = input || '';
  return txt.value;
}

function postBack(event, payload) {
  if (!event.source || typeof event.source.postMessage !== 'function') return;
  try {
    event.source.postMessage(payload, event.origin || '*');
  } catch {
    // ignore
  }
}

function parseMessageData(raw) {
  if (raw == null) return null;
  if (typeof raw === 'string') {
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
  }
  if (typeof raw === 'object') return raw;
  return null;
}

function parseColor(input) {
  const value = String(input || 'rgba(0,0,0,1)').trim();
  const hex = value.match(/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i);
  if (hex) {
    const v = hex[1];
    if (v.length === 3) {
      return {
        r: parseInt(v[0] + v[0], 16),
        g: parseInt(v[1] + v[1], 16),
        b: parseInt(v[2] + v[2], 16),
        a: 1
      };
    }
    if (v.length === 6) {
      return {
        r: parseInt(v.slice(0, 2), 16),
        g: parseInt(v.slice(2, 4), 16),
        b: parseInt(v.slice(4, 6), 16),
        a: 1
      };
    }
    return {
      r: parseInt(v.slice(0, 2), 16),
      g: parseInt(v.slice(2, 4), 16),
      b: parseInt(v.slice(4, 6), 16),
      a: parseInt(v.slice(6, 8), 16) / 255
    };
  }
  const rgba = value.match(/^rgba?\((.+)\)$/i);
  if (rgba) {
    const p = rgba[1].split(',').map((part) => Number(part.trim()));
    return {
      r: Number.isFinite(p[0]) ? p[0] : 0,
      g: Number.isFinite(p[1]) ? p[1] : 0,
      b: Number.isFinite(p[2]) ? p[2] : 0,
      a: Number.isFinite(p[3]) ? p[3] : 1
    };
  }
  return { r: 0, g: 0, b: 0, a: 1 };
}

function rotateAroundCenter(x, y, cx, cy, degrees) {
  const radians = (degrees * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const dx = x - cx;
  const dy = y - cy;
  return {
    x: cx + dx * cos - dy * sin,
    y: cy + dx * sin + dy * cos
  };
}

/** Off-DOM SVG path sampler for pathFollow. */
let __ionPathSampleEl = null;
function ensurePathSampleEl() {
  if (typeof document === 'undefined') return null;
  if (__ionPathSampleEl && __ionPathSampleEl.isConnected) return __ionPathSampleEl;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', '0');
  svg.setAttribute('height', '0');
  svg.style.cssText = 'position:absolute;left:-99999px;top:-99999px;width:0;height:0;overflow:hidden;';
  __ionPathSampleEl = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  svg.appendChild(__ionPathSampleEl);
  document.body.appendChild(svg);
  return __ionPathSampleEl;
}

function sampleSvgPathJ(d, progress01) {
  const el = ensurePathSampleEl();
  if (!el) return null;
  const pathD = String(d || '').trim();
  if (!pathD) return null;
  try {
    el.setAttribute('d', pathD);
    const length = el.getTotalLength();
    if (!Number.isFinite(length) || length <= 0) return null;
    const t = Math.max(0, Math.min(1, Number(progress01) || 0));
    const dist = t * length;
    const p = el.getPointAtLength(dist);
    const delta = Math.min(1, Math.max(0.25, length * 0.002));
    const a = el.getPointAtLength(Math.max(0, dist - delta));
    const b = el.getPointAtLength(Math.min(length, dist + delta));
    return {
      x: p.x,
      y: p.y,
      angleDeg: (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI,
      length
    };
  } catch {
    return null;
  }
}

function buildMotionPathDJ(pts, curve, closed, fallbackW, fallbackH) {
  const points = Array.isArray(pts) ? pts : [];
  if (points.length < 2) return '';
  if (points.length === 2) {
    return `M ${points[0].x} ${points[0].y} L ${points[1].x} ${points[1].y}`;
  }
  if (closed) {
    if ((curve || 'linear') !== 'bezier') {
      return `M ${points.map((p) => `${p.x} ${p.y}`).join(' L ')} Z`;
    }
    let d = `M ${points[0].x} ${points[0].y}`;
    for (let i = 0; i < points.length; i += 1) {
      const a = points[i];
      const b = points[(i + 1) % points.length];
      const aBezier = (a.curve || 'linear') === 'bezier';
      const bBezier = (b.curve || 'linear') === 'bezier';
      const c1x = aBezier ? (a.outX != null ? a.outX : a.x) : a.x;
      const c1y = aBezier ? (a.outY != null ? a.outY : a.y) : a.y;
      const c2x = bBezier ? (b.inX != null ? b.inX : b.x) : b.x;
      const c2y = bBezier ? (b.inY != null ? b.inY : b.y) : b.y;
      d += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${b.x} ${b.y}`;
    }
    return `${d} Z`;
  }
  if ((curve || 'linear') !== 'bezier') {
    return `M ${points.map((p) => `${p.x} ${p.y}`).join(' L ')}`;
  }
  let d = `M ${points[0].x} ${points[0].y}`;
  for (let i = 0; i < points.length - 1; i += 1) {
    const a = points[i];
    const b = points[i + 1];
    const aBezier = (a.curve || 'linear') === 'bezier';
    const bBezier = (b.curve || 'linear') === 'bezier';
    const c1x = aBezier ? (a.outX != null ? a.outX : a.x) : a.x;
    const c1y = aBezier ? (a.outY != null ? a.outY : a.y) : a.y;
    const c2x = bBezier ? (b.inX != null ? b.inX : b.x) : b.x;
    const c2y = bBezier ? (b.inY != null ? b.inY : b.y) : b.y;
    d += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${b.x} ${b.y}`;
  }
  return d || `M 0 0 L ${fallbackW || 0} 0`;
}

function layoutById(itemsLayout, id) {
  return (itemsLayout || []).find((it) => it && it.id === id) || null;
}

function resolveWorldRotationJ(itemsLayout, itemId) {
  const map = new Map((itemsLayout || []).map((it) => [it.id, it]));
  let current = map.get(itemId);
  if (!current) return 0;
  let rot = Number(current.rotation) || 0;
  let parent = current.parentId ? map.get(current.parentId) : null;
  let guard = 0;
  while (parent && guard < 50) {
    rot += Number(parent.rotation) || 0;
    parent = parent.parentId ? map.get(parent.parentId) : null;
    guard += 1;
  }
  return rot;
}

function itemLocalPointToWorldJ(itemsLayout, itemId, localX, localY) {
  const map = new Map((itemsLayout || []).map((it) => [it.id, it]));
  const item = map.get(itemId);
  if (!item) return { x: localX, y: localY };
  const w = Math.max(0, Number(item.width) || 0);
  const h = Math.max(0, Number(item.height) || 0);
  const ox = ((Number(item.pivotX) || 50) / 100) * w;
  const oy = ((Number(item.pivotY) || 50) / 100) * h;
  const sx = Number(item.scaleX);
  const sy = Number(item.scaleY);
  const scaleX = Number.isFinite(sx) ? sx : 1;
  const scaleY = Number.isFinite(sy) ? sy : 1;
  let dx = (localX - ox) * scaleX;
  let dy = (localY - oy) * scaleY;
  const rad = ((Number(item.rotation) || 0) * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  let x = Number(item.x) + ox + (dx * cos - dy * sin);
  let y = Number(item.y) + oy + (dx * sin + dy * cos);
  let parent = item.parentId ? map.get(item.parentId) : null;
  let guard = 0;
  while (parent && guard < 50) {
    const center = { x: (Number(parent.width) || 0) / 2, y: (Number(parent.height) || 0) / 2 };
    const rotated = rotateAroundCenter(x, y, center.x, center.y, Number(parent.rotation) || 0);
    x = Number(parent.x) + rotated.x;
    y = Number(parent.y) + rotated.y;
    parent = parent.parentId ? map.get(parent.parentId) : null;
    guard += 1;
  }
  return { x, y };
}

function worldPointToParentLocalJ(itemsLayout, itemId, worldX, worldY) {
  const map = new Map((itemsLayout || []).map((it) => [it.id, it]));
  const item = map.get(itemId);
  if (!item || !item.parentId) return { x: worldX, y: worldY };
  const chain = [];
  let current = item;
  let guard = 0;
  while (current && current.parentId && guard < 50) {
    const parent = map.get(current.parentId);
    if (!parent) break;
    chain.push(parent);
    current = parent;
    guard += 1;
  }
  let x = worldX;
  let y = worldY;
  for (let i = chain.length - 1; i >= 0; i -= 1) {
    const parent = chain[i];
    const lx = x - Number(parent.x);
    const ly = y - Number(parent.y);
    const center = { x: (Number(parent.width) || 0) / 2, y: (Number(parent.height) || 0) / 2 };
    const unrotated = rotateAroundCenter(lx, ly, center.x, center.y, -(Number(parent.rotation) || 0));
    x = unrotated.x;
    y = unrotated.y;
  }
  return { x, y };
}

function parentLocalToItemLocalJ(item, parentX, parentY) {
  const w = Math.max(0, Number(item.width) || 0);
  const h = Math.max(0, Number(item.height) || 0);
  const ox = ((Number(item.pivotX) || 50) / 100) * w;
  const oy = ((Number(item.pivotY) || 50) / 100) * h;
  let sx = Number(item.scaleX);
  let sy = Number(item.scaleY);
  if (!Number.isFinite(sx) || Math.abs(sx) < 1e-8) sx = 1;
  if (!Number.isFinite(sy) || Math.abs(sy) < 1e-8) sy = 1;
  let dx = parentX - Number(item.x) - ox;
  let dy = parentY - Number(item.y) - oy;
  const rad = (-(Number(item.rotation) || 0) * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  const ux = dx * cos - dy * sin;
  const uy = dx * sin + dy * cos;
  return { x: ux / sx + ox, y: uy / sy + oy };
}

function remapPathDToItemLocalJ(itemsLayout, sourceId, follower, pathD, samples) {
  const pts = [];
  const n = Math.max(8, samples || 72);
  for (let i = 0; i <= n; i += 1) {
    const sample = sampleSvgPathJ(pathD, i / n);
    if (!sample) continue;
    const world = itemLocalPointToWorldJ(itemsLayout, sourceId, sample.x, sample.y);
    const parentLocal = worldPointToParentLocalJ(itemsLayout, follower.id, world.x, world.y);
    pts.push(parentLocalToItemLocalJ(follower, parentLocal.x, parentLocal.y));
  }
  if (pts.length < 2) return '';
  return `M ${pts.map((p) => `${p.x} ${p.y}`).join(' L ')}`;
}

function resolvePathFollowPoseJ(itemsLayout, track, pathFollowMod) {
  if (!pathFollowMod || !pathFollowMod.enabled) return null;
  const sourceId = String(track.pathFollowSourceId || pathFollowMod.sourceElementId || '').trim();
  if (!sourceId || sourceId === track.id) return null;
  const pts = track.pathFollowSourcePolygon;
  if (!Array.isArray(pts) || pts.length < 2) return null;
  const sourceLayout = layoutById(itemsLayout, sourceId);
  if (!sourceLayout) return null;
  const followerLayout = layoutById(itemsLayout, track.id);
  if (!followerLayout) return null;

  const forceOpen = !!track.pathFollowSourceIsOpen;
  const pathD = buildMotionPathDJ(
    pts,
    track.pathFollowSourcePolygonCurve,
    forceOpen ? false : !!pathFollowMod.closed,
    sourceLayout.width,
    sourceLayout.height
  );
  if (!pathD) return null;

  let progress01 = (Number(pathFollowMod.progress) || 0) / 100;
  if (pathFollowMod.loop) progress01 = ((progress01 % 1) + 1) % 1;
  else progress01 = Math.max(-0.25, Math.min(1.25, progress01));
  const progress = progress01 * 100;

  const isText = String(track.domId || '').indexOf('txt_') === 0;
  // Text always letter-by-letter along path.
  if (isText) {
    const sameParent = (followerLayout.parentId || null) === (sourceLayout.parentId || null);
    if (sameParent) {
      return {
        mode: 'textAlong',
        x: sourceLayout.x,
        y: sourceLayout.y,
        width: sourceLayout.width,
        height: sourceLayout.height,
        rotation: sourceLayout.rotation,
        pathDLocal: pathD,
        progress
      };
    }
    const pathDLocal = remapPathDToItemLocalJ(itemsLayout, sourceId, followerLayout, pathD);
    if (!pathDLocal) return null;
    return {
      mode: 'textAlong',
      x: followerLayout.x,
      y: followerLayout.y,
      width: followerLayout.width,
      height: followerLayout.height,
      rotation: followerLayout.rotation,
      pathDLocal,
      progress
    };
  }

  const sample = sampleSvgPathJ(pathD, Math.max(0, Math.min(1, progress01)));
  if (!sample) return null;
  const world = itemLocalPointToWorldJ(itemsLayout, sourceId, sample.x, sample.y);
  const parentLocal = worldPointToParentLocalJ(itemsLayout, track.id, world.x, world.y);
  const pivotX = ((Number(followerLayout.pivotX) || 50) / 100) * Math.max(0, Number(followerLayout.width) || 0);
  const pivotY = ((Number(followerLayout.pivotY) || 50) / 100) * Math.max(0, Number(followerLayout.height) || 0);
  let angleDeg = sample.angleDeg + resolveWorldRotationJ(itemsLayout, sourceId);
  if (followerLayout.parentId) {
    angleDeg -= resolveWorldRotationJ(itemsLayout, followerLayout.parentId);
  }
  if (pathFollowMod.bankFlip) angleDeg += 180;
  angleDeg += Number(pathFollowMod.bankOffsetDeg) || 0;
  return {
    mode: 'follow',
    x: parentLocal.x - pivotX,
    y: parentLocal.y - pivotY,
    width: followerLayout.width,
    height: followerLayout.height,
    rotation: pathFollowMod.bank ? angleDeg : Number(followerLayout.rotation) || 0,
    pathDLocal: '',
    progress
  };
}

function applyTextAlongPathToNode(node, pathDLocal, progress, textContent, boxW, boxH, glyphs) {
  if (!node || !pathDLocal) return;
  let svg = node.querySelector('[data-ion-path-text]');
  const slot = node.querySelector('[data-ion-text]');
  if (slot) slot.style.display = 'none';
  if (!svg) {
    svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('data-ion-path-text', '1');
    svg.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;overflow:visible;pointer-events:none;';
    const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('data-ion-path-d', '1');
    path.setAttribute('fill', 'none');
    defs.appendChild(path);
    const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    text.setAttribute('data-ion-path-label', '1');
    text.setAttribute('dominant-baseline', 'middle');
    const textPath = document.createElementNS('http://www.w3.org/2000/svg', 'textPath');
    textPath.setAttribute('data-ion-path-tp', '1');
    textPath.setAttribute('method', 'align');
    textPath.setAttribute('spacing', 'auto');
    text.appendChild(textPath);
    svg.appendChild(defs);
    svg.appendChild(text);
    node.insertBefore(svg, node.firstChild);
  }
  const w = Math.max(1, Number(boxW) || parseFloat(node.style.width) || node.clientWidth || 1);
  const h = Math.max(1, Number(boxH) || parseFloat(node.style.height) || node.clientHeight || 1);
  svg.setAttribute('width', String(w));
  svg.setAttribute('height', String(h));
  svg.setAttribute('viewBox', `0 0 ${w} ${h}`);
  const pathId = `ion-pf-${node.id || 'x'}`;
  const path = svg.querySelector('[data-ion-path-d]');
  const text = svg.querySelector('[data-ion-path-label]');
  const textPath = svg.querySelector('[data-ion-path-tp]');
  if (path) {
    path.setAttribute('id', pathId);
    path.setAttribute('d', pathDLocal);
  }
  if (text) {
    text.setAttribute('fill', node.style.color || '#ffffff');
    text.setAttribute('font-family', node.style.fontFamily || 'sans-serif');
    text.setAttribute('font-size', String(parseFloat(node.style.fontSize) || 42));
    text.setAttribute('font-weight', node.style.fontWeight || '400');
  }
  if (textPath) {
    textPath.setAttribute('href', `#${pathId}`);
    textPath.setAttributeNS('http://www.w3.org/1999/xlink', 'xlink:href', `#${pathId}`);
    textPath.setAttribute('startOffset', `${progress}%`);
    while (textPath.firstChild) textPath.removeChild(textPath.firstChild);
    if (Array.isArray(glyphs) && glyphs.length) {
      for (let i = 0; i < glyphs.length; i += 1) {
        const g = glyphs[i];
        if (!g || g.isNewline) continue;
        const tspan = document.createElementNS('http://www.w3.org/2000/svg', 'tspan');
        tspan.setAttribute('opacity', String(g.opacity));
        if (Math.abs(Number(g.svgDxEm) || 0) > 1e-6) {
          tspan.setAttribute('dx', Number(g.svgDxEm).toFixed(4) + 'em');
        }
        if (Math.abs(Number(g.svgDyEm) || 0) > 1e-6) {
          tspan.setAttribute('dy', Number(g.svgDyEm).toFixed(4) + 'em');
        }
        if (Math.abs((Number(g.scale) || 1) - 1) > 1e-4) {
          tspan.setAttribute('font-size', ((Number(g.scale) || 1) * 100).toFixed(2) + '%');
        }
        if (g.fill) tspan.setAttribute('fill', String(g.fill));
        tspan.textContent = g.char;
        textPath.appendChild(tspan);
      }
    } else {
      textPath.textContent = textContent == null ? '' : String(textContent);
    }
  }
}

function clearTextAlongPathOnNode(node) {
  if (!node) return;
  const svg = node.querySelector('[data-ion-path-text]');
  if (svg) svg.remove();
  const slot = node.querySelector('[data-ion-text]');
  if (slot) slot.style.display = '';
}

function buildModifierFilter(modifiers) {
  const active = Array.isArray(modifiers) ? modifiers.filter((m) => m && m.enabled) : [];
  const filters = [];
  for (const mod of active) {
    if (mod.type === 'blur') {
      filters.push(`blur(${Math.max(0, Number(mod.radius) || 0)}px)`);
    } else if (mod.type === 'color') {
      filters.push(
        `brightness(${Number(mod.brightness) || 100}%) contrast(${Number(mod.contrast) || 100}%) saturate(${Number(mod.saturation) || 100}%) hue-rotate(${Number(mod.hueRotate) || 0}deg)`
      );
    } else if (mod.type === 'shadow') {
      filters.push(
        `drop-shadow(${Number(mod.x) || 0}px ${Number(mod.y) || 0}px ${Math.max(0, Number(mod.blur) || 0)}px ${mod.color || 'rgba(0,0,0,0.5)'})`
      );
    }
  }
  return filters.join(' ');
}

function buildModifierTransform(modifiers) {
  const active = Array.isArray(modifiers)
    ? modifiers.filter((m) => m && m.enabled && m.type === 'skew')
    : [];
  return active
    .map((mod) => `skew(${Number(mod.skewX) || 0}deg, ${Number(mod.skewY) || 0}deg)`)
    .join(' ');
}

function polygonPointsToClipPathJ(points) {
  if (!Array.isArray(points) || points.length < 3) return '';
  return `polygon(${points.map((p) => `${Math.round((Number(p.x) || 0) * 1000) / 1000}px ${Math.round((Number(p.y) || 0) * 1000) / 1000}px`).join(',')})`;
}

function invertPolygonToClipPathJ(points, targetWidth, targetHeight) {
  if (!Array.isArray(points) || points.length < 3) return '';
  const w = Math.max(1, Number(targetWidth) || 1);
  const h = Math.max(1, Number(targetHeight) || 1);
  const inner = points
    .map((p, i) => `${i === 0 ? 'M' : 'L'} ${Math.round((Number(p.x) || 0) * 1000) / 1000} ${Math.round((Number(p.y) || 0) * 1000) / 1000}`)
    .join(' ');
  return `path('M 0 0 L ${w} 0 L ${w} ${h} L 0 ${h} Z ${inner} Z')`;
}

function invertPathToClipPathJ(pathData, targetWidth, targetHeight) {
  if (!pathData) return '';
  const w = Math.max(1, Number(targetWidth) || 1);
  const h = Math.max(1, Number(targetHeight) || 1);
  return `path('M 0 0 L ${w} 0 L ${w} ${h} L 0 ${h} Z ${pathData} Z')`;
}

function buildModifierClipPath(modifiers, targetWidth, targetHeight) {
  const crop = Array.isArray(modifiers)
    ? modifiers.find((m) => m && m.enabled && m.type === 'crop')
    : null;
  if (crop) {
    const pct = Math.max(0, Math.min(100, Number(crop.percent) || 0));
    // 0% = fully revealed (no clip). Nested children stay visible; animate pct down to reveal.
    if (pct <= 0) return '';
    if (crop.direction === 'left') return `inset(0% 0% 0% ${pct}%)`;
    if (crop.direction === 'right') return `inset(0% ${pct}% 0% 0%)`;
    if (crop.direction === 'up') return `inset(${pct}% 0% 0% 0%)`;
    if (crop.direction === 'down') return `inset(0% 0% ${pct}% 0%)`;
  }
  const mask = Array.isArray(modifiers)
    ? modifiers.find((m) => m && m.enabled && m.type === 'mask')
    : null;
  if (!mask || mask.sourceElementId) return '';
  const tr = mask.transform || {};
  const x = Number(tr.x) || 0;
  const y = Number(tr.y) || 0;
  const w = Math.max(1, Number(tr.width) || 1);
  const h = Math.max(1, Number(tr.height) || 1);
  const r = Number(tr.rotation) || 0;
  const cx = x + w / 2;
  const cy = y + h / 2;

  if (mask.shape === 'ellipse') {
    return `ellipse(${w / 2}px ${h / 2}px at ${cx}px ${cy}px)`;
  }

  if (mask.shape === 'polygon' && Array.isArray(mask.polygonPoints) && mask.polygonPoints.length >= 3) {
    return mask.mode === 'invert'
      ? invertPolygonToClipPathJ(mask.polygonPoints, targetWidth, targetHeight)
      : polygonPointsToClipPathJ(mask.polygonPoints);
  }

  const p1 = rotateAroundCenter(x, y, cx, cy, r);
  const p2 = rotateAroundCenter(x + w, y, cx, cy, r);
  const p3 = rotateAroundCenter(x + w, y + h, cx, cy, r);
  const p4 = rotateAroundCenter(x, y + h, cx, cy, r);
  const rectPts = [p1, p2, p3, p4];
  return mask.mode === 'invert'
    ? invertPolygonToClipPathJ(rectPts, targetWidth, targetHeight)
    : polygonPointsToClipPathJ(rectPts);
}

function getActiveMask(modifiers) {
  if (!Array.isArray(modifiers)) return null;
  return modifiers.find((m) => m && m.enabled && m.type === 'mask') || null;
}

/**
 * maskGeometry (editor): `clip-path: polygon` in the target’s *local* space.
 * local → parent = left/top + rotate(θ) about center (editor DOM model).
 * Mirrors: src/lib/utils/maskGeometry.ts
 */
function localToParentL(it, lx, ly) {
  const w = Math.max(0.0001, Number(it.width) || 0);
  const h = Math.max(0.0001, Number(it.height) || 0);
  const cx = w / 2;
  const cy = h / 2;
  const rad = ((Number(it.rotation) || 0) * Math.PI) / 180;
  const c0 = Math.cos(rad);
  const s0 = Math.sin(rad);
  const dx = lx - cx;
  const dy = ly - cy;
  return { x: (Number(it.x) || 0) + cx + c0 * dx - s0 * dy, y: (Number(it.y) || 0) + cy + s0 * dx + c0 * dy };
}

function parentToLocalL(it, px, py) {
  const w = Math.max(0.0001, Number(it.width) || 0);
  const h = Math.max(0.0001, Number(it.height) || 0);
  const cx = w / 2;
  const cy = h / 2;
  const rad = ((Number(it.rotation) || 0) * Math.PI) / 180;
  const c0 = Math.cos(rad);
  const s0 = Math.sin(rad);
  const u = px - (Number(it.x) || 0) - cx;
  const v = py - (Number(it.y) || 0) - cy;
  const ddx = c0 * u + s0 * v;
  const ddy = -s0 * u + c0 * v;
  return { x: cx + ddx, y: cy + ddy };
}

function localPointToSceneJ(items, itemId, lx, ly) {
  const map = new Map(items.map((i) => [i.id, i]));
  let p = { x: lx, y: ly };
  let id = itemId;
  while (id) {
    const it = map.get(id);
    if (!it) break;
    p = localToParentL(it, p.x, p.y);
    id = it.parentId || null;
  }
  return p;
}

function chainRootToItemJ(items, itemId) {
  const map = new Map(items.map((i) => [i.id, i]));
  const rev = [];
  let id = itemId;
  while (id) {
    const it = map.get(id);
    if (!it) break;
    rev.push(it);
    id = it.parentId || null;
  }
  return rev.reverse();
}

function scenePointToItemLocalJ(items, itemId, sx, sy) {
  const ch = chainRootToItemJ(items, itemId);
  let p = { x: sx, y: sy };
  for (let i = 0; i < ch.length; i += 1) {
    p = parentToLocalL(ch[i], p.x, p.y);
  }
  return p;
}

function buildLinkedMaskPolygonPointsFromLayout(items, targetId, sourceId, sourcePoly) {
  if (!sourceId || targetId === sourceId) return [];
  const map = new Map(items.map((i) => [i.id, i]));
  if (!map.get(sourceId) || !map.get(targetId)) return [];
  let corners;
  if (Array.isArray(sourcePoly) && sourcePoly.length >= 3) {
    corners = sourcePoly;
  } else {
    const s = map.get(sourceId);
    const w = Math.max(1, Number(s.width) || 1);
    const h = Math.max(1, Number(s.height) || 1);
    corners = [
      { x: 0, y: 0 },
      { x: w, y: 0 },
      { x: w, y: h },
      { x: 0, y: h }
    ];
  }
  const inTarget = corners.map((c) => {
    const sc = localPointToSceneJ(items, sourceId, Number(c.x) || 0, Number(c.y) || 0);
    return scenePointToItemLocalJ(items, targetId, sc.x, sc.y);
  });
  if (inTarget.length < 3) return [];
  return inTarget;
}

function buildLinkedMaskBezierPathFromLayout(items, targetId, sourceId, sourcePoly) {
  if (!Array.isArray(sourcePoly) || sourcePoly.length < 3) return '';
  const mapPoint = (x, y) => {
    const sc = localPointToSceneJ(items, sourceId, Number(x) || 0, Number(y) || 0);
    return scenePointToItemLocalJ(items, targetId, sc.x, sc.y);
  };
  const first = mapPoint(sourcePoly[0].x, sourcePoly[0].y);
  let d = `M ${Math.round(first.x * 1000) / 1000} ${Math.round(first.y * 1000) / 1000}`;
  for (let i = 0; i < sourcePoly.length; i += 1) {
    const a = sourcePoly[i];
    const b = sourcePoly[(i + 1) % sourcePoly.length];
    const aBezier = (a.curve || 'linear') === 'bezier';
    const bBezier = (b.curve || 'linear') === 'bezier';
    const c1 = mapPoint(aBezier ? (a.outX != null ? a.outX : a.x) : a.x, aBezier ? (a.outY != null ? a.outY : a.y) : a.y);
    const c2 = mapPoint(bBezier ? (b.inX != null ? b.inX : b.x) : b.x, bBezier ? (b.inY != null ? b.inY : b.y) : b.y);
    const p = mapPoint(b.x, b.y);
    d += ` C ${Math.round(c1.x * 1000) / 1000} ${Math.round(c1.y * 1000) / 1000}, ${Math.round(c2.x * 1000) / 1000} ${Math.round(c2.y * 1000) / 1000}, ${Math.round(p.x * 1000) / 1000} ${Math.round(p.y * 1000) / 1000}`;
  }
  d += ' Z';
  return d;
}

/** Deterministic Canvas2D particles (mirrors editor particleSim.ts). */
function createParticleRng(seed) {
  let t = seed >>> 0;
  if (t === 0) t = 1;
  return () => {
    t += 0x6d2b79f5;
    let r = Math.imul(t ^ (t >>> 15), 1 | t);
    r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

function lerpNum(a, b, t) {
  return a + (b - a) * t;
}

function mixRgba(a, b, t) {
  return {
    r: lerpNum(a.r, b.r, t),
    g: lerpNum(a.g, b.g, t),
    b: lerpNum(a.b, b.b, t),
    a: lerpNum(a.a, b.a, t)
  };
}

function clamp01(t) {
  const n = Number(t);
  if (!Number.isFinite(n)) return 0;
  return Math.min(1, Math.max(0, n));
}

function normalizeColorRamp(stops, colorStart, colorEnd) {
  const fallbackStart = colorStart || 'rgba(255,255,255,1)';
  const fallbackEnd = colorEnd || 'rgba(255,255,255,0)';
  const cleaned = [];
  if (Array.isArray(stops)) {
    for (let i = 0; i < stops.length; i += 1) {
      const raw = stops[i];
      if (!raw || typeof raw !== 'object') continue;
      const color =
        typeof raw.color === 'string' && raw.color.trim() ? raw.color.trim() : fallbackStart;
      cleaned.push({ offset: clamp01(Number(raw.offset)), color });
    }
  }
  if (cleaned.length === 0) {
    return [
      { offset: 0, color: fallbackStart },
      { offset: 1, color: fallbackEnd }
    ];
  }
  cleaned.sort((a, b) => a.offset - b.offset);
  const deduped = [];
  for (let i = 0; i < cleaned.length; i += 1) {
    const stop = cleaned[i];
    const prev = deduped[deduped.length - 1];
    if (prev && Math.abs(prev.offset - stop.offset) < 1e-6) prev.color = stop.color;
    else deduped.push({ offset: stop.offset, color: stop.color });
  }
  if (deduped[0].offset > 0) deduped.unshift({ offset: 0, color: deduped[0].color });
  else deduped[0].offset = 0;
  const last = deduped[deduped.length - 1];
  if (last.offset < 1) deduped.push({ offset: 1, color: last.color });
  else last.offset = 1;
  if (deduped.length < 2) {
    return [
      { offset: 0, color: deduped[0]?.color || fallbackStart },
      { offset: 1, color: fallbackEnd }
    ];
  }
  return deduped;
}

function sampleColorRamp(stops, t, colorStart, colorEnd) {
  const ramp = normalizeColorRamp(stops, colorStart, colorEnd);
  const u = clamp01(t);
  if (u <= ramp[0].offset) return parseColor(ramp[0].color);
  const last = ramp[ramp.length - 1];
  if (u >= last.offset) return parseColor(last.color);
  for (let i = 0; i < ramp.length - 1; i += 1) {
    const a = ramp[i];
    const b = ramp[i + 1];
    if (u >= a.offset && u <= b.offset) {
      const span = Math.max(1e-9, b.offset - a.offset);
      return mixRgba(parseColor(a.color), parseColor(b.color), (u - a.offset) / span);
    }
  }
  return parseColor(last.color);
}

function normalizeValueRamp(stops, valueStart, valueEnd) {
  const fallbackStart = Number.isFinite(Number(valueStart)) ? Number(valueStart) : 8;
  const fallbackEnd = Number.isFinite(Number(valueEnd)) ? Number(valueEnd) : 2;
  const cleaned = [];
  if (Array.isArray(stops)) {
    for (let i = 0; i < stops.length; i += 1) {
      const raw = stops[i];
      if (!raw || typeof raw !== 'object') continue;
      const value = Number(raw.value);
      cleaned.push({
        offset: clamp01(Number(raw.offset)),
        value: Number.isFinite(value) ? value : fallbackStart
      });
    }
  }
  if (cleaned.length === 0) {
    return [
      { offset: 0, value: fallbackStart },
      { offset: 1, value: fallbackEnd }
    ];
  }
  cleaned.sort((a, b) => a.offset - b.offset);
  const deduped = [];
  for (let i = 0; i < cleaned.length; i += 1) {
    const stop = cleaned[i];
    const prev = deduped[deduped.length - 1];
    if (prev && Math.abs(prev.offset - stop.offset) < 1e-6) prev.value = stop.value;
    else deduped.push({ offset: stop.offset, value: stop.value });
  }
  if (deduped[0].offset > 0) deduped.unshift({ offset: 0, value: deduped[0].value });
  else deduped[0].offset = 0;
  const last = deduped[deduped.length - 1];
  if (last.offset < 1) deduped.push({ offset: 1, value: last.value });
  else last.offset = 1;
  if (deduped.length < 2) {
    return [
      { offset: 0, value: deduped[0]?.value ?? fallbackStart },
      { offset: 1, value: fallbackEnd }
    ];
  }
  return deduped;
}

function sampleValueRamp(stops, t, valueStart, valueEnd) {
  const ramp = normalizeValueRamp(stops, valueStart, valueEnd);
  const u = clamp01(t);
  if (u <= ramp[0].offset) return ramp[0].value;
  const last = ramp[ramp.length - 1];
  if (u >= last.offset) return last.value;
  for (let i = 0; i < ramp.length - 1; i += 1) {
    const a = ramp[i];
    const b = ramp[i + 1];
    if (u >= a.offset && u <= b.offset) {
      const span = Math.max(1e-9, b.offset - a.offset);
      return lerpNum(a.value, b.value, (u - a.offset) / span);
    }
  }
  return last.value;
}

function interpolateStepHold(keyframes, time, fallback) {
  if (!Array.isArray(keyframes) || keyframes.length === 0) return fallback;
  const sorted = keyframes.slice().sort((a, b) => a.time - b.time);
  if (time < sorted[0].time) return sorted[0].value;
  let value = sorted[0].value;
  for (let i = 0; i < sorted.length; i += 1) {
    if (sorted[i].time <= time) value = sorted[i].value;
    else break;
  }
  return value;
}

function simulateParticles(cfg, timeMs) {
  const tEnd = Math.max(0, Number(timeMs) || 0);
  const dt = 1000 / 60;
  const rng = createParticleRng(Number(cfg.seed) || 1);
  const particles = [];
  let spawnAcc = 0;
  let simT = 0;
  let spawnIndex = 0;
  const life = Math.max(1, Number(cfg.lifetimeMs) || 1);
  const maxCount = Math.max(1, Math.floor(Number(cfg.maxCount) || 1));
  const spreadRad = ((Math.max(0, Number(cfg.spreadDeg) || 0) * Math.PI) / 180) / 2;
  const baseAngle =
    ((Number.isFinite(Number(cfg.emitAngleDeg)) ? Number(cfg.emitAngleDeg) : -90) * Math.PI) / 180;
  const relative = cfg.attach === 'relative';
  const paletteColors =
    cfg.colorMode === 'paletteCycle' && Array.isArray(cfg.paletteColors)
      ? cfg.paletteColors.filter((c) => typeof c === 'string' && c.trim())
      : [];
  const usePaletteCycle = paletteColors.length > 0;
  const poseAt =
    typeof cfg.emitterAt === 'function'
      ? cfg.emitterAt
      : function () {
          return { x: Number(cfg.emitterX) || 0, y: Number(cfg.emitterY) || 0 };
        };
  const emittingAt =
    typeof cfg.emittingAt === 'function'
      ? cfg.emittingAt
      : function () {
          return cfg.emitting !== false && cfg.emitting !== 0;
        };
  const spawnRateAt =
    typeof cfg.spawnRateAt === 'function'
      ? cfg.spawnRateAt
      : function () {
          return Math.max(0, Number(cfg.spawnRate) || 0);
        };
  while (simT < tEnd) {
    const step = Math.min(dt, tEnd - simT);
    const nextT = simT + step;
    const pose = relative ? { x: 0, y: 0 } : poseAt(nextT);
    const rateNow = Math.max(0, spawnRateAt(nextT));
    const emittingNow = emittingAt(nextT);
    if (emittingNow && rateNow > 0) {
      spawnAcc += (rateNow * step) / 1000;
      while (spawnAcc >= 1 && particles.length < maxCount) {
        spawnAcc -= 1;
        const u1 = rng();
        const u2 = rng();
        const u3 = rng();
        const u4 = rng();
        const u5 = rng();
        const ang = baseAngle + (u1 * 2 - 1) * spreadRad;
        const spd = Number(cfg.speed) * (0.75 + u2 * 0.5);
        const shape = cfg.emitterShape === 'box' ? 'box' : 'circle';
        let ox = 0;
        let oy = 0;
        if (shape === 'box') {
          const hw = Math.max(0, Number(cfg.emitterWidth) || 0) * 0.5;
          const hh = Math.max(0, Number(cfg.emitterHeight) || 0) * 0.5;
          ox = (u3 * 2 - 1) * hw;
          oy = (u4 * 2 - 1) * hh;
        } else {
          const er = Math.max(0, Number(cfg.emitterRadius) || 0);
          const r = Math.sqrt(u3) * er;
          const a = u4 * Math.PI * 2;
          ox = Math.cos(a) * r;
          oy = Math.sin(a) * r;
        }
        const angle0Deg =
          (Number.isFinite(Number(cfg.rotationDeg)) ? Number(cfg.rotationDeg) : 0) +
          (u5 - 0.5) * Math.max(0, Number(cfg.rotationRandomDeg) || 0);
        let tint = null;
        if (usePaletteCycle) {
          const raw = parseColor(paletteColors[spawnIndex % paletteColors.length]);
          tint = { r: raw.r, g: raw.g, b: raw.b, a: 1 };
          spawnIndex += 1;
        }
        particles.push({
          x: pose.x + ox,
          y: pose.y + oy,
          vx: Math.cos(ang) * spd,
          vy: Math.sin(ang) * spd,
          bornMs: nextT,
          lifeMs: life,
          size0: Number(cfg.size) || 8,
          size1: Number(cfg.sizeEnd) || 2,
          angle0Deg,
          tint
        });
        rng();
      }
      if (particles.length >= maxCount) spawnAcc = 0;
    } else {
      spawnAcc = 0;
    }
    const stepSec = step / 1000;
    for (let i = particles.length - 1; i >= 0; i -= 1) {
      const p = particles[i];
      p.vx += Number(cfg.gravityX) * stepSec;
      p.vy += Number(cfg.gravityY) * stepSec;
      p.x += p.vx * stepSec;
      p.y += p.vy * stepSec;
      if (nextT - p.bornMs >= p.lifeMs) particles.splice(i, 1);
    }
    simT = nextT;
  }
  return particles;
}

function particleDrawAngleDeg(p, cfg, ageMs) {
  if (cfg.alignToVelocity) {
    return (Math.atan2(p.vy, p.vx) * 180) / Math.PI + (Number(p.angle0Deg) || 0);
  }
  return (Number(p.angle0Deg) || 0) + (Number(cfg.spinDeg) || 0) * (ageMs / 1000);
}

function drawParticlesFrame(ctx, width, height, cfg, timeMs, sprite) {
  ctx.clearRect(0, 0, width, height);
  const particles = simulateParticles(cfg, timeMs);
  const relative = cfg.attach === 'relative';
  const ox = relative ? Number(cfg.emitterX) || 0 : 0;
  const oy = relative ? Number(cfg.emitterY) || 0 : 0;
  const usePaletteAlpha = cfg.colorMode === 'paletteCycle';
  const alphaRamp = usePaletteAlpha ? normalizeValueRamp(cfg.alphaRamp, 1, 1) : null;
  ctx.save();
  ctx.globalCompositeOperation = cfg.blend === 'normal' ? 'source-over' : 'lighter';
  let tintCanvas = null;
  let tintCtx = null;
  for (let i = 0; i < particles.length; i += 1) {
    const p = particles[i];
    const age = Math.max(0, timeMs - p.bornMs);
    const t = Math.min(1, age / Math.max(1, p.lifeMs));
    const sizeScale = Math.max(0, sampleValueRamp(cfg.sizeRamp, t, 1, 0.25));
    const baseSize = Math.max(0.01, Number(cfg.size) || 8);
    const size = Math.max(0, baseSize * sizeScale);
    let col;
    if (p.tint && alphaRamp) {
      const alphaMul = Math.max(0, Math.min(1, sampleValueRamp(alphaRamp, t, 1, 1)));
      col = { r: p.tint.r, g: p.tint.g, b: p.tint.b, a: alphaMul };
    } else {
      col = sampleColorRamp(cfg.colorRamp, t, cfg.colorStart, cfg.colorEnd);
    }
    if (col.a <= 0.001 || size < 0.01) continue;
    const px = p.x + ox;
    const py = p.y + oy;
    if (sprite) {
      const w = Math.max(1, Math.ceil(size));
      const h = Math.max(1, Math.ceil(size));
      const rotRad = (particleDrawAngleDeg(p, cfg, age) * Math.PI) / 180;
      const drawRotated = (source) => {
        ctx.save();
        ctx.translate(px, py);
        ctx.rotate(rotRad);
        ctx.globalAlpha = col.a;
        ctx.drawImage(source, -size / 2, -size / 2, size, size);
        ctx.globalAlpha = 1;
        ctx.restore();
      };
      if (!tintCanvas) {
        tintCanvas = document.createElement('canvas');
        tintCtx = tintCanvas.getContext('2d');
      }
      if (tintCtx && tintCanvas) {
        if (tintCanvas.width !== w || tintCanvas.height !== h) {
          tintCanvas.width = w;
          tintCanvas.height = h;
        }
        tintCtx.clearRect(0, 0, w, h);
        tintCtx.globalCompositeOperation = 'source-over';
        tintCtx.globalAlpha = 1;
        tintCtx.drawImage(sprite, 0, 0, w, h);
        tintCtx.globalCompositeOperation = 'source-in';
        tintCtx.fillStyle = `rgb(${Math.round(col.r)}, ${Math.round(col.g)}, ${Math.round(col.b)})`;
        tintCtx.fillRect(0, 0, w, h);
        drawRotated(tintCanvas);
      } else {
        drawRotated(sprite);
      }
    } else {
      ctx.fillStyle = `rgba(${Math.round(col.r)}, ${Math.round(col.g)}, ${Math.round(col.b)}, ${Math.max(0, Math.min(1, col.a))})`;
      ctx.beginPath();
      ctx.arc(px, py, size / 2, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  ctx.restore();
}

/** Seek-stable lightning bolt (mirrors editor lightningItem.ts). */
function mulberry32Lightning(seed) {
  let t = seed >>> 0;
  return function () {
    t += 0x6d2b79f5;
    let r = Math.imul(t ^ (t >>> 15), 1 | t);
    r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

function buildLightningBolt(from, to, segments, amplitude, rng) {
  const pts = [{ x: from.x, y: from.y }];
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len;
  const ny = dx / len;
  const segs = Math.max(2, Math.round(segments));
  for (let i = 1; i < segs; i += 1) {
    const t = i / segs;
    const envelope = Math.sin(Math.PI * t);
    const offset = (rng() * 2 - 1) * amplitude * envelope;
    pts.push({
      x: from.x + dx * t + nx * offset,
      y: from.y + dy * t + ny * offset
    });
  }
  pts.push({ x: to.x, y: to.y });
  return pts;
}

function strokeLightningPolyline(ctx, pts, color, width, blur, alpha) {
  if (!pts || pts.length < 2) return;
  ctx.save();
  ctx.beginPath();
  ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length; i += 1) {
    ctx.lineTo(pts[i].x, pts[i].y);
  }
  ctx.strokeStyle = color;
  ctx.lineWidth = width;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.globalAlpha = Math.max(0, Math.min(1, alpha));
  ctx.shadowBlur = Math.max(0, blur);
  ctx.shadowColor = color;
  ctx.stroke();
  ctx.restore();
}

function drawLightningFrame(ctx, width, height, cfg, timeMs) {
  ctx.clearRect(0, 0, width, height);
  if (!cfg || cfg.emitting === false) return;
  const flicker = Math.max(8, Number(cfg.flickerMs) || 48);
  const bucket = Math.floor(Math.max(0, Number(timeMs) || 0) / flicker);
  const seed = (Number(cfg.seed) || 1) >>> 0 || 1;
  const rng = mulberry32Lightning((seed ^ (bucket * 0x9e3779b9)) >>> 0);
  const ax = Number(cfg.ax) || 0;
  const ay = Number(cfg.ay) || 0;
  const bx = Number(cfg.bx) || 0;
  const by = Number(cfg.by) || 0;
  const segments = Number(cfg.segments) || 14;
  const amplitude = Number(cfg.amplitude) || 0;
  const boltW = Math.max(0.5, Number(cfg.width) || 2);
  const glowW = Math.max(boltW, Number(cfg.glowWidth) || 0);
  const color = typeof cfg.color === 'string' && cfg.color.trim() ? cfg.color.trim() : '#e0e7ff';
  const glowColor =
    typeof cfg.glowColor === 'string' && cfg.glowColor.trim() ? cfg.glowColor.trim() : '#6366f1';
  const main = buildLightningBolt({ x: ax, y: ay }, { x: bx, y: by }, segments, amplitude, rng);
  strokeLightningPolyline(ctx, main, glowColor, glowW, glowW * 1.8, 0.55);
  strokeLightningPolyline(ctx, main, color, boltW, boltW * 2, 1);
  const branchCount = Math.max(0, Math.min(6, Math.round(Number(cfg.branches) || 0)));
  for (let b = 0; b < branchCount; b += 1) {
    if (main.length < 4) break;
    const startIdx = 1 + Math.floor(rng() * (main.length - 3));
    const start = main[startIdx];
    const prev = main[startIdx - 1];
    const dirX = start.x - prev.x;
    const dirY = start.y - prev.y;
    const dLen = Math.hypot(dirX, dirY) || 1;
    const side = rng() < 0.5 ? 1 : -1;
    const nx = (-dirY / dLen) * side;
    const ny = (dirX / dLen) * side;
    const blen = (0.2 + rng() * 0.35) * Math.hypot(bx - ax, by - ay);
    const end = {
      x: start.x + (dirX / dLen) * blen * 0.55 + nx * blen * 0.65,
      y: start.y + (dirY / dLen) * blen * 0.55 + ny * blen * 0.65
    };
    const branchSegs = Math.max(3, Math.round(segments * 0.45));
    const branch = buildLightningBolt(start, end, branchSegs, amplitude * 0.7, rng);
    strokeLightningPolyline(ctx, branch, glowColor, glowW * 0.55, glowW, 0.4);
    strokeLightningPolyline(ctx, branch, color, Math.max(0.75, boltW * 0.7), boltW, 0.9);
  }
  const ends = [
    { x: ax, y: ay },
    { x: bx, y: by }
  ];
  for (let i = 0; i < ends.length; i += 1) {
    const p = ends[i];
    ctx.save();
    ctx.beginPath();
    ctx.arc(p.x, p.y, Math.max(2, boltW * 1.5), 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.shadowBlur = Math.max(12, glowW * 2);
    ctx.shadowColor = glowColor;
    ctx.globalAlpha = 0.85;
    ctx.fill();
    ctx.restore();
  }
}

/** Seek-stable pattern fills (mirrors editor patternItem.ts). */
function wrapPatternPhasePx(phase01, period) {
  if (!(period > 0)) return 0;
  const t = ((phase01 % 1) + 1) % 1;
  return t * period;
}

function drawPatternChecker(ctx, width, height, cfg) {
  const cell = Math.max(2, Number(cfg.cellSize) || 32);
  const offset = wrapPatternPhasePx(Number(cfg.phase) || 0, cell * 2);
  ctx.fillStyle = cfg.colorA;
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = cfg.colorB;
  const startX = -cell * 2 + (-offset % (cell * 2));
  const startY = -cell * 2 + (-offset % (cell * 2));
  for (let y = startY; y < height + cell * 2; y += cell) {
    for (let x = startX; x < width + cell * 2; x += cell) {
      const ix = Math.floor((x + offset) / cell);
      const iy = Math.floor((y + offset) / cell);
      if ((ix + iy) % 2 === 0) continue;
      ctx.fillRect(Math.floor(x), Math.floor(y), cell, cell);
    }
  }
}

function drawPatternStripes(ctx, width, height, cfg) {
  const stripe = Math.max(1, Number(cfg.stripeWidth) || 16);
  const gap = Math.max(0, Number(cfg.stripeGap) || 0);
  const period = stripe + gap;
  const offset = wrapPatternPhasePx(Number(cfg.phase) || 0, period);
  ctx.fillStyle = cfg.colorA;
  ctx.fillRect(0, 0, width, height);
  if (period <= 0) return;
  const angle = ((Number(cfg.stripeAngleDeg) || 0) * Math.PI) / 180;
  const diag = Math.hypot(width, height) * 2;
  ctx.save();
  ctx.translate(width / 2, height / 2);
  ctx.rotate(angle);
  ctx.fillStyle = cfg.colorB;
  const start = -diag - offset;
  for (let x = start; x < diag; x += period) {
    ctx.fillRect(x, -diag, stripe, diag * 2);
  }
  ctx.restore();
}

function drawPatternDots(ctx, width, height, cfg) {
  const spacing = Math.max(2, Number(cfg.cellSize) || 32);
  const r = Math.max(0.5, Number(cfg.dotRadius) || 6);
  const offset = wrapPatternPhasePx(Number(cfg.phase) || 0, spacing);
  ctx.fillStyle = cfg.colorA;
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = cfg.colorB;
  const startX = -spacing + (-offset % spacing);
  const startY = -spacing + (-offset % spacing);
  for (let y = startY; y < height + spacing; y += spacing) {
    for (let x = startX; x < width + spacing; x += spacing) {
      ctx.beginPath();
      ctx.arc(x + spacing / 2, y + spacing / 2, r, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

function drawPatternGrid(ctx, width, height, cfg) {
  const cell = Math.max(2, Number(cfg.cellSize) || 32);
  const lw = Math.max(0.5, Number(cfg.lineWidth) || 2);
  const offset = wrapPatternPhasePx(Number(cfg.phase) || 0, cell);
  ctx.fillStyle = cfg.colorA;
  ctx.fillRect(0, 0, width, height);
  ctx.strokeStyle = cfg.colorB;
  ctx.lineWidth = lw;
  ctx.beginPath();
  const startX = -cell + (-offset % cell);
  const startY = -cell + (-offset % cell);
  for (let x = startX; x <= width + cell; x += cell) {
    const px = Math.round(x) + 0.5;
    ctx.moveTo(px, 0);
    ctx.lineTo(px, height);
  }
  for (let y = startY; y <= height + cell; y += cell) {
    const py = Math.round(y) + 0.5;
    ctx.moveTo(0, py);
    ctx.lineTo(width, py);
  }
  ctx.stroke();
}

function drawPatternFrame(ctx, width, height, cfg) {
  ctx.clearRect(0, 0, width, height);
  const kind = cfg && cfg.kind;
  if (kind === 'stripes') drawPatternStripes(ctx, width, height, cfg);
  else if (kind === 'dots') drawPatternDots(ctx, width, height, cfg);
  else if (kind === 'grid') drawPatternGrid(ctx, width, height, cfg);
  else drawPatternChecker(ctx, width, height, cfg);
}

/**
 * @param {object} data
 * @param {number} data.sceneDurationMs
 * @param {number[]} [data.pausePoints]
 * @param {object} data.ionEasingToAnime
 * @param {Array} data.animationTracks
 * @param {Array<{ id: string, fieldId: string, fallback: string }>} [data.textLayers]
 * @param {Array<{ id: string, domId?: string, fieldId: string, invert?: boolean }>} [data.visibilityLayers]
 * @param {Array} [data.particleLayers]
 * @param {Array} [data.lightningLayers]
 * @param {Array} [data.patternLayers]
 */
export function initIonFrameSpx(data) {
  const sceneDurationMs = Math.max(1, Number(data.sceneDurationMs) || 1000);
  const pausePoints = Array.isArray(data.pausePoints)
    ? data.pausePoints
        .map((n) => Math.max(0, Math.min(sceneDurationMs, Number(n) || 0)))
        .sort((a, b) => a - b)
    : [];
  const ION_EASING_TO_ANIME = data.ionEasingToAnime && typeof data.ionEasingToAnime === 'object' ? data.ionEasingToAnime : {};
  const animationTracks = Array.isArray(data.animationTracks) ? data.animationTracks : [];
  const particleLayers = Array.isArray(data.particleLayers) ? data.particleLayers : [];
  const lightningLayers = Array.isArray(data.lightningLayers) ? data.lightningLayers : [];
  const patternLayers = Array.isArray(data.patternLayers) ? data.patternLayers : [];
  const particleSprites = {};
  const cues = Array.isArray(data.cues) ? data.cues : [];
  const apiCues = cues
    .filter((cue) => cue && cue.type === 'api_call')
    .map((cue) => {
      const delayMs = Math.max(0, Number(cue.delayMs) || 0);
      const effectiveTimeMs = Math.max(
        0,
        Math.min(
          sceneDurationMs,
          cue.effectiveTimeMs != null
            ? Number(cue.effectiveTimeMs) || 0
            : Number(cue.time || 0) + delayMs
        )
      );
      return {
        id: String(cue.id || ''),
        label: cue.label || '',
        apiUrl: cue.apiUrl || '',
        apiMethod: String(cue.apiMethod || 'POST').toUpperCase() === 'GET' ? 'GET' : 'POST',
        apiBody: cue.apiBody || '',
        effectiveTimeMs
      };
    })
    .filter((cue) => cue.id && cue.apiUrl)
    .sort((a, b) => a.effectiveTimeMs - b.effectiveTimeMs);
  const scriptCues = cues
    .filter((cue) => cue && cue.type === 'script')
    .map((cue) => {
      const delayMs = Math.max(0, Number(cue.delayMs) || 0);
      const effectiveTimeMs = Math.max(
        0,
        Math.min(
          sceneDurationMs,
          cue.effectiveTimeMs != null
            ? Number(cue.effectiveTimeMs) || 0
            : Number(cue.time || 0) + delayMs
        )
      );
      return {
        id: String(cue.id || ''),
        label: cue.label || '',
        scriptFn: String(cue.scriptFn || '').trim(),
        effectiveTimeMs
      };
    })
    .filter((cue) => cue.id && cue.scriptFn)
    .sort((a, b) => a.effectiveTimeMs - b.effectiveTimeMs);
  const textLayers = Array.isArray(data.textLayers) ? data.textLayers : [];
  const imageLayers = Array.isArray(data.imageLayers) ? data.imageLayers : [];
  const visibilityLayers = Array.isArray(data.visibilityLayers) ? data.visibilityLayers : [];
  /** @type {Record<string, string | number | boolean>} */
  let ionboardLiveValues = {};

  let playbackRaf = null;
  let playbackStartMs = 0;
  let playbackPausedAtMs = 0;
  let playbackSegmentStartMs = 0;
  let playbackSegmentEndMs = sceneDurationMs;
  let playbackRunning = false;
  let playbackLastMs = 0;
  let currentSegmentIndex = 0;
  let firedApiCueIds = new Set();
  let firedScriptCueIds = new Set();

  function dbg(...args) {
    try {
      console.log('[IonFrame SPX]', ...args);
    } catch {
      // ignore
    }
  }

  const segmentEdges = [0, ...pausePoints.filter((t) => t > 0 && t < sceneDurationMs), sceneDurationMs];
  const segments = [];
  for (let i = 0; i < segmentEdges.length - 1; i += 1) {
    segments.push({ start: segmentEdges[i], end: segmentEdges[i + 1] });
  }

  async function executeApiCue(cue) {
    const options = { method: cue.apiMethod };
    if (cue.apiMethod === 'POST') {
      options.headers = { 'Content-Type': 'application/json' };
      options.body = cue.apiBody && String(cue.apiBody).trim() ? cue.apiBody : '{}';
    }
    try {
      await fetch(cue.apiUrl, options);
    } catch {
      // Keep playback resilient if cue endpoint fails.
    }
  }

  function executeScriptCue(cue) {
    try {
      const host = typeof window !== 'undefined' ? window.__ionframeScripts : null;
      const fn = host && cue.scriptFn ? host[cue.scriptFn] : null;
      if (typeof fn === 'function') {
        fn({ cueId: cue.id, label: cue.label });
      }
    } catch {
      // Optional host hook — ignore failures.
    }
  }

  function syncFiredApiCuesToTime(timeMs) {
    const next = new Set();
    for (const cue of apiCues) {
      if (cue.effectiveTimeMs <= timeMs) next.add(cue.id);
    }
    firedApiCueIds = next;
    const nextScript = new Set();
    for (const cue of scriptCues) {
      if (cue.effectiveTimeMs <= timeMs) nextScript.add(cue.id);
    }
    firedScriptCueIds = nextScript;
  }

  function runApiCuesForRange(fromMs, toMs) {
    const a = Math.min(fromMs, toMs);
    const b = Math.max(fromMs, toMs);
    for (const cue of apiCues) {
      if (firedApiCueIds.has(cue.id)) continue;
      if (cue.effectiveTimeMs > a && cue.effectiveTimeMs <= b) {
        firedApiCueIds.add(cue.id);
        void executeApiCue(cue);
      }
    }
    for (const cue of scriptCues) {
      if (firedScriptCueIds.has(cue.id)) continue;
      if (cue.effectiveTimeMs > a && cue.effectiveTimeMs <= b) {
        firedScriptCueIds.add(cue.id);
        executeScriptCue(cue);
      }
    }
  }

  function applyEasing(id, t) {
    const x = Math.max(0, Math.min(1, t));
    const key = ION_EASING_TO_ANIME[id] || 'linear';
    let fn = (eases && eases[key]) || eases.linear;
    // Anime.js v4: Back and Elastic eases are factories (`eases.outElastic()`
    // returns the curve). Calling them with `x` returned a function, so the
    // value became NaN and those keyframes jumped instead of easing.
    if (/(Back|Elastic)$/.test(key)) fn = fn();
    return fn(x);
  }

  function interpolateTrack(keyframes, time, fallback) {
    if (!Array.isArray(keyframes) || keyframes.length === 0) return fallback;
    const sorted = [...keyframes].sort((a, b) => a.time - b.time);
    if (time <= sorted[0].time) return sorted[0].value;
    if (time >= sorted[sorted.length - 1].time) return sorted[sorted.length - 1].value;
    for (let i = 0; i < sorted.length - 1; i += 1) {
      const a = sorted[i];
      const b = sorted[i + 1];
      if (time >= a.time && time <= b.time) {
        const linearT = (time - a.time) / Math.max(1, b.time - a.time);
        const te = applyEasing(b.easing || 'linear', linearT);
        return a.value + (b.value - a.value) * te;
      }
    }
    return fallback;
  }

  function silhouetteStrokeFilterJ(widthPx, color) {
    const w = Math.max(0, Math.min(48, Number(widthPx) || 0));
    if (w <= 0) return 'none';
    const c = String(color || '#000000').trim() || '#000000';
    const parts = [];
    for (let i = 0; i < 8; i++) {
      const a = (i / 8) * Math.PI * 2;
      parts.push(
        'drop-shadow(' +
          (Math.cos(a) * w).toFixed(2) +
          'px ' +
          (Math.sin(a) * w).toFixed(2) +
          'px 0px ' +
          c +
          ')'
      );
    }
    return parts.join(' ');
  }

  function buildLayoutAtTime(t) {
    return animationTracks.map((tr) => ({
      id: tr.id,
      parentId: tr.parentId || null,
      x: interpolateTrack(tr.timeline.x, t, tr.base.x),
      y: interpolateTrack(tr.timeline.y, t, tr.base.y),
      width: interpolateTrack(tr.timeline.width, t, tr.base.width),
      height: interpolateTrack(tr.timeline.height, t, tr.base.height),
      rotation: interpolateTrack(tr.timeline.rotation, t, tr.base.rotation),
      scaleX: interpolateTrack(tr.timeline.scaleX, t, Number(tr.base.scaleX) || 1),
      scaleY: interpolateTrack(tr.timeline.scaleY, t, Number(tr.base.scaleY) || 1),
      pivotX: Number(tr.base.pivotX) || 50,
      pivotY: Number(tr.base.pivotY) || 50
    }));
  }

  function applyFrame(timeMs) {
    const itemsLayout = buildLayoutAtTime(timeMs);
    for (const track of animationTracks) {
      const node = e(track.domId);
      if (!node) continue;
      // Local left/top — nested DOM parents carry children (crop clip-path reveals them).
      let x = interpolateTrack(track.timeline.x, timeMs, track.base.x);
      let y = interpolateTrack(track.timeline.y, timeMs, track.base.y);
      let width = interpolateTrack(track.timeline.width, timeMs, track.base.width);
      let height = interpolateTrack(track.timeline.height, timeMs, track.base.height);
      const fontSize = interpolateTrack(track.timeline.fontSize, timeMs, Number(track.base.fontSize) || 42);
      const textColorBase = parseColor(track.base.textColor || '#ffffff');
      const bgColorBase = parseColor(track.base.bgColor || '#00000000');
      const textColorR = interpolateTrack(track.timeline.textColorR, timeMs, textColorBase.r);
      const textColorG = interpolateTrack(track.timeline.textColorG, timeMs, textColorBase.g);
      const textColorB = interpolateTrack(track.timeline.textColorB, timeMs, textColorBase.b);
      const textColorA = interpolateTrack(track.timeline.textColorA, timeMs, textColorBase.a);
      const bgColorR = interpolateTrack(track.timeline.bgColorR, timeMs, bgColorBase.r);
      const bgColorG = interpolateTrack(track.timeline.bgColorG, timeMs, bgColorBase.g);
      const bgColorB = interpolateTrack(track.timeline.bgColorB, timeMs, bgColorBase.b);
      const bgColorA = interpolateTrack(track.timeline.bgColorA, timeMs, bgColorBase.a);
      let rotation = interpolateTrack(track.timeline.rotation, timeMs, track.base.rotation);
      const opacity = interpolateTrack(track.timeline.opacity, timeMs, track.base.opacity);
      const borderWidth = Math.max(
        0,
        interpolateTrack(track.timeline.borderWidth, timeMs, Number(track.base.borderWidth) || 0)
      );
      const scaleX = interpolateTrack(track.timeline.scaleX, timeMs, Number(track.base.scaleX) || 1);
      const scaleY = interpolateTrack(track.timeline.scaleY, timeMs, Number(track.base.scaleY) || 1);
      const animatedModifiers = Array.isArray(track.modifiers)
        ? track.modifiers.map((mod) => ({ ...mod }))
        : [];
      const blurModifier = animatedModifiers.find((mod) => mod.type === 'blur');
      const colorModifier = animatedModifiers.find((mod) => mod.type === 'color');
      const shadowModifier = animatedModifiers.find((mod) => mod.type === 'shadow');
      const cropModifier = animatedModifiers.find((mod) => mod.type === 'crop');
      const typewriterModifier = animatedModifiers.find((mod) => mod.type === 'typewriter');
      const pathFollowModifier = animatedModifiers.find((mod) => mod.type === 'pathFollow');
      const textStaggerModifier = animatedModifiers.find((mod) => mod.type === 'textStagger');
      const textScrambleModifier = animatedModifiers.find((mod) => mod.type === 'textScramble');
      const textGlyphColorModifier = animatedModifiers.find((mod) => mod.type === 'textGlyphColor');
      const skewModifier = animatedModifiers.find((mod) => mod.type === 'skew');
      if (blurModifier && blurModifier.type === 'blur') {
        blurModifier.radius = interpolateTrack(track.timeline.modBlurRadius, timeMs, Number(blurModifier.radius) || 0);
      }
      if (colorModifier && colorModifier.type === 'color') {
        colorModifier.brightness = interpolateTrack(track.timeline.modColorBrightness, timeMs, Number(colorModifier.brightness) || 100);
        colorModifier.contrast = interpolateTrack(track.timeline.modColorContrast, timeMs, Number(colorModifier.contrast) || 100);
        colorModifier.saturation = interpolateTrack(track.timeline.modColorSaturation, timeMs, Number(colorModifier.saturation) || 100);
        colorModifier.hueRotate = interpolateTrack(track.timeline.modColorHueRotate, timeMs, Number(colorModifier.hueRotate) || 0);
      }
      if (shadowModifier && shadowModifier.type === 'shadow') {
        shadowModifier.x = interpolateTrack(track.timeline.modShadowX, timeMs, Number(shadowModifier.x) || 0);
        shadowModifier.y = interpolateTrack(track.timeline.modShadowY, timeMs, Number(shadowModifier.y) || 0);
        shadowModifier.blur = interpolateTrack(track.timeline.modShadowBlur, timeMs, Number(shadowModifier.blur) || 0);
      }
      if (cropModifier && cropModifier.type === 'crop') {
        cropModifier.percent = interpolateTrack(track.timeline.modCropPercent, timeMs, Number(cropModifier.percent) || 0);
      }
      if (typewriterModifier && typewriterModifier.type === 'typewriter') {
        typewriterModifier.percent = interpolateTrack(
          track.timeline.modTypewriterPercent,
          timeMs,
          Number(typewriterModifier.percent) || 0
        );
      }
      if (pathFollowModifier && pathFollowModifier.type === 'pathFollow') {
        pathFollowModifier.progress = interpolateTrack(
          track.timeline.modPathProgress,
          timeMs,
          Number(pathFollowModifier.progress) || 0
        );
      }
      if (textStaggerModifier && textStaggerModifier.type === 'textStagger') {
        textStaggerModifier.progress = interpolateTrack(
          track.timeline.modTextStaggerProgress,
          timeMs,
          Number(textStaggerModifier.progress) || 0
        );
      }
      if (textScrambleModifier && textScrambleModifier.type === 'textScramble') {
        textScrambleModifier.progress = interpolateTrack(
          track.timeline.modTextScrambleProgress,
          timeMs,
          Number(textScrambleModifier.progress) || 0
        );
      }
      if (textGlyphColorModifier && textGlyphColorModifier.type === 'textGlyphColor') {
        textGlyphColorModifier.progress = interpolateTrack(
          track.timeline.modTextGlyphColorProgress,
          timeMs,
          Number(textGlyphColorModifier.progress) || 0
        );
      }
      if (skewModifier && skewModifier.type === 'skew') {
        skewModifier.skewX = interpolateTrack(track.timeline.modSkewX, timeMs, Number(skewModifier.skewX) || 0);
        skewModifier.skewY = interpolateTrack(track.timeline.modSkewY, timeMs, Number(skewModifier.skewY) || 0);
      }
      const pathFollowPose =
        pathFollowModifier && pathFollowModifier.enabled
          ? resolvePathFollowPoseJ(itemsLayout, track, pathFollowModifier)
          : null;
      if (pathFollowPose && pathFollowPose.mode === 'follow') {
        x = pathFollowPose.x;
        y = pathFollowPose.y;
        rotation = pathFollowPose.rotation;
      }
      if (pathFollowPose && pathFollowPose.mode === 'textAlong') {
        x = pathFollowPose.x;
        y = pathFollowPose.y;
        width = pathFollowPose.width;
        height = pathFollowPose.height;
        rotation = pathFollowPose.rotation;
      }
      const modifierTransform = buildModifierTransform(animatedModifiers);
      const modifierFilter = buildModifierFilter(animatedModifiers);
      const mask = getActiveMask(animatedModifiers);
      const modifierClipPath = buildModifierClipPath(animatedModifiers, width, height);
      let linkedClipPath = '';
      if (mask && mask.sourceElementId) {
        if (track.maskSourcePolygonCurve === 'bezier' && Array.isArray(track.maskSourcePolygon)) {
          const d = buildLinkedMaskBezierPathFromLayout(
            itemsLayout,
            track.id,
            mask.sourceElementId,
            track.maskSourcePolygon
          );
          linkedClipPath =
            mask.mode === 'invert'
              ? invertPathToClipPathJ(d, width, height)
              : (d ? `path('${d}')` : '');
        } else {
          const linkedPoints = buildLinkedMaskPolygonPointsFromLayout(
            itemsLayout,
            track.id,
            mask.sourceElementId,
            track.maskSourcePolygon
          );
          linkedClipPath =
            mask.mode === 'invert'
              ? invertPolygonToClipPathJ(linkedPoints, width, height)
              : polygonPointsToClipPathJ(linkedPoints);
        }
      }
      const feather = mask ? Math.max(0, Number(mask.feather) || 0) : 0;
      const featherFilter = feather > 0 ? `blur(${(feather * 0.2).toFixed(2)}px)` : '';
      node.style.left = x + 'px';
      node.style.top = y + 'px';
      node.style.width = width + 'px';
      node.style.height = height + 'px';
      node.style.transform = [
        'rotate(' + rotation + 'deg)',
        'scale(' + scaleX + ', ' + scaleY + ')',
        modifierTransform
      ].filter(Boolean).join(' ');
      node.style.opacity = String(opacity);
      // Only an animated width is written each frame: without keys the
      // layer's own CSS has it, including a 4-sided border (FrameCraft).
      if (Array.isArray(track.timeline.borderWidth) && track.timeline.borderWidth.length > 0) {
        node.style.borderWidth = borderWidth + 'px';
      }
      if (track.base.svgTint || node.getAttribute('data-ion-svg-tint') === '1') {
        const wrap = node.firstElementChild;
        if (wrap && wrap.style) {
          wrap.style.filter = silhouetteStrokeFilterJ(borderWidth, track.base.borderColor || '#000000');
        }
      }
      const strokeTargets = node.querySelectorAll('path, polygon, polyline');
      for (let si = 0; si < strokeTargets.length; si += 1) {
        const el = strokeTargets[si];
        if (!el.hasAttribute('stroke') && !el.hasAttribute('stroke-width')) continue;
        el.setAttribute('stroke-width', String(borderWidth));
      }
      if (track.domId.startsWith('txt_')) {
        node.style.fontSize = fontSize + 'px';
        node.style.color = `rgba(${Math.round(textColorR)}, ${Math.round(textColorG)}, ${Math.round(textColorB)}, ${Math.max(0, Math.min(1, textColorA))})`;
        const full =
          node.getAttribute('data-ion-full-text') != null
            ? node.getAttribute('data-ion-full-text')
            : (node.querySelector('[data-ion-text]') || {}).textContent || '';
        let visible =
          typewriterModifier && typewriterModifier.enabled
            ? typewriterVisibleText(full, typewriterModifier.percent)
            : full;
        if (textScrambleModifier && textScrambleModifier.enabled) {
          visible = textScrambleStringJ(visible, textScrambleModifier);
        }
        const staggerOn = !!(textStaggerModifier && textStaggerModifier.enabled);
        const colorOn = !!(textGlyphColorModifier && textGlyphColorModifier.enabled);
        const colors = colorOn ? textGlyphColorsJ(visible, textGlyphColorModifier) : null;
        let glyphs = staggerOn ? textStaggerGlyphsJ(visible, textStaggerModifier) : null;
        if (!glyphs && colors) {
          glyphs = identityGlyphsJ(visible);
        }
        if (glyphs && colors) {
          for (let gi = 0; gi < glyphs.length; gi += 1) {
            if (colors[gi]) glyphs[gi].fill = colors[gi];
          }
        }
        if (pathFollowPose && pathFollowPose.mode === 'textAlong') {
          applyTextAlongPathToNode(
            node,
            pathFollowPose.pathDLocal,
            pathFollowPose.progress,
            visible,
            width,
            height,
            glyphs
          );
        } else {
          clearTextAlongPathOnNode(node);
          if (glyphs) {
            applyTextStaggerToSlot(
              node,
              glyphs,
              staggerOn && textStaggerModifier ? textStaggerModifier.style : null
            );
          } else {
            clearTextStaggerOnNode(node);
            applyTypewriterToTextNode(node, visible);
          }
        }
      }
      // Polygon and path shapes draw in their own SVG: never paint the box;
      // a solid fill color goes to the SVG shape instead.
      const svgShape =
        track.domId.startsWith('shp_') && node.querySelector(':scope > svg > polygon, :scope > svg > path');
      if (svgShape) {
        node.style.backgroundColor = 'transparent';
        node.style.backgroundImage = 'none';
        const solid = String(track.base.backgroundType || 'solid') === 'solid';
        if (solid && !node.hasAttribute('data-ion-path-graphic')) {
          svgShape.setAttribute(
            'fill',
            `rgba(${Math.round(bgColorR)}, ${Math.round(bgColorG)}, ${Math.round(bgColorB)}, ${Math.max(0, Math.min(1, bgColorA))})`
          );
        }
      } else if (track.domId.startsWith('shp_') || track.domId.startsWith('cht_') || track.domId.startsWith('isl_') || (track.domId.startsWith('img_') && track.base.svgTint)) {
        // Gradient fills live in background-image with stop alphas. Never stamp an opaque
        // backgroundColor underneath — that makes semi-transparent stops look solid.
        const bgType = String(track.base.backgroundType || 'solid');
        const markedGradient = bgType === 'gradient';
        const markedImage = bgType === 'image';
        // Tinted SVG images paint on an inner masked layer so silhouette stroke filters stay intact.
        const paintNode =
          track.domId.startsWith('img_') && track.base.svgTint
            ? node.querySelector('[data-ion-svg-tint-fill]') || node
            : node;
        const cssGradient =
          !markedGradient &&
          !markedImage &&
          typeof paintNode.style.backgroundImage === 'string' &&
          paintNode.style.backgroundImage.indexOf('gradient') >= 0;
        if (markedImage) {
          const url = String(track.base.bgImageUrl || '').trim();
          paintNode.style.backgroundImage = url
            ? "url('" + url.replace(/\\/g, '\\\\').replace(/'/g, "\\'") + "')"
            : 'none';
          paintNode.style.backgroundColor = `rgba(${Math.round(bgColorR)}, ${Math.round(bgColorG)}, ${Math.round(bgColorB)}, ${Math.max(0, Math.min(1, bgColorA))})`;
          const fit = String(track.base.bgImageFit || 'cover');
          paintNode.style.backgroundSize =
            fit === 'contain' ? 'contain' : fit === 'stretch' ? '100% 100%' : 'cover';
          const pos = String(track.base.bgImagePosition || 'center');
          const posMap = {
            center: 'center center',
            top: 'center top',
            bottom: 'center bottom',
            left: 'left center',
            right: 'right center',
            'top-left': 'left top',
            'top-right': 'right top',
            'bottom-left': 'left bottom',
            'bottom-right': 'right bottom'
          };
          paintNode.style.backgroundPosition = posMap[pos] || 'center center';
          const rep = String(track.base.bgImageRepeat || 'no-repeat');
          paintNode.style.backgroundRepeat =
            rep === 'repeat' || rep === 'repeat-x' || rep === 'repeat-y' ? rep : 'no-repeat';
        } else if (markedGradient || cssGradient) {
          paintNode.style.backgroundColor = 'transparent';
          if (markedGradient) {
            const angle = Number(track.base.gradientAngle);
            const angleDeg = Number.isFinite(angle) ? angle : 90;
            const gType = String(track.base.gradientType || 'linear');
            const centerRaw = track.base.gradientCenter;
            let cx = 50;
            let cy = 50;
            if (centerRaw && typeof centerRaw === 'object') {
              const nx = Number(centerRaw.x);
              const ny = Number(centerRaw.y);
              if (Number.isFinite(nx)) cx = Math.max(0, Math.min(1, nx)) * 100;
              if (Number.isFinite(ny)) cy = Math.max(0, Math.min(1, ny)) * 100;
            }
            const at = cx + '% ' + cy + '%';
            const ramp = Array.isArray(track.base.gradientRamp) ? track.base.gradientRamp : null;
            let parts = '';
            if (ramp && ramp.length >= 2) {
              parts = ramp
                .map(function (s) {
                  const off = Number(s && s.offset);
                  const pct = Number.isFinite(off) ? Math.round(Math.max(0, Math.min(1, off)) * 1000) / 10 : 0;
                  return String((s && s.color) || 'rgba(255,255,255,1)') + ' ' + pct + '%';
                })
                .join(', ');
            } else {
              const start = track.base.gradientStart || 'rgba(99, 102, 241, 1)';
              const end = track.base.gradientEnd || 'rgba(56, 189, 248, 1)';
              parts = start + ' 0%, ' + end + ' 100%';
            }
            if (gType === 'circle') {
              paintNode.style.backgroundImage = 'radial-gradient(circle at ' + at + ', ' + parts + ')';
            } else if (gType === 'ellipse') {
              paintNode.style.backgroundImage = 'radial-gradient(ellipse at ' + at + ', ' + parts + ')';
            } else if (gType === 'conic') {
              paintNode.style.backgroundImage =
                'conic-gradient(from ' + angleDeg + 'deg at ' + at + ', ' + parts + ')';
            } else {
              paintNode.style.backgroundImage = 'linear-gradient(' + angleDeg + 'deg, ' + parts + ')';
            }
          }
        } else {
          paintNode.style.backgroundImage = 'none';
          paintNode.style.backgroundColor = `rgba(${Math.round(bgColorR)}, ${Math.round(bgColorG)}, ${Math.round(bgColorB)}, ${Math.max(0, Math.min(1, bgColorA))})`;
        }
      }
      node.style.filter = [modifierFilter, featherFilter].filter(Boolean).join(' ') || 'none';
      node.style.clipPath = linkedClipPath || modifierClipPath || 'none';
    }
    const applyChartReveal =
      typeof window !== 'undefined' && typeof window.__ionframeApplyChartReveal === 'function'
        ? window.__ionframeApplyChartReveal
        : null;
    if (applyChartReveal) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('cht_') !== 0) continue;
        const reveal = interpolateTrack(track.timeline.chartReveal, timeMs, 1);
        const fontSize = interpolateTrack(
          track.timeline.fontSize,
          timeMs,
          Number(track.base.fontSize) || 14
        );
        applyChartReveal(track.id, reveal, fontSize);
      }
    }
    const applyParticles =
      typeof window !== 'undefined' && typeof window.__ionframeApplyParticles === 'function'
        ? window.__ionframeApplyParticles
        : null;
    if (applyParticles) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('prt_') !== 0) continue;
        applyParticles(track.id, timeMs);
      }
    }
    const applyLightning =
      typeof window !== 'undefined' && typeof window.__ionframeApplyLightning === 'function'
        ? window.__ionframeApplyLightning
        : null;
    if (applyLightning) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('ltn_') !== 0) continue;
        applyLightning(track.id, timeMs);
      }
    }
    const applyPattern =
      typeof window !== 'undefined' && typeof window.__ionframeApplyPattern === 'function'
        ? window.__ionframeApplyPattern
        : null;
    if (applyPattern) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('pat_') !== 0) continue;
        applyPattern(track.id, timeMs);
      }
    }
    const applyLottie =
      typeof window !== 'undefined' && typeof window.__ionframeApplyLottie === 'function'
        ? window.__ionframeApplyLottie
        : null;
    if (applyLottie) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('lot_') !== 0) continue;
        applyLottie(track.id, timeMs);
      }
    }
    const applyVideo =
      typeof window !== 'undefined' && typeof window.__ionframeApplyVideo === 'function'
        ? window.__ionframeApplyVideo
        : null;
    if (applyVideo) {
      for (const track of animationTracks) {
        if (!track.domId || String(track.domId).indexOf('vid_') !== 0) continue;
        applyVideo(track.id, timeMs);
      }
    }
  }

  function stopPlayback() {
    playbackRunning = false;
    if (playbackRaf !== null) {
      cancelAnimationFrame(playbackRaf);
      playbackRaf = null;
    }
  }

  /** Resolves when the current playback range finishes (or immediately if idle). */
  function waitForPlaybackIdle() {
    return new Promise((resolve) => {
      if (!playbackRunning) {
        resolve(playbackPausedAtMs);
        return;
      }
      const tick = () => {
        if (!playbackRunning) {
          resolve(playbackPausedAtMs);
          return;
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
  }

  function seekTo(timeMs) {
    const clamped = Math.max(0, Math.min(sceneDurationMs, Number(timeMs) || 0));
    playbackPausedAtMs = clamped;
    playbackLastMs = clamped;
    syncFiredApiCuesToTime(clamped);
    // Deterministic seek (Air / export): stage must be visible; CSS starts #gfx at opacity 0 for SPX IN.
    const gfx = e('gfx');
    if (gfx) gfx.style.opacity = '1';
    applyFrame(clamped);
    return clamped;
  }

  function runPlaybackRange(startAtMs, endAtMs) {
    const start = startAtMs == null ? 0 : startAtMs;
    const end = endAtMs == null ? sceneDurationMs : endAtMs;
    stopPlayback();
    playbackPausedAtMs = seekTo(start);
    playbackSegmentStartMs = start;
    playbackSegmentEndMs = Math.max(start, end);
    playbackStartMs = performance.now() - playbackPausedAtMs;
    playbackLastMs = playbackPausedAtMs;
    playbackRunning = true;

    const tick = (now) => {
      if (!playbackRunning) return;
      const elapsed = now - playbackStartMs;
      const clamped = Math.max(playbackSegmentStartMs, Math.min(playbackSegmentEndMs, elapsed));
      applyFrame(clamped);
      runApiCuesForRange(playbackLastMs, clamped);
      playbackLastMs = clamped;
      if (clamped >= playbackSegmentEndMs) {
        stopPlayback();
        playbackPausedAtMs = clamped;
        return;
      }
      playbackRaf = requestAnimationFrame(tick);
    };
    playbackRaf = requestAnimationFrame(tick);
  }

  function runSegment(index) {
    if (!segments.length) {
      runPlaybackRange(0, sceneDurationMs);
      currentSegmentIndex = 1;
      return;
    }
    const safe = Math.max(0, Math.min(segments.length - 1, index));
    const seg = segments[safe];
    runPlaybackRange(seg.start, seg.end);
    currentSegmentIndex = safe + 1;
  }

  function typewriterVisibleText(full, percent) {
    const chars = Array.from(full == null ? '' : String(full));
    if (!chars.length) return '';
    const pct = Math.max(0, Math.min(100, Number(percent) || 0));
    if (pct >= 100) return chars.join('');
    if (pct <= 0) return '';
    const count = Math.floor((pct / 100) * chars.length);
    return chars.slice(0, count).join('');
  }

  function smoothstep01(t) {
    const x = Math.max(0, Math.min(1, t));
    return x * x * (3 - 2 * x);
  }

  const SCRAMBLE_ALNUM =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  const SCRAMBLE_SYMBOLS = '!@#$%^&*()_+-=[]{}|;:,.<>?/~`';

  function resolveScrambleCharsetJ(charset, custom) {
    if (charset === 'symbols') return SCRAMBLE_SYMBOLS;
    if (charset === 'custom') {
      const s = custom == null ? '' : String(custom);
      const cleaned = Array.from(s).filter((c) => c !== '\n' && c !== '\r').join('');
      return cleaned.length > 0 ? cleaned : SCRAMBLE_ALNUM;
    }
    return SCRAMBLE_ALNUM;
  }

  function mulberry32J(seed) {
    let t = seed >>> 0;
    return function () {
      t += 0x6d2b79f5;
      let r = Math.imul(t ^ (t >>> 15), 1 | t);
      r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
      return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
    };
  }

  function textScrambleStringJ(full, mod) {
    const chars = Array.from(full == null ? '' : String(full));
    if (!chars.length) return '';
    const n = chars.length;
    const p = Math.max(0, Math.min(1, (Number(mod && mod.progress) || 0) / 100));
    if (p >= 1) return chars.join('');
    const pool = resolveScrambleCharsetJ(mod && mod.charset, mod && mod.customCharset);
    const poolChars = Array.from(pool);
    if (!poolChars.length) return chars.join('');
    const order = (mod && mod.order) || 'forward';
    const baseSeed = Math.floor(Number(mod && mod.seed) || 0) >>> 0;
    const bucket = Math.floor(p * 64);
    const mid = (n - 1) / 2;
    const maxDist = Math.max(mid, n - 1 - mid) || 1;
    return chars
      .map(function (char, i) {
        if (char === '\n' || char === ' ' || char === '\t') return char;
        let orderT = 0;
        if (n > 1) {
          if (order === 'reverse') orderT = (n - 1 - i) / (n - 1);
          else if (order === 'center') orderT = Math.abs(i - mid) / maxDist;
          else orderT = i / (n - 1);
        }
        const settleAt = orderT;
        const edge = 1 / Math.max(n, 1);
        let settle = 0;
        if (p >= 1) settle = 1;
        else if (p <= 0) settle = 0;
        else if (p < settleAt) settle = 0;
        else if (p >= settleAt + edge) settle = 1;
        else settle = smoothstep01((p - settleAt) / edge);
        if (settle >= 1) return char;
        const rng = mulberry32J((baseSeed ^ (i * 0x9e3779b9) ^ (bucket * 0x85ebca6b)) >>> 0);
        const picks = 1 + Math.floor((1 - settle) * 3);
        let out = poolChars[0];
        for (let k = 0; k < picks; k += 1) {
          out = poolChars[Math.floor(rng() * poolChars.length)];
        }
        return out;
      })
      .join('');
  }

  function parseCssColorToRgbaJ(raw) {
    const s = (raw == null ? '' : String(raw)).trim();
    if (!s) return null;
    if (s.charAt(0) === '#') {
      let hex = s.slice(1);
      if (hex.length === 3 || hex.length === 4) {
        hex = hex
          .split('')
          .map(function (c) {
            return c + c;
          })
          .join('');
      }
      if (hex.length === 6 || hex.length === 8) {
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        const a = hex.length === 8 ? parseInt(hex.slice(6, 8), 16) / 255 : 1;
        if ([r, g, b].every(function (n) {
          return isFinite(n);
        })) {
          return { r: r, g: g, b: b, a: isFinite(a) ? a : 1 };
        }
      }
      return null;
    }
    const m = s.match(
      /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(?:\s*,\s*([\d.]+))?\s*\)$/i
    );
    if (!m) return null;
    const r = Number(m[1]);
    const g = Number(m[2]);
    const b = Number(m[3]);
    const a = m[4] != null ? Number(m[4]) : 1;
    if (![r, g, b, a].every(function (n) {
      return isFinite(n);
    })) {
      return null;
    }
    return { r: r, g: g, b: b, a: a };
  }

  function rgbaToCssJ(c) {
    const r = Math.round(Math.max(0, Math.min(255, c.r)));
    const g = Math.round(Math.max(0, Math.min(255, c.g)));
    const b = Math.round(Math.max(0, Math.min(255, c.b)));
    const a = Math.max(0, Math.min(1, c.a));
    if (a >= 1 - 1e-6) {
      return (
        '#' +
        [r, g, b]
          .map(function (n) {
            return n.toString(16).padStart(2, '0');
          })
          .join('')
      );
    }
    return 'rgba(' + r + ',' + g + ',' + b + ',' + Math.round(a * 1000) / 1000 + ')';
  }

  function mixCssColorsJ(from, to, t) {
    const c0 = parseCssColorToRgbaJ(from) || { r: 255, g: 255, b: 255, a: 1 };
    const c1 = parseCssColorToRgbaJ(to) || c0;
    const u = Math.max(0, Math.min(1, t));
    return rgbaToCssJ({
      r: c0.r + (c1.r - c0.r) * u,
      g: c0.g + (c1.g - c0.g) * u,
      b: c0.b + (c1.b - c0.b) * u,
      a: c0.a + (c1.a - c0.a) * u
    });
  }

  function textGlyphColorsJ(full, mod) {
    const chars = Array.from(full == null ? '' : String(full));
    if (!chars.length) return [];
    const n = chars.length;
    const amount = Math.max(0, Math.min(1, (Number(mod && mod.amount) || 0) / 100));
    const p = Math.max(0, Math.min(1, (Number(mod && mod.progress) || 0) / 100));
    const order = (mod && mod.order) || 'forward';
    const from =
      mod && typeof mod.fromColor === 'string' && mod.fromColor.trim()
        ? mod.fromColor.trim()
        : '#94a3b8';
    const to =
      mod && typeof mod.toColor === 'string' && mod.toColor.trim()
        ? mod.toColor.trim()
        : '#f8fafc';
    const mid = (n - 1) / 2;
    const maxDist = Math.max(mid, n - 1 - mid) || 1;
    return chars.map(function (char, i) {
      if (char === '\n') return to;
      let orderT = 0;
      if (n > 1) {
        if (order === 'reverse') orderT = (n - 1 - i) / (n - 1);
        else if (order === 'center') orderT = Math.abs(i - mid) / maxDist;
        else orderT = i / (n - 1);
      }
      const delay = amount * orderT * (n > 1 ? (n - 1) / n : 0);
      const dur = Math.max(1e-6, 1 - amount * ((n - 1) / Math.max(1, n)));
      const eased = smoothstep01((p - delay) / dur);
      return mixCssColorsJ(from, to, eased);
    });
  }

  function identityGlyphsJ(full) {
    const chars = Array.from(full == null ? '' : String(full));
    return chars.map(function (char) {
      return {
        char: char === ' ' ? '\u00a0' : char,
        opacity: 1,
        transform: 'none',
        offsetXEm: 0,
        offsetYEm: 0,
        scale: 1,
        scaleX: 1,
        svgDxEm: 0,
        svgDyEm: 0,
        isNewline: char === '\n'
      };
    });
  }

  function composeGlyphTransformJ(offsetXEm, offsetYEm, scale, scaleX) {
    const parts = [];
    if (Math.abs(offsetXEm) > 1e-6 || Math.abs(offsetYEm) > 1e-6) {
      parts.push(
        'translate(' + offsetXEm.toFixed(4) + 'em, ' + offsetYEm.toFixed(4) + 'em)'
      );
    }
    const sx = scale * scaleX;
    const sy = scale;
    if (Math.abs(sx - 1) > 1e-4 || Math.abs(sy - 1) > 1e-4) {
      parts.push('scale(' + sx.toFixed(4) + ', ' + sy.toFixed(4) + ')');
    }
    return parts.length ? parts.join(' ') : 'none';
  }

  function staggerOrderTJ(i, n, order) {
    if (n <= 1) return 0;
    if (order === 'reverse') return (n - 1 - i) / (n - 1);
    if (order === 'center') {
      const mid = (n - 1) / 2;
      const dist = Math.abs(i - mid);
      const maxDist = Math.max(mid, n - 1 - mid) || 1;
      return dist / maxDist;
    }
    return i / (n - 1);
  }

  function staggerMotionJ(style, eased, ctx) {
    const u = 1 - eased;
    const mid = (ctx.n - 1) / 2;
    const orderT = staggerOrderTJ(ctx.i, ctx.n, ctx.order);
    if (style === 'scale') {
      const scale = 0.35 + 0.65 * eased;
      return {
        opacity: eased,
        transform: composeGlyphTransformJ(0, 0, scale, 1),
        offsetXEm: 0,
        offsetYEm: 0,
        scale: scale,
        scaleX: 1
      };
    }
    if (style === 'slideUp') {
      return {
        opacity: eased,
        transform: composeGlyphTransformJ(0, u * 0.55, 1, 1),
        offsetXEm: 0,
        offsetYEm: u * 0.55,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'slideDown') {
      return {
        opacity: eased,
        transform: composeGlyphTransformJ(0, -u * 0.55, 1, 1),
        offsetXEm: 0,
        offsetYEm: -u * 0.55,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'slideLeft') {
      return {
        opacity: eased,
        transform: composeGlyphTransformJ(u * 0.45, 0, 1, 1),
        offsetXEm: u * 0.45,
        offsetYEm: 0,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'slideRight') {
      return {
        opacity: eased,
        transform: composeGlyphTransformJ(-u * 0.45, 0, 1, 1),
        offsetXEm: -u * 0.45,
        offsetYEm: 0,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'tracking') {
      const track = 0.04 + 0.9 * (ctx.spread01 || 0);
      const offsetXEm = (ctx.i - mid) * track * eased;
      return {
        opacity: 1,
        transform: composeGlyphTransformJ(offsetXEm, 0, 1, 1),
        offsetXEm: offsetXEm,
        offsetYEm: 0,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'stretch') {
      const track = 0.04 + 0.8 * (ctx.spread01 || 0);
      const offsetXEm = (ctx.i - mid) * track * eased;
      const scaleX = 1 + (0.1 + 0.9 * (ctx.amplitude01 || 0)) * eased;
      return {
        opacity: 1,
        transform: composeGlyphTransformJ(offsetXEm, 0, 1, scaleX),
        offsetXEm: offsetXEm,
        offsetYEm: 0,
        scale: 1,
        scaleX: scaleX
      };
    }
    if (style === 'wave') {
      const amp = 0.02 + 0.7 * (ctx.amplitude01 || 0);
      const cycles = 0.5 + 7.5 * (ctx.speed01 || 0);
      const phaseDir = ctx.order === 'reverse' ? -1 : 1;
      const phaseSpread = ctx.order === 'center' ? orderT * 1.8 : ctx.i * 0.55;
      const phase = ctx.progress01 * Math.PI * 2 * cycles + phaseDir * phaseSpread;
      const offsetYEm = amp * Math.sin(phase);
      const offsetXEm = amp * 0.4 * Math.cos(phase * 1.15 + ctx.i * 0.2);
      return {
        opacity: 1,
        transform: composeGlyphTransformJ(offsetXEm, offsetYEm, 1, 1),
        offsetXEm: offsetXEm,
        offsetYEm: offsetYEm,
        scale: 1,
        scaleX: 1
      };
    }
    if (style === 'nervous') {
      const amp = 0.02 + 0.4 * (ctx.amplitude01 || 0);
      const buckets = Math.max(1, Math.round(8 + 88 * (ctx.speed01 || 0)));
      const bucket = Math.floor(ctx.progress01 * buckets);
      const rng = mulberry32J(
        ((ctx.i * 0x9e3779b9) ^ (bucket * 0x85ebca6b) ^ 0xc0ffee01) >>> 0
      );
      const offsetXEm = (rng() - 0.5) * 2 * amp;
      const offsetYEm = (rng() - 0.5) * 2 * amp;
      return {
        opacity: 1,
        transform: composeGlyphTransformJ(offsetXEm, offsetYEm, 1, 1),
        offsetXEm: offsetXEm,
        offsetYEm: offsetYEm,
        scale: 1,
        scaleX: 1
      };
    }
    return {
      opacity: eased,
      transform: 'none',
      offsetXEm: 0,
      offsetYEm: 0,
      scale: 1,
      scaleX: 1
    };
  }

  function textStaggerGlyphsJ(full, mod) {
    const chars = Array.from(full == null ? '' : String(full));
    if (!chars.length) return [];
    const n = chars.length;
    const amount = Math.max(0, Math.min(1, (Number(mod && mod.amount) || 0) / 100));
    const spread01 = Math.max(
      0,
      Math.min(1, (Number(mod && mod.spread != null ? mod.spread : 55) || 0) / 100)
    );
    const amplitude01 = Math.max(
      0,
      Math.min(1, (Number(mod && mod.amplitude != null ? mod.amplitude : 55) || 0) / 100)
    );
    const speed01 = Math.max(
      0,
      Math.min(1, (Number(mod && mod.speed != null ? mod.speed : 45) || 0) / 100)
    );
    let p = Math.max(0, Math.min(1, (Number(mod && mod.progress) || 0) / 100));
    if (mod && mod.mode === 'out') p = 1 - p;
    const style = (mod && mod.style) || 'fade';
    const order = (mod && mod.order) || 'forward';
    const continuous = style === 'wave' || style === 'nervous';
    const raw = chars.map(function (char, i) {
      if (char === '\n') {
        return {
          char: char,
          opacity: 1,
          transform: 'none',
          offsetXEm: 0,
          offsetYEm: 0,
          scale: 1,
          scaleX: 1,
          svgDxEm: 0,
          svgDyEm: 0,
          isNewline: true
        };
      }
      const orderT = staggerOrderTJ(i, n, order);
      const delay = continuous ? 0 : amount * orderT * (n > 1 ? (n - 1) / n : 0);
      const dur = continuous
        ? 1
        : Math.max(1e-6, 1 - amount * ((n - 1) / Math.max(1, n)));
      const eased = continuous ? p : smoothstep01((p - delay) / dur);
      const motion = staggerMotionJ(style, eased, {
        i: i,
        n: n,
        amount01: amount,
        progress01: p,
        order: order,
        spread01: spread01,
        amplitude01: amplitude01,
        speed01: speed01
      });
      return {
        char: char === ' ' ? '\u00a0' : char,
        opacity: motion.opacity,
        transform: motion.transform,
        offsetXEm: motion.offsetXEm,
        offsetYEm: motion.offsetYEm,
        scale: motion.scale,
        scaleX: motion.scaleX,
        svgDxEm: 0,
        svgDyEm: 0,
        isNewline: false
      };
    });
    let prevX = 0;
    let prevY = 0;
    for (let i = 0; i < raw.length; i += 1) {
      const g = raw[i];
      if (g.isNewline) {
        prevX = 0;
        prevY = 0;
        continue;
      }
      g.svgDxEm = g.offsetXEm - prevX;
      g.svgDyEm = g.offsetYEm - prevY;
      prevX = g.offsetXEm;
      prevY = g.offsetYEm;
    }
    return raw;
  }

  function applyTextStaggerToSlot(node, glyphs, style) {
    if (!node || !Array.isArray(glyphs)) return;
    const slot = node.querySelector('[data-ion-text]');
    if (!slot) return;
    const needsVert =
      style === 'wave' ||
      style === 'nervous' ||
      style === 'slideUp' ||
      style === 'slideDown';
    slot.setAttribute('data-ion-stagger', '1');
    slot.style.whiteSpace = 'pre-wrap';
    slot.style.display = 'flex';
    slot.style.flexDirection = 'column';
    slot.style.justifyContent = needsVert ? 'center' : 'flex-start';
    slot.style.height = '100%';
    slot.style.boxSizing = 'border-box';
    slot.style.overflow = needsVert ? 'visible' : 'hidden';
    if (needsVert) node.style.overflow = 'visible';
    while (slot.firstChild) slot.removeChild(slot.firstChild);
    const wrap = document.createElement('div');
    wrap.style.width = '100%';
    for (let i = 0; i < glyphs.length; i += 1) {
      const g = glyphs[i];
      if (!g) continue;
      if (g.isNewline) {
        wrap.appendChild(document.createElement('br'));
        continue;
      }
      const span = document.createElement('span');
      span.style.display = 'inline-block';
      span.style.opacity = String(g.opacity);
      span.style.transform = g.transform || 'none';
      span.style.transformOrigin = 'center center';
      if (g.fill) span.style.color = String(g.fill);
      span.textContent = g.char;
      wrap.appendChild(span);
    }
    slot.appendChild(wrap);
  }

  function clearTextStaggerOnNode(node) {
    if (!node) return;
    const slot = node.querySelector('[data-ion-text]');
    if (!slot || slot.getAttribute('data-ion-stagger') !== '1') return;
    slot.removeAttribute('data-ion-stagger');
    slot.style.display = '';
    slot.style.flexDirection = '';
    slot.style.justifyContent = '';
    slot.style.height = '';
    slot.style.boxSizing = '';
    slot.style.overflow = '';
    node.style.overflow = '';
    const full = node.getAttribute('data-ion-full-text');
    slot.textContent = full != null ? full : '';
  }

  function applyTypewriterToTextNode(node, visibleText) {
    if (!node) return;
    const slot = node.querySelector('[data-ion-text]');
    if (slot) slot.textContent = visibleText == null ? '' : String(visibleText);
  }

  /** Hidden SPX fields live under display:none — use textContent, never innerText. */
  function fieldText(fieldId) {
    const node = e(fieldId);
    return node ? String(node.textContent || '').trim() : '';
  }

  function applyTextLayers() {
    const liveBag =
      (typeof window !== 'undefined' && window.__ionboardLiveValues && typeof window.__ionboardLiveValues === 'object'
        ? window.__ionboardLiveValues
        : null) || ionboardLiveValues;
    for (const row of textLayers) {
      if (!row || !row.id) continue;
      const target = e('txt_' + row.id);
      if (!target) continue;
      const fallback = row.fallback == null ? '' : String(row.fallback);
      const fieldId = (row.fieldId || '').trim();
      let next = fallback;
      if (fieldId) {
        const hasLive = liveBag && Object.prototype.hasOwnProperty.call(liveBag, fieldId);
        const fromDom = fieldText(fieldId);
        const raw = hasLive ? String(liveBag[fieldId] ?? '').trim() : fromDom;
        next = htmlDecode(raw || fallback);
      }
      // Only seed the full string — typewriter / stagger / path-follow paint via applyFrame.
      target.setAttribute('data-ion-full-text', next);
    }
    const t = playbackRunning ? playbackLastMs : playbackPausedAtMs;
    applyFrame(t);
  }

  function resolveImageSrc(raw) {
    const s = String(raw || '').trim();
    if (!s) return '';
    if (
      s.indexOf('http://') === 0 ||
      s.indexOf('https://') === 0 ||
      s.indexOf('data:') === 0 ||
      s.indexOf('/') === 0 ||
      s.indexOf('./') === 0
    ) {
      return s;
    }
    return s;
  }

  function applyImageLayers() {
    const liveBag =
      (typeof window !== 'undefined' && window.__ionboardLiveValues && typeof window.__ionboardLiveValues === 'object'
        ? window.__ionboardLiveValues
        : null) || ionboardLiveValues;
    for (const row of imageLayers) {
      if (!row || !row.id) continue;
      const target = e('isl_' + row.id);
      if (!target) continue;
      const img = target.querySelector('[data-ion-image-slot]');
      if (!img) continue;
      const fallback = row.fallback == null ? '' : String(row.fallback).trim();
      const fieldId = (row.fieldId || '').trim();
      let next = fallback;
      if (fieldId) {
        const hasLive = liveBag && Object.prototype.hasOwnProperty.call(liveBag, fieldId);
        const fromDom = fieldText(fieldId);
        const raw = hasLive ? String(liveBag[fieldId] ?? '').trim() : fromDom;
        next = htmlDecode(raw || fallback);
      }
      const src = resolveImageSrc(next);
      const fit = row.fit === 'contain' ? 'contain' : 'cover';
      img.style.objectFit = fit;
      if (src) {
        img.src = src;
        img.style.display = 'block';
        img.onerror = function () {
          img.style.display = 'none';
        };
      } else {
        img.removeAttribute('src');
        img.style.display = 'none';
      }
    }
  }

  function coerceLiveBool(v) {
    if (typeof v === 'boolean') return v;
    if (typeof v === 'number') return v !== 0 && !Number.isNaN(v);
    const s = String(v ?? '').trim().toLowerCase();
    if (!s) return false;
    return !(s === 'false' || s === '0' || s === 'off' || s === 'no' || s === 'null' || s === 'undefined');
  }

  function resolveVisibilityNode(row) {
    if (!row) return null;
    let target = row.domId ? e(row.domId) : null;
    if (target) return target;
    if (!row.id) return null;
    const prefixes = ['shp_', 'txt_', 'img_', 'isl_', 'cht_', 'prt_', 'lot_', 'vid_', 'emp_'];
    for (const prefix of prefixes) {
      target = e(prefix + row.id);
      if (target) return target;
    }
    return null;
  }

  function applyVisibilityLayers() {
    const liveBag =
      (typeof window !== 'undefined' && window.__ionboardLiveValues && typeof window.__ionboardLiveValues === 'object'
        ? window.__ionboardLiveValues
        : null) || ionboardLiveValues;
    for (const row of visibilityLayers) {
      if (!row) continue;
      const fieldId = (row.fieldId || '').trim();
      if (!fieldId) continue;
      const target = resolveVisibilityNode(row);
      if (!target) continue;
      if (!liveBag || !Object.prototype.hasOwnProperty.call(liveBag, fieldId)) {
        continue;
      }
      let on = coerceLiveBool(liveBag[fieldId]);
      if (row.invert) on = !on;
      target.style.visibility = on ? 'visible' : 'hidden';
      if (on) target.removeAttribute('data-ion-live-hidden');
      else target.setAttribute('data-ion-live-hidden', '1');
    }
  }

  window.runTemplateUpdate = function runTemplateUpdate() {
    applyTextLayers();
    applyImageLayers();
    applyVisibilityLayers();
    currentSegmentIndex = 0;
    firedApiCueIds = new Set();
    seekTo(0);
    // Do not auto-play — SPX play() / OGraf playAction() start IN separately.
    // (Calling IN here made OGraf load() via update() start playback before Play.)
  };

  window.runAnimationIN = function runAnimationIN() {
    dbg('runAnimationIN', { pausedAt: playbackPausedAtMs, segmentIndex: currentSegmentIndex });
    animate('#gfx', {
      opacity: [0, 1],
      duration: 450,
      easing: 'easeOutCubic'
    });
    firedApiCueIds = new Set();
    runSegment(0);
    return waitForPlaybackIdle().then((t) => {
      dbg('runAnimationIN idle', { pausedAt: t, segmentIndex: currentSegmentIndex });
      return t;
    });
  };

  window.runAnimationNext = function runAnimationNext() {
    dbg('runAnimationNEXT', {
      pausedAt: playbackPausedAtMs,
      segmentIndex: currentSegmentIndex,
      segmentCount: segments.length
    });
    if (!segments.length) {
      runPlaybackRange(playbackPausedAtMs, sceneDurationMs);
      currentSegmentIndex = 1;
      return waitForPlaybackIdle();
    }
    if (currentSegmentIndex >= segments.length) {
      dbg('runAnimationNEXT no-op (past last segment)');
      return Promise.resolve(playbackPausedAtMs);
    }
    runSegment(currentSegmentIndex);
    return waitForPlaybackIdle().then((t) => {
      dbg('runAnimationNEXT idle', { pausedAt: t, segmentIndex: currentSegmentIndex });
      return t;
    });
  };
  // SPX templates commonly expose uppercase NEXT.
  window.runAnimationNEXT = window.runAnimationNext;

  /** Play remaining timeline from the hold, then fade #gfx out (SPX stop / OGraf end). */
  window.runAnimationOUT = function runAnimationOUT() {
    dbg('runAnimationOUT start', {
      pausedAt: playbackPausedAtMs,
      duration: sceneDurationMs,
      segmentIndex: currentSegmentIndex,
      gfxOpacity: (e('gfx') && e('gfx').style.opacity) || null
    });
    currentSegmentIndex = segments.length;

    const fadeOut = () => {
      stopPlayback();
      const gfx = e('gfx');
      const from = gfx
        ? Math.max(0, Math.min(1, parseFloat(String(gfx.style.opacity || '1')) || 1))
        : 1;
      dbg('runAnimationOUT fade', { from });
      const anim = animate('#gfx', {
        opacity: [from, 0],
        duration: 450,
        easing: 'easeInCubic'
      });
      const done =
        anim && typeof anim.then === 'function'
          ? anim
          : new Promise((resolve) => setTimeout(resolve, 450));
      return Promise.resolve(done).then(() => {
        if (gfx) gfx.style.opacity = '0';
        dbg('runAnimationOUT fade done', { opacity: gfx && gfx.style.opacity });
        return playbackPausedAtMs;
      });
    };

    // Play rest of timeline first (visible). Do NOT fade before this — seekTo()
    // forces #gfx opacity to 1 and would cancel an early fade.
    if (playbackPausedAtMs < sceneDurationMs - 1) {
      dbg('runAnimationOUT play remaining', {
        from: playbackPausedAtMs,
        to: sceneDurationMs
      });
      runPlaybackRange(playbackPausedAtMs, sceneDurationMs);
      return waitForPlaybackIdle().then((t) => {
        dbg('runAnimationOUT remaining idle', { pausedAt: t });
        return fadeOut();
      });
    }
    dbg('runAnimationOUT already at end → fade only');
    return fadeOut();
  };

  function timestampFromGoToTimeArg(arg) {
    if (arg && typeof arg === 'object' && arg.timestamp != null) return Number(arg.timestamp) || 0;
    return Number(arg) || 0;
  }

  /** EBU OGraf: `goToTime({ timestamp })`. Also accepts a millisecond number. */
  window.goToTime = function goToTime(params) {
    stopPlayback();
    const t = seekTo(timestampFromGoToTimeArg(params));
    return Promise.resolve({ statusCode: 200, timestamp: t });
  };
  /** @deprecated Use `goToTime({ timestamp })`. Returns the clamped time in ms. */
  window.seek = function seek(timeMs) {
    stopPlayback();
    return seekTo(timeMs);
  };
  window.runTemplateSeek = window.seek;

  window.addEventListener('message', (event) => {
    const msg = parseMessageData(event && event.data);
    if (!msg || msg.type !== 'ionframe-spx') return;
    if (msg.action === 'ping') {
      postBack(event, { type: 'ionframe-spx', action: 'pong', replyId: msg.replyId, sceneDurationMs });
      return;
    }
    if ((msg.action === 'goToTime' || msg.action === 'seek') &&
        ('timestamp' in msg || 'timeMs' in msg)) {
      stopPlayback();
      const t = seekTo(msg.timestamp != null ? msg.timestamp : msg.timeMs);
      if (msg.replyId != null) {
        postBack(event, {
          type: 'ionframe-spx',
          action: msg.action === 'goToTime' ? 'goToTimeDone' : 'seekDone',
          timestamp: t,
          timeMs: t,
          replyId: msg.replyId
        });
      }
      return;
    }
    if (msg.action === 'next') {
      window.runAnimationNext();
      if (msg.replyId != null) {
        postBack(event, { type: 'ionframe-spx', action: 'nextDone', replyId: msg.replyId });
      }
      return;
    }
    if (msg.action === 'in') {
      window.runAnimationIN();
      if (msg.replyId != null) {
        postBack(event, { type: 'ionframe-spx', action: 'inDone', replyId: msg.replyId });
      }
      return;
    }
    if (msg.action === 'out') {
      window.runAnimationOUT();
      if (msg.replyId != null) {
        postBack(event, { type: 'ionframe-spx', action: 'outDone', replyId: msg.replyId });
      }
      return;
    }
    if (msg.action === 'getDuration') {
      postBack(event, { type: 'ionframe-spx', action: 'duration', durationMs: sceneDurationMs, replyId: msg.replyId });
      return;
    }
    if (msg.action === 'getRenderInfo') {
      postBack(event, {
        type: 'ionframe-spx',
        action: 'renderInfo',
        durationMs: sceneDurationMs,
        pausePointsMs: pausePoints.slice(),
        segments: segments.map((seg, index) => ({
          index,
          startMs: seg.start,
          endMs: seg.end
        })),
        replyId: msg.replyId
      });
      return;
    }
    if (msg.action === 'getStep') {
      postBack(event, {
        type: 'ionframe-spx',
        action: 'step',
        currentSegmentIndex,
        segmentCount: segments.length,
        replyId: msg.replyId
      });
    }
  });

  window.IonFrameSpx = {
    version: 1,
    goToTime: window.goToTime,
    seek: window.seek,
    runAnimationNext: window.runAnimationNext,
    runAnimationNEXT: window.runAnimationNEXT,
    waitForPlaybackIdle,
    isPlaybackRunning: function isPlaybackRunning() {
      return playbackRunning;
    },
    getDuration: function getDuration() {
      return sceneDurationMs;
    },
    getPausePoints: function getPausePoints() {
      return pausePoints.slice();
    },
    getStep: function getStep() {
      return { currentSegmentIndex, segmentCount: segments.length };
    },
    /** Re-paint current frame without stopping playback (live text / image updates). */
    refreshFrame: function refreshFrame() {
      applyFrame(playbackRunning ? playbackLastMs : playbackPausedAtMs);
    }
  };

  /** Used by the inline IonBoard poller before/without touching goToTime(). */
  window.__ionframeRefreshFrame = function __ionframeRefreshFrame() {
    applyFrame(playbackRunning ? playbackLastMs : playbackPausedAtMs);
  };

  /** Stable bridge for MENiAC Air offline render (see meta/air-render.json). */
  window.__ionframeAir = {
    version: 1,
    goToTime: window.goToTime,
    seek: window.seek,
    getDurationMs: function getDurationMs() {
      return sceneDurationMs;
    },
    getPausePointsMs: function getPausePointsMs() {
      return pausePoints.slice();
    },
    getSegments: function getSegments() {
      return segments.map((seg, index) => ({
        index,
        startMs: seg.start,
        endMs: seg.end
      }));
    }
  };

  // Particle layers: deterministic draw-at-t (scrub / SPX seek).
  for (const row of particleLayers) {
    const url = row && typeof row.spriteUrl === 'string' ? row.spriteUrl.trim() : '';
    if (!url || !row.id) continue;
    const img = new Image();
    // FrameCraft's preview: load sprites with CORS so the canvas stays
    // readable (renders and snapshots); exports opened from disk keep FRAME's way.
    if (location.protocol === 'framecraft:' || location.host === 'framecraft.preview') img.crossOrigin = 'anonymous';
    img.onload = () => {
      particleSprites[row.id] = img;
    };
    img.onerror = () => {
      particleSprites[row.id] = null;
    };
    img.src = url;
  }
  window.__ionframeApplyParticles = function applyParticles(id, timeMs) {
    const row = particleLayers.find((p) => p && p.id === id);
    if (!row) return;
    const canvas = e(`prt_canvas_${id}`);
    if (!canvas || typeof canvas.getContext !== 'function') return;
    const track = animationTracks.find((t) => t && t.id === id);
    const w = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.width, timeMs, track.base.width)
          : Number(row.width) || 1
      )
    );
    const h = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.height, timeMs, track.base.height)
          : Number(row.height) || 1
      )
    );
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const emitKeys =
      track && Array.isArray(track.timeline.particleEmitting) ? track.timeline.particleEmitting : [];
    const rateKeys =
      track && Array.isArray(track.timeline.particleSpawnRate) ? track.timeline.particleSpawnRate : [];
    const emittingRaw = emitKeys.length
      ? interpolateStepHold(emitKeys, timeMs, row.emitting === false ? 0 : 1)
      : row.emitting === false
        ? 0
        : 1;
    const spawnRate = rateKeys.length
      ? Math.max(0, interpolateTrack(rateKeys, timeMs, Number(row.spawnRate) || 0))
      : Number(row.spawnRate) || 0;
    const emitterX =
      track && Array.isArray(track.timeline.particleEmitterX) && track.timeline.particleEmitterX.length
        ? interpolateTrack(track.timeline.particleEmitterX, timeMs, Number(row.emitterX) || 0)
        : Number(row.emitterX) || 0;
    const emitterY =
      track && Array.isArray(track.timeline.particleEmitterY) && track.timeline.particleEmitterY.length
        ? interpolateTrack(track.timeline.particleEmitterY, timeMs, Number(row.emitterY) || 0)
        : Number(row.emitterY) || 0;
    const baseEmitterX = Number(row.emitterX) || 0;
    const baseEmitterY = Number(row.emitterY) || 0;
    const xKeys =
      track && Array.isArray(track.timeline.particleEmitterX) ? track.timeline.particleEmitterX : [];
    const yKeys =
      track && Array.isArray(track.timeline.particleEmitterY) ? track.timeline.particleEmitterY : [];
    const baseEmit = row.emitting === false ? 0 : 1;
    const baseRate = Number(row.spawnRate) || 0;
    const cfg = {
      emitting: emittingRaw >= 0.5,
      emitterX,
      emitterY,
      emitterRadius: Number(row.emitterRadius) || 0,
      emitterWidth: Number(row.emitterWidth) || 80,
      emitterHeight: Number(row.emitterHeight) || 48,
      emitterShape: row.emitterShape === 'box' ? 'box' : 'circle',
      emitAngleDeg: Number.isFinite(Number(row.emitAngleDeg)) ? Number(row.emitAngleDeg) : -90,
      spawnRate,
      maxCount: Number(row.maxCount) || 300,
      lifetimeMs: Number(row.lifetimeMs) || 1800,
      size: Number(row.size) || 8,
      sizeEnd: Number(row.sizeEnd) || 2,
      sizeRamp: normalizeValueRamp(
        Array.isArray(row.sizeRamp)
          ? row.sizeRamp.map((s) => ({
              offset: Number(s.offset) || 0,
              value: (() => {
                const v = Number(s.value);
                // Legacy absolute ramps: values > 1 → divide by base size.
                if (Number.isFinite(v) && v > 1.0001) {
                  return Math.min(1, Math.max(0, v / Math.max(0.01, Number(row.size) || 8)));
                }
                return Math.min(1, Math.max(0, Number.isFinite(v) ? v : 0));
              })()
            }))
          : null,
        1,
        Math.min(
          1,
          Math.max(0, (Number(row.sizeEnd) || 2) / Math.max(0.01, Number(row.size) || 8))
        )
      ),
      speed: Number(row.speed) || 80,
      spreadDeg: Number(row.spreadDeg) || 360,
      gravityX: Number(row.gravityX) || 0,
      gravityY: Number(row.gravityY) || 0,
      rotationDeg: Number(row.rotationDeg) || 0,
      rotationRandomDeg: Math.max(0, Number(row.rotationRandomDeg) || 0),
      spinDeg: Number(row.spinDeg) || 0,
      alignToVelocity: row.alignToVelocity === true,
      colorStart: row.colorStart || 'rgba(255,255,255,1)',
      colorEnd: row.colorEnd || 'rgba(255,255,255,0)',
      colorRamp: normalizeColorRamp(
        row.colorRamp,
        row.colorStart || 'rgba(255,255,255,1)',
        row.colorEnd || 'rgba(255,255,255,0)'
      ),
      colorMode: row.colorMode === 'paletteCycle' ? 'paletteCycle' : 'ramp',
      paletteColors: Array.isArray(row.paletteColors)
        ? row.paletteColors.filter((c) => typeof c === 'string' && c.trim())
        : [],
      alphaRamp: normalizeValueRamp(
        Array.isArray(row.alphaRamp) ? row.alphaRamp : null,
        1,
        1
      ),
      blend: row.blend === 'normal' ? 'normal' : 'additive',
      attach: row.attach === 'relative' ? 'relative' : 'absolute',
      seed: Number(row.seed) || 1,
      emitterAt: function (t) {
        return {
          x: xKeys.length ? interpolateTrack(xKeys, t, baseEmitterX) : baseEmitterX,
          y: yKeys.length ? interpolateTrack(yKeys, t, baseEmitterY) : baseEmitterY
        };
      },
      emittingAt: function (t) {
        return emitKeys.length
          ? interpolateStepHold(emitKeys, t, baseEmit) >= 0.5
          : baseEmit >= 0.5;
      },
      spawnRateAt: function (t) {
        return rateKeys.length
          ? Math.max(0, interpolateTrack(rateKeys, t, baseRate))
          : Math.max(0, baseRate);
      }
    };
    drawParticlesFrame(ctx, w, h, cfg, timeMs, particleSprites[id] || null);
  };

  window.__ionframeApplyLightning = function applyLightning(id, timeMs) {
    const row = lightningLayers.find((p) => p && p.id === id);
    if (!row) return;
    const canvas = e(`ltn_canvas_${id}`);
    if (!canvas || typeof canvas.getContext !== 'function') return;
    const track = animationTracks.find((t) => t && t.id === id);
    const w = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.width, timeMs, track.base.width)
          : Number(row.layoutWidth) || 1
      )
    );
    const h = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.height, timeMs, track.base.height)
          : Number(row.layoutHeight) || 1
      )
    );
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const emitKeys =
      track && Array.isArray(track.timeline.lightningEmitting) ? track.timeline.lightningEmitting : [];
    const axKeys = track && Array.isArray(track.timeline.lightningAx) ? track.timeline.lightningAx : [];
    const ayKeys = track && Array.isArray(track.timeline.lightningAy) ? track.timeline.lightningAy : [];
    const bxKeys = track && Array.isArray(track.timeline.lightningBx) ? track.timeline.lightningBx : [];
    const byKeys = track && Array.isArray(track.timeline.lightningBy) ? track.timeline.lightningBy : [];
    const baseEmit = row.emitting === false ? 0 : 1;
    const emittingRaw = emitKeys.length
      ? interpolateStepHold(emitKeys, timeMs, baseEmit)
      : baseEmit;
    const baseAx = Number(row.ax) || 0;
    const baseAy = Number(row.ay) || 0;
    const baseBx = Number(row.bx) || 0;
    const baseBy = Number(row.by) || 0;
    drawLightningFrame(
      ctx,
      w,
      h,
      {
        emitting: emittingRaw >= 0.5,
        ax: axKeys.length ? interpolateTrack(axKeys, timeMs, baseAx) : baseAx,
        ay: ayKeys.length ? interpolateTrack(ayKeys, timeMs, baseAy) : baseAy,
        bx: bxKeys.length ? interpolateTrack(bxKeys, timeMs, baseBx) : baseBx,
        by: byKeys.length ? interpolateTrack(byKeys, timeMs, baseBy) : baseBy,
        segments: Number(row.segments) || 14,
        amplitude: Number(row.amplitude) || 0,
        width: Number(row.width) || 2,
        glowWidth: Number(row.glowWidth) || 0,
        color: row.color,
        glowColor: row.glowColor,
        branches: Number(row.branches) || 0,
        seed: Number(row.seed) || 1,
        flickerMs: Number(row.flickerMs) || 48
      },
      timeMs
    );
  };

  window.__ionframeApplyPattern = function applyPattern(id, timeMs) {
    const row = patternLayers.find((p) => p && p.id === id);
    if (!row) return;
    const canvas = e(`pat_canvas_${id}`);
    if (!canvas || typeof canvas.getContext !== 'function') return;
    const track = animationTracks.find((t) => t && t.id === id);
    const w = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.width, timeMs, track.base.width)
          : Number(row.layoutWidth) || 1
      )
    );
    const h = Math.max(
      1,
      Math.round(
        track
          ? interpolateTrack(track.timeline.height, timeMs, track.base.height)
          : Number(row.layoutHeight) || 1
      )
    );
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const phaseKeys =
      track && Array.isArray(track.timeline.patternPhase) ? track.timeline.patternPhase : [];
    const basePhase = Number.isFinite(Number(row.phase)) ? Number(row.phase) : 0;
    const phase = phaseKeys.length ? interpolateTrack(phaseKeys, timeMs, basePhase) : basePhase;
    drawPatternFrame(ctx, w, h, {
      kind: row.kind,
      colorA: row.colorA,
      colorB: row.colorB,
      cellSize: row.cellSize,
      stripeWidth: row.stripeWidth,
      stripeGap: row.stripeGap,
      stripeAngleDeg: row.stripeAngleDeg,
      dotRadius: row.dotRadius,
      lineWidth: row.lineWidth,
      phase
    });
  };

  // Ensure local layout is resolved through hierarchy immediately.
  seekTo(0);

  // IonBoard live hook: poll compact JSON and map keys → text Variable IDs.
  // Do not call window.update() — that restarts the template animation.
  // Inline poller (in HTML) owns polling when present — it sets __ionboardHookInline
  // synchronously before this deferred module runs.
  const ionboardHook = data && data.ionboardHook;
  if (typeof window !== 'undefined' && window.__ionboardHookInline) {
    // Inline HTML poller owns IonBoard live updates.
  } else if (ionboardHook && ionboardHook.enabled) {
    const projectIdField = String(ionboardHook.projectIdField || 'ionboard_project_id').trim();
    const magicIdField = String(ionboardHook.magicIdField || 'ionboard_magic_id').trim();
    const apiBase = String(ionboardHook.apiBase || 'https://board.meniac.tech').replace(/\/$/, '');
    const pollMs = Math.max(200, Number(ionboardHook.pollMs) || 500);
    let lastPayload = '';
    let pollCount = 0;
    let lastHeartbeatAt = 0;

    const missingFieldIds = textLayers.filter((row) => row && !(row.fieldId || '').trim());
    const mappedFieldIds = textLayers
      .map((row) => (row && row.fieldId ? String(row.fieldId).trim() : ''))
      .filter(Boolean);

    if (missingFieldIds.length) {
      console.warn(
        '[IonBoard hook] ' +
          missingFieldIds.length +
          ' text layer(s) have empty Variable ID. Match IonBoard component names, then re-export.'
      );
    }

    let warnedEmptyId = false;
    let warnedKeyMismatch = false;
    let pollTimer = null;
    let pollingActive = false;

    function ensureHiddenField(fieldId) {
      let node = e(fieldId);
      if (node) return node;
      const holder = e('hiddenSpxData') || document.body;
      node = document.createElement('div');
      node.id = fieldId;
      holder.appendChild(node);
      return node;
    }

    function applyIonBoardJson(json) {
      ionboardLiveValues = json && typeof json === 'object' && !Array.isArray(json) ? { ...json } : {};
      if (typeof window !== 'undefined') window.__ionboardLiveValues = ionboardLiveValues;
      for (const key of Object.keys(ionboardLiveValues)) {
        if (key === projectIdField || key === magicIdField) continue;
        const node = ensureHiddenField(key);
        const val = ionboardLiveValues[key];
        node.textContent = val == null ? '' : String(val);
      }
      if (!warnedKeyMismatch && mappedFieldIds.length) {
        const missing = mappedFieldIds.filter((id) => !Object.prototype.hasOwnProperty.call(ionboardLiveValues, id));
        if (missing.length) {
          warnedKeyMismatch = true;
          console.warn(
            '[IonBoard hook] Variable IDs not found in live JSON:',
            missing,
            '— live keys:',
            Object.keys(ionboardLiveValues)
          );
        }
      }
      applyTextLayers();
      applyImageLayers();
      applyVisibilityLayers();
    }

    async function pollIonBoard() {
      if (!pollingActive) return;
      pollCount += 1;
      const projectId = fieldText(projectIdField);
      const magicId = fieldText(magicIdField);
      if (!projectId || !magicId) {
        if (!warnedEmptyId) {
          warnedEmptyId = true;
          console.warn(
            '[IonBoard hook] waiting for Magic ID (#' + magicIdField + ') and Project ID (#' + projectIdField + ')'
          );
        }
        return;
      }
      warnedEmptyId = false;
      const url =
        apiBase +
        '/api/live_board.php?project_id=' +
        encodeURIComponent(projectId) +
        '&format=data&magic=' +
        encodeURIComponent(magicId);
      try {
        const res = await fetch(url, {
          cache: 'no-store',
          credentials: 'omit',
          headers: { 'X-IonBoard-Magic': magicId }
        });
        if (!pollingActive) return;
        if (!res.ok) {
          console.warn('[IonBoard hook] poll HTTP ' + res.status);
          return;
        }
        const json = await res.json();
        if (!pollingActive) return;
        if (!json || typeof json !== 'object' || Array.isArray(json)) return;
        const encoded = JSON.stringify(json);
        if (encoded === lastPayload) {
          if (Object.keys(ionboardLiveValues).length) {
            applyTextLayers();
            applyImageLayers();
            applyVisibilityLayers();
          }
          return;
        }
        lastPayload = encoded;
        lastHeartbeatAt = Date.now();
        applyIonBoardJson(json);
      } catch (err) {
        if (pollingActive) console.warn('[IonBoard hook] poll failed', err);
      }
    }

    function startPolling(reason) {
      if (pollingActive) {
        lastPayload = '';
        void pollIonBoard();
        return;
      }
      pollingActive = true;
      void pollIonBoard();
      pollTimer = setInterval(pollIonBoard, pollMs);
    }

    function stopPolling(reason) {
      if (!pollingActive && !pollTimer) return;
      pollingActive = false;
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    }

    window.__ionboardStartPolling = startPolling;
    window.__ionboardStopPolling = stopPolling;

    const previousUpdate = window.update;
    window.update = function ionboardWrappedUpdate(data) {
      if (typeof previousUpdate === 'function') {
        previousUpdate(data);
      } else {
        const templateData = typeof data === 'string' ? JSON.parse(data) : data;
        Object.keys(templateData || {}).forEach((field) => {
          const target = e(field);
          if (target) target.textContent = templateData[field] ?? '';
        });
      }
      lastPayload = '';
      startPolling('spx update');
    };

    const previousPlay = window.play;
    window.play = function ionboardWrappedPlay() {
      startPolling('spx play');
      if (typeof previousPlay === 'function') previousPlay();
    };

    const previousStop = window.stop;
    window.stop = function ionboardWrappedStop() {
      stopPolling('spx stop / play-out');
      if (typeof previousStop === 'function') previousStop();
    };

    startPolling('init');
  }

  if (data.autoplay) {
    window.runAnimationIN();
  }

  // Offline / Air / first paint: seed text/images from SPX fields or fallbacks without waiting for update().
  applyTextLayers();
  applyImageLayers();

  // Author onLoad (template.js → window.__ionframeScripts.onLoad).
  try {
    const scripts = typeof window !== 'undefined' ? window.__ionframeScripts : null;
    if (scripts && typeof scripts.onLoad === 'function') {
      scripts.onLoad({ reason: 'init' });
    }
  } catch (err) {
    console.error('[template.js] onLoad failed', err);
  }

  // Flush SPX calls that arrived before this module finished loading.
  // Both can be pending (OGraf load→update, then playAction→play before init).
  if (window.__spxNeedsTemplateUpdate) {
    window.__spxNeedsTemplateUpdate = false;
    window.runTemplateUpdate();
  }
  if (window.__spxNeedsPlay) {
    window.__spxNeedsPlay = false;
    window.runAnimationIN();
  }
}
