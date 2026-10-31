<script lang="ts">
  import Menu from './Menu.svelte';
  import { closeOnScroll } from '../../app/ui/close-on-scroll';
  import { tick } from 'svelte';
  import { ZDropdownMenu, zModalService } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { linkMenu, openExternal, openInPreview } from '../../browser/browser.svelte';
  import { openInViewer } from '../../files/file-viewer.svelte';
  import { fileWrites } from '../../chat/runs/activity.svelte';
  import { toast } from '../../app/platform/toast';
  import { hostPlatform, nativePath } from '../../files/path-links';
  import { openWithItem, openWithOptions as getOpenWithOptions, type OpenWithOption } from '../../files/open-with';
  import { TEXT_FILE } from '../../chat/content/attachment-menu';

  let openWithOptions = $state<OpenWithOption[]>([]);

  // The right-click menu of any link in an answer: a web address (where to open it, or copy it), or a file of the
  // project (open it, copy its path, or delete it to the trash).
  let key = $state(0);
  let open = $state(false);
  $effect(() => closeOnScroll(() => open, () => (open = false)));

  $effect(() => {
    if (!linkMenu.open) return;
    const file = linkMenu.file;
    linkMenu.open = false;
    open = false;
    key++;
    openWithOptions = [];
    void (async () => {
      if (file) {
        try {
          openWithOptions = await getOpenWithOptions(file);
        } catch (error) {
          toast.error(String(error));
        }
      }
      await tick();
      open = true;
    })();
  });

  async function copyContents() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const contents = await invoke<string>('file_read', { root: linkMenu.root, path: linkMenu.file });
      await navigator.clipboard.writeText(contents);
      toast.success('File contents copied');
    } catch (error) {
      toast.error(String(error));
    }
  }

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

  // A folder cannot be opened in the viewer or read as text, and only a text file has contents to copy.
  const fileName = $derived(linkMenu.file.split(/[\\/]/).at(-1) ?? '');
  const fileItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Open file', icon: 'lucideFile', iconSize: '12', class: 'text-sm! py-1', disabled: linkMenu.dir, onClick: () => openInViewer(linkMenu.file, linkMenu.root) },
    ...openWithItem(linkMenu.file, openWithOptions).map((item) => ({ ...item, disabled: linkMenu.dir, submenu: linkMenu.dir ? undefined : item.submenu })),
    { label: 'Copy file contents', icon: 'lucideCopy', iconSize: '12', class: 'text-sm! py-1', disabled: linkMenu.dir || !TEXT_FILE.test(fileName), onClick: () => void copyContents() },
    {
      label: 'Open containing folder',
      icon: 'lucideFolder',
      iconSize: '12',
      class: 'text-sm! py-1',
      onClick: async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          // The file is selected in the file manager; a link is followed to the real folder (path_links.rs).
          await invoke('path_reveal', { path: linkMenu.file });
        } catch (error) {
          toast.error(String(error));
        }
      },
    },
    {
      label: 'Copy path',
      icon: 'lucideCopy',
      iconSize: '12',
      class: 'text-sm! py-1',
      onClick: () => {
        // As this system writes it: backslashes on Windows, slashes on macOS and Linux.
        void navigator.clipboard.writeText(nativePath(linkMenu.file, hostPlatform()));
        toast.success('Path copied');
      },
    },
    {
      label: 'Delete',
      icon: 'lucideTrash2',
      iconSize: '12',
      divide: 'before',
      class: 'text-sm! py-1 text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!',
      onClick: () => void deleteFile(),
    },
  ]);

  const webItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Open in AgentDeck browser', icon: 'lucideGlobe', iconSize: '12', class: 'text-sm! py-1', onClick: () => openInPreview(linkMenu.url) },
    { label: 'Open in external browser', icon: 'lucideExternalLink', iconSize: '12', class: 'text-sm! py-1', onClick: () => void openExternal(linkMenu.url) },
    { label: 'Copy link', icon: 'lucideCopy', iconSize: '12', class: 'text-sm! py-1', divide: 'before', onClick: () => void navigator.clipboard.writeText(linkMenu.url).then(() => toast.success('Link copied')) },
  ]);
</script>

{#key key}
  <Menu items={linkMenu.file ? fileItems : webItems} bind:open={open} at={{ x: linkMenu.x, y: linkMenu.y }} position="bottom-left" width={220} />
{/key}
