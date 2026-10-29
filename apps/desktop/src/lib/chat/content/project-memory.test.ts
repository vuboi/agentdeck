import assert from 'node:assert/strict';
import test from 'node:test';
import { MEMORY_PROMPT_CHARS, memoryHint, memoryPath } from './project-memory.ts';

test('the memory file sits in .agentdeck of the project, written with forward slashes', () => {
  assert.equal(memoryPath('E:\\WORK\\app\\'), 'E:/WORK/app/.agentdeck/memory.md');
  assert.equal(memoryPath('/home/me/app'), '/home/me/app/.agentdeck/memory.md');
});

test('no note when memory is off or the chat has no project', () => {
  assert.equal(memoryHint(false, '/p', 'x'), '');
  assert.equal(memoryHint(true, null, 'x'), '');
});

test('the note carries the memory and asks the agent to add what it learns', () => {
  const note = memoryHint(true, '/p', '  - uses pnpm\n');
  assert.match(note, /\/p\/\.agentdeck\/memory\.md/);
  assert.match(note, /- uses pnpm/);
  assert.match(note, /add one short bullet/);
  assert.match(memoryHint(true, '/p', ''), /empty so far/);
});

test('a long memory is cut in the prompt and says where the rest is', () => {
  const note = memoryHint(true, '/p', 'a'.repeat(MEMORY_PROMPT_CHARS + 500));
  assert.match(note, /cut: open the file/);
  assert.ok(note.length < MEMORY_PROMPT_CHARS + 600);
});
