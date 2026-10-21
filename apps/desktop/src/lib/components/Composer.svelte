<script lang="ts">
  import { untrack } from 'svelte';
  import { getPref, setPref } from '../prefs';
  import { ZButton, ZCommand, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import {
    activeSession,
    AGENTS,
    draftMentions,
    drafts,
    MODELS,
    projectOf,
    sendMessage,
    workspace,
    type Agent,
    type Effort,
    type Mention,
    type Permission,
  } from '../sessions.svelte';
  import ModelPicker from './ModelPicker.svelte';

  // What is typed belongs to the open session: switch away and back and it is still there.
  const sessionId = $derived(workspace.activeId);
  const draft = $derived(drafts[sessionId] ?? '');
  const mentions = $derived(draftMentions[sessionId] ?? []);
  const setDraft = (value: string) => (drafts[sessionId] = value);
  const setMentions = (value: Mention[]) => (draftMentions[sessionId] = value);
  let area: HTMLTextAreaElement | undefined = $state();
  let paletteOpen = $state(false);
  let innerW = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);

  // Agent + model of this message. They follow the active session and can be overridden with @AI.
  let agent = $state<Agent>('claude');
  let model = $state('claude-sonnet-5-5');
  let effort = $state<Effort>('medium');
  let fast = $state(false);
  let permission = $state<Permission>('ask');

  // Last picked model, effort, speed and approval mode come back after a reload.
  let pickerHydrated = false;
  getPref<{ agent: Agent; model: string; effort: Effort; fast: boolean; permission: Permission }>('composer.picker').then((saved) => {
    if (saved) {
      agent = saved.agent;
      model = saved.model;
      effort = saved.effort;
      fast = saved.fast;
      permission = saved.permission;
    }
    pickerHydrated = true;
  });
  $effect(() => {
    const picker = { agent, model, effort, fast, permission };
    if (pickerHydrated) setPref('composer.picker', picker);
  });
  $effect(() => {
    const session = activeSession();
    if (!session) return;
    untrack(() => {
      agent = session.agent;
      model = session.model;
    });
  });

  const agentIcon: Record<Agent, ZIconName> = {
    claude: 'brandClaude' as ZIconName,
    codex: 'brandOpenai' as ZIconName,
    gemini: 'brandGemini' as ZIconName,
  };

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
          keywords: [projectOf(item)?.name ?? '', item.gitBranch ?? '', item.agent],
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
      setMentions([...mentions.filter((entry) => entry.kind !== 'ai'), picked]);
    } else if (!mentions.some((entry) => entry.kind === picked.kind && entry.id === picked.id)) {
      setMentions([...mentions, picked]);
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

  // The textarea height follows the text of whichever session is open.
  $effect(() => {
    void sessionId;
    void draft;
    untrack(() => queueMicrotask(resize));
  });

  function submit() {
    if (!draft.trim()) return;
    sendMessage(draft);
    setDraft('');
    setMentions([]);
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

<svelte:window bind:innerWidth={innerW} />
<form class="min-w-0 rounded-2xl border border-input bg-card p-2.5 shadow-lg min-[720px]:p-3 transition-shadow focus-within:ring-2 focus-within:ring-ring/40" onsubmit={(event) => { event.preventDefault(); submit(); }}>
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
            onclick={() => setMentions(mentions.filter((entry) => entry !== mention))}
          >
            <ZIcon zType="lucideX" class="size-3" />
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <textarea
    bind:this={area}
    bind:value={() => draft, setDraft}
    oninput={resize}
    onkeydown={onKeydown}
    rows="1"
    wrap="soft"
    aria-label="Message"
    placeholder={innerW >= 720 ? "Message AgentDeck…  (@ to mention a session, project or AI)" : "Message…"}
    class="max-h-45 min-h-6 w-full resize-none overflow-x-hidden break-words border-0 bg-transparent px-1 pt-1 pb-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>

  <div class="flex min-w-0 items-center justify-between gap-2">
    <div class="flex min-w-0 items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Attach file">
        <ZIcon zType="lucidePaperclip" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="hidden h-7 w-7 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary min-[720px]:inline-flex" aria-label="Mention a session, project or AI" title="Mention (@)" onclick={() => (paletteOpen = true)}>
        <ZIcon zType="lucideAtSign" class="size-4" />
      </ZButton>
      <ModelPicker bind:agent bind:model bind:effort bind:fast bind:permission />
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
  zSize="default"
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
