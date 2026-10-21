<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { layout, toggleThemePanel, toggleTools } from '../layout.svelte';

  // The divider only makes sense when other actions come before this group.
  let { divider = true }: { divider?: boolean } = $props();

  let trigger: HTMLElement | null = $state(null);
  $effect(() => {
    const element = trigger;
    layout.themeAnchor = element;
    return () => {
      if (layout.themeAnchor === element) layout.themeAnchor = null;
    };
  });
</script>

<div class="flex shrink-0 items-center gap-0.5">
  {#if divider}
    <!-- Short divider: separates the preceding actions from palette + collapse. -->
    <span class="mx-1.5 h-4 w-px bg-border" aria-hidden="true"></span>
  {/if}
  <ZButton
    zWave={false}
    zType={layout.themePanelOpen ? 'subtle' : 'ghost'}
    zSize="sm"
    class="h-7 w-7"
    id="theme-trigger"
    bind:ref={trigger}
    aria-label="Theme colors"
    aria-haspopup="dialog"
    aria-expanded={layout.themePanelOpen}
    title="Theme colors"
    onclick={toggleThemePanel}
  >
    <ZIcon zType="lucidePalette" class="size-4" />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7" aria-label={layout.toolsOpen ? 'Hide tools panel' : 'Show tools panel'} title={layout.toolsOpen ? 'Hide tools' : 'Show tools'} onclick={toggleTools}>
    <ZIcon zType={layout.toolsOpen ? 'lucidePanelRightClose' : 'lucidePanelRightOpen'} class="size-4" />
  </ZButton>
</div>
