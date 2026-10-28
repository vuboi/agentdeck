import assert from 'node:assert/strict';
import test from 'node:test';
import { pathsAsNames } from './path-links.ts';
import { isVietnamese, sentences, speakable } from './speech.ts';

test('code, links, addresses and symbols are not read', () => {
  const text = speakable('# Tiêu đề\n\nĐây là **đậm** và [liên kết](https://x.com/a), xem https://example.com nhé 👋\n\n```ts\nconst a = 1;\n```\n\n- mục một\n- mục hai\n\nChạy `pnpm check(x)` hoặc `lệnh`.');
  assert.equal(text, 'Tiêu đề\nĐây là đậm và liên kết, xem nhé\nmục một\nmục hai\nChạy hoặc lệnh.');
});

test('a file path is said as its name', () => {
  assert.equal(speakable('Mở C:\\Users\\me\\báo cáo.txt rồi sửa /home/ci/log.txt nhé', pathsAsNames), 'Mở báo cáo.txt rồi sửa log.txt nhé');
});

test('a table reads as words', () => {
  assert.equal(speakable('| a | b |\n|---|---|\n| 1 | 2 |'), ', a , b ,\n, 1 , 2 ,');
});

test('text is cut at sentence ends, and long ones at commas', () => {
  assert.deepEqual(sentences('Một câu. Hai câu! Ba câu?\nDòng mới'), ['Một câu. Hai câu! Ba câu?', 'Dòng mới']);
  const long = `${'từ một hai ba, '.repeat(30)}hết.`;
  const parts = sentences(long, 100);
  assert.ok(parts.length > 3);
  assert.ok(parts.every((part) => part.length <= 100), parts.map((part) => part.length).join(','));
  assert.equal(parts.join(' ').replace(/\s+/g, ' ').trim(), long.replace(/\s+/g, ' ').trim());
});

test('Vietnamese is told from English by its marks', () => {
  assert.equal(isVietnamese('Xin chào bạn'), true);
  assert.equal(isVietnamese('Hello there, how are you?'), false);
  assert.equal(isVietnamese('Chào'), false);
});
