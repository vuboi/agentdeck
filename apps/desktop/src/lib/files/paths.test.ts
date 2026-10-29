import assert from 'node:assert/strict';
import test from 'node:test';
import { displayPath, fileRootOf, relativeTo, resolveInProject, withinRoot } from './paths.ts';

test('resolves project-relative paths with either separator on every host', () => {
  assert.equal(resolveInProject('apps/desktop/src/main.ts', 'C:\\work\\agentdeck'), 'C:\\work\\agentdeck\\apps\\desktop\\src\\main.ts');
  assert.equal(resolveInProject('/apps/desktop/src/main.ts', 'C:\\work\\agentdeck'), 'C:\\work\\agentdeck\\apps\\desktop\\src\\main.ts');
  assert.equal(resolveInProject('src\\main.ts', '/work/agentdeck'), '/work/agentdeck/src/main.ts');
});

test('keeps real roots and home paths outside the project join', () => {
  assert.equal(resolveInProject('D:/shared/file.txt', 'C:\\work\\agentdeck'), 'D:/shared/file.txt');
  assert.equal(resolveInProject('/tmp/file.txt', '/work/agentdeck'), '/tmp/file.txt');
  assert.equal(resolveInProject('~/Documents/file.txt', 'C:\\work\\agentdeck'), '~/Documents/file.txt');
  assert.equal(fileRootOf('~/Documents/file.txt', 'C:\\work\\agentdeck'), '~/Documents');
});

test('shows project files relative to the project while keeping outside paths compact', () => {
  assert.equal(displayPath('C:\\work\\agentdeck', 'c:/WORK/agentdeck/apps/desktop/src/main.ts'), 'apps/desktop/src/main.ts');
  assert.equal(displayPath('/work/agentdeck', '/tmp/output/file.txt'), 'file.txt');
  assert.equal(displayPath('', 'apps/desktop/src/main.ts'), 'apps/desktop/src/main.ts');
  assert.equal(relativeTo('C:\\work\\agentdeck', 'C:/work/agentdeck/apps/desktop/src/main.ts'), 'apps/desktop/src/main.ts');
  assert.equal(withinRoot('C:\\work\\agentdeck', 'C:/work/agentdeck-old/src/main.ts'), null);
});
