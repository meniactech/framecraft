// Minimal SPX interface bridge for draft exports.
function e(id) { return document.getElementById(id); }
function htmlDecode(input) {
  const txt = document.createElement('textarea');
  txt.innerHTML = input || '';
  return txt.value;
}
// ES module init is async — SPX often calls update/play before handlers exist.
window.__spxNeedsTemplateUpdate = false;
window.__spxNeedsPlay = false;
window.update = function update(data) {
  // SPX may pass a JSON string; OGraf updateAction may pass { data: fields }.
  let templateData = typeof data === 'string' ? JSON.parse(data) : data;
  if (templateData && templateData.data && typeof templateData.data === 'object' && !Array.isArray(templateData.data)) {
    const looksLikeSpxEnvelope =
      Object.keys(templateData).every((k) => k === 'data' || k === 'comment' || k === 'epochID');
    if (looksLikeSpxEnvelope || Object.keys(templateData).length === 1) {
      templateData = templateData.data;
    }
  }
  Object.keys(templateData || {}).forEach((field) => {
    const target = e(field);
    // textContent: hidden #hiddenSpxData fields are display:none (innerText is empty there).
    if (target) target.textContent = templateData[field] ?? '';
  });
  if (typeof window.runTemplateUpdate === 'function') {
    window.runTemplateUpdate();
  } else {
    window.__spxNeedsTemplateUpdate = true;
  }
};
window.play = function play() {
  console.log('[IonFrame SPX] window.play()');
  if (typeof window.__ionboardStartPolling === 'function') {
    window.__ionboardStartPolling('spx play');
  }
  if (typeof window.runAnimationIN === 'function') {
    return window.runAnimationIN();
  }
  window.__spxNeedsPlay = true;
};
window.stop = function stop() {
  console.log('[IonFrame SPX] window.stop()', {
    hasOUT: typeof window.runAnimationOUT
  });
  if (typeof window.__ionboardStopPolling === 'function') {
    window.__ionboardStopPolling('spx stop / play-out');
  }
  if (typeof window.runAnimationOUT === 'function') return window.runAnimationOUT();
  console.warn('[IonFrame SPX] window.stop(): runAnimationOUT missing');
};
window.next = function next() {
  console.log('[IonFrame SPX] window.next()');
  if (typeof window.runAnimationNEXT === 'function') return window.runAnimationNEXT();
  if (typeof window.runAnimationNext === 'function') return window.runAnimationNext();
};
