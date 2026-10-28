<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
  import { TRIGGER_ON_PLAIN } from '../popover';

  // The project filter of the Tasks, Usage and Workflow tools: a select (the button) over the app's one list
  // (`PickerPopover`, with its search box). `value` null is "All projects"; '' is a real choice (the chats outside any
  // project).
  let {
    value = $bindable(null),
    options,
    label,
    allLabel = 'All projects',
    class: className = '',
  }: { value?: string | null; options: string[]; label: (option: string) => string; allLabel?: string; class?: string } = $props();

  let trigger = $state<HTMLElement | null>(null);
  let open = $state(false);
  let query = $state('');

  // A project that left the list (nothing of it in the range any more) reads as "All projects", as the views filter.
  const current = $derived(value !== null && options.includes(value) ? value : null);
  const items = $derived<PickerItem<string | null>[]>([
    { id: '::all', label: allLabel, icon: 'lucideFolders' as ZIconName, selected: current === null, data: null },
    ...options.map((option) => ({ id: `project:${option}`, label: label(option), icon: 'lucideFolder' as ZIconName, selected: current === option, data: option })),
  ]);

  function pick(item: PickerItem<string | null>) {
    value = item.data;
    open = false;
    query = '';
  }
</script>

<button
  bind:this={trigger}
  type="button"
  class="inline-flex h-7 min-w-36 max-w-48 cursor-pointer items-center justify-between gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs outline-none hover:bg-primary/10 {open ? `border-primary/50 ${TRIGGER_ON_PLAIN}` : ''} {className}"
  aria-label="Project"
  aria-haspopup="listbox"
  aria-expanded={open}
  onclick={() => (open = !open)}
>
  <span class="truncate">{current === null ? allLabel : label(current)}</span>
  <ZIcon zType={'lucideChevronDown' as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
</button>

<PickerPopover anchor={trigger} bind:open bind:query {items} local search placeholder="Search projects…" label="Project" empty="No project matches" position="bottom-right" size="sm" width={220} onpick={pick} onclose={() => (query = '')} />
