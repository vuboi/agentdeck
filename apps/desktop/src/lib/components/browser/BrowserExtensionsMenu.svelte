<script lang="ts">
  import { ZButton, ZIcon, ZLoading, ZPopover, ZSwitch, zModalService, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { TRIGGER_ON } from '../../app/ui/popover';
  import { toast } from '../../app/platform/toast';
  import {
    activeTab,
    addBrowserExtension,
    enableBrowserExtension,
    listBrowserExtensions,
    openInPreview,
    reloadBrowserExtension,
    removeBrowserExtension,
    type BrowserExtension,
  } from '../../browser/browser.svelte';

  let { open = $bindable(false), buttonClass }: { open: boolean; buttonClass: string } = $props();

  const tab = $derived(activeTab());

  /** Adds an unpacked extension; also called from the toolbar's More menu. */
  export async function addExtension() {
    try {
      const name = await addBrowserExtension(tab.id);
      if (name) toast.success(tab.opened ? `Added ${name} to Browser Preview` : `Added ${name}: it loads when a page opens`);
      if (name && open) void loadExtensions();
    } catch (error) {
      toast.error(String(error));
    }
  }

  // The toolbar's extension menu: what the Preview profile has, each with on/off, reload, remove, and its toolbar page.
  let extButton = $state<HTMLElement | null>(null);
  let extensions = $state<BrowserExtension[]>([]);
  let extLoading = $state(false);
  /** The extension an action runs on (its id), so only its row shows it. */
  let extBusy = $state('');
  async function loadExtensions() {
    extLoading = true;
    try {
      extensions = await listBrowserExtensions(tab.id);
    } catch (error) {
      toast.error(String(error));
    } finally {
      extLoading = false;
    }
  }
  function toggleExtensions() {
    open = !open;
    if (open) void loadExtensions();
  }
  async function onExtension(item: BrowserExtension, action: () => Promise<unknown>, done: string) {
    extBusy = item.id;
    try {
      await action();
      toast.success(done);
      await loadExtensions();
    } catch (error) {
      toast.error(String(error));
    } finally {
      extBusy = '';
    }
  }
  function dropExtension(item: BrowserExtension) {
    zModalService.danger({
      zTitle: `Remove ${item.name}?`,
      zContent: 'It is removed from Browser Preview, with its copied files.',
      zConfirmStyle: 'dialog',
      zOkText: 'Remove',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => void onExtension(item, () => removeBrowserExtension(tab.id, item.id), `${item.name} removed`),
    });
  }
  function openExtensionPage(item: BrowserExtension) {
    if (!item.popup) return;
    open = false;
    openInPreview(item.popup, 'new');
  }
</script>

<button bind:this={extButton} type="button" class="{buttonClass} {open ? TRIGGER_ON : ''}" aria-label="Extensions" aria-expanded={open} {@attach zTooltip({ content: 'Extensions', position: 'top' })} onclick={toggleExtensions}>
  <ZIcon zType={'lucidePuzzle' as ZIconName} class="size-4" />
</button>
<ZPopover anchor={extButton} bind:open zTrigger="manual" zPosition="bottom-right" zOffset={6} zWidth={320}>
  {#snippet children()}
    <div class="flex flex-col text-sm">
      <div class="flex items-center gap-2 px-3 pt-2.5 pb-2">
        <ZIcon zType={'lucidePuzzle' as ZIconName} class="size-3.5 text-muted-foreground" />
        <span class="font-medium">Extensions</span>
        {#if extensions.length}<span class="rounded-full bg-muted px-1.5 text-[10px] text-muted-foreground tabular-nums">{extensions.length}</span>{/if}
        {#if extLoading}<ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="primary" />{/if}
      </div>
      {#if extensions.length === 0 && !extLoading}
        <p class="m-0 mx-2 mb-2 rounded-lg border border-dashed border-border px-3 py-4 text-center text-muted-foreground">No extension yet. Add an unpacked one: a folder with a manifest.json.</p>
      {/if}
      <div class="flex max-h-80 flex-col gap-1.5 overflow-y-auto px-2 pb-2">
        {#each extensions as item (item.id || item.name)}
          {@const busyRow = extBusy !== '' && extBusy === item.id}
          <!-- One card each: its logo and name on top, what can be done to it below. -->
          <div class="rounded-lg border border-border bg-card/40 transition-colors hover:border-primary/30">
            <button type="button" class="flex w-full items-center gap-2.5 px-2.5 pt-2.5 pb-2 text-left outline-none {item.popup && item.enabled ? 'cursor-pointer' : 'cursor-default'}" disabled={!item.popup || !item.enabled} title={item.popup && item.enabled ? 'Open its page in a new tab' : undefined} onclick={() => openExtensionPage(item)}>
              <span class="grid size-7 shrink-0 place-items-center rounded-md bg-muted {item.enabled ? '' : 'grayscale opacity-60'}">
                {#if item.icon}<img src={item.icon} alt="" class="size-5 object-contain" />{:else}<ZIcon zType={'lucidePuzzle' as ZIconName} class="size-4 text-muted-foreground" />{/if}
              </span>
              <span class="min-w-0 flex-1">
                <span class="block truncate font-medium {item.enabled ? 'text-foreground' : 'text-muted-foreground'}" {@attach zTooltip({ content: item.name, position: 'top', autoDetect: true })}>{item.name}</span>
                <span class="mt-0.5 block truncate text-[0.6875rem] text-muted-foreground">{!item.id ? 'Loads when a page opens' : item.enabled ? (item.popup ? 'On · click to open its page' : 'On') : 'Off'}</span>
              </span>
            </button>
            <div class="flex items-center gap-1 border-t border-border/60 px-2.5 py-1.5">
              <ZSwitch zVariant="dot" zSize="sm" checked={item.enabled} zLoading={busyRow} zDisabled={!item.id || busyRow} onChange={(on) => void onExtension(item, () => enableBrowserExtension(tab.id, item.id, on), on ? `${item.name} turned on` : `${item.name} turned off`)} />
              <span class="ml-1.5 text-[0.6875rem] text-muted-foreground">{item.enabled ? 'On' : 'Off'}</span>
              <span class="flex-1"></span>
              <button type="button" class="inline-flex h-5 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 text-[10px] text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary disabled:pointer-events-none disabled:opacity-40" aria-label="Reload {item.name}" disabled={!item.id || !item.enabled || busyRow} onclick={() => void onExtension(item, () => reloadBrowserExtension(tab.id, item.id), `${item.name} reloaded`)}>
                <ZIcon zType={'lucideRefreshCw' as ZIconName} class="size-2.5" /> Reload
              </button>
              <button type="button" class="inline-flex h-5 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 text-[10px] text-destructive outline-none hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-40" aria-label="Remove {item.name}" disabled={!item.id || busyRow} onclick={() => dropExtension(item)}>
                <ZIcon zType={'lucideTrash2' as ZIconName} class="size-2.5" /> Remove
              </button>
            </div>
          </div>
        {/each}
      </div>
      {#if extensions.some((item) => !item.id)}
        <p class="m-0 px-3 pb-2 text-[0.6875rem] text-muted-foreground">Open a page to turn them on or off, reload or remove them.</p>
      {/if}
      <div class="border-t border-border p-2">
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 w-full gap-1.5 px-3 text-sm" onclick={() => void addExtension()}>
          <ZIcon zType={'lucidePlus' as ZIconName} class="size-3.5" /> Add unpacked extension…
        </ZButton>
      </div>
    </div>
  {/snippet}
</ZPopover>
