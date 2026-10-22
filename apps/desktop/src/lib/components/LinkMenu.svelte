<script lang="ts">
  import { tick } from 'svelte';
  import { ZDropdownMenu, zModalService } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { linkMenu, openExternal, openInPreview } from '../browser.svelte';
  import { openFileTab } from '../content-tabs.svelte';
  import { fileWrites } from '../activity.svelte';
  import { toast } from '../toast';

  // The right-click menu of any link in an answer: a web address (where to open it, or copy it), or a file of the
  // project (open it, copy its path, or delete it to the trash).
  let key = $state(0);
  let open = $state(false);

  $effect(() => {
    if (!linkMenu.open) return;
    linkMenu.open = false;
    open = false;
    key++;
    void tick().then(() => (open = true));
  });

  async function deleteFile() {
    const file = linkMenu.file;
    const root = linkMenu.root;
    const name = file.split(/[\\/]/).at(-1) ?? file;
    zModalService.danger({
      zTitle: `Delete ${name}?`,
      zContent: 'It moves to ~/.agentdeck/trash/files, where it can be put back by hand.',
      zConfirmStyle: 'dialog',
      zOkText: 'Delete',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('path_trash', { root, path: file });
          fileWrites.tick++;
          toast.success(`Deleted ${name}`);
        } catch (error) {
          toast.error(String(error));
        }
      },
    });
  }

  const fileItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Open file', icon: 'lucideFile', iconSize: '12', class: 'text-xs! py-1', onClick: () => openFileTab(linkMenu.file, linkMenu.root) },
    {
      label: 'Reveal in File Explorer',
      icon: 'lucideFolder',
      iconSize: '12',
      class: 'text-xs! py-1',
      onClick: async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('reveal_path', { path: linkMenu.file.replace(/[\\/][^\\/]*$/, '') });
        } catch (error) {
          toast.error(String(error));
        }
      },
    },
    {
      label: 'Copy path',
      icon: 'lucideCopy',
      iconSize: '12',
      class: 'text-xs! py-1',
      onClick: () => {
        void navigator.clipboard.writeText(linkMenu.file);
        toast.success('Path copied');
      },
    },
    {
      label: 'Delete',
      icon: 'lucideTrash2',
      iconSize: '12',
      divide: 'before',
      class: 'text-xs! py-1 text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!',
      onClick: () => void deleteFile(),
    },
  ]);

  const webItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Open in AgentDeck browser', icon: 'lucideGlobe', iconSize: '12', class: 'text-xs! py-1', onClick: () => openInPreview(linkMenu.url) },
    { label: 'Open in external browser', icon: 'lucideExternalLink', iconSize: '12', class: 'text-xs! py-1', onClick: () => void openExternal(linkMenu.url) },
    { label: 'Copy link', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', divide: 'before', onClick: () => void navigator.clipboard.writeText(linkMenu.url).then(() => toast.success('Link copied')) },
  ]);
</script>

{#key key}
  <div class="pointer-events-none fixed z-50 size-px" style:left="{linkMenu.x}px" style:top="{linkMenu.y}px">
    <ZDropdownMenu zItems={linkMenu.file ? fileItems : webItems} bind:open zPosition="bottom-left" zOffset={4} zMinWidth={200} zContentClass="p-1 text-xs">
      <span class="block size-px"></span>
    </ZDropdownMenu>
  </div>
{/key}
