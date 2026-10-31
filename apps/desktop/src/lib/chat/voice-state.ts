/** States shared by the voice capture UI and its transition tests. */
export type VoicePhase = 'idle' | 'recording' | 'transcribing';

/** Events that move one voice capture attempt through its lifecycle. */
export type VoiceEvent = 'start' | 'stop' | 'cancel' | 'finish' | 'fail';

/** Returns the phase after a voice capture event. */
export function nextVoicePhase(
    phase: VoicePhase,
    event: VoiceEvent,
): VoicePhase {
    if (event === 'cancel' || event === 'fail') return 'idle';
    if (phase === 'idle' && event === 'start') return 'recording';
    if (phase === 'recording' && event === 'stop') return 'transcribing';
    if (phase === 'transcribing' && event === 'finish') return 'idle';
    return phase;
}
