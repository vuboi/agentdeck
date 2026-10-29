import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatAgents, privateContentIn } from './delegate-task.ts';

test('light public work may go to a free tier', () => {
  assert.equal(privateContentIn('Write a polite email asking the landlord to fix the heater by Friday.'), null);
  assert.equal(privateContentIn('Search the web for the latest Svelte 5 release notes and summarise them.'), null);
  assert.equal(privateContentIn('Shorten this line:\n```ts\nconst total = items.reduce((sum, item) => sum + item.price, 0);\n```'), null);
});

test('secrets never reach a free tier', () => {
  assert.match(privateContentIn('Use key sk-proj-abcdefghijklmnopqrstuvwx to call the API') ?? '', /API key/);
  assert.match(privateContentIn('token ghp_abcdefghijklmnopqrstuvwxyz0123456789') ?? '', /API key/);
  assert.match(privateContentIn('-----BEGIN OPENSSH PRIVATE KEY-----\nabc') ?? '', /private key/);
  assert.match(privateContentIn('connect to postgres://admin:hunter22@db.internal/app') ?? '', /password/);
  assert.match(privateContentIn('DB_PASSWORD="hunter2222"') ?? '', /secret/);
  assert.match(privateContentIn('export OPENAI_API_KEY=abc') ?? '', /environment/);
  assert.match(privateContentIn('the API is at http://10.0.0.5:8080') ?? '', /IP/);
  assert.match(privateContentIn('see https://billing.internal/docs') ?? '', /host/);
});

test('a file of source code never reaches a free tier', () => {
  const file = Array.from({ length: 40 }, (_, i) => `let v${i} = ${i};`).join('\n');
  assert.match(privateContentIn(`Refactor this:\n\`\`\`ts\n${file}\n\`\`\``) ?? '', /lines of code/);
});

test('list_agents marks a free tier', () => {
  const text = formatAgents([{ id: 'custom-1', label: 'Groq', models: [{ value: 'llama', label: 'llama' }], restricted: true }]);
  assert.match(text, /FREE TIER/);
});
