import { test } from 'node:test';
import assert from 'node:assert/strict';
import { askAsStep, formatAnswers, withoutQuestions, isQuick, parseQuestions, withoutOpenBlock } from './questions.ts';

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

test('one single-choice question with up to three options is a quick card', () => {
  const ask = (count: number, multiSelect = false) => [{ question: 'Which?', multiSelect, options: Array.from({ length: count }, (_, index) => ({ label: `Option ${index + 1}` })) }];
  assert.equal(isQuick(ask(1)), true);
  assert.equal(isQuick(ask(3)), true);
  assert.equal(isQuick(ask(4)), false);
  assert.equal(isQuick(ask(0)), false);
  assert.equal(isQuick(ask(2, true)), false);
  assert.equal(isQuick([...ask(2), ...ask(2)]), false);
});

test('a required question left open is not a preference the agent may settle', () => {
  const [ordinary, required] = parseQuestions('```agentdeck-ask\n{"questions":[{"question":"Colour?","options":["Red"]},{"question":"Which database?","required":true,"options":["A","B"]}]}\n```').questions;
  assert.equal(ordinary.required, false);
  assert.equal(required.required, true);
  const text = formatAnswers([ordinary, required], [{ picked: [], text: '' }, { picked: [], text: '' }]);
  assert.match(text, /Colour\?\n {3}→ \(no preference, decide yourself\)/);
  assert.match(text, /Which database\?\n {3}→ \(not answered: this one is required, do not guess it\)/);
});

test('the questions of an answer become one step to open later', () => {
  const step = askAsStep('Which one?\n\n```agentdeck-ask\n{"questions":[{"question":"Add it everywhere?","multi":true,"options":[{"label":"Yes","description":"4 pages"},"No"]}]}\n```');
  assert.ok(step);
  assert.equal(step.command, 'AskUser  Add it everywhere?');
  assert.equal(step.output, '1. Add it everywhere? (pick any)\n   1. Yes: 4 pages\n   2. No');
  assert.match(step.id, /^ask-/);
  assert.equal(askAsStep('No questions here.'), null);
  // The same questions give the same id, so the step keeps its place and its open state.
  assert.equal(askAsStep('x\n```agentdeck-ask\n{"questions":[{"question":"Q","options":["a"]}]}\n```')?.id, askAsStep('y\n```agentdeck-ask\n{"questions":[{"question":"Q","options":["a"]}]}\n```')?.id);
});

test('the questions block never shows as text, whatever state it is in', () => {
  const block = '```agentdeck-ask\n{"questions":[{"question":"Q","options":["a","b"]}]}\n```';
  // Written so far, not closed.
  assert.equal(withoutQuestions('Why I ask.\n\n```agentdeck-ask\n{"questions":[{"quest'), 'Why I ask.');
  // Closed, with more text coming after it (the stream is not over).
  assert.equal(withoutQuestions(`Why I ask.\n\n${block}\n\nMore words`), 'Why I ask.\n\nMore words');
  assert.equal(withoutQuestions(`Why I ask.\n\n${block}`), 'Why I ask.');
  assert.equal(withoutQuestions('Plain answer.'), 'Plain answer.');
  // Not valid JSON: stays as text, so the user still reads what was asked.
  assert.match(withoutQuestions('```agentdeck-ask\nnot json\n```'), /not json/);
});

test('a block written without its fence still becomes questions', () => {
  const bare = 'Lỗi PHP chưa đủ thông tin. Bạn đang gặp tình huống nào?\n{"questions":[{"question":"Bạn gặp tình huống nào?","header":"Cần làm rõ","options":[{"label":"Dán thông báo lỗi"},{"label":"Nâng cấp PHP"}]}]}';
  const { questions, rest } = parseQuestions(bare);
  assert.equal(questions.length, 1);
  assert.equal(questions[0].options.length, 2);
  assert.equal(rest, 'Lỗi PHP chưa đủ thông tin. Bạn đang gặp tình huống nào?');
  assert.equal(withoutQuestions(bare), 'Lỗi PHP chưa đủ thông tin. Bạn đang gặp tình huống nào?');
});

test('other providers names for the same parts become one shape', () => {
  const other = 'Which one?\n{"questions":[{"prompt":"Pick","choices":[{"title":"Yes"},"No"],"multiple":true}]}';
  const { questions } = parseQuestions(other);
  assert.equal(questions[0].question, 'Pick');
  assert.equal(questions[0].multiSelect, true);
  assert.deepEqual(questions[0].options.map((option) => option.label), ['Yes', 'No']);
});

test('a bare object that is not valid JSON stays as text', () => {
  const broken = 'Hỏi nhé {"questions":[{"question":"?"';
  assert.equal(parseQuestions(broken).questions.length, 0);
  assert.equal(parseQuestions(broken).rest, broken);
});
