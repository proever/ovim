import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

const source = readFileSync(new URL('../src-tauri/src/nvim_edit/browser_scripting/js/prepare_bmux_input.js', import.meta.url), 'utf8').replace('{{TARGET_ELEMENT_ID}}', 'test-editor');

function prepare(tagName = 'DIV') {
  let timeout;
  let removed = false;
  let collapsed = false;
  let style;
  const editable = {
    tagName, value: "text", selectionEnd: 4,
    focus() {}, contains: node => node === editable,
    setSelectionRange: (start, end) => { collapsed = start === 4 && end === 4; },
  };
  const selection = { rangeCount: 1, anchorNode: editable, focusNode: editable,
    removeAllRanges() {}, addRange() {}, collapseToEnd: () => { collapsed = true; } };
  const host = { matches: selector => selector === '.DraftEditor-root', querySelector: () => editable };
  const result = runInNewContext(source, {
    document: { createRange: () => ({ selectNodeContents() {} }), querySelector: () => host, createElement: () => ({ remove: () => { removed = true; } }),
      head: { appendChild: value => { style = value; } } },
    window: { getSelection: () => selection },
    setTimeout: callback => { timeout = callback; return 1; }, clearTimeout() {},
  });
  return { host, selection, result, timeout: () => timeout(), style: () => style,
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
