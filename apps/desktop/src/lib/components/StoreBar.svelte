<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ZInput } from '@shival99/z-ui-svelte';

  // The sub-header of every Store tab, one height for all four: what is listed can be searched, with the tab's own
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

<div class="@container shrink-0 border-b border-border">
  <div class="flex h-10 items-center gap-2 px-1.5">
    {@render lead?.()}
    <ZInput bind:value={query} zPlaceholder={placeholder} class="h-8 min-w-24 flex-1" />
    {@render filters?.()}
  </div>
</div>
