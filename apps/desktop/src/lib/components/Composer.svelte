<script lang="ts">
  import ProjectBar from './ProjectBar.svelte';
  import AgentLogo from './AgentLogo.svelte';
  import { untrack } from 'svelte';
  import { getPref, setPref } from '../prefs';
  import {
    ZButton,
    ZIcon,
    ZPopover,
    zSortable,
    zTooltip,
  } from '@shival99/z-ui-svelte';
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
  import type {
    ZCommandGroup,
    ZCommandItem,
    ZIconName,
  } from '@shival99/z-ui-svelte';
  import { loadSlash, slash, type SlashItem } from '../slash.svelte';
  import { prepare, search } from '../palette';
  import ElementCard from './ElementCard.svelte';
  import { TRIGGER_ON } from '../popover';
  import { agyBlocks, agyModes, loadAgyModes } from '../agy-modes.svelte';
  import {
    activeSession,
    AGENTS,
    draftElements,
    draftImages,
    draftFiles,
    draftMentions,
    drafts,
    isRunning,
    needsUserInput,
    lastChat,
    rememberMixed,
    MODELS,
    projectOf,
    queued,
    steerQueued,
    type Queued,
    sendMessage,
    prewarmChat,
    stopTurn,
    AGENT_ICON as agentIcon,
    turnOptions,
    workspace,
    type Agent,
    type Effort,
    type Attachment,
    type FileAttachment,
    type Mention,
    type Permission,
  } from '../sessions.svelte';
  import { openAttachment, openImages } from '../preview.svelte';
  import FileChip from './FileChip.svelte';
  import { clipFromPaste, imageMime, isImageName } from '../attachments';
  import { createTextHistory, type TextHistory } from '../text-history';
  import { fileSize, saveFileAttachment } from '../agent';
  import { customProviderOf } from '../provider-registry';
  import { onUsage, shortCount, totalTokens, usageLog, usd } from '../usage';
  import { CREDIT_PREFIX } from '../credit-model';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import ModelPicker from './ModelPicker.svelte';
  import {
    connectors,
    refreshAccounts,
    refreshAgyQuota,
    refreshProviderQuota,
    providerQuotas,
    claudeCreditState,
    refreshClaudeCredit,
    type UsageWindow,
  } from '../connectors.svelte';

  // What is typed belongs to the open session: switch away and back and it is still there.
  const sessionId = $derived(workspace.activeId);
  const draft = $derived(drafts[sessionId] ?? '');
  /** Recognize common Markdown without parsing the entire draft on every keystroke. */
  function hasMarkdown(text: string): boolean {
    return /(^|\n)\s{0,3}(#{1,6}\s|>\s|[-*+]\s|\d+[.)]\s|`{3}|~{3}|[-*_]{3,}\s*$)|\*\*[^\n]+\*\*|__[^\n]+__|~~[^\n]+~~|`[^`\n]+`|(?:^|\s)(?:\*[^*\n]+\*|_[^_\n]+_)(?=$|[\s.,!?])|!?\[[^\]]+\]\([^)]+\)|(^|\n)[^\n]*\|[^\n]*\n\s*\|?\s*:?-{3}/m.test(text);
  }
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
  const mac =
    typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);
  const waiting = $derived(queued[sessionId] ?? []);
  const waitingForUser = $derived(needsUserInput(sessionId));

  // Queued messages are reordered by dragging their handle (zSortable: the rows glide, Escape cancels).
  function moveQueued(from: number, to: number) {
    if (from === to) return;
    const list = [...waiting];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    queued[sessionId] = list;
  }
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
      area?.setSelectionRange(item.text.length, item.text.length);
      resize();
    });
  }
  const hasInput = $derived(
    draft.trim().length > 0 ||
      images.length > 0 ||
      elements.length > 0 ||
      attached.length > 0,
  );
  let detail = $state<string | null>(null);
  let chipAnchors = $state<Record<string, HTMLElement | null>>({});
  const setImages = (value: Attachment[]) => {
    draftImages[sessionId] = value;
  };
  let fileInput: HTMLInputElement | undefined = $state();
  let dragging = $state(false);
  let area: HTMLTextAreaElement | undefined = $state();
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
    const caret = area.selectionStart ?? 0;
    const before = draft.slice(0, caret);
    if (area.selectionEnd !== caret || before === dismissed)
      return void (trigger = null);
    dismissed = null;
    const found = /(^|\s)([@/])([^\s@/]*)$/.exec(before);
    trigger = found
      ? {
          char: found[2] as '@' | '/',
          at: caret - found[3].length - 1,
          query: found[3],
        }
      : null;
  }
  /** Takes the typed `@query` or `/query` out of the box and puts `insert` in its place. */
  function replaceTrigger(insert: string) {
    if (!trigger) return;
    const caret = area?.selectionStart ?? draft.length;
    const start = trigger.at;
    setDraft(`${draft.slice(0, start)}${insert}${draft.slice(caret)}`);
    trigger = null;
    queueMicrotask(() => {
      area?.focus();
      area?.setSelectionRange(start + insert.length, start + insert.length);
      resize();
    });
  }
  /** The @ button: an `@` where the caret is, which opens the mention picker. */
  function startMention() {
    const at = area?.selectionStart ?? draft.length;
    const before = draft.slice(0, at);
    const lead = before && !/\s$/.test(before) ? ' ' : '';
    setDraft(`${before}${lead}@${draft.slice(at)}`);
    queueMicrotask(() => {
      area?.focus();
      const caret = at + lead.length + 1;
      area?.setSelectionRange(caret, caret);
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

  // Keep the quota shown below the composer current for the provider selected here. These are the same
  // provider readings shown in Settings; providers that do not report a limit stay explicitly unavailable.
  $effect(() => {
    const selected = agent;
    if (!inTauri()) return;
    const refresh = () => {
      if (selected === 'antigravity') void refreshAgyQuota();
      else if (selected === 'claude' || selected === 'codex')
        void refreshAccounts(selected);
    };
    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => clearInterval(timer);
  });

  // A custom provider (OpenRouter…) of the chosen model: its credit left when it tells it, else what was used today.
  const customProvider = $derived(agent === 'custom' ? customProviderOf(model) : undefined);
  $effect(() => {
    const provider = customProvider;
    if (!provider || !inTauri()) return;
    void refreshProviderQuota(provider);
    const timer = window.setInterval(() => void refreshProviderQuota(provider), 120_000);
    return () => clearInterval(timer);
  });
  /** What this provider's key can still spend (OpenRouter), or its free-tier note. */
  const keyQuota = $derived(customProvider ? providerQuotas[customProvider.id] : undefined);

  // A Claude model billed to the Console API credit: the credit left, read from the Console now and then and after
  // each reply (a reply spends some of it).
  const onCredit = $derived(agent === 'claude' && model.startsWith(CREDIT_PREFIX));
  $effect(() => {
    if (!onCredit || !inTauri()) return;
    void refreshClaudeCredit();
    const timer = window.setInterval(() => void refreshClaudeCredit(), 120_000);
    const stop = onUsage(() => window.setTimeout(() => void refreshClaudeCredit(), 3000));
    return () => {
      clearInterval(timer);
      stop();
    };
  });
  const creditReading = $derived(onCredit ? claudeCreditState.reading : null);

  // What was used today with this agent (or custom provider), counted from the replies the app finished.
  let todayUsed = $state<{ tokens: number; replies: number; cost: number } | null>(null);
  async function readToday() {
    const log = await usageLog();
    const from = new Date();
    from.setHours(0, 0, 0, 0);
    const mine = log.filter((item) => new Date(item.at) >= from && item.agent === agent && (agent !== 'custom' || customProviderOf(item.model)?.id === customProvider?.id) && (agent !== 'claude' || (item.model ?? '').startsWith(CREDIT_PREFIX) === onCredit));
    todayUsed = { tokens: mine.reduce((sum, item) => sum + totalTokens(item.tokens), 0), replies: mine.length, cost: mine.reduce((sum, item) => sum + (item.cost ?? 0), 0) };
  }
  $effect(() => {
    void agent;
    void customProvider?.id;
    void onCredit;
    void readToday();
    return onUsage(() => void readToday());
  });

  const quota = $derived.by(() => {
    let windows: UsageWindow[] = [];
    let observedAt: number | null = null;
    if (onCredit) {
      // The credit left of what it was at its fullest (the grant, when the Console lists it).
      const top = Math.max(claudeCreditState.top, creditReading?.granted ?? 0, creditReading?.balance ?? 0);
      if (creditReading && top > 0) {
        windows = [{ label: `${usd(creditReading.balance)} credit`, pct: Math.min(100, 100 - (creditReading.balance / top) * 100), resets_at: null }];
        observedAt = creditReading.at;
      }
    } else if (keyQuota && keyQuota.limit && keyQuota.remaining !== null) {
      // A key with a credit limit reads like a quota window: the share still left.
      windows = [{ label: 'credit', pct: Math.min(100, ((keyQuota.limit - keyQuota.remaining) / keyQuota.limit) * 100), resets_at: null }];
      observedAt = keyQuota.at;
    } else if (agent === 'copilot') {
      const account = connectors.accounts.copilot?.accounts.find((item) => item.active);
      windows = account?.usage ?? [];
      observedAt = account?.observed_at ?? null;
    } else if (agent === 'claude' || agent === 'codex') {
      const account = connectors.accounts[agent]?.accounts.find(
        (item) => item.active,
      );
      windows = account?.usage ?? [];
      observedAt = account?.observed_at ?? null;
    } else if (agent === 'antigravity') {
      const family = model.startsWith('gemini-')
        ? 'gemini'
        : /^(claude-|gpt-oss-)/.test(model)
          ? '3p'
          : null;
      windows =
        connectors.agyQuota?.groups.find((group) => group.id === family)
          ?.windows ?? [];
      observedAt = connectors.agyQuota?.observed_at ?? null;
    }
    // The most restrictive window: the one with the least left. A window that already reset is full again. A 5h window
    // that is fresh does not help when the 7d one is used up: the next message would be refused all the same.
    const now = Date.now() / 1000;
    const left = (item: UsageWindow) =>
      item.resets_at !== null && item.resets_at <= now
        ? 100
        : Math.max(0, Math.min(100, 100 - item.pct));
    const minutesFor = (label: string) => {
      const match = /^(\d+)(m|h|d)$/.exec(label);
      if (!match) return Number.POSITIVE_INFINITY;
      const amount = Number(match[1]);
      return amount * (match[2] === 'd' ? 1440 : match[2] === 'h' ? 60 : 1);
    };
    const selected = [...windows].sort(
      (a, b) => left(a) - left(b) || minutesFor(a.label) - minutesFor(b.label),
    )[0];
    if (!selected) return null;
    return {
      label: selected.label,
      remaining: left(selected),
      resetsAt: selected.resets_at,
      observedAt,
    };
  });
  const quotaUnavailable = $derived(!quota);
  let previousQuotaRemaining: number | null = null;
  let quotaTipChanged = $state(false);
  let quotaTipTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const remaining = quota?.remaining;
    if (remaining === undefined) {
      previousQuotaRemaining = null;
      quotaTipChanged = false;
      return;
    }
    if (
      previousQuotaRemaining !== null &&
      previousQuotaRemaining !== remaining
    ) {
      quotaTipChanged = true;
      clearTimeout(quotaTipTimer);
      quotaTipTimer = setTimeout(() => (quotaTipChanged = false), 900);
    }
    previousQuotaRemaining = remaining;
  });

  function quotaResetText(timestamp: number | null) {
    if (timestamp === null) return '';
    const days = Math.floor((timestamp * 1000 - Date.now()) / 86_400_000);
    if (days >= 1) {
      const hours = Math.round(
        ((timestamp * 1000 - Date.now()) % 86_400_000) / 3_600_000,
      );
      return `resets in ${days}d${hours ? ` ${hours}h` : ''}`;
    }
    const minutes = Math.max(
      0,
      Math.ceil((timestamp * 1000 - Date.now()) / 60_000),
    );
    if (minutes === 0) return 'resets now';
    if (minutes < 60) return `resets in ${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    return `resets in ${hours}h${rest ? ` ${rest}m` : ''}`;
  }

  function quotaAgeText(timestamp: number | null) {
    if (timestamp === null) return '';
    const minutes = Math.max(
      0,
      Math.floor((Date.now() / 1000 - timestamp) / 60),
    );
    return minutes < 1 ? 'updated just now' : `updated ${minutes}m ago`;
  }
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
    if (agent === 'antigravity' && agyModes.loaded && agyBlocks(permission))
      permission = 'ask';
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
      if (
        session.draft &&
        lastChat.agent &&
        lastChat.agent === session.agent &&
        lastChat.model === session.model
      )
        effort = lastChat.effort;
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
      const { sessionId: id, agent: next, model: nextModel } = (event as CustomEvent<{ sessionId: string; agent: Agent; model: string }>).detail;
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
          session.agents = [
            ...new Set([
              ...(session.agents ?? [session.source ?? session.agent]),
              current.agent,
            ]),
          ];
          rememberMixed(session);
        }
        session.agent = current.agent;
        session.model = current.model;
      }
    });
  });

  // ---------- slash commands ----------
  // The agent's own commands and skills, plus AgentDeck's: read for the agent and project of this chat.
  const projectPath = $derived(
    workspace.projects.find(
      (item) =>
        item.id ===
        workspace.sessions.find((entry) => entry.id === sessionId)?.projectId,
    )?.path ?? null,
  );
  $effect(() => {
    void loadSlash(agent, projectPath);
  });
  const agentLabel = $derived(
    AGENTS.find((entry) => entry.value === agent)?.label ?? agent,
  );
  const slashGroups = $derived.by<ZCommandGroup<SlashItem>[]>(() => {
    const row = (item: SlashItem): ZCommandItem<SlashItem> => ({
      id: `${item.kind}:${item.origin}:${item.name}`,
      label: item.description
        ? `/${item.name}  ·  ${item.description}`
        : `/${item.name}`,
      icon: (item.kind === 'skill'
        ? 'lucideSparkles'
        : item.kind === 'builtin'
          ? 'lucideZap'
          : 'lucideTerminal') as ZIconName,
      keywords: [item.name, item.description, item.origin],
      data: item,
    });
    const of = (kind: SlashItem['kind']) =>
      slash.items.filter((item) => item.kind === kind).map(row);
    return [
      { heading: 'AgentDeck', items: of('builtin') },
      { heading: `${agentLabel} commands`, items: of('prompt') },
      { heading: `${agentLabel} skills`, items: of('skill') },
    ].filter((group) => group.items.length > 0);
  });

  const slashPrepared = $derived(
    slashOpen
      ? prepare(slashGroups, (item) => String(item.data?.name ?? item.label))
      : [],
  );
  // AgentDeck's own commands lead, so the first row selected is one of them.
  const slashShown = $derived(
    slashOpen ? search(slashPrepared, slashQuery, 40, 'AgentDeck') : [],
  );

  /** The picker's rows: the ranked groups flattened, each row under its group's heading. */
  function flatten<T>(
    shown: ZCommandGroup<T>[],
    row: (item: ZCommandItem<T>, group: string) => PickerItem<T>,
  ): PickerItem<T>[] {
    return shown.flatMap((group) =>
      group.items
        .filter((item) => item.data !== undefined)
        .map((item) => row(item, group.heading ?? '')),
    );
  }
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
  const projectNames = $derived(
    new Map(workspace.projects.map((project) => [project.id, project.name])),
  );
  const groups = $derived<ZCommandGroup<Mention>[]>([
    {
      heading: 'Sessions',
      items: workspace.sessions
        .filter((item) => item.id !== workspace.activeId)
        .map((item) => ({
          id: `session:${item.id}`,
          label: item.title,
          icon: (item.parentId
            ? 'lucideGitFork'
            : 'lucideMessageSquare') as ZIconName,
          keywords: [
            projectNames.get(item.projectId) ?? '',
            item.gitBranch ?? '',
            item.agent,
          ],
          data: {
            kind: 'session' as const,
            id: item.id,
            label: item.title,
          },
        })),
    },
    {
      heading: 'Projects',
      items: workspace.projects.map((item) => ({
        id: `project:${item.id}`,
        label: item.name,
        icon: 'lucideFolder' as ZIconName,
        keywords: [item.path],
        data: {
          kind: 'project' as const,
          id: item.id,
          label: item.name,
        },
      })),
    },
    {
      heading: 'Hand to an agent',
      items: AGENTS.flatMap((entry) =>
        MODELS[entry.value].map((entryModel) => ({
          id: `ai:${entry.value}/${entryModel.value}`,
          label: `${entry.label} · ${entryModel.label}`,
          icon: agentIcon[entry.value],
          data: {
            kind: 'ai' as const,
            id: `${entry.value}/${entryModel.value}`,
            label: `${entry.label} · ${entryModel.label}`,
          },
        })),
      ),
    },
  ]);

  const mentionPrepared = $derived(paletteOpen ? prepare(groups) : []);
  const mentionShown = $derived(
    paletteOpen ? search(mentionPrepared, mentionQuery) : [],
  );

  const mentionItems = $derived(
    flatten(mentionShown, (item, group) => ({
      id: item.id,
      label: item.label,
      hint: item.keywords?.[0] || undefined,
      icon: item.icon,
      agent:
        item.data?.kind === 'ai'
          ? (String(item.data.id).split('/')[0] as Agent)
          : undefined,
      group,
      data: item.data as Mention,
    })),
  );

  function pickMention(item: PickerItem<Mention>) {
    const picked = item.data;
    // The typed `@query` goes: the mention is a chip above the box.
    replaceTrigger('');
    // An AI mention hands the message to that agent: it works in a chat of its own and the answer comes back here.
    // Several can be added; they work side by side.
    if (
      !mentions.some(
        (entry) => entry.kind === picked.kind && entry.id === picked.id,
      )
    )
      setMentions([...mentions, picked]);
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
    canvas
      .getContext('2d')
      ?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    const mime = file.type === 'image/png' ? 'image/png' : 'image/jpeg';
    return {
      id: crypto.randomUUID(),
      name: file.name || 'pasted image',
      mime,
      src: canvas.toDataURL(mime, 0.88),
    };
  }

  const DOC_MAX = 25 * 1024 * 1024; // the backend refuses larger ones too
  const LONG_PASTE_CHARS = 10_000;

  /** Markdown paste travels as an attachment, leaving the composer free for a question. */
  function attachMarkdown(text: string, id: string) {
    void addDocuments([new File([text], 'Pasted Markdown.md', { type: 'text/markdown' })], id);
  }

  async function addFiles(files: File[]) {
    const isPicture = (file: File) =>
      /^image\/(png|jpeg|gif|webp)$/.test(file.type);
    const id = sessionId;
    const pictures = files.filter(isPicture);
    const documents = files.filter((file) => !isPicture(file));
    const room = MAX_IMAGES - images.length;
    const added = (
      await Promise.all(
        pictures
          .slice(0, Math.max(0, room))
          .map((file) => toAttachment(file).catch(() => null)),
      )
    ).filter((item): item is Attachment => item !== null);
    if (added.length) draftImages[id] = [...(draftImages[id] ?? []), ...added];
    if (documents.length) await addDocuments(documents, id);
  }

  const readBase64 = (file: Blob) =>
    new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () =>
        resolve(
          String(reader.result).slice(String(reader.result).indexOf(',') + 1),
        );
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });

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
        const path = await saveFileAttachment(
          file.name || 'file',
          await readBase64(file),
        );
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
          const bytes = Uint8Array.from(atob(base64), (char) =>
            char.charCodeAt(0),
          );
          const picture = await toAttachment(
            new File([bytes], name, { type: imageMime(name) }),
          );
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
    if (pics.length) draftImages[id] = [...(draftImages[id] ?? []), ...pics];
    if (docs.length) draftFiles[id] = [...(draftFiles[id] ?? []), ...docs];
  }

  // Tauri swallows the browser's own file drop, so the window's drop event is used: a drop over the conversation area
  // attaches the files.
  let formEl: HTMLFormElement | undefined = $state();
  $effect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    let gone = false;
    void import('@tauri-apps/api/webview').then(
      async ({ getCurrentWebview }) => {
        const stop = await getCurrentWebview().onDragDropEvent((event) => {
          const area = formEl?.closest('main')?.getBoundingClientRect();
          const payload = event.payload;
          if (payload.type === 'leave') {
            dragging = false;
            return;
          }
          const ratio = window.devicePixelRatio || 1;
          const inside =
            !!area &&
            payload.position.x / ratio >= area.left &&
            payload.position.x / ratio <= area.right &&
            payload.position.y / ratio >= area.top &&
            payload.position.y / ratio <= area.bottom;
          if (payload.type === 'drop') {
            dragging = false;
            if (inside && payload.paths.length) void addPaths(payload.paths);
          } else dragging = inside;
        });
        if (gone) stop();
        else off = stop;
      },
    );
    return () => {
      gone = true;
      off?.();
    };
  });

  function onPaste(event: ClipboardEvent) {
    // Pasting what "Copy all" copied from a message brings back its text, pictures, notes and files.
    const clip = clipFromPaste(event.clipboardData);
    if (clip) {
      event.preventDefault();
      if (hasMarkdown(clip.text)) attachMarkdown(clip.text, sessionId);
      else {
        const at = area?.selectionStart ?? draft.length;
        setDraft(`${draft.slice(0, at)}${clip.text}${draft.slice(area?.selectionEnd ?? at)}`);
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
      return;
    }
    const files = [...(event.clipboardData?.files ?? [])];
    if (files.length === 0) {
      const text = event.clipboardData?.getData('text/plain') ?? '';
      if (hasMarkdown(text)) {
        event.preventDefault();
        attachMarkdown(text, sessionId);
        return;
      }
      if (text.length <= LONG_PASTE_CHARS) return; // short plain text pastes as usual
      event.preventDefault();
      // Long text travels as a regular file chip, so it stays out of the prompt until the agent reads the attachment.
      void addDocuments(
        [new File([text], 'Pasted text.txt', { type: 'text/plain' })],
        sessionId,
      );
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

  function submit(whileRunning: 'queue' | 'interrupt' = 'queue') {
    if (!hasInput) return;
    // Handing work to an agent needs words that say what the work is.
    if (mentions.some((entry) => entry.kind === 'ai') && !draft.trim()) return;
    sendMessage(
      draft,
      images,
      elements,
      mentions,
      whileRunning,
      attached,
    );
    // Sent by hand: the chat goes to its newest content, even if it was scrolled up.
    window.dispatchEvent(new Event('agentdeck:scrolltoend'));
    draftElements[sessionId] = [];
    detail = null;
    setDraft('', { sent: true });
    setMentions([]);
    setImages([]);
    setFiles([]);
  }

  function onKeydown(event: KeyboardEvent) {
    // An open picker takes the arrows, Enter, Tab and Escape.
    if (trigger && picker?.key(event)) {
      if (event.key === 'Escape')
        dismissed = draft.slice(0, area?.selectionStart ?? draft.length);
      return;
    }
    if (
      historyOf(sessionId).key(event, (text) => {
        drafts[sessionId] = text;
        requestAnimationFrame(() => {
          area?.setSelectionRange(text.length, text.length);
          resize();
        });
      })
    ) {
      return;
    } else if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      // While the agent works: Enter waits in line (what waits goes out as one message when the reply is done),
      // Ctrl+Enter (Cmd+Enter on a Mac) stops it and goes first.
      submit(event.ctrlKey || event.metaKey ? 'interrupt' : 'queue');
    }
  }
</script>

<svelte:window bind:innerWidth={innerW} />
{#if waiting.length > 0}
  <!-- Queued messages sit behind the composer until the agent is free and any open question or permission is resolved. -->
  <div
    class="mx-3 -mb-4 overflow-hidden rounded-t-2xl border border-b-0 border-border bg-card pb-4 shadow-md"
  >
    <!-- Outside the sortable list, so the status and clear action are not queue rows. -->
    <div
      class="flex items-center justify-between border-b border-border px-4 py-1 text-[11px] text-muted-foreground"
    >
      <span
        >{waiting.length} queued{waitingForUser
          ? ' · waiting for your answer'
          : ''}</span
      >
      <button
        type="button"
        class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-destructive/10 hover:text-destructive"
        onclick={() => delete queued[sessionId]}
      >
        <ZIcon zType={'lucideTrash2' as ZIconName} zSize="12" class="size-3" />
        <span>Clear all</span>
      </button>
    </div>
    <div
      role="list"
      aria-label="Queued messages"
      {@attach zSortable({ onSort: moveQueued, handle: '[data-handle]' })}
    >
      {#each waiting as item (item)}
        <div
          class="flex items-center gap-2.5 border-t border-border bg-card px-4 py-1 text-xs first:border-t-0"
          role="listitem"
        >
          <!-- The list icon turns into a drag handle while the pointer is on it. -->
          <span
            data-handle
            class="group/handle grid size-3.5 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground active:cursor-grabbing"
            aria-label="Drag to reorder"
          >
            <ZIcon
              zType={'lucideListChecks' as ZIconName}
              class="size-3.5 group-hover/handle:hidden"
            />
            <ZIcon
              zType={'lucideGripVertical' as ZIconName}
              class="hidden size-3.5 group-hover/handle:block"
            />
          </span>
          <span class="min-w-0 flex-1 truncate text-foreground"
            >{item.text ||
              (item.images.length
                ? 'Image'
                : item.files?.length
                  ? 'File'
                  : 'Element')}</span
          >
          <span class="flex shrink-0 items-center gap-0.5">
            <button
              type="button"
              class="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
              onclick={() => steerQueued(sessionId, item)}
            >
              <ZIcon zType={'lucideArrowRight' as ZIconName} class="size-3" />
              <span>Steer</span>
            </button>
            <button
              type="button"
              class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
              aria-label="Edit in the message box"
              {@attach zTooltip({
                content: 'Edit',
                position: 'top',
              })}
              onclick={() => editQueued(item)}
            >
              <ZIcon
                zType={'lucideSquarePen' as ZIconName}
                zSize="13"
                class="size-[13px]"
              />
            </button>
            <button
              type="button"
              class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent hover:text-destructive"
              aria-label="Remove from the queue"
              onclick={() =>
                (queued[sessionId] = waiting.filter((entry) => entry !== item))}
            >
              <ZIcon
                zType={'lucideTrash2' as ZIconName}
                zSize="13"
                class="size-[13px]"
              />
            </button>
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}
<form
  bind:this={formEl}
  class="relative z-10 min-w-0 rounded-2xl border bg-card p-2 shadow-lg transition-shadow {dragging
    ? 'border-primary ring-2 ring-primary/40'
    : 'border-input'}"
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
          <button
            type="button"
            class="flex h-8 max-w-60 cursor-pointer items-center gap-1.5 rounded-md border px-2 font-mono text-[11px] outline-none {detail === item.id ? `${TRIGGER_ON} border-primary/60 ring-1 ring-primary/60` : 'border-border bg-muted/40 text-muted-foreground hover:bg-muted'}"
            aria-label="Details of {item.selector}"
            aria-expanded={detail === item.id}
            onclick={() => (detail = detail === item.id ? null : item.id)}
          >
            <ZIcon
              zType={'lucideMousePointerClick' as ZIconName}
              class="size-3.5 shrink-0 text-primary"
            />
            {#if item.picture}<img
                src={item.picture}
                alt=""
                class="size-5 shrink-0 rounded-sm border border-border object-cover"
              />{/if}
            <span class="truncate">&lt;{item.tag} /&gt; {item.text}</span>
          </button>
          <button
            type="button"
            class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/chip:opacity-100 focus-visible:opacity-100"
            aria-label="Remove element"
            onclick={() =>
              (draftElements[sessionId] = elements.filter(
                (entry) => entry !== item,
              ))}
          >
            <ZIcon
              zType="lucideX"
              zSize="12"
              class="block size-[12px]! shrink-0"
            />
          </button>
        </div>
      {/each}
    </div>
    {#each elements.filter((item) => item.id === detail) as item (item.id)}
      <ZPopover
        anchor={chipAnchors[item.id]}
        open={true}
        zTrigger="manual"
        zPosition="top-left"
        zOffset={8}
        zWidth={360}
        onClose={() => (detail = null)}
      >
        {#snippet children()}
          <ElementCard
            tag={item.tag}
            text={item.text}
            selector={item.selector}
            url={item.url}
            note={item.note}
            box={item.box}
            style={item.style}
            picture={item.picture}
          />
        {/snippet}
      </ZPopover>
    {/each}
  {/if}
  {#if images.length > 0}
    <div class="mb-2 flex flex-wrap gap-2" aria-label="Attached images">
      {#each images as image (image.id)}
        <div class="group/img relative">
          <button
            type="button"
            class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring"
            aria-label="Preview {image.name}"
            onclick={() => openImages(images, images.indexOf(image))}
          >
            <img
              src={image.src}
              alt={image.name}
              class="size-full object-cover"
            />
          </button>
          <button
            type="button"
            class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/img:opacity-100 focus-visible:opacity-100"
            aria-label="Remove {image.name}"
            onclick={() => setImages(images.filter((entry) => entry !== image))}
          >
            <ZIcon
              zType="lucideX"
              zSize="12"
              class="block size-[12px]! shrink-0"
            />
          </button>
        </div>
      {/each}
    </div>
  {/if}
  {#if attached.length > 0}
    <div class="mb-2 flex flex-wrap gap-2" aria-label="Attached files">
      {#each attached as file (file.id)}
        <FileChip
          name={file.name}
          path={file.path}
          size={file.size}
          onclick={() => void openAttachment(file.path, file.name, file.size)}
          onremove={() => setFiles(attached.filter((entry) => entry !== file))}
        />
      {/each}
    </div>
  {/if}
  {#if mentions.length > 0}
    <div class="mb-2 flex flex-wrap gap-1.5" aria-label="Mentions">
      {#each mentions as mention (mention.kind + mention.id)}
        <span
          class="inline-flex items-center gap-1 rounded-md bg-primary/10 py-1 pr-0.5 pl-1.5 text-xs text-primary"
        >
          {#if mention.kind === 'ai'}
            <!-- The provider's own logo: the same model name comes from several providers, so the logo tells which one. -->
            <AgentLogo agent={mention.id.split('/')[0]} model={mention.id.slice(mention.id.indexOf('/') + 1)} class="size-3 shrink-0" />
          {:else}
            <ZIcon zType={chipIcon[mention.kind]} class="size-3" />
          {/if}
          {mention.label}
          <button
            type="button"
            class="grid size-5 cursor-pointer place-items-center rounded hover:bg-primary/20"
            aria-label="Remove {mention.label}"
            onclick={() =>
              setMentions(mentions.filter((entry) => entry !== mention))}
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
    oninput={() => (resize(), detectTrigger())}
    onkeyup={(event) =>
      (event.key === 'ArrowLeft' ||
        event.key === 'ArrowRight' ||
        event.key === 'Home' ||
        event.key === 'End') &&
      detectTrigger()}
    onclick={detectTrigger}
    onblur={() => (trigger = null)}
    onkeydown={onKeydown}
    onpaste={onPaste}
    rows="1"
    wrap="soft"
    aria-label="Message"
    placeholder={running
      ? `Enter queues (sent together when done) · ${mac ? '⌘' : 'Ctrl'}+Enter to interrupt`
      : 'Do anything…'}
    class="max-h-45 min-h-10 w-[calc(100%+0.5rem)] resize-none overflow-x-hidden break-words border-0 bg-transparent pt-0 pr-0 pb-1 pl-0.5 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>

  <div class="flex min-w-0 items-center justify-between gap-2">
    <div class="flex min-w-0 flex-1 items-center gap-1">
      <ZButton
        zWave={false}
        zType="ghost"
        zSize="sm"
        class="size-7! !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary"
        aria-label="Attach files"
        title="Attach files (or paste / drop)"
        onclick={() => fileInput?.click()}
      >
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
      <ZButton
        zWave={false}
        zType="ghost"
        zSize="sm"
        class="hidden size-7! min-[720px]:inline-flex {paletteOpen
          ? 'bg-primary/15! text-primary!'
          : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
        aria-label="Mention a session, project or AI"
        title="Mention (@)"
        onclick={startMention}
      >
        <ZIcon zType="lucideAtSign" class="size-3.5" />
      </ZButton>
      <ModelPicker
        bind:agent
        bind:model
        bind:effort
        bind:fast
        bind:permission
      />
    </div>
    {#if running && !hasInput}
      <!-- A reply is being written and nothing is typed: this stops it. A ring turns around it while the reply runs. -->
      <span class="relative grid shrink-0 place-items-center">
        <span
          class="pointer-events-none absolute -inset-[3px] animate-spin rounded-full [background:conic-gradient(from_0deg,transparent_0_55%,var(--color-primary)_100%)] [mask:radial-gradient(farthest-side,transparent_calc(100%-2px),#000_calc(100%-2px))] motion-reduce:hidden"
          aria-hidden="true"
        ></span>
        <span
          class="pointer-events-none absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:1.8s] motion-reduce:hidden"
          aria-hidden="true"
        ></span>
        <ZButton
          zWave={false}
          zType="default"
          zSize="default"
          class="relative size-7! shrink-0 rounded-full p-0"
          aria-label="Stop"
          {@attach zTooltip({ content: 'Stop', position: 'top' })}
          onclick={() => stopTurn(sessionId)}
        >
          <svg class="size-3" viewBox="0 0 16 16" aria-hidden="true"
            ><rect
              x="2.5"
              y="2.5"
              width="11"
              height="11"
              rx="2.5"
              fill="currentColor"
            /></svg
          >
        </ZButton>
      </span>
    {:else}
      <ZButton
        zWave={false}
        type="submit"
        zType="default"
        zSize="default"
        class="size-7! shrink-0 rounded-full p-0"
        disabled={!hasInput}
        aria-label={running ? 'Queue the message' : 'Send message'}
        {@attach zTooltip({
          content: running
            ? `Queue: sent together when the reply is done · ${mac ? '⌘' : 'Ctrl'}+Enter to stop and send now`
            : 'Send',
          position: 'top',
        })}
      >
        <ZIcon zType="lucideArrowUp" class="size-3.5" />
      </ZButton>
    {/if}
  </div>
</form>
{#if quota}
  <div
    class="mt-1.5 flex min-w-0 flex-col gap-1 px-0.5"
    title="Remaining quota for the selected provider's most restrictive window"
  >
    <div class="flex min-w-0 items-center gap-2 text-[10px] leading-[1.25]">
      <span class="min-w-0 flex-1 truncate text-muted-foreground"
        >{quota.label} remaining{quotaResetText(quota.resetsAt)
          ? ` · ${quotaResetText(quota.resetsAt)}`
          : ''}</span
      >
      {#if quotaAgeText(quota.observedAt)}
        <span class="shrink-0 text-[9px] text-muted-foreground"
          >{quotaAgeText(quota.observedAt)}</span
        >
      {/if}
      <span
        class="shrink-0 tabular-nums {quota.remaining <= 10
          ? 'text-destructive'
          : 'text-muted-foreground'}">{Math.round(quota.remaining)}%</span
      >
    </div>
    <div
      class="relative h-1.5 rounded-full bg-muted"
      role="progressbar"
      aria-valuenow={Math.round(quota.remaining)}
      aria-valuemin="0"
      aria-valuemax="100"
      aria-label="{model} {quota.label} quota remaining"
    >
      <div
        class="relative h-full rounded-full transition-[width] duration-500 {quota.remaining <=
        10
          ? 'bg-destructive'
          : quota.remaining <= 30
            ? 'bg-amber-500'
            : 'bg-primary'}"
        class:quota-low={quota.remaining > 0 && quota.remaining <= 20}
        style:width="{quota.remaining}%"
      >
        {#if quota.remaining > 0}
          <span
            class="pointer-events-none absolute inset-0 overflow-hidden rounded-full"
            aria-hidden="true">
            <span class="quota-sheen"></span>
            <span class="quota-droplet" style="--level: 0.5px; --duration: 4.8s; --delay: -1.2s"></span>
            <span class="quota-droplet" style="--level: 2px; --duration: 6.2s; --delay: -3.8s"></span>
            <span class="quota-droplet" style="--level: 1px; --duration: 5.4s; --delay: -4.6s"></span>
          </span>
          <span
            class="absolute -right-1 top-1/2 grid size-2 -translate-y-1/2 place-items-center rounded-full {quota.remaining <=
            10
              ? 'text-destructive'
              : quota.remaining <= 30
                ? 'text-amber-500'
                : 'text-primary'}"
          >
            <!-- The tip glows on and off, like a pulse, so the end of the bar reads as live; a change pings it once more. -->
            <span
              class="quota-tip-halo absolute inset-0 rounded-full bg-current"
            ></span>
            {#if quotaTipChanged}<span
                class="absolute inset-0 rounded-full bg-current opacity-70 motion-safe:animate-ping"
              ></span>{/if}
            <span
              class="relative size-1.5 rounded-full border border-background bg-current shadow-[0_0_5px_currentColor]"
            ></span>
          </span>
        {/if}
      </div>
    </div>
  </div>
{:else if quotaUnavailable}
  <div
    class="mt-1.5 flex min-w-0 flex-col gap-1 px-0.5"
    title="This provider does not currently expose quota data to AgentDeck"
  >
    <span class="min-w-0 truncate text-[10px] leading-[1.25] text-muted-foreground"
      title={onCredit && !connectors.claudeCredit.sessionKey ? 'Add the Console sign-in in Settings > Providers to see the credit left' : undefined}
      >{#if onCredit}{claudeCreditState.error ? `Credit: ${claudeCreditState.error}` : connectors.claudeCredit.sessionKey ? 'Reading credit…' : 'Credit left unknown'}{:else if keyQuota?.free_tier}Free key: only “:free” models work, with daily request limits{:else if keyQuota}Spent {usd(keyQuota.used)} · this key has no limit{:else}This provider reports no limit{/if}{#if todayUsed && todayUsed.replies > 0}
        · today {shortCount(todayUsed.tokens)} tokens{todayUsed.cost > 0 ? ` · ${usd(todayUsed.cost)}` : ''} · {todayUsed.replies} {todayUsed.replies === 1 ? 'reply' : 'replies'}{/if}</span
    >
    <div
      class="h-1.5 rounded-full bg-muted"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-label="{model} quota unavailable"
      aria-valuetext="Quota unavailable"
    >
      <div class="h-full w-full rounded-full bg-muted-foreground/35"></div>
    </div>
  </div>
{/if}
<ProjectBar />

<!-- The pickers open over the message box, left-aligned to it, and follow what is typed after `@` or `/`. One at a
     time is mounted, so `picker` is always the open one. -->
{#if slashOpen}
  <PickerPopover
    bind:this={picker}
    anchor={formEl}
    open={slashOpen}
    items={slashItems}
    query={slashQuery}
    position="top-left"
    width={formEl?.clientWidth ? Math.min(formEl.clientWidth, 260) : 260}
    label="Commands and skills"
    empty="No command matches"
    footer="↑↓ navigate · ↵ use · esc close"
    onpick={pickSlash}
    onclose={() => (trigger = null)}
  />
{:else if paletteOpen}
  <PickerPopover
    bind:this={picker}
    anchor={formEl}
    open={paletteOpen}
    items={mentionItems}
    query={mentionQuery}
    position="top-left"
    width={formEl?.clientWidth ? Math.min(formEl.clientWidth, 360) : 340}
    label="Mention a chat, a project or an agent"
    rowHeight={34}
    empty="No chat, project or agent matches"
    footer="↑↓ navigate · ↵ add · esc close"
    onpick={pickMention}
    onclose={() => (trigger = null)}
  />
{/if}

<style>
  /* Adapt ZProgress's shine and drifting bubbles to the compact 6px quota track. */
  .quota-sheen {
    position: absolute;
    inset-block: 0;
    left: -38%;
    width: 38%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgb(255 255 255 / 0.12) 24%,
      rgb(255 255 255 / 0.72) 48%,
      rgb(255 255 255 / 0.88) 52%,
      rgb(255 255 255 / 0.12) 76%,
      transparent 100%
    );
    animation: quota-sheen 3s ease-in-out infinite;
  }
  .quota-droplet {
    position: absolute;
    left: -6%;
    bottom: var(--level);
    width: 2.5px;
    height: 2.5px;
    border: 0.5px solid rgb(255 255 255 / 0.75);
    border-radius: 50%;
    background: rgb(255 255 255 / 0.35);
    box-shadow: 0 0 3px rgb(255 255 255 / 0.55);
    animation: quota-droplet var(--duration) linear var(--delay) infinite;
  }
  /* At 20% or less the fill breathes once every couple of seconds; dim, not a strobe. */
  .quota-low {
    animation: quota-low 2.4s ease-in-out infinite;
  }
  .quota-tip-halo {
    animation: quota-tip-halo 2s ease-out infinite;
  }
  @keyframes quota-tip-halo {
    0% {
      transform: scale(0.8);
      opacity: 0.55;
    }
    70%,
    100% {
      transform: scale(2.6);
      opacity: 0;
    }
  }
  @keyframes quota-sheen {
    0% { left: -38%; }
    80%, 100% { left: 100%; }
  }
  @keyframes quota-droplet {
    from { left: -6%; }
    to { left: 106%; }
  }
  @keyframes quota-low {
    0%,
    70%,
    100% {
      opacity: 1;
    }
    82% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .quota-sheen,
    .quota-droplet { display: none; }
    .quota-low {
      animation: none;
    }
    .quota-tip-halo {
      animation: none;
      opacity: 0;
    }
  }
</style>
