import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
import { openAttachment } from '../../browser/preview.svelte';
import { toast } from '../../app/platform/toast';
import { hostPlatform, nativePath } from '../../files/path-links';
import { pictureData, saveAttachment } from '../agent';
import { copyImage } from './attachments';

// The right-click menu of a picture or document waiting in a message box (chat and image generator alike).
const row = 'text-sm! py-1';
const removeRow = `${row} text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!`;
const TEXT_FILE = /\.(txt|md|mdx|log|json|ya?ml|toml|xml|csv|tsv|[jt]sx?|svelte|vue|py|rs|go|java|c|cpp|h|cs|rb|php|sh|ps1|sql|html|css|scss)$/i;

export type Picture = { id: string; name: string; mime: string; src: string; path?: string };
export type Document = { name: string; path: string; size: number };
export type AttachmentActions = { preview: () => void; remove: () => void };

const dirOf = (path: string) => path.replace(/[\\/][^\\/]*$/, '');
const failed = (message: string) => () => toast.error(message);

async function invokeTauri<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

async function copyText(text: string, done: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(done);
  } catch {
    toast.error('Could not access the clipboard');
  }
}

/** As this system writes it: backslashes on Windows, slashes on macOS and Linux. */
const copyPath = (path: string) => copyText(nativePath(path, hostPlatform()), 'Path copied');
const showInFolder = (path: string) => invokeTauri('path_reveal', { path }).catch(failed('Could not show the file'));

async function copyFileContents(path: string): Promise<void> {
  try {
    const contents = await invokeTauri<string>('file_read', { root: dirOf(path), path });
    await copyText(contents, 'File contents copied');
  } catch (error) {
    toast.error(String(error));
  }
}

// A pasted picture lives only in the message box until it is sent: it is written to disk once, on the first path request.
const savedPictures = new Map<string, Promise<string>>();
function picturePath(picture: Picture): Promise<string> {
  if (picture.path) return Promise.resolve(picture.path);
  let saved = savedPictures.get(picture.id);
  if (!saved) {
    saved = saveAttachment(picture.mime, picture.src.slice(picture.src.indexOf(',') + 1));
    savedPictures.set(picture.id, saved);
    saved.catch(() => savedPictures.delete(picture.id));
  }
  return saved;
}

export function pictureMenuItems(picture: Picture, actions: AttachmentActions): ZDropdownMenuItem[] {
  return [
    { label: 'Preview', icon: 'lucideImage', iconSize: '12', class: row, onClick: actions.preview },
    {
      label: 'Copy image',
      icon: 'lucideImage',
      iconSize: '12',
      class: row,
      onClick: () => void pictureData(picture).then(copyImage).then((done) => (done ? toast.success('Image copied') : toast.error('Could not copy the image'))),
    },
    { label: 'Copy path', icon: 'lucideCopy', iconSize: '12', class: row, onClick: () => void picturePath(picture).then(copyPath, failed('Could not save the image')) },
    { label: 'Show in folder', icon: 'lucideFolder', iconSize: '12', class: row, onClick: () => void picturePath(picture).then(showInFolder, failed('Could not save the image')) },
    { label: 'Remove', icon: 'lucideX', iconSize: '12', divide: 'before', class: removeRow, onClick: actions.remove },
  ];
}

export function documentMenuItems(file: Document, remove: () => void): ZDropdownMenuItem[] {
  const contents: ZDropdownMenuItem[] = TEXT_FILE.test(file.name)
    ? [{ label: 'Copy file contents', icon: 'lucideCopy', iconSize: '12', class: row, onClick: () => void copyFileContents(file.path) }]
    : [];
  return [
    { label: 'Preview', icon: 'lucideFile', iconSize: '12', class: row, onClick: () => void openAttachment(file.path, file.name, file.size) },
    ...contents,
    { label: 'Copy path', icon: 'lucideCopy', iconSize: '12', class: row, onClick: () => void copyPath(file.path) },
    { label: 'Show in folder', icon: 'lucideFolder', iconSize: '12', class: row, onClick: () => void showInFolder(file.path) },
    { label: 'Remove', icon: 'lucideX', iconSize: '12', divide: 'before', class: removeRow, onClick: remove },
  ];
}
