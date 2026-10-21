<script lang="ts">
  import { onMount } from 'svelte';

  const tabs = ['Preview', 'Diff', 'Terminal', 'Workflow', 'Usage', 'Audit'] as const;
  let active = $state<(typeof tabs)[number]>('Preview');
  let width = $state(350);
  let collapsed = $state(false);
  let dragging = $state(false);
  let resizeHandle = $state<HTMLDivElement | undefined>();

  onMount(() => {
    clampWidth(width);
  });

  $effect(() => {
    const root = document.documentElement;
    root.classList.toggle('panel-resizing', dragging);
    return () => root.classList.remove('panel-resizing');
  });

  function maxWidth() {
    return Math.max(0, window.innerWidth - 260 - 480);
  }

  function clampWidth(nextWidth: number) {
    width = Math.min(maxWidth(), Math.max(0, nextWidth));
  }

  function startResize(event: PointerEvent) {
    if (!resizeHandle) return;
    dragging = true;
    resizeHandle.setPointerCapture(event.pointerId);
    resizeFromPointer(event);
  }

  function resizeFromPointer(event: PointerEvent) {
    if (dragging) clampWidth(window.innerWidth - event.clientX);
  }

  function endResize() {
    dragging = false;
  }

  function onTabKeydown(event: KeyboardEvent) {
    if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const index = tabs.indexOf(active);
    if (event.key === 'Home') active = tabs[0];
    else if (event.key === 'End') active = tabs[tabs.length - 1];
    else active = tabs[(index + (event.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length];
    document.getElementById(`tab-${active}`)?.focus();
  }
</script>

<svelte:window onresize={() => clampWidth(width)} />

{#if !collapsed}
  <aside class="right-panel" class:dragging style={`width: ${width}px`} aria-label="Tools panel">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="resize-handle"
      class:dragging
      bind:this={resizeHandle}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize tools panel"
      aria-valuenow={width}
      aria-valuemin="0"
      aria-valuemax={maxWidth()}
      tabindex="0"
      onpointerdown={startResize}
      onpointermove={resizeFromPointer}
      onpointerup={endResize}
      onpointercancel={endResize}
      onlostpointercapture={endResize}
      onkeydown={(event) => {
        if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
        event.preventDefault();
        clampWidth(width + (event.key === 'ArrowLeft' ? 16 : -16));
      }}
    ></div>
    <div class="panel-heading"><span>Workspace</span><button aria-label="Collapse tools panel" onclick={() => (collapsed = true)}>›</button></div>
    <div class="panel-tabs" role="tablist" aria-label="Workspace tools">
      {#each tabs as tab}
        <button role="tab" aria-selected={active === tab} id={`tab-${tab}`} aria-controls="tool-panel" tabindex={active === tab ? 0 : -1} onclick={() => (active = tab)} onkeydown={onTabKeydown}>{tab}</button>
      {/each}
    </div>
    <div id="tool-panel" class="panel-empty" role="tabpanel" aria-labelledby={`tab-${active}`}><div class="empty-icon">{active === 'Preview' ? '▧' : '⌁'}</div><h2>{active}</h2><p>{active === 'Preview' ? 'Your browser preview will appear here.' : `No ${active.toLowerCase()} data yet.`}</p></div>
  </aside>
{:else}
  <button class="expand-panel" aria-label="Expand tools panel" onclick={() => (collapsed = false)}>‹</button>
{/if}
