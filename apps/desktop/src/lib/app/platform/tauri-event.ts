// `listen` of Tauri's events, except in the Preview's own copy of the app: the events are sent to every webview, and the
// copy answering them as well would do everything twice (saving, replying to the agents). It gets none.

import type { EventCallback, EventName, Options, UnlistenFn } from '@tauri-apps/api/event';
import { isPreviewCopy } from './history';

export async function listen<T>(event: EventName, handler: EventCallback<T>, options?: Options): Promise<UnlistenFn> {
  if (isPreviewCopy()) return () => {};
  const { listen: real } = await import('@tauri-apps/api/event');
  return real<T>(event, handler, options);
}
