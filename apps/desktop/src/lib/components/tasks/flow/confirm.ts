import { zModalService } from '@shival99/z-ui-svelte';

/** Asks before steps are deleted (the dialog-style confirm); resolves true when the user agrees. */
export async function confirmDelete(names: string[]): Promise<boolean> {
  const one = names.length === 1;
  const ref = zModalService.danger({
    zTitle: one ? 'Delete this step?' : `Delete ${names.length} steps?`,
    zContent: `${one ? `“${names[0]}”` : names.map((name) => `“${name}”`).join(', ')} will be removed with ${one ? 'its' : 'their'} links. The steps after ${one ? 'it' : 'them'} are linked to the ones before.`,
    zConfirmStyle: 'dialog',
    zOkText: 'Delete',
    zCancelText: 'Cancel',
    zOkDestructive: true,
  });
  return (await ref.closed) === true;
}
