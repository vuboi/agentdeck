<script lang="ts">
  import { untrack } from 'svelte';
  import { getPref, setPref } from '../prefs';
  import { ZButton, ZCommand, ZIcon, ZPopover } from '@shival99/z-ui-svelte';
  import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
  import {
    activeSession,
    AGENTS,
    draftElements,
    draftImages,
    draftMentions,
    drafts,
    MODELS,
    projectOf,
    sendMessage,
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
  const setDraft = (value: string) => (drafts[sessionId] = value);
  const setMentions = (value: Mention[]) => (draftMentions[sessionId] = value);
  const images = $derived(draftImages[sessionId] ?? []);
  const elements = $derived(draftElements[sessionId] ?? []);
  let detail = $state<string | null>(null);
  let chipAnchors = $state<Record<string, HTMLElement | null>>({});
  const setImages = (value: Attachment[]) => (draftImages[sessionId] = value);
  let fileInput: HTMLInputElement | undefined = $state();
  let dragging = $state(false);
  let area: HTMLTextAreaElement | undefined = $state();
  let paletteOpen = $state(false);
  let innerW = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);

  // Agent + model of this message. They follow the active session and can be overridden with @AI.
  let agent = $state<Agent>('claude');
  let model = $state('claude-sonnet-5-5');
  let effort = $state<Effort>('medium');
  let fast = $state(false);
  let permission = $state<Permission>('ask');
  $effect(() => {
    turnOptions.permission = permission;
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
        session.agent = current.agent;
        session.model = current.model;
      }
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
    if (!draft.trim() && images.length === 0 && elements.length === 0) return;
    sendMessage(draft, images, elements);
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
    } else if (event.key === '@') {
      // The palette takes over the @: pick a session, project or AI instead of typing the name.
      event.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<svelte:window bind:innerWidth={innerW} />
<form
  class="min-w-0 rounded-2xl border bg-card p-2.5 shadow-lg min-[720px]:p-3 transition-shadow focus-within:ring-2 focus-within:ring-ring/40 {dragging ? 'border-primary ring-2 ring-primary/40' : 'border-input'}"
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
          <button type="button" class="absolute -top-1.5 -right-1.5 grid size-4 cursor-pointer place-items-center rounded-full bg-foreground text-background opacity-0 transition-opacity group-hover/chip:opacity-100 focus-visible:opacity-100" aria-label="Remove element" onclick={() => (draftElements[sessionId] = elements.filter((entry) => entry !== item))}>
            <ZIcon zType="lucideX" class="size-3" />
          </button>
        </div>
      {/each}
    </div>
    {#each elements.filter((item) => item.id === detail) as item (item.id)}
      <ZPopover anchor={chipAnchors[item.id]} open={true} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={360} onClose={() => (detail = null)}>
        {#snippet children()}
          <div class="flex max-h-[min(60vh,420px)] w-[360px] flex-col gap-2 overflow-y-auto p-3 text-xs">
            <div class="flex items-center gap-1.5 font-medium"><ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 text-primary" />Included with your message</div>
            <dl class="m-0 grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1.5">
              <dt class="text-muted-foreground">Element</dt><dd class="m-0 font-mono">&lt;{item.tag} /&gt;</dd>
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
            class="absolute -top-1.5 -right-1.5 grid size-4 cursor-pointer place-items-center rounded-full bg-foreground text-background opacity-0 transition-opacity group-hover/img:opacity-100 focus-visible:opacity-100"
            aria-label="Remove {image.name}"
            onclick={() => setImages(images.filter((entry) => entry !== image))}
          >
            <ZIcon zType="lucideX" class="size-3" />
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
    class="max-h-45 min-h-6 w-full resize-none overflow-x-hidden break-words border-0 bg-transparent px-1 pt-1 pb-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>

  <div class="flex min-w-0 items-center justify-between gap-2">
    <div class="flex min-w-0 items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Attach image" title="Attach image (or paste / drop)" onclick={() => fileInput?.click()}>
        <ZIcon zType="lucidePaperclip" class="size-4" />
      </ZButton>
      <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden onchange={(event) => { void addFiles([...(event.currentTarget.files ?? [])]); event.currentTarget.value = ''; }} />
      <ZButton zWave={false} zType="ghost" zSize="sm" class="hidden h-7 w-7 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary min-[720px]:inline-flex" aria-label="Mention a session, project or AI" title="Mention (@)" onclick={() => (paletteOpen = true)}>
        <ZIcon zType="lucideAtSign" class="size-4" />
      </ZButton>
      <ModelPicker bind:agent bind:model bind:effort bind:fast bind:permission />
    </div>
    <ZButton zWave={false} type="submit" zType="default" zSize="default" class="size-8 shrink-0 rounded-full p-0" disabled={!draft.trim() && images.length === 0 && elements.length === 0} aria-label="Send message">
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
