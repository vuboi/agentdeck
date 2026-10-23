<script lang="ts">
  import { untrack } from 'svelte';
  import { getPref, setPref } from '../prefs';
  import { ZButton, ZCommand, ZIcon, ZPopover, zSortable, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import { loadSlash, slash, type SlashItem } from '../slash.svelte';
  import { agyBlocks, agyModes, loadAgyModes } from '../agy-modes.svelte';
  import {
    activeSession,
    AGENTS,
    draftElements,
    draftImages,
    draftMentions,
    drafts,
    isRunning,
    lastChat,
    rememberMixed,
    MODELS,
    projectOf,
    queued,
    steerQueued,
    sendMessage,
    stopTurn,
    turnOptions,
    workspace,
    type Agent,
    type Effort,
    type Attachment,
    type Mention,
    type Permission,
  } from '../sessions.svelte';
  import { openImages } from '../preview.svelte';
  import ModelPicker from './ModelPicker.svelte';

  // What is typed belongs to the open session: switch away and back and it is still there.
  const sessionId = $derived(workspace.activeId);
  const draft = $derived(drafts[sessionId] ?? '');
  const mentions = $derived(draftMentions[sessionId] ?? []);
  const setDraft = (value: string) => {
    drafts[sessionId] = value;
  };
  const setMentions = (value: Mention[]) => {
    draftMentions[sessionId] = value;
  };
  const images = $derived(draftImages[sessionId] ?? []);
  const elements = $derived(draftElements[sessionId] ?? []);
  const running = $derived(isRunning(sessionId));
  const waiting = $derived(queued[sessionId] ?? []);

  // Queued messages are reordered by dragging their handle (zSortable: the rows glide, Escape cancels).
  function moveQueued(from: number, to: number) {
    if (from === to) return;
    const list = [...waiting];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    queued[sessionId] = list;
  }
  const hasInput = $derived(draft.trim().length > 0 || images.length > 0 || elements.length > 0);
  let detail = $state<string | null>(null);
  let chipAnchors = $state<Record<string, HTMLElement | null>>({});
  const setImages = (value: Attachment[]) => {
    draftImages[sessionId] = value;
  };
  let fileInput: HTMLInputElement | undefined = $state();
  let dragging = $state(false);
  let area: HTMLTextAreaElement | undefined = $state();
  let paletteOpen = $state(false);
  let slashOpen = $state(false);
  let innerW = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);

  // Agent + model of this message. They follow the active session and can be overridden with @AI.
  let agent = $state<Agent>('claude');
  let model = $state('claude-sonnet-5-5');
  let effort = $state<Effort>('medium');
  let fast = $state(false);
  let permission = $state<Permission>('auto');
  $effect(() => {
    turnOptions.permission = permission;
    turnOptions.effort = effort;
  });

  // Antigravity's modes depend on the account. Find out which ones this account has, and never leave the chat on one
  // it lacks (agy would ignore it and ask for every command): fall back to asking, the one setting that always exists.
  $effect(() => {
    if (agent === 'antigravity') void loadAgyModes();
  });
  $effect(() => {
    if (agent === 'antigravity' && agyModes.loaded && agyBlocks(permission)) permission = 'ask';
  });

  // The picker belongs to each session: agent, model, effort, speed and approval mode are remembered per
  // session in IndexedDB, so a reload brings back what that chat was using.
  type Picker = { agent: Agent; model: string; effort: Effort; fast: boolean; permission: Permission };
  let pickers: Record<string, Picker> = {};
  let pickersLoaded = false;

  function applyPickerFor(sessionId: string) {
    const saved = pickers[sessionId];
    if (saved) {
      agent = saved.agent;
      model = saved.model;
      effort = saved.effort;
      fast = saved.fast;
      permission = saved.permission;
      return;
    }
    const session = workspace.sessions.find((item) => item.id === sessionId);
    if (session) {
      agent = session.agent;
      model = session.model;
      // A new chat also takes the effort of the last chat that was written in.
      if (session.draft && lastChat.agent && lastChat.agent === session.agent && lastChat.model === session.model) effort = lastChat.effort;
    }
  }

  getPref<Record<string, Picker>>('composer.pickers').then((saved) => {
    pickers = saved ?? {};
    pickersLoaded = true;
    if (workspace.activeId) untrack(() => applyPickerFor(workspace.activeId));
  });

  // Switching session: show that session's picker.
  $effect(() => {
    const id = workspace.activeId;
    if (id) untrack(() => applyPickerFor(id));
  });

  // Any change of the picker: remember it for this session and make it the session's model.
  $effect(() => {
    const current: Picker = { agent, model, effort, fast, permission };
    const id = workspace.activeId;
    if (!pickersLoaded || !id) return;
    untrack(() => {
      pickers[id] = current;
      setPref('composer.pickers', pickers);
      const session = workspace.sessions.find((item) => item.id === id);
      if (session) {
        // A chat continued with another agent lists every agent it has had (the sidebar shows them stacked).
        if (session.agent !== current.agent && !session.draft) {
          session.agents = [...new Set([...(session.agents ?? [session.source ?? session.agent]), current.agent])];
          rememberMixed(session);
        }
        session.agent = current.agent;
        session.model = current.model;
      }
    });
  });

  const agentIcon: Record<Agent, ZIconName> = {
    claude: 'brandClaude' as ZIconName,
    codex: 'brandOpenai' as ZIconName,
    antigravity: 'brandGemini' as ZIconName,
    custom: 'lucideServer' as ZIconName,
  };

  // ---------- slash commands ----------
  // The agent's own commands and skills, plus AgentDeck's: read for the agent and project of this chat.
  const projectPath = $derived(workspace.projects.find((item) => item.id === workspace.sessions.find((entry) => entry.id === sessionId)?.projectId)?.path ?? null);
  $effect(() => {
    void loadSlash(agent, projectPath);
  });
  const agentLabel = $derived(AGENTS.find((entry) => entry.value === agent)?.label ?? agent);
  const slashGroups = $derived.by<ZCommandGroup<SlashItem>[]>(() => {
    const row = (item: SlashItem): ZCommandItem<SlashItem> => ({
      id: `${item.kind}:${item.origin}:${item.name}`,
      label: item.description ? `/${item.name}  ·  ${item.description}` : `/${item.name}`,
      icon: (item.kind === 'skill' ? 'lucideSparkles' : item.kind === 'builtin' ? 'lucideZap' : 'lucideTerminal') as ZIconName,
      keywords: [item.name, item.description, item.origin],
      data: item,
    });
    const of = (kind: SlashItem['kind']) => slash.items.filter((item) => item.kind === kind).map(row);
    return [
      { heading: 'AgentDeck', items: of('builtin') },
      { heading: `${agentLabel} commands`, items: of('prompt') },
      { heading: `${agentLabel} skills`, items: of('skill') },
    ].filter((group) => group.items.length > 0);
  });

  /** Puts `/name ` in the message box where the caret is, so the arguments can be typed right after it. */
  function pickSlash(item: ZCommandItem<SlashItem>) {
    const picked = item.data;
    if (!picked) return;
    const at = area?.selectionStart ?? draft.length;
    const before = draft.slice(0, at);
    const lead = before && !/\s$/.test(before) ? ' ' : '';
    setDraft(`${before}${lead}/${picked.name} ${draft.slice(at)}`);
    queueMicrotask(() => {
      area?.focus();
      const end = (before + lead + picked.name).length + 2;
      area?.setSelectionRange(end, end);
    });
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
      heading: 'Hand to an agent',
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
    // An AI mention hands the message to that agent: it works in a chat of its own and the answer comes back here.
    // Several can be added; they work side by side.
    if (!mentions.some((entry) => entry.kind === picked.kind && entry.id === picked.id)) setMentions([...mentions, picked]);
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

  // ---------- images: paste, drop or pick ----------
  const MAX_SIDE = 1568; // larger than this the model downsizes it anyway
  const MAX_IMAGES = 6;

  /** The image scaled to at most MAX_SIDE, as a data URL (PNG stays PNG, the rest becomes JPEG). */
  async function toAttachment(file: File): Promise<Attachment | null> {
    if (!/^image\/(png|jpeg|gif|webp)$/.test(file.type)) return null;
    const bitmap = await createImageBitmap(file);
    const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(bitmap.width * scale);
    canvas.height = Math.round(bitmap.height * scale);
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    const mime = file.type === 'image/png' ? 'image/png' : 'image/jpeg';
    return { id: crypto.randomUUID(), name: file.name || 'pasted image', mime, src: canvas.toDataURL(mime, 0.88) };
  }

  async function addFiles(files: File[]) {
    const room = MAX_IMAGES - images.length;
    const added = (await Promise.all(files.slice(0, Math.max(0, room)).map((file) => toAttachment(file).catch(() => null)))).filter((item): item is Attachment => item !== null);
    if (added.length) setImages([...images, ...added]);
  }

  function onPaste(event: ClipboardEvent) {
    const files = [...(event.clipboardData?.files ?? [])].filter((file) => file.type.startsWith('image/'));
    if (files.length === 0) return; // plain text pastes as usual
    event.preventDefault();
    void addFiles(files);
  }

  function onDrop(event: DragEvent) {
    dragging = false;
    const files = [...(event.dataTransfer?.files ?? [])];
    if (files.length === 0) return;
    event.preventDefault();
    void addFiles(files);
  }

  function submit() {
    if (!hasInput) return;
    // Handing work to an agent needs words that say what the work is.
    if (mentions.some((entry) => entry.kind === 'ai') && !draft.trim()) return;
    sendMessage(draft, images, elements, mentions);
    draftElements[sessionId] = [];
    detail = null;
    setDraft('');
    setMentions([]);
    setImages([]);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    } else if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && /(^|\s)$/.test(draft.slice(0, area?.selectionStart ?? draft.length))) {
      // A slash that starts a word opens the command list, as the @ opens mentions.
      event.preventDefault();
      slashOpen = true;
    } else if (event.key === '@') {
      // The palette takes over the @: pick a session, project or AI instead of typing the name.
      event.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<svelte:window bind:innerWidth={innerW} />
{#if waiting.length > 0}
  <!-- Messages typed while the agent works sit on a tray behind the composer: they go out one by one when it is
       free, or now with Steer. The composer overlaps the tray's lower edge. -->
  <div class="mx-3 -mb-4 overflow-hidden rounded-t-2xl border border-b-0 border-border bg-card pb-4 shadow-md" role="list" aria-label="Queued messages" {@attach zSortable({ onSort: moveQueued, handle: '[data-handle]' })}>
    {#each waiting as item, index (item)}
      <div class="flex items-center gap-2.5 bg-card px-4 py-1 text-xs {index > 0 ? 'border-t border-border' : ''}" role="listitem">
        <!-- The list icon turns into a drag handle while the pointer is on it. -->
        <span data-handle class="group/handle grid size-3.5 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground active:cursor-grabbing" aria-label="Drag to reorder">
          <ZIcon zType={'lucideListChecks' as ZIconName} class="size-3.5 group-hover/handle:hidden" />
          <ZIcon zType={'lucideGripVertical' as ZIconName} class="hidden size-3.5 group-hover/handle:block" />
        </span>
        <span class="min-w-0 flex-1 truncate text-foreground">{item.text || (item.images.length ? 'Image' : 'Element')}</span>
        <button type="button" class="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => steerQueued(sessionId, item)}>
          <ZIcon zType={'lucideArrowRight' as ZIconName} class="size-3" />
          <span>Steer</span>
        </button>
        <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent hover:text-destructive" aria-label="Remove from the queue" onclick={() => (queued[sessionId] = waiting.filter((entry) => entry !== item))}>
          <ZIcon zType={'lucideTrash2' as ZIconName} class="size-3.5" />
        </button>
      </div>
    {/each}
  </div>
{/if}
<form
  class="relative z-10 min-w-0 rounded-2xl border bg-card px-2.5 pt-2 pb-1.5 shadow-lg min-[720px]:px-3 min-[720px]:pt-2.5 min-[720px]:pb-1.5 transition-shadow {dragging ? 'border-primary ring-2 ring-primary/40' : 'border-input'}"
  onsubmit={(event) => { event.preventDefault(); submit(); }}
  ondragover={(event) => { if (event.dataTransfer?.types.includes('Files')) { event.preventDefault(); dragging = true; } }}
  ondragleave={() => (dragging = false)}
  ondrop={onDrop}
>
  {#if elements.length > 0}
    <!-- Elements picked in the browser travel with the message; click one to see what is included. -->
    <div class="mb-2 flex flex-wrap gap-1.5" aria-label="Picked elements">
      {#each elements as item (item.id)}
        <div class="group/chip relative" bind:this={chipAnchors[item.id]}>
          <button type="button" class="flex h-8 max-w-60 cursor-pointer items-center gap-1.5 rounded-md border border-border bg-muted/40 px-2 font-mono text-[11px] text-muted-foreground outline-none hover:bg-muted" aria-label="Details of {item.selector}" onclick={() => (detail = detail === item.id ? null : item.id)}>
            <ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 shrink-0 text-primary" />
            <span class="truncate">&lt;{item.tag} /&gt; {item.text}</span>
          </button>
          <button type="button" class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/chip:opacity-100 focus-visible:opacity-100" aria-label="Remove element" onclick={() => (draftElements[sessionId] = elements.filter((entry) => entry !== item))}>
            <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
          </button>
        </div>
      {/each}
    </div>
    {#each elements.filter((item) => item.id === detail) as item (item.id)}
      <ZPopover anchor={chipAnchors[item.id]} open={true} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={360} onClose={() => (detail = null)}>
        {#snippet children()}
          <div class="flex max-h-[min(60vh,420px)] w-full flex-col gap-2 overflow-y-auto p-3 text-xs">
            <div class="flex items-center gap-1.5 font-medium"><ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 text-primary" />Included with your message</div>
            <dl class="m-0 grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1.5">
              <dt class="text-muted-foreground">Element</dt><dd class="m-0 font-mono">&lt;{item.tag} /&gt;</dd>
              {#if item.note}<dt class="text-muted-foreground">Note</dt><dd class="m-0">{item.note}</dd>{/if}
              <dt class="text-muted-foreground">Text</dt><dd class="m-0">{item.text || '—'}</dd>
              <dt class="text-muted-foreground">Page</dt><dd class="m-0 break-all">{item.url}</dd>
              <dt class="text-muted-foreground">Selector</dt><dd class="m-0 rounded bg-muted/60 px-1.5 py-1 font-mono text-[11px] break-all">{item.selector}</dd>
              <dt class="text-muted-foreground">Size</dt><dd class="m-0">{item.box.w}×{item.box.h}</dd>
            </dl>
            <div class="text-muted-foreground">Computed styles</div>
            <dl class="m-0 grid grid-cols-[7rem_1fr] gap-x-2 gap-y-0.5 font-mono text-[11px]">
              {#each Object.entries(item.style) as [key, value] (key)}<dt class="text-muted-foreground">{key}</dt><dd class="m-0 break-all">{value}</dd>{/each}
            </dl>
          </div>
        {/snippet}
      </ZPopover>
    {/each}
  {/if}
  {#if images.length > 0}
    <div class="mb-2 flex flex-wrap gap-2" aria-label="Attached images">
      {#each images as image (image.id)}
        <div class="group/img relative">
          <button type="button" class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {image.name}" onclick={() => openImages(images, images.indexOf(image))}>
            <img src={image.src} alt={image.name} class="size-full object-cover" />
          </button>
          <button
            type="button"
            class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/img:opacity-100 focus-visible:opacity-100"
            aria-label="Remove {image.name}"
            onclick={() => setImages(images.filter((entry) => entry !== image))}
          >
            <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
          </button>
        </div>
      {/each}
    </div>
  {/if}
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
    onpaste={onPaste}
    rows="1"
    wrap="soft"
    aria-label="Message"
    placeholder="Do anything…"
    class="max-h-45 min-h-6 w-full resize-none overflow-x-hidden break-words border-0 bg-transparent px-1 pt-0.5 pb-1.5 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>

  <div class="flex min-w-0 items-center justify-between gap-2">
    <div class="flex min-w-0 items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Attach image" title="Attach image (or paste / drop)" onclick={() => fileInput?.click()}>
        <ZIcon zType="lucidePaperclip" class="size-3.5" />
      </ZButton>
      <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden onchange={(event) => { void addFiles([...(event.currentTarget.files ?? [])]); event.currentTarget.value = ''; }} />
      <ZButton zWave={false} zType="ghost" zSize="sm" class="hidden size-6! !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary min-[720px]:inline-flex" aria-label="Mention a session, project or AI" title="Mention (@)" onclick={() => (paletteOpen = true)}>
        <ZIcon zType="lucideAtSign" class="size-3.5" />
      </ZButton>
      <ModelPicker bind:agent bind:model bind:effort bind:fast bind:permission />
    </div>
    {#if running && !hasInput}
      <!-- A reply is being written and nothing is typed: this stops it. -->
      <ZButton zWave={false} zType="default" zSize="default" class="size-7! shrink-0 rounded-full p-0" aria-label="Stop" {@attach zTooltip({ content: 'Stop', position: 'top' })} onclick={() => stopTurn(sessionId)}>
        <svg class="size-3" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="currentColor" /></svg>
      </ZButton>
    {:else}
      <ZButton zWave={false} type="submit" zType="default" zSize="default" class="size-7! shrink-0 rounded-full p-0" disabled={!hasInput} aria-label={running ? (turnOptions.whileRunning === 'queue' ? 'Add to the queue' : 'Stop and send') : 'Send message'} {@attach zTooltip({ content: running ? (turnOptions.whileRunning === 'queue' ? 'Add to the queue' : 'Stop and send now') : 'Send', position: 'top' })}>
        <ZIcon zType="lucideArrowUp" class="size-3.5" />
      </ZButton>
    {/if}
  </div>
</form>

<ZCommand
  bind:open={slashOpen}
  zGroups={slashGroups}
  zSearch="fuzzy"
  zSize="default"
  zPlaceholder="Run a command or skill…"
  zRootLabel="Commands"
  onSelect={pickSlash}
  onClosed={() => area?.focus()}
>
  {#snippet footer()}
    <div class="flex gap-3 px-3 py-1.5 text-[10px] text-muted-foreground">
      <span>↑↓ navigate</span><span>↵ use</span><span>esc close</span>
    </div>
  {/snippet}
</ZCommand>

<ZCommand
  bind:open={paletteOpen}
  zGroups={groups}
  zSearch="fuzzy"
  zSize="default"
  zPlaceholder="Mention a chat, a project, or hand work to an agent…"
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
