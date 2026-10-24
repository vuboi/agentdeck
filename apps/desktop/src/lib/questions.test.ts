import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatAnswers, parseQuestions, withoutOpenBlock } from './questions.ts';

const reply = [
  'The task does not say which pages get the button.',
  '```agentdeck-ask',
  '{"questions":[{"id":"scope","header":"Scope","question":"Which pages?","multi":true,"options":[{"label":"Report page (Recommended)","description":"only there"},"Every page"]}]}',
  '```',
].join('\n');

test('a question block becomes questions, the text around it stays', () => {
  const { questions, rest } = parseQuestions(reply);
  assert.equal(questions.length, 1);
  assert.equal(questions[0].multiSelect, true);
  assert.deepEqual(questions[0].options.map((option) => option.label), ['Report page (Recommended)', 'Every page']);
  assert.equal(rest, 'The task does not say which pages get the button.');
});

test('a broken block is left as text', () => {
  const text = '```questions\n{not json\n```';
  assert.deepEqual(parseQuestions(text), { questions: [], rest: text });
});

test('a block still streaming is hidden', () => {
  assert.equal(withoutOpenBlock('Why.\n```agentdeck-ask\n{"questions":['), 'Why.');
  assert.equal(withoutOpenBlock(reply), reply);
});

test('answers are one line per question', () => {
  const { questions } = parseQuestions(reply);
  assert.equal(formatAnswers(questions, [{ picked: ['Every page'], text: 'and the dashboard' }]), 'My answers:\n1. Which pages?\n   → Every page; and the dashboard');
  assert.match(formatAnswers(questions, []), /decide yourself/);
});
