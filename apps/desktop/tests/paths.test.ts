// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --test tests/paths.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dirOf, fileRootOf, isAbsolutePath, joinPath, nameOf, relativeTo, resolveInProject, samePath, withinRoot } from '../src/lib/files/paths.ts';

test('absolute paths: drive, UNC and POSIX', () => {
  assert.ok(isAbsolutePath('C:\\a\\b'));
  assert.ok(isAbsolutePath('e:/a/b'));
  assert.ok(isAbsolutePath('\\\\host\\share\\x'));
  assert.ok(isAbsolutePath('/home/me/a'));
  assert.ok(!isAbsolutePath('src/a.ts'));
  assert.ok(!isAbsolutePath('./a.ts'));
});

test('the same place, whatever the separators, case or trailing slash', () => {
  assert.ok(samePath('E:\\Work\\App\\', 'e:/work/app'));
  assert.ok(!samePath('E:\\work\\app', 'E:\\work\\app2'));
  // POSIX paths are case sensitive.
  assert.ok(!samePath('/home/Me', '/home/me'));
  assert.ok(samePath('/home/me/', '/home/me'));
});

test('join keeps the separator style of the base and never doubles it', () => {
  assert.equal(joinPath('E:\\w\\app', 'src/a.ts'), 'E:\\w\\app\\src\\a.ts');
  assert.equal(joinPath('E:\\w\\app\\', '/src//a.ts'), 'E:\\w\\app\\src\\a.ts');
  assert.equal(joinPath('/home/me', './src/a.ts'), '/home/me/src/a.ts');
  assert.equal(joinPath('/home/me', 'C:\\x\\y'), 'C:\\x\\y');
});

test('inside a root: relative with slashes, null outside or climbing out', () => {
  assert.equal(withinRoot('E:\\w\\app', 'E:\\w\\app\\src\\a.ts'), 'src/a.ts');
  assert.equal(withinRoot('E:\\w\\app', 'e:/W/APP/src/a.ts'), 'src/a.ts');
  assert.equal(withinRoot('E:\\w\\app', 'src/a.ts'), 'src/a.ts');
  assert.equal(withinRoot('E:\\w\\app', 'E:\\w\\other\\a.ts'), null);
  assert.equal(withinRoot('E:\\w\\app', 'E:\\w\\app'), null);
  assert.equal(withinRoot('E:\\w\\app', '../x.ts'), null);
  assert.equal(withinRoot('/home/me/app', '/home/me/app/a.ts'), 'a.ts');
  assert.equal(withinRoot('/home/me/app', '/home/me/application/a.ts'), null);
});

test('relativeTo shows a file outside the folder with its full path', () => {
  assert.equal(relativeTo('E:\\w\\app', 'E:\\w\\app\\src\\a.ts'), 'src/a.ts');
  assert.equal(relativeTo('E:\\w\\app', 'E:\\w\\other\\a.ts'), 'E:/w/other/a.ts');
});

test('a project-relative path resolves under the project, an absolute one stays', () => {
  assert.equal(resolveInProject('src/a.ts', 'E:\\w\\app'), 'E:\\w\\app\\src\\a.ts');
  assert.equal(resolveInProject('C:\\x\\a.ts', 'E:\\w\\app'), 'C:\\x\\a.ts');
  assert.equal(resolveInProject('src/a.ts', ''), 'src/a.ts');
});

test('the folder a file is read from: the project when it is inside it', () => {
  assert.equal(fileRootOf('E:\\w\\app\\src\\a.ts', 'E:\\w\\app'), 'E:\\w\\app');
  assert.equal(fileRootOf('E:\\w\\other\\a.ts', 'E:\\w\\app'), 'E:\\w\\other');
  assert.equal(fileRootOf('E:\\w\\other\\a.ts', ''), 'E:\\w\\other');
});

test('folder and name of a path', () => {
  assert.equal(dirOf('E:\\w\\app\\a.ts'), 'E:\\w\\app');
  assert.equal(nameOf('E:\\w\\app\\a.ts'), 'a.ts');
  assert.equal(nameOf('/home/me/a.ts'), 'a.ts');
});
