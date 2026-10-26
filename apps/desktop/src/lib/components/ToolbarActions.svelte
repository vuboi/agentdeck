<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { layout, toggleThemePanel, toggleTools } from '../layout.svelte';
  import { terminal, toggleTerminal } from '../terminal.svelte';

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

<div class="flex shrink-0 items-center gap-1">
  {#if divider}
    <!-- Short divider: separates the preceding actions from terminal + palette + collapse. -->
    <span class="mx-1.5 h-4 w-px bg-border" aria-hidden="true"></span>
  {/if}
  <ZButton
    zWave={false}
    zType="ghost"
    zSize="sm"
    class="size-7! {terminal.open ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
    aria-label={terminal.open ? 'Hide terminal' : 'Show terminal'}
    aria-pressed={terminal.open}
    title={terminal.open ? 'Hide terminal (Ctrl+`)' : 'Show terminal (Ctrl+`)'}
    onclick={toggleTerminal}
  >
    <ZIcon zType="lucideSquareTerminal" class="size-4" />
  </ZButton>
  <ZButton
    zWave={false}
    zType="ghost"
    zSize="sm"
    class="size-7! {layout.themePanelOpen ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
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
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label={layout.toolsOpen ? 'Hide tools panel' : 'Show tools panel'} title={layout.toolsOpen ? 'Hide tools' : 'Show tools'} onclick={toggleTools}>
    <ZIcon zType={'lucidePanelRight'} class="size-4" />
  </ZButton>
</div>
