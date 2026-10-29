import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clock, findArtifacts, kindOfName, pathsInText, resolveArtifactPath } from './artifacts.ts';

test('kinds by extension', () => {
  assert.equal(kindOfName('crying-sound.wav'), 'audio');
  assert.equal(kindOfName('demo.MP4'), 'video');
  assert.equal(kindOfName('a.png'), 'image');
  assert.equal(kindOfName('report.pdf'), 'document');
  assert.equal(kindOfName('main.rs'), null);
});

test('paths are found in prose, quotes and links, not in urls or code fences', () => {
  const text = [
    'Saved the sound to `E:\\work\\out\\crying sound.wav` and a copy at docs/user-guides/a.docx.',
    'See [the chart](images/chart.png) and ![x](./pic one.jpg).',
    'Remote https://example.com/x.png is not a file.',
    '```',
    'const f = "ignored.mp3"',
    '```',
  ].join('\n');
  const found = pathsInText(text);
  assert.ok(found.includes('E:\\work\\out\\crying sound.wav'));
  assert.ok(found.includes('docs/user-guides/a.docx'));
  assert.ok(found.includes('images/chart.png'));
  assert.ok(found.some((path) => path.endsWith('pic one.jpg')));
  assert.ok(!found.some((path) => path.includes('example.com')));
  assert.ok(!found.includes('ignored.mp3'));
});

test('relative paths resolve under the project and use its separator', () => {
  assert.equal(resolveArtifactPath('out/a.wav', 'E:\\work\\app'), 'E:\\work\\app\\out\\a.wav');
  assert.equal(resolveArtifactPath('./out/a.wav', '/home/me/app/'), '/home/me/app/out/a.wav');
  assert.equal(resolveArtifactPath('C:\\x\\a.wav', 'E:\\work'), 'C:\\x\\a.wav');
});

test('artifacts of a reply: words and Write calls, each once, newest last', () => {
  const found = findArtifacts(
    [
      { kind: 'command', command: 'Write E:\\work\\app\\gen\\draft.png' },
      { kind: 'response', text: 'Done: gen/draft.png and `gen/final.png`, plus sound.wav.' },
    ],
    'E:\\work\\app',
  );
  assert.deepEqual(found.map((item) => item.name), ['draft.png', 'final.png', 'sound.wav']);
  assert.equal(found[0].kind, 'image');
});

test('clock', () => {
  assert.equal(clock(5.4), '0:05');
  assert.equal(clock(723), '12:03');
  assert.equal(clock(NaN), '0:00');
});

test('a URL-style Windows path from a markdown link is the Windows path', () => {
  assert.equal(resolveArtifactPath('/C:/Users/me/out/crying-sound.wav', 'E:\repo'), 'C:/Users/me/out/crying-sound.wav');
  assert.ok(pathsInText('![Sound](/C:/Users/me/out/crying-sound.wav)').some((path) => path.endsWith('crying-sound.wav')));
});
