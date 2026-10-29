import type { ArchiveBatch } from '../../chat/content/archive';
import type { SyncProvider } from './provider';

// A target that keeps everything in memory. It is the only one that works today: it proves the push and pull
// path (cursor, merge, `importArchive`) end to end, and it is what tests use instead of a real service. It is
// lost on reload, so it is never offered as a place to keep data.
const stored: ArchiveBatch[] = [];

export const memory: SyncProvider = {
  id: 'memory',
  label: 'Memory (test only)',
  icon: 'lucideFlaskConical',
  status: 'ready',
  summary: 'Keeps the archive in memory until the app closes. Used to test the sync path without any account.',
  fields: [],
  todo: [],
  configure: () => {},
  test: async () => {},
  push: async (batch) => {
    stored.push(structuredClone(batch));
  },
  pull: async (since) => {
    const newer = (savedAt: string) => since === null || savedAt > since;
    const messages = stored.flatMap((batch) => batch.messages).filter((item) => newer(item.savedAt));
    const sessions = stored.flatMap((batch) => batch.sessions).filter((item) => newer(item.savedAt));
    const to = [...messages, ...sessions].map((item) => item.savedAt).sort().at(-1) ?? since ?? new Date(0).toISOString();
    return { version: 1, from: since, to, sessions, messages };
  },
};

/** For tests: forget everything pushed. */
export const clearMemoryTarget = () => void (stored.length = 0);
