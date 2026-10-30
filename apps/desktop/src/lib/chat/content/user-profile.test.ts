import test from 'node:test';
import assert from 'node:assert/strict';
import { emptyProfile, guessLanguage, observe, ownWords, profileHint } from './user-profile.ts';

test('own words drop the AgentDeck lead and inline context', () => {
  assert.equal(ownWords('[AgentDeck] Write replies in Vietnamese.\n\nlàm đi'), 'làm đi');
  assert.equal(ownWords('[AgentDeck] x\n\nThe user included this chat/project context inline:\n\nProject "a" is at E:/a\n\n---\nđọc đi'), 'đọc đi');
});

test('language guess by script, Vietnamese also without diacritics', () => {
  assert.equal(guessLanguage('sửa lỗi này nhé'), 'Vietnamese');
  assert.equal(guessLanguage('sua cai nay di nha'), 'Vietnamese');
  assert.equal(guessLanguage('please fix the build'), 'English');
  assert.equal(guessLanguage('これを直して'), 'Japanese');
});

test('no note until enough prompts, then language, address and tone', () => {
  let profile = emptyProfile();
  for (const prompt of ['tao bảo mày sửa đi haha', 'ok làm đi m', 'uk vậy đọc qua đi rồi trả lời tao', 'giao cho sub agent đi =))']) profile = observe(profile, prompt);
  assert.equal(profileHint(profile), '');
  profile = observe(profile, 'review lại kĩ nha kk');
  const note = profileHint(profile);
  assert.match(note, /Vietnamese/);
  assert.match(note, /tao\/mày/);
  assert.match(note, /humour/);
  assert.match(note, /casual/);
  assert.match(note, /sub-agents/);
});

test('an empty prompt changes nothing', () => {
  const profile = emptyProfile();
  assert.equal(observe(profile, '   '), profile);
});
