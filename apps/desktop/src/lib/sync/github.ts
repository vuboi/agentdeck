import { plannedProvider } from './provider';

// GitHub: the archive as a private repository. Versioned, diffable, free, and readable without AgentDeck.
export const github = plannedProvider({
  id: 'github',
  label: 'GitHub repository',
  icon: 'lucideGithub',
  summary: 'A private repo (default `agentdeck-history`) with one JSONL file per chat per month. Full history, diffs, works across devices.',
  fields: [
    { key: 'repo', label: 'Repository', placeholder: 'owner/agentdeck-history', hint: 'Created private when it does not exist.' },
    { key: 'branch', label: 'Branch', placeholder: 'main' },
  ],
  todo: [
    'Layout: `sessions/<chatId>/meta.json` (ArchiveSession) and `sessions/<chatId>/<yyyy-mm>.jsonl` (ArchiveMessage, one per line, sorted by `at`). Append-only lines keep git diffs small and merges trivial.',
    'Transport: a Rust command (`sync_git`) using the `git` CLI in a local clone at `~/.agentdeck/sync/github` (clone, `pull --rebase`, write files, commit "sync: N messages", push). Chosen over the REST Git Data API because it handles conflicts and big batches.',
    'Auth: GitHub device flow (no client secret, works in a desktop app) or a fine-grained PAT limited to this one repo. The token is stored in the OS keyring (Rust `keyring` crate), never in IndexedDB, never in the clone URL that ends up in `.git/config`: pass it per command through a credential helper.',
    'push(batch): write the batch into the clone, commit, push. On non-fast-forward: pull --rebase and retry once. The pull cursor is the last `savedAt` seen, kept by archive.ts.',
    'pull(since): `git pull --rebase`, read changed files since the cursor (`git diff --name-only <last>..HEAD`), return them as an ArchiveBatch. importArchive keeps the newer `savedAt` per record.',
    'Add `device` (a random id made once) to ArchiveMessage so two machines writing the same chat can be told apart; it is also the tie-break when `savedAt` is equal.',
    'Privacy: chats contain code and sometimes secrets. Show a warning before the first push, add an optional redaction pass (API keys, tokens, `.env` lines), and an opt-in encryption step (`age`) so the repo can sit on a public host.',
    'Limits: GitHub warns at 50 MB per file and 100 MB hard: roll a new month file early. Rate limit is not a concern with git over HTTPS.',
    'UI: fields above, "Test connection", "Sync now", last push time and counts from `pushArchive`/`pullArchive`, and an error line.',
  ],
});
