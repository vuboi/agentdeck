// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --experimental-strip-types --test tests/handback.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { HANDBACK_HEAD, isHandbackPrompt } from '../src/lib/handback.ts';

const body = `${HANDBACK_HEAD}\n\n### Claude · Opus\nDone.\n\nUsing their answers, finish the user's request: fix it`;

test('a plain hand-back prompt is recognised', () => {
  assert.equal(isHandbackPrompt(body), true);
  assert.equal(isHandbackPrompt(`\n  ${body}`), true);
});

test('a hand-back behind the digest of missed turns is recognised', () => {
  const digest = 'Earlier in this chat, other agents answered these turns (you were not part of them):\n\nUser: hi\n\nClaude: hello\n\n---\nNow the user says:\n';
  assert.equal(isHandbackPrompt(digest + body), true);
});

test('a hand-back behind the AgentDeck hint (and a digest) is recognised', () => {
  const hint = '[AgentDeck] You run inside AgentDeck. Use the tools.\n\n';
  const digest = 'Earlier in this chat, other agents answered these turns (you were not part of them):\n\nUser: hi\n\n---\nNow the user says:\n';
  assert.equal(isHandbackPrompt(hint + body), true);
  assert.equal(isHandbackPrompt(hint + digest + body), true);
});

test('ordinary prompts are not hand-backs', () => {
  assert.equal(isHandbackPrompt('fix the login bug'), false);
  assert.equal(isHandbackPrompt(''), false);
  assert.equal(isHandbackPrompt('↩ Claude answered'), false);
  const digest = 'Earlier in this chat, other agents answered these turns (you were not part of them):\n\nUser: hi\n\n---\nNow the user says:\n';
  assert.equal(isHandbackPrompt(`${digest}what did they say?`), false);
  assert.equal(isHandbackPrompt(`Please explain: ${HANDBACK_HEAD}`), false);
  // a digest that never ends is not read as anything
  assert.equal(isHandbackPrompt('Earlier in this chat, other agents answered these turns'), false);
});
