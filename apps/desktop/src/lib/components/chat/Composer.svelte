<script lang="ts">
  import { fade } from 'svelte/transition';
  import ProjectBar from '../layout/ProjectBar.svelte';
  import { untrack } from 'svelte';
  import { getPref, setPref } from '../../app/platform/prefs';
  import { ZButton, ZIcon, ZPopover, zTooltip } from '@shival99/z-ui-svelte';
  import PickerPopover, { type PickerItem } from '../ui/PickerPopover.svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { loadSlash, slash, type SlashItem } from '../../chat/slash.svelte';
  import { prepare, search } from '../../app/ui/palette';
  import ElementCard from '../browser/ElementCard.svelte';
  import { TRIGGER_ON } from '../../app/ui/popover';
  import { agyBlocks, agyModes, loadAgyModes } from '../../agents/agy-modes.svelte';
  import { AGENTS, draftElements, draftImages, draftFiles, draftMentions, drafts, isRunning, needsUserInput, lastChat, rememberMixed, queued, queuePaused, type Queued, sendMessage, canListenToSession, prewarmChat, stopTurnForResume, resumeStoppedTurn, manuallyStopped, turnOptions, workspace, type Agent, type Effort, type Attachment, type FileAttachment, type Mention, type Permission } from '../../chat/sessions.svelte';
  import Menu from '../ui/Menu.svelte';
  import TextEditContextMenu from '../ui/TextEditContextMenu.svelte';
  import { clipFromContents, clipFromPaste, imageMime, isImageName } from '../../chat/content/attachments';
  import { createTextHistory, type TextHistory } from '../../app/text/text-history';
  import { fileSize, keepPicture, pictureSrc, saveFileAttachment } from '../../chat/agent';
  import { inTauri } from '../../app/platform/history';
  import { toast } from '../../app/platform/toast';
  import ModelPicker from './ModelPicker.svelte';
  import ComposerQueue from './ComposerQueue.svelte';
  import ComposerChips from './ComposerChips.svelte';
  import { editorText, editorSelection, setEditorSelection, replaceEditorRange, renderEditor } from './composer-editor';
  import ComposerQuota from './ComposerQuota.svelte';
  import { DOC_MAX, LONG_PASTE_CHARS, MAX_IMAGES, hasMarkdown, readBase64, toAttachment } from './composer-helpers';
  import { buildMentionGroups, buildSlashGroups, flatten } from './composer-pickers';

  // What is typed belongs to the open session: switch away and back and it is still there.
  const sessionId = $derived(workspace.activeId);
  const draft = $derived(drafts[sessionId] ?? '');
  const mentions = $derived(draftMentions[sessionId] ?? []);
  // Ctrl+Z / Ctrl+Y go through a history of our own (per chat): the draft is written from code (a send, a clear, another
  // chat's draft), which drops the textarea's own undo history. A sent message can be brought back with Ctrl+Z too.
  const histories: Record<string, TextHistory> = {};
  const historyOf = (id: string) => (histories[id] ??= createTextHistory(drafts[id] ?? ''));
  const setDraft = (value: string, { sent = false } = {}) => {
    historyOf(sessionId).note(value, { boundary: sent });
    drafts[sessionId] = value;
    // Typing a message starts the agent's CLI, so it is up by the time the message is sent.
    if (!sent && value.trim()) prewarmChat(sessionId);
  };
  const setMentions = (value: Mention[]) => {
    draftMentions[sessionId] = value;
  };
  const images = $derived(draftImages[sessionId] ?? []);
  const attached = $derived(draftFiles[sessionId] ?? []);
  const setFiles = (value: FileAttachment[]) => {
    draftFiles[sessionId] = value;
  };
  const elements = $derived(draftElements[sessionId] ?? []);
  const running = $derived(isRunning(sessionId));
  const listenerReady = $derived(canListenToSession(sessionId));
  const mac = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);
  const waiting = $derived(queued[sessionId] ?? []);
  const waitingForUser = $derived(needsUserInput(sessionId));

  // A queued message not sent yet goes back into the field to be changed; sending it again queues it as before. What
  // was in the field takes its place in the queue, so nothing typed is lost.
  function editQueued(item: Queued) {
    const list = [...waiting];
    const at = list.indexOf(item);
    if (at === -1) return;
    if (hasInput)
      list[at] = {
        text: draft,
        images: [...images],
        elements: [...elements],
        mentions: [...mentions],
        files: [...attached],
      };
    else list.splice(at, 1);
    if (list.length) queued[sessionId] = list;
    else delete queued[sessionId];
    setDraft(item.text);
    setImages(item.images);
    setFiles(item.files ?? []);
    draftElements[sessionId] = item.elements;
    setMentions(item.mentions ?? []);
    requestAnimationFrame(() => {
      area?.focus();
      if (area) setEditorSelection(area, item.text.length);
      resize();
    });
  }
  const hasInput = $derived(draft.trim().length > 0 || images.length > 0 || elements.length > 0 || attached.length > 0);
  const action = $derived(running && !hasInput ? 'stop' : manuallyStopped[sessionId] && !hasInput ? 'resume' : 'send');
  let detail = $state<string | null>(null);
  // The card stays mounted after it closes, so its exit animation can play: it keeps the last element shown.
  let lastDetail = $state<string | null>(null);
  $effect(() => {
    if (detail) lastDetail = detail;
  });
  let chipAnchors = $state<Record<string, HTMLElement | null>>({});
  const setImages = (value: Attachment[]) => {
    draftImages[sessionId] = value;
  };
  let fileInput: HTMLInputElement | undefined = $state();
  let dragging = $state(false);
  let area: HTMLDivElement | undefined = $state();
  let textEditMenu = $state<{ openAt: (event: MouseEvent, target: HTMLElement) => void } | undefined>();
  const caret = () => area ? editorSelection(area) : { start: draft.length, end: draft.length };
  $effect(() => {
    void sessionId;
    void draft;
    void mentions;
    if (area) renderEditor(area, draft, mentions);
  });
  // `@` or `/` at the start of a word opens a picker over the message box; what follows it (up to the caret) is the
  // search. The text stays in the box while the picker is open, as in Claude; picking replaces it.
  type Trigger = { char: '@' | '/'; at: number; query: string };
  let trigger = $state<Trigger | null>(null);
  // Escape puts a picker away until the text before the caret changes again.
  let dismissed: string | null = null;
  let picker = $state<{ key: (event: KeyboardEvent) => boolean }>();
  const paletteOpen = $derived(trigger?.char === '@');
  const slashOpen = $derived(trigger?.char === '/');
  const slashQuery = $derived(slashOpen ? (trigger?.query ?? '') : '');
  const mentionQuery = $derived(paletteOpen ? (trigger?.query ?? '') : '');
  /** Reads the word under the caret: a picker is open while it starts with `@` or `/`. */
  function detectTrigger() {
    if (!area) return;
    const position = caret();
    const before = draft.slice(0, position.start);
    if (position.end !== position.start || before === dismissed) return void (trigger = null);
    dismissed = null;
    const category = /(^|\s)\/([cmpw])(?:\s+([^\n]*))?$/i.exec(before);
    const found = category ?? /(^|\s)([@/])([^\s@/]*)$/.exec(before);
    trigger = found
      ? {
          char: category ? '/' : (found[2] as '@' | '/'),
          at: category ? position.start - (category[3]?.length ?? 0) - category[2].length - (category[0].match(/\s+/)?.[0].length ?? 0) - 1 : position.start - found[3].length - 1,
          query: category ? `${category[2]} ${category[3] ?? ''}`.trimEnd() : found[3],
        }
      : null;
  }
  /** Takes the typed `@query` or `/query` out of the box and puts `insert` in its place. */
  function replaceTrigger(insert: string) {
    if (!trigger) return;
    const selection = caret();
    const position = selection.start;
    const start = trigger.at;
    setDraft(`${draft.slice(0, start)}${insert}${draft.slice(selection.end)}`);
    trigger = null;
    queueMicrotask(() => {
      area?.focus();
      if (area) setEditorSelection(area, start + insert.length);
      resize();
    });
  }
  /** The @ button: an `@` where the caret is, which opens the mention picker. */
  function startMention() {
    const at = caret().start;
    const before = draft.slice(0, at);
    const lead = before && !/\s$/.test(before) ? ' ' : '';
    setDraft(`${before}${lead}@${draft.slice(at)}`);
    queueMicrotask(() => {
      area?.focus();
      const caret = at + lead.length + 1;
      if (area) setEditorSelection(area, caret);
      detectTrigger();
    });
  }
  let innerW = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);

  // Agent + model of this message. They follow the active session and can be overridden with @AI.
  let agent = $state<Agent>('claude');
  let model = $state('claude-sonnet-5-5');
  let effort = $state<Effort>('medium');
  let fast = $state(true);
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
  type Picker = {
    agent: Agent;
    model: string;
    effort: Effort;
    fast: boolean;
    permission: Permission;
  };
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
    // A chat with nothing saved starts with fast mode on.
    fast = true;
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

  // A chat moved to another agent by itself (its agent failed and the next one of the chain took over): its picker
  // follows, so the next message goes to the agent now in charge.
  $effect(() => {
    const moved = (event: Event) => {
      const {
        sessionId: id,
        agent: next,
        model: nextModel,
      } = (
        event as CustomEvent<{
          sessionId: string;
          agent: Agent;
          model: string;
        }>
      ).detail;
      const saved = pickers[id];
      if (saved) {
        pickers[id] = { ...saved, agent: next, model: nextModel };
        setPref('composer.pickers', pickers);
      }
      if (id === workspace.activeId) {
        agent = next;
        model = nextModel;
      }
    };
    window.addEventListener('agentdeck:agent-moved', moved);
    return () => window.removeEventListener('agentdeck:agent-moved', moved);
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

  // ---------- slash commands ----------
  // The agent's own commands and skills, plus AgentDeck's: read for the agent and project of this chat.
  const projectPath = $derived(workspace.projects.find((item) => item.id === workspace.sessions.find((entry) => entry.id === sessionId)?.projectId)?.path ?? null);
  $effect(() => {
    void loadSlash(agent, projectPath);
  });
  const agentLabel = $derived(AGENTS.find((entry) => entry.value === agent)?.label ?? agent);
  const slashGroups = $derived(buildSlashGroups(slash.items, agentLabel));

  const slashPrepared = $derived(slashOpen ? prepare(slashGroups, (item) => String(item.data?.name ?? item.label)) : []);
  // AgentDeck's own commands lead, so the first row selected is one of them.
  const slashShown = $derived(slashOpen ? search(slashPrepared, slashQuery, 40, 'AgentDeck') : []);

  const slashItems = $derived(
    flatten(slashShown, (item, group) => ({
      id: item.id,
      label: `/${item.data?.name ?? ''}`,
      icon: item.icon,
      description: item.data?.description || undefined,
      group,
      data: item.data as SlashItem,
    })),
  );

  /** Puts `/name ` in place of the typed `/query`, so the arguments can be typed right after it. */
  function pickSlash(item: PickerItem<SlashItem>) {
    replaceTrigger(`/${item.data.name} `);
  }

  // ---------- mentions ----------
  const categoryMode = $derived(slashOpen && /^[cmpw](?:\s|$)/i.test(slashQuery) ? ({ c: 'session', p: 'project', m: 'ai', w: 'workflow' } as const)[slashQuery[0].toLowerCase() as 'c' | 'p' | 'm' | 'w'] : undefined);
  const mentionPaletteOpen = $derived(paletteOpen || Boolean(categoryMode));
  const mentionSearch = $derived(categoryMode ? slashQuery.slice(1).trim() : mentionQuery);
  const groups = $derived(buildMentionGroups(categoryMode));

  const mentionPrepared = $derived(mentionPaletteOpen ? prepare(groups) : []);
  const mentionShown = $derived(mentionPaletteOpen ? search(mentionPrepared, mentionSearch) : []);

  const mentionItems = $derived(
    flatten(mentionShown, (item, group) => ({
      id: item.id,
      label: item.label,
      hint: item.keywords?.[0] || undefined,
      icon: item.icon,
      agent: item.data?.kind === 'ai' ? (String(item.data.id).split('/')[0] as Agent) : undefined,
      group,
      data: item.data as Mention,
    })),
  );

  function pickMention(item: PickerItem<Mention>) {
    const picked = item.data;
    // Mentions stay inline; resolved provider/model metadata is injected for the main model at send time.
    replaceTrigger(picked.kind === 'workflow' ? `/w ${picked.label} ` : `@${picked.label} `);
    if (picked.kind === 'workflow') setMentions([...mentions.filter((entry) => entry.kind !== 'workflow'), picked]);
    else if (!mentions.some((entry) => entry.kind === picked.kind && entry.id === picked.id)) setMentions([...mentions, picked]);
  }
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

  /** Markdown and very long clipboard text become attachments, matching the regular paste handler. */
  function routePastedText(text: string, id = sessionId): boolean {
    if (hasMarkdown(text)) {
      attachMarkdown(text, id);
      return true;
    }
    if (text.length > LONG_PASTE_CHARS) {
      void addDocuments([new File([text], 'Pasted text.txt', { type: 'text/plain' })], id);
      return true;
    }
    return false;
  }

  function onTextAreaContextMenu(event: MouseEvent) {
    if (area) textEditMenu?.openAt(event, area);
  }

  /** Markdown paste travels as an attachment, leaving the composer free for a question. */
  function attachMarkdown(text: string, id: string) {
    void addDocuments([new File([text], 'Pasted Markdown.md', { type: 'text/markdown' })], id);
  }

  async function addFiles(files: File[]) {
    const isPicture = (file: File) => /^image\/(png|jpeg|gif|webp)$/.test(file.type);
    const id = sessionId;
    const pictures = files.filter(isPicture);
    const documents = files.filter((file) => !isPicture(file));
    const room = MAX_IMAGES - images.length;
    const added = (await Promise.all(pictures.slice(0, Math.max(0, room)).map((file) => toAttachment(file).catch(() => null)))).filter((item): item is Attachment => item !== null);
    if (added.length) draftImages[id] = [...(draftImages[id] ?? []), ...(await Promise.all(added.map(keepPicture)))];
    if (documents.length) await addDocuments(documents, id);
  }

  /** Documents picked or dropped as File objects: written to disk (the agent is given the path). */
  async function addDocuments(documents: File[], id: string) {
    if (!inTauri()) {
      toast.error('Attaching documents needs the desktop app');
      return;
    }
    const added: FileAttachment[] = [];
    for (const file of documents) {
      if (file.size > DOC_MAX) {
        toast.error(`${file.name} is larger than 25 MB`);
        continue;
      }
      try {
        const path = await saveFileAttachment(file.name || 'file', await readBase64(file));
        added.push({
          id: crypto.randomUUID(),
          name: file.name || 'file',
          path,
          size: file.size,
        });
      } catch {
        toast.error(`Could not attach ${file.name}`);
      }
    }
    if (added.length) draftFiles[id] = [...(draftFiles[id] ?? []), ...added];
  }

  /** Files dropped on the window arrive as paths (Tauri's drop event): used where they are, pictures are read for a thumbnail. */
  async function addPaths(paths: string[]) {
    const id = sessionId;
    const docs: FileAttachment[] = [];
    const pics: Attachment[] = [];
    const { invoke } = await import('@tauri-apps/api/core');
    for (const path of paths) {
      const name = path.split(/[\/]/).pop() ?? path;
      try {
        if (isImageName(name) && images.length + pics.length < MAX_IMAGES) {
          const base64 = await invoke<string>('file_read_bytes', {
            root: path.replace(/[\/][^\/]*$/, ''),
            path,
          });
          const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
          const picture = await toAttachment(new File([bytes], name, { type: imageMime(name) }));
          if (picture) pics.push(picture);
          continue;
        }
        const size = await fileSize(path);
        if (size === null) continue; // a folder
        if (size > DOC_MAX) {
          toast.error(`${name} is larger than 25 MB`);
          continue;
        }
        docs.push({ id: crypto.randomUUID(), name, path, size });
      } catch {
        toast.error(`Could not attach ${name}`);
      }
    }
    if (pics.length) draftImages[id] = [...(draftImages[id] ?? []), ...(await Promise.all(pics.map(keepPicture)))];
    if (docs.length) draftFiles[id] = [...(draftFiles[id] ?? []), ...docs];
  }

  // Tauri swallows the browser's own file drop, so the window's drop event is used: a drop over the conversation area
  // attaches the files.
  let formEl: HTMLFormElement | undefined = $state();
  $effect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    let gone = false;
    void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
      const stop = await getCurrentWebview().onDragDropEvent((event) => {
        const area = formEl?.closest('main')?.getBoundingClientRect();
        const payload = event.payload;
        if (payload.type === 'leave') {
          dragging = false;
          return;
        }
        const ratio = window.devicePixelRatio || 1;
        const inside = !!area && payload.position.x / ratio >= area.left && payload.position.x / ratio <= area.right && payload.position.y / ratio >= area.top && payload.position.y / ratio <= area.bottom;
        if (payload.type === 'drop') {
          dragging = false;
          if (inside && payload.paths.length) void addPaths(payload.paths);
        } else dragging = inside;
      });
      if (gone) stop();
      else off = stop;
    });
    return () => {
      gone = true;
      off?.();
    };
  });

  /** Restores every part carried by Copy all through the same attachment queues used by keyboard paste. */
  function restoreClip(clip: NonNullable<ReturnType<typeof clipFromPaste>>) {
    if (hasMarkdown(clip.text)) attachMarkdown(clip.text, sessionId);
    else {
      const range = caret();
      setDraft(`${draft.slice(0, range.start)}${clip.text}${draft.slice(range.end)}`);
    }
    if (clip.images.length)
      setImages([
        ...images,
        ...clip.images.map((item) => ({
          ...item,
          id: crypto.randomUUID(),
        })),
      ]);
    if (clip.elements.length)
      draftElements[sessionId] = [
        ...elements,
        ...clip.elements.map((item) => ({
          ...item,
          id: crypto.randomUUID(),
        })),
      ];
    if (clip.files.length)
      setFiles([
        ...attached,
        ...clip.files.map((item) => ({
          ...item,
          id: crypto.randomUUID(),
        })),
      ]);
  }

  async function onMenuPaste(payload: { html: string; text: string; files: File[] }): Promise<boolean> {
    const clip = clipFromContents(payload.html, payload.text);
    if (clip) {
      restoreClip(clip);
      return true;
    }
    if (payload.files.length) {
      await addFiles(payload.files);
      return true;
    }
    return false;
  }

  function onPaste(event: ClipboardEvent) {
    // Pasting what "Copy all" copied from a message brings back its text, pictures, notes and files.
    const clip = clipFromPaste(event.clipboardData);
    if (clip) {
      event.preventDefault();
      restoreClip(clip);
      return;
    }
    const files = [...(event.clipboardData?.files ?? [])];
    if (files.length === 0) {
      const text = event.clipboardData?.getData('text/plain') ?? '';
      if (routePastedText(text)) event.preventDefault();
      else {
        event.preventDefault();
        const range = caret();
        if (!area) return;
        replaceEditorRange(area, range.start, range.end, text);
        setDraft(editorText(area));
        resize();
      }
      return;
    }
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

  let queueChoiceOpen = $state(false);

  function submit(whileRunning: 'queue' | 'next' | 'interrupt' | 'resume' = 'queue', resumeChoice = false) {
    if (!hasInput) return;
    // Handing work to an agent needs words that say what the work is.
    if (queuePaused[sessionId] && !resumeChoice) {
      queueChoiceOpen = true;
      return;
    }
    delete manuallyStopped[sessionId];
    sendMessage(draft, images, elements, mentions, whileRunning, attached);
    // Sent by hand: the chat goes to its newest content, even if it was scrolled up.
    window.dispatchEvent(new Event('agentdeck:scrolltoend'));
    draftElements[sessionId] = [];
    detail = null;
    setDraft('', { sent: true });
    setMentions([]);
    setImages([]);
    setFiles([]);
  }

  const queueChoiceItems = $derived<ZDropdownMenuItem[]>([
    {
      label: 'Send now · keep queue paused',
      icon: 'lucideSend' as ZIconName,
      iconSize: 12,
      class: 'text-sm! py-1',
      onClick: () => submit('interrupt', true),
    },
    {
      label: 'Add to queue & resume',
      icon: 'lucideListChecks' as ZIconName,
      iconSize: 12,
      class: 'text-sm! py-1',
      onClick: () => submit('resume', true),
    },
  ]);

  function onKeydown(event: KeyboardEvent) {
    // An open picker takes the arrows, Enter, Tab and Escape.
    if (trigger && picker?.key(event)) {
      if (event.key === 'Escape') dismissed = draft.slice(0, caret().start);
      return;
    }
    if (
      historyOf(sessionId).key(event, (text) => {
        drafts[sessionId] = text;
        requestAnimationFrame(() => {
          if (area) setEditorSelection(area, text.length);
          resize();
        });
      })
    ) {
      return;
    } else if (event.key === 'Enter' && event.shiftKey) {
      event.preventDefault();
      if (area) {
        const range = editorSelection(area);
        replaceEditorRange(area, range.start, range.end, '\n');
        setDraft(editorText(area));
        resize();
      }
    } else if (event.key === 'Enter') {
      event.preventDefault();
      // Enter queues at the end; Alt+Enter queues next without interrupting; Ctrl/Cmd+Enter stops and sends now.
      submit(event.altKey ? 'next' : event.ctrlKey || event.metaKey ? 'interrupt' : 'queue');
    }
  }
</script>

<svelte:window bind:innerWidth={innerW} />
{#if waiting.length > 0 || queuePaused[sessionId]}
  <ComposerQueue {sessionId} {waiting} {waitingForUser} onedit={editQueued} />
{/if}
<form
  bind:this={formEl}
  class="relative z-10 min-w-0 rounded-2xl border bg-card p-2 shadow-lg transition-shadow {dragging ? 'border-primary ring-2 ring-primary/40' : 'border-input'}"
  onsubmit={(event) => {
    event.preventDefault();
    submit();
  }}
  ondragover={(event) => {
    if (event.dataTransfer?.types.includes('Files')) {
      event.preventDefault();
      dragging = true;
    }
  }}
  ondragleave={() => (dragging = false)}
  ondrop={onDrop}
>
  {#if elements.length > 0}
    <!-- Elements picked in the browser travel with the message; click one to see what is included. -->
    <div class="mb-2 flex flex-wrap gap-1.5" aria-label="Picked elements">
      {#each elements as item (item.id)}
        <div class="group/chip relative" bind:this={chipAnchors[item.id]}>
          <button type="button" class="flex h-8 max-w-60 cursor-pointer items-center gap-1.5 rounded-md border px-2 font-mono text-[0.6875rem] outline-none {detail === item.id ? `${TRIGGER_ON} border-primary/60 ring-1 ring-primary/60` : 'border-border bg-muted/40 text-muted-foreground hover:bg-muted'}" aria-label="Details of {item.selector}" aria-expanded={detail === item.id} onclick={() => (detail = detail === item.id ? null : item.id)}>
            <ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 shrink-0 text-primary" />
            {#if item.picture}<img src={item.picture} alt="" class="size-5 shrink-0 rounded-sm border border-border object-cover" />{/if}
            <span class="truncate">&lt;{item.tag} /&gt; {item.text}</span>
          </button>
          <button type="button" class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/chip:opacity-100 focus-visible:opacity-100" aria-label="Remove element" onclick={() => (draftElements[sessionId] = elements.filter((entry) => entry !== item))}>
            <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
          </button>
        </div>
      {/each}
    </div>
    {#each elements.filter((item) => item.id === lastDetail) as item (item.id)}
      <ZPopover anchor={chipAnchors[item.id]} open={detail === item.id} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={360} onClose={() => (detail = null)}>
        {#snippet children()}
          <ElementCard tag={item.tag} text={item.text} selector={item.selector} url={item.url} note={item.note} box={item.box} style={item.style} picture={item.picture} />
        {/snippet}
      </ZPopover>
    {/each}
  {/if}
  <ComposerChips {images} {attached} {setImages} {setFiles} />

  <div class="relative min-w-0">
    <div
      bind:this={area}
      contenteditable="true"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      data-composer-editor
      oninput={() => {
        if (!area) return;
        setDraft(editorText(area));
        resize();
        detectTrigger();
      }}
      onscroll={resize}
      onkeyup={(event) => (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'Home' || event.key === 'End') && detectTrigger()}
      onclick={detectTrigger}
      onblur={() => (trigger = null)}
      onkeydown={onKeydown}
      onpaste={onPaste}
      oncontextmenu={onTextAreaContextMenu}
      aria-label="Message"
      data-placeholder={running ? (listenerReady ? `Enter starts a new reply · Alt+Enter queues · ${mac ? '⌘' : 'Ctrl'}+Enter interrupts` : `Enter queues · Alt+Enter queues next · ${mac ? '⌘' : 'Ctrl'}+Enter sends now`) : 'Do anything…'}
      class="composer-editor relative max-h-45 min-h-[50px] w-[calc(100%+0.5rem)] overflow-x-hidden break-words border-0 bg-transparent pt-0 pr-3 pb-1 pl-0.5 text-sm leading-normal text-foreground caret-foreground selection:bg-primary/25 outline-none"
    ></div>
  </div>
  <TextEditContextMenu bind:this={textEditMenu} onPasteText={routePastedText} onPasteClipboard={onMenuPaste} />
  <Menu items={queueChoiceItems} bind:open={queueChoiceOpen} anchor={area ?? null} position="top-right" width={248} label="Choose how to send" />

  <div class="flex min-w-0 items-center justify-between gap-2">
    <div class="flex min-w-0 flex-1 items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Attach files" title="Attach files (or paste / drop)" onclick={() => fileInput?.click()}>
        <ZIcon zType="lucidePaperclip" class="size-3.5" />
      </ZButton>
      <input
        bind:this={fileInput}
        type="file"
        multiple
        hidden
        onchange={(event) => {
          void addFiles([...(event.currentTarget.files ?? [])]);
          event.currentTarget.value = '';
        }}
      />
      <ZButton zWave={false} zType="ghost" zSize="sm" class="hidden size-7! min-[720px]:inline-flex {paletteOpen ? 'bg-primary/15! text-primary!' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}" aria-label="Mention a session, project or AI" title="Mention (@)" onclick={startMention}>
        <ZIcon zType="lucideAtSign" class="size-3.5" />
      </ZButton>
      <ModelPicker bind:agent bind:model bind:effort bind:fast bind:permission />
    </div>
    <!-- Keep one button mounted so stop, resume and send trade icons with the same gentle cross-fade. -->
    <span class="relative grid shrink-0 place-items-center">
      {#if action === 'stop'}
        <span class="pointer-events-none absolute -inset-[3px] animate-spin rounded-full [background:conic-gradient(from_0deg,transparent_0_55%,var(--color-primary)_100%)] [mask:radial-gradient(farthest-side,transparent_calc(100%-2px),#000_calc(100%-2px))] motion-reduce:hidden" aria-hidden="true"></span>
        <span class="pointer-events-none absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:1.8s] motion-reduce:hidden" aria-hidden="true"></span>
      {/if}
      <ZButton
        zWave={false}
        type={action === 'send' ? 'submit' : 'button'}
        zType="default"
        zSize="default"
        class="relative size-7! shrink-0 rounded-full p-0"
        disabled={action === 'send' && !hasInput}
        aria-label={action === 'stop' ? 'Stop' : action === 'resume' ? 'Continue' : queuePaused[sessionId] ? 'Choose how to send' : running ? (listenerReady ? 'Start a new reply while background work continues' : 'Queue the message') : 'Send message'}
        {@attach zTooltip({
          content: action === 'stop' ? 'Stop' : action === 'resume' ? 'Continue' : queuePaused[sessionId] ? 'Queue paused · choose Send now or Add & resume' : running ? `One answer per message · Alt+Enter queues next · ${mac ? '⌘' : 'Ctrl'}+Enter stops and sends now` : 'Send',
          position: 'top',
        })}
        onclick={() => {
          if (action === 'stop') stopTurnForResume(sessionId);
          else if (action === 'resume') resumeStoppedTurn(sessionId);
        }}
      >
        {#key action}
          <span in:fade={{ duration: 120 }} out:fade={{ duration: 120 }} class="grid size-full place-items-center motion-reduce:transition-none">
            {#if action === 'stop'}
              <svg class="size-3" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="currentColor" /></svg>
            {:else if action === 'resume'}
              <svg class="size-3.5" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.7a1 1 0 0 1 1.52-.85l8 5.3a1 1 0 0 1 0 1.7l-8 5.3A1 1 0 0 1 4 13.3z" fill="currentColor" /></svg>
            {:else}
              <ZIcon zType="lucideArrowUp" class="size-3.5" />
            {/if}
          </span>
        {/key}
      </ZButton>
    </span>
  </div>
</form>
<ComposerQuota {agent} {model} />
<ProjectBar />

<style>
  .composer-editor {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    overflow-y: auto;
  }
  .composer-editor:empty::before {
    content: attr(data-placeholder);
    color: var(--color-muted-foreground);
    pointer-events: none;
  }
</style>

<!-- The pickers open over the message box, left-aligned to it, and follow what is typed after `@` or `/`. One at a
     time is mounted, so `picker` is always the open one. -->
{#if slashOpen && !categoryMode}
  <PickerPopover bind:this={picker} anchor={formEl} open={slashOpen} items={slashItems} query={slashQuery} position="top-left" width={formEl?.clientWidth ? Math.min(formEl.clientWidth, 260) : 260} label="Commands and skills" empty="No command matches" footer="↑↓ navigate · ↵ use · esc close" onpick={pickSlash} onclose={() => (trigger = null)} />
{:else if mentionPaletteOpen}
  <PickerPopover
    bind:this={picker}
    anchor={formEl}
    open={mentionPaletteOpen}
    items={mentionItems}
    query={mentionSearch}
    position="top-left"
    width={formEl?.clientWidth ? Math.min(formEl.clientWidth, 360) : 340}
    label={categoryMode === 'session' ? 'Chats' : categoryMode === 'project' ? 'Projects' : categoryMode === 'ai' ? 'Models' : categoryMode === 'workflow' ? 'Flows' : 'Mention a chat, a project, a model or a flow'}
    rowHeight={34}
    empty="No matches"
    footer="@ mention · /c chat · /p project · /m model · /w flow"
    onpick={pickMention}
    onclose={() => (trigger = null)}
  />
{/if}
