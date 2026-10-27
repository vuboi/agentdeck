import { test } from 'node:test';
import assert from 'node:assert/strict';
import { baseModel, creditModel } from './credit-model.ts';

test('a credit id names the model the CLI runs', () => {
  assert.equal(creditModel('credit:claude-opus-5-5'), 'claude-opus-5-5');
  assert.equal(creditModel('claude-opus-5-5'), null);
  assert.equal(creditModel('credit:'), null);
  assert.equal(creditModel(null), null);
  assert.equal(baseModel('credit:claude-haiku-5-5'), 'claude-haiku-5-5');
  assert.equal(baseModel('gpt-6-sol'), 'gpt-6-sol');
});
