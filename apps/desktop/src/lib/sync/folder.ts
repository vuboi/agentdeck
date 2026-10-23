import { plannedProvider } from './provider';

// A plain folder. If it sits inside Dropbox, OneDrive, iCloud Drive, Syncthing or a NAS share, that tool does the
// syncing and AgentDeck needs no account of any service. The simplest provider and the one to build first.
export const folder = plannedProvider({
  id: 'folder',
  label: 'Folder (Dropbox, OneDrive, iCloud…)',
  icon: 'lucideFolderSync',
  summary: 'Writes the archive as JSONL files into a folder you pick. Another tool syncs the folder; no sign-in here.',
  fields: [{ key: 'path', label: 'Folder', placeholder: 'C:\\Users\\you\\Dropbox\\agentdeck', hint: 'Pick with the folder dialog.' }],
  todo: [
    'Folder picker: add `tauri-plugin-dialog` (and its capability) and a `sync_folder_pick` command; keep the chosen path in prefs (it is not a secret).',
    'Layout: `<folder>/<deviceId>/<chatId>-<yyyy-mm>.jsonl` plus `<folder>/<deviceId>/sessions.json`. One subfolder per device means two machines never write the same file, so the sync tool never creates "conflicted copy" files.',
    'push(batch): Rust command `sync_folder_write`: write to a temp file in the same folder and rename (atomic, so a half-written file is never uploaded). Merge with the existing file by record id and `savedAt`.',
    'pull(since): list the other devices\' folders, read files whose modified time is after the cursor, return one ArchiveBatch; importArchive merges. Skip the own device folder.',
    'Path safety: the command accepts only the folder saved in prefs and writes only `.jsonl`/`.json` names built from ids (no path parts from chat content).',
    'Detect the cloud tool for the hint text (path contains `Dropbox`, `OneDrive`, `iCloudDrive`) and warn when the folder is on a network drive that may be offline.',
    'Needs the `device` id in ArchiveMessage (see github.ts) so the folder name is stable.',
    'UI: path field with Browse, "Sync now", and "Open folder" (reuse `reveal_path`).',
  ],
});
