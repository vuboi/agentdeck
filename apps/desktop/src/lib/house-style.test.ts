import assert from 'node:assert/strict';
import test from 'node:test';
import { DEFAULT_HOUSE_STYLE, houseStyleHint } from './house-style.ts';

test('the convention is the default wording unless the user wrote their own', () => {
  assert.equal(houseStyleHint(true, ''), DEFAULT_HOUSE_STYLE);
  assert.equal(houseStyleHint(true, '   '), DEFAULT_HOUSE_STYLE);
  assert.equal(houseStyleHint(true, ' Be brief. '), 'Be brief.');
});

test('switched off, nothing is sent', () => {
  assert.equal(houseStyleHint(false, 'Be brief.'), '');
});

test('the default asks for file-name links and local pictures', () => {
  assert.match(DEFAULT_HOUSE_STYLE, /\[index\.html\]\(C:\/work\/site\/index\.html\)/);
  assert.match(DEFAULT_HOUSE_STYLE, /127\.0\.0\.1/);
});
