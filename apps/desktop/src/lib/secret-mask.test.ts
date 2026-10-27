import { test } from 'node:test';
import assert from 'node:assert/strict';
import { maskSecret } from './secret-mask.ts';

test('a saved key shows only its ends', () => {
  assert.equal(maskSecret('sk-ant-api03-abcdefghijklmnop1234'), 'sk-ant…••••••••1234');
  assert.equal(maskSecret('short-key'), '••••••••');
  assert.ok(!maskSecret('sk-or-v1-0123456789abcdef9999').includes('0123456789'));
});
