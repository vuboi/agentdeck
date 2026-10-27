<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { checkUpdates, store, updateSkill, updates, withAck } from '../store.svelte';

  const icon = (name: string) => name as ZIconName;

  // Library skills that came from a git source, compared with that source. An update is scanned again before it replaces
  // the library copy; a caution asks first, a block stops the update.
  onMount(() => {
    if (!updates.checked) void checkUpdates();
  });

  const update = (name: string) => withAck((acknowledged) => updateSkill(name, acknowledged), `The new version of ${name}`);
</script>

<div class="flex flex-col gap-3 text-xs">
  <div class="flex items-center gap-2">
    <span class="text-muted-foreground">{updates.items.length} skills from git sources</span>
    <ZButton zWave={false} zType="outline" zSize="sm" class="ml-auto h-7 gap-1.5 px-2.5 text-xs" zLoading={updates.checking} onclick={() => void checkUpdates()}>
      <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {updates.checking ? 'animate-spin' : ''}" /> Check for updates
    </ZButton>
  </div>

  <ul class="m-0 flex list-none flex-col gap-2 p-0">
    {#each updates.items as item (item.name)}
      <li class="flex items-center gap-2 rounded-md border border-border bg-card p-2.5">
        <div class="min-w-0 flex-1">
          <div class="truncate font-medium text-foreground">{item.name}</div>
          <div class="truncate font-mono text-[11px] text-muted-foreground">{item.source}</div>
          {#if item.error}<div class="text-destructive">{item.error}</div>{/if}
        </div>
        {#if item.changed}
          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" zLoading={store.busy === `update:${item.name}`} onclick={() => void update(item.name)}>
            Update
          </ZButton>
        {:else if !item.error}
          <span class="text-muted-foreground">up to date</span>
        {/if}
      </li>
    {:else}
      <li class="text-muted-foreground">{updates.checked ? 'No skill from a git source is installed.' : 'Checking…'}</li>
    {/each}
  </ul>
</div>
