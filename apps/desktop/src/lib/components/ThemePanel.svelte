<script lang="ts">
  import { tick } from 'svelte';
  import { ZIcon, ZPopover, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { theme } from '../theme.svelte';
  import { NEUTRALS, PRIMARIES, swatch, type Mode } from '../theme-tokens';
  import type { ChatLayout } from '../theme.svelte';

  const layouts = [
    { value: 'split', label: 'Both sides' },
    { value: 'single', label: 'One side' },
  ];

  const modes = [
    { value: 'light', label: 'Light', icon: 'lucideSun' as ZIconName },
    { value: 'dark', label: 'Dark', icon: 'lucideMoon' as ZIconName },
    { value: 'system', label: 'System', icon: 'lucideMonitor' as ZIconName },
  ];


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
    <div data-theme-panel class="flex w-[280px] min-w-[280px] flex-col gap-3 p-3">
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

      <div>
        <div class="mb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">PRIMARY</div>
        <div class="grid grid-cols-9 justify-items-center gap-y-2" role="radiogroup" aria-label="Primary color">
          {#each PRIMARIES as color}
            <button
              type="button"
              role="radio"
              aria-checked={theme.primary === color}
              aria-label={color}
              title={color}
              class="grid size-5 cursor-pointer place-items-center rounded-full outline-none ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring {theme.primary === color ? 'ring-2 ring-foreground' : ''}"
              style:background={swatch(color)}
              onclick={() => (theme.primary = color)}
            >
              {#if theme.primary === color}<ZIcon zType="lucideCheck" class="size-3 text-white mix-blend-difference" />{/if}
            </button>
          {/each}
        </div>
      </div>

      <div>
        <div class="mb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">NEUTRAL</div>
        <div class="grid grid-cols-9 justify-items-center" role="radiogroup" aria-label="Neutral color">
          {#each NEUTRALS as color}
            <button
              type="button"
              role="radio"
              aria-checked={theme.neutral === color}
              aria-label={color}
              title={color}
              class="grid size-5 cursor-pointer place-items-center rounded-full outline-none ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring {theme.neutral === color ? 'ring-2 ring-foreground' : ''}"
              style:background={swatch(color)}
              onclick={() => (theme.neutral = color)}
            >
              {#if theme.neutral === color}<ZIcon zType="lucideCheck" class="size-3 text-white mix-blend-difference" />{/if}
            </button>
          {/each}
        </div>
      </div>

      <div>
        <div class="mb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">CHAT LAYOUT</div>
        <ZTabs
          zTabs={layouts}
          zType="segmented"
          zSize="sm"
          zTabsOnly
          zNavFullWidth
          zCache={false}
          class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
          activeTab={theme.chatLayout}
          onTabChange={(value) => (theme.chatLayout = value as ChatLayout)}
        />
      </div>
    </div>
  {/snippet}
</ZPopover>
