<script lang="ts">
  import { tick } from 'svelte';
  import { ZDropdownMenu } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { isLocalUrl, linkMenu, openExternal, openInPreview } from '../browser.svelte';

  // The right-click menu of any link in an answer: where to open it, or copy it.
  let key = $state(0);
  let open = $state(false);

  $effect(() => {
    if (!linkMenu.open) return;
    linkMenu.open = false;
    open = false;
    key++;
    void tick().then(() => (open = true));
  });

  const items = $derived<ZDropdownMenuItem[]>([
    { label: 'Open in AgentDeck browser', icon: 'lucideGlobe', iconSize: '12', class: 'text-xs! py-1', onClick: () => openInPreview(linkMenu.url) },
    { label: 'Open in external browser', icon: 'lucideExternalLink', iconSize: '12', class: 'text-xs! py-1', onClick: () => void openExternal(linkMenu.url) },
    { label: 'Copy link', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', divide: 'before', onClick: () => void navigator.clipboard.writeText(linkMenu.url) },
  ]);
  void isLocalUrl;
</script>

{#key key}
  <div class="pointer-events-none fixed z-50 size-px" style:left="{linkMenu.x}px" style:top="{linkMenu.y}px">
    <ZDropdownMenu zItems={items} bind:open zPosition="bottom-left" zOffset={4} zMinWidth={200} zContentClass="p-1 text-xs">
      <span class="block size-px"></span>
    </ZDropdownMenu>
  </div>
{/key}
