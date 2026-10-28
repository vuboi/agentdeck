// The speaker button under an answer: reads it aloud. English sentences go to the system's own voice (instant, nothing
// to download); Vietnamese ones to Piper (tts.rs), which is downloaded the first time. Sentences are made one ahead of
// the one being played, so the sound starts at once and does not stop between them.
import { inTauri } from './history';
import { pathsAsNames } from './path-links';
import { isVietnamese, sentences, speakable } from './speech';
import { replyStyle } from './reply-style.svelte';
import { toast } from './toast';

export const speech = $state({ id: '', phase: 'idle' as 'idle' | 'installing' | 'loading' | 'playing', progress: 0 });

let token = 0;
let audio: HTMLAudioElement | null = null;

const stopSystem = () => {
  if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel();
};

/** Stops what is being read. */
export function stopSpeaking() {
  token++;
  audio?.pause();
  audio = null;
  stopSystem();
  speech.id = '';
  speech.phase = 'idle';
  speech.progress = 0;
}

async function invoke<T>(command: string, args?: Record<string, unknown>) {
  const { invoke: call } = await import('@tauri-apps/api/core');
  return call<T>(command, args);
}

/** Makes Piper and the voice available: downloads them once, with the progress in `speech.progress` (0 to 1). */
async function ensureVoice(mine: number) {
  const status = await invoke<{ installed: boolean; supported: boolean }>('tts_status', { voice: replyStyle.voice });
  if (status.installed) return;
  if (!status.supported) throw new Error('The Vietnamese voice is not available on this system');
  speech.phase = 'installing';
  const { listen } = await import('@tauri-apps/api/event');
  // Two downloads (the program, then the voice): the bar covers both by their sizes (about 22 MB and 63 MB).
  const share: Record<string, [number, number]> = { program: [0, 0.26], voice: [0.26, 1] };
  const off = await listen<{ stage: string; done: number; total: number }>('tts-progress', (event) => {
    if (mine !== token) return;
    const [from, to] = share[event.payload.stage] ?? [0, 1];
    speech.progress = from + (to - from) * (event.payload.total ? Math.min(1, event.payload.done / event.payload.total) : 0);
  });
  try {
    await invoke('tts_install', { voice: replyStyle.voice });
  } finally {
    off();
  }
}

function playUrl(url: string, mine: number): Promise<void> {
  return new Promise((resolve) => {
    const element = new Audio(url);
    audio = element;
    element.onended = element.onerror = () => resolve();
    if (mine !== token) return resolve();
    void element.play().catch(() => resolve());
  });
}

function playSystem(text: string, mine: number): Promise<void> {
  return new Promise((resolve) => {
    if (typeof speechSynthesis === 'undefined' || mine !== token) return resolve();
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = 'en-US';
    utterance.onend = utterance.onerror = () => resolve();
    speechSynthesis.speak(utterance);
  });
}

/** Reads `markdown` aloud, or stops if this one is already being read. `id` tells the buttons which message it is. */
export async function toggleSpeak(id: string, markdown: string) {
  const wasThis = speech.id === id && speech.phase !== 'idle';
  stopSpeaking();
  if (wasThis) return;
  // One voice for the whole answer: Piper when it has Vietnamese in it (English words inside are read by that same voice),
  // the system's English voice when it is all English.
  const plain = speakable(markdown, pathsAsNames);
  const vi = isVietnamese(plain);
  const parts = sentences(plain).map((text) => ({ text, vi }));
  if (!parts.length) return toast.error('Nothing to read');
  const mine = ++token;
  speech.id = id;
  speech.phase = 'loading';
  speech.progress = 0;
  try {
    const needsPiper = parts.some((part) => part.vi);
    if (needsPiper) {
      if (!inTauri()) throw new Error('Reading Vietnamese aloud needs the desktop app');
      await ensureVoice(mine);
    }
    if (mine !== token) return;
    const make = (part: { text: string; vi: boolean }): Promise<string | null> => (part.vi ? invoke<string>('tts_speak', { text: part.text, speed: 1, voice: replyStyle.voice }) : Promise.resolve(null));
    let next: Promise<string | null> = make(parts[0]);
    for (let index = 0; index < parts.length; index++) {
      const current = next;
      // The next sentence is being made while this one plays.
      if (index + 1 < parts.length) {
        next = make(parts[index + 1]);
        next.catch(() => {});
      }
      const url = await current;
      if (mine !== token) return;
      speech.phase = 'playing';
      if (url) await playUrl(url, mine);
      else await playSystem(parts[index].text, mine);
      if (mine !== token) return;
    }
  } catch (error) {
    if (mine === token) toast.error(error instanceof Error ? error.message : String(error));
  } finally {
    if (mine === token) stopSpeaking();
  }
}
