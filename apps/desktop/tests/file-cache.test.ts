// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --experimental-strip-types --test tests/file-cache.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_CACHED_TEXT, Recent, diffCache, fileKey, rememberText, textCache } from '../src/lib/files/file-cache.ts';

test('the same file has one key, whatever the slashes or the drive letter case', () => {
  assert.equal(fileKey('E:\\WORK\\proj', 'E:\\WORK\\proj/apps/a.ts'), fileKey('e:/work/proj', 'e:/work/proj/apps/a.ts'));
  assert.notEqual(fileKey('E:/proj', 'E:/proj/a.ts'), fileKey('E:/proj', 'E:/proj/b.ts'));
  assert.notEqual(fileKey('E:/one', 'E:/one/a.ts'), fileKey('E:/two', 'E:/one/a.ts'));
});

test('the entry used longest ago is the one forgotten', () => {
  const recent = new Recent<number>(2);
  recent.set('a', 1);
  recent.set('b', 2);
  recent.get('a'); // a is now newer than b
  recent.set('c', 3);
  assert.equal(recent.get('b'), undefined);
  assert.equal(recent.get('a'), 1);
  assert.equal(recent.get('c'), 3);
  assert.equal(recent.size, 2);
});

test('a diff given back is the one stored, rows and raw text', () => {
  const key = fileKey('E:/proj', 'E:/proj/new.ts');
  const rows = [{ kind: 'hunk', text: '@@ -0,0 +1 @@' }, { kind: 'add', now: 1, text: 'x' }];
  diffCache.set(key, { diff: '@@ -0,0 +1 @@\n+x\n', rows });
  assert.equal(diffCache.get(key)?.rows, rows);
  assert.equal(diffCache.get(key)?.diff, '@@ -0,0 +1 @@\n+x\n');
  assert.equal(diffCache.get(fileKey('E:/proj', 'E:/proj/other.ts')), undefined);
});

test('a file too big for the editor is not held, and replaces what was held', () => {
  const key = fileKey('E:/proj', 'E:/proj/big.txt');
  rememberText(key, 'small');
  assert.equal(textCache.get(key), 'small');
  rememberText(key, 'x'.repeat(MAX_CACHED_TEXT + 1));
  assert.equal(textCache.get(key), undefined);
});

test('an empty text is kept (an empty file is a file)', () => {
  const key = fileKey('E:/proj', 'E:/proj/empty.txt');
  rememberText(key, '');
  assert.equal(textCache.get(key), '');
});
