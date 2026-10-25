// One place for the small pill notes (Copied, Deleted, Renamed): the same look in chat, the chat list and Explore.
import { zToastService } from '@shival99/z-ui-svelte';
import type { ZToastOptions } from '@shival99/z-ui-svelte';

export const toast = {
  /** A short confirmation, with an undo action when there is one. */
  success(message: string, options?: { undo?: () => void }) {
    const undo = options?.undo;
    const extra: ZToastOptions | undefined = undo ? { action: { label: 'Undo', onClick: undo } } : undefined;
    return zToastService.success(message, extra);
  },
  error(message: string) {
    return zToastService.error(message);
  },
  info(message: string) {
    return zToastService.info(message);
  },
};
