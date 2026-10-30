import assert from 'node:assert/strict';
import test from 'node:test';
import { LOADER, loaderFor } from './loaders.ts';

test('maps chat work to the Matrix Trace family and focused task loaders', () => {
  assert.equal(LOADER.thinking, 'matrix-grid');
  assert.equal(LOADER.action, 'merge');
  assert.equal(LOADER.check, 'scan');
  assert.equal(LOADER.background, 'matrix-orbit');
  assert.equal(LOADER.queued, 'lift');
  assert.equal(LOADER.command, LOADER.thinking);
  assert.equal(LOADER.writing, LOADER.action);
});

test('selects the matching loader for a running step', () => {
  assert.equal(loaderFor('Agent  Search the repository'), LOADER.background);
  assert.equal(loaderFor('Edit  apps/desktop/src/lib/loaders.ts'), LOADER.action);
  assert.equal(loaderFor('Read  apps/desktop/src/lib/loaders.ts'), LOADER.check);
  assert.equal(loaderFor('Bash  pnpm check'), LOADER.command);
});
