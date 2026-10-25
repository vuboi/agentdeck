import type { Block, Message } from './mock';
import { loadImagePath } from './attachments';
import { saveAttachment } from './agent';
import { getPref, setPref } from './prefs';

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
    const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
    let record = records.find((item) => item.messageId === message.id);
    if (!record) {
      record = { messageId: message.id, at: message.at, images: [] };
      records.push(record);
    }
    const saved = record.images.find((item) => item.ordinal === ordinal);
    if (saved) Object.assign(saved, { command: commands[ordinal].command, path, image: path ? undefined : image });
    else record.images.push({ ordinal, command: commands[ordinal].command, path, image: path ? undefined : image });
    setPref(keyOf(sessionId), records);
  });
}

/** Adds a compact answer fingerprint so screenshots still match when CLI history assigns a new message id. */
export function finishCommandImages(sessionId: string, message: Message) {
  return enqueueWrite(sessionId, async () => {
    const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
    const record = records.find((item) => item.messageId === message.id);
    if (!record) return;
    record.fingerprint = responseFingerprint(message);
    setPref(keyOf(sessionId), records);
  });
}

/** Restores screenshots from AgentDeck's local attachment store into CLI transcripts and local chat history. */
export async function restoreCommandImages(sessionId: string, messages: Message[]): Promise<Message[]> {
  await writes.get(sessionId);
  const records = (await getPref<SavedTurnImages[]>(keyOf(sessionId))) ?? [];
  if (!records.length) return messages;
  const candidates = messages.filter((message) => message.role === 'assistant' && message.blocks.some((block) => block.kind === 'command'));

  for (const record of records) {
    const exact = record.fingerprint ? candidates.filter((message) => responseFingerprint(message) === record.fingerprint) : [];
    const pool = exact.length ? exact : candidates;
    const target =
      pool.find((message) => message.id === record.messageId) ??
      pool
        .map((message) => ({ message, distance: Math.abs(Date.parse(message.at ?? '') - Date.parse(record.at ?? '')) }))
        .filter((item) => Number.isFinite(item.distance))
        .sort((a, b) => a.distance - b.distance)[0]?.message;
    if (!target) continue;
    const commandIndices = target.blocks.flatMap((block, index) => (block.kind === 'command' ? [index] : []));
    for (const saved of record.images) {
      let blockIndex = commandIndices[saved.ordinal];
      if (target.blocks[blockIndex]?.kind !== 'command' || (target.blocks[blockIndex] as Extract<Block, { kind: 'command' }>).command !== saved.command) {
        blockIndex = commandIndices.find((index) => target.blocks[index]?.kind === 'command' && (target.blocks[index] as Extract<Block, { kind: 'command' }>).command === saved.command) ?? -1;
      }
      const block = target.blocks[blockIndex];
      if (blockIndex < 0 || block?.kind !== 'command' || block.image) continue;
      const image = saved.path ? await loadImagePath(saved.path) : saved.image;
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
