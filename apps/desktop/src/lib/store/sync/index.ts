import { registerSyncTarget } from '../../chat/content/archive';
import { folder } from './folder';
import { gdrive } from './gdrive';
import { github } from './github';
import { memory } from './memory';
import { s3 } from './s3';
import type { SyncProvider } from './provider';

export type { SyncField, SyncProvider, SyncStatus } from './provider';
export { SyncNotImplemented } from './provider';

/**
 * Every sync service, in the order the Settings → Sync page lists them. Adding one: write `sync/<name>.ts`
 * (start from `plannedProvider`), add it here. Nothing else in the app changes.
 *
 * Cross-cutting TODO, done once for all providers rather than per service:
 *  - `device`: a random id made on first run, written into ArchiveMessage/ArchiveSession (archive.ts), so records
 *    from two machines can be told apart and folder/bucket prefixes are stable.
 *  - `sync/privacy.ts`: a warning before the first push, a redaction pass (keys, tokens, `.env` lines) and optional
 *    client-side encryption (`age`), applied to a batch before any provider sees it.
 *  - Secrets (tokens, keys) in the OS keyring through Rust commands; the webview never holds them.
 *  - A scheduler: push after each finished turn (debounced), pull on start-up and every few minutes, with a
 *    "last synced" time and error per provider in the status bar.
 *  - Import of the history that exists from before the archive (old chats read from the CLIs' own files).
 */
export const syncProviders: SyncProvider[] = [folder, github, gdrive, s3, memory];

for (const provider of syncProviders) registerSyncTarget(provider);

export const syncProvider = (id: string) => syncProviders.find((item) => item.id === id);
