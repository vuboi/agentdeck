<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZCommand, ZIcon, ZSelect } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { activeSession, AGENTS, MODELS, projectOf, workspace, type Agent } from '../sessions.svelte';

  type Mention = { kind: 'session' | 'project' | 'ai'; id: string; label: string };

  let draft = $state('');
  let area: HTMLTextAreaElement | undefined = $state();
  let mentions = $state<Mention[]>([]);
  let paletteOpen = $state(false);

  // Agent + model of this message. They follow the active session and can be overridden with @AI.
  let agent = $state<Agent>('claude');
  let model = $state('sonnet-5-5');
  $effect(() => {
    const session = activeSession();
    if (!session) return;
    untrack(() => {
      agent = session.agent;
      model = session.model;
    });
  });

  const agentIcon: Record<Agent, ZIconName> = {
    claude: 'lucideSparkles' as ZIconName,
    codex: 'lucideTerminal' as ZIconName,
    gemini: 'lucideGem' as ZIconName,
  };
  const agentOptions = AGENTS.map((item) => ({ label: item.label, value: item.value, icon: agentIcon[item.value] }));
  const modelOptions = $derived(MODELS[agent].map((item) => ({ label: item.label, value: item.value })));

  function onAgentChange(value: unknown) {
    agent = value as Agent;
    if (!MODELS[agent].some((item) => item.value === model)) model = MODELS[agent][0].value;
  }

  // ---------- mentions ----------
  const groups = $derived<ZCommandGroup<Mention>[]>([
    {
      heading: 'Sessions',
      items: workspace.sessions
        .filter((item) => item.id !== workspace.activeId)
        .map((item) => ({
          id: `session:${item.id}`,
          label: item.title,
          icon: (item.parentId ? 'lucideGitFork' : 'lucideMessageSquare') as ZIconName,
          keywords: [projectOf(item)?.name ?? '', item.branch, item.agent],
          data: { kind: 'session' as const, id: item.id, label: item.title },
        })),
    },
    {
      heading: 'Projects',
      items: workspace.projects.map((item) => ({
        id: `project:${item.id}`,
        label: item.name,
        icon: 'lucideFolder' as ZIconName,
        keywords: [item.path],
        data: { kind: 'project' as const, id: item.id, label: item.name },
      })),
    },
    {
      heading: 'AI',
      items: AGENTS.flatMap((entry) =>
        MODELS[entry.value].map((entryModel) => ({
          id: `ai:${entry.value}/${entryModel.value}`,
          label: `${entry.label} · ${entryModel.label}`,
          icon: agentIcon[entry.value],
          data: { kind: 'ai' as const, id: `${entry.value}/${entryModel.value}`, label: `${entry.label} · ${entryModel.label}` },
        })),
      ),
    },
  ]);

  function pickMention(item: ZCommandItem<Mention>) {
    const picked = item.data;
    if (!picked) return;
    if (picked.kind === 'ai') {
      // An AI mention routes this message to that agent + model, so there is only ever one.
      const [nextAgent, nextModel] = picked.id.split('/');
      agent = nextAgent as Agent;
      model = nextModel;
      mentions = [...mentions.filter((entry) => entry.kind !== 'ai'), picked];
    } else if (!mentions.some((entry) => entry.kind === picked.kind && entry.id === picked.id)) {
      mentions = [...mentions, picked];
    }
  }

  const chipIcon: Record<Mention['kind'], ZIconName> = {
    session: 'lucideMessageSquare' as ZIconName,
    project: 'lucideFolder' as ZIconName,
    ai: 'lucideBot' as ZIconName,
  };

  function resize() {
    if (!area) return;
    area.style.height = 'auto';
    area.style.height = `${Math.min(area.scrollHeight, 180)}px`;
  }

  function submit() {
    draft = '';
    mentions = [];
    resize();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    } else if (event.key === '@') {
      // The palette takes over the @: pick a session, project or AI instead of typing the name.
      event.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<form class="rounded-2xl border border-input bg-card p-3 shadow-lg transition-shadow focus-within:ring-2 focus-within:ring-ring/40" onsubmit={(event) => { event.preventDefault(); submit(); }}>
  {#if mentions.length > 0}
    <div class="mb-2 flex flex-wrap gap-1.5" aria-label="Mentions">
      {#each mentions as mention (mention.kind + mention.id)}
        <span class="inline-flex items-center gap-1 rounded-md bg-primary/10 py-0.5 pr-0.5 pl-1.5 text-xs text-primary">
          <ZIcon zType={chipIcon[mention.kind]} class="size-3" />
          {mention.label}
          <button
            type="button"
            class="grid size-4 cursor-pointer place-items-center rounded hover:bg-primary/20"
            aria-label="Remove {mention.label}"
            onclick={() => (mentions = mentions.filter((entry) => entry !== mention))}
          >
            <ZIcon zType="lucideX" class="size-3" />
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <textarea
    bind:this={area}
    bind:value={draft}
    oninput={resize}
    onkeydown={onKeydown}
    rows="1"
    aria-label="Message"
    placeholder="Message AgentDeck…  (@ to mention a session, project or AI)"
    class="max-h-45 min-h-6 w-full resize-none border-0 bg-transparent px-1 pt-1 pb-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>

  <div class="flex items-center justify-between gap-2">
    <div class="flex min-w-0 items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7" aria-label="Attach file">
        <ZIcon zType="lucidePaperclip" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7" aria-label="Mention a session, project or AI" title="Mention (@)" onclick={() => (paletteOpen = true)}>
        <ZIcon zType="lucideAtSign" class="size-4" />
      </ZButton>
      <ZSelect
        class="w-32"
        zSize="sm"
        zVariant="ghost"
        zOptions={agentOptions}
        value={agent}
        onValueChange={onAgentChange}
        zDropdownWidth={160}
        zPosition="top-left"
      />
      <ZSelect
        class="w-40"
        zSize="sm"
        zVariant="ghost"
        zOptions={modelOptions}
        value={model}
        onValueChange={(value) => (model = value as string)}
        zDropdownWidth={176}
        zPosition="top-left"
      />
    </div>
    <ZButton zWave={false} type="submit" zType="default" zSize="default" class="size-8 shrink-0 rounded-full p-0" disabled={!draft.trim()} aria-label="Send message">
      <ZIcon zType="lucideArrowUp" class="size-4" />
    </ZButton>
  </div>
</form>

<ZCommand
  bind:open={paletteOpen}
  zGroups={groups}
  zSearch="fuzzy"
  zPlaceholder="Mention a session, project or AI…"
  zRootLabel="Mention"
  onSelect={pickMention}
  onClosed={() => area?.focus()}
>
  {#snippet footer()}
    <div class="flex gap-3 px-3 py-1.5 text-[10px] text-muted-foreground">
      <span>↑↓ navigate</span><span>↵ add</span><span>esc close</span>
    </div>
  {/snippet}
</ZCommand>
