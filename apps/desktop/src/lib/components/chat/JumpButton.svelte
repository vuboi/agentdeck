<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ZButton, ZIcon, ZLoading } from '@shival99/z-ui-svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';

  // The "Scroll to latest" button: only while scrolled up, at the bottom there is nothing to jump to, running or not.
  let { show, running, onclick }: { show: boolean; running: boolean; onclick: () => void } = $props();
</script>

{#if show}
  <!-- The wrapper carries the entrance (it rises into place while it fades in, and goes the same way); the button inside keeps its own look. -->
  <div transition:fly={{ y: 10, duration: 200 }} class="pointer-events-none absolute inset-x-0 bottom-3 z-20 flex justify-center">
    <ZButton
      zWave={false}
      zType="outline"
      class="group/jump pointer-events-auto size-8 rounded-full bg-card! p-0 shadow-md"
      aria-label="Scroll to latest"
      title="Scroll to latest"
      {onclick}
    >
      <!-- Own grid so the spinner and the arrow sit dead centre; the button's inline content box drifted down-right. -->
      <span class="grid size-full place-items-center">
        {#if running}
          <!-- Both sit in the same cell and trade places on hover: the spinner fades and shrinks out, the arrow grows in. -->
          <span class="col-start-1 row-start-1 grid place-items-center transition duration-200 ease-out group-hover/jump:scale-50 group-hover/jump:opacity-0 motion-reduce:transition-none"><ZLoading zLoading zSpinner={LOADER.scroll} zSize={LOADER_SIZE} zColor="primary" /></span>
          <ZIcon zType="lucideArrowDown" class="col-start-1 row-start-1 size-4 scale-50 opacity-0 transition duration-200 ease-out group-hover/jump:scale-100 group-hover/jump:opacity-100 motion-reduce:transition-none" />
        {:else}
          <ZIcon zType="lucideArrowDown" class="size-4" />
        {/if}
      </span>
    </ZButton>
  </div>
{/if}
