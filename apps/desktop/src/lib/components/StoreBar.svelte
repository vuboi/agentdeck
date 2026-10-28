<script lang="ts">
  import StoreHeader from './StoreHeader.svelte';
  import type { Snippet } from 'svelte';
  import { ZInput } from '@shival99/z-ui-svelte';

  // The sub-header of every Store tab, one height for all Store tabs: what is listed can be searched, with the tab's own
  // controls beside the search box. (Add and refresh are in the Store's top row: see store-actions.svelte.ts.)
  let {
    query = $bindable(''),
    placeholder,
    lead,
    filters,
  }: {
    query?: string;
    placeholder: string;
    /** Before the search box (the Installed tab's kind switch). */
    lead?: Snippet;
    /** After the search box (Discover's two filters). */
    filters?: Snippet;
  } = $props();
</script>

<StoreHeader>
    {@render lead?.()}
    <div class="min-w-0 flex-[7_1_0%]"><ZInput bind:value={query} zPlaceholder={placeholder} class="h-8 w-full min-w-0" /></div>
    {#if filters}<div class="flex min-w-0 flex-[3_1_0%] items-center gap-2">{@render filters()}</div>{/if}
</StoreHeader>
