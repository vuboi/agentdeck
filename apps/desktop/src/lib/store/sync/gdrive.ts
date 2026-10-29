import { plannedProvider } from './provider';

// Google Drive, in the hidden app-data folder: nothing shows up in the user's own Drive and the app cannot read
// any other file.
export const gdrive = plannedProvider({
  id: 'gdrive',
  label: 'Google Drive',
  icon: 'lucideCloud',
  summary: 'Hidden app folder in your Drive (scope `drive.appdata`). No repo to manage; good for a personal backup across devices.',
  fields: [{ key: 'account', label: 'Google account', placeholder: 'shown after sign-in', hint: 'Filled by the sign-in, not typed.' }],
  todo: [
    'Auth: OAuth 2.0 for installed apps with PKCE and a loopback redirect (`http://127.0.0.1:<random port>`): the Rust side opens a one-shot listener, the browser opens the consent page, the code comes back. Scope is only `https://www.googleapis.com/auth/drive.appdata`, which is not a sensitive scope, so no Google verification review is needed.',
    'Tokens: refresh token in the OS keyring (Rust `keyring` crate); the access token lives in memory. Never IndexedDB.',
    'Layout: one file per chat per month, `<chatId>-<yyyy-mm>.jsonl`, plus `index.json` (ArchiveSession list). Store `savedAt` and the device id in the file `appProperties` so a pull can filter without downloading everything.',
    'push(batch): group the batch by file, download the current file if it exists, merge by record id (newer `savedAt` wins), upload with the resumable upload API. Use the file `version`/ETag to detect a concurrent write and retry.',
    'pull(since): `changes.list` with a saved `startPageToken` (stored next to the push cursor) returns only what changed; download those files and return an ArchiveBatch.',
    'Rust command `sync_google` does the HTTP (the webview must not hold tokens); the frontend only sees counts and errors. `reqwest` is already a dependency.',
    'Quota and size: appdata counts against the user quota; the 5 TB/750 GB daily limits are far away. Chunk uploads over 5 MB.',
    'Same privacy options as GitHub (warning, redaction, optional encryption). Share the code for them in `sync/privacy.ts` when the first provider is built.',
    'UI: "Sign in with Google", the account email, "Sync now", "Disconnect" (revokes the token and clears the keyring).',
  ],
});
