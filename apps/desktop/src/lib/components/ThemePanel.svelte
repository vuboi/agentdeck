<script lang="ts">
  import { tick } from 'svelte';
  import { ZIcon, ZPopover, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { theme } from '../theme.svelte';
  import SectionTitle from './SectionTitle.svelte';
  import { NEUTRALS, PRIMARIES, swatch, type Mode } from '../theme-tokens';
  import type { ChatLayout } from '../theme.svelte';

  const layouts = [
    { value: 'split', label: 'Both sides', icon: 'lucideColumns3' as ZIconName },
    { value: 'single', label: 'One side', icon: 'lucideSquare' as ZIconName },
  ];

  const modes = [
    { value: 'light', label: 'Light', icon: 'lucideSun' as ZIconName },
    { value: 'dark', label: 'Dark', icon: 'lucideMoon' as ZIconName },
    { value: 'system', label: 'System', icon: 'lucideMonitor' as ZIconName },
  ];


  // The popover scales in; tabs measured during that are off-centre. They are drawn again once it has settled.
  let settled = $state(false);
  $effect(() => {
    if (!layout.themePanelOpen) {
      settled = false;
      return;
    }
    const timer = setTimeout(() => (settled = true), 280);
    return () => clearTimeout(timer);
  });

  // ZPopover owns outside click, Escape and the enter/exit animation. Focus enters the panel on open
  // and goes back to the trigger when it closes.
  function focusPanel() {
    void tick().then(() => document.querySelector<HTMLElement>('[data-theme-panel] button')?.focus());
  }
</script>

<ZPopover
  anchor={layout.themeAnchor}
  bind:open={layout.themePanelOpen}
  zTrigger="manual"
  zPosition="bottom-right"
  zOffset={6}
  zWidth={280}
  onOpen={focusPanel}
  onClosed={() => document.getElementById('theme-trigger')?.focus()}
>
  {#snippet children()}
    <div data-theme-panel class="flex w-full flex-col gap-4 p-3">
      <div>
        {#key settled}
        <ZTabs
          zTabs={modes}
          zType="segmented"
          zSize="sm"
          zTabsOnly
          zNavFullWidth
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          zCache={false}
          activeTab={theme.mode}
          onTabChange={(value) => (theme.mode = value as Mode)}
        />
        {/key}
      </div>

      <div>
        <SectionTitle label="PRIMARY" />
        <div class="grid grid-cols-9 justify-items-center gap-y-3" role="radiogroup" aria-label="Primary color">
          {#each PRIMARIES as color}
            <button
              type="button"
              role="radio"
              aria-checked={theme.primary === color}
              aria-label={color}
              title={color}
              class="swatch relative grid size-5 cursor-pointer place-items-center rounded-full outline-none ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring {theme.primary === color ? 'swatch-selected' : ''}"
              style:background={swatch(color)}
              style:--halo={swatch(color)}
              onclick={() => (theme.primary = color)}
            >
              {#if theme.primary === color}<ZIcon zType="lucideCheck" class="size-3 text-white mix-blend-difference" />{/if}
            </button>
          {/each}
        </div>
      </div>

      <div>
        <SectionTitle label="NEUTRAL" />
        <div class="grid grid-cols-9 justify-items-center gap-y-3" role="radiogroup" aria-label="Neutral color">
          {#each NEUTRALS as color}
            <button
              type="button"
              role="radio"
              aria-checked={theme.neutral === color}
              aria-label={color}
              title={color}
              class="swatch relative grid size-5 cursor-pointer place-items-center rounded-full outline-none ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring {theme.neutral === color ? 'swatch-selected' : ''}"
              style:background={swatch(color)}
              style:--halo={swatch(color)}
              onclick={() => (theme.neutral = color)}
            >
              {#if theme.neutral === color}<ZIcon zType="lucideCheck" class="size-3 text-white mix-blend-difference" />{/if}
            </button>
          {/each}
        </div>
      </div>

      <div>
        <SectionTitle label="CHAT LAYOUT" />
        {#key settled}
        <ZTabs
          zTabs={layouts}
          zType="segmented"
          zSize="sm"
          zTabsOnly
          zNavFullWidth
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          zCache={false}
          activeTab={theme.chatLayout}
          onTabChange={(value) => (theme.chatLayout = value as ChatLayout)}
        />
        {/key}
      </div>
    </div>
  {/snippet}
</ZPopover>

<style>
  /* Selected swatch: one soft halo in its own colour, same idea as the z-toast status halo. Static, no animation. */
  .swatch-selected {
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--halo) 40%, transparent);
  }
</style>
