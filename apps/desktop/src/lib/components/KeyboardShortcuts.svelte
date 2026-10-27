<script lang="ts">
  import { ZIcon, ZModal } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { SHORTCUT_GROUPS } from '../shortcuts';

  // Ctrl+/ (and Ctrl+?): every shortcut of the app, grouped, with a search box that filters by name or key.
  let { open = $bindable(false) }: { open?: boolean } = $props();

  let query = $state('');
  let input: HTMLInputElement | undefined = $state();
  const groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return SHORTCUT_GROUPS;
    return SHORTCUT_GROUPS.map((group) => ({
      ...group,
      items: group.items.filter((item) => item.label.toLowerCase().includes(q) || item.keys.some((combo) => combo.join('+').toLowerCase().includes(q))),
    })).filter((group) => group.items.length > 0);
  });

  $effect(() => {
    if (!open) return;
    query = '';
    requestAnimationFrame(() => input?.focus());
  });
</script>

<ZModal bind:open zTitle="Keyboard shortcuts" zWidth="520px" zHideFooter zBodyClass="p-0!">
  <div class="flex max-h-[70vh] flex-col">
    
    <label class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-4">
      <ZIcon zType={'lucideSearch' as ZIconName} class="size-4 shrink-0 text-muted-foreground" />
      <input bind:this={input} bind:value={query} class="h-full min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground" placeholder="Search shortcuts" aria-label="Search shortcuts" spellcheck="false" />
    </label>
    <div class="scrollbar-full min-h-0 flex-1 overflow-y-auto pt-3 pr-1 pb-4 pl-4">
      {#each groups as group (group.title)}
        <section class="mb-4 last:mb-0">
          <h3 class="mb-1 text-sm font-semibold">{group.title}</h3>
          <ul class="divide-y divide-border/60">
            {#each group.items as item (item.label)}
              <li class="flex min-h-9 items-center gap-3 py-1.5 text-sm">
                <span class="min-w-0 flex-1 text-muted-foreground">{item.label}</span>
                <span class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
                  {#each item.keys as combo, index (index)}
                    {#if index > 0}<span class="text-xs text-muted-foreground">{item.then ? 'then' : 'or'}</span>{/if}
                    <span class="flex items-center gap-0.5">
                      {#each combo as cap, n (n)}<kbd class="min-w-6 rounded border border-border bg-muted px-1.5 py-0.5 text-center font-sans text-[11px] leading-4 text-foreground shadow-[inset_0_-1px_0_var(--border)]">{cap}</kbd>{/each}
                    </span>
                  {/each}
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <p class="py-8 text-center text-sm text-muted-foreground">No shortcut matches “{query}”.</p>
      {/each}
    </div>
  </div>
</ZModal>
