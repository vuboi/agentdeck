<script lang="ts">
  import { ZPortal, Z_OVERLAY_BASE_Z_INDEX, zOverlayStack } from '@shival99/z-ui-svelte';
  import { fly } from 'svelte/transition';
  import { closePeek, holdPeek, layout, leavePeek, peekMotion } from '../layout.svelte';
  import Sidebar from './Sidebar.svelte';

  // The folded sidebar, floating over the app (a hover on the folded sidebar's button, or a click in a narrow window).
  // Not a drawer: no backdrop, no scroll lock (that one shrank the page's scrollbar gutter), nothing made inert, so the
  // button that pins the sidebar stays clickable under the pointer. Just under the overlay layer: the sidebar's own
  // menus and popovers (rendered there) open above it.

  let panel = $state<HTMLElement>();
  const width = $derived(layout.sidebarHidden ? Math.min(320, Math.round(window.innerWidth * 0.86)) : layout.sizes.sidebar);

  // A modal or drawer opening (Settings, a confirm) puts the peek away.
  $effect(() => zOverlayStack.onOpen(closePeek));

  // A press outside the panel (and outside the button that opened it) puts it away; Escape too.
  function onPointerDown(event: PointerEvent) {
    if (!layout.sidebarDrawer || !panel) return;
    const target = event.target as Element | null;
    if (panel.contains(target) || target?.closest('[data-sidebar-rail], #z-overlay-root')) return;
    closePeek();
  }
  // Into one of the sidebar's own menus (in the overlay layer, outside the panel) is not leaving it.
  function onLeave(event: PointerEvent) {
    const into = event.relatedTarget as Element | null;
    if (layout.sidebarHidden || into?.closest('#z-overlay-root')) return;
    leavePeek();
  }
  function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape' && layout.sidebarDrawer && !event.defaultPrevented) closePeek();
  }
</script>

<svelte:window onpointerdown={onPointerDown} onkeydown={onKey} />

{#if layout.sidebarDrawer}
  <ZPortal>
    <aside
      bind:this={panel}
      class="fixed inset-y-0 left-0 flex min-h-0 flex-col overflow-hidden border-r border-border bg-background shadow-2xl [&>*]:min-h-0 [&>*]:flex-1"
      style:z-index={Z_OVERLAY_BASE_Z_INDEX - 1}
      style:width="{width}px"
      aria-label="Sessions"
      data-sidebar-peek
      onpointerenter={holdPeek}
      onpointerleave={onLeave}
      transition:fly={{ x: -12, duration: peekMotion.instant ? 0 : 160 }}
    >
      <Sidebar />
    </aside>
  </ZPortal>
{/if}
