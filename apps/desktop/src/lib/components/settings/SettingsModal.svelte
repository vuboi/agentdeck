<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import { ZButton, ZIcon, ZModal } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { inTauri, isPreviewCopy } from '../../app/platform/history';
  import { getPref } from '../../app/platform/prefs';
  import {
    connectors,
    consumeCodexResetCredit,
    refreshAgyQuota,
    refreshAll,
    scanLocal,
    settings,
    startWatching,
    warnings,
    type Section,
  } from '../../agents/connectors.svelte';
  import AboutSection from './sections/AboutSection.svelte';
  import DebugSection from './sections/DebugSection.svelte';
  import AlertsSection from './sections/AlertsSection.svelte';
  import ComputerSection from './sections/ComputerSection.svelte';
  import ConnectorsSection from './sections/ConnectorsSection.svelte';
  import MemorySection from './sections/MemorySection.svelte';
  import OfficeSection from './sections/OfficeSection.svelte';
  import ProviderModals from './sections/ProviderModals.svelte';
  import { ProviderForm } from './sections/provider-form.svelte';
  import ProvidersSection from './sections/ProvidersSection.svelte';
  import RepliesSection from './sections/RepliesSection.svelte';
  import SyncSection from './sections/SyncSection.svelte';

  const icon = (name: string) => name as ZIconName;

  const NAV: { id: Section; label: string; icon: string; hint: string }[] = [
    { id: 'connectors', label: 'Connectors', icon: 'lucidePlug', hint: 'CLIs and accounts' },
    { id: 'providers', label: 'Providers', icon: 'lucideServer', hint: 'Custom and local models' },
    { id: 'replies', label: 'Replies', icon: 'lucideMessageSquare', hint: 'House style and file links' },
    { id: 'memory', label: 'Memory', icon: 'lucideBrain', hint: 'What agents know about a project' },
    { id: 'office', label: 'Office files', icon: 'lucideFileText', hint: 'Word, Excel and PowerPoint for agents' },
    { id: 'alerts', label: 'Limit alerts', icon: 'lucideBell', hint: 'Warn before a limit' },
    { id: 'sync', label: 'Sync', icon: 'lucideCloudUpload', hint: 'History backup (planned)' },
    { id: 'computer', label: 'Computer use', icon: 'lucideMonitor', hint: 'Windows screen and input via MCP or CLI' },
    { id: 'debug', label: 'Debug', icon: 'lucideBug', hint: 'Trace where the app hangs' },
    { id: 'about', label: 'About', icon: 'lucideInfo', hint: 'Version and links' },
  ];


  // Kept here so they survive a visit to another section.
  let memoryProjectId = $state('');
  let connectorTab = $state('');
  /** Account whose "reset limit" confirmation modal is open. */
  let resetAsk = $state('');
  const providerForm = new ProviderForm();

  let computerUseEnabled = $state(true);
  void getPref<boolean>('computer_use.enabled').then((value) => {
    computerUseEnabled = value ?? true;
    if (inTauri() && !isPreviewCopy()) void import('@tauri-apps/api/core').then(({ invoke }) => invoke('computer_use_set_enabled', { enabled: computerUseEnabled }));
  });
  async function setComputerUse(value: boolean) {
    computerUseEnabled = value;
    if (isPreviewCopy() || !inTauri()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('computer_use_set_enabled', { enabled: value });
    const { setPref } = await import('../../app/platform/prefs');
    setPref('computer_use.enabled', value);
  }

  // --- Alerts ---------------------------------------------------------------------------------------------
  const active = $derived(warnings(ticker.min));

  // --- Window ---------------------------------------------------------------------------------------------
  let panel: HTMLElement | undefined = $state();
  function close() {
    settings.open = false;
  }
  function onKey(event: KeyboardEvent) {
    if (settings.open && event.key === 'Escape') {
      event.stopPropagation();
      if (providerForm.draft) providerForm.draft = null;
      else close();
    }
  }
  $effect(() => {
    if (!settings.open) return;
    // Keep the connector data cached between opens. The header Refresh button reloads it on demand;
    // re-running all CLI/account checks whenever Settings opens makes the list feel like it is loading on scroll.
    untrack(() => {
      if (!connectors.detected) {
        void refreshAll(false);
        void scanLocal();
      }
    });
    void tick().then(() => panel?.focus());
  });

  // The button's icon turns while the CLIs are checked and the usage is read again.
  const refreshingAll = $derived(connectors.manualRefreshing);
  const refreshingDetect = $derived(Boolean(connectors.busy.detect));

  // Antigravity's figures are saved to a file by the status line of agy whenever it runs, not by AgentDeck: while Settings
  // is open the file is read again every half minute, so new figures show without pressing Refresh.
  $effect(() => {
    if (!settings.open) return;
    const timer = setInterval(() => void refreshAgyQuota(), 60_000);
    return () => clearInterval(timer);
  });

  onMount(() => startWatching());
</script>

<svelte:window onkeydowncapture={onKey} />

{#if settings.open}
  <div class="fixed inset-0 z-[49] grid place-items-center p-4 sm:p-8" role="presentation">
    <button type="button" class="absolute inset-0 cursor-default bg-black/45 backdrop-blur-[2px]" aria-label="Close settings" tabindex="-1" onclick={close} transition:fade={{ duration: 140 }}></button>

    <div
      bind:this={panel}
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      class="relative flex h-[min(680px,100%)] w-[min(1000px,100%)] overflow-hidden rounded-xl border border-border bg-background text-foreground shadow-2xl outline-none"
      transition:scale={{ duration: 160, start: 0.97 }}
    >
      <!-- Navigation -->
      <nav class="flex w-48 shrink-0 flex-col gap-1 border-r border-border bg-muted/30 p-3 max-[700px]:w-14 max-[700px]:items-center" aria-label="Settings sections">
        <div class="mb-2 flex items-center gap-2 px-2 pt-1 text-sm font-semibold max-[700px]:hidden">
          <ZIcon zType={icon('lucideSettings2')} class="size-4 text-primary" /> Settings
        </div>
        {#each NAV as item (item.id)}
          {@const on = settings.section === item.id}
          <button
            type="button"
            class="group relative flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring max-[700px]:size-9 max-[700px]:justify-center max-[700px]:p-0 {on ? 'bg-primary/12 text-primary' : 'text-muted-foreground hover:bg-primary/8 hover:text-foreground'}"
            aria-current={on ? 'page' : undefined}
            title={item.label}
            onclick={() => (settings.section = item.id)}
          >
            <ZIcon zType={icon(item.icon)} class="size-4 shrink-0" />
            <span class="min-w-0 max-[700px]:hidden">
              <span class="block truncate font-medium">{item.label}</span>
              <span class="block truncate text-[0.6875rem] font-normal opacity-70">{item.hint}</span>
            </span>
            {#if item.id === 'alerts' && active.length > 0}
              <span class="absolute top-2 right-2 size-2 rounded-full bg-destructive" aria-label="{active.length} warnings"></span>
            {/if}
          </button>
        {/each}
      </nav>

      <!-- Content -->
      <div class="flex min-w-0 flex-1 flex-col">
        <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-5">
          <h2 class="text-sm font-semibold">{NAV.find((item) => item.id === settings.section)?.label}</h2>
          <span class="ml-auto"></span>
          {#if settings.section === 'connectors'}
            <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2 text-sm" title="Check CLIs and reload usage" disabled={refreshingAll} onclick={() => void refreshAll(true)}>
              <ZIcon zType={icon(refreshingAll ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {refreshingAll ? 'animate-spin' : ''}" /> Refresh
            </ZButton>
          {/if}
          <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0" aria-label="Close settings" onclick={close}>
            <ZIcon zType={icon('lucideX')} class="size-4" />
          </ZButton>
        </header>

        <div class="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-5 py-4 [scrollbar-gutter:stable] max-[700px]:px-3">
          {#if !inTauri()}
            <p class="rounded-lg border border-dashed border-border p-6 text-center text-[0.75rem] text-muted-foreground">Connectors need the desktop app: open AgentDeck, not the browser preview.</p>

          {:else if settings.section === 'connectors'}
            <ConnectorsSection bind:connectorTab bind:resetAsk />

          {:else if settings.section === 'providers'}
            <ProvidersSection form={providerForm} />

          {:else if settings.section === 'debug'}
            <DebugSection />

          {:else if settings.section === 'about'}
            <AboutSection />

          {:else if settings.section === 'replies'}
            <RepliesSection />

          {:else if settings.section === 'memory'}
            <MemorySection bind:memoryProjectId />

          {:else if settings.section === 'office'}
            <OfficeSection />

          {:else if settings.section === 'computer'}
            <ComputerSection enabled={computerUseEnabled} onChange={(value) => void setComputerUse(value)} />

          {:else if settings.section === 'sync'}
            <SyncSection />

          {:else}
            <AlertsSection {active} />
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<ZModal open={resetAsk !== ''} zTitle="Reset Codex limits" zWidth="460px" zBodyClass="min-h-0! py-3" zOkText="Use one reset" zCancelText="Cancel" onOk={() => { const email = resetAsk; resetAsk = ''; void consumeCodexResetCredit(email); }} onCancel={() => (resetAsk = '')}>
  <p class="text-[0.75rem] text-muted-foreground">
    This will redeem one available reset credit for <b class="text-foreground">{resetAsk}</b> and refresh its Codex limits. The credit is consumed immediately.
  </p>
</ZModal>

<ProviderModals form={providerForm} />
