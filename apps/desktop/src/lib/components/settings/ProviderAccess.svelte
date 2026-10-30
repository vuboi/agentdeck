<script lang="ts" module>
  export type ProviderOption = { id: string; label: string; disabled?: boolean; hint?: string };
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import { ZButton, ZSwitch, ZIcon, ZInput, ZPopover } from '@shival99/z-ui-svelte';
  import { toast } from '../../app/platform/toast';
  import { glide, TRIGGER_ON } from '../../app/ui/popover';
  import AgentLogo from '../ui/AgentLogo.svelte';

  let { name, selected, options, onsave }: {
    name: string; selected: string[]; options: ProviderOption[];
    onsave: (next: string[], initial: string[]) => Promise<boolean>;
  } = $props();
  let anchor = $state<HTMLElement | null>(null);
  let open = $state(false);
  let query = $state('');
  let draft = $state<string[]>([]);
  let initial = $state<string[]>([]);
  let saving = $state(false);
  let pending = $state<string[]>([]);
  let input = $state<ZInput>();
  const uid = $props.id();
  const visible = $derived(options.filter((o) => `${o.label} ${o.hint ?? ''}`.toLowerCase().includes(query.trim().toLowerCase())));
  const loading = $derived([...new Set([...pending, ...difference(draft, initial)])]);

  function difference(next: string[], previous: string[]) {
    return [...new Set([...next, ...previous])].filter((id) => next.includes(id) !== previous.includes(id));
  }

  function show() {
    if (saving) return;
    if (open) return;
    initial = [...selected];
    draft = [...selected];
    query = '';
    open = true;
  }

  /** Queue complete selections so rapid toggles cannot overwrite an earlier in-flight save. */
  function change(id: string, checked: boolean) {
    if (loading.includes(id) || options.find((option) => option.id === id)?.disabled) return;
    draft = checked ? [...new Set([...draft, id])] : draft.filter((value) => value !== id);
    void flush();
  }

  async function flush() {
    if (saving) return;
    saving = true;
    try {
      while (difference(draft, initial).length) {
        const next = [...draft];
        const previous = [...initial];
        pending = difference(next, previous);
        let ok = false;
        try { ok = await onsave(next, previous); }
        catch (error) { toast.error(String(error)); }
        await tick();
        const actual = ok ? next : [...selected];
        if (!ok) {
          // Reconcile failed/partially applied rows without discarding choices queued during this save.
          for (const id of pending) {
            if (draft.includes(id) === next.includes(id)) {
              draft = actual.includes(id) ? [...new Set([...draft, id])] : draft.filter((value) => value !== id);
            }
          }
        }
        initial = actual;
        pending = [];
      }
    } finally { pending = []; saving = false; }
  }

</script>

<span bind:this={anchor} class="shrink-0">
  <ZButton zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2 text-sm {open ? TRIGGER_ON : ''}" zDisabled={saving} aria-label={`Manage providers for ${name}`} aria-haspopup="dialog" aria-expanded={open} onclick={show}>
    {#each selected.slice(0, 3) as agent (agent)}<AgentLogo {agent} class="size-3.5" />{/each}
    <span>Providers · {selected.length}</span><ZIcon zType="lucideChevronDown" class="size-3" />
  </ZButton>
</span>

<ZPopover {anchor} bind:open zTrigger="manual" zPosition="bottom-left" zWidth={300} zCloseOnOutside={!saving} zCloseOnEscape={!saving} onOpen={async () => { await tick(); input?.focus(); }}>
  {#snippet children()}
    <div class="flex max-h-[min(420px,calc(100dvh_-_16px))] min-w-0 flex-col overflow-hidden text-sm" role="dialog" aria-label={`Providers for ${name}`} aria-busy={saving}>
      <div class="flex shrink-0 flex-col gap-2 border-b border-border p-2">
        <div class="flex items-center justify-between gap-2"><span class="font-medium">Providers</span><span class="text-muted-foreground">{draft.length} selected</span></div>
        <ZInput autofocus bind:this={input} bind:value={query} zPlaceholder="Find provider…" class="h-8" />
      </div>
      <div class="scrollbar-slim min-h-0 max-h-[min(320px,45dvh)] overflow-y-auto [mask-image:linear-gradient(to_bottom,transparent,black_5px,black_calc(100%_-_5px),transparent)]">
        <div class="p-1.5" {@attach glide()}>
        {#each [false, true] as locked}
          {@const rows = visible.filter((o) => Boolean(o.disabled) === locked)}
          {#if rows.length}
          <div class="flex flex-col gap-1.5 {locked && visible.some((option) => !option.disabled) ? 'mt-2' : ''}">
            {#each rows as option (option.id)}
              <div data-glide-item={option.disabled ? undefined : ''} class="flex min-h-10 items-center gap-2 rounded-md border border-border/60 px-2 py-1.5 {option.disabled ? 'opacity-60' : ''}">
                <label for={`${uid}-${option.id}`} class="flex min-w-0 flex-1 items-center gap-2 {option.disabled ? 'cursor-default' : 'cursor-pointer'}">
                  <AgentLogo agent={option.id} class="size-4 shrink-0" />
                  <span class="flex min-w-0 items-center gap-1.5"><span class="truncate font-medium">{option.label}</span>{#if option.hint}<span class="inline-flex shrink-0 rounded border border-border bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-foreground">{option.hint}</span>{/if}</span>
                </label>
                <ZSwitch id={`${uid}-${option.id}`} zVariant="dot" zSize="sm" zWave={false} checked={draft.includes(option.id)} zLoading={loading.includes(option.id)} zDisabled={option.disabled} onChange={(checked) => change(option.id, checked)} />
              </div>
            {/each}
          </div>
          {/if}
        {/each}
        {#if !visible.length}<p class="px-2 py-4 text-center text-muted-foreground">No provider matches</p>{/if}
        </div>
      </div>
    </div>
  {/snippet}
</ZPopover>
