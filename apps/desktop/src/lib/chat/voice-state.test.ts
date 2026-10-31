import assert from 'node:assert/strict';
import test from 'node:test';
import { nextVoicePhase } from './voice-state.ts';

test('voice capture moves from recording through transcription to idle', () => {
    assert.equal(nextVoicePhase('idle', 'start'), 'recording');
    assert.equal(nextVoicePhase('recording', 'stop'), 'transcribing');
    assert.equal(nextVoicePhase('transcribing', 'finish'), 'idle');
});

test('cancel and failure always return the capture to idle', () => {
    assert.equal(nextVoicePhase('recording', 'cancel'), 'idle');
    assert.equal(nextVoicePhase('transcribing', 'cancel'), 'idle');
    assert.equal(nextVoicePhase('transcribing', 'fail'), 'idle');
});

test('irrelevant events do not corrupt the current phase', () => {
    assert.equal(nextVoicePhase('idle', 'stop'), 'idle');
    assert.equal(nextVoicePhase('recording', 'start'), 'recording');
    assert.equal(nextVoicePhase('transcribing', 'stop'), 'transcribing');
});
