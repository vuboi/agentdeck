// One place for the small pill notes (Copied, Deleted, Renamed): the same look in chat, the chat list and Explore.
import { zToastService } from '@shival99/z-ui-svelte';
import type { ZToastPillOptions } from '@shival99/z-ui-svelte';

export const toast = {
  /** A short confirmation, with an undo action when there is one. */
  success(message: string, options?: { undo?: () => void }) {
    const undo = options?.undo;
    const extra = undo ? { action: { label: 'Undo', run: undo } } : undefined;
    return zToastService.pill.success(message, extra as unknown as ZToastPillOptions | undefined);
  },
  error(message: string) {
    return zToastService.pill.error(message);
  },
  info(message: string) {
    return zToastService.pill.info(message);
  },
};
