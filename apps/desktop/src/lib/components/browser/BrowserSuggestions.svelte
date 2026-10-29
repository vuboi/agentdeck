<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { Suggestion } from './browser-helpers';

  let { suggestions, picked = $bindable(-1), onchoose }: { suggestions: Suggestion[]; picked: number; onchoose: (item: Suggestion) => void } = $props();
</script>

<!-- Drawn by the app over the page (data-page-over puts the page below the app while this is open). -->
<div id="address-suggestions" data-page-over role="listbox" tabindex="-1" class="absolute top-full right-0 left-0 z-50 max-h-72 overflow-x-hidden overflow-y-auto rounded-t-none rounded-b-md border border-t-0 border-primary bg-popover p-1 text-popover-foreground shadow-lg" onmousedown={(event) => event.preventDefault()}>
  {#each suggestions as item, index (`${item.kind}|${item.url}`)}
    <div
      role="option"
      tabindex="-1"
      aria-selected={index === picked}
      class="flex min-w-0 cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm {index === picked ? 'bg-primary/15 text-primary' : 'hover:bg-primary/10'}"
      onmousemove={() => (picked = index)}
      onclick={() => onchoose(item)}
      onkeydown={(event) => event.key === 'Enter' && onchoose(item)}
    >
      <ZIcon zType={(item.kind === 'history' ? 'lucideClock' : item.kind === 'go' ? 'lucideGlobe' : 'lucideSearch') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
      {#if item.kind === 'search'}
        <span class="min-w-0 flex-1 truncate">Search Google for "{item.text}"</span>
      {:else if item.kind === 'go'}
        <span class="min-w-0 flex-1 truncate">Go to {item.text}</span>
      {:else if item.kind === 'history'}
        <span class="min-w-0 flex-1 truncate">{item.title || item.text.replace(/^https?:\/\//, '')}</span>
        {#if item.title}<span class="max-w-[50%] shrink truncate text-muted-foreground">{item.text.replace(/^https?:\/\/(www\.)?/, '')}</span>{/if}
      {:else}
        <span class="min-w-0 flex-1 truncate">{item.text}</span>
      {/if}
    </div>
  {/each}
</div>
