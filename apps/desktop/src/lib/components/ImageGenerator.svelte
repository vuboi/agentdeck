<script lang="ts">
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
  import { ACTIVE_ON } from '../popover';
  import { onMount, tick, untrack } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { ZDrawer, ZIcon, ZPopover, ZSwitch, ZTabs, createVirtualizer, zHoverGlide, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { OPENAI_SVG, OPENCODE_SVG } from '../brand-icons';
  import { activeSession, draftImages, workspace, type Attachment } from '../sessions.svelte';
  import {
    revealFile,
    saveAttachment,
    type ImageHistoryRun,
    type ImageModelChoice,
    type ImageSettings,
    type SuggestedKeyword,
  } from '../agent';
  import { openImages } from '../preview.svelte';
  import { clearSuggestions, draft, gen, openRun, refreshHistory, runSuggest, startRun, stopRun, type Preview } from '../image-gen.svelte';
  import { findModel, freeFirst, modelCatalog, modelLabel, type ProviderId } from '../models.svelte';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import DevEmpty from './DevEmpty.svelte';
  import SectionTitle from './SectionTitle.svelte';
  import ImageDeveloping from './ImageDeveloping.svelte';

  type ImageModel = ImageModelChoice & { label: string };

  const models: ImageModel[] = [
    // Only models verified against this flow's Responses or Images backend belong here.
    { model: 'gpt-6.1-sol', backend: 'responses', label: 'GPT 6.1 Sol' },
    { model: 'gpt-6-astra', backend: 'responses', label: 'GPT 6 Astra' },
    { model: 'gpt-6-sol', backend: 'responses', label: 'GPT 6 Sol' },
    { model: 'gpt-6-luna', backend: 'responses', label: 'GPT 6 Luna' },
    { model: 'gpt-5.6-sol', backend: 'responses', label: 'GPT 5.6 Sol' },
    { model: 'gpt-5.6-terra', backend: 'responses', label: 'GPT 5.6 Terra' },
    { model: 'gpt-5.6-luna', backend: 'responses', label: 'GPT 5.6 Luna' },
    { model: 'gpt-image-2', backend: 'images', label: 'GPT Image 2' },
  ];
  // Match the backend's per-run limit without increasing the default generation workload.
  const MAX_IMAGE_MODELS = 4;
  // A panel that opens by growing from its top edge and goes by shrinking and fading, so what is below it never jumps.
  function fold(node: HTMLElement, { duration = 280 }: { duration?: number } = {}) {
    const grow = slide(node, { duration });
    return { ...grow, css: (t: number, u: number) => `${grow.css?.(t, u) ?? ''};opacity:${t}` };
  }
  const variantTabs = [1, 2, 3, 4].map((count) => ({ value: String(count), label: String(count) }));
  const EFFORT_LABEL: Record<string, string> = { off: 'Off', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'X-High', max: 'Max', ultra: 'Ultra' };
  // The settings under the prompt box. An empty value is Auto: nothing is sent for that setting.
  type SettingKey = 'style' | 'domain' | 'audience' | 'ratio' | 'resolution';
  const SETTINGS: { key: SettingKey; label: string; options: string[] }[] = [
    { key: 'style', label: 'Style', options: ['Photorealistic', 'Anime', '2D illustration', '3D render', 'Watercolor', 'Oil painting', 'Pixel art', 'Flat vector', 'Line art', 'Cyberpunk', 'Vintage film', 'Modern minimal'] },
    { key: 'domain', label: 'Domain', options: ['General', 'Medical and healthcare', 'Education', 'Math and geometry diagram', 'Science and biology', 'Architecture', 'Product and e-commerce', 'Fashion', 'Food', 'Game asset', 'Marketing poster', 'UI mockup'] },
    { key: 'audience', label: 'Audience', options: ['All ages', 'Teen (13+)', 'Adult (18+)'] },
    { key: 'ratio', label: 'Ratio', options: ['1:1', '16:9', '16:10', '4:3', '3:4', '9:16', '21:9', '3:2', '2:3'] },
    { key: 'resolution', label: 'Resolution', options: ['SD (640×480)', 'HD (1280×720)', 'Full HD (1920×1080)', '2K (2560×1440)', '4K (3840×2160)'] },
  ];
  // A small icon in front of the option names that have one (Style and Domain).
  const OPTION_ICON: Record<string, ZIconName> = {
    Photorealistic: 'lucideImage' as ZIconName,
    Anime: 'lucideSparkles' as ZIconName,
    '2D illustration': 'lucidePenLine' as ZIconName,
    '3D render': 'lucideLayers' as ZIconName,
    Watercolor: 'lucidePalette' as ZIconName,
    'Oil painting': 'lucideHighlighter' as ZIconName,
    'Pixel art': 'lucideTable2' as ZIconName,
    'Flat vector': 'lucideSquare' as ZIconName,
    'Line art': 'lucidePencil' as ZIconName,
    Cyberpunk: 'lucideZap' as ZIconName,
    'Vintage film': 'lucideArchive' as ZIconName,
    'Modern minimal': 'lucideMinus' as ZIconName,
    General: 'lucideGlobe' as ZIconName,
    'Medical and healthcare': 'lucideActivity' as ZIconName,
    Education: 'lucideBrain' as ZIconName,
    'Math and geometry diagram': 'lucideSlidersHorizontal' as ZIconName,
    'Science and biology': 'lucideFlaskConical' as ZIconName,
    Architecture: 'lucideColumns3' as ZIconName,
    'Product and e-commerce': 'lucideCoins' as ZIconName,
    Fashion: 'lucideHand' as ZIconName,
    Food: 'lucideCookie' as ZIconName,
    'Game asset': 'lucidePlay' as ZIconName,
    'Marketing poster': 'lucideBell' as ZIconName,
    'UI mockup': 'lucideAppWindow' as ZIconName,
  };
  // The icon of each setting, shown on its chip while nothing is picked (a picked option shows its own icon).
  const SETTING_ICON: Record<SettingKey, ZIconName> = {
    style: 'lucidePalette' as ZIconName,
    domain: 'lucideGlobe' as ZIconName,
    audience: 'lucideShieldCheck' as ZIconName,
    ratio: 'lucideSquare' as ZIconName,
    resolution: 'lucideMonitor' as ZIconName,
  };
  const SETTING_CATEGORY: Record<string, SettingKey> ={ style: 'style', domain: 'domain', audience: 'audience', age: 'audience', ratio: 'ratio', resolution: 'resolution' };
  const MAX_REFERENCES = 5;
  const PICTURE = /^image\/(png|jpeg|gif|webp)$/;
  // One highlight box glides between the hovered rows of a menu, as in the chat's model picker.
  const glide = () => zHoverGlide({ class: 'bg-primary/10 rounded-md', exclude: '[aria-checked="true"]' });

  // A picture pasted or dropped into the prompt box: shown above it, sent to the model with the prompt.
  type Reference = { id: string; name: string; mime: string; src: string; path?: string };
  let references = $state<Reference[]>([...draft.references]);
  let settings = $state<Record<SettingKey, string>>({ ...draft.settings });
  let settingKey = $state<SettingKey | null>(null);
  let settingAnchor = $state<HTMLElement | null>(null);
  let settingOpen = $state(false);
  const suggestedKeywords = $derived(gen.keywords);
  const currentOptions = $derived(SETTINGS.find((item) => item.key === settingKey)?.options ?? []);
  const settingFitsContent = $derived(settingKey === 'audience' || settingKey === 'ratio' || settingKey === 'resolution');
  let settingQuery = $state('');
  // The options as the app's one list: Auto first, the chosen one ticked.
  const settingItems = $derived<PickerItem<string>[]>(
    ['', ...currentOptions].map((option) => ({ id: option || '::auto', label: option || 'Auto', icon: option ? OPTION_ICON[option] : undefined, selected: (settingKey ? settings[settingKey] : '') === option, data: option })),
  );

  let prompt = $state(draft.prompt);
  let selectedModels = $state<string[]>(draft.selectedModels ?? ['gpt-6-luna', 'gpt-image-2']);
  let variants = $state(draft.variants);
  let fast = $state(draft.fast);
  let effort = $state(draft.effort);
  let failuresOpen = $state(false);
  // The run lives in the store, so it goes on while this panel is closed.
  const working = $derived(gen.working);
  const stopping = $derived(gen.stopping);
  const previews = $derived<Preview[]>(gen.previews);
  const failureLines = $derived(gen.failureLines);
  // Time of the current or last run, shown beside the history button.
  const runMs = $derived(gen.runMs);
  const loadingRun = $derived(gen.loadingRun);
  const runSkeleton = $derived(gen.runSkeleton);
  const history = $derived(gen.history);
  const activeRunId = $derived(gen.activeRunId);

  // What the inputs hold is kept for the next time the panel opens.
  $effect(() => {
    draft.prompt = prompt;
    draft.selectedModels = [...selectedModels];
    draft.variants = variants;
    draft.fast = fast;
    draft.effort = effort;
    draft.promptModel = promptModel;
    draft.settings = { ...settings };
    draft.references = [...references];
  });

  // Whole seconds only: "43 s", then "1m 05s", then "1h 02m 05s".
  function formatDuration(ms: number) {
    const total = Math.floor(ms / 1000);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = String(total % 60).padStart(2, '0');
    if (hours) return `${hours}h ${String(minutes).padStart(2, '0')}m ${seconds}s`;
    if (minutes) return `${minutes}m ${seconds}s`;
    return `${total} s`;
  }
  const promptSuggestions = $derived(gen.suggestions);
  let promptModel = $state(draft.promptModel);
  const suggesting = $derived(gen.suggesting);
  let settingsOpen = $state(false);
  let settingsTrigger = $state<HTMLElement | null>(null);
  let modelsOpen = $state(false);
  let modelsRow = $state<HTMLElement | null>(null);
  let promptModelsOpen = $state(false);
  let promptModelsAnchor = $state<HTMLElement | null>(null);
  let promptSubProvider = $state('codex');
  let nestedCloseTimer: ReturnType<typeof setTimeout> | undefined;
  let historyOpen = $state(false);

  // The full prompt of a history row that is cut off, shown in a popover while the pointer rests on the row.
  let peek = $state<ImageHistoryRun | null>(null);
  let peekAnchor = $state<HTMLElement | null>(null);
  let peekOpen = $state(false);
  let peekTimer: ReturnType<typeof setTimeout> | undefined;

  function peekRun(run: ImageHistoryRun, row: HTMLElement) {
    clearTimeout(peekTimer);
    const label = row.querySelector('span');
    if (!label || label.scrollWidth <= label.clientWidth) {
      peekOpen = false;
      return;
    }
    peek = run;
    peekAnchor = row;
    peekOpen = true;
  }

  function leavePeek() {
    clearTimeout(peekTimer);
    peekTimer = setTimeout(() => (peekOpen = false), 150);
  }

  function keepPeek() {
    clearTimeout(peekTimer);
  }

  const variantCount = $derived(Math.max(1, Math.min(4, Math.round(variants))));
  const candidateCount = $derived(selectedModels.length * variantCount);
  const settingsSummary = $derived(selectedModels.map((value) => models.find((item) => item.model === value)?.label ?? value).join(', ') || 'No model');
  // Models that can write the prompt suggestions, grouped by the CLI they run through.
  const promptGroups = $derived.by(() => {
    const catalog = modelCatalog();
    const codex = new Set<string>(['gpt-6-luna']);
    for (const item of catalog.codex) codex.add(item.value);
    const groups = [
      { provider: 'codex', label: 'Codex', items: [...codex].map((value) => ({ value, label: modelLabel(value) })) },
      { provider: 'claude', label: 'Claude', icon: 'brandClaude' as ZIconName, items: catalog.claude.map((item) => ({ value: item.value, label: item.label })) },
      { provider: 'antigravity', label: 'Antigravity', icon: 'brandGemini' as ZIconName, items: catalog.antigravity.map((item) => ({ value: item.value, label: item.label })) },
      { provider: 'opencode', label: 'OpenCode', icon: 'lucideTerminal' as ZIconName, items: catalog.opencode.map((item) => ({ value: item.value, label: item.label })) },
    ];
    return groups;
  });
  const promptModels = $derived(promptGroups.flatMap((group) => group.items));
  // The effort levels follow the model that writes the suggestions, as they do in the chat picker.
  const promptProvider = $derived((promptGroups.find((group) => group.items.some((item) => item.value === promptModel))?.provider ?? 'codex') as ProviderId);
  const effortTabs = $derived.by(() => {
    const levels: string[] = findModel(promptProvider, promptModel)?.efforts ?? ['low', 'medium', 'high', 'max'];
    return levels.map((value) => ({ value, label: EFFORT_LABEL[value] ?? value }));
  });
  $effect(() => {
    const levels = effortTabs.map((item) => item.value);
    if (!levels.includes(effort)) effort = findModel(promptProvider, promptModel)?.defaultEffort ?? levels[Math.min(1, levels.length - 1)];
  });
  const promptModelLabel = $derived(promptModels.find((item) => item.value === promptModel)?.label ?? promptModel);

  $effect(() => {
    if (!promptModels.some((item) => item.value === promptModel)) promptModel = promptModels[0]?.value ?? '';
  });

  const projectPath = $derived.by(() => {
    const session = activeSession();
    return session ? workspace.projects.find((project) => project.id === session.projectId)?.path ?? null : null;
  });

  onMount(() => {
    if (inTauri()) void refreshHistory(true);
  });

  function selected() {
    return models.filter((item) => selectedModels.includes(item.model));
  }

  function toggleModel(model: string) {
    if (!selectedModels.includes(model) && selectedModels.length >= MAX_IMAGE_MODELS) {
      toast.error(`Choose at most ${MAX_IMAGE_MODELS} image models at once`);
      return;
    }
    selectedModels = selectedModels.includes(model) ? selectedModels.filter((value) => value !== model) : [...selectedModels, model];
  }

  /** The picked settings for the backend: null for Auto. */
  function settingsPayload(): ImageSettings {
    const value = (key: SettingKey) => settings[key] || null;
    return { style: value('style'), domain: value('domain'), audience: value('audience'), ratio: value('ratio'), resolution: value('resolution') };
  }

  /** The reference pictures as files on disk, which the generator reads. Each picture is saved once. */
  async function referencePaths(): Promise<string[]> {
    const paths: string[] = [];
    for (const reference of references) {
      reference.path ??= await saveAttachment(reference.mime, reference.src.slice(reference.src.indexOf(',') + 1));
      paths.push(reference.path);
    }
    return paths;
  }

  async function suggestPrompts() {
    if (suggesting) return;
    if (!inTauri()) {
      toast.error('AI prompt suggestions need the desktop app');
      return;
    }
    if (!prompt.trim() && !references.length) {
      toast.error('Enter an image prompt or add a reference image first');
      return;
    }
    void runSuggest({ prompt: prompt.trim(), model: promptModel, provider: promptProvider, referencePaths, settings: settingsPayload() });
  }

  function useSuggestion(value: string) {
    prompt = value;
    clearSuggestions();
  }

  /** A suggested keyword goes to its setting when one of its options fits, otherwise it is added to the prompt. */
  function useKeyword(keyword: SuggestedKeyword) {
    const key = SETTING_CATEGORY[keyword.category];
    const term = keyword.value.toLowerCase();
    const option = SETTINGS.find((item) => item.key === key)?.options.find((value) => value.toLowerCase().startsWith(term));
    if (key && option) {
      settings[key] = option;
      return;
    }
    prompt = prompt.trim() ? `${prompt.trim()}, ${keyword.value}` : keyword.value;
  }

  function openSetting(key: SettingKey, anchor: HTMLElement) {
    if (settingOpen && settingKey === key) {
      settingOpen = false;
      return;
    }
    settingKey = key;
    settingAnchor = anchor;
    settingQuery = '';
    settingOpen = true;
  }

  function pickSetting(value: string) {
    if (settingKey) settings[settingKey] = value;
    settingOpen = false;
  }

  const readPicture = (file: File) =>
    new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });

  async function addPictures(files: File[]) {
    const pictures = files.filter((file) => PICTURE.test(file.type));
    if (!pictures.length) return;
    const room = MAX_REFERENCES - references.length;
    if (room <= 0) {
      toast.error(`Up to ${MAX_REFERENCES} reference images`);
      return;
    }
    for (const file of pictures.slice(0, room)) {
      const src = await readPicture(file);
      references = [...references, { id: crypto.randomUUID(), name: file.name || 'pasted image', mime: file.type, src }];
    }
  }

  function onPaste(event: ClipboardEvent) {
    const files = [...(event.clipboardData?.files ?? [])];
    if (!files.some((file) => PICTURE.test(file.type))) return; // text pastes as usual
    event.preventDefault();
    void addPictures(files);
  }

  function onPromptKeydown(event: KeyboardEvent) {
    if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
    event.preventDefault();
    if (working || suggesting || !prompt.trim() || selectedModels.length === 0) return;
    generate();
  }

  let fileInput = $state<HTMLInputElement | undefined>();

  function onDrop(event: DragEvent) {
    const files = [...(event.dataTransfer?.files ?? [])];
    if (!files.some((file) => PICTURE.test(file.type))) return;
    event.preventDefault();
    void addPictures(files);
  }

  function generate() {
    const choices = selected();
    if (!prompt.trim()) {
      toast.error('Write an image prompt first');
      return;
    }
    if (!choices.length) {
      toast.error('Choose at least one image model');
      return;
    }
    const project = workspace.projects.find((item) => item.path === projectPath)?.name ?? '';
    void startRun({ prompt, choices, variants: variantCount, cwd: projectPath, project, referencePaths, settings: settingsPayload() });
  }

  const stop = stopRun;

  function openPreview(index: number) {
    openImages(
      previews.map((image) => ({ src: image.src, name: `${image.model}-${image.variant}.png` })),
      index,
    );
  }

  function useInChat(image: Preview) {
    const session = activeSession();
    if (!session) return;
    const attachment: Attachment = { id: crypto.randomUUID(), name: `generated-${image.model}-${image.variant}.png`, mime: 'image/png', src: image.src };
    draftImages[session.id] = [...(draftImages[session.id] ?? []), attachment];
    toast.success('Image added to the composer');
  }

  function dayLabel(millis: number) {
    const date = new Date(millis);
    const today = new Date();
    const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
    if (date.toDateString() === today.toDateString()) return 'Today';
    if (date.toDateString() === yesterday.toDateString()) return 'Yesterday';
    return date.toLocaleDateString(undefined, { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric' });
  }

  function timeLabel(millis: number) {
    return new Date(millis).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  // History grouped by day: a header row, then the runs of that day. Rows have fixed heights so the list can be windowed.
  type HistoryItem = { kind: 'day'; key: string; label: string } | { kind: 'run'; key: string; run: ImageHistoryRun };
  const DAY_HEIGHT = 28;
  const RUN_HEIGHT = 52;
  const historyItems = $derived.by(() => {
    const items: HistoryItem[] = [];
    let day = '';
    for (const run of history) {
      const label = dayLabel(run.created_at);
      if (label !== day) {
        day = label;
        items.push({ kind: 'day', key: `day:${label}`, label });
      }
      items.push({ kind: 'run', key: run.id, run });
    }
    return items;
  });
  // TanStack Virtual (through z-ui), as the chat uses: only the rows in view are mounted.
  let historyScroller = $state<HTMLDivElement | undefined>();
  const historyKey = (index: number) => historyItems[index]?.key ?? index;
  const historyHeight = (index: number) => (historyItems[index]?.kind === 'day' ? DAY_HEIGHT : RUN_HEIGHT);
  const historyVirtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => historyScroller ?? null,
    estimateSize: historyHeight,
    getItemKey: historyKey,
    overscan: 6,
  });
  $effect(() => {
    const count = historyItems.length;
    untrack(() => $historyVirtualizer.setOptions({ count, getScrollElement: () => historyScroller ?? null, estimateSize: historyHeight, getItemKey: historyKey, overscan: 6 }));
  });
  const historyRows = $derived($historyVirtualizer.getVirtualItems());

  // One nested list at a time. Opening one closes the other directly: the pending close of the other row's leave
  // timer is cleared here, so it could not close it.
  function openSub(which: 'models') {
    clearTimeout(nestedCloseTimer);
    promptModelsOpen = false;
    modelsOpen = which === 'models';
  }

  function openPromptModels(provider: string, row: HTMLElement) {
    clearTimeout(nestedCloseTimer);
    modelsOpen = false;
    promptSubProvider = provider;
    promptModelsAnchor = row;
    promptModelsOpen = true;
  }

  function closeNestedMenusSoon() {
    clearTimeout(nestedCloseTimer);
    nestedCloseTimer = setTimeout(() => {
      modelsOpen = false;
      promptModelsOpen = false;
    }, 120);
  }

  function keepNestedMenusOpen() {
    clearTimeout(nestedCloseTimer);
  }

  // Opening the settings only opens the settings: the nested menus wait for the pointer to reach their row.
  function toggleSettings() {
    settingsOpen = !settingsOpen;
  }

  // The pointer may already rest on a row when the settings open: that is not a hover, so hover waits a moment.
  let settingsOpenedAt = 0;
  function hoverOpens() {
    return performance.now() - settingsOpenedAt > 300;
  }

  $effect(() => {
    if (settingsOpen) {
      settingsOpenedAt = performance.now();
    } else {
      clearTimeout(nestedCloseTimer);
      modelsOpen = false;
      promptModelsOpen = false;
    }
  });

  async function openHistoryRun(run: ImageHistoryRun) {
    historyOpen = false;
    clearTimeout(peekTimer);
    peekOpen = false;
    await openRun(run);
  }

  function statusLabel(status: ImageHistoryRun['status']) {
    return status === 'stopped' ? 'Stopped' : status === 'failed' ? 'Failed' : null;
  }
</script>

<!-- A ring that turns around a round button while its work runs, with a soft ripple; both stop for reduced motion. -->
{#snippet busyRing()}
  <span class="pointer-events-none absolute -inset-[2px] animate-spin rounded-full [background:conic-gradient(from_0deg,transparent_0_55%,var(--color-primary)_100%)] [mask:radial-gradient(farthest-side,transparent_calc(100%-2px),#000_calc(100%-2px))] motion-reduce:hidden" aria-hidden="true"></span>
  <span class="pointer-events-none absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:1.8s] motion-reduce:hidden" aria-hidden="true"></span>
{/snippet}

<div class="flex h-full min-h-0 flex-col" role="tabpanel" aria-label="Image generator">
  <header class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-[13px]">
    <span class="grid size-7 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType="lucideImage" class="size-4" /></span>
    <h2 class="m-0 min-w-0 flex-1 truncate text-xs font-semibold text-foreground">Image candidates</h2>
    {#if runMs > 0}
      <span class="inline-flex h-7 shrink-0 items-center rounded border border-border bg-background px-2.5 text-[11px] font-medium tabular-nums text-muted-foreground" title="Time of the last run" aria-live="off">{formatDuration(runMs)}</span>
    {/if}
    <button
      type="button"
      class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground transition-colors outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {historyOpen ? 'bg-primary/10 text-primary' : ''}"
      aria-label="Image history"
      title="Image history"
      aria-expanded={historyOpen}
      onclick={() => (historyOpen = !historyOpen)}
    >
      <ZIcon zType="lucideHistory" class="size-3.5" />
    </button>
  </header>

  <ZDrawer bind:open={historyOpen} zPlacement="right" zWidth="320px" zFloating zTitle="History" zBodyClass="p-0">
    <div class="flex h-full min-h-0 flex-col">
      {#if historyItems.length === 0}
        <DevEmpty icon="lucideImages" title="No history yet" hint="Each run you generate is kept here." />
      {:else}
        <div bind:this={historyScroller} class="min-h-0 flex-1 overflow-y-auto" aria-label="Image history">
          <div class="relative w-full" style:height="{$historyVirtualizer.getTotalSize()}px">
            {#each historyRows as row (row.key)}
              {@const item = historyItems[row.index]}
              {#if item?.kind === 'day'}
                <div class="absolute top-0 left-0 flex w-full items-center gap-2 px-2 text-[10px] font-medium tracking-wider text-muted-foreground uppercase" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                  <span class="shrink-0">{item.label}</span>
                  <span class="h-px flex-1 bg-border" aria-hidden="true"></span>
                </div>
              {:else if item}
                <div class="absolute top-0 left-0 w-full px-1 py-0.5" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                  <button
                    type="button"
                    class="flex h-full w-full cursor-pointer flex-col justify-center gap-0.5 rounded-md px-2 text-left outline-none transition-colors hover:bg-accent focus-visible:bg-accent {activeRunId === item.run.id ? 'bg-primary/10' : ''}"
                    onclick={() => void openHistoryRun(item.run)}
                    onpointerenter={(event) => peekRun(item.run, event.currentTarget)}
                    onpointerleave={leavePeek}
                  >
                    <span class="truncate text-xs">{item.run.prompt}</span>
                    <span class="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
                      <span class="shrink-0">{timeLabel(item.run.created_at)}</span>
                      <span class="truncate">· {item.run.models.length} model{item.run.models.length === 1 ? '' : 's'} · {item.run.images.length} image{item.run.images.length === 1 ? '' : 's'}</span>
                      {#if statusLabel(item.run.status)}<span class="shrink-0 text-warning">· {statusLabel(item.run.status)}</span>{/if}
                    </span>
                  </button>
                </div>
              {/if}
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </ZDrawer>

  <ZPopover
    anchor={peekAnchor}
    bind:open={peekOpen}
    zTrigger="manual"
    zPosition="left-top"
    zOffset={6}
    zWidth={256}
  >
    {#snippet children()}
      <!-- The handlers sit on the content: the popover host overrides its own pointer handlers. -->
      <div class="max-h-28 overflow-y-auto p-2 text-xs leading-snug break-words whitespace-normal" role="presentation" onpointerenter={keepPeek} onpointerleave={leavePeek}>{peek?.prompt}</div>
    {/snippet}
  </ZPopover>

  <div class="min-h-0 flex-1 overflow-y-auto py-3">
    <div class="flex min-h-full flex-col gap-3 px-3">
      <div class="relative min-w-0 rounded-2xl border border-input bg-card p-2 shadow-lg transition-[border-color,box-shadow] focus-within:border-ring focus-within:ring-ring/20 focus-within:ring-2">
        {#if references.length > 0}
          <div class="mb-2 flex flex-wrap gap-2" aria-label="Reference images">
            {#each references as reference (reference.id)}
              <div class="group/ref relative">
                <button type="button" class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {reference.name}" onclick={() => openImages(references, references.indexOf(reference))}>
                  <img src={reference.src} alt={reference.name} class="size-full object-cover" />
                </button>
                <button
                  type="button"
                  class="absolute -top-3 -right-2.5 flex size-5.5 cursor-pointer items-center justify-center rounded-full border border-border bg-popover text-popover-foreground shadow-md opacity-0 transition-opacity group-hover/ref:opacity-100 focus-visible:opacity-100"
                  aria-label="Remove {reference.name}"
                  onclick={() => (references = references.filter((item) => item !== reference))}
                >
                  <ZIcon zType="lucideX" zSize="12" class="block size-[12px]! shrink-0" />
                </button>
              </div>
            {/each}
          </div>
        {/if}
        <textarea
          bind:value={prompt}
          rows="2"
          aria-label="Image prompt"
          onkeydown={onPromptKeydown}
          onpaste={onPaste}
          ondragover={(event) => event.preventDefault()}
          ondrop={onDrop}
          placeholder="Describe the image, style, composition and constraints…"
          class="max-h-45 min-h-6 w-[calc(100%+0.5rem)] resize-none overflow-x-hidden break-words border-0 bg-transparent p-0 pr-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
        ></textarea>

        <div class="flex min-w-0 items-center justify-between gap-2">
          <div class="flex min-w-0 items-center gap-0.5">
          <button
            type="button"
            class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
            aria-label="Attach images"
            title="Attach images (up to {MAX_REFERENCES}, or paste / drop)"
            disabled={references.length >= MAX_REFERENCES}
            onclick={() => fileInput?.click()}
          >
            <ZIcon zType="lucidePaperclip" class="size-3.5" />
          </button>
          <input
            bind:this={fileInput}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            multiple
            hidden
            onchange={(event) => {
              void addPictures([...(event.currentTarget.files ?? [])]);
              event.currentTarget.value = '';
            }}
          />
          <button
            bind:this={settingsTrigger}
            type="button"
            class="flex h-8 min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {settingsOpen ? 'bg-primary/10 text-primary' : ''}"
            aria-haspopup="menu"
            aria-expanded={settingsOpen}
            aria-label="Image settings"
            onclick={toggleSettings}
          >
            <ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />
            <span class="truncate" {@attach zTooltip({ content: `${settingsSummary} · ${variantCount}×`, position: 'top', autoDetect: true })}>{settingsSummary}</span>
            <span class="hidden shrink-0 text-muted-foreground min-[720px]:inline">· {variantCount}×</span>
            <ZIcon zType="lucideChevronDown" class="size-3 shrink-0 text-muted-foreground" />
          </button>
          </div>

          <div class="flex h-8 shrink-0 items-center gap-0.5 rounded-full border border-border bg-background p-0.5" role="group" aria-label="Image actions">
            <button
              type="button"
              class="relative grid size-7 cursor-pointer place-items-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
              aria-label="AI suggestions"
              title="AI suggestions"
              disabled={(!prompt.trim() && references.length === 0) || !promptModel || working}
              aria-busy={suggesting}
              onclick={() => void suggestPrompts()}
            >
              {#if suggesting}
                {@render busyRing()}
                <ZIcon zType="lucideWandSparkles" class="relative size-3.5 text-primary" />
              {:else}
                <ZIcon zType="lucideWandSparkles" class="size-3.5" />
              {/if}
            </button>

            {#if working}
              <button
                type="button"
                class="relative grid size-7 cursor-pointer place-items-center rounded-full bg-primary text-primary-foreground transition-opacity outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
                aria-label="Stop generating"
                title="Stop"
                disabled={stopping}
                onclick={() => void stop()}
              >
                {@render busyRing()}
                <svg class="relative size-2.5" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="currentColor" /></svg>
              </button>
            {:else}
              <button
                type="button"
                class="grid size-7 cursor-pointer place-items-center rounded-full bg-primary text-primary-foreground transition-opacity outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
                aria-label="Generate images"
                title="Generate {candidateCount} image candidates"
                disabled={!prompt.trim() || selectedModels.length === 0 || suggesting}
                onclick={generate}
              >
                <ZIcon zType="lucideArrowUp" class="size-3.5" />
              </button>
            {/if}
          </div>
        </div>

        <ZPopover anchor={settingsTrigger} bind:open={settingsOpen} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={256}>
          {#snippet children()}
            <div class="flex w-full flex-col gap-4 overflow-x-hidden overflow-y-auto px-2.5 py-2.5 text-xs" role="menu" aria-label="Image settings" {@attach glide()}>
              <section aria-label="Image models">
                <SectionTitle label="MODEL" />
                <button
                  bind:this={modelsRow}
                  type="button"
                  data-glide-item
                  role="menuitem"
                  aria-haspopup="menu"
                  aria-expanded={modelsOpen}
                  class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {modelsOpen ? 'bg-primary/10 text-primary' : ''}"
                  onpointerenter={() => hoverOpens() && openSub('models')}
                  onpointerleave={closeNestedMenusSoon}
                >
                  <span class="min-w-0 flex-1 truncate" {@attach zTooltip({ content: settingsSummary, position: 'right', autoDetect: true })}>{settingsSummary}</span>
                  <ZIcon zType="lucideChevronRight" class="size-3.5 shrink-0 text-muted-foreground" />
                </button>
              </section>

              <section aria-label="Prompt model">
                <SectionTitle label="PROMPT MODEL" />
                <div class="flex flex-col gap-0.5">
                  {#each promptGroups as group (group.provider)}
                    {@const activeProvider = promptProvider === group.provider}
                    <button
                      type="button"
                      role="menuitem"
                      aria-haspopup="dialog"
                      aria-expanded={promptModelsOpen && promptSubProvider === group.provider}
                      data-glide-item
                      class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {activeProvider || (promptModelsOpen && promptSubProvider === group.provider) ? 'bg-primary/10' : ''} {promptModelsOpen && promptSubProvider === group.provider ? 'text-primary' : ''}"
                      onpointerenter={(event) => openPromptModels(group.provider, event.currentTarget)}
                      onpointerleave={closeNestedMenusSoon}
                      onfocus={(event) => openPromptModels(group.provider, event.currentTarget)}
                      onclick={(event) => openPromptModels(group.provider, event.currentTarget)}
                    >
                      {#if group.provider === 'codex'}
                        <ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />
                      {:else if group.provider === 'opencode'}
                        <ZIcon zSvg={OPENCODE_SVG} class="size-3.5 shrink-0 text-foreground" />
                      {:else}
                        <ZIcon zType={group.icon} class="size-3.5 shrink-0 text-muted-foreground" />
                      {/if}
                      <span class="flex-1 truncate">{group.label}</span>
                      {#if activeProvider}<span class="max-w-32 truncate text-[11px] text-muted-foreground" {@attach zTooltip({ content: promptModelLabel, position: 'right', autoDetect: true })}>{promptModelLabel}</span>{/if}
                      <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 text-muted-foreground" />
                    </button>
                  {/each}
                </div>
              </section>

              <section aria-label="Variants per model">
                <SectionTitle label="VARIANTS" />
                <ZTabs
                  zTabs={variantTabs}
                  zType="segmented"
                  zSize="xs"
                  zTabsOnly
                  zNavFullWidth
                  zCache={false}
                  class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
                  activeTab={String(variantCount)}
                  onTabChange={(value) => (variants = Number(value))}
                />
              </section>

              <section aria-label="Reasoning effort">
                <SectionTitle label="EFFORT" />
                <ZTabs
                  zTabs={effortTabs}
                  zType="segmented"
                  zSize="xs"
                  zTabsOnly
                  zNavFullWidth
                  zCache={false}
                  class="w-full [&_.z-tab-item]:flex-1 [&_.z-tab-item-label]:text-center"
                  activeTab={effort}
                  onTabChange={(value) => (effort = String(value))}
                />
              </section>

              <section aria-label="Speed" class="flex items-center justify-between gap-6 px-0.5">
                <div>
                  <div class="flex items-center gap-1.5 text-xs font-medium">
                    <ZIcon zType="lucideZap" class="size-3.5 text-warning" />
                    Fast mode
                  </div>
                  <div class="mt-1 text-[11px] leading-snug text-muted-foreground">Faster output, higher usage</div>
                </div>
                <ZSwitch bind:checked={fast} />
              </section>
            </div>
          {/snippet}
        </ZPopover>

        <ZPopover anchor={modelsRow} bind:open={modelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={173}>
          {#snippet children()}
            <div class="flex w-full flex-col gap-0.5 p-1.5 text-xs" role="menu" aria-label="Image models" tabindex="-1" onpointerenter={keepNestedMenusOpen} onpointerleave={closeNestedMenusSoon} {@attach glide()}>
              {#each models as item (item.model)}
                {@const active = selectedModels.includes(item.model)}
                <button
                  type="button"
                  data-glide-item
                  role="menuitemcheckbox"
                  aria-checked={active}
                  class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {active ? 'bg-primary/10 text-primary' : ''}"
                  onclick={() => toggleModel(item.model)}
                  {@attach zTooltip({ content: item.label, position: 'right', autoDetect: true, offset: 6 })}
                >
                  <ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />
                  <span class="min-w-0 flex-1 truncate">{item.label}</span>
                  {#if active}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0" />{/if}
                </button>
              {/each}
            </div>
          {/snippet}
        </ZPopover>

        <ZPopover anchor={promptModelsAnchor} bind:open={promptModelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={173}>
          {#snippet children()}
            {@const selectedGroup = promptGroups.find((group) => group.provider === promptSubProvider)}
            {@const sorted = freeFirst(selectedGroup?.items ?? [])}
            <div class="flex w-full flex-col gap-0.5 overflow-x-hidden overflow-y-auto py-1 pl-1 text-xs" style:max-height="320px" role="radiogroup" aria-label="Prompt models" tabindex="-1" onpointerenter={keepNestedMenusOpen} onpointerleave={closeNestedMenusSoon} {@attach glide()}>
              {#if !selectedGroup?.items.length}
                <!-- The same empty state as the chat's model picker: the provider listed no models. -->
                <div class="flex w-full flex-col items-center gap-1.5 px-2 py-3 text-center">
                  <ZIcon zType="lucideShieldAlert" class="size-5 text-muted-foreground" />
                  <div class="text-xs font-medium">No AI models</div>
                  <p class="m-0 text-[11px] leading-snug text-muted-foreground">{selectedGroup?.label ?? 'This provider'} returned no models, or its provider has an error. Check it in Settings.</p>
                </div>
              {/if}
              {#each sorted.items as item, index (item.value)}
                {@const active = item.value === promptModel}
                {#if index === sorted.paidAt}<div class="mr-1 mt-1 flex shrink-0 items-center gap-2 px-2 text-[10px] font-medium tracking-wide text-muted-foreground uppercase" role="separator"><span>Paid</span><span class="h-px flex-1 bg-border"></span></div>{/if}
                <button
                  type="button"
                  data-glide-item
                  role="radio"
                  aria-checked={active}
                  class="mr-1 flex cursor-pointer items-center justify-between rounded-md px-2 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {active ? 'bg-primary/10 text-primary' : ''}"
                  onclick={() => { promptModel = item.value; promptModelsOpen = false; }}
                  {@attach zTooltip({ content: item.label, position: 'right', autoDetect: true, offset: 6 })}
                >
                  <span class="min-w-0 truncate">{item.label}</span>
                  {#if active}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0 text-primary" />{/if}
                </button>
              {/each}
            </div>
          {/snippet}
        </ZPopover>

        {#if suggesting || promptSuggestions.length}
          <div transition:fold class="-mx-3 -mb-2 mt-2 flex flex-col gap-1 overflow-hidden border-t border-border px-3 pt-2 pb-2" aria-label="Prompt suggestions">
            <span class="px-1 text-[11px] text-muted-foreground">Suggested prompts · click to use</span>
            {#if suggesting}
              <div class="flex flex-col gap-2 px-1 py-1" role="status" aria-label="Writing prompt suggestions">
                {#each [0, 1, 2] as line (line)}
                  <div class="h-3 animate-pulse rounded bg-muted {line === 2 ? 'w-3/5' : 'w-full'}"></div>
                {/each}
              </div>
            {:else}
              {#each promptSuggestions as suggestion (suggestion)}
                <button in:fade={{ duration: 220 }} type="button" class="rounded-md bg-muted/50 px-2 py-1.5 text-left text-xs leading-4 text-foreground transition-colors hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10 focus-visible:text-primary outline-none" onclick={() => useSuggestion(suggestion)}>
                  {suggestion}
                </button>
              {/each}
              {#if suggestedKeywords.length > 0}
                <div class="flex flex-wrap gap-1 px-1 pt-1" aria-label="Suggested keywords">
                  {#each suggestedKeywords as keyword (keyword.category + keyword.value)}
                    <button type="button" class="rounded-full border border-primary/30 bg-primary/5 px-2 py-0.5 text-[11px] text-primary outline-none transition-colors hover:bg-primary/15 focus-visible:ring-2 focus-visible:ring-ring" title={SETTINGS.find((item) => item.key === SETTING_CATEGORY[keyword.category])?.label ?? 'Prompt keyword'} onclick={() => useKeyword(keyword)}>
                      {keyword.value}
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </div>

      <!-- The settings under the prompt: sent with it, and suggested with it by the prompt model. -->
      <div class="-mt-2 flex flex-wrap items-center gap-1.5" role="group" aria-label="Image settings">
        {#each SETTINGS as setting (setting.key)}
          {@const chosen = settings[setting.key]}
          <button
            type="button"
            class="flex h-7 min-w-0 cursor-pointer items-center gap-1 rounded border border-border bg-background px-2.5 text-[11px] outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {settingOpen && settingKey === setting.key ? ACTIVE_ON : ''} {chosen ? 'text-foreground' : 'text-muted-foreground'}"
            aria-haspopup="menu"
            aria-expanded={settingOpen && settingKey === setting.key}
            onclick={(event) => openSetting(setting.key, event.currentTarget)}
          >
            <ZIcon zType={(chosen && OPTION_ICON[chosen]) || SETTING_ICON[setting.key]} class="size-3.5 shrink-0 text-muted-foreground" />
            <span class="shrink-0 text-muted-foreground">{setting.label}</span>
            <span class="max-w-36 truncate">{chosen || 'Auto'}</span>
            <ZIcon zType="lucideChevronDown" class="size-3 shrink-0 text-muted-foreground" />
          </button>
        {/each}
      </div>

      <PickerPopover
        anchor={settingAnchor}
        bind:open={settingOpen}
        bind:query={settingQuery}
        items={settingItems}
        local
        search
        fitContent={settingFitsContent}
        fillHeight={!settingFitsContent}
        heightReduction={settingFitsContent ? 0 : 50}
        placeholder="Search…"
        label="Setting options"
        empty="No option matches"
        position="bottom-left"
        size="sm"
        width={210}
        onpick={(item) => pickSetting(item.data)}
        onclose={() => (settingQuery = '')}
      />

      <div class="-mx-3 flex min-h-0 flex-1 flex-col border-t border-border pt-3">
        <div class="flex min-h-0 flex-1 flex-col px-3">
          {#if working || loadingRun}
            <div class="grid grid-cols-2 gap-2" role="status" aria-label="Generating image candidates">
              {#each Array.from({ length: working ? runSkeleton : loadingRun }) as _, index (index)}
                {@const tile = working ? gen.runTiles[index] : undefined}
                <ImageDeveloping {index} label={tile?.label ?? ''} variant={tile?.variant ?? 0} ms={working ? runMs : null} />
              {/each}
            </div>
          {:else if previews.length}
            <div class="grid grid-cols-2 gap-2" aria-label="Generated image candidates">
              {#each previews as image, index (image.path)}
                <article class="group overflow-hidden rounded-lg border border-border bg-background">
                  <button type="button" class="block w-full cursor-zoom-in outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {modelLabel(image.model)} #{image.variant}" onclick={() => openPreview(index)}>
                    <img src={image.src} alt="{modelLabel(image.model)} candidate #{image.variant}" class="aspect-square w-full object-cover" />
                  </button>
                  <div class="flex items-center gap-1 px-1.5 py-1.5">
                    <span class="min-w-0 flex-1 truncate text-[10px] text-muted-foreground" title="{modelLabel(image.model)} · #{image.variant} — {image.path}">{modelLabel(image.model)} · #{image.variant}</span>
                    <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Use image in chat" title="Use image in chat" onclick={() => useInChat(image)}>
                      <ZIcon zType="lucideMessageSquarePlus" class="size-3.5" />
                    </button>
                    <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Reveal image file" title="Reveal image file" onclick={() => void revealFile(image.path)}>
                      <ZIcon zType="lucideFolderOpen" class="size-3.5" />
                    </button>
                  </div>
                </article>
              {/each}
            </div>
          {:else}
            <div class="flex flex-1 items-center justify-center">
              <DevEmpty icon="lucideImages" title="No image candidates yet" hint="Enter a prompt, choose your models, then generate a few options here." />
            </div>
          {/if}

          {#if failureLines.length && !working}
            <div class="mt-2 rounded-lg border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-[11px] text-destructive">
              <button type="button" class="flex w-full cursor-pointer items-center gap-1.5 text-left font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-expanded={failuresOpen} onclick={() => (failuresOpen = !failuresOpen)}>
                <ZIcon zType="lucideChevronDown" class="size-3.5 shrink-0 transition-transform duration-300 ease-out motion-reduce:transition-none {failuresOpen ? '' : '-rotate-90'}" />
                {failureLines.length} candidate{failureLines.length === 1 ? '' : 's'} failed
              </button>
              <!-- The rows animate their height with the grid, so the list never jumps: no margin outside the animated box. -->
              <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {failuresOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
                <div class="min-h-0 overflow-hidden">
                  <ul class="list-disc space-y-1 pt-1.5 pl-4">
                    {#each failureLines as line (line)}
                      <li>{line}</li>
                    {/each}
                  </ul>
                </div>
              </div>
            </div>
          {/if}

        </div>
      </div>
    </div>
  </div>
</div>
