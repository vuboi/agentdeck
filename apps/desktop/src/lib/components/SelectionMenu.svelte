<script lang="ts">
  import { closeOnScroll } from '../close-on-scroll';
  import { tick } from 'svelte';
  import { ZDropdownMenu } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { selectionMenu } from '../selection-menu.svelte';
  import { toast } from '../toast';

  // Right click on selected text (or anywhere in a message): Copy, Copy message, Copy message as Markdown.
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
  const items = $derived<ZDropdownMenuItem[]>([
    ...(selectionMenu.selected ? [copyItem] : []),
    { label: 'Copy message', icon: 'lucideClipboard', iconSize: '12', class: 'text-xs! py-1', divide: selectionMenu.selected ? 'before' : undefined, onClick: () => copy(selectionMenu.plain, 'Message copied') },
    { label: 'Copy message as Markdown', icon: 'lucideFileText', iconSize: '12', class: 'text-xs! py-1', onClick: () => copy(selectionMenu.markdown || selectionMenu.plain, 'Markdown copied') },
  ]);
</script>

{#key key}
  <div class="pointer-events-none fixed z-50 size-px" style:left="{selectionMenu.x}px" style:top="{selectionMenu.y}px">
    <ZDropdownMenu zItems={items} bind:open zPosition="bottom-left" zOffset={4} zMinWidth={200} zContentClass="p-1 text-xs">
      <span class="block size-px"></span>
    </ZDropdownMenu>
  </div>
{/key}
