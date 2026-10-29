import { test } from 'node:test';
import assert from 'node:assert/strict';
import { needsContext } from './context-need.ts';

test('short messages that lean on the conversation need it', () => {
  for (const text of ['ok', 'continue', 'do it', 'tiếp đi', 'làm tiếp phần trên', 'sửa lại cho đúng', 'why did that fail?', 'như cũ nhé', 'undo it']) {
    assert.equal(needsContext(text), true, text);
  }
});

test('a message with only attachments says nothing of its own', () => {
  assert.equal(needsContext(''), true);
  assert.equal(needsContext('   '), true);
});

test('a message that names its subject stands alone, long or short', () => {
  for (const text of [
    'fix the off-by-one in src/lib/queue.ts line 40',
    'run pnpm build and report the errors',
    'Open https://example.com/pricing and list the plan names',
    'rename `loadStore` to `loadInstalled` everywhere',
    'add a retry to fetchCatalog_page when the request times out',
  ]) {
    assert.equal(needsContext(text), false, text);
  }
});

test('a long message with no anchor and no pointer is still fine alone', () => {
  assert.equal(needsContext('Write a short summary of how the update dialog downloads and installs a new version'), false);
});

test('a pointer plus a long request without a named subject needs the history', () => {
  assert.equal(needsContext('sửa cái đó theo cách như trên rồi chạy lại cho tôi xem kết quả nhé'), true);
  assert.equal(needsContext('Please apply the same change to the other pages and then run the checks again for me'), true);
});

test('words that only contain a pointer are not a pointer', () => {
  // "item" holds "it", "đóng" holds "đó".
  assert.equal(needsContext('Add an item list component with grouped rows and a footer'), false);
  assert.equal(needsContext('thêm nút đóng modal trong phần cài đặt của ứng dụng chính'), false);
});
