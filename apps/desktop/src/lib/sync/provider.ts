import type { ArchiveBatch, SyncTarget } from '../archive';

// The sync layer follows the same idea as the agent layer (`crates/adapters/src/agent/mod.rs`, one `Provider`
// trait with `claude.rs`, `codex.rs`, `antigravity.rs` behind it): the app only talks to `SyncProvider`, and each
// service (GitHub, Google Drive, a folder, S3…) is one file in this folder that fills it in.
//
//   archive.ts      what is saved and in which format (`ArchiveBatch`), the registry, push/pull cursors
//   sync/provider.ts  this file: the contract and the `plannedProvider` stub
//   sync/<service>.ts one service; `plannedProvider({...})` until its TODO list is done
//   sync/index.ts     the list the Settings → Sync page shows and the registration with archive.ts

export type SyncStatus = 'planned' | 'ready';

/** One thing the Settings form asks for. Secrets never go to IndexedDB: they belong in the OS keyring. */
export type SyncField = { key: string; label: string; placeholder?: string; secret?: boolean; hint?: string };

export interface SyncProvider extends SyncTarget {
  /** Icon name for the Settings card. */
  icon: string;
  status: SyncStatus;
  /** One line: where the data goes and why one would pick it. */
  summary: string;
  fields: SyncField[];
  /** What is left to build, in order. Shown on the Sync page, so the plan lives next to the code. */
  todo: string[];
  /** Hands the form values to the provider (secrets are read from the keyring by the provider itself). */
  configure(config: Record<string, string>): void;
  /** Reaches the service with the saved settings: signed in, folder exists, repo is writable. Throws when not. */
  test(): Promise<void>;
}

export class SyncNotImplemented extends Error {
  constructor(provider: string, what: string) {
    super(`${provider}: ${what} is not built yet`);
    this.name = 'SyncNotImplemented';
  }
}

type Planned = Pick<SyncProvider, 'id' | 'label' | 'icon' | 'summary' | 'fields' | 'todo'>;

/** A provider whose design is written down but whose code is not: every call says what is missing. */
export function plannedProvider(definition: Planned): SyncProvider {
  const missing = (what: string) => Promise.reject(new SyncNotImplemented(definition.label, what));
  return {
    ...definition,
    status: 'planned',
    configure: () => {},
    test: () => missing('connecting'),
    push: (_batch: ArchiveBatch) => missing('push'),
    pull: (_since: string | null) => missing('pull'),
  };
}
