<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZIcon, ZSelect, ZSwitch, zSortable } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { connectors } from '../../../agents/connectors.svelte';
  import { modelSources } from '../../../agents/models.svelte';
  import { agentDisplayName } from '../../../agents/provider-registry';
  import { theme } from '../../../app/ui/theme.svelte';

  const icon = (name: string) => name as ZIconName;

  /** The agents a chat can move to: the ones with a CLI (events.svelte.ts CLI_OF). */
  const CHAIN_AGENTS = ['claude', 'codex', 'antigravity', 'opencode', 'copilot'];

  type Node = { value: string; label: string; agent: string; model?: string; children?: Node[] };

  /** The tree a row's select draws: one branch per agent, its models as leaves (a leaf's value is `agent|model`). */
  const tree = $derived.by((): Node[] =>
    modelSources().flatMap((source): Node[] => {
      if (!CHAIN_AGENTS.includes(source.provider)) return [];
      const children = source.models().flatMap((item): Node[] => {
        // The fallback runs on the plan's models, not on the Console credit.
        if (source.provider === 'claude' && item.value.startsWith('credit:')) return [];
        return [{ value: `${source.provider}|${item.value}`, label: item.label, agent: source.provider, model: item.value }];
      });
      return children.length ? [{ value: `source:${source.key}`, label: source.label, agent: source.provider, children }] : [];
    }),
  );

  const agentOf = (row: string) => row.split('|')[0];
  /** A row that names only an agent shows that agent's first model, as the fallback uses it. */
  function resolved(row: string): string | null {
    if (row.includes('|')) return row;
    const source = tree.find((node) => node.agent === row && node.children?.length);
    return source?.children?.[0]?.value ?? null;
  }

  function moveChain(from: number, to: number) {
    if (from === to) return;
    const list = [...connectors.recovery.chain];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    connectors.recovery.chain = list;
  }
  function setRow(index: number, value: string | null) {
    const route = value && !value.startsWith('source:') ? value : '';
    const list = [...connectors.recovery.chain];
    // A model is in one row only: picking it again moves it here.
    const next = list.map((item, at) => (at !== index && route && item === route ? '' : item));
    next[index] = route;
    connectors.recovery.chain = next;
  }
  function removeRow(index: number) {
    connectors.recovery.chain = connectors.recovery.chain.filter((_, at) => at !== index);
  }
  function addRow() {
    connectors.recovery.chain = [...connectors.recovery.chain, ''];
  }
  const canAdd = $derived(connectors.recovery.chain.at(-1) !== '');
</script>

<div class="flex flex-col gap-3" in:fade={{ duration: 200 }}>
  <section class="rounded-xl border border-border bg-card/40 p-4">
    <div class="flex items-start gap-3">
      <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={icon('lucideRoute')} class="size-4" /></span>
      <div class="min-w-0 flex-1">
        <div class="text-sm font-medium">Keep working when an agent fails</div>
        <p class="mt-0.5 text-[0.75rem] text-muted-foreground">A passing error is tried again. At a usage limit or a sign-out the next agent below carries on from where it stopped. Accounts are never switched automatically: switch them yourself.</p>
      </div>
      <ZSwitch zVariant="dot" checked={connectors.recovery.enabled} zSize="sm" onChange={(value) => (connectors.recovery = { ...connectors.recovery, enabled: value, chosen: true })} />
    </div>
    <div class="mt-3 flex items-start gap-3 rounded-lg border border-border px-3 py-2.5">
      <div class="min-w-0 flex-1">
        <div class="text-sm font-medium">Auto-continue when a limit resets</div>
        <p class="mt-0.5 text-[0.75rem] text-muted-foreground">When a chat stops at a usage limit and no other agent can take over, it carries on by itself once the limit resets (the time is read from the error or the account's usage). A card above the message box shows the time and lets you cancel or try again now.</p>
      </div>
      <ZSwitch zVariant="dot" checked={connectors.recovery.autoResume} zSize="sm" onChange={(value) => (connectors.recovery.autoResume = value)} />
    </div>

    <div class="mt-3 flex flex-col gap-2 {connectors.recovery.enabled ? '' : 'pointer-events-none opacity-50'}">
      <div class="text-[0.75rem] text-muted-foreground">Agents to carry on with, in order. Drag the handle to reorder; each row takes one model.</div>
      <ol class="flex flex-col gap-2.5" aria-label="Order of agents to carry on with" {@attach zSortable({ onSort: moveChain, handle: '[data-handle]' })}>
        {#each connectors.recovery.chain as row, index (index)}
          <li class="flex items-center gap-1.5 rounded-lg border border-border bg-background/60 py-0.5 pl-1 pr-1.5 transition-colors hover:border-primary/40">
            <span data-handle class="grid size-6 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground hover:text-foreground active:cursor-grabbing" aria-label="Drag to reorder">
              <ZIcon zType={icon('lucideGripVertical')} class="size-3.5" />
            </span>
            <ZSelect
              class="min-w-0 flex-1 text-sm"
              zVariant="ghost"
              zSize="sm"
              zTree
              zTreeRootLabel="All agents"
              zShowSearch
              zPlaceholderSearch="Search models…"
              zPlaceholder="Pick an agent and model"
              zPosition="bottom-right"
              zOptions={tree}
              value={resolved(row)}
              onValueChange={(value) => setRow(index, value === null ? null : String(value))}
            >
              {#snippet option(item)}
                <span class="flex min-w-0 flex-1 items-center gap-2"><AgentLogo agent={item.agent} model={item.children ? undefined : item.model} class="size-4 shrink-0" /><span class="truncate">{item.label}</span>{#if !item.children}<span class="ml-auto shrink-0 text-[0.6875rem] text-muted-foreground">{agentDisplayName(item.agent)}</span>{/if}</span>
              {/snippet}
              {#snippet selection(item)}
                <span class="flex min-w-0 items-center gap-2"><AgentLogo agent={item.agent} model={item.model} class="size-4 shrink-0" /><span class="truncate font-medium">{item.label}</span><span class="shrink-0 truncate text-[0.6875rem] text-muted-foreground">{agentDisplayName(item.agent)}</span></span>
              {/snippet}
            </ZSelect>
            <button
              type="button"
              class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring"
              aria-label="Remove this agent"
              onclick={() => removeRow(index)}
            >
              <ZIcon zType={icon('lucideX')} class="size-3.5" />
            </button>
          </li>
        {/each}
      </ol>
      <button
        type="button"
        class="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-dashed border-border px-2.5 text-sm text-muted-foreground outline-none hover:border-primary hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
        disabled={!canAdd}
        title={canAdd ? '' : 'Pick a model for the empty row first'}
        onclick={addRow}
      >
        <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add agent
      </button>
    </div>
  </section>

  <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
    <div class="min-w-0 flex-1">
      <div class="text-sm font-medium">Separate replies</div>
      <p class="mt-0.5 text-[0.75rem] text-muted-foreground">A thin line between two replies of the agent, and where a session resumed, compacted or was taken over by another agent.</p>
    </div>
    <ZSwitch zVariant="dot" checked={theme.turnDividers} zSize="sm" onChange={(value) => (theme.turnDividers = value)} />
  </section>
</div>
