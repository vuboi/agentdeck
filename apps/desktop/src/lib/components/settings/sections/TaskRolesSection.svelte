<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZIcon, ZSelect, ZSwitch, ZTabs, zSortable } from '@shival99/z-ui-svelte';
  import type { ZIconName, ZTab } from '@shival99/z-ui-svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { modelSources, parseCustomModel } from '../../../agents/models.svelte';
  import { agentDisplayName } from '../../../agents/provider-registry';
  import { TASK_ROLES, routesOf, type TaskRoleId } from '../../../agents/task-roles';
  import { roleOpen, setRoleOpen, taskRoles, tasksTab } from '../../../agents/task-roles.svelte';
  import ContinueSection from './ContinueSection.svelte';

  const icon = (name: string) => name as ZIconName;
  /** The two parts of the Tasks menu: how a failed agent is replaced, and which agent does which work. */
  const TABS: ZTab[] = [
    { value: 'fallback', label: 'Fallback and continue' },
    { value: 'roles', label: 'Task roles' },
  ];

  /** Reading pictures: the Console-credit Claude models, Copilot, OpenCode, Antigravity and the connectors. */
  const READING = ['claude', 'copilot', 'opencode', 'antigravity', 'custom'];
  /** Hand-offs with delegate_task: the plan Claude models, Codex, Antigravity and the connectors. */
  const ROUTING = ['claude', 'codex', 'antigravity', 'custom'];

  type Node = { value: string; label: string; agent: string; model?: string; children?: Node[] };

  /** The tree a row's select draws: one branch per provider, its models as leaves. A leaf's value is the route. */
  function treeFor(id: TaskRoleId): Node[] {
    // The read role reads pictures, so its agents are the ones that can (the pre-handle's list).
    const reading = id === 'read';
    const allowed = reading ? READING : ROUTING;
    return modelSources().flatMap((source): Node[] => {
      if (!allowed.includes(source.provider)) return [];
      const children = source.models().flatMap((item): Node[] => {
        if (source.provider === 'custom') {
          const target = parseCustomModel(item.value);
          return target ? [{ value: `${target.providerId}|${target.model}`, label: item.label, agent: target.providerId, model: target.model }] : [];
        }
        // The pre-handle reads with the Console-credit Claude models; hand-offs take the plan models.
        if (source.provider === 'claude' && reading !== item.value.startsWith('credit:')) return [];
        return [{ value: `${source.provider}|${item.value}`, label: item.label, agent: source.provider, model: item.value }];
      });
      return children.length ? [{ value: `source:${source.key}`, label: source.label, agent: source.provider, children }] : [];
    });
  }

  // The rows of a role are its routes in priority order; a row with no model yet is ''.
  const rowsOf = (id: TaskRoleId): string[] => taskRoles.routes[id] ?? [];
  const hasRows = (id: TaskRoleId) => rowsOf(id).length > 0;
  function setRow(id: TaskRoleId, index: number, value: string | null) {
    const route = value && !value.startsWith('source:') ? value : '';
    const list = [...rowsOf(id)];
    // A model is in one row only: picking it again moves it here.
    const next = list.map((item, at) => (at !== index && route && item === route ? '' : item));
    next[index] = route;
    taskRoles.routes[id] = next;
  }
  // At most this many agents per role, and a new row only once the last one has a model.
  const MAX_AGENTS = 5;
  const canAdd = (id: TaskRoleId) => rowsOf(id).length < MAX_AGENTS && rowsOf(id).at(-1) !== '';
  function addRow(id: TaskRoleId) {
    if (!canAdd(id)) return;
    taskRoles.routes[id] = [...rowsOf(id), ''];
  }
  function removeRow(id: TaskRoleId, index: number) {
    taskRoles.routes[id] = rowsOf(id).filter((_, at) => at !== index);
  }
  function moveRow(id: TaskRoleId, from: number, to: number) {
    if (from === to) return;
    const list = [...rowsOf(id)];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    taskRoles.routes[id] = list;
  }


</script>

<div class="flex h-full min-h-0 min-w-0 flex-col" in:fade={{ duration: 200 }}>
  <!-- The tab strip stays put; only the body below it scrolls. -->
  <div class="shrink-0 min-w-0 border-b border-border px-3.5 pt-2 pb-2 [&_.z-tab-item]:items-center [&_.z-tab-item>span:first-child]:flex [&_.z-tab-item>span:first-child]:items-center">
    {#snippet tabIcon(tab: ZTab)}
      <ZIcon zType={icon(tab.value === 'fallback' ? 'lucideRoute' : 'lucideWorkflow')} class="size-3.5" />
    {/snippet}
    <ZTabs zTabs={TABS.map((tab) => ({ ...tab, iconTemplate: tabIcon }))} zType="bar" zSize="sm" zTabsOnly zShowNavWrapper zCache={false} activeTab={tasksTab.value} onTabChange={(value) => (tasksTab.value = String(value))} />
  </div>
  <div class="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto p-3.5 [scrollbar-gutter:stable]">
  <div class="flex flex-col gap-3">
  {#if tasksTab.value === 'fallback'}
    <ContinueSection />
  {:else}
  <div class="flex items-center gap-3 rounded-xl border border-border bg-card/40 px-4 py-3">
    <div class="min-w-0 flex-1">
      <div class="text-sm font-medium">All roles</div>
      <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Off: no role is used, so work is not handed to other agents and the main agent reads the pictures. The picks are kept.</p>
    </div>
    <ZSwitch zVariant="dot" zSize="sm" checked={taskRoles.on} onChange={(value) => (taskRoles.on = value)} />
  </div>

  <p class="text-[0.75rem] text-muted-foreground">
    Each role lists the agents it may use, highest priority first; drag the handle to reorder. Pictures are read by the first
    available model in that order, and the main agent reads them when none is. Other work is handed to the agents listed,
    and the main agent chooses among them. Hand-offs happen only while Auto-orchestrate is on in the model menu.
  </p>

  <div class="flex flex-col gap-3">
    {#each TASK_ROLES as role (role.id)}
      {@const rows = rowsOf(role.id)}
      {@const picked = rowsOf(role.id).filter((route) => route !== '')}
      {@const on = roleOpen(role.id)}
      <section class="flex min-w-0 flex-col overflow-hidden rounded-xl border border-border bg-card/40">
        <header class="flex h-[4.5rem] items-start gap-3 p-4">
          <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={icon(role.icon)} class="size-4" /></span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2 text-sm font-medium">
              <span class="truncate">{role.label}</span>
              {#if picked.length}<span class="shrink-0 rounded-full bg-primary/12 px-1.5 text-[0.6875rem] font-normal text-primary tabular-nums">{picked.length}</span>{/if}
            </div>
            <p class="mt-0.5 truncate text-[0.75rem] text-muted-foreground" title={role.hint}>{role.hint}</p>
          </div>
          <ZSwitch zVariant="dot" zSize="sm" checked={on} zDisabled={!taskRoles.on} onChange={(value) => setRoleOpen(role.id, value)} />
        </header>

          <!-- The body grows from 0 to its height (the same grid-row motion as the thinking block). The list scrolls inside
               the card: the scrollbar sits on the card's own edge, the rows are padded inside it. -->
          <div class="grid transition-[grid-template-rows] duration-300 ease-out {on ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
          <div class="min-h-0 overflow-hidden">
          <div class="max-h-72 overflow-y-auto border-t border-border">
            <div class="flex flex-col gap-2 p-3">
              <ol class="flex flex-col gap-2.5"{@attach zSortable({ onSort: (from, to) => moveRow(role.id, from, to), handle: '[data-handle]' })}>
                {#each rows as row, index (index)}
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
                      zPlaceholder={role.id === 'read' ? 'Pick a model that reads pictures' : 'Pick an agent and model'}
                      zPosition="bottom-right"
                      zOptions={treeFor(role.id)}
                      value={row || null}
                      onValueChange={(value) => setRow(role.id, index, value === null ? null : String(value))}
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
                      onclick={() => removeRow(role.id, index)}
                    >
                      <ZIcon zType={icon('lucideX')} class="size-3.5" />
                    </button>
                  </li>
                {/each}
              </ol>
              <button
                type="button"
                class="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-dashed border-border px-2.5 text-sm text-muted-foreground outline-none hover:border-primary hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 disabled:hover:border-border disabled:hover:text-muted-foreground"
                disabled={!canAdd(role.id)}
                title={rows.length >= MAX_AGENTS ? `Up to ${MAX_AGENTS} agents` : canAdd(role.id) ? '' : 'Pick a model for the empty row first'}
                onclick={() => addRow(role.id)}
              >
                <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add agent
              </button>
            </div>
          </div>
          </div>
          </div>
      </section>
    {/each}
  </div>
  {/if}
  </div>
  </div>
</div>
