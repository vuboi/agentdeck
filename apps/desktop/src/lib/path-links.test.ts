import assert from 'node:assert/strict';
import test from 'node:test';
import { fileUrl, findPaths, labelOf, matchPath, nativePath, normalizePath, pathStart, startsAt } from './path-links.ts';

/** The shortest guess of every path a sentence names. */
const paths = (text: string) => findPaths(text).map((span) => span.candidates.at(-1)!.path);
/** The longest guess. */
const longest = (text: string) => findPaths(text).map((span) => span.candidates[0].path);

test('Windows: drive, UNC, WSL, long path and file URI', () => {
  assert.deepEqual(paths('mở C:\\Users\\me\\file.txt rồi'), ['C:\\Users\\me\\file.txt']);
  assert.deepEqual(paths('mở C:/Users/me/file.txt rồi'), ['C:/Users/me/file.txt']);
  assert.deepEqual(paths('xem \\\\server\\share\\file.txt'), ['\\\\server\\share\\file.txt']);
  assert.deepEqual(paths('xem //server/share/file.txt'), ['//server/share/file.txt']);
  assert.deepEqual(paths('xem \\\\wsl$\\Ubuntu\\home\\me\\a.py'), ['\\\\wsl$\\Ubuntu\\home\\me\\a.py']);
  assert.deepEqual(paths('xem \\\\wsl.localhost\\Ubuntu\\home\\me\\a.py'), ['\\\\wsl.localhost\\Ubuntu\\home\\me\\a.py']);
  assert.deepEqual(paths('xem \\\\?\\C:\\very\\long\\path.txt'), ['\\\\?\\C:\\very\\long\\path.txt']);
  assert.deepEqual(paths('xem file:///C:/Users/me/file.txt'), ['C:/Users/me/file.txt']);
});

test('macOS: home, volumes, system folders, homebrew, tilde, file URI', () => {
  assert.deepEqual(paths('mở /Users/me/Documents/report.pdf'), ['/Users/me/Documents/report.pdf']);
  assert.deepEqual(paths('mở /Volumes/MyDisk/data/a.png'), ['/Volumes/MyDisk/data/a.png']);
  assert.deepEqual(paths('mở /Applications/Safari.app'), ['/Applications/Safari.app']);
  assert.deepEqual(paths('mở /Library/Preferences/x.plist'), ['/Library/Preferences/x.plist']);
  assert.deepEqual(paths('mở /System/Library/CoreServices'), ['/System/Library/CoreServices']);
  assert.deepEqual(paths('mở /private/var/log/system.log'), ['/private/var/log/system.log']);
  assert.deepEqual(paths('mở /opt/homebrew/bin/node'), ['/opt/homebrew/bin/node']);
  assert.deepEqual(paths('mở ~/Desktop/note.md'), ['~/Desktop/note.md']);
  assert.deepEqual(paths('mở file:///Users/me/Documents/report.pdf'), ['/Users/me/Documents/report.pdf']);
});

test('Linux: home, root, mounts, system folders, tilde, file URI', () => {
  assert.deepEqual(paths('sửa /home/me/project/app.py'), ['/home/me/project/app.py']);
  assert.deepEqual(paths('sửa /root/.bashrc'), ['/root/.bashrc']);
  assert.deepEqual(paths('xem /mnt/data/a.png và /media/usb/b.jpg'), ['/mnt/data/a.png', '/media/usb/b.jpg']);
  for (const path of ['/etc/hosts', '/var/log/app.log', '/usr/local/bin/x', '/opt/app/x', '/srv/www', '/tmp/x']) assert.deepEqual(paths(`xem ${path}`), [path]);
  assert.deepEqual(paths('sửa ~/project/app.py'), ['~/project/app.py']);
  assert.deepEqual(paths('sửa file:///home/me/project/app.py'), ['/home/me/project/app.py']);
});

test('three platforms in one sentence are all found, whatever this machine runs', () => {
  assert.deepEqual(paths('sửa /Users/me/app/src/main.ts và /home/ci/build/log.txt rồi C:\\build\\out.txt'), ['/Users/me/app/src/main.ts', '/home/ci/build/log.txt', 'C:\\build\\out.txt']);
});

test('addresses and ordinary slashes are not paths', () => {
  for (const text of [
    'https://example.com/etc/hosts', 'http://localhost:3000/api/users/1', 'ws://host/tmp/x', 'mailto:me@home.com/opt/x',
    'and/or', '24/7', 'TCP/IP', 'km/h', 'ngày 2024/10/28', 'tỉ lệ 1/2', 'đọc /etc', 'thư mục /tmp', 'a//b', 'src/lib/a.ts', './a.ts',
    '/unknown/folder/file.txt', 'C:', 'C:\\', 'use the www.example.com/opt/x link',
  ]) assert.deepEqual(paths(text), [], text);
});

test('a path stands alone: the character before it decides', () => {
  assert.deepEqual(paths('(/etc/hosts)'), ['/etc/hosts']);
  assert.deepEqual(paths('"/etc/hosts"'), ['/etc/hosts']);
  assert.deepEqual(paths('path=/etc/hosts'), ['/etc/hosts']);
  assert.deepEqual(paths('x/etc/hosts'), []);
  assert.equal(matchPath('/etc/hosts', 'a'), null);
  assert.ok(matchPath('/etc/hosts', ' '));
  assert.ok(matchPath('/etc/hosts', ''));
});

test('punctuation stuck to the end of a path is dropped, but kept as a guess in case the file has it', () => {
  const [span] = findPaths('xem /tmp/report.v2.');
  assert.deepEqual(span.candidates.map((candidate) => candidate.path), ['/tmp/report.v2.', '/tmp/report.v2']);
  assert.deepEqual(paths('xem (/tmp/a.txt), rồi'), ['/tmp/a.txt']);
  assert.deepEqual(paths('xem /tmp/a.txt: ok'), ['/tmp/a.txt']);
  assert.deepEqual(paths('thấy "/tmp/a.txt".'), ['/tmp/a.txt']);
});

test('a line suffix is kept apart from the path', () => {
  const [span] = findPaths('lỗi ở /home/me/app.ts:42:7 đó');
  assert.equal(span.candidates.at(-1)!.path, '/home/me/app.ts');
  assert.equal(span.candidates.at(-1)!.line, ':42:7');
  assert.equal(labelOf('/home/me/app.ts', ':42:7'), 'app.ts:42:7');
});

test('a path with spaces, Vietnamese letters or emoji is guessed at every length, longest first', () => {
  const [span] = findPaths('mở C:\\Users\\boi.vn\\My Documents\\báo cáo 📄.docx rồi nhé');
  const guesses = span.candidates.map((candidate) => candidate.path);
  assert.equal(guesses[0], 'C:\\Users\\boi.vn\\My Documents\\báo cáo 📄.docx');
  assert.ok(guesses.includes('C:\\Users\\boi.vn\\My'), 'and the short guess stays');
  // A path that already ends in an extension is not stretched over the words after it.
  assert.deepEqual(longest('sửa /Users/me/app/src/main.ts và /home/ci/build/log.txt'), ['/Users/me/app/src/main.ts', '/home/ci/build/log.txt']);
});

test('a guess that reaches the next path stops before it', () => {
  const spans = findPaths('so /Users/me/app với /home/ci/app');
  assert.equal(spans.length, 2);
  assert.deepEqual(spans[0].candidates.map((candidate) => candidate.path), ['/Users/me/app với', '/Users/me/app']);
});

test('file URIs are percent-decoded and `/C:/x` becomes `C:/x`', () => {
  assert.equal(normalizePath('file:///C:/Users/me/a%20b.txt'), 'C:/Users/me/a b.txt');
  assert.equal(normalizePath('file:///home/me/%C3%A1.txt'), '/home/me/á.txt');
  assert.equal(normalizePath('file://server/share/a.txt'), '//server/share/a.txt');
});

test('the label is the name of the file or folder', () => {
  assert.equal(labelOf('C:\\Users\\me\\divider-feature-prompt.md'), 'divider-feature-prompt.md');
  assert.equal(labelOf('/Applications/Safari.app'), 'Safari.app');
  assert.equal(labelOf('/home/me/project/'), 'project');
  assert.equal(labelOf('~/Desktop/note.md'), 'note.md');
  assert.equal(labelOf('\\\\server\\share\\x.txt'), 'x.txt');
  assert.equal(labelOf('C:\\'), 'C:\\');
});

test('file:// addresses percent-encode spaces and special characters', () => {
  assert.equal(fileUrl('C:\\Users\\me\\My Docs\\a#b.txt'), 'file:///C:/Users/me/My%20Docs/a%23b.txt');
  assert.equal(fileUrl('/Users/me/báo cáo.pdf'), 'file:///Users/me/b%C3%A1o%20c%C3%A1o.pdf');
  assert.equal(fileUrl('\\\\server\\share\\a b.txt'), 'file://server/share/a%20b.txt');
  assert.equal(fileUrl('file:///home/me/x.txt'), 'file:///home/me/x.txt');
});

test('a copied path is in the form of the system it runs on', () => {
  assert.equal(nativePath('C:/Users/me/a.txt', 'windows'), 'C:\\Users\\me\\a.txt');
  assert.equal(nativePath('//server/share/a.txt', 'windows'), '\\\\server\\share\\a.txt');
  assert.equal(nativePath('/Users/me/a.txt', 'windows'), '/Users/me/a.txt');
  assert.equal(nativePath('/Users/me/a.txt', 'mac'), '/Users/me/a.txt');
  assert.equal(nativePath('file:///home/me/a.txt', 'linux'), '/home/me/a.txt');
  assert.equal(nativePath('C:\\x\\a.txt', 'mac'), 'C:\\x\\a.txt');
});

test('pathStart finds the first place a path may begin, for the Markdown tokenizer', () => {
  assert.equal(pathStart('sửa /etc/hosts'), 4);
  assert.equal(pathStart('and/or 24/7'), -1);
  assert.equal(pathStart('https://x.com/etc/hosts'), -1);
  assert.equal(startsAt('C:\\x', 0), true);
  assert.equal(startsAt('C:', 0), false);
});

test('a long text is scanned in one quick pass', () => {
  const text = Array.from({ length: 2000 }, (_, index) => `dòng ${index} /home/me/p${index}/a.ts và and/or 24/7`).join('\n');
  const started = performance.now();
  const found = findPaths(text);
  const took = performance.now() - started;
  assert.equal(found.length, 2000);
  assert.ok(took < 800, `took ${took} ms`);
});
