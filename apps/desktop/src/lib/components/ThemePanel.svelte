<script lang="ts">
  import { tick } from 'svelte';
  import { ZDivider, ZIcon, ZPopover, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { theme } from '../theme.svelte';
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
    <div data-theme-panel class="flex w-[280px] min-w-[280px] flex-col divide-y divide-border p-3">
      <div class="pb-4">
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
      </div>

      <div class="py-4">
        <ZDivider zLabel="PRIMARY" zAlign="start" zSize="sm" class="mb-1.5 text-[10px] tracking-widest text-muted-foreground" />
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

      <div class="py-4">
        <ZDivider zLabel="NEUTRAL" zAlign="start" zSize="sm" class="mb-1.5 text-[10px] tracking-widest text-muted-foreground" />
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

      <div class="pt-4">
        <ZDivider zLabel="CHAT LAYOUT" zAlign="start" zSize="sm" class="mb-1.5 text-[10px] tracking-widest text-muted-foreground" />
        <!-- Own segmented control with a sliding thumb. Its position is plain CSS from the two equal halves
             (no measuring), so the popover's scale-in can't throw it off-centre the way ZTabs did. -->
        <div class="relative grid grid-cols-2 gap-1 rounded-lg bg-accent p-0.5 dark:bg-muted/50" role="radiogroup" aria-label="Chat layout">
          <span
            aria-hidden="true"
            class="pointer-events-none absolute inset-y-0.5 rounded-md bg-background shadow-sm transition-[left] duration-200 ease-out"
            style:width="calc(50% - 4px)"
            style:left={theme.chatLayout === 'split' ? '2px' : 'calc(50% + 2px)'}
          ></span>
          {#each layouts as item}
            {@const active = theme.chatLayout === item.value}
            <button
              type="button"
              role="radio"
              aria-checked={active}
              class="relative z-10 flex h-7 cursor-pointer items-center justify-center gap-1.5 rounded-md text-xs font-medium transition-colors duration-200 outline-none focus-visible:ring-2 focus-visible:ring-ring {active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (theme.chatLayout = item.value as ChatLayout)}
            >
              <ZIcon zType={item.icon} class="size-3.5" />
              {item.label}
            </button>
          {/each}
        </div>
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
