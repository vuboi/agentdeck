import { test } from 'node:test';
import assert from 'node:assert/strict';
import { jsonInText } from './json-text.ts';

test('a text without JSON is left as it is', () => {
  assert.equal(jsonInText('turn failed: usage limit'), 'turn failed: usage limit');
  assert.equal(jsonInText('{}'), '{}');
  assert.equal(jsonInText('const a = [];'), 'const a = [];');
});

test('JSON after a message is printed indented, the message kept', () => {
  assert.equal(jsonInText('400 Bad Request: {"success":false,"messages":[]}'), '400 Bad Request: {\n  "success": false,\n  "messages": []\n}');
});

test('a JSON string inside JSON is read again', () => {
  const text = '{"detail":"{\\"code\\":400,\\"ok\\":false}"}';
  assert.equal(jsonInText(text), '{\n  "detail": {\n    "code": 400,\n    "ok": false\n  }\n}');
});

test('JSON embedded in a message is read, the rest of the message kept', () => {
  const text = '{"message":"AiError: {\\"error\\":{\\"code\\":400}} (id-1)"}';
  assert.equal(jsonInText(text), '{\n  "message": AiError: {\n    "error": {\n      "code": 400\n    }\n  } (id-1)\n}');
});

test('brackets inside strings do not close the JSON early', () => {
  assert.equal(jsonInText('x {"a":"}{"}'), 'x {\n  "a": "}{"\n}');
});
