import type { Block, Message } from '../mock';
import { pictureUrl, saveAttachment } from '../agent';
import { getPref, setPref } from '../../app/platform/prefs';

// Each screenshot is a file under ~/.agentdeck/attachments (WebP). The index below keeps only its path, per chat, in the
// IndexedDB `images` collection (prefs.ts). Pictures are drawn from the file by the asset protocol, never as base64.
// TODO(sync): when chats sync, upload each file once and keep its remote id next to `path` here; the local path stays the
// fast copy. Nothing here uploads yet.
type SavedCommandImage = { ordinal: number; command: string; path?: string; image?: string };
type SavedTurnImages = { messageId: string; at?: string; fingerprint?: string; images: SavedCommandImage[] };

const keyOf = (sessionId: string) => `chat.toolImages.${sessionId}`;
const writes = new Map<string, Promise<void>>();

function responseFingerprint(message: Message): string {
  const text = message.blocks.filter((block) => block.kind === 'response').map((block) => block.text).join('\n\n');
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index++) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${text.length}:${(hash >>> 0).toString(16)}`;
}

function enqueueWrite(sessionId: string, action: () => Promise<void>): Promise<void> {
  const next = (writes.get(sessionId) ?? Promise.resolve()).then(action).catch(() => {});
  writes.set(sessionId, next);
  return next;
}

/** Saves a tool screenshot as an AgentDeck attachment and records where it belongs in this assistant turn. */
export function persistCommandImage(sessionId: string, message: Message, commandId: string, image: string) {
  const headerEnd = image.indexOf(',');
  if (!image.startsWith('data:image/') || headerEnd < 0) return;
  const mime = image.slice(5, image.indexOf(';'));
  if (!['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(mime)) return;
  const commands = message.blocks.filter((block): block is Extract<Block, { kind: 'command' }> => block.kind === 'command');
  const ordinal = commands.findIndex((block) => block.id === commandId);
  if (ordinal < 0) return;

  void enqueueWrite(sessionId, async () => {
    let path: string | undefined;
    try {
      path = await saveAttachment(mime, image.slice(headerEnd + 1));
    } catch {
      // Keep the screenshot in AgentDeck's local history if the attachment directory is temporarily unavailable.
    }
    // The chat shows the file from now on, so the base64 leaves the live message as well as the index.
    if (path) {
      const block = message.blocks.find((item): item is Extract<Block, { kind: 'command' }> => item.kind === 'command' && item.id === commandId);
      if (block?.image?.startsWith('data:')) block.image = await pictureUrl(path);
    }
    const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
    let record = records.find((item) => item.messageId === message.id);
    if (!record) {
      record = { messageId: message.id, at: message.at, images: [] };
      records.push(record);
    }
    const saved = record.images.find((item) => item.ordinal === ordinal);
    if (saved) Object.assign(saved, { command: commands[ordinal].command, path, image: path ? undefined : image });
    else record.images.push({ ordinal, command: commands[ordinal].command, path, image: path ? undefined : image });
    await setPref(keyOf(sessionId), records);
  });
}

/** Adds a compact answer fingerprint so screenshots still match when CLI history assigns a new message id. */
export function finishCommandImages(sessionId: string, message: Message) {
  return enqueueWrite(sessionId, async () => {
    const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
    const record = records.find((item) => item.messageId === message.id);
    if (!record) return;
    record.fingerprint = responseFingerprint(message);
    await setPref(keyOf(sessionId), records);
  });
}

/** Restores screenshots from AgentDeck's local attachment store into CLI transcripts and local chat history. */
export async function restoreCommandImages(sessionId: string, messages: Message[]): Promise<Message[]> {
  await writes.get(sessionId);
  const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
  if (!records.length) return messages;
  const candidates = messages.filter((message) => message.role === 'assistant' && message.blocks.some((block) => block.kind === 'command'));

  for (const record of records) {
    const ranked = candidates
      .map((message) => {
        const commands = message.blocks.filter((block): block is Extract<Block, { kind: 'command' }> => block.kind === 'command');
        const matchingImages = record.images.filter(
          (saved) => commands[saved.ordinal]?.command === saved.command || commands.some((block) => block.command === saved.command),
        ).length;
        const distance = Math.abs(Date.parse(message.at ?? '') - Date.parse(record.at ?? ''));
        return {
          message,
          matchingImages,
          exactId: message.id === record.messageId,
          exactResponse: Boolean(record.fingerprint && responseFingerprint(message) === record.fingerprint),
          distance: Number.isFinite(distance) ? distance : Number.POSITIVE_INFINITY,
        };
      })
      .filter((item) => item.matchingImages > 0)
      .sort(
        (a, b) =>
          Number(b.exactId) - Number(a.exactId) ||
          b.matchingImages - a.matchingImages ||
          Number(b.exactResponse) - Number(a.exactResponse) ||
          a.distance - b.distance,
      );
    const target = ranked[0]?.message;
    if (!target) continue;
    const commandIndices = target.blocks.flatMap((block, index) => (block.kind === 'command' ? [index] : []));
    for (const saved of record.images) {
      let blockIndex = commandIndices[saved.ordinal];
      if (target.blocks[blockIndex]?.kind !== 'command' || (target.blocks[blockIndex] as Extract<Block, { kind: 'command' }>).command !== saved.command) {
        blockIndex = commandIndices.find((index) => target.blocks[index]?.kind === 'command' && (target.blocks[index] as Extract<Block, { kind: 'command' }>).command === saved.command) ?? -1;
      }
      const block = target.blocks[blockIndex];
      if (blockIndex < 0 || block?.kind !== 'command') continue;
      // A picture already drawn from its file stays. Base64 from an older save is replaced by the file, when there is one.
      if (block.image && !(saved.path && block.image.startsWith('data:'))) continue;
      const image = saved.path ? await pictureUrl(saved.path) : saved.image;
      if (image) target.blocks[blockIndex] = { ...block, image };
    }
  }
  return messages;
}

/** Keeps large screenshot data out of chat caches; paths are restored with `restoreCommandImages`. */
export function stripCommandImageData(messages: Message[]): Message[] {
  return messages.map((message) => ({
    ...message,
    blocks: message.blocks.map((block) => {
      if (block.kind !== 'command' || !block.image?.startsWith('data:')) return block;
      const { image: _image, ...withoutImage } = block;
      return withoutImage;
    }),
  }));
}

/** Drops the AgentDeck screenshot index when its chat is removed. */
export function deleteCommandImages(sessionId: string) {
  setPref(keyOf(sessionId), null);
}
