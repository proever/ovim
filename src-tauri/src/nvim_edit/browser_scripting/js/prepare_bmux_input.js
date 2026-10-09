(function () {
  function deepActiveElement(element) {
    if (element && element.shadowRoot && element.shadowRoot.activeElement) {
      return deepActiveElement(element.shadowRoot.activeElement);
    }
    return element;
  }

  // Reddit chat nests its composer inside several open shadow roots.
  function findTarget(root, selector) {
    var found = root.querySelector(selector);
    if (found) return found;
    var elements = root.querySelectorAll("*");
    for (var i = 0; i < elements.length; i++) {
      if (elements[i].shadowRoot) {
        found = findTarget(elements[i].shadowRoot, selector);
        if (found) return found;
      }
    }
    return null;
  }

  var token = "{{TARGET_ELEMENT_ID}}";
  var host = token
    ? findTarget(document, '[data-ovim-editor="' + token + '"]')
    : null;
  var editable = null;

  if (host) {
    editable = host.matches('input, textarea, [contenteditable="true"]')
      ? host
      : host.querySelector('[contenteditable="true"]');
  } else {
    editable = deepActiveElement(document.activeElement);
    if (!editable || editable === document.body || editable === document.documentElement) {
      return "no_element";
    }
    if (!editable.matches('input, textarea, [contenteditable="true"]')) {
      editable = editable.closest('[contenteditable="true"]');
    }
    if (!editable) return "unsupported_element";

    host = editable.closest(".DraftEditor-root") || editable;
    token = "ovim-" + Date.now() + "-" + Math.random().toString(36).slice(2);
    host.setAttribute("data-ovim-editor", token);
  }

  if (!editable) return "target_missing";
  // Keep trusted select-all/type for controlled editors, but do not paint
  // their temporary selection between separate CLI round trips.
  var style = document.createElement("style");
  style.textContent = '[data-ovim-editor="' + token + '"]::selection, ' +
    '[data-ovim-editor="' + token + '"] ::selection {' +
    'background: transparent !important; color: inherit !important; }';
  var root = editable.getRootNode();
  (root === document ? (document.head || document.documentElement) : root).appendChild(style);
  function editorSelection() {
    return root.getSelection ? root.getSelection() : window.getSelection();
  }
  var cleanups = window.__ovimInputCleanups || (window.__ovimInputCleanups = {});
  if (cleanups[token]) cleanups[token]();
  var timer;
  var cleaned = false;
  var cleanup = function () {
    if (cleaned) return;
    cleaned = true;
    clearTimeout(timer);
    try {
      if (editable.tagName === "INPUT" || editable.tagName === "TEXTAREA") {
        editable.setSelectionRange(editable.selectionEnd, editable.selectionEnd);
      } else {
        var selection = editorSelection();
        if (selection && selection.rangeCount && editable.contains(selection.anchorNode) &&
            editable.contains(selection.focusNode)) selection.collapseToEnd();
      }
    } finally {
      style.remove();
      delete host.__ovimInputCleanup;
      delete cleanups[token];
    }
  };
  host.__ovimInputCleanup = cleanup;
  cleanups[token] = cleanup;
  // Restore normal selection even if Ovim exits or the input channel fails.
  timer = setTimeout(cleanup, 5000);
  editable.focus();
  if (editable.tagName === "INPUT" || editable.tagName === "TEXTAREA") {
    editable.setSelectionRange(0, editable.value.length);
  } else {
    var selection = editorSelection();
    var range = document.createRange();
    range.selectNodeContents(editable);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  var kind = host.matches(".DraftEditor-root") ? "draftjs" : "editable";
  return "ok_" + kind + ":" + token;
})();
