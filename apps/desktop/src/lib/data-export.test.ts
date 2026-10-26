import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildExport, mergeProviders, parseExport } from './data-export.ts';

const stores = { prefs: [['providers.custom', [{ id: 'p1', name: 'Groq', apiKey: 'gsk_secret' }]], ['sidebar.view', 'tree']] as [string, unknown][] };

test('an export leaves the API keys out unless asked', () => {
  const plain = buildExport(stores, {}, false);
  assert.equal((plain.stores.prefs[0][1] as { apiKey: string }[])[0].apiKey, '');
  assert.equal(plain.stores.prefs[1][1], 'tree');
  const keyed = buildExport(stores, {}, true);
  assert.equal((keyed.stores.prefs[0][1] as { apiKey: string }[])[0].apiKey, 'gsk_secret');
});

test('an export reads back, and other files are refused', () => {
  const parsed = parseExport(JSON.stringify(buildExport(stores, { 'agentdeck.theme': '{}' }, false)));
  assert.ok(parsed.ok && parsed.file.local['agentdeck.theme'] === '{}');
  assert.equal(parseExport('nope').ok, false);
  assert.equal(parseExport('{"format":"other"}').ok, false);
  assert.equal(parseExport(JSON.stringify({ ...buildExport(stores, {}, false), version: 99 })).ok, false);
});

test('importing without keys keeps the keys saved here', () => {
  const merged = mergeProviders([{ id: 'p1', apiKey: '' }, { id: 'p2', apiKey: '' }], [{ id: 'p1', apiKey: 'kept' }]) as { apiKey: string }[];
  assert.deepEqual(merged.map((item) => item.apiKey), ['kept', '']);
});
