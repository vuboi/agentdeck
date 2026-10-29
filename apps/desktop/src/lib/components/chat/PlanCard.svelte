<script lang="ts">
  import { ZIcon, ZLoading } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { PlanItem } from '../../chat/content/blocks';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';

  // The agent's task list (TodoWrite / update_plan): a step waiting, the one being worked on, and the ones done.
  let { items }: { items: PlanItem[] } = $props();
  const done = $derived(items.filter((item) => item.status === 'completed').length);
</script>

{#if items.length > 0}
  <div class="rounded-lg border border-border bg-muted/20 px-3 py-2.5 text-xs">
    <div class="mb-1.5 text-[11px] text-muted-foreground tabular-nums">{done} of {items.length} done</div>
    <ul class="flex flex-col gap-1.5">
      {#each items as item, index (index)}
        <li class="flex items-start gap-2 {item.status === 'completed' ? 'text-muted-foreground' : ''}">
          <span class="mt-px grid size-3.5 shrink-0 place-items-center">
            {#if item.status === 'completed'}
              <ZIcon zType={'lucideCircleCheck' as ZIconName} class="size-3.5 text-success" />
            {:else if item.status === 'in_progress'}
              <ZLoading zLoading zSpinner={LOADER.thinking} zSize={LOADER_SIZE} zColor="primary" />
            {:else}
              <span class="size-3 rounded-full border border-muted-foreground/50 border-dashed"></span>
            {/if}
          </span>
          <span class="min-w-0 {item.status === 'completed' ? 'line-through decoration-muted-foreground/40' : item.status === 'in_progress' ? 'font-medium' : ''}">{item.content}</span>
        </li>
      {/each}
    </ul>
  </div>
{/if}
