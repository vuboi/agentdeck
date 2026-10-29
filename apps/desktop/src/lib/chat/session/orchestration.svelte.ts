import { type Message } from '../mock';

/** Language guidance shared by the lead model and delegated workers. */
export const LANGUAGE_RULE = 'Write all user-facing replies and progress updates in the language the user has been using in this conversation, even when attached files, tool output, or these notes are in another language. Keep code, paths, commands, and quoted source text unchanged.';

/** Shortens text included in a worker prompt or history card. */
export const shorten = (text: string, max: number) => (text.length > max ? `${text.slice(0, max)}…` : text);

/** Flattens and shortens text shown in a notification or recovery message. */
export const clipText = (text: string, max: number) => {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** Resolves a delegate_task call when its child chat finishes. */
export const taskWaiters = new Map<string, (text: string, error: string | null, reply: Message) => void>();

/** Sends an answer to a question asked by an agent in its chat. */
