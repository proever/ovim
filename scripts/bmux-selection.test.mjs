import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

const source = readFileSync(new URL('../src-tauri/src/nvim_edit/browser_scripting/js/prepare_bmux_input.js', import.meta.url), 'utf8').replace('{{TARGET_ELEMENT_ID}}', 'test-editor');

function prepare(tagName = 'DIV', shadowDepth = 0) {
  let timeout;
  let removed = false;
  let collapsed = false;
  let style;
  let document;
  const root = { appendChild: value => { style = value; } };
  const editable = {
    tagName, value: "text", selectionEnd: 4,
    getRootNode: () => shadowDepth ? root : document,
    focus() {}, contains: node => node === editable,
    setSelectionRange: (start, end) => { collapsed = start === 4 && end === 4; },
  };
  const selection = { rangeCount: 1, anchorNode: editable, focusNode: editable,
    removeAllRanges() {}, addRange() {}, collapseToEnd: () => { collapsed = true; } };
  const host = { matches: selector => selector === '.DraftEditor-root', querySelector: () => editable };
  let lookupRoot = { querySelector: () => host, querySelectorAll: () => [] };
  for (let i = 0; i < shadowDepth; i++) {
    const child = lookupRoot;
    lookupRoot = { querySelector: () => null, querySelectorAll: () => [{ shadowRoot: child }] };
  }
  document = { createRange: () => ({ selectNodeContents() {} }), ...lookupRoot, createElement: () => ({ remove: () => { removed = true; } }),
      head: { appendChild: value => { style = value; } } };
  const window = { getSelection: () => selection };
  const result = runInNewContext(source, {
    document, window,
    setTimeout: callback => { timeout = callback; return 1; }, clearTimeout() {},
  });
  return { host, window, selection, result, timeout: () => timeout(), style: () => style,
    removed: () => removed, collapsed: () => collapsed };
}

for (const tag of ['DIV', 'TEXTAREA']) {
  test(`restores selection after replacement or failure (${tag})`, () => {
    const fixture = prepare(tag);
    assert.equal(fixture.result, 'ok_draftjs:test-editor');
    assert.match(fixture.style().textContent, /data-ovim-editor="test-editor"/);
    assert.match(fixture.style().textContent, /background: transparent !important/);
    assert.equal(fixture.collapsed(), false);
    fixture.host.__ovimInputCleanup();
    assert.equal(fixture.collapsed(), true);
    assert.equal(fixture.removed(), true);
    assert.equal(fixture.host.__ovimInputCleanup, undefined);
  });
}

test('timeout restores styling if Ovim stops before cleanup', () => {
  const fixture = prepare();
  fixture.timeout();
  assert.equal(fixture.removed(), true);
  assert.equal(fixture.host.__ovimInputCleanup, undefined);
});

test('cleanup does not collapse a selection in another field', () => {
  const fixture = prepare();
  fixture.selection.anchorNode = {};
  fixture.selection.focusNode = {};
  fixture.host.__ovimInputCleanup();
  assert.equal(fixture.collapsed(), false);
  assert.equal(fixture.removed(), true);
});


test('finds a cached Reddit chat textarea through nested shadow roots', () => {
  const fixture = prepare('TEXTAREA', 4);
  assert.equal(fixture.result, 'ok_draftjs:test-editor');
  assert.match(fixture.style().textContent, /background: transparent !important/);
  assert.equal(typeof fixture.window.__ovimInputCleanups['test-editor'], 'function');
  fixture.window.__ovimInputCleanups['test-editor']();
  assert.equal(fixture.collapsed(), true);
  assert.equal(fixture.removed(), true);
  assert.equal(fixture.window.__ovimInputCleanups['test-editor'], undefined);
});
