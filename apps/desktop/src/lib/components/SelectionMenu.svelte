<script lang="ts">
  import Menu from './Menu.svelte';
  import { closeOnScroll } from '../close-on-scroll';
  import { tick } from 'svelte';
  import { ZDropdownMenu } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { selectionMenu } from '../selection-menu.svelte';
  import { copyImage } from '../attachments';
  import { toast } from '../toast';

  // Right click on a picture, selected text, or anywhere in a message: Copy image, Copy, Copy message, Copy message as Markdown.
  let key = $state(0);
  let open = $state(false);
  $effect(() => closeOnScroll(() => open, () => (open = false)));

  $effect(() => {
    if (!selectionMenu.open) return;
    selectionMenu.open = false;
    open = false;
    key++;
    void tick().then(() => (open = true));
  });

  const copy = (text: string, done: string) => void navigator.clipboard.writeText(text).then(() => toast.success(done), () => toast.error('Could not copy'));

  const copyItem: ZDropdownMenuItem = { label: 'Copy', icon: 'lucideCopy', iconSize: '12', shortcut: 'Ctrl+C', class: 'text-xs! py-1', onClick: () => copy(selectionMenu.selected, 'Copied') };
  const imageItem: ZDropdownMenuItem = { label: 'Copy image', icon: 'lucideImage', iconSize: '12', class: 'text-xs! py-1', onClick: () => void copyImage(selectionMenu.image).then((done) => (done ? toast.success('Image copied') : toast.error('Could not copy the image'))) };
  const lead = $derived(selectionMenu.image || selectionMenu.selected);
  const items = $derived<ZDropdownMenuItem[]>([
    ...(selectionMenu.image ? [imageItem] : []),
    ...(selectionMenu.selected ? [copyItem] : []),
    { label: 'Copy message', icon: 'lucideClipboard', iconSize: '12', class: 'text-xs! py-1', divide: lead ? 'before' : undefined, onClick: () => copy(selectionMenu.plain, 'Message copied') },
    { label: 'Copy message as Markdown', icon: 'lucideFileText', iconSize: '12', class: 'text-xs! py-1', onClick: () => copy(selectionMenu.markdown || selectionMenu.plain, 'Markdown copied') },
  ]);
</script>

{#key key}
  <Menu items={items} bind:open={open} at={{ x: selectionMenu.x, y: selectionMenu.y }} position="bottom-left" width={220} />
{/key}
